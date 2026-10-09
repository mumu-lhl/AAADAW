use std::sync::atomic::{AtomicU64, Ordering};

/// Maximum number of independently automated parameters accepted for one track FX instance.
/// This keeps per-block playback scheduling bounded even for malformed project files.
pub const MAX_TRACK_FX_PARAMETER_AUTOMATION_LANES: usize = 256;

static NEXT_FX_PLUGIN_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

/// The identifier of a track in a project.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TrackId(u64);

impl TrackId {
    pub(crate) fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// Reconstructs a nonzero stable track identifier from a serialized or IPC value.
    pub fn from_value(value: u64) -> Option<Self> {
        (value != 0).then_some(Self(value))
    }

    /// Returns the stable numeric value of this identifier.
    pub fn value(self) -> u64 {
        self.0
    }
}

/// One sample-clock point in a track's read-mode volume automation lane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VolumeAutomationPoint {
    sample: u64,
    gain_db: f32,
}

impl VolumeAutomationPoint {
    /// Creates a volume automation point with gain between -60 and +6 dB.
    pub fn new(sample: u64, gain_db: f32) -> Option<Self> {
        (gain_db.is_finite() && (-60.0..=6.0).contains(&gain_db))
            .then_some(Self { sample, gain_db })
    }

    /// Returns the absolute project sample at which this point occurs.
    pub fn sample(self) -> u64 {
        self.sample
    }

    /// Returns the point's gain in decibels.
    pub fn gain_db(self) -> f32 {
        self.gain_db
    }
}

/// One sample-clock value in a CLAP effect parameter automation lane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FxParameterAutomationPoint {
    sample: u64,
    value_bits: u64,
}

impl FxParameterAutomationPoint {
    /// Creates a finite CLAP parameter value at an absolute project sample.
    pub fn new(sample: u64, value: f64) -> Option<Self> {
        value.is_finite().then_some(Self {
            sample,
            value_bits: value.to_bits(),
        })
    }

    /// Returns the absolute project sample at which this value occurs.
    pub fn sample(self) -> u64 {
        self.sample
    }

    /// Returns the plugin parameter value.
    pub fn value(self) -> f64 {
        f64::from_bits(self.value_bits)
    }
}

/// A sample-ordered automation lane owned by one CLAP FX parameter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FxParameterAutomationLane {
    parameter_id: u32,
    points: Vec<FxParameterAutomationPoint>,
}

impl FxParameterAutomationLane {
    /// Creates a non-empty lane with strictly increasing sample positions.
    pub fn new(parameter_id: u32, points: Vec<FxParameterAutomationPoint>) -> Option<Self> {
        (!points.is_empty()
            && points.iter().all(|point| point.value().is_finite())
            && points
                .windows(2)
                .all(|pair| pair[0].sample < pair[1].sample))
        .then_some(Self {
            parameter_id,
            points,
        })
    }

    /// Returns the CLAP parameter ID controlled by this lane.
    pub fn parameter_id(&self) -> u32 {
        self.parameter_id
    }

    /// Returns points ordered by absolute project sample.
    pub fn points(&self) -> &[FxParameterAutomationPoint] {
        &self.points
    }
}

/// A track in a project.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub(crate) id: TrackId,
    pub(crate) name: String,
    pub(crate) is_bus: bool,
    pub(crate) is_folder: bool,
    pub(crate) parent_track: Option<TrackId>,
    pub(crate) output_track: Option<TrackId>,
    pub(crate) main_send_enabled: bool,
    pub(crate) sends: Vec<crate::AudioSend>,
    pub(crate) volume_db: f32,
    pub(crate) pan: f32,
    pub(crate) pan_mode: crate::PanMode,
    pub(crate) muted: bool,
    pub(crate) solo: bool,
    pub(crate) record_armed: bool,
    pub(crate) instrument: Option<TrackInstrument>,
    pub(crate) fx_chain: Vec<TrackFxPlugin>,
    pub(crate) volume_automation: Vec<VolumeAutomationPoint>,
    pub(crate) frozen_audio_item_id: Option<crate::ItemId>,
}

