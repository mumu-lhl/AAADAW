use aaadaw_core::{NoteId, Project, TimebaseError, Track, TrackId};
use std::collections::HashMap;
use std::fmt;

/// A MIDI event kind. Shared-sample ordering also accounts for sustain-on/off semantics.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MidiEventKind {
    ControllerChange,
    PitchBend,
    NoteOff,
    NoteOn,
}

/// A note event positioned relative to the beginning of an audio block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduledMidiEvent {
    pub sample_offset: usize,
    pub track_id: TrackId,
    pub note_id: Option<NoteId>,
    pub pitch: u8,
    pub velocity: u8,
    pub controller: Option<u8>,
    pub pitch_bend: Option<u16>,
    pub kind: MidiEventKind,
}

impl ScheduledMidiEvent {
    pub(crate) fn sort_priority(self) -> u8 {
        midi_event_priority(self.kind, self.controller, self.velocity)
    }
}

/// One decoded channel-voice message from a live MIDI input stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiInputMessage {
    status: u8,
    data1: u8,
    data2: u8,
    note_id: Option<NoteId>,
}

impl MidiInputMessage {
    /// Returns the paired note identifier for note-on and note-off messages.
    pub fn note_id(self) -> Option<NoteId> {
        self.note_id
    }

    /// Converts supported note, controller, and pitch-bend messages for one instrument track.
    pub fn scheduled_event(self, track_id: TrackId) -> Option<ScheduledMidiEvent> {
        let channel_message = self.status & 0xF0;
        let (kind, pitch, velocity, controller, pitch_bend) = match channel_message {
            0x80 => (MidiEventKind::NoteOff, self.data1, self.data2, None, None),
            0x90 if self.data2 == 0 => (MidiEventKind::NoteOff, self.data1, 0, None, None),
            0x90 => (MidiEventKind::NoteOn, self.data1, self.data2, None, None),
            0xB0 => (
                MidiEventKind::ControllerChange,
                self.data1,
                self.data2,
                Some(self.data1),
                None,
            ),
            0xE0 => (
                MidiEventKind::PitchBend,
                0,
                0,
                None,
                Some(u16::from(self.data1) | (u16::from(self.data2) << 7)),
            ),
            _ => return None,
        };
        Some(ScheduledMidiEvent {
            sample_offset: 0,
            track_id,
            pitch,
            velocity,
            controller,
            pitch_bend,
            note_id: matches!(kind, MidiEventKind::NoteOn | MidiEventKind::NoteOff)
                .then_some(self.note_id)
                .flatten(),
            kind,
        })
    }
}

/// Decodes channel-voice MIDI messages, including running status across packet boundaries.
#[derive(Clone, Debug)]
pub struct MidiInputDecoder {
    running_status: Option<u8>,
    data: [u8; 2],
    data_len: usize,
    active_notes: HashMap<(u8, u8), Vec<NoteId>>,
    next_note_id: u64,
}

impl Default for MidiInputDecoder {
    fn default() -> Self {
        Self {
            running_status: None,
            data: [0; 2],
            data_len: 0,
            active_notes: HashMap::new(),
            next_note_id: 1 << 30,
        }
    }
}

impl MidiInputDecoder {
    /// Appends complete note, controller, and pitch-bend messages found in `bytes`.
    pub fn push_bytes(&mut self, bytes: &[u8], output: &mut Vec<MidiInputMessage>) {
        for byte in bytes.iter().copied() {
            if byte >= 0xF8 {
                continue;
            }
            if byte & 0x80 != 0 {
                self.data_len = 0;
                self.running_status = (byte < 0xF0).then_some(byte);
                continue;
            }
            let Some(status) = self.running_status else {
                continue;
            };
            let expected = match status & 0xF0 {
                0xC0 | 0xD0 => 1,
                0x80 | 0x90 | 0xA0 | 0xB0 | 0xE0 => 2,
                _ => {
                    self.running_status = None;
                    self.data_len = 0;
                    continue;
                }
            };
            self.data[self.data_len] = byte;
            self.data_len += 1;
            if self.data_len == expected {
                let data1 = self.data[0];
                let data2 = if expected == 2 { self.data[1] } else { 0 };
                let note_id = match status & 0xF0 {
                    0x90 if data2 > 0 => {
                        let note_id = self.next_note_id();
                        self.active_notes
                            .entry((status & 0x0F, data1))
                            .or_default()
                            .push(note_id);
                        Some(note_id)
                    }
                    0x80 | 0x90 => {
                        let key = (status & 0x0F, data1);
                        let note_id = if let Some(note_id) =
                            self.active_notes.get_mut(&key).and_then(Vec::pop)
                        {
                            note_id
                        } else {
                            self.next_note_id()
                        };
                        Some(note_id)
                    }
                    _ => None,
                };
                output.push(MidiInputMessage {
                    status,
                    data1,
                    data2,
                    note_id,
                });
                self.data_len = 0;
            }
        }
    }

