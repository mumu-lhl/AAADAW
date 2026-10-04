//! Application-layer orchestration for preparing project audio playback.
//!
//! This crate resolves opaque media references through storage and connects background media
//! feeders to the realtime render graph. It also coordinates cancellable asset imports and turns
//! them into project actions. Call its APIs from a control thread, never an audio callback.

mod asset_management;
mod audio_editing;
mod audio_import;
mod capture_timeline;
mod clap_plugins;
mod live_recording;
mod midi_editing;
mod offline_render;
mod wav_export;
mod waveform;

pub use aaadaw_engine::ClapPluginDescriptor;
pub use aaadaw_engine::{
    AudioInputMonitorController, AudioInputMonitorGate, AudioMonitorConsumer, AudioMonitorProducer,
    MasterOutputCeiling, MasterOutputSafetyError,
};
pub use aaadaw_storage::AudioAssetSourceStatus;
pub use asset_management::{
    AudioAssetManagementOperation, AudioAssetManagementProgress, AudioAssetManagementResult,
    AudioAssetManagementWorker, AudioAssetSourceStatusEntry, relink_external_audio_source,
    start_audio_asset_management,
};
pub use audio_editing::{AudioEditError, duplicate_audio_item, set_audio_item_start_sample};
pub use audio_import::{
    AudioItemImportError, AudioItemImportProgress, AudioItemImportWorker,
    cleanup_unplaced_audio_assets, start_audio_item_import, start_audio_item_reimport,
};
pub use capture_timeline::CaptureTimelineAnchor;
pub use clap_plugins::{
    ClapPluginScanError, ClapPluginScanReport, default_clap_search_paths, scan_clap_plugins,
};
pub use live_recording::{
    AudioRecordingError, AudioRecordingWorker, RecordingRecoveryCandidate,
    RecordingRecoveryManifest, RecordingSegment, discard_recording_recovery,
    recover_recording_candidate, scan_recording_recoveries,
};
pub use midi_editing::{
    MidiEditError, add_quarter_note, adjust_midi_note_pitch, adjust_midi_note_velocity,
    create_four_beat_midi_item, delete_midi_note, move_midi_item_by_beat,
    move_midi_note_by_sixteenth, quantize_midi_item_to_sixteenth,
};
pub use offline_render::{
    DEFAULT_EFFECT_TAIL_SECONDS, OfflineRenderError, project_render_length_samples,
    render_graph_to_pcm24_wav, render_prepared_audio_to_pcm24_wav,
    render_project_file_to_pcm24_wav,
};
pub use wav_export::{Pcm24WavExport, Pcm24WavExportError};
pub use waveform::{AudioWaveformResult, AudioWaveformWorker};

use aaadaw_core::Project;
#[cfg(feature = "audio-device")]
use aaadaw_engine::MasterOutputSafetyController;
#[cfg(feature = "audio-device")]
use aaadaw_engine::StoppedTrackFxProcessor;
#[cfg(feature = "audio-device")]
use aaadaw_engine::TrackMixController;
#[cfg(feature = "audio-device")]
use aaadaw_engine::TransportClockAnchor;
use aaadaw_engine::{
    AudioGraphBuildError, AudioItemStream, AudioRenderGraph, PcmStreamError, audio_monitor_stream,
    pcm_stream,
};
#[cfg(feature = "jack-backend")]
use aaadaw_engine::{JackAudioOutput, JackOutputError, JackOutputStats};
#[cfg(feature = "pipewire-backend")]
use aaadaw_engine::{PipeWireAudioOutput, PipeWireOutputError, PipeWireOutputStats};
#[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
use aaadaw_engine::{WasapiAudioInput, WasapiAudioOutput, WasapiOutputError, WasapiOutputStats};
use aaadaw_media::{
    AudioFeedWorker, MediaError, spawn_audio_item_stream, spawn_audio_item_stream_at,
    spawn_audio_item_stream_from_reader, spawn_audio_item_stream_from_reader_at,
};
use aaadaw_storage::{ProjectStore, ResolvedAudioAsset, StorageError};
use std::error::Error as StdError;
use std::fmt;
use std::io::ErrorKind;
use std::path::Path;

pub use aaadaw_engine::{
    AudioCaptureConsumer, AudioCaptureControl, AudioCaptureProducer, audio_capture_stream,
};

/// Active audio input capture device.
#[cfg(feature = "audio-device")]
pub enum RunningAudioInput {
    #[cfg(feature = "jack-backend")]
    Jack(aaadaw_engine::JackAudioInput),
    #[cfg(feature = "pipewire-backend")]
    PipeWire(aaadaw_engine::PipeWireAudioInput),
    #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
    Wasapi(WasapiAudioInput),
    #[cfg(not(any(
        feature = "jack-backend",
        feature = "pipewire-backend",
        all(feature = "wasapi-backend", target_os = "windows")
    )))]
    #[allow(dead_code)]
    Unavailable,
}

/// An already-open input stream held while an armed track is monitored before recording.
#[cfg(feature = "audio-device")]
pub struct StandbyAudioInput {
    input: RunningAudioInput,
    consumer: AudioCaptureConsumer,
    control: AudioCaptureControl,
}

#[cfg(feature = "audio-device")]
impl StandbyAudioInput {
    pub fn new(
        input: RunningAudioInput,
        consumer: AudioCaptureConsumer,
        control: AudioCaptureControl,
    ) -> Self {
        Self {
            input,
            consumer,
            control,
        }
    }

    pub fn into_recording_parts(
        self,
    ) -> (RunningAudioInput, AudioCaptureConsumer, AudioCaptureControl) {
        (self.input, self.consumer, self.control)
    }

    pub fn shutdown(self) {
        self.input.shutdown();
    }
}

/// Result of mapping the output clock into the active input clock domain.
#[cfg(feature = "audio-device")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SharedFrameClockMapping {
    /// The input backend does not share an output frame clock.
    Unsupported,
    /// The backend shares the clock, but either the output anchor or input clock sample is absent.
    Unavailable,
    /// The output frame expressed in the input backend's extended clock domain.
    Mapped(u64),
}

