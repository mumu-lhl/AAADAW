use aaadaw_core::{Project, TimebaseError};
use std::fmt;

/// Pairs a backend frame position with the project transport sample observed at the same instant.
/// The backend frame may be a wrapping counter; its meaning is defined by the backend adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransportClockAnchor {
    pub backend_frame: u32,
    pub project_sample: u64,
}

/// A block's position and transport state as observed by one engine callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioBlock {
    pub start_sample: u64,
    pub frame_count: usize,
    pub is_playing: bool,
}

/// A half-open project-sample range used by transport repeat.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransportLoopRange {
    start_sample: u64,
    end_sample: u64,
}

impl TransportLoopRange {
    /// Creates a loop range when `start_sample < end_sample`.
    pub fn new(start_sample: u64, end_sample: u64) -> Option<Self> {
        (start_sample < end_sample).then_some(Self {
            start_sample,
            end_sample,
        })
    }

    pub fn start_sample(self) -> u64 {
        self.start_sample
    }

    pub fn end_sample(self) -> u64 {
        self.end_sample
    }

    pub fn length_samples(self) -> u64 {
        self.end_sample - self.start_sample
    }
}

/// One contiguous project-time portion of a device callback's output buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransportBlockSegment {
    /// Offset into the callback output buffer.
    pub output_offset: usize,
    /// Absolute project sample at which this segment begins.
    pub start_sample: u64,
    /// Number of frames before the next loop boundary or callback end.
    pub frame_count: usize,
    /// MIDI chase generation at the start of this segment.
    pub chase_generation: u64,
}

/// Why a repeating transport block could not be advanced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportBlockError {
    PositionOverflow,
    SegmentBufferTooSmall { required: usize, available: usize },
}

/// Whether transport frame arithmetic exceeded its supported range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransportPositionOverflow;

impl fmt::Display for TransportPositionOverflow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("transport position exceeds the supported sample range")
    }
}

impl std::error::Error for TransportPositionOverflow {}

/// Sample-position transport state. Once audio starts, mutate this object only
/// from the engine thread; control-thread requests should be sent as commands.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Transport {
    position_samples: u64,
    is_playing: bool,
    chase_generation: u64,
    loop_range: Option<TransportLoopRange>,
}

impl Transport {
    /// Creates a stopped transport positioned at sample zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the current absolute sample position.
    pub fn position_samples(&self) -> u64 {
        self.position_samples
    }

    /// Returns whether playback is active.
    pub fn is_playing(&self) -> bool {
        self.is_playing
    }

    /// Returns the generation used to detect playback starts and playhead jumps.
    pub(crate) fn chase_generation(&self) -> u64 {
        self.chase_generation
    }

    /// Returns the configured repeat range, if any.
    pub fn loop_range(&self) -> Option<TransportLoopRange> {
        self.loop_range
    }

    /// Sets or clears the repeat range. Changing it invalidates MIDI chase state.
    pub fn set_loop_range(&mut self, range: Option<TransportLoopRange>) {
        if self.loop_range != range {
            self.loop_range = range;
            self.chase_generation = self.chase_generation.wrapping_add(1);
        }
    }

    /// Starts or resumes playback from the current position.
    pub fn start(&mut self) {
        if !self.is_playing {
            self.chase_generation = self.chase_generation.wrapping_add(1);
        }
        self.is_playing = true;
    }

    /// Stops playback without resetting the playhead.
    pub fn stop(&mut self) {
        if self.is_playing {
            self.chase_generation = self.chase_generation.wrapping_add(1);
        }
        self.is_playing = false;
    }

    /// Moves the playhead to an absolute sample position.
    pub fn seek_sample(&mut self, position_samples: u64) {
        if self.position_samples != position_samples {
            self.chase_generation = self.chase_generation.wrapping_add(1);
        }
        self.position_samples = position_samples;
    }

    /// Positions the renderer at one already-planned segment without generating another seek.
    pub(crate) fn set_render_segment(&mut self, position_samples: u64, chase_generation: u64) {
        self.position_samples = position_samples;
        self.chase_generation = chase_generation;
    }

    /// Seeks to a musical tick using the project's current tempo map.
    pub fn seek_tick(&mut self, project: &Project, tick: u64) -> Result<(), TimebaseError> {
        self.seek_sample(project.sample_at_tick(tick)?);
        Ok(())
    }

    /// Returns the current musical tick using the project's current tempo map.
    pub fn position_tick(&self, project: &Project) -> Result<u64, TimebaseError> {
        project.tick_at_sample(self.position_samples)
    }

    /// Reserves a callback block and advances the playhead if playing.
    ///
    /// This method performs no allocation, locking, or I/O. If sample-position
    /// arithmetic overflows, the playhead remains unchanged. Use
    /// [`Self::advance_repeating_block`] when a repeat range is configured.
    pub fn advance_block(
        &mut self,
        frame_count: usize,
    ) -> Result<AudioBlock, TransportPositionOverflow> {
        let start_sample = self.position_samples;
        if self.is_playing {
            let frame_count_u64 =
                u64::try_from(frame_count).map_err(|_| TransportPositionOverflow)?;
            self.position_samples = start_sample
                .checked_add(frame_count_u64)
                .ok_or(TransportPositionOverflow)?;
        }
        Ok(AudioBlock {
            start_sample,
            frame_count,
            is_playing: self.is_playing,
        })
    }

