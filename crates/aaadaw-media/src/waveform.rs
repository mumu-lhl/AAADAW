use crate::{AudioStreamDecoder, MediaError};

/// Peak range for one consecutive group of source sample frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaveformPeak {
    /// Lowest downmixed sample in this bin.
    pub min: f32,
    /// Highest downmixed sample in this bin.
    pub max: f32,
}

/// A mono min/max overview in the media source's native sample clock.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioWaveform {
    sample_rate: u32,
    frames_per_peak: u32,
    frame_count: u64,
    peaks: Vec<WaveformPeak>,
}

impl AudioWaveform {
    /// Decodes an overview without retaining PCM. Bins span packets so their boundaries stay
    /// stable regardless of the decoder's packet size.
    pub fn decode(
        decoder: &mut AudioStreamDecoder,
        frames_per_peak: u32,
    ) -> Result<Self, MediaError> {
        Self::decode_with_cancel(decoder, frames_per_peak, || false)
    }

    /// Decodes until EOF or until `should_cancel` returns true between decoder packets.
    pub fn decode_with_cancel(
        decoder: &mut AudioStreamDecoder,
        frames_per_peak: u32,
        mut should_cancel: impl FnMut() -> bool,
    ) -> Result<Self, MediaError> {
        if frames_per_peak == 0 {
            return Err(MediaError::InvalidWaveformBinSize);
        }
        let sample_rate = decoder
            .metadata()
            .sample_rate
            .filter(|sample_rate| *sample_rate > 0)
            .ok_or(MediaError::InvalidDecodedAudioSpec)?;
        let mut peaks = Vec::new();
        let mut bin_min = f32::INFINITY;
        let mut bin_max = f32::NEG_INFINITY;
        let mut bin_frames = 0_u32;
        let mut frame_count = 0_u64;
        while let Some(chunk) = decoder.next_chunk()? {
            if should_cancel() {
                return Err(MediaError::WorkerCancelled);
            }
            let channels = chunk.channels();
            if channels == 0 {
                return Err(MediaError::InvalidDecodedAudioSpec);
            }
            if chunk.sample_rate() != sample_rate {
                return Err(MediaError::ChangedAudioSampleRate);
            }
            for frame in chunk.samples().chunks_exact(channels) {
                let sample =
                    (frame.iter().copied().sum::<f32>() / channels as f32).clamp(-1.0, 1.0);
                let sample = if sample.is_finite() { sample } else { 0.0 };
                bin_min = bin_min.min(sample);
                bin_max = bin_max.max(sample);
                bin_frames += 1;
                frame_count = frame_count.checked_add(1).ok_or(MediaError::AudioTooLong)?;
                if bin_frames == frames_per_peak {
                    peaks.push(WaveformPeak {
                        min: bin_min,
                        max: bin_max,
                    });
                    bin_min = f32::INFINITY;
                    bin_max = f32::NEG_INFINITY;
                    bin_frames = 0;
                }
            }
        }
        if bin_frames > 0 {
            peaks.push(WaveformPeak {
                min: bin_min,
                max: bin_max,
            });
        }
        Ok(Self {
            sample_rate,
            frames_per_peak,
            frame_count,
            peaks,
        })
    }

    /// Returns the source sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the source frames represented by each peak, except the final partial bin.
    pub fn frames_per_peak(&self) -> u32 {
        self.frames_per_peak
    }

    /// Returns the number of decoded source frames.
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Returns peak bins in source order.
    pub fn peaks(&self) -> &[WaveformPeak] {
        &self.peaks
    }
}
