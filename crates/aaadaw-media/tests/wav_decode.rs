use aaadaw_core::{DawAction, Project};
use aaadaw_engine::{AudioItemStream, AudioRenderGraph, pcm_stream};
use aaadaw_media::{
    AudioStreamDecoder, AudioWaveform, spawn_audio_item_stream, spawn_audio_item_stream_at,
    spawn_audio_item_stream_from_reader_at, spawn_mono_stream,
};
use aaadaw_storage::ProjectStore;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(0);

fn wav_path() -> PathBuf {
    let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("aaadaw-media-{}-{id}.wav", std::process::id()))
}

fn project_path() -> PathBuf {
    let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("aaadaw-media-{}-{id}.aaadaw", std::process::id()))
}

fn pcm_wav(samples: &[i16], sample_rate: u32) -> Vec<u8> {
    let data_len = u32::try_from(samples.len() * 2).expect("test fixture should fit in WAV");
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1_u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes()); // block alignment
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

#[test]
fn decodes_wav_packets_as_interleaved_f32() {
    let path = wav_path();
    std::fs::write(&path, pcm_wav(&[-32768, 0, 16384, 32767], 44_100))
        .expect("test WAV should be written");

    let probed = aaadaw_media::probe_audio_metadata(&path).expect("WAV metadata should probe");
    assert_eq!(probed.container, "wave");
    assert_eq!(probed.frame_count, Some(4));
    let mut decoder = AudioStreamDecoder::open(&path).expect("WAV should open");
    assert_eq!(decoder.metadata().container, "wave");
    assert_eq!(decoder.metadata().sample_rate, Some(44_100));
    assert_eq!(decoder.metadata().channel_count, Some(1));
    assert_eq!(decoder.metadata().bits_per_sample, Some(16));
    assert_eq!(decoder.metadata().frame_count, Some(4));
    assert_eq!(decoder.metadata().byte_len, Some(52));
    assert!(decoder.metadata().duration_nanos.is_some());
    let mut decoded_samples = Vec::new();
    let mut decoded_frames = 0;
    while let Some(chunk) = decoder.next_chunk().expect("WAV packet should decode") {
        assert_eq!(chunk.sample_rate(), 44_100);
        assert_eq!(chunk.channels(), 1);
        decoded_frames += chunk.frame_count();
        decoded_samples.extend_from_slice(chunk.samples());
    }

    assert_eq!(decoded_frames, 4);
    assert_eq!(decoded_samples.len(), 4);
    assert!((decoded_samples[0] + 1.0).abs() < 1.0e-6);
    assert_eq!(decoded_samples[1], 0.0);
    assert!((decoded_samples[2] - 0.5).abs() < 1.0e-6);
    assert!(decoded_samples[3] < 1.0 && decoded_samples[3] > 0.999);
    std::fs::remove_file(path).expect("test file should be removed");
}

#[test]
fn waveform_peaks_keep_bin_boundaries_across_decoder_packets() {
    let samples = [vec![-16_384; 4_096], vec![16_384; 4_096], vec![0]].concat();
    let bytes = pcm_wav(&samples, 48_000);
    let mut decoder = AudioStreamDecoder::from_reader(
        Cursor::new(bytes.clone()),
        Some(bytes.len() as u64),
        Some("wav"),
    )
    .expect("fixture should open");
    let waveform = AudioWaveform::decode(&mut decoder, 4_096)
        .expect("waveform should decode without retaining PCM");

    assert_eq!(waveform.sample_rate(), 48_000);
    assert_eq!(waveform.frame_count(), 8_193);
    assert_eq!(waveform.frames_per_peak(), 4_096);
    assert_eq!(waveform.peaks().len(), 3);
    assert!((waveform.peaks()[0].min + 0.5).abs() < 1.0e-6);
    assert!((waveform.peaks()[0].max + 0.5).abs() < 1.0e-6);
    assert!((waveform.peaks()[1].min - 0.5).abs() < 1.0e-6);
    assert!((waveform.peaks()[1].max - 0.5).abs() < 1.0e-6);
    assert_eq!(waveform.peaks()[2].min, 0.0);
    assert_eq!(waveform.peaks()[2].max, 0.0);
    assert_eq!(waveform.levels().len(), 3);
    assert_eq!(waveform.levels()[1].frames_per_peak(), 8_192);
    assert_eq!(waveform.levels()[1].peaks().len(), 2);
    assert!((waveform.levels()[1].peaks()[0].min + 0.5).abs() < 1.0e-6);
    assert!((waveform.levels()[1].peaks()[0].max - 0.5).abs() < 1.0e-6);
    assert_eq!(waveform.levels()[2].peaks().len(), 1);
    assert!((waveform.levels()[2].peaks()[0].min + 0.5).abs() < 1.0e-6);
    assert!((waveform.levels()[2].peaks()[0].max - 0.5).abs() < 1.0e-6);
    let (level, range) = waveform.peak_range_for_source_frames(4_096, 8_193, 4_096);
    assert_eq!(level.frames_per_peak(), 4_096);
    assert_eq!(range, 1..3);
    let (level, range) = waveform.peak_range_for_source_frames(4_096, 8_193, 8_192);
    assert_eq!(level.frames_per_peak(), 8_192);
    assert_eq!(range, 0..2);
    let (_, range) = waveform.peak_range_for_source_frames(9_000, 10_000, 4_096);
    assert!(range.is_empty());
}

