use aaadaw_core::{DawAction, Project};
use aaadaw_storage::ProjectStore;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("aaadaw-render-bounds-fixture.aaadaw"));
    let mut project = Project::new();
    project.apply(DawAction::CreateTrack {
        index: 0,
        name: "Render fixture".to_owned(),
    })?;
    let track_id = project.tracks()[0].id();
    project.apply(DawAction::InsertMidiItem {
        track_id,
        start_tick: 0,
        length_ticks: 4_800,
    })?;
    for (start_ticks, end_ticks, name) in [
        (0, 960, "Region 1"),
        (1_920, 2_880, "Region 2"),
        (3_840, 4_800, "Region 3"),
    ] {
        project.apply(DawAction::InsertProjectRegion {
            start_ticks,
            end_ticks,
            name: name.to_owned(),
        })?;
    }
    for (position_ticks, name) in [(0, "Marker 1"), (1_920, "Marker 2"), (3_840, "Marker 3")] {
        project.apply(DawAction::InsertProjectMarker {
            position_ticks,
            name: name.to_owned(),
        })?;
    }

    let mut store = ProjectStore::open(&path)?;
    store.save(&project)?;
    store.close()?;
    println!("{}", path.display());
    Ok(())
}
