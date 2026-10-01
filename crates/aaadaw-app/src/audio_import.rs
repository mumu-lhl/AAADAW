//! Background audio import orchestration and AudioItem action construction.

use aaadaw_core::{DawAction, TrackId};
use aaadaw_media::{AudioStreamDecoder, MediaError};
use aaadaw_storage::{
    AudioAssetImportProgress, AudioAssetImportWorker, AudioAssetMetadata, ProjectStore,
    ResolvedAudioAsset, StorageError,
};
use std::error::Error as StdError;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

/// Progress after a bounded audio-asset import batch has committed.
pub type AudioItemImportProgress = AudioAssetImportProgress;

/// Failures while importing an audio asset and preparing its project item action.
#[derive(Debug)]
pub enum AudioItemImportError {
    Storage(StorageError),
    Media(MediaError),
    ProjectFileMissing(PathBuf),
    InvalidProjectSampleRate,
    DurationUnavailable,
    DurationOutOfRange,
    ZeroLengthAudio,
    AssetNotEmbedded(String),
}

impl fmt::Display for AudioItemImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "audio import storage failed: {error}"),
            Self::Media(error) => write!(formatter, "imported audio could not be probed: {error}"),
            Self::ProjectFileMissing(path) => {
                write!(formatter, "project file {} does not exist", path.display())
            }
            Self::InvalidProjectSampleRate => {
                formatter.write_str("project sample rate must be greater than zero")
            }
            Self::DurationUnavailable => {
                formatter.write_str("audio duration is unavailable in decoder metadata")
            }
            Self::DurationOutOfRange => {
                formatter.write_str("audio duration exceeds the project sample clock")
            }
            Self::ZeroLengthAudio => {
                formatter.write_str("audio duration rounds to zero project samples")
            }
            Self::AssetNotEmbedded(media_ref) => {
                write!(
                    formatter,
                    "newly imported asset {media_ref:?} is not embedded"
                )
            }
        }
    }
}

impl StdError for AudioItemImportError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::Media(error) => Some(error),
            Self::ProjectFileMissing(_)
            | Self::InvalidProjectSampleRate
            | Self::DurationUnavailable
            | Self::DurationOutOfRange
            | Self::ZeroLengthAudio
            | Self::AssetNotEmbedded(_) => None,
        }
    }
}

impl From<StorageError> for AudioItemImportError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl From<MediaError> for AudioItemImportError {
    fn from(error: MediaError) -> Self {
        Self::Media(error)
    }
}

/// A cancellable import that can be polled without blocking the UI thread.
///
/// Start and finish this operation on background control workers. `finish` consumes the worker
/// after `is_finished` becomes true, probes the immutable embedded snapshot, persists metadata,
/// and returns the `DawAction` that places it in the project. Dropping the worker cancels import.
pub struct AudioItemImportWorker {
    worker: AudioAssetImportWorker,
    project_path: PathBuf,
    track_id: TrackId,
    start_sample: u64,
    project_sample_rate: u32,
}

impl AudioItemImportWorker {
    /// Receives progress for committed batches of embedded asset bytes.
    pub fn progress(&self) -> &Receiver<AudioItemImportProgress> {
        self.worker.progress()
    }

    /// Requests cancellation at the next bounded storage batch.
    pub fn cancel(&self) {
        self.worker.cancel();
    }

    /// Returns whether the storage import thread exited, without blocking.
    pub fn is_finished(&self) -> bool {
        self.worker.is_finished()
    }

    /// Finishes metadata preparation and returns an undoable project placement action.
    ///
    /// Call only after `is_finished` returns true; the storage join is then non-blocking, while
    /// metadata probing and SQLite access still belong on a background control worker.
    pub fn finish(self) -> Result<DawAction, AudioItemImportError> {
        let media_ref = self.worker.join()?;
        let mut store = ProjectStore::open(&self.project_path)?;
        let metadata_result = load_or_probe_metadata(&mut store, &media_ref);
        let close_result = store.close();
        let metadata = metadata_result?;
        close_result?;
        let length_samples = audio_item_length_samples(&metadata, self.project_sample_rate)?;

        Ok(DawAction::InsertAudioItem {
            track_id: self.track_id,
            media_ref,
            start_sample: self.start_sample,
            source_offset_samples: 0,
            length_samples,
        })
    }
}

