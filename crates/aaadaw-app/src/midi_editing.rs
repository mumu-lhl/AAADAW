//! MIDI editing intents that translate into validated project actions.
//!
//! This module keeps timeline defaults and common note gestures out of the UI. The returned
//! actions still pass through `Project::apply`, which owns validation and undo history.

use aaadaw_core::{DawAction, GridFraction, ItemId, MidiNoteData, NoteId, Project};
use std::fmt;

/// A failed MIDI editing intent that could not be turned into a project action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MidiEditError {
    NoTrack,
    ItemNotFound,
    NoteNotFound,
    TimelinePositionOutOfRange,
    ItemHasNoRoomForNote,
    CannotMoveBeforeTimelineStart,
    CannotMoveBeyondTimeline,
    InvalidGrid,
}

impl fmt::Display for MidiEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NoTrack => "add a track before creating a MIDI item",
            Self::ItemNotFound => "MIDI item no longer exists",
            Self::NoteNotFound => "MIDI note no longer exists",
            Self::TimelinePositionOutOfRange => "MIDI timeline position is out of range",
            Self::ItemHasNoRoomForNote => "MIDI item has no room for another quarter note",
            Self::CannotMoveBeforeTimelineStart => "MIDI item or note cannot move before tick zero",
            Self::CannotMoveBeyondTimeline => {
                "MIDI item or note cannot move beyond the tick timeline"
            }
            Self::InvalidGrid => "could not create the 1/16 MIDI grid",
        })
    }
}

impl std::error::Error for MidiEditError {}

/// Creates a four-quarter-note MIDI item on the first track, after existing MIDI items.
pub fn create_four_beat_midi_item(project: &Project) -> Result<DawAction, MidiEditError> {
    let track_id = project
        .tracks()
        .first()
        .map(|track| track.id())
        .ok_or(MidiEditError::NoTrack)?;
    let start_tick = project
        .midi_items()
        .iter()
        .try_fold(0_u64, |end, item| {
            item.start_tick()
                .checked_add(item.length_ticks())
                .map(|item_end| end.max(item_end))
        })
        .ok_or(MidiEditError::TimelinePositionOutOfRange)?;
    let length_ticks = u64::from(project.settings().ppq()) * 4;
    if start_tick.checked_add(length_ticks).is_none() {
        return Err(MidiEditError::TimelinePositionOutOfRange);
    }

    Ok(DawAction::InsertMidiItem {
        track_id,
        start_tick,
        length_ticks,
    })
}

/// Appends a quarter-note C4 at the first free tick in an item.
pub fn add_quarter_note(project: &Project, item_id: ItemId) -> Result<DawAction, MidiEditError> {
    let item = project
        .midi_items()
        .iter()
        .find(|item| item.id() == item_id)
        .ok_or(MidiEditError::ItemNotFound)?;
    let tick = item
        .notes()
        .iter()
        .filter_map(|note| note.tick().checked_add(note.duration()))
        .max()
        .unwrap_or(0);
    let duration = u64::from(project.settings().ppq());
    if tick
        .checked_add(duration)
        .is_none_or(|end| end > item.length_ticks())
    {
        return Err(MidiEditError::ItemHasNoRoomForNote);
    }

    Ok(DawAction::AddMidiNotes {
        item_id,
        notes: vec![MidiNoteData {
            pitch: 60,
            tick,
            duration,
            velocity: 100,
        }],
    })
}

/// Moves a MIDI item by one quarter note while preserving its length.
pub fn move_midi_item_by_beat(
    project: &Project,
    item_id: ItemId,
    direction: i8,
) -> Result<DawAction, MidiEditError> {
    let item = project
        .midi_items()
        .iter()
        .find(|item| item.id() == item_id)
        .ok_or(MidiEditError::ItemNotFound)?;
    let beat_ticks = u64::from(project.settings().ppq());
    let start_tick = match direction {
        -1 => item
            .start_tick()
            .checked_sub(beat_ticks)
            .ok_or(MidiEditError::CannotMoveBeforeTimelineStart)?,
        1 => item
            .start_tick()
            .checked_add(beat_ticks)
            .ok_or(MidiEditError::CannotMoveBeyondTimeline)?,
        _ => return Err(MidiEditError::CannotMoveBeyondTimeline),
    };
    if start_tick.checked_add(item.length_ticks()).is_none() {
        return Err(MidiEditError::CannotMoveBeyondTimeline);
    }

    Ok(DawAction::EditMidiItem {
        item_id,
        start_tick,
        length_ticks: item.length_ticks(),
    })
}

