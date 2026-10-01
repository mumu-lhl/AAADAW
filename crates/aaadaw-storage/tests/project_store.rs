use aaadaw_core::{DawAction, MidiNoteData, Project, TimeSignature};
use aaadaw_storage::{CURRENT_SCHEMA_VERSION, ProjectStore, StorageError};
use rusqlite::Connection;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(0);

fn project_path() -> PathBuf {
    let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("aaadaw-storage-{}-{id}.aaadaw", std::process::id()))
}

fn remove_database(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
}

struct FailingReader {
    remaining_bytes: usize,
}

struct GatedReader {
    data: Cursor<Vec<u8>>,
    entered: Option<Sender<()>>,
    release: Receiver<()>,
}

impl Read for GatedReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if let Some(entered) = self.entered.take() {
            entered
                .send(())
                .expect("test should still be waiting for reader");
            self.release
                .recv()
                .expect("test should release the source reader");
        }
        self.data.read(output)
    }
}

impl Read for FailingReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining_bytes == 0 {
            return Err(std::io::Error::other("simulated source failure"));
        }
        let amount = output.len().min(self.remaining_bytes);
        output[..amount].fill(0x5a);
        self.remaining_bytes -= amount;
        Ok(amount)
    }
}

#[test]
fn schema_migration_and_project_roundtrip_preserve_state() {
    let path = project_path();
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Keys".to_owned(),
        })
        .expect("track creation should succeed");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::SetTrackVolume {
            track_id,
            volume_db: -3.0,
        })
        .expect("volume change should succeed");
    project
        .apply(DawAction::SetTrackMute {
            track_id,
            muted: true,
        })
        .expect("mute change should succeed");
    project
        .apply(DawAction::SetTrackSolo {
            track_id,
            solo: true,
        })
        .expect("solo change should succeed");
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://room-tone".to_owned(),
            start_sample: 48_000,
            source_offset_samples: 2_400,
            length_samples: 96_000,
        })
        .expect("audio item insertion should succeed");
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 3840,
        })
        .expect("MIDI item insertion should succeed");
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 67,
                tick: 120,
                duration: 720,
                velocity: 96,
            }],
        })
        .expect("MIDI note insertion should succeed");
    project
        .apply(DawAction::SetTempo {
            start_tick: 960,
            bpm: 90.0,
        })
        .expect("tempo change should succeed");
    project
        .apply(DawAction::SetTempoCurve {
            start_tick: 0,
            curve: aaadaw_core::TempoCurve::Linear,
        })
        .expect("tempo curve change should succeed");
    project
        .apply(DawAction::SetTimeSignature {
            start_tick: 3840,
            signature: TimeSignature::new(7, 8).expect("7/8 is valid"),
        })
        .expect("meter change should succeed");

    let mut store =
        ProjectStore::open(&path).expect("a new file should migrate to the current schema");
    assert_eq!(
        store
            .schema_version()
            .expect("schema version should be readable"),
        CURRENT_SCHEMA_VERSION
    );
    store
        .save(&project)
        .expect("saving the project should succeed");
    store.close().expect("closing should checkpoint the WAL");

    let store = ProjectStore::open(&path).expect("the saved project should reopen");
    let restored = store.load().expect("the saved project should load");
    assert_eq!(restored.snapshot(), project.snapshot());
    store
        .close()
        .expect("the reopened project should close cleanly");
    remove_database(&path);
}

