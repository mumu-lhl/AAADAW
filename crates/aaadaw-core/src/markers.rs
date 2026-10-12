use crate::timebase::MAX_EXACT_FLOAT_POSITION;

/// The stable identity of a project marker.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MarkerId(u64);

impl MarkerId {
    pub(crate) fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// Returns this marker's stable numeric identity.
    pub fn value(self) -> u64 {
        self.0
    }
}

/// A named point on the project timeline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectMarker {
    pub(crate) id: MarkerId,
    pub(crate) position_ticks: u64,
    pub(crate) name: String,
}

impl ProjectMarker {
    pub(crate) fn new(id: MarkerId, position_ticks: u64, name: String) -> Option<Self> {
        (position_ticks <= MAX_EXACT_FLOAT_POSITION && valid_marker_name(&name)).then_some(Self {
            id,
            position_ticks,
            name,
        })
    }

    /// Returns the marker's stable identity.
    pub fn id(&self) -> MarkerId {
        self.id
    }

    /// Returns the marker's absolute project position in ticks.
    pub fn position_ticks(&self) -> u64 {
        self.position_ticks
    }

    /// Returns the marker's display name. An empty name uses its automatic timeline number.
    pub fn name(&self) -> &str {
        &self.name
    }
}

pub(crate) fn valid_marker_name(name: &str) -> bool {
    name.len() <= 1024 && !name.contains('\0')
}
