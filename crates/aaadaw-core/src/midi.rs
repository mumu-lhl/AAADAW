use crate::TrackId;
use std::sync::Arc;

/// Number of exact fractional PPQ positions stored within each project tick.
///
/// REAPER's item-snap actions can place MIDI items at values such as 964.8 PPQ.
/// Its swing grid is expressed as an integer percentage of half a grid interval,
/// so 200 subdivisions represent every such snap point exactly.
pub const MIDI_ITEM_SUBTICKS_PER_TICK: u16 = 200;
/// Number of addressable MIDI 1.0 channels in one event.
pub const MIDI_CHANNEL_COUNT: u8 = 16;
pub const MIDI_BUS_COUNT: u8 = 16;

/// An exact MIDI item position measured in project ticks and 1/200-tick units.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MidiItemPosition {
    tick: u64,
    subtick: u8,
}

impl MidiItemPosition {
    /// Creates a MIDI item position. `subtick` must be less than 200.
    pub fn new(tick: u64, subtick: u8) -> Option<Self> {
        (u16::from(subtick) < MIDI_ITEM_SUBTICKS_PER_TICK).then_some(Self { tick, subtick })
    }

    pub(crate) fn from_scaled_subticks(value: i128) -> Option<Self> {
        if value < 0 {
            return None;
        }
        let units = i128::from(MIDI_ITEM_SUBTICKS_PER_TICK);
        let tick = u64::try_from(value / units).ok()?;
        let subtick = u8::try_from(value % units).ok()?;
        Some(Self { tick, subtick })
    }

    pub(crate) fn scaled_subticks(self) -> i128 {
        i128::from(self.tick) * i128::from(MIDI_ITEM_SUBTICKS_PER_TICK) + i128::from(self.subtick)
    }

    /// Reconstructs an exact position from nonnegative 1/200-tick units.
    pub fn from_total_subticks(value: u128) -> Option<Self> {
        let units = u128::from(MIDI_ITEM_SUBTICKS_PER_TICK);
        Some(Self {
            tick: u64::try_from(value / units).ok()?,
            subtick: u8::try_from(value % units).ok()?,
        })
    }

    /// Returns this position as an exact count of 1/200-tick units.
    pub fn total_subticks(self) -> u128 {
        u128::from(self.tick) * u128::from(MIDI_ITEM_SUBTICKS_PER_TICK) + u128::from(self.subtick)
    }

    /// Returns the whole-tick component, flooring fractional PPQ positions.
    pub fn tick(self) -> u64 {
        self.tick
    }

    /// Returns the fractional component in 1/200-tick units.
    pub fn subtick(self) -> u8 {
        self.subtick
    }

    /// Returns this position as a continuous project-tick value.
    pub fn as_ticks_f64(self) -> f64 {
        self.tick as f64 + f64::from(self.subtick) / f64::from(MIDI_ITEM_SUBTICKS_PER_TICK)
    }

    /// Adds an integral tick duration while retaining the fractional position.
    pub fn checked_add_ticks(self, ticks: u64) -> Option<Self> {
        Some(Self {
            tick: self.tick.checked_add(ticks)?,
            subtick: self.subtick,
        })
    }
}

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
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MidiNoteData {
    pub pitch: u8,
    /// Zero-based MIDI channel in the inclusive range `0..16`.
    pub channel: u8,
    pub tick: u64,
    pub duration: u64,
    pub velocity: u8,
    /// Velocity emitted by the paired Note Off event, in the inclusive range `0..=127`.
    pub off_velocity: u8,
}

/// Input data for a MIDI control-change event; `tick` is relative to its item.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MidiControllerData {
    pub controller: u8,
    /// Zero-based MIDI channel in the inclusive range `0..16`.
    pub channel: u8,
    pub tick: u64,
    pub value: u8,
}

/// Input data for a MIDI 14-bit pitch-bend event; `tick` is relative to its item.
/// The neutral pitch-bend value is 8192.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MidiPitchBendData {
    /// Zero-based MIDI channel in the inclusive range `0..16`.
    pub channel: u8,
    pub tick: u64,
    pub value: u16,
}

/// Input data for a MIDI Program Change event; `tick` is relative to its item.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MidiProgramChangeData {
    /// Zero-based MIDI channel in the inclusive range `0..16`.
    pub channel: u8,
    pub tick: u64,
    /// Zero-based patch number in the range `0..=127`.
    pub program: u8,
}

