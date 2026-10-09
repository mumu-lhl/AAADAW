use aaadaw_core::{
    DawAction, MeterPointSnapshot, MidiControllerData, MidiNoteData, MidiPitchBendData, Project,
    VolumeAutomationPoint,
};
use aaadaw_storage::{CURRENT_SCHEMA_VERSION, ProjectStore, StorageError};
use rusqlite::Connection;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::process::Command;
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

#[test]
fn readonly_project_load_does_not_modify_saved_database() {
    let path = project_path();
    let mut store = ProjectStore::open(&path).unwrap();
    store.save(&Project::new()).unwrap();
    store.close().unwrap();

    let original_bytes = std::fs::read(&path).unwrap();
    let project = ProjectStore::load_read_only(&path).unwrap();
    assert!(project.tracks().is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), original_bytes);

    remove_database(&path);
}

#[test]
fn ordinary_track_receivers_survive_save_and_read_only_reopen() {
    let path = project_path();
    let mut project = Project::new();
    for (index, name) in ["Receiver", "Source"].into_iter().enumerate() {
        project
            .apply(DawAction::CreateTrack {
                index,
                name: name.into(),
            })
            .unwrap();
    }
    let receiver = project.tracks()[0].id();
    let source = project.tracks()[1].id();
    project
        .apply(DawAction::SetTrackOutput {
            track_id: source,
            output_track: Some(receiver),
        })
        .unwrap();
    let mut store = ProjectStore::open(&path).unwrap();
    store.save(&project).unwrap();
    store.close().unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let restored = ProjectStore::load_read_only(&path).unwrap();
    assert_eq!(restored.tracks()[1].output_track(), Some(receiver));
    assert!(!restored.tracks()[0].is_bus());
    assert_eq!(
        restored.settings().pan_mode(),
        aaadaw_core::PanMode::ZeroDbBalance
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    remove_database(&path);
}

#[test]
fn schema_seventeen_preserves_legacy_mix_policy_and_new_defaults_round_trip() {
    let path = project_path();
    let settings = aaadaw_core::ProjectSettings::default()
        .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo);
    let mut project = Project::with_settings(settings);
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Legacy source".into(),
        })
        .unwrap();
    project
        .apply(DawAction::CreateBusTrack {
            index: 1,
            name: "Legacy bus".into(),
        })
        .unwrap();
    project
        .apply(DawAction::SetTrackOutput {
            track_id: project.tracks()[0].id(),
            output_track: Some(project.tracks()[1].id()),
        })
        .unwrap();
    let mut store = ProjectStore::open(&path).unwrap();
    store.save(&project).unwrap();
    store.close().unwrap();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("ALTER TABLE project_meta DROP COLUMN pan_mode; PRAGMA user_version = 17;")
        .unwrap();
    drop(connection);
    let backup_path = path.with_extension("v17-backup");
    std::fs::copy(&path, &backup_path).unwrap();
    let backup_bytes = std::fs::read(&backup_path).unwrap();
    let store = ProjectStore::open(&path).unwrap();
    assert_eq!(store.schema_version().unwrap(), 18);
    let restored = store.load().unwrap();
    assert_eq!(restored.snapshot(), project.snapshot());
    assert!(
        restored
            .tracks()
            .iter()
            .all(|track| track.pan_mode() == aaadaw_core::PanMode::LegacyMonoStereo)
    );
    store.close().unwrap();
    let restored = ProjectStore::load_read_only(&path).unwrap();
    assert_eq!(
        restored.settings().pan_mode(),
        aaadaw_core::PanMode::LegacyMonoStereo
    );
    assert_eq!(std::fs::read(&backup_path).unwrap(), backup_bytes);
    assert!(matches!(
        ProjectStore::load_read_only(&backup_path),
        Err(StorageError::ReadOnlySchemaVersion { .. })
    ));
    std::fs::remove_file(backup_path).unwrap();
    remove_database(&path);
}

#[test]
fn project_store_round_trips_track_fx_parameter_automation() {
    let path = project_path();
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Synth".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::SetTrackFxChain {
            track_id,
            plugins: vec![
                aaadaw_core::TrackFxPlugin::new("org.example.filter", "/plugins/filter.clap")
                    .unwrap(),
            ],
        })
        .unwrap();
    project
        .apply(DawAction::SetTrackFxParameterAutomation {
            track_id,
            chain_index: 0,
            parameter_id: 23,
            points: vec![
                aaadaw_core::FxParameterAutomationPoint::new(0, 0.25).unwrap(),
                aaadaw_core::FxParameterAutomationPoint::new(48_000, 0.875).unwrap(),
            ],
        })
        .unwrap();

    let mut store = ProjectStore::open(&path).unwrap();
    store.save(&project).unwrap();
    store.close().unwrap();
    let reopened = ProjectStore::open(&path).unwrap().load().unwrap();

    assert_eq!(reopened.snapshot(), project.snapshot());
    remove_database(&path);
}

