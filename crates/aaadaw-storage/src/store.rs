use aaadaw_core::{
    AudioItemSnapshot, MeterPointSnapshot, MidiItemSnapshot, MidiNoteData, MidiNoteSnapshot,
    Project, ProjectSettings, ProjectSnapshot, SnapshotError, TempoCurve, TempoPointSnapshot,
    TrackSnapshot,
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
use std::sync::Mutex;
use std::time::Duration;

/// Latest database schema version understood by this release.
pub const CURRENT_SCHEMA_VERSION: u32 = 2;
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

const AUDIO_ASSET_CHUNK_SIZE: usize = 256 * 1024;

const AUDIO_ASSETS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS audio_assets (
    media_ref TEXT PRIMARY KEY CHECK (length(trim(media_ref)) > 0),
    original_name TEXT NOT NULL,
    byte_len INTEGER NOT NULL CHECK (byte_len >= 0),
    chunk_count INTEGER NOT NULL CHECK (chunk_count >= 0),
    chunk_size INTEGER NOT NULL CHECK (chunk_size > 0),
    source_path TEXT,
    content_hash BLOB
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
    EmptyAudioAsset,
    MemoryDatabaseHasNoIndependentAssetReader,
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
            Self::EmptyAudioAsset => formatter.write_str("audio asset content must not be empty"),
            Self::MemoryDatabaseHasNoIndependentAssetReader => formatter.write_str(
                "an independent audio asset reader is unavailable for in-memory databases",
            ),
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

/// Status of the original external file recorded when an asset was imported.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioAssetSourceStatus {
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
                let chunk = state
                    .connection
                    .query_row(
                        "SELECT data FROM audio_asset_chunks \
                         WHERE media_ref = ?1 AND chunk_index = ?2",
                        params![state.media_ref, sql_chunk_index],
                        |row| row.get::<_, Vec<u8>>(0),
                    )
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

    /// Imports an external file into the project and returns its opaque `asset://` reference.
    ///
    /// The source file is left untouched. Run this synchronous operation on a
    /// background thread; project assets are read incrementally into bounded chunks.
    pub fn import_audio_file(&mut self, path: impl AsRef<Path>) -> Result<String, StorageError> {
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
        let token: String =
            self.connection
                .query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))?;
        let media_ref = format!("asset://{token}");
        self.import_audio_asset_with_source_path(&media_ref, &original_name, source_path, source)?;
        Ok(media_ref)
    }

    /// Imports an audio asset as bounded SQLite BLOB chunks under an immutable reference.
    ///
    /// The reader is consumed incrementally; at most one chunk is buffered in memory.
    /// Import runs in a single transaction, so incomplete assets are never visible.
    pub fn import_audio_asset(
        &mut self,
        media_ref: &str,
        original_name: &str,
        source: impl Read,
    ) -> Result<u64, StorageError> {
        self.import_audio_asset_with_source_path(media_ref, original_name, None, source)
    }

    fn import_audio_asset_with_source_path(
        &mut self,
        media_ref: &str,
        original_name: &str,
        source_path: Option<&str>,
        mut source: impl Read,
    ) -> Result<u64, StorageError> {
        if media_ref.trim().is_empty() {
            return Err(StorageError::AudioAssetReferenceEmpty);
        }
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
             chunk_size, source_path, content_hash) VALUES(?1, ?2, 0, 0, ?3, ?4, NULL)",
            params![
                media_ref,
                original_name,
                AUDIO_ASSET_CHUNK_SIZE as i64,
                source_path
            ],
        )?;

        let mut chunk_buffer = vec![0; AUDIO_ASSET_CHUNK_SIZE];
        let mut byte_len = 0_u64;
        let mut chunk_count = 0_usize;
        let mut content_hasher = Sha256::new();
        loop {
            let chunk_len = fill_chunk(&mut source, &mut chunk_buffer)?;
            if chunk_len == 0 {
                break;
            }
            transaction.execute(
                "INSERT INTO audio_asset_chunks(media_ref, chunk_index, data) \
                 VALUES(?1, ?2, ?3)",
                params![
                    media_ref,
                    usize_to_sql(chunk_count)?,
                    &chunk_buffer[..chunk_len]
                ],
            )?;
            content_hasher.update(&chunk_buffer[..chunk_len]);
            byte_len = byte_len
                .checked_add(chunk_len as u64)
                .ok_or(StorageError::IntegerOutOfRange(u64::MAX))?;
            to_sql_integer(byte_len)?;
            chunk_count = chunk_count
                .checked_add(1)
                .ok_or(StorageError::IntegerOutOfRange(u64::MAX))?;
        }
        if byte_len == 0 {
            return Err(StorageError::EmptyAudioAsset);
        }
        transaction.execute(
            "UPDATE audio_assets SET byte_len = ?2, chunk_count = ?3, content_hash = ?4 \
             WHERE media_ref = ?1",
            params![
                media_ref,
                to_sql_integer(byte_len)?,
                usize_to_sql(chunk_count)?,
                content_hasher.finalize().as_slice()
            ],
        )?;
        transaction.commit()?;
        Ok(byte_len)
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
                "SELECT byte_len, chunk_count, chunk_size, original_name \
                 FROM audio_assets WHERE media_ref = ?1",
                [media_ref],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| StorageError::AudioAssetNotFound(media_ref.to_owned()))?;
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
                chunk_size,
                position: 0,
                cached_chunk: None,
            }),
        })
    }

    /// Re-hashes the recorded original file to determine whether it still matches the import snapshot.
    ///
    /// This reads the entire external file and should run on a background thread.
    pub fn audio_asset_source_status(
        &self,
        media_ref: &str,
    ) -> Result<AudioAssetSourceStatus, StorageError> {
        let metadata = self
            .connection
            .query_row(
                "SELECT source_path, content_hash FROM audio_assets WHERE media_ref = ?1",
                [media_ref],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<Vec<u8>>>(1)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| StorageError::AudioAssetNotFound(media_ref.to_owned()))?;
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
        let actual_hash = sha256_reader(&mut source)?;
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
    for (column, definition) in [("source_path", "TEXT"), ("content_hash", "BLOB")] {
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
    Ok(())
}

fn sha256_reader(reader: &mut impl Read) -> Result<[u8; 32], io::Error> {
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
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
            "INSERT INTO tracks(id, position, name, volume_db, pan, muted, solo) \
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                to_sql_integer(track.id)?,
                usize_to_sql(position)?,
                track.name,
                f64::from(track.volume_db),
                f64::from(track.pan),
                track.muted,
                track.solo
            ],
        )?;
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
    let mut statement = connection.prepare(
        "SELECT id, position, name, volume_db, pan, muted, solo FROM tracks ORDER BY position",
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
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(id, position, name, volume_db, pan, muted, solo)| {
            let _ = from_sql_u64(position)?;
            Ok(TrackSnapshot {
                id: from_sql_u64(id)?,
                name,
                volume_db: volume_db as f32,
                pan: pan as f32,
                muted,
                solo,
            })
        })
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
