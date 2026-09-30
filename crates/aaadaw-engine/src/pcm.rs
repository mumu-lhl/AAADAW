use std::fmt;

/// A predecoded mono PCM clip held in memory for realtime playback.
#[derive(Clone, Debug, PartialEq)]
pub struct MonoPcmClip {
    sample_rate: u32,
    samples: Vec<f32>,
}

/// A clip player that linearly resamples into caller-owned output memory.
#[derive(Clone, Debug)]
pub struct MonoPcmPlayer<'a> {
    clip: &'a MonoPcmClip,
    source_position: f64,
    source_frames_per_output_frame: f64,
}

/// PCM clip or player configuration is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcmError {
    /// Source and output sample rates must be greater than zero.
    InvalidSampleRate,
    /// The requested source-frame seek is beyond the clip's end.
    SeekOutOfRange,
}

impl fmt::Display for PcmError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSampleRate => formatter.write_str("PCM sample rate must be positive"),
            Self::SeekOutOfRange => formatter.write_str("PCM seek exceeds the clip length"),
        }
    }
}

impl std::error::Error for PcmError {}

impl MonoPcmClip {
    /// Creates a clip from decoded mono floating-point samples.
    pub fn new(samples: Vec<f32>, sample_rate: u32) -> Result<Self, PcmError> {
        if sample_rate == 0 {
            return Err(PcmError::InvalidSampleRate);
        }
        Ok(Self {
            sample_rate,
            samples,
        })
    }

    /// Returns the clip's source sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the decoded source samples.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// Creates a player configured for the output device's sample rate.
    pub fn player(&self, output_sample_rate: u32) -> Result<MonoPcmPlayer<'_>, PcmError> {
        if output_sample_rate == 0 {
            return Err(PcmError::InvalidSampleRate);
        }
        Ok(MonoPcmPlayer {
            clip: self,
            source_position: 0.0,
            source_frames_per_output_frame: f64::from(self.sample_rate)
                / f64::from(output_sample_rate),
        })
    }
}

impl MonoPcmPlayer<'_> {
    /// Returns the current fractional position in source frames.
    pub fn source_position(&self) -> f64 {
        self.source_position
    }

    /// Seeks to a source frame, including the exact end position.
    pub fn seek_source_frame(&mut self, frame: usize) -> Result<(), PcmError> {
        if frame > self.clip.samples.len() {
            return Err(PcmError::SeekOutOfRange);
        }
        self.source_position = frame as f64;
        Ok(())
    }

    /// Resamples into caller-owned output memory using linear interpolation.
    ///
    /// Once the clip ends, remaining output frames are zero-filled. This
    /// callback-safe primitive allocates no memory and performs no I/O; linear
    /// interpolation is intentionally a basic quality mode, not a substitute
    /// for a band-limited sample-rate converter or time-stretch algorithm.
    pub fn render_into(&mut self, output: &mut [f32]) {
        let source_frame_count = self.clip.samples.len() as f64;
        for output_sample in output {
            let source_frame = self.source_position;
            if source_frame >= source_frame_count {
                *output_sample = 0.0;
                continue;
            }

            let left_index = source_frame as usize;
            let right_index = (left_index + 1).min(self.clip.samples.len() - 1);
            let fraction = (source_frame - left_index as f64) as f32;
            let left = self.clip.samples[left_index];
            let right = self.clip.samples[right_index];
            *output_sample = left + (right - left) * fraction;
            self.source_position =
                (source_frame + self.source_frames_per_output_frame).min(source_frame_count);
        }
    }
}
