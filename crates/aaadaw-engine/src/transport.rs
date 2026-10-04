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

#[cfg(test)]
mod tests {
    use super::Transport;

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
}
