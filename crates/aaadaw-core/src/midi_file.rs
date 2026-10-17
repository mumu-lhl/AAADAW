use crate::{FrameRate, ItemId, MidiItem, MidiNote, Project, TempoCurve, TrackId};
use std::collections::{HashMap, HashSet};

const MAX_MIDI_FILE_EVENTS: usize = 1_000_000;

const MAX_VLQ: u64 = 0x0fff_ffff;
const MAX_TEMPO_EVENTS: usize = 100_000;
const TEMPO_CURVE_TOLERANCE_BPM: f64 = 0.02;

#[derive(Debug)]
struct Event {
    tick: u64,
    order: u8,
    bytes: Vec<u8>,
}

struct TempoEvent {
    tick: u64,
    bpm: f64,
    order: u8,
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

/// Which project time range is consolidated into the MIDI file.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MidiExportTimeScope {
    /// Export all MIDI events in the project.
    #[default]
    EntireProject,
    /// Export events intersecting the active project time selection, rebased to tick zero.
    TimeSelectionOnly,
}

/// Which MIDI items are consolidated into the MIDI file.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MidiExportItemScope {
    /// Export MIDI items on every project track.
    #[default]
    All,
    /// Export MIDI items on selected project tracks.
    SelectedTracksOnly,
    /// Export selected MIDI items only.
    SelectedItemsOnly,
}

/// The live editor selection used by MIDI export's selected-item and time scopes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MidiExportSelection {
    /// IDs of selected project tracks at export time.
    pub selected_track_ids: Vec<TrackId>,
    /// IDs of selected media items at export time.
    pub selected_item_ids: Vec<ItemId>,
    /// Active project time selection in project ticks, if any.
    pub time_selection_ticks: Option<(u64, u64)>,
}

/// Options used when encoding a project as a Standard MIDI File.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiExportOptions {
    pub file_type: MidiFileType,
    pub ppq: u16,
    pub time_scope: MidiExportTimeScope,
    pub item_scope: MidiExportItemScope,
    pub embed_tempo_and_time_signature: bool,
    pub embed_smpte_offset: bool,
    pub export_markers: bool,
    pub marker_type: MidiMarkerType,
    pub only_markers_starting_with_hash: bool,
    /// Include individually or ruler-hidden markers in Type 1 and time-scoped Type 0 exports.
    /// REAPER 7.82's entire-project Type 0 path still omits individually hidden markers.
    pub export_hidden_markers: bool,
}

impl Default for MidiExportOptions {
    fn default() -> Self {
        Self {
            file_type: MidiFileType::Type1,
            ppq: 960,
            time_scope: MidiExportTimeScope::EntireProject,
            item_scope: MidiExportItemScope::All,
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

struct MidiExportScope<'a> {
    included_midi_items: Vec<&'a MidiItem>,
    time_selection: Option<(f64, f64)>,
}

fn midi_export_scope<'a>(
    project: &'a Project,
    options: MidiExportOptions,
    selection: &MidiExportSelection,
) -> Result<MidiExportScope<'a>, String> {
    let time_selection = match options.time_scope {
        MidiExportTimeScope::EntireProject => None,
        MidiExportTimeScope::TimeSelectionOnly => {
            let Some((start_tick, end_tick)) = selection.time_selection_ticks else {
                return Err("time-selection export requires an active time selection".to_owned());
            };
            if end_tick <= start_tick {
                return Err("time selection must have a positive length".to_owned());
            }
            Some((start_tick as f64, end_tick as f64))
        }
    };
    let selected_track_ids = selection
        .selected_track_ids
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let selected_item_ids = selection
        .selected_item_ids
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let muted_track_ids = project
        .tracks()
        .iter()
        .filter(|track| track.is_muted())
        .map(|track| track.id())
        .collect::<HashSet<_>>();
    let included_midi_items = project
        .midi_items()
        .iter()
        .filter(|item| match options.item_scope {
            MidiExportItemScope::All => true,
            MidiExportItemScope::SelectedTracksOnly => {
                selected_track_ids.contains(&item.track_id())
            }
            MidiExportItemScope::SelectedItemsOnly => selected_item_ids.contains(&item.id()),
        })
        .filter(|item| !item.is_muted() && !muted_track_ids.contains(&item.track_id()))
        .filter(|item| {
            let Some((selection_start, selection_end)) = time_selection else {
                return true;
            };
            let item_start = item.start_position().as_ticks_f64();
            let Some(item_end) = item.start_position().checked_add_ticks(item.length_ticks())
            else {
                return false;
            };
            item_end.as_ticks_f64() > selection_start && item_start < selection_end
        })
        .collect::<Vec<_>>();

