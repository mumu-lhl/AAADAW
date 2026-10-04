use rtrb::{Consumer, PopError, Producer, PushError, RingBuffer};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The lock-free PCM queue could not be created.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcmStreamError {
    /// A stream queue must have nonzero capacity.
    ZeroCapacity,
}

impl fmt::Display for PcmStreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroCapacity => formatter.write_str("PCM stream capacity must be positive"),
        }
    }
}

impl std::error::Error for PcmStreamError {}

/// The worker-side handle for pushing decoded, output-rate mono PCM samples.
pub struct PcmStreamProducer {
    producer: Producer<f32>,
}

/// The audio-thread handle for consuming PCM without locks or allocation.
pub struct PcmStreamConsumer {
    consumer: Consumer<f32>,
}

/// The worker-side handle for pushing decoded, output-rate stereo PCM frames.
pub struct StereoPcmStreamProducer {
    producer: Producer<[f32; 2]>,
    stereo_content: Arc<AtomicBool>,
}

/// The audio-thread handle for consuming stereo PCM without locks or allocation.
pub struct StereoPcmStreamConsumer {
    consumer: Consumer<[f32; 2]>,
    stereo_content: Arc<AtomicBool>,
}

/// Creates a fixed-capacity SPSC queue. Create and split it before starting
/// either owning thread; send each handle to its single designated thread.
pub fn pcm_stream(
    capacity_samples: usize,
) -> Result<(PcmStreamProducer, PcmStreamConsumer), PcmStreamError> {
    if capacity_samples == 0 {
        return Err(PcmStreamError::ZeroCapacity);
    }
    let (producer, consumer) = RingBuffer::new(capacity_samples);
    Ok((
        PcmStreamProducer { producer },
        PcmStreamConsumer { consumer },
    ))
}

/// Creates a fixed-capacity SPSC queue of interleaved stereo frames.
pub fn stereo_pcm_stream(
    capacity_frames: usize,
) -> Result<(StereoPcmStreamProducer, StereoPcmStreamConsumer), PcmStreamError> {
    if capacity_frames == 0 {
        return Err(PcmStreamError::ZeroCapacity);
    }
    let (producer, consumer) = RingBuffer::new(capacity_frames);
    let stereo_content = Arc::new(AtomicBool::new(false));
    Ok((
        StereoPcmStreamProducer {
            producer,
            stereo_content: Arc::clone(&stereo_content),
        },
        StereoPcmStreamConsumer {
            consumer,
            stereo_content,
        },
    ))
}

impl PcmStreamProducer {
    /// Pushes as many samples as currently fit and returns the count accepted.
    /// The caller can retry the remaining slice after the consumer advances.
    pub fn push_samples(&mut self, samples: &[f32]) -> usize {
        let mut pushed = 0;
        for sample in samples.iter().copied() {
            match self.producer.push(sample) {
                Ok(()) => pushed += 1,
                Err(PushError::Full(_)) => break,
            }
        }
        pushed
    }

    /// Returns the number of samples that can be pushed without waiting.
    pub fn available_capacity(&self) -> usize {
        self.producer.slots()
    }
}

impl PcmStreamConsumer {
    /// Reads samples into the caller's output buffer, zero-filling on queue
    /// underflow and returning the number of silence-filled frames. This is
    /// suitable for an audio callback when the producer has already resampled
    /// to the active device rate.
    pub fn read_into(&mut self, output: &mut [f32]) -> usize {
        let mut index = 0;
        while index < output.len() {
            match self.consumer.pop() {
                Ok(sample) => {
                    output[index] = sample;
                    index += 1;
                }
                Err(PopError::Empty) => {
                    output[index..].fill(0.0);
                    return output.len() - index;
                }
            }
        }
        0
    }

    /// Returns the number of samples currently available without waiting.
    pub fn available_samples(&self) -> usize {
        self.consumer.slots()
    }

    /// Reads mono samples into a stereo frame buffer, centering each sample.
    pub fn read_stereo_into(&mut self, output: &mut [[f32; 2]]) -> usize {
        let mut index = 0;
        while index < output.len() {
            match self.consumer.pop() {
                Ok(sample) => {
                    output[index] = [sample, sample];
                    index += 1;
                }
                Err(PopError::Empty) => {
                    output[index..].fill([0.0, 0.0]);
                    return output.len() - index;
                }
            }
        }
        0
    }
}

impl StereoPcmStreamProducer {
    /// Marks whether queued frames retain distinct left and right source channels.
    pub fn set_stereo_content(&self, is_stereo: bool) {
        self.stereo_content.store(is_stereo, Ordering::Release);
    }

    /// Pushes as many stereo frames as currently fit and returns the count accepted.
    pub fn push_frames(&mut self, frames: &[[f32; 2]]) -> usize {
        let mut pushed = 0;
        for frame in frames.iter().copied() {
            match self.producer.push(frame) {
                Ok(()) => pushed += 1,
                Err(PushError::Full(_)) => break,
            }
        }
        pushed
    }

    /// Returns the number of frames that can be pushed without waiting.
    pub fn available_capacity(&self) -> usize {
        self.producer.slots()
    }
}

impl StereoPcmStreamConsumer {
    /// Returns whether the decoded source contains distinct left and right channels.
    pub fn is_stereo_content(&self) -> bool {
        self.stereo_content.load(Ordering::Acquire)
    }

    /// Reads frames into the caller's output buffer, zero-filling on queue underflow.
    pub fn read_into(&mut self, output: &mut [[f32; 2]]) -> usize {
        let mut index = 0;
        while index < output.len() {
            match self.consumer.pop() {
                Ok(frame) => {
                    output[index] = frame;
                    index += 1;
                }
                Err(PopError::Empty) => {
                    output[index..].fill([0.0, 0.0]);
                    return output.len() - index;
                }
            }
        }
        0
    }

    /// Returns the number of frames currently available without waiting.
    pub fn available_frames(&self) -> usize {
        self.consumer.slots()
    }
}