/// A CLAP instrument selected for a track. The bundle path is a load hint;
/// `plugin_id` is checked against the loaded plugin before processing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackInstrument {
    plugin_id: String,
    bundle_path: String,
    state: Option<Vec<u8>>,
}

impl TrackInstrument {
    /// Creates a reference to a CLAP plugin using its stable ID and bundle path.
    pub fn new(plugin_id: impl Into<String>, bundle_path: impl Into<String>) -> Option<Self> {
        let plugin_id = plugin_id.into();
        let bundle_path = bundle_path.into();
        if plugin_id.trim().is_empty() || bundle_path.trim().is_empty() {
            return None;
        }
        Some(Self {
            plugin_id,
            bundle_path,
            state: None,
        })
    }

    /// Returns the plugin's stable CLAP identifier.
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    /// Returns the bundle path used by the host to locate this plugin.
    pub fn bundle_path(&self) -> &str {
        &self.bundle_path
    }

    /// Returns the last persisted opaque CLAP state, if the plugin supports state serialization.
    pub fn state(&self) -> Option<&[u8]> {
        self.state.as_deref()
    }

    /// Returns a copy carrying the plugin's serialized state.
    pub fn with_state(mut self, state: Option<Vec<u8>>) -> Self {
        self.state = state;
        self
    }
}

/// A CLAP plugin reference inserted in a track's ordered FX chain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackFxPlugin {
    instance_id: u64,
    plugin_id: String,
    bundle_path: String,
    enabled: bool,
    state: Option<Vec<u8>>,
    parameter_values: Vec<(u32, u64)>,
    parameter_automation: Vec<FxParameterAutomationLane>,
}

impl TrackFxPlugin {
    /// Creates an enabled plugin reference for a track FX chain.
    pub fn new(plugin_id: impl Into<String>, bundle_path: impl Into<String>) -> Option<Self> {
        let plugin_id = plugin_id.into();
        let bundle_path = bundle_path.into();
        if plugin_id.trim().is_empty() || bundle_path.trim().is_empty() {
            return None;
        }
        Some(Self {
            instance_id: NEXT_FX_PLUGIN_INSTANCE_ID.fetch_add(1, Ordering::Relaxed),
            plugin_id,
            bundle_path,
            enabled: true,
            state: None,
            parameter_values: Vec::new(),
            parameter_automation: Vec::new(),
        })
    }

    /// Returns the identity of this inserted plugin instance. Clones retain it across edits and undo history.
    pub fn instance_id(&self) -> u64 {
        self.instance_id
    }

    /// Returns this plugin's stable CLAP identifier.
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    /// Returns the CLAP entry path used to load this plugin.
    pub fn bundle_path(&self) -> &str {
        &self.bundle_path
    }

    /// Returns whether this plugin currently participates in processing.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the last persisted opaque CLAP state, if the plugin supports state serialization.
    pub fn state(&self) -> Option<&[u8]> {
        self.state.as_deref()
    }

    /// Returns a copy with the requested enabled state.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Returns a copy carrying the plugin's serialized state.
    pub fn with_state(mut self, state: Option<Vec<u8>>) -> Self {
        self.state = state;
        self
    }

    /// Returns the host's stored value for a CLAP parameter, when available.
    pub fn parameter_value(&self, id: u32) -> Option<f64> {
        self.parameter_values
            .iter()
            .find(|(parameter_id, _)| *parameter_id == id)
            .map(|(_, value)| f64::from_bits(*value))
    }