/// Opens the selected native input backend for a stereo take.
#[cfg(feature = "audio-device")]
#[allow(unused_variables)]
pub fn open_audio_input(
    backend: PlaybackBackend,
    mut producer: AudioCaptureProducer,
    monitor: Option<AudioMonitorProducer>,
    control: AudioCaptureControl,
    sample_rate: u32,
) -> Result<RunningAudioInput, String> {
    if let Some(monitor) = monitor {
        producer.attach_monitor(monitor);
    }
    match backend {
        #[cfg(feature = "jack-backend")]
        PlaybackBackend::Jack => {
            aaadaw_engine::JackAudioInput::open(producer, control, sample_rate)
                .map(RunningAudioInput::Jack)
                .map_err(|error| error.to_string())
        }
        #[cfg(feature = "pipewire-backend")]
        PlaybackBackend::PipeWire => {
            aaadaw_engine::PipeWireAudioInput::open(producer, control, sample_rate)
                .map(RunningAudioInput::PipeWire)
                .map_err(|error| error.to_string())
        }
        #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
        PlaybackBackend::Wasapi => WasapiAudioInput::open(producer, control, sample_rate)
            .map(RunningAudioInput::Wasapi)
            .map_err(|error| error.to_string()),
        #[cfg(all(
            feature = "audio-device",
            not(any(
                feature = "jack-backend",
                feature = "pipewire-backend",
                all(feature = "wasapi-backend", target_os = "windows")
            ))
        ))]
        PlaybackBackend::Unavailable => Err("WASAPI is only available on Windows".to_owned()),
    }
}

#[cfg(feature = "audio-device")]
impl RunningAudioInput {
    /// Maps an optional playback frame anchor into the input backend's extended clock domain.
    pub fn map_shared_frame_time(&self, _frame: Option<u32>) -> SharedFrameClockMapping {
        match self {
            #[cfg(feature = "jack-backend")]
            Self::Jack(input) => _frame
                .and_then(|frame| input.map_shared_frame_time(frame))
                .map_or(
                    SharedFrameClockMapping::Unavailable,
                    SharedFrameClockMapping::Mapped,
                ),
            #[cfg(feature = "pipewire-backend")]
            Self::PipeWire(_) => SharedFrameClockMapping::Unsupported,
            #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
            Self::Wasapi(_) => SharedFrameClockMapping::Unsupported,
            #[cfg(not(any(
                feature = "jack-backend",
                feature = "pipewire-backend",
                all(feature = "wasapi-backend", target_os = "windows")
            )))]
            _ => SharedFrameClockMapping::Unsupported,
        }
    }

    /// Returns a precise capture-path latency reported by the input backend, if available.
    pub fn reported_capture_latency_frames(&self) -> Option<u32> {
        match self {
            #[cfg(feature = "jack-backend")]
            Self::Jack(input) => input.reported_capture_latency_frames(),
            #[cfg(feature = "pipewire-backend")]
            Self::PipeWire(_) => None,
            #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
            Self::Wasapi(_) => None,
            #[cfg(not(any(
                feature = "jack-backend",
                feature = "pipewire-backend",
                all(feature = "wasapi-backend", target_os = "windows")
            )))]
            _ => None,
        }
    }

    /// Stops the input callback and releases the device.
    pub fn shutdown(self) {
        match self {
            #[cfg(feature = "jack-backend")]
            Self::Jack(mut input) => input.shutdown(),
            #[cfg(feature = "pipewire-backend")]
            Self::PipeWire(mut input) => input.shutdown(),
            #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
            Self::Wasapi(mut input) => input.shutdown(),
            #[cfg(not(any(
                feature = "jack-backend",
                feature = "pipewire-backend",
                all(feature = "wasapi-backend", target_os = "windows")
            )))]
            Self::Unavailable => {}
        }
    }
}

/// Errors while resolving project media and preparing its render graph.
#[derive(Debug)]
pub enum PlaybackBuildError {
    Storage(StorageError),
    Media(MediaError),
    PcmStream(PcmStreamError),
    AudioGraph(AudioGraphBuildError),
    #[cfg(all(
        feature = "audio-device",
        not(any(
            feature = "jack-backend",
            feature = "pipewire-backend",
            all(feature = "wasapi-backend", target_os = "windows")
        ))
    ))]
    NoAudioOutputBackend,
    #[cfg(feature = "jack-backend")]
    Jack(JackOutputError),
    #[cfg(feature = "pipewire-backend")]
    PipeWire(PipeWireOutputError),
    #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
    Wasapi(WasapiOutputError),
    ExternalSourceUnavailable {
        media_ref: String,
    },
}

impl fmt::Display for PlaybackBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "project media resolution failed: {error}"),
            Self::Media(error) => write!(formatter, "audio feeder could not start: {error}"),
            Self::PcmStream(error) => write!(formatter, "PCM stream setup failed: {error}"),
            Self::AudioGraph(error) => write!(formatter, "render graph setup failed: {error}"),
            #[cfg(all(
                feature = "audio-device",
                not(any(
                    feature = "jack-backend",
                    feature = "pipewire-backend",
                    all(feature = "wasapi-backend", target_os = "windows")
                ))
            ))]
            Self::NoAudioOutputBackend => {
                formatter.write_str("no audio output backend is available on this platform")
            }
            #[cfg(feature = "jack-backend")]
            Self::Jack(error) => write!(formatter, "JACK output setup failed: {error}"),
            #[cfg(feature = "pipewire-backend")]
            Self::PipeWire(error) => write!(formatter, "PipeWire output setup failed: {error}"),
            #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
            Self::Wasapi(error) => write!(formatter, "WASAPI output setup failed: {error}"),
            Self::ExternalSourceUnavailable { media_ref } => {
                write!(
                    formatter,
                    "external source for {media_ref:?} is unavailable"
                )
            }
        }
    }
}

impl StdError for PlaybackBuildError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::Media(error) => Some(error),
            Self::PcmStream(error) => Some(error),
            Self::AudioGraph(error) => Some(error),
            #[cfg(feature = "jack-backend")]
            Self::Jack(error) => Some(error),
            #[cfg(feature = "pipewire-backend")]
            Self::PipeWire(error) => Some(error),
            #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
            Self::Wasapi(error) => Some(error),
            #[cfg(all(
                feature = "audio-device",
                not(any(
                    feature = "jack-backend",
                    feature = "pipewire-backend",
                    all(feature = "wasapi-backend", target_os = "windows")
                ))
            ))]
            Self::NoAudioOutputBackend => None,
            Self::ExternalSourceUnavailable { .. } => None,
        }
    }
}

