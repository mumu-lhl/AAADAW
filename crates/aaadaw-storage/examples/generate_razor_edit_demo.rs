use aaadaw_core::{DawAction, Project, RazorEditArea};
use aaadaw_storage::ProjectStore;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project_path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from("target/reaper-dock-review/item-enclose-razor-demo.aaadaw")
        });
    let media_path = std::env::args_os()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/reaper782/item-copy-loop-source.wav"));
    let media_bytes = fs::read(media_path)?;
    let mut project = Project::new();
    project.apply(DawAction::CreateTrack {
        index: 0,
        name: "Selected audio A".to_owned(),
    })?;
    project.apply(DawAction::CreateTrack {
        index: 1,
        name: "Selected audio B".to_owned(),
    })?;
    project.apply(DawAction::CreateTrack {
        index: 2,
        name: "Unselected track".to_owned(),
    })?;
    let track_a = project.tracks()[0].id();
    let track_b = project.tracks()[1].id();
    let track_c = project.tracks()[2].id();
    for (track_id, start_sample, length_samples) in [
        (track_a, 96_000, 96_000),
        (track_a, 288_000, 48_000),
        (track_a, 672_000, 96_000),
        (track_b, 192_000, 96_000),
    ] {
        project.apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://razor-edit-demo-source".to_owned(),
            start_sample,
            source_offset_samples: 0,
            length_samples,
        })?;
    }
    project.apply(DawAction::InsertMidiItem {
        track_id: track_a,
        start_tick: 19_200,
        length_ticks: 3_840,
    })?;
    let area = RazorEditArea::new(96_000, 576_000).expect("the demo area is non-empty");
    project.apply(DawAction::SetTrackRazorEditAreas {
        track_areas: vec![
            (track_a, vec![area]),
            (track_b, vec![area]),
            (track_c, Vec::new()),
        ],
    })?;

    let mut store = ProjectStore::open(&project_path)?;
    store.import_audio_asset(
        "asset://razor-edit-demo-source",
        "razor-edit-demo-source.wav",
        Cursor::new(media_bytes),
    )?;
    store.save(&project)?;
    store.close()?;
    println!("{}", project_path.display());
    Ok(())
}
