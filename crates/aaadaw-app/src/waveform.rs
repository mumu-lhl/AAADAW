use aaadaw_media::{
    AudioStreamDecoder, AudioWaveform, AudioWaveformCacheEntry, MediaError, decode_waveform_cache,
    encode_waveform_cache,
};
use aaadaw_storage::{ProjectStore, ResolvedAudioAsset};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::SystemTime;

const FRAMES_PER_PEAK: u32 = 256;
const MAX_PEAKS_PER_ASSET: u64 = 32_768;
static NEXT_CACHE_WRITE_ID: AtomicU64 = AtomicU64::new(0);

/// One decoded media overview, or its per-asset failure.
#[derive(Clone, Debug)]
pub struct AudioWaveformResult {
    /// Stable media reference in the project store.
    pub media_ref: String,
    /// Decoded peaks, or a message suitable for the project status line.
    pub waveform: Result<Arc<AudioWaveform>, String>,
    /// Whether the worker reused a matching `.aaapeaks` entry.
    pub cache_hit: bool,
}

/// Decodes project audio overviews off the UI and realtime audio threads.
pub struct AudioWaveformWorker {
    cancelled: Arc<AtomicBool>,
    results: Receiver<AudioWaveformResult>,
    thread: Option<JoinHandle<Result<(), String>>>,
}

#[derive(Debug)]
struct SourceFingerprint {
    hash: [u8; 32],
    linked_source: Option<LinkedSourceState>,
}

#[derive(Debug)]
struct LinkedSourceState {
    path: PathBuf,
    byte_len: u64,
    modified: SystemTime,
    file: File,
}

impl AudioWaveformWorker {
    /// Starts decoding each unique media reference from a saved project store.
    pub fn start(project_path: PathBuf, mut media_refs: Vec<String>) -> Result<Self, String> {
        media_refs.sort_unstable();
        media_refs.dedup();
        let cancelled = Arc::new(AtomicBool::new(false));
        Self::start_with_cancelled(project_path, media_refs, cancelled)
    }

    fn start_with_cancelled(
        project_path: PathBuf,
        media_refs: Vec<String>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        let worker_cancelled = Arc::clone(&cancelled);
        let (sender, results) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("aaadaw-audio-waveforms".to_owned())
            .spawn(move || {
                let cache_path = waveform_cache_path(&project_path);
                let store = ProjectStore::open(project_path).map_err(|error| error.to_string())?;
                let requested = media_refs
                    .iter()
                    .cloned()
                    .collect::<std::collections::HashSet<_>>();
                let mut cache_entries = read_waveform_cache(&cache_path, &worker_cancelled)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|entry| requested.contains(&entry.media_ref))
                    .map(|entry| (entry.media_ref.clone(), entry))
                    .collect::<HashMap<_, _>>();
                let mut cache_dirty = false;
                for media_ref in media_refs {
                    if worker_cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    let mut cache_hit = false;
                    let waveform = match source_fingerprint(&store, &media_ref, &worker_cancelled) {
                        Ok(Some(fingerprint)) => match cache_entries.get(&media_ref) {
                            Some(entry) if entry.source_hash == fingerprint.hash => {
                                cache_hit = true;
                                Ok(Arc::new(entry.waveform.clone()))
                            }
                            _ => match decode_asset(
                                &store,
                                &media_ref,
                                fingerprint
                                    .linked_source
                                    .as_ref()
                                    .map(|source| &source.file),
                                &worker_cancelled,
                            ) {
                                Ok(waveform) => {
                                    if linked_source_unchanged(&store, &media_ref, &fingerprint) {
                                        let entry = AudioWaveformCacheEntry {
                                            media_ref: media_ref.clone(),
                                            source_hash: fingerprint.hash,
                                            waveform: waveform.clone(),
                                        };
                                        cache_entries.insert(media_ref.clone(), entry);
                                        cache_dirty = true;
                                    }
                                    Ok(Arc::new(waveform))
                                }
                                Err(error) => Err(error.to_string()),
                            },
                        },
                        Ok(None) => decode_asset(&store, &media_ref, None, &worker_cancelled)
                            .map(Arc::new)
                            .map_err(|error| error.to_string()),
                        Err(error) => Err(error.to_string()),
                    };
                    if worker_cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    if sender
                        .send(AudioWaveformResult {
                            media_ref,
                            waveform,
                            cache_hit,
                        })
                        .is_err()
                    {
                        worker_cancelled.store(true, Ordering::Release);
                        break;
                    }
                }
                if cache_dirty && !worker_cancelled.load(Ordering::Acquire) {
                    let _ = write_waveform_cache(&cache_path, &cache_entries);
                }
                store.close().map_err(|error| error.to_string())?;
                Ok(())
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            cancelled,
            results,
            thread: Some(thread),
        })
    }

    /// Requests cancellation after the current decoder packet.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Drains decoded assets without waiting for the worker.
    pub fn results(&self) -> Vec<AudioWaveformResult> {
        self.results.try_iter().collect()
    }

    /// Returns whether decoding and store closure have completed.
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Joins after completion and returns any project-store error.
    pub fn join(mut self) -> Result<(), String> {
        self.thread
            .take()
            .expect("waveform thread is present")
            .join()
            .map_err(|_| "audio waveform worker panicked".to_owned())?
    }
}

