/// The identifier of a track in a project.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TrackId(u64);

impl TrackId {
    pub(crate) fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// Returns the stable numeric value of this identifier.
    pub fn value(self) -> u64 {
        self.0
    }
}

/// A track in a project.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub(crate) id: TrackId,
    pub(crate) name: String,
    pub(crate) volume_db: f32,
    pub(crate) pan: f32,
}

impl Track {
    /// Returns this track's identifier.
    pub fn id(&self) -> TrackId {
        self.id
    }

    /// Returns this track's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns this track's volume in decibels.
    pub fn volume_db(&self) -> f32 {
        self.volume_db
    }

    /// Returns this track's pan position in the inclusive range `-1.0..=1.0`.
    pub fn pan(&self) -> f32 {
        self.pan
    }
}
