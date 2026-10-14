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

    /// Reconstructs a nonzero stable note identifier from a serialized or IPC value.
    pub fn from_value(value: u64) -> Option<Self> {
        (value != 0).then_some(Self(value))
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

/// Input data for a MIDI 14-bit pitch-bend event; `tick` is relative to its item.
/// The neutral pitch-bend value is 8192.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiPitchBendData {
    pub tick: u64,
    pub value: u16,
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
    pub(crate) name: String,
    pub(crate) start_tick: u64,
    pub(crate) source_offset_ticks: u64,
    pub(crate) loop_source: bool,
    pub(crate) source_loop_length_ticks: u64,
    pub(crate) length_ticks: u64,
    pub(crate) muted: bool,
    pub(crate) notes: Arc<Vec<MidiNote>>,
    pub(crate) controllers: Arc<Vec<MidiControllerData>>,
    pub(crate) pitch_bends: Arc<Vec<MidiPitchBendData>>,
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

    /// Returns the user-visible name of this MIDI item.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the item's start position in project ticks.
    pub fn start_tick(&self) -> u64 {
        self.start_tick
    }

    /// Returns how far the visible clip start is trimmed into its stored MIDI content.
    pub fn source_offset_ticks(&self) -> u64 {
        self.source_offset_ticks
    }

    /// Returns whether the MIDI source repeats across this item's visible length.
    pub fn is_loop_source(&self) -> bool {
        self.loop_source
    }

    /// Returns the fixed source loop length in MIDI ticks.
    pub fn source_loop_length_ticks(&self) -> u64 {
        self.source_loop_length_ticks
    }

    /// Maps a project tick inside this item to its stored MIDI source tick.
    pub fn source_tick_at_project_tick(&self, project_tick: u64) -> Option<u64> {
        let project_offset = project_tick.checked_sub(self.start_tick)?;
        if project_offset >= self.length_ticks {
            return None;
        }
        let source_tick = self.source_offset_ticks.checked_add(project_offset)?;
        if self.loop_source {
            (self.source_loop_length_ticks > 0).then(|| source_tick % self.source_loop_length_ticks)
        } else {
            Some(source_tick)
        }
    }

    /// Returns each project occurrence of a stored source event visible in this item.
    ///
    /// The iterator is allocation-free. Looped events advance by the preserved source loop
    /// length; a non-looped item yields at most one occurrence.
    pub fn project_ticks_at_content_tick(&self, content_tick: u64) -> impl Iterator<Item = u64> {
        let clip_end = self.start_tick.saturating_add(self.length_ticks);
        let first_tick = if self.loop_source {
            if self.source_loop_length_ticks == 0 {
                None
            } else {
                let loop_length = self.source_loop_length_ticks;
                let source_tick = content_tick % loop_length;
                let source_offset = self.source_offset_ticks % loop_length;
                let phase = if source_tick >= source_offset {
                    source_tick - source_offset
                } else {
                    loop_length - (source_offset - source_tick)
                };
                self.start_tick.checked_add(phase)
            }
        } else {
            self.project_tick_at_content_tick(content_tick)
        };
        let interval = self.loop_source.then_some(self.source_loop_length_ticks);
        let mut next_tick = first_tick;
        std::iter::from_fn(move || {
            let tick = next_tick?;
            if tick < self.start_tick || tick >= clip_end {
                next_tick = None;
                return None;
            }
            next_tick = interval.and_then(|interval| tick.checked_add(interval));
            Some(tick)
        })
    }

    /// Maps a stored event tick to its project position, if it fits the project timeline.
    pub fn project_tick_at_content_tick(&self, content_tick: u64) -> Option<u64> {
        if self.loop_source {
            return self.project_ticks_at_content_tick(content_tick).next();
        }
        let project_tick = i128::from(self.start_tick) + i128::from(content_tick)
            - i128::from(self.source_offset_ticks);
        u64::try_from(project_tick).ok()
    }

    /// Returns the item's duration in project ticks.
    pub fn length_ticks(&self) -> u64 {
        self.length_ticks
    }

    /// Returns whether this MIDI item is muted.
    pub fn is_muted(&self) -> bool {
        self.muted
    }

    /// Returns the item's notes in insertion order.
    pub fn notes(&self) -> &[MidiNote] {
        &self.notes
    }

    /// Returns control-change events sorted by tick, then controller number.
    pub fn controllers(&self) -> &[MidiControllerData] {
        &self.controllers
    }

    /// Returns pitch-bend events sorted by tick.
    pub fn pitch_bends(&self) -> &[MidiPitchBendData] {
        &self.pitch_bends
    }
}
