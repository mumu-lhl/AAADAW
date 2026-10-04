//! Realtime-oriented audio processing primitives.
//!
//! This crate provides a fixed-topology, allocation-free streaming mixer,
//! transport, MIDI scheduling, CLAP instrument processing, and optional Linux audio backends.
//! Project-to-plugin assignment remains an application-layer responsibility.

mod capture;
mod clap_gui;
mod clap_instrument;
#[cfg(feature = "jack-backend")]
mod jack_input;
#[cfg(feature = "jack-backend")]
mod jack_output;
mod master_output;
mod midi;
mod pcm;
#[cfg(feature = "pipewire-backend")]
mod pipewire_input;
#[cfg(feature = "pipewire-backend")]
mod pipewire_output;
mod stream;
mod transport;
#[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
mod wasapi_common;
#[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
mod wasapi_input;
#[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
mod wasapi_output;

pub use capture::{
    AudioCaptureConsumer, AudioCaptureControl, AudioCaptureProducer, CapturedFrames,
    audio_capture_stream,
};
pub use clap_gui::ClapPluginGuiOwner;
pub use clap_instrument::{
    ClapEffectOwner, ClapEffectProcessor, ClapInstrumentDescriptor, ClapInstrumentError,
    ClapInstrumentOwner, ClapInstrumentProcessor, ClapParameterCommand, ClapParameterInfo,
    ClapParameterSender, ClapPluginDescriptor, StoppedClapEffectProcessor,
    StoppedClapInstrumentProcessor, inspect_clap_instrument_entry, inspect_clap_plugin_entry,
};
#[cfg(feature = "jack-backend")]
pub use jack_input::{JackAudioInput, JackInputError};
#[cfg(feature = "jack-backend")]
pub use jack_output::{JackAudioOutput, JackOutputError, JackOutputStats};
pub use master_output::{
    MASTER_OUTPUT_DEFAULT_CEILING_DBFS, MasterOutputCeiling, MasterOutputSafetyController,
    MasterOutputSafetyError,
};
pub use midi::{MidiEventKind, MidiEventPlan, MidiScheduleError, ScheduledMidiEvent};
pub use pcm::{MonoPcmClip, MonoPcmPlayer, PcmError};
#[cfg(feature = "pipewire-backend")]
pub use pipewire_input::{PipeWireAudioInput, PipeWireInputError};
#[cfg(feature = "pipewire-backend")]
pub use pipewire_output::{PipeWireAudioOutput, PipeWireOutputError, PipeWireOutputStats};
pub use stream::{PcmStreamConsumer, PcmStreamError, PcmStreamProducer, pcm_stream};
pub use transport::{AudioBlock, Transport, TransportPositionOverflow};
#[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
pub use wasapi_input::{WasapiAudioInput, WasapiInputError};
#[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
pub use wasapi_output::{WasapiAudioOutput, WasapiOutputError, WasapiOutputStats};

use aaadaw_core::{ItemId, Track, TrackId};
use std::f64::consts::FRAC_PI_4;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// Precomputed per-track mix coefficients, built outside the audio callback.
#[derive(Clone, Debug)]
pub struct MixerPlan {
    max_block_frames: usize,
    tracks: Vec<TrackGains>,
    has_solo: bool,
}

#[derive(Clone, Debug)]
struct TrackGains {
    track_id: TrackId,
    left: f32,
    right: f32,
    stereo_left: f32,
    stereo_right: f32,
    live: Arc<LiveTrackGains>,
    muted: bool,
    solo: bool,
}

#[derive(Debug)]
struct LiveTrackGains {
    version: AtomicU32,
    left: AtomicU32,
    right: AtomicU32,
    stereo_left: AtomicU32,
    stereo_right: AtomicU32,
}

#[derive(Clone, Copy, Debug)]
struct GainCoefficients {
    left: f32,
    right: f32,
    stereo_left: f32,
    stereo_right: f32,
}

/// Control-thread handle for updating the active graph's per-track volume and pan.
///
/// Values are fully converted to gain coefficients on the caller thread. The audio callback
/// performs only bounded atomic loads and falls back to the graph's compiled coefficients if it
/// observes an update in progress.
#[derive(Clone, Debug)]
pub struct TrackMixController {
    tracks: Vec<(TrackId, Arc<LiveTrackGains>)>,
}

impl TrackMixController {
    /// Updates one track's volume and pan without rebuilding the render graph.
    pub fn set_track_mix(&self, track_id: TrackId, volume_db: f32, pan: f32) -> bool {
        let Some((_, live)) = self.tracks.iter().find(|(id, _)| *id == track_id) else {
            return false;
        };
        let Some(gains) = gain_coefficients(volume_db, pan) else {
            return false;
        };

        live.version.fetch_add(1, Ordering::SeqCst);
        live.left.store(gains.left.to_bits(), Ordering::SeqCst);
        live.right.store(gains.right.to_bits(), Ordering::SeqCst);
        live.stereo_left
            .store(gains.stereo_left.to_bits(), Ordering::SeqCst);
        live.stereo_right
            .store(gains.stereo_right.to_bits(), Ordering::SeqCst);
        live.version.fetch_add(1, Ordering::SeqCst);
        true
    }
}

impl LiveTrackGains {
    fn new(gains: GainCoefficients) -> Self {
        Self {
            version: AtomicU32::new(0),
            left: AtomicU32::new(gains.left.to_bits()),
            right: AtomicU32::new(gains.right.to_bits()),
            stereo_left: AtomicU32::new(gains.stereo_left.to_bits()),
            stereo_right: AtomicU32::new(gains.stereo_right.to_bits()),
        }
    }

    fn snapshot(&self, fallback: GainCoefficients) -> GainCoefficients {
        let before = self.version.load(Ordering::SeqCst);
        if before % 2 != 0 {
            return fallback;
        }
        let gains = GainCoefficients {
            left: f32::from_bits(self.left.load(Ordering::SeqCst)),
            right: f32::from_bits(self.right.load(Ordering::SeqCst)),
            stereo_left: f32::from_bits(self.stereo_left.load(Ordering::SeqCst)),
            stereo_right: f32::from_bits(self.stereo_right.load(Ordering::SeqCst)),
        };
        if self.version.load(Ordering::SeqCst) == before {
            gains
        } else {
            fallback
        }
    }
}

fn gain_coefficients(volume_db: f32, pan: f32) -> Option<GainCoefficients> {
    if !volume_db.is_finite() || !(-1.0..=1.0).contains(&pan) {
        return None;
    }
    let linear_gain = 10.0_f64.powf(f64::from(volume_db) / 20.0);
    if !linear_gain.is_finite() || linear_gain > f64::from(f32::MAX) {
        return None;
    }
    let gain = linear_gain as f32;
    let (left, right) = match pan {
        -1.0 => (gain, 0.0),
        1.0 => (0.0, gain),
        pan => {
            let angle = (f64::from(pan) + 1.0) * FRAC_PI_4;
            (angle.cos() as f32 * gain, angle.sin() as f32 * gain)
        }
    };
    let (stereo_left, stereo_right) = if pan < 0.0 {
        (gain, (1.0 + pan) * gain)
    } else {
        ((1.0 - pan) * gain, gain)
    };
    Some(GainCoefficients {
        left,
        right,
        stereo_left,
        stereo_right,
    })
}

