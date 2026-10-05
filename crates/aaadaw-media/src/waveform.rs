use crate::{AudioStreamDecoder, MediaError};
use std::ops::Range;

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
    levels: Vec<WaveformLevel>,
}

/// One level in the min/max reduction pyramid.
#[derive(Clone, Debug, PartialEq)]
pub struct WaveformLevel {
    frames_per_peak: u32,
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
        Self::from_base(sample_rate, frames_per_peak, frame_count, peaks)
            .ok_or(MediaError::InvalidDecodedAudioSpec)
    }

    pub(crate) fn from_base(
        sample_rate: u32,
        frames_per_peak: u32,
        frame_count: u64,
        peaks: Vec<WaveformPeak>,
    ) -> Option<Self> {
        if sample_rate == 0 || frames_per_peak == 0 {
            return None;
        }
        let expected_peaks = frame_count.div_ceil(u64::from(frames_per_peak));
        if u64::try_from(peaks.len()).ok()? != expected_peaks
            || peaks.iter().any(|peak| {
                !peak.min.is_finite()
                    || !peak.max.is_finite()
                    || peak.min < -1.0
                    || peak.max > 1.0
                    || peak.min > peak.max
            })
        {
            return None;
        }
        let mut levels = vec![WaveformLevel {
            frames_per_peak,
            peaks,
        }];
        while levels.last().is_some_and(|level| level.peaks.len() > 1) {
            let previous = levels.last().expect("waveform base level exists");
            let next_frames_per_peak = previous.frames_per_peak.saturating_mul(2);
            let next_peaks = previous
                .peaks
                .chunks(2)
                .map(|pair| WaveformPeak {
                    min: pair
                        .iter()
                        .map(|peak| peak.min)
                        .fold(f32::INFINITY, f32::min),
                    max: pair
                        .iter()
                        .map(|peak| peak.max)
                        .fold(f32::NEG_INFINITY, f32::max),
                })
                .collect();
            levels.push(WaveformLevel {
                frames_per_peak: next_frames_per_peak,
                peaks: next_peaks,
            });
        }

        Some(Self {
            sample_rate,
            frames_per_peak,
            frame_count,
            levels,
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
        &self.levels[0].peaks
    }

    /// Returns the coarsest level that still provides at least one bin per requested source
    /// frame span. This lets the Arrangement avoid uploading sub-pixel peak geometry.
    pub fn level_for_frames_per_peak(&self, requested: u32) -> &WaveformLevel {
        self.levels
            .iter()
            .take_while(|level| level.frames_per_peak <= requested.max(self.frames_per_peak))
            .last()
            .unwrap_or(&self.levels[0])
    }

    /// Returns the peak-bin indexes needed to draw a source-frame interval at the requested
    /// horizontal resolution. The returned range never extends beyond this waveform's bins.
    pub fn peak_range_for_source_frames(
        &self,
        start_frame: u64,
        end_frame: u64,
        requested_frames_per_pixel: u32,
    ) -> (&WaveformLevel, Range<usize>) {
        let level = self.level_for_frames_per_peak(requested_frames_per_pixel);
        let bin_size = u64::from(level.frames_per_peak());
        let start = start_frame.min(self.frame_count);
        let end = end_frame.max(start).min(self.frame_count);
        let first = usize::try_from(start / bin_size).unwrap_or(usize::MAX);
        let last = if start == end {
            first
        } else {
            usize::try_from(end.div_ceil(bin_size))
                .unwrap_or(usize::MAX)
                .min(level.peaks.len())
        };
        (level, first.min(last)..last)
    }

    /// Returns every min/max level, from finest to coarsest.
    pub fn levels(&self) -> &[WaveformLevel] {
        &self.levels
    }
}

impl WaveformLevel {
    /// Returns source frames represented by each bin.
    pub fn frames_per_peak(&self) -> u32 {
        self.frames_per_peak
    }

    /// Returns min/max bins in source order.
    pub fn peaks(&self) -> &[WaveformPeak] {
        &self.peaks
    }
}