    /// Plans and advances a callback through one or more contiguous project-time segments.
    ///
    /// The caller provides callback-owned storage sized for the maximum number of loop wraps.
    /// This method performs no allocation, locking, or I/O. If the storage is too small or
    /// sample arithmetic would overflow, transport state remains unchanged.
    pub fn advance_repeating_block(
        &mut self,
        frame_count: usize,
        segments: &mut [TransportBlockSegment],
    ) -> Result<usize, TransportBlockError> {
        let mut cursor = self.position_samples;
        let mut remaining = if self.is_playing { frame_count } else { 0 };
        let mut output_offset = 0_usize;
        let mut chase_generation = self.chase_generation;
        let mut required = 0_usize;

        while remaining > 0 {
            let Some(range) = self.loop_range else {
                if required < segments.len() {
                    segments[required] = TransportBlockSegment {
                        output_offset,
                        start_sample: cursor,
                        frame_count: remaining,
                        chase_generation,
                    };
                }
                cursor = cursor
                    .checked_add(
                        u64::try_from(remaining)
                            .map_err(|_| TransportBlockError::PositionOverflow)?,
                    )
                    .ok_or(TransportBlockError::PositionOverflow)?;
                required = required
                    .checked_add(1)
                    .ok_or(TransportBlockError::PositionOverflow)?;
                output_offset = output_offset
                    .checked_add(remaining)
                    .ok_or(TransportBlockError::PositionOverflow)?;
                remaining = 0;
                continue;
            };

            // A callback ending exactly on the loop edge wraps before the next callback.
            if cursor == range.end_sample {
                cursor = range.start_sample;
                chase_generation = chase_generation.wrapping_add(1);
            }

            let distance_to_boundary = if cursor < range.start_sample {
                range.start_sample - cursor
            } else if cursor < range.end_sample {
                range.end_sample - cursor
            } else {
                u64::try_from(remaining).map_err(|_| TransportBlockError::PositionOverflow)?
            };
            let segment_frames =
                remaining.min(usize::try_from(distance_to_boundary).unwrap_or(usize::MAX));
            if segment_frames == 0 {
                return Err(TransportBlockError::PositionOverflow);
            }

            if required < segments.len() {
                segments[required] = TransportBlockSegment {
                    output_offset,
                    start_sample: cursor,
                    frame_count: segment_frames,
                    chase_generation,
                };
            }
            required = required
                .checked_add(1)
                .ok_or(TransportBlockError::PositionOverflow)?;
            let segment_frames_u64 =
                u64::try_from(segment_frames).map_err(|_| TransportBlockError::PositionOverflow)?;
            cursor = cursor
                .checked_add(segment_frames_u64)
                .ok_or(TransportBlockError::PositionOverflow)?;
            output_offset = output_offset
                .checked_add(segment_frames)
                .ok_or(TransportBlockError::PositionOverflow)?;
            remaining -= segment_frames;

            if cursor == range.end_sample {
                cursor = range.start_sample;
                chase_generation = chase_generation.wrapping_add(1);
            }
        }

        // A stopped callback has one stationary segment for its output span.
        if !self.is_playing && frame_count > 0 {
            required = 1;
            if !segments.is_empty() {
                segments[0] = TransportBlockSegment {
                    output_offset: 0,
                    start_sample: self.position_samples,
                    frame_count,
                    chase_generation: self.chase_generation,
                };
            }
        }

        if required > segments.len() {
            return Err(TransportBlockError::SegmentBufferTooSmall {
                required,
                available: segments.len(),
            });
        }

        if self.is_playing {
            self.position_samples = cursor;
            self.chase_generation = chase_generation;
        }
        Ok(required)
    }
}

#[cfg(test)]
mod tests {
    use super::{Transport, TransportBlockError, TransportBlockSegment, TransportLoopRange};

    fn segment_buffer<const N: usize>() -> [TransportBlockSegment; N] {
        [TransportBlockSegment {
            output_offset: 0,
            start_sample: 0,
            frame_count: 0,
            chase_generation: 0,
        }; N]
    }

    #[test]
    fn stop_pauses_at_the_exact_sample_and_start_resumes_there() {
        let mut transport = Transport::new();
        transport.seek_sample(12_345);
        transport.start();
        let block = transport.advance_block(128).unwrap();
        assert_eq!(block.start_sample, 12_345);
        assert_eq!(transport.position_samples(), 12_473);

        transport.stop();
        let paused_block = transport.advance_block(256).unwrap();
        assert!(!paused_block.is_playing);
        assert_eq!(paused_block.start_sample, 12_473);
        assert_eq!(transport.position_samples(), 12_473);

        transport.start();
        let resumed_block = transport.advance_block(64).unwrap();
        assert_eq!(resumed_block.start_sample, 12_473);
        assert!(resumed_block.is_playing);
        assert_eq!(transport.position_samples(), 12_537);
    }