/// The mix plan could not be compiled from project track controls.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MixerPlanError {
    /// The maximum callback block size must be positive.
    ZeroBlockCapacity,
    /// The track's volume cannot be represented as an `f32` gain.
    InvalidTrackGain { track_id: u64 },
}

impl fmt::Display for MixerPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroBlockCapacity => formatter.write_str("maximum block size must be positive"),
            Self::InvalidTrackGain { track_id } => {
                write!(formatter, "track {track_id} gain is not representable")
            }
        }
    }
}

impl std::error::Error for MixerPlanError {}

/// Input buffers did not match the compiled mix plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MixError {
    /// The callback requested more frames than the plan was compiled for.
    BlockTooLarge { requested: usize, maximum: usize },
    /// The number of input buffers must equal the number of compiled tracks.
    TrackCountMismatch { expected: usize, found: usize },
    /// An input buffer has a different frame count than the output buffer.
    InputLengthMismatch {
        track_index: usize,
        expected: usize,
        found: usize,
    },
}

impl fmt::Display for MixError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BlockTooLarge { requested, maximum } => {
                write!(
                    formatter,
                    "block has {requested} frames; maximum is {maximum}"
                )
            }
            Self::TrackCountMismatch { expected, found } => {
                write!(
                    formatter,
                    "mix plan has {expected} tracks; received {found} inputs"
                )
            }
            Self::InputLengthMismatch {
                track_index,
                expected,
                found,
            } => write!(
                formatter,
                "track {track_index} has {found} frames; expected {expected}"
            ),
        }
    }
}

impl std::error::Error for MixError {}

impl MixerPlan {
    /// Compiles the project's current track controls for a maximum callback
    /// block size. Call this on a control thread, not in the audio callback.
    pub fn compile(tracks: &[Track], max_block_frames: usize) -> Result<Self, MixerPlanError> {
        if max_block_frames == 0 {
            return Err(MixerPlanError::ZeroBlockCapacity);
        }

        let has_solo = tracks.iter().any(Track::is_solo);
        let mut compiled = Vec::with_capacity(tracks.len());
        for track in tracks {
            let Some(gains) = gain_coefficients(track.volume_db(), track.pan()) else {
                return Err(MixerPlanError::InvalidTrackGain {
                    track_id: track.id().value(),
                });
            };
            compiled.push(TrackGains {
                track_id: track.id(),
                left: gains.left,
                right: gains.right,
                stereo_left: gains.stereo_left,
                stereo_right: gains.stereo_right,
                live: Arc::new(LiveTrackGains::new(gains)),
                muted: track.is_muted(),
                solo: track.is_solo(),
            });
        }

        Ok(Self {
            max_block_frames,
            tracks: compiled,
            has_solo,
        })
    }

    /// Creates a controller for this graph's live volume and pan coefficients.
    pub fn track_mix_controller(&self) -> TrackMixController {
        TrackMixController {
            tracks: self
                .tracks
                .iter()
                .map(|gains| (gains.track_id, Arc::clone(&gains.live)))
                .collect(),
        }
    }

    /// Mixes one mono input buffer per project track, in project order, into
    /// interleaved stereo frames. Validation happens before output is modified. This method does
    /// not allocate, lock, or perform I/O.
    pub fn mix_mono_into(
        &self,
        inputs: &[&[f32]],
        output: &mut [[f32; 2]],
    ) -> Result<(), MixError> {
        if output.len() > self.max_block_frames {
            return Err(MixError::BlockTooLarge {
                requested: output.len(),
                maximum: self.max_block_frames,
            });
        }
        if inputs.len() != self.tracks.len() {
            return Err(MixError::TrackCountMismatch {
                expected: self.tracks.len(),
                found: inputs.len(),
            });
        }
        for (track_index, input) in inputs.iter().enumerate() {
            if input.len() != output.len() {
                return Err(MixError::InputLengthMismatch {
                    track_index,
                    expected: output.len(),
                    found: input.len(),
                });
            }
        }

        output.fill([0.0, 0.0]);
        for (track_index, input) in inputs.iter().enumerate() {
            self.mix_track_unchecked(track_index, input, output);
        }
        Ok(())
    }

    fn mix_track_unchecked(&self, track_index: usize, input: &[f32], output: &mut [[f32; 2]]) {
        let track = &self.tracks[track_index];
        if track.muted || (self.has_solo && !track.solo) {
            return;
        }
        let fallback = GainCoefficients {
            left: track.left,
            right: track.right,
            stereo_left: track.stereo_left,
            stereo_right: track.stereo_right,
        };
        let gains = track.live.snapshot(fallback);
        for (frame, sample) in output.iter_mut().zip(input.iter().copied()) {
            frame[0] += sample * gains.left;
            frame[1] += sample * gains.right;
        }
    }

    fn mix_stereo_track_unchecked(
        &self,
        track_index: usize,
        input: &[[f32; 2]],
        output: &mut [[f32; 2]],
    ) {
        let track = &self.tracks[track_index];
        if track.muted || (self.has_solo && !track.solo) {
            return;
        }
        let fallback = GainCoefficients {
            left: track.left,
            right: track.right,
            stereo_left: track.stereo_left,
            stereo_right: track.stereo_right,
        };
        let gains = track.live.snapshot(fallback);
        for (frame, sample) in output.iter_mut().zip(input.iter()) {
            frame[0] += sample[0] * gains.stereo_left;
            frame[1] += sample[1] * gains.stereo_right;
        }
    }
}

/// Construction failure for a streaming graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AudioGraphBuildError {
    MixerPlan(MixerPlanError),
    MidiSchedule(MidiScheduleError),
    TrackStreamCountMismatch {
        tracks: usize,
        streams: usize,
    },
    AudioItemStreamCountMismatch {
        items: usize,
        streams: usize,
    },
    AudioItemStreamOrderMismatch {
        expected: ItemId,
        found: ItemId,
    },
    AudioItemStreamStartOutOfRange {
        item_id: ItemId,
        start_sample: u64,
    },
    MissingFxTrack {
        track_id: u64,
    },
    FxChainSlotMismatch {
        track_id: u64,
        chain_index: usize,
    },
    DuplicateTrackFxProcessor {
        track_id: u64,
        chain_index: usize,
    },
    FxProcessorBlockCapacity {
        track_id: u64,
        chain_index: usize,
        required: usize,
        available: usize,
    },
    MissingInstrumentTrack {
        track_id: u64,
    },
    DuplicateTrackInstrument {
        track_id: u64,
    },
    InstrumentBlockCapacity {
        track_id: u64,
        required: usize,
        available: usize,
    },
    InstrumentEventCapacity {
        track_id: u64,
        required: usize,
        available: usize,
    },
    InstrumentControllerDialect {
        track_id: u64,
    },
}

