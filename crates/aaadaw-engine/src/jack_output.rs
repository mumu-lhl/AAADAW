use crate::{AudioRenderGraph, TransportClockAnchor};
use jack::{
    AudioOut, Client, ClientOptions, Control, NotificationHandler, Port, ProcessHandler,
    ProcessScope,
};
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering, fence};
use std::thread;
use std::time::{Duration, Instant};

const TRANSPORT_COMMAND_CAPACITY: usize = 16;

enum TransportCommand {
    Play,
    Stop,
    PanicMidi,
    ReplaceGraph {
        graph: Box<AudioRenderGraph>,
        start_playing: bool,
    },
    Shutdown,
}

#[derive(Default)]
struct CallbackCounters {
    shutdown_acknowledged: AtomicBool,
    rendered_blocks: AtomicU64,
    underrun_samples: AtomicU64,
    master_guarded_samples: AtomicU64,
    master_non_finite_samples: AtomicU64,
    device_xruns: AtomicU64,
    callback_errors: AtomicU64,
    playhead_sample: AtomicU64,
    transport_clock_sequence: AtomicU64,
    transport_server_frame: AtomicU64,
    transport_project_sample: AtomicU64,
    has_transport_clock_anchor: AtomicBool,
}

struct JackNotifications {
    counters: Arc<CallbackCounters>,
}

impl JackNotifications {
    fn record_xrun(&self) {
        self.counters.device_xruns.fetch_add(1, Ordering::Relaxed);
    }
}

impl NotificationHandler for JackNotifications {
    fn xrun(&mut self, _client: &Client) -> Control {
        self.record_xrun();
        Control::Continue
    }
}

struct JackProcessHandler {
    graph: Box<AudioRenderGraph>,
    left: Port<AudioOut>,
    right: Port<AudioOut>,
    scratch: Vec<[f32; 2]>,
    commands: Consumer<TransportCommand>,
    retired_graphs: Producer<Box<AudioRenderGraph>>,
    pending_retired_graph: Option<Box<AudioRenderGraph>>,
    counters: Arc<CallbackCounters>,
    shutdown_requested: bool,
}

impl JackProcessHandler {
    fn apply_transport_commands(&mut self) {
        self.flush_retired_graph();
        while let Ok(command) = self.commands.pop() {
            match command {
                TransportCommand::Play => self.graph.transport_mut().start(),
                TransportCommand::Stop => {
                    let failures = self.graph.release_midi_notes();
                    self.counters
                        .callback_errors
                        .fetch_add(failures as u64, Ordering::Relaxed);
                    self.graph.transport_mut().stop();
                }
                TransportCommand::PanicMidi => {
                    let failures = self.graph.release_midi_notes();
                    self.counters
                        .callback_errors
                        .fetch_add(failures as u64, Ordering::Relaxed);
                }
                TransportCommand::ReplaceGraph {
                    mut graph,
                    start_playing,
                } => {
                    if start_playing {
                        graph.transport_mut().start();
                    } else {
                        graph.transport_mut().stop();
                    }
                    let failures = self.graph.stop_instruments();
                    let fx_failures = self.graph.stop_fx_processors();
                    self.counters.callback_errors.fetch_add(
                        failures.saturating_add(fx_failures) as u64,
                        Ordering::Relaxed,
                    );
                    let retired = std::mem::replace(&mut self.graph, graph);
                    match self.retired_graphs.push(retired) {
                        Ok(()) => {}
                        Err(PushError::Full(retired)) => {
                            self.pending_retired_graph = Some(retired);
                        }
                    }
                }
                TransportCommand::Shutdown => {
                    let failures = self.graph.release_midi_notes();
                    let instrument_failures = self.graph.stop_instruments();
                    let fx_failures = self.graph.stop_fx_processors();
                    self.counters.callback_errors.fetch_add(
                        failures
                            .saturating_add(instrument_failures)
                            .saturating_add(fx_failures) as u64,
                        Ordering::Relaxed,
                    );
                    self.graph.transport_mut().stop();
                    self.shutdown_requested = true;
                    self.counters
                        .shutdown_acknowledged
                        .store(true, Ordering::Release);
                }
            }
        }
    }

    fn flush_retired_graph(&mut self) {
        let Some(retired) = self.pending_retired_graph.take() else {
            return;
        };
        if let Err(PushError::Full(retired)) = self.retired_graphs.push(retired) {
            self.pending_retired_graph = Some(retired);
        }
    }
}

