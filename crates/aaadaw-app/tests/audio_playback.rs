use aaadaw_app::{
    PlaybackBuildError, prepare_audio_playback, prepare_audio_playback_at,
    render_prepared_audio_to_pcm24_wav,
};
use aaadaw_core::{DawAction, Project};
use aaadaw_storage::ProjectStore;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
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

fn stereo_pcm_wav(frames: &[[i16; 2]], sample_rate: u32) -> Vec<u8> {
    let data_len = u32::try_from(frames.len() * 4).expect("test WAV should fit");
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 4).to_le_bytes());
    bytes.extend_from_slice(&4_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for frame in frames {
        for sample in frame {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
    }
    bytes
}

fn project_with_audio_item(media_ref: String, length_samples: u64) -> Project {
    project_with_audio_item_at(media_ref, 0, length_samples)
}

fn project_with_audio_item_at(
    media_ref: String,
    start_sample: u64,
    length_samples: u64,
) -> Project {
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
            start_sample,
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
fn repeated_embedded_source_reuses_cached_pcm_and_preserves_each_trim() {
    let database_path = unique_path("aaadaw");
    let frames = [
        [4096, -4096],
        [8192, -8192],
        [12_288, -12_288],
        [16_384, -16_384],
    ];
    let wav = stereo_pcm_wav(&frames, 48_000);
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    store
        .import_audio_asset(
            "asset://shared-short-source",
            "shared.wav",
            Cursor::new(&wav),
        )
        .expect("WAV should embed");
    let mut project = project_with_audio_item("asset://shared-short-source".to_owned(), 4);
    project
        .apply(DawAction::InsertAudioItem {
            track_id: project.tracks()[0].id(),
            media_ref: "asset://shared-short-source".to_owned(),
            start_sample: 0,
            source_offset_samples: 1,
            length_samples: 3,
        })
        .expect("second item should be inserted");

    let prepared = prepare_audio_playback(&project, &store, 16, 4)
        .expect("repeated embedded source should prepare");
    assert_eq!(prepared.feeder_count(), 2);
    let (mut graph, feeders) = prepared.into_parts();
    for feeder in feeders {
        feeder.join().expect("cached source feeder should finish");
    }
    graph.transport_mut().start();
    let mut output = [[0.0; 2]; 4];
    let stats = graph
        .render_into(&mut output)
        .expect("cached source should render");
    assert_eq!(stats.underrun_samples, 0);
    assert!((output[0][0] - 0.375).abs() < 1.0e-5);
    assert!((output[1][0] - 0.625).abs() < 1.0e-5);
    assert!((output[2][0] - 0.875).abs() < 1.0e-5);
    assert!((output[3][0] - 0.5).abs() < 1.0e-5);
    assert_eq!(output.map(|frame| frame[1]), output.map(|frame| -frame[0]));
    drop(graph);

    let prepared = prepare_audio_playback_at(&project, &store, 1, 16, 4)
        .expect("cached source should prepare again after a seek");
    let (mut graph, feeders) = prepared.into_parts();
    for feeder in feeders {
        feeder.join().expect("cached seek feeder should finish");
    }
    graph.transport_mut().start();
    let mut seek_output = [[0.0; 2]; 3];
    let seek_stats = graph
        .render_into(&mut seek_output)
        .expect("cached source should render after a seek");
    assert_eq!(seek_stats.underrun_samples, 0);
    assert!((seek_output[0][0] - 0.625).abs() < 1.0e-5);
    assert!((seek_output[1][0] - 0.875).abs() < 1.0e-5);
    assert!((seek_output[2][0] - 0.5).abs() < 1.0e-5);
    drop(graph);

    store.close().expect("project should close");
    remove_database(&database_path);
}

#[test]
fn oversized_repeated_embedded_source_falls_back_to_streaming() {
    let database_path = unique_path("aaadaw");
    let mut samples = vec![0; 2_097_153];
    samples[..4].copy_from_slice(&[4096, 8192, 12_288, 16_384]);
    let wav = pcm_wav(&samples, 48_000);
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    store
        .import_audio_asset(
            "asset://shared-large-source",
            "large.wav",
            Cursor::new(&wav),
        )
        .expect("large WAV should embed");
    let mut project = project_with_audio_item("asset://shared-large-source".to_owned(), 4);
    project
        .apply(DawAction::InsertAudioItem {
            track_id: project.tracks()[0].id(),
            media_ref: "asset://shared-large-source".to_owned(),
            start_sample: 0,
            source_offset_samples: 1,
            length_samples: 3,
        })
        .expect("second item should be inserted");

    let prepared = prepare_audio_playback(&project, &store, 16, 4)
        .expect("oversized repeated source should prepare through streaming");
    assert_eq!(prepared.feeder_count(), 2);
    let (mut graph, feeders) = prepared.into_parts();
    for feeder in feeders {
        feeder
            .join()
            .expect("streaming source feeder should finish");
    }
    graph.transport_mut().start();
    let mut output = [[0.0; 2]; 4];
    let stats = graph
        .render_into(&mut output)
        .expect("streaming fallback should render");
    assert_eq!(stats.underrun_samples, 0);
    let center_gain = std::f32::consts::FRAC_1_SQRT_2;
    assert!((output[0][0] - center_gain * 0.375).abs() < 1.0e-5);
    assert!((output[1][0] - center_gain * 0.625).abs() < 1.0e-5);
    assert!((output[2][0] - center_gain * 0.875).abs() < 1.0e-5);
    assert!((output[3][0] - center_gain * 0.5).abs() < 1.0e-5);
    assert_eq!(output.map(|frame| frame[1]), output.map(|frame| frame[0]));
    drop(graph);

    let prepared = prepare_audio_playback_at(&project, &store, 1, 16, 4)
        .expect("oversized source should stream after seeking");
    let (mut graph, feeders) = prepared.into_parts();
    for feeder in feeders {
        feeder.join().expect("streaming seek feeder should finish");
    }
    graph.transport_mut().start();
    let mut seek_output = [[0.0; 2]; 3];
    let seek_stats = graph
        .render_into(&mut seek_output)
        .expect("streaming fallback should render after seeking");
    assert_eq!(seek_stats.underrun_samples, 0);
    assert!((seek_output[0][0] - center_gain * 0.625).abs() < 1.0e-5);
    assert!((seek_output[1][0] - center_gain * 0.875).abs() < 1.0e-5);
    assert!((seek_output[2][0] - center_gain * 0.5).abs() < 1.0e-5);
    assert_eq!(
        seek_output.map(|frame| frame[1]),
        seek_output.map(|frame| frame[0])
    );
    drop(graph);

    store.close().expect("project should close");
    remove_database(&database_path);
}

#[test]
fn stereo_audio_item_keeps_channel_separation_in_playback_and_offline_export() {
    let database_path = unique_path("aaadaw");
    let export_path = unique_path("wav");
    let wav = stereo_pcm_wav(&[[16384, -8192], [8192, -16384]], 24_000);
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    store
        .import_audio_asset("asset://stereo", "stereo.wav", Cursor::new(&wav))
        .expect("stereo WAV should embed");
    let project = project_with_audio_item("asset://stereo".to_owned(), 4);

    let prepared = prepare_audio_playback(&project, &store, 16, 4)
        .expect("embedded stereo source should prepare");
    let (mut graph, feeders) = prepared.into_parts();
    for feeder in feeders {
        feeder.join().expect("stereo source should decode");
    }
    graph.transport_mut().start();
    let mut output = [[0.0; 2]; 4];
    let stats = graph
        .render_into(&mut output)
        .expect("stereo source should render");
    assert_eq!(stats.underrun_samples, 0);
    assert!((output[0][0] - 0.5).abs() < 1.0e-5);
    assert!((output[0][1] + 0.25).abs() < 1.0e-5);
    assert!((output[1][0] - 0.375).abs() < 1.0e-5);
    assert!((output[1][1] + 0.375).abs() < 1.0e-5);
    assert!((output[2][0] - 0.25).abs() < 1.0e-5);
    assert!((output[2][1] + 0.5).abs() < 1.0e-5);
    assert_eq!(output[2], output[3]);

    let prepared = prepare_audio_playback_at(&project, &store, 2, 8, 4)
        .expect("stereo seek refill should prepare");
    let (mut graph, feeders) = prepared.into_parts();
    for feeder in feeders {
        feeder.join().expect("stereo refill should decode");
    }
    graph.transport_mut().start();
    let mut seek_output = [[0.0; 2]; 2];
    let seek_stats = graph
        .render_into(&mut seek_output)
        .expect("stereo seek refill should render");
    assert_eq!(seek_stats.underrun_samples, 0);
    assert!((seek_output[0][0] - 0.25).abs() < 1.0e-5);
    assert!((seek_output[0][1] + 0.5).abs() < 1.0e-5);
    assert_eq!(seek_output[0], seek_output[1]);

    let prepared = prepare_audio_playback(&project, &store, 1, 4)
        .expect("stereo source should prepare for offline export");
    store.close().expect("project should close before export");
    render_prepared_audio_to_pcm24_wav(
        prepared,
        &project,
        &export_path,
        4,
        &AtomicBool::new(false),
        |_, _| {},
    )
    .expect("offline render should finish");
    let mut decoder =
        aaadaw_media::AudioStreamDecoder::open(&export_path).expect("export should be a valid WAV");
    let chunk = decoder
        .next_chunk()
        .expect("export should decode")
        .expect("export should contain audio");
    assert_eq!(chunk.channels(), 2);
    let exported = chunk.samples();
    assert!(exported[0] > 0.49 && exported[1] < -0.24);
    assert!(exported[2] > 0.37 && exported[3] < -0.37);
    assert!(exported[4] > 0.24 && exported[5] < -0.49);

    std::fs::remove_file(export_path).expect("export should be removed");
    remove_database(&database_path);
}

#[test]
fn offline_render_waits_for_bounded_media_feeders_and_exports_the_mix() {
    let database_path = unique_path("aaadaw");
    let export_path = unique_path("wav");
    let wav = pcm_wav(&[16384, 0, -16384, 32767], 48_000);
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    store
        .import_audio_asset("asset://offline", "offline.wav", Cursor::new(&wav))
        .expect("WAV should embed");
    let project = project_with_audio_item("asset://offline".to_owned(), 4);
    let prepared =
        prepare_audio_playback(&project, &store, 1, 4).expect("embedded source should prepare");
    store.close().expect("project should close");
    let cancel = AtomicBool::new(false);
    let mut progress = Vec::new();

    render_prepared_audio_to_pcm24_wav(
        prepared,
        &project,
        &export_path,
        4,
        &cancel,
        |done, total| progress.push((done, total)),
    )
    .expect("offline render should finish");

    let mut decoder =
        aaadaw_media::AudioStreamDecoder::open(&export_path).expect("export should be a valid WAV");
    assert_eq!(decoder.metadata().sample_rate, Some(48_000));
    assert_eq!(decoder.metadata().channel_count, Some(2));
    let mut decoded_frames = 0;
    while let Some(chunk) = decoder.next_chunk().expect("WAV should decode") {
        assert_eq!(chunk.channels(), 2);
        decoded_frames += chunk.frame_count();
    }
    assert_eq!(decoded_frames, 4);
    assert_eq!(progress, [(0, 4), (1, 4), (2, 4), (3, 4), (4, 4)]);
    std::fs::remove_file(export_path).expect("test export should be removed");
    remove_database(&database_path);
}

#[test]
fn prepares_audio_refill_at_seek_sample_and_starts_transport_there() {
    let database_path = unique_path("aaadaw");
    let wav = pcm_wav(&[0, 8192, 16384, 24576], 48_000);
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    store
        .import_audio_asset("asset://seekable", "seek.wav", Cursor::new(&wav))
        .expect("WAV should embed");
    let project = project_with_audio_item_at("asset://seekable".to_owned(), 10, 4);

    let mut prepared = prepare_audio_playback_at(&project, &store, 12, 64, 8)
        .expect("seek position should prepare a refilled stream");
    assert_eq!(prepared.graph_mut().transport_mut().position_samples(), 12);
    let (mut graph, feeders) = prepared.into_parts();
    for feeder in feeders {
        feeder.join().expect("refilled audio should decode");
    }
    graph.transport_mut().start();
    let mut output = [[0.0; 2]; 3];
    let stats = graph
        .render_into(&mut output)
        .expect("seek-position graph should render");
    let center_gain = std::f32::consts::FRAC_1_SQRT_2;
    assert_eq!(stats.underrun_samples, 0);
    assert!((output[0][0] - center_gain * 0.5).abs() < 1.0e-6);
    assert!((output[1][0] - center_gain * 0.75).abs() < 1.0e-6);
    assert_eq!(output[2], [0.0, 0.0]);

    store.close().expect("project should close");
    remove_database(&database_path);
}

#[test]
fn seek_past_an_item_does_not_resolve_its_missing_source() {
    let database_path = unique_path("aaadaw");
    let store = ProjectStore::open(&database_path).expect("project should open");
    let project = project_with_audio_item("asset://missing-before-playhead".to_owned(), 4);

    let prepared = prepare_audio_playback_at(&project, &store, 8, 64, 8)
        .expect("item before the seek position does not need media resolution");
    assert_eq!(prepared.feeder_count(), 0);
    let (mut graph, feeders) = prepared.into_parts();
    for feeder in feeders {
        feeder.join().expect("no feeders should need to run");
    }
    graph.transport_mut().start();
    let mut output = [[1.0; 2]; 2];
    let stats = graph
        .render_into(&mut output)
        .expect("past item should not be scheduled at this position");
    assert_eq!(output, [[0.0; 2]; 2]);
    assert_eq!(stats.underrun_samples, 0);

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

#[test]
fn linked_stereo_source_keeps_distinct_channels() {
    let database_path = unique_path("aaadaw");
    let source_path = unique_path("wav");
    std::fs::write(&source_path, stereo_pcm_wav(&[[8192, -16384]], 48_000))
        .expect("stereo fixture should be written");
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    let media_ref = store
        .link_external_audio_file(&source_path)
        .expect("stereo source should link");
    let project = project_with_audio_item(media_ref, 1);
    let prepared = prepare_audio_playback(&project, &store, 4, 2)
        .expect("linked stereo source should prepare");
    let (mut graph, feeders) = prepared.into_parts();
    for feeder in feeders {
        feeder.join().expect("linked stereo source should decode");
    }
    graph.transport_mut().start();
    let mut output = [[0.0; 2]; 1];
    graph
        .render_into(&mut output)
        .expect("linked stereo source should render");
    assert!((output[0][0] - 0.25).abs() < 1.0e-5);
    assert!((output[0][1] + 0.5).abs() < 1.0e-5);

    store.close().expect("project should close");
    std::fs::remove_file(source_path).expect("linked source should be removed");
    remove_database(&database_path);
}

#[test]
fn stereo_source_trim_advances_left_and_right_together() {
    let database_path = unique_path("aaadaw");
    let wav = stereo_pcm_wav(&[[16384, -8192], [8192, -16384]], 48_000);
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    store
        .import_audio_asset("asset://stereo-trim", "trim.wav", Cursor::new(&wav))
        .expect("stereo WAV should embed");
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Stereo".to_owned(),
        })
        .expect("track should be created");
    project
        .apply(DawAction::InsertAudioItem {
            track_id: project.tracks()[0].id(),
            media_ref: "asset://stereo-trim".to_owned(),
            start_sample: 0,
            source_offset_samples: 1,
            length_samples: 1,
        })
        .expect("trimmed audio item should be valid");

    let prepared =
        prepare_audio_playback(&project, &store, 4, 2).expect("trimmed stereo item should prepare");
    let (mut graph, feeders) = prepared.into_parts();
    for feeder in feeders {
        feeder.join().expect("trimmed stereo item should decode");
    }
    graph.transport_mut().start();
    let mut output = [[0.0; 2]; 1];
    graph
        .render_into(&mut output)
        .expect("trimmed stereo item should render");
    assert!((output[0][0] - 0.25).abs() < 1.0e-5);
    assert!((output[0][1] + 0.5).abs() < 1.0e-5);

    store.close().expect("project should close");
    remove_database(&database_path);
}