    fn next_note_id(&mut self) -> NoteId {
        let value = self.next_note_id.clamp(1 << 30, i32::MAX as u64);
        self.next_note_id = if value == i32::MAX as u64 {
            1 << 30
        } else {
            value + 1
        };
        NoteId::from_value(value).expect("generated live MIDI note identifiers are nonzero")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CompiledMidiEvent {
    absolute_sample: u64,
    track_id: TrackId,
    note_id: Option<NoteId>,
    pitch: u8,
    velocity: u8,
    controller: Option<u8>,
    pitch_bend: Option<u16>,
    kind: MidiEventKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CompiledControllerValue {
    absolute_sample: u64,
    value: u8,
}

#[derive(Clone, Debug)]
struct ControllerTimeline {
    track_id: TrackId,
    controller: u8,
    values: Vec<CompiledControllerValue>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CompiledPitchBendValue {
    absolute_sample: u64,
    value: u16,
}

#[derive(Clone, Debug)]
struct PitchBendTimeline {
    track_id: TrackId,
    values: Vec<CompiledPitchBendValue>,
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
    controller_timelines: Vec<ControllerTimeline>,
    pitch_bend_timelines: Vec<PitchBendTimeline>,
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
        let mut controller_timelines = Vec::<ControllerTimeline>::new();
        let mut pitch_bend_timelines = Vec::<PitchBendTimeline>::new();

        let mut reachable_solo = std::collections::HashMap::new();
        if has_solo {
            for track in tracks {
                let mut pending: Vec<_> = track
                    .output_track()
                    .filter(|_| track.main_send_enabled())
                    .into_iter()
                    .chain(
                        track
                            .sends()
                            .iter()
                            .filter(|send| !send.parameters().muted)
                            .map(|send| send.destination()),
                    )
                    .collect();
                let mut visited = std::collections::HashSet::new();
                let mut routed_to_solo = false;
                while let Some(target_id) = pending.pop() {
                    if !visited.insert(target_id) {
                        continue;
                    }
                    let Some(target) = tracks.iter().find(|candidate| candidate.id() == target_id)
                    else {
                        continue;
                    };
                    if target.is_solo() {
                        routed_to_solo = true;
                        break;
                    }
                    pending.extend(
                        target
                            .output_track()
                            .filter(|_| target.main_send_enabled())
                            .into_iter()
                            .chain(
                                target
                                    .sends()
                                    .iter()
                                    .filter(|send| !send.parameters().muted)
                                    .map(|send| send.destination()),
                            ),
                    );
                }
                reachable_solo.insert(track.id(), routed_to_solo);
            }
        }
        for item in project.midi_items() {
            let track = tracks
                .iter()
                .find(|track| track.id() == item.track_id())
                .ok_or(MidiScheduleError::MissingTrack {
                    track_id: item.track_id().value(),
                })?;
            let routed_to_solo = reachable_solo.get(&track.id()).copied().unwrap_or(false);
            if track.is_muted() || (has_solo && !track.is_solo() && !routed_to_solo) {
                continue;
            }

            let clip_end_tick = item
                .start_tick()
                .checked_add(item.length_ticks())
                .ok_or(MidiScheduleError::PositionOutOfRange)?;
            for note in item.notes() {
                let Some(start_tick) = item.project_tick_at_content_tick(note.tick()) else {
                    continue;
                };
                if start_tick < item.start_tick() || start_tick >= clip_end_tick {
                    continue;
                }
                let end_tick = start_tick
                    .checked_add(note.duration())
                    .ok_or(MidiScheduleError::PositionOutOfRange)?
                    .min(clip_end_tick);
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
                    note_id: Some(note.id()),
                    pitch: note.pitch(),
                    velocity: note.velocity(),
                    controller: None,
                    pitch_bend: None,
                    kind: MidiEventKind::NoteOn,
                });
                events.push(CompiledMidiEvent {
                    absolute_sample: end_sample,
                    track_id: item.track_id(),
                    note_id: Some(note.id()),
                    pitch: note.pitch(),
                    velocity: 0,
                    controller: None,
                    pitch_bend: None,
                    kind: MidiEventKind::NoteOff,
                });
            }
            for controller in item.controllers() {
                let Some(absolute_tick) = item.project_tick_at_content_tick(controller.tick) else {
                    continue;
                };
                if absolute_tick < item.start_tick() || absolute_tick >= clip_end_tick {
                    continue;
                }
                let absolute_sample = project
                    .sample_at_tick(absolute_tick)
                    .map_err(MidiScheduleError::Timebase)?;
                let timeline = controller_timelines.iter_mut().find(|timeline| {
                    timeline.track_id == item.track_id()
                        && timeline.controller == controller.controller
                });
                let timeline = if let Some(timeline) = timeline {
                    timeline
                } else {
                    controller_timelines.push(ControllerTimeline {
                        track_id: item.track_id(),
                        controller: controller.controller,
                        values: Vec::new(),
                    });
                    controller_timelines
                        .last_mut()
                        .expect("controller timeline was just inserted")
                };
                timeline.values.push(CompiledControllerValue {
                    absolute_sample,
                    value: controller.value,
                });
                events.push(CompiledMidiEvent {
                    absolute_sample,
                    track_id: item.track_id(),
                    note_id: None,
                    pitch: controller.controller,
                    velocity: controller.value,
                    controller: Some(controller.controller),
                    pitch_bend: None,
                    kind: MidiEventKind::ControllerChange,
                });
            }
            for bend in item.pitch_bends() {
                let Some(absolute_tick) = item.project_tick_at_content_tick(bend.tick) else {
                    continue;
                };
                if absolute_tick < item.start_tick() || absolute_tick >= clip_end_tick {
                    continue;
                }
                let absolute_sample = project
                    .sample_at_tick(absolute_tick)
                    .map_err(MidiScheduleError::Timebase)?;
                let timeline = pitch_bend_timelines
                    .iter_mut()
                    .find(|timeline| timeline.track_id == item.track_id());
                let timeline = if let Some(timeline) = timeline {
                    timeline
                } else {
                    pitch_bend_timelines.push(PitchBendTimeline {
                        track_id: item.track_id(),
                        values: Vec::new(),
                    });
                    pitch_bend_timelines
                        .last_mut()
                        .expect("pitch-bend timeline was just inserted")
                };
                timeline.values.push(CompiledPitchBendValue {
                    absolute_sample,
                    value: bend.value,
                });
                events.push(CompiledMidiEvent {
                    absolute_sample,
                    track_id: item.track_id(),
                    note_id: None,
                    pitch: 0,
                    velocity: 0,
                    controller: None,
                    pitch_bend: Some(bend.value),
                    kind: MidiEventKind::PitchBend,
                });
            }
        }

        events.sort_by_key(|event| {
            (
                event.absolute_sample,
                midi_event_priority(event.kind, event.controller, event.velocity),
                event.track_id.value(),
                event.pitch,
                event.note_id.map_or(0, NoteId::value),
                event.pitch_bend.unwrap_or(0),
            )
        });
        for timeline in &mut controller_timelines {
            timeline.values.sort_by_key(|value| value.absolute_sample);
        }
        for timeline in &mut pitch_bend_timelines {
            timeline.values.sort_by_key(|value| value.absolute_sample);
        }
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
            controller_timelines,
            pitch_bend_timelines,
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

    /// Copies the last controller value before `sample` as a block-offset-zero event per track
    /// and controller. Controller timelines are compiled and sorted off the audio callback.
    pub fn active_controllers_at(
        &self,
        sample: u64,
        output: &mut [Option<ScheduledMidiEvent>],
    ) -> Result<usize, MidiScheduleError> {
        let active = self
            .controller_timelines
            .iter()
            .filter(|timeline| {
                timeline
                    .values
                    .partition_point(|value| value.absolute_sample < sample)
                    > 0
            })
            .count();
        if output.len() < active {
            return Err(MidiScheduleError::OutputBufferTooSmall {
                required: active,
                available: output.len(),
            });
        }
        let mut written = 0;
        for timeline in &self.controller_timelines {
            let end = timeline
                .values
                .partition_point(|value| value.absolute_sample < sample);
            if let Some(value) = end.checked_sub(1).map(|index| timeline.values[index]) {
                output[written] = Some(ScheduledMidiEvent {
                    sample_offset: 0,
                    track_id: timeline.track_id,
                    note_id: None,
                    pitch: timeline.controller,
                    velocity: value.value,
                    controller: Some(timeline.controller),
                    pitch_bend: None,
                    kind: MidiEventKind::ControllerChange,
                });
                written += 1;
            }
        }
        Ok(written)
    }

    /// Copies the pitch-bend state at `sample`, once per track, for seek chase.
    pub fn active_pitch_bends_at(
        &self,
        sample: u64,
        output: &mut [Option<ScheduledMidiEvent>],
    ) -> Result<usize, MidiScheduleError> {
        let active = self
            .pitch_bend_timelines
            .iter()
            .filter(|timeline| {
                let before = timeline
                    .values
                    .partition_point(|value| value.absolute_sample < sample);
                let through = timeline
                    .values
                    .partition_point(|value| value.absolute_sample <= sample);
                before == through
            })
            .count();
        if output.len() < active {
            return Err(MidiScheduleError::OutputBufferTooSmall {
                required: active,
                available: output.len(),
            });
        }
        let mut written = 0;
        for timeline in &self.pitch_bend_timelines {
            let end = timeline
                .values
                .partition_point(|value| value.absolute_sample < sample);
            let through = timeline
                .values
                .partition_point(|value| value.absolute_sample <= sample);
            if through != end {
                // The block event at this exact sample will apply the state once.
                continue;
            }
            let value = end
                .checked_sub(1)
                .map_or(8192, |index| timeline.values[index].value);
            output[written] = Some(ScheduledMidiEvent {
                sample_offset: 0,
                track_id: timeline.track_id,
                note_id: None,
                pitch: 0,
                velocity: 0,
                controller: None,
                pitch_bend: Some(value),
                kind: MidiEventKind::PitchBend,
            });
            written += 1;
        }
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
                note_id: Some(note.note_id),
                pitch: note.pitch,
                velocity: note.velocity,
                controller: None,
                pitch_bend: None,
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

    pub(crate) fn midi_control_event_count_for_track(&self, track_id: TrackId) -> usize {
        self.events
            .iter()
            .filter(|event| {
                event.track_id == track_id
                    && matches!(
                        event.kind,
                        MidiEventKind::ControllerChange | MidiEventKind::PitchBend
                    )
            })
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
                controller: event.controller,
                pitch_bend: event.pitch_bend,
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

fn midi_event_priority(kind: MidiEventKind, controller: Option<u8>, value: u8) -> u8 {
    match kind {
        MidiEventKind::PitchBend => 0,
        MidiEventKind::ControllerChange if controller == Some(64) && value < 64 => 2,
        MidiEventKind::ControllerChange => 0,
        MidiEventKind::NoteOff => 1,
        MidiEventKind::NoteOn => 3,
    }
}

#[cfg(test)]
mod input_tests {
    use super::{MidiEventKind, MidiInputDecoder};
    use aaadaw_core::TrackId;

    #[test]
    fn live_midi_decoder_handles_running_status_and_packet_boundaries() {
        let mut decoder = MidiInputDecoder::default();
        let mut messages = Vec::new();
        decoder.push_bytes(&[0x92, 60], &mut messages);
        assert!(messages.is_empty());
        decoder.push_bytes(&[100, 60, 0, 0xF8, 64, 0, 67, 90], &mut messages);

        assert_eq!(messages.len(), 4);
        let track_id = TrackId::from_value(7).unwrap();
        let first = messages[0].scheduled_event(track_id).unwrap();
        assert_eq!(first.kind, MidiEventKind::NoteOn);
        assert_eq!((first.pitch, first.velocity), (60, 100));
        let second = messages[1].scheduled_event(track_id).unwrap();
        assert_eq!(second.kind, MidiEventKind::NoteOff);
        assert_eq!((second.pitch, second.velocity), (60, 0));
        let third = messages[3].scheduled_event(track_id).unwrap();
        assert_eq!(third.kind, MidiEventKind::NoteOn);
        assert_eq!((third.pitch, third.velocity), (67, 90));
        let note_on = messages[0].scheduled_event(track_id).unwrap();
        assert_eq!(note_on.note_id, second.note_id);
        assert!(note_on.note_id.unwrap().value() <= i32::MAX as u64);
    }

    #[test]
    fn live_midi_decoder_maps_controllers_and_pitch_bend() {
        let mut decoder = MidiInputDecoder::default();
        let mut messages = Vec::new();
        decoder.push_bytes(&[0xB0, 7, 100, 0xE0, 0, 64], &mut messages);
        let track_id = TrackId::from_value(9).unwrap();

        let controller = messages[0].scheduled_event(track_id).unwrap();
        assert_eq!(controller.kind, MidiEventKind::ControllerChange);
        assert_eq!(controller.controller, Some(7));
        assert_eq!(controller.velocity, 100);
        let bend = messages[1].scheduled_event(track_id).unwrap();
        assert_eq!(bend.kind, MidiEventKind::PitchBend);
        assert_eq!(bend.pitch_bend, Some(8192));
    }
}
