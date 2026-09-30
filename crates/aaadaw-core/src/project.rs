use crate::timebase::TempoMap;
use crate::{
    ActionError, DawAction, ItemId, MidiItem, MidiNote, NoteId, ProjectSettings, TimebaseError,
    Track, TrackId,
};
use std::sync::Arc;

/// Mutable project state. All changes are made through [`DawAction`]s.
#[derive(Debug, Default)]
pub struct Project {
    state: ProjectState,
    ids: IdAllocator,
    history: Vec<ProjectEvent>,
    history_cursor: usize,
}

#[derive(Clone, Debug, Default)]
struct ProjectState {
    tracks: Vec<Track>,
    midi_items: Vec<MidiItem>,
    tempo_map: TempoMap,
}

#[derive(Clone, Debug, Default)]
struct IdAllocator {
    next_track_id: u64,
    next_item_id: u64,
    next_note_id: u64,
}

#[derive(Clone, Debug, PartialEq)]
enum ProjectEvent {
    TrackCreated {
        index: usize,
        track: Track,
    },
    TrackDeleted {
        index: usize,
        track: Track,
        midi_items: Vec<(usize, MidiItem)>,
    },
    TrackRestored {
        index: usize,
        track: Track,
        midi_items: Vec<(usize, MidiItem)>,
    },
    TrackVolumeChanged {
        track_id: TrackId,
        before: f32,
        after: f32,
    },
    TrackPanChanged {
        track_id: TrackId,
        before: f32,
        after: f32,
    },
    TrackRenamed {
        track_id: TrackId,
        before: String,
        after: String,
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
    MidiItemInserted {
        index: usize,
        item: MidiItem,
    },
    MidiItemRemoved {
        index: usize,
        item: MidiItem,
    },
    MidiNotesAdded {
        item_id: ItemId,
        notes: Vec<(usize, MidiNote)>,
    },
    MidiNotesRemoved {
        item_id: ItemId,
        notes: Vec<(usize, MidiNote)>,
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
                midi_items: Vec::new(),
            },
            Self::TrackDeleted {
                index,
                track,
                midi_items,
            } => Self::TrackRestored {
                index: *index,
                track: track.clone(),
                midi_items: midi_items.clone(),
            },
            Self::TrackRestored {
                index,
                track,
                midi_items,
            } => Self::TrackDeleted {
                index: *index,
                track: track.clone(),
                midi_items: midi_items.clone(),
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
            Self::TrackPanChanged {
                track_id,
                before,
                after,
            } => Self::TrackPanChanged {
                track_id: *track_id,
                before: *after,
                after: *before,
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
            Self::MidiItemInserted { index, item } => Self::MidiItemRemoved {
                index: *index,
                item: item.clone(),
            },
            Self::MidiItemRemoved { index, item } => Self::MidiItemInserted {
                index: *index,
                item: item.clone(),
            },
            Self::MidiNotesAdded { item_id, notes } => Self::MidiNotesRemoved {
                item_id: *item_id,
                notes: notes.clone(),
            },
            Self::MidiNotesRemoved { item_id, notes } => Self::MidiNotesAdded {
                item_id: *item_id,
                notes: notes.clone(),
            },
            Self::Transaction { tx_id, events } => Self::Transaction {
                tx_id: *tx_id,
                events: events.iter().rev().map(Self::inverse).collect(),
            },
        }
    }
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
                tempo_map: TempoMap::new(settings),
                ..ProjectState::default()
            },
            ..Self::default()
        }
    }

    /// Returns the project's immutable sample-rate and PPQ settings.
    pub fn settings(&self) -> ProjectSettings {
        self.state.tempo_map.settings()
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

    /// Applies an action atomically and records it as one undoable history entry.
    ///
    /// A failed action, including a failed nested action in a transaction, leaves
    /// project state, identifier allocation, and history unchanged.
    pub fn apply(&mut self, action: DawAction) -> Result<(), ActionError> {
        let mut state = self.state.clone();
        let mut ids = self.ids.clone();
        let event = Self::apply_action(&mut state, &mut ids, action)?;

        self.state = state;
        self.ids = ids;
        self.history.truncate(self.history_cursor);
        self.history.push(event);
        self.history_cursor += 1;
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
        Ok(true)
    }

    /// Returns the project's tracks in timeline order.
    pub fn tracks(&self) -> &[Track] {
        &self.state.tracks
    }

    /// Returns all MIDI items in project insertion order.
    pub fn midi_items(&self) -> &[MidiItem] {
        &self.state.midi_items
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
                    volume_db: 0.0,
                    pan: 0.0,
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
                let next_id = ids
                    .next_item_id
                    .checked_add(1)
                    .ok_or(ActionError::ItemIdExhausted)?;
                let item = MidiItem {
                    id: ItemId::from_raw(ids.next_item_id),
                    track_id,
                    start_tick,
                    length_ticks,
                    notes: Arc::new(Vec::new()),
                };
                ids.next_item_id = next_id;
                ProjectEvent::MidiItemInserted {
                    index: state.midi_items.len(),
                    item,
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
                        || data
                            .tick
                            .checked_add(data.duration)
                            .is_none_or(|end| end > item.length_ticks)
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
            DawAction::DeleteTrack { track_id } => {
                let index = state
                    .tracks
                    .iter()
                    .position(|track| track.id == track_id)
                    .ok_or(ActionError::TrackNotFound { track_id })?;
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
                midi_items,
            } => {
                if state.tracks.get(*index).map(|item| item.id) != Some(track.id) {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                Self::remove_midi_items(state, midi_items)?;
                state.tracks.remove(*index);
            }
            ProjectEvent::TrackRestored {
                index,
                track,
                midi_items,
            } => {
                if *index > state.tracks.len()
                    || state.tracks.iter().any(|item| item.id == track.id)
                {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.tracks.insert(*index, track.clone());
                for (item_index, item) in midi_items {
                    if item.track_id != track.id || *item_index > state.midi_items.len() {
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
            ProjectEvent::MidiItemInserted { index, item } => {
                if *index > state.midi_items.len()
                    || !state.tracks.iter().any(|track| track.id == item.track_id)
                    || state
                        .midi_items
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
            ProjectEvent::Transaction { events, .. } => {
                for event in events {
                    Self::apply_event(state, event)?;
                }
            }
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