fn waveform_cache_path(project_path: &std::path::Path) -> PathBuf {
    project_path.with_extension("aaapeaks")
}

fn read_waveform_cache(
    path: &std::path::Path,
    cancelled: &AtomicBool,
) -> Result<Vec<AudioWaveformCacheEntry>, String> {
    const MAX_CACHE_BYTES: usize = 128 * 1024 * 1024;
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::with_capacity(1024 * 1024);
    let mut buffer = [0; 64 * 1024];
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(MediaError::WorkerCancelled.to_string());
        }
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        if bytes.len().saturating_add(count) > MAX_CACHE_BYTES {
            return Err("waveform cache exceeds 128 MiB".to_owned());
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    decode_waveform_cache(&bytes).map_err(|error| error.to_string())
}

fn write_waveform_cache(
    path: &std::path::Path,
    entries: &HashMap<String, AudioWaveformCacheEntry>,
) -> Result<(), String> {
    let mut entries = entries.values().cloned().collect::<Vec<_>>();
    entries.sort_unstable_by(|left, right| left.media_ref.cmp(&right.media_ref));
    let bytes = encode_waveform_cache(&entries).map_err(|error| error.to_string())?;
    let file_name = path
        .file_name()
        .ok_or_else(|| "waveform cache path has no file name".to_owned())?;
    let temporary_path = path.with_file_name(format!(
        ".{}.{}.{}.partial",
        file_name.to_string_lossy(),
        std::process::id(),
        NEXT_CACHE_WRITE_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary_path)
            .map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        replace_cache_file(&temporary_path, path).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

#[cfg(not(windows))]
fn replace_cache_file(
    temporary_path: &std::path::Path,
    path: &std::path::Path,
) -> std::io::Result<()> {
    fs::rename(temporary_path, path)
}

#[cfg(windows)]
fn replace_cache_file(
    temporary_path: &std::path::Path,
    path: &std::path::Path,
) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let temporary_path = temporary_path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let path = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: both paths are NUL-terminated UTF-16 buffers that stay alive during the call.
    let replaced = unsafe {
        MoveFileExW(
            temporary_path.as_ptr(),
            path.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn source_fingerprint(
    store: &ProjectStore,
    media_ref: &str,
    cancelled: &AtomicBool,
) -> Result<Option<SourceFingerprint>, String> {
    match store
        .resolve_audio_asset(media_ref)
        .map_err(|error| error.to_string())?
    {
        ResolvedAudioAsset::Embedded(reader) => match store
            .audio_asset_content_hash(media_ref)
            .map_err(|error| error.to_string())?
        {
            Some(hash) => Ok(Some(SourceFingerprint {
                hash,
                linked_source: None,
            })),
            None => hash_reader(reader, cancelled).map(|hash| {
                Some(SourceFingerprint {
                    hash,
                    linked_source: None,
                })
            }),
        },
        ResolvedAudioAsset::LinkedFile { path, .. } => {
            if cancelled.load(Ordering::Acquire) {
                return Err(MediaError::WorkerCancelled.to_string());
            }
            let Some(path_bytes) = path.to_str().map(str::as_bytes) else {
                return Ok(None);
            };
            let before = fs::metadata(&path).map_err(|error| error.to_string())?;
            let mut file = File::open(&path).map_err(|error| error.to_string())?;
            let mut hasher = Sha256::new();
            hasher.update(path_bytes);
            hash_reader_into(&mut file, &mut hasher, cancelled)?;
            let after = fs::metadata(&path).map_err(|error| error.to_string())?;
            let (Ok(before_modified), Ok(after_modified)) = (before.modified(), after.modified())
            else {
                return Ok(None);
            };
            if before.len() != after.len() || before_modified != after_modified {
                return Ok(None);
            }
            file.seek(SeekFrom::Start(0))
                .map_err(|error| error.to_string())?;
            Ok(Some(SourceFingerprint {
                hash: hasher.finalize().into(),
                linked_source: Some(LinkedSourceState {
                    path,
                    byte_len: after.len(),
                    modified: after_modified,
                    file,
                }),
            }))
        }
    }
}

fn linked_source_unchanged(
    store: &ProjectStore,
    media_ref: &str,
    fingerprint: &SourceFingerprint,
) -> bool {
    let Some(expected) = &fingerprint.linked_source else {
        return true;
    };
    let Ok(ResolvedAudioAsset::LinkedFile { path, .. }) = store.resolve_audio_asset(media_ref)
    else {
        return false;
    };
    let Ok(metadata) = fs::metadata(&path) else {
        return false;
    };
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    expected.path == path && expected.byte_len == metadata.len() && expected.modified == modified
}

fn hash_reader(mut reader: impl Read, cancelled: &AtomicBool) -> Result<[u8; 32], String> {
    let mut hasher = Sha256::new();
    hash_reader_into(&mut reader, &mut hasher, cancelled)?;
    Ok(hasher.finalize().into())
}

fn hash_reader_into(
    reader: &mut impl Read,
    hasher: &mut Sha256,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    let mut buffer = [0; 64 * 1024];
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(MediaError::WorkerCancelled.to_string());
        }
        let count = reader
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if count == 0 {
            return Ok(());
        }
        hasher.update(&buffer[..count]);
    }
}

impl Drop for AudioWaveformWorker {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl std::fmt::Debug for AudioWaveformWorker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AudioWaveformWorker(..)")
    }
}

fn decode_asset(
    store: &ProjectStore,
    media_ref: &str,
    linked_file: Option<&File>,
    cancelled: &AtomicBool,
) -> Result<AudioWaveform, MediaError> {
    let mut decoder = match store.resolve_audio_asset(media_ref).map_err(|error| {
        MediaError::WorkerStartupFailed {
            io_kind: None,
            message: error.to_string(),
        }
    })? {
        ResolvedAudioAsset::Embedded(reader) => {
            let byte_len = reader.byte_len();
            let extension = std::path::Path::new(reader.original_name())
                .extension()
                .and_then(|value| value.to_str())
                .map(str::to_owned);
            AudioStreamDecoder::from_reader(reader, Some(byte_len), extension.as_deref())?
        }
        ResolvedAudioAsset::LinkedFile { path, .. } => match linked_file {
            Some(file) => {
                let file = file.try_clone().map_err(MediaError::Io)?;
                let byte_len = file.metadata().map_err(MediaError::Io)?.len();
                let extension = path.extension().and_then(|extension| extension.to_str());
                AudioStreamDecoder::from_reader(file, Some(byte_len), extension)?
            }
            None => AudioStreamDecoder::open(path)?,
        },
    };
    let metadata = decoder.metadata();
    let source_frames = metadata.frame_count.or_else(|| {
        metadata
            .sample_rate
            .zip(metadata.duration_nanos)
            .and_then(|(rate, nanos)| {
                u64::try_from(u128::from(rate) * u128::from(nanos) / 1_000_000_000).ok()
            })
    });
    let frames_per_peak = source_frames
        .map(|frames| {
            frames
                .div_ceil(MAX_PEAKS_PER_ASSET)
                .max(u64::from(FRAMES_PER_PEAK))
        })
        .and_then(|frames| u32::try_from(frames).ok())
        .unwrap_or(FRAMES_PER_PEAK);
    AudioWaveform::decode_with_cancel(&mut decoder, frames_per_peak, || {
        cancelled.load(Ordering::Acquire)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aaadaw_storage::ProjectStore;
    use std::io::Cursor;
    use std::sync::atomic::AtomicU64;

    static NEXT_CACHE_FILE: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn cache_read_honors_cancellation() {
        let id = NEXT_CACHE_FILE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "aaadaw-cancelled-waveform-cache-{}-{id}.aaapeaks",
            std::process::id()
        ));
        fs::write(&path, b"cache data").unwrap();
        let cancelled = AtomicBool::new(true);
        let error = read_waveform_cache(&path, &cancelled).unwrap_err();
        assert_eq!(error, MediaError::WorkerCancelled.to_string());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn cancelled_scan_publishes_no_waveform_or_partial_cache() {
        let id = NEXT_CACHE_FILE.fetch_add(1, Ordering::Relaxed);
        let project_path = std::env::temp_dir().join(format!(
            "aaadaw-cancelled-waveform-project-{}-{id}.aaadaw",
            std::process::id()
        ));
        let mut store = ProjectStore::open(&project_path).unwrap();
        store
            .import_audio_asset(
                "asset://cancelled",
                "cancelled.wav",
                Cursor::new(b"fixture bytes".to_vec()),
            )
            .unwrap();
        store.close().unwrap();

        let cancelled = Arc::new(AtomicBool::new(true));
        let worker = AudioWaveformWorker::start_with_cancelled(
            project_path.clone(),
            vec!["asset://cancelled".to_owned()],
            cancelled,
        )
        .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !worker.is_finished() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(
            worker.is_finished(),
            "cancelled worker should finish promptly"
        );
        assert!(worker.results().is_empty());
        worker.join().unwrap();
        assert!(!waveform_cache_path(&project_path).exists());

        let _ = fs::remove_file(&project_path);
        for suffix in ["-wal", "-shm"] {
            let sidecar = format!("{}{suffix}", project_path.display());
            let _ = fs::remove_file(sidecar);
        }
    }
}
