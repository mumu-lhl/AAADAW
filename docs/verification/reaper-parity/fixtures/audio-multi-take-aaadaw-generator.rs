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
        "asset://audio-multi-take-a",
        "Hit-01.wav",
        File::open(&media_a)?,
    )?;
    store.import_audio_asset(
        "asset://audio-multi-take-b",
        "Hit-02.wav",
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
    store.set_audio_asset_metadata("asset://audio-multi-take-a", &metadata)?;
    store.set_audio_asset_metadata(
        "asset://audio-multi-take-b",
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
    project.apply(DawAction::InsertAudioItem {
        track_id,
        media_ref: "asset://audio-multi-take-a".to_owned(),
        start_sample: 0,
        source_offset_samples: 0,
        length_samples: 96_000,
    })?;
    let item_id = project.audio_items()[0].id();
    project.apply(DawAction::SetAudioItemName {
        item_id,
        name: "Take A".to_owned(),
    })?;
    project.apply(DawAction::DuplicateAudioItemTake { item_id })?;

    // This is a verification fixture representing two takes produced from two sources.
    // The user-facing insertion action is still open work in the parity matrix.
    let mut snapshot = project.snapshot();
    let alternate = snapshot.audio_items[0]
        .other_takes
        .iter_mut()
        .find(|take| take.index == 0)
        .ok_or("original take should remain at index 0")?;
    alternate.media_ref = "asset://audio-multi-take-b".to_owned();
    alternate.name = "Take B".to_owned();
    project = Project::from_snapshot(snapshot)?;

    store.save_with_arrangement_view_state(&project, &ArrangementViewState::default())?;
    store.close()?;
    println!("Wrote {}", project_path.display());
    Ok(())
}
