use aaadaw_core::{DawAction, MidiNoteData, Project, TimeSignature};
use aaadaw_storage::{CURRENT_SCHEMA_VERSION, ProjectStore, StorageError};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

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
