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
    pub(crate) muted: bool,
    pub(crate) solo: bool,
    pub(crate) record_armed: bool,
    pub(crate) instrument: Option<TrackInstrument>,
    pub(crate) fx_chain: Vec<TrackFxPlugin>,
}

/// A CLAP instrument selected for a track. The bundle path is a load hint;
/// `plugin_id` is checked against the loaded plugin before processing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackInstrument {
    plugin_id: String,
    bundle_path: String,
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
}

/// A CLAP plugin reference inserted in a track's ordered FX chain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackFxPlugin {
    plugin_id: String,
    bundle_path: String,
    enabled: bool,
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
            plugin_id,
            bundle_path,
            enabled: true,
        })
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

    /// Returns a copy with the requested enabled state.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
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
}
