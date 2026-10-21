use aaadaw_core::{DawAction, Project};
use aaadaw_storage::{ArrangementViewState, AudioAssetMetadata, ProjectStore};
use std::fs::{self, File};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let project_path = PathBuf::from(arguments.next().ok_or("expected a project output path")?);
    let target_media = PathBuf::from(arguments.next().ok_or("expected target WAV path")?);
    let source_media = PathBuf::from(arguments.next().ok_or("expected source WAV path")?);
    if arguments.next().is_some() {
        return Err("expected only a project path and two WAV paths".into());
    }
    if let Some(parent) = project_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut store = ProjectStore::open(&project_path)?;
    store.import_audio_asset(
        "asset://paste-as-takes-target",
        "Target-A.wav",
        File::open(&target_media)?,
    )?;
    store.import_audio_asset(
        "asset://paste-as-takes-source",
        "Source-B.wav",
        File::open(&source_media)?,
    )?;
    for (media_ref, path) in [
        ("asset://paste-as-takes-target", &target_media),
        ("asset://paste-as-takes-source", &source_media),
    ] {
        store.set_audio_asset_metadata(
            media_ref,
            &AudioAssetMetadata {
                container: "wav".to_owned(),
                codec: "pcm".to_owned(),
                sample_rate: Some(48_000),
                channel_count: Some(2),
                bits_per_sample: Some(24),
                frame_count: Some(48_000),
                duration_nanos: Some(1_000_000_000),
                byte_len: Some(fs::metadata(path)?.len()),
            },
        )?;
    }

    let mut project = Project::new();
    project.apply(DawAction::CreateTrack {
        index: 0,
        name: "Audio Takes".to_owned(),
    })?;
    let track_id = project.tracks()[0].id();
    for (media_ref, name, start_sample) in [
        ("asset://paste-as-takes-target", "Target A", 0),
        ("asset://paste-as-takes-source", "Source B", 96_000),
    ] {
        project.apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: media_ref.to_owned(),
            start_sample,
            source_offset_samples: 0,
            length_samples: 48_000,
        })?;
        let item_id = project
            .audio_items()
            .last()
            .ok_or("inserted audio item should exist")?
            .id();
        project.apply(DawAction::SetAudioItemName {
            item_id,
            name: name.to_owned(),
        })?;
    }

    store.save_with_arrangement_view_state(&project, &ArrangementViewState::default())?;
    store.close()?;
    println!("Wrote {}", project_path.display());
    Ok(())
}
