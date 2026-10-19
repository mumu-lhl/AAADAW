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
        "asset://audio-paste-multi-a",
        "Take-A.wav",
        File::open(&media_a)?,
    )?;
    store.import_audio_asset(
        "asset://audio-paste-multi-b",
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
    store.set_audio_asset_metadata("asset://audio-paste-multi-a", &metadata)?;
    store.set_audio_asset_metadata(
        "asset://audio-paste-multi-b",
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
        media_ref: "asset://audio-paste-multi-a".to_owned(),
        start_sample: 0,
        source_offset_samples: 0,
        length_samples: 48_000,
    })?;
    let first_id = project.audio_items()[0].id();
    let mut first_alternate = project.audio_items()[0].active_take_snapshot();
    first_alternate.media_ref = "asset://audio-paste-multi-b".to_owned();
    first_alternate.name = "Take B".to_owned();
    project.apply(DawAction::PasteAudioItemAsTake {
        item_id: first_id,
        take: first_alternate,
    })?;
    project.apply(DawAction::MoveAudioItemActiveTakeToTop { item_id: first_id })?;

    project.apply(DawAction::InsertAudioItem {
        track_id,
        media_ref: "asset://audio-paste-multi-a".to_owned(),
        start_sample: 96_000,
        source_offset_samples: 0,
        length_samples: 48_000,
    })?;
    let single_id = project.audio_items()[1].id();
    project.apply(DawAction::SetAudioItemName {
        item_id: single_id,
        name: "Single take".to_owned(),
    })?;

    project.apply(DawAction::InsertAudioItem {
        track_id,
        media_ref: "asset://audio-paste-multi-b".to_owned(),
        start_sample: 192_000,
        source_offset_samples: 0,
        length_samples: 48_000,
    })?;
    let last_id = project.audio_items()[2].id();
    let mut last_alternate = project.audio_items()[2].active_take_snapshot();
    last_alternate.media_ref = "asset://audio-paste-multi-a".to_owned();
    last_alternate.name = "Take A".to_owned();
    project.apply(DawAction::PasteAudioItemAsTake {
        item_id: last_id,
        take: last_alternate,
    })?;

    project.apply(DawAction::InsertAudioItem {
        track_id,
        media_ref: "asset://audio-paste-multi-b".to_owned(),
        start_sample: 288_000,
        source_offset_samples: 0,
        length_samples: 48_000,
    })?;
    let source_id = project.audio_items()[3].id();
    project.apply(DawAction::SetAudioItemName {
        item_id: source_id,
        name: "Clipboard B".to_owned(),
    })?;

    store.save_with_arrangement_view_state(&project, &ArrangementViewState::default())?;
    store.close()?;
    println!("Wrote {}", project_path.display());
    Ok(())
}