#[test]
fn project_store_round_trips_frozen_track_and_its_render_item() {
    let path = project_path();
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Frozen lead".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::SetTrackInstrument {
            track_id,
            instrument: Some(
                aaadaw_core::TrackInstrument::new("org.example.synth", "/synth.clap").unwrap(),
            ),
        })
        .unwrap();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 60,
                tick: 0,
                duration: 480,
                velocity: 100,
            }],
        })
        .unwrap();
    project
        .apply(DawAction::FreezeTrack {
            track_id,
            media_ref: "asset://freeze-render".to_owned(),
            start_sample: 0,
            length_samples: 48_000,
        })
        .unwrap();

    let mut store = ProjectStore::open(&path).unwrap();
    store.save(&project).unwrap();
    store.close().unwrap();
    let store = ProjectStore::open(&path).unwrap();
    let reopened = store.load().unwrap();
    assert_eq!(reopened.snapshot(), project.snapshot());
    assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    store.close().unwrap();
    remove_database(&path);
}

#[test]
fn schema_fourteen_migrates_track_freeze_reference_as_empty() {
    let path = project_path();
    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Legacy".to_owned(),
        })
        .unwrap();
    let mut store = ProjectStore::open(&path).unwrap();
    store.save(&project).unwrap();
    store.close().unwrap();

    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "ALTER TABLE tracks DROP COLUMN frozen_audio_item_id; \
             ALTER TABLE items DROP COLUMN source_offset_ticks; ALTER TABLE items DROP COLUMN name; \
             ALTER TABLE project_meta DROP COLUMN pan_mode; PRAGMA user_version = 14;",
        )
        .unwrap();
    drop(connection);

    let store = ProjectStore::open(&path).unwrap();
    assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    assert_eq!(store.load().unwrap().snapshot(), project.snapshot());
    store.close().unwrap();
    remove_database(&path);
}

#[test]
fn midi_item_name_round_trips_and_schema_fifteen_defaults_existing_names() {
    let path = project_path();
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Keys".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::SetMidiItemName {
            item_id,
            name: "Chorus".to_owned(),
        })
        .unwrap();

    let mut store = ProjectStore::open(&path).unwrap();
    store.save(&project).unwrap();
    store.close().unwrap();
    let loaded = ProjectStore::open(&path).unwrap().load().unwrap();
    assert_eq!(loaded.midi_items()[0].name(), "Chorus");

    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "ALTER TABLE items DROP COLUMN source_offset_ticks; ALTER TABLE items DROP COLUMN name; ALTER TABLE project_meta DROP COLUMN pan_mode; PRAGMA user_version = 15;",
        )
        .unwrap();
    drop(connection);

    let migrated = ProjectStore::open(&path).unwrap();
    assert_eq!(migrated.load().unwrap().midi_items()[0].name(), "MIDI");
    assert_eq!(
        migrated.load().unwrap().midi_items()[0].source_offset_ticks(),
        0
    );
    assert_eq!(migrated.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    migrated.close().unwrap();
    remove_database(&path);
}

#[test]
fn midi_item_source_offset_round_trips_through_storage() {
    let path = project_path();
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Keys".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 960,
            length_ticks: 1_920,
        })
        .unwrap();
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::TrimMidiItemStart {
            item_id,
            start_tick: 1_440,
            length_ticks: 1_440,
            source_offset_ticks: 480,
        })
        .unwrap();

    let mut store = ProjectStore::open(&path).unwrap();
    store.save(&project).unwrap();
    store.close().unwrap();
    let loaded = ProjectStore::open(&path).unwrap().load().unwrap();
    assert_eq!(loaded.midi_items()[0].start_tick(), 1_440);
    assert_eq!(loaded.midi_items()[0].source_offset_ticks(), 480);
    assert_eq!(loaded.snapshot(), project.snapshot());
    remove_database(&path);
}

#[test]
fn readonly_project_load_rejects_older_schema_without_migrating_it() {
    let path = project_path();
    let mut store = ProjectStore::open(&path).unwrap();
    store.save(&Project::new()).unwrap();
    store.close().unwrap();

    let connection = Connection::open(&path).unwrap();
    connection
        .pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION - 1)
        .unwrap();
    drop(connection);
    let original_bytes = std::fs::read(&path).unwrap();

    assert!(matches!(
        ProjectStore::load_read_only(&path),
        Err(StorageError::ReadOnlySchemaVersion { .. })
    ));
    assert_eq!(std::fs::read(&path).unwrap(), original_bytes);

    remove_database(&path);
}

