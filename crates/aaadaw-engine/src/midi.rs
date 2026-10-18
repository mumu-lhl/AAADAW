use crate::transport::TransportBlockSegment;
use aaadaw_core::{MidiItemPosition, NoteId, Project, TimebaseError, Track, TrackId};
use std::collections::HashMap;
use std::fmt;

pub use aaadaw_core::MIDI_CHANNEL_COUNT;

const MAX_COMPILED_MIDI_EVENTS: usize = 1_000_000;
const FIRST_LIVE_MIDI_NOTE_ID: u64 = 1 << 30;

fn sample_at_midi_position(
    project: &Project,
    position: MidiItemPosition,
) -> Result<u64, MidiScheduleError> {
    let sample = project
        .sample_at_tick_position(position.as_ticks_f64())
        .map_err(MidiScheduleError::Timebase)?;
    if !sample.is_finite() || sample < 0.0 || sample > u64::MAX as f64 {
        return Err(MidiScheduleError::PositionOutOfRange);
    }
    Ok(sample.round() as u64)
}

/// A MIDI event kind. Shared-sample ordering also accounts for sustain-on/off semantics.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MidiEventKind {
    ControllerChange,
    PitchBend,
    ProgramChange,
    NoteOff,
    NoteOn,
}

/// A note event positioned relative to the beginning of an audio block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduledMidiEvent {
    pub sample_offset: usize,
    pub track_id: TrackId,
    /// Zero-based channel in the inclusive range `0..16`.
    pub channel: u8,
    pub note_id: Option<NoteId>,
    pub pitch: u8,
    pub velocity: u8,
    pub controller: Option<u8>,
    pub pitch_bend: Option<u16>,
    pub program: Option<u8>,
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
        let (kind, pitch, velocity, controller, pitch_bend, program) = match channel_message {
            0x80 => (
                MidiEventKind::NoteOff,
                self.data1,
                self.data2,
                None,
                None,
                None,
            ),
            0x90 if self.data2 == 0 => (MidiEventKind::NoteOff, self.data1, 0, None, None, None),
            0x90 => (
                MidiEventKind::NoteOn,
                self.data1,
                self.data2,
                None,
                None,
                None,
            ),
            0xB0 => (
                MidiEventKind::ControllerChange,
                self.data1,
                self.data2,
                Some(self.data1),
                None,
                None,
            ),
            0xC0 => (
                MidiEventKind::ProgramChange,
                0,
                0,
                None,
                None,
                Some(self.data1),
            ),
            0xE0 => (
                MidiEventKind::PitchBend,
                0,
                0,
                None,
                Some(u16::from(self.data1) | (u16::from(self.data2) << 7)),
                None,
            ),
            _ => return None,
        };
        Some(ScheduledMidiEvent {
            sample_offset: 0,
            track_id,
            channel: self.status & 0x0f,
            pitch,
            velocity,
            controller,
            pitch_bend,
            program,
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
            next_note_id: FIRST_LIVE_MIDI_NOTE_ID,
        }
    }
}