impl ProcessHandler for JackProcessHandler {
    fn process(&mut self, _client: &Client, scope: &ProcessScope) -> Control {
        self.apply_transport_commands();
        let server_frame = scope.last_frame_time();
        let project_sample_at_cycle_start = self.graph.transport_mut().position_samples();
        let frame_count = scope.n_frames() as usize;
        let left = self.left.as_mut_slice(scope);
        let right = self.right.as_mut_slice(scope);
        if frame_count > self.scratch.len()
            || left.len() != frame_count
            || right.len() != frame_count
        {
            left.fill(0.0);
            right.fill(0.0);
            self.counters
                .callback_errors
                .fetch_add(1, Ordering::Relaxed);
            return Control::Continue;
        }

        if self.shutdown_requested {
            left.fill(0.0);
            right.fill(0.0);
            return Control::Continue;
        }

        match self.graph.render_into(&mut self.scratch[..frame_count]) {
            Ok(stats) => {
                for ((left_sample, right_sample), frame) in
                    left.iter_mut().zip(right).zip(&self.scratch[..frame_count])
                {
                    *left_sample = frame[0];
                    *right_sample = frame[1];
                }
                self.counters
                    .underrun_samples
                    .fetch_add(stats.underrun_samples as u64, Ordering::Relaxed);
                self.counters
                    .master_guarded_samples
                    .fetch_add(stats.master_guarded_samples as u64, Ordering::Relaxed);
                self.counters
                    .master_non_finite_samples
                    .fetch_add(stats.master_non_finite_samples as u64, Ordering::Relaxed);
                self.counters.rendered_blocks.fetch_add(1, Ordering::AcqRel);
                self.counters.playhead_sample.store(
                    self.graph.transport_mut().position_samples(),
                    Ordering::Relaxed,
                );
                self.counters
                    .transport_clock_sequence
                    .fetch_add(1, Ordering::AcqRel);
                self.counters
                    .transport_server_frame
                    .store(u64::from(server_frame), Ordering::Relaxed);
                self.counters
                    .transport_project_sample
                    .store(project_sample_at_cycle_start, Ordering::Relaxed);
                self.counters
                    .transport_clock_sequence
                    .fetch_add(1, Ordering::Release);
                self.counters
                    .has_transport_clock_anchor
                    .store(true, Ordering::Release);
            }
            Err(_) => {
                left.fill(0.0);
                right.fill(0.0);
                self.counters
                    .callback_errors
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
        Control::Continue
    }
}

/// Errors encountered while configuring or controlling JACK output.
#[derive(Debug)]
pub enum JackOutputError {
    Jack(jack::Error),
    SampleRateMismatch { project: u32, device: u32 },
    DeviceBlockTooLarge { device: usize, maximum: usize },
    ControlQueueFull,
    GraphReplacementInFlight,
    ShutdownTimedOut,
}

impl fmt::Display for JackOutputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Jack(error) => write!(formatter, "JACK output error: {error}"),
            Self::SampleRateMismatch { project, device } => write!(
                formatter,
                "project sample rate {project} Hz does not match JACK rate {device} Hz"
            ),
            Self::DeviceBlockTooLarge { device, maximum } => write!(
                formatter,
                "JACK block size {device} exceeds render capacity {maximum}"
            ),
            Self::ControlQueueFull => formatter.write_str("JACK transport command queue is full"),
            Self::GraphReplacementInFlight => {
                formatter.write_str("previous JACK render graph replacement is not collected")
            }
            Self::ShutdownTimedOut => {
                formatter.write_str("JACK callback did not acknowledge playback shutdown")
            }
        }
    }
}

impl StdError for JackOutputError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Jack(error) => Some(error),
            _ => None,
        }
    }
}

impl From<jack::Error> for JackOutputError {
    fn from(error: jack::Error) -> Self {
        Self::Jack(error)
    }
}

/// A running JACK stereo-output client.
///
/// The process callback only touches preallocated buffers, lock-free queues,
/// atomics, and the render graph. Replaced graphs return through a second SPSC
/// queue and are reclaimed by the control thread.
pub struct JackAudioOutput {
    _active: Option<jack::AsyncClient<JackNotifications, JackProcessHandler>>,
    commands: Producer<TransportCommand>,
    retired_graphs: Consumer<Box<AudioRenderGraph>>,
    counters: Arc<CallbackCounters>,
    device_sample_rate: u32,
    device_block_size: usize,
    replacement_pending: bool,
}

/// Callback counters that can be read safely from a non-realtime thread.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JackOutputStats {
    pub rendered_blocks: u64,
    pub underrun_samples: u64,
    pub master_guarded_samples: u64,
    pub master_non_finite_samples: u64,
    pub device_xruns: u64,
    pub callback_errors: u64,
    /// Project sample position after the most recent successful render callback.
    pub playhead_sample: u64,
    /// JACK server frame and project sample at the start of the same successful render callback.
    pub transport_clock_anchor: Option<TransportClockAnchor>,
}

