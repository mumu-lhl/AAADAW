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
}

impl ProcessHandler for JackCaptureHandler {
    fn process(&mut self, _client: &Client, scope: &ProcessScope) -> Control {
        let left = self.left.as_slice(scope);
        let right = self.right.as_slice(scope);
        if left.len() != right.len() {
            self.control.fail();
            return Control::Continue;
        }
        self.producer.push_planar(left, right);
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