/// A prepared project graph together with the workers that feed its item queues.
///
/// Keep this value alive while rendering. Dropping it requests cancellation of the feeders.
pub struct PreparedAudioPlayback {
    graph: AudioRenderGraph,
    feeders: Vec<AudioFeedWorker>,
    input_monitor_producer: Option<AudioMonitorProducer>,
    input_monitor_consumer: AudioMonitorConsumer,
    input_monitor_gate: AudioInputMonitorGate,
}

impl PreparedAudioPlayback {
    /// Returns the prepared realtime render graph.
    pub fn graph(&self) -> &AudioRenderGraph {
        &self.graph
    }

    /// Returns mutable graph access for transport control and rendering.
    pub fn graph_mut(&mut self) -> &mut AudioRenderGraph {
        &mut self.graph
    }

    pub fn clone_input_monitor_queue(&self) -> (AudioMonitorConsumer, AudioInputMonitorGate) {
        (
            self.input_monitor_consumer.clone(),
            self.input_monitor_gate.clone(),
        )
    }

    /// Sets this prepared graph's final Master sample-peak ceiling.
    pub fn set_master_output_ceiling_dbfs(&self, ceiling: MasterOutputCeiling) {
        self.graph
            .master_output_safety_controller()
            .set_ceiling(ceiling)
    }

    /// Returns the number of background media feeders owned by this playback.
    pub fn feeder_count(&self) -> usize {
        self.feeders.len()
    }

    /// Transfers the graph and feeder workers to a device-backend owner.
    ///
    /// The caller must keep the workers alive for as long as the graph consumes their queues.
    pub fn into_parts(self) -> (AudioRenderGraph, Vec<AudioFeedWorker>) {
        (self.graph, self.feeders)
    }

    /// Transfers the input-callback side of the graph's optional monitoring queue.
    pub fn take_input_monitor_producer(&mut self) -> Option<AudioMonitorProducer> {
        self.input_monitor_producer.take()
    }

    /// Opens JACK output and retains feeder workers for the lifetime of playback.
    #[cfg(feature = "jack-backend")]
    pub fn into_jack_output(self) -> Result<RunningJackPlayback, PlaybackBuildError> {
        let mix_controller = self.graph.track_mix_controller();
        let master_output_safety = self.graph.master_output_safety_controller();
        let input_monitor_controller = self.graph.input_monitor_controller();
        let mut this = self;
        let input_monitor_producer = this.take_input_monitor_producer();
        let (input_monitor_consumer, input_monitor_gate) = this.clone_input_monitor_queue();
        let (graph, feeders) = this.into_parts();
        let output = JackAudioOutput::open(graph).map_err(PlaybackBuildError::Jack)?;
        Ok(RunningJackPlayback {
            output,
            mix_controller,
            master_output_safety,
            input_monitor_controller,
            input_monitor_producer,
            input_monitor_consumer,
            input_monitor_gate,
            feeders,
            retired_feeders: None,
            retired_instrument_processors: Vec::new(),
            retired_fx_processors: Vec::new(),
            is_playing: false,
        })
    }

    /// Opens the selected device output and retains feeder workers for its lifetime.
    #[cfg(any(
        feature = "jack-backend",
        feature = "pipewire-backend",
        all(feature = "wasapi-backend", target_os = "windows")
    ))]
    pub fn into_output(
        self,
        backend: PlaybackBackend,
    ) -> Result<RunningAudioPlayback, PlaybackBuildError> {
        let mix_controller = self.graph.track_mix_controller();
        let master_output_safety = self.graph.master_output_safety_controller();
        let input_monitor_controller = self.graph.input_monitor_controller();
        let mut this = self;
        let input_monitor_producer = this.take_input_monitor_producer();
        let (input_monitor_consumer, input_monitor_gate) = this.clone_input_monitor_queue();
        let (graph, feeders) = this.into_parts();
        let output = match backend {
            #[cfg(feature = "jack-backend")]
            PlaybackBackend::Jack => DeviceAudioOutput::Jack(
                JackAudioOutput::open(graph).map_err(PlaybackBuildError::Jack)?,
            ),
            #[cfg(feature = "pipewire-backend")]
            PlaybackBackend::PipeWire => DeviceAudioOutput::PipeWire(
                PipeWireAudioOutput::open(graph).map_err(PlaybackBuildError::PipeWire)?,
            ),
            #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
            PlaybackBackend::Wasapi => DeviceAudioOutput::Wasapi(
                WasapiAudioOutput::open(graph).map_err(PlaybackBuildError::Wasapi)?,
            ),
        };
        Ok(RunningAudioPlayback {
            output,
            mix_controller,
            master_output_safety,
            input_monitor_controller,
            input_monitor_producer,
            input_monitor_consumer,
            input_monitor_gate,
            standby_input: None,
            feeders,
            retired_feeders: None,
            retired_instrument_processors: Vec::new(),
            retired_fx_processors: Vec::new(),
            is_playing: false,
        })
    }

    #[cfg(all(
        feature = "audio-device",
        not(any(
            feature = "jack-backend",
            feature = "pipewire-backend",
            all(feature = "wasapi-backend", target_os = "windows")
        ))
    ))]
    pub fn into_output(
        self,
        _backend: PlaybackBackend,
    ) -> Result<RunningAudioPlayback, PlaybackBuildError> {
        Err(PlaybackBuildError::NoAudioOutputBackend)
    }
}

/// Available native output backends in this build.
#[cfg(feature = "audio-device")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PlaybackBackend {
    #[cfg(feature = "jack-backend")]
    #[cfg_attr(feature = "jack-backend", default)]
    Jack,
    #[cfg(feature = "pipewire-backend")]
    #[cfg_attr(
        all(
            not(feature = "jack-backend"),
            not(all(feature = "wasapi-backend", target_os = "windows"))
        ),
        default
    )]
    PipeWire,
    #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
    #[cfg_attr(
        all(not(feature = "jack-backend"), not(feature = "pipewire-backend")),
        default
    )]
    Wasapi,
    #[cfg(all(
        feature = "audio-device",
        not(any(
            feature = "jack-backend",
            feature = "pipewire-backend",
            all(feature = "wasapi-backend", target_os = "windows")
        ))
    ))]
    #[default]
    #[allow(dead_code)]
    Unavailable,
}