impl JackAudioOutput {
    /// Opens the default JACK client and starts the render callback.
    pub fn open(graph: AudioRenderGraph) -> Result<Self, JackOutputError> {
        let (client, _status) = Client::new("aaadaw", ClientOptions::default())?;
        let device_sample_rate = client.sample_rate();
        if device_sample_rate != graph.sample_rate() {
            return Err(JackOutputError::SampleRateMismatch {
                project: graph.sample_rate(),
                device: device_sample_rate,
            });
        }
        let device_block_size = client.buffer_size() as usize;
        if device_block_size > graph.max_block_frames() {
            return Err(JackOutputError::DeviceBlockTooLarge {
                device: device_block_size,
                maximum: graph.max_block_frames(),
            });
        }

        let left = client.register_port("out_l", AudioOut::default())?;
        let right = client.register_port("out_r", AudioOut::default())?;
        let (commands, command_consumer) = RingBuffer::new(TRANSPORT_COMMAND_CAPACITY);
        let (retired_graphs, retired_graph_consumer) = RingBuffer::new(1);
        let counters = Arc::new(CallbackCounters {
            shutdown_acknowledged: AtomicBool::new(false),
            rendered_blocks: AtomicU64::new(0),
            underrun_samples: AtomicU64::new(0),
            master_guarded_samples: AtomicU64::new(0),
            master_non_finite_samples: AtomicU64::new(0),
            device_xruns: AtomicU64::new(0),
            callback_errors: AtomicU64::new(0),
            playhead_sample: AtomicU64::new(0),
            transport_clock_sequence: AtomicU64::new(0),
            transport_server_frame: AtomicU64::new(0),
            transport_project_sample: AtomicU64::new(0),
            has_transport_clock_anchor: AtomicBool::new(false),
        });
        let process_handler = JackProcessHandler {
            graph: Box::new(graph),
            left,
            right,
            scratch: vec![[0.0; 2]; device_block_size],
            commands: command_consumer,
            retired_graphs,
            pending_retired_graph: None,
            counters: Arc::clone(&counters),
            shutdown_requested: false,
        };
        let notifications = JackNotifications {
            counters: Arc::clone(&counters),
        };
        let active = client.activate_async(notifications, process_handler)?;
        Ok(Self {
            _active: Some(active),
            commands,
            retired_graphs: retired_graph_consumer,
            counters,
            device_sample_rate,
            device_block_size,
            replacement_pending: false,
        })
    }

    /// Queues a start request for the next audio callback.
    pub fn play(&mut self) -> Result<(), JackOutputError> {
        self.enqueue(TransportCommand::Play)
    }

    /// Queues a stop request for the next audio callback.
    pub fn stop(&mut self) -> Result<(), JackOutputError> {
        self.enqueue(TransportCommand::Stop)
    }

    /// Queues an immediate MIDI note/controller reset without stopping transport.
    pub fn panic_midi(&mut self) -> Result<(), JackOutputError> {
        self.enqueue(TransportCommand::PanicMidi)
    }

    /// Queues a new render graph and preserves playback state.
    ///
    /// Call [`collect_retired_graphs`](Self::collect_retired_graphs) before the next replacement.
    /// The old graph is reclaimed only on this control thread.
    pub fn replace_graph(
        &mut self,
        graph: AudioRenderGraph,
        start_playing: bool,
    ) -> Result<(), JackOutputError> {
        if self.replacement_pending {
            return Err(JackOutputError::GraphReplacementInFlight);
        }
        if graph.sample_rate() != self.device_sample_rate {
            return Err(JackOutputError::SampleRateMismatch {
                project: graph.sample_rate(),
                device: self.device_sample_rate,
            });
        }
        if graph.max_block_frames() < self.device_block_size {
            return Err(JackOutputError::DeviceBlockTooLarge {
                device: self.device_block_size,
                maximum: graph.max_block_frames(),
            });
        }
        self.enqueue(TransportCommand::ReplaceGraph {
            graph: Box::new(graph),
            start_playing,
        })?;
        self.replacement_pending = true;
        Ok(())
    }

    /// Reclaims graphs retired by the callback. Call only from a non-realtime thread.
    pub fn collect_retired_graphs(&mut self) -> usize {
        let retired = self.take_retired_graphs();
        let count = retired.len();
        drop(retired);
        count
    }

