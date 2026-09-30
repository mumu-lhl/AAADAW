use aaadaw_core::{DawAction, Project};
use aaadaw_engine::{AudioRenderGraph, pcm_stream};
use aaadaw_media::{AudioStreamDecoder, spawn_mono_stream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(0);

fn wav_path() -> PathBuf {
    let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("aaadaw-media-{}-{id}.wav", std::process::id()))
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

    let mut decoder = AudioStreamDecoder::open(&path).expect("WAV should open");
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
fn decoded_wav_is_resampled_and_fed_to_the_engine_stream() {
    let path = wav_path();
    std::fs::write(&path, pcm_wav(&[-32768, 32767], 24_000)).expect("test WAV should be written");
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".to_owned(),
        })
        .expect("audio track should be created");
    let (producer, consumer) = pcm_stream(8).expect("stream capacity should be positive");
    let worker = spawn_mono_stream(&path, 48_000, producer)
        .expect("valid output sample rate should start a worker");
    worker.join().expect("worker should decode through EOF");

    let mut graph = AudioRenderGraph::new(&project, vec![consumer], 4)
        .expect("stream count should match the track count");
    graph.transport_mut().start();
    let mut output = [[0.0; 2]; 4];
    let stats = graph
        .render_into(&mut output)
        .expect("decoded audio block should render");
    assert_eq!(stats.underrun_samples, 0);
    let center_gain = std::f32::consts::FRAC_1_SQRT_2;
    let expected = [
        -1.0,
        -1.0 / 65_536.0,
        32_767.0 / 32_768.0,
        32_767.0 / 32_768.0,
    ];
    for (frame, expected_sample) in output.iter().zip(expected) {
        assert!((frame[0] - expected_sample * center_gain).abs() < 1.0e-5);
        assert!((frame[1] - expected_sample * center_gain).abs() < 1.0e-5);
    }
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