#[cfg(feature = "audio-device")]
impl PlaybackBackend {
    pub fn is_available(self) -> bool {
        match self {
            #[cfg(feature = "jack-backend")]
            Self::Jack => true,
            #[cfg(feature = "pipewire-backend")]
            Self::PipeWire => true,
            #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
            Self::Wasapi => true,
            #[cfg(all(
                feature = "audio-device",
                not(any(
                    feature = "jack-backend",
                    feature = "pipewire-backend",
                    all(feature = "wasapi-backend", target_os = "windows")
                ))
            ))]
            Self::Unavailable => false,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            #[cfg(feature = "jack-backend")]
            Self::Jack => "JACK",
            #[cfg(feature = "pipewire-backend")]
            Self::PipeWire => "PipeWire",
            #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
            Self::Wasapi => "WASAPI",
            #[cfg(all(
                feature = "audio-device",
                not(any(
                    feature = "jack-backend",
                    feature = "pipewire-backend",
                    all(feature = "wasapi-backend", target_os = "windows")
                ))
            ))]
            Self::Unavailable => "Unavailable",
        }
    }
}

#[cfg(feature = "audio-device")]
enum DeviceAudioOutput {
    #[cfg(feature = "jack-backend")]
    Jack(JackAudioOutput),
    #[cfg(feature = "pipewire-backend")]
    PipeWire(PipeWireAudioOutput),
    #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
    Wasapi(WasapiAudioOutput),
    #[cfg(all(
        feature = "audio-device",
        not(any(
            feature = "jack-backend",
            feature = "pipewire-backend",
            all(feature = "wasapi-backend", target_os = "windows")
        ))
    ))]
    #[allow(dead_code)]
    Unavailable,
}

/// Active device playback and its source feeder workers.
#[cfg(feature = "audio-device")]
pub struct RunningAudioPlayback {
    output: DeviceAudioOutput,
    mix_controller: TrackMixController,
    master_output_safety: MasterOutputSafetyController,
    input_monitor_controller: Option<AudioInputMonitorController>,
    input_monitor_producer: Option<AudioMonitorProducer>,
    input_monitor_consumer: AudioMonitorConsumer,
    input_monitor_gate: AudioInputMonitorGate,
    standby_input: Option<StandbyAudioInput>,
    feeders: Vec<AudioFeedWorker>,
    retired_feeders: Option<Vec<AudioFeedWorker>>,
    retired_instrument_processors: Vec<aaadaw_engine::StoppedTrackInstrument>,
    retired_fx_processors: Vec<StoppedTrackFxProcessor>,
    is_playing: bool,
}

#[cfg(feature = "audio-device")]
impl RunningAudioPlayback {
    /// Updates a track's live playback coefficients without replacing the graph.
    pub fn set_track_mix(&self, track_id: aaadaw_core::TrackId, volume_db: f32, pan: f32) -> bool {
        self.mix_controller.set_track_mix(track_id, volume_db, pan)
    }

    /// Updates a track's live mute and solo state without replacing the graph.
    pub fn set_track_mute_solo(
        &self,
        track_id: aaadaw_core::TrackId,
        muted: bool,
        solo: bool,
    ) -> bool {
        self.mix_controller
            .set_track_mute_solo(track_id, muted, solo)
    }

    pub fn set_track_input_monitor(&self, track_id: aaadaw_core::TrackId, enabled: bool) -> bool {
        self.input_monitor_controller
            .as_ref()
            .is_some_and(|controller| controller.set_track_enabled(track_id, enabled))
    }

    pub fn input_monitor_enabled(&self, track_id: aaadaw_core::TrackId) -> bool {
        self.input_monitor_controller
            .as_ref()
            .is_some_and(|controller| controller.is_track_enabled(track_id))
    }

    pub fn can_monitor_track_input(&self, track_id: aaadaw_core::TrackId) -> bool {
        self.input_monitor_controller
            .as_ref()
            .is_some_and(|controller| controller.has_armed_track(track_id))
    }

    pub fn disable_input_monitoring(&self) {
        if let Some(controller) = &self.input_monitor_controller {
            controller.disable_all();
        }
    }

    /// Transfers the queue producer to the input callback before opening an input backend.
    pub fn take_input_monitor_producer(&mut self) -> Option<AudioMonitorProducer> {
        self.input_monitor_producer.take()
    }

    pub fn input_monitor_producer(&self) -> Option<AudioMonitorProducer> {
        self.input_monitor_producer.clone()
    }

    pub fn has_standby_input(&self) -> bool {
        self.standby_input.is_some()
    }

    pub fn has_enabled_input_monitor(&self) -> bool {
        self.input_monitor_controller
            .as_ref()
            .is_some_and(|controller| !controller.enabled_tracks().is_empty())
    }

    pub fn install_standby_input(&mut self, input: StandbyAudioInput) {
        self.standby_input = Some(input);
    }

    pub fn take_standby_input(&mut self) -> Option<StandbyAudioInput> {
        self.standby_input.take()
    }

    /// Changes the Master sample-peak ceiling in the active callback without rebuilding the graph.
    pub fn set_master_output_ceiling_dbfs(&self, ceiling: MasterOutputCeiling) {
        self.master_output_safety.set_ceiling(ceiling)
    }

