use aaadaw_core::{DawAction, Project};
use aaadaw_storage::ProjectStore;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project_path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/reaper782/item-split-cursor-demo.aaadaw"));
    let mut project = Project::new();
    for (index, name) in ["Cursor split A", "Cursor split B"].into_iter().enumerate() {
        project.apply(DawAction::CreateTrack {
            index,
            name: name.to_owned(),
        })?;
    }
    let track_ids = project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    for track_id in track_ids {
        project.apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 19_200,
        })?;
    }

    let mut store = ProjectStore::open(&project_path)?;
    store.save(&project)?;
    store.close()?;
    println!("{}", project_path.display());
    Ok(())
}
