use crate::snapshot::{
    AudioItemSnapshot, MeterPointSnapshot, MidiItemSnapshot, MidiNoteSnapshot, ProjectSnapshot,
    SnapshotError, TempoPointSnapshot, TrackFxParameterAutomationLaneSnapshot,
    TrackFxParameterAutomationPointSnapshot, TrackFxParameterValueSnapshot, TrackFxPluginSnapshot,
    TrackSnapshot,
};
use crate::timebase::{MeterMap, TempoMap};
use crate::{
    ActionError, AudioItem, DawAction, FxParameterAutomationLane, FxParameterAutomationPoint,
    ItemId, MAX_TRACK_FX_PARAMETER_AUTOMATION_LANES, MidiControllerData, MidiItem, MidiNote,
    MidiPitchBendData, MusicalPosition, NoteId, ProjectSettings, TempoCurve, TimeSignature,
    TimebaseError, Track, TrackFxPlugin, TrackId, TrackInstrument,
};
use std::collections::HashSet;
use std::sync::Arc;

const MAX_VOLUME_AUTOMATION_POINTS: usize = 65_536;
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

fn valid_fx_parameter_automation(points: &[FxParameterAutomationPoint]) -> bool {
    points.len() <= MAX_FX_PARAMETER_AUTOMATION_POINTS
        && points.iter().all(|point| point.value().is_finite())
        && points
            .windows(2)
            .all(|pair| pair[0].sample() < pair[1].sample())
}

fn routing_destinations(track: &Track) -> impl Iterator<Item = TrackId> + '_ {
    // Dormant connections remain part of validation so toggling mute/main send
    // cannot introduce a cycle that was hidden while the route was disabled.
    track
        .output_track
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

#[derive(Clone, Debug, Default)]
struct ProjectState {
    pan_mode: crate::PanMode,
    tracks: Vec<Track>,
    audio_items: Vec<AudioItem>,
    midi_items: Vec<MidiItem>,
    tempo_map: TempoMap,
    meter_map: MeterMap,
}

#[derive(Clone, Debug, Default)]
struct IdAllocator {
    next_track_id: u64,
    next_send_id: u64,
    next_item_id: u64,
    next_note_id: u64,
}