    pub fn backend(&self) -> PlaybackBackend {
        match self.output {
            #[cfg(feature = "jack-backend")]
            DeviceAudioOutput::Jack(_) => PlaybackBackend::Jack,
            #[cfg(feature = "pipewire-backend")]
            DeviceAudioOutput::PipeWire(_) => PlaybackBackend::PipeWire,
            #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
            DeviceAudioOutput::Wasapi(_) => PlaybackBackend::Wasapi,
            #[cfg(all(
                feature = "audio-device",
                not(any(
                    feature = "jack-backend",
                    feature = "pipewire-backend",
                    all(feature = "wasapi-backend", target_os = "windows")
                ))
            ))]
            DeviceAudioOutput::Unavailable => PlaybackBackend::Unavailable,
        }
    }

    pub fn play(&mut self) -> Result<(), PlaybackBuildError> {
        match &mut self.output {
            #[cfg(feature = "jack-backend")]
            DeviceAudioOutput::Jack(output) => output.play().map_err(PlaybackBuildError::Jack),
            #[cfg(feature = "pipewire-backend")]
            DeviceAudioOutput::PipeWire(output) => {
                output.play().map_err(PlaybackBuildError::PipeWire)
            }
            #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
            DeviceAudioOutput::Wasapi(output) => output.play().map_err(PlaybackBuildError::Wasapi),
            #[cfg(all(
                feature = "audio-device",
                not(any(
                    feature = "jack-backend",
                    feature = "pipewire-backend",
                    all(feature = "wasapi-backend", target_os = "windows")
                ))
            ))]
            DeviceAudioOutput::Unavailable => Err(PlaybackBuildError::NoAudioOutputBackend),
        }?;
        self.is_playing = true;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), PlaybackBuildError> {
        match &mut self.output {
            #[cfg(feature = "jack-backend")]
            DeviceAudioOutput::Jack(output) => output.stop().map_err(PlaybackBuildError::Jack),
            #[cfg(feature = "pipewire-backend")]
            DeviceAudioOutput::PipeWire(output) => {
                output.stop().map_err(PlaybackBuildError::PipeWire)
            }
            #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
            DeviceAudioOutput::Wasapi(output) => output.stop().map_err(PlaybackBuildError::Wasapi),
            #[cfg(all(
                feature = "audio-device",
                not(any(
                    feature = "jack-backend",
                    feature = "pipewire-backend",
                    all(feature = "wasapi-backend", target_os = "windows")
                ))
            ))]
            DeviceAudioOutput::Unavailable => Err(PlaybackBuildError::NoAudioOutputBackend),
        }?;
        self.is_playing = false;
        Ok(())
    }

    /// Requests an immediate MIDI reset while leaving the transport running.
    pub fn panic_midi(&mut self) -> Result<(), PlaybackBuildError> {
        match &mut self.output {
            #[cfg(feature = "jack-backend")]
            DeviceAudioOutput::Jack(output) => {
                output.panic_midi().map_err(PlaybackBuildError::Jack)
            }
            #[cfg(feature = "pipewire-backend")]
            DeviceAudioOutput::PipeWire(output) => {
                output.panic_midi().map_err(PlaybackBuildError::PipeWire)
            }
            #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
            DeviceAudioOutput::Wasapi(output) => {
                output.panic_midi().map_err(PlaybackBuildError::Wasapi)
            }
            #[cfg(all(
                feature = "audio-device",
                not(any(
                    feature = "jack-backend",
                    feature = "pipewire-backend",
                    all(feature = "wasapi-backend", target_os = "windows")
                ))
            ))]
            DeviceAudioOutput::Unavailable => Err(PlaybackBuildError::NoAudioOutputBackend),
        }
    }

    pub fn replace_graph(
        &mut self,
        mut prepared: PreparedAudioPlayback,
    ) -> Result<(), PlaybackBuildError> {
        self.collect_retired_graphs();
        if self.retired_feeders.is_some() {
            return Err(match self.backend() {
                #[cfg(feature = "jack-backend")]
                PlaybackBackend::Jack => {
                    PlaybackBuildError::Jack(JackOutputError::GraphReplacementInFlight)
                }
                #[cfg(feature = "pipewire-backend")]
                PlaybackBackend::PipeWire => {
                    PlaybackBuildError::PipeWire(PipeWireOutputError::GraphReplacementInFlight)
                }
                #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
                PlaybackBackend::Wasapi => {
                    PlaybackBuildError::Wasapi(WasapiOutputError::GraphReplacementInFlight)
                }
                #[cfg(all(
                    feature = "audio-device",
                    not(any(
                        feature = "jack-backend",
                        feature = "pipewire-backend",
                        all(feature = "wasapi-backend", target_os = "windows")
                    ))
                ))]
                PlaybackBackend::Unavailable => PlaybackBuildError::NoAudioOutputBackend,
            });
        }
        let enabled_monitor_tracks = self
            .input_monitor_controller
            .as_ref()
            .map(AudioInputMonitorController::enabled_tracks)
            .unwrap_or_default();
        if !enabled_monitor_tracks.is_empty() {
            // Advance the queue generation without interrupting the input stream or monitor gate.
            self.input_monitor_gate.set_enabled(true);
        }
        let input_monitor_controller = prepared.graph.install_input_monitor(
            self.input_monitor_consumer.clone(),
            self.input_monitor_gate.clone(),
        );
        for track_id in enabled_monitor_tracks {
            input_monitor_controller.set_track_enabled(track_id, true);
        }
        let mix_controller = prepared.graph.track_mix_controller();
        let master_output_safety = prepared.graph.master_output_safety_controller();
        master_output_safety.set_ceiling(self.master_output_safety.ceiling());
        let (graph, feeders) = prepared.into_parts();
        #[cfg(all(
            feature = "audio-device",
            not(any(
                feature = "jack-backend",
                feature = "pipewire-backend",
                all(feature = "wasapi-backend", target_os = "windows")
            ))
        ))]
        let _ = graph;
        match &mut self.output {
            #[cfg(feature = "jack-backend")]
            DeviceAudioOutput::Jack(output) => output
                .replace_graph(graph, self.is_playing)
                .map_err(PlaybackBuildError::Jack)?,
            #[cfg(feature = "pipewire-backend")]
            DeviceAudioOutput::PipeWire(output) => output
                .replace_graph(graph, self.is_playing)
                .map_err(PlaybackBuildError::PipeWire)?,
            #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
            DeviceAudioOutput::Wasapi(output) => output
                .replace_graph(graph, self.is_playing)
                .map_err(PlaybackBuildError::Wasapi)?,
            #[cfg(all(
                feature = "audio-device",
                not(any(
                    feature = "jack-backend",
                    feature = "pipewire-backend",
                    all(feature = "wasapi-backend", target_os = "windows")
                ))
            ))]
            DeviceAudioOutput::Unavailable => {}
        }
        self.mix_controller = mix_controller;
        self.master_output_safety = master_output_safety;
        self.input_monitor_controller = Some(input_monitor_controller);
        self.retired_feeders = Some(std::mem::replace(&mut self.feeders, feeders));
        Ok(())
    }

    pub fn seek_to_sample(
        &mut self,
        project: &Project,
        store: &ProjectStore,
        timeline_sample: u64,
        queue_capacity_samples: usize,
        max_block_frames: usize,
    ) -> Result<(), PlaybackBuildError> {
        let prepared = prepare_audio_playback_at(
            project,
            store,
            timeline_sample,
            queue_capacity_samples,
            max_block_frames,
        )?;
        self.replace_graph(prepared)
    }

    pub fn collect_retired_graphs(&mut self) -> bool {
        let mut retired: Vec<AudioRenderGraph> = match &mut self.output {
            #[cfg(feature = "jack-backend")]
            DeviceAudioOutput::Jack(output) => output.take_retired_graphs(),
            #[cfg(feature = "pipewire-backend")]
            DeviceAudioOutput::PipeWire(output) => output.take_retired_graphs(),
            #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
            DeviceAudioOutput::Wasapi(output) => output.take_retired_graphs(),
            #[cfg(all(
                feature = "audio-device",
                not(any(
                    feature = "jack-backend",
                    feature = "pipewire-backend",
                    all(feature = "wasapi-backend", target_os = "windows")
                ))
            ))]
            DeviceAudioOutput::Unavailable => Vec::new(),
        };
        let collected = !retired.is_empty();
        for graph in &mut retired {
            self.retired_instrument_processors
                .append(&mut graph.take_stopped_instruments());
            self.retired_fx_processors
                .append(&mut graph.take_stopped_fx_processors());
        }
        drop(retired);
        if collected {
            drop(self.retired_feeders.take());
            true
        } else {
            false
        }
    }

    /// Returns stopped effect processors collected from retired graphs.
    pub fn take_retired_fx_processors(&mut self) -> Vec<StoppedTrackFxProcessor> {
        std::mem::take(&mut self.retired_fx_processors)
    }

    /// Returns stopped instrument processors collected from retired graphs.
    pub fn take_retired_instrument_processors(
        &mut self,
    ) -> Vec<aaadaw_engine::StoppedTrackInstrument> {
        std::mem::take(&mut self.retired_instrument_processors)
    }

    /// Stops playback and returns all stopped plugin processors for control-thread teardown.
    pub fn shutdown(
        mut self,
    ) -> Result<
        (
            Vec<aaadaw_engine::StoppedTrackInstrument>,
            Vec<StoppedTrackFxProcessor>,
        ),
        PlaybackBuildError,
    > {
        if let Some(standby_input) = self.standby_input.take() {
            standby_input.shutdown();
        }
        self.collect_retired_graphs();
        let mut instruments = std::mem::take(&mut self.retired_instrument_processors);
        let mut stopped = std::mem::take(&mut self.retired_fx_processors);
        let mut graphs: Vec<AudioRenderGraph> = match self.output {
            #[cfg(feature = "jack-backend")]
            DeviceAudioOutput::Jack(mut output) => {
                output.shutdown().map_err(PlaybackBuildError::Jack)?
            }
            #[cfg(feature = "pipewire-backend")]
            DeviceAudioOutput::PipeWire(mut output) => {
                output.shutdown().map_err(PlaybackBuildError::PipeWire)?
            }
            #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
            DeviceAudioOutput::Wasapi(mut output) => {
                output.shutdown().map_err(PlaybackBuildError::Wasapi)?
            }
            #[cfg(all(
                feature = "audio-device",
                not(any(
                    feature = "jack-backend",
                    feature = "pipewire-backend",
                    all(feature = "wasapi-backend", target_os = "windows")
                ))
            ))]
            DeviceAudioOutput::Unavailable => Vec::new(),
        };
        for graph in &mut graphs {
            instruments.append(&mut graph.take_stopped_instruments());
            stopped.append(&mut graph.take_stopped_fx_processors());
        }
        drop(graphs);
        drop(self.feeders);
        drop(self.retired_feeders.take());
        Ok((instruments, stopped))
    }

    pub fn stats(&self) -> PlaybackStats {
        match &self.output {
            #[cfg(feature = "jack-backend")]
            DeviceAudioOutput::Jack(output) => output.stats().into(),
            #[cfg(feature = "pipewire-backend")]
            DeviceAudioOutput::PipeWire(output) => output.stats().into(),
            #[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
            DeviceAudioOutput::Wasapi(output) => output.stats().into(),
            #[cfg(all(
                feature = "audio-device",
                not(any(
                    feature = "jack-backend",
                    feature = "pipewire-backend",
                    all(feature = "wasapi-backend", target_os = "windows")
                ))
            ))]
            DeviceAudioOutput::Unavailable => PlaybackStats::default(),
        }
    }
}