/// Input data for a MIDI Polyphonic Aftertouch event; `tick` is relative to its item.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MidiPolyAftertouchData {
    /// Zero-based MIDI channel in the inclusive range `0..16`.
    pub channel: u8,
    pub tick: u64,
    /// MIDI note number whose pressure is changed.
    pub note: u8,
    /// Polyphonic pressure in the range `0..=127`.
    pub value: u8,
}

/// Input data for a MIDI Channel Pressure event; `tick` is relative to its item.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MidiChannelPressureData {
    /// Zero-based MIDI channel in the inclusive range `0..16`.
    pub channel: u8,
    pub tick: u64,
    /// Channel pressure in the range `0..=127`.
    pub value: u8,
}

/// A non-channel event retained from a Standard MIDI File MIDI item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MidiOpaqueEventData {
    /// Event position relative to the MIDI item's source.
    pub tick: u64,
    /// Source order among opaque events at this position.
    pub order: u32,
    pub kind: MidiOpaqueEventKind,
}

/// The typed event whose same-tick ordering was recorded while importing an SMF.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MidiEventOrderTarget {
    Note(MidiNoteData),
    Controller(MidiControllerData),
    PitchBend(MidiPitchBendData),
    ProgramChange(MidiProgramChangeData),
    PolyAftertouch(MidiPolyAftertouchData),
    ChannelPressure(MidiChannelPressureData),
}

/// Source ordinal for a typed MIDI event imported from a Standard MIDI File.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiEventOrderData {
    pub target: MidiEventOrderTarget,
    pub order: u32,
    /// Source ordinal of a paired Note Off; absent for legacy snapshots and non-note events.
    pub note_off_order: Option<u32>,
}

/// A SysEx or Meta payload that does not map to a channel-event model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MidiOpaqueEventKind {
    SystemExclusive { status: u8, data: Vec<u8> },
    Meta { kind: u8, data: Vec<u8> },
}

impl MidiOpaqueEventKind {
    pub(crate) fn is_valid(&self) -> bool {
        match self {
            Self::SystemExclusive { status, .. } => matches!(status, 0xF0 | 0xF7),
            Self::Meta { .. } => true,
        }
    }

    pub(crate) fn data_len(&self) -> usize {
        match self {
            Self::SystemExclusive { data, .. } | Self::Meta { data, .. } => data.len(),
        }
    }
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

