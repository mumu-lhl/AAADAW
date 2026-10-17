use aaadaw_core::{DawAction, Project};
use aaadaw_storage::{AudioAssetMetadata, ProjectStore};
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let project_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/item-hover-audio-demo.aaadaw"));
    let media_path = args.next().map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(
            "docs/verification/reaper-parity/fixtures/item-hover-actionlist-target-audio.wav",
        )
    });
    let audio_start_sample = args
        .next()
        .map(|value| value.to_string_lossy().parse::<u64>())
        .transpose()?
        .unwrap_or(192_000);
    let media_bytes = fs::read(&media_path)?;
    let byte_len = media_bytes.len() as u64;
    let mut project = Project::new();
    for (index, name) in ["Track A", "Track B"].into_iter().enumerate() {
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
    project.apply(DawAction::InsertMidiItem {
        track_id: tracks[0],
        start_tick: 3_840,
        length_ticks: 960,
    })?;
    project.apply(DawAction::InsertAudioItem {
        track_id: tracks[1],
        media_ref: "asset://item-hover-audio-demo".to_owned(),
        start_sample: audio_start_sample,
        source_offset_samples: 0,
        length_samples: 48_000,
    })?;

    let mut store = ProjectStore::open(&project_path)?;
    store.import_audio_asset(
        "asset://item-hover-audio-demo",
        "item-hover-actionlist-target-audio.wav",
        Cursor::new(media_bytes),
    )?;
    store.set_audio_asset_metadata(
        "asset://item-hover-audio-demo",
        &AudioAssetMetadata {
            container: "wav".to_owned(),
            codec: "pcm".to_owned(),
            sample_rate: Some(48_000),
            channel_count: Some(1),
            bits_per_sample: Some(16),
            frame_count: Some(48_000),
            duration_nanos: Some(1_000_000_000),
            byte_len: Some(byte_len),
        },
    )?;
    store.save(&project)?;
    store.close()?;
    println!("{}", project_path.display());
    Ok(())
}