#[cfg(feature = "audio-device")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlaybackStats {
    pub rendered_blocks: u64,
    pub underrun_samples: u64,
    pub master_guarded_samples: u64,
    pub master_non_finite_samples: u64,
    /// JACK-reported device xruns. `None` means the active backend has no verified signal.
    pub jack_xruns: Option<u64>,
    pub callback_errors: u64,
    pub playhead_sample: u64,
    /// JACK server frame paired with the project sample at that callback's start.
    pub transport_clock_anchor: Option<TransportClockAnchor>,
    /// True after Windows invalidates the active stream because its device changed or disappeared.
    pub output_device_lost: bool,
}

#[cfg(feature = "jack-backend")]
impl From<JackOutputStats> for PlaybackStats {
    fn from(stats: JackOutputStats) -> Self {
        Self {
            rendered_blocks: stats.rendered_blocks,
            underrun_samples: stats.underrun_samples,
            master_guarded_samples: stats.master_guarded_samples,
            master_non_finite_samples: stats.master_non_finite_samples,
            jack_xruns: Some(stats.device_xruns),
            callback_errors: stats.callback_errors,
            playhead_sample: stats.playhead_sample,
            transport_clock_anchor: stats.transport_clock_anchor,
            output_device_lost: false,
        }
    }
}

#[cfg(feature = "pipewire-backend")]
impl From<PipeWireOutputStats> for PlaybackStats {
    fn from(stats: PipeWireOutputStats) -> Self {
        Self {
            rendered_blocks: stats.rendered_blocks,
            underrun_samples: stats.underrun_samples,
            master_guarded_samples: stats.master_guarded_samples,
            master_non_finite_samples: stats.master_non_finite_samples,
            jack_xruns: None,
            callback_errors: stats.callback_errors,
            playhead_sample: stats.playhead_sample,
            transport_clock_anchor: None,
            output_device_lost: false,
        }
    }
}

