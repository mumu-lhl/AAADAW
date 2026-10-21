use aaadaw_core::{DawAction, Project};
use aaadaw_storage::{ArrangementViewState, AudioAssetMetadata, ProjectStore};
use std::fs::{self, File};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let project_path = PathBuf::from(arguments.next().ok_or("expected a project output path")?);
    let media_a = PathBuf::from(arguments.next().ok_or("expected Take A WAV path")?);
    let media_b = PathBuf::from(arguments.next().ok_or("expected Take B WAV path")?);
    if arguments.next().is_some() {
        return Err("expected only a project path and two WAV paths".into());
    }
    if let Some(parent) = project_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut store = ProjectStore::open(&project_path)?;
    store.import_audio_asset(
        "asset://audio-duplicate-active-take-a",
        "Take-A.wav",
        File::open(&media_a)?,
    )?;
    store.import_audio_asset(
        "asset://audio-duplicate-active-take-b",
        "Take-B.wav",
        File::open(&media_b)?,
    )?;
    let metadata = AudioAssetMetadata {
        container: "wav".to_owned(),
        codec: "pcm".to_owned(),
        sample_rate: Some(48_000),
        channel_count: Some(2),
        bits_per_sample: Some(24),
        frame_count: Some(48_000),
        duration_nanos: Some(1_000_000_000),
        byte_len: Some(fs::metadata(&media_a)?.len()),
    };
    store.set_audio_asset_metadata("asset://audio-duplicate-active-take-a", &metadata)?;
    store.set_audio_asset_metadata(
        "asset://audio-duplicate-active-take-b",
        &AudioAssetMetadata {
            byte_len: Some(fs::metadata(&media_b)?.len()),
            ..metadata
        },
    )?;

    let mut project = Project::new();
    project.apply(DawAction::CreateTrack {
        index: 0,
        name: "Audio Takes".to_owned(),
    })?;
    let track_id = project.tracks()[0].id();
    let targets = [
        (0, "First take active", Some(0)),
        (96_000, "Single take", None),
        (192_000, "Last take active", Some(1)),
    ];

    for (index, (start_sample, name, active_take_index)) in targets.into_iter().enumerate() {
        project.apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://audio-duplicate-active-take-a".to_owned(),
            start_sample,
            source_offset_samples: 0,
            length_samples: 96_000,
        })?;
        let item_id = project.audio_items()[index].id();
        project.apply(DawAction::SetAudioItemName {
            item_id,
            name: name.to_owned(),
        })?;
        if let Some(active_take_index) = active_take_index {
            project.apply(DawAction::DuplicateAudioItemTake { item_id })?;
            let mut snapshot = project.snapshot();
            let item = snapshot
                .audio_items
                .iter_mut()
                .find(|item| item.id == item_id.value())
                .ok_or("duplicated audio item was not found")?;
            let original = item
                .other_takes
                .iter_mut()
                .find(|take| take.index == 0)
                .ok_or("original take should remain at index 0")?;
            original.media_ref = "asset://audio-duplicate-active-take-b".to_owned();
            original.name = "Take B".to_owned();
            item.name = "Take A".to_owned();
            project = Project::from_snapshot(snapshot)?;
            project.apply(DawAction::SetAudioItemActiveTake {
                item_id,
                take_index: active_take_index,
            })?;
        }
    }

    store.save_with_arrangement_view_state(&project, &ArrangementViewState::default())?;
    store.close()?;
    println!("Wrote {}", project_path.display());
    Ok(())
}
