use crate::{FrameRate, Project, TempoCurve};
use std::collections::HashMap;

const MAX_VLQ: u64 = 0x0fff_ffff;
const MAX_TEMPO_EVENTS: usize = 100_000;
const TEMPO_CURVE_TOLERANCE_BPM: f64 = 0.02;

#[derive(Debug)]
struct Event {
    tick: u64,
    order: u8,
    bytes: Vec<u8>,
}

/// The Standard MIDI File track layout selected for project export.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MidiFileType {
    /// Merge all project MIDI into one SMF track.
    Type0,
    /// Keep a conductor track and one MIDI track per project track.
    #[default]
    Type1,
}

/// Whether exported project markers use the MIDI marker or cue-point meta event.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MidiMarkerType {
    #[default]
    Markers,
    Cues,
}

/// Options used when encoding a project as a Standard MIDI File.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiExportOptions {
    pub file_type: MidiFileType,
    pub ppq: u16,
    pub embed_tempo_and_time_signature: bool,
    pub embed_smpte_offset: bool,
    pub export_markers: bool,
    pub marker_type: MidiMarkerType,
    pub only_markers_starting_with_hash: bool,
    /// Include markers hidden individually or by the project marker ruler lane.
    pub export_hidden_markers: bool,
}

impl Default for MidiExportOptions {
    fn default() -> Self {
        Self {
            file_type: MidiFileType::Type1,
            ppq: 960,
            embed_tempo_and_time_signature: false,
            embed_smpte_offset: false,
            export_markers: false,
            marker_type: MidiMarkerType::Markers,
            only_markers_starting_with_hash: false,
            export_hidden_markers: false,
        }
    }
}

impl MidiExportOptions {
    /// Uses the project's PPQ resolution and REAPER's default export selections.
    pub fn for_project(project: &Project) -> Self {
        Self {
            ppq: project.settings().ppq().min(0x7fff) as u16,
            ..Self::default()
        }
    }
}