#[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
impl From<WasapiOutputStats> for PlaybackStats {
    fn from(stats: WasapiOutputStats) -> Self {
        Self {
            rendered_blocks: stats.rendered_blocks,
            underrun_samples: stats.underrun_samples,
            master_guarded_samples: stats.master_guarded_samples,
            master_non_finite_samples: stats.master_non_finite_samples,
            jack_xruns: None,
            callback_errors: stats.callback_errors,
            playhead_sample: stats.playhead_sample,
            transport_clock_anchor: None,
            output_device_lost: stats.device_lost,
        }
    }
}

/// A JACK client and the background feeders that supply its render graph.
#[cfg(feature = "jack-backend")]
pub struct RunningJackPlayback {
    output: JackAudioOutput,
    mix_controller: TrackMixController,
    master_output_safety: MasterOutputSafetyController,
    input_monitor_controller: Option<AudioInputMonitorController>,
    input_monitor_producer: Option<AudioMonitorProducer>,
    input_monitor_consumer: AudioMonitorConsumer,
    input_monitor_gate: AudioInputMonitorGate,
    feeders: Vec<AudioFeedWorker>,
    retired_feeders: Option<Vec<AudioFeedWorker>>,
    retired_instrument_processors: Vec<aaadaw_engine::StoppedTrackInstrument>,
    retired_fx_processors: Vec<StoppedTrackFxProcessor>,
    is_playing: bool,
}

#[cfg(feature = "jack-backend")]
impl RunningJackPlayback {
    /// Updates a track's live playback coefficients without replacing the graph.
    pub fn set_track_mix(&self, track_id: aaadaw_core::TrackId, volume_db: f32, pan: f32) -> bool {
        self.mix_controller.set_track_mix(track_id, volume_db, pan)
    }

    /// Updates a track's live mute and solo state without replacing the graph.
    pub fn set_track_mute_solo(
        &self,
        track_id: aaadaw_core::TrackId,
        muted: bool,
        solo: bool,
    ) -> bool {
        self.mix_controller
            .set_track_mute_solo(track_id, muted, solo)
    }

    pub fn set_track_input_monitor(&self, track_id: aaadaw_core::TrackId, enabled: bool) -> bool {
        self.input_monitor_controller
            .as_ref()
            .is_some_and(|controller| controller.set_track_enabled(track_id, enabled))
    }

    pub fn input_monitor_enabled(&self, track_id: aaadaw_core::TrackId) -> bool {
        self.input_monitor_controller
            .as_ref()
            .is_some_and(|controller| controller.is_track_enabled(track_id))
    }

    pub fn disable_input_monitoring(&self) {
        if let Some(controller) = &self.input_monitor_controller {
            controller.disable_all();
        }
    }

    pub fn take_input_monitor_producer(&mut self) -> Option<AudioMonitorProducer> {
        self.input_monitor_producer.take()
    }

    pub fn set_master_output_ceiling_dbfs(&self, ceiling: MasterOutputCeiling) {
        self.master_output_safety.set_ceiling(ceiling)
    }

    /// Queues playback for the next JACK callback.
    pub fn play(&mut self) -> Result<(), JackOutputError> {
        self.output.play()?;
        self.is_playing = true;
        Ok(())
    }

    /// Queues a stop for the next JACK callback.
    pub fn stop(&mut self) -> Result<(), JackOutputError> {
        self.output.stop()?;
        self.is_playing = false;
        Ok(())
    }

    /// Prepares and queues a seek without interrupting the active graph during media refill.
    ///
    /// Call from a control worker: asset resolution and decoder startup can block. If preparation
    /// fails, current graph and playback position remain unchanged.
    pub fn seek_to_sample(
        &mut self,
        project: &Project,
        store: &ProjectStore,
        timeline_sample: u64,
        queue_capacity_samples: usize,
        max_block_frames: usize,
    ) -> Result<(), PlaybackBuildError> {
        self.collect_retired_graphs();
        if self.retired_feeders.is_some() {
            return Err(PlaybackBuildError::Jack(
                JackOutputError::GraphReplacementInFlight,
            ));
        }
        let prepared = prepare_audio_playback_at(
            project,
            store,
            timeline_sample,
            queue_capacity_samples,
            max_block_frames,
        )?;
        self.replace_graph(prepared)
    }

    /// Replaces active graph and retains old feeders until callback retires old graph.
    pub fn replace_graph(
        &mut self,
        mut prepared: PreparedAudioPlayback,
    ) -> Result<(), PlaybackBuildError> {
        self.collect_retired_graphs();
        if self.retired_feeders.is_some() {
            return Err(PlaybackBuildError::Jack(
                JackOutputError::GraphReplacementInFlight,
            ));
        }
        let enabled_monitor_tracks = self
            .input_monitor_controller
            .as_ref()
            .map(AudioInputMonitorController::enabled_tracks)
            .unwrap_or_default();
        if !enabled_monitor_tracks.is_empty() {
            // Advance the queue generation without interrupting the input stream or monitor gate.
            self.input_monitor_gate.set_enabled(true);
        }
        let input_monitor_controller = prepared.graph.install_input_monitor(
            self.input_monitor_consumer.clone(),
            self.input_monitor_gate.clone(),
        );
        for track_id in enabled_monitor_tracks {
            input_monitor_controller.set_track_enabled(track_id, true);
        }
        let mix_controller = prepared.graph.track_mix_controller();
        let master_output_safety = prepared.graph.master_output_safety_controller();
        master_output_safety.set_ceiling(self.master_output_safety.ceiling());
        let (graph, feeders) = prepared.into_parts();
        self.output
            .replace_graph(graph, self.is_playing)
            .map_err(PlaybackBuildError::Jack)?;
        self.mix_controller = mix_controller;
        self.master_output_safety = master_output_safety;
        self.input_monitor_controller = Some(input_monitor_controller);
        self.retired_feeders = Some(std::mem::replace(&mut self.feeders, feeders));
        Ok(())
    }

    /// Reclaims replaced graphs and stops their feeder workers after the callback swap.
    pub fn collect_retired_graphs(&mut self) -> bool {
        let mut retired = self.output.take_retired_graphs();
        let collected = !retired.is_empty();
        for graph in &mut retired {
            self.retired_instrument_processors
                .append(&mut graph.take_stopped_instruments());
            self.retired_fx_processors
                .append(&mut graph.take_stopped_fx_processors());
        }
        drop(retired);
        if collected {
            drop(self.retired_feeders.take());
            true
        } else {
            false
        }
    }

