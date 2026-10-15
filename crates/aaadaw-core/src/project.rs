use crate::snapshot::{
    AudioItemSnapshot, MeterPointSnapshot, MidiItemSnapshot, MidiNoteSnapshot, ProjectSnapshot,
    SnapshotError, TempoPointSnapshot, TrackFxParameterAutomationLaneSnapshot,
    TrackFxParameterAutomationPointSnapshot, TrackFxParameterValueSnapshot, TrackFxPluginSnapshot,
    TrackSnapshot,
};
use crate::timebase::{MeterMap, TempoMap};
use crate::{
    ActionError, AudioItem, DawAction, FxParameterAutomationLane, FxParameterAutomationPoint,
    ItemId, MAX_TRACK_FX_PARAMETER_AUTOMATION_LANES, MarkerId, MidiControllerData, MidiItem,
    MidiNote, MidiPitchBendData, MusicalPosition, NoteId, ProjectMarker, ProjectMarkerSnapshot,
    ProjectRegion, ProjectRegionSnapshot, ProjectSettings, RazorEditArea, RegionId, TempoCurve,
    TimeSignature, TimebaseError, Track, TrackFxPlugin, TrackId, TrackInstrument,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

fn valid_audio_source_offset(
    source_offset_samples: i64,
    loop_source: bool,
    section_start_samples: Option<u64>,
    section_length_samples: Option<u64>,
) -> bool {
    if loop_source && source_offset_samples < 0 {
        return false;
    }
    match (section_start_samples, section_length_samples) {
        (None, None) => true,
        (Some(start), Some(length)) => {
            if start.checked_add(length).is_none() || length == 0 {
                return false;
            }
            if loop_source {
                u64::try_from(source_offset_samples).is_ok_and(|offset| offset < length)
            } else {
                true
            }
        }
        _ => false,
    }
}

const MAX_VOLUME_AUTOMATION_POINTS: usize = 65_536;
const MAX_RAZOR_EDIT_AREAS_PER_TRACK: usize = 65_536;
pub const MAX_FX_PARAMETER_AUTOMATION_POINTS: usize = 65_536;

fn valid_volume_automation(points: &[crate::VolumeAutomationPoint]) -> bool {
    points.len() <= MAX_VOLUME_AUTOMATION_POINTS
        && points
            .iter()
            .all(|point| point.gain_db().is_finite() && (-60.0..=6.0).contains(&point.gain_db()))
        && points
            .windows(2)
            .all(|pair| pair[0].sample() < pair[1].sample())
}

fn valid_razor_edit_areas(areas: &[RazorEditArea]) -> bool {
    areas.len() <= MAX_RAZOR_EDIT_AREAS_PER_TRACK
        && areas
            .iter()
            .all(|area| area.start_sample() < area.end_sample())
        && areas.windows(2).all(|pair| {
            pair[0].start_sample() < pair[1].start_sample()
                && pair[0].end_sample() <= pair[1].start_sample()
        })
}

fn valid_fx_parameter_automation(points: &[FxParameterAutomationPoint]) -> bool {
    points.len() <= MAX_FX_PARAMETER_AUTOMATION_POINTS
        && points.iter().all(|point| point.value().is_finite())
        && points
            .windows(2)
            .all(|pair| pair[0].sample() < pair[1].sample())
}

fn valid_project_markers(markers: &[ProjectMarker]) -> bool {
    let mut ids = HashSet::with_capacity(markers.len());
    markers.iter().all(|marker| {
        marker.position_ticks <= crate::timebase::MAX_EXACT_FLOAT_POSITION
            && (1..=crate::MAX_PROJECT_MARKER_NUMBER).contains(&marker.number)
            && crate::markers::valid_marker_name(&marker.name)
            && ids.insert(marker.id)
    }) && markers.windows(2).all(|pair| {
        (pair[0].position_ticks, pair[0].id.value()) <= (pair[1].position_ticks, pair[1].id.value())
    })
}

fn lowest_available_marker_number(markers: &[ProjectMarker]) -> Option<u32> {
    let used: HashSet<_> = markers.iter().map(ProjectMarker::number).collect();
    let mut candidate = 1;
    while used.contains(&candidate) {
        candidate = candidate.checked_add(1)?;
        if candidate > crate::MAX_PROJECT_MARKER_NUMBER {
            return None;
        }
    }
    Some(candidate)
}

fn valid_project_regions(regions: &[ProjectRegion]) -> bool {
    let mut ids = HashSet::with_capacity(regions.len());
    regions.iter().all(|region| {
        region.start_ticks < region.end_ticks
            && region.end_ticks <= crate::MAX_PROJECT_REGION_TICKS
            && (1..=crate::MAX_PROJECT_REGION_NUMBER).contains(&region.number)
            && crate::markers::valid_marker_name(&region.name)
            && ids.insert(region.id)
    }) && regions.windows(2).all(|pair| {
        (pair[0].start_ticks, pair[0].id.value()) <= (pair[1].start_ticks, pair[1].id.value())
    })
}

fn lowest_available_region_number(regions: &[ProjectRegion]) -> Option<u32> {
    let used: HashSet<_> = regions.iter().map(ProjectRegion::number).collect();
    let mut candidate = 1;
    while used.contains(&candidate) {
        candidate = candidate.checked_add(1)?;
        if candidate > crate::MAX_PROJECT_REGION_NUMBER {
            return None;
        }
    }
    Some(candidate)
}

type TrackStructure = Vec<(TrackId, Option<TrackId>, bool)>;

#[derive(Clone, Debug, PartialEq)]
struct TrackSampleClockSnapshot {
    track_id: TrackId,
    volume_automation: Vec<crate::VolumeAutomationPoint>,
    fx_parameter_automation: Vec<Vec<FxParameterAutomationLane>>,
}

#[derive(Clone, Debug, PartialEq)]
struct ProjectSampleClockSnapshot {
    tempo_map: TempoMap,
    audio_items: Vec<AudioItem>,
    tracks: Vec<TrackSampleClockSnapshot>,
}

impl ProjectSampleClockSnapshot {
    fn capture(state: &ProjectState) -> Self {
        Self {
            tempo_map: state.tempo_map.clone(),
            audio_items: state.audio_items.clone(),
            tracks: state
                .tracks
                .iter()
                .map(|track| TrackSampleClockSnapshot {
                    track_id: track.id,
                    volume_automation: track.volume_automation.clone(),
                    fx_parameter_automation: track
                        .fx_chain
                        .iter()
                        .map(|plugin| plugin.parameter_automation().to_vec())
                        .collect(),
                })
                .collect(),
        }
    }

    fn rescale(&mut self, sample_rate: u32) -> Result<(), ActionError> {
        let old_sample_rate = self.tempo_map.settings().sample_rate();
        if sample_rate == 0 {
            return Err(ActionError::InvalidProjectSampleRate);
        }
        if sample_rate == old_sample_rate {
            return Ok(());
        }
        let scale = |sample| rescale_project_sample(sample, old_sample_rate, sample_rate);

        self.tempo_map
            .set_sample_rate(sample_rate)
            .map_err(|error| match error {
                TimebaseError::InvalidSampleRate => ActionError::InvalidProjectSampleRate,
                TimebaseError::InvalidTempo => ActionError::InvalidTempoBpm,
                _ => ActionError::TempoMapOutOfRange,
            })?;

        for item in &mut self.audio_items {
            item.start_sample = scale(item.start_sample)?;
            item.length_samples = scale(item.length_samples)?.max(1);
            if item.start_sample.checked_add(item.length_samples).is_none()
                || item.start_sample + item.length_samples
                    > crate::timebase::MAX_EXACT_FLOAT_POSITION
            {
                return Err(ActionError::ProjectSampleClockOutOfRange);
            }
            let ratio = f64::from(sample_rate) / f64::from(old_sample_rate);
            let scale_fade = |fade: crate::AudioFade| {
                crate::AudioFade::with_curve(fade.length_samples() * ratio, fade.curve())
            };
            item.fades = crate::AudioItemFades {
                fade_in: scale_fade(item.fades.fade_in)?,
                fade_out: scale_fade(item.fades.fade_out)?,
            };
        }

        for track in &mut self.tracks {
            for point in &mut track.volume_automation {
                let sample = scale(point.sample())?;
                *point = crate::VolumeAutomationPoint::new(sample, point.gain_db())
                    .ok_or(ActionError::InvalidVolumeAutomation)?;
            }
            if track
                .volume_automation
                .windows(2)
                .any(|pair| pair[0].sample() >= pair[1].sample())
            {
                return Err(ActionError::ProjectSampleRateAutomationCollision);
            }
            for lanes in &mut track.fx_parameter_automation {
                for lane in lanes {
                    let points = lane
                        .points()
                        .iter()
                        .map(|point| {
                            FxParameterAutomationPoint::new(scale(point.sample())?, point.value())
                                .ok_or(ActionError::InvalidTrackFxParameterAutomation)
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    if points
                        .windows(2)
                        .any(|pair| pair[0].sample() >= pair[1].sample())
                    {
                        return Err(ActionError::ProjectSampleRateAutomationCollision);
                    }
                    *lane = FxParameterAutomationLane::new(lane.parameter_id(), points)
                        .ok_or(ActionError::InvalidTrackFxParameterAutomation)?;
                }
            }
        }
        Ok(())
    }

    fn matches(&self, state: &ProjectState) -> bool {
        Self::capture(state) == *self
    }

    fn restore(&self, state: &mut ProjectState) -> Result<(), ActionError> {
        if state.tracks.len() != self.tracks.len()
            || state
                .tracks
                .iter()
                .zip(&self.tracks)
                .any(|(track, snapshot)| {
                    track.id != snapshot.track_id
                        || track.fx_chain.len() != snapshot.fx_parameter_automation.len()
                })
        {
            return Err(ActionError::HistoryInvariantViolation);
        }
        state.tempo_map = self.tempo_map.clone();
        state.audio_items = self.audio_items.clone();
        for (track, snapshot) in state.tracks.iter_mut().zip(&self.tracks) {
            track.volume_automation = snapshot.volume_automation.clone();
            for (plugin, lanes) in track
                .fx_chain
                .iter_mut()
                .zip(&snapshot.fx_parameter_automation)
            {
                let old_ids: Vec<_> = plugin
                    .parameter_automation()
                    .iter()
                    .map(FxParameterAutomationLane::parameter_id)
                    .collect();
                let mut restored = plugin.clone();
                for parameter_id in old_ids {
                    restored = restored.with_parameter_automation(parameter_id, None);
                }
                for lane in lanes {
                    restored =
                        restored.with_parameter_automation(lane.parameter_id(), Some(lane.clone()));
                }
                *plugin = restored;
            }
        }
        Ok(())
    }
}

fn rescale_project_sample(sample: u64, old_rate: u32, new_rate: u32) -> Result<u64, ActionError> {
    if old_rate == 0 || new_rate == 0 {
        return Err(ActionError::InvalidProjectSampleRate);
    }
    let numerator = u128::from(sample) * u128::from(new_rate);
    let denominator = u128::from(old_rate);
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let rounded = quotient + u128::from(remainder * 2 >= denominator);
    let scaled = u64::try_from(rounded).map_err(|_| ActionError::ProjectSampleClockOutOfRange)?;
    if scaled > crate::timebase::MAX_EXACT_FLOAT_POSITION {
        return Err(ActionError::ProjectSampleClockOutOfRange);
    }
    Ok(scaled)
}

fn track_structure(tracks: &[Track]) -> TrackStructure {
    tracks
        .iter()
        .map(|track| (track.id, track.parent_track, track.is_folder))
        .collect()
}

fn valid_track_hierarchy(tracks: &[Track]) -> bool {
    let mut ancestors = Vec::new();
    for track in tracks {
        match track.parent_track {
            None => ancestors.clear(),
            Some(parent) => {
                let Some(index) = ancestors.iter().position(|id| *id == parent) else {
                    return false;
                };
                ancestors.truncate(index + 1);
            }
        }
        if track.is_folder {
            ancestors.push(track.id);
        }
    }
    true
}

fn is_descendant(tracks: &[Track], id: TrackId, ancestor: TrackId) -> bool {
    let mut parent = tracks
        .iter()
        .find(|track| track.id == id)
        .and_then(|track| track.parent_track);
    for _ in 0..tracks.len() {
        let Some(id) = parent else {
            return false;
        };
        if id == ancestor {
            return true;
        }
        parent = tracks
            .iter()
            .find(|track| track.id == id)
            .and_then(|track| track.parent_track);
    }
    false
}

fn reparent_tracks(
    tracks: &[Track],
    source: TrackId,
    parent: Option<TrackId>,
) -> Result<Vec<Track>, ActionError> {
    if !tracks.iter().any(|track| track.id == source) {
        return Err(ActionError::TrackNotFound { track_id: source });
    }
    if parent.is_some_and(|id| {
        id == source
            || is_descendant(tracks, id, source)
            || !tracks.iter().any(|track| track.id == id && track.is_folder)
    }) {
        return Err(ActionError::InvalidTrackHierarchy);
    }
    if tracks
        .iter()
        .find(|track| track.id == source)
        .unwrap()
        .parent_track
        == parent
    {
        return Ok(tracks.to_vec());
    }
    let (mut group, mut remaining): (Vec<_>, Vec<_>) = tracks
        .iter()
        .cloned()
        .partition(|track| track.id == source || is_descendant(tracks, track.id, source));
    group[0].parent_track = parent;
    let index = match parent {
        None => remaining.len(),
        Some(id) => {
            remaining.iter().position(|track| track.id == id).unwrap()
                + 1
                + remaining
                    .iter()
                    .filter(|track| is_descendant(&remaining, track.id, id))
                    .count()
        }
    };
    remaining.splice(index..index, group);
    Ok(remaining)
}

fn structure_change(before: &[Track], after: &[Track]) -> Result<ProjectEvent, ActionError> {
    if !valid_track_hierarchy(after) {
        return Err(ActionError::InvalidTrackHierarchy);
    }
    if !valid_track_routing(after) {
        return Err(ActionError::TrackRoutingCycle);
    }
    Ok(ProjectEvent::TrackStructureChanged {
        before: track_structure(before),
        after: track_structure(after),
    })
}

fn routing_destinations(track: &Track) -> impl Iterator<Item = TrackId> + '_ {
    // Dormant connections remain part of validation so toggling mute/main send
    // cannot introduce a cycle that was hidden while the route was disabled.
    track
        .effective_output_track()
        .into_iter()
        .chain(track.sends.iter().map(|send| send.destination))
}

fn valid_track_routing(tracks: &[Track]) -> bool {
    for source in tracks {
        let mut pending: Vec<_> = routing_destinations(source).collect();
        let mut visited = HashSet::new();
        while let Some(target_id) = pending.pop() {
            if target_id == source.id {
                return false;
            }
            if !visited.insert(target_id) {
                continue;
            }
            let Some(target) = tracks.iter().find(|track| track.id == target_id) else {
                return false;
            };
            pending.extend(routing_destinations(target));
        }
    }
    true
}

/// Mutable project state. All changes are made through [`DawAction`]s.
#[derive(Debug, Default)]
pub struct Project {
    state: ProjectState,
    ids: IdAllocator,
    history: Vec<ProjectEvent>,
    history_cursor: usize,
    last_fx_parameter_change: Option<FxParameterChange>,
}

/// The parameter value affected by the most recent history operation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxParameterChange {
    pub track_id: TrackId,
    pub chain_index: usize,
    pub parameter_id: u32,
    pub value: f64,
}

#[derive(Clone, Debug)]
struct ProjectState {
    notes: crate::ProjectNotes,
    master_mix: crate::MasterMix,
    pan_mode: crate::PanMode,
    frame_rate: crate::FrameRate,
    base_ruler_label_spacing_on_measure: bool,
    project_start_time_offset_seconds: f64,
    project_start_time_fractional_frame_offset_allowed: bool,
    tracks: Vec<Track>,
    audio_items: Vec<AudioItem>,
    midi_items: Vec<MidiItem>,
    markers: Vec<ProjectMarker>,
    show_project_markers: bool,
    show_project_regions: bool,
    regions: Vec<ProjectRegion>,
    tempo_map: TempoMap,
    meter_map: MeterMap,
}

impl Default for ProjectState {
    fn default() -> Self {
        Self {
            notes: crate::ProjectNotes::default(),
            master_mix: crate::MasterMix::default(),
            pan_mode: crate::PanMode::default(),
            frame_rate: crate::FrameRate::default(),
            base_ruler_label_spacing_on_measure: true,
            project_start_time_offset_seconds: 0.0,
            project_start_time_fractional_frame_offset_allowed: false,
            tracks: Vec::new(),
            audio_items: Vec::new(),
            midi_items: Vec::new(),
            markers: Vec::new(),
            show_project_markers: true,
            show_project_regions: true,
            regions: Vec::new(),
            tempo_map: TempoMap::default(),
            meter_map: MeterMap::default(),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct IdAllocator {
    next_track_id: u64,
    next_send_id: u64,
    next_item_id: u64,
    next_note_id: u64,
    next_marker_id: u64,
    next_region_id: u64,
}

#[derive(Clone, Debug, PartialEq)]
enum ProjectEvent {
    ProjectNotesChanged {
        before: crate::ProjectNotes,
        after: crate::ProjectNotes,
    },
    FrameRateChanged {
        before: crate::FrameRate,
        after: crate::FrameRate,
    },
    ProjectSampleRateChanged {
        before: ProjectSampleClockSnapshot,
        after: ProjectSampleClockSnapshot,
    },
    ForceTempoChangesToWholeSamplesChanged {
        before: bool,
        after: bool,
    },
    BpmBasisChanged {
        before: crate::BpmBasis,
        after: crate::BpmBasis,
    },
    BaseRulerLabelSpacingChanged {
        before: bool,
        after: bool,
    },
    ProjectStartTimeOffsetChanged {
        before: f64,
        after: f64,
    },
    ProjectStartTimeFractionalFrameOffsetAllowedChanged {
        before: bool,
        after: bool,
    },
    ProjectStartMeasureChanged {
        before: i32,
        after: i32,
    },
    ProjectMarkersChanged {
        before: Vec<ProjectMarker>,
        after: Vec<ProjectMarker>,
    },
    ProjectMarkerLaneVisibilityChanged {
        before: bool,
        after: bool,
    },
    ProjectRegionLaneVisibilityChanged {
        before: bool,
        after: bool,
    },
    ProjectRegionsChanged {
        before: Vec<ProjectRegion>,
        after: Vec<ProjectRegion>,
    },
    MasterMixChanged {
        before: crate::MasterMix,
        after: crate::MasterMix,
    },
    TrackStructureChanged {
        before: TrackStructure,
        after: TrackStructure,
    },
    TrackMainSendChanged {
        track_id: TrackId,
        before: bool,
        after: bool,
    },
    TrackSendsChanged {
        track_id: TrackId,
        before: Vec<crate::AudioSend>,
        after: Vec<crate::AudioSend>,
    },
    TrackCreated {
        index: usize,
        track: Track,
    },
    TrackDeleted {
        index: usize,
        track: Track,
        audio_items: Vec<(usize, AudioItem)>,
        midi_items: Vec<(usize, MidiItem)>,
    },
    TrackRestored {
        index: usize,
        track: Track,
        audio_items: Vec<(usize, AudioItem)>,
        midi_items: Vec<(usize, MidiItem)>,
    },
    TrackVolumeChanged {
        track_id: TrackId,
        before: f32,
        after: f32,
    },
    TrackVolumeAutomationChanged {
        track_id: TrackId,
        before: Vec<crate::VolumeAutomationPoint>,
        after: Vec<crate::VolumeAutomationPoint>,
    },
    TrackRazorEditAreasChanged {
        before: Vec<(TrackId, Vec<RazorEditArea>)>,
        after: Vec<(TrackId, Vec<RazorEditArea>)>,
    },
    TrackPanChanged {
        track_id: TrackId,
        before: f32,
        after: f32,
    },
    TrackPhaseChanged {
        track_id: TrackId,
        before: bool,
        after: bool,
    },
    TrackMuteChanged {
        track_id: TrackId,
        before: bool,
        after: bool,
    },
    TrackSoloChanged {
        track_id: TrackId,
        before: bool,
        after: bool,
    },
    TrackRecordArmChanged {
        track_id: TrackId,
        before: bool,
        after: bool,
    },
    TrackInstrumentChanged {
        track_id: TrackId,
        before: Option<TrackInstrument>,
        after: Option<TrackInstrument>,
    },
    TrackFreezeChanged {
        track_id: TrackId,
        before: Option<ItemId>,
        after: Option<ItemId>,
        before_render: Option<(usize, AudioItem)>,
        after_render: Option<(usize, AudioItem)>,
    },
    TrackFxChainChanged {
        track_id: TrackId,
        before: Vec<TrackFxPlugin>,
        after: Vec<TrackFxPlugin>,
    },
    TrackFxParameterChanged {
        track_id: TrackId,
        chain_index: usize,
        parameter_id: u32,
        before: f64,
        after: f64,
        before_state: Option<Vec<u8>>,
        after_state: Option<Vec<u8>>,
    },
    TrackFxParameterAutomationChanged {
        track_id: TrackId,
        chain_index: usize,
        parameter_id: u32,
        before: Option<FxParameterAutomationLane>,
        after: Option<FxParameterAutomationLane>,
    },
    TrackRenamed {
        track_id: TrackId,
        before: String,
        after: String,
    },
    TrackOutputChanged {
        track_id: TrackId,
        before: Option<TrackId>,
        after: Option<TrackId>,
    },
    TempoChanged {
        start_tick: u64,
        before: Option<f64>,
        after: Option<f64>,
    },
    TempoCurveChanged {
        start_tick: u64,
        before: TempoCurve,
        after: TempoCurve,
    },
    MeterChanged {
        start_tick: u64,
        before: Option<TimeSignature>,
        after: Option<TimeSignature>,
    },
    MeterMapReplaced {
        before: Vec<MeterPointSnapshot>,
        after: Vec<MeterPointSnapshot>,
    },
    AudioItemInserted {
        index: usize,
        item: AudioItem,
    },
    AudioItemRemoved {
        index: usize,
        item: AudioItem,
    },
    AudioItemFadesChanged {
        item_id: ItemId,
        before: crate::AudioItemFades,
        after: crate::AudioItemFades,
    },
    AudioItemVolumeChanged {
        item_id: ItemId,
        before: f32,
        after: f32,
    },
    AudioItemTakeMixChanged {
        item_id: ItemId,
        before: (f32, f32),
        after: (f32, f32),
    },
    AudioItemChanged {
        before: AudioItem,
        after: AudioItem,
    },
    MidiItemInserted {
        index: usize,
        item: MidiItem,
    },
    MidiItemRemoved {
        index: usize,
        item: MidiItem,
    },
    MidiItemChanged {
        before: MidiItem,
        after: MidiItem,
    },
    MidiItemsReplaced {
        index: usize,
        before: Vec<MidiItem>,
        after: Vec<MidiItem>,
    },
    MidiNotesAdded {
        item_id: ItemId,
        notes: Vec<(usize, MidiNote)>,
    },
    MidiNotesRemoved {
        item_id: ItemId,
        notes: Vec<(usize, MidiNote)>,
    },
    MidiNoteChanged {
        item_id: ItemId,
        before: MidiNote,
        after: MidiNote,
    },
    MidiControllersChanged {
        item_id: ItemId,
        before: Vec<MidiControllerData>,
        after: Vec<MidiControllerData>,
    },
    MidiPitchBendsChanged {
        item_id: ItemId,
        before: Vec<crate::MidiPitchBendData>,
        after: Vec<crate::MidiPitchBendData>,
    },
    MidiNotesQuantized {
        item_id: ItemId,
        changes: Vec<(NoteId, u64, u64)>,
    },
    Transaction {
        tx_id: u64,
        events: Vec<ProjectEvent>,
    },
}

impl ProjectEvent {
    fn inverse(&self) -> Self {
        match self {
            Self::ProjectNotesChanged { before, after } => Self::ProjectNotesChanged {
                before: after.clone(),
                after: before.clone(),
            },
            Self::FrameRateChanged { before, after } => Self::FrameRateChanged {
                before: *after,
                after: *before,
            },
            Self::ProjectSampleRateChanged { before, after } => Self::ProjectSampleRateChanged {
                before: after.clone(),
                after: before.clone(),
            },
            Self::ForceTempoChangesToWholeSamplesChanged { before, after } => {
                Self::ForceTempoChangesToWholeSamplesChanged {
                    before: *after,
                    after: *before,
                }
            }
            Self::BpmBasisChanged { before, after } => Self::BpmBasisChanged {
                before: *after,
                after: *before,
            },
            Self::BaseRulerLabelSpacingChanged { before, after } => {
                Self::BaseRulerLabelSpacingChanged {
                    before: *after,
                    after: *before,
                }
            }
            Self::ProjectStartTimeOffsetChanged { before, after } => {
                Self::ProjectStartTimeOffsetChanged {
                    before: *after,
                    after: *before,
                }
            }
            Self::ProjectStartTimeFractionalFrameOffsetAllowedChanged { before, after } => {
                Self::ProjectStartTimeFractionalFrameOffsetAllowedChanged {
                    before: *after,
                    after: *before,
                }
            }
            Self::ProjectStartMeasureChanged { before, after } => {
                Self::ProjectStartMeasureChanged {
                    before: *after,
                    after: *before,
                }
            }
            Self::ProjectMarkersChanged { before, after } => Self::ProjectMarkersChanged {
                before: after.clone(),
                after: before.clone(),
            },
            Self::ProjectMarkerLaneVisibilityChanged { before, after } => {
                Self::ProjectMarkerLaneVisibilityChanged {
                    before: *after,
                    after: *before,
                }
            }
            Self::ProjectRegionLaneVisibilityChanged { before, after } => {
                Self::ProjectRegionLaneVisibilityChanged {
                    before: *after,
                    after: *before,
                }
            }
            Self::ProjectRegionsChanged { before, after } => Self::ProjectRegionsChanged {
                before: after.clone(),
                after: before.clone(),
            },
            Self::MasterMixChanged { before, after } => Self::MasterMixChanged {
                before: *after,
                after: *before,
            },
            Self::TrackStructureChanged { before, after } => Self::TrackStructureChanged {
                before: after.clone(),
                after: before.clone(),
            },
            Self::TrackCreated { index, track } => Self::TrackDeleted {
                index: *index,
                track: track.clone(),
                audio_items: Vec::new(),
                midi_items: Vec::new(),
            },
            Self::TrackDeleted {
                index,
                track,
                audio_items,
                midi_items,
            } => Self::TrackRestored {
                index: *index,
                track: track.clone(),
                audio_items: audio_items.clone(),
                midi_items: midi_items.clone(),
            },
            Self::TrackRestored {
                index,
                track,
                audio_items,
                midi_items,
            } => Self::TrackDeleted {
                index: *index,
                track: track.clone(),
                audio_items: audio_items.clone(),
                midi_items: midi_items.clone(),
            },
            Self::TrackMainSendChanged {
                track_id,
                before,
                after,
            } => Self::TrackMainSendChanged {
                track_id: *track_id,
                before: *after,
                after: *before,
            },
            Self::TrackSendsChanged {
                track_id,
                before,
                after,
            } => Self::TrackSendsChanged {
                track_id: *track_id,
                before: after.clone(),
                after: before.clone(),
            },
            Self::TrackVolumeChanged {
                track_id,
                before,
                after,
            } => Self::TrackVolumeChanged {
                track_id: *track_id,
                before: *after,
                after: *before,
            },
            Self::TrackVolumeAutomationChanged {
                track_id,
                before,
                after,
            } => Self::TrackVolumeAutomationChanged {
                track_id: *track_id,
                before: after.clone(),
                after: before.clone(),
            },
            Self::TrackRazorEditAreasChanged { before, after } => {
                Self::TrackRazorEditAreasChanged {
                    before: after.clone(),
                    after: before.clone(),
                }
            }
            Self::TrackPanChanged {
                track_id,
                before,
                after,
            } => Self::TrackPanChanged {
                track_id: *track_id,
                before: *after,
                after: *before,
            },
            Self::TrackPhaseChanged {
                track_id,
                before,
                after,
            } => Self::TrackPhaseChanged {
                track_id: *track_id,
                before: *after,
                after: *before,
            },
            Self::TrackMuteChanged {
                track_id,
                before,
                after,
            } => Self::TrackMuteChanged {
                track_id: *track_id,
                before: *after,
                after: *before,
            },
            Self::TrackSoloChanged {
                track_id,
                before,
                after,
            } => Self::TrackSoloChanged {
                track_id: *track_id,
                before: *after,
                after: *before,
            },
            Self::TrackRecordArmChanged {
                track_id,
                before,
                after,
            } => Self::TrackRecordArmChanged {
                track_id: *track_id,
                before: *after,
                after: *before,
            },
            Self::TrackInstrumentChanged {
                track_id,
                before,
                after,
            } => Self::TrackInstrumentChanged {
                track_id: *track_id,
                before: after.clone(),
                after: before.clone(),
            },
            Self::TrackFreezeChanged {
                track_id,
                before,
                after,
                before_render,
                after_render,
            } => Self::TrackFreezeChanged {
                track_id: *track_id,
                before: *after,
                after: *before,
                before_render: after_render.clone(),
                after_render: before_render.clone(),
            },
            Self::TrackFxChainChanged {
                track_id,
                before,
                after,
            } => Self::TrackFxChainChanged {
                track_id: *track_id,
                before: after.clone(),
                after: before.clone(),
            },
            Self::TrackFxParameterChanged {
                track_id,
                chain_index,
                parameter_id,
                before,
                after,
                before_state,
                after_state,
            } => Self::TrackFxParameterChanged {
                track_id: *track_id,
                chain_index: *chain_index,
                parameter_id: *parameter_id,
                before: *after,
                after: *before,
                before_state: after_state.clone(),
                after_state: before_state.clone(),
            },
            Self::TrackFxParameterAutomationChanged {
                track_id,
                chain_index,
                parameter_id,
                before,
                after,
            } => Self::TrackFxParameterAutomationChanged {
                track_id: *track_id,
                chain_index: *chain_index,
                parameter_id: *parameter_id,
                before: after.clone(),
                after: before.clone(),
            },
            Self::TrackRenamed {
                track_id,
                before,
                after,
            } => Self::TrackRenamed {
                track_id: *track_id,
                before: after.clone(),
                after: before.clone(),
            },
            Self::TrackOutputChanged {
                track_id,
                before,
                after,
            } => Self::TrackOutputChanged {
                track_id: *track_id,
                before: *after,
                after: *before,
            },
            Self::TempoChanged {
                start_tick,
                before,
                after,
            } => Self::TempoChanged {
                start_tick: *start_tick,
                before: *after,
                after: *before,
            },
            Self::TempoCurveChanged {
                start_tick,
                before,
                after,
            } => Self::TempoCurveChanged {
                start_tick: *start_tick,
                before: *after,
                after: *before,
            },
            Self::MeterChanged {
                start_tick,
                before,
                after,
            } => Self::MeterChanged {
                start_tick: *start_tick,
                before: *after,
                after: *before,
            },
            Self::MeterMapReplaced { before, after } => Self::MeterMapReplaced {
                before: after.clone(),
                after: before.clone(),
            },
            Self::AudioItemInserted { index, item } => Self::AudioItemRemoved {
                index: *index,
                item: item.clone(),
            },
            Self::AudioItemRemoved { index, item } => Self::AudioItemInserted {
                index: *index,
                item: item.clone(),
            },
            Self::AudioItemFadesChanged {
                item_id,
                before,
                after,
            } => Self::AudioItemFadesChanged {
                item_id: *item_id,
                before: *after,
                after: *before,
            },
            Self::AudioItemVolumeChanged {
                item_id,
                before,
                after,
            } => Self::AudioItemVolumeChanged {
                item_id: *item_id,
                before: *after,
                after: *before,
            },
            Self::AudioItemTakeMixChanged {
                item_id,
                before,
                after,
            } => Self::AudioItemTakeMixChanged {
                item_id: *item_id,
                before: *after,
                after: *before,
            },
            Self::AudioItemChanged { before, after } => Self::AudioItemChanged {
                before: after.clone(),
                after: before.clone(),
            },
            Self::MidiItemInserted { index, item } => Self::MidiItemRemoved {
                index: *index,
                item: item.clone(),
            },
            Self::MidiItemRemoved { index, item } => Self::MidiItemInserted {
                index: *index,
                item: item.clone(),
            },
            Self::MidiItemChanged { before, after } => Self::MidiItemChanged {
                before: after.clone(),
                after: before.clone(),
            },
            Self::MidiItemsReplaced {
                index,
                before,
                after,
            } => Self::MidiItemsReplaced {
                index: *index,
                before: after.clone(),
                after: before.clone(),
            },
            Self::MidiNotesAdded { item_id, notes } => Self::MidiNotesRemoved {
                item_id: *item_id,
                notes: notes.clone(),
            },
            Self::MidiNotesRemoved { item_id, notes } => Self::MidiNotesAdded {
                item_id: *item_id,
                notes: notes.clone(),
            },
            Self::MidiNoteChanged {
                item_id,
                before,
                after,
            } => Self::MidiNoteChanged {
                item_id: *item_id,
                before: after.clone(),
                after: before.clone(),
            },
            Self::MidiControllersChanged {
                item_id,
                before,
                after,
            } => Self::MidiControllersChanged {
                item_id: *item_id,
                before: after.clone(),
                after: before.clone(),
            },
            Self::MidiPitchBendsChanged {
                item_id,
                before,
                after,
            } => Self::MidiPitchBendsChanged {
                item_id: *item_id,
                before: after.clone(),
                after: before.clone(),
            },
            Self::MidiNotesQuantized { item_id, changes } => Self::MidiNotesQuantized {
                item_id: *item_id,
                changes: changes
                    .iter()
                    .map(|(note_id, before, after)| (*note_id, *after, *before))
                    .collect(),
            },
            Self::Transaction { tx_id, events } => Self::Transaction {
                tx_id: *tx_id,
                events: events.iter().rev().map(Self::inverse).collect(),
            },
        }
    }
}

fn fx_parameter_change(event: &ProjectEvent) -> Option<FxParameterChange> {
    match event {
        ProjectEvent::TrackFxParameterChanged {
            track_id,
            chain_index,
            parameter_id,
            after,
            ..
        } => Some(FxParameterChange {
            track_id: *track_id,
            chain_index: *chain_index,
            parameter_id: *parameter_id,
            value: *after,
        }),
        ProjectEvent::Transaction { events, .. } => {
            events.iter().rev().find_map(fx_parameter_change)
        }
        _ => None,
    }
}

fn duplicate_midi_item_at(
    state: &mut ProjectState,
    ids: &mut IdAllocator,
    item_id: ItemId,
    start_tick: u64,
) -> Result<ProjectEvent, ActionError> {
    let track_id = state
        .midi_items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(ActionError::MidiItemNotFound { item_id })?
        .track_id;
    duplicate_midi_item_to_track(state, ids, item_id, track_id, start_tick)
}

fn duplicate_midi_item_to_track(
    state: &mut ProjectState,
    ids: &mut IdAllocator,
    item_id: ItemId,
    track_id: TrackId,
    start_tick: u64,
) -> Result<ProjectEvent, ActionError> {
    let original = state
        .midi_items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(ActionError::MidiItemNotFound { item_id })?;
    let target_track = state
        .tracks
        .iter()
        .find(|track| track.id == track_id)
        .ok_or(ActionError::TrackNotFound { track_id })?;
    if target_track.is_frozen() {
        return Err(ActionError::CannotEditFrozenTrackSource { track_id });
    }
    let item_end = start_tick
        .checked_add(original.length_ticks)
        .ok_or(ActionError::InvalidMidiItemPosition)?;
    let next_item_id = ids
        .next_item_id
        .checked_add(1)
        .ok_or(ActionError::ItemIdExhausted)?;
    let mut next_note_id = ids.next_note_id;
    let mut notes = Vec::with_capacity(original.notes.len());
    for note in original.notes.iter() {
        let next_id = next_note_id
            .checked_add(1)
            .ok_or(ActionError::NoteIdExhausted)?;
        notes.push(MidiNote {
            id: NoteId::from_raw(next_note_id),
            data: note.data,
        });
        next_note_id = next_id;
    }
    let duplicate = MidiItem {
        id: ItemId::from_raw(ids.next_item_id),
        track_id,
        name: original.name.clone(),
        start_tick,
        source_offset_ticks: original.source_offset_ticks,
        loop_source: original.loop_source,
        source_loop_length_ticks: original.source_loop_length_ticks,
        length_ticks: item_end - start_tick,
        muted: original.muted,
        notes: Arc::new(notes),
        controllers: Arc::clone(&original.controllers),
        pitch_bends: Arc::clone(&original.pitch_bends),
    };
    ids.next_item_id = next_item_id;
    ids.next_note_id = next_note_id;
    Ok(ProjectEvent::MidiItemInserted {
        index: state.midi_items.len(),
        item: duplicate,
    })
}

impl Project {
    /// Current stereo Master controls, separate from ordinary track IDs.
    pub fn master_mix(&self) -> crate::MasterMix {
        self.state.master_mix
    }
    /// Creates an empty project with 48 kHz, 960 PPQ and 120 BPM defaults.
    pub fn new() -> Self {
        Self::with_settings(ProjectSettings::default())
    }

    /// Creates an empty project with explicit timebase settings.
    pub fn with_settings(settings: ProjectSettings) -> Self {
        Self::with_settings_and_initial_time_signature(
            settings,
            TimeSignature::new(4, 4).expect("4/4 is a valid time signature"),
        )
        .expect("the default time signature is valid for every supported PPQ")
    }

    /// Creates an empty project with explicit timebase settings and starting meter.
    pub fn with_settings_and_initial_time_signature(
        settings: ProjectSettings,
        initial_time_signature: TimeSignature,
    ) -> Result<Self, TimebaseError> {
        let mut meter_map = MeterMap::new(settings.ppq(), settings.start_measure());
        meter_map.replace_points(&[(0, initial_time_signature)])?;
        Ok(Self {
            state: ProjectState {
                pan_mode: settings.pan_mode(),
                frame_rate: settings.frame_rate(),
                base_ruler_label_spacing_on_measure: settings.base_ruler_label_spacing_on_measure(),
                project_start_time_offset_seconds: settings.project_start_time_offset_seconds(),
                project_start_time_fractional_frame_offset_allowed: settings
                    .project_start_time_fractional_frame_offset_allowed(),
                tempo_map: TempoMap::new(settings),
                meter_map,
                ..ProjectState::default()
            },
            ..Self::default()
        })
    }

    /// Whether a folder can own this track and its complete subtree.
    pub fn can_parent_to(&self, source: TrackId, parent: TrackId) -> bool {
        reparent_tracks(&self.state.tracks, source, Some(parent))
            .is_ok_and(|tracks| valid_track_hierarchy(&tracks) && valid_track_routing(&tracks))
    }

    pub fn folder_depth(&self, track_id: TrackId) -> usize {
        let mut depth = 0;
        let mut parent = self
            .state
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .and_then(|track| track.parent_track);
        while let Some(id) = parent {
            depth += 1;
            parent = self
                .state
                .tracks
                .iter()
                .find(|track| track.id == id)
                .and_then(|track| track.parent_track);
        }
        depth
    }

    pub fn track_subtree_len(&self, track_id: TrackId) -> usize {
        usize::from(self.state.tracks.iter().any(|track| track.id == track_id))
            + self
                .state
                .tracks
                .iter()
                .filter(|track| is_descendant(&self.state.tracks, track.id, track_id))
                .count()
    }

    /// Whether adding a connection between two tracks keeps the graph acyclic.
    pub fn can_route_to(&self, source: TrackId, destination: TrackId) -> bool {
        if source == destination
            || !self.state.tracks.iter().any(|track| track.id == source)
            || !self
                .state
                .tracks
                .iter()
                .any(|track| track.id == destination)
        {
            return false;
        }
        let mut pending = vec![destination];
        let mut visited = HashSet::new();
        while let Some(id) = pending.pop() {
            if id == source {
                return false;
            }
            if !visited.insert(id) {
                continue;
            }
            if let Some(track) = self.state.tracks.iter().find(|track| track.id == id) {
                pending.extend(routing_destinations(track));
            }
        }
        true
    }

    /// Returns the project's immutable sample-rate and PPQ settings.
    pub fn settings(&self) -> ProjectSettings {
        self.state
            .tempo_map
            .settings()
            .with_pan_mode(self.state.pan_mode)
            .with_frame_rate(self.state.frame_rate)
            .with_start_measure(self.state.meter_map.start_measure())
            .with_base_ruler_label_spacing_on_measure(
                self.state.base_ruler_label_spacing_on_measure,
            )
            .with_project_start_time_offset_seconds(self.state.project_start_time_offset_seconds)
            .expect("project start time offset is validated when actions are applied")
            .with_project_start_time_fractional_frame_offset_allowed(
                self.state
                    .project_start_time_fractional_frame_offset_allowed,
            )
    }

    /// Converts a PPQ tick position to the nearest sample index.
    pub fn sample_at_tick(&self, tick: u64) -> Result<u64, TimebaseError> {
        self.state.tempo_map.sample_at_tick(tick)
    }

    /// Converts a sample index to the nearest PPQ tick position.
    pub fn tick_at_sample(&self, sample: u64) -> Result<u64, TimebaseError> {
        self.state.tempo_map.tick_at_sample(sample)
    }

    /// Converts a project sample position to its displayed clock time.
    pub fn project_time_at_sample(&self, sample: u64) -> Result<f64, TimebaseError> {
        if sample > 1_u64 << 53 {
            return Err(TimebaseError::PositionOutOfRange);
        }
        let settings = self.settings();
        let seconds = settings.project_start_time_offset_seconds()
            + sample as f64 / f64::from(settings.sample_rate());
        seconds
            .is_finite()
            .then_some(seconds)
            .ok_or(TimebaseError::PositionOutOfRange)
    }

    /// Converts a displayed clock time back to a project sample position.
    pub fn sample_at_project_time(&self, seconds: f64) -> Result<u64, TimebaseError> {
        if !seconds.is_finite() {
            return Err(TimebaseError::PositionOutOfRange);
        }
        let settings = self.settings();
        let sample = (seconds - settings.project_start_time_offset_seconds())
            * f64::from(settings.sample_rate());
        if !sample.is_finite() || !(0.0..=((1_u64 << 53) - 1) as f64).contains(&sample) {
            return Err(TimebaseError::PositionOutOfRange);
        }
        Ok(sample.round() as u64)
    }

    /// Converts a continuous PPQ position without quantizing to ticks or samples.
    /// Intended for gestures and geometry; render block clocks remain integral.
    pub fn sample_at_tick_position(&self, tick: f64) -> Result<f64, TimebaseError> {
        self.state.tempo_map.sample_at_tick_position(tick)
    }

    /// Converts a continuous sample position without rounding the PPQ result.
    pub fn tick_at_sample_position(&self, sample: f64) -> Result<f64, TimebaseError> {
        self.state.tempo_map.tick_at_sample_position(sample)
    }

    /// Converts a MIDI source PPQ phase into the project-sample offset used by a take.
    /// The offset is measured backward from the item start through the project Tempo Map.
    pub fn midi_source_offset_samples(
        &self,
        item_start_tick: u64,
        source_phase_ticks: u64,
    ) -> Result<f64, TimebaseError> {
        const MAX_EXACT: f64 = (1_u64 << 53) as f64;
        if item_start_tick > (1_u64 << 53) || source_phase_ticks > (1_u64 << 53) {
            return Err(TimebaseError::PositionOutOfRange);
        }
        let item_start_tick = item_start_tick as f64;
        let item_start_sample = self.sample_at_tick_position(item_start_tick)?;
        let source_tick = item_start_tick - source_phase_ticks as f64;
        let source_sample = self.sample_at_signed_tick_position(source_tick)?;
        let offset_samples = item_start_sample - source_sample;
        if !offset_samples.is_finite() || !(0.0..=MAX_EXACT).contains(&offset_samples) {
            return Err(TimebaseError::PositionOutOfRange);
        }
        Ok(offset_samples)
    }

    /// Converts a take's source-time offset back to its wrapped MIDI source PPQ phase.
    pub fn midi_source_phase_at_offset_samples(
        &self,
        item_start_tick: u64,
        source_loop_length_ticks: u64,
        source_offset_samples: f64,
    ) -> Result<u64, TimebaseError> {
        const MAX_EXACT: f64 = (1_u64 << 53) as f64;
        if item_start_tick > (1_u64 << 53)
            || source_loop_length_ticks == 0
            || source_loop_length_ticks > (1_u64 << 53)
            || !source_offset_samples.is_finite()
            || !(0.0..=MAX_EXACT).contains(&source_offset_samples)
        {
            return Err(TimebaseError::PositionOutOfRange);
        }
        let item_start_tick = item_start_tick as f64;
        let item_start_sample = self.sample_at_tick_position(item_start_tick)?;
        let source_sample = item_start_sample - source_offset_samples;
        let source_tick = self.tick_at_signed_sample_position(source_sample)?;
        let phase = (item_start_tick - source_tick).round();
        if !phase.is_finite() || !(0.0..=MAX_EXACT).contains(&phase) {
            return Err(TimebaseError::PositionOutOfRange);
        }
        Ok((phase as u64) % source_loop_length_ticks)
    }

    fn sample_at_signed_tick_position(&self, tick: f64) -> Result<f64, TimebaseError> {
        const MAX_EXACT: f64 = (1_u64 << 53) as f64;
        if !tick.is_finite() || tick.abs() > MAX_EXACT {
            return Err(TimebaseError::PositionOutOfRange);
        }
        if tick >= 0.0 {
            return self.sample_at_tick_position(tick);
        }
        let settings = self.settings();
        let sample = tick * f64::from(settings.sample_rate()) * 60.0
            / (f64::from(settings.ppq())
                * self.tempo_at_tick(0)
                * settings.bpm_basis().quarter_notes_per_beat());
        (sample.is_finite() && sample.abs() <= MAX_EXACT)
            .then_some(sample)
            .ok_or(TimebaseError::PositionOutOfRange)
    }

    fn tick_at_signed_sample_position(&self, sample: f64) -> Result<f64, TimebaseError> {
        const MAX_EXACT: f64 = (1_u64 << 53) as f64;
        if !sample.is_finite() || sample.abs() > MAX_EXACT {
            return Err(TimebaseError::PositionOutOfRange);
        }
        if sample >= 0.0 {
            return self.tick_at_sample_position(sample);
        }
        let settings = self.settings();
        let tick = sample
            * f64::from(settings.ppq())
            * self.tempo_at_tick(0)
            * settings.bpm_basis().quarter_notes_per_beat()
            / (60.0 * f64::from(settings.sample_rate()));
        (tick.is_finite() && tick.abs() <= MAX_EXACT)
            .then_some(tick)
            .ok_or(TimebaseError::PositionOutOfRange)
    }

    /// Returns the displayed BPM active at a PPQ tick position in the project's selected basis.
    pub fn tempo_at_tick(&self, tick: u64) -> f64 {
        self.state.tempo_map.tempo_at_tick(tick)
    }

    /// Returns the ordered tempo points as `(start_tick, bpm, curve_to_next)` tuples.
    pub fn tempo_points(&self) -> impl Iterator<Item = (u64, f64, TempoCurve)> + '_ {
        self.state.tempo_map.points()
    }

    /// Converts a tick to a signed measure/beat and an in-beat tick offset.
    pub fn musical_position_at_tick(&self, tick: u64) -> Result<MusicalPosition, TimebaseError> {
        self.state.meter_map.position_at_tick(tick)
    }

    /// Converts a signed displayed measure/beat and fractional in-beat tick offset.
    /// Rejects positions outside the active meter instead of carrying into another bar.
    pub fn tick_at_musical_position(
        &self,
        measure: i64,
        beat: u32,
        tick_in_beat: f64,
    ) -> Result<f64, TimebaseError> {
        self.state
            .meter_map
            .tick_at_position(measure, beat, tick_in_beat)
    }

    /// Returns the time signature active at a PPQ tick position.
    pub fn time_signature_at_tick(&self, tick: u64) -> TimeSignature {
        self.state.meter_map.signature_at_tick(tick)
    }

    /// Returns the project's ordered time-signature changes, including tick zero.
    pub fn time_signature_points(&self) -> impl Iterator<Item = (u64, TimeSignature)> + '_ {
        self.state.meter_map.points()
    }

    /// Returns the meter map as ordered tick/signature snapshots.
    pub fn time_signature_map(&self) -> Vec<MeterPointSnapshot> {
        self.state
            .meter_map
            .points()
            .map(|(start_tick, signature)| MeterPointSnapshot {
                start_tick,
                numerator: signature.numerator(),
                denominator: signature.denominator(),
            })
            .collect()
    }

    /// Returns whether a committed action is available to undo.
    pub fn can_undo(&self) -> bool {
        self.history_cursor > 0
    }

    /// Returns whether a committed action is available to redo.
    pub fn can_redo(&self) -> bool {
        self.history_cursor < self.history.len()
    }

    /// Returns the current position in the project's undo history.
    pub fn history_cursor(&self) -> usize {
        self.history_cursor
    }

    /// Discards redoable project actions when an application-level undoable
    /// view action establishes a new history branch.
    pub fn discard_redo_history(&mut self) {
        self.history.truncate(self.history_cursor);
    }

    /// Returns whether the next undo changes only live mix controls or volume automation.
    pub fn can_undo_track_mix(&self) -> bool {
        matches!(
            self.history_cursor
                .checked_sub(1)
                .and_then(|index| self.history.get(index)),
            Some(
                ProjectEvent::TrackVolumeChanged { .. }
                    | ProjectEvent::MasterMixChanged { .. }
                    | ProjectEvent::TrackVolumeAutomationChanged { .. }
                    | ProjectEvent::TrackPanChanged { .. }
                    | ProjectEvent::TrackPhaseChanged { .. }
                    | ProjectEvent::AudioItemFadesChanged { .. }
                    | ProjectEvent::AudioItemVolumeChanged { .. }
                    | ProjectEvent::AudioItemTakeMixChanged { .. },
            )
        )
    }

    /// Returns whether the next redo changes only live mix controls or volume automation.
    pub fn can_redo_track_mix(&self) -> bool {
        matches!(
            self.history.get(self.history_cursor),
            Some(
                ProjectEvent::TrackVolumeChanged { .. }
                    | ProjectEvent::MasterMixChanged { .. }
                    | ProjectEvent::TrackVolumeAutomationChanged { .. }
                    | ProjectEvent::TrackPanChanged { .. }
                    | ProjectEvent::TrackPhaseChanged { .. }
                    | ProjectEvent::AudioItemFadesChanged { .. }
                    | ProjectEvent::AudioItemVolumeChanged { .. }
                    | ProjectEvent::AudioItemTakeMixChanged { .. },
            )
        )
    }

    /// Applies an action atomically. Media edits add one history entry; project
    /// documentation preserves the existing undo/redo history.
    ///
    /// A failed action, including a failed nested action in a transaction, leaves
    /// project state, identifier allocation, and history unchanged.
    pub fn apply(&mut self, action: DawAction) -> Result<(), ActionError> {
        let mut state = self.state.clone();
        let mut ids = self.ids.clone();
        let event = Self::apply_action(&mut state, &mut ids, action)?;
        if matches!(
            &event,
            ProjectEvent::MidiNotesQuantized { changes, .. } if changes.is_empty()
        ) || matches!(
            &event,
            ProjectEvent::ProjectMarkersChanged { before, after } if before == after
        ) || matches!(
            &event,
            ProjectEvent::ProjectRegionsChanged { before, after } if before == after
        ) || matches!(
            &event,
            ProjectEvent::ProjectMarkerLaneVisibilityChanged { before, after } if before == after
        ) || matches!(
            &event,
            ProjectEvent::ProjectRegionLaneVisibilityChanged { before, after } if before == after
        ) || matches!(
            &event,
            ProjectEvent::TrackRazorEditAreasChanged { before, after } if before == after
        ) {
            return Ok(());
        }

        self.state = state;
        self.ids = ids;
        let Some(event) = undoable_event(event) else {
            return Ok(());
        };
        self.history.truncate(self.history_cursor);
        self.history.push(event);
        self.history_cursor += 1;
        self.last_fx_parameter_change = fx_parameter_change(self.history.last().unwrap());
        Ok(())
    }

    /// Undoes the most recently applied action, returning `false` at the start
    /// of history.
    pub fn undo(&mut self) -> Result<bool, ActionError> {
        if self.history_cursor == 0 {
            return Ok(false);
        }

        let event = self.history[self.history_cursor - 1].inverse();
        let mut state = self.state.clone();
        Self::apply_event(&mut state, &event)?;

        self.state = state;
        self.history_cursor -= 1;
        self.last_fx_parameter_change = fx_parameter_change(&event);
        Ok(true)
    }

    /// Reapplies the next action in history, returning `false` at its end.
    pub fn redo(&mut self) -> Result<bool, ActionError> {
        let Some(event) = self.history.get(self.history_cursor) else {
            return Ok(false);
        };
        let mut state = self.state.clone();
        Self::apply_event(&mut state, event)?;

        self.state = state;
        self.history_cursor += 1;
        self.last_fx_parameter_change = fx_parameter_change(event);
        Ok(true)
    }

    /// Returns the parameter affected by the last apply, undo, or redo operation.
    pub fn last_fx_parameter_change(&self) -> Option<FxParameterChange> {
        self.last_fx_parameter_change
    }

    /// Returns the project's tracks in timeline order.
    pub fn tracks(&self) -> &[Track] {
        &self.state.tracks
    }

    /// Returns all audio items in project insertion order.
    pub fn audio_items(&self) -> &[AudioItem] {
        &self.state.audio_items
    }

    /// Returns all MIDI items in project insertion order.
    pub fn midi_items(&self) -> &[MidiItem] {
        &self.state.midi_items
    }

    /// Returns the project's markers ordered by timeline position and stable identity.
    pub fn markers(&self) -> &[ProjectMarker] {
        &self.state.markers
    }

    /// Whether the project marker lane is visible in the ruler.
    pub fn show_project_markers(&self) -> bool {
        self.state.show_project_markers
    }

    /// Whether the project region lane is visible in the ruler.
    pub fn show_project_regions(&self) -> bool {
        self.state.show_project_regions
    }

    /// Returns the project's regions ordered by timeline start and stable identity.
    pub fn regions(&self) -> &[ProjectRegion] {
        &self.state.regions
    }

    /// Project documentation, independent of media undo/redo history.
    pub fn project_notes(&self) -> &crate::ProjectNotes {
        &self.state.notes
    }

    /// Creates a serialization-friendly copy of the current project state.
    pub fn snapshot(&self) -> ProjectSnapshot {
        ProjectSnapshot {
            notes: self.state.notes.clone(),
            settings: self.settings(),
            master_mix: self.state.master_mix,
            tracks: self
                .state
                .tracks
                .iter()
                .map(|track| TrackSnapshot {
                    id: track.id.value(),
                    name: track.name.clone(),
                    is_bus: track.is_bus,
                    is_folder: track.is_folder,
                    parent_track_id: track.parent_track.map(TrackId::value),
                    output_track_id: track.output_track.map(TrackId::value),
                    main_send_enabled: track.main_send_enabled,
                    sends: track
                        .sends
                        .iter()
                        .map(|send| crate::AudioSendSnapshot {
                            id: send.id.value(),
                            destination_track_id: send.destination.value(),
                            parameters: send.parameters,
                        })
                        .collect(),
                    volume_db: track.volume_db,
                    pan: track.pan,
                    muted: track.muted,
                    phase_inverted: track.phase_inverted,
                    solo: track.solo,
                    record_armed: track.record_armed,
                    instrument: track.instrument.as_ref().map(|instrument| {
                        crate::TrackInstrumentSnapshot {
                            plugin_id: instrument.plugin_id().to_owned(),
                            bundle_path: instrument.bundle_path().to_owned(),
                            state: instrument.state().map(<[u8]>::to_vec),
                        }
                    }),
                    fx_chain: track
                        .fx_chain
                        .iter()
                        .map(|plugin| TrackFxPluginSnapshot {
                            plugin_id: plugin.plugin_id().to_owned(),
                            bundle_path: plugin.bundle_path().to_owned(),
                            enabled: plugin.is_enabled(),
                            state: plugin.state().map(<[u8]>::to_vec),
                            parameter_values: plugin
                                .parameter_values()
                                .map(|(parameter_id, value)| TrackFxParameterValueSnapshot {
                                    parameter_id,
                                    value,
                                })
                                .collect(),
                            parameter_automation: plugin
                                .parameter_automation()
                                .iter()
                                .map(|lane| TrackFxParameterAutomationLaneSnapshot {
                                    parameter_id: lane.parameter_id(),
                                    points: lane
                                        .points()
                                        .iter()
                                        .map(|point| TrackFxParameterAutomationPointSnapshot {
                                            sample: point.sample(),
                                            value: point.value(),
                                        })
                                        .collect(),
                                })
                                .collect(),
                        })
                        .collect(),
                    volume_automation: track.volume_automation.clone(),
                    razor_edit_areas: track.razor_edit_areas.clone(),
                    frozen_audio_item_id: track.frozen_audio_item_id.map(|id| id.value()),
                })
                .collect(),
            audio_items: self
                .state
                .audio_items
                .iter()
                .map(|item| AudioItemSnapshot {
                    fades: item.fades,
                    volume: item.volume,
                    take_volume: item.take_volume,
                    take_pan: item.take_pan,
                    id: item.id.value(),
                    track_id: item.track_id.value(),
                    media_ref: item.media_ref.clone(),
                    start_sample: item.start_sample,
                    source_offset_samples: item.source_offset_samples,
                    length_samples: item.length_samples,
                    muted: item.muted,
                    phase_inverted: item.phase_inverted,
                    loop_source: item.loop_source,
                    source_section_start_samples: item.source_section_start_samples,
                    source_section_length_samples: item.source_section_length_samples,
                })
                .collect(),
            midi_items: self
                .state
                .midi_items
                .iter()
                .map(|item| MidiItemSnapshot {
                    id: item.id.value(),
                    track_id: item.track_id.value(),
                    name: item.name.clone(),
                    start_tick: item.start_tick,
                    source_offset_ticks: item.source_offset_ticks,
                    loop_source: item.loop_source,
                    source_loop_length_ticks: item.source_loop_length_ticks,
                    length_ticks: item.length_ticks,
                    muted: item.muted,
                    notes: item
                        .notes
                        .iter()
                        .map(|note| MidiNoteSnapshot {
                            id: note.id.value(),
                            data: note.data,
                        })
                        .collect(),
                    controllers: item.controllers.as_ref().clone(),
                    pitch_bends: item.pitch_bends.as_ref().clone(),
                })
                .collect(),
            markers: self
                .state
                .markers
                .iter()
                .map(ProjectMarkerSnapshot::from)
                .collect(),
            show_project_markers: self.state.show_project_markers,
            show_project_regions: self.state.show_project_regions,
            regions: self
                .state
                .regions
                .iter()
                .map(ProjectRegionSnapshot::from)
                .collect(),
            tempo_points: self
                .state
                .tempo_map
                .points()
                .map(|(start_tick, bpm, curve_to_next)| TempoPointSnapshot {
                    start_tick,
                    bpm,
                    curve_to_next,
                })
                .collect(),
            meter_points: self
                .state
                .meter_map
                .points()
                .map(|(start_tick, signature)| MeterPointSnapshot {
                    start_tick,
                    numerator: signature.numerator(),
                    denominator: signature.denominator(),
                })
                .collect(),
        }
    }

    /// Restores project state from a validated snapshot with a fresh undo history.
    pub fn from_snapshot(snapshot: ProjectSnapshot) -> Result<Self, SnapshotError> {
        let settings = snapshot.settings;
        let show_project_markers = snapshot.show_project_markers;
        let show_project_regions = snapshot.show_project_regions;
        let mut send_ids = HashSet::new();
        let mut max_send_id = None;
        let mut tempo_map = TempoMap::new(settings);
        let first_tempo = snapshot
            .tempo_points
            .first()
            .ok_or(SnapshotError::InvalidProjectData)?;
        if first_tempo.start_tick != 0 || first_tempo.bpm != settings.initial_tempo_bpm() {
            return Err(SnapshotError::InvalidProjectData);
        }
        let mut previous_tick = 0;
        for point in snapshot.tempo_points.iter().skip(1) {
            if point.start_tick <= previous_tick {
                return Err(SnapshotError::InvalidProjectData);
            }
            tempo_map
                .set_point(point.start_tick, Some(point.bpm))
                .map_err(SnapshotError::InvalidTimebase)?;
            previous_tick = point.start_tick;
        }
        for point in &snapshot.tempo_points {
            tempo_map
                .set_curve(point.start_tick, point.curve_to_next)
                .map_err(SnapshotError::InvalidTimebase)?;
        }

        let mut meter_map = MeterMap::new(settings.ppq(), settings.start_measure());
        let first_meter = snapshot
            .meter_points
            .first()
            .ok_or(SnapshotError::InvalidProjectData)?;
        if first_meter.start_tick != 0 {
            return Err(SnapshotError::InvalidProjectData);
        }
        let initial_signature = TimeSignature::new(first_meter.numerator, first_meter.denominator)
            .map_err(SnapshotError::InvalidTimebase)?;
        meter_map
            .set_point(0, Some(initial_signature))
            .map_err(SnapshotError::InvalidTimebase)?;
        previous_tick = 0;
        for point in snapshot.meter_points.iter().skip(1) {
            if point.start_tick <= previous_tick {
                return Err(SnapshotError::InvalidProjectData);
            }
            let signature = TimeSignature::new(point.numerator, point.denominator)
                .map_err(SnapshotError::InvalidTimebase)?;
            meter_map
                .set_point(point.start_tick, Some(signature))
                .map_err(SnapshotError::InvalidTimebase)?;
            previous_tick = point.start_tick;
        }

        let mut marker_ids = HashSet::with_capacity(snapshot.markers.len());
        let mut max_marker_id = None;
        let mut markers = Vec::with_capacity(snapshot.markers.len());
        for marker in snapshot.markers {
            if !marker_ids.insert(marker.id) {
                return Err(SnapshotError::InvalidProjectData);
            }
            max_marker_id = Some(max_marker_id.map_or(marker.id, |max: u64| max.max(marker.id)));
            let mut project_marker = ProjectMarker::new(
                MarkerId::from_raw(marker.id),
                marker.number,
                marker.position_ticks,
                marker.name,
            )
            .ok_or(SnapshotError::InvalidProjectData)?;
            project_marker.visible = marker.visible;
            markers.push(project_marker);
        }
        markers.sort_unstable_by_key(|marker| (marker.position_ticks, marker.id.value()));

        let mut region_ids = HashSet::with_capacity(snapshot.regions.len());
        let mut max_region_id = None;
        let mut regions = Vec::with_capacity(snapshot.regions.len());
        for region in snapshot.regions {
            if !region_ids.insert(region.id) {
                return Err(SnapshotError::InvalidProjectData);
            }
            max_region_id = Some(max_region_id.map_or(region.id, |max: u64| max.max(region.id)));
            let mut project_region = ProjectRegion::new(
                RegionId::from_raw(region.id),
                region.number,
                region.start_ticks,
                region.end_ticks,
                region.name,
            )
            .ok_or(SnapshotError::InvalidProjectData)?;
            project_region.visible = region.visible;
            regions.push(project_region);
        }
        regions.sort_unstable_by_key(|region| (region.start_ticks, region.id.value()));

        let mut track_ids = HashSet::with_capacity(snapshot.tracks.len());
        let mut tracks = Vec::with_capacity(snapshot.tracks.len());
        let mut max_track_id = None;
        for track in snapshot.tracks {
            let instrument = match track.instrument {
                Some(instrument) => Some(
                    TrackInstrument::new(instrument.plugin_id, instrument.bundle_path)
                        .ok_or(SnapshotError::InvalidProjectData)?
                        .with_state(instrument.state),
                ),
                None => None,
            };
            let fx_chain = track
                .fx_chain
                .into_iter()
                .map(|plugin| {
                    let mut plugin_ref = TrackFxPlugin::new(plugin.plugin_id, plugin.bundle_path)
                        .map(|plugin_ref| {
                            plugin_ref
                                .with_enabled(plugin.enabled)
                                .with_state(plugin.state)
                        })
                        .ok_or(SnapshotError::InvalidProjectData)?;
                    let mut parameter_ids = HashSet::with_capacity(plugin.parameter_values.len());
                    for parameter in plugin.parameter_values {
                        if !parameter.value.is_finite()
                            || !parameter_ids.insert(parameter.parameter_id)
                        {
                            return Err(SnapshotError::InvalidProjectData);
                        }
                        plugin_ref = plugin_ref
                            .with_parameter_value(parameter.parameter_id, parameter.value);
                    }
                    let mut automation_ids =
                        HashSet::with_capacity(plugin.parameter_automation.len());
                    if plugin.parameter_automation.len() > MAX_TRACK_FX_PARAMETER_AUTOMATION_LANES {
                        return Err(SnapshotError::InvalidProjectData);
                    }
                    for lane in plugin.parameter_automation {
                        if !automation_ids.insert(lane.parameter_id) {
                            return Err(SnapshotError::InvalidProjectData);
                        }
                        if lane.points.len() > MAX_FX_PARAMETER_AUTOMATION_POINTS {
                            return Err(SnapshotError::InvalidProjectData);
                        }
                        let lane = FxParameterAutomationLane::try_from(lane)?;
                        plugin_ref =
                            plugin_ref.with_parameter_automation(lane.parameter_id(), Some(lane));
                    }
                    Ok(plugin_ref)
                })
                .collect::<Result<Vec<_>, _>>()?;
            if !track_ids.insert(track.id)
                || !track.volume_db.is_finite()
                || !track.pan.is_finite()
                || !(-1.0..=1.0).contains(&track.pan)
                || !valid_volume_automation(&track.volume_automation)
                || !valid_razor_edit_areas(&track.razor_edit_areas)
            {
                return Err(SnapshotError::InvalidProjectData);
            }
            max_track_id = Some(max_track_id.map_or(track.id, |max: u64| max.max(track.id)));
            let mut sends = Vec::new();
            for send in track.sends {
                if !send_ids.insert(send.id) || !send.parameters.is_valid() {
                    return Err(SnapshotError::InvalidProjectData);
                }
                max_send_id = Some(max_send_id.map_or(send.id, |max: u64| max.max(send.id)));
                sends.push(crate::AudioSend {
                    id: crate::SendId::from_value(send.id),
                    destination: TrackId::from_raw(send.destination_track_id),
                    parameters: send.parameters,
                });
            }
            tracks.push(Track {
                id: TrackId::from_raw(track.id),
                name: track.name,
                is_bus: track.is_bus,
                is_folder: track.is_folder,
                parent_track: track.parent_track_id.map(TrackId::from_raw),
                output_track: track.output_track_id.map(TrackId::from_raw),
                main_send_enabled: track.main_send_enabled,
                sends,
                volume_db: track.volume_db,
                pan: track.pan,
                pan_mode: settings.pan_mode(),
                muted: track.muted,
                phase_inverted: track.phase_inverted,
                solo: track.solo,
                record_armed: track.record_armed,
                instrument,
                fx_chain,
                volume_automation: track.volume_automation,
                razor_edit_areas: track.razor_edit_areas,
                frozen_audio_item_id: track.frozen_audio_item_id.map(ItemId::from_raw),
            });
        }
        if !valid_track_hierarchy(&tracks) || !valid_track_routing(&tracks) {
            return Err(SnapshotError::InvalidProjectData);
        }

        let mut item_ids = HashSet::with_capacity(
            snapshot
                .audio_items
                .len()
                .saturating_add(snapshot.midi_items.len()),
        );
        let mut audio_items = Vec::with_capacity(snapshot.audio_items.len());
        let mut midi_items = Vec::with_capacity(snapshot.midi_items.len());
        let mut max_item_id = None;
        for item in snapshot.audio_items {
            if !item_ids.insert(item.id)
                || !track_ids.contains(&item.track_id)
                || item.media_ref.trim().is_empty()
                || !item.volume.is_finite()
                || !(0.0..=crate::MAX_AUDIO_ITEM_VOLUME).contains(&item.volume)
                || !item.take_volume.is_finite()
                || !(0.0..=crate::MAX_AUDIO_ITEM_TAKE_VOLUME).contains(&item.take_volume)
                || !item.take_pan.is_finite()
                || !(-1.0..=1.0).contains(&item.take_pan)
                || item.length_samples == 0
                || item.start_sample.checked_add(item.length_samples).is_none()
                || !valid_audio_source_offset(
                    item.source_offset_samples,
                    item.loop_source,
                    item.source_section_start_samples,
                    item.source_section_length_samples,
                )
            {
                return Err(SnapshotError::InvalidProjectData);
            }
            max_item_id = Some(max_item_id.map_or(item.id, |max: u64| max.max(item.id)));
            audio_items.push(AudioItem {
                fades: item.fades,
                volume: item.volume,
                take_volume: item.take_volume,
                take_pan: item.take_pan,
                id: ItemId::from_raw(item.id),
                track_id: TrackId::from_raw(item.track_id),
                media_ref: item.media_ref,
                start_sample: item.start_sample,
                source_offset_samples: item.source_offset_samples,
                length_samples: item.length_samples,
                muted: item.muted,
                phase_inverted: item.phase_inverted,
                loop_source: item.loop_source,
                source_section_start_samples: item.source_section_start_samples,
                source_section_length_samples: item.source_section_length_samples,
            });
        }
        let mut note_ids = HashSet::new();
        let mut max_note_id = None;
        for item in snapshot.midi_items {
            if !item_ids.insert(item.id)
                || !track_ids.contains(&item.track_id)
                || item.name.trim().is_empty()
                || item.name.chars().count() > 128
                || item.length_ticks == 0
                || item.source_loop_length_ticks == 0
                || item.start_tick.checked_add(item.length_ticks).is_none()
                || item
                    .source_offset_ticks
                    .checked_add(item.length_ticks)
                    .is_none()
            {
                return Err(SnapshotError::InvalidProjectData);
            }
            max_item_id = Some(max_item_id.map_or(item.id, |max: u64| max.max(item.id)));
            let mut notes = Vec::with_capacity(item.notes.len());
            for note in item.notes {
                let data = note.data;
                if !note_ids.insert(note.id)
                    || data.pitch > 127
                    || data.velocity > 127
                    || data.duration == 0
                    || data.tick.checked_add(data.duration).is_none()
                {
                    return Err(SnapshotError::InvalidProjectData);
                }
                max_note_id = Some(max_note_id.map_or(note.id, |max: u64| max.max(note.id)));
                notes.push(MidiNote {
                    id: NoteId::from_raw(note.id),
                    data,
                });
            }
            let mut controllers = item.controllers;
            controllers.sort_unstable_by_key(|controller| (controller.tick, controller.controller));
            if !valid_midi_controllers(&controllers) {
                return Err(SnapshotError::InvalidProjectData);
            }
            let mut pitch_bends = item.pitch_bends;
            pitch_bends.sort_unstable_by_key(|bend| bend.tick);
            if !valid_midi_pitch_bends(&pitch_bends) {
                return Err(SnapshotError::InvalidProjectData);
            }
            midi_items.push(MidiItem {
                id: ItemId::from_raw(item.id),
                track_id: TrackId::from_raw(item.track_id),
                name: item.name,
                start_tick: item.start_tick,
                source_offset_ticks: item.source_offset_ticks,
                loop_source: item.loop_source,
                source_loop_length_ticks: item.source_loop_length_ticks,
                length_ticks: item.length_ticks,
                muted: item.muted,
                notes: Arc::new(notes),
                controllers: Arc::new(controllers),
                pitch_bends: Arc::new(pitch_bends),
            });
        }

        let mut frozen_render_ids = HashSet::new();
        for track in &tracks {
            let Some(render_id) = track.frozen_audio_item_id else {
                continue;
            };
            let Some(render) = audio_items.iter().find(|item| item.id == render_id) else {
                return Err(SnapshotError::InvalidProjectData);
            };
            if render.track_id != track.id
                || track.is_bus
                || track.is_folder
                || track.instrument.is_none()
                || !frozen_render_ids.insert(render_id)
                || !midi_items
                    .iter()
                    .any(|item| item.track_id == track.id && !item.notes().is_empty())
            {
                return Err(SnapshotError::InvalidProjectData);
            }
        }

        Ok(Self {
            state: ProjectState {
                notes: snapshot.notes,
                master_mix: snapshot.master_mix,
                pan_mode: settings.pan_mode(),
                frame_rate: settings.frame_rate(),
                base_ruler_label_spacing_on_measure: settings.base_ruler_label_spacing_on_measure(),
                project_start_time_offset_seconds: settings.project_start_time_offset_seconds(),
                project_start_time_fractional_frame_offset_allowed: settings
                    .project_start_time_fractional_frame_offset_allowed(),
                tracks,
                audio_items,
                midi_items,
                markers,
                show_project_markers,
                show_project_regions,
                regions,
                tempo_map,
                meter_map,
            },
            ids: IdAllocator {
                next_track_id: next_id(max_track_id)?,
                next_send_id: next_id(max_send_id)?,
                next_item_id: next_id(max_item_id)?,
                next_note_id: next_id(max_note_id)?,
                next_marker_id: next_id(max_marker_id)?,
                next_region_id: next_id(max_region_id)?,
            },
            history: Vec::new(),
            history_cursor: 0,
            last_fx_parameter_change: None,
        })
    }

    fn apply_action(
        state: &mut ProjectState,
        ids: &mut IdAllocator,
        action: DawAction,
    ) -> Result<ProjectEvent, ActionError> {
        let action = match action {
            DawAction::BatchTransaction { tx_id, actions } => {
                let mut events = Vec::with_capacity(actions.len());
                for action in actions {
                    events.push(Self::apply_action(state, ids, action)?);
                }
                return Ok(ProjectEvent::Transaction { tx_id, events });
            }
            action => action,
        };

        if let Some(track_id) = source_track_for_action(state, &action)
            && state
                .tracks
                .iter()
                .any(|track| track.id == track_id && track.is_frozen())
        {
            return Err(ActionError::CannotEditFrozenTrackSource { track_id });
        }

        let event = match action {
            DawAction::SetProjectNotes { notes } => ProjectEvent::ProjectNotesChanged {
                before: state.notes.clone(),
                after: notes,
            },
            DawAction::SetFrameRate { rate } => ProjectEvent::FrameRateChanged {
                before: state.frame_rate,
                after: rate,
            },
            DawAction::SetProjectSampleRate { sample_rate } => {
                let before = ProjectSampleClockSnapshot::capture(state);
                let mut after = before.clone();
                after.rescale(sample_rate)?;
                ProjectEvent::ProjectSampleRateChanged { before, after }
            }
            DawAction::SetForceTempoChangesToWholeSamples { enabled } => {
                ProjectEvent::ForceTempoChangesToWholeSamplesChanged {
                    before: state
                        .tempo_map
                        .settings()
                        .force_tempo_changes_to_whole_samples(),
                    after: enabled,
                }
            }
            DawAction::SetProjectBpmBasis { basis } => ProjectEvent::BpmBasisChanged {
                before: state.tempo_map.settings().bpm_basis(),
                after: basis,
            },
            DawAction::SetBaseRulerLabelSpacingOnMeasure { enabled } => {
                ProjectEvent::BaseRulerLabelSpacingChanged {
                    before: state.base_ruler_label_spacing_on_measure,
                    after: enabled,
                }
            }
            DawAction::SetProjectStartTimeOffsetSeconds { seconds } => {
                if !seconds.is_finite() {
                    return Err(ActionError::InvalidProjectTimeOffset);
                }
                ProjectEvent::ProjectStartTimeOffsetChanged {
                    before: state.project_start_time_offset_seconds,
                    after: seconds,
                }
            }
            DawAction::SetProjectStartTimeFractionalFrameOffsetAllowed { allowed } => {
                ProjectEvent::ProjectStartTimeFractionalFrameOffsetAllowedChanged {
                    before: state.project_start_time_fractional_frame_offset_allowed,
                    after: allowed,
                }
            }
            DawAction::SetProjectStartMeasure { measure } => {
                ProjectEvent::ProjectStartMeasureChanged {
                    before: state.meter_map.start_measure(),
                    after: measure,
                }
            }
            DawAction::SetMasterMix { mix } => ProjectEvent::MasterMixChanged {
                before: state.master_mix,
                after: mix,
            },
            DawAction::InsertProjectMarker {
                position_ticks,
                name,
            } => {
                let next_marker_id = ids
                    .next_marker_id
                    .checked_add(1)
                    .ok_or(ActionError::MarkerIdExhausted)?;
                let number = lowest_available_marker_number(&state.markers)
                    .ok_or(ActionError::ProjectMarkerNumberExhausted)?;
                let marker = ProjectMarker::new(
                    MarkerId::from_raw(ids.next_marker_id),
                    number,
                    position_ticks,
                    name,
                )
                .ok_or(ActionError::InvalidProjectMarker)?;
                let before = state.markers.clone();
                let mut after = before.clone();
                after.push(marker);
                after.sort_unstable_by_key(|marker| (marker.position_ticks, marker.id.value()));
                ids.next_marker_id = next_marker_id;
                ProjectEvent::ProjectMarkersChanged { before, after }
            }
            DawAction::InsertNumberedProjectMarker {
                number,
                position_ticks,
                name,
            } => {
                let next_marker_id = ids
                    .next_marker_id
                    .checked_add(1)
                    .ok_or(ActionError::MarkerIdExhausted)?;
                let marker = ProjectMarker::new(
                    MarkerId::from_raw(ids.next_marker_id),
                    number,
                    position_ticks,
                    name,
                )
                .ok_or(ActionError::InvalidProjectMarker)?;
                let before = state.markers.clone();
                let mut after = before.clone();
                after.push(marker);
                after.sort_unstable_by_key(|marker| (marker.position_ticks, marker.id.value()));
                ids.next_marker_id = next_marker_id;
                ProjectEvent::ProjectMarkersChanged { before, after }
            }
            DawAction::UpdateProjectMarker {
                marker_id,
                position_ticks,
                name,
            } => {
                let before = state.markers.clone();
                let mut after = before.clone();
                let marker = after
                    .iter_mut()
                    .find(|marker| marker.id == marker_id)
                    .ok_or(ActionError::ProjectMarkerNotFound { marker_id })?;
                let visible = marker.visible;
                *marker = ProjectMarker::new(marker_id, marker.number, position_ticks, name)
                    .ok_or(ActionError::InvalidProjectMarker)?;
                marker.visible = visible;
                after.sort_unstable_by_key(|marker| (marker.position_ticks, marker.id.value()));
                ProjectEvent::ProjectMarkersChanged { before, after }
            }
            DawAction::UpdateProjectMarkerProperties {
                marker_id,
                number,
                position_ticks,
                name,
            } => {
                let before = state.markers.clone();
                let mut after = before.clone();
                let marker = after
                    .iter_mut()
                    .find(|marker| marker.id == marker_id)
                    .ok_or(ActionError::ProjectMarkerNotFound { marker_id })?;
                let visible = marker.visible;
                *marker = ProjectMarker::new(marker_id, number, position_ticks, name)
                    .ok_or(ActionError::InvalidProjectMarker)?;
                marker.visible = visible;
                after.sort_unstable_by_key(|marker| (marker.position_ticks, marker.id.value()));
                ProjectEvent::ProjectMarkersChanged { before, after }
            }
            DawAction::SetProjectMarkerVisibility { marker_id, visible } => {
                let before = state.markers.clone();
                let mut after = before.clone();
                let marker = after
                    .iter_mut()
                    .find(|marker| marker.id == marker_id)
                    .ok_or(ActionError::ProjectMarkerNotFound { marker_id })?;
                marker.visible = visible;
                ProjectEvent::ProjectMarkersChanged { before, after }
            }
            DawAction::SetProjectMarkersVisibleOnRuler { visible } => {
                ProjectEvent::ProjectMarkerLaneVisibilityChanged {
                    before: state.show_project_markers,
                    after: visible,
                }
            }
            DawAction::SetProjectRegionsVisibleOnRuler { visible } => {
                ProjectEvent::ProjectRegionLaneVisibilityChanged {
                    before: state.show_project_regions,
                    after: visible,
                }
            }
            DawAction::SetProjectRegionVisibility { region_id, visible } => {
                let before = state.regions.clone();
                let mut after = before.clone();
                let region = after
                    .iter_mut()
                    .find(|region| region.id == region_id)
                    .ok_or(ActionError::ProjectRegionNotFound { region_id })?;
                region.visible = visible;
                ProjectEvent::ProjectRegionsChanged { before, after }
            }
            DawAction::DeleteProjectMarker { marker_id } => {
                let before = state.markers.clone();
                if !before.iter().any(|marker| marker.id == marker_id) {
                    return Err(ActionError::ProjectMarkerNotFound { marker_id });
                }
                let after = before
                    .iter()
                    .filter(|marker| marker.id != marker_id)
                    .cloned()
                    .collect();
                ProjectEvent::ProjectMarkersChanged { before, after }
            }
            DawAction::InsertProjectRegion {
                start_ticks,
                end_ticks,
                name,
            } => {
                let next_region_id = ids
                    .next_region_id
                    .checked_add(1)
                    .ok_or(ActionError::RegionIdExhausted)?;
                let number = lowest_available_region_number(&state.regions)
                    .ok_or(ActionError::ProjectRegionNumberExhausted)?;
                let region = ProjectRegion::new(
                    RegionId::from_raw(ids.next_region_id),
                    number,
                    start_ticks,
                    end_ticks,
                    name,
                )
                .ok_or(ActionError::InvalidProjectRegion)?;
                let before = state.regions.clone();
                let mut after = before.clone();
                after.push(region);
                after.sort_unstable_by_key(|region| (region.start_ticks, region.id.value()));
                ids.next_region_id = next_region_id;
                ProjectEvent::ProjectRegionsChanged { before, after }
            }
            DawAction::UpdateProjectRegion {
                region_id,
                number,
                start_ticks,
                end_ticks,
                name,
            } => {
                let before = state.regions.clone();
                let mut after = before.clone();
                let region = after
                    .iter_mut()
                    .find(|region| region.id == region_id)
                    .ok_or(ActionError::ProjectRegionNotFound { region_id })?;
                let visible = region.visible;
                *region = ProjectRegion::new(region_id, number, start_ticks, end_ticks, name)
                    .ok_or(ActionError::InvalidProjectRegion)?;
                region.visible = visible;
                after.sort_unstable_by_key(|region| (region.start_ticks, region.id.value()));
                ProjectEvent::ProjectRegionsChanged { before, after }
            }
            DawAction::DeleteProjectRegion { region_id } => {
                let before = state.regions.clone();
                if !before.iter().any(|region| region.id == region_id) {
                    return Err(ActionError::ProjectRegionNotFound { region_id });
                }
                let after = before
                    .iter()
                    .filter(|region| region.id != region_id)
                    .cloned()
                    .collect();
                ProjectEvent::ProjectRegionsChanged { before, after }
            }
            DawAction::CreateTrack { index, name } => {
                if index > state.tracks.len() {
                    return Err(ActionError::TrackIndexOutOfBounds {
                        index,
                        track_count: state.tracks.len(),
                    });
                }
                let next_id = ids
                    .next_track_id
                    .checked_add(1)
                    .ok_or(ActionError::TrackIdExhausted)?;
                let track = Track {
                    id: TrackId::from_raw(ids.next_track_id),
                    name,
                    is_bus: false,
                    is_folder: false,
                    parent_track: state.tracks.get(index).and_then(|next| next.parent_track),
                    output_track: None,
                    main_send_enabled: true,
                    sends: Vec::new(),
                    volume_db: 0.0,
                    pan: 0.0,
                    pan_mode: state.pan_mode,
                    muted: false,
                    phase_inverted: false,
                    solo: false,
                    record_armed: false,
                    instrument: None,
                    fx_chain: Vec::new(),
                    volume_automation: Vec::new(),
                    razor_edit_areas: Vec::new(),
                    frozen_audio_item_id: None,
                };
                ids.next_track_id = next_id;
                ProjectEvent::TrackCreated { index, track }
            }
            DawAction::CreateBusTrack { index, name } => {
                if index > state.tracks.len() {
                    return Err(ActionError::TrackIndexOutOfBounds {
                        index,
                        track_count: state.tracks.len(),
                    });
                }
                let next_id = ids
                    .next_track_id
                    .checked_add(1)
                    .ok_or(ActionError::TrackIdExhausted)?;
                let track = Track {
                    id: TrackId::from_raw(ids.next_track_id),
                    name,
                    is_bus: true,
                    is_folder: false,
                    parent_track: state.tracks.get(index).and_then(|next| next.parent_track),
                    output_track: None,
                    main_send_enabled: true,
                    sends: Vec::new(),
                    volume_db: 0.0,
                    pan: 0.0,
                    pan_mode: state.pan_mode,
                    muted: false,
                    phase_inverted: false,
                    solo: false,
                    record_armed: false,
                    instrument: None,
                    fx_chain: Vec::new(),
                    volume_automation: Vec::new(),
                    razor_edit_areas: Vec::new(),
                    frozen_audio_item_id: None,
                };
                ids.next_track_id = next_id;
                ProjectEvent::TrackCreated { index, track }
            }
            DawAction::SetTempo { start_tick, bpm } => {
                if !bpm.is_finite() || bpm <= 0.0 {
                    return Err(ActionError::InvalidTempoBpm);
                }
                let mut candidate_map = state.tempo_map.clone();
                candidate_map
                    .set_point(start_tick, Some(bpm))
                    .map_err(|error| match error {
                        TimebaseError::InvalidTempo => ActionError::InvalidTempoBpm,
                        _ => ActionError::TempoMapOutOfRange,
                    })?;
                ProjectEvent::TempoChanged {
                    start_tick,
                    before: state.tempo_map.point_at(start_tick),
                    after: Some(bpm),
                }
            }
            DawAction::DeleteTempoPoint { start_tick } => {
                let before = state
                    .tempo_map
                    .point_at(start_tick)
                    .ok_or(ActionError::TempoPointNotFound { start_tick })?;
                let mut candidate_map = state.tempo_map.clone();
                candidate_map
                    .set_point(start_tick, None)
                    .map_err(|error| match error {
                        TimebaseError::CannotRemoveInitialTempo => {
                            ActionError::CannotRemoveInitialTempo
                        }
                        TimebaseError::TempoPointNotFound => {
                            ActionError::TempoPointNotFound { start_tick }
                        }
                        _ => ActionError::TempoMapOutOfRange,
                    })?;
                ProjectEvent::TempoChanged {
                    start_tick,
                    before: Some(before),
                    after: None,
                }
            }
            DawAction::SetTempoCurve { start_tick, curve } => {
                let before = state
                    .tempo_map
                    .curve_at(start_tick)
                    .ok_or(ActionError::TempoPointNotFound { start_tick })?;
                let mut candidate_map = state.tempo_map.clone();
                candidate_map
                    .set_curve(start_tick, curve)
                    .map_err(|_| ActionError::TempoMapOutOfRange)?;
                ProjectEvent::TempoCurveChanged {
                    start_tick,
                    before,
                    after: curve,
                }
            }
            DawAction::SetTimeSignature {
                start_tick,
                signature,
            } => {
                let mut candidate_map = state.meter_map.clone();
                candidate_map
                    .set_point(start_tick, Some(signature))
                    .map_err(|error| match error {
                        TimebaseError::InvalidTimeSignature => ActionError::InvalidTimeSignature,
                        TimebaseError::MeterChangeNotOnBarBoundary => {
                            ActionError::MeterChangeNotOnBarBoundary
                        }
                        _ => ActionError::MeterMapOutOfRange,
                    })?;
                ProjectEvent::MeterChanged {
                    start_tick,
                    before: state.meter_map.point_at(start_tick),
                    after: Some(signature),
                }
            }
            DawAction::SetTimeSignatureMap { points } => {
                let before = state
                    .meter_map
                    .points()
                    .map(|(start_tick, signature)| MeterPointSnapshot {
                        start_tick,
                        numerator: signature.numerator(),
                        denominator: signature.denominator(),
                    })
                    .collect::<Vec<_>>();
                let mut candidate_map = state.meter_map.clone();
                let candidates = points
                    .iter()
                    .map(|point| {
                        TimeSignature::new(point.numerator, point.denominator)
                            .map(|signature| (point.start_tick, signature))
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| ActionError::InvalidTimeSignature)?;
                candidate_map
                    .replace_points(&candidates)
                    .map_err(|error| match error {
                        TimebaseError::InvalidTimeSignature => ActionError::InvalidTimeSignature,
                        TimebaseError::MeterChangeNotOnBarBoundary => {
                            ActionError::MeterChangeNotOnBarBoundary
                        }
                        TimebaseError::CannotRemoveInitialMeter => {
                            ActionError::CannotRemoveInitialMeter
                        }
                        TimebaseError::InvalidMeterMap => ActionError::InvalidMeterMap,
                        _ => ActionError::MeterMapOutOfRange,
                    })?;
                ProjectEvent::MeterMapReplaced {
                    before,
                    after: points,
                }
            }
            DawAction::SetTrackVolume {
                track_id,
                volume_db,
            } => {
                if !volume_db.is_finite() {
                    return Err(ActionError::InvalidVolumeDb);
                }
                let track = state
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                ProjectEvent::TrackVolumeChanged {
                    track_id,
                    before: track.volume_db,
                    after: volume_db,
                }
            }
            DawAction::SetTrackVolumeAutomation { track_id, points } => {
                if !valid_volume_automation(&points) {
                    return Err(ActionError::InvalidVolumeAutomation);
                }
                let track = state
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                ProjectEvent::TrackVolumeAutomationChanged {
                    track_id,
                    before: track.volume_automation.clone(),
                    after: points,
                }
            }
            DawAction::SetTrackRazorEditAreas { mut track_areas } => {
                let mut seen = HashSet::with_capacity(track_areas.len());
                if track_areas.iter().any(|(track_id, areas)| {
                    !seen.insert(*track_id) || !valid_razor_edit_areas(areas)
                }) {
                    return Err(ActionError::InvalidRazorEditAreas);
                }
                track_areas.sort_unstable_by_key(|(track_id, _)| track_id.value());
                let mut before = Vec::with_capacity(track_areas.len());
                let mut after = Vec::with_capacity(track_areas.len());
                for (track_id, areas) in track_areas {
                    let track = state
                        .tracks
                        .iter()
                        .find(|track| track.id == track_id)
                        .ok_or(ActionError::TrackNotFound { track_id })?;
                    before.push((track_id, track.razor_edit_areas.clone()));
                    after.push((track_id, areas));
                }
                ProjectEvent::TrackRazorEditAreasChanged { before, after }
            }
            DawAction::SetTrackMainSend { track_id, enabled } => {
                let track = state
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                ProjectEvent::TrackMainSendChanged {
                    track_id,
                    before: track.main_send_enabled,
                    after: enabled,
                }
            }
            DawAction::CreateAudioSend {
                track_id,
                destination,
                parameters,
            } => {
                let next = ids
                    .next_send_id
                    .checked_add(1)
                    .ok_or(ActionError::SendIdExhausted)?;
                let track = state
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                let mut after = track.sends.clone();
                after.push(crate::AudioSend {
                    id: crate::SendId::from_value(ids.next_send_id),
                    destination,
                    parameters,
                });
                validate_send_edit(&state.tracks, track_id, &after)?;
                ids.next_send_id = next;
                ProjectEvent::TrackSendsChanged {
                    track_id,
                    before: track.sends.clone(),
                    after,
                }
            }
            DawAction::UpdateAudioSend {
                track_id,
                send_id,
                destination,
                parameters,
            } => {
                let track = state
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                let mut after = track.sends.clone();
                let send = after
                    .iter_mut()
                    .find(|send| send.id == send_id)
                    .ok_or(ActionError::AudioSendNotFound { send_id })?;
                send.destination = destination;
                send.parameters = parameters;
                validate_send_edit(&state.tracks, track_id, &after)?;
                ProjectEvent::TrackSendsChanged {
                    track_id,
                    before: track.sends.clone(),
                    after,
                }
            }
            DawAction::DeleteAudioSend { track_id, send_id } => {
                let track = state
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                let mut after = track.sends.clone();
                let index = after
                    .iter()
                    .position(|send| send.id == send_id)
                    .ok_or(ActionError::AudioSendNotFound { send_id })?;
                after.remove(index);
                ProjectEvent::TrackSendsChanged {
                    track_id,
                    before: track.sends.clone(),
                    after,
                }
            }
            DawAction::SetTrackOutput {
                track_id,
                output_track,
            } => {
                let source = state
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                if output_track == Some(track_id) {
                    return Err(ActionError::InvalidTrackOutput);
                }
                if output_track
                    .is_some_and(|target| !state.tracks.iter().any(|track| track.id == target))
                {
                    return Err(ActionError::InvalidTrackOutput);
                }
                let mut candidate = state.tracks.clone();
                candidate
                    .iter_mut()
                    .find(|track| track.id == track_id)
                    .unwrap()
                    .output_track = output_track;
                if !valid_track_routing(&candidate) {
                    return Err(ActionError::TrackRoutingCycle);
                }
                ProjectEvent::TrackOutputChanged {
                    track_id,
                    before: source.output_track,
                    after: output_track,
                }
            }
            DawAction::SetTrackFolder { track_id, enabled } => {
                let mut after = state.tracks.clone();
                let track = after
                    .iter_mut()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                if enabled && track.frozen_audio_item_id.is_some() {
                    return Err(ActionError::CannotEditFrozenTrackSource { track_id });
                }
                track.is_folder = enabled;
                structure_change(&state.tracks, &after)?
            }
            DawAction::SetTrackParent { track_id, parent } => {
                let after = reparent_tracks(&state.tracks, track_id, parent)?;
                structure_change(&state.tracks, &after)?
            }
            DawAction::SetTrackPan { track_id, pan } => {
                if !pan.is_finite() || !(-1.0..=1.0).contains(&pan) {
                    return Err(ActionError::InvalidPan);
                }
                let track = state
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                ProjectEvent::TrackPanChanged {
                    track_id,
                    before: track.pan,
                    after: pan,
                }
            }
            DawAction::SetTrackPhase {
                track_id,
                phase_inverted,
            } => {
                let track = state
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                ProjectEvent::TrackPhaseChanged {
                    track_id,
                    before: track.phase_inverted,
                    after: phase_inverted,
                }
            }
            DawAction::SetTrackMute { track_id, muted } => {
                let track = state
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                ProjectEvent::TrackMuteChanged {
                    track_id,
                    before: track.muted,
                    after: muted,
                }
            }
            DawAction::SetTrackSolo { track_id, solo } => {
                let track = state
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                ProjectEvent::TrackSoloChanged {
                    track_id,
                    before: track.solo,
                    after: solo,
                }
            }
            DawAction::SetTrackRecordArm { track_id, armed } => {
                let track = state
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                ProjectEvent::TrackRecordArmChanged {
                    track_id,
                    before: track.record_armed,
                    after: armed,
                }
            }
            DawAction::SetTrackInstrument {
                track_id,
                instrument,
            } => {
                if instrument.as_ref().is_some_and(|value| {
                    value.plugin_id().trim().is_empty() || value.bundle_path().trim().is_empty()
                }) {
                    return Err(ActionError::InvalidTrackInstrument);
                }
                let track = state
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                ProjectEvent::TrackInstrumentChanged {
                    track_id,
                    before: track.instrument.clone(),
                    after: instrument,
                }
            }
            DawAction::FreezeTrack {
                track_id,
                media_ref,
                start_sample,
                length_samples,
            } => {
                if media_ref.trim().is_empty() {
                    return Err(ActionError::InvalidAudioMediaRef);
                }
                if length_samples == 0 {
                    return Err(ActionError::InvalidAudioItemLength);
                }
                if start_sample.checked_add(length_samples).is_none() {
                    return Err(ActionError::InvalidAudioItemPosition);
                }
                let track = state
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                if track.frozen_audio_item_id.is_some() {
                    return Err(ActionError::TrackAlreadyFrozen { track_id });
                }
                if track.is_bus
                    || track.is_folder
                    || track.instrument.is_none()
                    || state
                        .audio_items
                        .iter()
                        .any(|item| item.track_id == track_id)
                    || !state
                        .midi_items
                        .iter()
                        .any(|item| item.track_id == track_id && !item.notes().is_empty())
                {
                    return Err(ActionError::TrackCannotBeFrozen);
                }
                let next_id = ids
                    .next_item_id
                    .checked_add(1)
                    .ok_or(ActionError::ItemIdExhausted)?;
                let item = AudioItem {
                    fades: crate::AudioItemFades::default(),
                    volume: 1.0,
                    take_volume: 1.0,
                    take_pan: 0.0,
                    id: ItemId::from_raw(ids.next_item_id),
                    track_id,
                    media_ref,
                    start_sample,
                    source_offset_samples: 0,
                    length_samples,
                    muted: false,
                    phase_inverted: false,
                    loop_source: false,
                    source_section_start_samples: None,
                    source_section_length_samples: None,
                };
                ids.next_item_id = next_id;
                ProjectEvent::TrackFreezeChanged {
                    track_id,
                    before: None,
                    after: Some(item.id),
                    before_render: None,
                    after_render: Some((state.audio_items.len(), item)),
                }
            }
            DawAction::UnfreezeTrack { track_id } => {
                let track = state
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                let frozen_audio_item_id = track
                    .frozen_audio_item_id
                    .ok_or(ActionError::TrackNotFrozen { track_id })?;
                let index = state
                    .audio_items
                    .iter()
                    .position(|item| item.id == frozen_audio_item_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                ProjectEvent::TrackFreezeChanged {
                    track_id,
                    before: Some(frozen_audio_item_id),
                    after: None,
                    before_render: Some((index, state.audio_items[index].clone())),
                    after_render: None,
                }
            }
            DawAction::SetTrackFxChain { track_id, plugins } => {
                if plugins.iter().any(|plugin| {
                    plugin.plugin_id().trim().is_empty() || plugin.bundle_path().trim().is_empty()
                }) {
                    return Err(ActionError::InvalidTrackFxPlugin);
                }
                let track = state
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                ProjectEvent::TrackFxChainChanged {
                    track_id,
                    before: track.fx_chain.clone(),
                    after: plugins,
                }
            }
            DawAction::SetTrackFxParameter {
                track_id,
                chain_index,
                parameter_id,
                before,
                after,
                before_state,
                after_state,
            } => {
                if !before.is_finite() || !after.is_finite() || before == after {
                    return Err(ActionError::InvalidTrackFxParameter);
                }
                let track = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                let plugin = track
                    .fx_chain
                    .get_mut(chain_index)
                    .ok_or(ActionError::InvalidTrackFxParameter)?;
                if plugin.state() != before_state.as_deref() {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                // The loaded plugin can report a newer effective value after a plugin update or
                // a restored CLAP State. Treat the host's observed `before` value as the current
                // baseline; undo then restores what the user actually saw in the plugin.
                if plugin
                    .parameter_value(parameter_id)
                    .is_some_and(|stored| stored != before)
                {
                    *plugin = plugin.clone().with_parameter_value(parameter_id, before);
                }
                ProjectEvent::TrackFxParameterChanged {
                    track_id,
                    chain_index,
                    parameter_id,
                    before,
                    after,
                    before_state,
                    after_state,
                }
            }
            DawAction::SetTrackFxParameterAutomation {
                track_id,
                chain_index,
                parameter_id,
                points,
            } => {
                if !valid_fx_parameter_automation(&points) {
                    return Err(ActionError::InvalidTrackFxParameterAutomation);
                }
                let after = if points.is_empty() {
                    None
                } else {
                    Some(
                        FxParameterAutomationLane::new(parameter_id, points)
                            .ok_or(ActionError::InvalidTrackFxParameterAutomation)?,
                    )
                };
                let track = state
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                let plugin = track
                    .fx_chain
                    .get(chain_index)
                    .ok_or(ActionError::InvalidTrackFxParameter)?;
                if after.is_some()
                    && plugin.parameter_automation_for(parameter_id).is_none()
                    && plugin.parameter_automation().len()
                        >= MAX_TRACK_FX_PARAMETER_AUTOMATION_LANES
                {
                    return Err(ActionError::InvalidTrackFxParameterAutomation);
                }
                ProjectEvent::TrackFxParameterAutomationChanged {
                    track_id,
                    chain_index,
                    parameter_id,
                    before: plugin.parameter_automation_for(parameter_id).cloned(),
                    after,
                }
            }
            DawAction::SetTrackName { track_id, name } => {
                let track = state
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                ProjectEvent::TrackRenamed {
                    track_id,
                    before: track.name.clone(),
                    after: name,
                }
            }
            DawAction::MoveTrack { track_id, index } => {
                if index >= state.tracks.len() {
                    return Err(ActionError::TrackIndexOutOfBounds {
                        index,
                        track_count: state.tracks.len(),
                    });
                }
                let from = state
                    .tracks
                    .iter()
                    .position(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                let end = from
                    + 1
                    + state
                        .tracks
                        .iter()
                        .filter(|track| is_descendant(&state.tracks, track.id, track_id))
                        .count();
                if index == from {
                    structure_change(&state.tracks, &state.tracks)?
                } else {
                    let mut after = state.tracks.clone();
                    let mut group: Vec<_> = after.drain(from..end).collect();
                    let target = index.min(after.len());
                    group[0].parent_track = after.get(target).and_then(|track| track.parent_track);
                    after.splice(target..target, group);
                    structure_change(&state.tracks, &after)?
                }
            }
            DawAction::InsertAudioItem {
                track_id,
                media_ref,
                start_sample,
                source_offset_samples,
                length_samples,
            } => {
                if !state.tracks.iter().any(|track| track.id == track_id) {
                    return Err(ActionError::TrackNotFound { track_id });
                }
                if state
                    .tracks
                    .iter()
                    .any(|track| track.id == track_id && track.frozen_audio_item_id.is_some())
                {
                    return Err(ActionError::CannotEditFrozenTrackSource { track_id });
                }
                if media_ref.trim().is_empty() {
                    return Err(ActionError::InvalidAudioMediaRef);
                }
                if length_samples == 0 {
                    return Err(ActionError::InvalidAudioItemLength);
                }
                if start_sample.checked_add(length_samples).is_none() {
                    return Err(ActionError::InvalidAudioItemPosition);
                }
                let next_id = ids
                    .next_item_id
                    .checked_add(1)
                    .ok_or(ActionError::ItemIdExhausted)?;
                let item = AudioItem {
                    fades: crate::AudioItemFades::default(),
                    volume: 1.0,
                    take_volume: 1.0,
                    take_pan: 0.0,
                    id: ItemId::from_raw(ids.next_item_id),
                    track_id,
                    media_ref,
                    start_sample,
                    source_offset_samples,
                    length_samples,
                    muted: false,
                    phase_inverted: false,
                    loop_source: source_offset_samples >= 0,
                    source_section_start_samples: None,
                    source_section_length_samples: None,
                };
                ids.next_item_id = next_id;
                ProjectEvent::AudioItemInserted {
                    index: state.audio_items.len(),
                    item,
                }
            }
            DawAction::InsertLoopedAudioItem {
                track_id,
                media_ref,
                start_sample,
                source_offset_samples,
                length_samples,
                muted,
                phase_inverted,
                source_section_start_samples,
                source_section_length_samples,
            } => {
                if !state.tracks.iter().any(|track| track.id == track_id) {
                    return Err(ActionError::TrackNotFound { track_id });
                }
                if state
                    .tracks
                    .iter()
                    .any(|track| track.id == track_id && track.frozen_audio_item_id.is_some())
                {
                    return Err(ActionError::CannotEditFrozenTrackSource { track_id });
                }
                if media_ref.trim().is_empty() {
                    return Err(ActionError::InvalidAudioMediaRef);
                }
                if length_samples == 0 || source_section_length_samples == 0 {
                    return Err(ActionError::InvalidAudioItemLength);
                }
                if start_sample.checked_add(length_samples).is_none() {
                    return Err(ActionError::InvalidAudioItemPosition);
                }
                if source_section_start_samples
                    .checked_add(source_section_length_samples)
                    .is_none()
                {
                    return Err(ActionError::InvalidAudioItemPosition);
                }
                if source_offset_samples >= source_section_length_samples {
                    return Err(ActionError::InvalidAudioItemPosition);
                }
                let Ok(source_offset_samples_signed) = i64::try_from(source_offset_samples) else {
                    return Err(ActionError::InvalidAudioItemPosition);
                };
                let next_id = ids
                    .next_item_id
                    .checked_add(1)
                    .ok_or(ActionError::ItemIdExhausted)?;
                let item = AudioItem {
                    fades: crate::AudioItemFades::default(),
                    volume: 1.0,
                    take_volume: 1.0,
                    take_pan: 0.0,
                    id: ItemId::from_raw(ids.next_item_id),
                    track_id,
                    media_ref,
                    start_sample,
                    source_offset_samples: source_offset_samples_signed,
                    length_samples,
                    muted,
                    phase_inverted,
                    loop_source: true,
                    source_section_start_samples: Some(source_section_start_samples),
                    source_section_length_samples: Some(source_section_length_samples),
                };
                ids.next_item_id = next_id;
                ProjectEvent::AudioItemInserted {
                    index: state.audio_items.len(),
                    item,
                }
            }
            DawAction::SetAudioItemLoopSource { item_id, enabled } => {
                let item = state
                    .audio_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::AudioItemNotFound { item_id })?;
                if is_frozen_render(state, item_id) {
                    return Err(ActionError::FrozenRenderCannotBeEdited { item_id });
                }
                let before = item.clone();
                if enabled
                    && !valid_audio_source_offset(
                        item.source_offset_samples,
                        true,
                        item.source_section_start_samples,
                        item.source_section_length_samples,
                    )
                {
                    return Err(ActionError::InvalidAudioItemPosition);
                }
                let after = AudioItem {
                    loop_source: enabled,
                    ..before.clone()
                };
                ProjectEvent::AudioItemChanged { before, after }
            }
            DawAction::SetAudioItemMute { item_id, muted } => {
                let item = state
                    .audio_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::AudioItemNotFound { item_id })?;
                if is_frozen_render(state, item_id) {
                    return Err(ActionError::FrozenRenderCannotBeEdited { item_id });
                }
                let before = item.clone();
                let after = AudioItem {
                    muted,
                    ..before.clone()
                };
                ProjectEvent::AudioItemChanged { before, after }
            }
            DawAction::SetAudioItemPhase { item_id, inverted } => {
                let item = state
                    .audio_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::AudioItemNotFound { item_id })?;
                if is_frozen_render(state, item_id) {
                    return Err(ActionError::FrozenRenderCannotBeEdited { item_id });
                }
                let before = item.clone();
                let after = AudioItem {
                    phase_inverted: inverted,
                    ..before.clone()
                };
                ProjectEvent::AudioItemChanged { before, after }
            }
            DawAction::SetAudioItemFades { item_id, fades } => {
                let item = state
                    .audio_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::AudioItemNotFound { item_id })?;
                if is_frozen_render(state, item_id) {
                    return Err(ActionError::FrozenRenderCannotBeEdited { item_id });
                }
                ProjectEvent::AudioItemFadesChanged {
                    item_id,
                    before: item.fades,
                    after: fades,
                }
            }
            DawAction::SetAudioItemVolume { item_id, volume } => {
                if !volume.is_finite() || !(0.0..=crate::MAX_AUDIO_ITEM_VOLUME).contains(&volume) {
                    return Err(ActionError::InvalidAudioItemVolume);
                }
                let item = state
                    .audio_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::AudioItemNotFound { item_id })?;
                if is_frozen_render(state, item_id) {
                    return Err(ActionError::FrozenRenderCannotBeEdited { item_id });
                }
                ProjectEvent::AudioItemVolumeChanged {
                    item_id,
                    before: item.volume,
                    after: volume,
                }
            }
            DawAction::SetAudioItemTakeMix {
                item_id,
                volume,
                pan,
            } => {
                if !volume.is_finite()
                    || !(0.0..=crate::MAX_AUDIO_ITEM_TAKE_VOLUME).contains(&volume)
                {
                    return Err(ActionError::InvalidAudioItemTakeVolume);
                }
                if !pan.is_finite() || !(-1.0..=1.0).contains(&pan) {
                    return Err(ActionError::InvalidAudioItemTakePan);
                }
                let item = state
                    .audio_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::AudioItemNotFound { item_id })?;
                if is_frozen_render(state, item_id) {
                    return Err(ActionError::FrozenRenderCannotBeEdited { item_id });
                }
                ProjectEvent::AudioItemTakeMixChanged {
                    item_id,
                    before: (item.take_volume, item.take_pan),
                    after: (volume, pan),
                }
            }
            DawAction::DuplicateAudioItemAt {
                item_id,
                track_id,
                start_sample,
            } => {
                let source = state
                    .audio_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::AudioItemNotFound { item_id })?;
                if is_frozen_render(state, item_id) {
                    return Err(ActionError::FrozenRenderCannotBeEdited { item_id });
                }
                let target = state
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                if target.frozen_audio_item_id.is_some() {
                    return Err(ActionError::CannotEditFrozenTrackSource { track_id });
                }
                if start_sample.checked_add(source.length_samples).is_none() {
                    return Err(ActionError::InvalidAudioItemPosition);
                }
                let next_id = ids
                    .next_item_id
                    .checked_add(1)
                    .ok_or(ActionError::ItemIdExhausted)?;
                let item = AudioItem {
                    id: ItemId::from_raw(ids.next_item_id),
                    track_id,
                    start_sample,
                    ..source.clone()
                };
                ids.next_item_id = next_id;
                ProjectEvent::AudioItemInserted {
                    index: state.audio_items.len(),
                    item,
                }
            }
            DawAction::EditAudioItem {
                item_id,
                media_ref,
                start_sample,
                source_offset_samples,
                length_samples,
            } => {
                if media_ref.trim().is_empty() {
                    return Err(ActionError::InvalidAudioMediaRef);
                }
                if length_samples == 0 {
                    return Err(ActionError::InvalidAudioItemLength);
                }
                if start_sample.checked_add(length_samples).is_none() {
                    return Err(ActionError::InvalidAudioItemPosition);
                }
                let item = state
                    .audio_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::AudioItemNotFound { item_id })?;
                if is_frozen_render(state, item_id) {
                    return Err(ActionError::FrozenRenderCannotBeEdited { item_id });
                }
                if !valid_audio_source_offset(
                    source_offset_samples,
                    item.loop_source,
                    item.source_section_start_samples,
                    item.source_section_length_samples,
                ) {
                    return Err(ActionError::InvalidAudioItemPosition);
                }
                let before = item.clone();
                let after = AudioItem {
                    media_ref,
                    start_sample,
                    source_offset_samples,
                    length_samples,
                    ..before.clone()
                };
                ProjectEvent::AudioItemChanged { before, after }
            }
            DawAction::MoveItemToTrack { item_id, track_id } => {
                if !state.tracks.iter().any(|track| track.id == track_id) {
                    return Err(ActionError::TrackNotFound { track_id });
                }
                if state
                    .tracks
                    .iter()
                    .any(|track| track.id == track_id && track.frozen_audio_item_id.is_some())
                {
                    return Err(ActionError::CannotEditFrozenTrackSource { track_id });
                }
                if let Some(item) = state.audio_items.iter().find(|item| item.id == item_id) {
                    if is_frozen_render(state, item_id) {
                        return Err(ActionError::FrozenRenderCannotBeEdited { item_id });
                    }
                    let before = item.clone();
                    let after = AudioItem {
                        track_id,
                        ..before.clone()
                    };
                    ProjectEvent::AudioItemChanged { before, after }
                } else if let Some(item) = state.midi_items.iter().find(|item| item.id == item_id) {
                    let before = item.clone();
                    let after = MidiItem {
                        track_id,
                        ..before.clone()
                    };
                    ProjectEvent::MidiItemChanged { before, after }
                } else {
                    return Err(ActionError::ItemNotFound { item_id });
                }
            }
            DawAction::DeleteAudioItem { item_id } => {
                if is_frozen_render(state, item_id) {
                    return Err(ActionError::FrozenRenderCannotBeEdited { item_id });
                }
                let index = state
                    .audio_items
                    .iter()
                    .position(|item| item.id == item_id)
                    .ok_or(ActionError::AudioItemNotFound { item_id })?;
                ProjectEvent::AudioItemRemoved {
                    index,
                    item: state.audio_items[index].clone(),
                }
            }
            DawAction::InsertMidiItem {
                track_id,
                start_tick,
                length_ticks,
            } => {
                if !state.tracks.iter().any(|track| track.id == track_id) {
                    return Err(ActionError::TrackNotFound { track_id });
                }
                if length_ticks == 0 {
                    return Err(ActionError::InvalidMidiItemLength);
                }
                if start_tick.checked_add(length_ticks).is_none() {
                    return Err(ActionError::InvalidMidiItemPosition);
                }
                let next_id = ids
                    .next_item_id
                    .checked_add(1)
                    .ok_or(ActionError::ItemIdExhausted)?;
                let item = MidiItem {
                    id: ItemId::from_raw(ids.next_item_id),
                    track_id,
                    name: "MIDI".to_owned(),
                    start_tick,
                    source_offset_ticks: 0,
                    loop_source: true,
                    source_loop_length_ticks: length_ticks,
                    length_ticks,
                    muted: false,
                    notes: Arc::new(Vec::new()),
                    controllers: Arc::new(Vec::new()),
                    pitch_bends: Arc::new(Vec::new()),
                };
                ids.next_item_id = next_id;
                ProjectEvent::MidiItemInserted {
                    index: state.midi_items.len(),
                    item,
                }
            }
            DawAction::SetMidiItemName { item_id, name } => {
                if name.trim().is_empty() || name.chars().count() > 128 {
                    return Err(ActionError::InvalidMidiItemName);
                }
                let before = state
                    .midi_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::MidiItemNotFound { item_id })?
                    .clone();
                let after = MidiItem {
                    name,
                    ..before.clone()
                };
                ProjectEvent::MidiItemChanged { before, after }
            }
            DawAction::EditMidiItem {
                item_id,
                start_tick,
                length_ticks,
            } => {
                if length_ticks == 0 {
                    return Err(ActionError::InvalidMidiItemLength);
                }
                if start_tick.checked_add(length_ticks).is_none() {
                    return Err(ActionError::InvalidMidiItemPosition);
                }
                let item = state
                    .midi_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::MidiItemNotFound { item_id })?;
                if !valid_midi_controllers(&item.controllers) {
                    return Err(ActionError::InvalidMidiController);
                }
                if !valid_midi_pitch_bends(&item.pitch_bends) {
                    return Err(ActionError::InvalidMidiPitchBend);
                }
                let before = item.clone();
                let after = MidiItem {
                    start_tick,
                    length_ticks,
                    ..before.clone()
                };
                ProjectEvent::MidiItemChanged { before, after }
            }
            DawAction::EditMidiItemPlacement {
                item_id,
                start_tick,
                length_ticks,
                source_offset_ticks,
            } => {
                if length_ticks == 0 {
                    return Err(ActionError::InvalidMidiItemLength);
                }
                if start_tick.checked_add(length_ticks).is_none()
                    || source_offset_ticks.checked_add(length_ticks).is_none()
                {
                    return Err(ActionError::InvalidMidiItemPosition);
                }
                let item = state
                    .midi_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::MidiItemNotFound { item_id })?;
                if !valid_midi_controllers(&item.controllers) {
                    return Err(ActionError::InvalidMidiController);
                }
                if !valid_midi_pitch_bends(&item.pitch_bends) {
                    return Err(ActionError::InvalidMidiPitchBend);
                }
                let before = item.clone();
                let after = MidiItem {
                    start_tick,
                    length_ticks,
                    source_offset_ticks,
                    ..before.clone()
                };
                ProjectEvent::MidiItemChanged { before, after }
            }
            DawAction::SetMidiItemContents {
                item_id,
                source_offset_ticks,
                source_loop_length_ticks,
                content_shift_ticks,
            } => {
                if source_loop_length_ticks == 0 {
                    return Err(ActionError::InvalidMidiItemLength);
                }
                let item = state
                    .midi_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::MidiItemNotFound { item_id })?;
                if !valid_midi_controllers(&item.controllers) {
                    return Err(ActionError::InvalidMidiController);
                }
                if !valid_midi_pitch_bends(&item.pitch_bends) {
                    return Err(ActionError::InvalidMidiPitchBend);
                }
                if source_offset_ticks.checked_add(item.length_ticks).is_none() {
                    return Err(ActionError::InvalidMidiItemPosition);
                }

                let before = item.clone();
                let notes = before
                    .notes
                    .iter()
                    .map(|note| {
                        let tick = note
                            .data
                            .tick
                            .checked_add(content_shift_ticks)
                            .ok_or(ActionError::InvalidMidiItemPosition)?;
                        if tick.checked_add(note.data.duration).is_none() {
                            return Err(ActionError::InvalidMidiItemPosition);
                        }
                        Ok(MidiNote {
                            id: note.id,
                            data: crate::MidiNoteData { tick, ..note.data },
                        })
                    })
                    .collect::<Result<Vec<_>, ActionError>>()?;
                let controllers = before
                    .controllers
                    .iter()
                    .map(|controller| {
                        Ok(MidiControllerData {
                            tick: controller
                                .tick
                                .checked_add(content_shift_ticks)
                                .ok_or(ActionError::InvalidMidiItemPosition)?,
                            ..*controller
                        })
                    })
                    .collect::<Result<Vec<_>, ActionError>>()?;
                let pitch_bends = before
                    .pitch_bends
                    .iter()
                    .map(|bend| {
                        Ok(MidiPitchBendData {
                            tick: bend
                                .tick
                                .checked_add(content_shift_ticks)
                                .ok_or(ActionError::InvalidMidiItemPosition)?,
                            ..*bend
                        })
                    })
                    .collect::<Result<Vec<_>, ActionError>>()?;
                let after = MidiItem {
                    source_offset_ticks,
                    source_loop_length_ticks,
                    notes: Arc::new(notes),
                    controllers: Arc::new(controllers),
                    pitch_bends: Arc::new(pitch_bends),
                    ..before.clone()
                };
                ProjectEvent::MidiItemChanged { before, after }
            }
            DawAction::SetMidiItemMute { item_id, muted } => {
                let before = state
                    .midi_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::MidiItemNotFound { item_id })?
                    .clone();
                let after = MidiItem {
                    muted,
                    ..before.clone()
                };
                ProjectEvent::MidiItemChanged { before, after }
            }
            DawAction::SetMidiItemLoopSource { item_id, enabled } => {
                let before = state
                    .midi_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::MidiItemNotFound { item_id })?
                    .clone();
                let after = MidiItem {
                    loop_source: enabled,
                    ..before.clone()
                };
                ProjectEvent::MidiItemChanged { before, after }
            }
            DawAction::TrimMidiItemStart {
                item_id,
                start_tick,
                length_ticks,
                source_offset_ticks,
            } => {
                if length_ticks == 0 {
                    return Err(ActionError::InvalidMidiItemLength);
                }
                if start_tick.checked_add(length_ticks).is_none()
                    || source_offset_ticks.checked_add(length_ticks).is_none()
                {
                    return Err(ActionError::InvalidMidiItemPosition);
                }
                let item = state
                    .midi_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::MidiItemNotFound { item_id })?;
                let expected_source_offset = i128::from(item.source_offset_ticks)
                    + i128::from(start_tick)
                    - i128::from(item.start_tick);
                if u64::try_from(expected_source_offset).ok() != Some(source_offset_ticks) {
                    return Err(ActionError::InvalidMidiItemPosition);
                }
                let before = item.clone();
                let after = MidiItem {
                    start_tick,
                    source_offset_ticks,
                    length_ticks,
                    ..before.clone()
                };
                ProjectEvent::MidiItemChanged { before, after }
            }
            DawAction::DuplicateMidiItem { item_id } => {
                let original = state
                    .midi_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::MidiItemNotFound { item_id })?;
                let start_tick = original
                    .start_tick
                    .checked_add(original.length_ticks)
                    .ok_or(ActionError::InvalidMidiItemPosition)?;
                duplicate_midi_item_at(state, ids, item_id, start_tick)?
            }
            DawAction::DuplicateMidiItemAt {
                item_id,
                start_tick,
            } => duplicate_midi_item_at(state, ids, item_id, start_tick)?,
            DawAction::DuplicateMidiItemToTrack {
                item_id,
                track_id,
                start_tick,
            } => duplicate_midi_item_to_track(state, ids, item_id, track_id, start_tick)?,
            DawAction::SplitMidiItem {
                item_id,
                split_ticks,
            } => {
                let index = state
                    .midi_items
                    .iter()
                    .position(|item| item.id == item_id)
                    .ok_or(ActionError::MidiItemNotFound { item_id })?;
                let original = state.midi_items[index].clone();
                let item_end = original
                    .start_tick
                    .checked_add(original.length_ticks)
                    .ok_or(ActionError::InvalidMidiItemPosition)?;
                let mut split_ticks = split_ticks
                    .into_iter()
                    .filter(|tick| original.start_tick < *tick && *tick < item_end)
                    .collect::<Vec<_>>();
                split_ticks.sort_unstable();
                split_ticks.dedup();
                if split_ticks.is_empty() {
                    return Err(ActionError::InvalidMidiItemPosition);
                }

                let mut absolute_points = Vec::with_capacity(split_ticks.len() + 2);
                absolute_points.push(original.start_tick);
                absolute_points.extend(split_ticks);
                absolute_points.push(item_end);
                let relative_points = absolute_points
                    .iter()
                    .map(|tick| tick - original.start_tick)
                    .collect::<Vec<_>>();
                let content_points = relative_points
                    .iter()
                    .map(|tick| original.source_offset_ticks.saturating_add(*tick))
                    .collect::<Vec<_>>();
                let mut segment_notes = vec![Vec::new(); absolute_points.len() - 1];
                let mut next_note_id = ids.next_note_id;
                for note in original.notes.iter() {
                    let note_start = note.data.tick;
                    let note_end = note_start
                        .checked_add(note.data.duration)
                        .ok_or(ActionError::InvalidMidiNote)?;
                    let mut first_piece = true;
                    for (segment_index, segment) in content_points.windows(2).enumerate() {
                        let segment_source_start = if segment_index == 0 { 0 } else { segment[0] };
                        let overlap_start = note_start.max(segment_source_start);
                        let is_last_segment = segment_index + 1 == segment_notes.len();
                        let overlap_end = if is_last_segment {
                            note_end
                        } else {
                            note_end.min(segment[1])
                        };
                        if overlap_start >= overlap_end {
                            continue;
                        }
                        let note_id = if first_piece {
                            first_piece = false;
                            note.id
                        } else {
                            let id = NoteId::from_raw(next_note_id);
                            next_note_id = next_note_id
                                .checked_add(1)
                                .ok_or(ActionError::NoteIdExhausted)?;
                            id
                        };
                        segment_notes[segment_index].push(MidiNote {
                            id: note_id,
                            data: crate::MidiNoteData {
                                pitch: note.data.pitch,
                                tick: if segment_index == 0 {
                                    overlap_start
                                } else {
                                    overlap_start - segment_source_start
                                },
                                duration: overlap_end - overlap_start,
                                velocity: note.data.velocity,
                            },
                        });
                    }
                }

                let mut next_item_id = ids.next_item_id;
                let segment_count = segment_notes.len();
                let mut after = Vec::with_capacity(segment_notes.len());
                for (index, notes) in segment_notes.into_iter().enumerate() {
                    let segment_start = absolute_points[index];
                    let segment_end = absolute_points[index + 1];
                    let segment_id = if index == 0 {
                        original.id
                    } else {
                        let id = ItemId::from_raw(next_item_id);
                        next_item_id = next_item_id
                            .checked_add(1)
                            .ok_or(ActionError::ItemIdExhausted)?;
                        id
                    };
                    after.push(MidiItem {
                        id: segment_id,
                        track_id: original.track_id,
                        name: original.name.clone(),
                        start_tick: segment_start,
                        source_offset_ticks: if index == 0 {
                            original.source_offset_ticks
                        } else {
                            0
                        },
                        loop_source: original.loop_source,
                        source_loop_length_ticks: original.source_loop_length_ticks,
                        length_ticks: segment_end - segment_start,
                        muted: original.muted,
                        notes: Arc::new(notes),
                        controllers: Arc::new(controllers_for_segment(
                            &original.controllers,
                            if index == 0 { 0 } else { content_points[index] },
                            content_points[index + 1],
                            index + 1 == segment_count,
                        )),
                        pitch_bends: Arc::new(pitch_bends_for_segment(
                            &original.pitch_bends,
                            if index == 0 { 0 } else { content_points[index] },
                            content_points[index + 1],
                            index + 1 == segment_count,
                        )),
                    });
                }
                ids.next_item_id = next_item_id;
                ids.next_note_id = next_note_id;
                ProjectEvent::MidiItemsReplaced {
                    index,
                    before: vec![original],
                    after,
                }
            }
            DawAction::DeleteMidiItem { item_id } => {
                let index = state
                    .midi_items
                    .iter()
                    .position(|item| item.id == item_id)
                    .ok_or(ActionError::MidiItemNotFound { item_id })?;
                ProjectEvent::MidiItemRemoved {
                    index,
                    item: state.midi_items[index].clone(),
                }
            }
            DawAction::AddMidiNotes { item_id, notes } => {
                let item = state
                    .midi_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::MidiItemNotFound { item_id })?;
                let mut added = Vec::with_capacity(notes.len());
                let mut next_note_id = ids.next_note_id;
                for data in notes {
                    if data.pitch > 127
                        || data.velocity > 127
                        || !valid_midi_note_source_range(item, data.tick, data.duration)
                    {
                        return Err(ActionError::InvalidMidiNote);
                    }
                    let next_id = next_note_id
                        .checked_add(1)
                        .ok_or(ActionError::NoteIdExhausted)?;
                    added.push(MidiNote {
                        id: NoteId::from_raw(next_note_id),
                        data,
                    });
                    next_note_id = next_id;
                }
                let first_index = item.notes.len();
                ids.next_note_id = next_note_id;
                ProjectEvent::MidiNotesAdded {
                    item_id,
                    notes: added
                        .into_iter()
                        .enumerate()
                        .map(|(offset, note)| (first_index + offset, note))
                        .collect(),
                }
            }
            DawAction::EditMidiNote {
                item_id,
                note_id,
                data,
            } => {
                let item = state
                    .midi_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::MidiItemNotFound { item_id })?;
                if data.pitch > 127
                    || data.velocity > 127
                    || !valid_midi_note_source_range(item, data.tick, data.duration)
                {
                    return Err(ActionError::InvalidMidiNote);
                }
                let before = item
                    .notes
                    .iter()
                    .find(|note| note.id == note_id)
                    .cloned()
                    .ok_or(ActionError::MidiNoteNotFound { item_id, note_id })?;
                ProjectEvent::MidiNoteChanged {
                    item_id,
                    before,
                    after: MidiNote { id: note_id, data },
                }
            }
            DawAction::DeleteMidiNotes { item_id, note_ids } => {
                let item = state
                    .midi_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::MidiItemNotFound { item_id })?;
                let mut selected = Vec::with_capacity(note_ids.len());
                let mut seen = std::collections::HashSet::with_capacity(note_ids.len());
                for note_id in note_ids {
                    if !seen.insert(note_id) {
                        return Err(ActionError::InvalidMidiNote);
                    }
                    let (index, note) = item
                        .notes
                        .iter()
                        .enumerate()
                        .find(|(_, note)| note.id == note_id)
                        .ok_or(ActionError::InvalidMidiNote)?;
                    selected.push((index, note.clone()));
                }
                selected.sort_by_key(|(index, _)| *index);
                ProjectEvent::MidiNotesRemoved {
                    item_id,
                    notes: selected,
                }
            }
            DawAction::SetMidiControllers {
                item_id,
                mut controllers,
            } => {
                let item = state
                    .midi_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::MidiItemNotFound { item_id })?;
                controllers
                    .sort_unstable_by_key(|controller| (controller.tick, controller.controller));
                let (source_start_tick, source_end_tick) = midi_item_source_tick_bounds(item);
                if !valid_midi_controllers(&controllers)
                    || controllers.iter().any(|controller| {
                        (controller.tick < source_start_tick || controller.tick >= source_end_tick)
                            && !item.controllers.contains(controller)
                    })
                {
                    return Err(ActionError::InvalidMidiController);
                }
                ProjectEvent::MidiControllersChanged {
                    item_id,
                    before: item.controllers.as_ref().clone(),
                    after: controllers,
                }
            }
            DawAction::SetMidiPitchBends {
                item_id,
                mut pitch_bends,
            } => {
                let item = state
                    .midi_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::MidiItemNotFound { item_id })?;
                pitch_bends.sort_unstable_by_key(|bend| bend.tick);
                let (source_start_tick, source_end_tick) = midi_item_source_tick_bounds(item);
                if !valid_midi_pitch_bends(&pitch_bends)
                    || pitch_bends.iter().any(|bend| {
                        (bend.tick < source_start_tick || bend.tick >= source_end_tick)
                            && !item.pitch_bends.contains(bend)
                    })
                {
                    return Err(ActionError::InvalidMidiPitchBend);
                }
                ProjectEvent::MidiPitchBendsChanged {
                    item_id,
                    before: item.pitch_bends.as_ref().clone(),
                    after: pitch_bends,
                }
            }
            DawAction::QuantizeItem {
                item_id,
                grid,
                strength,
            } => {
                if !strength.is_finite() || !(0.0..=1.0).contains(&strength) {
                    return Err(ActionError::InvalidQuantizeStrength);
                }
                let grid_ticks = grid
                    .ticks(state.tempo_map.settings().ppq())
                    .map_err(|_| ActionError::InvalidQuantizeGrid)?;
                let item = state
                    .midi_items
                    .iter()
                    .find(|item| item.id == item_id)
                    .ok_or(ActionError::MidiItemNotFound { item_id })?;
                let mut changes = Vec::new();
                for note in item.notes.iter() {
                    let absolute_tick = item
                        .project_tick_at_content_tick(note.data.tick)
                        .ok_or(ActionError::InvalidMidiNote)?;
                    let lower = absolute_tick / grid_ticks * grid_ticks;
                    let remainder = absolute_tick - lower;
                    let round_up_threshold = grid_ticks / 2 + grid_ticks % 2;
                    let nearest = if remainder >= round_up_threshold {
                        lower.checked_add(grid_ticks).unwrap_or(lower)
                    } else {
                        lower
                    };
                    let distance = nearest as i128 - absolute_tick as i128;
                    let adjustment = (distance as f64 * f64::from(strength)).round() as i128;
                    let quantized_absolute = u64::try_from(absolute_tick as i128 + adjustment)
                        .map_err(|_| ActionError::InvalidMidiNote)?;
                    let tick = midi_source_tick_at_project_tick(item, quantized_absolute)
                        .ok_or(ActionError::InvalidMidiNote)?;
                    if !valid_midi_note_source_range(item, tick, note.data.duration) {
                        return Err(ActionError::InvalidMidiNote);
                    }
                    if tick != note.data.tick {
                        changes.push((note.id, note.data.tick, tick));
                    }
                }
                ProjectEvent::MidiNotesQuantized { item_id, changes }
            }
            DawAction::DeleteTrack { track_id } => {
                let index = state
                    .tracks
                    .iter()
                    .position(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
                if state.tracks.iter().any(|track| {
                    track.parent_track == Some(track_id)
                        || routing_destinations(track).any(|destination| destination == track_id)
                }) {
                    return Err(ActionError::TrackHasRoutingDependents { track_id });
                }
                let audio_items = state
                    .audio_items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| item.track_id == track_id)
                    .map(|(index, item)| (index, item.clone()))
                    .collect();
                let midi_items = state
                    .midi_items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| item.track_id == track_id)
                    .map(|(index, item)| (index, item.clone()))
                    .collect();
                ProjectEvent::TrackDeleted {
                    index,
                    track: state.tracks[index].clone(),
                    audio_items,
                    midi_items,
                }
            }
            DawAction::BatchTransaction { .. } => {
                return Err(ActionError::HistoryInvariantViolation);
            }
        };

        Self::apply_event(state, &event)?;
        Ok(event)
    }

    fn apply_event(state: &mut ProjectState, event: &ProjectEvent) -> Result<(), ActionError> {
        match event {
            ProjectEvent::ProjectNotesChanged { before, after } => {
                if state.notes != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.notes = after.clone();
            }
            ProjectEvent::FrameRateChanged { before, after } => {
                if state.frame_rate != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.frame_rate = *after;
            }
            ProjectEvent::ProjectSampleRateChanged { before, after } => {
                if !before.matches(state) {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                after.restore(state)?;
            }
            ProjectEvent::ForceTempoChangesToWholeSamplesChanged { before, after } => {
                if state
                    .tempo_map
                    .settings()
                    .force_tempo_changes_to_whole_samples()
                    != *before
                {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state
                    .tempo_map
                    .set_force_tempo_changes_to_whole_samples(*after)
                    .map_err(|_| ActionError::TempoMapOutOfRange)?;
            }
            ProjectEvent::BpmBasisChanged { before, after } => {
                if state.tempo_map.settings().bpm_basis() != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state
                    .tempo_map
                    .set_bpm_basis(*after)
                    .map_err(|error| match error {
                        TimebaseError::InvalidTempo => ActionError::InvalidTempoBpm,
                        _ => ActionError::TempoMapOutOfRange,
                    })?;
            }
            ProjectEvent::BaseRulerLabelSpacingChanged { before, after } => {
                if state.base_ruler_label_spacing_on_measure != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.base_ruler_label_spacing_on_measure = *after;
            }
            ProjectEvent::ProjectStartTimeOffsetChanged { before, after } => {
                if !after.is_finite() {
                    return Err(ActionError::InvalidProjectTimeOffset);
                }
                if state.project_start_time_offset_seconds != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.project_start_time_offset_seconds = *after;
            }
            ProjectEvent::ProjectStartTimeFractionalFrameOffsetAllowedChanged { before, after } => {
                if state.project_start_time_fractional_frame_offset_allowed != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.project_start_time_fractional_frame_offset_allowed = *after;
            }
            ProjectEvent::ProjectStartMeasureChanged { before, after } => {
                if state.meter_map.start_measure() != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state
                    .meter_map
                    .set_start_measure(*after)
                    .map_err(|_| ActionError::MeterMapOutOfRange)?;
            }
            ProjectEvent::ProjectMarkersChanged { before, after } => {
                if state.markers != *before || !valid_project_markers(after) {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.markers = after.clone();
            }
            ProjectEvent::ProjectMarkerLaneVisibilityChanged { before, after } => {
                if state.show_project_markers != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.show_project_markers = *after;
            }
            ProjectEvent::ProjectRegionLaneVisibilityChanged { before, after } => {
                if state.show_project_regions != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.show_project_regions = *after;
            }
            ProjectEvent::ProjectRegionsChanged { before, after } => {
                if state.regions != *before || !valid_project_regions(after) {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.regions = after.clone();
            }
            ProjectEvent::MasterMixChanged { before, after } => {
                if state.master_mix != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.master_mix = *after;
            }
            ProjectEvent::TrackStructureChanged { before, after } => {
                if track_structure(&state.tracks) != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                let old = std::mem::take(&mut state.tracks);
                let mut by_id: HashMap<_, _> =
                    old.into_iter().map(|track| (track.id, track)).collect();
                for (id, parent, folder) in after {
                    let mut track = by_id
                        .remove(id)
                        .ok_or(ActionError::HistoryInvariantViolation)?;
                    track.parent_track = *parent;
                    track.is_folder = *folder;
                    state.tracks.push(track);
                }
                if !by_id.is_empty()
                    || !valid_track_hierarchy(&state.tracks)
                    || !valid_track_routing(&state.tracks)
                {
                    return Err(ActionError::HistoryInvariantViolation);
                }
            }
            ProjectEvent::TrackMainSendChanged {
                track_id,
                before,
                after,
            } => {
                let track = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == *track_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if track.main_send_enabled != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                track.main_send_enabled = *after;
            }
            ProjectEvent::TrackSendsChanged {
                track_id,
                before,
                after,
            } => {
                let track = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == *track_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if track.sends != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                track.sends = after.clone();
            }

            ProjectEvent::TrackCreated { index, track } => {
                if *index > state.tracks.len()
                    || state.tracks.iter().any(|item| item.id == track.id)
                {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.tracks.insert(*index, track.clone());
            }
            ProjectEvent::TrackDeleted {
                index,
                track,
                audio_items,
                midi_items,
            } => {
                if state.tracks.get(*index).map(|item| item.id) != Some(track.id) {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                Self::remove_audio_items(state, audio_items)?;
                Self::remove_midi_items(state, midi_items)?;
                state.tracks.remove(*index);
            }
            ProjectEvent::TrackRestored {
                index,
                track,
                audio_items,
                midi_items,
            } => {
                if *index > state.tracks.len()
                    || state.tracks.iter().any(|item| item.id == track.id)
                {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.tracks.insert(*index, track.clone());
                for (item_index, item) in audio_items {
                    if item.track_id != track.id
                        || *item_index > state.audio_items.len()
                        || state
                            .audio_items
                            .iter()
                            .any(|existing| existing.id == item.id)
                        || state
                            .midi_items
                            .iter()
                            .any(|existing| existing.id == item.id)
                        || !valid_audio_item(item)
                    {
                        return Err(ActionError::HistoryInvariantViolation);
                    }
                    state.audio_items.insert(*item_index, item.clone());
                }
                for (item_index, item) in midi_items {
                    if item.track_id != track.id
                        || *item_index > state.midi_items.len()
                        || state
                            .audio_items
                            .iter()
                            .any(|existing| existing.id == item.id)
                        || state
                            .midi_items
                            .iter()
                            .any(|existing| existing.id == item.id)
                    {
                        return Err(ActionError::HistoryInvariantViolation);
                    }
                    state.midi_items.insert(*item_index, item.clone());
                }
            }
            ProjectEvent::TrackVolumeChanged {
                track_id,
                before,
                after,
            } => {
                let track = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == *track_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if track.volume_db != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                track.volume_db = *after;
            }
            ProjectEvent::TrackVolumeAutomationChanged {
                track_id,
                before,
                after,
            } => {
                let track = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == *track_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if track.volume_automation != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                track.volume_automation = after.clone();
            }
            ProjectEvent::TrackRazorEditAreasChanged { before, after } => {
                if before.len() != after.len()
                    || before
                        .iter()
                        .zip(after)
                        .any(|((before_id, areas), (after_id, _))| {
                            before_id != after_id
                                || !state.tracks.iter().any(|track| {
                                    track.id == *before_id && track.razor_edit_areas == *areas
                                })
                        })
                {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                for (track_id, areas) in after {
                    let track = state
                        .tracks
                        .iter_mut()
                        .find(|track| track.id == *track_id)
                        .ok_or(ActionError::HistoryInvariantViolation)?;
                    if !valid_razor_edit_areas(areas) {
                        return Err(ActionError::HistoryInvariantViolation);
                    }
                    track.razor_edit_areas = areas.clone();
                }
            }
            ProjectEvent::TrackPanChanged {
                track_id,
                before,
                after,
            } => {
                let track = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == *track_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if track.pan != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                track.pan = *after;
            }
            ProjectEvent::TrackPhaseChanged {
                track_id,
                before,
                after,
            } => {
                let track = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == *track_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if track.phase_inverted != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                track.phase_inverted = *after;
            }
            ProjectEvent::TrackMuteChanged {
                track_id,
                before,
                after,
            } => {
                let track = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == *track_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if track.muted != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                track.muted = *after;
            }
            ProjectEvent::TrackSoloChanged {
                track_id,
                before,
                after,
            } => {
                let track = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == *track_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if track.solo != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                track.solo = *after;
            }
            ProjectEvent::TrackRecordArmChanged {
                track_id,
                before,
                after,
            } => {
                let track = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == *track_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if track.record_armed != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                track.record_armed = *after;
            }
            ProjectEvent::TrackInstrumentChanged {
                track_id,
                before,
                after,
            } => {
                let track = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == *track_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if track.instrument != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                track.instrument = after.clone();
            }
            ProjectEvent::TrackFreezeChanged {
                track_id,
                before,
                after,
                before_render,
                after_render,
            } => {
                let track_index = state
                    .tracks
                    .iter()
                    .position(|track| track.id == *track_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if state.tracks[track_index].frozen_audio_item_id != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                if let Some((index, item)) = before_render {
                    if before != &Some(item.id)
                        || item.track_id != *track_id
                        || state.audio_items.get(*index) != Some(item)
                    {
                        return Err(ActionError::HistoryInvariantViolation);
                    }
                } else if before.is_some() {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                if let Some((index, item)) = after_render {
                    if after != &Some(item.id)
                        || item.track_id != *track_id
                        || *index > state.audio_items.len()
                        || !valid_audio_item(item)
                        || state
                            .audio_items
                            .iter()
                            .any(|existing| existing.id == item.id)
                        || state
                            .midi_items
                            .iter()
                            .any(|existing| existing.id == item.id)
                    {
                        return Err(ActionError::HistoryInvariantViolation);
                    }
                } else if after.is_some() {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                if let Some((index, _)) = before_render {
                    state.audio_items.remove(*index);
                }
                if let Some((index, item)) = after_render {
                    state.audio_items.insert(*index, item.clone());
                }
                state.tracks[track_index].frozen_audio_item_id = *after;
            }
            ProjectEvent::TrackFxChainChanged {
                track_id,
                before,
                after,
            } => {
                let track = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == *track_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if track.fx_chain != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                track.fx_chain.clone_from(after);
            }
            ProjectEvent::TrackFxParameterChanged {
                track_id,
                chain_index,
                parameter_id,
                before: _,
                after,
                before_state,
                after_state,
            } => {
                let track = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == *track_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                let plugin = track
                    .fx_chain
                    .get_mut(*chain_index)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                // A live plugin can change independently (for example through its native
                // editor or after a plugin update), so history restores recorded values even
                // when the current host cache no longer matches this event's expected value.
                if plugin.state() != before_state.as_deref() {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                *plugin = plugin
                    .clone()
                    .with_state(after_state.clone())
                    .with_parameter_value(*parameter_id, *after);
            }
            ProjectEvent::TrackFxParameterAutomationChanged {
                track_id,
                chain_index,
                parameter_id,
                before,
                after,
            } => {
                let track = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == *track_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                let plugin = track
                    .fx_chain
                    .get_mut(*chain_index)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if plugin.parameter_automation_for(*parameter_id).cloned() != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                *plugin = plugin
                    .clone()
                    .with_parameter_automation(*parameter_id, after.clone());
            }
            ProjectEvent::TrackRenamed {
                track_id,
                before,
                after,
            } => {
                let track = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == *track_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if track.name != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                track.name.clone_from(after);
            }
            ProjectEvent::TrackOutputChanged {
                track_id,
                before,
                after,
            } => {
                let track = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == *track_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if track.output_track != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                track.output_track = *after;
            }
            ProjectEvent::TempoChanged {
                start_tick,
                before,
                after,
            } => {
                if state.tempo_map.point_at(*start_tick) != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state
                    .tempo_map
                    .set_point(*start_tick, *after)
                    .map_err(|_| ActionError::HistoryInvariantViolation)?;
            }
            ProjectEvent::TempoCurveChanged {
                start_tick,
                before,
                after,
            } => {
                if state.tempo_map.curve_at(*start_tick) != Some(*before) {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state
                    .tempo_map
                    .set_curve(*start_tick, *after)
                    .map_err(|_| ActionError::HistoryInvariantViolation)?;
            }
            ProjectEvent::MeterChanged {
                start_tick,
                before,
                after,
            } => {
                if state.meter_map.point_at(*start_tick) != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state
                    .meter_map
                    .set_point(*start_tick, *after)
                    .map_err(|_| ActionError::HistoryInvariantViolation)?;
            }
            ProjectEvent::MeterMapReplaced { before, after } => {
                if state
                    .meter_map
                    .points()
                    .map(|(start_tick, signature)| MeterPointSnapshot {
                        start_tick,
                        numerator: signature.numerator(),
                        denominator: signature.denominator(),
                    })
                    .ne(before.iter().copied())
                {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                let replacement = after
                    .iter()
                    .map(|point| {
                        TimeSignature::new(point.numerator, point.denominator)
                            .map(|signature| (point.start_tick, signature))
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| ActionError::HistoryInvariantViolation)?;
                state
                    .meter_map
                    .replace_points(&replacement)
                    .map_err(|_| ActionError::HistoryInvariantViolation)?;
            }
            ProjectEvent::AudioItemInserted { index, item } => {
                if *index > state.audio_items.len()
                    || !state.tracks.iter().any(|track| track.id == item.track_id)
                    || state
                        .audio_items
                        .iter()
                        .any(|existing| existing.id == item.id)
                    || state
                        .midi_items
                        .iter()
                        .any(|existing| existing.id == item.id)
                    || !valid_audio_item(item)
                {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.audio_items.insert(*index, item.clone());
            }
            ProjectEvent::AudioItemRemoved { index, item } => {
                if state.audio_items.get(*index).map(|existing| existing.id) != Some(item.id) {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.audio_items.remove(*index);
            }
            ProjectEvent::AudioItemFadesChanged {
                item_id,
                before,
                after,
            } => {
                let item = state
                    .audio_items
                    .iter_mut()
                    .find(|item| item.id == *item_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if item.fades != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                item.fades = *after;
            }
            ProjectEvent::AudioItemVolumeChanged {
                item_id,
                before,
                after,
            } => {
                let item = state
                    .audio_items
                    .iter_mut()
                    .find(|item| item.id == *item_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if item.volume != *before
                    || !after.is_finite()
                    || !(0.0..=crate::MAX_AUDIO_ITEM_VOLUME).contains(after)
                {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                item.volume = *after;
            }
            ProjectEvent::AudioItemTakeMixChanged {
                item_id,
                before,
                after,
            } => {
                let item = state
                    .audio_items
                    .iter_mut()
                    .find(|item| item.id == *item_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if (item.take_volume, item.take_pan) != *before
                    || !after.0.is_finite()
                    || !(0.0..=crate::MAX_AUDIO_ITEM_TAKE_VOLUME).contains(&after.0)
                    || !after.1.is_finite()
                    || !(-1.0..=1.0).contains(&after.1)
                {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                item.take_volume = after.0;
                item.take_pan = after.1;
            }
            ProjectEvent::AudioItemChanged { before, after } => {
                let index = state
                    .audio_items
                    .iter()
                    .position(|item| item.id == before.id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if state.audio_items[index] != *before
                    || before.id != after.id
                    || !state.tracks.iter().any(|track| track.id == after.track_id)
                    || !valid_audio_item(after)
                {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.audio_items[index] = after.clone();
            }
            ProjectEvent::MidiItemInserted { index, item } => {
                if *index > state.midi_items.len()
                    || !state.tracks.iter().any(|track| track.id == item.track_id)
                    || state
                        .midi_items
                        .iter()
                        .any(|existing| existing.id == item.id)
                    || state
                        .audio_items
                        .iter()
                        .any(|existing| existing.id == item.id)
                {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.midi_items.insert(*index, item.clone());
            }
            ProjectEvent::MidiItemRemoved { index, item } => {
                if state.midi_items.get(*index).map(|existing| existing.id) != Some(item.id) {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.midi_items.remove(*index);
            }
            ProjectEvent::MidiItemChanged { before, after } => {
                let index = state
                    .midi_items
                    .iter()
                    .position(|item| item.id == before.id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if state.midi_items[index] != *before
                    || before.id != after.id
                    || state.audio_items.iter().any(|item| item.id == after.id)
                    || !state.tracks.iter().any(|track| track.id == after.track_id)
                    || after.length_ticks == 0
                    || after.start_tick.checked_add(after.length_ticks).is_none()
                    || after
                        .source_offset_ticks
                        .checked_add(after.length_ticks)
                        .is_none()
                    || !valid_midi_controllers(&after.controllers)
                    || !valid_midi_pitch_bends(&after.pitch_bends)
                {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.midi_items[index] = after.clone();
            }
            ProjectEvent::MidiItemsReplaced {
                index,
                before,
                after,
            } => {
                let end = index
                    .checked_add(before.len())
                    .filter(|end| *end <= state.midi_items.len())
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if before.is_empty() || state.midi_items[*index..end] != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                let mut existing_item_ids = state
                    .audio_items
                    .iter()
                    .map(|item| item.id)
                    .chain(
                        state
                            .midi_items
                            .iter()
                            .enumerate()
                            .filter(|(item_index, _)| !(*index..end).contains(item_index))
                            .map(|(_, item)| item.id),
                    )
                    .collect::<HashSet<_>>();
                let mut existing_note_ids = state
                    .midi_items
                    .iter()
                    .enumerate()
                    .filter(|(item_index, _)| !(*index..end).contains(item_index))
                    .flat_map(|(_, item)| item.notes.iter().map(|note| note.id))
                    .collect::<HashSet<_>>();
                if after.is_empty()
                    || after.iter().any(|item| {
                        !state.tracks.iter().any(|track| track.id == item.track_id)
                            || item.length_ticks == 0
                            || item.start_tick.checked_add(item.length_ticks).is_none()
                            || item
                                .source_offset_ticks
                                .checked_add(item.length_ticks)
                                .is_none()
                            || !valid_midi_controllers(&item.controllers)
                            || !valid_midi_pitch_bends(&item.pitch_bends)
                            || !existing_item_ids.insert(item.id)
                            || item.notes.iter().any(|note| {
                                note.data.pitch > 127
                                    || note.data.velocity > 127
                                    || note.data.duration == 0
                                    || note.data.tick.checked_add(note.data.duration).is_none()
                                    || !existing_note_ids.insert(note.id)
                            })
                    })
                {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.midi_items.splice(*index..end, after.clone());
            }
            ProjectEvent::MidiNotesAdded { item_id, notes } => {
                let item = state
                    .midi_items
                    .iter_mut()
                    .find(|item| item.id == *item_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                let source_bounds = midi_item_source_tick_bounds(item);
                let current_notes = Arc::make_mut(&mut item.notes);
                for (index, note) in notes {
                    if *index > current_notes.len()
                        || current_notes.iter().any(|existing| existing.id == note.id)
                        || note.data.pitch > 127
                        || note.data.velocity > 127
                        || !valid_midi_note_range_in_bounds(
                            source_bounds,
                            note.data.tick,
                            note.data.duration,
                        )
                    {
                        return Err(ActionError::HistoryInvariantViolation);
                    }
                    current_notes.insert(*index, note.clone());
                }
            }
            ProjectEvent::MidiNotesRemoved { item_id, notes } => {
                let item = state
                    .midi_items
                    .iter_mut()
                    .find(|item| item.id == *item_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                let current_notes = Arc::make_mut(&mut item.notes);
                for (index, note) in notes.iter().rev() {
                    if current_notes.get(*index).map(|existing| existing.id) != Some(note.id) {
                        return Err(ActionError::HistoryInvariantViolation);
                    }
                    current_notes.remove(*index);
                }
            }
            ProjectEvent::MidiNoteChanged {
                item_id,
                before,
                after,
            } => {
                let item = state
                    .midi_items
                    .iter_mut()
                    .find(|item| item.id == *item_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if before.id != after.id
                    || after.data.pitch > 127
                    || after.data.velocity > 127
                    || !valid_midi_note_source_range(item, after.data.tick, after.data.duration)
                {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                let note = Arc::make_mut(&mut item.notes)
                    .iter_mut()
                    .find(|note| note.id == before.id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                if *note != *before {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                *note = after.clone();
            }
            ProjectEvent::MidiControllersChanged {
                item_id,
                before,
                after,
            } => {
                let item = state
                    .midi_items
                    .iter_mut()
                    .find(|item| item.id == *item_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                let (source_start, source_end) = midi_item_source_tick_bounds(item);
                if item.controllers.as_ref() != before
                    || !valid_midi_controllers(after)
                    || after.iter().any(|controller| {
                        (controller.tick < source_start || controller.tick >= source_end)
                            && !before.contains(controller)
                    })
                {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                item.controllers = Arc::new(after.clone());
            }
            ProjectEvent::MidiPitchBendsChanged {
                item_id,
                before,
                after,
            } => {
                let item = state
                    .midi_items
                    .iter_mut()
                    .find(|item| item.id == *item_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                let (source_start, source_end) = midi_item_source_tick_bounds(item);
                if item.pitch_bends.as_ref() != before
                    || !valid_midi_pitch_bends(after)
                    || after.iter().any(|bend| {
                        (bend.tick < source_start || bend.tick >= source_end)
                            && !before.contains(bend)
                    })
                {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                item.pitch_bends = Arc::new(after.clone());
            }
            ProjectEvent::MidiNotesQuantized { item_id, changes } => {
                let item = state
                    .midi_items
                    .iter_mut()
                    .find(|item| item.id == *item_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                let source_bounds = midi_item_source_tick_bounds(item);
                let current_notes = Arc::make_mut(&mut item.notes);
                for (note_id, before, after) in changes {
                    let note = current_notes
                        .iter_mut()
                        .find(|note| note.id == *note_id)
                        .ok_or(ActionError::HistoryInvariantViolation)?;
                    if note.data.tick != *before
                        || !valid_midi_note_range_in_bounds(
                            source_bounds,
                            *after,
                            note.data.duration,
                        )
                    {
                        return Err(ActionError::HistoryInvariantViolation);
                    }
                    note.data.tick = *after;
                }
            }
            ProjectEvent::Transaction { events, .. } => {
                for event in events {
                    Self::apply_event(state, event)?;
                }
            }
        }
        Ok(())
    }

    fn remove_audio_items(
        state: &mut ProjectState,
        audio_items: &[(usize, AudioItem)],
    ) -> Result<(), ActionError> {
        for (index, item) in audio_items.iter().rev() {
            if state.audio_items.get(*index).map(|existing| existing.id) != Some(item.id) {
                return Err(ActionError::HistoryInvariantViolation);
            }
            state.audio_items.remove(*index);
        }
        Ok(())
    }

    fn remove_midi_items(
        state: &mut ProjectState,
        midi_items: &[(usize, MidiItem)],
    ) -> Result<(), ActionError> {
        for (index, item) in midi_items.iter().rev() {
            if state.midi_items.get(*index).map(|existing| existing.id) != Some(item.id) {
                return Err(ActionError::HistoryInvariantViolation);
            }
            state.midi_items.remove(*index);
        }
        Ok(())
    }
}

fn valid_audio_item(item: &AudioItem) -> bool {
    !item.media_ref.trim().is_empty()
        && item.volume.is_finite()
        && (0.0..=crate::MAX_AUDIO_ITEM_VOLUME).contains(&item.volume)
        && item.take_volume.is_finite()
        && (0.0..=crate::MAX_AUDIO_ITEM_TAKE_VOLUME).contains(&item.take_volume)
        && item.take_pan.is_finite()
        && (-1.0..=1.0).contains(&item.take_pan)
        && item.length_samples > 0
        && item.start_sample.checked_add(item.length_samples).is_some()
        && valid_audio_source_offset(
            item.source_offset_samples,
            item.loop_source,
            item.source_section_start_samples,
            item.source_section_length_samples,
        )
}

fn is_frozen_render(state: &ProjectState, item_id: ItemId) -> bool {
    state
        .tracks
        .iter()
        .any(|track| track.frozen_audio_item_id == Some(item_id))
}

fn source_track_for_action(state: &ProjectState, action: &DawAction) -> Option<TrackId> {
    match action {
        DawAction::SetTrackInstrument { track_id, .. }
        | DawAction::SetTrackFxChain { track_id, .. }
        | DawAction::SetTrackFxParameter { track_id, .. }
        | DawAction::SetTrackFxParameterAutomation { track_id, .. }
        | DawAction::InsertMidiItem { track_id, .. } => Some(*track_id),
        DawAction::EditMidiItem { item_id, .. }
        | DawAction::EditMidiItemPlacement { item_id, .. }
        | DawAction::SetMidiItemContents { item_id, .. }
        | DawAction::TrimMidiItemStart { item_id, .. }
        | DawAction::DuplicateMidiItem { item_id }
        | DawAction::DuplicateMidiItemAt { item_id, .. }
        | DawAction::DuplicateMidiItemToTrack { item_id, .. }
        | DawAction::SplitMidiItem { item_id, .. }
        | DawAction::DeleteMidiItem { item_id }
        | DawAction::AddMidiNotes { item_id, .. }
        | DawAction::EditMidiNote { item_id, .. }
        | DawAction::DeleteMidiNotes { item_id, .. }
        | DawAction::SetMidiControllers { item_id, .. }
        | DawAction::SetMidiPitchBends { item_id, .. }
        | DawAction::QuantizeItem { item_id, .. } => state
            .midi_items
            .iter()
            .find(|item| item.id == *item_id)
            .map(|item| item.track_id),
        DawAction::MoveItemToTrack { item_id, .. } => state
            .midi_items
            .iter()
            .find(|item| item.id == *item_id)
            .map(|item| item.track_id),
        _ => None,
    }
}

fn valid_midi_controllers(controllers: &[MidiControllerData]) -> bool {
    let mut positions = HashSet::with_capacity(controllers.len());
    controllers.iter().all(|controller| {
        controller.controller <= 127
            && controller.value <= 127
            && positions.insert((controller.controller, controller.tick))
    })
}

fn midi_item_source_tick_bounds(item: &MidiItem) -> (u64, u64) {
    if item.is_loop_source() {
        (0, item.source_loop_length_ticks())
    } else {
        (
            item.source_offset_ticks(),
            item.source_offset_ticks()
                .saturating_add(item.length_ticks()),
        )
    }
}

fn valid_midi_note_source_range(item: &MidiItem, tick: u64, duration: u64) -> bool {
    valid_midi_note_range_in_bounds(midi_item_source_tick_bounds(item), tick, duration)
}

fn valid_midi_note_range_in_bounds(
    (source_start, source_end): (u64, u64),
    tick: u64,
    duration: u64,
) -> bool {
    duration > 0
        && tick >= source_start
        && tick
            .checked_add(duration)
            .is_some_and(|end| end <= source_end)
}

fn midi_source_tick_at_project_tick(item: &MidiItem, project_tick: u64) -> Option<u64> {
    if item.is_loop_source() {
        item.source_tick_at_project_tick(project_tick)
    } else {
        project_tick
            .checked_sub(item.start_tick())?
            .checked_add(item.source_offset_ticks())
    }
}

fn valid_midi_pitch_bends(pitch_bends: &[MidiPitchBendData]) -> bool {
    let mut positions = HashSet::with_capacity(pitch_bends.len());
    pitch_bends
        .iter()
        .all(|bend| bend.value <= 16_383 && positions.insert(bend.tick))
}

fn controllers_for_segment(
    controllers: &[MidiControllerData],
    start_tick: u64,
    end_tick: u64,
    preserve_tail: bool,
) -> Vec<MidiControllerData> {
    let mut segment = controllers
        .iter()
        .filter(|controller| {
            start_tick <= controller.tick && (preserve_tail || controller.tick < end_tick)
        })
        .map(|controller| MidiControllerData {
            tick: controller.tick - start_tick,
            ..*controller
        })
        .collect::<Vec<_>>();
    let mut latest_by_controller = std::collections::HashMap::<u8, MidiControllerData>::new();
    for controller in controllers
        .iter()
        .filter(|controller| controller.tick < start_tick)
    {
        latest_by_controller
            .entry(controller.controller)
            .and_modify(|latest| {
                if latest.tick < controller.tick {
                    *latest = *controller;
                }
            })
            .or_insert(*controller);
    }
    for (number, latest) in latest_by_controller {
        if !segment
            .iter()
            .any(|controller| controller.controller == number && controller.tick == 0)
        {
            segment.push(MidiControllerData { tick: 0, ..latest });
        }
    }
    segment.sort_unstable_by_key(|controller| (controller.tick, controller.controller));
    segment
}

fn pitch_bends_for_segment(
    pitch_bends: &[MidiPitchBendData],
    start_tick: u64,
    end_tick: u64,
    preserve_tail: bool,
) -> Vec<MidiPitchBendData> {
    let mut result = Vec::new();
    if let Some(latest) = pitch_bends
        .iter()
        .filter(|bend| bend.tick <= start_tick)
        .max_by_key(|bend| bend.tick)
    {
        result.push(MidiPitchBendData {
            tick: 0,
            value: latest.value,
        });
    }
    result.extend(
        pitch_bends
            .iter()
            .filter(|bend| bend.tick > start_tick && (preserve_tail || bend.tick < end_tick))
            .map(|bend| MidiPitchBendData {
                tick: bend.tick - start_tick,
                value: bend.value,
            }),
    );
    result
}

fn next_id(max_id: Option<u64>) -> Result<u64, SnapshotError> {
    max_id.map_or(Ok(0), |id| {
        id.checked_add(1).ok_or(SnapshotError::IdentifierExhausted)
    })
}

fn validate_send_edit(
    tracks: &[Track],
    source: TrackId,
    sends: &[crate::AudioSend],
) -> Result<(), ActionError> {
    if sends.iter().any(|send| !send.parameters.is_valid()) {
        return Err(ActionError::InvalidAudioSend);
    }
    if sends.iter().any(|send| {
        send.destination == source || !tracks.iter().any(|track| track.id == send.destination)
    }) {
        return Err(ActionError::InvalidTrackOutput);
    }
    let mut candidate = tracks.to_vec();
    candidate
        .iter_mut()
        .find(|track| track.id == source)
        .unwrap()
        .sends = sends.to_vec();
    if !valid_track_routing(&candidate) {
        return Err(ActionError::TrackRoutingCycle);
    }
    Ok(())
}

// Metadata is applied atomically, including in batches, but never rewound by media history.
fn undoable_event(event: ProjectEvent) -> Option<ProjectEvent> {
    match event {
        ProjectEvent::ProjectNotesChanged { .. } => None,
        ProjectEvent::ProjectSampleRateChanged { before, after } if before == after => None,
        ProjectEvent::Transaction { tx_id, events } => {
            let events: Vec<_> = events.into_iter().filter_map(undoable_event).collect();
            if events.is_empty() {
                None
            } else {
                Some(ProjectEvent::Transaction { tx_id, events })
            }
        }
        event => Some(event),
    }
}
