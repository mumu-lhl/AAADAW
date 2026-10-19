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
        ("asset://explode-a", "Explode A.wav", &media_a),
        ("asset://explode-b", "Explode B.wav", &media_b),
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
    for (index, name) in ["Top multi-take", "Bottom multi-take"]
        .into_iter()
        .enumerate()
    {
        project.apply(DawAction::CreateTrack {
            index,
            name: name.to_owned(),
        })?;
    }
    let top_track = project.tracks()[0].id();
    let bottom_track = project.tracks()[1].id();

    project.apply(DawAction::InsertAudioItem {
        track_id: top_track,
        media_ref: "asset://explode-a".to_owned(),
        start_sample: 288_000,
        source_offset_samples: 0,
        length_samples: 96_000,
    })?;
    let top_id = project.audio_items()[0].id();
    project.apply(DawAction::SetAudioItemName {
        item_id: top_id,
        name: "Top take A".to_owned(),
    })?;
    project.apply(DawAction::SetAudioItemTakePlaybackRate {
        item_id: top_id,
        rate: 0.8,
    })?;
    for (media_ref, name, offset, rate) in [
        ("asset://explode-b", "Top take B", 6_000, 1.0),
        ("asset://explode-a", "Top take C", 12_000, 1.2),
    ] {
        let mut take = project.audio_items()[0].active_take_snapshot();
        take.media_ref = media_ref.to_owned();
        take.name = name.to_owned();
        take.source_offset_samples = offset;
        take.playback_rate = rate;
        project.apply(DawAction::PasteAudioItemAsTake {
            item_id: top_id,
            take,
        })?;
    }
    project.apply(DawAction::SetAudioItemActiveTake {
        item_id: top_id,
        take_index: 1,
    })?;

    project.apply(DawAction::InsertAudioItem {
        track_id: bottom_track,
        media_ref: "asset://explode-b".to_owned(),
        start_sample: 192_000,
        source_offset_samples: 3_000,
        length_samples: 72_000,
    })?;
    let bottom_id = project.audio_items()[1].id();
    project.apply(DawAction::SetAudioItemName {
        item_id: bottom_id,
        name: "Bottom take A".to_owned(),
    })?;
    let mut bottom_alternate = project.audio_items()[1].active_take_snapshot();
    bottom_alternate.media_ref = "asset://explode-a".to_owned();
    bottom_alternate.name = "Bottom take B".to_owned();
    bottom_alternate.source_offset_samples = 9_000;
    bottom_alternate.playback_rate = 1.1;
    project.apply(DawAction::PasteAudioItemAsTake {
        item_id: bottom_id,
        take: bottom_alternate,
    })?;

    project.apply(DawAction::InsertAudioItem {
        track_id: bottom_track,
        media_ref: "asset://explode-a".to_owned(),
        start_sample: 384_000,
        source_offset_samples: 0,
        length_samples: 48_000,
    })?;
    let single_id = project.audio_items()[2].id();
    project.apply(DawAction::SetAudioItemName {
        item_id: single_id,
        name: "Single-take control".to_owned(),
    })?;

    store.save_with_arrangement_view_state(&project, &ArrangementViewState::default())?;
    store.close()?;
    println!("Wrote {}", project_path.display());
    Ok(())
}
