use aaadaw_app::{
    PlaybackBuildError, ProjectWavRenderSettings, ProjectWavRenderSource, WavExportOptions,
    WavSampleFormat, prepare_audio_playback, prepare_audio_playback_at,
    prepare_audio_playback_at_with_loop_range, prepare_audio_playback_with_resampling_quality,
    render_prepared_audio_to_pcm24_wav, render_prepared_audio_to_wav,
    render_project_file_to_wav_with_settings,
};
use aaadaw_core::{DawAction, Project};
use aaadaw_engine::TransportLoopRange;
use aaadaw_media::ResamplingQuality;
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
    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
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
fn inverted_audio_items_export_with_reversed_polarity() {
    let database_path = unique_path("aaadaw");
    let render_path = unique_path("wav");
    let wav = pcm_wav(&[8_192; 8], 48_000);
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    store
        .import_audio_asset("asset://phase-inverted", "phase.wav", Cursor::new(&wav))
        .expect("WAV should embed");
    let mut project = project_with_audio_item("asset://phase-inverted".to_owned(), 8);
    let item_id = project.audio_items()[0].id();
    project
        .apply(DawAction::SetAudioItemPhase {
            item_id,
            inverted: true,
        })
        .expect("audio item should invert phase");

    let prepared = prepare_audio_playback_with_resampling_quality(
        &project,
        &store,
        16,
        8,
        ResamplingQuality::Sinc192,
    )
    .expect("phase-inverted source should prepare for offline rendering");
    render_prepared_audio_to_pcm24_wav(
        prepared,
        &project,
        &render_path,
        8,
        &AtomicBool::new(false),
        |_, _| {},
    )
    .expect("phase-inverted source should export");
    let mut decoder = aaadaw_media::AudioStreamDecoder::open(&render_path)
        .expect("offline output should be a readable WAV");
    let mut rendered = Vec::new();
    while let Some(chunk) = decoder.next_chunk().expect("offline WAV should decode") {
        rendered.extend_from_slice(chunk.samples());
    }
    assert_eq!(rendered.len(), 16);
    let expected = -0.25 * std::f32::consts::FRAC_1_SQRT_2;
    assert!(
        rendered
            .iter()
            .all(|sample| (*sample - expected).abs() < 1.0e-6)
    );

    store.close().expect("project should close");
    remove_database(&database_path);
    let _ = std::fs::remove_file(render_path);
}

#[test]
fn muted_audio_items_stay_silent_in_realtime_and_offline_rendering() {
    let database_path = unique_path("aaadaw");
    let render_path = unique_path("wav");
    let wav = pcm_wav(&[8_192; 8], 48_000);
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    store
        .import_audio_asset("asset://muted", "muted.wav", Cursor::new(&wav))
        .expect("WAV should embed");
    let mut project = project_with_audio_item("asset://muted".to_owned(), 8);
    let item_id = project.audio_items()[0].id();
    project
        .apply(DawAction::SetAudioItemMute {
            item_id,
            muted: true,
        })
        .expect("audio item should mute");

    let prepared = prepare_audio_playback(&project, &store, 16, 8)
        .expect("muted source should prepare for realtime playback");
    let (mut graph, feeders) = prepared.into_parts();
    for feeder in feeders {
        feeder.join().expect("muted source should decode");
    }
    graph.transport_mut().start();
    let mut playback = [[1.0; 2]; 8];
    let stats = graph
        .render_into(&mut playback)
        .expect("muted source should render in realtime");
    assert_eq!(stats.underrun_samples, 0);
    assert_eq!(playback, [[0.0; 2]; 8]);

    let prepared = prepare_audio_playback_with_resampling_quality(
        &project,
        &store,
        16,
        8,
        ResamplingQuality::Sinc192,
    )
    .expect("muted source should prepare for offline rendering");
    render_prepared_audio_to_pcm24_wav(
        prepared,
        &project,
        &render_path,
        8,
        &AtomicBool::new(false),
        |_, _| {},
    )
    .expect("muted source should render offline");
    let mut decoder = aaadaw_media::AudioStreamDecoder::open(&render_path)
        .expect("offline output should be a readable WAV");
    let mut rendered = Vec::new();
    while let Some(chunk) = decoder.next_chunk().expect("offline WAV should decode") {
        rendered.extend_from_slice(chunk.samples());
    }
    assert_eq!(rendered, [0.0; 16]);

    store.close().expect("project should close");
    remove_database(&database_path);
    let _ = std::fs::remove_file(render_path);
}

