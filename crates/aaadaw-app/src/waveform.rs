use aaadaw_media::{AudioStreamDecoder, AudioWaveform, MediaError};
use aaadaw_storage::{ProjectStore, ResolvedAudioAsset};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};

const FRAMES_PER_PEAK: u32 = 256;
const MAX_PEAKS_PER_ASSET: u64 = 32_768;

/// One decoded media overview, or its per-asset failure.
#[derive(Clone, Debug)]
pub struct AudioWaveformResult {
    /// Stable media reference in the project store.
    pub media_ref: String,
    /// Decoded peaks, or a message suitable for the project status line.
    pub waveform: Result<Arc<AudioWaveform>, String>,
}

/// Decodes project audio overviews off the UI and realtime audio threads.
pub struct AudioWaveformWorker {
    cancelled: Arc<AtomicBool>,
    results: Receiver<AudioWaveformResult>,
    thread: Option<JoinHandle<Result<(), String>>>,
}

impl AudioWaveformWorker {
    /// Starts decoding each unique media reference from a saved project store.
    pub fn start(project_path: PathBuf, mut media_refs: Vec<String>) -> Result<Self, String> {
        media_refs.sort_unstable();
        media_refs.dedup();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let (sender, results) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("aaadaw-audio-waveforms".to_owned())
            .spawn(move || {
                let store = ProjectStore::open(project_path).map_err(|error| error.to_string())?;
                for media_ref in media_refs {
                    if worker_cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    let waveform = match decode_asset(&store, &media_ref, &worker_cancelled) {
                        Ok(waveform) => Ok(Arc::new(waveform)),
                        Err(MediaError::WorkerCancelled)
                            if worker_cancelled.load(Ordering::Acquire) =>
                        {
                            break;
                        }
                        Err(error) => Err(error.to_string()),
                    };
                    if sender
                        .send(AudioWaveformResult {
                            media_ref,
                            waveform,
                        })
                        .is_err()
                    {
                        worker_cancelled.store(true, Ordering::Release);
                        break;
                    }
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
        ResolvedAudioAsset::LinkedFile { path, .. } => AudioStreamDecoder::open(path)?,
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
