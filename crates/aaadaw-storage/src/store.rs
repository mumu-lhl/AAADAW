use aaadaw_core::{
    AudioItemSnapshot, MeterPointSnapshot, MidiItemSnapshot, MidiNoteData, MidiNoteSnapshot,
    Project, ProjectSettings, ProjectSnapshot, SnapshotError, TempoCurve, TempoPointSnapshot,
    TrackFxPluginSnapshot, TrackInstrumentSnapshot, TrackSnapshot,
};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior, params,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Latest database schema version understood by this release.
pub const CURRENT_SCHEMA_VERSION: u32 = 5;
const APPLICATION_ID: i64 = 0x4141_4441;
const PAGE_SIZE: u32 = 4096;

const MIGRATION_1: &str = r#"
CREATE TABLE project_meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    sample_rate INTEGER NOT NULL CHECK (sample_rate > 0),
    ppq INTEGER NOT NULL CHECK (ppq > 0),
    initial_tempo_bpm REAL NOT NULL CHECK (initial_tempo_bpm > 0)
);

CREATE TABLE tracks (
    id INTEGER PRIMARY KEY CHECK (id >= 0),
    position INTEGER NOT NULL UNIQUE CHECK (position >= 0),
    name TEXT NOT NULL,
    volume_db REAL NOT NULL,
    pan REAL NOT NULL CHECK (pan >= -1 AND pan <= 1),
    muted INTEGER NOT NULL DEFAULT 0 CHECK (muted IN (0, 1)),
    solo INTEGER NOT NULL DEFAULT 0 CHECK (solo IN (0, 1))
);

CREATE TABLE items (
    id INTEGER PRIMARY KEY CHECK (id >= 0),
    track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    start_tick INTEGER NOT NULL CHECK (start_tick >= 0),
    length_ticks INTEGER NOT NULL CHECK (length_ticks > 0),
    UNIQUE (track_id, position)
);

CREATE INDEX items_by_project_position ON items(position, id);

CREATE TABLE midi_notes (
    id INTEGER PRIMARY KEY CHECK (id >= 0),
    item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    pitch INTEGER NOT NULL CHECK (pitch BETWEEN 0 AND 127),
    tick INTEGER NOT NULL CHECK (tick >= 0),
    duration INTEGER NOT NULL CHECK (duration > 0),
    velocity INTEGER NOT NULL CHECK (velocity BETWEEN 0 AND 127),
    UNIQUE (item_id, position)
);

CREATE INDEX midi_notes_by_item_position ON midi_notes(item_id, position);

CREATE TABLE tempo_points (
    start_tick INTEGER PRIMARY KEY CHECK (start_tick >= 0),
    bpm REAL NOT NULL CHECK (bpm > 0)
);

CREATE TABLE meter_points (
    start_tick INTEGER PRIMARY KEY CHECK (start_tick >= 0),
    numerator INTEGER NOT NULL CHECK (numerator > 0),
    denominator INTEGER NOT NULL CHECK (denominator > 0)
);
"#;

const MIGRATION_2: &str = r#"
ALTER TABLE tempo_points
ADD COLUMN curve_to_next INTEGER NOT NULL DEFAULT 0 CHECK (curve_to_next IN (0, 1));
"#;

const MIGRATION_3: &str = r#"
ALTER TABLE tracks ADD COLUMN instrument_id TEXT
    CHECK (instrument_id IS NULL OR length(trim(instrument_id)) > 0);
ALTER TABLE tracks ADD COLUMN instrument_path TEXT
    CHECK (instrument_path IS NULL OR length(trim(instrument_path)) > 0);
"#;

const MIGRATION_4: &str = r#"
CREATE TABLE track_fx_plugins (
    track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    plugin_id TEXT NOT NULL CHECK (length(trim(plugin_id)) > 0),
    plugin_path TEXT NOT NULL CHECK (length(trim(plugin_path)) > 0),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    PRIMARY KEY (track_id, position)
);
"#;

const MIGRATION_5: &str = r#"
ALTER TABLE tracks ADD COLUMN record_armed INTEGER NOT NULL DEFAULT 0 CHECK (record_armed IN (0, 1));
"#;

const AUDIO_ASSET_CHUNK_SIZE: usize = 256 * 1024;
const AUDIO_ASSET_IMPORT_BATCH_CHUNKS: usize = 16;