#[test]
fn copied_loop_sources_repeat_in_realtime_and_offline_project_rendering() {
    let database_path = unique_path("aaadaw");
    let playback_wav = pcm_wav(&[0, 8_192, 16_384], 48_000);
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    store
        .import_audio_asset(
            "asset://looped-playback",
            "loop.wav",
            Cursor::new(&playback_wav),
        )
        .expect("looped source should embed");

    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
    for index in 0..2 {
        project
            .apply(DawAction::CreateTrack {
                index,
                name: format!("Loop {}", index + 1),
            })
            .expect("audio track should be created");
        project
            .apply(DawAction::InsertLoopedAudioItem {
                track_id: project.tracks()[index].id(),
                media_ref: "asset://looped-playback".to_owned(),
                start_sample: 0,
                source_offset_samples: 0,
                phase_inverted: false,
                source_section_start_samples: 1,
                length_samples: 5,
                muted: false,
                source_section_length_samples: 2,
            })
            .expect("looped item should be inserted");
    }

    let prepared = prepare_audio_playback(&project, &store, 16, 8)
        .expect("looped embedded sources should prepare for playback");
    assert_eq!(prepared.feeder_count(), 2);
    let (mut graph, feeders) = prepared.into_parts();
    for feeder in feeders {
        feeder
            .join()
            .expect("playback feeder should loop the source section");
    }
    graph.transport_mut().start();
    let mut playback = [[0.0; 2]; 5];
    let stats = graph
        .render_into(&mut playback)
        .expect("looped playback graph should render");
    assert_eq!(stats.underrun_samples, 0);
    let center_gain = std::f32::consts::FRAC_1_SQRT_2;
    let expected_mono = [0.25_f32, 0.5, 0.25, 0.5, 0.25];
    for (frame, sample) in playback.iter().zip(expected_mono) {
        assert!((frame[0] - sample * center_gain * 2.0).abs() < 1.0e-5);
        assert!((frame[1] - sample * center_gain * 2.0).abs() < 1.0e-5);
    }

    let prepared = prepare_audio_playback_with_resampling_quality(
        &project,
        &store,
        16,
        8,
        ResamplingQuality::Sinc192,
    )
    .expect("looped sources should prepare for offline render quality");
    let render_path = unique_path("wav");
    render_prepared_audio_to_pcm24_wav(
        prepared,
        &project,
        &render_path,
        5,
        &AtomicBool::new(false),
        |_, _| {},
    )
    .expect("offline renderer should repeat the copied source sections");
    let mut decoder = aaadaw_media::AudioStreamDecoder::open(&render_path)
        .expect("offline output should be a readable WAV");
    let mut rendered = Vec::new();
    while let Some(chunk) = decoder.next_chunk().expect("offline WAV should decode") {
        rendered.extend_from_slice(chunk.samples());
    }
    assert_eq!(decoder.metadata().channel_count, Some(2));
    assert_eq!(rendered.len(), 10);
    for (frame, sample) in rendered.as_chunks::<2>().0.iter().zip(expected_mono) {
        assert!((frame[0] - sample * center_gain * 2.0).abs() < 2.0e-5);
        assert!((frame[1] - sample * center_gain * 2.0).abs() < 2.0e-5);
    }

    store.close().expect("project should close");
    remove_database(&database_path);
    let _ = std::fs::remove_file(render_path);
}

