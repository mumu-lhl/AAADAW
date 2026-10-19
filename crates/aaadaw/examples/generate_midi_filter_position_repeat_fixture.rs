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
        name: "Filter Position Repeat".to_owned(),
    })?;
    let track_id = project.tracks()[0].id();
    project.apply(DawAction::InsertMidiItem {
        track_id,
        start_tick: 0,
        length_ticks: 15_360,
    })?;
    let item_id = project.midi_items()[0].id();
    project.apply(DawAction::AddMidiNotes {
        item_id,
        notes: [
            (60, 60),
            (180, 62),
            (420, 64),
            (3_900, 65),
            (7_740, 67),
            (11_580, 69),
        ]
        .into_iter()
        .map(|(tick, pitch)| MidiNoteData {
            channel: 0,
            pitch,
            tick,
            duration: 120,
            velocity: 90,
            off_velocity: 0,
        })
        .collect(),
    })?;
    project.apply(DawAction::SetMidiControllers {
        item_id,
        controllers: (0..=15_240)
            .step_by(120)
            .map(|tick| MidiControllerData {
                channel: 0,
                controller: 7,
                tick,
                value: 64,
            })
            .collect(),
    })?;

    let mut store = ProjectStore::open(&path)?;
    store.save_with_arrangement_view_state(&project, &ArrangementViewState::default())?;
    store.close()?;
    println!("Wrote {}", path.display());
    Ok(())
}
