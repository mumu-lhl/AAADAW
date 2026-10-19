use aaadaw_core::{DawAction, Project};
use aaadaw_storage::{ArrangementViewState, AudioAssetMetadata, ProjectStore};
use std::fs::{self, File};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let project_path = PathBuf::from(arguments.next().ok_or("expected project output path")?);
    let media_a = PathBuf::from(arguments.next().ok_or("expected source A WAV path")?);
    let media_b = PathBuf::from(arguments.next().ok_or("expected source B WAV path")?);
    if arguments.next().is_some() {
        return Err("expected one project path and two WAV paths".into());
    }
    if let Some(parent) = project_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut store = ProjectStore::open(&project_path)?;
    for (media_ref, name, media_path) in [
        ("asset://cross-implode-a", "Source A.wav", &media_a),
        ("asset://cross-implode-b", "Source B.wav", &media_b),
    ] {
        store.import_audio_asset(media_ref, name, File::open(media_path)?)?;
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
                byte_len: Some(fs::metadata(media_path)?.len()),
            },
        )?;
    }

    let mut project = Project::new();
    for (index, name) in ["Top destination", "Middle donor", "Bottom donor"]
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

    project.apply(DawAction::InsertAudioItem {
        track_id: tracks[0],
        media_ref: "asset://cross-implode-a".to_owned(),
        start_sample: 48_000,
        source_offset_samples: 6_000,
        length_samples: 72_000,
    })?;
    let destination_id = project.audio_items()[0].id();
    project.apply(DawAction::SetAudioItemName {
        item_id: destination_id,
        name: "Top destination".to_owned(),
    })?;
    let mut destination_alternate = project.audio_items()[0].active_take_snapshot();
    destination_alternate.media_ref = "asset://cross-implode-b".to_owned();
    destination_alternate.name = "Top alternate".to_owned();
    destination_alternate.source_offset_samples = 12_000;
    destination_alternate.playback_rate = 0.9;
    project.apply(DawAction::PasteAudioItemAsTake {
        item_id: destination_id,
        take: destination_alternate,
    })?;
    project.apply(DawAction::SetAudioItemActiveTake {
        item_id: destination_id,
        take_index: 1,
    })?;

    project.apply(DawAction::InsertAudioItem {
        track_id: tracks[1],
        media_ref: "asset://cross-implode-b".to_owned(),
        start_sample: 0,
        source_offset_samples: 12_000,
        length_samples: 72_000,
    })?;
    let middle_id = project.audio_items()[1].id();
    project.apply(DawAction::SetAudioItemName {
        item_id: middle_id,
        name: "Middle donor".to_owned(),
    })?;
    project.apply(DawAction::SetAudioItemTakePlaybackRate {
        item_id: middle_id,
        rate: 1.1,
    })?;
    let mut middle_alternate = project.audio_items()[1].active_take_snapshot();
    middle_alternate.media_ref = "asset://cross-implode-a".to_owned();
    middle_alternate.name = "Middle alternate".to_owned();
    middle_alternate.source_offset_samples = 24_000;
    middle_alternate.playback_rate = 0.8;
    project.apply(DawAction::PasteAudioItemAsTake {
        item_id: middle_id,
        take: middle_alternate,
    })?;
    project.apply(DawAction::SetAudioItemActiveTake {
        item_id: middle_id,
        take_index: 1,
    })?;

    project.apply(DawAction::InsertAudioItem {
        track_id: tracks[2],
        media_ref: "asset://cross-implode-a".to_owned(),
        start_sample: 96_000,
        source_offset_samples: 18_000,
        length_samples: 60_000,
    })?;
    let bottom_id = project.audio_items()[2].id();
    project.apply(DawAction::SetAudioItemName {
        item_id: bottom_id,
        name: "Bottom donor".to_owned(),
    })?;
    project.apply(DawAction::SetAudioItemTakePlaybackRate {
        item_id: bottom_id,
        rate: 1.2,
    })?;

    store.save_with_arrangement_view_state(&project, &ArrangementViewState::default())?;
    store.close()?;
    println!("Wrote {}", project_path.display());
    Ok(())
}