/// Starts embedding an audio file in a saved project.
///
/// The returned worker reports committed-byte progress, supports cancellation, and must be
/// polled/finished from a control thread. The supplied track and start position are only used to
/// construct the final action; this function never mutates project state.
pub fn start_audio_item_import(
    project_path: impl AsRef<Path>,
    source_path: impl AsRef<Path>,
    track_id: TrackId,
    start_sample: u64,
    project_sample_rate: u32,
) -> Result<AudioItemImportWorker, AudioItemImportError> {
    if project_sample_rate == 0 {
        return Err(AudioItemImportError::InvalidProjectSampleRate);
    }
    let project_path = project_path.as_ref().to_owned();
    if !project_path.is_file() {
        return Err(AudioItemImportError::ProjectFileMissing(project_path));
    }
    let source_path = source_path.as_ref().to_owned();
    AudioStreamDecoder::open(&source_path)?;
    let store = ProjectStore::open(&project_path)?;
    let worker = store.start_audio_asset_import(&source_path)?;
    // The worker writes through its own connection; do not checkpoint against it while active.
    drop(store);

    Ok(AudioItemImportWorker {
        worker,
        project_path,
        track_id,
        start_sample,
        project_sample_rate,
    })
}

fn load_or_probe_metadata(
    store: &mut ProjectStore,
    media_ref: &str,
) -> Result<AudioAssetMetadata, AudioItemImportError> {
    if let Some(metadata) = store.audio_asset_metadata(media_ref)? {
        return Ok(metadata);
    }
    let ResolvedAudioAsset::Embedded(reader) = store.resolve_audio_asset(media_ref)? else {
        return Err(AudioItemImportError::AssetNotEmbedded(media_ref.to_owned()));
    };
    let byte_len = reader.byte_len();
    let extension = Path::new(reader.original_name())
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_owned);
    let metadata = AudioStreamDecoder::from_reader(reader, Some(byte_len), extension.as_deref())?
        .metadata()
        .clone();
    let metadata = AudioAssetMetadata {
        container: metadata.container,
        codec: metadata.codec,
        sample_rate: metadata.sample_rate,
        channel_count: metadata.channel_count,
        bits_per_sample: metadata.bits_per_sample,
        frame_count: metadata.frame_count,
        duration_nanos: metadata.duration_nanos,
        byte_len: metadata.byte_len,
    };
    store.set_audio_asset_metadata(media_ref, &metadata)?;
    Ok(metadata)
}

fn audio_item_length_samples(
    metadata: &AudioAssetMetadata,
    target_sample_rate: u32,
) -> Result<u64, AudioItemImportError> {
    if target_sample_rate == 0 {
        return Err(AudioItemImportError::InvalidProjectSampleRate);
    }
    let length = match (metadata.frame_count, metadata.sample_rate) {
        (Some(frame_count), Some(source_sample_rate)) if source_sample_rate > 0 => {
            (u128::from(frame_count) * u128::from(target_sample_rate)
                + u128::from(source_sample_rate) / 2)
                / u128::from(source_sample_rate)
        }
        _ => {
            let duration_nanos = metadata
                .duration_nanos
                .ok_or(AudioItemImportError::DurationUnavailable)?;
            (u128::from(duration_nanos) * u128::from(target_sample_rate) + 500_000_000)
                / 1_000_000_000
        }
    };
    let length = u64::try_from(length).map_err(|_| AudioItemImportError::DurationOutOfRange)?;
    if length == 0 {
        return Err(AudioItemImportError::ZeroLengthAudio);
    }
    Ok(length)
}
