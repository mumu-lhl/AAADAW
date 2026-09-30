//! Realtime-oriented audio processing primitives.
//!
//! This crate provides a fixed-topology, allocation-free streaming mixer,
//! transport, and MIDI scheduling primitives. Device backends and project
//! AudioItem integration remain separate follow-up work.

mod midi;
mod pcm;
mod stream;
mod transport;

pub use midi::{MidiEventKind, MidiEventPlan, MidiScheduleError, ScheduledMidiEvent};
pub use pcm::{MonoPcmClip, MonoPcmPlayer, PcmError};
pub use stream::{PcmStreamConsumer, PcmStreamError, PcmStreamProducer, pcm_stream};
pub use transport::{AudioBlock, Transport, TransportPositionOverflow};

use aaadaw_core::Track;
use std::f64::consts::FRAC_PI_4;
use std::fmt;

/// Precomputed per-track mix coefficients, built outside the audio callback.
#[derive(Clone, Debug)]
pub struct MixerPlan {
    max_block_frames: usize,
    tracks: Vec<TrackGains>,
    has_solo: bool,
}

#[derive(Clone, Copy, Debug)]
struct TrackGains {
    left: f32,
    right: f32,
    muted: bool,
    solo: bool,
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
            let linear_gain = 10.0_f64.powf(f64::from(track.volume_db()) / 20.0);
            if !linear_gain.is_finite() || linear_gain > f64::from(f32::MAX) {
                return Err(MixerPlanError::InvalidTrackGain {
                    track_id: track.id().value(),
                });
            }
            let gain = linear_gain as f32;
            let (left, right) = match track.pan() {
                -1.0 => (gain, 0.0),
                1.0 => (0.0, gain),
                pan => {
                    let angle = (f64::from(pan) + 1.0) * FRAC_PI_4;
                    (angle.cos() as f32 * gain, angle.sin() as f32 * gain)
                }
            };
            compiled.push(TrackGains {
                left,
                right,
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
        let track = self.tracks[track_index];
        if track.muted || (self.has_solo && !track.solo) {
            return;
        }
        for (frame, sample) in output.iter_mut().zip(input.iter().copied()) {
            frame[0] += sample * track.left;
            frame[1] += sample * track.right;
        }
    }
}

/// Construction failure for a streaming graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AudioGraphBuildError {
    MixerPlan(MixerPlanError),
    MidiSchedule(MidiScheduleError),
    TrackStreamCountMismatch { tracks: usize, streams: usize },
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
        }
    }
}

impl std::error::Error for AudioGraphBuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MixerPlan(error) => Some(error),
            Self::MidiSchedule(error) => Some(error),
            Self::TrackStreamCountMismatch { .. } => None,
        }
    }
}

/// A streaming render callback failed before it could render a block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioGraphError {
    BlockTooLarge { requested: usize, maximum: usize },
    MidiSchedule(MidiScheduleError),
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
            _ => None,
        }
    }
}

/// Results for one rendered callback block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioRenderStats {
    pub block: AudioBlock,
    /// Sum of silence-filled samples across all tracks.
    pub underrun_samples: usize,
    /// MIDI note events written to the caller's event buffer.
    pub midi_event_count: usize,
}

/// A fixed-topology streaming mixer suitable for a device callback.
///
/// Construct this on a control thread. It owns preallocated per-track scratch
/// buffers and one SPSC consumer per project track; the producer side should
/// decode/resample on a worker and push samples at the device's active rate.
pub struct AudioRenderGraph {
    mixer: MixerPlan,
    midi_plan: MidiEventPlan,
    transport: Transport,
    streams: Vec<PcmStreamConsumer>,
    scratch: Vec<Vec<f32>>,
}

impl AudioRenderGraph {
    /// Compiles track controls and allocates scratch storage before audio starts.
    pub fn new(
        project: &aaadaw_core::Project,
        streams: Vec<PcmStreamConsumer>,
        max_block_frames: usize,
    ) -> Result<Self, AudioGraphBuildError> {
        let mixer = MixerPlan::compile(project.tracks(), max_block_frames)
            .map_err(AudioGraphBuildError::MixerPlan)?;
        let midi_plan =
            MidiEventPlan::compile(project).map_err(AudioGraphBuildError::MidiSchedule)?;
        if streams.len() != mixer.tracks.len() {
            return Err(AudioGraphBuildError::TrackStreamCountMismatch {
                tracks: mixer.tracks.len(),
                streams: streams.len(),
            });
        }
        let scratch = (0..streams.len())
            .map(|_| vec![0.0; max_block_frames])
            .collect();
        Ok(Self {
            mixer,
            midi_plan,
            transport: Transport::new(),
            streams,
            scratch,
        })
    }

    /// Returns the callback-owned transport for start/stop/seek control.
    pub fn transport_mut(&mut self) -> &mut Transport {
        &mut self.transport
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
        let midi_event_count = if self.transport.is_playing() && include_midi {
            self.midi_plan
                .events_for_block(self.transport.position_samples(), output.len(), midi_output)
                .map_err(AudioGraphError::MidiSchedule)?
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
            });
        }

        output.fill([0.0, 0.0]);
        let mut underrun_samples = 0_usize;
        for track_index in 0..self.streams.len() {
            let input = &mut self.scratch[track_index][..output.len()];
            underrun_samples =
                underrun_samples.saturating_add(self.streams[track_index].read_into(input));
            self.mixer.mix_track_unchecked(track_index, input, output);
        }
        Ok(AudioRenderStats {
            block,
            underrun_samples,
            midi_event_count,
        })
    }
}