const AUDIO_ASSETS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS audio_assets (
    media_ref TEXT PRIMARY KEY CHECK (length(trim(media_ref)) > 0),
    original_name TEXT NOT NULL,
    byte_len INTEGER NOT NULL CHECK (byte_len >= 0),
    chunk_count INTEGER NOT NULL CHECK (chunk_count >= 0),
    chunk_size INTEGER NOT NULL CHECK (chunk_size > 0),
    source_path TEXT,
    content_hash BLOB,
    storage_key TEXT,
    import_state INTEGER NOT NULL DEFAULT 1 CHECK (import_state IN (0, 1))
);
CREATE TABLE IF NOT EXISTS audio_asset_storage_chunks (
    storage_key TEXT NOT NULL,
    chunk_index INTEGER NOT NULL CHECK (chunk_index >= 0),
    data BLOB NOT NULL CHECK (length(data) > 0),
    PRIMARY KEY (storage_key, chunk_index)
);
CREATE TABLE IF NOT EXISTS audio_asset_metadata (
    media_ref TEXT PRIMARY KEY REFERENCES audio_assets(media_ref) ON DELETE CASCADE,
    container TEXT NOT NULL CHECK (length(trim(container)) > 0),
    codec TEXT NOT NULL CHECK (length(trim(codec)) > 0),
    sample_rate INTEGER CHECK (sample_rate IS NULL OR sample_rate > 0),
    channel_count INTEGER CHECK (channel_count IS NULL OR channel_count > 0),
    bits_per_sample INTEGER CHECK (bits_per_sample IS NULL OR bits_per_sample > 0),
    frame_count INTEGER CHECK (frame_count IS NULL OR frame_count >= 0),
    duration_nanos INTEGER CHECK (duration_nanos IS NULL OR duration_nanos >= 0),
    byte_len INTEGER CHECK (byte_len IS NULL OR byte_len >= 0)
);
CREATE TABLE IF NOT EXISTS audio_asset_links (
    media_ref TEXT PRIMARY KEY CHECK (length(trim(media_ref)) > 0),
    original_name TEXT NOT NULL,
    external_path TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS audio_asset_chunks (
    media_ref TEXT NOT NULL REFERENCES audio_assets(media_ref) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL CHECK (chunk_index >= 0),
    data BLOB NOT NULL CHECK (length(data) > 0),
    PRIMARY KEY (media_ref, chunk_index)
);
"#;

const AUDIO_ITEMS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS audio_items (
    id INTEGER PRIMARY KEY CHECK (id >= 0),
    track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    media_ref TEXT NOT NULL CHECK (length(trim(media_ref)) > 0),
    start_sample INTEGER NOT NULL CHECK (start_sample >= 0),
    source_offset_samples INTEGER NOT NULL CHECK (source_offset_samples >= 0),
    length_samples INTEGER NOT NULL CHECK (length_samples > 0),
    UNIQUE (track_id, position)
);
CREATE INDEX IF NOT EXISTS audio_items_by_project_position ON audio_items(position, id);
"#;

/// SQLite persistence failures, including incompatible project files.
#[derive(Debug)]
pub enum StorageError {
    Sql(rusqlite::Error),
    Snapshot(SnapshotError),
    UnsupportedSchemaVersion { found: i64, current: u32 },
    WrongApplicationId(i64),
    MissingMigration(i64),
    InvalidStoredData(&'static str),
    IntegerOutOfRange(u64),
    UnsupportedJournalMode(String),
    CheckpointBusy,
    AudioAssetReferenceEmpty,
    AudioAssetAlreadyExists(String),
    AudioAssetNotFound(String),
    AudioAssetNotLinked(String),
    ExternalPathNotUtf8,
    EmptyAudioAsset,
    MemoryDatabaseHasNoIndependentAssetReader,
    MemoryDatabaseHasNoBackgroundAssetWorkers,
    AudioAssetImportCancelled,
    AudioAssetImportWorkerPanicked,
    AudioAssetPackCancelled,
    AudioAssetPackWorkerPanicked,
    AudioAssetSourceScanCancelled,
    AudioAssetSourceScanWorkerPanicked,
    Io(io::Error),
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sql(error) => write!(formatter, "SQLite error: {error}"),
            Self::Snapshot(error) => write!(formatter, "invalid project snapshot: {error}"),
            Self::UnsupportedSchemaVersion { found, current } => write!(
                formatter,
                "project schema version {found} is newer than supported version {current}"
            ),
            Self::WrongApplicationId(id) => {
                write!(
                    formatter,
                    "SQLite file has unexpected application id {id:#x}"
                )
            }
            Self::MissingMigration(version) => {
                write!(
                    formatter,
                    "no migration is defined from schema version {version}"
                )
            }
            Self::InvalidStoredData(reason) => {
                write!(formatter, "invalid project database: {reason}")
            }
            Self::IntegerOutOfRange(value) => {
                write!(
                    formatter,
                    "value {value} cannot be represented by SQLite INTEGER"
                )
            }
            Self::UnsupportedJournalMode(mode) => {
                write!(formatter, "SQLite refused WAL mode and selected {mode}")
            }
            Self::CheckpointBusy => formatter.write_str("WAL checkpoint could not complete"),
            Self::AudioAssetReferenceEmpty => {
                formatter.write_str("audio asset reference must not be empty")
            }
            Self::AudioAssetAlreadyExists(media_ref) => {
                write!(formatter, "audio asset {media_ref:?} already exists")
            }
            Self::AudioAssetNotFound(media_ref) => {
                write!(formatter, "audio asset {media_ref:?} was not found")
            }
            Self::AudioAssetNotLinked(media_ref) => {
                write!(
                    formatter,
                    "audio asset {media_ref:?} is not externally linked"
                )
            }
            Self::ExternalPathNotUtf8 => {
                formatter.write_str("external audio path is not valid UTF-8")
            }
            Self::EmptyAudioAsset => formatter.write_str("audio asset content must not be empty"),
            Self::MemoryDatabaseHasNoIndependentAssetReader => formatter.write_str(
                "an independent audio asset reader is unavailable for in-memory databases",
            ),
            Self::MemoryDatabaseHasNoBackgroundAssetWorkers => formatter
                .write_str("background audio asset workers require a file-backed project database"),
            Self::AudioAssetImportCancelled => formatter.write_str("audio asset import cancelled"),
            Self::AudioAssetImportWorkerPanicked => {
                formatter.write_str("audio asset import worker panicked")
            }
            Self::AudioAssetPackCancelled => formatter.write_str("audio asset pack cancelled"),
            Self::AudioAssetPackWorkerPanicked => {
                formatter.write_str("audio asset pack worker panicked")
            }
            Self::AudioAssetSourceScanCancelled => {
                formatter.write_str("audio asset source scan cancelled")
            }
            Self::AudioAssetSourceScanWorkerPanicked => {
                formatter.write_str("audio asset source scan worker panicked")
            }
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sql(error) => Some(error),
            Self::Snapshot(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for StorageError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sql(error)
    }
}

impl From<SnapshotError> for StorageError {
    fn from(error: SnapshotError) -> Self {
        Self::Snapshot(error)
    }
}

impl From<io::Error> for StorageError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Technical header metadata discovered by the media decoder for an embedded asset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioAssetMetadata {
    pub container: String,
    pub codec: String,
    pub sample_rate: Option<u32>,
    pub channel_count: Option<u32>,
    pub bits_per_sample: Option<u32>,
    pub frame_count: Option<u64>,
    pub duration_nanos: Option<u64>,
    pub byte_len: Option<u64>,
}

/// Status of the original external file recorded when an asset was imported.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioAssetSourceStatus {
    /// The asset currently resolves directly to an external file.
    Linked,
    /// The asset was imported from a reader with no original file path.
    Untracked,
    /// The file exists and matches the embedded bytes' SHA-256 fingerprint.
    Unchanged,
    /// The file exists but its content differs from the embedded snapshot.
    Changed,
    /// The recorded source path no longer exists.
    Missing,
    /// The asset predates source fingerprint metadata.
    Unverified,
}

/// Progress for a background audio-file import.
///
/// Bytes count committed to staging chunks; the asset stays unresolved until final publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioAssetImportProgress {
    pub bytes_imported: u64,
    pub total_bytes: u64,
}

/// Progress while packing external assets into the project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioAssetPackProgress {
    pub media_ref: String,
    pub bytes_imported: u64,
    pub total_bytes: u64,
    pub completed_assets: usize,
    pub total_assets: usize,
    pub asset_complete: bool,
}

/// Progress after inspecting one asset's original source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioAssetSourceScanProgress {
    pub media_ref: String,
    pub status: AudioAssetSourceStatus,
    pub scanned_assets: usize,
    pub total_assets: usize,
}

/// A cancellable background import which embeds one external audio file.
pub struct AudioAssetImportWorker {
    cancelled: Arc<AtomicBool>,
    progress: Receiver<AudioAssetImportProgress>,
    thread: Option<JoinHandle<Result<String, StorageError>>>,
}

impl AudioAssetImportWorker {
    /// Requests cancellation; the import cleans up at its next committed batch boundary.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Receives progress updates until the import thread closes the channel.
    pub fn progress(&self) -> &Receiver<AudioAssetImportProgress> {
        &self.progress
    }

    /// Returns whether the import thread has exited, without blocking the caller.
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Waits for the import and returns its new media reference.
    pub fn join(mut self) -> Result<String, StorageError> {
        self.thread
            .take()
            .expect("worker thread is joined once")
            .join()
            .map_err(|_| StorageError::AudioAssetImportWorkerPanicked)?
    }
}

impl Drop for AudioAssetImportWorker {
    fn drop(&mut self) {
        self.cancel();
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
    }
}

/// A cancellable background operation that packs all linked external assets.
pub struct AudioAssetPackWorker {
    cancelled: Arc<AtomicBool>,
    progress: Receiver<AudioAssetPackProgress>,
    thread: Option<JoinHandle<Result<Vec<String>, StorageError>>>,
}

impl AudioAssetPackWorker {
    /// Requests cancellation at the next chunk-batch boundary.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Receives per-asset progress until the worker closes the channel.
    pub fn progress(&self) -> &Receiver<AudioAssetPackProgress> {
        &self.progress
    }

    /// Returns whether the worker has exited without blocking the caller.
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Waits for the operation and returns references packed before completion.
    pub fn join(mut self) -> Result<Vec<String>, StorageError> {
        self.thread
            .take()
            .expect("worker thread is joined once")
            .join()
            .map_err(|_| StorageError::AudioAssetPackWorkerPanicked)?
    }
}

impl Drop for AudioAssetPackWorker {
    fn drop(&mut self) {
        self.cancel();
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
    }
}

type AudioAssetSourceScanResult = Vec<(String, AudioAssetSourceStatus)>;
type AudioAssetSourceScanThread = JoinHandle<Result<AudioAssetSourceScanResult, StorageError>>;

/// A cancellable background scan of imported and linked source files.
pub struct AudioAssetSourceScanWorker {
    cancelled: Arc<AtomicBool>,
    progress: Receiver<AudioAssetSourceScanProgress>,
    thread: Option<AudioAssetSourceScanThread>,
}

impl AudioAssetSourceScanWorker {
    /// Requests cancellation; hashing stops at the next 64 KiB read boundary.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Receives one result for each scanned asset until the worker closes the channel.
    pub fn progress(&self) -> &Receiver<AudioAssetSourceScanProgress> {
        &self.progress
    }

    /// Returns whether the worker has exited without blocking the caller.
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Waits for the scan and returns all source statuses collected before completion.
    pub fn join(mut self) -> Result<Vec<(String, AudioAssetSourceStatus)>, StorageError> {
        self.thread
            .take()
            .expect("worker thread is joined once")
            .join()
            .map_err(|_| StorageError::AudioAssetSourceScanWorkerPanicked)?
    }
}

impl Drop for AudioAssetSourceScanWorker {
    fn drop(&mut self) {
        self.cancel();
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
    }
}

