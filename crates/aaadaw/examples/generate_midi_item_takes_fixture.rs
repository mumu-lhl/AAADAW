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
        name: "MIDI takes".into(),
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
        notes: vec![MidiNoteData {
            pitch: 72,
            channel: 0,
            tick: 0,
            duration: 1_920,
            velocity: 100,
            off_velocity: 0,
        }],
    })?;

    let mut snapshot = project.snapshot();
    let item = &mut snapshot.midi_items[0];
    item.active_take_index = 1;
    item.name = "MIDI take B".into();
    item.other_takes = vec![MidiItemTakeSnapshot {
        index: 0,
        name: "MIDI take A".into(),
        source_offset_ticks: 0,
        loop_source: false,
        source_loop_length_ticks: item.length_ticks,
        notes: vec![MidiNoteSnapshot {
            id: 2,
            data: MidiNoteData {
                pitch: 60,
                channel: 0,
                tick: 0,
                duration: 960,
                velocity: 92,
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
    }];
    let project = Project::from_snapshot(snapshot)?;
    let mut store = ProjectStore::open(&output)?;
    store.save(&project)?;
    store.close()?;
    println!("Wrote {}", output.display());
    Ok(())
}