impl MidiInputDecoder {
    /// Appends complete supported channel-voice messages found in `bytes`.
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
        let value = self
            .next_note_id
            .clamp(FIRST_LIVE_MIDI_NOTE_ID, i32::MAX as u64);
        self.next_note_id = if value == i32::MAX as u64 {
            FIRST_LIVE_MIDI_NOTE_ID
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
    channel: u8,
    note_id: Option<NoteId>,
    pitch: u8,
    velocity: u8,
    controller: Option<u8>,
    pitch_bend: Option<u16>,
    program: Option<u8>,
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
    channel: u8,
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
    channel: u8,
    values: Vec<CompiledPitchBendValue>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CompiledProgramValue {
    absolute_sample: u64,
    program: u8,
}

#[derive(Clone, Debug)]
struct ProgramTimeline {
    track_id: TrackId,
    channel: u8,
    values: Vec<CompiledProgramValue>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CompiledMidiNote {
    start_sample: u64,
    end_sample: u64,
    track_id: TrackId,
    channel: u8,
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
    program_timelines: Vec<ProgramTimeline>,
    notes: Vec<CompiledMidiNote>,
    note_interval_nodes: Vec<MidiNoteIntervalNode>,
    note_interval_root: Option<usize>,
}

fn midi_route_destinations(
    project: &Project,
    source: TrackId,
) -> Result<Vec<TrackId>, MidiScheduleError> {
    let mut destinations = vec![source];
    let mut pending = vec![source];
    while let Some(track_id) = pending.pop() {
        let Some(track) = project.tracks().iter().find(|track| track.id() == track_id) else {
            continue;
        };
        if let Some(destination) = track
            .effective_output_track()
            .filter(|_| track.main_send_enabled())
        {
            if destinations.len() == MAX_COMPILED_MIDI_EVENTS {
                return Err(MidiScheduleError::EventLimitExceeded);
            }
            destinations.push(destination);
            pending.push(destination);
        }
        for send in track.sends().iter().rev().filter(|send| {
            let parameters = send.parameters();
            parameters.midi_enabled && !parameters.muted
        }) {
            if destinations.len() == MAX_COMPILED_MIDI_EVENTS {
                return Err(MidiScheduleError::EventLimitExceeded);
            }
            let destination = send.destination();
            destinations.push(destination);
            pending.push(destination);
        }
    }
    Ok(destinations)
}

/// Cursor state to carry between callback-level MIDI schedules that may wrap.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MidiSegmentCursor {
    /// Exclusive project sample at the end of the last scheduled segment.
    pub sample_end: Option<u64>,
    /// MIDI chase generation at the start of the last scheduled segment.
    pub chase_generation: Option<u64>,
}

/// Summary returned after scheduling MIDI across transport callback segments.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MidiSegmentSchedule {
    /// Number of callback-relative MIDI events written to the output slice.
    pub event_count: usize,
    /// State to pass to the next callback's segmented schedule.
    pub cursor: MidiSegmentCursor,
}

/// The project could not be compiled into a MIDI event schedule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MidiScheduleError {
    /// A MIDI item references a missing track.
    MissingTrack { track_id: u64 },
    /// Tick/sample arithmetic exceeds the supported range.
    PositionOutOfRange,
    /// Expanding MIDI source loops would exceed the bounded offline schedule size.
    EventLimitExceeded,
    /// The bounded nonzero note-ID space cannot represent all routed note instances.
    RouteNoteIdExhausted,
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
            Self::EventLimitExceeded => formatter
                .write_str("MIDI source loops expand beyond the supported event schedule limit"),
            Self::RouteNoteIdExhausted => {
                formatter.write_str("MIDI Send route note identifiers are exhausted")
            }
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
    /// Compiles note-on/off events from a project. Muted items are always
    /// omitted; muted tracks and tracks excluded by solo are omitted for live
    /// playback. Call this off the audio callback; the returned plan can be
    /// queried without allocation.
    pub fn compile(project: &Project) -> Result<Self, MidiScheduleError> {
        Self::compile_with_audibility(project, true)
    }

    // Instruments must retain muted/excluded notes so live mix changes can
    // restore their current voices without rebuilding or seeking the graph.
    pub(crate) fn compile_for_render(project: &Project) -> Result<Self, MidiScheduleError> {
        Self::compile_with_audibility(project, false)
    }

