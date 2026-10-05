use aaadaw_app::AudioWaveformWorker;
use aaadaw_storage::ProjectStore;
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(0);

fn waveform_project_path() -> std::path::PathBuf {
    let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "aaadaw-waveform-{}-{id}.aaadaw",
        std::process::id()
    ))
}

fn mono_wav(samples: &[i16], sample_rate: u32) -> Vec<u8> {
    let data_len = u32::try_from(samples.len() * 2).unwrap();
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

#[test]
fn worker_decodes_embedded_and_linked_audio_off_thread() {
    let project_path = waveform_project_path();
    let external_path = project_path.with_extension("wav");
    let bytes = mono_wav(&[16_384; 512], 48_000);
    std::fs::write(&external_path, &bytes).unwrap();
    let mut store = ProjectStore::open(&project_path).unwrap();
    let embedded_ref = "asset://embedded".to_owned();
    store
        .import_audio_asset(&embedded_ref, "embedded.wav", Cursor::new(bytes))
        .unwrap();
    let linked_ref = store.link_external_audio_file(&external_path).unwrap();
    store.close().unwrap();

    let cache_path = project_path.with_extension("aaapeaks");
    std::fs::write(&cache_path, b"corrupt waveform cache").unwrap();
    let worker = AudioWaveformWorker::start(
        project_path.clone(),
        vec![embedded_ref.clone(), linked_ref.clone()],
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !worker.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        worker.is_finished(),
        "waveform worker should finish promptly"
    );
    let mut results = worker.results();
    worker.join().unwrap();
    results.sort_unstable_by(|a, b| a.media_ref.cmp(&b.media_ref));
    assert_eq!(results.len(), 2);
    for result in results {
        let waveform = result.waveform.unwrap();
        assert_eq!(waveform.sample_rate(), 48_000);
        assert_eq!(waveform.frame_count(), 512);
        assert_eq!(waveform.peaks().len(), 2);
        assert!((waveform.peaks()[0].max - 0.5).abs() < 1.0e-6);
    }

    assert!(cache_path.is_file(), "worker should persist waveform peaks");
    let worker = AudioWaveformWorker::start(
        project_path.clone(),
        vec![embedded_ref.clone(), linked_ref.clone()],
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !worker.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    let results = worker.results();
    worker.join().unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|result| result.cache_hit));

    let original_modified = std::fs::metadata(&external_path)
        .unwrap()
        .modified()
        .unwrap();
    std::fs::write(&external_path, mono_wav(&[24_576; 512], 48_000)).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&external_path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(original_modified))
        .unwrap();
    let worker =
        AudioWaveformWorker::start(project_path.clone(), vec![embedded_ref, linked_ref.clone()])
            .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !worker.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    let results = worker.results();
    worker.join().unwrap();
    assert_eq!(results.len(), 2);
    let embedded_result = results
        .iter()
        .find(|result| result.media_ref == "asset://embedded")
        .unwrap();
    assert!(embedded_result.cache_hit);
    let linked_result = results
        .into_iter()
        .find(|result| result.media_ref == linked_ref)
        .unwrap();
    assert!(!linked_result.cache_hit);
    let waveform = linked_result.waveform.unwrap();
    assert!((waveform.peaks()[0].max - 0.75).abs() < 1.0e-6);

    let _ = std::fs::remove_file(&project_path);
    let _ = std::fs::remove_file(&external_path);
    let _ = std::fs::remove_file(&cache_path);
    for suffix in ["-wal", "-shm"] {
        let sidecar = format!("{}{suffix}", project_path.display());
        let _ = std::fs::remove_file(sidecar);
    }
}