impl fmt::Display for AudioGraphBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MixerPlan(error) => write!(formatter, "invalid mixer plan: {error}"),
            Self::MidiSchedule(error) => write!(formatter, "invalid MIDI event plan: {error}"),
            Self::TrackStreamCountMismatch { tracks, streams } => write!(
                formatter,
                "audio graph has {tracks} tracks but {streams} PCM streams"
            ),
            Self::AudioItemStreamCountMismatch { items, streams } => write!(
                formatter,
                "audio graph has {items} audio items but {streams} PCM streams"
            ),
            Self::AudioItemStreamOrderMismatch { expected, found } => write!(
                formatter,
                "audio stream for item {} was expected, but item {} was supplied",
                expected.value(),
                found.value()
            ),
            Self::AudioItemStreamStartOutOfRange {
                item_id,
                start_sample,
            } => write!(
                formatter,
                "audio stream for item {} starts at sample {start_sample}, outside its timeline range",
                item_id.value()
            ),
            Self::MissingFxTrack { track_id } => {
                write!(formatter, "CLAP effect targets missing track {track_id}")
            }
            Self::FxChainSlotMismatch {
                track_id,
                chain_index,
            } => write!(
                formatter,
                "CLAP effect does not match enabled FX slot {chain_index} on track {track_id}"
            ),
            Self::DuplicateTrackFxProcessor {
                track_id,
                chain_index,
            } => write!(
                formatter,
                "track {track_id} FX slot {chain_index} has multiple prepared processors"
            ),
            Self::FxProcessorBlockCapacity {
                track_id,
                chain_index,
                required,
                available,
            } => write!(
                formatter,
                "CLAP effect in slot {chain_index} on track {track_id} supports {available} frames; playback needs {required}"
            ),
            Self::MissingInstrumentTrack { track_id } => {
                write!(
                    formatter,
                    "CLAP instrument targets missing track {track_id}"
                )
            }
            Self::DuplicateTrackInstrument { track_id } => {
                write!(
                    formatter,
                    "track {track_id} has more than one CLAP instrument"
                )
            }
            Self::InstrumentBlockCapacity {
                track_id,
                required,
                available,
            } => write!(
                formatter,
                "CLAP instrument on track {track_id} supports {available} frames; playback needs {required}"
            ),
            Self::InstrumentEventCapacity {
                track_id,
                required,
                available,
            } => write!(
                formatter,
                "CLAP instrument on track {track_id} supports {available} MIDI events per block; playback may need {required}"
            ),
            Self::InstrumentControllerDialect { track_id } => write!(
                formatter,
                "CLAP instrument on track {track_id} does not support MIDI 1.0 controller events required by the project"
            ),
        }
    }
}

impl std::error::Error for AudioGraphBuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MixerPlan(error) => Some(error),
            Self::MidiSchedule(error) => Some(error),
            Self::TrackStreamCountMismatch { .. }
            | Self::AudioItemStreamCountMismatch { .. }
            | Self::AudioItemStreamOrderMismatch { .. }
            | Self::AudioItemStreamStartOutOfRange { .. }
            | Self::MissingFxTrack { .. }
            | Self::FxChainSlotMismatch { .. }
            | Self::DuplicateTrackFxProcessor { .. }
            | Self::FxProcessorBlockCapacity { .. }
            | Self::MissingInstrumentTrack { .. }
            | Self::DuplicateTrackInstrument { .. }
            | Self::InstrumentBlockCapacity { .. }
            | Self::InstrumentEventCapacity { .. } => None,
            Self::InstrumentControllerDialect { .. } => None,
        }
    }
}

/// A streaming render callback failed before it could render a block.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AudioGraphError {
    BlockTooLarge {
        requested: usize,
        maximum: usize,
    },
    MidiSchedule(MidiScheduleError),
    InstrumentEventBufferFull {
        track_id: TrackId,
    },
    InstrumentProcess {
        track_id: TrackId,
        error: ClapInstrumentError,
    },
    FxProcess {
        track_id: TrackId,
        chain_index: usize,
        error: ClapInstrumentError,
    },
    AudioItemSeekRequiresRefill {
        item_id: ItemId,
    },
    TransportPositionOverflow,
}

impl fmt::Display for AudioGraphError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BlockTooLarge { requested, maximum } => {
                write!(
                    formatter,
                    "audio block has {requested} frames; maximum is {maximum}"
                )
            }
            Self::MidiSchedule(error) => write!(formatter, "MIDI scheduling failed: {error}"),
            Self::InstrumentEventBufferFull { track_id } => write!(
                formatter,
                "MIDI event buffer for track {} is too small",
                track_id.value()
            ),
            Self::InstrumentProcess { track_id, error } => write!(
                formatter,
                "CLAP instrument on track {} failed to process MIDI/audio: {error}",
                track_id.value()
            ),
            Self::FxProcess {
                track_id,
                chain_index,
                error,
            } => write!(
                formatter,
                "CLAP effect in slot {chain_index} on track {} failed: {error}",
                track_id.value()
            ),
            Self::AudioItemSeekRequiresRefill { item_id } => write!(
                formatter,
                "seeking into audio item {} requires refilling its PCM stream",
                item_id.value()
            ),
            Self::TransportPositionOverflow => {
                formatter.write_str("transport position exceeds the supported sample range")
            }
        }
    }
}

impl std::error::Error for AudioGraphError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MidiSchedule(error) => Some(error),
            Self::InstrumentProcess { error, .. } => Some(error),
            Self::FxProcess { error, .. } => Some(error),
            _ => None,
        }
    }
}

/// Results for one rendered callback block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioRenderStats {
    pub block: AudioBlock,
    /// Sum of silence-filled samples across all active PCM sources.
    pub underrun_samples: usize,
    /// MIDI note events written to the caller's event buffer.
    pub midi_event_count: usize,
    /// Interleaved samples clamped to the configured Master sample-peak ceiling.
    pub master_guarded_samples: usize,
    /// Non-finite interleaved samples replaced with silence by the Master output guard.
    pub master_non_finite_samples: usize,
}

/// One SPSC consumer associated with a project AudioItem.
pub struct AudioItemStream {
    item_id: ItemId,
    consumer: PcmStreamConsumer,
    source_start_sample: Option<u64>,
}

/// A prepared CLAP processor associated with one project track.
pub struct TrackInstrumentProcessor {
    track_id: TrackId,
    instance_id: u64,
    processor: ClapInstrumentProcessor,
}

