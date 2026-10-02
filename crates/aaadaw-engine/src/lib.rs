//! Realtime-oriented audio processing primitives.
//!
//! This crate provides a fixed-topology, allocation-free streaming mixer,
//! transport, MIDI scheduling primitives, and an optional Linux JACK backend.
//! Project AudioItem integration remains a follow-up work item.

#[cfg(feature = "jack-backend")]
mod jack_output;
mod midi;
mod pcm;
#[cfg(feature = "pipewire-backend")]
mod pipewire_output;
mod stream;
mod transport;

#[cfg(feature = "jack-backend")]
pub use jack_output::{JackAudioOutput, JackOutputError, JackOutputStats};
pub use midi::{MidiEventKind, MidiEventPlan, MidiScheduleError, ScheduledMidiEvent};
pub use pcm::{MonoPcmClip, MonoPcmPlayer, PcmError};
#[cfg(feature = "pipewire-backend")]
pub use pipewire_output::{PipeWireAudioOutput, PipeWireOutputError, PipeWireOutputStats};
pub use stream::{PcmStreamConsumer, PcmStreamError, PcmStreamProducer, pcm_stream};
pub use transport::{AudioBlock, Transport, TransportPositionOverflow};

use aaadaw_core::{ItemId, Track};
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
    AudioItemStreamCountMismatch { items: usize, streams: usize },
    AudioItemStreamOrderMismatch { expected: ItemId, found: ItemId },
    AudioItemStreamStartOutOfRange { item_id: ItemId, start_sample: u64 },
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
            | Self::AudioItemStreamStartOutOfRange { .. } => None,
        }
    }
}

/// A streaming render callback failed before it could render a block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioGraphError {
    BlockTooLarge { requested: usize, maximum: usize },
    MidiSchedule(MidiScheduleError),
    AudioItemSeekRequiresRefill { item_id: ItemId },
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
}

/// One SPSC consumer associated with a project AudioItem.
pub struct AudioItemStream {
    item_id: ItemId,
    consumer: PcmStreamConsumer,
    source_start_sample: Option<u64>,
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
    midi_plan: MidiEventPlan,
    sample_rate: u32,
    transport: Transport,
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
        let stream_track_indices = (0..streams.len()).collect();
        let source_ranges = vec![None; streams.len()];
        let source_cursors = vec![None; streams.len()];
        let source_item_ids = vec![None; streams.len()];
        Self::build(
            project,
            streams,
            stream_track_indices,
            source_ranges,
            source_cursors,
            source_item_ids,
            max_block_frames,
        )
    }

    /// Compiles one PCM consumer per AudioItem, preserving and validating project item order.
    pub fn new_for_audio_items(
        project: &aaadaw_core::Project,
        item_streams: Vec<AudioItemStream>,
        max_block_frames: usize,
    ) -> Result<Self, AudioGraphBuildError> {
        if item_streams.len() != project.audio_items().len() {
            return Err(AudioGraphBuildError::AudioItemStreamCountMismatch {
                items: project.audio_items().len(),
                streams: item_streams.len(),
            });
        }

        let mut streams = Vec::with_capacity(item_streams.len());
        let mut stream_track_indices = Vec::with_capacity(item_streams.len());
        let mut source_ranges = Vec::with_capacity(item_streams.len());
        let mut source_cursors = Vec::with_capacity(item_streams.len());
        let mut source_item_ids = Vec::with_capacity(item_streams.len());
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
            streams.push(stream.consumer);
            stream_track_indices.push(track_index);
            source_ranges.push(Some((item.start_sample(), item.end_sample())));
            source_cursors.push(Some(source_start_sample));
            source_item_ids.push(Some(item.id()));
        }

        Self::build(
            project,
            streams,
            stream_track_indices,
            source_ranges,
            source_cursors,
            source_item_ids,
            max_block_frames,
        )
    }

    fn build(
        project: &aaadaw_core::Project,
        streams: Vec<PcmStreamConsumer>,
        stream_track_indices: Vec<usize>,
        source_ranges: Vec<Option<(u64, u64)>>,
        source_cursors: Vec<Option<u64>>,
        source_item_ids: Vec<Option<ItemId>>,
        max_block_frames: usize,
    ) -> Result<Self, AudioGraphBuildError> {
        let mixer = MixerPlan::compile(project.tracks(), max_block_frames)
            .map_err(AudioGraphBuildError::MixerPlan)?;
        let midi_plan =
            MidiEventPlan::compile(project).map_err(AudioGraphBuildError::MidiSchedule)?;
        let scratch = (0..streams.len())
            .map(|_| vec![0.0; max_block_frames])
            .collect();
        Ok(Self {
            mixer,
            midi_plan,
            sample_rate: project.settings().sample_rate(),
            transport: Transport::new(),
            streams,
            stream_track_indices,
            source_ranges,
            source_cursors,
            source_item_ids,
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

        let midi_event_count = if self.transport.is_playing() && include_midi {
            self.midi_plan
                .events_for_block(block_start_sample, output.len(), midi_output)
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
            self.mixer
                .mix_track_unchecked(self.stream_track_indices[stream_index], input, output);
        }
        Ok(AudioRenderStats {
            block,
            underrun_samples,
            midi_event_count,
        })
    }
}
