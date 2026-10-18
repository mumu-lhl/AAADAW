use aaadaw_core::{DawAction, MidiControllerData, MidiNoteData, MidiProgramChangeData, Project};
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
        name: "Bank Program Select".to_owned(),
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
            off_velocity: 0,
        }],
    })?;
    project.apply(DawAction::SetMidiControllers {
        item_id,
        controllers: vec![
            MidiControllerData {
                controller: 0,
                channel: 2,
                tick: 480,
                value: 12,
            },
            MidiControllerData {
                controller: 32,
                channel: 2,
                tick: 480,
                value: 23,
            },
        ],
    })?;
    project.apply(DawAction::SetMidiProgramChanges {
        item_id,
        program_changes: vec![MidiProgramChangeData {
            channel: 2,
            tick: 480,
            program: 42,
        }],
    })?;

    let mut store = ProjectStore::open(&path)?;
    store.save_with_arrangement_view_state(&project, &ArrangementViewState::default())?;
    store.close()?;
    println!("Wrote {}", path.display());
    Ok(())
}