    /// Stops the active graph on JACK's callback thread and returns all retired graphs.
    pub fn shutdown(&mut self) -> Result<Vec<AudioRenderGraph>, JackOutputError> {
        if self._active.is_none() {
            return Ok(Vec::new());
        }
        if !self.counters.shutdown_acknowledged.load(Ordering::Acquire) {
            self.enqueue(TransportCommand::Shutdown)?;
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while !self.counters.shutdown_acknowledged.load(Ordering::Acquire) {
            if Instant::now() >= deadline {
                return Err(JackOutputError::ShutdownTimedOut);
            }
            thread::sleep(Duration::from_millis(1));
        }

        let active = self._active.take().expect("JACK output is active");
        let (_client, _notification, mut handler) =
            active.deactivate().map_err(JackOutputError::Jack)?;
        let mut graphs = Vec::new();
        while let Ok(graph) = self.retired_graphs.pop() {
            graphs.push(*graph);
        }
        if let Some(graph) = handler.pending_retired_graph.take() {
            graphs.push(*graph);
        }
        graphs.push(*handler.graph);
        Ok(graphs)
    }

    /// Returns retired graphs to the control thread so their stopped processors can be deactivated.
    pub fn take_retired_graphs(&mut self) -> Vec<AudioRenderGraph> {
        let mut retired_graphs = Vec::new();
        while let Ok(graph) = self.retired_graphs.pop() {
            retired_graphs.push(*graph);
        }
        if !retired_graphs.is_empty() {
            self.replacement_pending = false;
        }
        retired_graphs
    }

    /// Reads callback counters without blocking the audio thread.
    pub fn stats(&self) -> JackOutputStats {
        let transport_clock_anchor = if self
            .counters
            .has_transport_clock_anchor
            .load(Ordering::Acquire)
        {
            read_transport_clock_anchor(&self.counters)
        } else {
            None
        };
        JackOutputStats {
            rendered_blocks: self.counters.rendered_blocks.load(Ordering::Relaxed),
            underrun_samples: self.counters.underrun_samples.load(Ordering::Relaxed),
            master_guarded_samples: self.counters.master_guarded_samples.load(Ordering::Relaxed),
            master_non_finite_samples: self
                .counters
                .master_non_finite_samples
                .load(Ordering::Relaxed),
            device_xruns: self.counters.device_xruns.load(Ordering::Relaxed),
            callback_errors: self.counters.callback_errors.load(Ordering::Relaxed),
            playhead_sample: self.counters.playhead_sample.load(Ordering::Relaxed),
            transport_clock_anchor,
        }
    }

    fn enqueue(&mut self, command: TransportCommand) -> Result<(), JackOutputError> {
        match self.commands.push(command) {
            Ok(()) => Ok(()),
            Err(PushError::Full(_)) => Err(JackOutputError::ControlQueueFull),
        }
    }
}

fn read_transport_clock_anchor(counters: &CallbackCounters) -> Option<TransportClockAnchor> {
    for _ in 0..4 {
        let before = counters.transport_clock_sequence.load(Ordering::Acquire);
        if before % 2 != 0 {
            std::hint::spin_loop();
            continue;
        }
        let frame = counters.transport_server_frame.load(Ordering::Relaxed);
        let sample = counters.transport_project_sample.load(Ordering::Relaxed);
        fence(Ordering::Acquire);
        let after = counters.transport_clock_sequence.load(Ordering::Relaxed);
        if before == after {
            return u32::try_from(frame)
                .ok()
                .map(|server_frame| TransportClockAnchor {
                    backend_frame: server_frame,
                    project_sample: sample,
                });
        }
    }
    None
}

impl Drop for JackAudioOutput {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::{CallbackCounters, JackNotifications, read_transport_clock_anchor};
    use crate::TransportClockAnchor;
    use std::sync::Arc;

    #[test]
    fn jack_xrun_notifications_increment_the_device_counter() {
        let counters = Arc::new(CallbackCounters::default());
        let notifications = JackNotifications {
            counters: Arc::clone(&counters),
        };

        notifications.record_xrun();
        notifications.record_xrun();
        notifications.record_xrun();

        assert_eq!(
            counters
                .device_xruns
                .load(std::sync::atomic::Ordering::Relaxed),
            3
        );
        assert_eq!(
            counters
                .underrun_samples
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
    }

    #[test]
    fn transport_clock_anchor_requires_a_stable_callback_snapshot() {
        let counters = CallbackCounters::default();
        counters
            .transport_clock_sequence
            .store(2, std::sync::atomic::Ordering::Relaxed);
        counters
            .transport_server_frame
            .store(1234, std::sync::atomic::Ordering::Relaxed);
        counters
            .transport_project_sample
            .store(56_789, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(
            read_transport_clock_anchor(&counters),
            Some(TransportClockAnchor {
                backend_frame: 1234,
                project_sample: 56_789,
            })
        );

        counters
            .transport_clock_sequence
            .store(3, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(read_transport_clock_anchor(&counters), None);
    }
}