    /// Returns the zero-based MIDI channel in the range `0..16`.
    pub fn channel(&self) -> u8 {
        self.data.channel
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

    /// Returns the velocity emitted by this note's paired Note Off event.
    pub fn off_velocity(&self) -> u8 {
        self.data.off_velocity
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MidiItemTake {
    pub(crate) index: u32,
    pub(crate) name: String,
    pub(crate) source_offset_ticks: i64,
    pub(crate) loop_source: bool,
    pub(crate) source_loop_length_ticks: u64,
    pub(crate) notes: Arc<Vec<MidiNote>>,
    pub(crate) controllers: Arc<Vec<MidiControllerData>>,
    pub(crate) pitch_bends: Arc<Vec<MidiPitchBendData>>,
    pub(crate) program_changes: Arc<Vec<MidiProgramChangeData>>,
    pub(crate) poly_aftertouches: Arc<Vec<MidiPolyAftertouchData>>,
    pub(crate) channel_pressures: Arc<Vec<MidiChannelPressureData>>,
    pub(crate) opaque_events: Arc<Vec<MidiOpaqueEventData>>,
    pub(crate) event_order: Arc<Vec<MidiEventOrderData>>,
}

/// Take metadata needed by item-level property controls without copying MIDI events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MidiItemTakeProperties {
    pub index: u32,
    pub name: String,
    pub source_offset_ticks: i64,
    pub loop_source: bool,
    pub source_loop_length_ticks: u64,
    pub event_end_ticks: u64,
}

/// A MIDI clip positioned on a track.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MidiItem {
    pub(crate) id: ItemId,
    pub(crate) track_id: TrackId,
    pub(crate) name: String,
    pub(crate) active_take_index: u32,
    pub(crate) other_takes: Vec<MidiItemTake>,
    pub(crate) start_tick: u64,
    pub(crate) start_tick_subtick: u8,
    pub(crate) source_offset_ticks: i64,
    pub(crate) loop_source: bool,
    pub(crate) source_loop_length_ticks: u64,
    pub(crate) length_ticks: u64,
    pub(crate) muted: bool,
    pub(crate) notes: Arc<Vec<MidiNote>>,
    pub(crate) controllers: Arc<Vec<MidiControllerData>>,
    pub(crate) pitch_bends: Arc<Vec<MidiPitchBendData>>,
    pub(crate) program_changes: Arc<Vec<MidiProgramChangeData>>,
    pub(crate) poly_aftertouches: Arc<Vec<MidiPolyAftertouchData>>,
    pub(crate) channel_pressures: Arc<Vec<MidiChannelPressureData>>,
    pub(crate) opaque_events: Arc<Vec<MidiOpaqueEventData>>,
    pub(crate) event_order: Arc<Vec<MidiEventOrderData>>,
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

    /// Returns the zero-based index of the active Take.
    pub fn active_take_index(&self) -> u32 {
        self.active_take_index
    }

    /// Returns the number of Takes stored in this item.
    pub fn take_count(&self) -> usize {
        self.other_takes.len() + 1
    }

    /// Returns the take indices and names without copying any take events.
    pub fn take_names(&self) -> Vec<(u32, String)> {
        let mut takes = self
            .other_takes
            .iter()
            .map(|take| (take.index, take.name.clone()))
            .collect::<Vec<_>>();
        takes.push((self.active_take_index, self.name.clone()));
        takes.sort_unstable_by_key(|(index, _)| *index);
        takes
    }

    /// Returns one take's property metadata without copying its event collections.
    pub fn take_properties(&self, index: u32) -> Option<MidiItemTakeProperties> {
        if index == self.active_take_index {
            return Some(MidiItemTakeProperties {
                index,
                name: self.name.clone(),
                source_offset_ticks: self.source_offset_ticks,
                loop_source: self.loop_source,
                source_loop_length_ticks: self.source_loop_length_ticks,
                event_end_ticks: midi_take_event_end(
                    &self.notes,
                    &self.controllers,
                    &self.pitch_bends,
                ),
            });
        }

        self.other_takes
            .iter()
            .find(|take| take.index == index)
            .map(|take| MidiItemTakeProperties {
                index: take.index,
                name: take.name.clone(),
                source_offset_ticks: take.source_offset_ticks,
                loop_source: take.loop_source,
                source_loop_length_ticks: take.source_loop_length_ticks,
                event_end_ticks: midi_take_event_end(
                    &take.notes,
                    &take.controllers,
                    &take.pitch_bends,
                ),
            })
    }

    pub(crate) fn switch_active_take(&mut self, index: u32) -> bool {
        if index == self.active_take_index {
            return false;
        }
        let Some(position) = self.other_takes.iter().position(|take| take.index == index) else {
            return false;
        };
        let previous = MidiItemTake {
            index: self.active_take_index,
            name: self.name.clone(),
            source_offset_ticks: self.source_offset_ticks,
            loop_source: self.loop_source,
            source_loop_length_ticks: self.source_loop_length_ticks,
            notes: Arc::clone(&self.notes),
            controllers: Arc::clone(&self.controllers),
            pitch_bends: Arc::clone(&self.pitch_bends),
            program_changes: Arc::clone(&self.program_changes),
            poly_aftertouches: Arc::clone(&self.poly_aftertouches),
            channel_pressures: Arc::clone(&self.channel_pressures),
            opaque_events: Arc::clone(&self.opaque_events),
            event_order: Arc::clone(&self.event_order),
        };
        let next = self.other_takes.remove(position);
        self.active_take_index = next.index;
        self.name = next.name;
        self.source_offset_ticks = next.source_offset_ticks;
        self.loop_source = next.loop_source;
        self.source_loop_length_ticks = next.source_loop_length_ticks;
        self.notes = next.notes;
        self.controllers = next.controllers;
        self.pitch_bends = next.pitch_bends;
        self.program_changes = next.program_changes;
        self.poly_aftertouches = next.poly_aftertouches;
        self.channel_pressures = next.channel_pressures;
        self.opaque_events = next.opaque_events;
        self.event_order = next.event_order;
        self.other_takes.push(previous);
        self.other_takes.sort_unstable_by_key(|take| take.index);
        true
    }

    /// Removes the active Take, selecting the preceding Take when possible and compacting indices.
    pub(crate) fn delete_active_take(&mut self) -> bool {
        if self.take_count() <= 1 {
            return false;
        }
        let removed_index = self.active_take_index;
        let next_index = if removed_index == 0 {
            1
        } else {
            removed_index - 1
        };
        if !self.switch_active_take(next_index) {
            return false;
        }
        self.other_takes.retain(|take| take.index != removed_index);
        for take in &mut self.other_takes {
            if take.index > removed_index {
                take.index -= 1;
            }
        }
        if self.active_take_index > removed_index {
            self.active_take_index -= 1;
        }
        true
    }

    /// Moves the active Take to list index zero while preserving every Take's source and events.
    pub(crate) fn move_active_take_to_top(&mut self) -> bool {
        if self.active_take_index == 0 {
            return false;
        }
        let active_index = self.active_take_index;
        for take in &mut self.other_takes {
            if take.index < active_index {
                take.index += 1;
            }
        }
        self.active_take_index = 0;
        self.other_takes.sort_unstable_by_key(|take| take.index);
        true
    }

    /// Removes inactive Takes while preserving the active Take and its source events.
    pub(crate) fn crop_to_active_take(&mut self) -> bool {
        if self.other_takes.is_empty() {
            return false;
        }
        self.other_takes.clear();
        self.active_take_index = 0;
        true
    }

    /// Returns the item Takes in their original order, including the active Take.
    pub fn take_snapshots(&self) -> Vec<crate::MidiItemTakeSnapshot> {
        let mut takes = Vec::with_capacity(self.take_count());
        takes.push(crate::MidiItemTakeSnapshot {
            index: self.active_take_index,
            name: self.name.clone(),
            source_offset_ticks: self.source_offset_ticks,
            loop_source: self.loop_source,
            source_loop_length_ticks: self.source_loop_length_ticks,
            notes: self
                .notes
                .iter()
                .map(|note| crate::MidiNoteSnapshot {
                    id: note.id.value(),
                    data: note.data,
                })
                .collect(),
            controllers: self.controllers.as_ref().clone(),
            pitch_bends: self.pitch_bends.as_ref().clone(),
            program_changes: self.program_changes.as_ref().clone(),
            poly_aftertouches: self.poly_aftertouches.as_ref().clone(),
            channel_pressures: self.channel_pressures.as_ref().clone(),
            opaque_events: self.opaque_events.as_ref().clone(),
            event_order: self.event_order.as_ref().clone(),
        });
        takes.extend(self.other_takes.iter().map(|take| {
            crate::MidiItemTakeSnapshot {
                index: take.index,
                name: take.name.clone(),
                source_offset_ticks: take.source_offset_ticks,
                loop_source: take.loop_source,
                source_loop_length_ticks: take.source_loop_length_ticks,
                notes: take
                    .notes
                    .iter()
                    .map(|note| crate::MidiNoteSnapshot {
                        id: note.id.value(),
                        data: note.data,
                    })
                    .collect(),
                controllers: take.controllers.as_ref().clone(),
                pitch_bends: take.pitch_bends.as_ref().clone(),
                program_changes: take.program_changes.as_ref().clone(),
                poly_aftertouches: take.poly_aftertouches.as_ref().clone(),
                channel_pressures: take.channel_pressures.as_ref().clone(),
                opaque_events: take.opaque_events.as_ref().clone(),
                event_order: take.event_order.as_ref().clone(),
            }
        }));
        takes.sort_unstable_by_key(|take| take.index);
        takes
    }

    /// Returns the item's start position in project ticks.
    pub fn start_tick(&self) -> u64 {
        self.start_tick
    }

    /// Returns the exact project position, including a fractional PPQ component.
    pub fn start_position(&self) -> MidiItemPosition {
        // Project validation guarantees this invariant for every constructed item.
        MidiItemPosition {
            tick: self.start_tick,
            subtick: self.start_tick_subtick,
        }
    }

    /// Returns how far the visible clip start is trimmed into its stored MIDI content.
    pub fn source_offset_ticks(&self) -> u64 {
        u64::try_from(self.source_offset_ticks).unwrap_or(0)
    }

    /// Returns the signed source-time offset of the visible item start.
    /// Negative values place the item start before the source's first event.
    pub fn signed_source_offset_ticks(&self) -> i64 {
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
        let project_subticks = u128::from(project_tick) * u128::from(MIDI_ITEM_SUBTICKS_PER_TICK);
        let item_start_subticks = self.start_position().total_subticks();
        let project_offset_subticks = project_subticks.checked_sub(item_start_subticks)?;
        let item_length_subticks =
            u128::from(self.length_ticks) * u128::from(MIDI_ITEM_SUBTICKS_PER_TICK);
        if project_offset_subticks >= item_length_subticks {
            return None;
        }
        let source_subticks = i128::from(self.source_offset_ticks)
            .checked_mul(i128::from(MIDI_ITEM_SUBTICKS_PER_TICK))?
            .checked_add(i128::try_from(project_offset_subticks).ok()?)?;
        if source_subticks < 0 && !self.loop_source {
            return None;
        }
        if self.loop_source {
            (self.source_loop_length_ticks > 0).then(|| {
                let loop_subticks = i128::from(self.source_loop_length_ticks)
                    * i128::from(MIDI_ITEM_SUBTICKS_PER_TICK);
                source_subticks
                    .rem_euclid(loop_subticks)
                    .div_euclid(i128::from(MIDI_ITEM_SUBTICKS_PER_TICK)) as u64
            })
        } else {
            u64::try_from(source_subticks.div_euclid(i128::from(MIDI_ITEM_SUBTICKS_PER_TICK))).ok()
        }
    }

    /// Returns each project occurrence of a stored source event visible in this item.
    ///
    /// The iterator is allocation-free. Looped events advance by the preserved source loop
    /// length; a non-looped item yields at most one occurrence.
    pub fn project_ticks_at_content_tick(&self, content_tick: u64) -> impl Iterator<Item = u64> {
        self.project_positions_at_content_tick(content_tick)
            .map(MidiItemPosition::tick)
    }

    /// Returns each exact project occurrence of a stored source event visible in this item.
    ///
    /// The iterator is allocation-free, including for looped sources. Positions preserve the
    /// fractional PPQ component of the item start.
    pub fn project_positions_at_content_tick(
        &self,
        content_tick: u64,
    ) -> impl Iterator<Item = MidiItemPosition> {
        let item_start = self.start_position();
        let clip_end = item_start.checked_add_ticks(self.length_ticks);
        let first_position = if self.loop_source {
            if self.source_loop_length_ticks == 0 {
                None
            } else {
                let loop_length = self.source_loop_length_ticks;
                let source_tick = content_tick % loop_length;
                let source_offset =
                    i128::from(self.source_offset_ticks).rem_euclid(i128::from(loop_length)) as u64;
                let phase = if source_tick >= source_offset {
                    source_tick - source_offset
                } else {
                    loop_length - (source_offset - source_tick)
                };
                item_start.checked_add_ticks(phase)
            }
        } else {
            let source_delta = i128::from(content_tick) - i128::from(self.source_offset_ticks);
            MidiItemPosition::from_scaled_subticks(
                item_start.scaled_subticks()
                    + source_delta * i128::from(MIDI_ITEM_SUBTICKS_PER_TICK),
            )
        };
        let interval = self.loop_source.then_some(self.source_loop_length_ticks);
        let mut next_position = first_position;
        std::iter::from_fn(move || {
            let position = next_position?;
            if position < item_start || clip_end.is_none_or(|end| position >= end) {
                next_position = None;
                return None;
            }
            next_position = interval.and_then(|interval| position.checked_add_ticks(interval));
            Some(position)
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

    /// Maps a stored event tick to its exact project position, if it fits the timeline.
    pub fn project_position_at_content_tick(&self, content_tick: u64) -> Option<MidiItemPosition> {
        self.project_positions_at_content_tick(content_tick).next()
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

    /// Returns Program Change events sorted by tick, then channel.
    pub fn program_changes(&self) -> &[MidiProgramChangeData] {
        &self.program_changes
    }

    /// Returns Polyphonic Aftertouch events sorted by tick, then channel and note.
    pub fn poly_aftertouches(&self) -> &[MidiPolyAftertouchData] {
        &self.poly_aftertouches
    }

    /// Returns Channel Pressure events sorted by tick, then channel.
    pub fn channel_pressures(&self) -> &[MidiChannelPressureData] {
        &self.channel_pressures
    }

    /// Returns retained text, marker, SysEx, and other opaque source events.
    pub fn opaque_events(&self) -> &[MidiOpaqueEventData] {
        &self.opaque_events
    }

    /// Returns imported source ordinals for typed events used to reproduce same-tick order.
    pub fn event_order(&self) -> &[MidiEventOrderData] {
        &self.event_order
    }
}

fn midi_take_event_end(
    notes: &[MidiNote],
    controllers: &[MidiControllerData],
    pitch_bends: &[MidiPitchBendData],
) -> u64 {
    notes
        .iter()
        .map(|note| note.tick().saturating_add(note.duration()))
        .chain(controllers.iter().map(|event| event.tick.saturating_add(1)))
        .chain(pitch_bends.iter().map(|event| event.tick.saturating_add(1)))
        .max()
        .unwrap_or(0)
}
