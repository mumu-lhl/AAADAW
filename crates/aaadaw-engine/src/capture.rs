use rtrb::{Consumer, PopError, Producer, PushError, RingBuffer};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const CAPTURE_BLOCK_QUEUE_CAPACITY: usize = 16_384;

#[derive(Default)]
struct CaptureState {
    enabled: AtomicBool,
    failed: AtomicBool,
    timing_error: AtomicBool,
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

    /// Returns whether the current take is accepting input callback frames.
    pub fn is_enabled(&self) -> bool {
        self.0.enabled.load(Ordering::Acquire) && !self.0.failed.load(Ordering::Acquire)
    }

    /// Stops accepting frames while leaving queued data available to drain.
    pub fn stop(&self) {
        self.0.enabled.store(false, Ordering::Release);
    }

    /// Returns whether queue overflow invalidated the current take.
    pub fn has_overflowed(&self) -> bool {
        self.0.overflow_frames.load(Ordering::Acquire) > 0
    }

    /// Returns whether overflow or an input backend error invalidated the take.
    pub fn has_failed(&self) -> bool {
        self.0.failed.load(Ordering::Acquire)
    }

    /// Returns whether an input backend reported a stream or format error.
    pub fn has_callback_error(&self) -> bool {
        self.0.callback_error.load(Ordering::Acquire)
    }

    /// Returns whether capture timing was invalid or could not be represented.
    pub fn has_timing_error(&self) -> bool {
        self.0.timing_error.load(Ordering::Acquire)
    }

    /// Marks the input stream unusable after a backend callback format or stream error.
    pub fn fail(&self) {
        self.0.failed.store(true, Ordering::Release);
        self.0.callback_error.store(true, Ordering::Release);
        self.0.enabled.store(false, Ordering::Release);
    }

    /// Invalidates an active take when the backend provides unusable frame timing.
    pub fn fail_timing(&self) {
        self.0.failed.store(true, Ordering::Release);
        self.0.timing_error.store(true, Ordering::Release);
        self.0.enabled.store(false, Ordering::Release);
    }

    /// Invalidates an active take when backend timing fails outside an armed capture.
    pub fn fail_timing_if_enabled(&self) {
        if self.0.enabled.load(Ordering::Acquire) {
            self.fail_timing();
        }
    }

    /// Invalidates an active take when the input backend drops a capture block.
    pub fn fail_if_enabled(&self) {
        if self.0.enabled.load(Ordering::Acquire) {
            self.fail();
        }
    }

    /// Returns the number of input frames that could not be retained.
    pub fn overflow_frames(&self) -> u64 {
        self.0.overflow_frames.load(Ordering::Relaxed)
    }
}

/// Timing metadata for the frames copied from one input callback block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CapturedFrames {
    pub first_frame: u64,
    pub frame_count: usize,
}

/// The input-callback side of a bounded stereo capture queue.
pub struct AudioCaptureProducer {
    producer: Producer<[f32; 2]>,
    block_producer: Producer<CapturedFrames>,
    state: Arc<CaptureState>,
    next_contiguous_frame: u64,
}

/// The worker side of a bounded stereo capture queue.
pub struct AudioCaptureConsumer {
    consumer: Consumer<[f32; 2]>,
    block_consumer: Consumer<CapturedFrames>,
    state: Arc<CaptureState>,
    pending_block: Option<CapturedFrames>,
    pending_offset: usize,
}

/// Creates bounded queues for stereo frames and callback-block timing metadata.
pub fn audio_capture_stream(
    capacity_frames: usize,
) -> (
    AudioCaptureProducer,
    AudioCaptureConsumer,
    AudioCaptureControl,
) {
    audio_capture_stream_with_capacity(capacity_frames, CAPTURE_BLOCK_QUEUE_CAPACITY)
}

fn audio_capture_stream_with_capacity(
    capacity_frames: usize,
    capacity_blocks: usize,
) -> (
    AudioCaptureProducer,
    AudioCaptureConsumer,
    AudioCaptureControl,
) {
    let (producer, consumer) = RingBuffer::new(capacity_frames.max(1));
    let (block_producer, block_consumer) = RingBuffer::new(capacity_blocks.max(1));
    let state = Arc::new(CaptureState::default());
    (
        AudioCaptureProducer {
            producer,
            block_producer,
            state: Arc::clone(&state),
            next_contiguous_frame: 0,
        },
        AudioCaptureConsumer {
            consumer,
            block_consumer,
            state: Arc::clone(&state),
            pending_block: None,
            pending_offset: 0,
        },
        AudioCaptureControl(state),
    )
}