#[test]
fn schema_thirteen_migrates_with_compatible_empty_arrangement_view_state() {
    let path = project_path();
    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Legacy project".to_owned(),
        })
        .unwrap();
    let mut store = ProjectStore::open(&path).unwrap();
    store.save(&project).unwrap();
    store.close().unwrap();

    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "DROP TABLE arrangement_fx_lanes; \
             DROP TABLE arrangement_volume_lanes; \
             DROP TABLE arrangement_view_meta; \
             ALTER TABLE tracks DROP COLUMN frozen_audio_item_id; \
             ALTER TABLE items DROP COLUMN source_offset_ticks; ALTER TABLE items DROP COLUMN name; \
             ALTER TABLE project_meta DROP COLUMN pan_mode; PRAGMA user_version = 13;",
        )
        .unwrap();
    drop(connection);

    let store = ProjectStore::open(&path).unwrap();
    assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    assert_eq!(store.load().unwrap().snapshot(), project.snapshot());
    assert_eq!(store.load_arrangement_view_state().unwrap(), None);
    store.close().unwrap();
    remove_database(&path);
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
        .apply(DawAction::CreateBusTrack {
            index: 1,
            name: "Submix".to_owned(),
        })
        .expect("bus creation should succeed");
    let bus_id = project.tracks()[1].id();
    project
        .apply(DawAction::SetTrackOutput {
            track_id,
            output_track: Some(bus_id),
        })
        .expect("track output should route to the bus");
    project
        .apply(DawAction::SetTrackVolume {
            track_id,
            volume_db: -3.0,
        })
        .expect("volume change should succeed");
    project
        .apply(DawAction::SetTrackVolumeAutomation {
            track_id,
            points: vec![
                VolumeAutomationPoint::new(0, -12.0).unwrap(),
                VolumeAutomationPoint::new(48_000, 0.0).unwrap(),
            ],
        })
        .expect("track volume automation should persist");
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
        .apply(DawAction::SetTrackRecordArm {
            track_id,
            armed: true,
        })
        .expect("record arm should be saved with the project");
    project
        .apply(DawAction::SetTrackInstrument {
            track_id,
            instrument: Some(
                aaadaw_core::TrackInstrument::new(
                    "org.example.piano",
                    "/home/user/.clap/piano.clap",
                )
                .expect("a plugin ID and bundle path make a valid instrument reference")
                .with_state(Some(vec![0, 1, 2, 255])),
            ),
        })
        .expect("instrument assignment should succeed");
    project
        .apply(DawAction::SetTrackFxChain {
            track_id,
            plugins: vec![
                aaadaw_core::TrackFxPlugin::new("org.example.room", "/home/user/.clap/room.clap")
                    .expect("first plugin reference should be valid")
                    .with_state(Some(vec![42, 0, 17])),
                aaadaw_core::TrackFxPlugin::new(
                    "org.example.limiter",
                    "/home/user/.clap/limiter.clap",
                )
                .expect("second plugin reference should be valid")
                .with_enabled(false),
            ],
        })
        .expect("track FX chain should be assigned");
    project
        .apply(DawAction::SetTrackFxParameter {
            track_id,
            chain_index: 1,
            parameter_id: 23,
            before: 0.0,
            after: 0.375,
            before_state: None,
            after_state: None,
        })
        .expect("host parameter value should be stored independently of CLAP State");
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
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![
                MidiControllerData {
                    controller: 64,
                    tick: 0,
                    value: 127,
                },
                MidiControllerData {
                    controller: 64,
                    tick: 960,
                    value: 0,
                },
                MidiControllerData {
                    controller: 1,
                    tick: 480,
                    value: 64,
                },
                MidiControllerData {
                    controller: 7,
                    tick: 360,
                    value: 88,
                },
                MidiControllerData {
                    controller: 10,
                    tick: 480,
                    value: 32,
                },
                MidiControllerData {
                    controller: 11,
                    tick: 720,
                    value: 96,
                },
            ],
        })
        .expect("MIDI sustain events should be stored in the project");
    project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![
                MidiPitchBendData {
                    tick: 0,
                    value: 8192,
                },
                MidiPitchBendData {
                    tick: 720,
                    value: 11_337,
                },
            ],
        })
        .expect("MIDI pitch bends should be stored in the project");
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
        .apply(DawAction::SetTempo {
            start_tick: 1920,
            bpm: 105.0,
        })
        .expect("second tempo point should be accepted");
    project
        .apply(DawAction::SetTempoCurve {
            start_tick: 960,
            curve: aaadaw_core::TempoCurve::Logarithmic,
        })
        .expect("logarithmic tempo curve should be accepted");
    project
        .apply(DawAction::SetTempo {
            start_tick: 2880,
            bpm: 120.0,
        })
        .expect("third tempo point should be accepted");
    project
        .apply(DawAction::SetTempoCurve {
            start_tick: 1920,
            curve: aaadaw_core::TempoCurve::Bézier,
        })
        .expect("Bézier tempo curve should be accepted");
    project
        .apply(DawAction::SetTimeSignatureMap {
            points: vec![
                MeterPointSnapshot {
                    start_tick: 0,
                    numerator: 3,
                    denominator: 4,
                },
                MeterPointSnapshot {
                    start_tick: 2880,
                    numerator: 7,
                    denominator: 8,
                },
            ],
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
    assert!(!PathBuf::from(format!("{}-wal", path.display())).exists());
    assert!(!PathBuf::from(format!("{}-shm", path.display())).exists());

    let copy_path = path.with_extension("copy.aaadaw");
    std::fs::copy(&path, &copy_path).expect("closed project should copy as one file");
    let copy = ProjectStore::open(&copy_path).expect("copied project should reopen");
    assert_eq!(
        copy.load().unwrap().snapshot(),
        project.snapshot(),
        "single-file copy should preserve the saved project"
    );
    copy.close().expect("copied project should close cleanly");
    remove_database(&path);
    remove_database(&copy_path);
}

#[test]
fn checkpoint_reports_busy_while_a_reader_pins_the_wal_and_recovers_afterward() {
    let path = project_path();
    let mut store = ProjectStore::open(&path).expect("project should open");
    store
        .save(&Project::new())
        .expect("initial state should save");

    let mut reader = Connection::open(&path).expect("reader should open the project");
    reader
        .busy_timeout(Duration::from_millis(25))
        .expect("reader busy timeout should be configured");
    let read_transaction = reader.transaction().expect("read transaction should begin");
    read_transaction
        .query_row("SELECT COUNT(*) FROM tracks", [], |row| {
            row.get::<_, i64>(0)
        })
        .expect("reader should pin its WAL snapshot");

    let mut changed_project = Project::new();
    changed_project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "WAL checkpoint".to_owned(),
        })
        .expect("track should be created");
    store
        .save(&changed_project)
        .expect("writer should commit while a reader is active");

    assert!(matches!(
        store.checkpoint(),
        Err(StorageError::CheckpointBusy)
    ));
    drop(read_transaction);
    drop(reader);

    store
        .checkpoint()
        .expect("checkpoint should succeed after the reader closes");
    store.close().expect("project should close cleanly");
    remove_database(&path);
}

