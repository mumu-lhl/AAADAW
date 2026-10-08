use rtrb::{Consumer, PopError, Producer, PushError, RingBuffer};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering, fence};

const CAPTURE_BLOCK_QUEUE_CAPACITY: usize = 16_384;

fn sample_peak(sample: f32) -> f32 {
    if sample.is_finite() {
        sample.abs()
    } else {
        0.0
    }
}

#[derive(Default)]
struct CaptureState {
    enabled: AtomicBool,
    failed: AtomicBool,
    timing_error: AtomicBool,
    overflow_frames: AtomicU64,
    callback_error: AtomicBool,
    first_capture_frame: AtomicU64,
    has_first_capture_frame: AtomicBool,
    input_peak: [AtomicU32; 2],
}

impl CaptureState {
    fn observe_input_peak(&self, peaks: [f32; 2]) {
        for (channel, peak) in peaks.into_iter().enumerate() {
            let peak = if peak.is_finite() { peak.abs() } else { 0.0 };
            self.input_peak[channel].fetch_max(peak.to_bits(), Ordering::Relaxed);
        }
    }
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

    /// Returns the timestamp of the first callback block accepted after capture was enabled.
    pub fn first_capture_frame(&self) -> Option<u64> {
        self.0
            .has_first_capture_frame
            .load(Ordering::Acquire)
            .then(|| self.0.first_capture_frame.load(Ordering::Relaxed))
    }

    /// Takes the accumulated stereo input sample peaks since the previous poll.
    pub fn take_input_peak(&self) -> [f32; 2] {
        [
            f32::from_bits(self.0.input_peak[0].swap(0, Ordering::Relaxed)),
            f32::from_bits(self.0.input_peak[1].swap(0, Ordering::Relaxed)),
        ]
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
    monitor: Option<AudioMonitorProducer>,
    state: Arc<CaptureState>,
    next_contiguous_frame: u64,
    published_capture_frame: bool,
}

/// Control-thread handle for enabling the bounded input-monitor tap.
#[derive(Clone, Debug, Default)]
pub struct AudioInputMonitorGate(Arc<MonitorGateState>);

#[derive(Debug, Default)]
struct MonitorGateState {
    enabled: AtomicBool,
    generation: AtomicU64,
}

impl AudioInputMonitorGate {
    /// Enables or disables copying input frames to the output-side monitor queue.
    pub fn set_enabled(&self, enabled: bool) {
        if enabled {
            self.0.generation.fetch_add(1, Ordering::AcqRel);
            self.0.enabled.store(true, Ordering::Release);
        } else {
            self.0.enabled.store(false, Ordering::Release);
        }
    }

    /// Returns whether any armed track currently routes the input monitor.
    pub fn is_enabled(&self) -> bool {
        self.0.enabled.load(Ordering::Acquire)
    }

    fn generation(&self) -> u64 {
        self.0.generation.load(Ordering::Acquire)
    }
}

/// Input-callback side of a best-effort, bounded live-monitor queue.
///
/// Clones share the queue cursor so its owner can retain a handle across render-graph swaps. Only
/// one clone may publish at a time; use the clone in the single active audio-input callback.
#[derive(Clone)]
pub struct AudioMonitorProducer {
    queue: Arc<AudioMonitorQueue>,
    gate: AudioInputMonitorGate,
}

/// Output-callback side of a best-effort, bounded live-monitor queue.
///
/// Clones share the read cursor. Render-graph replacement must leave only one active reader at a
/// time; audio backends swap graphs at callback boundaries.
#[derive(Clone)]
pub struct AudioMonitorConsumer {
    queue: Arc<AudioMonitorQueue>,
    next_read: Arc<AtomicU64>,
    gate: AudioInputMonitorGate,
}

struct AudioMonitorQueue {
    frames: Box<[AudioMonitorFrame]>,
    next_write_index: AtomicU64,
    write_index: AtomicU64,
}

struct AudioMonitorFrame {
    sequence: AtomicU64,
    samples: AtomicU64,
    generation: AtomicU64,
}

/// Creates a bounded stereo monitor queue. Monitor overrun skips stale frames and never
/// invalidates the recording take.
pub fn audio_monitor_stream(
    capacity_frames: usize,
) -> (
    AudioMonitorProducer,
    AudioMonitorConsumer,
    AudioInputMonitorGate,
) {
    let queue = Arc::new(AudioMonitorQueue {
        frames: (0..capacity_frames.max(1))
            .map(|_| AudioMonitorFrame {
                sequence: AtomicU64::new(0),
                samples: AtomicU64::new(0),
                generation: AtomicU64::new(0),
            })
            .collect(),
        next_write_index: AtomicU64::new(0),
        write_index: AtomicU64::new(0),
    });
    let gate = AudioInputMonitorGate::default();
    (
        AudioMonitorProducer {
            queue: Arc::clone(&queue),
            gate: gate.clone(),
        },
        AudioMonitorConsumer {
            queue,
            next_read: Arc::new(AtomicU64::new(0)),
            gate: gate.clone(),
        },
        gate,
    )
}

impl AudioMonitorProducer {
    /// Publishes one frame when monitoring is enabled, overwriting the oldest buffered frame.
    pub fn push_frame(&mut self, frame: [f32; 2]) -> bool {
        if !self.gate.is_enabled() {
            return false;
        }
        self.write_frame(frame);
        true
    }

