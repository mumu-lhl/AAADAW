use aaadaw_core::{DawAction, MidiNoteData, Project};
use aaadaw_storage::ProjectStore;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/item-mute-demo.aaadaw"));
    let mut project = Project::new();
    project.apply(DawAction::CreateTrack {
        index: 0,
        name: "Mute probe".to_owned(),
    })?;
    project.apply(DawAction::InsertMidiItem {
        track_id: project.tracks()[0].id(),
        start_tick: 0,
        length_ticks: 3_840,
    })?;
    let item_id = project.midi_items()[0].id();
    project.apply(DawAction::AddMidiNotes {
        item_id,
        notes: vec![MidiNoteData {
            pitch: 60,
            tick: 0,
            duration: 960,
            velocity: 100,
        }],
    })?;
    project.apply(DawAction::SetMidiItemMute {
        item_id,
        muted: true,
    })?;

    let mut store = ProjectStore::open(&path)?;
    store.save(&project)?;
    store.close()?;
    println!("{}", path.display());
    Ok(())
}
