use crate::timebase::MAX_EXACT_FLOAT_POSITION;

/// Largest REAPER-compatible positive project marker number.
pub const MAX_PROJECT_MARKER_NUMBER: u32 = i32::MAX as u32;

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
    pub(crate) number: u32,
    pub(crate) position_ticks: u64,
    pub(crate) name: String,
    pub(crate) visible: bool,
}

impl ProjectMarker {
    pub(crate) fn new(
        id: MarkerId,
        number: u32,
        position_ticks: u64,
        name: String,
    ) -> Option<Self> {
        (number > 0
            && number <= MAX_PROJECT_MARKER_NUMBER
            && position_ticks <= MAX_EXACT_FLOAT_POSITION
            && valid_marker_name(&name))
        .then_some(Self {
            id,
            number,
            position_ticks,
            name,
            visible: true,
        })
    }

    /// Returns the marker's stable identity.
    pub fn id(&self) -> MarkerId {
        self.id
    }

    /// Returns the REAPER-compatible marker number shown in rulers and dialogs.
    pub fn number(&self) -> u32 {
        self.number
    }

    /// Returns the marker's absolute project position in ticks.
    pub fn position_ticks(&self) -> u64 {
        self.position_ticks
    }

    /// Returns the marker's display name. An empty name uses its automatic timeline number.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether this marker is visible in the project ruler.
    pub fn is_visible(&self) -> bool {
        self.visible
    }
}

pub(crate) fn valid_marker_name(name: &str) -> bool {
    name.len() <= 1024 && !name.contains('\0')
}