#[test]
fn transport_loop_preparation_repeats_an_embedded_bounded_stereo_item() {
    let database_path = unique_path("aaadaw");
    let source_frames: Vec<_> = (0_i16..16)
        .map(|index| {
            let sample = (index - 8) * 2_048;
            [sample, -sample]
        })
        .collect();
    let wav = stereo_pcm_wav(&source_frames, 48_000);
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    store
        .import_audio_asset("asset://transport-loop", "loop.wav", Cursor::new(&wav))
        .expect("loop source should embed");

    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Transport loop".to_owned(),
        })
        .expect("track should be created");
    let loop_range = TransportLoopRange::new(16, 32).expect("loop should be nonempty");
    project
        .apply(DawAction::InsertLoopedAudioItem {
            track_id: project.tracks()[0].id(),
            media_ref: "asset://transport-loop".to_owned(),
            start_sample: loop_range.start_sample(),
            source_offset_samples: 0,
            phase_inverted: false,
            source_section_start_samples: 0,
            length_samples: loop_range.length_samples(),
            muted: false,
            source_section_length_samples: loop_range.length_samples(),
        })
        .expect("looped item should be inserted");

    let prepared = prepare_audio_playback_at_with_loop_range(
        &project,
        &store,
        20,
        Some(loop_range),
        64,
        32,
        ResamplingQuality::Sinc64,
    )
    .expect("matching embedded loop item should prepare");
    assert_eq!(prepared.feeder_count(), 1);
    let (mut graph, feeders) = prepared.into_parts();
    std::thread::sleep(std::time::Duration::from_millis(20));
    graph.transport_mut().start();
    let mut output = [[0.0_f32; 2]; 32];
    let stats = graph
        .render_into(&mut output)
        .expect("prepared loop graph should render through two wraps");
    assert_eq!(stats.underrun_samples, 0);
    for (frame_index, frame) in output.iter().enumerate() {
        let source_index = (frame_index + 4) % source_frames.len();
        let expected = [
            source_frames[source_index][0] as f32 / 32_768.0,
            source_frames[source_index][1] as f32 / 32_768.0,
        ];
        assert!((frame[0] - expected[0]).abs() < 1.0e-6);
        assert!((frame[1] - expected[1]).abs() < 1.0e-6);
    }
    assert_eq!(graph.transport_mut().position_samples(), 20);
    for feeder in feeders {
        feeder.cancel().expect("loop feeder should stop cleanly");
    }
    drop(graph);

    project
        .apply(DawAction::SetAudioItemLoopSource {
            item_id: project.audio_items()[0].id(),
            enabled: false,
        })
        .expect("loop-source mode should be editable while stopped");
    assert!(matches!(
        prepare_audio_playback_at_with_loop_range(
            &project,
            &store,
            20,
            Some(loop_range),
            64,
            32,
            ResamplingQuality::Sinc64,
        ),
        Err(PlaybackBuildError::Media(
            aaadaw_media::MediaError::InvalidTimelineLoop
        ))
    ));

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
    let mut project = project_with_audio_item("asset://stereo".to_owned(), 4);
    let item_id = project.audio_items()[0].id();
    project
        .apply(DawAction::SetAudioItemLoopSource {
            item_id,
            enabled: false,
        })
        .expect("stereo fixture should play its source once");

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
    assert!((output[3][0] - output[2][0]).abs() < 0.05);
    assert!((output[3][1] - output[2][1]).abs() < 0.05);

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
    assert!((seek_output[1][0] - 0.25).abs() < 0.05);
    assert!((seek_output[1][1] + 0.5).abs() < 0.05);

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
fn render_quality_uses_a_sharper_sinc_filter_than_playback_quality() {
    let database_path = unique_path("aaadaw");
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    let source_rate = 96_000.0;
    let source = (0..96_000)
        .map(|index| {
            (2.0 * std::f32::consts::PI * 26_000.0 * index as f32 / source_rate).sin() * 0.8
        })
        .map(|sample| (sample * i16::MAX as f32) as i16)
        .collect::<Vec<_>>();
    store
        .import_audio_asset(
            "asset://resampling",
            "resampling.wav",
            Cursor::new(pcm_wav(&source, 96_000)),
        )
        .expect("high-rate WAV should embed");
    let project = project_with_audio_item("asset://resampling".to_owned(), 48_000);

    let measured_alias = |quality| {
        let prepared = prepare_audio_playback_with_resampling_quality(
            &project, &store, 50_000, 2_048, quality,
        )
        .expect("audio should prepare at requested quality");
        let (mut graph, feeders) = prepared.into_parts();
        for feeder in feeders {
            feeder.join().expect("resampling worker should finish");
        }
        graph.transport_mut().start();
        let mut output = vec![[0.0; 2]; 48_000];
        for block in output.chunks_mut(2_048) {
            let stats = graph
                .render_into(block)
                .expect("prepared audio should render");
            assert_eq!(stats.underrun_samples, 0);
        }
        let center_gain = std::f32::consts::FRAC_1_SQRT_2;
        let margin = 256;
        let stable = &output[margin..output.len() - margin];
        let projection = stable
            .iter()
            .enumerate()
            .map(|(index, frame)| {
                let absolute_index = index + margin;
                (frame[0] / center_gain)
                    * (2.0 * std::f32::consts::PI * 22_000.0 * absolute_index as f32 / 48_000.0)
                        .sin()
            })
            .sum::<f32>();
        2.0 * projection.abs() / stable.len() as f32
    };

    let playback_alias = measured_alias(ResamplingQuality::Sinc64);
    let render_alias = measured_alias(ResamplingQuality::Sinc192);
    assert!(
        render_alias < playback_alias * 0.7,
        "192pt render alias {render_alias} should be below 70% of 64pt playback alias {playback_alias}"
    );

    store.close().expect("project should close");
    remove_database(&database_path);
}

