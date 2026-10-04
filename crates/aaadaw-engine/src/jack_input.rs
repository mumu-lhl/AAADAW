use crate::{AudioCaptureControl, AudioCaptureProducer};
use jack::{
    AudioIn, Client, ClientOptions, Control, Port, PortFlags, ProcessHandler, ProcessScope,
};
use std::error::Error as StdError;
use std::fmt;

struct JackCaptureHandler {
    left: Port<AudioIn>,
    right: Port<AudioIn>,
    producer: AudioCaptureProducer,
    control: AudioCaptureControl,
    frame_clock: JackFrameClock,
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
    active: Option<jack::AsyncClient<(), JackCaptureHandler>>,
    sample_rate: u32,
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
        let active = client.activate_async(
            (),
            JackCaptureHandler {
                left,
                right,
                producer,
                control,
                frame_clock: JackFrameClock::default(),
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
        Ok(Self {
            active: Some(active),
            sample_rate,
        })
    }

    /// Returns the device sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Stops the callback and disconnects the JACK input ports.
    pub fn shutdown(&mut self) {
        drop(self.active.take());
    }
}

impl Drop for JackAudioInput {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::JackFrameClock;

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
}