    fn write_frame(&mut self, frame: [f32; 2]) {
        // Producers may be cloned for an input stream that must survive output graph swaps.
        // The device still has one active callback writer; reserving indices in the shared queue
        // keeps that writer aligned with any retained control-side producer handle.
        let index = self.queue.next_write_index.fetch_add(1, Ordering::Relaxed);
        let slot = &self.queue.frames[(index % self.queue.frames.len() as u64) as usize];
        let writing_sequence = index.wrapping_mul(2).wrapping_add(1);
        slot.sequence.swap(writing_sequence, Ordering::AcqRel);
        slot.samples.store(
            u64::from(frame[0].to_bits()) | (u64::from(frame[1].to_bits()) << 32),
            Ordering::Relaxed,
        );
        slot.generation
            .store(self.gate.generation(), Ordering::Relaxed);
        slot.sequence
            .store(writing_sequence.wrapping_add(1), Ordering::Release);
        self.queue
            .write_index
            .store(index.wrapping_add(1), Ordering::Release);
    }
}

impl AudioMonitorConsumer {
    /// Copies available frames and clears any underrun tail without blocking or allocating.
    pub fn read_into(&mut self, output: &mut [[f32; 2]]) -> usize {
        output.fill([0.0, 0.0]);
        let available_end = self.queue.write_index.load(Ordering::Acquire);
        let capacity = self.queue.frames.len() as u64;
        let oldest_retained = available_end.saturating_sub(capacity);
        let mut next_read = self.next_read.load(Ordering::Relaxed);
        if next_read < oldest_retained {
            next_read = oldest_retained;
        }
        let mut read = 0;
        while read < output.len() && next_read < available_end {
            let index = next_read;
            let slot = &self.queue.frames[(index % self.queue.frames.len() as u64) as usize];
            let expected_sequence = index.wrapping_mul(2).wrapping_add(2);
            if slot.sequence.load(Ordering::Acquire) == expected_sequence {
                let packed = slot.samples.load(Ordering::Relaxed);
                let generation = slot.generation.load(Ordering::Relaxed);
                fence(Ordering::Acquire);
                if slot.sequence.load(Ordering::Acquire) == expected_sequence
                    && self.gate.is_enabled()
                    && generation == self.gate.generation()
                {
                    output[read] = [
                        f32::from_bits(packed as u32),
                        f32::from_bits((packed >> 32) as u32),
                    ];
                }
            }
            read += 1;
            next_read = next_read.wrapping_add(1);
        }
        self.next_read.store(next_read, Ordering::Release);
        output.len() - read
    }
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
            monitor: None,
            state: Arc::clone(&state),
            next_contiguous_frame: 0,
            published_capture_frame: false,
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
    /// Attaches the output graph's bounded live-monitor queue before starting the backend.
    pub fn attach_monitor(&mut self, monitor: AudioMonitorProducer) {
        self.monitor = Some(monitor);
    }

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
        let capture_enabled = self.state.enabled.load(Ordering::Acquire)
            && !self.state.failed.load(Ordering::Relaxed);
        let monitor_enabled = self
            .monitor
            .as_ref()
            .is_some_and(|monitor| monitor.gate.is_enabled());
        let mut frames = frames.into_iter();
        if !capture_enabled {
            if monitor_enabled && let Some(monitor) = &mut self.monitor {
                let mut peak = [0.0_f32; 2];
                for frame in frames {
                    peak[0] = peak[0].max(sample_peak(frame[0]));
                    peak[1] = peak[1].max(sample_peak(frame[1]));
                    monitor.write_frame(frame);
                }
                self.state.observe_input_peak(peak);
            }
            return 0;
        }