#[test]
fn project_render_time_selection_starts_at_its_sample_and_adds_the_requested_tail() {
    let database_path = unique_path("aaadaw");
    let output_path = unique_path("wav");
    let mut source = vec![0_i16; 96_000];
    source[24_000..48_000].fill(16_384);
    source[48_000..].fill(8_192);
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    store
        .import_audio_asset(
            "asset://render-range",
            "range.wav",
            Cursor::new(pcm_wav(&source, 48_000)),
        )
        .expect("source WAV should embed");
    store
        .close()
        .expect("project should close before rendering");
    let project = project_with_audio_item("asset://render-range".to_owned(), 96_000);

    render_project_file_to_wav_with_settings(
        &database_path,
        &project,
        &output_path,
        ProjectWavRenderSettings {
            master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
            export_options: WavExportOptions::default(),
            output_sample_rate: 44_100,
            output_channels: aaadaw_app::WavChannelCount::Mono,
            source: ProjectWavRenderSource::MasterMix,
            start_sample: 24_000,
            end_sample: 48_000,
            tail_milliseconds: 1_250,
            normalize_peak: true,
            normalize_peak_target_centi_dbfs: 0,
        },
        &AtomicBool::new(false),
        |_, _| {},
    )
    .expect("selected range should render with its requested tail");

    let bytes = std::fs::read(&output_path).expect("range render should be published");
    assert_eq!(
        u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
        44_100
    );
    assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 1);
    assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 24);
    let data_bytes = u32::from_le_bytes(bytes[40..44].try_into().expect("WAV data size"));
    assert_eq!(data_bytes, 77_175 * 3);
    assert_eq!(bytes.len(), 44 + data_bytes as usize);
    let (samples, remainder) = bytes[44..].as_chunks::<3>();
    assert!(remainder.is_empty());
    let peak = samples
        .iter()
        .map(|sample| {
            let value = i32::from_le_bytes([
                sample[0],
                sample[1],
                sample[2],
                if sample[2] & 0x80 == 0 { 0 } else { 0xff },
            ]);
            value.unsigned_abs()
        })
        .max()
        .expect("rendered samples should be present");
    assert!(
        peak >= 8_380_000,
        "peak-normalized PCM24 output should reach 0 dBFS, got {peak}"
    );
    let stable_left_offset = 44 + 1_000 * 3;
    let first_left = i32::from_le_bytes([
        bytes[stable_left_offset],
        bytes[stable_left_offset + 1],
        bytes[stable_left_offset + 2],
        if bytes[stable_left_offset + 2] & 0x80 == 0 {
            0
        } else {
            0xff
        },
    ]);
    assert!(
        first_left > 2_000_000,
        "resampled output should preserve the selected passband, got {first_left}"
    );
    let tail_left_offset = 44 + (22_050 + 1_000) * 3;
    let tail_left = i32::from_le_bytes([
        bytes[tail_left_offset],
        bytes[tail_left_offset + 1],
        bytes[tail_left_offset + 2],
        if bytes[tail_left_offset + 2] & 0x80 == 0 {
            0
        } else {
            0xff
        },
    ]);
    assert!(
        tail_left > 900_000,
        "the appended timeline tail should contain the following project audio, got {tail_left}"
    );

    let untailed_output_path = unique_path("wav");
    render_project_file_to_wav_with_settings(
        &database_path,
        &project,
        &untailed_output_path,
        ProjectWavRenderSettings {
            master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
            export_options: WavExportOptions::default(),
            output_sample_rate: 44_100,
            output_channels: aaadaw_app::WavChannelCount::Mono,
            source: ProjectWavRenderSource::MasterMix,
            start_sample: 24_000,
            end_sample: 48_000,
            tail_milliseconds: 0,
            normalize_peak: false,
            normalize_peak_target_centi_dbfs: 0,
        },
        &AtomicBool::new(false),
        |_, _| {},
    )
    .expect("disabled tail should render only the selected range");
    let untailed_bytes =
        std::fs::read(&untailed_output_path).expect("untailed range render should be published");
    let untailed_data_bytes =
        u32::from_le_bytes(untailed_bytes[40..44].try_into().expect("WAV data size"));
    assert_eq!(untailed_data_bytes, 22_050 * 3);
    assert_eq!(untailed_bytes.len(), 44 + untailed_data_bytes as usize);

    std::fs::remove_file(output_path).expect("rendered file should be removed");
    std::fs::remove_file(untailed_output_path).expect("untailed render should be removed");
    remove_database(&database_path);
}

