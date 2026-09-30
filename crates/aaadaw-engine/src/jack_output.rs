use crate::AudioRenderGraph;
use jack::{AudioOut, Client, ClientOptions, Control, Port, ProcessHandler, ProcessScope};
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

const TRANSPORT_COMMAND_CAPACITY: usize = 16;

#[derive(Clone, Copy)]
enum TransportCommand {
    Play,
    Stop,
}

struct CallbackCounters {
    rendered_blocks: AtomicU64,
    underrun_samples: AtomicU64,
    callback_errors: AtomicU64,
}

struct JackProcessHandler {
    graph: AudioRenderGraph,
    left: Port<AudioOut>,
    right: Port<AudioOut>,
    scratch: Vec<[f32; 2]>,
    commands: Consumer<TransportCommand>,
    counters: Arc<CallbackCounters>,
}

impl JackProcessHandler {
    fn apply_transport_commands(&mut self) {
        while let Ok(command) = self.commands.pop() {
            match command {
                TransportCommand::Play => self.graph.transport_mut().start(),
                TransportCommand::Stop => self.graph.transport_mut().stop(),
            }
        }
    }
}

impl ProcessHandler for JackProcessHandler {
    fn process(&mut self, _client: &Client, scope: &ProcessScope) -> Control {
        self.apply_transport_commands();
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
                    .rendered_blocks
                    .fetch_add(1, Ordering::Relaxed);
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
/// atomics, and the render graph. Transport control is sent from the caller via
/// the internal SPSC queue; seeking is intentionally withheld until audio
/// streams can be repositioned together with the transport.
pub struct JackAudioOutput {
    _active: jack::AsyncClient<(), JackProcessHandler>,
    commands: Producer<TransportCommand>,
    counters: Arc<CallbackCounters>,
}

/// Callback counters that can be read safely from a non-realtime thread.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JackOutputStats {
    pub rendered_blocks: u64,
    pub underrun_samples: u64,
    pub callback_errors: u64,
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
        let counters = Arc::new(CallbackCounters {
            rendered_blocks: AtomicU64::new(0),
            underrun_samples: AtomicU64::new(0),
            callback_errors: AtomicU64::new(0),
        });
        let process_handler = JackProcessHandler {
            graph,
            left,
            right,
            scratch: vec![[0.0; 2]; device_block_size],
            commands: command_consumer,
            counters: Arc::clone(&counters),
        };
        let active = client.activate_async((), process_handler)?;
        Ok(Self {
            _active: active,
            commands,
            counters,
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

    /// Reads callback counters without blocking the audio thread.
    pub fn stats(&self) -> JackOutputStats {
        JackOutputStats {
            rendered_blocks: self.counters.rendered_blocks.load(Ordering::Relaxed),
            underrun_samples: self.counters.underrun_samples.load(Ordering::Relaxed),
            callback_errors: self.counters.callback_errors.load(Ordering::Relaxed),
        }
    }

    fn enqueue(&mut self, command: TransportCommand) -> Result<(), JackOutputError> {
        match self.commands.push(command) {
            Ok(()) => Ok(()),
            Err(PushError::Full(_)) => Err(JackOutputError::ControlQueueFull),
        }
    }
}
