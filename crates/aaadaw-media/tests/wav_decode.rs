use aaadaw_core::{DawAction, Project};
use aaadaw_engine::{AudioItemStream, AudioRenderGraph, pcm_stream};
use aaadaw_media::{
    AudioStreamDecoder, AudioWaveform, spawn_audio_item_stream, spawn_audio_item_stream_at,
    spawn_audio_item_stream_from_reader_at, spawn_mono_stream,
    spawn_stereo_audio_item_stream_from_reader,
};
use aaadaw_storage::ProjectStore;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

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

fn stereo_pcm_wav(frames: &[[i16; 2]], sample_rate: u32) -> Vec<u8> {
    let data_len = u32::try_from(frames.len() * 4).expect("test fixture should fit in WAV");
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

#[derive(Default)]
struct ReadGateState {
    stall_at: Option<u64>,
    waiting: bool,
    blocked_at: Option<u64>,
    released: bool,
}

#[derive(Default)]
struct ReadGate {
    state: Mutex<ReadGateState>,
    changed: Condvar,
}

impl ReadGate {
    fn arm(&self, byte_position: u64) {
        let mut state = self.state.lock().expect("read gate should not be poisoned");
        state.stall_at = Some(byte_position);
    }

    fn wait_until_blocked(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = self.state.lock().expect("read gate should not be poisoned");
        while !state.waiting {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            let (next_state, wait) = self
                .changed
                .wait_timeout(state, remaining)
                .expect("read gate should not be poisoned");
            state = next_state;
            if wait.timed_out() && !state.waiting {
                return false;
            }
        }
        true
    }

    fn blocked_position(&self) -> Option<u64> {
        self.state
            .lock()
            .expect("read gate should not be poisoned")
            .blocked_at
    }

    fn release(&self) {
        let mut state = self.state.lock().expect("read gate should not be poisoned");
        state.released = true;
        self.changed.notify_all();
    }
}

struct ReadGateRelease(Arc<ReadGate>);

impl Drop for ReadGateRelease {
    fn drop(&mut self) {
        self.0.release();
    }
}

struct DelayedReader {
    cursor: Cursor<Vec<u8>>,
    gate: Arc<ReadGate>,
    byte_position: Arc<AtomicU64>,
}

impl Read for DelayedReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let position = self.cursor.position();
        let mut state = self
            .gate
            .state
            .lock()
            .expect("read gate should not be poisoned");
        while state.stall_at.is_some_and(|stall_at| position >= stall_at) && !state.released {
            state.waiting = true;
            state.blocked_at = Some(position);
            self.gate.changed.notify_all();
            state = self
                .gate
                .changed
                .wait(state)
                .expect("read gate should not be poisoned");
        }
        let maximum = state
            .stall_at
            .filter(|stall_at| position < *stall_at)
            .map_or(buffer.len(), |stall_at| {
                buffer.len().min((stall_at - position) as usize)
            });
        drop(state);
        let read = self.cursor.read(&mut buffer[..maximum]);
        self.byte_position
            .store(self.cursor.position(), Ordering::Release);
        read
    }
}

