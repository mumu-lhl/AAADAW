use crate::TrackId;

/// Stable identity of one sender-owned audio connection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SendId(u64);
impl SendId {
    /// Reconstructs a persisted send identity.
    pub fn from_value(value: u64) -> Self {
        Self(value)
    }
    /// Returns the stable numeric identity.
    pub fn value(self) -> u64 {
        self.0
    }
}

/// Signal stage tapped by an audio send.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AudioSendTap {
    #[default]
    PostFader,
    PreFx,
    PreFader,
}

/// REAPER's hardware-output send mode. Mode 2 is retained for compatibility with projects that
/// contain the deprecated Post-FX value; mode 3 is its current equivalent.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AudioHardwareOutputSendMode {
    #[default]
    PostFader,
    PreFx,
    DeprecatedPostFx,
    PostFx,
}
impl AudioHardwareOutputSendMode {
    /// Returns the native `I_SENDMODE` value used by REAPER.
    pub const fn native_value(self) -> u8 {
        match self {
            Self::PostFader => 0,
            Self::PreFx => 1,
            Self::DeprecatedPostFx => 2,
            Self::PostFx => 3,
        }
    }

    /// Reconstructs one of REAPER's supported hardware-output `I_SENDMODE` values.
    pub const fn from_native_value(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::PostFader),
            1 => Some(Self::PreFx),
            2 => Some(Self::DeprecatedPostFx),
            3 => Some(Self::PostFx),
            _ => None,
        }
    }
}

/// Source channel selection for an audio send.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AudioSendSourceChannels {
    /// Send source channels 1/2 as a stereo pair.
    #[default]
    Stereo,
    /// Send source channel 1 as mono.
    Channel1,
    /// Send source channel 2 as mono.
    Channel2,
}

/// Destination channel selection for an audio send.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AudioSendDestinationChannels {
    /// Send to destination channels 1/2 as a stereo pair.
    #[default]
    Stereo,
    /// Send the mono sum to destination channel 1.
    Channel1,
    /// Send the mono sum to destination channel 2.
    Channel2,
}

/// Independent audio send controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioSendParameters {
    pub tap: AudioSendTap,
    pub source_channels: AudioSendSourceChannels,
    pub destination_channels: AudioSendDestinationChannels,
    pub volume_db: f32,
    pub pan: f32,
    pub muted: bool,
    pub phase_inverted: bool,
    /// Forward the source track's MIDI events through this send.
    pub midi_enabled: bool,
    /// Zero-based source MIDI channel, or `None` to pass every channel.
    pub midi_source_channel: Option<u8>,
    /// Zero-based destination MIDI channel, or `None` to preserve each event's channel.
    pub midi_destination_channel: Option<u8>,
    /// Zero-based source MIDI bus, or `None` to accept every bus.
    pub midi_source_bus: Option<u8>,
    /// Zero-based destination MIDI bus, or `None` to preserve each event's bus.
    pub midi_destination_bus: Option<u8>,
}
impl Default for AudioSendParameters {
    fn default() -> Self {
        Self {
            tap: AudioSendTap::default(),
            source_channels: AudioSendSourceChannels::default(),
            destination_channels: AudioSendDestinationChannels::default(),
            volume_db: 0.0,
            pan: 0.0,
            muted: false,
            phase_inverted: false,
            // REAPER creates ordinary sends with MIDI set to All -> All.
            midi_enabled: true,
            midi_source_channel: None,
            midi_destination_channel: None,
            midi_source_bus: None,
            midi_destination_bus: None,
        }
    }
}
impl AudioSendParameters {
    pub(crate) fn is_valid(self) -> bool {
        self.volume_db.is_finite()
            && 10.0_f32.powf(self.volume_db / 20.0).is_finite()
            && self.pan.is_finite()
            && (-1.0..=1.0).contains(&self.pan)
            && self.midi_source_channel.is_none_or(|channel| channel < 16)
            && self
                .midi_destination_channel
                .is_none_or(|channel| channel < 16)
            && self.midi_source_bus.is_none_or(|bus| bus < 16)
            && self.midi_destination_bus.is_none_or(|bus| bus < 16)
    }
}

/// Independent controls for an audio connection to the active device output.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioHardwareOutputParameters {
    pub send_mode: AudioHardwareOutputSendMode,
    pub source_channels: AudioSendSourceChannels,
    pub destination_channels: AudioSendDestinationChannels,
    pub volume_db: f32,
    pub pan: f32,
    pub muted: bool,
    pub phase_inverted: bool,
}
impl Default for AudioHardwareOutputParameters {
    fn default() -> Self {
        Self {
            send_mode: AudioHardwareOutputSendMode::default(),
            source_channels: AudioSendSourceChannels::default(),
            destination_channels: AudioSendDestinationChannels::default(),
            volume_db: 0.0,
            pan: 0.0,
            muted: false,
            phase_inverted: false,
        }
    }
}
impl AudioHardwareOutputParameters {
    pub(crate) fn is_valid(self) -> bool {
        self.volume_db.is_finite()
            && 10.0_f32.powf(self.volume_db / 20.0).is_finite()
            && self.pan.is_finite()
            && (-1.0..=1.0).contains(&self.pan)
    }
}

/// A connection owned by its source; receives are derived views.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioSend {
    pub(crate) id: SendId,
    pub(crate) destination: TrackId,
    pub(crate) parameters: AudioSendParameters,
}
impl AudioSend {
    pub fn id(&self) -> SendId {
        self.id
    }
    pub fn destination(&self) -> TrackId {
        self.destination
    }
    pub fn parameters(&self) -> AudioSendParameters {
        self.parameters
    }
}

/// Serialization-friendly sender-owned audio connection.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioSendSnapshot {
    pub id: u64,
    pub destination_track_id: u64,
    pub parameters: AudioSendParameters,
}

/// A direct connection from a track to the active audio device output.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioHardwareOutput {
    pub(crate) id: SendId,
    pub(crate) parameters: AudioHardwareOutputParameters,
}
impl AudioHardwareOutput {
    pub fn id(&self) -> SendId {
        self.id
    }

    pub fn parameters(&self) -> AudioHardwareOutputParameters {
        self.parameters
    }
}

/// Serialization-friendly direct connection to the active audio device output.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioHardwareOutputSnapshot {
    pub id: u64,
    pub parameters: AudioHardwareOutputParameters,
}