#[test]
fn waveform_decode_honors_cancellation_before_publishing_peaks() {
    let bytes = pcm_wav(&[16_384; 4_096], 48_000);
    let mut decoder = AudioStreamDecoder::from_reader(
        Cursor::new(bytes.clone()),
        Some(bytes.len() as u64),
        Some("wav"),
    )
    .expect("fixture should open");
    let result = AudioWaveform::decode_with_cancel(&mut decoder, 256, || true);
    assert!(matches!(
        result,
        Err(aaadaw_media::MediaError::WorkerCancelled)
    ));
}

#[test]
fn decoded_audio_item_is_trimmed_resampled_and_scheduled_on_the_sample_clock() {
    let path = wav_path();
    std::fs::write(&path, pcm_wav(&[-32768, 0, 16384, 32767], 24_000))
        .expect("test WAV should be written");
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".to_owned(),
        })
        .expect("audio track should be created");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://test-wav".to_owned(),
            start_sample: 2,
            source_offset_samples: 1,
            length_samples: 4,
        })
        .expect("audio item should be inserted");
    let item = project.audio_items()[0].clone();
    let (producer, consumer) = pcm_stream(8).expect("stream capacity should be positive");
    let worker = spawn_audio_item_stream(&item, &path, 48_000, producer)
        .expect("valid audio item should start a worker");
    worker
        .join()
        .expect("worker should decode through the item range");

    let mut graph = AudioRenderGraph::new_for_audio_items(
        &project,
        vec![AudioItemStream::new(item.id(), consumer)],
        6,
    )
    .expect("item stream should match project audio item");
    graph.transport_mut().start();
    let mut output = [[0.0; 2]; 6];
    let stats = graph
        .render_into(&mut output)
        .expect("decoded audio item should render");
    assert_eq!(stats.underrun_samples, 0);
    assert_eq!(output[..2], [[0.0, 0.0]; 2]);
    let center_gain = std::f32::consts::FRAC_1_SQRT_2;
    let expected = [0.0, 0.25, 0.5, 49_151.0 / 65_536.0];
    for (frame, expected_sample) in output[2..].iter().zip(expected) {
        assert!(
            (frame[0] - expected_sample * center_gain).abs() < 1.0e-5,
            "output: {output:?}"
        );
        assert!((frame[1] - expected_sample * center_gain).abs() < 1.0e-5);
    }
    std::fs::remove_file(path).expect("test file should be removed");
}