/// A prepared audio effect assigned to one ordered slot in a track's FX chain.
pub struct TrackFxProcessor {
    track_id: TrackId,
    chain_index: usize,
    plugin_id: String,
    instance_id: u64,
    processor: ClapEffectProcessor,
}

impl TrackFxProcessor {
    /// Associates an activated CLAP audio effect with a track chain slot.
    pub fn new(
        track_id: TrackId,
        chain_index: usize,
        plugin_id: impl Into<String>,
        processor: ClapEffectProcessor,
    ) -> Self {
        Self {
            track_id,
            chain_index,
            plugin_id: plugin_id.into(),
            instance_id: processor.instance_id(),
            processor,
        }
    }

    /// Returns the matching control-thread owner's identity.
    pub fn instance_id(&self) -> u64 {
        self.instance_id
    }

    /// Returns the owner identity and activated processor if graph setup fails.
    pub fn into_parts(self) -> (u64, TrackId, usize, String, ClapEffectProcessor) {
        (
            self.instance_id,
            self.track_id,
            self.chain_index,
            self.plugin_id,
            self.processor,
        )
    }
}

impl TrackInstrumentProcessor {
    /// Associates an activated processor with the track whose MIDI it will render.
    pub fn new(track_id: TrackId, processor: ClapInstrumentProcessor) -> Self {
        let instance_id = processor.instance_id();
        Self {
            track_id,
            instance_id,
            processor,
        }
    }

    /// Returns the target track ID.
    pub fn track_id(&self) -> TrackId {
        self.track_id
    }

    /// Returns the matching control-thread owner's identity.
    pub fn instance_id(&self) -> u64 {
        self.instance_id
    }

    /// Returns the ID and processor if graph construction fails.
    pub fn into_parts(self) -> (TrackId, ClapInstrumentProcessor) {
        (self.track_id, self.processor)
    }
}

/// A CLAP processor stopped on the audio thread and ready for control-thread deactivation.
pub struct StoppedTrackInstrument {
    track_id: TrackId,
    instance_id: u64,
    processor: StoppedClapInstrumentProcessor,
}

impl StoppedTrackInstrument {
    /// Returns the track whose processor was stopped.
    pub fn track_id(&self) -> TrackId {
        self.track_id
    }

    /// Returns the identity of the instrument owner that must receive this processor.
    pub fn instance_id(&self) -> u64 {
        self.instance_id
    }

    /// Returns the stopped processor for its matching control-thread owner.
    pub fn into_parts(self) -> (TrackId, StoppedClapInstrumentProcessor) {
        (self.track_id, self.processor)
    }
}

/// A track effect stopped on the audio thread and ready for control-thread deactivation.
pub struct StoppedTrackFxProcessor {
    track_id: TrackId,
    chain_index: usize,
    plugin_id: String,
    instance_id: u64,
    processor: StoppedClapEffectProcessor,
}

impl StoppedTrackFxProcessor {
    /// Returns the target track and chain slot.
    pub fn location(&self) -> (TrackId, usize) {
        (self.track_id, self.chain_index)
    }

    /// Returns the effect plugin ID.
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    /// Returns the identity of the effect owner that must receive this processor.
    pub fn instance_id(&self) -> u64 {
        self.instance_id
    }

    /// Returns the identity and stopped processor for its matching owner.
    pub fn into_parts(self) -> (TrackId, usize, String, StoppedClapEffectProcessor) {
        (
            self.track_id,
            self.chain_index,
            self.plugin_id,
            self.processor,
        )
    }
}

struct InstrumentRoute {
    track_id: TrackId,
    track_index: usize,
    instance_id: u64,
    processor: Option<ClapInstrumentProcessor>,
    stopped_processor: Option<StoppedClapInstrumentProcessor>,
    midi_events: Vec<ScheduledMidiEvent>,
    audio: Vec<[f32; 2]>,
}

struct FxRoute {
    track_id: TrackId,
    track_index: usize,
    chain_index: usize,
    plugin_id: String,
    instance_id: u64,
    processor: Option<ClapEffectProcessor>,
    stopped_processor: Option<StoppedClapEffectProcessor>,
}

type TrackEffectBuffers = Vec<Option<Vec<[f32; 2]>>>;

struct RenderGraphSources {
    streams: Vec<PcmStreamConsumer>,
    track_indices: Vec<usize>,
    ranges: Vec<Option<(u64, u64)>>,
    cursors: Vec<Option<u64>>,
    item_ids: Vec<Option<ItemId>>,
}

fn compile_fx_routes(
    project: &aaadaw_core::Project,
    effects: &mut Vec<TrackFxProcessor>,
    existing: &[FxRoute],
    max_block_frames: usize,
) -> Result<(Vec<FxRoute>, TrackEffectBuffers), AudioGraphBuildError> {
    let mut seen_effect_slots: Vec<Vec<bool>> = project
        .tracks()
        .iter()
        .map(|track| vec![false; track.fx_chain().len()])
        .collect();
    for route in existing {
        if let Some(track_slots) = project
            .tracks()
            .iter()
            .position(|track| track.id() == route.track_id)
            .and_then(|track_index| seen_effect_slots.get_mut(track_index))
            && let Some(slot) = track_slots.get_mut(route.chain_index)
        {
            *slot = true;
        }
    }

    let mut routes = Vec::with_capacity(effects.len());
    for effect in effects.iter() {
        let track_index = project
            .tracks()
            .iter()
            .position(|track| track.id() == effect.track_id)
            .ok_or(AudioGraphBuildError::MissingFxTrack {
                track_id: effect.track_id.value(),
            })?;
        if !project.tracks()[track_index]
            .fx_chain()
            .get(effect.chain_index)
            .is_some_and(|slot| slot.is_enabled() && slot.plugin_id() == effect.plugin_id)
        {
            return Err(AudioGraphBuildError::FxChainSlotMismatch {
                track_id: effect.track_id.value(),
                chain_index: effect.chain_index,
            });
        }
        if std::mem::replace(
            &mut seen_effect_slots[track_index][effect.chain_index],
            true,
        ) {
            return Err(AudioGraphBuildError::DuplicateTrackFxProcessor {
                track_id: effect.track_id.value(),
                chain_index: effect.chain_index,
            });
        }
        let available_block_frames = effect.processor.max_block_frames();
        if available_block_frames < max_block_frames {
            return Err(AudioGraphBuildError::FxProcessorBlockCapacity {
                track_id: effect.track_id.value(),
                chain_index: effect.chain_index,
                required: max_block_frames,
                available: available_block_frames,
            });
        }
        routes.push(FxRoute {
            track_id: effect.track_id,
            track_index,
            chain_index: effect.chain_index,
            plugin_id: effect.plugin_id.clone(),
            instance_id: effect.instance_id,
            processor: None,
            stopped_processor: None,
        });
    }

    routes.sort_by_key(|route| (route.track_index, route.chain_index));
    effects.sort_by_key(|effect| {
        (
            project
                .tracks()
                .iter()
                .position(|track| track.id() == effect.track_id)
                .expect("validated effect track remains present"),
            effect.chain_index,
        )
    });
    for (route, effect) in routes.iter_mut().zip(effects.drain(..)) {
        route.processor = Some(effect.processor);
    }

    let mut track_has_effects = vec![false; project.tracks().len()];
    for route in existing.iter().chain(&routes) {
        track_has_effects[route.track_index] = true;
    }
    let track_effect_buffers = track_has_effects
        .iter()
        .map(|has_effect| has_effect.then(|| vec![[0.0; 2]; max_block_frames]))
        .collect();
    Ok((routes, track_effect_buffers))
}