    fn compile_with_audibility(
        project: &Project,
        filter_audibility: bool,
    ) -> Result<Self, MidiScheduleError> {
        let tracks = project.tracks();
        let has_solo = tracks.iter().any(Track::is_solo);
        let mut events = Vec::new();
        let mut notes = Vec::new();
        let mut controller_timelines = Vec::<ControllerTimeline>::new();
        let mut pitch_bend_timelines = Vec::<PitchBendTimeline>::new();
        let mut program_timelines = Vec::<ProgramTimeline>::new();
        let mut midi_routes = HashMap::<TrackId, Vec<TrackId>>::new();
        let mut routed_note_ids = HashMap::<(TrackId, NoteId, usize), NoteId>::new();
        let mut used_note_ids: std::collections::HashSet<_> = project
            .midi_items()
            .iter()
            .flat_map(|item| item.notes().iter().map(|note| note.id().value()))
            .collect();
        // Live input reserves this upper half of the CLAP note-ID range.
        let mut next_routed_note_id = FIRST_LIVE_MIDI_NOTE_ID - 1;

        let mut reachable_solo = std::collections::HashMap::new();
        if filter_audibility && has_solo {
            for track in tracks {
                let mut pending: Vec<_> = track
                    .effective_output_track()
                    .filter(|_| track.main_send_enabled())
                    .into_iter()
                    .chain(
                        track
                            .sends()
                            .iter()
                            .filter(|send| {
                                let parameters = send.parameters();
                                parameters.midi_enabled && !parameters.muted
                            })
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
                            .effective_output_track()
                            .filter(|_| target.main_send_enabled())
                            .into_iter()
                            .chain(
                                target
                                    .sends()
                                    .iter()
                                    .filter(|send| {
                                        let parameters = send.parameters();
                                        parameters.midi_enabled && !parameters.muted
                                    })
                                    .map(|send| send.destination()),
                            ),
                    );
                }
                reachable_solo.insert(track.id(), routed_to_solo);
            }
        }
        for item in project.midi_items() {
            if item.is_muted() {
                continue;
            }
            let track = tracks
                .iter()
                .find(|track| track.id() == item.track_id())
                .ok_or(MidiScheduleError::MissingTrack {
                    track_id: item.track_id().value(),
                })?;
            let routed_to_solo = reachable_solo.get(&track.id()).copied().unwrap_or(false);
            if filter_audibility
                && (track.is_muted() || (has_solo && !track.is_solo() && !routed_to_solo))
            {
                continue;
            }

            if let std::collections::hash_map::Entry::Vacant(entry) = midi_routes.entry(track.id())
            {
                entry.insert(midi_route_destinations(project, track.id())?);
            }
            let routed_tracks = &midi_routes[&track.id()];

            let clip_end_position = item
                .start_position()
                .checked_add_ticks(item.length_ticks())
                .ok_or(MidiScheduleError::PositionOutOfRange)?;
            for note in item.notes() {
                for start_position in item.project_positions_at_content_tick(note.tick()) {
                    let end_position = start_position
                        .checked_add_ticks(note.duration())
                        .ok_or(MidiScheduleError::PositionOutOfRange)?
                        .min(clip_end_position);
                    let start_sample = sample_at_midi_position(project, start_position)?;
                    let end_sample = sample_at_midi_position(project, end_position)?;
                    for (route_index, routed_track) in routed_tracks.iter().enumerate() {
                        if events.len() > MAX_COMPILED_MIDI_EVENTS - 2 {
                            return Err(MidiScheduleError::EventLimitExceeded);
                        }
                        let routed_note_id = if route_index == 0 {
                            note.id()
                        } else {
                            let key = (item.track_id(), note.id(), route_index);
                            if let Some(id) = routed_note_ids.get(&key) {
                                *id
                            } else {
                                loop {
                                    if next_routed_note_id == 0 {
                                        return Err(MidiScheduleError::RouteNoteIdExhausted);
                                    }
                                    let value = next_routed_note_id;
                                    next_routed_note_id -= 1;
                                    if used_note_ids.insert(value) {
                                        let id = NoteId::from_value(value)
                                            .expect("routed MIDI note identifiers are nonzero");
                                        routed_note_ids.insert(key, id);
                                        break id;
                                    }
                                }
                            }
                        };
                        notes.push(CompiledMidiNote {
                            start_sample,
                            end_sample,
                            track_id: *routed_track,
                            channel: note.channel(),
                            note_id: routed_note_id,
                            pitch: note.pitch(),
                            velocity: note.velocity(),
                        });
                        events.push(CompiledMidiEvent {
                            absolute_sample: start_sample,
                            track_id: *routed_track,
                            channel: note.channel(),
                            note_id: Some(routed_note_id),
                            pitch: note.pitch(),
                            velocity: note.velocity(),
                            controller: None,
                            pitch_bend: None,
                            program: None,
                            kind: MidiEventKind::NoteOn,
                        });
                        events.push(CompiledMidiEvent {
                            absolute_sample: end_sample,
                            track_id: *routed_track,
                            channel: note.channel(),
                            note_id: Some(routed_note_id),
                            pitch: note.pitch(),
                            velocity: 0,
                            controller: None,
                            pitch_bend: None,
                            program: None,
                            kind: MidiEventKind::NoteOff,
                        });
                    }
                }
            }
            for controller in item.controllers() {
                for position in item.project_positions_at_content_tick(controller.tick) {
                    let absolute_sample = sample_at_midi_position(project, position)?;
                    for routed_track in routed_tracks {
                        if events.len() == MAX_COMPILED_MIDI_EVENTS {
                            return Err(MidiScheduleError::EventLimitExceeded);
                        }
                        let timeline = controller_timelines.iter_mut().find(|timeline| {
                            timeline.track_id == *routed_track
                                && timeline.channel == controller.channel
                                && timeline.controller == controller.controller
                        });
                        let timeline = if let Some(timeline) = timeline {
                            timeline
                        } else {
                            controller_timelines.push(ControllerTimeline {
                                track_id: *routed_track,
                                channel: controller.channel,
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
                            track_id: *routed_track,
                            channel: controller.channel,
                            note_id: None,
                            pitch: controller.controller,
                            velocity: controller.value,
                            controller: Some(controller.controller),
                            pitch_bend: None,
                            program: None,
                            kind: MidiEventKind::ControllerChange,
                        });
                    }
                }
            }
            for bend in item.pitch_bends() {
                for position in item.project_positions_at_content_tick(bend.tick) {
                    let absolute_sample = sample_at_midi_position(project, position)?;
                    for routed_track in routed_tracks {
                        if events.len() == MAX_COMPILED_MIDI_EVENTS {
                            return Err(MidiScheduleError::EventLimitExceeded);
                        }
                        let timeline = pitch_bend_timelines.iter_mut().find(|timeline| {
                            timeline.track_id == *routed_track && timeline.channel == bend.channel
                        });
                        let timeline = if let Some(timeline) = timeline {
                            timeline
                        } else {
                            pitch_bend_timelines.push(PitchBendTimeline {
                                track_id: *routed_track,
                                channel: bend.channel,
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
                            track_id: *routed_track,
                            channel: bend.channel,
                            note_id: None,
                            pitch: 0,
                            velocity: 0,
                            controller: None,
                            pitch_bend: Some(bend.value),
                            program: None,
                            kind: MidiEventKind::PitchBend,
                        });
                    }
                }
            }
            for change in item.program_changes() {
                for position in item.project_positions_at_content_tick(change.tick) {
                    let absolute_sample = sample_at_midi_position(project, position)?;
                    for routed_track in routed_tracks {
                        if events.len() == MAX_COMPILED_MIDI_EVENTS {
                            return Err(MidiScheduleError::EventLimitExceeded);
                        }
                        let timeline = program_timelines.iter_mut().find(|timeline| {
                            timeline.track_id == *routed_track && timeline.channel == change.channel
                        });
                        let timeline = if let Some(timeline) = timeline {
                            timeline
                        } else {
                            program_timelines.push(ProgramTimeline {
                                track_id: *routed_track,
                                channel: change.channel,
                                values: Vec::new(),
                            });
                            program_timelines
                                .last_mut()
                                .expect("program timeline was just inserted")
                        };
                        timeline.values.push(CompiledProgramValue {
                            absolute_sample,
                            program: change.program,
                        });
                        events.push(CompiledMidiEvent {
                            absolute_sample,
                            track_id: *routed_track,
                            channel: change.channel,
                            note_id: None,
                            pitch: 0,
                            velocity: 0,
                            controller: None,
                            pitch_bend: None,
                            program: Some(change.program),
                            kind: MidiEventKind::ProgramChange,
                        });
                    }
                }
            }
        }

        events.sort_by_key(|event| {
            (
                event.absolute_sample,
                midi_event_priority(event.kind, event.controller, event.velocity),
                event.track_id.value(),
                event.channel,
                event.pitch,
                event.note_id.map_or(0, NoteId::value),
                event.pitch_bend.unwrap_or(0),
                event.program.unwrap_or(0),
            )
        });
        for timeline in &mut controller_timelines {
            timeline.values.sort_by_key(|value| value.absolute_sample);
        }
        for timeline in &mut pitch_bend_timelines {
            timeline.values.sort_by_key(|value| value.absolute_sample);
        }
        for timeline in &mut program_timelines {
            timeline.values.sort_by_key(|value| value.absolute_sample);
        }
        notes.sort_unstable_by_key(|note| {
            (
                note.start_sample,
                note.track_id.value(),
                note.channel,
                note.note_id.value(),
            )
        });
        let (note_interval_nodes, note_interval_root) = compile_note_interval_tree(&notes);
        Ok(Self {
            events,
            controller_timelines,
            pitch_bend_timelines,
            program_timelines,
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
                    channel: timeline.channel,
                    note_id: None,
                    pitch: timeline.controller,
                    velocity: value.value,
                    controller: Some(timeline.controller),
                    pitch_bend: None,
                    program: None,
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
                channel: timeline.channel,
                note_id: None,
                pitch: 0,
                velocity: 0,
                controller: None,
                pitch_bend: Some(value),
                program: None,
                kind: MidiEventKind::PitchBend,
            });
            written += 1;
        }
        Ok(written)
    }

