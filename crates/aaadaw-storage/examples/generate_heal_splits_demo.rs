use aaadaw_core::{DawAction, Project};
use aaadaw_storage::{AudioAssetMetadata, ProjectStore};
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project_path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/reaper782/item-heal-splits-demo.aaadaw"));
    let media_path = std::env::args_os()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/reaper782/item-copy-loop-source.wav"));
    let media_bytes = fs::read(&media_path)?;
    let mut project = Project::new();
    project.apply(DawAction::CreateTrack {
        index: 0,
        name: "Selected split audio".to_owned(),
    })?;
    let track_id = project.tracks()[0].id();
    for (start_sample, source_offset_samples) in [(0, 0), (240_000, 240_000)] {
        project.apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://heal-splits-demo-source".to_owned(),
            start_sample,
            source_offset_samples,
            length_samples: 240_000,
        })?;
    }

    let mut store = ProjectStore::open(&project_path)?;
    store.import_audio_asset(
        "asset://heal-splits-demo-source",
        "heal-splits-demo-source.wav",
        Cursor::new(media_bytes),
    )?;
    store.set_audio_asset_metadata(
        "asset://heal-splits-demo-source",
        &AudioAssetMetadata {
            container: "wav".to_owned(),
            codec: "pcm".to_owned(),
            sample_rate: Some(48_000),
            channel_count: Some(1),
            bits_per_sample: Some(16),
            frame_count: Some(480_000),
            duration_nanos: Some(10_000_000_000),
            byte_len: Some(fs::metadata(media_path)?.len()),
        },
    )?;
    store.save(&project)?;
    store.close()?;
    println!("{}", project_path.display());
    Ok(())
}
