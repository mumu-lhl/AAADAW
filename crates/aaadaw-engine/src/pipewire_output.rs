use crate::AudioRenderGraph;
use pipewire as pw;
use pw::spa::pod::Pod;
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const TRANSPORT_COMMAND_CAPACITY: usize = 16;
const RETIRED_GRAPH_CAPACITY: usize = 1;

enum TransportCommand {
    Play,
    Stop,
    ReplaceGraph {
        graph: Box<AudioRenderGraph>,
        start_playing: bool,
    },
}

enum ThreadCommand {
    Shutdown,
}

#[derive(Default)]
struct CallbackCounters {
    rendered_blocks: AtomicU64,
    underrun_samples: AtomicU64,
    callback_errors: AtomicU64,
    playhead_sample: AtomicU64,
}

struct ProcessData {
    graph: Box<AudioRenderGraph>,
    scratch: Vec<[f32; 2]>,
    commands: Consumer<TransportCommand>,
    retired_graphs: Producer<Box<AudioRenderGraph>>,
    pending_retired_graph: Option<Box<AudioRenderGraph>>,
    counters: Arc<CallbackCounters>,
}

impl ProcessData {
    fn apply_commands(&mut self) {
        self.flush_retired_graph();
        while let Ok(command) = self.commands.pop() {
            match command {
                TransportCommand::Play => self.graph.transport_mut().start(),
                TransportCommand::Stop => self.graph.transport_mut().stop(),
                TransportCommand::ReplaceGraph {
                    mut graph,
                    start_playing,
                } => {
                    if start_playing {
                        graph.transport_mut().start();
                    } else {
                        graph.transport_mut().stop();
                    }
                    let retired = std::mem::replace(&mut self.graph, graph);
                    if let Err(PushError::Full(retired)) = self.retired_graphs.push(retired) {
                        self.pending_retired_graph = Some(retired);
                    }
                }
            }
        }
    }

    fn flush_retired_graph(&mut self) {
        let Some(retired) = self.pending_retired_graph.take() else {
            return;
        };
        if let Err(PushError::Full(retired)) = self.retired_graphs.push(retired) {
            self.pending_retired_graph = Some(retired);
        }
    }

    fn process_bytes(&mut self, output: &mut [u8]) -> usize {
        self.apply_commands();
        const FRAME_BYTES: usize = 2 * std::mem::size_of::<f32>();
        let frame_count = output.len() / FRAME_BYTES;
        if output.len() % FRAME_BYTES != 0 || frame_count > self.scratch.len() {
            output.fill(0);
            self.counters
                .callback_errors
                .fetch_add(1, Ordering::Relaxed);
            return 0;
        }

        match self.graph.render_into(&mut self.scratch[..frame_count]) {
            Ok(stats) => {
                for (frame, bytes) in self.scratch[..frame_count]
                    .iter()
                    .zip(output.chunks_exact_mut(FRAME_BYTES))
                {
                    bytes[..4].copy_from_slice(&frame[0].to_le_bytes());
                    bytes[4..].copy_from_slice(&frame[1].to_le_bytes());
                }
                self.counters
                    .underrun_samples
                    .fetch_add(stats.underrun_samples as u64, Ordering::Relaxed);
                self.counters
                    .rendered_blocks
                    .fetch_add(1, Ordering::Relaxed);
                self.counters.playhead_sample.store(
                    self.graph.transport_mut().position_samples(),
                    Ordering::Relaxed,
                );
                frame_count
            }
            Err(_) => {
                output.fill(0);
                self.counters
                    .callback_errors
                    .fetch_add(1, Ordering::Relaxed);
                0
            }
        }
    }
}

/// Errors encountered while configuring or controlling PipeWire output.
#[derive(Debug)]
pub enum PipeWireOutputError {
    PipeWire(pw::Error),
    Thread(String),
    SampleRateMismatch { project: u32, device: u32 },
    DeviceBlockTooLarge { device: usize, maximum: usize },
    RenderCapacityUnsupported { frames: usize },
    ControlQueueFull,
    GraphReplacementInFlight,
}