    /// Copies the last program change before `sample` as a block-offset-zero event per track and
    /// MIDI channel. A change exactly at the seek point stays in the ordinary block event list.
    pub fn active_program_changes_at(
        &self,
        sample: u64,
        output: &mut [Option<ScheduledMidiEvent>],
    ) -> Result<usize, MidiScheduleError> {
        let active = self
            .program_timelines
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
        for timeline in &self.program_timelines {
            let end = timeline
                .values
                .partition_point(|value| value.absolute_sample < sample);
            if let Some(value) = end.checked_sub(1).map(|index| timeline.values[index]) {
                output[written] = Some(ScheduledMidiEvent {
                    sample_offset: 0,
                    track_id: timeline.track_id,
                    channel: timeline.channel,
                    note_id: None,
                    pitch: 0,
                    velocity: 0,
                    controller: None,
                    pitch_bend: None,
                    program: Some(value.program),
                    kind: MidiEventKind::ProgramChange,
                });
                written += 1;
            }
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
                channel: note.channel,
                note_id: Some(note.note_id),
                pitch: note.pitch,
                velocity: note.velocity,
                controller: None,
                pitch_bend: None,
                program: None,
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
                        MidiEventKind::ControllerChange
                            | MidiEventKind::PitchBend
                            | MidiEventKind::ProgramChange
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
                channel: event.channel,
                note_id: event.note_id,
                pitch: event.pitch,
                velocity: event.velocity,
                controller: event.controller,
                pitch_bend: event.pitch_bend,
                program: event.program,
                kind: event.kind,
            });
        }
        Ok(required)
    }

    /// Plans scheduled MIDI across contiguous transport segments, including loop wraps.
    ///
    /// The caller supplies all three buffers on the control thread. `aggregate` holds the
    /// callback-wide event list while `temporary` is reused for each query; `output` is only
    /// modified after every segment fits. When a segment jumps away from the prior sample end,
    /// sustain is released, sounding notes are turned off, and controller, pitch-bend, and note
    /// state are chased at the destination when transport moves backward across a loop.
    pub fn events_for_segments(
        &self,
        segments: &[TransportBlockSegment],
        previous: MidiSegmentCursor,
        output: &mut [Option<ScheduledMidiEvent>],
        aggregate: &mut [Option<ScheduledMidiEvent>],
        temporary: &mut [Option<ScheduledMidiEvent>],
    ) -> Result<MidiSegmentSchedule, MidiScheduleError> {
        let mut written = 0;
        let mut cursor = previous;

        for segment in segments {
            let schedule = self.events_for_segment(*segment, cursor, temporary)?;
            for event in temporary[..schedule.event_count].iter_mut().flatten() {
                event.sample_offset = event
                    .sample_offset
                    .checked_add(segment.output_offset)
                    .ok_or(MidiScheduleError::PositionOutOfRange)?;
            }
            append_segment_events(aggregate, &mut written, temporary, schedule.event_count)?;
            cursor = schedule.cursor;
        }

        if output.len() < written {
            return Err(MidiScheduleError::OutputBufferTooSmall {
                required: written,
                available: output.len(),
            });
        }
        output[..written].copy_from_slice(&aggregate[..written]);
        Ok(MidiSegmentSchedule {
            event_count: written,
            cursor,
        })
    }

    pub(crate) fn events_for_segment(
        &self,
        segment: TransportBlockSegment,
        previous: MidiSegmentCursor,
        output: &mut [Option<ScheduledMidiEvent>],
    ) -> Result<MidiSegmentSchedule, MidiScheduleError> {
        if segment.frame_count == 0 {
            return Ok(MidiSegmentSchedule {
                event_count: 0,
                cursor: previous,
            });
        }
        let segment_end = segment
            .start_sample
            .checked_add(
                u64::try_from(segment.frame_count)
                    .map_err(|_| MidiScheduleError::PositionOutOfRange)?,
            )
            .ok_or(MidiScheduleError::PositionOutOfRange)?;
        let mut event_count = 0;
        let mut boundary_release_count = 0;

        if previous
            .sample_end
            .is_some_and(|sample_end| segment.start_sample < sample_end)
            && let Some(previous_end) = previous.sample_end
        {
            let controller_count =
                self.active_controllers_at(previous_end, &mut output[event_count..])?;
            let mut sustain_release_count = 0;
            let controller_start = event_count;
            for index in controller_start..controller_start + controller_count {
                let Some(mut event) = output[index] else {
                    continue;
                };
                if event.controller == Some(64) && event.velocity >= 64 {
                    event = ScheduledMidiEvent {
                        velocity: 0,
                        sample_offset: 0,
                        ..event
                    };
                    output[event_count + sustain_release_count] = Some(event);
                    sustain_release_count += 1;
                }
            }
            event_count += sustain_release_count;

            if let Some(sample_before_end) = previous_end.checked_sub(1) {
                let count = self.active_notes_at(sample_before_end, &mut output[event_count..])?;
                for event in output[event_count..event_count + count]
                    .iter_mut()
                    .flatten()
                {
                    event.kind = MidiEventKind::NoteOff;
                    event.velocity = 0;
                    event.sample_offset = 0;
                }
                event_count += count;
            }
            boundary_release_count = event_count;
        }

        let should_chase = previous.sample_end != Some(segment.start_sample)
            || previous.chase_generation != Some(segment.chase_generation);
        if should_chase {
            let count =
                self.active_controllers_at(segment.start_sample, &mut output[event_count..])?;
            for event in output[event_count..event_count + count]
                .iter_mut()
                .flatten()
            {
                event.sample_offset = 0;
            }
            event_count += count;

            let count =
                self.active_pitch_bends_at(segment.start_sample, &mut output[event_count..])?;
            for event in output[event_count..event_count + count]
                .iter_mut()
                .flatten()
            {
                event.sample_offset = 0;
            }
            event_count += count;

            let count =
                self.active_program_changes_at(segment.start_sample, &mut output[event_count..])?;
            for event in output[event_count..event_count + count]
                .iter_mut()
                .flatten()
            {
                event.sample_offset = 0;
            }
            event_count += count;

            let count = self.active_notes_at(segment.start_sample, &mut output[event_count..])?;
            for event in output[event_count..event_count + count]
                .iter_mut()
                .flatten()
            {
                event.sample_offset = 0;
            }
            event_count += count;
        }

        let scheduled_count = self.events_for_block(
            segment.start_sample,
            segment.frame_count,
            &mut output[event_count..],
        )?;
        event_count += scheduled_count;
        if previous.sample_end != Some(segment.start_sample)
            || previous.chase_generation != Some(segment.chase_generation)
        {
            output[boundary_release_count..event_count].sort_unstable_by_key(|event| {
                let event = event.expect("segmented MIDI output slots are initialized");
                (
                    event.sample_offset,
                    midi_event_priority(event.kind, event.controller, event.velocity),
                    event.track_id.value(),
                    event.controller.unwrap_or(event.pitch),
                    event.note_id.map_or(0, NoteId::value),
                )
            });
        }
        Ok(MidiSegmentSchedule {
            event_count,
            cursor: MidiSegmentCursor {
                sample_end: Some(segment_end),
                chase_generation: Some(segment.chase_generation),
            },
        })
    }
}

