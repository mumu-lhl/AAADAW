use crate::AudioRenderGraph;
use crate::cpal_common::is_supported_pcm_format;
#[cfg(target_os = "android")]
use crate::{MIDI_INPUT_EVENTS_PER_BLOCK, MidiEventKind, ScheduledMidiEvent};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, FromSample, Sample, SampleFormat, SizedSample, SupportedBufferSize};
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use std::error::Error as StdError;
use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const COMMAND_CAPACITY: usize = 16;
const RETIRED_GRAPH_CAPACITY: usize = 1;
const PREFERRED_CALLBACK_FRAMES: u32 = 512;
#[cfg(target_os = "android")]
const MIDI_QUEUE_CAPACITY: usize = 4096;

#[cfg(target_os = "android")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AndroidMidiOutputMessage {
    pub status: u8,
    pub data1: u8,
    pub data2: u8,
    pub sample_offset: usize,
    pub sample_rate: u32,
}

#[cfg(target_os = "android")]
pub struct CpalMidiInputSender(Producer<ScheduledMidiEvent>);

#[cfg(target_os = "android")]
impl CpalMidiInputSender {
    pub fn send(&mut self, event: ScheduledMidiEvent) -> Result<(), ScheduledMidiEvent> {
        self.0.push(event).map_err(|error| match error {
            PushError::Full(event) => event,
        })
    }
}

#[cfg(target_os = "android")]
pub struct CpalMidiOutputReceiver(Consumer<AndroidMidiOutputMessage>);

#[cfg(target_os = "android")]
impl CpalMidiOutputReceiver {
    pub fn try_receive(&mut self) -> Option<AndroidMidiOutputMessage> {
        self.0.pop().ok()
    }
}

fn callback_buffer_size(
    supported: &SupportedBufferSize,
    graph_capacity: usize,
) -> Result<BufferSize, CpalOutputError> {
    match supported {
        SupportedBufferSize::Range { min, max } => {
            let capacity = u32::try_from(graph_capacity).unwrap_or(u32::MAX);
            let largest_supported = (*max).min(capacity);
            if largest_supported < *min {
                return Err(CpalOutputError::DeviceBlockTooLarge {
                    device: *min as usize,
                    maximum: graph_capacity,
                });
            }
            Ok(BufferSize::Fixed(
                PREFERRED_CALLBACK_FRAMES.clamp(*min, largest_supported),
            ))
        }
        SupportedBufferSize::Unknown => Err(CpalOutputError::UnknownBufferSize),
    }
}

fn convert_stereo_output<T>(output: &mut [T], frames: &[[f32; 2]])
where
    T: Sample + FromSample<f32>,
{
    for (out, frame) in output.as_chunks_mut::<2>().0.iter_mut().zip(frames) {
        out[0] = T::from_sample(frame[0]);
        out[1] = T::from_sample(frame[1]);
    }
}

fn supports_project_output_config(
    channels: u16,
    format: SampleFormat,
    minimum_rate: u32,
    maximum_rate: u32,
    project_rate: u32,
) -> bool {
    channels == 2
        && is_supported_pcm_format(format)
        && minimum_rate <= project_rate
        && project_rate <= maximum_rate
}

enum Command {
    Play,
    Stop,
    PanicMidi,
    ReplaceGraph {
        graph: Box<AudioRenderGraph>,
        playing: bool,
    },
    Shutdown,
}

#[derive(Default)]
struct Counters {
    shutdown: AtomicBool,
    device_lost: AtomicBool,
    rendered_blocks: AtomicU64,
    underrun_samples: AtomicU64,
    master_guarded_samples: AtomicU64,
    master_non_finite_samples: AtomicU64,
    callback_errors: AtomicU64,
    playhead_sample: AtomicU64,
}

struct Callback {
    graph: Option<Box<AudioRenderGraph>>,
    scratch: Vec<[f32; 2]>,
    commands: Consumer<Command>,
    retired: Producer<Box<AudioRenderGraph>>,
    pending_retired: Option<Box<AudioRenderGraph>>,
    returned_graphs: Arc<Mutex<Vec<AudioRenderGraph>>>,
    counters: Arc<Counters>,
    shutting_down: bool,
    #[cfg(target_os = "android")]
    midi_input: Consumer<ScheduledMidiEvent>,
    #[cfg(target_os = "android")]
    midi_input_scratch: Vec<Option<ScheduledMidiEvent>>,
    #[cfg(target_os = "android")]
    midi_output: Producer<AndroidMidiOutputMessage>,
    #[cfg(target_os = "android")]
    midi_output_scratch: Vec<Option<ScheduledMidiEvent>>,
    #[cfg(target_os = "android")]
    sample_rate: u32,
}

