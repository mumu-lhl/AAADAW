use aaadaw_core::{
    DawAction, MidiChannelPressureData, MidiControllerData, MidiNoteData, MidiOpaqueEventData,
    MidiOpaqueEventKind, MidiProgramChangeData, Project,
};
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
        name: "Filter From Program Channel Pressure Opaque Selection".to_owned(),
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
            channel: 0,
            pitch: 60,
            tick: 0,
            duration: 240,
            velocity: 90,
            off_velocity: 0,
        }],
    })?;
    project.apply(DawAction::SetMidiProgramChanges {
        item_id,
        program_changes: vec![
            MidiProgramChangeData {
                channel: 1,
                tick: 600,
                program: 8,
            },
            MidiProgramChangeData {
                channel: 3,
                tick: 840,
                program: 32,
            },
        ],
    })?;
    project.apply(DawAction::SetMidiChannelPressures {
        item_id,
        channel_pressures: vec![
            MidiChannelPressureData {
                channel: 1,
                tick: 480,
                value: 32,
            },
            MidiChannelPressureData {
                channel: 3,
                tick: 720,
                value: 64,
            },
        ],
    })?;
    project.apply(DawAction::SetMidiControllers {
        item_id,
        controllers: vec![MidiControllerData {
            channel: 1,
            controller: 7,
            tick: 600,
            value: 64,
        }],
    })?;
    project.apply(DawAction::SetMidiOpaqueEvents {
        item_id,
        events: vec![
            MidiOpaqueEventData {
                tick: 720,
                order: 0,
                kind: MidiOpaqueEventKind::Meta {
                    kind: 0x01,
                    data: b"Unselected Text Meta in range".to_vec(),
                },
            },
            MidiOpaqueEventData {
                tick: 960,
                order: 1,
                kind: MidiOpaqueEventKind::Meta {
                    kind: 0x01,
                    data: b"Selected Text Meta".to_vec(),
                },
            },
            MidiOpaqueEventData {
                tick: 1_500,
                order: 2,
                kind: MidiOpaqueEventKind::SystemExclusive {
                    status: 0xF0,
                    data: vec![0x43, 0x12, 0xF7],
                },
            },
        ],
    })?;

    let mut store = ProjectStore::open(&path)?;
    store.save_with_arrangement_view_state(&project, &ArrangementViewState::default())?;
    store.close()?;
    println!("Wrote {}", path.display());
    Ok(())
}
