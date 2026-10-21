use aaadaw_core::{DawAction, MidiNoteData, MidiOpaqueEventData, MidiOpaqueEventKind, Project};
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
        name: "Opaque MIDI quantize".to_owned(),
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
                channel: 0,
                pitch: 60,
                tick: 120,
                duration: 240,
                velocity: 96,
                off_velocity: 0,
            },
            MidiNoteData {
                channel: 0,
                pitch: 64,
                tick: 360,
                duration: 240,
                velocity: 96,
                off_velocity: 0,
            },
        ],
    })?;
    project.apply(DawAction::SetMidiOpaqueEvents {
        item_id,
        events: vec![
            MidiOpaqueEventData {
                tick: 475,
                order: 20,
                kind: MidiOpaqueEventKind::Meta {
                    kind: 0x01,
                    data: vec![0xff, 0x00, b't'],
                },
            },
            MidiOpaqueEventData {
                tick: 480,
                order: 10,
                kind: MidiOpaqueEventKind::SystemExclusive {
                    status: 0xF7,
                    data: vec![0x43, 0x12, 0x34],
                },
            },
            MidiOpaqueEventData {
                tick: 595,
                order: 30,
                kind: MidiOpaqueEventKind::SystemExclusive {
                    status: 0xF0,
                    data: vec![0x43, 0x56],
                },
            },
        ],
    })?;
    let mut store = ProjectStore::open(&path)?;
    store.save_with_arrangement_view_state(&project, &ArrangementViewState::default())?;
    Ok(())
}