        let mut frame_count = 0_usize;
        let mut peak = [0.0_f32; 2];
        while let Some(frame) = frames.next() {
            peak[0] = peak[0].max(sample_peak(frame[0]));
            peak[1] = peak[1].max(sample_peak(frame[1]));
            if monitor_enabled && let Some(monitor) = &mut self.monitor {
                monitor.write_frame(frame);
            }
            if let Err(PushError::Full(_)) = self.producer.push(frame) {
                let mut dropped_frames = 1_u64;
                for frame in frames {
                    peak[0] = peak[0].max(sample_peak(frame[0]));
                    peak[1] = peak[1].max(sample_peak(frame[1]));
                    dropped_frames = dropped_frames.saturating_add(1);
                }
                self.state.observe_input_peak(peak);
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
        self.state.observe_input_peak(peak);
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
        if matches!(self.block_producer.push(block), Err(PushError::Full(_))) {
            return false;
        }
        if !self.published_capture_frame {
            self.state
                .first_capture_frame
                .store(first_frame, Ordering::Relaxed);
            self.state
                .has_first_capture_frame
                .store(true, Ordering::Release);
            self.published_capture_frame = true;
        }
        true
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
    use super::{audio_capture_stream, audio_capture_stream_with_capacity, audio_monitor_stream};

    #[test]
    fn input_meter_reports_stereo_peaks_for_capture_and_standby_monitoring() {
        let (mut producer, _, control) = audio_capture_stream(8);
        control.start();
        producer.push_frames_at(0, [[-0.75, 0.25], [0.5, -1.2]]);
        assert_eq!(control.take_input_peak(), [0.75, 1.2]);
        assert_eq!(control.take_input_peak(), [0.0; 2]);

        let (mut producer, _, control) = audio_capture_stream(8);
        let (monitor, _, gate) = audio_monitor_stream(8);
        producer.attach_monitor(monitor);
        gate.set_enabled(true);
        producer.push_frames_at(0, [[-0.4, 0.1], [0.2, -0.9]]);
        assert_eq!(control.take_input_peak(), [0.4, 0.9]);
    }

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
    fn input_meter_includes_peaks_after_capture_queue_overflow() {
        let (mut producer, _consumer, control) = super::audio_capture_stream_with_capacity(2, 2);
        control.start();
        producer.push_frames_at(0, [[0.1, 0.2], [0.3, 0.4], [0.2, 0.5], [0.6, -1.4]]);

        assert!(control.has_overflowed());
        assert_eq!(control.overflow_frames(), 2);
        assert_eq!(control.take_input_peak(), [0.6, 1.4]);
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
    fn control_remembers_the_first_published_frame_after_capture_is_armed() {
        let (mut producer, _consumer, control) = audio_capture_stream(8);
        producer.push_frames_at(40, [[0.0, 0.0]]);
        assert_eq!(control.first_capture_frame(), None);

        control.start();
        producer.push_frames_at(100, [[0.1, -0.1]]);
        producer.push_frames_at(120, [[0.2, -0.2]]);

        assert_eq!(control.first_capture_frame(), Some(100));
    }

    #[test]
    fn monitor_tap_is_off_by_default_and_cannot_overflow_or_invalidate_recording() {
        let (mut producer, mut capture, control) = super::audio_capture_stream(8);
        let (monitor_producer, mut monitor, gate) = super::audio_monitor_stream(2);
        producer.attach_monitor(monitor_producer);
        control.start();

        producer.push_frames_at(100, [[0.1, -0.1], [0.2, -0.2]]);
        let mut captured = [[0.0; 2]; 2];
        assert_eq!(
            capture.pop_timed_frames(&mut captured),
            Some(super::CapturedFrames {
                first_frame: 100,
                frame_count: 2,
            })
        );
        assert_eq!(captured, [[0.1, -0.1], [0.2, -0.2]]);
        let mut monitored = [[9.0; 2]; 2];
        assert_eq!(monitor.read_into(&mut monitored), 2);
        assert_eq!(monitored, [[0.0; 2]; 2]);

        gate.set_enabled(true);
        producer.push_frames_at(102, [[0.3, -0.3], [0.4, -0.4], [0.5, -0.5]]);
        assert_eq!(monitor.read_into(&mut monitored), 0);
        assert_eq!(monitored, [[0.4, -0.4], [0.5, -0.5]]);
        assert!(!control.has_failed());
        assert_eq!(control.overflow_frames(), 0);
        let mut captured = [[0.0; 2]; 3];
        assert_eq!(
            capture.pop_timed_frames(&mut captured).unwrap().frame_count,
            3
        );
        assert_eq!(captured, [[0.3, -0.3], [0.4, -0.4], [0.5, -0.5]]);
    }

    #[test]
    fn standby_monitoring_does_not_capture_preroll_and_recording_reuses_the_tap() {
        let (mut producer, mut capture, control) = super::audio_capture_stream(8);
        let (monitor_producer, mut monitor, gate) = super::audio_monitor_stream(8);
        producer.attach_monitor(monitor_producer);
        gate.set_enabled(true);

        producer.push_frames_at(400, [[0.1, -0.1], [0.2, -0.2]]);
        let mut frames = [[0.0; 2]; 4];
        assert_eq!(monitor.read_into(&mut frames[..2]), 0);
        assert_eq!(frames[..2], [[0.1, -0.1], [0.2, -0.2]]);
        assert!(capture.pop_timed_frames(&mut frames).is_none());
        assert_eq!(control.first_capture_frame(), None);

        control.start();
        producer.push_frames_at(402, [[0.3, -0.3], [0.4, -0.4]]);
        let captured = capture
            .pop_timed_frames(&mut frames)
            .expect("recording should accept frames after capture is enabled");
        assert_eq!(captured.first_frame, 402);
        assert_eq!(captured.frame_count, 2);
        assert_eq!(frames[..2], [[0.3, -0.3], [0.4, -0.4]]);
        assert_eq!(control.first_capture_frame(), Some(402));
    }

    #[test]
    fn cloned_monitor_endpoints_keep_queue_positions_across_graph_replacement() {
        let (mut callback_producer, consumer, gate) = super::audio_monitor_stream(4);
        let mut retained_producer = callback_producer.clone();
        let mut old_graph_consumer = consumer;
        let mut replacement_graph_consumer = old_graph_consumer.clone();
        gate.set_enabled(true);
        assert!(callback_producer.push_frame([0.1, -0.1]));

        let mut output = [[0.0; 2]; 1];
        assert_eq!(old_graph_consumer.read_into(&mut output), 0);
        assert_eq!(output, [[0.1, -0.1]]);
        assert!(retained_producer.push_frame([0.2, -0.2]));
        assert_eq!(replacement_graph_consumer.read_into(&mut output), 0);
        assert_eq!(output, [[0.2, -0.2]]);
    }

    #[test]
    fn monitor_reenable_discards_frames_from_the_previous_monitor_session() {
        let (mut producer, mut consumer, gate) = super::audio_monitor_stream(4);
        gate.set_enabled(true);
        assert!(producer.push_frame([0.1, -0.1]));
        gate.set_enabled(false);
        gate.set_enabled(true);
        assert!(producer.push_frame([0.9, -0.9]));

        let mut output = [[0.0; 2]; 2];
        assert_eq!(consumer.read_into(&mut output), 0);
        assert_eq!(output, [[0.0, 0.0], [0.9, -0.9]]);
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
