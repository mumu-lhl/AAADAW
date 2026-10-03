use aaadaw_core::{Project, TimebaseError};
use std::fmt;

/// A block's position and transport state as observed by one engine callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioBlock {
    pub start_sample: u64,
    pub frame_count: usize,
    pub is_playing: bool,
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
    /// arithmetic overflows, the playhead remains unchanged.
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
}
