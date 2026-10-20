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
        name: "MIDI same-track implode".into(),
    })?;
    let track_id = project.tracks()[0].id();
    let mut item_ids = Vec::new();
    for (start_tick, pitch) in [(0, 60), (3_840, 67), (7_680, 79)] {
        project.apply(DawAction::InsertMidiItem {
            track_id,
            start_tick,
            length_ticks: 11_520,
        })?;
        let item_id = project.midi_items().last().ok_or("MIDI item missing")?.id();
        project.apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![
                MidiNoteData {
                    pitch,
                    channel: 0,
                    tick: 120,
                    duration: 240,
                    velocity: 90,
                    off_velocity: 0,
                },
                MidiNoteData {
                    pitch: pitch + 2,
                    channel: 0,
                    tick: 720,
                    duration: 240,
                    velocity: 84,
                    off_velocity: 0,
                },
            ],
        })?;
        item_ids.push(item_id);
    }

    let mut snapshot = project.snapshot();
    snapshot.midi_items[0].name = "Base A".into();
    snapshot.midi_items[0].source_offset_ticks = 125;
    snapshot.midi_items[1].name = "Donor B active".into();
    snapshot.midi_items[1].active_take_index = 1;
    snapshot.midi_items[1].source_offset_ticks = 375;
    snapshot.midi_items[1].other_takes = vec![MidiItemTakeSnapshot {
        index: 0,
        name: "Donor B".into(),
        source_offset_ticks: 250,
        loop_source: false,
        source_loop_length_ticks: 11_520,
        notes: vec![MidiNoteSnapshot {
            id: 7,
            data: MidiNoteData {
                pitch: 69,
                channel: 1,
                tick: 120,
                duration: 240,
                velocity: 88,
                off_velocity: 4,
            },
        }],
        controllers: Vec::new(),
        pitch_bends: Vec::new(),
        program_changes: Vec::new(),
        poly_aftertouches: Vec::new(),
        channel_pressures: Vec::new(),
        opaque_events: Vec::new(),
        event_order: Vec::new(),
    }];
    snapshot.midi_items[2].name = "Final C".into();
    snapshot.midi_items[2].source_offset_ticks = 500;
    let project = Project::from_snapshot(snapshot)?;
    let mut store = ProjectStore::open(&output)?;
    store.save(&project)?;
    store.close()?;
    println!("Wrote {} from items {:?}", output.display(), item_ids);
    Ok(())
}
