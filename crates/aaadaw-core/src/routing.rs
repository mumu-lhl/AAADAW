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

/// Independent post-fader audio send controls.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AudioSendParameters {
    pub volume_db: f32,
    pub pan: f32,
    pub muted: bool,
    pub phase_inverted: bool,
}
impl AudioSendParameters {
    pub(crate) fn is_valid(self) -> bool {
        self.volume_db.is_finite()
            && 10.0_f32.powf(self.volume_db / 20.0).is_finite()
            && self.pan.is_finite()
            && (-1.0..=1.0).contains(&self.pan)
    }
}

/// A post-fader connection owned by its source; receives are derived views.
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