impl AudioCaptureProducer {
    /// Copies a planar block at the next contiguous frame position.
    ///
    /// Use `push_planar_at` when the backend exposes a frame clock.
    pub fn push_planar(&mut self, left: &[f32], right: &[f32]) {
        if left.len() != right.len() {
            self.fail_timing_if_enabled();
            return;
        }
        self.push_frames(
            left.iter()
                .copied()
                .zip(right.iter().copied())
                .map(|(left, right)| [left, right]),
        );
    }

    /// Copies a planar block and its backend frame position without allocating or waiting.
    pub fn push_planar_at(&mut self, first_frame: u64, left: &[f32], right: &[f32]) {
        if left.len() != right.len() {
            self.fail_timing_if_enabled();
            return;
        }
        self.push_frames_at(
            first_frame,
            left.iter()
                .copied()
                .zip(right.iter().copied())
                .map(|(left, right)| [left, right]),
        );
    }

    /// Copies interleaved frames with an explicit first-frame position.
    pub fn push_frames_at(&mut self, first_frame: u64, frames: impl IntoIterator<Item = [f32; 2]>) {
        self.push_timed_frames(first_frame, frames);
    }

    /// Copies a block whose position follows the previous accepted block.
    ///
    /// This is a compatibility path for backends that do not yet expose per-block timestamps.
    pub fn push_frames(&mut self, frames: impl IntoIterator<Item = [f32; 2]>) {
        let first_frame = self.next_contiguous_frame;
        let frame_count = self.push_timed_frames(first_frame, frames);
        if frame_count > 0 {
            match first_frame.checked_add(frame_count as u64) {
                Some(next) => self.next_contiguous_frame = next,
                None => self.fail_timing(),
            }
        }
    }

    /// Marks capture timing unusable from the backend callback.
    pub fn fail_timing(&self) {
        self.state.failed.store(true, Ordering::Release);
        self.state.timing_error.store(true, Ordering::Release);
        self.state.enabled.store(false, Ordering::Release);
    }

    fn fail_timing_if_enabled(&self) {
        if self.state.enabled.load(Ordering::Acquire) {
            self.fail_timing();
        }
    }

    fn push_timed_frames(
        &mut self,
        first_frame: u64,
        frames: impl IntoIterator<Item = [f32; 2]>,
    ) -> usize {
        if !self.state.enabled.load(Ordering::Acquire) || self.state.failed.load(Ordering::Relaxed)
        {
            return 0;
        }

        let mut frame_count = 0_usize;
        let mut frames = frames.into_iter();
        while let Some(frame) = frames.next() {
            if let Err(PushError::Full(_)) = self.producer.push(frame) {
                let dropped_frames = 1_u64.saturating_add(frames.count() as u64);
                if frame_count > 0 && !self.publish_block(first_frame, frame_count) {
                    self.fail_overflow(dropped_frames.saturating_add(frame_count as u64));
                    return 0;
                }
                self.fail_overflow(dropped_frames);
                return frame_count;
            }
            frame_count += 1;
        }
        if frame_count == 0 {
            return 0;
        }
        if first_frame.checked_add(frame_count as u64).is_none() {
            self.fail_timing();
            return 0;
        }

        if !self.publish_block(first_frame, frame_count) {
            self.fail_overflow(frame_count as u64);
            return 0;
        }
        frame_count
    }

    fn publish_block(&mut self, first_frame: u64, frame_count: usize) -> bool {
        let block = CapturedFrames {
            first_frame,
            frame_count,
        };
        !matches!(self.block_producer.push(block), Err(PushError::Full(_)))
    }

    fn fail_overflow(&self, frame_count: u64) {
        self.state
            .overflow_frames
            .fetch_add(frame_count, Ordering::Relaxed);
        self.state.failed.store(true, Ordering::Release);
        self.state.enabled.store(false, Ordering::Release);
    }
}

impl AudioCaptureConsumer {
    /// Removes part of the next timestamped callback block without blocking or allocating.
    ///
    /// A large callback may be returned in several chunks. Each chunk carries its adjusted first
    /// frame position so the worker can process into a fixed-size buffer.
    pub fn pop_timed_frames(&mut self, output: &mut [[f32; 2]]) -> Option<CapturedFrames> {
        if output.is_empty() {
            return None;
        }
        if self.pending_block.is_none() {
            self.pending_block = match self.block_consumer.pop() {
                Ok(block) if block.frame_count > 0 => Some(block),
                Ok(_) => {
                    self.fail_timing();
                    return None;
                }
                Err(PopError::Empty) => return None,
            };
            self.pending_offset = 0;
        }

        let block = self.pending_block.expect("pending block was loaded");
        let Some(first_frame) = block.first_frame.checked_add(self.pending_offset as u64) else {
            self.fail_timing();
            return None;
        };
        let available = block.frame_count.saturating_sub(self.pending_offset);
        let frame_count = available.min(output.len());
        for frame in &mut output[..frame_count] {
            match self.consumer.pop() {
                Ok(sample) => *frame = sample,
                Err(PopError::Empty) => {
                    self.fail_timing();
                    return None;
                }
            }
        }

        self.pending_offset += frame_count;
        if self.pending_offset == block.frame_count {
            self.pending_block = None;
            self.pending_offset = 0;
        }
        Some(CapturedFrames {
            first_frame,
            frame_count,
        })
    }