#[test]
fn schema_twelve_migrates_v11_tempo_curve_values() {
    let path = project_path();
    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
    project
        .apply(DawAction::SetTempo {
            start_tick: 1920,
            bpm: 80.0,
        })
        .expect("tempo point should be accepted");
    project
        .apply(DawAction::SetTempoCurve {
            start_tick: 0,
            curve: aaadaw_core::TempoCurve::Linear,
        })
        .expect("linear tempo curve should be accepted");

    let mut store = ProjectStore::open(&path).expect("database should open");
    store.save(&project).expect("project should save");
    store.close().expect("database should close");

    let connection = Connection::open(&path).expect("closed project should be editable");
    connection
        .execute_batch(
            "DROP TABLE arrangement_fx_lanes;
             DROP TABLE arrangement_volume_lanes;
             DROP TABLE arrangement_view_meta;
             ALTER TABLE tracks DROP COLUMN frozen_audio_item_id;
             ALTER TABLE items DROP COLUMN source_offset_ticks; ALTER TABLE items DROP COLUMN name;
             ALTER TABLE project_meta DROP COLUMN pan_mode; PRAGMA user_version = 11;
             DROP TABLE track_fx_parameter_automation_points;
             CREATE TABLE tempo_points_v11 (
                 start_tick INTEGER PRIMARY KEY CHECK (start_tick >= 0),
                 bpm REAL NOT NULL CHECK (bpm > 0),
                 curve_to_next INTEGER NOT NULL DEFAULT 0 CHECK (curve_to_next IN (0, 1))
             );
             INSERT INTO tempo_points_v11 SELECT * FROM tempo_points;
             DROP TABLE tempo_points;
             ALTER TABLE tempo_points_v11 RENAME TO tempo_points;",
        )
        .expect("database should match the v11 tempo schema");
    drop(connection);

    let store = ProjectStore::open(&path).expect("v11 project should migrate");
    assert_eq!(
        store.schema_version().expect("schema should be readable"),
        CURRENT_SCHEMA_VERSION
    );
    let restored = store.load().expect("v11 project state should load");
    assert_eq!(restored.snapshot(), project.snapshot());
    store.close().expect("migrated project should close");
    remove_database(&path);
}

#[test]
fn schema_two_tracks_migrate_without_an_instrument_assignment() {
    let path = project_path();
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Legacy".to_owned(),
        })
        .expect("legacy track should be created");
    let mut store = ProjectStore::open(&path).expect("new project should open");
    store.save(&project).expect("legacy state should save");
    store.close().expect("project should close");

    let connection = Connection::open(&path).expect("project should be SQLite");
    connection
        .execute_batch(
            "DROP TABLE arrangement_fx_lanes; \
             DROP TABLE arrangement_volume_lanes; \
             DROP TABLE arrangement_view_meta; \
             ALTER TABLE tracks DROP COLUMN frozen_audio_item_id; \
             ALTER TABLE items DROP COLUMN source_offset_ticks; ALTER TABLE items DROP COLUMN name; \
             ALTER TABLE tracks DROP COLUMN instrument_path; \
             ALTER TABLE tracks DROP COLUMN instrument_id; \
             ALTER TABLE tracks DROP COLUMN instrument_state; \
             ALTER TABLE tracks DROP COLUMN record_armed; \
             ALTER TABLE track_fx_plugins DROP COLUMN state; \
             DROP TABLE track_fx_parameter_values; \
             DROP TABLE track_fx_parameter_automation_points; \
             DROP TABLE midi_controllers; \
             DROP TABLE midi_pitch_bends; \
             DROP TABLE track_volume_automation; \
             ALTER TABLE tracks DROP COLUMN output_track_id; \
             ALTER TABLE tracks DROP COLUMN is_bus; \
             DROP TABLE track_fx_plugins; \
             ALTER TABLE project_meta DROP COLUMN pan_mode; PRAGMA user_version = 2;",
        )
        .expect("remove v3 columns to represent a v2 project");
    drop(connection);

    let store = ProjectStore::open(&path).expect("v2 project should migrate to the current schema");
    assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    let migrated = store.load().expect("migrated project should load");
    assert_eq!(migrated.tracks()[0].name(), "Legacy");
    assert_eq!(migrated.tracks()[0].instrument(), None);
    assert!(!migrated.tracks()[0].is_record_armed());
    store.close().expect("migrated project should close");
    remove_database(&path);
}