/// A resolved source for a project media reference.
pub enum ResolvedAudioAsset {
    /// Immutable bytes embedded in the project database.
    Embedded(SqliteAudioAssetReader),
    /// A live external file; playback follows the current contents at this path.
    LinkedFile {
        path: PathBuf,
        original_name: String,
    },
}

/// Seekable, chunk-backed reader for an audio asset embedded in a project file.
///
/// Reads fetch bounded chunks from an independent read-only SQLite connection,
/// so a decoder can seek without loading the whole asset into memory.
pub struct SqliteAudioAssetReader {
    original_name: String,
    byte_len: u64,
    state: Mutex<AudioAssetReaderState>,
}

struct AudioAssetReaderState {
    connection: Connection,
    media_ref: String,
    storage_key: Option<String>,
    chunk_size: u64,
    position: u64,
    cached_chunk: Option<(u64, Vec<u8>)>,
}

impl SqliteAudioAssetReader {
    /// Returns the source file name recorded at import time.
    pub fn original_name(&self) -> &str {
        &self.original_name
    }

    /// Returns the embedded asset's byte length.
    pub fn byte_len(&self) -> u64 {
        self.byte_len
    }

    fn state_mut(&mut self) -> io::Result<&mut AudioAssetReaderState> {
        self.state
            .get_mut()
            .map_err(|_| io::Error::other("audio asset reader mutex was poisoned"))
    }
}

impl Read for SqliteAudioAssetReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let byte_len = self.byte_len;
        let state = self.state_mut()?;
        if output.is_empty() || state.position >= byte_len {
            return Ok(0);
        }

        let bytes_to_read = output
            .len()
            .min(usize::try_from(byte_len - state.position).unwrap_or(usize::MAX));
        let mut written = 0;
        while written < bytes_to_read {
            let chunk_index = state.position / state.chunk_size;
            let chunk_offset = (state.position % state.chunk_size) as usize;
            if state
                .cached_chunk
                .as_ref()
                .is_none_or(|(cached_index, _)| *cached_index != chunk_index)
            {
                let sql_chunk_index = i64::try_from(chunk_index).map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "audio asset chunk index overflow",
                    )
                })?;
                let chunk = if let Some(storage_key) = &state.storage_key {
                    state.connection.query_row(
                        "SELECT data FROM audio_asset_storage_chunks \
                             WHERE storage_key = ?1 AND chunk_index = ?2",
                        params![storage_key, sql_chunk_index],
                        |row| row.get::<_, Vec<u8>>(0),
                    )
                } else {
                    state.connection.query_row(
                        "SELECT data FROM audio_asset_chunks \
                             WHERE media_ref = ?1 AND chunk_index = ?2",
                        params![state.media_ref, sql_chunk_index],
                        |row| row.get::<_, Vec<u8>>(0),
                    )
                }
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                state.cached_chunk = Some((chunk_index, chunk));
            }

            let (_, chunk) = state.cached_chunk.as_ref().expect("chunk was just loaded");
            let Some(chunk_available) = chunk.len().checked_sub(chunk_offset) else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "audio asset chunk is shorter than its declared range",
                ));
            };
            if chunk_available == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "audio asset contains an empty or truncated chunk",
                ));
            }
            let amount = chunk_available.min(bytes_to_read - written);
            output[written..written + amount]
                .copy_from_slice(&chunk[chunk_offset..chunk_offset + amount]);
            written += amount;
            state.position += amount as u64;
        }
        Ok(written)
    }
}

impl Seek for SqliteAudioAssetReader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let byte_len = self.byte_len;
        let state = self.state_mut()?;
        let position = match from {
            SeekFrom::Start(position) => i128::from(position),
            SeekFrom::Current(offset) => i128::from(state.position) + i128::from(offset),
            SeekFrom::End(offset) => i128::from(byte_len) + i128::from(offset),
        };
        if position < 0 || position > i128::from(u64::MAX) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "audio asset seek is outside the supported range",
            ));
        }
        state.position = position as u64;
        Ok(state.position)
    }
}

/// Owns a SQLite connection for one `.aaadaw` project file.
///
/// Saves are atomic full-snapshot replacements. The connection uses WAL while
/// open; call [`ProjectStore::close`] to checkpoint and compact the project back
/// into a portable single file.
pub struct ProjectStore {
    connection: Connection,
    database_path: PathBuf,
}