impl fmt::Display for PipeWireOutputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PipeWire(error) => write!(formatter, "PipeWire output error: {error}"),
            Self::Thread(error) => write!(formatter, "PipeWire output thread failed: {error}"),
            Self::SampleRateMismatch { project, device } => write!(
                formatter,
                "project sample rate {project} Hz does not match PipeWire rate {device} Hz"
            ),
            Self::DeviceBlockTooLarge { device, maximum } => write!(
                formatter,
                "PipeWire block size {device} exceeds render capacity {maximum}"
            ),
            Self::RenderCapacityUnsupported { frames } => write!(
                formatter,
                "PipeWire cannot request buffers for a render capacity of {frames} frames"
            ),
            Self::ControlQueueFull => {
                formatter.write_str("PipeWire transport command queue is full")
            }
            Self::GraphReplacementInFlight => {
                formatter.write_str("previous PipeWire render graph replacement is not collected")
            }
        }
    }
}

impl StdError for PipeWireOutputError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::PipeWire(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PipeWireOutputStats {
    pub rendered_blocks: u64,
    pub underrun_samples: u64,
    pub callback_errors: u64,
    pub playhead_sample: u64,
}

struct PipeWireParts {
    commands: Producer<TransportCommand>,
    retired_graphs: Consumer<Box<AudioRenderGraph>>,
    counters: Arc<CallbackCounters>,
}

/// A native PipeWire stereo output. The PipeWire objects stay on a dedicated control thread;
/// its realtime process callback uses only preallocated memory, SPSC queues, and atomics.
pub struct PipeWireAudioOutput {
    thread_command: Sender<ThreadCommand>,
    thread: Option<JoinHandle<()>>,
    commands: Producer<TransportCommand>,
    retired_graphs: Consumer<Box<AudioRenderGraph>>,
    counters: Arc<CallbackCounters>,
    device_sample_rate: u32,
    maximum_block_frames: usize,
    replacement_pending: bool,
}

impl PipeWireAudioOutput {
    /// Opens the default PipeWire output and starts its process callback.
    pub fn open(graph: AudioRenderGraph) -> Result<Self, PipeWireOutputError> {
        if graph.max_block_frames() == 0 {
            return Err(PipeWireOutputError::DeviceBlockTooLarge {
                device: 1,
                maximum: 0,
            });
        }
        if graph.max_block_frames() > i32::MAX as usize / (2 * std::mem::size_of::<f32>()) {
            return Err(PipeWireOutputError::RenderCapacityUnsupported {
                frames: graph.max_block_frames(),
            });
        }
        let sample_rate = graph.sample_rate();
        let maximum_block_frames = graph.max_block_frames();
        let (thread_command, shutdown) = mpsc::channel();
        let (setup_tx, setup_rx) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("aaadaw-pipewire".to_owned())
            .spawn(move || pipewire_thread(graph, shutdown, setup_tx))
            .map_err(|error| PipeWireOutputError::Thread(error.to_string()))?;
        let parts = match setup_rx.recv() {
            Ok(Ok(parts)) => parts,
            Ok(Err(error)) => {
                let _ = thread.join();
                return Err(PipeWireOutputError::Thread(error));
            }
            Err(error) => {
                let _ = thread.join();
                return Err(PipeWireOutputError::Thread(error.to_string()));
            }
        };
        Ok(Self {
            thread_command,
            thread: Some(thread),
            commands: parts.commands,
            retired_graphs: parts.retired_graphs,
            counters: parts.counters,
            device_sample_rate: sample_rate,
            maximum_block_frames,
            replacement_pending: false,
        })
    }

    pub fn play(&mut self) -> Result<(), PipeWireOutputError> {
        self.enqueue(TransportCommand::Play)
    }

    pub fn stop(&mut self) -> Result<(), PipeWireOutputError> {
        self.enqueue(TransportCommand::Stop)
    }

    pub fn replace_graph(
        &mut self,
        graph: AudioRenderGraph,
        start_playing: bool,
    ) -> Result<(), PipeWireOutputError> {
        if self.replacement_pending {
            return Err(PipeWireOutputError::GraphReplacementInFlight);
        }
        if graph.sample_rate() != self.device_sample_rate {
            return Err(PipeWireOutputError::SampleRateMismatch {
                project: graph.sample_rate(),
                device: self.device_sample_rate,
            });
        }
        if graph.max_block_frames() < self.maximum_block_frames {
            return Err(PipeWireOutputError::DeviceBlockTooLarge {
                device: self.maximum_block_frames,
                maximum: graph.max_block_frames(),
            });
        }
        self.enqueue(TransportCommand::ReplaceGraph {
            graph: Box::new(graph),
            start_playing,
        })?;
        self.replacement_pending = true;
        Ok(())
    }

    pub fn collect_retired_graphs(&mut self) -> usize {
        let mut collected = 0;
        while let Ok(graph) = self.retired_graphs.pop() {
            drop(graph);
            collected += 1;
        }
        if collected > 0 {
            self.replacement_pending = false;
        }
        collected
    }

    pub fn stats(&self) -> PipeWireOutputStats {
        PipeWireOutputStats {
            rendered_blocks: self.counters.rendered_blocks.load(Ordering::Relaxed),
            underrun_samples: self.counters.underrun_samples.load(Ordering::Relaxed),
            callback_errors: self.counters.callback_errors.load(Ordering::Relaxed),
            playhead_sample: self.counters.playhead_sample.load(Ordering::Relaxed),
        }
    }

    fn enqueue(&mut self, command: TransportCommand) -> Result<(), PipeWireOutputError> {
        match self.commands.push(command) {
            Ok(()) => Ok(()),
            Err(PushError::Full(_)) => Err(PipeWireOutputError::ControlQueueFull),
        }
    }
}

impl Drop for PipeWireAudioOutput {
    fn drop(&mut self) {
        let _ = self.thread_command.send(ThreadCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn pipewire_thread(
    graph: AudioRenderGraph,
    shutdown: Receiver<ThreadCommand>,
    setup: mpsc::SyncSender<Result<PipeWireParts, String>>,
) {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(pw::init);

    if let Err(error) = setup_pipewire_stream(graph, shutdown, &setup) {
        let _ = setup.send(Err(error));
    }
}

fn setup_pipewire_stream(
    graph: AudioRenderGraph,
    shutdown: Receiver<ThreadCommand>,
    setup: &mpsc::SyncSender<Result<PipeWireParts, String>>,
) -> Result<(), String> {
    let sample_rate = graph.sample_rate();
    let max_block_frames = graph.max_block_frames();
    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(|error| error.to_string())?;
    let context =
        pw::context::ContextRc::new(&mainloop, None).map_err(|error| error.to_string())?;
    let core = context
        .connect_rc(None)
        .map_err(|error| error.to_string())?;
    let stream = pw::stream::StreamBox::new(
        &core,
        "AAADAW Playback",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_ROLE => "Music",
            *pw::keys::MEDIA_CATEGORY => "Playback",
            *pw::keys::AUDIO_CHANNELS => "2",
        },
    )
    .map_err(|error| error.to_string())?;
    let (commands, command_consumer) = RingBuffer::new(TRANSPORT_COMMAND_CAPACITY);
    let (retired_graphs, retired_graph_consumer) = RingBuffer::new(RETIRED_GRAPH_CAPACITY);
    let counters = Arc::new(CallbackCounters::default());
    let process_data = ProcessData {
        graph: Box::new(graph),
        scratch: vec![[0.0; 2]; max_block_frames],
        commands: command_consumer,
        retired_graphs,
        pending_retired_graph: None,
        counters: Arc::clone(&counters),
    };
    let listener = stream
        .add_local_listener_with_user_data(process_data)
        .state_changed(|_, data, _, state| {
            if matches!(state, pw::stream::StreamState::Error(_)) {
                data.counters
                    .callback_errors
                    .fetch_add(1, Ordering::Relaxed);
            }
        })
        .process(|stream, data| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let datas = buffer.datas_mut();
            if let Some(output) = datas.first_mut() {
                let produced = match output.data() {
                    Some(bytes) => data.process_bytes(bytes),
                    None => {
                        data.counters
                            .callback_errors
                            .fetch_add(1, Ordering::Relaxed);
                        0
                    }
                };
                let chunk = output.chunk_mut();
                *chunk.offset_mut() = 0;
                *chunk.stride_mut() = (2 * std::mem::size_of::<f32>()) as i32;
                *chunk.size_mut() = (produced * 2 * std::mem::size_of::<f32>()) as u32;
            } else {
                data.counters
                    .callback_errors
                    .fetch_add(1, Ordering::Relaxed);
            }
        })
        .register()
        .map_err(|error| error.to_string())?;

    let mut audio_info = pw::spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(pw::spa::param::audio::AudioFormat::F32LE);
    audio_info.set_rate(sample_rate);
    audio_info.set_channels(2);
    let mut position = [0; pw::spa::param::audio::MAX_CHANNELS];
    position[0] = pw::spa::sys::SPA_AUDIO_CHANNEL_FL;
    position[1] = pw::spa::sys::SPA_AUDIO_CHANNEL_FR;
    audio_info.set_position(position);
    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(pw::spa::pod::Object {
            type_: pw::spa::sys::SPA_TYPE_OBJECT_Format,
            id: pw::spa::sys::SPA_PARAM_EnumFormat,
            properties: audio_info.into(),
        }),
    )
    .map_err(|error| error.to_string())?
    .0
    .into_inner();
    let max_buffer_bytes = i32::try_from(
        max_block_frames
            .checked_mul(2 * std::mem::size_of::<f32>())
            .ok_or_else(|| "render capacity overflows PipeWire buffer size".to_owned())?,
    )
    .map_err(|_| "render capacity exceeds PipeWire buffer size range".to_owned())?;
    let default_buffer_bytes = max_buffer_bytes.min(1024 * 2 * std::mem::size_of::<f32>() as i32);
    let minimum_buffer_bytes = max_buffer_bytes.min(128 * 2 * std::mem::size_of::<f32>() as i32);
    let buffer_values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(pw::spa::pod::Object {
            type_: pw::spa::sys::SPA_TYPE_OBJECT_ParamBuffers,
            id: pw::spa::sys::SPA_PARAM_Buffers,
            properties: vec![pw::spa::pod::Property::new(
                pw::spa::sys::SPA_PARAM_BUFFERS_size,
                pw::spa::pod::Value::Choice(pw::spa::pod::ChoiceValue::Int(
                    pw::spa::utils::Choice(
                        pw::spa::utils::ChoiceFlags::empty(),
                        pw::spa::utils::ChoiceEnum::Range {
                            default: default_buffer_bytes,
                            min: minimum_buffer_bytes,
                            max: max_buffer_bytes,
                        },
                    ),
                )),
            )],
        }),
    )
    .map_err(|error| error.to_string())?
    .0
    .into_inner();
    let audio_param = Pod::from_bytes(&values).ok_or_else(|| "invalid audio format".to_owned())?;
    let buffer_param = Pod::from_bytes(&buffer_values)
        .ok_or_else(|| "invalid audio buffer constraints".to_owned())?;
    let mut params = [audio_param, buffer_param];
    stream
        .connect(
            pw::spa::utils::Direction::Output,
            None,
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS
                | pw::stream::StreamFlags::NO_CONVERT,
            &mut params,
        )
        .map_err(|error| error.to_string())?;
    if setup
        .send(Ok(PipeWireParts {
            commands,
            retired_graphs: retired_graph_consumer,
            counters,
        }))
        .is_err()
    {
        return Err("output setup receiver was dropped".to_owned());
    }
    loop {
        match shutdown.try_recv() {
            Ok(ThreadCommand::Shutdown) | Err(TryRecvError::Disconnected) => break,
            Err(TryRecvError::Empty) => {}
        }
        mainloop
            .loop_()
            .iterate(pw::loop_::Timeout::Finite(Duration::from_millis(20)));
    }
    drop(listener);
    drop(stream);
    drop(core);
    drop(context);
    drop(mainloop);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aaadaw_core::Project;

    fn graph(max_frames: usize) -> AudioRenderGraph {
        let project = Project::new();
        AudioRenderGraph::new(&project, Vec::new(), max_frames)
            .expect("empty project graph should compile")
    }

    #[test]
    fn output_callback_writes_stereo_float_and_advances_transport() {
        let (mut command_producer, command_consumer) = RingBuffer::new(2);
        command_producer
            .push(TransportCommand::Play)
            .expect("play command should fit");
        let mut data = ProcessData {
            graph: Box::new(graph(8)),
            scratch: vec![[0.0; 2]; 8],
            commands: command_consumer,
            retired_graphs: RingBuffer::new(1).0,
            pending_retired_graph: None,
            counters: Arc::new(CallbackCounters::default()),
        };
        let mut bytes = vec![0xff; 4 * 2 * std::mem::size_of::<f32>()];
        assert_eq!(data.process_bytes(&mut bytes), 4);
        assert_eq!(bytes, vec![0; bytes.len()]);
        assert_eq!(data.counters.rendered_blocks.load(Ordering::Relaxed), 1);
        assert_eq!(data.counters.playhead_sample.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn oversized_output_is_silenced_and_counted_without_advancing_graph() {
        let (mut command_producer, command_consumer) = RingBuffer::new(2);
        command_producer
            .push(TransportCommand::Play)
            .expect("play command should fit");
        let mut data = ProcessData {
            graph: Box::new(graph(2)),
            scratch: vec![[0.0; 2]; 2],
            commands: command_consumer,
            retired_graphs: RingBuffer::new(1).0,
            pending_retired_graph: None,
            counters: Arc::new(CallbackCounters::default()),
        };
        let mut bytes = vec![0xff; 3 * 2 * std::mem::size_of::<f32>()];
        assert_eq!(data.process_bytes(&mut bytes), 0);
        assert_eq!(bytes, vec![0; bytes.len()]);
        assert_eq!(data.counters.callback_errors.load(Ordering::Relaxed), 1);
        assert_eq!(data.graph.transport_mut().position_samples(), 0);
    }

    #[test]
    fn graph_replacement_retires_the_old_graph_and_applies_play_state() {
        let (mut command_producer, command_consumer) = RingBuffer::new(2);
        let (retired_producer, mut retired_consumer) = RingBuffer::new(1);
        command_producer
            .push(TransportCommand::ReplaceGraph {
                graph: Box::new(graph(8)),
                start_playing: true,
            })
            .expect("graph replacement should fit");
        let mut data = ProcessData {
            graph: Box::new(graph(8)),
            scratch: vec![[0.0; 2]; 8],
            commands: command_consumer,
            retired_graphs: retired_producer,
            pending_retired_graph: None,
            counters: Arc::new(CallbackCounters::default()),
        };
        let mut bytes = vec![0; 2 * std::mem::size_of::<f32>()];

        assert_eq!(data.process_bytes(&mut bytes), 1);
        assert!(retired_consumer.pop().is_ok());
        assert_eq!(data.graph.transport_mut().position_samples(), 1);
    }
}
