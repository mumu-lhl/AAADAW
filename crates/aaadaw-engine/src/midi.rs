use aaadaw_core::{NoteId, Project, TimebaseError, Track, TrackId};
use std::fmt;

/// A MIDI event kind, ordered so note-offs precede note-ons at shared block positions.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MidiEventKind {
    NoteOff,
    NoteOn,
}

/// A note event positioned relative to the beginning of an audio block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduledMidiEvent {
    pub sample_offset: usize,
    pub track_id: TrackId,
    pub note_id: NoteId,
    pub pitch: u8,
    pub velocity: u8,
    pub kind: MidiEventKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CompiledMidiEvent {
    absolute_sample: u64,
    track_id: TrackId,
    note_id: NoteId,
    pitch: u8,
    velocity: u8,
    kind: MidiEventKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CompiledMidiNote {
    start_sample: u64,
    end_sample: u64,
    track_id: TrackId,
    note_id: NoteId,
    pitch: u8,
    velocity: u8,
}

/// A sorted MIDI note-event schedule compiled from an immutable project state.
#[derive(Clone, Debug, Default)]
pub struct MidiEventPlan {
    events: Vec<CompiledMidiEvent>,
    notes: Vec<CompiledMidiNote>,
}

/// The project could not be compiled into a MIDI event schedule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MidiScheduleError {
    /// A MIDI item references a missing track.
    MissingTrack { track_id: u64 },
    /// Tick/sample arithmetic exceeds the supported range.
    PositionOutOfRange,
    /// Converting a project tick to a sample failed.
    Timebase(TimebaseError),
    /// The caller-provided output slice cannot hold the requested block events.
    OutputBufferTooSmall { required: usize, available: usize },
}

impl fmt::Display for MidiScheduleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingTrack { track_id } => {
                write!(formatter, "MIDI item references missing track {track_id}")
            }
            Self::PositionOutOfRange => formatter.write_str("MIDI event position is out of range"),
            Self::Timebase(error) => {
                write!(formatter, "MIDI event timebase conversion failed: {error}")
            }
            Self::OutputBufferTooSmall {
                required,
                available,
            } => write!(
                formatter,
                "MIDI output buffer has {available} slots; {required} events are required"
            ),
        }
    }
}

impl std::error::Error for MidiScheduleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Timebase(error) => Some(error),
            _ => None,
        }
    }
}

impl MidiEventPlan {
    /// Compiles note-on/off events from a project. Muted tracks and tracks
    /// excluded by the project's solo state are omitted. Call this off the
    /// audio callback; the returned plan can be queried without allocation.
    pub fn compile(project: &Project) -> Result<Self, MidiScheduleError> {
        let tracks = project.tracks();
        let has_solo = tracks.iter().any(Track::is_solo);
        let mut events = Vec::new();
        let mut notes = Vec::new();

        for item in project.midi_items() {
            let track = tracks
                .iter()
                .find(|track| track.id() == item.track_id())
                .ok_or(MidiScheduleError::MissingTrack {
                    track_id: item.track_id().value(),
                })?;
            if track.is_muted() || (has_solo && !track.is_solo()) {
                continue;
            }

            for note in item.notes() {
                let start_tick = item
                    .start_tick()
                    .checked_add(note.tick())
                    .ok_or(MidiScheduleError::PositionOutOfRange)?;
                let end_tick = start_tick
                    .checked_add(note.duration())
                    .ok_or(MidiScheduleError::PositionOutOfRange)?;
                let start_sample = project
                    .sample_at_tick(start_tick)
                    .map_err(MidiScheduleError::Timebase)?;
                let end_sample = project
                    .sample_at_tick(end_tick)
                    .map_err(MidiScheduleError::Timebase)?;
                notes.push(CompiledMidiNote {
                    start_sample,
                    end_sample,
                    track_id: item.track_id(),
                    note_id: note.id(),
                    pitch: note.pitch(),
                    velocity: note.velocity(),
                });
                events.push(CompiledMidiEvent {
                    absolute_sample: start_sample,
                    track_id: item.track_id(),
                    note_id: note.id(),
                    pitch: note.pitch(),
                    velocity: note.velocity(),
                    kind: MidiEventKind::NoteOn,
                });
                events.push(CompiledMidiEvent {
                    absolute_sample: end_sample,
                    track_id: item.track_id(),
                    note_id: note.id(),
                    pitch: note.pitch(),
                    velocity: 0,
                    kind: MidiEventKind::NoteOff,
                });
            }
        }

        events.sort_unstable_by_key(|event| {
            (
                event.absolute_sample,
                event.kind,
                event.track_id.value(),
                event.pitch,
                event.note_id.value(),
            )
        });
        notes.sort_unstable_by_key(|note| {
            (
                note.start_sample,
                note.track_id.value(),
                note.note_id.value(),
            )
        });
        Ok(Self { events, notes })
    }

    /// Returns the number of compiled note events.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Returns whether the schedule contains no events.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Copies notes sounding at `sample` as note-ons at block offset zero.
    ///
    /// Notes beginning exactly at `sample` are left to `events_for_block`, avoiding duplicate
    /// note-ons when playback starts on the attack. The caller provides preallocated storage.
    pub fn active_notes_at(
        &self,
        sample: u64,
        output: &mut [Option<ScheduledMidiEvent>],
    ) -> Result<usize, MidiScheduleError> {
        let mut required = 0;
        for note in &self.notes {
            if note.start_sample < sample && sample < note.end_sample {
                required += 1;
            }
        }
        if output.len() < required {
            return Err(MidiScheduleError::OutputBufferTooSmall {
                required,
                available: output.len(),
            });
        }
        let mut written = 0;
        for note in &self.notes {
            if note.start_sample < sample && sample < note.end_sample {
                output[written] = Some(ScheduledMidiEvent {
                    sample_offset: 0,
                    track_id: note.track_id,
                    note_id: note.note_id,
                    pitch: note.pitch,
                    velocity: note.velocity,
                    kind: MidiEventKind::NoteOn,
                });
                written += 1;
            }
        }
        Ok(written)
    }

    pub(crate) fn event_count_for_track(&self, track_id: TrackId) -> usize {
        self.events
            .iter()
            .filter(|event| event.track_id == track_id)
            .count()
    }

    /// Copies the events in `[start_sample, start_sample + frame_count)` into
    /// the caller's preallocated slice. On error the output is not modified.
    pub fn events_for_block(
        &self,
        start_sample: u64,
        frame_count: usize,
        output: &mut [Option<ScheduledMidiEvent>],
    ) -> Result<usize, MidiScheduleError> {
        let frame_count =
            u64::try_from(frame_count).map_err(|_| MidiScheduleError::PositionOutOfRange)?;
        let end_sample = start_sample
            .checked_add(frame_count)
            .ok_or(MidiScheduleError::PositionOutOfRange)?;
        let first = self
            .events
            .partition_point(|event| event.absolute_sample < start_sample);
        let end = self
            .events
            .partition_point(|event| event.absolute_sample < end_sample);
        let required = end - first;
        if output.len() < required {
            return Err(MidiScheduleError::OutputBufferTooSmall {
                required,
                available: output.len(),
            });
        }

        for (destination, event) in output.iter_mut().zip(&self.events[first..end]) {
            *destination = Some(ScheduledMidiEvent {
                sample_offset: (event.absolute_sample - start_sample) as usize,
                track_id: event.track_id,
                note_id: event.note_id,
                pitch: event.pitch,
                velocity: event.velocity,
                kind: event.kind,
            });
        }
        Ok(required)
    }
}