impl ProjectStore {
    /// Opens or creates a project database and applies pending schema migrations.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let requested_path = path.as_ref().to_owned();
        let mut connection = Connection::open(&requested_path)?;
        let database_path =
            if requested_path == Path::new(":memory:") || requested_path.is_absolute() {
                requested_path
            } else {
                std::env::current_dir()?.join(requested_path)
            };
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "page_size", PAGE_SIZE)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        let journal_mode: String =
            connection.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
        if !journal_mode.eq_ignore_ascii_case("wal") {
            return Err(StorageError::UnsupportedJournalMode(journal_mode));
        }
        connection.pragma_update(None, "synchronous", 1_i64)?;
        connection.pragma_update(None, "wal_autocheckpoint", 1000_i64)?;
        connection.pragma_update(None, "foreign_keys", 1_i64)?;

        let application_id: i64 =
            connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
        if application_id != 0 && application_id != APPLICATION_ID {
            return Err(StorageError::WrongApplicationId(application_id));
        }
        connection.pragma_update(None, "application_id", APPLICATION_ID)?;
        migrate(&mut connection)?;
        Ok(Self {
            connection,
            database_path,
        })
    }

    fn new_asset_reference(&self) -> Result<String, StorageError> {
        for _ in 0..8 {
            let token: String =
                self.connection
                    .query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))?;
            let media_ref = format!("asset://{token}");
            let exists: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM audio_assets WHERE media_ref = ?1) \
                 OR EXISTS(SELECT 1 FROM audio_asset_links WHERE media_ref = ?1)",
                [&media_ref],
                |row| row.get(0),
            )?;
            if !exists {
                return Ok(media_ref);
            }
        }
        Err(StorageError::InvalidStoredData(
            "failed to allocate a unique audio asset reference",
        ))
    }

    /// Starts importing an external file on a background thread.
    ///
    /// Progress messages are emitted once per committed chunk batch. Dropping the worker
    /// requests cancellation and waits for the current bounded batch to finish.
    pub fn start_audio_asset_import(
        &self,
        source_path: impl AsRef<Path>,
    ) -> Result<AudioAssetImportWorker, StorageError> {
        if self.database_path == Path::new(":memory:") {
            return Err(StorageError::MemoryDatabaseHasNoBackgroundAssetWorkers);
        }
        let project_path = self.database_path.clone();
        let source_path = source_path.as_ref().to_owned();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let (progress_sender, progress) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("aaadaw-asset-import".to_owned())
            .spawn(move || {
                let total_bytes = std::fs::metadata(&source_path)?.len();
                let mut store = ProjectStore::open(project_path)?;
                store.import_audio_file_with_progress(&source_path, |bytes_imported| {
                    if worker_cancelled.load(Ordering::Acquire) {
                        return Err(StorageError::AudioAssetImportCancelled);
                    }
                    progress_sender
                        .send(AudioAssetImportProgress {
                            bytes_imported,
                            total_bytes,
                        })
                        .map_err(|_| StorageError::AudioAssetImportCancelled)
                })
            })
            .map_err(StorageError::Io)?;
        Ok(AudioAssetImportWorker {
            cancelled,
            progress,
            thread: Some(thread),
        })
    }

    /// Starts packing every linked external asset into the project on a background thread.
    ///
    /// Each asset publishes independently; cancellation leaves completed assets packed and keeps
    /// the current/remaining external links available for retry.
    pub fn start_audio_asset_pack_all(&self) -> Result<AudioAssetPackWorker, StorageError> {
        if self.database_path == Path::new(":memory:") {
            return Err(StorageError::MemoryDatabaseHasNoBackgroundAssetWorkers);
        }
        let project_path = self.database_path.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let (progress_sender, progress) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("aaadaw-asset-pack".to_owned())
            .spawn(move || {
                let mut store = ProjectStore::open(project_path)?;
                store.pack_all_external_audio_assets_with_progress(|update| {
                    if !update.asset_complete && worker_cancelled.load(Ordering::Acquire) {
                        return Err(StorageError::AudioAssetPackCancelled);
                    }
                    progress_sender
                        .send(update)
                        .map_err(|_| StorageError::AudioAssetPackCancelled)
                })
            })
            .map_err(StorageError::Io)?;
        Ok(AudioAssetPackWorker {
            cancelled,
            progress,
            thread: Some(thread),
        })
    }

    /// Starts a background SHA-256 check of every embedded asset's original source.
    ///
    /// The worker also reports whether linked external files still exist. Hashing checks
    /// cancellation between 64 KiB reads.
    pub fn start_audio_asset_source_scan(
        &self,
    ) -> Result<AudioAssetSourceScanWorker, StorageError> {
        if self.database_path == Path::new(":memory:") {
            return Err(StorageError::MemoryDatabaseHasNoBackgroundAssetWorkers);
        }
        let project_path = self.database_path.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let (progress_sender, progress) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("aaadaw-asset-source-scan".to_owned())
            .spawn(move || {
                let store = ProjectStore::open(project_path)?;
                let media_refs = store.audio_asset_references()?;
                let total_assets = media_refs.len();
                let mut results = Vec::with_capacity(total_assets);
                for (index, media_ref) in media_refs.into_iter().enumerate() {
                    if worker_cancelled.load(Ordering::Acquire) {
                        return Err(StorageError::AudioAssetSourceScanCancelled);
                    }
                    let status = store.audio_asset_source_status_with_cancel(&media_ref, || {
                        worker_cancelled.load(Ordering::Acquire)
                    })?;
                    let update = AudioAssetSourceScanProgress {
                        media_ref: media_ref.clone(),
                        status,
                        scanned_assets: index + 1,
                        total_assets,
                    };
                    progress_sender
                        .send(update)
                        .map_err(|_| StorageError::AudioAssetSourceScanCancelled)?;
                    results.push((media_ref, status));
                }
                Ok(results)
            })
            .map_err(StorageError::Io)?;
        Ok(AudioAssetSourceScanWorker {
            cancelled,
            progress,
            thread: Some(thread),
        })
    }

    /// Imports an external file into the project and returns its opaque `asset://` reference.
    ///
    /// The source file is left untouched. Run this synchronous operation on a
    /// background thread; bounded batches keep SQLite write transactions short.
    pub fn import_audio_file(&mut self, path: impl AsRef<Path>) -> Result<String, StorageError> {
        self.import_audio_file_with_progress(path, |_| Ok(()))
    }

    /// Imports a file and reports bytes staged after each committed chunk batch.
    ///
    /// The callback runs on the importing thread and should return quickly. Returning an error
    /// cancels the import and removes its unpublished manifest and staged chunks.
    pub fn import_audio_file_with_progress(
        &mut self,
        path: impl AsRef<Path>,
        mut on_progress: impl FnMut(u64) -> Result<(), StorageError>,
    ) -> Result<String, StorageError> {
        let path = path.as_ref();
        let source_path = if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir()?.join(path)
        };
        let source_path = source_path.to_str();
        let source = File::open(path)?;
        let original_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "audio".to_owned());
        let media_ref = self.new_asset_reference()?;
        self.import_audio_asset_with_source_path(
            &media_ref,
            &original_name,
            source_path,
            source,
            &mut on_progress,
        )?;
        Ok(media_ref)
    }

    /// Registers a live external-file reference without copying its audio bytes into the project.
    ///
    /// The file's canonical absolute path is persisted. Use this opt-in mode for shared or very
    /// large sources; moving the file or project may require relinking it later.
    pub fn link_external_audio_file(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<String, StorageError> {
        let path = path.as_ref().canonicalize()?;
        let source = File::open(&path)?;
        if !source.metadata()?.is_file() {
            return Err(StorageError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "external audio source is not a regular file",
            )));
        }
        let external_path = path.to_str().ok_or(StorageError::ExternalPathNotUtf8)?;
        let original_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "audio".to_owned());
        let media_ref = self.new_asset_reference()?;
        self.connection.execute(
            "INSERT INTO audio_asset_links(media_ref, original_name, external_path) \
             VALUES(?1, ?2, ?3)",
            params![media_ref, original_name, external_path],
        )?;
        Ok(media_ref)
    }

    /// Resolves a media reference to embedded bytes or a live external path.
    pub fn resolve_audio_asset(&self, media_ref: &str) -> Result<ResolvedAudioAsset, StorageError> {
        let embedded: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM audio_assets WHERE media_ref = ?1 AND import_state = 1)",
            [media_ref],
            |row| row.get(0),
        )?;
        if embedded {
            return self
                .audio_asset_reader(media_ref)
                .map(ResolvedAudioAsset::Embedded);
        }
        let linked = self
            .connection
            .query_row(
                "SELECT original_name, external_path FROM audio_asset_links WHERE media_ref = ?1",
                [media_ref],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .ok_or_else(|| StorageError::AudioAssetNotFound(media_ref.to_owned()))?;
        Ok(ResolvedAudioAsset::LinkedFile {
            original_name: linked.0,
            path: PathBuf::from(linked.1),
        })
    }

    /// Changes the source path of an external link after user-confirmed relinking.
    pub fn relink_external_audio_file(
        &mut self,
        media_ref: &str,
        new_path: impl AsRef<Path>,
    ) -> Result<(), StorageError> {
        let path = new_path.as_ref().canonicalize()?;
        let source = File::open(&path)?;
        if !source.metadata()?.is_file() {
            return Err(StorageError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "external audio source is not a regular file",
            )));
        }
        let external_path = path.to_str().ok_or(StorageError::ExternalPathNotUtf8)?;
        let updated = self.connection.execute(
            "UPDATE audio_asset_links SET external_path = ?2 WHERE media_ref = ?1",
            params![media_ref, external_path],
        )?;
        if updated == 0 {
            return Err(StorageError::AudioAssetNotLinked(media_ref.to_owned()));
        }
        Ok(())
    }

    /// Embeds the current contents of an external link while keeping its media reference stable.
    pub fn pack_external_audio_asset(&mut self, media_ref: &str) -> Result<(), StorageError> {
        self.pack_external_audio_asset_with_progress(media_ref, |_, _| Ok(()))
    }

    /// Packs one external asset and reports staged bytes after each committed chunk batch.
    pub fn pack_external_audio_asset_with_progress(
        &mut self,
        media_ref: &str,
        mut on_progress: impl FnMut(u64, u64) -> Result<(), StorageError>,
    ) -> Result<(), StorageError> {
        let linked = self
            .connection
            .query_row(
                "SELECT original_name, external_path FROM audio_asset_links WHERE media_ref = ?1",
                [media_ref],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let Some((original_name, external_path)) = linked else {
            let embedded: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM audio_assets WHERE media_ref = ?1 AND import_state = 1)",
                [media_ref],
                |row| row.get(0),
            )?;
            return if embedded {
                Ok(())
            } else {
                Err(StorageError::AudioAssetNotLinked(media_ref.to_owned()))
            };
        };
        let already_embedded: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM audio_assets WHERE media_ref = ?1 AND import_state = 1)",
            [media_ref],
            |row| row.get(0),
        )?;
        if !already_embedded {
            let source = File::open(&external_path)?;
            let total_bytes = source.metadata()?.len();
            self.import_audio_asset_with_source_path(
                media_ref,
                &original_name,
                Some(&external_path),
                source,
                |bytes_imported| on_progress(bytes_imported, total_bytes),
            )?;
        }
        self.connection.execute(
            "DELETE FROM audio_asset_links WHERE media_ref = ?1",
            [media_ref],
        )?;
        Ok(())
    }

    /// Packs every currently linked external asset into the project.
    ///
    /// Each asset is packed independently. If one file is missing or unreadable,
    /// earlier assets remain embedded and the remaining links are safe to retry.
    pub fn pack_all_external_audio_assets(&mut self) -> Result<Vec<String>, StorageError> {
        self.pack_all_external_audio_assets_with_progress(|_| Ok(()))
    }

    /// Packs all linked assets, reporting per-asset progress and completed asset count.
    pub fn pack_all_external_audio_assets_with_progress(
        &mut self,
        mut on_progress: impl FnMut(AudioAssetPackProgress) -> Result<(), StorageError>,
    ) -> Result<Vec<String>, StorageError> {
        let mut statement = self
            .connection
            .prepare("SELECT media_ref FROM audio_asset_links ORDER BY media_ref")?;
        let media_refs = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);

        let total_assets = media_refs.len();
        let mut packed = Vec::with_capacity(total_assets);
        for media_ref in media_refs {
            let mut total_bytes = 0;
            self.pack_external_audio_asset_with_progress(&media_ref, |bytes_imported, total| {
                total_bytes = total;
                on_progress(AudioAssetPackProgress {
                    media_ref: media_ref.clone(),
                    bytes_imported,
                    total_bytes: total,
                    completed_assets: packed.len(),
                    total_assets,
                    asset_complete: false,
                })
            })?;
            packed.push(media_ref.clone());
            on_progress(AudioAssetPackProgress {
                media_ref,
                bytes_imported: total_bytes,
                total_bytes,
                completed_assets: packed.len(),
                total_assets,
                asset_complete: true,
            })?;
        }
        Ok(packed)
    }

    /// Imports an audio asset as bounded SQLite BLOB chunks under an immutable reference.
    ///
    /// The reader is consumed incrementally and at most one bounded batch is buffered in memory.
    /// Chunk batches commit independently; a pending manifest stays hidden until a final short
    /// transaction publishes the complete asset. Errors remove staged data when possible.
    pub fn import_audio_asset(
        &mut self,
        media_ref: &str,
        original_name: &str,
        source: impl Read,
    ) -> Result<u64, StorageError> {
        self.import_audio_asset_with_source_path(media_ref, original_name, None, source, |_| Ok(()))
    }

    fn import_audio_asset_with_source_path(
        &mut self,
        media_ref: &str,
        original_name: &str,
        source_path: Option<&str>,
        mut source: impl Read,
        mut on_progress: impl FnMut(u64) -> Result<(), StorageError>,
    ) -> Result<u64, StorageError> {
        if media_ref.trim().is_empty() {
            return Err(StorageError::AudioAssetReferenceEmpty);
        }
        let storage_key: String =
            self.connection
                .query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))?;
        {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            if transaction
                .query_row(
                    "SELECT 1 FROM audio_assets WHERE media_ref = ?1",
                    [media_ref],
                    |_| Ok(()),
                )
                .optional()?
                .is_some()
            {
                return Err(StorageError::AudioAssetAlreadyExists(media_ref.to_owned()));
            }
            transaction.execute(
                "INSERT INTO audio_assets(media_ref, original_name, byte_len, chunk_count, \
                 chunk_size, source_path, content_hash, storage_key, import_state) \
                 VALUES(?1, ?2, 0, 0, ?3, ?4, NULL, ?5, 0)",
                params![
                    media_ref,
                    original_name,
                    AUDIO_ASSET_CHUNK_SIZE as i64,
                    source_path,
                    storage_key
                ],
            )?;
            transaction.commit()?;
        }

        let import_result = (|| {
            on_progress(0)?;
            let mut chunk_buffer = vec![0; AUDIO_ASSET_CHUNK_SIZE];
            let mut pending_chunks = Vec::with_capacity(AUDIO_ASSET_IMPORT_BATCH_CHUNKS);
            let mut byte_len = 0_u64;
            let mut chunk_count = 0_usize;
            let mut content_hasher = Sha256::new();
            loop {
                let chunk_len = fill_chunk(&mut source, &mut chunk_buffer)?;
                if chunk_len == 0 {
                    break;
                }
                content_hasher.update(&chunk_buffer[..chunk_len]);
                byte_len = byte_len
                    .checked_add(chunk_len as u64)
                    .ok_or(StorageError::IntegerOutOfRange(u64::MAX))?;
                to_sql_integer(byte_len)?;
                pending_chunks.push(chunk_buffer[..chunk_len].to_vec());
                if pending_chunks.len() == AUDIO_ASSET_IMPORT_BATCH_CHUNKS {
                    write_audio_asset_chunk_batch(
                        &mut self.connection,
                        &storage_key,
                        chunk_count,
                        &pending_chunks,
                    )?;
                    chunk_count = chunk_count
                        .checked_add(pending_chunks.len())
                        .ok_or(StorageError::IntegerOutOfRange(u64::MAX))?;
                    pending_chunks.clear();
                    on_progress(byte_len)?;
                }
            }
            if !pending_chunks.is_empty() {
                write_audio_asset_chunk_batch(
                    &mut self.connection,
                    &storage_key,
                    chunk_count,
                    &pending_chunks,
                )?;
                chunk_count = chunk_count
                    .checked_add(pending_chunks.len())
                    .ok_or(StorageError::IntegerOutOfRange(u64::MAX))?;
                on_progress(byte_len)?;
            }
            if byte_len == 0 {
                return Err(StorageError::EmptyAudioAsset);
            }

            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            let updated = transaction.execute(
                "UPDATE audio_assets SET byte_len = ?2, chunk_count = ?3, content_hash = ?4, \
                 import_state = 1 WHERE media_ref = ?1 AND storage_key = ?5 AND import_state = 0",
                params![
                    media_ref,
                    to_sql_integer(byte_len)?,
                    usize_to_sql(chunk_count)?,
                    content_hasher.finalize().as_slice(),
                    storage_key
                ],
            )?;
            if updated != 1 {
                return Err(StorageError::InvalidStoredData(
                    "staged audio asset manifest disappeared before publication",
                ));
            }
            transaction.commit()?;
            Ok(byte_len)
        })();

        if import_result.is_err() {
            self.discard_incomplete_audio_asset(media_ref, &storage_key)?;
        }
        import_result
    }

    fn discard_incomplete_audio_asset(
        &mut self,
        media_ref: &str,
        storage_key: &str,
    ) -> Result<(), StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "DELETE FROM audio_asset_storage_chunks WHERE storage_key = ?1",
            [storage_key],
        )?;
        transaction.execute(
            "DELETE FROM audio_assets WHERE media_ref = ?1 AND storage_key = ?2 \
             AND import_state = 0",
            params![media_ref, storage_key],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Removes manifests and chunk data left by a process crash during import.
    ///
    /// Call only when no audio asset import workers are active for this project.
    pub fn cleanup_incomplete_audio_asset_imports(&mut self) -> Result<usize, StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let storage_keys = {
            let mut statement = transaction
                .prepare("SELECT storage_key FROM audio_assets WHERE import_state = 0")?;
            statement
                .query_map([], |row| row.get::<_, Option<String>>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        for storage_key in storage_keys.into_iter().flatten() {
            transaction.execute(
                "DELETE FROM audio_asset_storage_chunks WHERE storage_key = ?1",
                [storage_key],
            )?;
        }
        let removed = transaction.execute("DELETE FROM audio_assets WHERE import_state = 0", [])?;
        transaction.commit()?;
        Ok(removed)
    }

    /// Opens an independent reader that streams one embedded asset from SQLite chunks.
    pub fn audio_asset_reader(
        &self,
        media_ref: &str,
    ) -> Result<SqliteAudioAssetReader, StorageError> {
        if self.database_path == Path::new(":memory:") {
            return Err(StorageError::MemoryDatabaseHasNoIndependentAssetReader);
        }
        let connection =
            Connection::open_with_flags(&self.database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        let metadata = connection
            .query_row(
                "SELECT byte_len, chunk_count, chunk_size, original_name, storage_key, import_state \
                 FROM audio_assets WHERE media_ref = ?1 AND import_state = 1",
                [media_ref],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| StorageError::AudioAssetNotFound(media_ref.to_owned()))?;
        if metadata.5 != 1 {
            return Err(StorageError::AudioAssetNotFound(media_ref.to_owned()));
        }
        let byte_len = u64::try_from(metadata.0)
            .map_err(|_| StorageError::InvalidStoredData("negative audio asset byte length"))?;
        let chunk_count = u64::try_from(metadata.1)
            .map_err(|_| StorageError::InvalidStoredData("negative audio asset chunk count"))?;
        let chunk_size = u64::try_from(metadata.2)
            .ok()
            .filter(|size| *size > 0)
            .ok_or(StorageError::InvalidStoredData(
                "audio asset chunk size must be positive",
            ))?;
        let expected_chunks = byte_len.div_ceil(chunk_size);
        usize::try_from(chunk_size).map_err(|_| {
            StorageError::InvalidStoredData("audio asset chunk size exceeds platform limits")
        })?;
        if byte_len == 0 || expected_chunks != chunk_count {
            return Err(StorageError::InvalidStoredData(
                "audio asset metadata has an invalid length or chunk count",
            ));
        }
        Ok(SqliteAudioAssetReader {
            original_name: metadata.3,
            byte_len,
            state: Mutex::new(AudioAssetReaderState {
                connection,
                media_ref: media_ref.to_owned(),
                storage_key: metadata.4,
                chunk_size,
                position: 0,
                cached_chunk: None,
            }),
        })
    }

    /// Stores decoder-discovered metadata for a complete embedded asset.
    pub fn set_audio_asset_metadata(
        &mut self,
        media_ref: &str,
        metadata: &AudioAssetMetadata,
    ) -> Result<(), StorageError> {
        if metadata.container.trim().is_empty() || metadata.codec.trim().is_empty() {
            return Err(StorageError::InvalidStoredData(
                "audio metadata container and codec labels must not be empty",
            ));
        }
        let embedded: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM audio_assets WHERE media_ref = ?1 AND import_state = 1)",
            [media_ref],
            |row| row.get(0),
        )?;
        if !embedded {
            return Err(StorageError::AudioAssetNotFound(media_ref.to_owned()));
        }
        self.connection.execute(
            "INSERT INTO audio_asset_metadata(\
                media_ref, container, codec, sample_rate, channel_count, bits_per_sample, \
                frame_count, duration_nanos, byte_len\
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
             ON CONFLICT(media_ref) DO UPDATE SET \
                container = excluded.container, codec = excluded.codec, \
                sample_rate = excluded.sample_rate, channel_count = excluded.channel_count, \
                bits_per_sample = excluded.bits_per_sample, frame_count = excluded.frame_count, \
                duration_nanos = excluded.duration_nanos, byte_len = excluded.byte_len",
            params![
                media_ref,
                metadata.container,
                metadata.codec,
                metadata.sample_rate.map(i64::from),
                metadata.channel_count.map(i64::from),
                metadata.bits_per_sample.map(i64::from),
                metadata.frame_count.map(to_sql_integer).transpose()?,
                metadata.duration_nanos.map(to_sql_integer).transpose()?,
                metadata.byte_len.map(to_sql_integer).transpose()?
            ],
        )?;
        Ok(())
    }

    /// Returns metadata previously probed for a complete embedded asset.
    pub fn audio_asset_metadata(
        &self,
        media_ref: &str,
    ) -> Result<Option<AudioAssetMetadata>, StorageError> {
        let row = self
            .connection
            .query_row(
                "SELECT container, codec, sample_rate, channel_count, bits_per_sample, \
                 frame_count, duration_nanos, byte_len FROM audio_asset_metadata \
                 WHERE media_ref = ?1",
                [media_ref],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                        row.get::<_, Option<i64>>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                        row.get::<_, Option<i64>>(6)?,
                        row.get::<_, Option<i64>>(7)?,
                    ))
                },
            )
            .optional()?;
        row.map(
            |(
                container,
                codec,
                sample_rate,
                channel_count,
                bits_per_sample,
                frame_count,
                duration_nanos,
                byte_len,
            )| {
                Ok(AudioAssetMetadata {
                    container,
                    codec,
                    sample_rate: sample_rate.map(from_sql_u32).transpose()?,
                    channel_count: channel_count.map(from_sql_u32).transpose()?,
                    bits_per_sample: bits_per_sample.map(from_sql_u32).transpose()?,
                    frame_count: frame_count.map(from_sql_u64).transpose()?,
                    duration_nanos: duration_nanos.map(from_sql_u64).transpose()?,
                    byte_len: byte_len.map(from_sql_u64).transpose()?,
                })
            },
        )
        .transpose()
    }

    /// Re-hashes the recorded original file to determine whether it still matches the import snapshot.
    ///
    /// This reads the entire external file and should run on a background thread.
    pub fn audio_asset_source_status(
        &self,
        media_ref: &str,
    ) -> Result<AudioAssetSourceStatus, StorageError> {
        self.audio_asset_source_status_with_cancel(media_ref, || false)
    }

    /// Returns the recorded original path for an embedded asset, when one is available.
    pub fn audio_asset_source_path(
        &self,
        media_ref: &str,
    ) -> Result<Option<PathBuf>, StorageError> {
        let source_path = self
            .connection
            .query_row(
                "SELECT source_path FROM audio_assets WHERE media_ref = ?1 AND import_state = 1",
                [media_ref],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .ok_or_else(|| StorageError::AudioAssetNotFound(media_ref.to_owned()))?;
        Ok(source_path.map(PathBuf::from))
    }

    fn audio_asset_references(&self) -> Result<Vec<String>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT media_ref FROM audio_assets WHERE import_state = 1 \
             UNION SELECT media_ref FROM audio_asset_links ORDER BY media_ref",
        )?;
        Ok(statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?)
    }

    fn audio_asset_source_status_with_cancel(
        &self,
        media_ref: &str,
        mut is_cancelled: impl FnMut() -> bool,
    ) -> Result<AudioAssetSourceStatus, StorageError> {
        let metadata = self
            .connection
            .query_row(
                "SELECT source_path, content_hash FROM audio_assets \
                 WHERE media_ref = ?1 AND import_state = 1",
                [media_ref],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<Vec<u8>>>(1)?,
                    ))
                },
            )
            .optional()?;
        let Some(metadata) = metadata else {
            let external_path: Option<String> = self
                .connection
                .query_row(
                    "SELECT external_path FROM audio_asset_links WHERE media_ref = ?1",
                    [media_ref],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(external_path) = external_path else {
                return Err(StorageError::AudioAssetNotFound(media_ref.to_owned()));
            };
            return match File::open(external_path) {
                Ok(_) => Ok(AudioAssetSourceStatus::Linked),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    Ok(AudioAssetSourceStatus::Missing)
                }
                Err(error) => Err(StorageError::Io(error)),
            };
        };
        let Some(source_path) = metadata.0 else {
            return Ok(AudioAssetSourceStatus::Untracked);
        };
        let Some(expected_hash) = metadata.1 else {
            return Ok(AudioAssetSourceStatus::Unverified);
        };
        let expected_hash: [u8; 32] = expected_hash.try_into().map_err(|_| {
            StorageError::InvalidStoredData("audio asset SHA-256 fingerprint has an invalid length")
        })?;
        let mut source = match File::open(source_path) {
            Ok(source) => source,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(AudioAssetSourceStatus::Missing);
            }
            Err(error) => return Err(StorageError::Io(error)),
        };
        let actual_hash = sha256_reader_with_cancel(&mut source, &mut is_cancelled)?
            .ok_or(StorageError::AudioAssetSourceScanCancelled)?;
        if actual_hash == expected_hash {
            Ok(AudioAssetSourceStatus::Unchanged)
        } else {
            Ok(AudioAssetSourceStatus::Changed)
        }
    }

    /// Returns the schema version after migrations have been applied.
    pub fn schema_version(&self) -> Result<u32, StorageError> {
        let version: i64 = self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
        u32::try_from(version)
            .map_err(|_| StorageError::InvalidStoredData("negative schema version"))
    }

    /// Atomically replaces the stored project with the supplied project's state.
    pub fn save(&mut self, project: &Project) -> Result<(), StorageError> {
        let snapshot = project.snapshot();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        write_snapshot(&transaction, &snapshot)?;
        transaction.commit()?;
        Ok(())
    }

    /// Loads a project. A newly created, empty database yields a default project.
    pub fn load(&self) -> Result<Project, StorageError> {
        let metadata = self
            .connection
            .query_row(
                "SELECT sample_rate, ppq, initial_tempo_bpm FROM project_meta WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, f64>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((sample_rate, ppq, initial_tempo_bpm)) = metadata else {
            let rows: i64 = self.connection.query_row(
                "SELECT (SELECT COUNT(*) FROM tracks) + (SELECT COUNT(*) FROM items) + \
                 (SELECT COUNT(*) FROM midi_notes) + (SELECT COUNT(*) FROM audio_items) + \
                 (SELECT COUNT(*) FROM tempo_points) + (SELECT COUNT(*) FROM meter_points)",
                [],
                |row| row.get(0),
            )?;
            return if rows == 0 {
                Ok(Project::new())
            } else {
                Err(StorageError::InvalidStoredData(
                    "project rows exist without project metadata",
                ))
            };
        };

        let settings = ProjectSettings::new(
            from_sql_u32(sample_rate)?,
            from_sql_u32(ppq)?,
            initial_tempo_bpm,
        )
        .map_err(|error| StorageError::Snapshot(SnapshotError::InvalidTimebase(error)))?;

        let tracks = read_tracks(&self.connection)?;
        let audio_items = read_audio_items(&self.connection)?;
        let (midi_items, orphan_notes) = read_items_and_notes(&self.connection)?;
        if orphan_notes {
            return Err(StorageError::InvalidStoredData(
                "MIDI notes reference a missing item",
            ));
        }
        let tempo_points = read_tempo_points(&self.connection)?;
        let meter_points = read_meter_points(&self.connection)?;

        Project::from_snapshot(ProjectSnapshot {
            settings,
            tracks,
            audio_items,
            midi_items,
            tempo_points,
            meter_points,
        })
        .map_err(StorageError::Snapshot)
    }

    /// Checkpoints the WAL, keeping the database usable while this store remains open.
    pub fn checkpoint(&mut self) -> Result<(), StorageError> {
        let (busy, _log_frames, _checkpointed_frames): (i64, i64, i64) = self
            .connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
        if busy != 0 {
            return Err(StorageError::CheckpointBusy);
        }
        Ok(())
    }

    /// Checkpoints and closes the database so its contents are packaged in the main file.
    pub fn close(mut self) -> Result<(), StorageError> {
        self.checkpoint()?;
        drop(self);
        Ok(())
    }
}

