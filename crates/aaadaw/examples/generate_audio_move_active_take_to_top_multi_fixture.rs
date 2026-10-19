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
        "asset://audio-move-take-a",
        "Hit-01.wav",
        File::open(&media_a)?,
    )?;
    store.import_audio_asset(
        "asset://audio-move-take-b",
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
    store.set_audio_asset_metadata("asset://audio-move-take-a", &metadata)?;
    store.set_audio_asset_metadata(
        "asset://audio-move-take-b",
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
    let fixtures = [
        (
            "asset://audio-move-take-a",
            "Take A",
            Some(("asset://audio-move-take-b", "Take B")),
            1,
        ),
        ("asset://audio-move-take-a", "Single take", None, 0),
        (
            "asset://audio-move-take-a",
            "Take A",
            Some(("asset://audio-move-take-b", "Take B")),
            0,
        ),
        (
            "asset://audio-move-take-b",
            "Take B",
            Some(("asset://audio-move-take-a", "Take A")),
            1,
        ),
    ];

    for (index, (media_ref, name, alternate, active_take_index)) in fixtures.into_iter().enumerate()
    {
        project.apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: media_ref.to_owned(),
            start_sample: index as u64 * 48_000,
            source_offset_samples: 0,
            length_samples: 48_000,
        })?;
        let item_id = project.audio_items()[index].id();
        project.apply(DawAction::SetAudioItemName {
            item_id,
            name: name.to_owned(),
        })?;
        if let Some((alternate_media_ref, alternate_name)) = alternate {
            project.apply(DawAction::DuplicateAudioItemTake { item_id })?;
            let mut snapshot = project.snapshot();
            let item = snapshot
                .audio_items
                .iter_mut()
                .find(|item| item.id == item_id.value())
                .ok_or("duplicated audio item was not found")?;
            let alternate_take = item
                .other_takes
                .iter_mut()
                .find(|take| take.index == 0)
                .ok_or("duplicate action did not create take index 0")?;
            alternate_take.media_ref = alternate_media_ref.to_owned();
            alternate_take.name = alternate_name.to_owned();
            project = Project::from_snapshot(snapshot)?;
            if active_take_index == 0 {
                project.apply(DawAction::SetAudioItemActiveTake {
                    item_id,
                    take_index: 0,
                })?;
            }
        }
    }

    store.save_with_arrangement_view_state(&project, &ArrangementViewState::default())?;
    store.close()?;
    println!("Wrote {}", project_path.display());
    Ok(())
}
