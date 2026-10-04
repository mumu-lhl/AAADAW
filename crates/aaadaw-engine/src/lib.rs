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
    AudioCaptureConsumer, AudioCaptureControl, AudioCaptureProducer, AudioInputMonitorGate,
    AudioMonitorConsumer, AudioMonitorProducer, CapturedFrames, audio_capture_stream,
    audio_monitor_stream,
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
pub use stream::{
    PcmStreamConsumer, PcmStreamError, PcmStreamProducer, StereoPcmStreamConsumer,
    StereoPcmStreamProducer, pcm_stream, stereo_pcm_stream,
};
pub use transport::{AudioBlock, Transport, TransportClockAnchor, TransportPositionOverflow};
#[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
pub use wasapi_input::{WasapiAudioInput, WasapiInputError};
#[cfg(all(feature = "wasapi-backend", target_os = "windows"))]
pub use wasapi_output::{WasapiAudioOutput, WasapiOutputError, WasapiOutputStats};

use aaadaw_core::{ItemId, Track, TrackId, VolumeAutomationPoint};
use std::cell::Cell;
use std::f64::consts::FRAC_PI_4;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Precomputed per-track mix coefficients, built outside the audio callback.
#[derive(Clone, Debug)]
pub struct MixerPlan {
    max_block_frames: usize,
    tracks: Vec<TrackGains>,
    routing_order: Vec<usize>,
    has_solo: Arc<AtomicBool>,
}

#[derive(Clone, Debug)]
struct TrackGains {
    track_id: TrackId,
    output_track_index: Option<usize>,
    is_bus: bool,
    live: Arc<LiveTrackGains>,
    mix_ramp: Cell<GainRamp>,
    record_armed: bool,
    volume_automation: Vec<VolumeAutomationPoint>,
}

#[derive(Debug)]
struct LiveTrackGains {
    version: AtomicU32,
    left: AtomicU32,
    right: AtomicU32,
    stereo_left: AtomicU32,
    stereo_right: AtomicU32,
    muted: AtomicBool,
    solo: AtomicBool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct GainCoefficients {
    left: f32,
    right: f32,
    stereo_left: f32,
    stereo_right: f32,
}

#[derive(Clone, Copy, Debug)]
struct GainRamp {
    current: GainCoefficients,
    target: GainCoefficients,
    step: GainCoefficients,
    remaining_frames: usize,
    ramp_frames: usize,
}

impl GainRamp {
    fn new(initial: GainCoefficients, ramp_frames: usize) -> Self {
        Self {
            current: initial,
            target: initial,
            step: GainCoefficients::zero(),
            remaining_frames: 0,
            ramp_frames: ramp_frames.max(1),
        }
    }

    fn retarget(&mut self, target: GainCoefficients) {
        if target == self.target {
            return;
        }
        self.target = target;
        self.remaining_frames = self.ramp_frames;
        let divisor = self.ramp_frames as f32;
        self.step = GainCoefficients {
            left: (target.left - self.current.left) / divisor,
            right: (target.right - self.current.right) / divisor,
            stereo_left: (target.stereo_left - self.current.stereo_left) / divisor,
            stereo_right: (target.stereo_right - self.current.stereo_right) / divisor,
        };
    }

    fn next_frame(&mut self) -> GainCoefficients {
        if self.remaining_frames > 0 {
            self.current.left += self.step.left;
            self.current.right += self.step.right;
            self.current.stereo_left += self.step.stereo_left;
            self.current.stereo_right += self.step.stereo_right;
            self.remaining_frames -= 1;
            if self.remaining_frames == 0 {
                self.current = self.target;
            }
        }
        self.current
    }
}

impl GainCoefficients {
    fn zero() -> Self {
        Self {
            left: 0.0,
            right: 0.0,
            stereo_left: 0.0,
            stereo_right: 0.0,
        }
    }
}

#[derive(Clone, Copy)]
struct MixBlock {
    start_sample: u64,
    advances_timeline: bool,
    frame_count: usize,
}

/// Control-thread handle for retargeting the active graph's per-track volume and pan.
///
/// Values are fully converted to gain coefficients on the caller thread. The audio callback
/// performs bounded atomic loads and applies stable targets through preallocated sample ramps.
#[derive(Clone, Debug)]
pub struct TrackMixController {
    tracks: Vec<(TrackId, Arc<LiveTrackGains>)>,
    has_solo: Arc<AtomicBool>,
}

/// Control-thread handle for routing the live input tap to explicitly monitored armed tracks.
#[derive(Clone, Debug)]
pub struct AudioInputMonitorController {
    gate: AudioInputMonitorGate,
    tracks: Vec<(TrackId, bool, Arc<AtomicBool>)>,
}

impl AudioInputMonitorController {
    /// Enables input monitoring for an armed track. Unarmed tracks cannot be monitored.
    pub fn set_track_enabled(&self, track_id: TrackId, enabled: bool) -> bool {
        let Some((_, armed, state)) = self.tracks.iter().find(|(id, _, _)| *id == track_id) else {
            return false;
        };
        if enabled && !armed {
            return false;
        }
        state.store(enabled, Ordering::Release);
        let any_enabled = self
            .tracks
            .iter()
            .any(|(_, _, state)| state.load(Ordering::Acquire));
        if any_enabled != self.gate.is_enabled() {
            self.gate.set_enabled(any_enabled);
        }
        true
    }