    Ok(MidiExportScope {
        included_midi_items,
        time_selection,
    })
}

/// Returns whether any MIDI items match the current item and time export scopes.
///
/// The desktop export dialog uses this lightweight check to ask before writing a header-only file.
pub fn midi_export_has_items_in_scope(
    project: &Project,
    options: MidiExportOptions,
    selection: &MidiExportSelection,
) -> Result<bool, String> {
    Ok(!midi_export_scope(project, options, selection)?
        .included_midi_items
        .is_empty())
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
    encode_standard_midi_with_conductor_track_name(project, options, "Tempo map")
}

/// Encodes a project using the conductor track name derived from the output file name.
///
/// REAPER labels the format-1 conductor track with the destination MIDI file's stem.
/// Format 0 has no track-name meta-event when the optional metadata controls are off.
/// Callers that do not have an output path can use [`encode_standard_midi`], which
/// retains the generic `Tempo map` label for format 1.
pub fn encode_standard_midi_with_conductor_track_name(
    project: &Project,
    options: MidiExportOptions,
    conductor_track_name: &str,
) -> Result<Vec<u8>, String> {
    encode_standard_midi_with_selection(
        project,
        options,
        conductor_track_name,
        &MidiExportSelection::default(),
    )
}

/// Encodes selected MIDI items and/or the selected project time range when requested.
pub fn encode_standard_midi_with_selection(
    project: &Project,
    options: MidiExportOptions,
    conductor_track_name: &str,
    selection: &MidiExportSelection,
) -> Result<Vec<u8>, String> {
    let project_ppq = project.settings().ppq();
    if options.ppq == 0 || options.ppq > 0x7fff {
        return Err(format!(
            "MIDI PPQN {} is outside the supported range 1..=32767",
            options.ppq
        ));
    }
    let ppq = u64::from(options.ppq);

    let scope = midi_export_scope(project, options, selection)?;
    let time_selection = scope.time_selection;
    let included_midi_items = scope.included_midi_items;

    let export_tracks = project
        .tracks()
        .iter()
        .filter(|track| {
            included_midi_items
                .iter()
                .any(|item| item.track_id() == track.id())
        })
        .collect::<Vec<_>>();

    let track_indices: HashMap<_, _> = export_tracks
        .iter()
        .enumerate()
        .map(|(index, track)| (track.id().value(), index))
        .collect();
    let mut events_by_track = (0..export_tracks.len())
        .map(|_| Vec::new())
        .collect::<Vec<Vec<Event>>>();
    let mut midi_event_count = 0_usize;

    for item in included_midi_items {
        let Some(&track_index) = track_indices.get(&item.track_id().value()) else {
            return Err(format!(
                "MIDI item {} refers to a missing track",
                item.id().value()
            ));
        };
        let item_end = item
            .start_position()
            .checked_add_ticks(item.length_ticks())
            .ok_or_else(|| "MIDI item extends beyond the supported timeline".to_owned())?;
        for note in item.notes() {
            let note_end_tick = note
                .tick()
                .checked_add(note.duration())
                .ok_or_else(|| "MIDI note extends beyond the supported timeline".to_owned())?;
            if item.is_loop_source() {
                for project_start in item.project_positions_at_content_tick(note.tick()) {
                    let project_end = project_start
                        .checked_add_ticks(note.duration())
                        .ok_or_else(|| "MIDI note ends beyond the supported timeline".to_owned())?
                        .min(item_end);
                    let added = push_midi_note_events_in_time_selection(
                        &mut events_by_track[track_index],
                        project_midi_item_event_tick(project, item, project_start)?,
                        project_midi_item_event_tick(project, item, project_end)?,
                        project_ppq,
                        options.ppq,
                        note,
                        time_selection,
                    )?;
                    ensure_midi_event_capacity(&mut midi_event_count, added)?;
                }
            } else {
                let visible_content_end = item
                    .source_offset_ticks()
                    .checked_add(item.length_ticks())
                    .ok_or_else(|| {
                        "trimmed MIDI item extends beyond the supported timeline".to_owned()
                    })?;
                let visible_start = note.tick().max(item.source_offset_ticks());
                let visible_end = note_end_tick.min(visible_content_end);
                if visible_end <= visible_start {
                    continue;
                }
                let project_start_position = item
                    .start_position()
                    .checked_add_ticks(visible_start - item.source_offset_ticks())
                    .ok_or_else(|| "MIDI note starts beyond the supported timeline".to_owned())?;
                let project_end_position = item
                    .start_position()
                    .checked_add_ticks(visible_end - item.source_offset_ticks())
                    .ok_or_else(|| "MIDI note ends beyond the supported timeline".to_owned())?;
                let added = push_midi_note_events_in_time_selection(
                    &mut events_by_track[track_index],
                    project_midi_item_event_tick(project, item, project_start_position)?,
                    project_midi_item_event_tick(project, item, project_end_position)?,
                    project_ppq,
                    options.ppq,
                    note,
                    time_selection,
                )?;
                ensure_midi_event_capacity(&mut midi_event_count, added)?;
            }
        }

        for controller in item.controllers() {
            if item.is_loop_source() {
                for project_position in item.project_positions_at_content_tick(controller.tick) {
                    let project_tick =
                        project_midi_item_event_tick(project, item, project_position)?;
                    let order = if time_selection
                        .is_some_and(|(selection_start, _)| project_tick < selection_start)
                    {
                        2
                    } else {
                        1
                    };
                    let Some(project_tick) =
                        time_selection_state_event_tick(project_tick, time_selection)
                    else {
                        continue;
                    };
                    ensure_midi_event_capacity(&mut midi_event_count, 1)?;
                    let tick = scale_tick_position(project_tick, project_ppq, options.ppq)?;
                    events_by_track[track_index].push(Event {
                        tick,
                        order,
                        bytes: vec![0xb0, controller.controller, controller.value],
                    });
                }
            } else {
                let visible_content_end = item
                    .source_offset_ticks()
                    .checked_add(item.length_ticks())
                    .ok_or_else(|| {
                        "trimmed MIDI item extends beyond the supported timeline".to_owned()
                    })?;
                if controller.tick >= item.source_offset_ticks()
                    && controller.tick < visible_content_end
                {
                    let project_position = item
                        .project_position_at_content_tick(controller.tick)
                        .ok_or_else(|| {
                        "MIDI controller is beyond the supported timeline".to_owned()
                    })?;
                    let project_tick =
                        project_midi_item_event_tick(project, item, project_position)?;
                    let order = if time_selection
                        .is_some_and(|(selection_start, _)| project_tick < selection_start)
                    {
                        2
                    } else {
                        1
                    };
                    let Some(project_tick) =
                        time_selection_state_event_tick(project_tick, time_selection)
                    else {
                        continue;
                    };
                    ensure_midi_event_capacity(&mut midi_event_count, 1)?;
                    let tick = scale_tick_position(project_tick, project_ppq, options.ppq)?;
                    events_by_track[track_index].push(Event {
                        tick,
                        order,
                        bytes: vec![0xb0, controller.controller, controller.value],
                    });
                }
            }
        }

        for bend in item.pitch_bends() {
            if item.is_loop_source() {
                for project_position in item.project_positions_at_content_tick(bend.tick) {
                    let project_tick =
                        project_midi_item_event_tick(project, item, project_position)?;
                    let Some(project_tick) =
                        time_selection_state_event_tick(project_tick, time_selection)
                    else {
                        continue;
                    };
                    ensure_midi_event_capacity(&mut midi_event_count, 1)?;
                    let tick = scale_tick_position(project_tick, project_ppq, options.ppq)?;
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
            } else {
                let visible_content_end = item
                    .source_offset_ticks()
                    .checked_add(item.length_ticks())
                    .ok_or_else(|| {
                        "trimmed MIDI item extends beyond the supported timeline".to_owned()
                    })?;
                if bend.tick >= item.source_offset_ticks() && bend.tick < visible_content_end {
                    let project_position = item
                        .project_position_at_content_tick(bend.tick)
                        .ok_or_else(|| {
                            "MIDI pitch bend is beyond the supported timeline".to_owned()
                        })?;
                    let project_tick =
                        project_midi_item_event_tick(project, item, project_position)?;
                    let Some(project_tick) =
                        time_selection_state_event_tick(project_tick, time_selection)
                    else {
                        continue;
                    };
                    ensure_midi_event_capacity(&mut midi_event_count, 1)?;
                    let tick = scale_tick_position(project_tick, project_ppq, options.ppq)?;
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
        }
    }

    let mut tracks = Vec::new();
    let smf_type = match options.file_type {
        MidiFileType::Type0 => {
            let last_midi_event_tick = events_by_track
                .iter()
                .flat_map(|track_events| track_events.iter().map(|event| event.tick))
                .max();
            let mut events = conductor_events(
                project,
                options,
                project_ppq,
                None,
                options.embed_tempo_and_time_signature,
                time_selection,
            )?;
            if let Some(last_conductor_order_at_start) = events
                .iter()
                .filter(|event| event.tick == 0)
                .map(|event| event.order)
                .max()
            {
                let midi_order_offset = last_conductor_order_at_start.saturating_add(1);
                for event in events_by_track
                    .iter_mut()
                    .flat_map(|track_events| track_events.iter_mut())
                    .filter(|event| event.tick == 0)
                {
                    event.order = event.order.saturating_add(midi_order_offset);
                }
            }
            if let Some(last_midi_event_tick) = last_midi_event_tick {
                // REAPER's merged Type 0 exporter stops writing tempo-map events after
                // the final MIDI event, while Type 1 keeps the full selected map.
                events.retain(|event| {
                    !is_tempo_map_event(event) || event.tick <= last_midi_event_tick
                });
            }
            for track_events in &mut events_by_track {
                events.append(track_events);
            }
            tracks.push(encode_track(events, 0)?);
            0_u16
        }
        MidiFileType::Type1 => {
            let conductor = conductor_events(
                project,
                options,
                project_ppq,
                Some(conductor_track_name.as_bytes()),
                true,
                time_selection,
            )?;
            tracks.push(encode_track(conductor, 0)?);
            if export_tracks.is_empty() {
                // REAPER writes a blank MIDI track after the conductor when there are no
                // unmuted MIDI items in the selected item/time scope.
                tracks.push(encode_track(Vec::new(), 0)?);
            }
            for (index, track) in export_tracks.iter().enumerate() {
                let mut events = vec![Event {
                    tick: 0,
                    order: 0,
                    bytes: meta_event(0x03, &reaper_midi_track_name(track.name()))?,
                }];
                events.append(&mut events_by_track[index]);
                tracks.push(encode_track(events, 0)?);
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

fn ensure_midi_event_capacity(count: &mut usize, additional: usize) -> Result<(), String> {
    let next = count
        .checked_add(additional)
        .ok_or_else(|| "MIDI export exceeds the supported event limit".to_owned())?;
    if next > MAX_MIDI_FILE_EVENTS {
        return Err("MIDI export exceeds the supported event limit".to_owned());
    }
    *count = next;
    Ok(())
}

fn is_tempo_map_event(event: &Event) -> bool {
    event.bytes.len() >= 2 && event.bytes[0] == 0xff && matches!(event.bytes[1], 0x01 | 0x51 | 0x58)
}

fn project_midi_item_event_tick(
    project: &Project,
    item: &crate::MidiItem,
    event_position: crate::MidiItemPosition,
) -> Result<f64, String> {
    // REAPER exports source events at their elapsed-time position inside the item. Mapping the
    // item-relative fraction through the project's timebase preserves its native ramp behavior.
    let item_start = item.start_position();
    let item_length = item.length_ticks();
    let item_end = item_start
        .checked_add_ticks(item_length)
        .ok_or_else(|| "MIDI item extends beyond the supported timeline".to_owned())?;
    if item_length == 0 || !(item_start..=item_end).contains(&event_position) {
        return Err("MIDI event lies outside its item's visible time range".to_owned());
    }
    let item_start_tick = item_start.as_ticks_f64();
    let item_end_tick = item_end.as_ticks_f64();
    let event_tick = event_position.as_ticks_f64();
    let start_sample = project
        .sample_at_tick_position(item_start_tick)
        .map_err(|error| error.to_string())?;
    let end_sample = project
        .sample_at_tick_position(item_end_tick)
        .map_err(|error| error.to_string())?;
    let progress = (event_tick - item_start_tick) / item_length as f64;
    let event_sample = start_sample + (end_sample - start_sample) * progress;
    let event_tick = project
        .tick_at_sample_position(event_sample)
        .map_err(|error| error.to_string())?;
    if !event_tick.is_finite() || event_tick < 0.0 || event_tick > (1_u64 << 53) as f64 {
        return Err("MIDI event lies beyond the supported project timeline".to_owned());
    }
    Ok(event_tick)
}

fn push_midi_note_events(
    events: &mut Vec<Event>,
    project_start_tick: f64,
    project_end_tick: f64,
    project_ppq: u32,
    export_ppq: u16,
    pitch: u8,
    velocity: u8,
) -> Result<(), String> {
    let start_tick = scale_tick_position(project_start_tick, project_ppq, export_ppq)?;
    let mut end_tick = scale_tick_position(project_end_tick, project_ppq, export_ppq)?;
    // Preserve a positive note duration when PPQ conversion rounds both ends to the
    // same exported tick. A zero-length note-off would sort before its note-on below.
    if end_tick <= start_tick {
        end_tick = start_tick
            .checked_add(1)
            .ok_or_else(|| "MIDI note ends beyond the supported timeline".to_owned())?;
    }
    events.push(Event {
        tick: end_tick,
        order: 0,
        bytes: vec![0x80, pitch, 0],
    });
    events.push(Event {
        tick: start_tick,
        order: 2,
        bytes: vec![0x90, pitch, velocity],
    });
    Ok(())
}

fn push_midi_note_events_in_time_selection(
    events: &mut Vec<Event>,
    project_start_tick: f64,
    project_end_tick: f64,
    project_ppq: u32,
    export_ppq: u16,
    note: &MidiNote,
    time_selection: Option<(f64, f64)>,
) -> Result<usize, String> {
    let Some((selection_start, selection_end)) = time_selection else {
        push_midi_note_events(
            events,
            project_start_tick,
            project_end_tick,
            project_ppq,
            export_ppq,
            note.pitch(),
            note.velocity(),
        )?;
        return Ok(2);
    };
    if project_end_tick <= selection_start || project_start_tick >= selection_end {
        return Ok(0);
    }

    let start_tick = scale_tick_position(
        project_start_tick.max(selection_start) - selection_start,
        project_ppq,
        export_ppq,
    )?;
    let order = if project_start_tick < selection_start {
        3
    } else {
        2
    };
    events.push(Event {
        tick: start_tick,
        order,
        bytes: vec![0x90, note.pitch(), note.velocity()],
    });
    if project_end_tick < selection_end {
        let mut end_tick =
            scale_tick_position(project_end_tick - selection_start, project_ppq, export_ppq)?;
        if end_tick <= start_tick {
            end_tick = start_tick
                .checked_add(1)
                .ok_or_else(|| "MIDI note ends beyond the supported timeline".to_owned())?;
        }
        events.push(Event {
            tick: end_tick,
            order: 0,
            bytes: vec![0x80, note.pitch(), 0],
        });
        Ok(2)
    } else {
        Ok(1)
    }
}

// REAPER carries controller and pitch-bend state from an overlapping item into tick zero.
fn time_selection_state_event_tick(
    project_tick: f64,
    time_selection: Option<(f64, f64)>,
) -> Option<f64> {
    let Some((selection_start, selection_end)) = time_selection else {
        return Some(project_tick);
    };
    if project_tick < selection_start {
        Some(0.0)
    } else if project_tick < selection_end {
        Some(project_tick - selection_start)
    } else {
        None
    }
}

fn time_selection_output_tick(
    project_tick: u64,
    time_selection: Option<(f64, f64)>,
    keep_initial_metadata: bool,
) -> Option<u64> {
    let Some((selection_start, selection_end)) = time_selection else {
        return Some(project_tick);
    };
    let project_tick_f64 = project_tick as f64;
    if project_tick_f64 >= selection_start && project_tick_f64 < selection_end {
        Some((project_tick_f64 - selection_start) as u64)
    } else if keep_initial_metadata && project_tick == 0 {
        Some(0)
    } else {
        None
    }
}

fn marker_time_selection_output_tick(
    project_tick: u64,
    time_selection: Option<(f64, f64)>,
    include_pre_selection_markers: bool,
) -> Option<u64> {
    let Some((selection_start, selection_end)) = time_selection else {
        return Some(project_tick);
    };
    let project_tick_f64 = project_tick as f64;
    if project_tick_f64 < selection_start {
        return include_pre_selection_markers.then_some(0);
    }
    // REAPER includes markers at both time-selection boundaries; MIDI item events remain half-open.
    (project_tick_f64 >= selection_start && project_tick_f64 <= selection_end)
        .then_some((project_tick_f64 - selection_start) as u64)
}

/// REAPER replaces each non-ASCII character in exported track names with `_`.
fn reaper_midi_track_name(name: &str) -> Vec<u8> {
    name.chars()
        .map(|character| {
            if character.is_ascii() {
                character as u8
            } else {
                b'_'
            }
        })
        .collect()
}

fn conductor_events(
    project: &Project,
    options: MidiExportOptions,
    project_ppq: u32,
    track_name: Option<&[u8]>,
    include_base_tempo_and_time_signature: bool,
    time_selection: Option<(f64, f64)>,
) -> Result<Vec<Event>, String> {
    let mut events = Vec::new();
    if let Some(track_name) = track_name {
        events.push(Event {
            tick: 0,
            order: 0,
            bytes: meta_event(0x03, track_name)?,
        });
    }
    let smpte_order_offset = u8::from(options.embed_smpte_offset);
    let starting_tempo_order = 2 + smpte_order_offset;
    let ramp_tempo_order = starting_tempo_order + 1;
    let mut tempo_events = Vec::new();
    if options.embed_tempo_and_time_signature || include_base_tempo_and_time_signature {
        let tempo_points = project.tempo_points().collect::<Vec<_>>();
        let basis = project.settings().bpm_basis().quarter_notes_per_beat();
        let ppq = u64::from(project_ppq);
        let point_count = if options.embed_tempo_and_time_signature {
            tempo_points.len()
        } else {
            tempo_points.len().min(1)
        };

        for (index, &(start_tick, bpm, curve)) in tempo_points.iter().take(point_count).enumerate()
        {
            if !options.embed_tempo_and_time_signature || curve == TempoCurve::Step {
                push_tempo_event(
                    &mut tempo_events,
                    start_tick,
                    bpm * basis,
                    starting_tempo_order,
                )?;
                continue;
            }
            let Some(&(end_tick, end_bpm, _)) = tempo_points.get(index + 1) else {
                push_tempo_event(
                    &mut tempo_events,
                    start_tick,
                    bpm * basis,
                    starting_tempo_order,
                )?;
                continue;
            };
            if end_tick <= start_tick {
                push_tempo_event(
                    &mut tempo_events,
                    start_tick,
                    bpm * basis,
                    starting_tempo_order,
                )?;
                continue;
            }
            match curve {
                TempoCurve::Linear => {
                    let ramp_start = format!("REAPER gradual tempo change start {bpm:.6}");
                    events.push(Event {
                        tick: scale_tick(start_tick, project_ppq, options.ppq)?,
                        order: starting_tempo_order,
                        bytes: meta_event(0x01, ramp_start.as_bytes())?,
                    });
                    let interval_ticks = (ppq / 8).max(1);
                    let mut interval_start = start_tick;
                    let mut last_interval_start = start_tick;
                    while interval_start < end_tick {
                        if tempo_events.len() >= MAX_TEMPO_EVENTS {
                            return Err("tempo ramp exceeds the MIDI export event limit".to_owned());
                        }
                        let interval_end =
                            interval_start.saturating_add(interval_ticks).min(end_tick);
                        let start_sample = project
                            .sample_at_tick_position(interval_start as f64)
                            .map_err(|error| error.to_string())?;
                        let end_sample = project
                            .sample_at_tick_position(interval_end as f64)
                            .map_err(|error| error.to_string())?;
                        let midpoint_tick = project
                            .tick_at_sample_position((start_sample + end_sample) / 2.0)
                            .map_err(|error| error.to_string())?;
                        let midpoint_bpm = project.tempo_at_tick_position(midpoint_tick) * basis;
                        push_tempo_event(
                            &mut tempo_events,
                            interval_start,
                            midpoint_bpm,
                            ramp_tempo_order,
                        )?;
                        last_interval_start = interval_start;
                        interval_start = interval_end;
                    }
                    let ramp_end = "REAPER gradual tempo change interpolation end";
                    events.push(Event {
                        tick: scale_tick(last_interval_start, project_ppq, options.ppq)?,
                        order: ramp_tempo_order + 1,
                        bytes: meta_event(0x01, ramp_end.as_bytes())?,
                    });
                }
                TempoCurve::Step => unreachable!("step tempo points are handled above"),
                _ => {
                    push_tempo_event(
                        &mut tempo_events,
                        start_tick,
                        bpm * basis,
                        starting_tempo_order,
                    )?;
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
                        starting_tempo_order,
                        &mut tempo_events,
                    )?;
                }
            }
        }
    }

    for tempo_event in tempo_events {
        let microseconds_per_quarter = (60_000_000.0 / tempo_event.bpm).round();
        if !microseconds_per_quarter.is_finite()
            || !(1.0..=16_777_215.0).contains(&microseconds_per_quarter)
        {
            return Err(format!(
                "tempo {:.3} BPM cannot be represented in a MIDI file",
                tempo_event.bpm
            ));
        }
        let value = microseconds_per_quarter as u32;
        let Some(tick) = time_selection_output_tick(
            tempo_event.tick,
            time_selection,
            include_base_tempo_and_time_signature && tempo_event.tick == 0,
        ) else {
            continue;
        };
        events.push(Event {
            tick: scale_tick(tick, project_ppq, options.ppq)?,
            order: tempo_event.order,
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

    for (index, (tick, signature)) in project.time_signature_points().enumerate() {
        if !options.embed_tempo_and_time_signature
            && (index > 0 || !include_base_tempo_and_time_signature)
        {
            continue;
        }
        let numerator = u8::try_from(signature.numerator())
            .map_err(|_| "time-signature numerator does not fit in a MIDI file".to_owned())?;
        let denominator = signature.denominator();
        if !denominator.is_power_of_two() {
            return Err("MIDI files require a power-of-two time-signature denominator".to_owned());
        }
        let Some(tick) = time_selection_output_tick(
            tick,
            time_selection,
            include_base_tempo_and_time_signature && index == 0,
        ) else {
            continue;
        };
        events.push(Event {
            tick: scale_tick(tick, project_ppq, options.ppq)?,
            order: 1 + smpte_order_offset,
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

    if options.embed_smpte_offset {
        events.push(Event {
            tick: 0,
            order: 1,
            bytes: encode_smpte_offset(project, time_selection)?,
        });
    }

    if options.export_markers {
        let marker_kind = match options.marker_type {
            MidiMarkerType::Markers => 0x06,
            MidiMarkerType::Cues => 0x07,
        };
        // REAPER 7.82's entire-project Type 0 path ignores individual marker visibility,
        // while its time-selection path includes individually hidden markers when requested.
        let include_ruler_hidden_markers = options.export_hidden_markers;
        let include_individually_hidden_markers = options.export_hidden_markers
            && (options.file_type == MidiFileType::Type1 || time_selection.is_some());
        for marker in project.markers() {
            let hidden_by_ruler = !project.show_project_markers();
            let individually_hidden = !marker.is_visible();
            if hidden_by_ruler && !include_ruler_hidden_markers {
                continue;
            }
            if individually_hidden && !include_individually_hidden_markers {
                continue;
            }
            if options.only_markers_starting_with_hash && !marker.name().starts_with('#') {
                continue;
            }
            let marker_position = marker.position_ticks();
            let Some(tick) = marker_time_selection_output_tick(
                marker_position,
                time_selection,
                options.file_type == MidiFileType::Type1,
            ) else {
                continue;
            };
            let precedes_time_selection = time_selection
                .is_some_and(|(selection_start, _)| (marker_position as f64) < selection_start);
            events.push(Event {
                tick: scale_tick(tick, project_ppq, options.ppq)?,
                order: match (
                    options.file_type,
                    time_selection.is_some(),
                    precedes_time_selection,
                ) {
                    (MidiFileType::Type0, true, _) | (MidiFileType::Type1, true, true) => 0,
                    _ => ramp_tempo_order + 1,
                },
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
    tempo_order: u8,
    events: &mut Vec<TempoEvent>,
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
        tempo_order,
        events,
    )?;
    push_tempo_event(events, middle_tick, middle_bpm, tempo_order)?;
    subdivide_tempo_curve(
        project,
        basis,
        middle_tick,
        middle_bpm,
        end_tick,
        end_bpm,
        maximum_span,
        depth + 1,
        tempo_order,
        events,
    )
}

fn push_tempo_event(
    events: &mut Vec<TempoEvent>,
    tick: u64,
    bpm: f64,
    order: u8,
) -> Result<(), String> {
    if !bpm.is_finite() || bpm <= 0.0 {
        return Err("project contains an invalid tempo".to_owned());
    }
    if events.len() >= MAX_TEMPO_EVENTS {
        return Err("tempo map exceeds the MIDI export event limit".to_owned());
    }
    events.push(TempoEvent { tick, bpm, order });
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

fn scale_tick_position(tick: f64, project_ppq: u32, export_ppq: u16) -> Result<u64, String> {
    if project_ppq == 0 {
        return Err("project PPQ resolution must be greater than zero".to_owned());
    }
    let scaled = tick * f64::from(export_ppq) / f64::from(project_ppq);
    if !scaled.is_finite() || scaled < 0.0 || scaled >= u64::MAX as f64 {
        return Err("scaled MIDI position exceeds the supported timeline".to_owned());
    }
    Ok(scaled.round() as u64)
}

fn encode_smpte_offset(
    project: &Project,
    time_selection: Option<(f64, f64)>,
) -> Result<Vec<u8>, String> {
    let frame_rate = project.settings().frame_rate();
    let rate_code: u8 = match frame_rate {
        FrameRate::Fps23976 | FrameRate::Fps24 => 0,
        FrameRate::Fps25 => 1,
        FrameRate::Fps2997Drop => 2,
        // REAPER 7.82 emits the SMF 30 fps code for its 29.97 non-drop preset.
        FrameRate::Fps2997 | FrameRate::Fps30 => 3,
        FrameRate::Fps48 | FrameRate::Fps50 | FrameRate::Fps60 | FrameRate::Fps75 => {
            return Err(format!(
                "{frame_rate} FPS is not a supported MIDI SMPTE rate"
            ));
        }
    };
    let settings = project.settings();
    let selection_start_seconds = match time_selection {
        Some((start_tick, _)) => {
            project
                .sample_at_tick_position(start_tick)
                .map_err(|error| error.to_string())?
                / f64::from(settings.sample_rate())
        }
        None => 0.0,
    };
    let seconds = settings.project_start_time_offset_seconds() + selection_start_seconds;
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
