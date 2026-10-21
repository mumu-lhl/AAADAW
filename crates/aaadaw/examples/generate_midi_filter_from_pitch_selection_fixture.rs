use aaadaw_core::{DawAction, MidiControllerData, MidiNoteData, MidiPitchBendData, Project};
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
        name: "Filter From Pitch Selection".to_owned(),
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
        notes: vec![MidiNoteData {
            channel: 0,
            pitch: 60,
            tick: 0,
            duration: 240,
            velocity: 90,
            off_velocity: 0,
        }],
    })?;
    project.apply(DawAction::SetMidiPitchBends {
        item_id,
        pitch_bends: vec![
            MidiPitchBendData {
                channel: 1,
                tick: 480,
                value: 4_096,
            },
            MidiPitchBendData {
                channel: 1,
                tick: 600,
                value: 10_000,
            },
            MidiPitchBendData {
                channel: 2,
                tick: 960,
                value: 12_288,
            },
        ],
    })?;
    project.apply(DawAction::SetMidiControllers {
        item_id,
        controllers: vec![MidiControllerData {
            channel: 1,
            controller: 7,
            tick: 720,
            value: 64,
        }],
    })?;

    let mut store = ProjectStore::open(&path)?;
    store.save_with_arrangement_view_state(&project, &ArrangementViewState::default())?;
    store.close()?;
    println!("Wrote {}", path.display());
    Ok(())
}
