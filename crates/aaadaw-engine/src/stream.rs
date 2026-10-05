use rtrb::{Consumer, PopError, Producer, PushError, RingBuffer};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

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

const MAX_STALE_TIMELINE_FRAMES_PER_BLOCK: usize = 4_096;

#[derive(Clone, Copy)]
struct QueuedMonoSample {
    value: f32,
    timeline_sample: Option<u64>,
}

#[derive(Clone, Copy)]
struct QueuedStereoFrame {
    value: [f32; 2],
    timeline_sample: Option<u64>,
}

/// Memory occupied by one stereo PCM queue slot, including its timeline tag.
pub const STEREO_PCM_QUEUE_FRAME_BYTES: usize = std::mem::size_of::<QueuedStereoFrame>();

/// The worker-side handle for pushing decoded, output-rate mono PCM samples.
pub struct PcmStreamProducer {
    producer: Producer<QueuedMonoSample>,
}

/// The audio-thread handle for consuming PCM without locks or allocation.
pub struct PcmStreamConsumer {
    consumer: Consumer<QueuedMonoSample>,
    pending: Option<QueuedMonoSample>,
}

/// The worker-side handle for pushing decoded, output-rate stereo PCM frames.
pub struct StereoPcmStreamProducer {
    producer: Producer<QueuedStereoFrame>,
    stereo_content: Arc<AtomicBool>,
}

/// The audio-thread handle for consuming stereo PCM without locks or allocation.
pub struct StereoPcmStreamConsumer {
    consumer: Consumer<QueuedStereoFrame>,
    stereo_content: Arc<AtomicBool>,
    pending: Option<QueuedStereoFrame>,
}

/// Shared sample-clock resume point between the render callback and a feeder worker.
///
/// The callback only publishes monotonically increasing samples with an atomic max;
/// the worker reads the latest value and discards decoded frames that have fallen behind.
#[derive(Clone, Debug)]
pub struct AudioStreamPosition(Arc<AtomicU64>);

impl AudioStreamPosition {
    /// Creates a stream position initialized to the first timeline sample in its queue.
    pub fn new(sample: u64) -> Self {
        Self(Arc::new(AtomicU64::new(sample)))
    }

    /// Publishes a later sample as the earliest useful sample for this stream.
    pub fn resume_from(&self, sample: u64) {
        self.0.fetch_max(sample, Ordering::Relaxed);
    }

    /// Returns the latest sample published by the render callback.
    pub fn requested_sample(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
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
        PcmStreamConsumer {
            consumer,
            pending: None,
        },
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
            pending: None,
        },
    ))
}

impl PcmStreamProducer {
    /// Pushes as many samples as currently fit and returns the count accepted.
    /// The caller can retry the remaining slice after the consumer advances.
    pub fn push_samples(&mut self, samples: &[f32]) -> usize {
        let mut pushed = 0;
        for sample in samples.iter().copied() {
            match self.producer.push(QueuedMonoSample {
                value: sample,
                timeline_sample: None,
            }) {
                Ok(()) => pushed += 1,
                Err(PushError::Full(_)) => break,
            }
        }
        pushed
    }

    /// Pushes mono samples tagged with their project timeline positions.
    pub fn push_samples_at(&mut self, start_sample: u64, samples: &[f32]) -> usize {
        let mut pushed = 0;
        for (offset, sample) in samples.iter().copied().enumerate() {
            let Some(timeline_sample) = start_sample.checked_add(offset as u64) else {
                break;
            };
            match self.producer.push(QueuedMonoSample {
                value: sample,
                timeline_sample: Some(timeline_sample),
            }) {
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
                    output[index] = sample.value;
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
                    output[index] = [sample.value, sample.value];
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

    /// Reads timeline-tagged samples, dropping stale PCM and silencing timeline gaps.
    pub fn read_timeline_stereo_into(
        &mut self,
        output: &mut [[f32; 2]],
        start_sample: u64,
    ) -> usize {
        let mut underruns = 0_usize;
        let mut stale_discarded = 0;
        for (index, frame) in output.iter_mut().enumerate() {
            let expected_sample = start_sample.saturating_add(index as u64);
            loop {
                let queued = self.pending.take().or_else(|| self.consumer.pop().ok());
                let Some(sample) = queued else {
                    output[index..].fill([0.0, 0.0]);
                    return underruns.saturating_add(output.len() - index);
                };
                if sample
                    .timeline_sample
                    .is_some_and(|sample_sample| sample_sample < expected_sample)
                {
                    stale_discarded += 1;
                    if stale_discarded >= MAX_STALE_TIMELINE_FRAMES_PER_BLOCK {
                        output[index..].fill([0.0, 0.0]);
                        return underruns.saturating_add(output.len() - index);
                    }
                    continue;
                }
                if sample
                    .timeline_sample
                    .is_some_and(|sample_sample| sample_sample > expected_sample)
                {
                    self.pending = Some(sample);
                    *frame = [0.0, 0.0];
                    underruns += 1;
                } else {
                    *frame = [sample.value, sample.value];
                }
                break;
            }
        }
        underruns
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
            match self.producer.push(QueuedStereoFrame {
                value: frame,
                timeline_sample: None,
            }) {
                Ok(()) => pushed += 1,
                Err(PushError::Full(_)) => break,
            }
        }
        pushed
    }

    /// Pushes stereo frames tagged with their project timeline positions.
    pub fn push_frames_at(&mut self, start_sample: u64, frames: &[[f32; 2]]) -> usize {
        let mut pushed = 0;
        for (offset, frame) in frames.iter().copied().enumerate() {
            let Some(timeline_sample) = start_sample.checked_add(offset as u64) else {
                break;
            };
            match self.producer.push(QueuedStereoFrame {
                value: frame,
                timeline_sample: Some(timeline_sample),
            }) {
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
                    output[index] = frame.value;
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

    /// Reads timeline-tagged frames, dropping stale PCM and silencing timeline gaps.
    pub fn read_timeline_into(&mut self, output: &mut [[f32; 2]], start_sample: u64) -> usize {
        let mut underruns = 0_usize;
        let mut stale_discarded = 0;
        for (index, frame) in output.iter_mut().enumerate() {
            let expected_sample = start_sample.saturating_add(index as u64);
            loop {
                let queued = self.pending.take().or_else(|| self.consumer.pop().ok());
                let Some(sample) = queued else {
                    output[index..].fill([0.0, 0.0]);
                    return underruns.saturating_add(
                        (output.len() - index).saturating_mul(if self.is_stereo_content() {
                            2
                        } else {
                            1
                        }),
                    );
                };
                if sample
                    .timeline_sample
                    .is_some_and(|sample_sample| sample_sample < expected_sample)
                {
                    stale_discarded += 1;
                    if stale_discarded >= MAX_STALE_TIMELINE_FRAMES_PER_BLOCK {
                        output[index..].fill([0.0, 0.0]);
                        return underruns.saturating_add(
                            (output.len() - index).saturating_mul(if self.is_stereo_content() {
                                2
                            } else {
                                1
                            }),
                        );
                    }
                    continue;
                }
                if sample
                    .timeline_sample
                    .is_some_and(|sample_sample| sample_sample > expected_sample)
                {
                    self.pending = Some(sample);
                    *frame = [0.0, 0.0];
                    underruns += if self.is_stereo_content() { 2 } else { 1 };
                } else {
                    *frame = sample.value;
                }
                break;
            }
        }
        underruns
    }

    /// Returns the number of frames currently available without waiting.
    pub fn available_frames(&self) -> usize {
        self.consumer.slots()
    }
}