impl AudioItemStream {
    /// Associates a worker-fed PCM consumer with its timeline item.
    pub fn new(item_id: ItemId, consumer: PcmStreamConsumer) -> Self {
        Self {
            item_id,
            consumer,
            source_start_sample: None,
        }
    }

    /// Associates a refilled stream whose first PCM sample corresponds to the given timeline sample.
    /// Position the graph transport at this sample before playback; the stream contains no earlier PCM.
    pub fn new_at_sample(
        item_id: ItemId,
        source_start_sample: u64,
        consumer: PcmStreamConsumer,
    ) -> Self {
        Self {
            item_id,
            consumer,
            source_start_sample: Some(source_start_sample),
        }
    }
}

/// A fixed-topology streaming mixer suitable for a device callback.
///
/// Construct this on a control thread. Scratch buffers, source-to-track routes,
/// and sample-clock item ranges are compiled before audio starts. Worker threads
/// decode/resample PCM and feed one SPSC consumer per source.
pub struct AudioRenderGraph {
    mixer: MixerPlan,
    master_output_safety: master_output::MasterOutputSafety,
    midi_plan: MidiEventPlan,
    midi_scratch: Vec<Option<ScheduledMidiEvent>>,
    instruments: Vec<InstrumentRoute>,
    effects: Vec<FxRoute>,
    track_effect_buffers: TrackEffectBuffers,
    sample_rate: u32,
    transport: Transport,
    last_midi_sample_end: Option<u64>,
    last_midi_chase_generation: Option<u64>,
    streams: Vec<PcmStreamConsumer>,
    stream_track_indices: Vec<usize>,
    source_ranges: Vec<Option<(u64, u64)>>,
    source_cursors: Vec<Option<u64>>,
    source_item_ids: Vec<Option<ItemId>>,
    scratch: Vec<Vec<f32>>,
}

impl AudioRenderGraph {
    /// Compiles one continuously streamed PCM source per project track.
    pub fn new(
        project: &aaadaw_core::Project,
        streams: Vec<PcmStreamConsumer>,
        max_block_frames: usize,
    ) -> Result<Self, AudioGraphBuildError> {
        if streams.len() != project.tracks().len() {
            return Err(AudioGraphBuildError::TrackStreamCountMismatch {
                tracks: project.tracks().len(),
                streams: streams.len(),
            });
        }
        let sources = RenderGraphSources {
            track_indices: (0..streams.len()).collect(),
            ranges: vec![None; streams.len()],
            cursors: vec![None; streams.len()],
            item_ids: vec![None; streams.len()],
            streams,
        };
        let mut instruments = Vec::new();
        let mut effects = Vec::new();
        Self::build(
            project,
            sources,
            &mut instruments,
            &mut effects,
            max_block_frames,
        )
    }

    /// Compiles one PCM consumer per AudioItem, preserving and validating project item order.
    pub fn new_for_audio_items(
        project: &aaadaw_core::Project,
        item_streams: Vec<AudioItemStream>,
        max_block_frames: usize,
    ) -> Result<Self, AudioGraphBuildError> {
        let mut instruments = Vec::new();
        Self::new_for_audio_items_with_instruments(
            project,
            item_streams,
            &mut instruments,
            max_block_frames,
        )
    }

    /// Compiles AudioItem streams and track-associated CLAP processors into a render graph.
    ///
    /// Each processor must have been activated off the audio thread with enough event and frame
    /// capacity for its track. Its owner remains on the control thread. On error, `instruments`
    /// is left intact so the caller can stop and deactivate its processors.
    pub fn new_for_audio_items_with_instruments(
        project: &aaadaw_core::Project,
        item_streams: Vec<AudioItemStream>,
        instruments: &mut Vec<TrackInstrumentProcessor>,
        max_block_frames: usize,
    ) -> Result<Self, AudioGraphBuildError> {
        let mut effects = Vec::new();
        Self::new_for_audio_items_with_processors(
            project,
            item_streams,
            instruments,
            &mut effects,
            max_block_frames,
        )
    }

    /// Compiles AudioItem streams, track instruments, and ordered audio effects into one graph.
    ///
    /// `instruments` and `effects` remain intact on build failure so their matching owners can
    /// cleanly stop and deactivate the processors.
    pub fn new_for_audio_items_with_processors(
        project: &aaadaw_core::Project,
        item_streams: Vec<AudioItemStream>,
        instruments: &mut Vec<TrackInstrumentProcessor>,
        effects: &mut Vec<TrackFxProcessor>,
        max_block_frames: usize,
    ) -> Result<Self, AudioGraphBuildError> {
        if item_streams.len() != project.audio_items().len() {
            return Err(AudioGraphBuildError::AudioItemStreamCountMismatch {
                items: project.audio_items().len(),
                streams: item_streams.len(),
            });
        }

        let mut sources = RenderGraphSources {
            streams: Vec::with_capacity(item_streams.len()),
            track_indices: Vec::with_capacity(item_streams.len()),
            ranges: Vec::with_capacity(item_streams.len()),
            cursors: Vec::with_capacity(item_streams.len()),
            item_ids: Vec::with_capacity(item_streams.len()),
        };
        for (item, stream) in project.audio_items().iter().zip(item_streams) {
            if item.id() != stream.item_id {
                return Err(AudioGraphBuildError::AudioItemStreamOrderMismatch {
                    expected: item.id(),
                    found: stream.item_id,
                });
            }
            let source_start_sample = stream
                .source_start_sample
                .unwrap_or_else(|| item.start_sample());
            if !(item.start_sample()..=item.end_sample()).contains(&source_start_sample) {
                return Err(AudioGraphBuildError::AudioItemStreamStartOutOfRange {
                    item_id: item.id(),
                    start_sample: source_start_sample,
                });
            }
            let track_index = project
                .tracks()
                .iter()
                .position(|track| track.id() == item.track_id())
                .expect("Project guarantees every AudioItem has an existing track");
            sources.streams.push(stream.consumer);
            sources.track_indices.push(track_index);
            sources
                .ranges
                .push(Some((item.start_sample(), item.end_sample())));
            sources.cursors.push(Some(source_start_sample));
            sources.item_ids.push(Some(item.id()));
        }

        Self::build(project, sources, instruments, effects, max_block_frames)
    }

