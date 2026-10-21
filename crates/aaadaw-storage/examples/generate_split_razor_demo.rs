use aaadaw_core::{DawAction, Project, RazorEditArea};
use aaadaw_storage::{AudioAssetMetadata, ProjectStore, StorageError};
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project_path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/reaper-dock-review/split-razor-demo.aaadaw"));
    let media_path = std::env::args_os()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/reaper782/item-copy-loop-source.wav"));
    let media_bytes = fs::read(media_path)?;
    let media_byte_len = media_bytes.len() as u64;
    let mut project = Project::new();
    for (index, name) in [
        "Razor area, no item selection",
        "Time selection, selected item",
        "Time selection, no item selection",
        "Outside razor area",
    ]
    .into_iter()
    .enumerate()
    {
        project.apply(DawAction::CreateTrack {
            index,
            name: name.to_owned(),
        })?;
    }
    let tracks = project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    for track_id in &tracks {
        project.apply(DawAction::InsertAudioItem {
            track_id: *track_id,
            media_ref: "asset://split-razor-demo-source".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 480_000,
        })?;
    }
    let area = RazorEditArea::new(144_000, 336_000).expect("demo area is non-empty");
    project.apply(DawAction::SetTrackRazorEditAreas {
        track_areas: vec![(tracks[0], vec![area])],
    })?;

    let mut store = ProjectStore::open(&project_path)?;
    match store.import_audio_asset(
        "asset://split-razor-demo-source",
        "split-razor-demo-source.wav",
        Cursor::new(media_bytes),
    ) {
        Ok(_) | Err(StorageError::AudioAssetAlreadyExists(_)) => {}
        Err(error) => return Err(error.into()),
    }
    store.set_audio_asset_metadata(
        "asset://split-razor-demo-source",
        &AudioAssetMetadata {
            container: "wav".to_owned(),
            codec: "pcm".to_owned(),
            sample_rate: Some(48_000),
            channel_count: Some(1),
            bits_per_sample: Some(16),
            frame_count: Some(480_000),
            duration_nanos: Some(10_000_000_000),
            byte_len: Some(media_byte_len),
        },
    )?;
    store.save(&project)?;
    store.close()?;
    println!("{}", project_path.display());
    Ok(())
}
