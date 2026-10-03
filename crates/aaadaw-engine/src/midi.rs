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

#[derive(Clone, Debug, Default)]
struct MidiNoteIntervalNode {
    center_sample: u64,
    by_start: Vec<usize>,
    by_end: Vec<usize>,
    left: Option<usize>,
    right: Option<usize>,
}

#[derive(Clone, Copy)]
struct ActiveNoteQuery {
    sample: u64,
}

/// A sorted MIDI note-event schedule compiled from an immutable project state.
#[derive(Clone, Debug, Default)]
pub struct MidiEventPlan {
    events: Vec<CompiledMidiEvent>,
    notes: Vec<CompiledMidiNote>,
    note_interval_nodes: Vec<MidiNoteIntervalNode>,
    note_interval_root: Option<usize>,
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
        let (note_interval_nodes, note_interval_root) = compile_note_interval_tree(&notes);
        Ok(Self {
            events,
            notes,
            note_interval_nodes,
            note_interval_root,
        })
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
    /// note-ons when playback starts on the attack. The caller provides preallocated storage;
    /// the prebuilt centered interval tree queries active notes in O(log n + active notes).
    pub fn active_notes_at(
        &self,
        sample: u64,
        output: &mut [Option<ScheduledMidiEvent>],
    ) -> Result<usize, MidiScheduleError> {
        let required = self.count_active_notes(self.note_interval_root, sample);
        if output.len() < required {
            return Err(MidiScheduleError::OutputBufferTooSmall {
                required,
                available: output.len(),
            });
        }
        let mut written = 0;
        self.write_active_notes(
            self.note_interval_root,
            ActiveNoteQuery { sample },
            output,
            &mut written,
        );
        Ok(written)
    }

    fn count_active_notes(&self, node_index: Option<usize>, sample: u64) -> usize {
        let Some(node_index) = node_index else {
            return 0;
        };
        let node = &self.note_interval_nodes[node_index];
        if sample <= node.center_sample {
            let active_here = node
                .by_start
                .iter()
                .take_while(|note_index| self.notes[**note_index].start_sample < sample)
                .count();
            active_here
                + if sample < node.center_sample {
                    self.count_active_notes(node.left, sample)
                } else {
                    0
                }
        } else {
            let active_here = node
                .by_end
                .iter()
                .take_while(|note_index| self.notes[**note_index].end_sample > sample)
                .count();
            active_here + self.count_active_notes(node.right, sample)
        }
    }

    fn write_active_notes(
        &self,
        node_index: Option<usize>,
        query: ActiveNoteQuery,
        output: &mut [Option<ScheduledMidiEvent>],
        written: &mut usize,
    ) {
        let Some(node_index) = node_index else {
            return;
        };
        let node = &self.note_interval_nodes[node_index];
        let (matching_notes, next_node) = if query.sample <= node.center_sample {
            let matching_count = node
                .by_start
                .iter()
                .take_while(|note_index| self.notes[**note_index].start_sample < query.sample)
                .count();
            (
                &node.by_start[..matching_count],
                if query.sample < node.center_sample {
                    node.left
                } else {
                    None
                },
            )
        } else {
            let matching_count = node
                .by_end
                .iter()
                .take_while(|note_index| self.notes[**note_index].end_sample > query.sample)
                .count();
            (&node.by_end[..matching_count], node.right)
        };
        for note_index in matching_notes {
            let note = self.notes[*note_index];
            output[*written] = Some(ScheduledMidiEvent {
                sample_offset: 0,
                track_id: note.track_id,
                note_id: note.note_id,
                pitch: note.pitch,
                velocity: note.velocity,
                kind: MidiEventKind::NoteOn,
            });
            *written += 1;
        }
        self.write_active_notes(next_node, query, output, written);
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

fn compile_note_interval_tree(
    notes: &[CompiledMidiNote],
) -> (Vec<MidiNoteIntervalNode>, Option<usize>) {
    fn build(
        notes: &[CompiledMidiNote],
        note_indices: &[usize],
        nodes: &mut Vec<MidiNoteIntervalNode>,
    ) -> Option<usize> {
        if note_indices.is_empty() {
            return None;
        }
        let center_sample = notes[note_indices[note_indices.len() / 2]].start_sample;
        let node_index = nodes.len();
        nodes.push(MidiNoteIntervalNode::default());
        let mut left_notes = Vec::new();
        let mut right_notes = Vec::new();
        let mut by_start = Vec::new();
        for note_index in note_indices {
            let note = notes[*note_index];
            if note.end_sample <= center_sample {
                left_notes.push(*note_index);
            } else if note.start_sample > center_sample {
                right_notes.push(*note_index);
            } else {
                by_start.push(*note_index);
            }
        }
        let mut by_end = by_start.clone();
        by_end.sort_unstable_by_key(|note_index| std::cmp::Reverse(notes[*note_index].end_sample));
        let left = build(notes, &left_notes, nodes);
        let right = build(notes, &right_notes, nodes);
        nodes[node_index] = MidiNoteIntervalNode {
            center_sample,
            by_start,
            by_end,
            left,
            right,
        };
        Some(node_index)
    }

    let note_indices: Vec<_> = notes
        .iter()
        .enumerate()
        .filter_map(|(index, note)| (note.start_sample < note.end_sample).then_some(index))
        .collect();
    let mut nodes = Vec::new();
    let root = build(notes, &note_indices, &mut nodes);
    (nodes, root)
}
