use aaadaw_core::{DawAction, MidiNoteData, Project};
use aaadaw_storage::ProjectStore;
use std::fs;
use std::path::PathBuf;

const ITEM_LENGTH_TICKS: u64 = 7_680;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output_dir = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("expected an output directory")?,
    );
    fs::create_dir_all(&output_dir)?;

    let empty_path = output_dir.join("midi-event-list-empty-end.aaadaw");
    write_project(&empty_path, false)?;
    let populated_path = output_dir.join("midi-event-list-populated-end.aaadaw");
    write_project(&populated_path, true)?;
    println!("Wrote {}", empty_path.display());
    println!("Wrote {}", populated_path.display());
    Ok(())
}

fn write_project(path: &PathBuf, populated: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut project = Project::new();
    project.apply(DawAction::CreateTrack {
        index: 0,
        name: "MIDI Event List".to_owned(),
    })?;
    let track_id = project.tracks()[0].id();
    project.apply(DawAction::InsertMidiItem {
        track_id,
        start_tick: 0,
        length_ticks: ITEM_LENGTH_TICKS,
    })?;

    if populated {
        let item_id = project.midi_items()[0].id();
        project.apply(DawAction::AddMidiNotes {
            item_id,
            notes: (0..4)
                .map(|index| MidiNoteData {
                    channel: index + 1,
                    pitch: [60, 64, 67, 72][index as usize],
                    tick: u64::from(index) * 480,
                    duration: 240,
                    velocity: 91 + index,
                    off_velocity: 0,
                })
                .collect(),
        })?;
    }

    let mut store = ProjectStore::open(path)?;
    store.save(&project)?;
    store.close()?;
    Ok(())
}