    /// Returns stopped effect processors collected from retired graphs.
    pub fn take_retired_fx_processors(&mut self) -> Vec<StoppedTrackFxProcessor> {
        std::mem::take(&mut self.retired_fx_processors)
    }

    /// Returns stopped instrument processors collected from retired graphs.
    pub fn take_retired_instrument_processors(
        &mut self,
    ) -> Vec<aaadaw_engine::StoppedTrackInstrument> {
        std::mem::take(&mut self.retired_instrument_processors)
    }

    /// Stops JACK and returns all stopped plugin processors for control-thread teardown.
    pub fn shutdown(
        mut self,
    ) -> Result<
        (
            Vec<aaadaw_engine::StoppedTrackInstrument>,
            Vec<StoppedTrackFxProcessor>,
        ),
        PlaybackBuildError,
    > {
        self.collect_retired_graphs();
        let mut instruments = std::mem::take(&mut self.retired_instrument_processors);
        let mut stopped = std::mem::take(&mut self.retired_fx_processors);
        let mut graphs = self.output.shutdown().map_err(PlaybackBuildError::Jack)?;
        for graph in &mut graphs {
            instruments.append(&mut graph.take_stopped_instruments());
            stopped.append(&mut graph.take_stopped_fx_processors());
        }
        drop(graphs);
        drop(self.feeders);
        drop(self.retired_feeders.take());
        Ok((instruments, stopped))
    }

    /// Returns lock-free callback counters.
    pub fn stats(&self) -> JackOutputStats {
        self.output.stats()
    }
}

/// Resolves every project AudioItem and prepares a fixed-topology streaming graph.
///
/// Queue capacity and callback block size are specified in mono samples/frames. Embedded assets
/// stream directly from independent SQLite readers; linked files are opened and probed on background
/// workers. This call waits for each worker's source-open result, so invoke it on a background
/// control thread when the UI must remain responsive. Partial setup is cancelled if a source fails.
pub fn prepare_audio_playback(
    project: &Project,
    store: &ProjectStore,
    queue_capacity_samples: usize,
    max_block_frames: usize,
) -> Result<PreparedAudioPlayback, PlaybackBuildError> {
    prepare_audio_playback_at(project, store, 0, queue_capacity_samples, max_block_frames)
}

/// Prepares playback with the graph transport positioned at an arbitrary project sample.
///
/// Items containing the seek sample are re-decoded off-thread and their earlier output is discarded;
/// items already behind the playhead receive empty queues until a later graph replacement.
pub fn prepare_audio_playback_at(
    project: &Project,
    store: &ProjectStore,
    timeline_sample: u64,
    queue_capacity_samples: usize,
    max_block_frames: usize,
) -> Result<PreparedAudioPlayback, PlaybackBuildError> {
    let monitor_capacity = max_block_frames.saturating_mul(2).clamp(256, 4_096);
    let (monitor_producer, monitor_consumer, monitor_gate) = audio_monitor_stream(monitor_capacity);
    let output_sample_rate = project.settings().sample_rate();
    let mut feeders = Vec::with_capacity(project.audio_items().len());
    let mut item_streams = Vec::with_capacity(project.audio_items().len());

    for item in project.audio_items() {
        let (producer, consumer) =
            pcm_stream(queue_capacity_samples).map_err(PlaybackBuildError::PcmStream)?;
        if timeline_sample >= item.end_sample() {
            drop(producer);
            item_streams.push(AudioItemStream::new_at_sample(
                item.id(),
                item.end_sample(),
                consumer,
            ));
            continue;
        }
        let needs_refill = timeline_sample > item.start_sample();
        let resolved = store
            .resolve_audio_asset(item.media_ref())
            .map_err(PlaybackBuildError::Storage)?;
        let is_linked = matches!(&resolved, ResolvedAudioAsset::LinkedFile { .. });
        let mut feeder = match resolved {
            ResolvedAudioAsset::Embedded(reader) => {
                let byte_len = reader.byte_len();
                let extension = Path::new(reader.original_name())
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(str::to_owned);
                if needs_refill {
                    spawn_audio_item_stream_from_reader_at(
                        item,
                        timeline_sample,
                        reader,
                        Some(byte_len),
                        extension.as_deref(),
                        output_sample_rate,
                        producer,
                    )
                } else {
                    spawn_audio_item_stream_from_reader(
                        item,
                        reader,
                        Some(byte_len),
                        extension.as_deref(),
                        output_sample_rate,
                        producer,
                    )
                }
                .map_err(PlaybackBuildError::Media)?
            }
            ResolvedAudioAsset::LinkedFile { path, .. } => if needs_refill {
                spawn_audio_item_stream_at(
                    item,
                    timeline_sample,
                    path,
                    output_sample_rate,
                    producer,
                )
            } else {
                spawn_audio_item_stream(item, path, output_sample_rate, producer)
            }
            .map_err(PlaybackBuildError::Media)?,
        };
        if let Err(error) = feeder.wait_ready() {
            if is_linked
                && matches!(
                    &error,
                    MediaError::WorkerStartupFailed {
                        io_kind: Some(ErrorKind::NotFound),
                        ..
                    }
                )
            {
                return Err(PlaybackBuildError::ExternalSourceUnavailable {
                    media_ref: item.media_ref().to_owned(),
                });
            }
            return Err(PlaybackBuildError::Media(error));
        }
        feeders.push(feeder);
        if needs_refill {
            item_streams.push(AudioItemStream::new_at_sample(
                item.id(),
                timeline_sample,
                consumer,
            ));
        } else {
            item_streams.push(AudioItemStream::new(item.id(), consumer));
        }
    }

    let mut graph = AudioRenderGraph::new_for_audio_items(project, item_streams, max_block_frames)
        .map_err(PlaybackBuildError::AudioGraph)?;
    graph.install_input_monitor(monitor_consumer.clone(), monitor_gate.clone());
    graph.transport_mut().seek_sample(timeline_sample);
    Ok(PreparedAudioPlayback {
        graph,
        feeders,
        input_monitor_producer: Some(monitor_producer),
        input_monitor_consumer: monitor_consumer,
        input_monitor_gate: monitor_gate,
    })
}
