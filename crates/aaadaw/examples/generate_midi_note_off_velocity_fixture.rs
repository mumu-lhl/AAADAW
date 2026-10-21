use aaadaw_core::{DawAction, MidiNoteData, Project};
use aaadaw_storage::{ArrangementViewState, ProjectStore};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("expected an output path")?,
    );
    let mut project = Project::new();
    project.apply(DawAction::CreateTrack {
        index: 0,
        name: "Note Off velocity probe".to_owned(),
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
            channel: 2,
            pitch: 60,
            tick: 0,
            duration: 240,
            velocity: 91,
            off_velocity: 37,
        }],
    })?;

    let mut store = ProjectStore::open(&path)?;
    store.save_with_arrangement_view_state(&project, &ArrangementViewState::default())?;
    store.close()?;
    println!("Wrote {}", path.display());
    Ok(())
}