impl Seek for DelayedReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let position = self.cursor.seek(position)?;
        self.byte_position.store(position, Ordering::Release);
        Ok(position)
    }
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
fn delayed_stereo_reader_skips_pcm_that_falls_behind_the_playhead() {
    let target_offset = 1_000_000_u64;
    let mut frames = vec![[-16_384, 16_384]; target_offset as usize + 2];
    frames.extend(vec![[8_192, -8_192]; 8_192]);
    let bytes = stereo_pcm_wav(&frames, 48_000);
    let gate = Arc::new(ReadGate::default());
    let byte_position = Arc::new(AtomicU64::new(0));
    let reader = DelayedReader {
        cursor: Cursor::new(bytes.clone()),
        gate: Arc::clone(&gate),
        byte_position: Arc::clone(&byte_position),
    };

    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Delayed media".to_owned(),
        })
        .expect("audio track should be created");
    project
        .apply(DawAction::InsertAudioItem {
            track_id: project.tracks()[0].id(),
            media_ref: "asset://delayed-media".to_owned(),
            start_sample: 100,
            source_offset_samples: 3,
            length_samples: target_offset + 40_000,
        })
        .expect("audio item should be inserted");
    let item = &project.audio_items()[0];
    let (producer, consumer) =
        aaadaw_engine::stereo_pcm_stream(2).expect("stream queue should have positive capacity");
    let mut feeder = spawn_stereo_audio_item_stream_from_reader(
        item,
        reader,
        Some(bytes.len() as u64),
        Some("wav"),
        48_000,
        producer,
    )
    .expect("stereo media feeder should start");
    feeder.wait_ready().expect("WAV reader should initialize");

    let initial_deadline = Instant::now() + Duration::from_secs(5);
    while consumer.available_frames() < 2 && Instant::now() < initial_deadline {
        std::thread::yield_now();
    }
    if consumer.available_frames() < 2 {
        gate.release();
        feeder
            .cancel()
            .expect("feeder should stop during test cleanup");
        panic!(
            "initial PCM did not prefill; reader byte position was {}",
            byte_position.load(Ordering::Acquire)
        );
    }
    gate.arm(byte_position.load(Ordering::Acquire));
    let _read_gate_release = ReadGateRelease(Arc::clone(&gate));
    let position = feeder.timeline_position();
    let stream = AudioItemStream::new_stereo_at_sample_with_position(
        item.id(),
        item.start_sample() + target_offset,
        consumer,
        position.clone(),
    );
    let mut graph = AudioRenderGraph::new_for_audio_items(&project, vec![stream], 2)
        .expect("delayed item should build a render graph");
    graph
        .transport_mut()
        .seek_sample(item.start_sample() + target_offset);
    graph.transport_mut().start();

    let mut stale = [[1.0; 2]; 2];
    let stats = graph
        .render_into(&mut stale)
        .expect("callback should render the delayed item");
    assert_eq!(stats.underrun_samples, 4);
    assert_eq!(stale, [[0.0; 2]; 2]);
    assert_eq!(
        position.requested_sample(),
        item.start_sample() + target_offset + 2
    );
    let reader_blocked = gate.wait_until_blocked(Duration::from_secs(5));
    if !reader_blocked {
        gate.release();
        feeder
            .cancel()
            .expect("feeder should stop during test cleanup");
        panic!(
            "reader should stall after its prefetched bytes are consumed; byte position was {}",
            byte_position.load(Ordering::Acquire)
        );
    }
    let blocked_at = gate
        .blocked_position()
        .expect("blocked reader should report its source position");
    let target_source_byte = 44 + (item.source_offset_samples() + target_offset + 2) * 4;
    assert!(
        blocked_at < target_source_byte,
        "reader should block before target samples are read; blocked at {blocked_at}, target starts at {target_source_byte}"
    );
    gate.release();

    let refill_deadline = Instant::now() + Duration::from_secs(5);
    let mut resumed = [[0.0; 2]; 2];
    let mut resumed_stats = None;
    while Instant::now() < refill_deadline {
        std::thread::sleep(Duration::from_millis(1));
        let stats = graph
            .render_into(&mut resumed)
            .expect("callback should continue after the reader resumes");
        if resumed == [[0.25, -0.25]; 2] {
            resumed_stats = Some(stats);
            break;
        }
    }
    assert_eq!(
        resumed,
        [[0.25, -0.25]; 2],
        "feeder should resume at target"
    );
    assert_eq!(
        resumed_stats
            .expect("resumed samples should have a render block")
            .underrun_samples,
        0
    );

    feeder
        .cancel()
        .expect("feeder should cancel after recovery");
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

    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
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
    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
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
