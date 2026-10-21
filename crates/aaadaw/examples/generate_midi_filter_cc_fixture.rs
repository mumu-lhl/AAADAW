use aaadaw_core::{DawAction, MidiControllerData, MidiNoteData, Project};
use aaadaw_storage::{ArrangementViewState, ProjectStore};
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("expected an output project path")?,
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut project = Project::new();
    project.apply(DawAction::CreateTrack {
        index: 0,
        name: "Filter Parameter Value".to_owned(),
    })?;
    let track_id = project.tracks()[0].id();
    project.apply(DawAction::InsertMidiItem {
        track_id,
        start_tick: 0,
        length_ticks: 7_680,
    })?;
    let item_id = project.midi_items()[0].id();
    project.apply(DawAction::AddMidiNotes {
        item_id,
        notes: [(0, 60, 91), (480, 64, 92), (960, 67, 93)]
            .into_iter()
            .map(|(tick, pitch, velocity)| MidiNoteData {
                channel: 0,
                pitch,
                tick,
                duration: 240,
                velocity,
                off_velocity: 0,
            })
            .collect(),
    })?;
    project.apply(DawAction::SetMidiControllers {
        item_id,
        controllers: [(0, 7, 64), (240, 7, 100), (480, 10, 64), (720, 7, 127)]
            .into_iter()
            .map(|(tick, controller, value)| MidiControllerData {
                channel: 0,
                controller,
                tick,
                value,
            })
            .collect(),
    })?;

    let mut store = ProjectStore::open(&path)?;
    store.save_with_arrangement_view_state(&project, &ArrangementViewState::default())?;
    store.close()?;
    println!("Wrote {}", path.display());
    Ok(())
}
