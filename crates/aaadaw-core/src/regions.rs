use crate::timebase::MAX_EXACT_FLOAT_POSITION;

/// Largest positive project region number supported by the current storage model.
pub const MAX_PROJECT_REGION_NUMBER: u32 = i32::MAX as u32;

/// The stable identity of a project region.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RegionId(u64);

impl RegionId {
    pub(crate) fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// Returns this region's stable numeric identity.
    pub fn value(self) -> u64 {
        self.0
    }
}

/// A named, bounded range on the project timeline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRegion {
    pub(crate) id: RegionId,
    pub(crate) number: u32,
    pub(crate) start_ticks: u64,
    pub(crate) end_ticks: u64,
    pub(crate) name: String,
}

impl ProjectRegion {
    pub(crate) fn new(
        id: RegionId,
        number: u32,
        start_ticks: u64,
        end_ticks: u64,
        name: String,
    ) -> Option<Self> {
        (number > 0
            && number <= MAX_PROJECT_REGION_NUMBER
            && start_ticks < end_ticks
            && end_ticks <= MAX_EXACT_FLOAT_POSITION
            && crate::markers::valid_marker_name(&name))
        .then_some(Self {
            id,
            number,
            start_ticks,
            end_ticks,
            name,
        })
    }

    /// Returns this region's stable identity.
    pub fn id(&self) -> RegionId {
        self.id
    }

    /// Returns the visible region number shown in rulers and dialogs.
    pub fn number(&self) -> u32 {
        self.number
    }

    /// Returns the region's timeline start boundary in ticks.
    pub fn start_ticks(&self) -> u64 {
        self.start_ticks
    }

    /// Returns the region's timeline end boundary in ticks.
    pub fn end_ticks(&self) -> u64 {
        self.end_ticks
    }

    /// Returns the region's display name.
    pub fn name(&self) -> &str {
        &self.name
    }
}
