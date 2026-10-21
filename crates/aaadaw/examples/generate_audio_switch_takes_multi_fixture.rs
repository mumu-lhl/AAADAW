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
    for (asset, display_name, path) in [
        ("asset://switch-take-a", "Hit-01.wav", &media_a),
        ("asset://switch-take-b", "Hit-02.wav", &media_b),
    ] {
        store.import_audio_asset(asset, display_name, File::open(path)?)?;
        store.set_audio_asset_metadata(
            asset,
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

    project.apply(DawAction::InsertAudioItem {
        track_id,
        media_ref: "asset://switch-take-a".to_owned(),
        start_sample: 0,
        source_offset_samples: 0,
        length_samples: 48_000,
    })?;
    let first_id = project.audio_items()[0].id();
    project.apply(DawAction::DuplicateAudioItemTake { item_id: first_id })?;
    let mut snapshot = project.snapshot();
    let first_item = snapshot
        .audio_items
        .iter_mut()
        .find(|item| item.id == first_id.value())
        .ok_or("first audio item was not found")?;
    let first_inactive = first_item
        .other_takes
        .iter_mut()
        .find(|take| take.index == 0)
        .ok_or("first item's original take should be inactive")?;
    first_inactive.media_ref = "asset://switch-take-b".to_owned();
    first_inactive.name = "Take B".to_owned();
    project = Project::from_snapshot(snapshot)?;
    project.apply(DawAction::SetAudioItemActiveTake {
        item_id: first_id,
        take_index: 0,
    })?;

    project.apply(DawAction::InsertAudioItem {
        track_id,
        media_ref: "asset://switch-take-a".to_owned(),
        start_sample: 96_000,
        source_offset_samples: 0,
        length_samples: 48_000,
    })?;

    project.apply(DawAction::InsertAudioItem {
        track_id,
        media_ref: "asset://switch-take-a".to_owned(),
        start_sample: 192_000,
        source_offset_samples: 0,
        length_samples: 48_000,
    })?;
    let last_id = project.audio_items()[2].id();
    project.apply(DawAction::DuplicateAudioItemTake { item_id: last_id })?;
    let mut snapshot = project.snapshot();
    let last_item = snapshot
        .audio_items
        .iter_mut()
        .find(|item| item.id == last_id.value())
        .ok_or("last audio item was not found")?;
    let last_original = last_item
        .other_takes
        .iter_mut()
        .find(|take| take.index == 0)
        .ok_or("last item's original take should be inactive")?;
    last_original.media_ref = "asset://switch-take-b".to_owned();
    last_original.name = "Take B".to_owned();
    project = Project::from_snapshot(snapshot)?;
    project.apply(DawAction::DuplicateAudioItemTake { item_id: last_id })?;
    project.apply(DawAction::SetAudioItemName {
        item_id: first_id,
        name: "Take B".to_owned(),
    })?;
    project.apply(DawAction::SetAudioItemName {
        item_id: last_id,
        name: "Take A".to_owned(),
    })?;

    store.save_with_arrangement_view_state(&project, &ArrangementViewState::default())?;
    store.close()?;
    println!("Wrote {}", project_path.display());
    Ok(())
}