#[test]
fn selected_track_via_master_render_omits_unselected_track_media() {
    let database_path = unique_path("aaadaw");
    let selected_path = unique_path("wav");
    let master_path = unique_path("wav");
    let source = pcm_wav(&vec![4_096; 48_000], 48_000);
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    store
        .import_audio_asset(
            "asset://selected-render-track",
            "selected-render-track.wav",
            Cursor::new(source),
        )
        .expect("shared track source should embed");
    store
        .close()
        .expect("project should close before rendering");

    let mut project = project_with_audio_item("asset://selected-render-track".to_owned(), 48_000);
    let selected_track = project.tracks()[0].id();
    project
        .apply(DawAction::CreateTrack {
            index: 1,
            name: "Unselected".to_owned(),
        })
        .expect("second track should be created");
    project
        .apply(DawAction::InsertAudioItem {
            track_id: project.tracks()[1].id(),
            media_ref: "asset://selected-render-track".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .expect("second track should use the same source");

    let settings = |source| ProjectWavRenderSettings {
        master_ceiling: aaadaw_engine::MasterOutputCeiling::default(),
        export_options: WavExportOptions::default(),
        output_sample_rate: 48_000,
        output_channels: aaadaw_app::WavChannelCount::Stereo,
        source,
        start_sample: 0,
        end_sample: 48_000,
        tail_milliseconds: 0,
        normalize_peak: false,
        normalize_peak_target_centi_dbfs: 0,
    };
    render_project_file_to_wav_with_settings(
        &database_path,
        &project,
        &selected_path,
        settings(ProjectWavRenderSource::SelectedTrackViaMaster(
            selected_track,
        )),
        &AtomicBool::new(false),
        |_, _| {},
    )
    .expect("selected track should render through the master");
    render_project_file_to_wav_with_settings(
        &database_path,
        &project,
        &master_path,
        settings(ProjectWavRenderSource::MasterMix),
        &AtomicBool::new(false),
        |_, _| {},
    )
    .expect("the master mix should render both tracks");

    let selected = std::fs::read(&selected_path).expect("selected-track WAV should exist");
    let master = std::fs::read(&master_path).expect("master WAV should exist");
    let stable_left_sample = |bytes: &[u8]| {
        let offset = 44 + 1_000 * 6;
        i32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            if bytes[offset + 2] & 0x80 == 0 {
                0
            } else {
                0xff
            },
        ])
    };
    let selected_level = stable_left_sample(&selected);
    let master_level = stable_left_sample(&master);
    assert!(selected_level > 0, "selected source should be audible");
    let ratio = f64::from(master_level) / f64::from(selected_level);
    assert!(
        (ratio - 2.0).abs() < 0.02,
        "the full mix should contain both identical sources (ratio {ratio})"
    );
    assert!(
        project.tracks().iter().all(|track| !track.is_solo()),
        "the render source must not mutate the saved project solo state"
    );

    std::fs::remove_file(selected_path).expect("selected WAV should be removed");
    std::fs::remove_file(master_path).expect("master WAV should be removed");
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
fn offline_render_routes_linear_companded_and_adpcm_formats_through_the_same_graph() {
    let database_path = unique_path("aaadaw");
    let wav = pcm_wav(&[16_384, -16_384, 8_192, -8_192], 48_000);
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    store
        .import_audio_asset("asset://formats", "formats.wav", Cursor::new(&wav))
        .expect("WAV should embed");
    let project = project_with_audio_item("asset://formats".to_owned(), 4);
    store.close().expect("project should close");
    for sample_format in WavSampleFormat::ALL {
        let path = unique_path("render-format-wav");
        let store = ProjectStore::open(&database_path).expect("project should reopen");
        let prepared = prepare_audio_playback(&project, &store, 1, 4)
            .expect("embedded source should prepare for selected output format");
        store
            .close()
            .expect("project should close before rendering");
        render_prepared_audio_to_wav(
            prepared,
            &project,
            &path,
            4,
            WavExportOptions {
                sample_format,
                dither: false,
            },
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap_or_else(|error| panic!("{sample_format} render should finish: {error}"));

        let bytes = std::fs::read(&path).expect("rendered WAV should exist");
        let expected_bits = match sample_format {
            WavSampleFormat::Pcm8 => 8,
            WavSampleFormat::Pcm16 => 16,
            WavSampleFormat::Pcm24 => 24,
            WavSampleFormat::Pcm32 => 32,
            WavSampleFormat::Float32 => 32,
            WavSampleFormat::Float64 => 64,
            WavSampleFormat::MuLaw8 => 8,
            WavSampleFormat::ImaAdpcm4 => 4,
            WavSampleFormat::Cadpcm2 => 2,
        };
        assert_eq!(
            u16::from_le_bytes(bytes[34..36].try_into().unwrap()),
            expected_bits,
            "{sample_format} WAV bit depth"
        );
        if matches!(
            sample_format,
            WavSampleFormat::Float32 | WavSampleFormat::Float64
        ) {
            assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 3);
        } else if sample_format == WavSampleFormat::MuLaw8 {
            assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 7);
        } else if sample_format == WavSampleFormat::ImaAdpcm4 {
            assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 17);
        } else if sample_format == WavSampleFormat::Cadpcm2 {
            assert_eq!(
                u16::from_le_bytes(bytes[20..22].try_into().unwrap()),
                0xac0c
            );
        } else {
            assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 1);
        }

        let mut decoder = aaadaw_media::AudioStreamDecoder::open(&path)
            .unwrap_or_else(|error| panic!("{sample_format} output should decode: {error}"));
        let chunk = decoder
            .next_chunk()
            .unwrap_or_else(|error| panic!("{sample_format} decoder should read: {error}"))
            .unwrap_or_else(|| panic!("{sample_format} output should contain frames"));
        assert_eq!(chunk.channels(), 2);
        let expected_centered_mono = 0.5 * std::f32::consts::FRAC_1_SQRT_2;
        let tolerance = if matches!(
            sample_format,
            WavSampleFormat::MuLaw8 | WavSampleFormat::ImaAdpcm4 | WavSampleFormat::Cadpcm2
        ) {
            0.02
        } else if sample_format == WavSampleFormat::Pcm8 {
            1.0 / 128.0
        } else if sample_format == WavSampleFormat::Pcm16 {
            1.0e-4
        } else {
            1.0e-6
        };
        assert!((chunk.samples()[0] - expected_centered_mono).abs() < tolerance);
        assert!((chunk.samples()[1] - expected_centered_mono).abs() < tolerance);
        std::fs::remove_file(path).expect("rendered WAV should be removed");
    }

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
    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
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
