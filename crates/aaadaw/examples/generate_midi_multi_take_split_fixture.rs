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
        name: "MIDI multi-take split".into(),
    })?;
    let track_id = project.tracks()[0].id();
    project.apply(DawAction::InsertMidiItem {
        track_id,
        start_tick: 0,
        length_ticks: 3_840,
    })?;
    let item_id = project.midi_items()[0].id();
    project.apply(DawAction::AddMidiNotes {
        item_id,
        notes: vec![
            MidiNoteData {
                pitch: 60,
                channel: 0,
                tick: 960,
                duration: 1_920,
                velocity: 100,
                off_velocity: 0,
            },
            MidiNoteData {
                pitch: 67,
                channel: 0,
                tick: 3_000,
                duration: 480,
                velocity: 90,
                off_velocity: 0,
            },
            MidiNoteData {
                pitch: 62,
                channel: 0,
                tick: 480,
                duration: 240,
                velocity: 84,
                off_velocity: 0,
            },
            MidiNoteData {
                pitch: 64,
                channel: 0,
                tick: 1_200,
                duration: 240,
                velocity: 92,
                off_velocity: 41,
            },
            MidiNoteData {
                pitch: 72,
                channel: 0,
                tick: 3_360,
                duration: 360,
                velocity: 88,
                off_velocity: 0,
            },
        ],
    })?;

    let mut snapshot = project.snapshot();
    let item = &mut snapshot.midi_items[0];
    let take_b_notes = item.notes.clone();
    item.notes = take_b_notes[2..].to_vec();
    item.active_take_index = 1;
    item.name = "Take B".into();
    item.source_offset_ticks = 2_880;
    item.loop_source = true;
    item.source_loop_length_ticks = 3_840;
    item.other_takes = vec![MidiItemTakeSnapshot {
        index: 0,
        name: "Take A".into(),
        source_offset_ticks: 0,
        loop_source: true,
        source_loop_length_ticks: 3_840,
        notes: vec![
            MidiNoteSnapshot {
                id: 6,
                data: take_b_notes[0].data,
            },
            MidiNoteSnapshot {
                id: 7,
                data: take_b_notes[1].data,
            },
        ],
        controllers: Vec::new(),
        pitch_bends: Vec::new(),
        program_changes: Vec::new(),
        poly_aftertouches: Vec::new(),
        channel_pressures: Vec::new(),
        opaque_events: Vec::new(),
        event_order: Vec::new(),
    }];
    let project = Project::from_snapshot(snapshot)?;
    let mut store = ProjectStore::open(&output)?;
    store.save(&project)?;
    store.close()?;
    println!("Wrote {}", output.display());
    Ok(())
}