#[derive(Clone, Debug, PartialEq)]
enum ProjectEvent {
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
    TrackPanChanged {
        track_id: TrackId,
        before: f32,
        after: f32,
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
    TrackMoved {
        track_id: TrackId,
        from: usize,
        to: usize,
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
            Self::TrackPanChanged {
                track_id,
                before,
                after,
            } => Self::TrackPanChanged {
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
            Self::TrackMoved { track_id, from, to } => Self::TrackMoved {
                track_id: *track_id,
                from: *to,
                to: *from,
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
        length_ticks: item_end - start_tick,
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
    /// Creates an empty project with 48 kHz, 960 PPQ and 120 BPM defaults.
    pub fn new() -> Self {
        Self::with_settings(ProjectSettings::default())
    }

    /// Creates an empty project with explicit timebase settings.
    pub fn with_settings(settings: ProjectSettings) -> Self {
        Self {
            state: ProjectState {
                pan_mode: settings.pan_mode(),
                tempo_map: TempoMap::new(settings),
                meter_map: MeterMap::new(settings.ppq()),
                ..ProjectState::default()
            },
            ..Self::default()
        }
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
    }

    /// Converts a PPQ tick position to the nearest sample index.
    pub fn sample_at_tick(&self, tick: u64) -> Result<u64, TimebaseError> {
        self.state.tempo_map.sample_at_tick(tick)
    }

    /// Converts a sample index to the nearest PPQ tick position.
    pub fn tick_at_sample(&self, sample: u64) -> Result<u64, TimebaseError> {
        self.state.tempo_map.tick_at_sample(sample)
    }

    /// Returns the tempo active at a PPQ tick position.
    pub fn tempo_at_tick(&self, tick: u64) -> f64 {
        self.state.tempo_map.tempo_at_tick(tick)
    }

    /// Returns the ordered tempo points as `(start_tick, bpm, curve_to_next)` tuples.
    pub fn tempo_points(&self) -> impl Iterator<Item = (u64, f64, TempoCurve)> + '_ {
        self.state.tempo_map.points()
    }

    /// Converts a tick to a one-based measure/beat and an in-beat tick offset.
    pub fn musical_position_at_tick(&self, tick: u64) -> Result<MusicalPosition, TimebaseError> {
        self.state.meter_map.position_at_tick(tick)
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

    /// Returns whether the next undo changes only a track's volume or pan.
    pub fn can_undo_track_mix(&self) -> bool {
        matches!(
            self.history_cursor
                .checked_sub(1)
                .and_then(|index| self.history.get(index)),
            Some(
                ProjectEvent::TrackVolumeChanged { .. }
                    | ProjectEvent::TrackVolumeAutomationChanged { .. }
                    | ProjectEvent::TrackPanChanged { .. },
            )
        )
    }

    /// Returns whether the next redo changes only a track's volume or pan.
    pub fn can_redo_track_mix(&self) -> bool {
        matches!(
            self.history.get(self.history_cursor),
            Some(
                ProjectEvent::TrackVolumeChanged { .. }
                    | ProjectEvent::TrackVolumeAutomationChanged { .. }
                    | ProjectEvent::TrackPanChanged { .. },
            )
        )
    }

    /// Applies an action atomically and records it as one undoable history entry.
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
        ) {
            return Ok(());
        }

        self.state = state;
        self.ids = ids;
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

    /// Creates a serialization-friendly copy of the current project state.
    pub fn snapshot(&self) -> ProjectSnapshot {
        ProjectSnapshot {
            settings: self.settings(),
            tracks: self
                .state
                .tracks
                .iter()
                .map(|track| TrackSnapshot {
                    id: track.id.value(),
                    name: track.name.clone(),
                    is_bus: track.is_bus,
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
                    frozen_audio_item_id: track.frozen_audio_item_id.map(|id| id.value()),
                })
                .collect(),
            audio_items: self
                .state
                .audio_items
                .iter()
                .map(|item| AudioItemSnapshot {
                    id: item.id.value(),
                    track_id: item.track_id.value(),
                    media_ref: item.media_ref.clone(),
                    start_sample: item.start_sample,
                    source_offset_samples: item.source_offset_samples,
                    length_samples: item.length_samples,
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
                    length_ticks: item.length_ticks,
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

        let mut meter_map = MeterMap::new(settings.ppq());
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
                output_track: track.output_track_id.map(TrackId::from_raw),
                main_send_enabled: track.main_send_enabled,
                sends,
                volume_db: track.volume_db,
                pan: track.pan,
                pan_mode: settings.pan_mode(),
                muted: track.muted,
                solo: track.solo,
                record_armed: track.record_armed,
                instrument,
                fx_chain,
                volume_automation: track.volume_automation,
                frozen_audio_item_id: track.frozen_audio_item_id.map(ItemId::from_raw),
            });
        }
        if !valid_track_routing(&tracks) {
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
                || item.length_samples == 0
                || item.start_sample.checked_add(item.length_samples).is_none()
            {
                return Err(SnapshotError::InvalidProjectData);
            }
            max_item_id = Some(max_item_id.map_or(item.id, |max: u64| max.max(item.id)));
            audio_items.push(AudioItem {
                id: ItemId::from_raw(item.id),
                track_id: TrackId::from_raw(item.track_id),
                media_ref: item.media_ref,
                start_sample: item.start_sample,
                source_offset_samples: item.source_offset_samples,
                length_samples: item.length_samples,
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
                length_ticks: item.length_ticks,
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
                pan_mode: settings.pan_mode(),
                tracks,
                audio_items,
                midi_items,
                tempo_map,
                meter_map,
            },
            ids: IdAllocator {
                next_track_id: next_id(max_track_id)?,
                next_send_id: next_id(max_send_id)?,
                next_item_id: next_id(max_item_id)?,
                next_note_id: next_id(max_note_id)?,
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
                    output_track: None,
                    main_send_enabled: true,
                    sends: Vec::new(),
                    volume_db: 0.0,
                    pan: 0.0,
                    pan_mode: state.pan_mode,
                    muted: false,
                    solo: false,
                    record_armed: false,
                    instrument: None,
                    fx_chain: Vec::new(),
                    volume_automation: Vec::new(),
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
                    output_track: None,
                    main_send_enabled: true,
                    sends: Vec::new(),
                    volume_db: 0.0,
                    pan: 0.0,
                    pan_mode: state.pan_mode,
                    muted: false,
                    solo: false,
                    record_armed: false,
                    instrument: None,
                    fx_chain: Vec::new(),
                    volume_automation: Vec::new(),
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
                    id: ItemId::from_raw(ids.next_item_id),
                    track_id,
                    media_ref,
                    start_sample,
                    source_offset_samples: 0,
                    length_samples,
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
                ProjectEvent::TrackMoved {
                    track_id,
                    from,
                    to: index,
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
                    id: ItemId::from_raw(ids.next_item_id),
                    track_id,
                    media_ref,
                    start_sample,
                    source_offset_samples,
                    length_samples,
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
                    length_ticks,
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
                        length_ticks: segment_end - segment_start,
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
                        || data.duration == 0
                        || data.tick < item.source_offset_ticks
                        || data.tick.checked_add(data.duration).is_none_or(|end| {
                            end > item.source_offset_ticks.saturating_add(item.length_ticks)
                        })
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
                    || data.duration == 0
                    || data.tick < item.source_offset_ticks
                    || data.tick.checked_add(data.duration).is_none_or(|end| {
                        end > item.source_offset_ticks.saturating_add(item.length_ticks)
                    })
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
                let source_end_tick = item.source_offset_ticks.saturating_add(item.length_ticks);
                if !valid_midi_controllers(&controllers)
                    || controllers.iter().any(|controller| {
                        (controller.tick < item.source_offset_ticks
                            || controller.tick >= source_end_tick)
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
                let source_end_tick = item.source_offset_ticks.saturating_add(item.length_ticks);
                if !valid_midi_pitch_bends(&pitch_bends)
                    || pitch_bends.iter().any(|bend| {
                        (bend.tick < item.source_offset_ticks || bend.tick >= source_end_tick)
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
                    let tick = quantized_absolute
                        .saturating_sub(item.start_tick)
                        .saturating_add(item.source_offset_ticks);
                    if tick.checked_add(note.data.duration).is_none_or(|end| {
                        tick < item.source_offset_ticks
                            || end > item.source_offset_ticks.saturating_add(item.length_ticks)
                    }) {
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
                    routing_destinations(track).any(|destination| destination == track_id)
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
            ProjectEvent::TrackMoved { track_id, from, to } => {
                if *to >= state.tracks.len()
                    || state.tracks.get(*from).map(|track| track.id) != Some(*track_id)
                {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                let track = state.tracks.remove(*from);
                state.tracks.insert(*to, track);
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
                let current_notes = Arc::make_mut(&mut item.notes);
                for (index, note) in notes {
                    if *index > current_notes.len()
                        || current_notes.iter().any(|existing| existing.id == note.id)
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
                    || after.data.duration == 0
                    || after.data.tick < item.source_offset_ticks
                    || after
                        .data
                        .tick
                        .checked_add(after.data.duration)
                        .is_none_or(|end| {
                            end > item.source_offset_ticks.saturating_add(item.length_ticks)
                        })
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
                if item.controllers.as_ref() != before || !valid_midi_controllers(after) {
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
                if item.pitch_bends.as_ref() != before || !valid_midi_pitch_bends(after) {
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
                let source_offset_ticks = item.source_offset_ticks;
                let source_end_tick = source_offset_ticks.saturating_add(item.length_ticks);
                let current_notes = Arc::make_mut(&mut item.notes);
                for (note_id, before, after) in changes {
                    let note = current_notes
                        .iter_mut()
                        .find(|note| note.id == *note_id)
                        .ok_or(ActionError::HistoryInvariantViolation)?;
                    if note.data.tick != *before
                        || *after < source_offset_ticks
                        || after
                            .checked_add(note.data.duration)
                            .is_none_or(|end| end > source_end_tick)
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
        && item.length_samples > 0
        && item.start_sample.checked_add(item.length_samples).is_some()
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
