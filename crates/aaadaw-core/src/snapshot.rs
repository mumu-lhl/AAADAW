use crate::{
    FxParameterAutomationLane, FxParameterAutomationPoint, MidiChannelPressureData,
    MidiControllerData, MidiNoteData, MidiOpaqueEventData, MidiPitchBendData,
    MidiPolyAftertouchData, MidiProgramChangeData, ProjectMarker, ProjectRegion, ProjectSettings,
    RazorEditArea, TempoCurve, TimebaseError, VolumeAutomationPoint,
};
use std::fmt;

/// A validated, serialization-friendly copy of project state.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectSnapshot {
    pub notes: crate::ProjectNotes,
    pub settings: ProjectSettings,
    pub master_mix: crate::MasterMix,
    pub tracks: Vec<TrackSnapshot>,
    pub audio_items: Vec<AudioItemSnapshot>,
    pub midi_items: Vec<MidiItemSnapshot>,
    pub markers: Vec<ProjectMarkerSnapshot>,
    pub show_project_markers: bool,
    pub show_project_regions: bool,
    pub regions: Vec<ProjectRegionSnapshot>,
    pub tempo_points: Vec<TempoPointSnapshot>,
    pub meter_points: Vec<MeterPointSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectMarkerSnapshot {
    pub id: u64,
    pub number: u32,
    pub position_ticks: u64,
    pub name: String,
    pub visible: bool,
}