#[test]
fn existing_schema_two_files_get_additive_audio_tables_without_version_bump() {
    let path = project_path();
    let store = ProjectStore::open(&path).expect("database should open");
    store.close().expect("database should close");

    let connection = Connection::open(&path).expect("database should be SQLite");
    connection
        .execute_batch(
            "DROP TABLE audio_items; DROP TABLE audio_asset_chunks; \
             DROP TABLE audio_asset_storage_chunks; DROP TABLE audio_asset_links; \
             DROP TABLE audio_assets",
        )
        .expect("simulate a schema-two file without audio tables");
    drop(connection);

    let mut store = ProjectStore::open(&path).expect("missing additive table should be ensured");
    assert_eq!(
        store
            .schema_version()
            .expect("schema version should be readable"),
        CURRENT_SCHEMA_VERSION
    );
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".to_owned(),
        })
        .expect("track creation should succeed");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://legacy-database".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .expect("audio item insertion should succeed");
    store.save(&project).expect("audio item should save");
    assert_eq!(
        store.load().expect("audio item should load").snapshot(),
        project.snapshot()
    );
    store.close().expect("database should close");

    let connection = Connection::open(&path).expect("database should be SQLite");
    connection
        .execute_batch(
            "DROP INDEX audio_assets_storage_key; \
             ALTER TABLE audio_assets DROP COLUMN content_hash; \
             ALTER TABLE audio_assets DROP COLUMN source_path; \
             ALTER TABLE audio_assets DROP COLUMN storage_key; \
             ALTER TABLE audio_assets DROP COLUMN import_state",
        )
        .expect("simulate an earlier additive audio-asset table");
    drop(connection);
    let store = ProjectStore::open(&path).expect("missing fingerprint columns should be added");
    assert_eq!(
        store
            .schema_version()
            .expect("schema version should remain readable"),
        CURRENT_SCHEMA_VERSION
    );
    store.close().expect("upgraded asset database should close");
    remove_database(&path);
}

