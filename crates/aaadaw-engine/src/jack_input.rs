use crate::{AudioCaptureControl, AudioCaptureProducer};
use jack::{
    AudioIn, Client, ClientOptions, Control, LatencyType, NotificationHandler, Port, PortFlags,
    PortId, ProcessHandler, ProcessScope,
};
use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

struct JackCaptureHandler {
    left: Port<AudioIn>,
    right: Port<AudioIn>,
    producer: AudioCaptureProducer,
    control: AudioCaptureControl,
    frame_clock: JackFrameClock,
    latest_frame: Arc<AtomicU64>,
    has_latest_frame: Arc<AtomicBool>,
}

#[derive(Default)]
struct JackFrameClock {
    previous: Option<u32>,
    epoch: u64,
}

impl JackFrameClock {
    fn extend(&mut self, frame: u32) -> Option<u64> {
        if let Some(previous) = self.previous {
            if frame < previous {
                if previous.wrapping_sub(frame) <= u32::MAX / 2 {
                    return None;
                }
                self.epoch = self.epoch.checked_add(u64::from(u32::MAX) + 1)?;
            } else if frame - previous > u32::MAX / 2 {
                return None;
            }
        }
        self.previous = Some(frame);
        self.epoch.checked_add(u64::from(frame))
    }

    fn extend_near(frame: u32, reference: u64) -> Option<u64> {
        let delta = frame.wrapping_sub(reference as u32) as i32;
        if delta >= 0 {
            reference.checked_add(delta as u64)
        } else {
            reference.checked_sub(u64::from(delta.unsigned_abs()))
        }
    }
}

impl ProcessHandler for JackCaptureHandler {
    fn process(&mut self, _client: &Client, scope: &ProcessScope) -> Control {
        let left = self.left.as_slice(scope);
        let right = self.right.as_slice(scope);
        if left.len() != right.len() {
            self.control.fail();
            return Control::Continue;
        }
        let Some(first_frame) = self.frame_clock.extend(scope.last_frame_time()) else {
            self.control.fail_timing_if_enabled();
            return Control::Continue;
        };
        self.latest_frame.store(first_frame, Ordering::Relaxed);
        self.has_latest_frame.store(true, Ordering::Release);
        self.producer.push_planar_at(first_frame, left, right);
        Control::Continue
    }
}

/// Errors encountered while connecting a stereo JACK input capture client.
#[derive(Debug)]
pub enum JackInputError {
    Jack(jack::Error),
    SampleRateMismatch { project: u32, device: u32 },
    NoStereoInput { found: usize },
}

impl fmt::Display for JackInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Jack(error) => write!(formatter, "JACK input setup failed: {error}"),
            Self::SampleRateMismatch { project, device } => write!(
                formatter,
                "project sample rate {project} Hz does not match JACK rate {device} Hz"
            ),
            Self::NoStereoInput { found } => write!(
                formatter,
                "JACK has {found} physical audio input port(s); stereo recording needs two"
            ),
        }
    }
}

impl StdError for JackInputError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Jack(error) => Some(error),
            Self::SampleRateMismatch { .. } | Self::NoStereoInput { .. } => None,
        }
    }
}

impl From<jack::Error> for JackInputError {
    fn from(error: jack::Error) -> Self {
        Self::Jack(error)
    }
}

/// A JACK capture client connected to the first two physical audio input ports.
pub struct JackAudioInput {
    active: Option<jack::AsyncClient<JackCaptureNotifications, JackCaptureHandler>>,
    sample_rate: u32,
    reported_capture_latency_frames: Arc<AtomicU64>,
    latest_frame: Arc<AtomicU64>,
    has_latest_frame: Arc<AtomicBool>,
}

struct JackCaptureNotifications {
    left_port_name: String,
    right_port_name: String,
    reported_capture_latency_frames: Arc<AtomicU64>,
}

impl NotificationHandler for JackCaptureNotifications {
    fn ports_connected(
        &mut self,
        client: &Client,
        _port_id_a: PortId,
        _port_id_b: PortId,
        _are_connected: bool,
    ) {
        update_reported_capture_latency(
            client,
            &self.left_port_name,
            &self.right_port_name,
            &self.reported_capture_latency_frames,
        );
    }
}

fn update_reported_capture_latency(
    client: &Client,
    left_port_name: &str,
    right_port_name: &str,
    reported_capture_latency_frames: &AtomicU64,
) {
    let latency = |name: &str| {
        client
            .port_by_name(name)
            .map(|port| port.get_latency_range(LatencyType::Capture))
    };
    let frames = latency(left_port_name)
        .zip(latency(right_port_name))
        .and_then(|(left, right)| precise_shared_capture_latency(left, right));
    // Zero is intentionally the unavailable sentinel: JACK also defaults an unreported port
    // latency to zero, so it must never be mistaken for a measured correction.
    reported_capture_latency_frames.store(u64::from(frames.unwrap_or(0)), Ordering::Release);
}