#[test]
fn mismatched_stored_track_instrument_fields_are_rejected() {
    let path = project_path();
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Broken".to_owned(),
        })
        .expect("track should be created");
    let mut store = ProjectStore::open(&path).expect("project should open");
    store.save(&project).expect("project should save");
    store.close().expect("project should close");

    let connection = Connection::open(&path).expect("project should remain a SQLite file");
    connection
        .execute(
            "UPDATE tracks SET instrument_id = ?1 WHERE id = 0",
            ["org.example.broken"],
        )
        .expect("simulate a partial instrument reference");
    drop(connection);

    let store = ProjectStore::open(&path).expect("project database should open");
    assert!(matches!(
        store.load(),
        Err(StorageError::InvalidStoredData(
            "track instrument reference"
        ))
    ));
    store
        .close()
        .expect("project should close after a read error");
    remove_database(&path);
}

#[test]
fn project_store_recovers_committed_state_after_unexpected_process_exit() {
    const CHILD_PATH_ENV: &str = "AAADAW_TEST_WAL_CHILD_PATH";
    if let Some(path) = std::env::var_os(CHILD_PATH_ENV) {
        let path = PathBuf::from(path);
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Recovered".to_owned(),
            })
            .expect("child project state should be created");
        let mut store = ProjectStore::open(&path).expect("child project should open");
        store
            .save(&project)
            .expect("child project should commit to the WAL");
        let wal_path = PathBuf::from(format!("{}-wal", path.display()));
        assert!(
            std::fs::metadata(&wal_path).is_ok_and(|metadata| metadata.len() > 32),
            "child must exit with committed frames still in the WAL"
        );

        // Skip ProjectStore::close and Rust destructors to simulate a process crash.
        std::process::exit(0);
    }

    let path = project_path();
    let child = Command::new(std::env::current_exe().expect("test executable path should exist"))
        .arg("--exact")
        .arg("project_store_recovers_committed_state_after_unexpected_process_exit")
        .env(CHILD_PATH_ENV, &path)
        .status()
        .expect("WAL recovery child should start");
    assert!(child.success(), "WAL recovery child should commit and exit");

    let store = ProjectStore::open(&path).expect("project should recover from the WAL");
    let recovered = store.load().expect("recovered project should load");
    assert_eq!(recovered.tracks().len(), 1);
    assert_eq!(recovered.tracks()[0].name(), "Recovered");
    store
        .close()
        .expect("recovered project should close cleanly");
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
             DROP TABLE audio_asset_storage_chunks; DROP TABLE audio_asset_metadata; \
             DROP TABLE audio_asset_links; DROP TABLE audio_assets",
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
fn changed_source_reimport_creates_a_new_snapshot_and_action_updates_placements() {
    let database_path = project_path();
    let source_path = database_path.with_extension("wav");
    std::fs::write(&source_path, [1, 2, 3, 4]).expect("original source should be written");
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    let old_ref = store
        .import_audio_file(&source_path)
        .expect("original source should embed");
    let mut project = Project::new();
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
            media_ref: old_ref.clone(),
            start_sample: 123,
            source_offset_samples: 4,
            length_samples: 8,
        })
        .expect("item should reference the original snapshot");
    let item = project.audio_items()[0].clone();

    std::fs::write(&source_path, [9, 8, 7, 6, 5]).expect("original source should be changed");
    assert_eq!(
        store
            .audio_asset_source_status(&old_ref)
            .expect("source change should be detected"),
        aaadaw_storage::AudioAssetSourceStatus::Changed
    );
    let new_ref = store
        .import_audio_file(&source_path)
        .expect("changed file should create a new immutable snapshot");
    assert_ne!(new_ref, old_ref);

    project
        .apply(DawAction::EditAudioItem {
            item_id: item.id(),
            media_ref: new_ref.clone(),
            start_sample: item.start_sample(),
            source_offset_samples: item.source_offset_samples(),
            length_samples: item.length_samples(),
        })
        .expect("existing edit action should retarget the placement");
    assert_eq!(project.audio_items()[0].media_ref(), new_ref);
    assert!(project.undo().expect("retargeting should be undoable"));
    assert_eq!(project.audio_items()[0].media_ref(), old_ref);
    assert!(project.redo().expect("retargeting should be redoable"));
    assert_eq!(project.audio_items()[0].media_ref(), new_ref);

    store
        .save(&project)
        .expect("retargeted project should save");
    store.close().expect("project should close");
    let reopened = ProjectStore::open(&database_path).expect("project should reopen");
    let restored = reopened.load().expect("project snapshot should load");
    assert_eq!(restored.audio_items()[0].media_ref(), new_ref);
    let mut old_asset = reopened
        .audio_asset_reader(&old_ref)
        .expect("original immutable asset should remain available");
    let mut old_bytes = Vec::new();
    old_asset
        .read_to_end(&mut old_bytes)
        .expect("original snapshot should remain readable");
    assert_eq!(old_bytes, [1, 2, 3, 4]);

    drop(old_asset);
    reopened.close().expect("reopened project should close");
    remove_database(&database_path);
    std::fs::remove_file(source_path).expect("source file should be removed");
}

