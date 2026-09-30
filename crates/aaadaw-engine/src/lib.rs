//! Realtime-oriented audio processing primitives.
//!
//! This crate currently provides a precompiled, allocation-free mono-track
//! mixer. Device integration, playback scheduling, and audio asset decoding are
//! separate follow-up work.

mod midi;
mod pcm;
mod transport;

pub use midi::{MidiEventKind, MidiEventPlan, MidiScheduleError, ScheduledMidiEvent};
pub use pcm::{MonoPcmClip, MonoPcmPlayer, PcmError};
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
            let pan_angle = (f64::from(track.pan()) + 1.0) * FRAC_PI_4;
            let gain = linear_gain as f32;
            compiled.push(TrackGains {
                left: pan_angle.cos() as f32 * gain,
                right: pan_angle.sin() as f32 * gain,
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
        for (track, input) in self.tracks.iter().zip(inputs) {
            if track.muted || (self.has_solo && !track.solo) {
                continue;
            }
            for (frame, sample) in output.iter_mut().zip(input.iter().copied()) {
                frame[0] += sample * track.left;
                frame[1] += sample * track.right;
            }
        }
        Ok(())
    }
}