fn ensure_audio_asset_metadata_columns(connection: &Connection) -> Result<(), StorageError> {
    for (column, definition) in [
        ("source_path", "TEXT"),
        ("content_hash", "BLOB"),
        ("storage_key", "TEXT"),
        (
            "import_state",
            "INTEGER NOT NULL DEFAULT 1 CHECK (import_state IN (0, 1))",
        ),
    ] {
        let mut statement = connection.prepare("PRAGMA table_info(audio_assets)")?;
        let mut rows = statement.query([])?;
        let mut found = false;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            found |= name == column;
        }
        if !found {
            connection.execute_batch(&format!(
                "ALTER TABLE audio_assets ADD COLUMN {column} {definition}"
            ))?;
        }
    }
    connection.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS audio_assets_storage_key \
         ON audio_assets(storage_key) WHERE storage_key IS NOT NULL",
    )?;
    Ok(())
}

fn sha256_reader_with_cancel(
    reader: &mut impl Read,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<Option<[u8; 32]>, io::Error> {
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        if is_cancelled() {
            return Ok(None);
        }
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    if is_cancelled() {
        Ok(None)
    } else {
        Ok(Some(hasher.finalize().into()))
    }
}

fn write_audio_asset_chunk_batch(
    connection: &mut Connection,
    storage_key: &str,
    first_chunk_index: usize,
    chunks: &[Vec<u8>],
) -> Result<(), StorageError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for (offset, chunk) in chunks.iter().enumerate() {
        let chunk_index = first_chunk_index
            .checked_add(offset)
            .ok_or(StorageError::IntegerOutOfRange(u64::MAX))?;
        transaction.execute(
            "INSERT INTO audio_asset_storage_chunks(storage_key, chunk_index, data) \
             VALUES(?1, ?2, ?3)",
            params![storage_key, usize_to_sql(chunk_index)?, chunk],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

fn fill_chunk(reader: &mut impl Read, buffer: &mut [u8]) -> Result<usize, io::Error> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..])? {
            0 => break,
            read => filled += read,
        }
    }
    Ok(filled)
}

