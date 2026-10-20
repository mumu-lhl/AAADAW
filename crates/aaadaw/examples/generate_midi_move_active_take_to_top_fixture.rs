use aaadaw_core::{DawAction, MidiItemTakeSnapshot, MidiNoteData, MidiNoteSnapshot, Project};
use aaadaw_storage::ProjectStore;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("expected the output project path")?,
    );
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut project = Project::new();
    project.apply(DawAction::CreateTrack {
        index: 0,
        name: "MIDI active take ordering".into(),
    })?;
    let track_id = project.tracks()[0].id();
    let items = [
        (0, vec!["Take B", "Take A"], 0),
        (3_840, vec!["Single Take"], 0),
        (7_680, vec!["Take A", "Take B", "Take C"], 2),
        (11_520, vec!["Take B", "Take A"], 1),
    ];

    for (start_tick, names, active_take_index) in items {
        project.apply(DawAction::InsertMidiItem {
            track_id,
            start_tick,
            length_ticks: 2_880,
        })?;
        let item_id = project.midi_items().last().ok_or("MIDI item missing")?.id();
        project.apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 60,
                channel: 0,
                tick: 240,
                duration: 240,
                velocity: 90,
                off_velocity: 0,
            }],
        })?;

        let mut snapshot = project.snapshot();
        let next_note_id = snapshot
            .midi_items
            .iter()
            .flat_map(|item| {
                item.notes
                    .iter()
                    .chain(item.other_takes.iter().flat_map(|take| take.notes.iter()))
                    .map(|note| note.id)
            })
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("MIDI note id exhausted")?;
        let take_snapshots = names
            .iter()
            .enumerate()
            .map(|(index, name)| MidiItemTakeSnapshot {
                index: index as u32,
                name: (*name).to_owned(),
                source_offset_ticks: 125 * (index as i64 + 1),
                loop_source: index == 0,
                source_loop_length_ticks: 1_920,
                notes: vec![MidiNoteSnapshot {
                    id: next_note_id + index as u64,
                    data: MidiNoteData {
                        pitch: 60 + index as u8 * 4 + (start_tick / 3_840) as u8,
                        channel: index as u8,
                        tick: 120 + index as u64 * 240,
                        duration: 240,
                        velocity: 100 - index as u8 * 5,
                        off_velocity: 0,
                    },
                }],
                controllers: Vec::new(),
                pitch_bends: Vec::new(),
                program_changes: Vec::new(),
                poly_aftertouches: Vec::new(),
                channel_pressures: Vec::new(),
                opaque_events: Vec::new(),
                event_order: Vec::new(),
            })
            .collect::<Vec<_>>();
        let active = take_snapshots
            .iter()
            .find(|take| take.index == active_take_index)
            .ok_or("active take missing")?
            .clone();
        let item = snapshot
            .midi_items
            .iter_mut()
            .find(|item| item.id == item_id.value())
            .ok_or("MIDI snapshot missing")?;
        item.name = active.name;
        item.active_take_index = active_take_index;
        item.source_offset_ticks = active.source_offset_ticks;
        item.loop_source = active.loop_source;
        item.source_loop_length_ticks = active.source_loop_length_ticks;
        item.notes = active.notes;
        item.controllers = active.controllers;
        item.pitch_bends = active.pitch_bends;
        item.program_changes = active.program_changes;
        item.poly_aftertouches = active.poly_aftertouches;
        item.channel_pressures = active.channel_pressures;
        item.opaque_events = active.opaque_events;
        item.event_order = active.event_order;
        item.other_takes = take_snapshots
            .into_iter()
            .filter(|take| take.index != active_take_index)
            .collect();
        project = Project::from_snapshot(snapshot)?;
    }

    let mut store = ProjectStore::open(&output)?;
    store.save(&project)?;
    store.close()?;
    println!("Wrote {}", output.display());
    Ok(())
}