impl From<&ProjectMarker> for ProjectMarkerSnapshot {
    fn from(marker: &ProjectMarker) -> Self {
        Self {
            id: marker.id().value(),
            number: marker.number(),
            position_ticks: marker.position_ticks(),
            name: marker.name().to_owned(),
            visible: marker.is_visible(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRegionSnapshot {
    pub id: u64,
    pub number: u32,
    pub start_ticks: u64,
    pub end_ticks: u64,
    pub name: String,
    pub visible: bool,
    pub render_master_mix: bool,
    pub render_track_ids: Vec<u64>,
}

impl From<&ProjectRegion> for ProjectRegionSnapshot {
    fn from(region: &ProjectRegion) -> Self {
        Self {
            id: region.id().value(),
            number: region.number(),
            start_ticks: region.start_ticks(),
            end_ticks: region.end_ticks(),
            name: region.name().to_owned(),
            visible: region.is_visible(),
            render_master_mix: region.renders_master_mix(),
            render_track_ids: region
                .render_track_ids()
                .iter()
                .map(|track_id| track_id.value())
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrackSnapshot {
    pub id: u64,
    pub name: String,
    pub is_bus: bool,
    pub is_folder: bool,
    pub parent_track_id: Option<u64>,
    pub output_track_id: Option<u64>,
    pub main_send_enabled: bool,
    pub sends: Vec<crate::AudioSendSnapshot>,
    pub volume_db: f32,
    pub pan: f32,
    pub muted: bool,
    pub phase_inverted: bool,
    pub solo: bool,
    pub record_armed: bool,
    pub instrument: Option<TrackInstrumentSnapshot>,
    pub fx_chain: Vec<TrackFxPluginSnapshot>,
    pub volume_automation: Vec<VolumeAutomationPoint>,
    pub razor_edit_areas: Vec<RazorEditArea>,
    pub frozen_audio_item_id: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackInstrumentSnapshot {
    pub plugin_id: String,
    pub bundle_path: String,
    pub state: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrackFxPluginSnapshot {
    /// Stable identifier reported by the CLAP plugin.
    pub plugin_id: String,
    /// Local path to the CLAP entry library or bundle.
    pub bundle_path: String,
    /// Whether the plugin is enabled in the chain.
    pub enabled: bool,
    pub state: Option<Vec<u8>>,
    /// Last host parameter values, retained independently of the optional CLAP State extension.
    pub parameter_values: Vec<TrackFxParameterValueSnapshot>,
    pub parameter_automation: Vec<TrackFxParameterAutomationLaneSnapshot>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrackFxParameterAutomationLaneSnapshot {
    pub parameter_id: u32,
    pub points: Vec<TrackFxParameterAutomationPointSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackFxParameterAutomationPointSnapshot {
    pub sample: u64,
    pub value: f64,
}

impl From<&FxParameterAutomationLane> for TrackFxParameterAutomationLaneSnapshot {
    fn from(lane: &FxParameterAutomationLane) -> Self {
        Self {
            parameter_id: lane.parameter_id(),
            points: lane
                .points()
                .iter()
                .map(|point| TrackFxParameterAutomationPointSnapshot {
                    sample: point.sample(),
                    value: point.value(),
                })
                .collect(),
        }
    }
}

impl TryFrom<TrackFxParameterAutomationLaneSnapshot> for FxParameterAutomationLane {
    type Error = SnapshotError;

    fn try_from(lane: TrackFxParameterAutomationLaneSnapshot) -> Result<Self, Self::Error> {
        let points = lane
            .points
            .into_iter()
            .map(|point| {
                FxParameterAutomationPoint::new(point.sample, point.value)
                    .ok_or(SnapshotError::InvalidProjectData)
            })
            .collect::<Result<Vec<_>, _>>()?;
        FxParameterAutomationLane::new(lane.parameter_id, points)
            .ok_or(SnapshotError::InvalidProjectData)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackFxParameterValueSnapshot {
    pub parameter_id: u32,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioItemSnapshot {
    pub fades: crate::AudioItemFades,
    pub volume: f32,
    pub take_volume: f32,
    pub take_pan: f32,
    pub take_playback_rate: f64,
    pub id: u64,
    pub track_id: u64,
    pub media_ref: String,
    pub name: String,
    pub start_sample: u64,
    pub source_offset_samples: i64,
    pub length_samples: u64,
    pub muted: bool,
    pub phase_inverted: bool,
    pub loop_source: bool,
    pub source_section_start_samples: Option<u64>,
    pub source_section_length_samples: Option<u64>,
    pub active_take_index: u32,
    pub other_takes: Vec<AudioItemTakeSnapshot>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioItemTakeSnapshot {
    pub index: u32,
    pub media_ref: String,
    pub name: String,
    pub source_offset_samples: i64,
    pub phase_inverted: bool,
    pub loop_source: bool,
    pub source_section_start_samples: Option<u64>,
    pub source_section_length_samples: Option<u64>,
    pub volume: f32,
    pub pan: f32,
    pub playback_rate: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MidiItemSnapshot {
    pub id: u64,
    pub track_id: u64,
    pub name: String,
    pub active_take_index: u32,
    pub other_takes: Vec<MidiItemTakeSnapshot>,
    pub start_tick: u64,
    pub start_tick_subtick: u8,
    pub source_offset_ticks: i64,
    pub loop_source: bool,
    pub source_loop_length_ticks: u64,
    pub length_ticks: u64,
    pub muted: bool,
    pub notes: Vec<MidiNoteSnapshot>,
    pub controllers: Vec<MidiControllerData>,
    pub pitch_bends: Vec<MidiPitchBendData>,
    pub program_changes: Vec<MidiProgramChangeData>,
    pub poly_aftertouches: Vec<MidiPolyAftertouchData>,
    pub channel_pressures: Vec<MidiChannelPressureData>,
    pub opaque_events: Vec<MidiOpaqueEventData>,
    pub event_order: Vec<crate::MidiEventOrderData>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MidiItemTakeSnapshot {
    pub index: u32,
    pub name: String,
    pub source_offset_ticks: i64,
    pub loop_source: bool,
    pub source_loop_length_ticks: u64,
    pub notes: Vec<MidiNoteSnapshot>,
    pub controllers: Vec<MidiControllerData>,
    pub pitch_bends: Vec<MidiPitchBendData>,
    pub program_changes: Vec<MidiProgramChangeData>,
    pub poly_aftertouches: Vec<MidiPolyAftertouchData>,
    pub channel_pressures: Vec<MidiChannelPressureData>,
    pub opaque_events: Vec<MidiOpaqueEventData>,
    pub event_order: Vec<crate::MidiEventOrderData>,
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