/// Moves a MIDI note by one sixteenth note, retaining its other properties.
pub fn move_midi_note_by_sixteenth(
    project: &Project,
    item_id: ItemId,
    note_id: NoteId,
    direction: i8,
) -> Result<DawAction, MidiEditError> {
    let item = project
        .midi_items()
        .iter()
        .find(|item| item.id() == item_id)
        .ok_or(MidiEditError::ItemNotFound)?;
    let mut data = find_note(project, item_id, note_id)?;
    let step = (u64::from(project.settings().ppq()) / 4).max(1);
    data.tick = match direction {
        -1 => data
            .tick
            .checked_sub(step)
            .ok_or(MidiEditError::CannotMoveBeforeTimelineStart)?,
        1 => data
            .tick
            .checked_add(step)
            .ok_or(MidiEditError::CannotMoveBeyondTimeline)?,
        _ => return Err(MidiEditError::CannotMoveBeyondTimeline),
    };
    if data
        .tick
        .checked_add(data.duration)
        .is_none_or(|end| end > item.length_ticks())
    {
        return Err(MidiEditError::CannotMoveBeyondTimeline);
    }
    Ok(edit_note_action(item_id, note_id, data))
}

/// Adjusts MIDI pitch, clamping it to the standard 0–127 range.
pub fn adjust_midi_note_pitch(
    project: &Project,
    item_id: ItemId,
    note_id: NoteId,
    delta: i8,
) -> Result<DawAction, MidiEditError> {
    let mut data = find_note(project, item_id, note_id)?;
    data.pitch = (i16::from(data.pitch) + i16::from(delta)).clamp(0, 127) as u8;
    Ok(edit_note_action(item_id, note_id, data))
}

/// Adjusts MIDI velocity, clamping it to the standard 0–127 range.
pub fn adjust_midi_note_velocity(
    project: &Project,
    item_id: ItemId,
    note_id: NoteId,
    delta: i8,
) -> Result<DawAction, MidiEditError> {
    let mut data = find_note(project, item_id, note_id)?;
    data.velocity = (i16::from(data.velocity) + i16::from(delta)).clamp(0, 127) as u8;
    Ok(edit_note_action(item_id, note_id, data))
}

/// Creates an action that removes a single note from an item.
pub fn delete_midi_note(item_id: ItemId, note_id: NoteId) -> DawAction {
    DawAction::DeleteMidiNotes {
        item_id,
        note_ids: vec![note_id],
    }
}

/// Creates a full-strength sixteenth-note quantization action.
pub fn quantize_midi_item_to_sixteenth(item_id: ItemId) -> Result<DawAction, MidiEditError> {
    let grid = GridFraction::new(1, 16).map_err(|_| MidiEditError::InvalidGrid)?;
    Ok(DawAction::QuantizeItem {
        item_id,
        grid,
        strength: 1.0,
    })
}

fn find_note(
    project: &Project,
    item_id: ItemId,
    note_id: NoteId,
) -> Result<MidiNoteData, MidiEditError> {
    let item = project
        .midi_items()
        .iter()
        .find(|item| item.id() == item_id)
        .ok_or(MidiEditError::ItemNotFound)?;
    let note = item
        .notes()
        .iter()
        .find(|note| note.id() == note_id)
        .ok_or(MidiEditError::NoteNotFound)?;
    Ok(MidiNoteData {
        pitch: note.pitch(),
        tick: note.tick(),
        duration: note.duration(),
        velocity: note.velocity(),
    })
}

fn edit_note_action(item_id: ItemId, note_id: NoteId, data: MidiNoteData) -> DawAction {
    DawAction::EditMidiNote {
        item_id,
        note_id,
        data,
    }
}