#[test]
fn project_writes_can_proceed_while_an_import_waits_for_source_data() {
    let path = project_path();
    let store = ProjectStore::open(&path).expect("project database should open");
    store.close().expect("project database should close");
    let (entered_sender, entered_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::channel();
    let import_path = path.clone();
    let importer = thread::spawn(move || {
        let mut store = ProjectStore::open(import_path).expect("import project should open");
        let result = store.import_audio_asset(
            "asset://gated-import",
            "gated.wav",
            GatedReader {
                data: Cursor::new(vec![1, 2, 3]),
                entered: Some(entered_sender),
                release: release_receiver,
            },
        );
        store.close().expect("import project should close");
        result
    });

    entered_receiver
        .recv()
        .expect("import should reach its source read");
    let observer = ProjectStore::open(&path).expect("reader should open during import");
    assert!(matches!(
        observer.resolve_audio_asset("asset://gated-import"),
        Err(StorageError::AudioAssetNotFound(_))
    ));
    observer
        .close()
        .expect("observer should close without seeing staged data");
    let writer = Connection::open(&path).expect("another writer should open the project");
    writer
        .busy_timeout(Duration::from_millis(100))
        .expect("short lock timeout should configure");
    writer
        .execute("CREATE TABLE lock_probe(value INTEGER)", [])
        .expect("source I/O should not hold a SQLite write transaction");
    drop(writer);

    release_sender
        .send(())
        .expect("source reader should be released");
    assert_eq!(
        importer
            .join()
            .expect("import worker should not panic")
            .expect("staged asset should publish"),
        3
    );
    remove_database(&path);
}

#[test]
fn background_asset_import_reports_progress_and_cancellation_rolls_back() {
    let database_path = project_path();
    let source_path = database_path.with_extension("wav");
    let source_bytes = (0_usize..700_123)
        .map(|index| (index.wrapping_mul(17) % 251) as u8)
        .collect::<Vec<_>>();
    std::fs::write(&source_path, &source_bytes).expect("source fixture should be written");
    let mut store = ProjectStore::open(&database_path).expect("project should open");

    let cancelled = store.import_audio_file_with_progress(&source_path, |bytes| {
        if bytes > 0 {
            Err(StorageError::AudioAssetImportCancelled)
        } else {
            Ok(())
        }
    });
    assert!(matches!(
        cancelled,
        Err(StorageError::AudioAssetImportCancelled)
    ));
    let connection = Connection::open(&database_path).expect("database should remain readable");
    let incomplete_assets: i64 = connection
        .query_row("SELECT COUNT(*) FROM audio_assets", [], |row| row.get(0))
        .expect("cancelled transaction should leave no asset row");
    assert_eq!(incomplete_assets, 0);
    drop(connection);

    let worker = store
        .start_audio_asset_import(&source_path)
        .expect("background import should start");
    let mut progress = Vec::new();
    while let Ok(update) = worker.progress().recv() {
        progress.push(update);
    }
    let media_ref = worker.join().expect("background import should complete");
    assert_eq!(progress.first().unwrap().bytes_imported, 0);
    assert_eq!(
        progress.last().unwrap().bytes_imported,
        source_bytes.len() as u64
    );
    assert!(progress.windows(2).all(|pair| {
        pair[0].bytes_imported <= pair[1].bytes_imported
            && pair
                .iter()
                .all(|entry| entry.total_bytes == source_bytes.len() as u64)
    }));
    let mut reader = store
        .audio_asset_reader(&media_ref)
        .expect("completed import should resolve");
    let mut embedded = Vec::new();
    reader
        .read_to_end(&mut embedded)
        .expect("completed import should be readable");
    assert_eq!(embedded, source_bytes);
    drop(reader);

    store.close().expect("project database should close");
    remove_database(&database_path);
    std::fs::remove_file(source_path).expect("source fixture should be removed");
}

#[test]
fn importing_an_external_file_embeds_it_and_returns_a_media_reference() {
    let database_path = project_path();
    let source_path = database_path.with_extension("wav");
    let source_bytes = (0..32_000_u32)
        .map(|sample| (sample % 251) as u8)
        .collect::<Vec<_>>();
    std::fs::write(&source_path, &source_bytes).expect("source fixture should be written");
    let mut store = ProjectStore::open(&database_path).expect("project should open");

    let media_ref = store
        .import_audio_file(&source_path)
        .expect("external file should be embedded");

    assert!(media_ref.starts_with("asset://"));
    let mut reader = store
        .audio_asset_reader(&media_ref)
        .expect("returned reference should resolve");
    assert_eq!(
        reader.original_name(),
        source_path.file_name().unwrap().to_string_lossy()
    );
    let mut embedded_bytes = Vec::new();
    reader
        .read_to_end(&mut embedded_bytes)
        .expect("embedded bytes should be readable");
    assert_eq!(embedded_bytes, source_bytes);
    assert_eq!(
        std::fs::read(&source_path).expect("source should remain"),
        source_bytes
    );
    assert_eq!(
        store
            .audio_asset_source_status(&media_ref)
            .expect("source fingerprint should be checked"),
        aaadaw_storage::AudioAssetSourceStatus::Unchanged
    );

    let changed_bytes = vec![0x3c; source_bytes.len()];
    std::fs::write(&source_path, &changed_bytes).expect("original source should be editable");
    assert_eq!(
        store
            .audio_asset_source_status(&media_ref)
            .expect("changed source should be detected"),
        aaadaw_storage::AudioAssetSourceStatus::Changed
    );
    assert_eq!(
        embedded_bytes, source_bytes,
        "embedded snapshot stays immutable"
    );
    drop(reader);
    std::fs::remove_file(&source_path).expect("original source should be removable");
    assert_eq!(
        store
            .audio_asset_source_status(&media_ref)
            .expect("missing source should be reported"),
        aaadaw_storage::AudioAssetSourceStatus::Missing
    );

    store.close().expect("project database should close");
    remove_database(&database_path);
}

#[test]
fn external_audio_links_can_be_relinked_and_packed_under_the_same_reference() {
    let database_path = project_path();
    let original_path = database_path.with_extension("wav");
    let replacement_path = database_path.with_extension("replacement.wav");
    std::fs::write(&original_path, [1, 2, 3]).expect("external fixture should be written");
    let replacement_bytes = vec![4, 5, 6, 7];
    std::fs::write(&replacement_path, &replacement_bytes)
        .expect("replacement fixture should be written");
    let mut store = ProjectStore::open(&database_path).expect("project database should open");

    let media_ref = store
        .link_external_audio_file(&original_path)
        .expect("external file should link without embedding");
    let linked = store
        .resolve_audio_asset(&media_ref)
        .expect("linked reference should resolve");
    assert!(matches!(
        linked,
        aaadaw_storage::ResolvedAudioAsset::LinkedFile { path, .. }
            if path == original_path.canonicalize().unwrap()
    ));
    assert_eq!(
        store
            .audio_asset_source_status(&media_ref)
            .expect("linked files should report their storage mode"),
        aaadaw_storage::AudioAssetSourceStatus::Linked
    );

    store
        .relink_external_audio_file(&media_ref, &replacement_path)
        .expect("external reference should be relinkable");
    let second_media_ref = store
        .link_external_audio_file(&original_path)
        .expect("another external file should link");
    store
        .pack_external_audio_asset(&media_ref)
        .expect("linked bytes should be packed into the project");
    store
        .pack_external_audio_asset(&media_ref)
        .expect("packing an already embedded reference should be idempotent");
    assert_eq!(
        store
            .pack_all_external_audio_assets()
            .expect("all remaining external links should pack"),
        vec![second_media_ref]
    );
    let mut resolved = store
        .resolve_audio_asset(&media_ref)
        .expect("packed reference should resolve to embedded content");
    let aaadaw_storage::ResolvedAudioAsset::Embedded(ref mut reader) = resolved else {
        panic!("packed media should no longer use the external link");
    };
    let mut embedded = Vec::new();
    reader
        .read_to_end(&mut embedded)
        .expect("packed bytes should be readable");
    assert_eq!(embedded, replacement_bytes);
    assert_eq!(
        store
            .audio_asset_source_status(&media_ref)
            .expect("packed origin should be checkable"),
        aaadaw_storage::AudioAssetSourceStatus::Unchanged
    );

    drop(resolved);
    std::fs::remove_file(&replacement_path).expect("external source should be removable");
    assert!(matches!(
        store.resolve_audio_asset(&media_ref),
        Ok(aaadaw_storage::ResolvedAudioAsset::Embedded(_))
    ));
    assert_eq!(
        store
            .audio_asset_source_status(&media_ref)
            .expect("missing source should be reported"),
        aaadaw_storage::AudioAssetSourceStatus::Missing
    );

    store.close().expect("project database should close");
    remove_database(&database_path);
    std::fs::remove_file(original_path).expect("original fixture should be removed");
}

#[test]
fn audio_assets_are_imported_in_chunks_and_read_back_with_seeking() {
    let path = project_path();
    let bytes = (0_usize..700_123)
        .map(|index| (index.wrapping_mul(31) % 251) as u8)
        .collect::<Vec<_>>();
    let mut store = ProjectStore::open(&path).expect("project database should open");
    assert_eq!(
        store
            .import_audio_asset("asset://large-fixture", "fixture.bin", Cursor::new(&bytes))
            .expect("asset import should succeed"),
        bytes.len() as u64
    );
    assert!(matches!(
        store.import_audio_asset(
            "asset://large-fixture",
            "duplicate.bin",
            Cursor::new(&bytes)
        ),
        Err(StorageError::AudioAssetAlreadyExists(_))
    ));
    assert!(matches!(
        store.import_audio_asset("asset://empty", "empty.bin", Cursor::new([])),
        Err(StorageError::EmptyAudioAsset)
    ));
    assert!(matches!(
        store.audio_asset_reader("asset://empty"),
        Err(StorageError::AudioAssetNotFound(_))
    ));
    assert!(matches!(
        store.import_audio_asset(
            "asset://partial",
            "interrupted.bin",
            FailingReader {
                remaining_bytes: 256 * 1024 + 17,
            },
        ),
        Err(StorageError::Io(_))
    ));
    assert!(matches!(
        store.audio_asset_reader("asset://partial"),
        Err(StorageError::AudioAssetNotFound(_))
    ));
    store
        .import_audio_asset("asset://partial", "complete.bin", Cursor::new(&bytes))
        .expect("failed partial import should leave the reference reusable");

    let mut reader = store
        .audio_asset_reader("asset://large-fixture")
        .expect("asset reader should open");
    assert_eq!(reader.original_name(), "fixture.bin");
    assert_eq!(reader.byte_len(), bytes.len() as u64);
    assert_eq!(
        store
            .audio_asset_source_status("asset://large-fixture")
            .expect("reader-only imports have no source path"),
        aaadaw_storage::AudioAssetSourceStatus::Untracked
    );
    let mut around_boundary = [0; 19];
    reader
        .seek(SeekFrom::Start(256 * 1024 - 7))
        .expect("reader should seek to a chunk boundary");
    reader
        .read_exact(&mut around_boundary)
        .expect("reader should span adjacent chunks");
    assert_eq!(&around_boundary, &bytes[256 * 1024 - 7..256 * 1024 + 12]);

    reader
        .seek(SeekFrom::End(-17))
        .expect("reader should seek relative to EOF");
    let mut tail = [0; 17];
    reader
        .read_exact(&mut tail)
        .expect("tail should be readable");
    assert_eq!(&tail, &bytes[bytes.len() - 17..]);
    drop(reader);
    store.close().expect("asset database should checkpoint");

    let reopened = ProjectStore::open(&path).expect("asset database should reopen");
    let mut reader = reopened
        .audio_asset_reader("asset://large-fixture")
        .expect("embedded asset should persist");
    let mut restored = vec![0; bytes.len()];
    reader
        .read_exact(&mut restored)
        .expect("all chunks should be readable after reopen");
    assert_eq!(restored, bytes);
    drop(reader);
    reopened.close().expect("reopened database should close");
    remove_database(&path);
}

#[test]
fn legacy_chunks_remain_readable_and_crashed_imports_can_be_cleaned() {
    let path = project_path();
    let store = ProjectStore::open(&path).expect("project database should open");
    store.close().expect("project database should close");

    let connection = Connection::open(&path).expect("database should be SQLite");
    connection
        .execute(
            "INSERT INTO audio_assets(media_ref, original_name, byte_len, chunk_count, \
             chunk_size, source_path, content_hash, storage_key, import_state) \
             VALUES('asset://legacy-chunks', 'legacy.wav', 3, 1, 262144, NULL, NULL, NULL, 1)",
            [],
        )
        .expect("legacy manifest should insert");
    connection
        .execute(
            "INSERT INTO audio_asset_chunks(media_ref, chunk_index, data) \
             VALUES('asset://legacy-chunks', 0, ?1)",
            [[11_u8, 22, 33].as_slice()],
        )
        .expect("legacy chunk should insert");
    connection
        .execute(
            "INSERT INTO audio_assets(media_ref, original_name, byte_len, chunk_count, \
             chunk_size, source_path, content_hash, storage_key, import_state) \
             VALUES('asset://abandoned', 'incomplete.wav', 0, 0, 262144, NULL, NULL, 'deadbeef', 0)",
            [],
        )
        .expect("pending manifest should insert");
    connection
        .execute(
            "INSERT INTO audio_asset_storage_chunks(storage_key, chunk_index, data) \
             VALUES('deadbeef', 0, ?1)",
            [[99_u8].as_slice()],
        )
        .expect("pending chunk should insert");
    drop(connection);

    let mut store = ProjectStore::open(&path).expect("project should reopen");
    let mut legacy = store
        .audio_asset_reader("asset://legacy-chunks")
        .expect("pre-staging chunk rows should remain readable");
    let mut legacy_bytes = Vec::new();
    legacy
        .read_to_end(&mut legacy_bytes)
        .expect("legacy bytes should read");
    assert_eq!(legacy_bytes, [11, 22, 33]);
    drop(legacy);

    assert_eq!(
        store
            .cleanup_incomplete_audio_asset_imports()
            .expect("crashed import residue should be cleaned"),
        1
    );
    assert!(matches!(
        store.audio_asset_reader("asset://abandoned"),
        Err(StorageError::AudioAssetNotFound(_))
    ));
    let connection = Connection::open(&path).expect("project file should remain accessible");
    let remaining_chunks: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM audio_asset_storage_chunks WHERE storage_key = 'deadbeef'",
            [],
            |row| row.get(0),
        )
        .expect("abandoned chunks should be removed");
    assert_eq!(remaining_chunks, 0);
    drop(connection);

    store.close().expect("reopened project should close");
    remove_database(&path);
}

#[test]
fn files_from_newer_schema_versions_are_rejected_without_downgrade() {
    let path = project_path();
    let store = ProjectStore::open(&path).expect("a new database should open");
    store.close().expect("the database should close");

    let connection = Connection::open(&path).expect("the database should be a SQLite file");
    let future_version = i64::from(CURRENT_SCHEMA_VERSION) + 1;
    connection
        .pragma_update(None, "user_version", future_version)
        .expect("the test database should be marked as a future version");
    drop(connection);

    let error = match ProjectStore::open(&path) {
        Ok(store) => {
            store
                .close()
                .expect("unexpectedly opened store should close");
            panic!("a newer schema version must not be opened");
        }
        Err(error) => error,
    };
    assert!(matches!(
        error,
        StorageError::UnsupportedSchemaVersion { found, .. } if found == future_version
    ));

    let connection = Connection::open(&path).expect("rejected file should remain readable");
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("schema version should still be available");
    assert_eq!(version, future_version);
    drop(connection);
    remove_database(&path);
}