    fn fail_timing(&self) {
        self.state.failed.store(true, Ordering::Release);
        self.state.timing_error.store(true, Ordering::Release);
        self.state.enabled.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::{audio_capture_stream, audio_capture_stream_with_capacity};

    #[test]
    fn capture_queue_only_accepts_armed_frames_and_reports_backpressure() {
        let (mut producer, mut consumer, control) = audio_capture_stream(2);
        assert!(!control.is_enabled());
        producer.push_frames_at(100, [[9.0, 9.0]]);
        let mut output = [[0.0; 2]; 4];
        assert!(consumer.pop_timed_frames(&mut output).is_none());

        control.start();
        assert!(control.is_enabled());
        producer.push_frames_at(100, [[0.1, -0.1], [0.2, -0.2], [0.3, -0.3]]);

        assert!(control.has_overflowed());
        assert!(!control.is_enabled());
        assert_eq!(control.overflow_frames(), 1);
        let block = consumer
            .pop_timed_frames(&mut output)
            .expect("accepted frames retain timing metadata");
        assert_eq!(block.first_frame, 100);
        assert_eq!(block.frame_count, 2);
        assert_eq!(output[..2], [[0.1, -0.1], [0.2, -0.2]]);
        assert!(consumer.pop_timed_frames(&mut output).is_none());
    }

    #[test]
    fn stopped_capture_still_allows_the_worker_to_drain_queued_frames() {
        let (mut producer, mut consumer, control) = audio_capture_stream(4);
        control.start();
        producer.push_frames_at(250, [[0.25, -0.25], [0.5, -0.5]]);
        control.stop();
        assert!(!control.is_enabled());

        let mut output = [[0.0; 2]; 4];
        let block = consumer
            .pop_timed_frames(&mut output)
            .expect("queued block should remain drainable after stop");
        assert_eq!(block.first_frame, 250);
        assert_eq!(block.frame_count, 2);
        assert_eq!(output[..2], [[0.25, -0.25], [0.5, -0.5]]);
    }

    #[test]
    fn backend_error_fails_and_disarms_the_active_take() {
        let (mut producer, mut consumer, control) = audio_capture_stream(4);
        control.start();
        control.fail();
        assert!(control.has_failed());
        assert!(control.has_callback_error());
        assert!(!control.is_enabled());

        producer.push_frames_at(0, [[0.5, -0.5]]);
        let mut output = [[0.0; 2]; 1];
        assert!(consumer.pop_timed_frames(&mut output).is_none());
    }

    #[test]
    fn partial_reads_adjust_the_next_chunk_frame_position() {
        let (mut producer, mut consumer, control) = audio_capture_stream(8);
        control.start();
        producer.push_frames_at(1_000, [[1.0, 1.0]; 5]);
        let mut output = [[0.0; 2]; 2];

        let first = consumer
            .pop_timed_frames(&mut output)
            .expect("first chunk should be available");
        let second = consumer
            .pop_timed_frames(&mut output)
            .expect("remaining chunk should be available");
        assert_eq!((first.first_frame, first.frame_count), (1_000, 2));
        assert_eq!((second.first_frame, second.frame_count), (1_002, 2));
        assert_eq!(
            consumer
                .pop_timed_frames(&mut output)
                .expect("last frame should be available")
                .first_frame,
            1_004
        );
    }

    #[test]
    fn descriptor_queue_overflow_invalidates_the_take() {
        let (mut producer, _consumer, control) = audio_capture_stream_with_capacity(8, 1);
        control.start();
        producer.push_frames_at(10, [[0.1, -0.1]]);
        producer.push_frames_at(11, [[0.2, -0.2]]);

        assert!(control.has_failed());
        assert!(control.has_overflowed());
        assert_eq!(control.overflow_frames(), 1);
    }

    #[test]
    fn frame_position_overflow_invalidates_timing_before_publishing_a_block() {
        let (mut producer, _consumer, control) = audio_capture_stream(8);
        control.start();
        producer.push_frames_at(u64::MAX, [[0.1, -0.1]]);

        assert!(control.has_failed());
        assert!(control.has_timing_error());
        assert_eq!(control.overflow_frames(), 0);
    }
}