#[test]
fn background_source_scan_reports_embedded_changes_and_live_links() {
    let database_path = project_path();
    let imported_path = database_path.with_extension("imported.wav");
    let linked_path = database_path.with_extension("linked.wav");
    let original_bytes = vec![0x21; 128_000];
    std::fs::write(&imported_path, &original_bytes).expect("import source should be written");
    std::fs::write(&linked_path, [1, 2, 3]).expect("linked source should be written");
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    let embedded_ref = store
        .import_audio_file(&imported_path)
        .expect("file should be imported");
    let linked_ref = store
        .link_external_audio_file(&linked_path)
        .expect("second source should be linked");
    std::fs::write(&imported_path, vec![0x22; original_bytes.len()])
        .expect("original source should be modified");

    let worker = store
        .start_audio_asset_source_scan()
        .expect("source scan should start");
    let mut progress = Vec::new();
    while let Ok(update) = worker.progress().recv() {
        progress.push(update);
    }
    let results = worker.join().expect("source scan should complete");
    assert!(results.contains(&(
        embedded_ref.clone(),
        aaadaw_storage::AudioAssetSourceStatus::Changed
    )));
    assert!(results.contains(&(
        linked_ref.clone(),
        aaadaw_storage::AudioAssetSourceStatus::Linked
    )));
    assert_eq!(progress.len(), 2);
    assert!(progress.iter().all(|update| update.total_assets == 2));
    assert_eq!(progress.last().unwrap().scanned_assets, 2);

    store.close().expect("project should close");
    remove_database(&database_path);
    std::fs::remove_file(imported_path).expect("import source should be removed");
    std::fs::remove_file(linked_path).expect("linked source should be removed");
}

#[test]
fn background_pack_worker_reports_asset_progress_and_packs_linked_files() {
    let database_path = project_path();
    let source_path = database_path.with_extension("wav");
    let source_bytes = (0_usize..700_123)
        .map(|index| (index.wrapping_mul(23) % 251) as u8)
        .collect::<Vec<_>>();
    std::fs::write(&source_path, &source_bytes).expect("source fixture should be written");
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    let media_ref = store
        .link_external_audio_file(&source_path)
        .expect("source should link");

    let worker = store
        .start_audio_asset_pack_all()
        .expect("background pack should start");
    let mut progress = Vec::new();
    while let Ok(update) = worker.progress().recv() {
        progress.push(update);
    }
    assert_eq!(
        worker.join().expect("pack worker should finish"),
        vec![media_ref.clone()]
    );
    assert!(progress.iter().any(|update| {
        update.media_ref == media_ref
            && !update.asset_complete
            && update.bytes_imported == 0
            && update.total_bytes == source_bytes.len() as u64
            && update.completed_assets == 0
            && update.total_assets == 1
    }));
    assert!(progress.last().is_some_and(|update| {
        update.media_ref == media_ref
            && update.asset_complete
            && update.bytes_imported == source_bytes.len() as u64
            && update.completed_assets == 1
            && update.total_assets == 1
    }));

    let mut resolved = store
        .resolve_audio_asset(&media_ref)
        .expect("completed asset should resolve");
    let aaadaw_storage::ResolvedAudioAsset::Embedded(reader) = &mut resolved else {
        panic!("completed pack should resolve as embedded");
    };
    let mut embedded = Vec::new();
    reader
        .read_to_end(&mut embedded)
        .expect("packed bytes should be readable");
    assert_eq!(embedded, source_bytes);

    drop(resolved);
    store.close().expect("project should close");
    remove_database(&database_path);
    std::fs::remove_file(source_path).expect("external fixture should be removed");
}

