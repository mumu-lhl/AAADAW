use aaadaw_app::{PlaybackBuildError, prepare_audio_playback};
use aaadaw_core::{DawAction, Project};
use aaadaw_storage::ProjectStore;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(0);

fn unique_path(extension: &str) -> PathBuf {
    let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "aaadaw-app-{}-{id}.{extension}",
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

fn project_with_audio_item(media_ref: String, length_samples: u64) -> Project {
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".to_owned(),
        })
        .expect("audio track should be created");
    project
        .apply(DawAction::InsertAudioItem {
            track_id: project.tracks()[0].id(),
            media_ref,
            start_sample: 0,
            source_offset_samples: 0,
            length_samples,
        })
        .expect("audio item should be inserted");
    project
}

fn remove_database(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
}

#[test]
fn prepares_and_renders_an_embedded_audio_item() {
    let database_path = unique_path("aaadaw");
    let wav = pcm_wav(&[-32768, 0, 16384, 32767], 48_000);
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    store
        .import_audio_asset("asset://embedded", "clip.wav", Cursor::new(&wav))
        .expect("WAV should embed");
    let project = project_with_audio_item("asset://embedded".to_owned(), 4);

    let prepared =
        prepare_audio_playback(&project, &store, 64, 8).expect("embedded source should prepare");
    assert_eq!(prepared.feeder_count(), 1);
    assert_eq!(prepared.graph().sample_rate(), 48_000);
    let (mut graph, feeders) = prepared.into_parts();
    for feeder in feeders {
        feeder.join().expect("embedded source should decode");
    }
    graph.transport_mut().start();
    let mut output = [[0.0; 2]; 4];
    let stats = graph
        .render_into(&mut output)
        .expect("prepared graph should render");
    let center_gain = std::f32::consts::FRAC_1_SQRT_2;
    assert_eq!(stats.underrun_samples, 0);
    assert!((output[0][0] + center_gain).abs() < 1.0e-5);
    assert!((output[1][0]).abs() < 1.0e-5);
    assert!((output[2][0] - center_gain * 0.5).abs() < 1.0e-5);

    store.close().expect("project should close");
    remove_database(&database_path);
}

#[test]
fn prepares_external_audio_and_reports_missing_links_before_playback() {
    let database_path = unique_path("aaadaw");
    let source_path = unique_path("wav");
    std::fs::write(&source_path, pcm_wav(&[0, 16384, 0, -16384], 48_000))
        .expect("external fixture should be written");
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    let media_ref = store
        .link_external_audio_file(&source_path)
        .expect("external source should link");
    let project = project_with_audio_item(media_ref.clone(), 4);
    let prepared =
        prepare_audio_playback(&project, &store, 64, 8).expect("linked source should prepare");
    assert_eq!(prepared.feeder_count(), 1);
    drop(prepared);

    std::fs::remove_file(&source_path).expect("external source should be removable");
    assert!(matches!(
        prepare_audio_playback(&project, &store, 64, 8),
        Err(PlaybackBuildError::ExternalSourceUnavailable { media_ref: missing })
            if missing == media_ref
    ));

    store.close().expect("project should close");
    remove_database(&database_path);
}
