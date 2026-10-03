use rtrb::{Consumer, PopError, Producer, PushError, RingBuffer};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Default)]
struct CaptureState {
    enabled: AtomicBool,
    failed: AtomicBool,
    overflow_frames: AtomicU64,
    callback_error: AtomicBool,
}

/// Control-thread access to a capture stream's armed and overflow state.
#[derive(Clone, Default)]
pub struct AudioCaptureControl(Arc<CaptureState>);

impl AudioCaptureControl {
    /// Starts copying frames from the realtime input callback into the bounded queue.
    pub fn start(&self) {
        self.0.enabled.store(true, Ordering::Release);
    }

    /// Stops accepting frames while leaving queued data available to drain.
    pub fn stop(&self) {
        self.0.enabled.store(false, Ordering::Release);
    }

    /// Returns whether queue overflow invalidated the current take.
    pub fn has_overflowed(&self) -> bool {
        self.0.failed.load(Ordering::Acquire)
    }

    /// Marks the input stream unusable after a backend callback format or stream error.
    pub fn fail(&self) {
        self.0.failed.store(true, Ordering::Release);
        self.0.callback_error.store(true, Ordering::Release);
        self.0.enabled.store(false, Ordering::Release);
    }

    /// Invalidates an active take when a backend drops an input buffer.
    pub fn fail_if_enabled(&self) {
        if self.0.enabled.load(Ordering::Acquire) {
            self.fail();
        }
    }

    /// Returns whether overflow or an input backend error invalidated the take.
    pub fn has_failed(&self) -> bool {
        self.0.failed.load(Ordering::Acquire)
    }

    /// Returns whether the input backend reported a stream or format error.
    pub fn has_callback_error(&self) -> bool {
        self.0.callback_error.load(Ordering::Acquire)
    }

    /// Returns the number of input frames that could not be queued.
    pub fn overflow_frames(&self) -> u64 {
        self.0.overflow_frames.load(Ordering::Relaxed)
    }
}

/// The input-callback side of a bounded stereo capture queue.
pub struct AudioCaptureProducer {
    producer: Producer<[f32; 2]>,
    state: Arc<CaptureState>,
}

/// The worker side of a bounded stereo capture queue.
pub struct AudioCaptureConsumer {
    consumer: Consumer<[f32; 2]>,
}

/// Creates an SPSC queue for interleaved stereo frames and a control handle.
pub fn audio_capture_stream(
    capacity_frames: usize,
) -> (
    AudioCaptureProducer,
    AudioCaptureConsumer,
    AudioCaptureControl,
) {
    let (producer, consumer) = RingBuffer::new(capacity_frames.max(1));
    let state = Arc::new(CaptureState::default());
    (
        AudioCaptureProducer {
            producer,
            state: Arc::clone(&state),
        },
        AudioCaptureConsumer { consumer },
        AudioCaptureControl(state),
    )
}

impl AudioCaptureProducer {
    /// Copies a planar input block into the queue without allocating or waiting.
    /// If the queue fills, it invalidates the take and stops accepting later frames.
    pub fn push_planar(&mut self, left: &[f32], right: &[f32]) {
        self.push_frames(
            left.iter()
                .copied()
                .zip(right.iter().copied())
                .map(|(l, r)| [l, r]),
        );
    }

    /// Copies an interleaved frame iterator into the queue without allocating or waiting.
    pub fn push_frames(&mut self, frames: impl IntoIterator<Item = [f32; 2]>) {
        if !self.state.enabled.load(Ordering::Acquire) || self.state.failed.load(Ordering::Relaxed)
        {
            return;
        }
        let mut frames = frames.into_iter();
        while let Some(frame) = frames.next() {
            if let Err(PushError::Full(_)) = self.producer.push(frame) {
                let dropped = 1_u64.saturating_add(frames.count() as u64);
                self.state
                    .overflow_frames
                    .fetch_add(dropped, Ordering::Relaxed);
                self.state.failed.store(true, Ordering::Release);
                self.state.enabled.store(false, Ordering::Release);
                return;
            }
        }
    }
}

impl AudioCaptureConsumer {
    /// Removes up to `output.len()` frames without blocking or allocating.
    pub fn pop_frames(&mut self, output: &mut [[f32; 2]]) -> usize {
        let mut popped = 0;
        while popped < output.len() {
            match self.consumer.pop() {
                Ok(frame) => {
                    output[popped] = frame;
                    popped += 1;
                }
                Err(PopError::Empty) => break,
            }
        }
        popped
    }
}

#[cfg(test)]
mod tests {
    use super::audio_capture_stream;

    #[test]
    fn capture_queue_only_accepts_armed_frames_and_reports_backpressure() {
        let (mut producer, mut consumer, control) = audio_capture_stream(2);
        producer.push_planar(&[9.0], &[9.0]);
        let mut output = [[0.0; 2]; 4];
        assert_eq!(consumer.pop_frames(&mut output), 0);

        control.start();
        producer.push_planar(&[0.1, 0.2, 0.3], &[-0.1, -0.2, -0.3]);

        assert!(control.has_overflowed());
        assert_eq!(control.overflow_frames(), 1);
        assert_eq!(consumer.pop_frames(&mut output), 2);
        assert_eq!(output[..2], [[0.1, -0.1], [0.2, -0.2]]);

        control.start();
        producer.push_planar(&[0.4], &[-0.4]);
        assert_eq!(consumer.pop_frames(&mut output), 0);
    }

    #[test]
    fn stopped_capture_still_allows_the_worker_to_drain_queued_frames() {
        let (mut producer, mut consumer, control) = audio_capture_stream(4);
        control.start();
        producer.push_planar(&[0.25, 0.5], &[-0.25, -0.5]);
        control.stop();

        let mut output = [[0.0; 2]; 4];
        assert_eq!(consumer.pop_frames(&mut output), 2);
        assert_eq!(output[..2], [[0.25, -0.25], [0.5, -0.5]]);
    }
}