    /// Returns all host parameter values for snapshot and plugin restoration.
    pub fn parameter_values(&self) -> impl Iterator<Item = (u32, f64)> + '_ {
        self.parameter_values
            .iter()
            .map(|(parameter_id, bits)| (*parameter_id, f64::from_bits(*bits)))
    }

    /// Returns the plugin's sample-clock parameter automation lanes in parameter-ID order.
    pub fn parameter_automation(&self) -> &[FxParameterAutomationLane] {
        &self.parameter_automation
    }

    pub fn parameter_automation_for(&self, id: u32) -> Option<&FxParameterAutomationLane> {
        self.parameter_automation
            .binary_search_by_key(&id, FxParameterAutomationLane::parameter_id)
            .ok()
            .map(|index| &self.parameter_automation[index])
    }

    pub(crate) fn with_parameter_value(mut self, id: u32, value: f64) -> Self {
        match self
            .parameter_values
            .binary_search_by_key(&id, |(parameter_id, _)| *parameter_id)
        {
            Ok(index) => self.parameter_values[index].1 = value.to_bits(),
            Err(index) => self.parameter_values.insert(index, (id, value.to_bits())),
        }
        self
    }

    pub(crate) fn with_parameter_automation(
        mut self,
        parameter_id: u32,
        lane: Option<FxParameterAutomationLane>,
    ) -> Self {
        match self
            .parameter_automation
            .binary_search_by_key(&parameter_id, FxParameterAutomationLane::parameter_id)
        {
            Ok(index) => match lane {
                Some(lane) => self.parameter_automation[index] = lane,
                None => {
                    self.parameter_automation.remove(index);
                }
            },
            Err(index) => {
                if let Some(lane) = lane {
                    self.parameter_automation.insert(index, lane);
                }
            }
        }
        self
    }
}

impl Track {
    /// Whether the existing main output (Master or selected track) is enabled.
    pub fn main_send_enabled(&self) -> bool {
        self.main_send_enabled
    }
    /// Ordered sender-owned post-fader audio connections.
    pub fn sends(&self) -> &[crate::AudioSend] {
        &self.sends
    }

    /// Returns the inherited project gain policy.
    pub fn pan_mode(&self) -> crate::PanMode {
        self.pan_mode
    }

    /// Returns this track's identifier.
    pub fn id(&self) -> TrackId {
        self.id
    }

    /// Returns this track's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns whether this track is a subgroup bus destination.
    pub fn is_bus(&self) -> bool {
        self.is_bus
    }

    /// Returns this track's bus output, or `None` when routed directly to Master.
    pub fn output_track(&self) -> Option<TrackId> {
        self.output_track
    }

    pub fn is_folder(&self) -> bool {
        self.is_folder
    }

    pub fn parent_track(&self) -> Option<TrackId> {
        self.parent_track
    }

    /// Main output follows the folder parent unless an explicit output is set.
    pub fn effective_output_track(&self) -> Option<TrackId> {
        self.output_track.or(self.parent_track)
    }

    /// Returns this track's volume in decibels.
    pub fn volume_db(&self) -> f32 {
        self.volume_db
    }

    /// Returns this track's pan position in the inclusive range `-1.0..=1.0`.
    pub fn pan(&self) -> f32 {
        self.pan
    }

    /// Returns whether this track is muted.
    pub fn is_muted(&self) -> bool {
        self.muted
    }

    /// Returns whether this track is soloed.
    pub fn is_solo(&self) -> bool {
        self.solo
    }

    /// Returns whether this track is armed to receive the next audio take.
    pub fn is_record_armed(&self) -> bool {
        self.record_armed
    }

    /// Returns this track's optional CLAP instrument reference.
    pub fn instrument(&self) -> Option<&TrackInstrument> {
        self.instrument.as_ref()
    }

    /// Returns the ordered CLAP FX chain for this track.
    pub fn fx_chain(&self) -> &[TrackFxPlugin] {
        &self.fx_chain
    }

    /// Returns this track's ordered sample-clock volume automation points.
    pub fn volume_automation(&self) -> &[VolumeAutomationPoint] {
        &self.volume_automation
    }

    /// Returns the frozen render item while this track is frozen.
    pub fn frozen_audio_item_id(&self) -> Option<crate::ItemId> {
        self.frozen_audio_item_id
    }

    /// Returns whether playback uses this track's frozen render instead of its live source chain.
    pub fn is_frozen(&self) -> bool {
        self.frozen_audio_item_id.is_some()
    }
}
