use crate::AudioRenderGraph;
use crate::wasapi_common::is_supported_pcm_format;
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

fn callback_buffer_size(
    supported: &SupportedBufferSize,
    graph_capacity: usize,
) -> Result<BufferSize, WasapiOutputError> {
    match supported {
        SupportedBufferSize::Range { min, max } => {
            let capacity = u32::try_from(graph_capacity).unwrap_or(u32::MAX);
            let largest_supported = (*max).min(capacity);
            if largest_supported < *min {
                return Err(WasapiOutputError::DeviceBlockTooLarge {
                    device: *min as usize,
                    maximum: graph_capacity,
                });
            }
            Ok(BufferSize::Fixed(
                PREFERRED_CALLBACK_FRAMES.clamp(*min, largest_supported),
            ))
        }
        SupportedBufferSize::Unknown => Err(WasapiOutputError::UnknownBufferSize),
    }
}

fn convert_stereo_output<T>(output: &mut [T], frames: &[[f32; 2]])
where
    T: Sample + FromSample<f32>,
{
    for (out, frame) in output.chunks_exact_mut(2).zip(frames) {
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
}

impl Callback {
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
                }
                Command::PanicMidi => {
                    if let Some(graph) = &mut self.graph {
                        let failures = graph.release_midi_notes();
                        self.counters
                            .callback_errors
                            .fetch_add(failures as u64, Ordering::Relaxed);
                    }
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
                    if let Some(retired) = self.graph.replace(graph) {
                        if let Err(PushError::Full(retired)) = self.retired.push(retired) {
                            self.pending_retired = Some(retired);
                        }
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
        if output.len() % 2 != 0 || output.len() / 2 > self.scratch.len() {
            self.counters
                .callback_errors
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        let frames = output.len() / 2;
        match graph.render_into(&mut self.scratch[..frames]) {
            Ok(stats) => {
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
pub enum WasapiOutputError {
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

impl fmt::Display for WasapiOutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeviceUnavailable => f.write_str("Windows has no default playback device"),
            Self::SelectedDeviceUnavailable(id) => {
                write!(f, "selected WASAPI output device is unavailable ({id})")
            }
            Self::InvalidDeviceId(id) => {
                write!(f, "saved WASAPI output device ID is invalid ({id})")
            }
            Self::Cpal(error) => write!(f, "WASAPI output error: {error}"),
            Self::NoStereoConfig {
                sample_rate,
                is_default_device,
            } => write!(
                f,
                "{} WASAPI device has no stereo PCM configuration supporting {sample_rate} Hz",
                if *is_default_device {
                    "default"
                } else {
                    "selected"
                }
            ),
            Self::UnsupportedSampleFormat(format) => write!(
                f,
                "WASAPI sample format {format:?} is not supported for output"
            ),
            Self::SampleRateMismatch { project, device } => write!(
                f,
                "project sample rate {project} Hz does not match WASAPI rate {device} Hz"
            ),
            Self::UnknownBufferSize => {
                f.write_str("WASAPI did not report a supported output buffer size")
            }
            Self::DeviceBlockTooLarge { device, maximum } => write!(
                f,
                "WASAPI callback block {device} exceeds render capacity {maximum}"
            ),
            Self::ControlQueueFull => f.write_str("WASAPI transport command queue is full"),
            Self::GraphReplacementInFlight => {
                f.write_str("WASAPI graph replacement is still in flight")
            }
            Self::ShutdownTimedOut => {
                f.write_str("WASAPI output callback did not acknowledge shutdown")
            }
        }
    }
}
impl StdError for WasapiOutputError {}
impl From<cpal::Error> for WasapiOutputError {
    fn from(error: cpal::Error) -> Self {
        Self::Cpal(error)
    }
}

pub struct WasapiOutputStats {
    pub rendered_blocks: u64,
    pub underrun_samples: u64,
    pub master_guarded_samples: u64,
    pub master_non_finite_samples: u64,
    pub callback_errors: u64,
    pub playhead_sample: u64,
    pub device_lost: bool,
}

pub struct WasapiAudioOutput {
    stream: Option<cpal::Stream>,
    commands: Producer<Command>,
    retired: Consumer<Box<AudioRenderGraph>>,
    counters: Arc<Counters>,
    sample_rate: u32,
    max_block_frames: usize,
    replacement_pending: bool,
    returned_graphs: Arc<Mutex<Vec<AudioRenderGraph>>>,
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

impl WasapiAudioOutput {
    pub fn open(
        graph: AudioRenderGraph,
        selected_device_id: Option<&str>,
    ) -> Result<Self, WasapiOutputError> {
        let host = cpal::default_host();
        let device = match selected_device_id {
            Some(id) => {
                let parsed = parse_wasapi_device_id(id)?;
                let device = host
                    .output_devices()?
                    .find(|device| device.id().is_ok_and(|device_id| device_id == parsed));
                require_selected_output_device(id, device)?
            }
            None => host
                .default_output_device()
                .ok_or(WasapiOutputError::DeviceUnavailable)?,
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
            .ok_or(WasapiOutputError::NoStereoConfig {
                sample_rate: project_rate,
                is_default_device,
            })?;
        let sample_format = selected.sample_format();
        let mut config = selected.with_sample_rate(project_rate).config();
        config.buffer_size = buffer_size;
        let (commands, command_reader) = RingBuffer::new(COMMAND_CAPACITY);
        let (retired_writer, retired) = RingBuffer::new(RETIRED_GRAPH_CAPACITY);
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
            unsupported => return Err(WasapiOutputError::UnsupportedSampleFormat(unsupported)),
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
        })
    }

    pub fn play(&mut self) -> Result<(), WasapiOutputError> {
        self.enqueue(Command::Play)
    }
    pub fn stop(&mut self) -> Result<(), WasapiOutputError> {
        self.enqueue(Command::Stop)
    }
    pub fn panic_midi(&mut self) -> Result<(), WasapiOutputError> {
        self.enqueue(Command::PanicMidi)
    }
    pub fn replace_graph(
        &mut self,
        graph: AudioRenderGraph,
        playing: bool,
    ) -> Result<(), WasapiOutputError> {
        if self.replacement_pending {
            return Err(WasapiOutputError::GraphReplacementInFlight);
        }
        if graph.sample_rate() != self.sample_rate {
            return Err(WasapiOutputError::SampleRateMismatch {
                project: graph.sample_rate(),
                device: self.sample_rate,
            });
        }
        if graph.max_block_frames() < self.max_block_frames {
            return Err(WasapiOutputError::DeviceBlockTooLarge {
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
    pub fn shutdown(&mut self) -> Result<Vec<AudioRenderGraph>, WasapiOutputError> {
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
                return Err(WasapiOutputError::ShutdownTimedOut);
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
    pub fn stats(&self) -> WasapiOutputStats {
        WasapiOutputStats {
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
    fn enqueue(&mut self, command: Command) -> Result<(), WasapiOutputError> {
        self.commands
            .push(command)
            .map_err(|_| WasapiOutputError::ControlQueueFull)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasapiOutputDeviceInfo {
    pub id: String,
    pub name: String,
}

fn parse_wasapi_device_id(id: &str) -> Result<cpal::DeviceId, WasapiOutputError> {
    cpal::DeviceId::from_str(id).map_err(|_| WasapiOutputError::InvalidDeviceId(id.to_owned()))
}

fn require_selected_output_device<T>(id: &str, device: Option<T>) -> Result<T, WasapiOutputError> {
    device.ok_or_else(|| WasapiOutputError::SelectedDeviceUnavailable(id.to_owned()))
}

pub fn enumerate_output_devices() -> Result<Vec<WasapiOutputDeviceInfo>, WasapiOutputError> {
    let host = cpal::default_host();
    host.output_devices()?
        .map(|device| {
            Ok(WasapiOutputDeviceInfo {
                id: device.id()?.to_string(),
                name: device.description()?.name().to_owned(),
            })
        })
        .collect()
}

impl Drop for WasapiAudioOutput {
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

    #[test]
    fn persisted_wasapi_device_ids_parse_and_invalid_ids_are_reported() {
        assert!(parse_wasapi_device_id("wasapi:mock-endpoint").is_ok());
        assert!(matches!(
            parse_wasapi_device_id("not-a-device-id"),
            Err(WasapiOutputError::InvalidDeviceId(_))
        ));
    }

    #[test]
    fn missing_selected_device_is_an_error_instead_of_default_device_fallback() {
        assert!(matches!(
            require_selected_output_device::<()>("wasapi:disconnected", None),
            Err(WasapiOutputError::SelectedDeviceUnavailable(id)) if id == "wasapi:disconnected"
        ));
        assert_eq!(
            require_selected_output_device("wasapi:present", Some(17)).unwrap(),
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
            Err(WasapiOutputError::DeviceBlockTooLarge { .. })
        ));
        assert!(matches!(
            callback_buffer_size(&SupportedBufferSize::Unknown, 2_048),
            Err(WasapiOutputError::UnknownBufferSize)
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
        let mut output = WasapiAudioOutput {
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
