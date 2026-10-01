use crate::snapshot::{
    AudioItemSnapshot, MeterPointSnapshot, MidiItemSnapshot, MidiNoteSnapshot, ProjectSnapshot,
    SnapshotError, TempoPointSnapshot, TrackSnapshot,
};
use crate::timebase::{MeterMap, TempoMap};
use crate::{
    ActionError, AudioItem, DawAction, ItemId, MidiItem, MidiNote, MusicalPosition, NoteId,
    ProjectSettings, TempoCurve, TimeSignature, TimebaseError, Track, TrackId,
};
use std::collections::HashSet;
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
    audio_items: Vec<AudioItem>,
    midi_items: Vec<MidiItem>,
    tempo_map: TempoMap,
    meter_map: MeterMap,
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
                meter_map: MeterMap::new(settings.ppq()),
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

    /// Converts a tick to a one-based measure/beat and an in-beat tick offset.
    pub fn musical_position_at_tick(&self, tick: u64) -> Result<MusicalPosition, TimebaseError> {
        self.state.meter_map.position_at_tick(tick)
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
            settings: self.state.tempo_map.settings(),
            tracks: self
                .state
                .tracks
                .iter()
                .map(|track| TrackSnapshot {
                    id: track.id.value(),
                    name: track.name.clone(),
                    volume_db: track.volume_db,
                    pan: track.pan,
                    muted: track.muted,
                    solo: track.solo,
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
                    start_tick: item.start_tick,
                    length_ticks: item.length_ticks,
                    notes: item
                        .notes
                        .iter()
                        .map(|note| MidiNoteSnapshot {
                            id: note.id.value(),
                            data: note.data,
                        })
                        .collect(),
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
        if first_meter.start_tick != 0 || first_meter.numerator != 4 || first_meter.denominator != 4
        {
            return Err(SnapshotError::InvalidProjectData);
        }
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
            if !track_ids.insert(track.id)
                || !track.volume_db.is_finite()
                || !track.pan.is_finite()
                || !(-1.0..=1.0).contains(&track.pan)
            {
                return Err(SnapshotError::InvalidProjectData);
            }
            max_track_id = Some(max_track_id.map_or(track.id, |max: u64| max.max(track.id)));
            tracks.push(Track {
                id: TrackId::from_raw(track.id),
                name: track.name,
                volume_db: track.volume_db,
                pan: track.pan,
                muted: track.muted,
                solo: track.solo,
            });
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
                || item.length_ticks == 0
                || item.start_tick.checked_add(item.length_ticks).is_none()
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
                    || data
                        .tick
                        .checked_add(data.duration)
                        .is_none_or(|end| end > item.length_ticks)
                {
                    return Err(SnapshotError::InvalidProjectData);
                }
                max_note_id = Some(max_note_id.map_or(note.id, |max: u64| max.max(note.id)));
                notes.push(MidiNote {
                    id: NoteId::from_raw(note.id),
                    data,
                });
            }
            midi_items.push(MidiItem {
                id: ItemId::from_raw(item.id),
                track_id: TrackId::from_raw(item.track_id),
                start_tick: item.start_tick,
                length_ticks: item.length_ticks,
                notes: Arc::new(notes),
            });
        }

        Ok(Self {
            state: ProjectState {
                tracks,
                audio_items,
                midi_items,
                tempo_map,
                meter_map,
            },
            ids: IdAllocator {
                next_track_id: next_id(max_track_id)?,
                next_item_id: next_id(max_item_id)?,
                next_note_id: next_id(max_note_id)?,
            },
            history: Vec::new(),
            history_cursor: 0,
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
                    muted: false,
                    solo: false,
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
            DawAction::DeleteAudioItem { item_id } => {
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
                if item.notes.iter().any(|note| {
                    note.data
                        .tick
                        .checked_add(note.data.duration)
                        .is_none_or(|end| end > length_ticks)
                }) {
                    return Err(ActionError::InvalidMidiNote);
                }
                let before = item.clone();
                let after = MidiItem {
                    start_tick,
                    length_ticks,
                    ..before.clone()
                };
                ProjectEvent::MidiItemChanged { before, after }
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
                    || data
                        .tick
                        .checked_add(data.duration)
                        .is_none_or(|end| end > item.length_ticks)
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
                        .start_tick
                        .checked_add(note.data.tick)
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
                    let tick = quantized_absolute.saturating_sub(item.start_tick);
                    if tick
                        .checked_add(note.data.duration)
                        .is_none_or(|end| end > item.length_ticks)
                    {
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
                    || before.track_id != after.track_id
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
                    || before.track_id != after.track_id
                    || state.audio_items.iter().any(|item| item.id == after.id)
                    || !state.tracks.iter().any(|track| track.id == after.track_id)
                    || after.length_ticks == 0
                    || after.start_tick.checked_add(after.length_ticks).is_none()
                    || after.notes.iter().any(|note| {
                        note.data
                            .tick
                            .checked_add(note.data.duration)
                            .is_none_or(|end| end > after.length_ticks)
                    })
                {
                    return Err(ActionError::HistoryInvariantViolation);
                }
                state.midi_items[index] = after.clone();
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
                    || after
                        .data
                        .tick
                        .checked_add(after.data.duration)
                        .is_none_or(|end| end > item.length_ticks)
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
            ProjectEvent::MidiNotesQuantized { item_id, changes } => {
                let item = state
                    .midi_items
                    .iter_mut()
                    .find(|item| item.id == *item_id)
                    .ok_or(ActionError::HistoryInvariantViolation)?;
                let length_ticks = item.length_ticks;
                let current_notes = Arc::make_mut(&mut item.notes);
                for (note_id, before, after) in changes {
                    let note = current_notes
                        .iter_mut()
                        .find(|note| note.id == *note_id)
                        .ok_or(ActionError::HistoryInvariantViolation)?;
                    if note.data.tick != *before
                        || after
                            .checked_add(note.data.duration)
                            .is_none_or(|end| end > length_ticks)
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

fn next_id(max_id: Option<u64>) -> Result<u64, SnapshotError> {
    max_id.map_or(Ok(0), |id| {
        id.checked_add(1).ok_or(SnapshotError::IdentifierExhausted)
    })
}