    #[test]
    fn repeating_block_without_a_loop_is_one_linear_segment() {
        let mut transport = Transport::new();
        transport.seek_sample(10);
        transport.start();
        let mut segments = segment_buffer::<1>();

        assert_eq!(transport.advance_repeating_block(8, &mut segments), Ok(1));
        assert_eq!(segments[0].output_offset, 0);
        assert_eq!(segments[0].start_sample, 10);
        assert_eq!(segments[0].frame_count, 8);
        assert_eq!(transport.position_samples(), 18);
    }

    #[test]
    fn repeating_block_is_split_at_the_loop_end_and_chase_generation_changes_on_wrap() {
        let mut transport = Transport::new();
        transport.seek_sample(10);
        transport.set_loop_range(TransportLoopRange::new(8, 12));
        transport.start();
        let mut segments = segment_buffer::<4>();

        let count = transport.advance_repeating_block(5, &mut segments).unwrap();

        assert_eq!(count, 2);
        assert_eq!(segments[0].output_offset, 0);
        assert_eq!(segments[0].start_sample, 10);
        assert_eq!(segments[0].frame_count, 2);
        assert_eq!(segments[1].output_offset, 2);
        assert_eq!(segments[1].start_sample, 8);
        assert_eq!(segments[1].frame_count, 3);
        assert_eq!(
            segments[1].chase_generation,
            segments[0].chase_generation + 1
        );
        assert_eq!(transport.position_samples(), 11);
    }

    #[test]
    fn repeating_block_handles_multiple_wraps_inside_one_callback_without_allocating() {
        let mut transport = Transport::new();
        transport.seek_sample(3);
        transport.set_loop_range(TransportLoopRange::new(3, 5));
        transport.start();
        let mut segments = segment_buffer::<8>();

        let count = transport.advance_repeating_block(7, &mut segments).unwrap();

        assert_eq!(count, 4);
        assert_eq!(
            segments[..count]
                .iter()
                .map(|segment| (
                    segment.output_offset,
                    segment.start_sample,
                    segment.frame_count
                ))
                .collect::<Vec<_>>(),
            [(0, 3, 2), (2, 3, 2), (4, 3, 2), (6, 3, 1)]
        );
        assert!(
            segments[..count]
                .windows(2)
                .all(|pair| pair[1].chase_generation == pair[0].chase_generation + 1)
        );
        assert_eq!(transport.position_samples(), 4);
    }

    #[test]
    fn repeat_waits_until_playhead_enters_the_range_and_wraps_at_an_exact_block_edge() {
        let mut transport = Transport::new();
        transport.seek_sample(0);
        transport.set_loop_range(TransportLoopRange::new(2, 4));
        transport.start();
        let mut segments = segment_buffer::<8>();

        let count = transport.advance_repeating_block(4, &mut segments).unwrap();

        assert_eq!(count, 2);
        assert_eq!(segments[0].start_sample, 0);
        assert_eq!(segments[0].frame_count, 2);
        assert_eq!(segments[1].start_sample, 2);
        assert_eq!(segments[1].frame_count, 2);
        assert_eq!(transport.position_samples(), 2);

        let count = transport.advance_repeating_block(1, &mut segments).unwrap();
        assert_eq!(count, 1);
        assert_eq!(segments[0].start_sample, 2);
        assert_eq!(
            segments[0].chase_generation,
            segments[1].chase_generation + 1
        );
        assert_eq!(transport.position_samples(), 3);
    }

    #[test]
    fn insufficient_segment_storage_and_sample_overflow_leave_transport_unchanged() {
        let mut transport = Transport::new();
        transport.seek_sample(3);
        transport.set_loop_range(TransportLoopRange::new(3, 5));
        transport.start();
        let before = transport;
        let mut too_small = segment_buffer::<2>();
        assert_eq!(
            transport.advance_repeating_block(5, &mut too_small),
            Err(TransportBlockError::SegmentBufferTooSmall {
                required: 3,
                available: 2,
            })
        );
        assert_eq!(transport, before);

        let mut overflowing = Transport::new();
        overflowing.seek_sample(u64::MAX - 1);
        overflowing.start();
        let before = overflowing;
        let mut segments = segment_buffer::<1>();
        assert_eq!(
            overflowing.advance_repeating_block(2, &mut segments),
            Err(TransportBlockError::PositionOverflow)
        );
        assert_eq!(overflowing, before);
    }

    #[test]
    fn empty_loop_ranges_are_rejected_and_stopped_transport_does_not_advance() {
        assert_eq!(TransportLoopRange::new(5, 5), None);
        assert_eq!(TransportLoopRange::new(6, 5), None);

        let mut transport = Transport::new();
        transport.seek_sample(9);
        transport.set_loop_range(TransportLoopRange::new(2, 4));
        let mut segments = segment_buffer::<1>();
        assert_eq!(transport.advance_repeating_block(16, &mut segments), Ok(1));
        assert_eq!(segments[0].start_sample, 9);
        assert_eq!(segments[0].frame_count, 16);
        assert_eq!(transport.position_samples(), 9);
    }
}
