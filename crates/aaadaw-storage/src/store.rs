use aaadaw_core::{
    MeterPointSnapshot, MidiItemSnapshot, MidiNoteData, MidiNoteSnapshot, Project, ProjectSettings,
    ProjectSnapshot, SnapshotError, TempoCurve, TempoPointSnapshot, TrackSnapshot,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use std::collections::HashMap;
use std::fmt;
use std::path::Path;
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
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sql(error) => Some(error),
            Self::Snapshot(error) => Some(error),
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

/// Owns a SQLite connection for one `.aaadaw` project file.
///
/// Saves are atomic full-snapshot replacements. The connection uses WAL while
/// open; call [`ProjectStore::close`] to checkpoint and compact the project back
/// into a portable single file.
pub struct ProjectStore {
    connection: Connection,
}

impl ProjectStore {
    /// Opens or creates a project database and applies pending schema migrations.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let mut connection = Connection::open(path)?;
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
        Ok(Self { connection })
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
                 (SELECT COUNT(*) FROM midi_notes) + (SELECT COUNT(*) FROM tempo_points) + \
                 (SELECT COUNT(*) FROM meter_points)",
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
    Ok(())
}

fn write_snapshot(
    transaction: &Transaction<'_>,
    snapshot: &ProjectSnapshot,
) -> Result<(), StorageError> {
    transaction.execute("DELETE FROM midi_notes", [])?;
    transaction.execute("DELETE FROM items", [])?;
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
