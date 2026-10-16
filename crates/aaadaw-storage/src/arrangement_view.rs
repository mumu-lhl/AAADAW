use std::fmt;

/// Arrangement, range, and transport state saved with a project.
///
/// Track and parameter identities are project-local. The application must only
/// restore entries whose track and FX instance still exist in the loaded project.
#[derive(Clone, Debug, PartialEq)]
pub struct ArrangementViewState {
    pub folder_compact: Vec<FolderCompactViewState>,
    pub volume_lanes: Vec<VolumeAutomationLaneViewState>,
    pub fx_lanes: Vec<FxAutomationLaneViewState>,
    /// Project-time positions in seconds from the project's sample origin.
    pub time_selection: Option<ArrangementTimeRangeState>,
    /// Loop points are independent of the time selection and may have zero length.
    pub loop_points: Option<ArrangementTimeRangeState>,
    /// Whether transport Repeat is enabled for this project.
    pub repeat_enabled: bool,
    /// Whether timeline elements should snap to the selected project grid.
    pub snap_enabled: bool,
    /// Arrangement project-grid division.
    pub snap_grid: GridDivision,
    /// MIDI Editor grid division.
    pub midi_editor_snap_grid: GridDivision,
    /// Whether odd project-grid points are shifted by the configured amount.
    pub swing_grid_enabled: bool,
    /// Swing offset, in percent of half a grid interval.
    pub swing_amount_percent: i16,
}

impl Default for ArrangementViewState {
    fn default() -> Self {
        Self {
            folder_compact: Vec::new(),
            volume_lanes: Vec::new(),
            fx_lanes: Vec::new(),
            time_selection: None,
            loop_points: None,
            repeat_enabled: false,
            snap_enabled: true,
            snap_grid: GridDivision::Quarter,
            midi_editor_snap_grid: GridDivision::Sixteenth,
            swing_grid_enabled: false,
            swing_amount_percent: 50,
        }
    }
}

/// A note-value division used by Arrangement and MIDI Editor snapping.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(i64)]
pub enum GridDivision {
    Whole = 0,
    Half = 1,
    #[default]
    Quarter = 2,
    Eighth = 3,
    Sixteenth = 4,
    ThirtySecond = 5,
    DottedHalf = 6,
    DottedQuarter = 7,
    DottedEighth = 8,
    DottedSixteenth = 9,
    HalfTriplet = 10,
    QuarterTriplet = 11,
    EighthTriplet = 12,
    SixteenthTriplet = 13,
    ThirtySecondTriplet = 14,
}

impl GridDivision {
    pub const ALL: [Self; 15] = [
        Self::Whole,
        Self::Half,
        Self::Quarter,
        Self::Eighth,
        Self::Sixteenth,
        Self::ThirtySecond,
        Self::DottedHalf,
        Self::DottedQuarter,
        Self::DottedEighth,
        Self::DottedSixteenth,
        Self::HalfTriplet,
        Self::QuarterTriplet,
        Self::EighthTriplet,
        Self::SixteenthTriplet,
        Self::ThirtySecondTriplet,
    ];

    pub fn tick_interval(self, ppq: u32) -> Option<u64> {
        let (numerator, denominator) = self.ratio();
        let ticks = u64::from(ppq).checked_mul(numerator)?;
        (ticks % denominator == 0)
            .then_some(ticks / denominator)
            .filter(|ticks| *ticks > 0)
    }

    pub(crate) fn storage_id(self) -> i64 {
        self as i64
    }

    pub(crate) fn from_storage_id(id: i64) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|division| division.storage_id() == id)
    }

    fn ratio(self) -> (u64, u64) {
        match self {
            Self::Whole => (4, 1),
            Self::Half => (2, 1),
            Self::Quarter => (1, 1),
            Self::Eighth => (1, 2),
            Self::Sixteenth => (1, 4),
            Self::ThirtySecond => (1, 8),
            Self::DottedHalf => (3, 1),
            Self::DottedQuarter => (3, 2),
            Self::DottedEighth => (3, 4),
            Self::DottedSixteenth => (3, 8),
            Self::HalfTriplet => (4, 3),
            Self::QuarterTriplet => (2, 3),
            Self::EighthTriplet => (1, 3),
            Self::SixteenthTriplet => (1, 6),
            Self::ThirtySecondTriplet => (1, 12),
        }
    }
}

impl fmt::Display for GridDivision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Whole => "1/1",
            Self::Half => "1/2",
            Self::Quarter => "1/4",
            Self::Eighth => "1/8",
            Self::Sixteenth => "1/16",
            Self::ThirtySecond => "1/32",
            Self::DottedHalf => "1/2 dotted",
            Self::DottedQuarter => "1/4 dotted",
            Self::DottedEighth => "1/8 dotted",
            Self::DottedSixteenth => "1/16 dotted",
            Self::HalfTriplet => "1/2 triplet",
            Self::QuarterTriplet => "1/4 triplet",
            Self::EighthTriplet => "1/8 triplet",
            Self::SixteenthTriplet => "1/16 triplet",
            Self::ThirtySecondTriplet => "1/32 triplet",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArrangementTimeRangeState {
    pub start_seconds: f64,
    pub end_seconds: f64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FolderCompactViewState {
    pub track_id: u64,
    /// 0: normal, 1: small children, 2: tiny children.
    pub mode: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VolumeAutomationLaneViewState {
    pub track_id: u64,
    pub visible: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FxAutomationLaneViewState {
    pub track_id: u64,
    pub chain_index: usize,
    pub plugin_id: String,
    pub bundle_path: String,
    pub parameter_id: u32,
    pub name: String,
    pub value_range: (f64, f64),
    pub stepped: bool,
    pub height: f32,
}