#[test]
fn embedded_sqlite_audio_asset_decodes_into_an_audio_item_stream() {
    let path = project_path();
    let wav = pcm_wav(&[-32768, 0, 16384, 32767], 48_000);
    let mut store = ProjectStore::open(&path).expect("project database should open");
    store
        .import_audio_asset("asset://embedded-wav", "embedded.wav", Cursor::new(&wav))
        .expect("WAV should be embedded in bounded database chunks");
    let metadata_reader = store
        .audio_asset_reader("asset://embedded-wav")
        .expect("embedded metadata source should open");
    let metadata_decoder =
        AudioStreamDecoder::from_reader(metadata_reader, Some(wav.len() as u64), Some("wav"))
            .expect("embedded WAV headers should probe");
    let probed = metadata_decoder.metadata();
    store
        .set_audio_asset_metadata(
            "asset://embedded-wav",
            &aaadaw_storage::AudioAssetMetadata {
                container: probed.container.clone(),
                codec: probed.codec.clone(),
                sample_rate: probed.sample_rate,
                channel_count: probed.channel_count,
                bits_per_sample: probed.bits_per_sample,
                frame_count: probed.frame_count,
                duration_nanos: probed.duration_nanos,
                byte_len: probed.byte_len,
            },
        )
        .expect("header metadata should persist beside the immutable asset");
    assert_eq!(
        store
            .audio_asset_metadata("asset://embedded-wav")
            .expect("stored metadata should load")
            .unwrap()
            .frame_count,
        Some(4)
    );
    drop(metadata_decoder);
    let reader = store
        .audio_asset_reader("asset://embedded-wav")
        .expect("embedded asset should resolve to a reader");

    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Embedded Audio".to_owned(),
        })
        .expect("track should be created");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://embedded-wav".to_owned(),
            start_sample: 1,
            source_offset_samples: 1,
            length_samples: 2,
        })
        .expect("item should reference the embedded asset");
    let item = project.audio_items()[0].clone();
    let (producer, consumer) = pcm_stream(4).expect("stream capacity should be positive");
    spawn_audio_item_stream_from_reader_at(
        &item,
        2,
        reader,
        Some(wav.len() as u64),
        Some("wav"),
        48_000,
        producer,
    )
    .expect("embedded reader should start a background decoder")
    .join()
    .expect("embedded WAV should decode into the PCM stream");

    let mut graph = AudioRenderGraph::new_for_audio_items(
        &project,
        vec![AudioItemStream::new_at_sample(item.id(), 2, consumer)],
        4,
    )
    .expect("render graph should bind the refilled stream to the item");
    graph.transport_mut().seek_sample(2);
    graph.transport_mut().start();
    let mut output = [[0.0; 2]; 4];
    let stats = graph
        .render_into(&mut output)
        .expect("embedded item should render");
    let center_gain = std::f32::consts::FRAC_1_SQRT_2;
    assert_eq!(stats.underrun_samples, 0);
    assert!((output[0][0] - 0.5 * center_gain).abs() < 1.0e-6);
    assert!((output[0][1] - 0.5 * center_gain).abs() < 1.0e-6);
    assert_eq!(output[1], [0.0, 0.0]);
    assert_eq!(output[2], [0.0, 0.0]);
    assert_eq!(output[3], [0.0, 0.0]);

    store.close().expect("project database should close");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
}

#[test]
fn file_backed_audio_item_stream_can_refill_at_a_timeline_seek() {
    let path = wav_path();
    std::fs::write(&path, pcm_wav(&[0, 8192, 16384, 24576], 48_000))
        .expect("test WAV should be written");
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".to_owned(),
        })
        .expect("track should be created");
    let item = {
        let track_id = project.tracks()[0].id();
        project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: "asset://file-seek".to_owned(),
                start_sample: 10,
                source_offset_samples: 0,
                length_samples: 4,
            })
            .expect("item should be inserted");
        project.audio_items()[0].clone()
    };
    let (producer, mut consumer) = pcm_stream(8).expect("queue should have capacity");
    let mut worker = spawn_audio_item_stream_at(&item, 12, &path, 48_000, producer)
        .expect("file-backed item should start at the requested sample");
    worker
        .wait_ready()
        .expect("source should open on the worker");
    worker
        .join()
        .expect("worker should decode the remaining item samples");
    let mut samples = [0.0; 2];
    assert_eq!(consumer.read_into(&mut samples), 0);
    assert!((samples[0] - 0.5).abs() < 1.0e-6);
    assert!((samples[1] - 0.75).abs() < 1.0e-6);
    std::fs::remove_file(path).expect("test file should be removed");
}

#[test]
fn mono_stream_rejects_a_zero_output_rate_before_spawning() {
    let (producer, _) = pcm_stream(1).expect("stream capacity should be positive");
    let error = match spawn_mono_stream("unused.wav", 0, producer) {
        Ok(_) => panic!("zero output sample rate should be rejected"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        aaadaw_media::MediaError::InvalidOutputSampleRate
    ));
}