    /// Disables every monitor route and stops the input callback's monitor fanout.
    pub fn disable_all(&self) {
        for (_, _, state) in &self.tracks {
            state.store(false, Ordering::Release);
        }
        self.gate.set_enabled(false);
    }

    pub fn is_track_enabled(&self, track_id: TrackId) -> bool {
        self.tracks
            .iter()
            .find(|(id, _, _)| *id == track_id)
            .is_some_and(|(_, _, state)| state.load(Ordering::Acquire))
    }

    pub fn has_armed_track(&self, track_id: TrackId) -> bool {
        self.tracks
            .iter()
            .any(|(id, armed, _)| *id == track_id && *armed)
    }

    /// Returns the armed routes currently enabled for the live input tap.
    pub fn enabled_tracks(&self) -> Vec<TrackId> {
        self.tracks
            .iter()
            .filter_map(|(track_id, armed, state)| {
                (*armed && state.load(Ordering::Acquire)).then_some(*track_id)
            })
            .collect()
    }
}

impl TrackMixController {
    /// Retargets one track's volume and pan with a sample ramp and no graph rebuild.
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

    /// Updates one track's mute and solo state without rebuilding the render graph.
    pub fn set_track_mute_solo(&self, track_id: TrackId, muted: bool, solo: bool) -> bool {
        let Some((_, live)) = self.tracks.iter().find(|(id, _)| *id == track_id) else {
            return false;
        };

        live.muted.store(muted, Ordering::Release);
        live.solo.store(solo, Ordering::Release);
        self.has_solo.store(
            self.tracks
                .iter()
                .any(|(_, live)| live.solo.load(Ordering::Acquire)),
            Ordering::Release,
        );
        true
    }
}

impl LiveTrackGains {
    fn new(gains: GainCoefficients, muted: bool, solo: bool) -> Self {
        Self {
            version: AtomicU32::new(0),
            left: AtomicU32::new(gains.left.to_bits()),
            right: AtomicU32::new(gains.right.to_bits()),
            stereo_left: AtomicU32::new(gains.stereo_left.to_bits()),
            stereo_right: AtomicU32::new(gains.stereo_right.to_bits()),
            muted: AtomicBool::new(muted),
            solo: AtomicBool::new(solo),
        }
    }

