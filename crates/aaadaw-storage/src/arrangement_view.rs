/// Arrangement, range, and transport state saved with a project.
///
/// Track and parameter identities are project-local. The application must only
/// restore entries whose track and FX instance still exist in the loaded project.
#[derive(Clone, Debug, Default, PartialEq)]
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