fn migrate(connection: &mut Connection) -> Result<(), StorageError> {
    let mut version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > i64::from(CURRENT_SCHEMA_VERSION) {
        return Err(StorageError::UnsupportedSchemaVersion {
            found: version,
            current: CURRENT_SCHEMA_VERSION,
        });
    }
    while version < i64::from(CURRENT_SCHEMA_VERSION) {
        let next_version = version + 1;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Exclusive)?;
        match next_version {
            1 => transaction.execute_batch(MIGRATION_1)?,
            2 => transaction.execute_batch(MIGRATION_2)?,
            3 => transaction.execute_batch(MIGRATION_3)?,
            4 => transaction.execute_batch(MIGRATION_4)?,
            5 => transaction.execute_batch(MIGRATION_5)?,
            missing => return Err(StorageError::MissingMigration(missing - 1)),
        }
        transaction.pragma_update(None, "user_version", next_version)?;
        transaction.commit()?;
        version = next_version;
    }
    connection.execute_batch(AUDIO_ITEMS_TABLE)?;
    connection.execute_batch(AUDIO_ASSETS_TABLE)?;
    ensure_audio_asset_metadata_columns(connection)?;
    Ok(())
}

fn write_snapshot(
    transaction: &Transaction<'_>,
    snapshot: &ProjectSnapshot,
) -> Result<(), StorageError> {
    transaction.execute("DELETE FROM midi_notes", [])?;
    transaction.execute("DELETE FROM items", [])?;
    transaction.execute("DELETE FROM audio_items", [])?;
    transaction.execute("DELETE FROM tracks", [])?;
    transaction.execute("DELETE FROM tempo_points", [])?;
    transaction.execute("DELETE FROM meter_points", [])?;
    transaction.execute("DELETE FROM project_meta", [])?;

    let settings = snapshot.settings;
    transaction.execute(
        "INSERT INTO project_meta(singleton, sample_rate, ppq, initial_tempo_bpm) \
         VALUES(1, ?1, ?2, ?3)",
        params![
            i64::from(settings.sample_rate()),
            i64::from(settings.ppq()),
            settings.initial_tempo_bpm()
        ],
    )?;

    for (position, track) in snapshot.tracks.iter().enumerate() {
        transaction.execute(
            "INSERT INTO tracks(id, position, name, volume_db, pan, muted, solo, record_armed, instrument_id, instrument_path) \
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                to_sql_integer(track.id)?,
                usize_to_sql(position)?,
                track.name,
                f64::from(track.volume_db),
                f64::from(track.pan),
                track.muted,
                track.solo,
                track.record_armed,
                track.instrument.as_ref().map(|instrument| &instrument.plugin_id),
                track.instrument.as_ref().map(|instrument| &instrument.bundle_path)
            ],
        )?;
    }

    for track in &snapshot.tracks {
        for (position, plugin) in track.fx_chain.iter().enumerate() {
            transaction.execute(
                "INSERT INTO track_fx_plugins(track_id, position, plugin_id, plugin_path, enabled) \
                 VALUES(?1, ?2, ?3, ?4, ?5)",
                params![
                    to_sql_integer(track.id)?,
                    usize_to_sql(position)?,
                    plugin.plugin_id,
                    plugin.bundle_path,
                    plugin.enabled,
                ],
            )?;
        }
    }

    for (position, item) in snapshot.audio_items.iter().enumerate() {
        transaction.execute(
            "INSERT INTO audio_items(id, track_id, position, media_ref, start_sample, \
             source_offset_samples, length_samples) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                to_sql_integer(item.id)?,
                to_sql_integer(item.track_id)?,
                usize_to_sql(position)?,
                item.media_ref,
                to_sql_integer(item.start_sample)?,
                to_sql_integer(item.source_offset_samples)?,
                to_sql_integer(item.length_samples)?
            ],
        )?;
    }

    for (position, item) in snapshot.midi_items.iter().enumerate() {
        transaction.execute(
            "INSERT INTO items(id, track_id, position, start_tick, length_ticks) \
             VALUES(?1, ?2, ?3, ?4, ?5)",
            params![
                to_sql_integer(item.id)?,
                to_sql_integer(item.track_id)?,
                usize_to_sql(position)?,
                to_sql_integer(item.start_tick)?,
                to_sql_integer(item.length_ticks)?
            ],
        )?;
        for (position, note) in item.notes.iter().enumerate() {
            transaction.execute(
                "INSERT INTO midi_notes(id, item_id, position, pitch, tick, duration, velocity) \
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    to_sql_integer(note.id)?,
                    to_sql_integer(item.id)?,
                    usize_to_sql(position)?,
                    i64::from(note.data.pitch),
                    to_sql_integer(note.data.tick)?,
                    to_sql_integer(note.data.duration)?,
                    i64::from(note.data.velocity)
                ],
            )?;
        }
    }

    for point in &snapshot.tempo_points {
        transaction.execute(
            "INSERT INTO tempo_points(start_tick, bpm, curve_to_next) VALUES(?1, ?2, ?3)",
            params![
                to_sql_integer(point.start_tick)?,
                point.bpm,
                tempo_curve_to_sql(point.curve_to_next)
            ],
        )?;
    }
    for point in &snapshot.meter_points {
        transaction.execute(
            "INSERT INTO meter_points(start_tick, numerator, denominator) VALUES(?1, ?2, ?3)",
            params![
                to_sql_integer(point.start_tick)?,
                i64::from(point.numerator),
                i64::from(point.denominator)
            ],
        )?;
    }
    Ok(())
}

