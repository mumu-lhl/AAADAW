use crate::{MidiNoteData, ProjectSettings, TempoCurve, TimebaseError};
use std::fmt;

/// A validated, serialization-friendly copy of project state.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectSnapshot {
    pub settings: ProjectSettings,
    pub tracks: Vec<TrackSnapshot>,
    pub audio_items: Vec<AudioItemSnapshot>,
    pub midi_items: Vec<MidiItemSnapshot>,
    pub tempo_points: Vec<TempoPointSnapshot>,
    pub meter_points: Vec<MeterPointSnapshot>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrackSnapshot {
    pub id: u64,
    pub name: String,
    pub volume_db: f32,
    pub pan: f32,
    pub muted: bool,
    pub solo: bool,
    pub record_armed: bool,
    pub instrument: Option<TrackInstrumentSnapshot>,
    pub fx_chain: Vec<TrackFxPluginSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackInstrumentSnapshot {
    pub plugin_id: String,
    pub bundle_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackFxPluginSnapshot {
    /// Stable identifier reported by the CLAP plugin.
    pub plugin_id: String,
    /// Local path to the CLAP entry library or bundle.
    pub bundle_path: String,
    /// Whether the plugin is enabled in the chain.
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioItemSnapshot {
    pub id: u64,
    pub track_id: u64,
    pub media_ref: String,
    pub start_sample: u64,
    pub source_offset_samples: u64,
    pub length_samples: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MidiItemSnapshot {
    pub id: u64,
    pub track_id: u64,
    pub start_tick: u64,
    pub length_ticks: u64,
    pub notes: Vec<MidiNoteSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MidiNoteSnapshot {
    pub id: u64,
    pub data: MidiNoteData,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempoPointSnapshot {
    pub start_tick: u64,
    pub bpm: f64,
    pub curve_to_next: TempoCurve,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeterPointSnapshot {
    pub start_tick: u64,
    pub numerator: u32,
    pub denominator: u32,
}

/// Project data could not be restored from a snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnapshotError {
    /// A stored ID is exhausted and cannot be advanced for the next new object.
    IdentifierExhausted,
    /// Project data violates domain invariants.
    InvalidProjectData,
    /// The snapshot contains an invalid timebase map.
    InvalidTimebase(TimebaseError),
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdentifierExhausted => formatter.write_str("project identifiers are exhausted"),
            Self::InvalidProjectData => formatter.write_str("project snapshot data is invalid"),
            Self::InvalidTimebase(error) => write!(formatter, "invalid project timebase: {error}"),
        }
    }
}

impl std::error::Error for SnapshotError {}
