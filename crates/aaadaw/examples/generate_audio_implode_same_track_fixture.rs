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
        ("asset://implode-a", "Source A.wav", &media_a),
        ("asset://implode-b", "Source B.wav", &media_b),
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
    project.apply(DawAction::CreateTrack {
        index: 0,
        name: "Implode takes".to_owned(),
    })?;
    let track_id = project.tracks()[0].id();

    let mut items = Vec::new();
    for (media_ref, name, start_sample) in [
        ("asset://implode-a", "Base A", 0),
        ("asset://implode-b", "Middle B", 48_000),
        ("asset://implode-a", "Late C", 96_000),
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
            .expect("inserted audio item should exist")
            .id();
        project.apply(DawAction::SetAudioItemName {
            item_id,
            name: name.to_owned(),
        })?;
        items.push(item_id);
    }

    let mut base_alternate = project
        .audio_items()
        .iter()
        .find(|item| item.id() == items[0])
        .expect("base item should exist")
        .active_take_snapshot();
    base_alternate.media_ref = "asset://implode-b".to_owned();
    base_alternate.name = "Base B".to_owned();
    project.apply(DawAction::PasteAudioItemAsTake {
        item_id: items[0],
        take: base_alternate,
    })?;

    let mut middle_alternate = project
        .audio_items()
        .iter()
        .find(|item| item.id() == items[1])
        .expect("middle item should exist")
        .active_take_snapshot();
    middle_alternate.media_ref = "asset://implode-a".to_owned();
    middle_alternate.name = "Middle A".to_owned();
    project.apply(DawAction::PasteAudioItemAsTake {
        item_id: items[1],
        take: middle_alternate,
    })?;

    store.save_with_arrangement_view_state(&project, &ArrangementViewState::default())?;
    store.close()?;
    println!("Wrote {}", project_path.display());
    Ok(())
}