/// Encodes a project as a Standard MIDI File using the selected export options.
///
/// Project tracks, notes, controllers, pitch bends, and selected tempo, meter, SMPTE, and marker
/// metadata are encoded as MIDI events. Tempo ramps are sampled because SMF stores discrete tempo
/// changes only.
pub fn encode_standard_midi(
    project: &Project,
    options: MidiExportOptions,
) -> Result<Vec<u8>, String> {
    let project_ppq = project.settings().ppq();
    if options.ppq == 0 || options.ppq > 0x7fff {
        return Err(format!(
            "MIDI PPQN {} is outside the supported range 1..=32767",
            options.ppq
        ));
    }
    let ppq = u64::from(options.ppq);

    let track_indices: HashMap<_, _> = project
        .tracks()
        .iter()
        .enumerate()
        .map(|(index, track)| (track.id().value(), index))
        .collect();
    let mut events_by_track = (0..project.tracks().len())
        .map(|_| Vec::new())
        .collect::<Vec<Vec<Event>>>();
    let mut track_end_ticks = vec![0; project.tracks().len()];

    for item in project.midi_items() {
        let Some(&track_index) = track_indices.get(&item.track_id().value()) else {
            return Err(format!(
                "MIDI item {} refers to a missing track",
                item.id().value()
            ));
        };
        let item_end = item
            .start_tick()
            .checked_add(item.length_ticks())
            .ok_or_else(|| "MIDI item extends beyond the supported timeline".to_owned())?;
        track_end_ticks[track_index] =
            track_end_ticks[track_index].max(scale_tick(item_end, project_ppq, options.ppq)?);

        let visible_content_end = item
            .source_offset_ticks()
            .checked_add(item.length_ticks())
            .ok_or_else(|| "trimmed MIDI item extends beyond the supported timeline".to_owned())?;
        for note in item.notes() {
            let note_content_end = note
                .tick()
                .checked_add(note.duration())
                .ok_or_else(|| "MIDI note extends beyond the supported timeline".to_owned())?;
            let visible_start = note.tick().max(item.source_offset_ticks());
            let visible_end = note_content_end.min(visible_content_end);
            if visible_end <= visible_start {
                continue;
            }
            let project_start_tick = item
                .start_tick()
                .checked_add(visible_start - item.source_offset_ticks())
                .ok_or_else(|| "MIDI note starts beyond the supported timeline".to_owned())?;
            let project_end_tick = item
                .start_tick()
                .checked_add(visible_end - item.source_offset_ticks())
                .ok_or_else(|| "MIDI note ends beyond the supported timeline".to_owned())?;
            let start_tick = scale_tick(project_start_tick, project_ppq, options.ppq)?;
            let mut end_tick = scale_tick(project_end_tick, project_ppq, options.ppq)?;
            // Preserve a positive note duration when PPQ conversion rounds both ends to the
            // same exported tick. A zero-length note-off would sort before its note-on below.
            if end_tick <= start_tick {
                end_tick = start_tick
                    .checked_add(1)
                    .ok_or_else(|| "MIDI note ends beyond the supported timeline".to_owned())?;
            }
            events_by_track[track_index].push(Event {
                tick: end_tick,
                order: 0,
                bytes: vec![0x80, note.pitch(), 0],
            });
            events_by_track[track_index].push(Event {
                tick: start_tick,
                order: 2,
                bytes: vec![0x90, note.pitch(), note.velocity()],
            });
        }

        for controller in item.controllers() {
            if controller.tick < item.source_offset_ticks()
                || controller.tick >= visible_content_end
            {
                continue;
            }
            let project_tick = item
                .start_tick()
                .checked_add(controller.tick - item.source_offset_ticks())
                .ok_or_else(|| "MIDI controller is beyond the supported timeline".to_owned())?;
            let tick = scale_tick(project_tick, project_ppq, options.ppq)?;
            events_by_track[track_index].push(Event {
                tick,
                order: 1,
                bytes: vec![0xb0, controller.controller, controller.value],
            });
        }

        for bend in item.pitch_bends() {
            if bend.tick < item.source_offset_ticks() || bend.tick >= visible_content_end {
                continue;
            }
            let project_tick = item
                .start_tick()
                .checked_add(bend.tick - item.source_offset_ticks())
                .ok_or_else(|| "MIDI pitch bend is beyond the supported timeline".to_owned())?;
            let tick = scale_tick(project_tick, project_ppq, options.ppq)?;
            events_by_track[track_index].push(Event {
                tick,
                order: 1,
                bytes: vec![
                    0xe0,
                    (bend.value & 0x7f) as u8,
                    ((bend.value >> 7) & 0x7f) as u8,
                ],
            });
        }
    }

    let mut tracks = Vec::new();
    let smf_type = match options.file_type {
        MidiFileType::Type0 => {
            let mut events = conductor_events(project, options, project_ppq, b"Project MIDI")?;
            let mut end_tick = 0;
            for (index, track_events) in events_by_track.iter_mut().enumerate() {
                end_tick = end_tick.max(track_end_ticks[index]);
                events.append(track_events);
            }
            tracks.push(encode_track(events, end_tick)?);
            0_u16
        }
        MidiFileType::Type1 => {
            let conductor = conductor_events(project, options, project_ppq, b"Tempo map")?;
            tracks.push(encode_track(conductor, 0)?);
            for (index, track) in project.tracks().iter().enumerate() {
                let mut events = vec![Event {
                    tick: 0,
                    order: 0,
                    bytes: meta_event(0x03, track.name().as_bytes())?,
                }];
                events.append(&mut events_by_track[index]);
                tracks.push(encode_track(events, track_end_ticks[index])?);
            }
            1_u16
        }
    };
    let track_count = u16::try_from(tracks.len())
        .map_err(|_| "project has too many tracks for a MIDI file".to_owned())?;

    let mut file = Vec::new();
    file.extend_from_slice(b"MThd");
    file.extend_from_slice(&6_u32.to_be_bytes());
    file.extend_from_slice(&smf_type.to_be_bytes());
    file.extend_from_slice(&track_count.to_be_bytes());
    file.extend_from_slice(&(ppq as u16).to_be_bytes());
    for track in tracks {
        file.extend_from_slice(&track);
    }
    Ok(file)
}

