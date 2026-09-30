use rtrb::{Consumer, PopError, Producer, PushError, RingBuffer};
use std::fmt;

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
}