fn read_tracks(connection: &Connection) -> Result<Vec<TrackSnapshot>, StorageError> {
    let mut fx_chains = HashMap::<i64, Vec<TrackFxPluginSnapshot>>::new();
    let mut fx_statement = connection.prepare(
        "SELECT track_id, position, plugin_id, plugin_path, enabled \
         FROM track_fx_plugins ORDER BY track_id, position",
    )?;
    let fx_rows = fx_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, bool>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (track_id, position, plugin_id, bundle_path, enabled) in fx_rows {
        let _ = from_sql_u64(position)?;
        if plugin_id.trim().is_empty() || bundle_path.trim().is_empty() {
            return Err(StorageError::InvalidStoredData("track FX plugin reference"));
        }
        fx_chains
            .entry(track_id)
            .or_default()
            .push(TrackFxPluginSnapshot {
                plugin_id,
                bundle_path,
                enabled,
            });
    }

    let mut statement = connection.prepare(
        "SELECT id, position, name, volume_db, pan, muted, solo, record_armed, instrument_id, instrument_path \
         FROM tracks ORDER BY position",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, bool>(5)?,
                row.get::<_, bool>(6)?,
                row.get::<_, bool>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(
            |(
                id,
                position,
                name,
                volume_db,
                pan,
                muted,
                solo,
                record_armed,
                instrument_id,
                instrument_path,
            )| {
                let _ = from_sql_u64(position)?;
                let instrument = match (instrument_id, instrument_path) {
                    (None, None) => None,
                    (Some(plugin_id), Some(bundle_path))
                        if !plugin_id.trim().is_empty() && !bundle_path.trim().is_empty() =>
                    {
                        Some(TrackInstrumentSnapshot {
                            plugin_id,
                            bundle_path,
                        })
                    }
                    _ => {
                        return Err(StorageError::InvalidStoredData(
                            "track instrument reference",
                        ));
                    }
                };
                Ok(TrackSnapshot {
                    id: from_sql_u64(id)?,
                    name,
                    volume_db: volume_db as f32,
                    pan: pan as f32,
                    muted,
                    solo,
                    record_armed,
                    instrument,
                    fx_chain: fx_chains.remove(&id).unwrap_or_default(),
                })
            },
        )
        .collect()
}

fn read_audio_items(connection: &Connection) -> Result<Vec<AudioItemSnapshot>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT id, track_id, position, media_ref, start_sample, source_offset_samples, \
         length_samples FROM audio_items ORDER BY position",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(
            |(id, track_id, position, media_ref, start_sample, source_offset, length)| {
                let _ = from_sql_u64(position)?;
                Ok(AudioItemSnapshot {
                    id: from_sql_u64(id)?,
                    track_id: from_sql_u64(track_id)?,
                    media_ref,
                    start_sample: from_sql_u64(start_sample)?,
                    source_offset_samples: from_sql_u64(source_offset)?,
                    length_samples: from_sql_u64(length)?,
                })
            },
        )
        .collect()
}