    fn build(
        project: &aaadaw_core::Project,
        sources: RenderGraphSources,
        instrument_processors: &mut Vec<TrackInstrumentProcessor>,
        effect_processors: &mut Vec<TrackFxProcessor>,
        max_block_frames: usize,
    ) -> Result<Self, AudioGraphBuildError> {
        let mixer = MixerPlan::compile(project.tracks(), max_block_frames)
            .map_err(AudioGraphBuildError::MixerPlan)?;
        let midi_plan =
            MidiEventPlan::compile(project).map_err(AudioGraphBuildError::MidiSchedule)?;
        let mut instrument_routes = Vec::with_capacity(instrument_processors.len());
        let mut has_instrument = vec![false; project.tracks().len()];
        for instrument in instrument_processors.iter() {
            let track_index = project
                .tracks()
                .iter()
                .position(|track| track.id() == instrument.track_id)
                .ok_or(AudioGraphBuildError::MissingInstrumentTrack {
                    track_id: instrument.track_id.value(),
                })?;
            if std::mem::replace(&mut has_instrument[track_index], true) {
                return Err(AudioGraphBuildError::DuplicateTrackInstrument {
                    track_id: instrument.track_id.value(),
                });
            }
            let available_block_frames = instrument.processor.max_block_frames();
            if available_block_frames < max_block_frames {
                return Err(AudioGraphBuildError::InstrumentBlockCapacity {
                    track_id: instrument.track_id.value(),
                    required: max_block_frames,
                    available: available_block_frames,
                });
            }
            let required_events = midi_plan.event_count_for_track(instrument.track_id);
            let available_events = instrument.processor.max_events();
            if available_events < required_events {
                return Err(AudioGraphBuildError::InstrumentEventCapacity {
                    track_id: instrument.track_id.value(),
                    required: required_events,
                    available: available_events,
                });
            }
            if midi_plan.controller_event_count_for_track(instrument.track_id) > 0
                && !instrument.processor.supports_midi_controllers()
            {
                return Err(AudioGraphBuildError::InstrumentControllerDialect {
                    track_id: instrument.track_id.value(),
                });
            }
            instrument_routes.push(InstrumentRoute {
                track_id: instrument.track_id,
                track_index,
                instance_id: instrument.instance_id,
                midi_events: Vec::with_capacity(available_events),
                audio: vec![[0.0, 0.0]; max_block_frames],
                processor: None,
                stopped_processor: None,
            });
        }
        let (effect_routes, track_effect_buffers) =
            compile_fx_routes(project, effect_processors, &[], max_block_frames)?;
        let scratch = (0..sources.streams.len())
            .map(|_| vec![0.0; max_block_frames])
            .collect();
        let midi_scratch = vec![None; midi_plan.len()];
        for (route, instrument) in instrument_routes
            .iter_mut()
            .zip(instrument_processors.drain(..))
        {
            route.processor = Some(instrument.processor);
        }
        Ok(Self {
            mixer,
            master_output_safety: master_output::MasterOutputSafety::default(),
            midi_plan,
            midi_scratch,
            instruments: instrument_routes,
            effects: effect_routes,
            track_effect_buffers,
            sample_rate: project.settings().sample_rate(),
            transport: Transport::new(),
            last_midi_sample_end: None,
            last_midi_chase_generation: None,
            streams: sources.streams,
            stream_track_indices: sources.track_indices,
            source_ranges: sources.ranges,
            source_cursors: sources.cursors,
            source_item_ids: sources.item_ids,
            scratch,
        })
    }

    /// Returns the project sample rate used by its tempo map.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the maximum frames accepted by the preallocated callback buffers.
    pub fn max_block_frames(&self) -> usize {
        self.mixer.max_block_frames
    }

    /// Returns a control-thread handle for live volume/pan updates on this graph.
    pub fn track_mix_controller(&self) -> TrackMixController {
        self.mixer.track_mix_controller()
    }

    /// Returns a lock-free control handle for the final Master sample-peak ceiling.
    pub fn master_output_safety_controller(&self) -> MasterOutputSafetyController {
        self.master_output_safety.controller()
    }

    /// Returns the callback-owned transport for start/stop/seek control.
    pub fn transport_mut(&mut self) -> &mut Transport {
        &mut self.transport
    }

    /// Returns the number of prepared track instruments.
    pub fn instrument_count(&self) -> usize {
        self.instruments.len()
    }

    /// Returns the MIDI event capacity required by one track in the compiled project schedule.
    pub fn midi_event_capacity_for_track(&self, track_id: TrackId) -> usize {
        self.midi_plan.event_count_for_track(track_id)
    }

    /// Installs pre-activated track instruments before the graph enters an audio callback.
    ///
    /// `project` must be the same project snapshot used to compile this graph. On validation
    /// failure, `instruments` remains intact so matching owners can stop and deactivate them.
    pub fn install_instrument_processors(
        &mut self,
        project: &aaadaw_core::Project,
        instruments: &mut Vec<TrackInstrumentProcessor>,
    ) -> Result<(), AudioGraphBuildError> {
        if instruments.is_empty() {
            return Ok(());
        }
        let mut seen_tracks = vec![false; project.tracks().len()];
        for route in &self.instruments {
            if let Some(track_index) = project
                .tracks()
                .iter()
                .position(|track| track.id() == route.track_id)
                && let Some(seen) = seen_tracks.get_mut(track_index)
            {
                *seen = true;
            }
        }
        let mut routes = Vec::with_capacity(instruments.len());
        for instrument in instruments.iter() {
            let track_index = project
                .tracks()
                .iter()
                .position(|track| track.id() == instrument.track_id)
                .ok_or(AudioGraphBuildError::MissingInstrumentTrack {
                    track_id: instrument.track_id.value(),
                })?;
            if std::mem::replace(&mut seen_tracks[track_index], true) {
                return Err(AudioGraphBuildError::DuplicateTrackInstrument {
                    track_id: instrument.track_id.value(),
                });
            }
            let available_block_frames = instrument.processor.max_block_frames();
            if available_block_frames < self.max_block_frames() {
                return Err(AudioGraphBuildError::InstrumentBlockCapacity {
                    track_id: instrument.track_id.value(),
                    required: self.max_block_frames(),
                    available: available_block_frames,
                });
            }
            let required_events = self.midi_plan.event_count_for_track(instrument.track_id);
            let available_events = instrument.processor.max_events();
            if available_events < required_events {
                return Err(AudioGraphBuildError::InstrumentEventCapacity {
                    track_id: instrument.track_id.value(),
                    required: required_events,
                    available: available_events,
                });
            }
            if self
                .midi_plan
                .controller_event_count_for_track(instrument.track_id)
                > 0
                && !instrument.processor.supports_midi_controllers()
            {
                return Err(AudioGraphBuildError::InstrumentControllerDialect {
                    track_id: instrument.track_id.value(),
                });
            }
            routes.push(InstrumentRoute {
                track_id: instrument.track_id,
                track_index,
                instance_id: instrument.instance_id,
                midi_events: Vec::with_capacity(available_events),
                audio: vec![[0.0, 0.0]; self.max_block_frames()],
                processor: None,
                stopped_processor: None,
            });
        }

        routes.sort_by_key(|route| route.track_index);
        instruments.sort_by_key(|instrument| {
            project
                .tracks()
                .iter()
                .position(|track| track.id() == instrument.track_id)
                .expect("validated instrument track remains present")
        });
        for (route, instrument) in routes.iter_mut().zip(instruments.drain(..)) {
            route.processor = Some(instrument.processor);
        }
        self.instruments.append(&mut routes);
        self.instruments.sort_by_key(|route| route.track_index);
        if self.midi_scratch.len() < self.midi_plan.len() {
            self.midi_scratch.resize(self.midi_plan.len(), None);
        }
        Ok(())
    }

