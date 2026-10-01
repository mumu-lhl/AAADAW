use aaadaw_app::{AudioItemImportError, prepare_audio_playback, start_audio_item_import};
use aaadaw_core::{DawAction, Project};
use aaadaw_storage::ProjectStore;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(0);

fn unique_path(extension: &str) -> PathBuf {
    let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "aaadaw-app-import-{}-{id}.{extension}",
        std::process::id()
    ))
}

fn pcm_wav(samples: &[i16], sample_rate: u32) -> Vec<u8> {
    let data_len = u32::try_from(samples.len() * 2).expect("test WAV should fit");
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn remove_database(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
}

#[test]
fn background_import_produces_metadata_and_an_undoable_placement_action() {
    let project_path = unique_path("aaadaw");
    let source_path = unique_path("wav");
    std::fs::write(&source_path, pcm_wav(&vec![1_000; 441], 44_100))
        .expect("source WAV should be written");
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".to_owned(),
        })
        .expect("track should be created");
    let track_id = project.tracks()[0].id();
    let mut store = ProjectStore::open(&project_path).expect("project should open");
    store.save(&project).expect("project should save");
    store.close().expect("project should close");

    let worker = start_audio_item_import(
        &project_path,
        &source_path,
        track_id,
        128,
        project.settings().sample_rate(),
    )
    .expect("background import should start");
    while !worker.is_finished() {
        let _ = worker.progress().try_iter().count();
        std::thread::sleep(Duration::from_millis(1));
    }
    let action = worker.finish().expect("import should finish with metadata");
    let media_ref = match &action {
        DawAction::InsertAudioItem {
            track_id: action_track,
            media_ref,
            start_sample,
            source_offset_samples,
            length_samples,
        } => {
            assert_eq!(*action_track, track_id);
            assert_eq!(*start_sample, 128);
            assert_eq!(*source_offset_samples, 0);
            assert_eq!(*length_samples, 480);
            media_ref.clone()
        }
        other => panic!("expected InsertAudioItem, got {other:?}"),
    };
    project
        .apply(action)
        .expect("placement action should update the project");

    let store = ProjectStore::open(&project_path).expect("project should reopen");
    let metadata = store
        .audio_asset_metadata(&media_ref)
        .expect("metadata query should succeed")
        .expect("imported metadata should be persisted");
    assert_eq!(metadata.sample_rate, Some(44_100));
    assert_eq!(metadata.frame_count, Some(441));
    assert!(store.resolve_audio_asset(&media_ref).is_ok());
    // The test joins feeders before rendering, so give the complete short item queue space.
    let prepared = prepare_audio_playback(&project, &store, 512, 8)
        .expect("imported audio should resolve for playback");
    let (mut graph, feeders) = prepared.into_parts();
    for feeder in feeders {
        feeder.join().expect("imported audio should decode");
    }
    graph.transport_mut().seek_sample(128);
    graph.transport_mut().start();
    let mut output = [[0.0; 2]; 4];
    let stats = graph
        .render_into(&mut output)
        .expect("imported item should render");
    assert_eq!(stats.underrun_samples, 0);
    assert!(output.iter().any(|frame| frame[0] > 0.0));
    store.close().expect("project should close");
    remove_database(&project_path);
    let _ = std::fs::remove_file(source_path);
}

#[test]
fn import_rejects_missing_project_without_creating_a_database() {
    let project_path = unique_path("aaadaw");
    let source_path = unique_path("wav");
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".to_owned(),
        })
        .expect("track should be created");
    let track_id = project.tracks()[0].id();
    let error = match start_audio_item_import(&project_path, &source_path, track_id, 0, 48_000) {
        Ok(worker) => {
            drop(worker);
            panic!("a missing project should be rejected")
        }
        Err(error) => error,
    };
    assert!(matches!(error, AudioItemImportError::ProjectFileMissing(_)));
    assert!(!project_path.exists());
}