#[test]
fn cancelled_external_pack_keeps_its_link_and_can_be_retried() {
    let database_path = project_path();
    let source_path = database_path.with_extension("wav");
    std::fs::write(&source_path, vec![0x5a; 700_123]).expect("source fixture should be written");
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    let media_ref = store
        .link_external_audio_file(&source_path)
        .expect("source should link");

    let cancelled = store.pack_all_external_audio_assets_with_progress(|update| {
        if !update.asset_complete && update.bytes_imported > 0 {
            Err(StorageError::AudioAssetPackCancelled)
        } else {
            Ok(())
        }
    });
    assert!(matches!(
        cancelled,
        Err(StorageError::AudioAssetPackCancelled)
    ));
    assert!(matches!(
        store.resolve_audio_asset(&media_ref),
        Ok(aaadaw_storage::ResolvedAudioAsset::LinkedFile { .. })
    ));
    assert!(matches!(
        store.audio_asset_reader(&media_ref),
        Err(StorageError::AudioAssetNotFound(_))
    ));
    assert_eq!(
        store
            .pack_all_external_audio_assets()
            .expect("cancelled pack should be retryable"),
        vec![media_ref]
    );

    store.close().expect("project should close");
    remove_database(&database_path);
    std::fs::remove_file(source_path).expect("external fixture should be removed");
}

#[test]
fn decoder_metadata_persists_only_for_complete_embedded_assets() {
    let database_path = project_path();
    let linked_path = database_path.with_extension("linked.wav");
    std::fs::write(&linked_path, [1, 2, 3]).expect("linked source should be written");
    let mut store = ProjectStore::open(&database_path).expect("project should open");
    let embedded_ref = "asset://metadata";
    store
        .import_audio_asset(embedded_ref, "metadata.wav", Cursor::new([1, 2, 3, 4]))
        .expect("embedded asset should import");
    let linked_ref = store
        .link_external_audio_file(&linked_path)
        .expect("external file should link");
    let metadata = aaadaw_storage::AudioAssetMetadata {
        container: "wav".to_owned(),
        codec: "pcm".to_owned(),
        sample_rate: Some(48_000),
        channel_count: Some(2),
        bits_per_sample: Some(24),
        frame_count: Some(256),
        duration_nanos: Some(5_333_333),
        byte_len: Some(4),
    };
    store
        .set_audio_asset_metadata(embedded_ref, &metadata)
        .expect("metadata should attach to the embedded snapshot");
    assert_eq!(
        store
            .audio_asset_metadata(embedded_ref)
            .expect("metadata should be queryable"),
        Some(metadata.clone())
    );
    assert!(matches!(
        store.set_audio_asset_metadata(&linked_ref, &metadata),
        Err(StorageError::AudioAssetNotFound(_))
    ));

    store.close().expect("project should close");
    let reopened = ProjectStore::open(&database_path).expect("project should reopen");
    assert_eq!(
        reopened
            .audio_asset_metadata(embedded_ref)
            .expect("metadata should survive reopen"),
        Some(metadata)
    );
    reopened.close().expect("reopened project should close");
    remove_database(&database_path);
    std::fs::remove_file(linked_path).expect("linked fixture should be removed");
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
fn unreferenced_audio_asset_cleanup_preserves_assets_used_by_saved_items() {
    let path = project_path();
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".to_owned(),
        })
        .expect("track should be created");
    let track_id = project.tracks()[0].id();
    let mut store = ProjectStore::open(&path).expect("project store should open");
    store.save(&project).expect("project should be persisted");

    store
        .import_audio_asset("asset://unplaced", "unused.wav", Cursor::new([1, 2, 3]))
        .expect("unplaced asset should import");
    assert!(
        store
            .remove_unreferenced_audio_asset("asset://unplaced")
            .expect("unreferenced asset should be removed")
    );
    assert!(matches!(
        store.audio_asset_reader("asset://unplaced"),
        Err(StorageError::AudioAssetNotFound(_))
    ));

    store
        .import_audio_asset("asset://placed", "used.wav", Cursor::new([4, 5, 6]))
        .expect("placed asset should import");
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://placed".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 1,
        })
        .expect("item should reference asset");
    store.save(&project).expect("placement should be persisted");
    assert!(
        !store
            .remove_unreferenced_audio_asset("asset://placed")
            .expect("referenced asset should be retained")
    );
    assert!(store.audio_asset_reader("asset://placed").is_ok());

    store.close().expect("project store should close");
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

#[test]
fn schema_six_projects_migrate_host_fx_parameter_storage() {
    let path = project_path();
    let store = ProjectStore::open(&path).expect("new project should open");
    store.close().expect("project should close");
    let connection = Connection::open(&path).expect("project should be SQLite");
    connection
        .execute_batch(
            "DROP TABLE arrangement_fx_lanes; DROP TABLE arrangement_volume_lanes; DROP TABLE arrangement_view_meta; DROP TABLE track_fx_parameter_automation_points; DROP TABLE track_volume_automation; ALTER TABLE tracks DROP COLUMN frozen_audio_item_id; ALTER TABLE tracks DROP COLUMN output_track_id; ALTER TABLE tracks DROP COLUMN is_bus; ALTER TABLE items DROP COLUMN source_offset_ticks; ALTER TABLE items DROP COLUMN name; DROP TABLE track_fx_parameter_values; DROP TABLE midi_controllers; DROP TABLE midi_pitch_bends; ALTER TABLE project_meta DROP COLUMN pan_mode; PRAGMA user_version = 6;",
        )
        .expect("project should resemble a schema-six database");
    drop(connection);

    let store = ProjectStore::open(&path).expect("schema six should migrate to current");
    assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    store.close().expect("migrated project should close");
    remove_database(&path);
}