fn append_segment_events(
    aggregate: &mut [Option<ScheduledMidiEvent>],
    written: &mut usize,
    temporary: &[Option<ScheduledMidiEvent>],
    count: usize,
) -> Result<(), MidiScheduleError> {
    let required = written
        .checked_add(count)
        .ok_or(MidiScheduleError::PositionOutOfRange)?;
    if aggregate.len() < required {
        return Err(MidiScheduleError::OutputBufferTooSmall {
            required,
            available: aggregate.len(),
        });
    }
    aggregate[*written..required].copy_from_slice(&temporary[..count]);
    *written = required;
    Ok(())
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
        MidiEventKind::ProgramChange => 1,
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
        decoder.push_bytes(&[0xB3, 7, 100, 0xE5, 0, 64], &mut messages);
        let track_id = TrackId::from_value(9).unwrap();

        let controller = messages[0].scheduled_event(track_id).unwrap();
        assert_eq!(controller.kind, MidiEventKind::ControllerChange);
        assert_eq!(controller.channel, 3);
        assert_eq!(controller.controller, Some(7));
        assert_eq!(controller.velocity, 100);
        let bend = messages[1].scheduled_event(track_id).unwrap();
        assert_eq!(bend.kind, MidiEventKind::PitchBend);
        assert_eq!(bend.channel, 5);
        assert_eq!(bend.pitch_bend, Some(8192));
    }

    #[test]
    fn live_midi_decoder_pairs_same_pitch_notes_per_channel() {
        let mut decoder = MidiInputDecoder::default();
        let mut messages = Vec::new();
        decoder.push_bytes(
            &[0x90, 60, 100, 0x91, 60, 101, 0x80, 60, 0, 0x81, 60, 0],
            &mut messages,
        );
        let track_id = TrackId::from_value(10).unwrap();
        let events = messages
            .iter()
            .map(|message| message.scheduled_event(track_id).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(
            events.iter().map(|event| event.channel).collect::<Vec<_>>(),
            [0, 1, 0, 1]
        );
        assert_eq!(events[0].note_id, events[2].note_id);
        assert_eq!(events[1].note_id, events[3].note_id);
        assert_ne!(events[0].note_id, events[1].note_id);
    }

    #[test]
    fn live_midi_decoder_accepts_one_byte_program_changes_and_running_status() {
        let mut decoder = MidiInputDecoder::default();
        let mut messages = Vec::new();
        decoder.push_bytes(&[0xc3, 41, 42], &mut messages);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].data1, 41);
        assert_eq!(messages[1].data1, 42);
        let event = messages[0]
            .scheduled_event(TrackId::from_value(7).unwrap())
            .unwrap();
        assert_eq!(event.kind, MidiEventKind::ProgramChange);
        assert_eq!(event.channel, 3);
        assert_eq!(event.program, Some(41));
        assert_eq!(event.note_id, None);
    }
}

#[cfg(test)]
mod item_mute_tests {
    use super::MidiEventPlan;
    use aaadaw_core::{DawAction, MidiNoteData, Project};

    #[test]
    fn offline_midi_plan_omits_muted_items() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Muted MIDI".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertMidiItem {
                track_id: project.tracks()[0].id(),
                start_tick: 0,
                length_ticks: 960,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        project
            .apply(DawAction::AddMidiNotes {
                item_id,
                notes: vec![MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 0,
                    duration: 480,
                    velocity: 100,
                }],
            })
            .unwrap();
        project
            .apply(DawAction::SetMidiItemMute {
                item_id,
                muted: true,
            })
            .unwrap();

        let plan = MidiEventPlan::compile_for_render(&project).unwrap();
        assert!(plan.is_empty());
    }
}