fn conductor_events(
    project: &Project,
    options: MidiExportOptions,
    project_ppq: u32,
    track_name: &[u8],
) -> Result<Vec<Event>, String> {
    let mut events = vec![Event {
        tick: 0,
        order: 0,
        bytes: meta_event(0x03, track_name)?,
    }];
    let mut tempo_events = Vec::new();
    if options.embed_tempo_and_time_signature {
        let tempo_points = project.tempo_points().collect::<Vec<_>>();
        let basis = project.settings().bpm_basis().quarter_notes_per_beat();
        let ppq = u64::from(project_ppq);

        for (index, &(start_tick, bpm, curve)) in tempo_points.iter().enumerate() {
            push_tempo_event(&mut tempo_events, start_tick, bpm * basis)?;
            if curve == TempoCurve::Step {
                continue;
            }
            let Some(&(end_tick, end_bpm, _)) = tempo_points.get(index + 1) else {
                continue;
            };
            if end_tick <= start_tick {
                continue;
            }
            let first_bpm = bpm * basis;
            let last_bpm = end_bpm * basis;
            let maximum_span = (ppq / 16).max(1);
            subdivide_tempo_curve(
                project,
                basis,
                start_tick,
                first_bpm,
                end_tick,
                last_bpm,
                maximum_span,
                0,
                &mut tempo_events,
            )?;
        }
    }

    for (tick, bpm) in tempo_events {
        let microseconds_per_quarter = (60_000_000.0 / bpm).round();
        if !microseconds_per_quarter.is_finite()
            || !(1.0..=16_777_215.0).contains(&microseconds_per_quarter)
        {
            return Err(format!(
                "tempo {bpm:.3} BPM cannot be represented in a MIDI file"
            ));
        }
        let value = microseconds_per_quarter as u32;
        events.push(Event {
            tick: scale_tick(tick, project_ppq, options.ppq)?,
            order: 1,
            bytes: vec![
                0xff,
                0x51,
                0x03,
                (value >> 16) as u8,
                (value >> 8) as u8,
                value as u8,
            ],
        });
    }

    if options.embed_tempo_and_time_signature {
        for (tick, signature) in project.time_signature_points() {
            let numerator = u8::try_from(signature.numerator())
                .map_err(|_| "time-signature numerator does not fit in a MIDI file".to_owned())?;
            let denominator = signature.denominator();
            if !denominator.is_power_of_two() {
                return Err(
                    "MIDI files require a power-of-two time-signature denominator".to_owned(),
                );
            }
            events.push(Event {
                tick: scale_tick(tick, project_ppq, options.ppq)?,
                order: 2,
                bytes: vec![
                    0xff,
                    0x58,
                    0x04,
                    numerator,
                    denominator.trailing_zeros() as u8,
                    24,
                    8,
                ],
            });
        }
    }

    if options.embed_smpte_offset {
        events.push(Event {
            tick: 0,
            order: 3,
            bytes: encode_smpte_offset(project)?,
        });
    }

    if options.export_markers {
        let marker_kind = match options.marker_type {
            MidiMarkerType::Markers => 0x06,
            MidiMarkerType::Cues => 0x07,
        };
        for marker in project.markers() {
            let hidden = !project.show_project_markers() || !marker.is_visible();
            if hidden && !options.export_hidden_markers {
                continue;
            }
            if options.only_markers_starting_with_hash && !marker.name().starts_with('#') {
                continue;
            }
            events.push(Event {
                tick: scale_tick(marker.position_ticks(), project_ppq, options.ppq)?,
                order: 4,
                bytes: meta_event(marker_kind, marker.name().as_bytes())?,
            });
        }
    }
    Ok(events)
}

#[allow(clippy::too_many_arguments)]
fn subdivide_tempo_curve(
    project: &Project,
    basis: f64,
    start_tick: u64,
    start_bpm: f64,
    end_tick: u64,
    end_bpm: f64,
    maximum_span: u64,
    depth: u8,
    events: &mut Vec<(u64, f64)>,
) -> Result<(), String> {
    if end_tick.saturating_sub(start_tick) <= 1 || depth >= 64 {
        return Ok(());
    }
    let middle_tick = start_tick + (end_tick - start_tick) / 2;
    let middle_bpm = project.tempo_at_tick(middle_tick) * basis;
    let linear_midpoint = (start_bpm + end_bpm) / 2.0;
    if end_tick - start_tick <= maximum_span
        && (middle_bpm - linear_midpoint).abs() <= TEMPO_CURVE_TOLERANCE_BPM
    {
        return Ok(());
    }
    if events.len() >= MAX_TEMPO_EVENTS {
        return Err("tempo ramp exceeds the MIDI export event limit".to_owned());
    }
    subdivide_tempo_curve(
        project,
        basis,
        start_tick,
        start_bpm,
        middle_tick,
        middle_bpm,
        maximum_span,
        depth + 1,
        events,
    )?;
    events.push((middle_tick, middle_bpm));
    subdivide_tempo_curve(
        project,
        basis,
        middle_tick,
        middle_bpm,
        end_tick,
        end_bpm,
        maximum_span,
        depth + 1,
        events,
    )
}

fn push_tempo_event(events: &mut Vec<(u64, f64)>, tick: u64, bpm: f64) -> Result<(), String> {
    if !bpm.is_finite() || bpm <= 0.0 {
        return Err("project contains an invalid tempo".to_owned());
    }
    if events.len() >= MAX_TEMPO_EVENTS {
        return Err("tempo map exceeds the MIDI export event limit".to_owned());
    }
    events.push((tick, bpm));
    Ok(())
}