impl Callback {
    #[cfg(target_os = "android")]
    fn send_midi_panic(&mut self) {
        const RESET_MESSAGES: [(u8, u8, u8); 4] =
            [(0xB0, 64, 0), (0xB0, 120, 0), (0xB0, 123, 0), (0xE0, 0, 64)];
        for channel in 0..16 {
            for (status, data1, data2) in RESET_MESSAGES {
                if self
                    .midi_output
                    .push(AndroidMidiOutputMessage {
                        status: status + channel,
                        data1,
                        data2,
                        sample_offset: 0,
                        sample_rate: self.sample_rate,
                    })
                    .is_err()
                {
                    self.counters
                        .callback_errors
                        .fetch_add(1, Ordering::Relaxed);
                    return;
                }
            }
        }
    }

    fn apply_commands(&mut self) {
        self.flush_retired();
        while let Ok(command) = self.commands.pop() {
            match command {
                Command::Play => {
                    if let Some(graph) = &mut self.graph {
                        graph.transport_mut().start();
                    }
                }
                Command::Stop => {
                    if let Some(graph) = &mut self.graph {
                        let failures = graph.release_midi_notes();
                        self.counters
                            .callback_errors
                            .fetch_add(failures as u64, Ordering::Relaxed);
                        graph.transport_mut().stop();
                    }
                    #[cfg(target_os = "android")]
                    self.send_midi_panic();
                }
                Command::PanicMidi => {
                    if let Some(graph) = &mut self.graph {
                        let failures = graph.release_midi_notes();
                        self.counters
                            .callback_errors
                            .fetch_add(failures as u64, Ordering::Relaxed);
                    }
                    #[cfg(target_os = "android")]
                    self.send_midi_panic();
                }
                Command::ReplaceGraph { mut graph, playing } => {
                    if playing {
                        graph.transport_mut().start();
                    } else {
                        graph.transport_mut().stop();
                    }
                    if let Some(active) = &mut self.graph {
                        let failures = active
                            .stop_instruments()
                            .saturating_add(active.stop_fx_processors());
                        self.counters
                            .callback_errors
                            .fetch_add(failures as u64, Ordering::Relaxed);
                    }
                    if let Some(retired) = self.graph.replace(graph)
                        && let Err(PushError::Full(retired)) = self.retired.push(retired)
                    {
                        self.pending_retired = Some(retired);
                    }
                }
                Command::Shutdown => {
                    if let Some(graph) = &mut self.graph {
                        let failures = graph
                            .release_midi_notes()
                            .saturating_add(graph.stop_instruments())
                            .saturating_add(graph.stop_fx_processors());
                        self.counters
                            .callback_errors
                            .fetch_add(failures as u64, Ordering::Relaxed);
                        graph.transport_mut().stop();
                    }
                    self.shutting_down = true;
                    self.counters.shutdown.store(true, Ordering::Release);
                    #[cfg(target_os = "android")]
                    self.send_midi_panic();
                    break;
                }
            }
        }
    }

    fn flush_retired(&mut self) {
        let Some(graph) = self.pending_retired.take() else {
            return;
        };
        if let Err(PushError::Full(graph)) = self.retired.push(graph) {
            self.pending_retired = Some(graph);
        }
    }

