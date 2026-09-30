use crate::{ItemId, MidiNoteData, NoteId, TrackId};

/// A command that changes project state.
#[derive(Clone, Debug, PartialEq)]
pub enum DawAction {
    /// Create a track at `index` in the project's ordered track list.
    CreateTrack { index: usize, name: String },
    /// Set or insert a tempo point at the given project tick.
    SetTempo { start_tick: u64, bpm: f64 },
    /// Set a track's volume in decibels.
    SetTrackVolume { track_id: TrackId, volume_db: f32 },
    /// Set a track's pan position in the inclusive range `-1.0..=1.0`.
    SetTrackPan { track_id: TrackId, pan: f32 },
    /// Rename a track.
    SetTrackName { track_id: TrackId, name: String },
    /// Move a track to a final position in the ordered track list.
    MoveTrack { track_id: TrackId, index: usize },
    /// Insert an empty MIDI item on a track.
    InsertMidiItem {
        track_id: TrackId,
        start_tick: u64,
        length_ticks: u64,
    },
    /// Add notes to a MIDI item. Note ticks are relative to the item start.
    AddMidiNotes {
        item_id: ItemId,
        notes: Vec<MidiNoteData>,
    },
    /// Delete notes from a MIDI item by identifier.
    DeleteMidiNotes {
        item_id: ItemId,
        note_ids: Vec<NoteId>,
    },
    /// Delete a track from the project.
    DeleteTrack { track_id: TrackId },
    /// Apply several actions as one atomic, undoable transaction.
    BatchTransaction { tx_id: u64, actions: Vec<DawAction> },
}
