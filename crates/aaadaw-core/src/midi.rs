use crate::TrackId;
use std::sync::Arc;

/// The identifier of a MIDI item in a project.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ItemId(u64);

impl ItemId {
    pub(crate) fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// Returns the stable numeric value of this identifier.
    pub fn value(self) -> u64 {
        self.0
    }
}

/// The identifier of a MIDI note in a project.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NoteId(u64);

impl NoteId {
    pub(crate) fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// Returns the stable numeric value of this identifier.
    pub fn value(self) -> u64 {
        self.0
    }
}

/// Input data for a MIDI note; `tick` is relative to its containing item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiNoteData {
    pub pitch: u8,
    pub tick: u64,
    pub duration: u64,
    pub velocity: u8,
}

/// Input data for a MIDI control-change event; `tick` is relative to its item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiControllerData {
    pub controller: u8,
    pub tick: u64,
    pub value: u8,
}

/// A MIDI note stored in a project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MidiNote {
    pub(crate) id: NoteId,
    pub(crate) data: MidiNoteData,
}

impl MidiNote {
    /// Returns this note's identifier.
    pub fn id(&self) -> NoteId {
        self.id
    }

    /// Returns the MIDI pitch in the range `0..=127`.
    pub fn pitch(&self) -> u8 {
        self.data.pitch
    }

    /// Returns the note's start tick relative to its item.
    pub fn tick(&self) -> u64 {
        self.data.tick
    }

    /// Returns the note duration in ticks.
    pub fn duration(&self) -> u64 {
        self.data.duration
    }

    /// Returns the MIDI velocity in the range `0..=127`.
    pub fn velocity(&self) -> u8 {
        self.data.velocity
    }
}

/// A MIDI clip positioned on a track.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MidiItem {
    pub(crate) id: ItemId,
    pub(crate) track_id: TrackId,
    pub(crate) start_tick: u64,
    pub(crate) length_ticks: u64,
    pub(crate) notes: Arc<Vec<MidiNote>>,
    pub(crate) controllers: Arc<Vec<MidiControllerData>>,
}

impl MidiItem {
    /// Returns this item's identifier.
    pub fn id(&self) -> ItemId {
        self.id
    }

    /// Returns the identifier of the track containing this item.
    pub fn track_id(&self) -> TrackId {
        self.track_id
    }

    /// Returns the item's start position in project ticks.
    pub fn start_tick(&self) -> u64 {
        self.start_tick
    }

    /// Returns the item's duration in project ticks.
    pub fn length_ticks(&self) -> u64 {
        self.length_ticks
    }

    /// Returns the item's notes in insertion order.
    pub fn notes(&self) -> &[MidiNote] {
        &self.notes
    }

    /// Returns control-change events sorted by tick, then controller number.
    pub fn controllers(&self) -> &[MidiControllerData] {
        &self.controllers
    }
}
