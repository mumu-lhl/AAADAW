use aaadaw_core::{DawAction, MidiControllerData, MidiNoteData, MidiPitchBendData, Project};
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
        name: "MIDI channel filter".to_owned(),
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
        notes: vec![
            MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 0,
                duration: 480,
                velocity: 96,
                off_velocity: 0,
            },
            MidiNoteData {
                channel: 1,
                pitch: 62,
                tick: 960,
                duration: 480,
                velocity: 96,
                off_velocity: 0,
            },
            MidiNoteData {
                channel: 2,
                pitch: 64,
                tick: 1_920,
                duration: 480,
                velocity: 96,
                off_velocity: 0,
            },
            MidiNoteData {
                channel: 0,
                pitch: 65,
                tick: 2_880,
                duration: 480,
                velocity: 96,
                off_velocity: 0,
            },
            MidiNoteData {
                channel: 1,
                pitch: 67,
                tick: 3_840,
                duration: 480,
                velocity: 96,
                off_velocity: 0,
            },
            MidiNoteData {
                channel: 2,
                pitch: 69,
                tick: 4_800,
                duration: 480,
                velocity: 96,
                off_velocity: 0,
            },
        ],
    })?;
    project.apply(DawAction::SetMidiControllers {
        item_id,
        controllers: vec![
            MidiControllerData {
                channel: 0,
                controller: 1,
                tick: 0,
                value: 64,
            },
            MidiControllerData {
                channel: 1,
                controller: 1,
                tick: 1_920,
                value: 96,
            },
            MidiControllerData {
                channel: 2,
                controller: 1,
                tick: 3_840,
                value: 32,
            },
        ],
    })?;
    project.apply(DawAction::SetMidiPitchBends {
        item_id,
        pitch_bends: vec![
            MidiPitchBendData {
                channel: 0,
                tick: 0,
                value: 8_192,
            },
            MidiPitchBendData {
                channel: 1,
                tick: 1_920,
                value: 12_288,
            },
            MidiPitchBendData {
                channel: 2,
                tick: 3_840,
                value: 4_096,
            },
        ],
    })?;
    let mut store = ProjectStore::open(&path)?;
    store.save_with_arrangement_view_state(&project, &ArrangementViewState::default())?;
    Ok(())
}