    fn snapshot(&self) -> Option<GainCoefficients> {
        let before = self.version.load(Ordering::SeqCst);
        if before % 2 != 0 {
            return None;
        }
        let gains = GainCoefficients {
            left: f32::from_bits(self.left.load(Ordering::SeqCst)),
            right: f32::from_bits(self.right.load(Ordering::SeqCst)),
            stereo_left: f32::from_bits(self.stereo_left.load(Ordering::SeqCst)),
            stereo_right: f32::from_bits(self.stereo_right.load(Ordering::SeqCst)),
        };
        if self.version.load(Ordering::SeqCst) == before {
            Some(gains)
        } else {
            None
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

/// Returns envelope gain, the per-sample geometric step for the current linear-dB segment, and
/// the next boundary that requires recalculating that segment.
fn volume_automation_state(
    points: &[VolumeAutomationPoint],
    sample: u64,
) -> (f32, f32, Option<u64>) {
    if points.is_empty() {
        return (1.0, 1.0, None);
    }
    let next_index = points.partition_point(|point| point.sample() <= sample);
    if next_index == 0 {
        let gain = 10.0_f64.powf(f64::from(points[0].gain_db()) / 20.0) as f32;
        return (gain, 1.0, Some(points[0].sample()));
    }
    let previous = points[next_index - 1];
    if next_index == points.len() {
        let gain = 10.0_f64.powf(f64::from(previous.gain_db()) / 20.0) as f32;
        return (gain, 1.0, None);
    }

    let next = points[next_index];
    let span = next.sample() - previous.sample();
    let offset = sample - previous.sample();
    let delta_db = f64::from(next.gain_db() - previous.gain_db());
    let current_db = f64::from(previous.gain_db()) + delta_db * (offset as f64 / span as f64);
    let gain = 10.0_f64.powf(current_db / 20.0) as f32;
    let gain_step = 10.0_f64.powf(delta_db / (span as f64 * 20.0)) as f32;
    (gain, gain_step, Some(next.sample()))
}

/// The mix plan could not be compiled from project track controls.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MixerPlanError {
    /// The maximum callback block size must be positive.
    ZeroBlockCapacity,
    /// The project sample rate must be positive.
    ZeroSampleRate,
    /// The track's volume cannot be represented as an `f32` gain.
    InvalidTrackGain { track_id: u64 },
}

impl fmt::Display for MixerPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroBlockCapacity => formatter.write_str("maximum block size must be positive"),
            Self::ZeroSampleRate => formatter.write_str("sample rate must be positive"),
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
    /// Compiles the project's current track controls for a maximum callback block size.
    /// The live-control ramp uses a 48 kHz sample rate; use
    /// [`compile_with_sample_rate`](Self::compile_with_sample_rate) for another rate.
    /// Call this on a control thread, not in the audio callback.
    pub fn compile(tracks: &[Track], max_block_frames: usize) -> Result<Self, MixerPlanError> {
        Self::compile_with_sample_rate(tracks, max_block_frames, 48_000)
    }

    /// Compiles a mixer with a live-control ramp duration of five milliseconds at `sample_rate`.
    pub fn compile_with_sample_rate(
        tracks: &[Track],
        max_block_frames: usize,
        sample_rate: u32,
    ) -> Result<Self, MixerPlanError> {
        if max_block_frames == 0 {
            return Err(MixerPlanError::ZeroBlockCapacity);
        }
        if sample_rate == 0 {
            return Err(MixerPlanError::ZeroSampleRate);
        }
        let ramp_frames = (sample_rate / 200).max(1) as usize;

        let has_solo = Arc::new(AtomicBool::new(tracks.iter().any(Track::is_solo)));
        let mut compiled = Vec::with_capacity(tracks.len());
        for track in tracks {
            let Some(gains) = gain_coefficients(track.volume_db(), track.pan()) else {
                return Err(MixerPlanError::InvalidTrackGain {
                    track_id: track.id().value(),
                });
            };
            compiled.push(TrackGains {
                track_id: track.id(),
                output_track_index: track.output_track().and_then(|output| {
                    tracks.iter().position(|candidate| candidate.id() == output)
                }),
                is_bus: track.is_bus(),
                live: Arc::new(LiveTrackGains::new(
                    gains,
                    track.is_muted(),
                    track.is_solo(),
                )),
                mix_ramp: Cell::new(GainRamp::new(gains, ramp_frames)),
                record_armed: track.is_record_armed(),
                volume_automation: track.volume_automation().to_vec(),
            });
        }

        let mut routing_order: Vec<usize> = (0..compiled.len()).collect();
        let route_depth = |mut index: usize| {
            let mut depth = 0;
            while let Some(output) = compiled[index].output_track_index {
                depth += 1;
                index = output;
            }
            depth
        };
        routing_order.sort_by_key(|index| (std::cmp::Reverse(route_depth(*index)), *index));

        Ok(Self {
            max_block_frames,
            tracks: compiled,
            routing_order,
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
            has_solo: Arc::clone(&self.has_solo),
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
            self.mix_track_unchecked(track_index, input, output, 0, true);
        }
        Ok(())
    }

    fn mix_track_unchecked(
        &self,
        track_index: usize,
        input: &[f32],
        output: &mut [[f32; 2]],
        start_sample: u64,
        advances_timeline: bool,
    ) {
        let track = &self.tracks[track_index];
        if track.live.muted.load(Ordering::Acquire)
            || (self.has_solo.load(Ordering::Acquire) && !track.live.solo.load(Ordering::Acquire))
        {
            return;
        }
        let mut ramp = track.mix_ramp.get();
        if let Some(target) = track.live.snapshot() {
            ramp.retarget(target);
        }
        let (mut automation_gain, mut gain_step, mut next_point_sample) =
            volume_automation_state(&track.volume_automation, start_sample);
        let ramp_frames = ramp.remaining_frames.min(output.len());
        for (offset, (frame, sample)) in output
            .iter_mut()
            .zip(input.iter().copied())
            .take(ramp_frames)
            .enumerate()
        {
            let gains = ramp.next_frame();
            if advances_timeline
                && next_point_sample.is_some_and(|next_sample| {
                    start_sample.saturating_add(offset as u64) >= next_sample
                })
            {
                (automation_gain, gain_step, next_point_sample) = volume_automation_state(
                    &track.volume_automation,
                    start_sample.saturating_add(offset as u64),
                );
            }
            frame[0] += sample * gains.left * automation_gain;
            frame[1] += sample * gains.right * automation_gain;
            if advances_timeline {
                automation_gain *= gain_step;
            }
        }
        if ramp_frames < output.len() {
            let gains = ramp.current;
            for (offset, (frame, sample)) in output
                .iter_mut()
                .zip(input.iter().copied())
                .skip(ramp_frames)
                .enumerate()
            {
                let offset = offset + ramp_frames;
                if advances_timeline
                    && next_point_sample.is_some_and(|next_sample| {
                        start_sample.saturating_add(offset as u64) >= next_sample
                    })
                {
                    (automation_gain, gain_step, next_point_sample) = volume_automation_state(
                        &track.volume_automation,
                        start_sample.saturating_add(offset as u64),
                    );
                }
                frame[0] += sample * gains.left * automation_gain;
                frame[1] += sample * gains.right * automation_gain;
                if advances_timeline {
                    automation_gain *= gain_step;
                }
            }
        }
        track.mix_ramp.set(ramp);
    }

    fn mix_stereo_routed_unchecked(
        &self,
        track_index: usize,
        input: &[[f32; 2]],
        output: &mut [[f32; 2]],
        block: MixBlock,
        solo_allowed: bool,
        mono_input: bool,
    ) {
        let track = &self.tracks[track_index];
        if track.live.muted.load(Ordering::Acquire)
            || (self.has_solo.load(Ordering::Acquire) && !solo_allowed)
        {
            return;
        }
        let mut ramp = track.mix_ramp.get();
        if let Some(target) = track.live.snapshot() {
            ramp.retarget(target);
        }
        let (mut automation_gain, mut gain_step, mut next_point_sample) =
            volume_automation_state(&track.volume_automation, block.start_sample);
        let ramp_frames = ramp.remaining_frames.min(output.len());
        for (offset, (frame, sample)) in output
            .iter_mut()
            .zip(input.iter())
            .take(ramp_frames)
            .enumerate()
        {
            let gains = ramp.next_frame();
            if block.advances_timeline
                && next_point_sample.is_some_and(|next_sample| {
                    block.start_sample.saturating_add(offset as u64) >= next_sample
                })
            {
                (automation_gain, gain_step, next_point_sample) = volume_automation_state(
                    &track.volume_automation,
                    block.start_sample.saturating_add(offset as u64),
                );
            }
            let (left_gain, right_gain) = if mono_input {
                (gains.left, gains.right)
            } else {
                (gains.stereo_left, gains.stereo_right)
            };
            frame[0] += sample[0] * left_gain * automation_gain;
            frame[1] += sample[1] * right_gain * automation_gain;
            if block.advances_timeline {
                automation_gain *= gain_step;
            }
        }
        if ramp_frames < output.len() {
            let gains = ramp.current;
            let (left_gain, right_gain) = if mono_input {
                (gains.left, gains.right)
            } else {
                (gains.stereo_left, gains.stereo_right)
            };
            for (offset, (frame, sample)) in output
                .iter_mut()
                .zip(input.iter())
                .skip(ramp_frames)
                .enumerate()
            {
                let offset = offset + ramp_frames;
                if block.advances_timeline
                    && next_point_sample.is_some_and(|next_sample| {
                        block.start_sample.saturating_add(offset as u64) >= next_sample
                    })
                {
                    (automation_gain, gain_step, next_point_sample) = volume_automation_state(
                        &track.volume_automation,
                        block.start_sample.saturating_add(offset as u64),
                    );
                }
                frame[0] += sample[0] * left_gain * automation_gain;
                frame[1] += sample[1] * right_gain * automation_gain;
                if block.advances_timeline {
                    automation_gain *= gain_step;
                }
            }
        }
        track.mix_ramp.set(ramp);
    }

    fn compile_solo_audibility(&self, audible: &mut [bool], solo_bus_subtrees: &mut [bool]) {
        if !self.has_solo.load(Ordering::Acquire) {
            audible.fill(true);
            return;
        }
        audible.fill(false);
        solo_bus_subtrees.fill(false);

        // Walk from Master toward source tracks so a soloed bus includes every
        // track below it without searching the full graph for each track.
        for track_index in self.routing_order.iter().rev().copied() {
            let track = &self.tracks[track_index];
            solo_bus_subtrees[track_index] = (track.is_bus
                && track.live.solo.load(Ordering::Acquire))
                || track
                    .output_track_index
                    .is_some_and(|parent| solo_bus_subtrees[parent]);
            audible[track_index] = solo_bus_subtrees[track_index];
        }

        // A soloed source and each bus above it must remain open on the path to Master.
        for (track_index, track) in self.tracks.iter().enumerate() {
            if !track.live.solo.load(Ordering::Acquire) {
                continue;
            }
            let mut path = Some(track_index);
            while let Some(index) = path {
                audible[index] = true;
                path = self.tracks[index].output_track_index;
            }
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
    consumer: AudioItemPcmConsumer,
    source_start_sample: Option<u64>,
}

enum AudioItemPcmConsumer {
    Mono(PcmStreamConsumer),
    Stereo(StereoPcmStreamConsumer),
}

impl AudioItemPcmConsumer {
    fn is_stereo(&self) -> bool {
        match self {
            Self::Mono(_) => false,
            Self::Stereo(consumer) => consumer.is_stereo_content(),
        }
    }

    fn available_frames(&self) -> usize {
        match self {
            Self::Mono(consumer) => consumer.available_samples(),
            Self::Stereo(consumer) => consumer.available_frames(),
        }
    }

    fn read_into_stereo(&mut self, output: &mut [[f32; 2]]) -> usize {
        match self {
            Self::Mono(consumer) => consumer.read_stereo_into(output),
            Self::Stereo(consumer) => consumer
                .read_into(output)
                .saturating_mul(if consumer.is_stereo_content() { 2 } else { 1 }),
        }
    }
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

#[derive(Clone, Copy)]
struct TrackRoute {
    source_index: usize,
    destination_index: usize,
}

fn mix_track_buffer_to_bus(
    buffers: &mut TrackEffectBuffers,
    route: TrackRoute,
    mixer: &MixerPlan,
    block: MixBlock,
    solo_allowed: bool,
    mono_input: bool,
) {
    if route.source_index < route.destination_index {
        let (before_destination, destination_and_after) =
            buffers.split_at_mut(route.destination_index);
        let source = before_destination[route.source_index]
            .as_ref()
            .expect("every track has a preallocated routing buffer");
        let destination = destination_and_after[0]
            .as_mut()
            .expect("every bus has a preallocated routing buffer");
        mixer.mix_stereo_routed_unchecked(
            route.source_index,
            &source[..block.frame_count],
            &mut destination[..block.frame_count],
            block,
            solo_allowed,
            mono_input,
        );
    } else {
        let (before_source, source_and_after) = buffers.split_at_mut(route.source_index);
        let destination = before_source[route.destination_index]
            .as_mut()
            .expect("every bus has a preallocated routing buffer");
        let source = source_and_after[0]
            .as_ref()
            .expect("every track has a preallocated routing buffer");
        mixer.mix_stereo_routed_unchecked(
            route.source_index,
            &source[..block.frame_count],
            &mut destination[..block.frame_count],
            block,
            solo_allowed,
            mono_input,
        );
    }
}

struct RenderGraphSources {
    streams: Vec<AudioItemPcmConsumer>,
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

    // Every track needs an accumulator so incoming tracks can be summed into a
    // bus before that bus's own inserts and fader are applied.
    let track_effect_buffers = (0..project.tracks().len())
        .map(|_| Some(vec![[0.0; 2]; max_block_frames]))
        .collect();
    Ok((routes, track_effect_buffers))
}

impl AudioItemStream {
    /// Associates a worker-fed PCM consumer with its timeline item.
    pub fn new(item_id: ItemId, consumer: PcmStreamConsumer) -> Self {
        Self {
            item_id,
            consumer: AudioItemPcmConsumer::Mono(consumer),
            source_start_sample: None,
        }
    }

    /// Associates a stereo worker-fed PCM consumer with its timeline item.
    pub fn new_stereo(item_id: ItemId, consumer: StereoPcmStreamConsumer) -> Self {
        Self {
            item_id,
            consumer: AudioItemPcmConsumer::Stereo(consumer),
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
            consumer: AudioItemPcmConsumer::Mono(consumer),
            source_start_sample: Some(source_start_sample),
        }
    }

    /// Associates a refilled stereo stream whose first frame corresponds to the given sample.
    pub fn new_stereo_at_sample(
        item_id: ItemId,
        source_start_sample: u64,
        consumer: StereoPcmStreamConsumer,
    ) -> Self {
        Self {
            item_id,
            consumer: AudioItemPcmConsumer::Stereo(consumer),
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
    track_has_stereo_input: Vec<bool>,
    solo_audible_tracks: Vec<bool>,
    solo_bus_subtrees: Vec<bool>,
    sample_rate: u32,
    transport: Transport,
    last_midi_sample_end: Option<u64>,
    last_midi_chase_generation: Option<u64>,
    streams: Vec<AudioItemPcmConsumer>,
    stream_track_indices: Vec<usize>,
    source_ranges: Vec<Option<(u64, u64)>>,
    source_cursors: Vec<Option<u64>>,
    source_item_ids: Vec<Option<ItemId>>,
    scratch: Vec<Vec<[f32; 2]>>,
    input_monitor: Option<AudioMonitorConsumer>,
    input_monitor_gate: Option<AudioInputMonitorGate>,
    input_monitor_scratch: Vec<[f32; 2]>,
    input_monitor_states: Vec<(TrackId, usize, Arc<AtomicBool>)>,
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
            streams: streams
                .into_iter()
                .map(AudioItemPcmConsumer::Mono)
                .collect(),
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
        let mixer = MixerPlan::compile_with_sample_rate(
            project.tracks(),
            max_block_frames,
            project.settings().sample_rate(),
        )
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
            if midi_plan.midi_control_event_count_for_track(instrument.track_id) > 0
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
        let (mut effect_routes, track_effect_buffers) =
            compile_fx_routes(project, effect_processors, &[], max_block_frames)?;
        effect_routes.sort_by_key(|route| {
            (
                mixer
                    .routing_order
                    .iter()
                    .position(|track_index| *track_index == route.track_index)
                    .unwrap_or(usize::MAX),
                route.chain_index,
            )
        });
        let scratch = (0..sources.streams.len())
            .map(|_| vec![[0.0, 0.0]; max_block_frames])
            .collect();
        let midi_scratch = vec![None; midi_plan.len()];
        let track_has_stereo_input = vec![false; project.tracks().len()];
        let solo_audible_tracks = vec![false; project.tracks().len()];
        let solo_bus_subtrees = vec![false; project.tracks().len()];
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
            track_has_stereo_input,
            solo_audible_tracks,
            solo_bus_subtrees,
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
            input_monitor: None,
            input_monitor_gate: None,
            input_monitor_scratch: vec![[0.0, 0.0]; max_block_frames],
            input_monitor_states: Vec::new(),
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

    /// Returns an AudioItem whose PCM queue cannot yet supply the next render block.
    ///
    /// This control-thread readiness query is intended for pull-based offline rendering. Device
    /// callbacks must continue calling `render_into` directly and rely on its silence-on-underrun
    /// behavior. `None` means all active AudioItem ranges have enough queued frames.
    pub fn audio_item_missing_for_next_block(&self, frame_count: usize) -> Option<ItemId> {
        let start = self.transport.position_samples();
        let end = start.checked_add(u64::try_from(frame_count).ok()?)?;
        for (index, range) in self.source_ranges.iter().enumerate() {
            let Some((item_start, item_end)) = range else {
                continue;
            };
            let required = end.min(*item_end).saturating_sub(start.max(*item_start));
            if required > 0
                && self.streams[index].available_frames() < usize::try_from(required).ok()?
            {
                return self.source_item_ids[index];
            }
        }
        None
    }

    /// Returns the largest prefix of the next render block available from every active AudioItem.
    ///
    /// The offline control thread uses this to consume bounded queues even when their capacity is
    /// smaller than the graph's maximum block size. Device callbacks should continue rendering
    /// their negotiated block directly and handle underruns through `render_into`.
    pub fn audio_item_frames_available_for_next_block(&self, max_frames: usize) -> usize {
        let start = self.transport.position_samples();
        let Some(end) = start.checked_add(max_frames as u64) else {
            return 0;
        };
        let mut available_frames = max_frames;
        for (index, range) in self.source_ranges.iter().enumerate() {
            let Some((item_start, item_end)) = range else {
                continue;
            };
            let active_start = start.max(*item_start);
            let active_end = end.min(*item_end);
            if active_start >= active_end {
                continue;
            }
            let queued = self.streams[index].available_frames() as u64;
            let safe_until = active_start
                .saturating_sub(start)
                .saturating_add(queued.min(active_end.saturating_sub(active_start)));
            available_frames = available_frames.min(safe_until as usize);
        }
        available_frames
    }

    /// Returns a control-thread handle for live volume/pan updates on this graph.
    pub fn track_mix_controller(&self) -> TrackMixController {
        self.mixer.track_mix_controller()
    }

    /// Adds the optional live-input tap before the graph is handed to an output callback.
    pub fn install_input_monitor(
        &mut self,
        consumer: AudioMonitorConsumer,
        gate: AudioInputMonitorGate,
    ) -> AudioInputMonitorController {
        self.input_monitor_states.clear();
        let tracks = self
            .mixer
            .tracks
            .iter()
            .enumerate()
            .map(|(index, track)| {
                let state = Arc::new(AtomicBool::new(false));
                self.input_monitor_states
                    .push((track.track_id, index, Arc::clone(&state)));
                (track.track_id, track.record_armed, state)
            })
            .collect();
        self.input_monitor = Some(consumer);
        self.input_monitor_gate = Some(gate.clone());
        AudioInputMonitorController { gate, tracks }
    }

    /// Returns the live-input monitoring controller when this graph has a monitor queue.
    pub fn input_monitor_controller(&self) -> Option<AudioInputMonitorController> {
        let gate = self.input_monitor_gate.clone()?;
        let tracks = self
            .mixer
            .tracks
            .iter()
            .filter_map(|track| {
                self.input_monitor_states
                    .iter()
                    .find(|(track_id, _, _)| *track_id == track.track_id)
                    .map(|(_, _, state)| (track.track_id, track.record_armed, Arc::clone(state)))
            })
            .collect();
        Some(AudioInputMonitorController { gate, tracks })
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
                .midi_control_event_count_for_track(instrument.track_id)
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
        self.effects.sort_by_key(|route| {
            (
                self.mixer
                    .routing_order
                    .iter()
                    .position(|track_index| *track_index == route.track_index)
                    .unwrap_or(usize::MAX),
                route.chain_index,
            )
        });
        self.track_effect_buffers = buffers;
        Ok(())
    }

    /// Releases held MIDI voices and resets controller-capable instruments without
    /// changing the transport state.
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

    /// Stops processors before their graph moves to the retirement queue.
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

    /// Stops track FX processors before graph retirement.
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

    /// Stops CLAP processors after an offline render so their owners can deactivate them safely.
    ///
    /// Call on the same background thread that rendered this graph. Device output callbacks use
    /// their backend-specific retirement path instead.
    pub fn stop_processors_after_offline_render(&mut self) -> usize {
        self.release_midi_notes() + self.stop_instruments() + self.stop_fx_processors()
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
            let (chase_state_count, chase_note_count) = if self.last_midi_sample_end
                != Some(block_start_sample)
                || self.last_midi_chase_generation != Some(chase_generation)
            {
                let controller_count = self
                    .midi_plan
                    .active_controllers_at(block_start_sample, &mut self.midi_scratch)
                    .map_err(AudioGraphError::MidiSchedule)?;
                let pitch_bend_count = self
                    .midi_plan
                    .active_pitch_bends_at(
                        block_start_sample,
                        &mut self.midi_scratch[controller_count..],
                    )
                    .map_err(AudioGraphError::MidiSchedule)?;
                let note_count = self
                    .midi_plan
                    .active_notes_at(
                        block_start_sample,
                        &mut self.midi_scratch[controller_count + pitch_bend_count..],
                    )
                    .map_err(AudioGraphError::MidiSchedule)?;
                (controller_count + pitch_bend_count, note_count)
            } else {
                (0, 0)
            };
            let chase_count = chase_state_count + chase_note_count;
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
        let monitor_active = self
            .input_monitor_gate
            .as_ref()
            .is_some_and(AudioInputMonitorGate::is_enabled);
        if !block.is_playing && !monitor_active {
            if let Some(monitor) = &mut self.input_monitor {
                let _ = monitor.read_into(&mut self.input_monitor_scratch[..output.len()]);
            }
            for (_, _, state) in &self.input_monitor_states {
                state.store(false, Ordering::Release);
            }
            if let Some(gate) = &self.input_monitor_gate {
                gate.set_enabled(false);
            }
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
        self.track_has_stereo_input.fill(false);
        for (track_index, track) in self.mixer.tracks.iter().enumerate() {
            if track.is_bus {
                self.track_has_stereo_input[track_index] = true;
            }
        }
        let block_end_sample = if block.is_playing {
            let block_frame_count = u64::try_from(block.frame_count)
                .map_err(|_| AudioGraphError::TransportPositionOverflow)?;
            block
                .start_sample
                .checked_add(block_frame_count)
                .ok_or(AudioGraphError::TransportPositionOverflow)?
        } else {
            block.start_sample
        };
        let mut underrun_samples = 0_usize;
        for stream_index in 0..if block.is_playing {
            self.streams.len()
        } else {
            0
        } {
            let input = &mut self.scratch[stream_index][..output.len()];
            input.fill([0.0, 0.0]);
            if let Some((item_start, item_end)) = self.source_ranges[stream_index] {
                let overlap_start = block.start_sample.max(item_start);
                let overlap_end = block_end_sample.min(item_end);
                if overlap_start < overlap_end {
                    let offset = (overlap_start - block.start_sample) as usize;
                    let length = (overlap_end - overlap_start) as usize;
                    let underruns = self.streams[stream_index]
                        .read_into_stereo(&mut input[offset..offset + length]);
                    underrun_samples = underrun_samples.saturating_add(underruns);
                    self.source_cursors[stream_index] = Some(overlap_end);
                }
            } else {
                underrun_samples = underrun_samples
                    .saturating_add(self.streams[stream_index].read_into_stereo(input));
            }
            let track_index = self.stream_track_indices[stream_index];
            self.track_has_stereo_input[track_index] |= self.streams[stream_index].is_stereo();
            let track_buffer = self.track_effect_buffers[track_index]
                .as_mut()
                .expect("every track has a preallocated routing buffer");
            for (frame, sample) in track_buffer[..output.len()]
                .iter_mut()
                .zip(input.iter().copied())
            {
                frame[0] += sample[0];
                frame[1] += sample[1];
            }
        }
        if let Some(monitor) = &mut self.input_monitor {
            let input = &mut self.input_monitor_scratch[..output.len()];
            let _ = monitor.read_into(input);
            for (_track_id, track_index, enabled) in &self.input_monitor_states {
                if !enabled.load(Ordering::Acquire) {
                    continue;
                }
                self.track_has_stereo_input[*track_index] = true;
                let track_buffer = self.track_effect_buffers[*track_index]
                    .as_mut()
                    .expect("every track has a preallocated routing buffer");
                for (frame, sample) in track_buffer[..output.len()]
                    .iter_mut()
                    .zip(input.iter().copied())
                {
                    frame[0] += sample[0];
                    frame[1] += sample[1];
                }
            }
        }
        let scheduled_events = self.midi_scratch.iter().take(midi_event_count);
        for route in self.instruments.iter_mut().filter(|_| block.is_playing) {
            self.track_has_stereo_input[route.track_index] = true;
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
            let track_buffer = self.track_effect_buffers[route.track_index]
                .as_mut()
                .expect("every track has a preallocated routing buffer");
            for (frame, sample) in track_buffer[..output.len()]
                .iter_mut()
                .zip(route.audio[..output.len()].iter())
            {
                frame[0] += sample[0];
                frame[1] += sample[1];
            }
        }
        let mut next_effect = 0;
        let mix_block = MixBlock {
            start_sample: block.start_sample,
            advances_timeline: block.is_playing,
            frame_count: output.len(),
        };
        self.mixer
            .compile_solo_audibility(&mut self.solo_audible_tracks, &mut self.solo_bus_subtrees);
        for track_index in self.mixer.routing_order.iter().copied() {
            while self
                .effects
                .get(next_effect)
                .is_some_and(|route| route.track_index == track_index)
            {
                let route = &mut self.effects[next_effect];
                let track_buffer = self.track_effect_buffers[track_index]
                    .as_mut()
                    .expect("every track has a preallocated routing buffer");
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
                next_effect += 1;
            }
            let source = self.track_effect_buffers[track_index]
                .as_ref()
                .expect("every track has a preallocated routing buffer");
            let solo_allowed = self.solo_audible_tracks[track_index];
            if let Some(destination) = self.mixer.tracks[track_index].output_track_index {
                mix_track_buffer_to_bus(
                    &mut self.track_effect_buffers,
                    TrackRoute {
                        source_index: track_index,
                        destination_index: destination,
                    },
                    &self.mixer,
                    mix_block,
                    solo_allowed,
                    !self.track_has_stereo_input[track_index],
                );
            } else {
                self.mixer.mix_stereo_routed_unchecked(
                    track_index,
                    &source[..output.len()],
                    output,
                    mix_block,
                    solo_allowed,
                    !self.track_has_stereo_input[track_index],
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