    /// Installs pre-activated track effects before the graph enters an audio callback.
    ///
    /// `project` must be the same project snapshot used to compile this graph. On validation
    /// failure, `effects` remains intact so its matching owners can stop and deactivate them.
    pub fn install_fx_processors(
        &mut self,
        project: &aaadaw_core::Project,
        effects: &mut Vec<TrackFxProcessor>,
    ) -> Result<(), AudioGraphBuildError> {
        if effects.is_empty() {
            return Ok(());
        }
        let (mut routes, buffers) =
            compile_fx_routes(project, effects, &self.effects, self.max_block_frames())?;
        self.effects.append(&mut routes);
        self.effects
            .sort_by_key(|route| (route.track_index, route.chain_index));
        self.track_effect_buffers = buffers;
        Ok(())
    }

    /// Releases held MIDI voices and resets controller-capable instruments without
    /// changing the transport state.
    #[cfg(any(
        feature = "jack-backend",
        feature = "pipewire-backend",
        all(feature = "wasapi-backend", target_os = "windows"),
        test
    ))]
    pub(crate) fn release_midi_notes(&mut self) -> usize {
        let mut failures = 0;
        for route in &mut self.instruments {
            if route
                .processor
                .as_mut()
                .is_some_and(|processor| processor.all_notes_off().is_err())
            {
                failures += 1;
            }
        }
        failures
    }

    /// Stops processors on the audio thread before their graph moves to the retirement queue.
    #[cfg(any(
        feature = "jack-backend",
        feature = "pipewire-backend",
        all(feature = "wasapi-backend", target_os = "windows"),
        test
    ))]
    pub(crate) fn stop_instruments(&mut self) -> usize {
        let mut failures = 0;
        for route in &mut self.instruments {
            if let Some(processor) = route.processor.take() {
                let (stopped, released) = processor.stop_with_status();
                failures += usize::from(!released);
                route.stopped_processor = Some(stopped);
            }
        }
        failures
    }

    /// Stops track FX processors on the audio thread before graph retirement.
    #[cfg(any(
        feature = "jack-backend",
        feature = "pipewire-backend",
        all(feature = "wasapi-backend", target_os = "windows"),
        test
    ))]
    pub(crate) fn stop_fx_processors(&mut self) -> usize {
        let mut failures = 0;
        for route in &mut self.effects {
            if let Some(processor) = route.processor.take() {
                route.stopped_processor = Some(processor.stop());
            } else {
                failures += 1;
            }
        }
        failures
    }

    /// Moves stopped processors out after the retired graph reaches a control thread.
    pub fn take_stopped_instruments(&mut self) -> Vec<StoppedTrackInstrument> {
        self.instruments
            .iter_mut()
            .filter_map(|route| {
                route
                    .stopped_processor
                    .take()
                    .map(|processor| StoppedTrackInstrument {
                        track_id: route.track_id,
                        instance_id: route.instance_id,
                        processor,
                    })
            })
            .collect()
    }

    /// Moves stopped track FX processors out after the retired graph reaches a control thread.
    pub fn take_stopped_fx_processors(&mut self) -> Vec<StoppedTrackFxProcessor> {
        self.effects
            .iter_mut()
            .filter_map(|route| {
                route
                    .stopped_processor
                    .take()
                    .map(|processor| StoppedTrackFxProcessor {
                        track_id: route.track_id,
                        chain_index: route.chain_index,
                        plugin_id: route.plugin_id.clone(),
                        instance_id: route.instance_id,
                        processor,
                    })
            })
            .collect()
    }

    /// Renders one block into caller-owned interleaved stereo memory.
    ///
    /// The callback path allocates no memory, takes no locks, and performs no
    /// I/O. Stopped blocks are cleared without consuming PCM. Underrunning
    /// streams are zero-filled and counted in the returned statistics.
    pub fn render_into(
        &mut self,
        output: &mut [[f32; 2]],
    ) -> Result<AudioRenderStats, AudioGraphError> {
        self.render_block(false, &mut [], output)
    }

    /// Renders audio and writes note events for the same half-open callback
    /// block. A too-small event buffer fails before transport or PCM is changed.
    pub fn render_with_midi(
        &mut self,
        midi_output: &mut [Option<ScheduledMidiEvent>],
        output: &mut [[f32; 2]],
    ) -> Result<AudioRenderStats, AudioGraphError> {
        self.render_block(true, midi_output, output)
    }

    fn render_block(
        &mut self,
        include_midi: bool,
        midi_output: &mut [Option<ScheduledMidiEvent>],
        output: &mut [[f32; 2]],
    ) -> Result<AudioRenderStats, AudioGraphError> {
        if output.len() > self.mixer.max_block_frames {
            return Err(AudioGraphError::BlockTooLarge {
                requested: output.len(),
                maximum: self.mixer.max_block_frames,
            });
        }
        let block_start_sample = self.transport.position_samples();
        if self.transport.is_playing() {
            let frame_count = u64::try_from(output.len())
                .map_err(|_| AudioGraphError::TransportPositionOverflow)?;
            let block_end_sample = block_start_sample
                .checked_add(frame_count)
                .ok_or(AudioGraphError::TransportPositionOverflow)?;
            for (index, source_range) in self.source_ranges.iter().enumerate() {
                let Some((item_start, item_end)) = source_range else {
                    continue;
                };
                let overlap_start = block_start_sample.max(*item_start);
                let overlap_end = block_end_sample.min(*item_end);
                if overlap_start < overlap_end && self.source_cursors[index] != Some(overlap_start)
                {
                    return Err(AudioGraphError::AudioItemSeekRequiresRefill {
                        item_id: self.source_item_ids[index]
                            .expect("timeline ranges belong to audio items"),
                    });
                }
            }
        }

        let was_playing = self.transport.is_playing();
        let chase_generation = self.transport.chase_generation();
        let midi_is_processed = !self.instruments.is_empty() || include_midi;
        let midi_event_count = if was_playing && !output.is_empty() && midi_is_processed {
            let (chase_controller_count, chase_note_count) = if self.last_midi_sample_end
                != Some(block_start_sample)
                || self.last_midi_chase_generation != Some(chase_generation)
            {
                let controller_count = self
                    .midi_plan
                    .active_controllers_at(block_start_sample, &mut self.midi_scratch)
                    .map_err(AudioGraphError::MidiSchedule)?;
                let note_count = self
                    .midi_plan
                    .active_notes_at(
                        block_start_sample,
                        &mut self.midi_scratch[controller_count..],
                    )
                    .map_err(AudioGraphError::MidiSchedule)?;
                (controller_count, note_count)
            } else {
                (0, 0)
            };
            let chase_count = chase_controller_count + chase_note_count;
            let scheduled_count = self
                .midi_plan
                .events_for_block(
                    block_start_sample,
                    output.len(),
                    &mut self.midi_scratch[chase_count..],
                )
                .map_err(AudioGraphError::MidiSchedule)?;
            let count = chase_count + scheduled_count;
            if chase_count > 0 {
                self.midi_scratch[..count].sort_unstable_by_key(|event| {
                    let event = event.expect("rendered MIDI event slots are initialized");
                    (
                        event.sample_offset,
                        event.sort_priority(),
                        event.track_id.value(),
                        event.controller.unwrap_or(event.pitch),
                        event.note_id.map_or(0, aaadaw_core::NoteId::value),
                    )
                });
            }
            if include_midi {
                if midi_output.len() < count {
                    return Err(AudioGraphError::MidiSchedule(
                        MidiScheduleError::OutputBufferTooSmall {
                            required: count,
                            available: midi_output.len(),
                        },
                    ));
                }
                midi_output
                    .iter_mut()
                    .zip(self.midi_scratch.iter().take(count))
                    .for_each(|(destination, source)| *destination = *source);
            }
            count
        } else {
            0
        };
        let block = self
            .transport
            .advance_block(output.len())
            .map_err(|_| AudioGraphError::TransportPositionOverflow)?;
        if !block.is_playing {
            output.fill([0.0, 0.0]);
            return Ok(AudioRenderStats {
                block,
                underrun_samples: 0,
                midi_event_count,
                master_guarded_samples: 0,
                master_non_finite_samples: 0,
            });
        }
        output.fill([0.0, 0.0]);
        for buffer in self.track_effect_buffers.iter_mut().flatten() {
            buffer[..output.len()].fill([0.0, 0.0]);
        }
        let block_frame_count = u64::try_from(block.frame_count)
            .map_err(|_| AudioGraphError::TransportPositionOverflow)?;
        let block_end_sample = block
            .start_sample
            .checked_add(block_frame_count)
            .ok_or(AudioGraphError::TransportPositionOverflow)?;
        let mut underrun_samples = 0_usize;
        for stream_index in 0..self.streams.len() {
            let input = &mut self.scratch[stream_index][..output.len()];
            input.fill(0.0);
            if let Some((item_start, item_end)) = self.source_ranges[stream_index] {
                let overlap_start = block.start_sample.max(item_start);
                let overlap_end = block_end_sample.min(item_end);
                if overlap_start < overlap_end {
                    let offset = (overlap_start - block.start_sample) as usize;
                    let length = (overlap_end - overlap_start) as usize;
                    let underruns =
                        self.streams[stream_index].read_into(&mut input[offset..offset + length]);
                    underrun_samples = underrun_samples.saturating_add(underruns);
                    self.source_cursors[stream_index] = Some(overlap_end);
                }
            } else {
                underrun_samples =
                    underrun_samples.saturating_add(self.streams[stream_index].read_into(input));
            }
            let track_index = self.stream_track_indices[stream_index];
            if let Some(track_buffer) = self.track_effect_buffers[track_index].as_mut() {
                for (frame, sample) in track_buffer[..output.len()]
                    .iter_mut()
                    .zip(input.iter().copied())
                {
                    frame[0] += sample;
                    frame[1] += sample;
                }
            } else {
                self.mixer.mix_track_unchecked(track_index, input, output);
            }
        }
        let scheduled_events = self.midi_scratch.iter().take(midi_event_count);
        for route in &mut self.instruments {
            route.midi_events.clear();
            for event in scheduled_events.clone().flatten() {
                if event.track_id == route.track_id {
                    if route.midi_events.len() == route.midi_events.capacity() {
                        return Err(AudioGraphError::InstrumentEventBufferFull {
                            track_id: route.track_id,
                        });
                    }
                    route.midi_events.push(*event);
                }
            }
            let processor = route
                .processor
                .as_mut()
                .expect("active render graphs retain their instrument processors");
            processor
                .process(&route.midi_events, &mut route.audio[..output.len()])
                .map_err(|error| AudioGraphError::InstrumentProcess {
                    track_id: route.track_id,
                    error,
                })?;
            if let Some(track_buffer) = self.track_effect_buffers[route.track_index].as_mut() {
                for (frame, sample) in track_buffer[..output.len()]
                    .iter_mut()
                    .zip(route.audio[..output.len()].iter())
                {
                    frame[0] += sample[0];
                    frame[1] += sample[1];
                }
            } else {
                self.mixer.mix_stereo_track_unchecked(
                    route.track_index,
                    &route.audio[..output.len()],
                    output,
                );
            }
        }
        for route in &mut self.effects {
            let track_buffer = self.track_effect_buffers[route.track_index]
                .as_mut()
                .expect("active effects have a preallocated track buffer");
            route
                .processor
                .as_mut()
                .expect("active graphs retain their effect processors")
                .process(&mut track_buffer[..output.len()])
                .map_err(|error| AudioGraphError::FxProcess {
                    track_id: route.track_id,
                    chain_index: route.chain_index,
                    error,
                })?;
        }
        for (track_index, track_buffer) in self.track_effect_buffers.iter().enumerate() {
            if let Some(track_buffer) = track_buffer {
                self.mixer.mix_stereo_track_unchecked(
                    track_index,
                    &track_buffer[..output.len()],
                    output,
                );
            }
        }
        let master_guard = self.master_output_safety.process(output);
        if was_playing && midi_is_processed && block.frame_count > 0 {
            self.last_midi_sample_end = Some(
                block
                    .start_sample
                    .checked_add(
                        u64::try_from(block.frame_count)
                            .map_err(|_| AudioGraphError::TransportPositionOverflow)?,
                    )
                    .ok_or(AudioGraphError::TransportPositionOverflow)?,
            );
            self.last_midi_chase_generation = Some(chase_generation);
        }
        Ok(AudioRenderStats {
            block,
            underrun_samples,
            midi_event_count,
            master_guarded_samples: master_guard.guarded_samples,
            master_non_finite_samples: master_guard.non_finite_samples,
        })
    }
}