fn read_items_and_notes(
    connection: &Connection,
) -> Result<(Vec<MidiItemSnapshot>, bool), StorageError> {
    let mut note_statement = connection.prepare(
        "SELECT item_id, id, position, pitch, tick, duration, velocity \
         FROM midi_notes ORDER BY item_id, position",
    )?;
    let note_rows = note_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut notes_by_item: HashMap<u64, Vec<(u64, MidiNoteSnapshot)>> = HashMap::new();
    for (item_id, note_id, position, pitch, tick, duration, velocity) in note_rows {
        let note = MidiNoteSnapshot {
            id: from_sql_u64(note_id)?,
            data: MidiNoteData {
                pitch: from_sql_u8(pitch)?,
                tick: from_sql_u64(tick)?,
                duration: from_sql_u64(duration)?,
                velocity: from_sql_u8(velocity)?,
            },
        };
        notes_by_item
            .entry(from_sql_u64(item_id)?)
            .or_default()
            .push((from_sql_u64(position)?, note));
    }

    let mut item_statement = connection.prepare(
        "SELECT id, track_id, position, start_tick, length_ticks \
         FROM items ORDER BY position",
    )?;
    let item_rows = item_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut items = Vec::with_capacity(item_rows.len());
    for (id, track_id, position, start_tick, length_ticks) in item_rows {
        let id = from_sql_u64(id)?;
        let _ = from_sql_u64(position)?;
        let notes = notes_by_item.remove(&id).unwrap_or_default();
        items.push(MidiItemSnapshot {
            id,
            track_id: from_sql_u64(track_id)?,
            start_tick: from_sql_u64(start_tick)?,
            length_ticks: from_sql_u64(length_ticks)?,
            notes: notes.into_iter().map(|(_, note)| note).collect(),
        });
    }
    Ok((items, !notes_by_item.is_empty()))
}

fn read_tempo_points(connection: &Connection) -> Result<Vec<TempoPointSnapshot>, StorageError> {
    let mut statement = connection
        .prepare("SELECT start_tick, bpm, curve_to_next FROM tempo_points ORDER BY start_tick")?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(start_tick, bpm, curve)| {
            Ok(TempoPointSnapshot {
                start_tick: from_sql_u64(start_tick)?,
                bpm,
                curve_to_next: tempo_curve_from_sql(curve)?,
            })
        })
        .collect()
}

fn read_meter_points(connection: &Connection) -> Result<Vec<MeterPointSnapshot>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT start_tick, numerator, denominator FROM meter_points ORDER BY start_tick",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(start_tick, numerator, denominator)| {
            Ok(MeterPointSnapshot {
                start_tick: from_sql_u64(start_tick)?,
                numerator: from_sql_u32(numerator)?,
                denominator: from_sql_u32(denominator)?,
            })
        })
        .collect()
}

fn tempo_curve_to_sql(curve: TempoCurve) -> i64 {
    match curve {
        TempoCurve::Step => 0,
        TempoCurve::Linear => 1,
    }
}

fn tempo_curve_from_sql(value: i64) -> Result<TempoCurve, StorageError> {
    match value {
        0 => Ok(TempoCurve::Step),
        1 => Ok(TempoCurve::Linear),
        _ => Err(StorageError::InvalidStoredData("unknown tempo curve")),
    }
}

fn to_sql_integer(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::IntegerOutOfRange(value))
}

fn usize_to_sql(value: usize) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::InvalidStoredData("position is out of range"))
}

fn from_sql_u64(value: i64) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| StorageError::InvalidStoredData("negative integer"))
}

fn from_sql_u32(value: i64) -> Result<u32, StorageError> {
    u32::try_from(value).map_err(|_| StorageError::InvalidStoredData("integer is out of range"))
}

fn from_sql_u8(value: i64) -> Result<u8, StorageError> {
    u8::try_from(value).map_err(|_| StorageError::InvalidStoredData("MIDI value is out of range"))
}