#[test]
fn schema_seven_projects_migrate_controller_storage_without_changing_notes() {
    let path = project_path();
    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Legacy MIDI".to_owned(),
        })
        .expect("track should be created");
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 3840,
        })
        .expect("MIDI item should be created");
    let item_id = project.midi_items()[0].id();
    project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 60,
                tick: 120,
                duration: 240,
                velocity: 100,
            }],
        })
        .expect("legacy note should be added");
    let mut store = ProjectStore::open(&path).expect("project should open");
    store.save(&project).expect("legacy project should save");
    store.close().expect("project should close");

    let connection = Connection::open(&path).expect("project should be SQLite");
    connection
        .execute_batch("DROP TABLE arrangement_fx_lanes; DROP TABLE arrangement_volume_lanes; DROP TABLE arrangement_view_meta; DROP TABLE track_fx_parameter_automation_points; DROP TABLE track_volume_automation; ALTER TABLE tracks DROP COLUMN frozen_audio_item_id; ALTER TABLE tracks DROP COLUMN output_track_id; ALTER TABLE tracks DROP COLUMN is_bus; ALTER TABLE items DROP COLUMN source_offset_ticks; ALTER TABLE items DROP COLUMN name; DROP TABLE midi_controllers; DROP TABLE midi_pitch_bends; ALTER TABLE project_meta DROP COLUMN pan_mode; PRAGMA user_version = 7;")
        .expect("project should resemble a schema-seven database");
    drop(connection);

    let store = ProjectStore::open(&path).expect("schema seven should migrate");
    assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    assert_eq!(store.load().unwrap().snapshot(), project.snapshot());
    store.close().expect("migrated project should close");
    remove_database(&path);
}

#[test]
fn schema_eight_projects_migrate_volume_automation_storage_without_changing_tracks() {
    let path = project_path();
    let mut project = Project::with_settings(
        aaadaw_core::ProjectSettings::default()
            .with_pan_mode(aaadaw_core::PanMode::LegacyMonoStereo),
    );
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Legacy track".to_owned(),
        })
        .expect("track should be created");
    let mut store = ProjectStore::open(&path).expect("project should open");
    store.save(&project).expect("project should save");
    store.close().expect("project should close");

    let connection = Connection::open(&path).expect("project should be SQLite");
    connection
        .execute_batch("DROP TABLE arrangement_fx_lanes; DROP TABLE arrangement_volume_lanes; DROP TABLE arrangement_view_meta; DROP TABLE track_fx_parameter_automation_points; DROP TABLE track_volume_automation; ALTER TABLE tracks DROP COLUMN frozen_audio_item_id; ALTER TABLE tracks DROP COLUMN output_track_id; ALTER TABLE tracks DROP COLUMN is_bus; ALTER TABLE items DROP COLUMN source_offset_ticks; ALTER TABLE items DROP COLUMN name; DROP TABLE midi_pitch_bends; ALTER TABLE project_meta DROP COLUMN pan_mode; PRAGMA user_version = 8;")
        .expect("project should resemble a schema-eight database");
    drop(connection);

    let store = ProjectStore::open(&path).expect("schema eight should migrate");
    assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    assert_eq!(store.load().unwrap().snapshot(), project.snapshot());
    store.close().expect("migrated project should close");
    remove_database(&path);
}

#[test]
fn schema_nine_projects_migrate_tracks_to_master_by_default() {
    let path = project_path();
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Legacy".to_owned(),
        })
        .unwrap();
    let mut store = ProjectStore::open(&path).expect("project should open");
    store.save(&project).expect("project should save");
    store.close().expect("project should close");

    let connection = Connection::open(&path).expect("project should be SQLite");
    connection.execute_batch("DROP TABLE arrangement_fx_lanes; DROP TABLE arrangement_volume_lanes; DROP TABLE arrangement_view_meta; DROP TABLE track_fx_parameter_automation_points; ALTER TABLE tracks DROP COLUMN frozen_audio_item_id; ALTER TABLE tracks DROP COLUMN output_track_id; ALTER TABLE tracks DROP COLUMN is_bus; ALTER TABLE items DROP COLUMN source_offset_ticks; ALTER TABLE items DROP COLUMN name; DROP TABLE midi_pitch_bends; ALTER TABLE project_meta DROP COLUMN pan_mode; PRAGMA user_version = 9;").unwrap();
    drop(connection);
    let store = ProjectStore::open(&path).expect("schema nine should migrate");
    let restored = store.load().unwrap();
    assert!(!restored.tracks()[0].is_bus());
    assert_eq!(restored.tracks()[0].output_track(), None);
    store.close().expect("migrated project should close");
    remove_database(&path);
}