fn scale_tick(tick: u64, project_ppq: u32, export_ppq: u16) -> Result<u64, String> {
    if project_ppq == 0 {
        return Err("project PPQ resolution must be greater than zero".to_owned());
    }
    let numerator = u128::from(tick) * u128::from(export_ppq);
    let scaled = (numerator + u128::from(project_ppq / 2)) / u128::from(project_ppq);
    u64::try_from(scaled)
        .map_err(|_| "scaled MIDI position exceeds the supported timeline".to_owned())
}

fn encode_smpte_offset(project: &Project) -> Result<Vec<u8>, String> {
    let frame_rate = project.settings().frame_rate();
    let rate_code: u8 = match frame_rate {
        FrameRate::Fps23976 | FrameRate::Fps24 => 0,
        FrameRate::Fps25 => 1,
        FrameRate::Fps2997Drop => 2,
        FrameRate::Fps2997 => {
            return Err("29.97 non-drop is not representable in an SMF SMPTE offset".to_owned());
        }
        FrameRate::Fps30 => 3,
        FrameRate::Fps48 | FrameRate::Fps50 | FrameRate::Fps60 | FrameRate::Fps75 => {
            return Err(format!(
                "{frame_rate} FPS is not a supported MIDI SMPTE rate"
            ));
        }
    };
    let seconds = project.settings().project_start_time_offset_seconds();
    let timecode = frame_rate
        .format_timecode(seconds)
        .map_err(|error| format!("project SMPTE offset cannot be represented: {error}"))?;
    let fields = timecode
        .split(':')
        .map(str::parse::<u8>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "project SMPTE offset is not a valid MIDI timecode".to_owned())?;
    let [hours, minutes, seconds, frames] = fields.as_slice() else {
        return Err("project SMPTE offset is not a valid MIDI timecode".to_owned());
    };
    if *hours > 23 {
        return Err("MIDI SMPTE offsets must be shorter than 24 hours".to_owned());
    }
    meta_event(
        0x54,
        &[(rate_code << 5) | *hours, *minutes, *seconds, *frames, 0],
    )
}

fn meta_event(kind: u8, payload: &[u8]) -> Result<Vec<u8>, String> {
    if payload.len() as u64 > MAX_VLQ {
        return Err("MIDI metadata field is too long".to_owned());
    }
    let mut bytes = vec![0xff, kind];
    write_vlq(payload.len() as u64, &mut bytes);
    bytes.extend_from_slice(payload);
    Ok(bytes)
}

fn encode_track(mut events: Vec<Event>, end_tick: u64) -> Result<Vec<u8>, String> {
    events.sort_by(|left, right| {
        left.tick
            .cmp(&right.tick)
            .then_with(|| left.order.cmp(&right.order))
    });
    let end_tick = events
        .last()
        .map_or(end_tick, |event| event.tick.max(end_tick));
    let mut contents = Vec::new();
    let mut previous_tick = 0;
    for event in events {
        if event.tick < previous_tick {
            return Err("MIDI events are not ordered by project position".to_owned());
        }
        write_delta(event.tick - previous_tick, &mut contents);
        contents.extend_from_slice(&event.bytes);
        previous_tick = event.tick;
    }
    write_delta(end_tick.saturating_sub(previous_tick), &mut contents);
    contents.extend_from_slice(&[0xff, 0x2f, 0x00]);

    let length = u32::try_from(contents.len())
        .map_err(|_| "MIDI track exceeds the Standard MIDI File size limit".to_owned())?;
    let mut track = Vec::with_capacity(contents.len() + 8);
    track.extend_from_slice(b"MTrk");
    track.extend_from_slice(&length.to_be_bytes());
    track.extend_from_slice(&contents);
    Ok(track)
}

fn write_delta(mut delta: u64, output: &mut Vec<u8>) {
    while delta > MAX_VLQ {
        write_vlq(MAX_VLQ, output);
        output.extend_from_slice(&[0xff, 0x7f, 0x00]);
        delta -= MAX_VLQ;
    }
    write_vlq(delta, output);
}

fn write_vlq(value: u64, output: &mut Vec<u8>) {
    let mut buffer = [0_u8; 4];
    let mut index = 3;
    buffer[index] = (value & 0x7f) as u8;
    let mut remaining = value >> 7;
    while remaining > 0 {
        index -= 1;
        buffer[index] = ((remaining & 0x7f) as u8) | 0x80;
        remaining >>= 7;
    }
    output.extend_from_slice(&buffer[index..]);
}