    fn render<T>(&mut self, output: &mut [T])
    where
        T: Sample + FromSample<f32>,
    {
        output.fill(T::EQUILIBRIUM);
        self.apply_commands();
        if self.shutting_down {
            return;
        }
        let Some(graph) = &mut self.graph else {
            return;
        };
        if self.counters.device_lost.load(Ordering::Relaxed) {
            graph.transport_mut().stop();
            return;
        }
        if !output.len().is_multiple_of(2) || output.len() / 2 > self.scratch.len() {
            self.counters
                .callback_errors
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        let frames = output.len() / 2;
        #[cfg(target_os = "android")]
        let render_result = {
            let mut input_count = 0;
            while input_count < self.midi_input_scratch.len() {
                match self.midi_input.pop() {
                    Ok(event) => {
                        self.midi_input_scratch[input_count] = Some(event);
                        input_count += 1;
                    }
                    Err(_) => break,
                }
            }
            for event in &mut self.midi_output_scratch {
                *event = None;
            }
            graph.render_with_midi_input(
                &mut self.midi_output_scratch,
                &self.midi_input_scratch[..input_count],
                &mut self.scratch[..frames],
            )
        };
        #[cfg(not(target_os = "android"))]
        let render_result = graph.render_into(&mut self.scratch[..frames]);
        match render_result {
            Ok(stats) => {
                #[cfg(target_os = "android")]
                for event in self
                    .midi_output_scratch
                    .iter()
                    .take(stats.midi_event_count)
                    .flatten()
                {
                    let (status, data1, data2) = match event.kind {
                        MidiEventKind::NoteOn => (0x90, event.pitch, event.velocity),
                        MidiEventKind::NoteOff => (0x80, event.pitch, event.velocity),
                        MidiEventKind::ControllerChange => (
                            0xB0,
                            event.controller.unwrap_or(event.pitch),
                            event.velocity,
                        ),
                        MidiEventKind::PitchBend => {
                            let bend = event.pitch_bend.unwrap_or(8192);
                            (0xE0, bend as u8 & 0x7F, (bend >> 7) as u8 & 0x7F)
                        }
                    };
                    if self
                        .midi_output
                        .push(AndroidMidiOutputMessage {
                            status,
                            data1,
                            data2,
                            sample_offset: event.sample_offset,
                            sample_rate: self.sample_rate,
                        })
                        .is_err()
                    {
                        self.counters
                            .callback_errors
                            .fetch_add(1, Ordering::Relaxed);
                    }
                }
                convert_stereo_output(output, &self.scratch[..frames]);
                self.counters
                    .underrun_samples
                    .fetch_add(stats.underrun_samples as u64, Ordering::Relaxed);
                self.counters
                    .master_guarded_samples
                    .fetch_add(stats.master_guarded_samples as u64, Ordering::Relaxed);
                self.counters
                    .master_non_finite_samples
                    .fetch_add(stats.master_non_finite_samples as u64, Ordering::Relaxed);
                self.counters
                    .rendered_blocks
                    .fetch_add(1, Ordering::Relaxed);
                self.counters
                    .playhead_sample
                    .store(graph.transport_mut().position_samples(), Ordering::Relaxed);
            }
            Err(_) => {
                self.counters
                    .callback_errors
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

impl Drop for Callback {
    fn drop(&mut self) {
        let mut returned = self
            .returned_graphs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(graph) = self.pending_retired.take() {
            returned.push(*graph);
        }
        if let Some(graph) = self.graph.take() {
            returned.push(*graph);
        }
    }
}

#[derive(Debug)]
pub enum CpalOutputError {
    DeviceUnavailable,
    SelectedDeviceUnavailable(String),
    InvalidDeviceId(String),
    Cpal(cpal::Error),
    NoStereoConfig {
        sample_rate: u32,
        is_default_device: bool,
    },
    UnsupportedSampleFormat(SampleFormat),
    SampleRateMismatch {
        project: u32,
        device: u32,
    },
    UnknownBufferSize,
    DeviceBlockTooLarge {
        device: usize,
        maximum: usize,
    },
    ControlQueueFull,
    GraphReplacementInFlight,
    ShutdownTimedOut,
}

impl fmt::Display for CpalOutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeviceUnavailable => {
                f.write_str("no default system audio output device is available")
            }
            Self::SelectedDeviceUnavailable(id) => {
                write!(
                    f,
                    "selected system audio output device is unavailable ({id})"
                )
            }
            Self::InvalidDeviceId(id) => {
                write!(f, "saved system audio output device ID is invalid ({id})")
            }
            Self::Cpal(error) => write!(f, "System audio output error: {error}"),
            Self::NoStereoConfig {
                sample_rate,
                is_default_device,
            } => write!(
                f,
                "{} system audio device has no stereo PCM configuration supporting {sample_rate} Hz",
                if *is_default_device {
                    "default"
                } else {
                    "selected"
                }
            ),
            Self::UnsupportedSampleFormat(format) => write!(
                f,
                "system audio sample format {format:?} is not supported for output"
            ),
            Self::SampleRateMismatch { project, device } => write!(
                f,
                "project sample rate {project} Hz does not match system audio rate {device} Hz"
            ),
            Self::UnknownBufferSize => {
                f.write_str("system audio did not report a supported output buffer size")
            }
            Self::DeviceBlockTooLarge { device, maximum } => write!(
                f,
                "system audio callback block {device} exceeds render capacity {maximum}"
            ),
            Self::ControlQueueFull => f.write_str("system audio transport command queue is full"),
            Self::GraphReplacementInFlight => {
                f.write_str("system audio graph replacement is still in flight")
            }
            Self::ShutdownTimedOut => {
                f.write_str("system audio output callback did not acknowledge shutdown")
            }
        }
    }
}
impl StdError for CpalOutputError {}
impl From<cpal::Error> for CpalOutputError {
    fn from(error: cpal::Error) -> Self {
        Self::Cpal(error)
    }
}

pub struct CpalOutputStats {
    pub rendered_blocks: u64,
    pub underrun_samples: u64,
    pub master_guarded_samples: u64,
    pub master_non_finite_samples: u64,
    pub callback_errors: u64,
    pub playhead_sample: u64,
    pub device_lost: bool,
}

pub struct CpalAudioOutput {
    stream: Option<cpal::Stream>,
    commands: Producer<Command>,
    retired: Consumer<Box<AudioRenderGraph>>,
    counters: Arc<Counters>,
    sample_rate: u32,
    max_block_frames: usize,
    replacement_pending: bool,
    returned_graphs: Arc<Mutex<Vec<AudioRenderGraph>>>,
    #[cfg(target_os = "android")]
    midi_input_sender: Option<CpalMidiInputSender>,
    #[cfg(target_os = "android")]
    midi_output_receiver: Option<CpalMidiOutputReceiver>,
}

fn build_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut callback: Callback,
    counters: Arc<Counters>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + Sample + FromSample<f32>,
{
    device.build_output_stream(
        config,
        move |output: &mut [T], _| callback.render(output),
        move |_error| counters.device_lost.store(true, Ordering::Release),
        Some(Duration::from_secs(2)),
    )
}

impl CpalAudioOutput {
    pub fn open(
        graph: AudioRenderGraph,
        selected_device_id: Option<&str>,
    ) -> Result<Self, CpalOutputError> {
        let host = cpal::default_host();
        let device = match selected_device_id {
            Some(id) => {
                let parsed = parse_cpal_device_id(id)?;
                let device = host
                    .output_devices()?
                    .find(|device| device.id().is_ok_and(|device_id| device_id == parsed));
                require_selected_output_device(id, device)?
            }
            None => host
                .default_output_device()
                .ok_or(CpalOutputError::DeviceUnavailable)?,
        };
        let is_default_device = selected_device_id.is_none();
        let project_rate = graph.sample_rate();
        let max_block_frames = graph.max_block_frames();
        let supported = device.supported_output_configs()?;
        let (selected, buffer_size) = supported
            .filter(|range| {
                supports_project_output_config(
                    range.channels(),
                    range.sample_format(),
                    range.min_sample_rate(),
                    range.max_sample_rate(),
                    project_rate,
                )
            })
            .filter_map(|range| {
                callback_buffer_size(range.buffer_size(), max_block_frames)
                    .ok()
                    .map(|buffer_size| (range, buffer_size))
            })
            .min_by_key(|(range, _)| u8::from(range.sample_format() != SampleFormat::F32))
            .ok_or(CpalOutputError::NoStereoConfig {
                sample_rate: project_rate,
                is_default_device,
            })?;
        let sample_format = selected.sample_format();
        let mut config = selected.with_sample_rate(project_rate).config();
        config.buffer_size = buffer_size;
        let (commands, command_reader) = RingBuffer::new(COMMAND_CAPACITY);
        let (retired_writer, retired) = RingBuffer::new(RETIRED_GRAPH_CAPACITY);
        #[cfg(target_os = "android")]
        let midi_event_capacity = graph.midi_event_capacity();
        #[cfg(target_os = "android")]
        let (midi_input_sender, midi_input) = RingBuffer::new(MIDI_QUEUE_CAPACITY);
        #[cfg(target_os = "android")]
        let (midi_output, midi_output_receiver) = RingBuffer::new(MIDI_QUEUE_CAPACITY);
        let counters = Arc::new(Counters::default());
        let callback_counters = Arc::clone(&counters);
        let returned_graphs = Arc::new(Mutex::new(Vec::new()));
        let callback = Callback {
            graph: Some(Box::new(graph)),
            scratch: vec![[0.0; 2]; max_block_frames],
            commands: command_reader,
            retired: retired_writer,
            pending_retired: None,
            counters: Arc::clone(&counters),
            shutting_down: false,
            returned_graphs: Arc::clone(&returned_graphs),
            #[cfg(target_os = "android")]
            midi_input,
            #[cfg(target_os = "android")]
            midi_input_scratch: vec![None; MIDI_INPUT_EVENTS_PER_BLOCK],
            #[cfg(target_os = "android")]
            midi_output,
            #[cfg(target_os = "android")]
            midi_output_scratch: vec![None; midi_event_capacity],
            #[cfg(target_os = "android")]
            sample_rate: project_rate,
        };
        let stream = match sample_format {
            SampleFormat::F32 => build_stream::<f32>(&device, config, callback, callback_counters),
            SampleFormat::F64 => build_stream::<f64>(&device, config, callback, callback_counters),
            SampleFormat::I8 => build_stream::<i8>(&device, config, callback, callback_counters),
            SampleFormat::I16 => build_stream::<i16>(&device, config, callback, callback_counters),
            SampleFormat::I24 => {
                build_stream::<cpal::I24>(&device, config, callback, callback_counters)
            }
            SampleFormat::I32 => build_stream::<i32>(&device, config, callback, callback_counters),
            SampleFormat::I64 => build_stream::<i64>(&device, config, callback, callback_counters),
            SampleFormat::U8 => build_stream::<u8>(&device, config, callback, callback_counters),
            SampleFormat::U16 => build_stream::<u16>(&device, config, callback, callback_counters),
            SampleFormat::U24 => {
                build_stream::<cpal::U24>(&device, config, callback, callback_counters)
            }
            SampleFormat::U32 => build_stream::<u32>(&device, config, callback, callback_counters),
            SampleFormat::U64 => build_stream::<u64>(&device, config, callback, callback_counters),
            unsupported => return Err(CpalOutputError::UnsupportedSampleFormat(unsupported)),
        }?;
        stream.play()?;
        Ok(Self {
            stream: Some(stream),
            commands,
            retired,
            counters,
            sample_rate: project_rate,
            max_block_frames,
            replacement_pending: false,
            returned_graphs,
            #[cfg(target_os = "android")]
            midi_input_sender: Some(CpalMidiInputSender(midi_input_sender)),
            #[cfg(target_os = "android")]
            midi_output_receiver: Some(CpalMidiOutputReceiver(midi_output_receiver)),
        })
    }

    #[cfg(target_os = "android")]
    pub fn take_midi_input_sender(&mut self) -> Option<CpalMidiInputSender> {
        self.midi_input_sender.take()
    }

    #[cfg(target_os = "android")]
    pub fn take_midi_output_receiver(&mut self) -> Option<CpalMidiOutputReceiver> {
        self.midi_output_receiver.take()
    }

    pub fn play(&mut self) -> Result<(), CpalOutputError> {
        self.enqueue(Command::Play)
    }
    pub fn stop(&mut self) -> Result<(), CpalOutputError> {
        self.enqueue(Command::Stop)
    }
    pub fn panic_midi(&mut self) -> Result<(), CpalOutputError> {
        self.enqueue(Command::PanicMidi)
    }
    pub fn replace_graph(
        &mut self,
        graph: AudioRenderGraph,
        playing: bool,
    ) -> Result<(), CpalOutputError> {
        if self.replacement_pending {
            return Err(CpalOutputError::GraphReplacementInFlight);
        }
        if graph.sample_rate() != self.sample_rate {
            return Err(CpalOutputError::SampleRateMismatch {
                project: graph.sample_rate(),
                device: self.sample_rate,
            });
        }
        if graph.max_block_frames() < self.max_block_frames {
            return Err(CpalOutputError::DeviceBlockTooLarge {
                device: self.max_block_frames,
                maximum: graph.max_block_frames(),
            });
        }
        self.enqueue(Command::ReplaceGraph {
            graph: Box::new(graph),
            playing,
        })?;
        self.replacement_pending = true;
        Ok(())
    }
    pub fn collect_retired_graphs(&mut self) -> usize {
        let graphs = self.take_retired_graphs();
        let count = graphs.len();
        drop(graphs);
        count
    }
    pub fn take_retired_graphs(&mut self) -> Vec<AudioRenderGraph> {
        let mut graphs = Vec::new();
        while let Ok(graph) = self.retired.pop() {
            graphs.push(*graph);
        }
        if !graphs.is_empty() {
            self.replacement_pending = false;
        }
        graphs
    }
    pub fn shutdown(&mut self) -> Result<Vec<AudioRenderGraph>, CpalOutputError> {
        if self.counters.device_lost.load(Ordering::Acquire) {
            return Ok(self.shutdown_after_device_loss());
        }
        if self.stream.is_none() {
            return Ok(self.take_retired_graphs());
        }
        self.enqueue(Command::Shutdown)?;
        let deadline = Instant::now() + Duration::from_secs(2);
        while !self.counters.shutdown.load(Ordering::Acquire) {
            if self.counters.device_lost.load(Ordering::Acquire) {
                return Ok(self.shutdown_after_device_loss());
            }
            if Instant::now() >= deadline {
                return Err(CpalOutputError::ShutdownTimedOut);
            }
            thread::sleep(Duration::from_millis(1));
        }
        if let Some(stream) = self.stream.take() {
            let _ = stream.pause();
            drop(stream);
        }
        let mut graphs = self.take_retired_graphs();
        graphs.append(
            &mut self
                .returned_graphs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        Ok(graphs)
    }
    fn shutdown_after_device_loss(&mut self) -> Vec<AudioRenderGraph> {
        if let Some(stream) = self.stream.take() {
            let _ = stream.pause();
            drop(stream);
        }
        let mut graphs = self.take_retired_graphs();
        graphs.append(
            &mut self
                .returned_graphs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        for graph in &mut graphs {
            let _ = graph.release_midi_notes();
            let _ = graph
                .stop_instruments()
                .saturating_add(graph.stop_fx_processors());
        }
        self.counters.shutdown.store(true, Ordering::Release);
        graphs
    }
    pub fn stats(&self) -> CpalOutputStats {
        CpalOutputStats {
            rendered_blocks: self.counters.rendered_blocks.load(Ordering::Relaxed),
            underrun_samples: self.counters.underrun_samples.load(Ordering::Relaxed),
            master_guarded_samples: self.counters.master_guarded_samples.load(Ordering::Relaxed),
            master_non_finite_samples: self
                .counters
                .master_non_finite_samples
                .load(Ordering::Relaxed),
            callback_errors: self.counters.callback_errors.load(Ordering::Relaxed),
            playhead_sample: self.counters.playhead_sample.load(Ordering::Relaxed),
            device_lost: self.counters.device_lost.load(Ordering::Acquire),
        }
    }
    fn enqueue(&mut self, command: Command) -> Result<(), CpalOutputError> {
        self.commands
            .push(command)
            .map_err(|_| CpalOutputError::ControlQueueFull)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CpalOutputDeviceInfo {
    pub id: String,
    pub name: String,
}

fn parse_cpal_device_id(id: &str) -> Result<cpal::DeviceId, CpalOutputError> {
    cpal::DeviceId::from_str(id).map_err(|_| CpalOutputError::InvalidDeviceId(id.to_owned()))
}

fn require_selected_output_device<T>(id: &str, device: Option<T>) -> Result<T, CpalOutputError> {
    device.ok_or_else(|| CpalOutputError::SelectedDeviceUnavailable(id.to_owned()))
}

pub fn enumerate_output_devices() -> Result<Vec<CpalOutputDeviceInfo>, CpalOutputError> {
    let host = cpal::default_host();
    host.output_devices()?
        .map(|device| {
            Ok(CpalOutputDeviceInfo {
                id: device.id()?.to_string(),
                name: device.description()?.name().to_owned(),
            })
        })
        .collect()
}

impl Drop for CpalAudioOutput {
    fn drop(&mut self) {
        if self.stream.is_some() {
            let _ = self.shutdown();
        }
        if let Some(stream) = self.stream.take() {
            let _ = stream.pause();
            drop(stream);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aaadaw_core::{DawAction, Project};
    use cpal::FrameCount;

    fn fixture_device_id(id: &str) -> String {
        let host = if cfg!(target_os = "windows") {
            "wasapi"
        } else {
            "coreaudio"
        };
        format!("{host}:{id}")
    }

    #[test]
    fn persisted_cpal_device_ids_parse_and_invalid_ids_are_reported() {
        assert!(parse_cpal_device_id(&fixture_device_id("mock-endpoint")).is_ok());
        assert!(matches!(
            parse_cpal_device_id("not-a-device-id"),
            Err(CpalOutputError::InvalidDeviceId(_))
        ));
    }

    #[test]
    fn missing_selected_device_is_an_error_instead_of_default_device_fallback() {
        let disconnected_id = fixture_device_id("disconnected");
        assert!(matches!(
            require_selected_output_device::<()>(&disconnected_id, None),
            Err(CpalOutputError::SelectedDeviceUnavailable(id)) if id == disconnected_id
        ));
        assert_eq!(
            require_selected_output_device("cpal:present", Some(17)).unwrap(),
            17
        );
    }

    struct CallbackFixture {
        callback: Callback,
        commands: Producer<Command>,
        retired: Consumer<Box<AudioRenderGraph>>,
        counters: Arc<Counters>,
        returned_graphs: Arc<Mutex<Vec<AudioRenderGraph>>>,
    }

    fn callback_state(max_block_frames: usize) -> CallbackFixture {
        let project = Project::new();
        let graph = AudioRenderGraph::new(&project, Vec::new(), max_block_frames).unwrap();
        let (command_writer, commands) = RingBuffer::new(COMMAND_CAPACITY);
        let (retired, retired_reader) = RingBuffer::new(RETIRED_GRAPH_CAPACITY);
        let counters = Arc::new(Counters::default());
        let returned_graphs = Arc::new(Mutex::new(Vec::new()));
        let callback = Callback {
            graph: Some(Box::new(graph)),
            scratch: vec![[0.0; 2]; max_block_frames],
            commands,
            retired,
            pending_retired: None,
            counters: Arc::clone(&counters),
            shutting_down: false,
            returned_graphs: Arc::clone(&returned_graphs),
        };
        CallbackFixture {
            callback,
            commands: command_writer,
            retired: retired_reader,
            counters,
            returned_graphs,
        }
    }

    #[test]
    fn shared_mode_config_policy_requires_stereo_pcm_and_project_rate_support() {
        assert!(supports_project_output_config(
            2,
            SampleFormat::F32,
            44_100,
            96_000,
            48_000
        ));
        assert!(supports_project_output_config(
            2,
            SampleFormat::I16,
            44_100,
            48_000,
            44_100
        ));
        assert!(!supports_project_output_config(
            1,
            SampleFormat::F32,
            44_100,
            96_000,
            48_000
        ));
        assert!(!supports_project_output_config(
            6,
            SampleFormat::F32,
            44_100,
            96_000,
            48_000
        ));
        assert!(!supports_project_output_config(
            2,
            SampleFormat::F32,
            48_001,
            96_000,
            48_000
        ));
        assert!(!supports_project_output_config(
            2,
            SampleFormat::DsdU8,
            44_100,
            96_000,
            48_000
        ));
    }

    #[test]
    fn callback_buffer_policy_stays_within_device_and_graph_limits() {
        let range = SupportedBufferSize::Range {
            min: 128 as FrameCount,
            max: 1_024 as FrameCount,
        };
        assert_eq!(
            callback_buffer_size(&range, 2_048).unwrap(),
            BufferSize::Fixed(512)
        );
        assert_eq!(
            callback_buffer_size(&range, 256).unwrap(),
            BufferSize::Fixed(256)
        );
        assert!(matches!(
            callback_buffer_size(&range, 64),
            Err(CpalOutputError::DeviceBlockTooLarge { .. })
        ));
        assert!(matches!(
            callback_buffer_size(&SupportedBufferSize::Unknown, 2_048),
            Err(CpalOutputError::UnknownBufferSize)
        ));
    }

    #[test]
    fn output_conversion_preserves_interleaved_stereo_channel_order() {
        let mut output = [0_i16; 4];
        convert_stereo_output(&mut output, &[[0.5, -0.5], [1.0, -1.0]]);
        assert_eq!(output, [16_384, -16_384, i16::MAX, i16::MIN]);
    }

    #[test]
    fn callback_counts_missing_pcm_as_underrun_samples() {
        let CallbackFixture {
            mut callback,
            counters,
            ..
        } = callback_state(4);
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Audio".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: "asset://silence".to_owned(),
                start_sample: 0,
                source_offset_samples: 0,
                length_samples: 4,
            })
            .unwrap();
        let item_id = project.audio_items()[0].id();
        let (_producer, consumer) = crate::pcm_stream(4).unwrap();
        let mut graph = AudioRenderGraph::new_for_audio_items(
            &project,
            vec![crate::AudioItemStream::new(item_id, consumer)],
            4,
        )
        .unwrap();
        graph.transport_mut().start();
        callback.graph = Some(Box::new(graph));

        callback.render(&mut [0.0_f32; 8]);

        assert_eq!(counters.underrun_samples.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn callback_converts_float_samples_and_rejects_invalid_stereo_blocks() {
        assert_eq!(i16::from_sample(0.5_f32), 16_384);
        let CallbackFixture {
            mut callback,
            counters,
            ..
        } = callback_state(4);
        let mut valid_block = [1.0_f32; 8];
        callback.render(&mut valid_block);
        assert_eq!(valid_block, [0.0; 8]);

        let mut odd_channel_block = [1.0_f32; 3];
        callback.render(&mut odd_channel_block);
        assert_eq!(odd_channel_block, [0.0; 3]);
        assert_eq!(counters.callback_errors.load(Ordering::Relaxed), 1);

        let mut oversized_block = [1.0_f32; 10];
        callback.render(&mut oversized_block);
        assert_eq!(oversized_block, [0.0; 10]);
        assert_eq!(counters.callback_errors.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn callback_applies_transport_and_returns_the_shutdown_graph_off_callback() {
        let CallbackFixture {
            mut callback,
            mut commands,
            counters,
            returned_graphs,
            ..
        } = callback_state(4);
        commands.push(Command::Play).unwrap();
        let mut block = [0.0_f32; 8];
        callback.render(&mut block);
        assert_eq!(counters.playhead_sample.load(Ordering::Relaxed), 4);

        commands.push(Command::Stop).unwrap();
        callback.render(&mut block);
        assert_eq!(counters.playhead_sample.load(Ordering::Relaxed), 4);

        commands.push(Command::Shutdown).unwrap();
        callback.render(&mut block);
        assert!(counters.shutdown.load(Ordering::Acquire));
        drop(callback);
        assert_eq!(returned_graphs.lock().unwrap().len(), 1);
    }

    #[test]
    fn callback_retires_replaced_graph_for_control_thread_destruction() {
        let CallbackFixture {
            mut callback,
            mut commands,
            mut retired,
            ..
        } = callback_state(4);
        let replacement = AudioRenderGraph::new(&Project::new(), Vec::new(), 4).unwrap();
        commands
            .push(Command::ReplaceGraph {
                graph: Box::new(replacement),
                playing: false,
            })
            .unwrap();
        callback.render(&mut [0.0_f32; 8]);

        assert!(retired.pop().is_ok());
    }

    #[test]
    fn device_loss_shutdown_reclaims_graphs_without_waiting_for_a_callback() {
        let graph = AudioRenderGraph::new(&Project::new(), Vec::new(), 4).unwrap();
        let (_retired_writer, retired) = RingBuffer::new(RETIRED_GRAPH_CAPACITY);
        let counters = Arc::new(Counters::default());
        counters.device_lost.store(true, Ordering::Release);
        let returned_graphs = Arc::new(Mutex::new(vec![graph]));
        let mut output = CpalAudioOutput {
            stream: None,
            commands: RingBuffer::new(COMMAND_CAPACITY).0,
            retired,
            counters: Arc::clone(&counters),
            sample_rate: 48_000,
            max_block_frames: 4,
            replacement_pending: false,
            returned_graphs: Arc::clone(&returned_graphs),
        };

        let graphs = output.shutdown().unwrap();
        assert_eq!(graphs.len(), 1);
        assert!(counters.shutdown.load(Ordering::Acquire));
    }

    #[test]
    fn removed_default_device_silences_output_and_stops_transport_progress() {
        let CallbackFixture {
            mut callback,
            mut commands,
            counters,
            ..
        } = callback_state(4);
        commands.push(Command::Play).unwrap();
        counters.device_lost.store(true, Ordering::Release);
        let mut block = [1.0_f32; 8];
        callback.render(&mut block);
        assert_eq!(block, [0.0; 8]);
        assert_eq!(counters.playhead_sample.load(Ordering::Relaxed), 0);
    }
}