impl JackAudioInput {
    /// Opens the default JACK server's stereo input at the project's sample rate.
    pub fn open(
        producer: AudioCaptureProducer,
        control: AudioCaptureControl,
        project_sample_rate: u32,
    ) -> Result<Self, JackInputError> {
        let (client, _) = Client::new("aaadaw_capture", ClientOptions::default())?;
        let sample_rate = client.sample_rate();
        if sample_rate != project_sample_rate {
            return Err(JackInputError::SampleRateMismatch {
                project: project_sample_rate,
                device: sample_rate,
            });
        }
        let left = client.register_port("in_l", AudioIn::default())?;
        let right = client.register_port("in_r", AudioIn::default())?;
        let left_name = left.name()?;
        let right_name = right.name()?;
        let latest_frame = Arc::new(AtomicU64::new(0));
        let has_latest_frame = Arc::new(AtomicBool::new(false));
        let reported_capture_latency_frames = Arc::new(AtomicU64::new(0));
        let active = client.activate_async(
            JackCaptureNotifications {
                left_port_name: left_name.clone(),
                right_port_name: right_name.clone(),
                reported_capture_latency_frames: Arc::clone(&reported_capture_latency_frames),
            },
            JackCaptureHandler {
                left,
                right,
                producer,
                control,
                frame_clock: JackFrameClock::default(),
                latest_frame: Arc::clone(&latest_frame),
                has_latest_frame: Arc::clone(&has_latest_frame),
            },
        )?;

        let sources = active.as_client().ports(
            None,
            Some("32 bit float mono audio"),
            PortFlags::IS_OUTPUT | PortFlags::IS_PHYSICAL,
        );
        if sources.len() < 2 {
            return Err(JackInputError::NoStereoInput {
                found: sources.len(),
            });
        }
        active
            .as_client()
            .connect_ports_by_name(&sources[0], &left_name)?;
        active
            .as_client()
            .connect_ports_by_name(&sources[1], &right_name)?;
        update_reported_capture_latency(
            active.as_client(),
            &left_name,
            &right_name,
            &reported_capture_latency_frames,
        );
        Ok(Self {
            active: Some(active),
            sample_rate,
            reported_capture_latency_frames,
            latest_frame,
            has_latest_frame,
        })
    }

    /// Returns the device sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns a precise nonzero capture-path latency shared by both connected input ports.
    ///
    /// JACK ranges that are unset, ambiguous, or different between channels are treated as
    /// unavailable; the caller can still use the user's recording calibration offset.
    pub fn reported_capture_latency_frames(&self) -> Option<u32> {
        u32::try_from(self.reported_capture_latency_frames.load(Ordering::Acquire))
            .ok()
            .filter(|frames| *frames > 0)
    }

    /// Extends a frame position from the shared JACK server clock around the latest input frame.
    pub fn map_shared_frame_time(&self, frame: u32) -> Option<u64> {
        self.active.as_ref()?;
        if self.has_latest_frame.load(Ordering::Acquire) {
            JackFrameClock::extend_near(frame, self.latest_frame.load(Ordering::Relaxed))
        } else {
            // Without an input-side frame we cannot know whether this raw server frame belongs
            // just before or after a 32-bit wrap. The caller can retain its transport estimate.
            None
        }
    }

    /// Stops the callback and disconnects the JACK input ports.
    pub fn shutdown(&mut self) {
        drop(self.active.take());
    }
}

fn precise_shared_capture_latency(left: (u32, u32), right: (u32, u32)) -> Option<u32> {
    match (left, right) {
        ((left_min, left_max), (right_min, right_max))
            if left_min > 0
                && left_min == left_max
                && right_min == right_max
                && left_min == right_min =>
        {
            Some(left_min)
        }
        _ => None,
    }
}

impl Drop for JackAudioInput {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::{JackFrameClock, precise_shared_capture_latency};

    #[test]
    fn jack_capture_latency_requires_matching_precise_nonzero_channel_ranges() {
        assert_eq!(
            precise_shared_capture_latency((128, 128), (128, 128)),
            Some(128)
        );
        assert_eq!(precise_shared_capture_latency((0, 0), (0, 0)), None);
        assert_eq!(precise_shared_capture_latency((128, 256), (128, 256)), None);
        assert_eq!(precise_shared_capture_latency((128, 128), (256, 256)), None);
    }

    #[test]
    fn jack_frame_clock_extends_counter_wraparound() {
        let mut clock = JackFrameClock::default();
        assert_eq!(clock.extend(u32::MAX - 10), Some(u64::from(u32::MAX - 10)));
        assert_eq!(clock.extend(4), Some((u64::from(u32::MAX) + 1) + 4));
    }

    #[test]
    fn jack_frame_clock_rejects_small_regressions_and_old_epochs() {
        let mut clock = JackFrameClock::default();
        assert_eq!(clock.extend(10_000), Some(10_000));
        assert_eq!(clock.extend(9_999), None);
        assert_eq!(clock.extend(10_001), Some(10_001));

        let mut wrapped = JackFrameClock::default();
        assert_eq!(wrapped.extend(4), Some(4));
        assert_eq!(wrapped.extend(u32::MAX - 2), None);
    }

    #[test]
    fn jack_frame_clock_places_control_thread_time_in_the_latest_epoch() {
        let before_wrap = u64::from(u32::MAX - 2);
        assert_eq!(
            JackFrameClock::extend_near(u32::MAX, before_wrap),
            Some(before_wrap + 2)
        );

        let after_wrap = u64::from(u32::MAX) + 4;
        assert_eq!(
            JackFrameClock::extend_near(7, after_wrap),
            Some(after_wrap + 4)
        );
        assert_eq!(
            JackFrameClock::extend_near(u32::MAX - 1, after_wrap),
            Some(after_wrap - 5)
        );
        assert_eq!(JackFrameClock::extend_near(0, 0), Some(0));
    }
}
