use crate::{
    ArrangementViewState, FxAutomationLaneViewState, GridDivision, VolumeAutomationLaneViewState,
};
use aaadaw_core::{
    AudioItemSnapshot, BpmBasis, MeterPointSnapshot, MidiChannelPressureData, MidiControllerData,
    MidiEventOrderData, MidiEventOrderTarget, MidiItemSnapshot, MidiItemTakeSnapshot, MidiNoteData,
    MidiNoteSnapshot, MidiOpaqueEventData, MidiOpaqueEventKind, MidiPitchBendData,
    MidiPolyAftertouchData, MidiProgramChangeData, PanMode, Project, ProjectMarkerSnapshot,
    ProjectRegionSnapshot, ProjectSettings, ProjectSnapshot, RazorEditArea, SnapshotError,
    TempoCurve, TempoPointSnapshot, TrackFxParameterAutomationLaneSnapshot,
    TrackFxParameterAutomationPointSnapshot, TrackFxParameterValueSnapshot, TrackFxPluginSnapshot,
    TrackInstrumentSnapshot, TrackSnapshot, VolumeAutomationPoint,
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
pub const CURRENT_SCHEMA_VERSION: u32 = 69;

const EMPTY_ACTIVE_MIDI_TAKE_NAME: &str = "__AAADAW_EMPTY_ACTIVE_MIDI_TAKE__";

const MIGRATION_29: &str = "ALTER TABLE project_meta ADD COLUMN start_measure INTEGER NOT NULL DEFAULT 1 CHECK(start_measure BETWEEN -2147483648 AND 2147483647);";
const MIGRATION_30: &str = "ALTER TABLE project_meta ADD COLUMN bpm_basis INTEGER NOT NULL DEFAULT 1 CHECK(bpm_basis BETWEEN 0 AND 5);";
const MIGRATION_31: &str = "ALTER TABLE project_meta ADD COLUMN base_ruler_label_spacing_on_measure INTEGER NOT NULL DEFAULT 1 CHECK(base_ruler_label_spacing_on_measure IN (0,1));";
// SQLite parses these sentinels as infinities, so the check accepts every finite f64.
const MIGRATION_32: &str = "ALTER TABLE project_meta ADD COLUMN project_start_time_offset_seconds REAL NOT NULL DEFAULT 0 CHECK(project_start_time_offset_seconds > -1.0e999 AND project_start_time_offset_seconds < 1.0e999);";
const MIGRATION_33: &str = "ALTER TABLE project_meta ADD COLUMN project_start_time_fractional_frame_offset_allowed INTEGER NOT NULL DEFAULT 0 CHECK(project_start_time_fractional_frame_offset_allowed IN (0,1));";
const MIGRATION_34: &str = "ALTER TABLE project_meta ADD COLUMN force_tempo_changes_to_whole_samples INTEGER NOT NULL DEFAULT 0 CHECK(force_tempo_changes_to_whole_samples IN (0,1));";
const MIGRATION_35: &str = r#"
CREATE TABLE IF NOT EXISTS project_markers (
    id INTEGER PRIMARY KEY CHECK (id >= 0),
    position_ticks INTEGER NOT NULL CHECK (position_ticks >= 0),
    name TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS project_markers_by_position ON project_markers(position_ticks, id);
"#;
const MIGRATION_36: &str = r#"
ALTER TABLE project_markers ADD COLUMN marker_number INTEGER NOT NULL DEFAULT 1 CHECK(marker_number BETWEEN 1 AND 2147483647);
UPDATE project_markers SET marker_number = CASE WHEN id >= 2147483646 THEN 2147483647 ELSE id + 1 END;
"#;
const MIGRATION_37: &str = r#"
CREATE TABLE IF NOT EXISTS project_regions (
    id INTEGER PRIMARY KEY CHECK (id >= 0),
    region_number INTEGER NOT NULL CHECK (region_number BETWEEN 1 AND 2147483647),
    start_ticks INTEGER NOT NULL CHECK (start_ticks >= 0),
    end_ticks INTEGER NOT NULL CHECK (end_ticks > start_ticks),
    name TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS project_regions_by_start ON project_regions(start_ticks, id);
"#;
const MIGRATION_53: &str = r#"
CREATE TABLE IF NOT EXISTS project_region_render_tracks (
    region_id INTEGER NOT NULL REFERENCES project_regions(id) ON DELETE CASCADE,
    track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK(position >= 0),
    PRIMARY KEY(region_id, track_id),
    UNIQUE(region_id, position)
);
"#;
const MIGRATION_54: &str = "ALTER TABLE audio_items ADD COLUMN name TEXT NOT NULL DEFAULT '';";
const MIGRATION_56: &str = "ALTER TABLE items ADD COLUMN start_tick_subtick INTEGER NOT NULL DEFAULT 0 CHECK (start_tick_subtick BETWEEN 0 AND 199);";

const MIGRATION_38: &str = r#"
CREATE TABLE IF NOT EXISTS arrangement_time_ranges (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    time_selection_start_seconds REAL,
    time_selection_end_seconds REAL,
    loop_start_seconds REAL,
    loop_end_seconds REAL,
    CHECK((time_selection_start_seconds IS NULL) = (time_selection_end_seconds IS NULL)),
    CHECK(time_selection_start_seconds IS NULL OR (
        time_selection_start_seconds > -1.0e999 AND time_selection_start_seconds < 1.0e999 AND
        time_selection_end_seconds > -1.0e999 AND time_selection_end_seconds < 1.0e999 AND
        time_selection_start_seconds < time_selection_end_seconds
    )),
    CHECK((loop_start_seconds IS NULL) = (loop_end_seconds IS NULL)),
    CHECK(loop_start_seconds IS NULL OR (
        loop_start_seconds > -1.0e999 AND loop_start_seconds < 1.0e999 AND
        loop_end_seconds > -1.0e999 AND loop_end_seconds < 1.0e999 AND
        loop_start_seconds <= loop_end_seconds
    ))
);
"#;

const MIGRATION_39: &str = r#"
ALTER TABLE audio_items ADD COLUMN volume REAL NOT NULL DEFAULT 1.0
    CHECK(volume >= 0.0 AND volume <= 16.0);
"#;

const MIGRATION_40_TAKE_VOLUME: &str = r#"
ALTER TABLE audio_items ADD COLUMN take_volume REAL NOT NULL DEFAULT 1.0
    CHECK(take_volume >= 0.0 AND take_volume <= 16.0);
"#;
const MIGRATION_40_TAKE_PAN: &str = r#"
ALTER TABLE audio_items ADD COLUMN take_pan REAL NOT NULL DEFAULT 0.0
    CHECK(take_pan >= -1.0 AND take_pan <= 1.0);
"#;

const MIGRATION_41: &str = r#"
CREATE TABLE IF NOT EXISTS track_razor_edit_areas (
    track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK(position >= 0),
    start_sample INTEGER NOT NULL CHECK(start_sample >= 0),
    end_sample INTEGER NOT NULL CHECK(end_sample > start_sample),
    PRIMARY KEY(track_id, position)
);
"#;

const MIGRATION_28: &str = "CREATE TABLE IF NOT EXISTS project_notes(singleton INTEGER PRIMARY KEY CHECK(singleton = 1), title TEXT NOT NULL, author TEXT NOT NULL, notes TEXT NOT NULL, show_on_load INTEGER NOT NULL CHECK(show_on_load IN (0,1)), word_wrap INTEGER NOT NULL CHECK(word_wrap IN (0,1))); INSERT OR IGNORE INTO project_notes VALUES(1, '', '', '', 0, 1);";

const MIGRATION_27: &str = "CREATE TABLE IF NOT EXISTS project_timecode(singleton INTEGER PRIMARY KEY CHECK(singleton = 1), frame_rate INTEGER NOT NULL CHECK(frame_rate BETWEEN 0 AND 9)); INSERT OR IGNORE INTO project_timecode VALUES(1, 5);";

const MIGRATION_26: &str = "CREATE TABLE IF NOT EXISTS audio_item_fade_curves(item_id INTEGER PRIMARY KEY REFERENCES audio_item_fades(item_id) ON DELETE CASCADE, in_curvature REAL, in_s REAL, out_curvature REAL, out_s REAL, CHECK((in_curvature IS NULL) = (in_s IS NULL)), CHECK((out_curvature IS NULL) = (out_s IS NULL)), CHECK(in_curvature BETWEEN -1 AND 1 AND in_s BETWEEN -1 AND 1), CHECK(out_curvature BETWEEN -1 AND 1 AND out_s BETWEEN -1 AND 1), CHECK(in_curvature IS NOT NULL OR out_curvature IS NOT NULL));";

const MIGRATION_25: &str = "CREATE TABLE IF NOT EXISTS audio_item_fades(item_id INTEGER PRIMARY KEY REFERENCES audio_items(id) ON DELETE CASCADE, fade_in_samples REAL NOT NULL CHECK(fade_in_samples >= 0), fade_out_samples REAL NOT NULL CHECK(fade_out_samples >= 0), fade_in_shape INTEGER NOT NULL CHECK(fade_in_shape BETWEEN 0 AND 6), fade_out_shape INTEGER NOT NULL CHECK(fade_out_shape BETWEEN 0 AND 6));";

const MIGRATION_24: &str = "CREATE TABLE project_master_mix(singleton INTEGER PRIMARY KEY CHECK(singleton = 1), volume_db REAL NOT NULL, pan REAL NOT NULL CHECK(pan BETWEEN -1 AND 1)); INSERT INTO project_master_mix VALUES(1, 0, 0);";
const APPLICATION_ID: i64 = 0x4141_4441;
const PAGE_SIZE: u32 = 4096;

// Additive migration: existing projects retain their historical signal path.
const MIGRATION_23: &str = "ALTER TABLE tracks ADD COLUMN phase_inverted INTEGER NOT NULL DEFAULT 0 CHECK(phase_inverted IN (0, 1));";

const MIGRATION_22: &str = "CREATE TABLE arrangement_folder_compact (track_id INTEGER PRIMARY KEY CHECK(track_id >= 0), mode INTEGER NOT NULL CHECK(mode BETWEEN 0 AND 2));";

const MIGRATION_21: &str = "ALTER TABLE tracks ADD COLUMN is_folder INTEGER NOT NULL DEFAULT 0 CHECK(is_folder IN (0, 1)); ALTER TABLE tracks ADD COLUMN parent_track_id INTEGER REFERENCES tracks(id);";

const MIGRATION_20: &str =
    "ALTER TABLE track_sends ADD COLUMN tap INTEGER NOT NULL DEFAULT 0 CHECK(tap IN (0, 1, 3));";

const MIGRATION_19: &str = "
ALTER TABLE tracks ADD COLUMN main_send_enabled INTEGER NOT NULL DEFAULT 1 CHECK(main_send_enabled IN (0, 1));
CREATE TABLE track_sends (
    id INTEGER PRIMARY KEY CHECK(id >= 0),
    source_track_id INTEGER NOT NULL REFERENCES tracks(id),
    position INTEGER NOT NULL CHECK(position >= 0),
    destination_track_id INTEGER NOT NULL REFERENCES tracks(id),
    volume_db REAL NOT NULL,
    pan REAL NOT NULL CHECK(pan BETWEEN -1 AND 1),
    muted INTEGER NOT NULL CHECK(muted IN (0, 1)),
    phase_inverted INTEGER NOT NULL CHECK(phase_inverted IN (0, 1)),
    UNIQUE(source_track_id, position)
);";

const MIGRATION_18: &str = "ALTER TABLE project_meta ADD COLUMN pan_mode INTEGER NOT NULL DEFAULT 0 CHECK(pan_mode IN (0, 1));";

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
    start_tick_subtick INTEGER NOT NULL DEFAULT 0 CHECK (start_tick_subtick BETWEEN 0 AND 199),
    length_ticks INTEGER NOT NULL CHECK (length_ticks > 0),
    muted INTEGER NOT NULL DEFAULT 0 CHECK (muted IN (0, 1)),
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

const MIGRATION_6: &str = r#"
ALTER TABLE tracks ADD COLUMN instrument_state BLOB;
ALTER TABLE track_fx_plugins ADD COLUMN state BLOB;
"#;

const MIGRATION_7: &str = r#"
CREATE TABLE track_fx_parameter_values (
    track_id INTEGER NOT NULL,
    position INTEGER NOT NULL,
    parameter_id INTEGER NOT NULL CHECK (parameter_id BETWEEN 0 AND 4294967295),
    value REAL NOT NULL CHECK (value BETWEEN -1.7976931348623157e308 AND 1.7976931348623157e308),
    PRIMARY KEY (track_id, position, parameter_id),
    FOREIGN KEY (track_id, position)
        REFERENCES track_fx_plugins(track_id, position) ON DELETE CASCADE
);
"#;

const MIGRATION_8: &str = r#"
CREATE TABLE midi_controllers (
    item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    controller INTEGER NOT NULL CHECK (controller BETWEEN 0 AND 127),
    tick INTEGER NOT NULL CHECK (tick >= 0),
    value INTEGER NOT NULL CHECK (value BETWEEN 0 AND 127),
    PRIMARY KEY (item_id, position),
    UNIQUE (item_id, controller, tick)
);
CREATE INDEX midi_controllers_by_item_tick ON midi_controllers(item_id, tick, controller);
"#;

const MIGRATION_9: &str = r#"
CREATE TABLE track_volume_automation (
    track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    sample INTEGER NOT NULL CHECK (sample >= 0),
    gain_db REAL NOT NULL CHECK (gain_db BETWEEN -60 AND 6),
    PRIMARY KEY (track_id, position),
    UNIQUE (track_id, sample)
);
CREATE INDEX track_volume_automation_by_sample
    ON track_volume_automation(track_id, sample);
"#;

const MIGRATION_10: &str = r#"
ALTER TABLE tracks ADD COLUMN is_bus INTEGER NOT NULL DEFAULT 0 CHECK (is_bus IN (0, 1));
ALTER TABLE tracks ADD COLUMN output_track_id INTEGER REFERENCES tracks(id)
    DEFERRABLE INITIALLY DEFERRED;
"#;

const MIGRATION_11: &str = r#"
CREATE TABLE midi_pitch_bends (
    item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    tick INTEGER NOT NULL CHECK (tick >= 0),
    value INTEGER NOT NULL CHECK (value BETWEEN 0 AND 16383),
    PRIMARY KEY (item_id, position),
    UNIQUE (item_id, tick)
);
CREATE INDEX midi_pitch_bends_by_item_tick ON midi_pitch_bends(item_id, tick);
"#;

const MIGRATION_12: &str = r#"
CREATE TABLE tempo_points_v12 (
    start_tick INTEGER PRIMARY KEY CHECK (start_tick >= 0),
    bpm REAL NOT NULL CHECK (bpm > 0),
    curve_to_next INTEGER NOT NULL DEFAULT 0 CHECK (curve_to_next BETWEEN 0 AND 3)
);
INSERT INTO tempo_points_v12(start_tick, bpm, curve_to_next)
    SELECT start_tick, bpm, curve_to_next FROM tempo_points;
DROP TABLE tempo_points;
ALTER TABLE tempo_points_v12 RENAME TO tempo_points;
"#;

const MIGRATION_13: &str = r#"
CREATE TABLE track_fx_parameter_automation_points (
    track_id INTEGER NOT NULL,
    position INTEGER NOT NULL,
    parameter_id INTEGER NOT NULL CHECK (parameter_id BETWEEN 0 AND 4294967295),
    point_position INTEGER NOT NULL CHECK (point_position >= 0),
    sample INTEGER NOT NULL CHECK (sample >= 0),
    value REAL NOT NULL CHECK (value BETWEEN -1.7976931348623157e308 AND 1.7976931348623157e308),
    PRIMARY KEY (track_id, position, parameter_id, point_position),
    UNIQUE (track_id, position, parameter_id, sample),
    FOREIGN KEY (track_id, position)
        REFERENCES track_fx_plugins(track_id, position) ON DELETE CASCADE
);
CREATE INDEX track_fx_parameter_automation_by_sample
    ON track_fx_parameter_automation_points(track_id, position, parameter_id, sample);
"#;

const MIGRATION_14: &str = r#"
CREATE TABLE arrangement_view_meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    state_version INTEGER NOT NULL CHECK (state_version = 1)
);

CREATE TABLE arrangement_volume_lanes (
    track_id INTEGER PRIMARY KEY CHECK (track_id >= 0),
    visible INTEGER NOT NULL CHECK (visible IN (0, 1))
);

CREATE TABLE arrangement_fx_lanes (
    track_id INTEGER NOT NULL CHECK (track_id >= 0),
    chain_index INTEGER NOT NULL CHECK (chain_index >= 0),
    plugin_id TEXT NOT NULL,
    bundle_path TEXT NOT NULL,
    parameter_id INTEGER NOT NULL CHECK (parameter_id >= 0),
    name TEXT NOT NULL,
    value_min REAL NOT NULL,
    value_max REAL NOT NULL,
    stepped INTEGER NOT NULL CHECK (stepped IN (0, 1)),
    height REAL NOT NULL CHECK (height > 0),
    PRIMARY KEY (track_id, chain_index, parameter_id)
);
"#;

const MIGRATION_15: &str = r#"
ALTER TABLE tracks ADD COLUMN frozen_audio_item_id INTEGER
    CHECK (frozen_audio_item_id IS NULL OR frozen_audio_item_id >= 0);
"#;

const MIGRATION_16: &str = r#"
ALTER TABLE items ADD COLUMN name TEXT NOT NULL DEFAULT 'MIDI'
    CHECK (length(trim(name)) BETWEEN 1 AND 128);
"#;

const MIGRATION_17: &str = r#"
ALTER TABLE items ADD COLUMN source_offset_ticks INTEGER NOT NULL DEFAULT 0
    CHECK (source_offset_ticks >= 0);
"#;

const MIGRATION_68_MIDI_ACTIVE_EMPTY_TAKE: &str = r#"
ALTER TABLE items ADD COLUMN active_take_name_is_empty INTEGER NOT NULL DEFAULT 0
    CHECK(active_take_name_is_empty IN (0, 1));
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
    name TEXT NOT NULL DEFAULT '',
    start_sample INTEGER NOT NULL CHECK (start_sample >= 0),
    source_offset_samples INTEGER NOT NULL CHECK (source_offset_samples >= 0),
    signed_source_offset_samples INTEGER NOT NULL DEFAULT 0,
    length_samples INTEGER NOT NULL CHECK (length_samples > 0),
    volume REAL NOT NULL DEFAULT 1.0 CHECK (volume >= 0.0 AND volume <= 16.0),
    take_volume REAL NOT NULL DEFAULT 1.0 CHECK (take_volume >= 0.0 AND take_volume <= 16.0),
    take_pan REAL NOT NULL DEFAULT 0.0 CHECK (take_pan >= -1.0 AND take_pan <= 1.0),
    take_playback_rate REAL NOT NULL DEFAULT 1.0 CHECK (take_playback_rate > 0.0),
    muted INTEGER NOT NULL DEFAULT 0 CHECK (muted IN (0, 1)),
    phase_inverted INTEGER NOT NULL DEFAULT 0 CHECK (phase_inverted IN (0, 1)),
    active_take_index INTEGER NOT NULL DEFAULT 0 CHECK (active_take_index >= 0),
    UNIQUE (track_id, position)
);
CREATE INDEX IF NOT EXISTS audio_items_by_project_position ON audio_items(position, id);
"#;

const MIGRATION_49: &str = r#"
ALTER TABLE audio_items ADD COLUMN signed_source_offset_samples INTEGER NOT NULL DEFAULT 0;
UPDATE audio_items SET signed_source_offset_samples = source_offset_samples;
"#;

const MIGRATION_50: &str = r#"
UPDATE audio_items
SET signed_source_offset_samples = signed_source_offset_samples - source_section_start_samples,
    source_offset_samples = MAX(0, signed_source_offset_samples - source_section_start_samples)
WHERE source_section_start_samples IS NOT NULL
  AND source_section_length_samples IS NOT NULL;
"#;

const MIGRATION_51: &str = r#"
ALTER TABLE items ADD COLUMN source_loop_length_ticks INTEGER NOT NULL DEFAULT 1
    CHECK(source_loop_length_ticks > 0);
UPDATE items
SET source_loop_length_ticks = MAX(
    length_ticks,
    COALESCE((
        SELECT MIN(9223372036854775807, MAX(tick + duration))
        FROM midi_notes WHERE midi_notes.item_id = items.id
    ), 0),
    COALESCE((SELECT MIN(9223372036854775807, MAX(tick + 1)) FROM midi_controllers WHERE midi_controllers.item_id = items.id), 0),
    COALESCE((SELECT MIN(9223372036854775807, MAX(tick + 1)) FROM midi_pitch_bends WHERE midi_pitch_bends.item_id = items.id), 0)
);
"#;

const MIGRATION_52: &str = r#"
ALTER TABLE arrangement_view_meta ADD COLUMN repeat_enabled INTEGER NOT NULL DEFAULT 0
    CHECK(repeat_enabled IN (0, 1));
"#;

const MIGRATION_59_CONTROLLERS: &str = r#"
CREATE TABLE midi_controllers_v59 (
    item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    controller INTEGER NOT NULL CHECK (controller BETWEEN 0 AND 127),
    channel INTEGER NOT NULL DEFAULT 0 CHECK (channel BETWEEN 0 AND 15),
    tick INTEGER NOT NULL CHECK (tick >= 0),
    value INTEGER NOT NULL CHECK (value BETWEEN 0 AND 127),
    PRIMARY KEY (item_id, position),
    UNIQUE (item_id, channel, controller, tick)
);
INSERT INTO midi_controllers_v59(item_id, position, controller, channel, tick, value)
    SELECT item_id, position, controller, 0, tick, value FROM midi_controllers;
DROP TABLE midi_controllers;
ALTER TABLE midi_controllers_v59 RENAME TO midi_controllers;
CREATE INDEX midi_controllers_by_item_tick
    ON midi_controllers(item_id, tick, channel, controller);
"#;

const MIGRATION_59_PITCH_BENDS: &str = r#"
CREATE TABLE midi_pitch_bends_v59 (
    item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    channel INTEGER NOT NULL DEFAULT 0 CHECK (channel BETWEEN 0 AND 15),
    tick INTEGER NOT NULL CHECK (tick >= 0),
    value INTEGER NOT NULL CHECK (value BETWEEN 0 AND 16383),
    PRIMARY KEY (item_id, position),
    UNIQUE (item_id, channel, tick)
);
INSERT INTO midi_pitch_bends_v59(item_id, position, channel, tick, value)
    SELECT item_id, position, 0, tick, value FROM midi_pitch_bends;
DROP TABLE midi_pitch_bends;
ALTER TABLE midi_pitch_bends_v59 RENAME TO midi_pitch_bends;
CREATE INDEX midi_pitch_bends_by_item_tick
    ON midi_pitch_bends(item_id, tick, channel);
"#;

const CREATE_MIDI_CONTROLLERS_59: &str = r#"
CREATE TABLE midi_controllers (
    item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    controller INTEGER NOT NULL CHECK (controller BETWEEN 0 AND 127),
    channel INTEGER NOT NULL DEFAULT 0 CHECK (channel BETWEEN 0 AND 15),
    tick INTEGER NOT NULL CHECK (tick >= 0),
    value INTEGER NOT NULL CHECK (value BETWEEN 0 AND 127),
    PRIMARY KEY (item_id, position),
    UNIQUE (item_id, channel, controller, tick)
);
CREATE INDEX midi_controllers_by_item_tick ON midi_controllers(item_id, tick, channel, controller);
"#;

const CREATE_MIDI_PITCH_BENDS_59: &str = r#"
CREATE TABLE midi_pitch_bends (
    item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    channel INTEGER NOT NULL DEFAULT 0 CHECK (channel BETWEEN 0 AND 15),
    tick INTEGER NOT NULL CHECK (tick >= 0),
    value INTEGER NOT NULL CHECK (value BETWEEN 0 AND 16383),
    PRIMARY KEY (item_id, position),
    UNIQUE (item_id, channel, tick)
);
CREATE INDEX midi_pitch_bends_by_item_tick ON midi_pitch_bends(item_id, tick, channel);
"#;

const MIGRATION_60_PROGRAM_CHANGES: &str = r#"
CREATE TABLE midi_program_changes (
    item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    channel INTEGER NOT NULL CHECK (channel BETWEEN 0 AND 15),
    tick INTEGER NOT NULL CHECK (tick >= 0),
    program INTEGER NOT NULL CHECK (program BETWEEN 0 AND 127),
    PRIMARY KEY (item_id, position),
    UNIQUE (item_id, channel, tick)
);
CREATE INDEX midi_program_changes_by_item_tick
    ON midi_program_changes(item_id, tick, channel);
"#;

const MIGRATION_61_MIDI_AFTERTOUCH: &str = r#"
CREATE TABLE IF NOT EXISTS midi_poly_aftertouches (
    item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    channel INTEGER NOT NULL CHECK (channel BETWEEN 0 AND 15),
    tick INTEGER NOT NULL CHECK (tick >= 0),
    note INTEGER NOT NULL CHECK (note BETWEEN 0 AND 127),
    value INTEGER NOT NULL CHECK (value BETWEEN 0 AND 127),
    PRIMARY KEY (item_id, position),
    UNIQUE (item_id, channel, note, tick)
);
CREATE INDEX IF NOT EXISTS midi_poly_aftertouches_by_item_tick
    ON midi_poly_aftertouches(item_id, tick, channel, note);
CREATE TABLE IF NOT EXISTS midi_channel_pressures (
    item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    channel INTEGER NOT NULL CHECK (channel BETWEEN 0 AND 15),
    tick INTEGER NOT NULL CHECK (tick >= 0),
    value INTEGER NOT NULL CHECK (value BETWEEN 0 AND 127),
    PRIMARY KEY (item_id, position),
    UNIQUE (item_id, channel, tick)
);
CREATE INDEX IF NOT EXISTS midi_channel_pressures_by_item_tick
    ON midi_channel_pressures(item_id, tick, channel);
"#;
const MIGRATION_62_NOTE_OFF_VELOCITY: &str = "ALTER TABLE midi_notes ADD COLUMN off_velocity INTEGER NOT NULL DEFAULT 0 CHECK (off_velocity BETWEEN 0 AND 127);";

const MIGRATION_63_MIDI_OPAQUE_EVENTS: &str = r#"
CREATE TABLE IF NOT EXISTS midi_opaque_events (
    item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    tick INTEGER NOT NULL CHECK (tick >= 0),
    source_order INTEGER NOT NULL CHECK (source_order BETWEEN 0 AND 4294967295),
    event_kind INTEGER NOT NULL CHECK (event_kind BETWEEN 0 AND 2),
    meta_type INTEGER CHECK (meta_type BETWEEN 0 AND 255),
    data BLOB NOT NULL,
    PRIMARY KEY (item_id, position),
    CHECK ((event_kind = 0 AND meta_type IS NOT NULL) OR
           (event_kind IN (1, 2) AND meta_type IS NULL))
);
CREATE INDEX IF NOT EXISTS midi_opaque_events_by_item_tick
    ON midi_opaque_events(item_id, tick, source_order);
"#;
const MIGRATION_64_MIDI_EVENT_ORDER: &str = r#"
CREATE TABLE IF NOT EXISTS midi_event_order (
    item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    source_order INTEGER NOT NULL CHECK (source_order BETWEEN 0 AND 4294967295),
    event_kind INTEGER NOT NULL CHECK (event_kind BETWEEN 0 AND 5),
    tick INTEGER NOT NULL CHECK (tick >= 0),
    channel INTEGER NOT NULL CHECK (channel BETWEEN 0 AND 15),
    data1 INTEGER NOT NULL CHECK (data1 BETWEEN 0 AND 16383),
    data2 INTEGER NOT NULL CHECK (data2 BETWEEN 0 AND 127),
    data3 INTEGER NOT NULL CHECK (data3 BETWEEN 0 AND 127),
    duration INTEGER NOT NULL CHECK (duration >= 0),
    PRIMARY KEY (item_id, position),
    UNIQUE (item_id, source_order)
);
CREATE INDEX IF NOT EXISTS midi_event_order_by_item_source_order
    ON midi_event_order(item_id, source_order);
"#;
const MIGRATION_65_MIDI_NOTE_OFF_ORDER: &str = r#"
ALTER TABLE midi_event_order
    ADD COLUMN note_off_order INTEGER
    CHECK (note_off_order IS NULL OR note_off_order BETWEEN 0 AND 4294967295);
"#;
const MIGRATION_66_AUDIO_TAKES: &str = r#"
CREATE TABLE IF NOT EXISTS audio_item_takes (
    item_id INTEGER NOT NULL REFERENCES audio_items(id) ON DELETE CASCADE,
    take_index INTEGER NOT NULL CHECK(take_index >= 0),
    media_ref TEXT NOT NULL CHECK(length(trim(media_ref)) > 0),
    name TEXT NOT NULL,
    source_offset_samples INTEGER NOT NULL,
    phase_inverted INTEGER NOT NULL CHECK(phase_inverted IN (0,1)),
    loop_source INTEGER NOT NULL CHECK(loop_source IN (0,1)),
    source_section_start_samples INTEGER CHECK(source_section_start_samples IS NULL OR source_section_start_samples >= 0),
    source_section_length_samples INTEGER CHECK(source_section_length_samples IS NULL OR source_section_length_samples > 0),
    take_volume REAL NOT NULL CHECK(take_volume >= 0.0 AND take_volume <= 16.0),
    take_pan REAL NOT NULL CHECK(take_pan >= -1.0 AND take_pan <= 1.0),
    take_playback_rate REAL NOT NULL CHECK(take_playback_rate > 0.0),
    PRIMARY KEY(item_id, take_index),
    CHECK((source_section_start_samples IS NULL) = (source_section_length_samples IS NULL))
);
"#;
const MIGRATION_67_MIDI_TAKES: &str = r#"
CREATE TABLE IF NOT EXISTS midi_item_takes (
    item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    take_index INTEGER NOT NULL CHECK(take_index >= 0),
    name TEXT NOT NULL,
    source_offset_ticks INTEGER NOT NULL CHECK(source_offset_ticks >= 0),
    loop_source INTEGER NOT NULL CHECK(loop_source IN (0, 1)),
    source_loop_length_ticks INTEGER NOT NULL CHECK(source_loop_length_ticks >= 0),
    PRIMARY KEY(item_id, take_index)
);
CREATE TABLE IF NOT EXISTS midi_item_take_state (
    item_id INTEGER PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,
    active_take_index INTEGER NOT NULL CHECK(active_take_index >= 0)
);

CREATE TABLE IF NOT EXISTS midi_take_events (
    item_id INTEGER NOT NULL,
    take_index INTEGER NOT NULL,
    event_kind INTEGER NOT NULL CHECK(event_kind BETWEEN 0 AND 7),
    position INTEGER NOT NULL CHECK(position >= 0),
    note_id INTEGER CHECK(note_id IS NULL OR note_id >= 0),
    target_kind INTEGER,
    tick INTEGER NOT NULL CHECK(tick >= 0),
    channel INTEGER NOT NULL DEFAULT 0 CHECK(channel BETWEEN 0 AND 15),
    data1 INTEGER NOT NULL DEFAULT 0 CHECK(data1 BETWEEN 0 AND 16383),
    data2 INTEGER NOT NULL DEFAULT 0 CHECK(data2 BETWEEN 0 AND 127),
    data3 INTEGER NOT NULL DEFAULT 0 CHECK(data3 BETWEEN 0 AND 127),
    duration INTEGER NOT NULL DEFAULT 0 CHECK(duration >= 0),
    value INTEGER NOT NULL DEFAULT 0 CHECK(value BETWEEN 0 AND 16383),
    payload BLOB,
    source_order INTEGER NOT NULL DEFAULT 0 CHECK(source_order BETWEEN 0 AND 4294967295),
    note_off_order INTEGER CHECK(note_off_order IS NULL OR note_off_order BETWEEN 0 AND 4294967295),
    PRIMARY KEY(item_id, take_index, event_kind, position),
    FOREIGN KEY(item_id, take_index)
        REFERENCES midi_item_takes(item_id, take_index) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS midi_take_events_by_tick
    ON midi_take_events(item_id, take_index, event_kind, tick, position);
"#;

/// SQLite persistence failures, including incompatible project files.
#[derive(Debug)]
pub enum StorageError {
    Sql(rusqlite::Error),
    Snapshot(SnapshotError),
    UnsupportedSchemaVersion { found: i64, current: u32 },
    ReadOnlySchemaVersion { found: i64, required: u32 },
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
            Self::ReadOnlySchemaVersion { found, required } => write!(
                formatter,
                "read-only project access requires schema version {required}, found {found}"
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

#[cfg(test)]
mod arrangement_view_state_tests {
    use super::{ProjectStore, StorageError};
    use crate::{
        ArrangementTimeRangeState, ArrangementViewState, CURRENT_SCHEMA_VERSION,
        FxAutomationLaneViewState, VolumeAutomationLaneViewState,
    };
    use aaadaw_core::{DawAction, Project, TrackFxPlugin};

    #[test]
    fn arrangement_view_state_round_trips_and_survives_project_only_saves() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "FX".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        project
            .apply(DawAction::SetTrackFxChain {
                track_id,
                plugins: vec![TrackFxPlugin::new("vendor.eq", "/plugins/eq.clap").unwrap()],
            })
            .unwrap();
        let view_state = ArrangementViewState {
            folder_compact: Vec::new(),
            volume_lanes: vec![VolumeAutomationLaneViewState {
                track_id: track_id.value(),
                visible: false,
            }],
            fx_lanes: vec![FxAutomationLaneViewState {
                track_id: track_id.value(),
                chain_index: 0,
                plugin_id: "vendor.eq".to_owned(),
                bundle_path: "/plugins/eq.clap".to_owned(),
                parameter_id: 7,
                name: "Mix".to_owned(),
                value_range: (-1.0, 1.0),
                stepped: false,
                height: 48.0,
            }],
            time_selection: Some(ArrangementTimeRangeState {
                start_seconds: 1.25,
                end_seconds: 2.5,
            }),
            loop_points: Some(ArrangementTimeRangeState {
                start_seconds: 3.5,
                end_seconds: 3.5,
            }),
            repeat_enabled: true,
            snap_enabled: false,
            snap_grid: crate::GridDivision::DottedEighth,
            midi_editor_snap_grid: crate::GridDivision::ThirtySecondTriplet,
            swing_grid_enabled: true,
            swing_amount_percent: -73,
        };

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("view-state.aaadaw");
        let mut store = ProjectStore::open(&path).unwrap();
        store
            .save_with_arrangement_view_state(&project, &view_state)
            .unwrap();
        assert_eq!(
            store.load_arrangement_view_state().unwrap(),
            Some(view_state.clone())
        );

        store.save(&project).unwrap();
        assert_eq!(
            store.load_arrangement_view_state().unwrap(),
            Some(view_state)
        );
        assert_eq!(store.load().unwrap().tracks()[0].id(), track_id);
        store.close().unwrap();
    }

    #[test]
    fn projects_without_saved_arrangement_view_state_use_compatible_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("old-project.aaadaw");
        let store = ProjectStore::open(&path).unwrap();
        assert_eq!(store.load_arrangement_view_state().unwrap(), None);
        store.close().unwrap();
    }

    #[test]
    fn schema_51_projects_default_repeat_off_and_persist_it_per_project() {
        use rusqlite::Connection;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("repeat-schema-51.aaadaw");
        let mut store = ProjectStore::open(&path).unwrap();
        let saved_state = ArrangementViewState {
            repeat_enabled: true,
            ..ArrangementViewState::default()
        };
        store
            .save_with_arrangement_view_state(&Project::new(), &saved_state)
            .unwrap();
        drop(store);

        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "ALTER TABLE arrangement_view_meta DROP COLUMN repeat_enabled; \
                 PRAGMA user_version = 51;",
            )
            .unwrap();
        drop(connection);

        let mut store = ProjectStore::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        assert_eq!(
            store.load_arrangement_view_state().unwrap(),
            Some(ArrangementViewState::default())
        );
        store
            .save_with_arrangement_view_state(&Project::new(), &saved_state)
            .unwrap();
        assert_eq!(
            store.load_arrangement_view_state().unwrap(),
            Some(saved_state)
        );
        store.close().unwrap();
    }

    #[test]
    fn schema_54_projects_gain_default_snap_and_swing_settings() {
        use rusqlite::Connection;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snap-grid-schema-54.aaadaw");
        let mut store = ProjectStore::open(&path).unwrap();
        let saved_state = ArrangementViewState {
            repeat_enabled: true,
            ..ArrangementViewState::default()
        };
        store
            .save_with_arrangement_view_state(&Project::new(), &saved_state)
            .unwrap();
        drop(store);

        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "ALTER TABLE arrangement_view_meta DROP COLUMN snap_enabled; \
                 ALTER TABLE arrangement_view_meta DROP COLUMN snap_grid; \
                 ALTER TABLE arrangement_view_meta DROP COLUMN midi_editor_snap_grid; \
                 ALTER TABLE arrangement_view_meta DROP COLUMN swing_grid_enabled; \
                 ALTER TABLE arrangement_view_meta DROP COLUMN swing_amount_percent; \
                 UPDATE arrangement_view_meta SET state_version = 1; \
                 PRAGMA user_version = 54;",
            )
            .unwrap();
        drop(connection);

        let mut store = ProjectStore::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        assert_eq!(
            store.load_arrangement_view_state().unwrap(),
            Some(saved_state.clone())
        );
        store
            .save_with_arrangement_view_state(&Project::new(), &saved_state)
            .unwrap();
        assert_eq!(
            store.load_arrangement_view_state().unwrap(),
            Some(saved_state)
        );
        store.close().unwrap();
    }

    #[test]
    fn schema_37_projects_gain_empty_time_ranges_and_invalid_ranges_roll_back() {
        use rusqlite::Connection;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("time-range-schema-37.aaadaw");
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Original".to_owned(),
            })
            .unwrap();
        let initial_state = ArrangementViewState {
            time_selection: Some(ArrangementTimeRangeState {
                start_seconds: 1.0,
                end_seconds: 2.0,
            }),
            loop_points: Some(ArrangementTimeRangeState {
                start_seconds: 3.0,
                end_seconds: 3.0,
            }),
            ..ArrangementViewState::default()
        };

        let mut store = ProjectStore::open(&path).unwrap();
        store
            .save_with_arrangement_view_state(&project, &initial_state)
            .unwrap();
        let invalid_state = ArrangementViewState {
            time_selection: Some(ArrangementTimeRangeState {
                start_seconds: 2.0,
                end_seconds: 1.0,
            }),
            ..ArrangementViewState::default()
        };
        assert!(matches!(
            store.save_with_arrangement_view_state(&Project::new(), &invalid_state),
            Err(StorageError::InvalidStoredData(_))
        ));
        assert_eq!(store.load().unwrap().tracks()[0].name(), "Original");
        assert_eq!(
            store.load_arrangement_view_state().unwrap(),
            Some(initial_state)
        );
        drop(store);

        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("DROP TABLE arrangement_time_ranges; PRAGMA user_version = 37;")
            .unwrap();
        drop(connection);

        let store = ProjectStore::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        assert_eq!(store.load().unwrap().tracks()[0].name(), "Original");
        assert_eq!(
            store.load_arrangement_view_state().unwrap(),
            Some(ArrangementViewState::default())
        );
    }

    #[test]
    fn invalid_view_state_does_not_partially_replace_the_saved_project() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Original".to_owned(),
            })
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("atomic-view-state.aaadaw");
        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&project).unwrap();

        let invalid_view_state = ArrangementViewState {
            folder_compact: Vec::new(),
            volume_lanes: Vec::new(),
            fx_lanes: vec![FxAutomationLaneViewState {
                track_id: 1,
                chain_index: 0,
                plugin_id: "vendor.eq".to_owned(),
                bundle_path: "/plugins/eq.clap".to_owned(),
                parameter_id: 7,
                name: "Mix".to_owned(),
                value_range: (1.0, 1.0),
                stepped: false,
                height: 48.0,
            }],
            time_selection: None,
            loop_points: None,
            repeat_enabled: false,
            ..ArrangementViewState::default()
        };
        assert!(matches!(
            store.save_with_arrangement_view_state(&Project::new(), &invalid_view_state),
            Err(StorageError::InvalidStoredData(_))
        ));
        assert_eq!(store.load().unwrap().tracks()[0].name(), "Original");
        store.close().unwrap();
    }
}

#[cfg(test)]
mod project_marker_storage_tests {
    use super::ProjectStore;
    use crate::CURRENT_SCHEMA_VERSION;
    use aaadaw_core::{DawAction, Project};
    use rusqlite::Connection;

    #[test]
    fn project_markers_round_trip_and_schema_34_migrates_without_markers() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("markers.aaadaw");
        let mut project = Project::new();
        project
            .apply(DawAction::InsertProjectMarker {
                position_ticks: 1_920,
                name: "Verse".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertProjectMarker {
                position_ticks: 960,
                name: "Intro".to_owned(),
            })
            .unwrap();
        let verse_id = project
            .markers()
            .iter()
            .find(|marker| marker.name() == "Verse")
            .unwrap()
            .id();
        project
            .apply(DawAction::SetProjectMarkerVisibility {
                marker_id: verse_id,
                visible: false,
            })
            .unwrap();
        project
            .apply(DawAction::SetProjectMarkersVisibleOnRuler { visible: false })
            .unwrap();
        let expected = project.markers().to_vec();

        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&project).unwrap();
        assert_eq!(store.load().unwrap().markers(), expected);
        assert!(!store.load().unwrap().show_project_markers());
        drop(store);

        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("DROP INDEX project_markers_by_position; DROP TABLE project_markers; PRAGMA user_version = 34;")
            .unwrap();
        drop(connection);

        let store = ProjectStore::open(&path).unwrap();
        assert_eq!(store.load().unwrap().markers().len(), 0);
        let schema_version: i64 = store
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(schema_version, i64::from(CURRENT_SCHEMA_VERSION));
    }

    #[test]
    fn schema_35_marker_rows_migrate_to_visible_numbers_without_losing_stable_ids() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("markers-schema-35.aaadaw");
        let mut project = Project::new();
        for (number, position_ticks, name) in [(5, 960, "Five"), (10, 1_920, "Ten")] {
            project
                .apply(DawAction::InsertNumberedProjectMarker {
                    number,
                    position_ticks,
                    name: name.to_owned(),
                })
                .unwrap();
        }

        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&project).unwrap();
        store
            .connection
            .execute_batch(
                "ALTER TABLE project_markers DROP COLUMN marker_number; PRAGMA user_version = 35;",
            )
            .unwrap();
        drop(store);

        let store = ProjectStore::open(&path).unwrap();
        let markers = store.load().unwrap().markers().to_vec();
        assert_eq!(markers[0].id().value(), 0);
        assert_eq!(markers[0].number(), 1);
        assert_eq!(markers[1].id().value(), 1);
        assert_eq!(markers[1].number(), 2);
        assert_eq!(markers[0].name(), "Five");
        assert_eq!(markers[1].name(), "Ten");
    }

    #[test]
    fn project_marker_round_trip_preserves_sparse_and_duplicate_numbers() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("markers-number-round-trip.aaadaw");
        let mut project = Project::new();
        for (number, position_ticks, name) in [
            (5, 960, "Five"),
            (10, 1_920, "Ten earlier"),
            (10, 2_880, "Ten later"),
        ] {
            project
                .apply(DawAction::InsertNumberedProjectMarker {
                    number,
                    position_ticks,
                    name: name.to_owned(),
                })
                .unwrap();
        }
        let expected = project.markers().to_vec();

        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&project).unwrap();
        assert_eq!(store.load().unwrap().markers(), expected);
    }

    #[test]
    fn project_regions_round_trip_and_schema_36_migrates_without_regions() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("regions.aaadaw");
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Drums".to_owned(),
            })
            .unwrap();
        let drums = project.tracks()[0].id();
        project
            .apply(DawAction::InsertProjectRegion {
                start_ticks: 1_920,
                end_ticks: 3_840,
                name: "Verse".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertProjectRegion {
                start_ticks: 0,
                end_ticks: 1_920,
                name: "Intro".to_owned(),
            })
            .unwrap();
        let hidden_region_id = project.regions()[0].id();
        project
            .apply(DawAction::SetProjectRegionRenderMatrix {
                region_id: hidden_region_id,
                render_master_mix: true,
                render_track_ids: vec![drums],
            })
            .unwrap();
        project
            .apply(DawAction::SetProjectRegionVisibility {
                region_id: hidden_region_id,
                visible: false,
            })
            .unwrap();
        project
            .apply(DawAction::SetProjectRegionsVisibleOnRuler { visible: false })
            .unwrap();
        let expected = project.regions().to_vec();

        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&project).unwrap();
        assert_eq!(store.load().unwrap().regions(), expected);
        assert!(!store.load().unwrap().show_project_regions());
        drop(store);

        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "DROP TABLE project_region_render_tracks; DROP INDEX project_regions_by_start; DROP TABLE project_regions; PRAGMA user_version = 36;",
            )
            .unwrap();
        drop(connection);

        let store = ProjectStore::open(&path).unwrap();
        assert!(store.load().unwrap().regions().is_empty());
        let schema_version: i64 = store
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(schema_version, i64::from(CURRENT_SCHEMA_VERSION));
    }

    #[test]
    fn schema_47_region_visibility_migration_defaults_to_visible() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("regions-schema-47.aaadaw");
        let mut project = Project::new();
        project
            .apply(DawAction::InsertProjectRegion {
                start_ticks: 0,
                end_ticks: 1_920,
                name: "Intro".to_owned(),
            })
            .unwrap();
        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&project).unwrap();
        drop(store);

        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "ALTER TABLE project_regions DROP COLUMN visible;
                 ALTER TABLE project_meta DROP COLUMN show_project_regions;
                 PRAGMA user_version = 47;",
            )
            .unwrap();
        drop(connection);

        let store = ProjectStore::open(&path).unwrap();
        let loaded = store.load().unwrap();
        assert!(loaded.show_project_regions());
        assert!(loaded.regions()[0].is_visible());
        assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    }
}

#[cfg(test)]
mod low_disk_save_tests {
    use super::ProjectStore;
    use aaadaw_core::{DawAction, Project, TrackFxPlugin};
    use rusqlite::{Error as SqliteError, ErrorCode};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(0);

    fn project_path() -> PathBuf {
        let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "aaadaw-storage-full-{}-{id}.aaadaw",
            std::process::id()
        ))
    }

    fn remove_database(path: &PathBuf) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    fn project_with_fx_state(state: Option<Vec<u8>>) -> Project {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Keys".to_owned(),
            })
            .expect("track creation should succeed");
        let track_id = project.tracks()[0].id();
        let plugin = TrackFxPlugin::new("org.example.test", "/plugins/test.clap")
            .expect("valid plugin reference")
            .with_state(state);
        project
            .apply(DawAction::SetTrackFxChain {
                track_id,
                plugins: vec![plugin],
            })
            .expect("FX assignment should succeed");
        project
    }

    fn assert_sqlite_full(error: &super::StorageError) {
        assert!(
            matches!(error, super::StorageError::Sql(SqliteError::SqliteFailure(code, _)) if code.code == ErrorCode::DiskFull),
            "expected SQLite SQLITE_FULL, got {error}"
        );
    }

    fn restrict_to_one_additional_page(store: &ProjectStore) -> i64 {
        let page_count: i64 = store
            .connection
            .pragma_query_value(None, "page_count", |row| row.get(0))
            .expect("page count should be readable");
        store
            .connection
            .pragma_update(None, "max_page_count", page_count + 1)
            .expect("database should accept a restrictive page limit");
        page_count
    }

    #[test]
    fn failed_save_preserves_last_snapshot_and_can_be_retried_after_space_is_freed() {
        let path = project_path();
        let original = project_with_fx_state(None);
        let replacement = project_with_fx_state(Some(vec![0xa5; 1024 * 1024]));
        let mut store = ProjectStore::open(&path).expect("project should open");
        store.save(&original).expect("initial snapshot should save");

        let page_count = restrict_to_one_additional_page(&store);

        let error = store
            .save(&replacement)
            .expect_err("the larger snapshot should exceed the page limit");
        assert_sqlite_full(&error);
        assert_eq!(
            store
                .load()
                .expect("store should remain queryable")
                .snapshot(),
            original.snapshot(),
            "failed save must leave the last committed snapshot intact"
        );

        store.close().expect("failed save should not prevent close");
        let mut store = ProjectStore::open(&path).expect("old snapshot should reopen");
        assert_eq!(
            store.load().expect("old snapshot should load").snapshot(),
            original.snapshot(),
            "rollback must survive close and reopen"
        );

        store
            .connection
            .pragma_update(None, "max_page_count", page_count + 1024)
            .expect("freeing space should permit growth");
        store
            .save(&replacement)
            .expect("retry should save after capacity is restored");
        store.close().expect("retried project should close");

        let store = ProjectStore::open(&path).expect("retried snapshot should reopen");
        assert_eq!(
            store
                .load()
                .expect("retried snapshot should load")
                .snapshot(),
            replacement.snapshot()
        );
        store.close().expect("reopened project should close");
        remove_database(&path);
    }

    #[test]
    fn failed_save_can_be_retried_on_the_same_open_store() {
        let path = project_path();
        let original = project_with_fx_state(None);
        let replacement = project_with_fx_state(Some(vec![0x5a; 1024 * 1024]));
        let mut store = ProjectStore::open(&path).expect("project should open");
        store.save(&original).expect("initial snapshot should save");
        let page_count = restrict_to_one_additional_page(&store);

        let error = store
            .save(&replacement)
            .expect_err("the larger snapshot should exceed the page limit");
        assert_sqlite_full(&error);
        assert_eq!(
            store
                .load()
                .expect("store should remain queryable")
                .snapshot(),
            original.snapshot()
        );

        store
            .connection
            .pragma_update(None, "max_page_count", page_count + 1024)
            .expect("freeing space should permit growth");
        store
            .save(&replacement)
            .expect("the failed transaction should allow a retry on this connection");
        assert_eq!(
            store
                .load()
                .expect("retried snapshot should load")
                .snapshot(),
            replacement.snapshot()
        );
        store.close().expect("retried project should close");
        remove_database(&path);
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
    /// Loads a project without running schema migrations or enabling writes.
    ///
    /// This is intended for external read-only consumers such as the MCP server.
    /// The project must already use the current schema; opening it cannot create,
    /// migrate, or modify project files.
    pub fn load_read_only(path: impl AsRef<Path>) -> Result<Project, StorageError> {
        let requested_path = path.as_ref().to_owned();
        let connection = Connection::open_with_flags(
            &requested_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection.busy_timeout(Duration::from_secs(5))?;

        let application_id: i64 =
            connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
        if application_id != APPLICATION_ID {
            return Err(StorageError::WrongApplicationId(application_id));
        }
        let schema_version: i64 =
            connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if schema_version != i64::from(CURRENT_SCHEMA_VERSION) {
            return Err(StorageError::ReadOnlySchemaVersion {
                found: schema_version,
                required: CURRENT_SCHEMA_VERSION,
            });
        }

        ProjectStore {
            connection,
            database_path: requested_path,
        }
        .load()
    }

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

    /// Copies every published embedded or externally linked audio asset from another project.
    ///
    /// Media references remain stable, so snapshots that already point at these assets need no
    /// rewriting. The copy commits all assets together and ignores unpublished imports.
    pub fn copy_audio_assets_from(&mut self, source: &ProjectStore) -> Result<(), StorageError> {
        if source.database_path == Path::new(":memory:") {
            return Err(StorageError::InvalidStoredData(
                "in-memory database cannot be copied",
            ));
        }
        if self.database_path == source.database_path {
            return Ok(());
        }

        let source_path = source.database_path.to_string_lossy().into_owned();
        self.connection
            .execute("ATTACH DATABASE ?1 AS source_assets", [&source_path])?;

        let copy_result = (|| {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch(
                "INSERT INTO main.audio_assets
                 SELECT * FROM source_assets.audio_assets WHERE import_state = 1;
                 INSERT INTO main.audio_asset_storage_chunks
                 SELECT chunks.* FROM source_assets.audio_asset_storage_chunks AS chunks
                 JOIN source_assets.audio_assets AS assets
                   ON assets.storage_key = chunks.storage_key
                 WHERE assets.import_state = 1;
                 INSERT INTO main.audio_asset_metadata
                 SELECT metadata.* FROM source_assets.audio_asset_metadata AS metadata
                 JOIN source_assets.audio_assets AS assets
                   ON assets.media_ref = metadata.media_ref
                 WHERE assets.import_state = 1;
                 INSERT INTO main.audio_asset_links
                 SELECT * FROM source_assets.audio_asset_links;
                 INSERT INTO main.audio_asset_chunks
                 SELECT chunks.* FROM source_assets.audio_asset_chunks AS chunks
                 JOIN source_assets.audio_assets AS assets
                   ON assets.media_ref = chunks.media_ref
                 WHERE assets.import_state = 1;",
            )?;
            transaction.commit()?;
            Ok::<_, StorageError>(())
        })();

        let detach_result = self
            .connection
            .execute_batch("DETACH DATABASE source_assets");
        copy_result?;
        detach_result?;
        Ok(())
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

    /// Removes a project audio asset only when no persisted audio item references it.
    ///
    /// This is used to roll back multi-file imports that fail before their placement action is
    /// committed. Assets are immutable; normal editing should use undoable project actions.
    pub fn remove_unreferenced_audio_asset(
        &mut self,
        media_ref: &str,
    ) -> Result<bool, StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let referenced: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM audio_items WHERE media_ref = ?1)",
            [media_ref],
            |row| row.get(0),
        )?;
        if referenced {
            return Ok(false);
        }
        let storage_key: Option<String> = transaction
            .query_row(
                "SELECT storage_key FROM audio_assets WHERE media_ref = ?1",
                [media_ref],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(storage_key) = storage_key {
            transaction.execute(
                "DELETE FROM audio_asset_storage_chunks WHERE storage_key = ?1",
                [storage_key],
            )?;
        }
        let removed =
            transaction.execute("DELETE FROM audio_assets WHERE media_ref = ?1", [media_ref])?;
        transaction.execute(
            "DELETE FROM audio_asset_links WHERE media_ref = ?1",
            [media_ref],
        )?;
        transaction.commit()?;
        Ok(removed > 0)
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

    /// Returns the SHA-256 fingerprint stored for a complete embedded asset.
    ///
    /// Older project files may not have a fingerprint. Linked files are not stored in
    /// `audio_assets` and return `None`.
    pub fn audio_asset_content_hash(
        &self,
        media_ref: &str,
    ) -> Result<Option<[u8; 32]>, StorageError> {
        let hash = self
            .connection
            .query_row(
                "SELECT content_hash FROM audio_assets \
                 WHERE media_ref = ?1 AND import_state = 1",
                [media_ref],
                |row| row.get::<_, Option<Vec<u8>>>(0),
            )
            .optional()?
            .flatten();
        Ok(hash.and_then(|bytes| bytes.try_into().ok()))
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

    /// Atomically saves project data and its Arrangement automation-lane view state.
    pub fn save_with_arrangement_view_state(
        &mut self,
        project: &Project,
        view_state: &ArrangementViewState,
    ) -> Result<(), StorageError> {
        let snapshot = project.snapshot();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        write_snapshot(&transaction, &snapshot)?;
        write_arrangement_view_state(&transaction, view_state)?;
        transaction.commit()?;
        Ok(())
    }

    /// Returns the saved Arrangement view state, or `None` for older projects.
    pub fn load_arrangement_view_state(
        &self,
    ) -> Result<Option<ArrangementViewState>, StorageError> {
        let state_version = self
            .connection
            .query_row(
                "SELECT state_version, repeat_enabled, snap_enabled, snap_grid, \
                 midi_editor_snap_grid, swing_grid_enabled, swing_amount_percent \
                 FROM arrangement_view_meta WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            state_version,
            repeat_enabled,
            snap_enabled,
            snap_grid,
            midi_editor_snap_grid,
            swing_grid_enabled,
            swing_amount_percent,
        )) = state_version
        else {
            return Ok(None);
        };
        if state_version != 1 {
            return Err(StorageError::InvalidStoredData(
                "unsupported Arrangement view state version",
            ));
        }

        let mut view_state = ArrangementViewState {
            repeat_enabled: match repeat_enabled {
                0 => false,
                1 => true,
                _ => return Err(StorageError::InvalidStoredData("invalid Repeat state")),
            },
            snap_enabled: match snap_enabled {
                0 => false,
                1 => true,
                _ => return Err(StorageError::InvalidStoredData("invalid Snap state")),
            },
            snap_grid: GridDivision::from_storage_id(snap_grid)
                .ok_or(StorageError::InvalidStoredData("invalid Arrangement grid"))?,
            midi_editor_snap_grid: GridDivision::from_storage_id(midi_editor_snap_grid)
                .ok_or(StorageError::InvalidStoredData("invalid MIDI Editor grid"))?,
            swing_grid_enabled: match swing_grid_enabled {
                0 => false,
                1 => true,
                _ => return Err(StorageError::InvalidStoredData("invalid Swing state")),
            },
            swing_amount_percent: i16::try_from(swing_amount_percent)
                .ok()
                .filter(|value| (-100..=100).contains(value))
                .ok_or(StorageError::InvalidStoredData("invalid Swing amount"))?,
            ..ArrangementViewState::default()
        };
        let mut volume_statement = self
            .connection
            .prepare("SELECT track_id, visible FROM arrangement_volume_lanes ORDER BY track_id")?;
        let mut volume_rows = volume_statement.query([])?;
        while let Some(row) = volume_rows.next()? {
            let track_id = from_sql_u64(row.get(0)?)?;
            let visible = match row.get::<_, i64>(1)? {
                0 => false,
                1 => true,
                _ => return Err(StorageError::InvalidStoredData("invalid lane visibility")),
            };
            view_state
                .volume_lanes
                .push(VolumeAutomationLaneViewState { track_id, visible });
        }

        let mut fx_statement = self.connection.prepare(
            "SELECT track_id, chain_index, plugin_id, bundle_path, parameter_id, name, \
             value_min, value_max, stepped, height FROM arrangement_fx_lanes \
             ORDER BY track_id, chain_index, parameter_id",
        )?;
        let mut fx_rows = fx_statement.query([])?;
        while let Some(row) = fx_rows.next()? {
            let value_range = (row.get::<_, f64>(6)?, row.get::<_, f64>(7)?);
            let height = row.get::<_, f64>(9)? as f32;
            if !value_range.0.is_finite()
                || !value_range.1.is_finite()
                || value_range.0 >= value_range.1
                || !height.is_finite()
                || height <= 0.0
            {
                return Err(StorageError::InvalidStoredData(
                    "invalid FX automation lane view state",
                ));
            }
            let stepped = match row.get::<_, i64>(8)? {
                0 => false,
                1 => true,
                _ => return Err(StorageError::InvalidStoredData("invalid stepped flag")),
            };
            view_state.fx_lanes.push(FxAutomationLaneViewState {
                track_id: from_sql_u64(row.get(0)?)?,
                chain_index: usize::try_from(row.get::<_, i64>(1)?)
                    .map_err(|_| StorageError::InvalidStoredData("negative chain index"))?,
                plugin_id: row.get(2)?,
                bundle_path: row.get(3)?,
                parameter_id: from_sql_u32(row.get(4)?)?,
                name: row.get(5)?,
                value_range,
                stepped,
                height,
            });
        }
        let mut statement = self
            .connection
            .prepare("SELECT track_id, mode FROM arrangement_folder_compact ORDER BY track_id")?;
        let rows =
            statement.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?;
        for row in rows {
            let (id, mode) = row?;
            if !(0..=2).contains(&mode) {
                return Err(StorageError::InvalidStoredData(
                    "invalid folder compact mode",
                ));
            }
            view_state
                .folder_compact
                .push(crate::FolderCompactViewState {
                    track_id: from_sql_u64(id)?,
                    mode: mode as u8,
                });
        }
        if let Some((selection_columns, loop_columns)) = self
            .connection
            .query_row(
                "SELECT time_selection_start_seconds, time_selection_end_seconds, \
                 loop_start_seconds, loop_end_seconds FROM arrangement_time_ranges WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        (row.get::<_, Option<f64>>(0)?, row.get::<_, Option<f64>>(1)?),
                        (row.get::<_, Option<f64>>(2)?, row.get::<_, Option<f64>>(3)?),
                    ))
                },
            )
            .optional()?
        {
            view_state.time_selection = time_range_from_columns(selection_columns, false)?;
            view_state.loop_points = time_range_from_columns(loop_columns, true)?;
        }
        Ok(Some(view_state))
    }

    /// Loads a project. A newly created, empty database yields a default project.
    pub fn load(&self) -> Result<Project, StorageError> {
        let (title, author, text, show_on_load, word_wrap) = self.connection.query_row(
            "SELECT title, author, notes, show_on_load, word_wrap FROM project_notes WHERE singleton = 1", [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(3)?, row.get::<_, i64>(4)?)))?;
        if !matches!(show_on_load, 0 | 1) || !matches!(word_wrap, 0 | 1) {
            return Err(StorageError::InvalidStoredData(
                "invalid project notes display preferences",
            ));
        }
        let notes = aaadaw_core::ProjectNotes {
            title,
            author,
            text,
            show_on_load: show_on_load == 1,
            word_wrap: word_wrap == 1,
        };

        let frame_rate = self.connection.query_row(
            "SELECT frame_rate FROM project_timecode WHERE singleton = 1",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        let frame_rate = u32::try_from(frame_rate)
            .ok()
            .and_then(aaadaw_core::FrameRate::from_storage_code)
            .ok_or(StorageError::InvalidStoredData("unsupported frame rate"))?;
        let master_mix = self
            .connection
            .query_row(
                "SELECT volume_db, pan FROM project_master_mix WHERE singleton = 1",
                [],
                |row| Ok((row.get::<_, f32>(0)?, row.get::<_, f32>(1)?)),
            )
            .map_err(StorageError::from)
            .and_then(|(volume, pan)| {
                aaadaw_core::MasterMix::new(volume, pan)
                    .map_err(|_| StorageError::InvalidStoredData("invalid Master mix"))
            })?;
        let metadata = self
            .connection
            .query_row(
                "SELECT sample_rate, ppq, initial_tempo_bpm, pan_mode, start_measure, bpm_basis, base_ruler_label_spacing_on_measure, project_start_time_offset_seconds, project_start_time_fractional_frame_offset_allowed, force_tempo_changes_to_whole_samples, show_project_markers, show_project_regions FROM project_meta WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, f64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, f64>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, i64>(9)?,
                        row.get::<_, i64>(10)?,
                        row.get::<_, i64>(11)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            sample_rate,
            ppq,
            initial_tempo_bpm,
            pan_mode,
            start_measure,
            bpm_basis,
            base_ruler_label_spacing_on_measure,
            project_start_time_offset_seconds,
            project_start_time_fractional_frame_offset_allowed,
            force_tempo_changes_to_whole_samples,
            show_project_markers,
            show_project_regions,
        )) = metadata
        else {
            let rows: i64 = self.connection.query_row(
                "SELECT (SELECT COUNT(*) FROM tracks) + (SELECT COUNT(*) FROM items) + \
            (SELECT COUNT(*) FROM midi_notes) + (SELECT COUNT(*) FROM audio_items) + \
             (SELECT COUNT(*) FROM midi_controllers) + \
             (SELECT COUNT(*) FROM midi_pitch_bends) + \
                 (SELECT COUNT(*) FROM tempo_points) + (SELECT COUNT(*) FROM meter_points)",
                [],
                |row| row.get(0),
            )?;
            return if rows == 0
                && master_mix == aaadaw_core::MasterMix::default()
                && frame_rate == aaadaw_core::FrameRate::default()
                && notes == aaadaw_core::ProjectNotes::default()
            {
                Ok(Project::new())
            } else {
                Err(StorageError::InvalidStoredData(
                    "project rows exist without project metadata",
                ))
            };
        };
        let start_measure = i32::try_from(start_measure).map_err(|_| {
            StorageError::InvalidStoredData("project start measure is out of range")
        })?;
        let bpm_basis = u32::try_from(bpm_basis)
            .ok()
            .and_then(BpmBasis::from_storage_code)
            .ok_or(StorageError::InvalidStoredData("unsupported BPM basis"))?;
        let base_ruler_label_spacing_on_measure = match base_ruler_label_spacing_on_measure {
            0 => false,
            1 => true,
            _ => {
                return Err(StorageError::InvalidStoredData(
                    "unsupported ruler label spacing mode",
                ));
            }
        };
        let project_start_time_fractional_frame_offset_allowed =
            match project_start_time_fractional_frame_offset_allowed {
                0 => false,
                1 => true,
                _ => {
                    return Err(StorageError::InvalidStoredData(
                        "unsupported fractional frame offset mode",
                    ));
                }
            };
        let force_tempo_changes_to_whole_samples = match force_tempo_changes_to_whole_samples {
            0 => false,
            1 => true,
            _ => {
                return Err(StorageError::InvalidStoredData(
                    "unsupported whole-sample tempo mode",
                ));
            }
        };
        let show_project_markers = match show_project_markers {
            0 => false,
            1 => true,
            _ => {
                return Err(StorageError::InvalidStoredData(
                    "unsupported project marker lane visibility",
                ));
            }
        };
        let show_project_regions = match show_project_regions {
            0 => false,
            1 => true,
            _ => {
                return Err(StorageError::InvalidStoredData(
                    "unsupported project region lane visibility",
                ));
            }
        };

        let settings = ProjectSettings::new(
            from_sql_u32(sample_rate)?,
            from_sql_u32(ppq)?,
            initial_tempo_bpm,
        )
        .map_err(|error| StorageError::Snapshot(SnapshotError::InvalidTimebase(error)))?
        .with_frame_rate(frame_rate)
        .with_pan_mode(match pan_mode {
            0 => PanMode::LegacyMonoStereo,
            1 => PanMode::ZeroDbBalance,
            _ => return Err(StorageError::InvalidStoredData("unsupported pan mode")),
        })
        .with_start_measure(start_measure)
        .with_bpm_basis(bpm_basis)
        .map_err(|error| StorageError::Snapshot(SnapshotError::InvalidTimebase(error)))?
        .with_base_ruler_label_spacing_on_measure(base_ruler_label_spacing_on_measure)
        .with_project_start_time_offset_seconds(project_start_time_offset_seconds)
        .map_err(|error| StorageError::Snapshot(SnapshotError::InvalidTimebase(error)))?
        .with_project_start_time_fractional_frame_offset_allowed(
            project_start_time_fractional_frame_offset_allowed,
        )
        .with_force_tempo_changes_to_whole_samples(force_tempo_changes_to_whole_samples);

        let tracks = read_tracks(&self.connection)?;
        let audio_items = read_audio_items(&self.connection)?;
        let (midi_items, orphan_events) = read_midi_items(&self.connection)?;
        if orphan_events {
            return Err(StorageError::InvalidStoredData(
                "MIDI events reference a missing item",
            ));
        }
        let tempo_points = read_tempo_points(&self.connection)?;
        let meter_points = read_meter_points(&self.connection)?;
        let markers = read_project_markers(&self.connection)?;
        let regions = read_project_regions(&self.connection)?;

        Project::from_snapshot(ProjectSnapshot {
            notes,
            settings,
            master_mix,
            tracks,
            audio_items,
            midi_items,
            markers,
            show_project_markers,
            show_project_regions,
            regions,
            tempo_points,
            meter_points,
        })
        .map_err(StorageError::Snapshot)
    }

    /// Returns whether a project snapshot has been written to this database.
    pub fn has_saved_snapshot(&self) -> Result<bool, StorageError> {
        self.connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM project_meta WHERE singleton = 1)",
                [],
                |row| row.get(0),
            )
            .map_err(StorageError::from)
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

fn ensure_audio_item_loop_columns(connection: &Connection) -> Result<(), StorageError> {
    if !table_has_column(connection, "audio_items", "loop_source")? {
        connection.execute_batch(
            "ALTER TABLE audio_items ADD COLUMN loop_source INTEGER NOT NULL DEFAULT 0 CHECK(loop_source IN (0, 1));",
        )?;
    }
    if !table_has_column(connection, "audio_items", "source_section_length_samples")? {
        connection.execute_batch(
            "ALTER TABLE audio_items ADD COLUMN source_section_length_samples INTEGER CHECK(source_section_length_samples IS NULL OR source_section_length_samples > 0);",
        )?;
    }
    Ok(())
}

fn ensure_audio_item_loop_section_start_column(
    connection: &Connection,
) -> Result<(), StorageError> {
    if !table_has_column(connection, "audio_items", "source_section_start_samples")? {
        connection.execute_batch(
            "ALTER TABLE audio_items ADD COLUMN source_section_start_samples INTEGER CHECK(source_section_start_samples IS NULL OR source_section_start_samples >= 0);",
        )?;
    }
    Ok(())
}

fn ensure_audio_item_take_schema(connection: &Connection) -> Result<(), StorageError> {
    if !table_has_column(connection, "audio_items", "active_take_index")? {
        connection.execute_batch(
            "ALTER TABLE audio_items ADD COLUMN active_take_index INTEGER NOT NULL DEFAULT 0 CHECK(active_take_index >= 0);",
        )?;
    }
    connection.execute_batch(MIGRATION_66_AUDIO_TAKES)?;
    Ok(())
}

fn ensure_audio_item_mute_column(connection: &Connection) -> Result<(), StorageError> {
    if !table_has_column(connection, "audio_items", "muted")? {
        connection.execute_batch(
            "ALTER TABLE audio_items ADD COLUMN muted INTEGER NOT NULL DEFAULT 0 CHECK(muted IN (0, 1));",
        )?;
    }
    Ok(())
}

fn ensure_audio_item_phase_column(connection: &Connection) -> Result<(), StorageError> {
    if !table_has_column(connection, "audio_items", "phase_inverted")? {
        connection.execute_batch(
            "ALTER TABLE audio_items ADD COLUMN phase_inverted INTEGER NOT NULL DEFAULT 0 CHECK(phase_inverted IN (0, 1));",
        )?;
    }
    Ok(())
}

fn ensure_audio_item_signed_source_offset_column(
    connection: &Connection,
) -> Result<(), StorageError> {
    if !table_has_column(connection, "audio_items", "signed_source_offset_samples")? {
        connection.execute_batch(
            "ALTER TABLE audio_items ADD COLUMN signed_source_offset_samples INTEGER NOT NULL DEFAULT 0;\
             UPDATE audio_items SET signed_source_offset_samples = source_offset_samples;",
        )?;
    }
    Ok(())
}

fn ensure_midi_item_mute_column(connection: &Connection) -> Result<(), StorageError> {
    if !table_has_column(connection, "items", "muted")? {
        connection.execute_batch(
            "ALTER TABLE items ADD COLUMN muted INTEGER NOT NULL DEFAULT 0 CHECK(muted IN (0, 1));",
        )?;
    }
    Ok(())
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, StorageError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [table],
        |row| row.get(0),
    )?)
}

fn table_has_column(
    connection: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, StorageError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
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
            6 => transaction.execute_batch(MIGRATION_6)?,
            7 => transaction.execute_batch(MIGRATION_7)?,
            8 => transaction.execute_batch(MIGRATION_8)?,
            9 => transaction.execute_batch(MIGRATION_9)?,
            10 => transaction.execute_batch(MIGRATION_10)?,
            11 => transaction.execute_batch(MIGRATION_11)?,
            12 => transaction.execute_batch(MIGRATION_12)?,
            13 => transaction.execute_batch(MIGRATION_13)?,
            14 => transaction.execute_batch(MIGRATION_14)?,
            15 => transaction.execute_batch(MIGRATION_15)?,
            16 => transaction.execute_batch(MIGRATION_16)?,
            17 => transaction.execute_batch(MIGRATION_17)?,
            18 => transaction.execute_batch(MIGRATION_18)?,
            19 => transaction.execute_batch(MIGRATION_19)?,
            20 => transaction.execute_batch(MIGRATION_20)?,
            21 => transaction.execute_batch(MIGRATION_21)?,
            22 => transaction.execute_batch(MIGRATION_22)?,
            23 => transaction.execute_batch(MIGRATION_23)?,
            24 => transaction.execute_batch(MIGRATION_24)?,
            25 => transaction.execute_batch(MIGRATION_25)?,
            26 => transaction.execute_batch(MIGRATION_26)?,
            27 => transaction.execute_batch(MIGRATION_27)?,
            28 => transaction.execute_batch(MIGRATION_28)?,
            29 => {
                if !table_has_column(&transaction, "project_meta", "start_measure")? {
                    transaction.execute_batch(MIGRATION_29)?;
                }
            }
            30 => {
                if !table_has_column(&transaction, "project_meta", "bpm_basis")? {
                    transaction.execute_batch(MIGRATION_30)?;
                }
            }
            31 => {
                if !table_has_column(
                    &transaction,
                    "project_meta",
                    "base_ruler_label_spacing_on_measure",
                )? {
                    transaction.execute_batch(MIGRATION_31)?;
                }
            }
            32 => {
                if !table_has_column(
                    &transaction,
                    "project_meta",
                    "project_start_time_offset_seconds",
                )? {
                    transaction.execute_batch(MIGRATION_32)?;
                }
            }
            33 => {
                if !table_has_column(
                    &transaction,
                    "project_meta",
                    "project_start_time_fractional_frame_offset_allowed",
                )? {
                    transaction.execute_batch(MIGRATION_33)?;
                }
            }
            34 => {
                if !table_has_column(
                    &transaction,
                    "project_meta",
                    "force_tempo_changes_to_whole_samples",
                )? {
                    transaction.execute_batch(MIGRATION_34)?;
                }
            }
            35 => transaction.execute_batch(MIGRATION_35)?,
            36 => {
                if !table_has_column(&transaction, "project_markers", "marker_number")? {
                    transaction.execute_batch(MIGRATION_36)?;
                }
            }
            37 => transaction.execute_batch(MIGRATION_37)?,
            38 => transaction.execute_batch(MIGRATION_38)?,
            39 => {
                transaction.execute_batch(AUDIO_ITEMS_TABLE)?;
                if !table_has_column(&transaction, "audio_items", "volume")? {
                    transaction.execute_batch(MIGRATION_39)?;
                }
            }
            40 => {
                transaction.execute_batch(AUDIO_ITEMS_TABLE)?;
                if !table_has_column(&transaction, "audio_items", "take_volume")? {
                    transaction.execute_batch(MIGRATION_40_TAKE_VOLUME)?;
                }
                if !table_has_column(&transaction, "audio_items", "take_pan")? {
                    transaction.execute_batch(MIGRATION_40_TAKE_PAN)?;
                }
            }
            41 => transaction.execute_batch(MIGRATION_41)?,
            42 => {
                if table_exists(&transaction, "audio_items")? {
                    ensure_audio_item_loop_columns(&transaction)?;
                }
            }
            43 => {
                if table_exists(&transaction, "audio_items")? {
                    ensure_audio_item_loop_columns(&transaction)?;
                    ensure_audio_item_loop_section_start_column(&transaction)?;
                    transaction.execute_batch(
                        "UPDATE audio_items SET source_section_start_samples = source_offset_samples \
                         WHERE loop_source = 1 AND source_section_length_samples IS NOT NULL \
                         AND source_section_start_samples IS NULL;",
                    )?;
                }
            }
            44 => {
                if table_exists(&transaction, "audio_items")? {
                    ensure_audio_item_mute_column(&transaction)?;
                }
            }
            45 => {
                if table_exists(&transaction, "items")? {
                    ensure_midi_item_mute_column(&transaction)?;
                }
            }
            46 => {
                if table_exists(&transaction, "audio_items")? {
                    ensure_audio_item_phase_column(&transaction)?;
                }
            }
            47 => {
                if !table_has_column(&transaction, "project_meta", "show_project_markers")? {
                    transaction.execute_batch(
                        "ALTER TABLE project_meta ADD COLUMN show_project_markers INTEGER NOT NULL DEFAULT 1 CHECK(show_project_markers IN (0,1));",
                    )?;
                }
                if !table_has_column(&transaction, "project_markers", "visible")? {
                    transaction.execute_batch(
                        "ALTER TABLE project_markers ADD COLUMN visible INTEGER NOT NULL DEFAULT 1 CHECK(visible IN (0,1));",
                    )?;
                }
            }
            48 => {
                if !table_has_column(&transaction, "project_meta", "show_project_regions")? {
                    transaction.execute_batch(
                        "ALTER TABLE project_meta ADD COLUMN show_project_regions INTEGER NOT NULL DEFAULT 1 CHECK(show_project_regions IN (0,1));",
                    )?;
                }
                if !table_has_column(&transaction, "project_regions", "visible")? {
                    transaction.execute_batch(
                        "ALTER TABLE project_regions ADD COLUMN visible INTEGER NOT NULL DEFAULT 1 CHECK(visible IN (0,1));",
                    )?;
                }
            }
            49 => {
                if !table_has_column(&transaction, "audio_items", "signed_source_offset_samples")? {
                    transaction.execute_batch(MIGRATION_49)?;
                }
            }
            50 => transaction.execute_batch(MIGRATION_50)?,
            51 => {
                if !table_has_column(&transaction, "items", "loop_source")? {
                    transaction.execute_batch(
                        "ALTER TABLE items ADD COLUMN loop_source INTEGER NOT NULL DEFAULT 1 CHECK(loop_source IN (0, 1));",
                    )?;
                }
                if !table_has_column(&transaction, "items", "source_loop_length_ticks")? {
                    transaction.execute_batch(MIGRATION_51)?;
                }
            }
            52 => {
                if table_exists(&transaction, "arrangement_view_meta")?
                    && !table_has_column(&transaction, "arrangement_view_meta", "repeat_enabled")?
                {
                    transaction.execute_batch(MIGRATION_52)?;
                }
            }
            53 => {
                if !table_has_column(&transaction, "project_regions", "render_master_mix")? {
                    transaction.execute_batch(
                        "ALTER TABLE project_regions ADD COLUMN render_master_mix INTEGER NOT NULL DEFAULT 0 CHECK(render_master_mix IN (0,1));",
                    )?;
                }
                transaction.execute_batch(MIGRATION_53)?;
            }
            54 => {
                if table_exists(&transaction, "audio_items")?
                    && !table_has_column(&transaction, "audio_items", "name")?
                {
                    transaction.execute_batch(MIGRATION_54)?;
                }
            }
            55 => {
                for (column, migration) in [
                    (
                        "snap_enabled",
                        "ALTER TABLE arrangement_view_meta ADD COLUMN snap_enabled INTEGER NOT NULL DEFAULT 1 CHECK(snap_enabled IN (0, 1));",
                    ),
                    (
                        "snap_grid",
                        "ALTER TABLE arrangement_view_meta ADD COLUMN snap_grid INTEGER NOT NULL DEFAULT 2 CHECK(snap_grid BETWEEN 0 AND 14);",
                    ),
                    (
                        "midi_editor_snap_grid",
                        "ALTER TABLE arrangement_view_meta ADD COLUMN midi_editor_snap_grid INTEGER NOT NULL DEFAULT 5 CHECK(midi_editor_snap_grid BETWEEN 0 AND 14);",
                    ),
                    (
                        "swing_grid_enabled",
                        "ALTER TABLE arrangement_view_meta ADD COLUMN swing_grid_enabled INTEGER NOT NULL DEFAULT 0 CHECK(swing_grid_enabled IN (0, 1));",
                    ),
                    (
                        "swing_amount_percent",
                        "ALTER TABLE arrangement_view_meta ADD COLUMN swing_amount_percent INTEGER NOT NULL DEFAULT 50 CHECK(swing_amount_percent BETWEEN -100 AND 100);",
                    ),
                ] {
                    if !table_has_column(&transaction, "arrangement_view_meta", column)? {
                        transaction.execute_batch(migration)?;
                    }
                }
            }
            56 => {
                if table_exists(&transaction, "items")?
                    && !table_has_column(&transaction, "items", "start_tick_subtick")?
                {
                    transaction.execute_batch(MIGRATION_56)?;
                }
            }
            57 => {
                if table_exists(&transaction, "audio_items")?
                    && !table_has_column(&transaction, "audio_items", "take_playback_rate")?
                {
                    transaction.execute_batch(
                        "ALTER TABLE audio_items ADD COLUMN take_playback_rate REAL NOT NULL DEFAULT 1.0 CHECK (take_playback_rate > 0.0);",
                    )?;
                }
            }
            58 => {
                if table_exists(&transaction, "track_sends")?
                    && !table_has_column(&transaction, "track_sends", "midi_enabled")?
                {
                    transaction.execute_batch(
                        "ALTER TABLE track_sends ADD COLUMN midi_enabled INTEGER NOT NULL DEFAULT 1 CHECK(midi_enabled IN (0, 1));",
                    )?;
                }
            }
            59 => {
                if !table_has_column(&transaction, "midi_notes", "channel")? {
                    transaction.execute_batch(
                        "ALTER TABLE midi_notes ADD COLUMN channel INTEGER NOT NULL DEFAULT 0 CHECK(channel BETWEEN 0 AND 15);",
                    )?;
                }
                if !table_exists(&transaction, "midi_controllers")? {
                    transaction.execute_batch(CREATE_MIDI_CONTROLLERS_59)?;
                } else if !table_has_column(&transaction, "midi_controllers", "channel")? {
                    transaction.execute_batch(MIGRATION_59_CONTROLLERS)?;
                }
                if !table_exists(&transaction, "midi_pitch_bends")? {
                    transaction.execute_batch(CREATE_MIDI_PITCH_BENDS_59)?;
                } else if !table_has_column(&transaction, "midi_pitch_bends", "channel")? {
                    transaction.execute_batch(MIGRATION_59_PITCH_BENDS)?;
                }
            }
            60 => {
                if !table_exists(&transaction, "midi_program_changes")? {
                    transaction.execute_batch(MIGRATION_60_PROGRAM_CHANGES)?;
                }
            }
            61 => transaction.execute_batch(MIGRATION_61_MIDI_AFTERTOUCH)?,
            62 => {
                if !table_has_column(&transaction, "midi_notes", "off_velocity")? {
                    transaction.execute_batch(MIGRATION_62_NOTE_OFF_VELOCITY)?;
                }
            }
            63 => transaction.execute_batch(MIGRATION_63_MIDI_OPAQUE_EVENTS)?,
            64 => transaction.execute_batch(MIGRATION_64_MIDI_EVENT_ORDER)?,
            65 => {
                if !table_has_column(&transaction, "midi_event_order", "note_off_order")? {
                    transaction.execute_batch(MIGRATION_65_MIDI_NOTE_OFF_ORDER)?;
                }
            }
            66 => {
                if table_exists(&transaction, "audio_items")?
                    && !table_has_column(&transaction, "audio_items", "active_take_index")?
                {
                    transaction.execute_batch(
                        "ALTER TABLE audio_items ADD COLUMN active_take_index INTEGER NOT NULL DEFAULT 0 CHECK(active_take_index >= 0);",
                    )?;
                }
                transaction.execute_batch(MIGRATION_66_AUDIO_TAKES)?;
            }
            67 => transaction.execute_batch(MIGRATION_67_MIDI_TAKES)?,
            68 => {
                if !table_has_column(&transaction, "items", "active_take_name_is_empty")? {
                    transaction.execute_batch(MIGRATION_68_MIDI_ACTIVE_EMPTY_TAKE)?;
                }
            }
            69 => {
                if !table_has_column(&transaction, "items", "source_offset_negative")? {
                    transaction.execute_batch(
                        "ALTER TABLE items ADD COLUMN source_offset_negative INTEGER NOT NULL DEFAULT 0 CHECK(source_offset_negative IN (0, 1));",
                    )?;
                }
                if table_exists(&transaction, "midi_item_takes")?
                    && !table_has_column(&transaction, "midi_item_takes", "source_offset_negative")?
                {
                    transaction.execute_batch(
                        "ALTER TABLE midi_item_takes ADD COLUMN source_offset_negative INTEGER NOT NULL DEFAULT 0 CHECK(source_offset_negative IN (0, 1));",
                    )?;
                }
            }
            missing => return Err(StorageError::MissingMigration(missing - 1)),
        }
        transaction.pragma_update(None, "user_version", next_version)?;
        transaction.commit()?;
        version = next_version;
    }
    connection.execute_batch(AUDIO_ITEMS_TABLE)?;
    ensure_audio_item_loop_columns(connection)?;
    ensure_audio_item_loop_section_start_column(connection)?;
    ensure_audio_item_take_schema(connection)?;
    ensure_audio_item_mute_column(connection)?;
    ensure_audio_item_phase_column(connection)?;
    ensure_audio_item_signed_source_offset_column(connection)?;
    ensure_midi_item_mute_column(connection)?;
    connection.execute_batch(AUDIO_ASSETS_TABLE)?;
    ensure_audio_asset_metadata_columns(connection)?;
    Ok(())
}

fn write_snapshot(
    transaction: &Transaction<'_>,
    snapshot: &ProjectSnapshot,
) -> Result<(), StorageError> {
    transaction.execute("DELETE FROM project_region_render_tracks", [])?;
    transaction.execute("DELETE FROM track_sends", [])?;
    transaction.execute("DELETE FROM track_razor_edit_areas", [])?;
    transaction.execute("DELETE FROM midi_notes", [])?;
    transaction.execute("DELETE FROM midi_item_take_state", [])?;
    transaction.execute("DELETE FROM midi_item_takes", [])?;
    transaction.execute("DELETE FROM midi_controllers", [])?;
    transaction.execute("DELETE FROM midi_pitch_bends", [])?;
    transaction.execute("DELETE FROM midi_program_changes", [])?;
    transaction.execute("DELETE FROM midi_poly_aftertouches", [])?;
    transaction.execute("DELETE FROM midi_channel_pressures", [])?;
    transaction.execute("DELETE FROM midi_opaque_events", [])?;
    transaction.execute("DELETE FROM midi_event_order", [])?;
    transaction.execute("DELETE FROM track_volume_automation", [])?;
    transaction.execute("DELETE FROM items", [])?;
    transaction.execute("DELETE FROM audio_item_fade_curves", [])?;
    transaction.execute("DELETE FROM audio_item_fades", [])?;
    transaction.execute("DELETE FROM audio_item_takes", [])?;
    transaction.execute("DELETE FROM audio_items", [])?;
    transaction.execute("DELETE FROM tracks", [])?;
    transaction.execute("DELETE FROM tempo_points", [])?;
    transaction.execute("DELETE FROM meter_points", [])?;
    transaction.execute("DELETE FROM project_markers", [])?;
    transaction.execute("DELETE FROM project_regions", [])?;
    transaction.execute("DELETE FROM project_meta", [])?;
    transaction.execute("DELETE FROM project_master_mix", [])?;
    transaction.execute(
        "INSERT INTO project_master_mix(singleton, volume_db, pan) VALUES(1, ?1, ?2)",
        params![snapshot.master_mix.volume_db(), snapshot.master_mix.pan()],
    )?;

    transaction.execute("DELETE FROM project_notes", [])?;
    transaction.execute("INSERT INTO project_notes(singleton, title, author, notes, show_on_load, word_wrap) VALUES(1, ?1, ?2, ?3, ?4, ?5)", params![snapshot.notes.title, snapshot.notes.author, snapshot.notes.text, snapshot.notes.show_on_load, snapshot.notes.word_wrap])?;
    let settings = snapshot.settings;
    transaction.execute("DELETE FROM project_timecode", [])?;
    transaction.execute(
        "INSERT INTO project_timecode(singleton, frame_rate) VALUES(1, ?1)",
        params![i64::from(settings.frame_rate().storage_code())],
    )?;
    transaction.execute(
        "INSERT INTO project_meta(singleton, sample_rate, ppq, initial_tempo_bpm, pan_mode, start_measure, bpm_basis, base_ruler_label_spacing_on_measure, project_start_time_offset_seconds, project_start_time_fractional_frame_offset_allowed, force_tempo_changes_to_whole_samples, show_project_markers, show_project_regions) \
         VALUES(1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            i64::from(settings.sample_rate()),
            i64::from(settings.ppq()),
            settings.initial_tempo_bpm(),
            match settings.pan_mode() {
                PanMode::LegacyMonoStereo => 0,
                PanMode::ZeroDbBalance => 1,
            },
            i64::from(settings.start_measure()),
            i64::from(settings.bpm_basis().storage_code()),
            settings.base_ruler_label_spacing_on_measure(),
            settings.project_start_time_offset_seconds(),
            settings.project_start_time_fractional_frame_offset_allowed(),
            settings.force_tempo_changes_to_whole_samples(),
            snapshot.show_project_markers,
            snapshot.show_project_regions,
        ],
    )?;

    for (position, track) in snapshot.tracks.iter().enumerate() {
        transaction.execute(
            "INSERT INTO tracks(id, position, name, volume_db, pan, muted, solo, record_armed, instrument_id, instrument_path, instrument_state, is_bus, output_track_id, frozen_audio_item_id, main_send_enabled, is_folder, parent_track_id, phase_inverted) \
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
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
                track.instrument.as_ref().map(|instrument| &instrument.bundle_path),
                track.instrument.as_ref().and_then(|instrument| instrument.state.as_deref()),
                track.is_bus,
                track.output_track_id.map(to_sql_integer).transpose()?,
                track.frozen_audio_item_id.map(to_sql_integer).transpose()?,
                track.main_send_enabled,
                track.is_folder,
                track.parent_track_id.map(to_sql_integer).transpose()?,
                track.phase_inverted
            ],
        )?;
    }

    for track in &snapshot.tracks {
        for (position, send) in track.sends.iter().enumerate() {
            transaction.execute("INSERT INTO track_sends(id, source_track_id, position, destination_track_id, volume_db, pan, muted, phase_inverted, tap, midi_enabled) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)", params![
                to_sql_integer(send.id)?, to_sql_integer(track.id)?, usize_to_sql(position)?,
                to_sql_integer(send.destination_track_id)?, f64::from(send.parameters.volume_db),
                f64::from(send.parameters.pan), send.parameters.muted, send.parameters.phase_inverted,
                match send.parameters.tap { aaadaw_core::AudioSendTap::PostFader => 0, aaadaw_core::AudioSendTap::PreFx => 1, aaadaw_core::AudioSendTap::PreFader => 3 },
                send.parameters.midi_enabled,
            ])?;
        }
        for (position, point) in track.volume_automation.iter().enumerate() {
            transaction.execute(
                "INSERT INTO track_volume_automation(track_id, position, sample, gain_db) \
                 VALUES(?1, ?2, ?3, ?4)",
                params![
                    to_sql_integer(track.id)?,
                    usize_to_sql(position)?,
                    to_sql_integer(point.sample())?,
                    f64::from(point.gain_db()),
                ],
            )?;
        }
        for (position, area) in track.razor_edit_areas.iter().enumerate() {
            transaction.execute(
                "INSERT INTO track_razor_edit_areas(track_id, position, start_sample, end_sample) \
                 VALUES(?1, ?2, ?3, ?4)",
                params![
                    to_sql_integer(track.id)?,
                    usize_to_sql(position)?,
                    to_sql_integer(area.start_sample())?,
                    to_sql_integer(area.end_sample())?,
                ],
            )?;
        }
        for (position, plugin) in track.fx_chain.iter().enumerate() {
            transaction.execute(
                "INSERT INTO track_fx_plugins(track_id, position, plugin_id, plugin_path, enabled, state) \
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    to_sql_integer(track.id)?,
                    usize_to_sql(position)?,
                    plugin.plugin_id,
                    plugin.bundle_path,
                    plugin.enabled,
                    plugin.state.as_deref(),
                ],
            )?;
            for parameter in &plugin.parameter_values {
                if !parameter.value.is_finite() {
                    return Err(StorageError::InvalidStoredData("track FX parameter value"));
                }
                transaction.execute(
                    "INSERT INTO track_fx_parameter_values(track_id, position, parameter_id, value) \
                     VALUES(?1, ?2, ?3, ?4)",
                    params![
                        to_sql_integer(track.id)?,
                        usize_to_sql(position)?,
                        i64::from(parameter.parameter_id),
                        parameter.value,
                    ],
                )?;
            }
            for lane in &plugin.parameter_automation {
                for (point_position, point) in lane.points.iter().enumerate() {
                    if !point.value.is_finite() {
                        return Err(StorageError::InvalidStoredData(
                            "track FX parameter automation value",
                        ));
                    }
                    transaction.execute(
                        "INSERT INTO track_fx_parameter_automation_points(\
                         track_id, position, parameter_id, point_position, sample, value) \
                         VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                        params![
                            to_sql_integer(track.id)?,
                            usize_to_sql(position)?,
                            i64::from(lane.parameter_id),
                            usize_to_sql(point_position)?,
                            to_sql_integer(point.sample)?,
                            point.value,
                        ],
                    )?;
                }
            }
        }
    }

    for (position, item) in snapshot.audio_items.iter().enumerate() {
        transaction.execute(
            "INSERT INTO audio_items(id, track_id, position, media_ref, name, start_sample, \
             source_offset_samples, signed_source_offset_samples, length_samples, volume, take_volume, take_pan, take_playback_rate, muted, phase_inverted, loop_source, \
             source_section_start_samples, source_section_length_samples, active_take_index) \
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            params![
                to_sql_integer(item.id)?,
                to_sql_integer(item.track_id)?,
                usize_to_sql(position)?,
                item.media_ref,
                item.name,
                to_sql_integer(item.start_sample)?,
                item.source_offset_samples.max(0),
                item.source_offset_samples,
                to_sql_integer(item.length_samples)?,
                f64::from(item.volume),
                f64::from(item.take_volume),
                f64::from(item.take_pan),
                item.take_playback_rate,
                item.muted,
                item.phase_inverted,
                item.loop_source,
                item.source_section_start_samples
                    .map(to_sql_integer)
                    .transpose()?,
                item.source_section_length_samples
                    .map(to_sql_integer)
                    .transpose()?,
                i64::from(item.active_take_index),
            ],
        )?;
        for take in &item.other_takes {
            transaction.execute(
                "INSERT INTO audio_item_takes(item_id, take_index, media_ref, name, source_offset_samples, \
                 phase_inverted, loop_source, source_section_start_samples, source_section_length_samples, \
                 take_volume, take_pan, take_playback_rate) \
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    to_sql_integer(item.id)?,
                    i64::from(take.index),
                    take.media_ref,
                    take.name,
                    take.source_offset_samples,
                    take.phase_inverted,
                    take.loop_source,
                    take.source_section_start_samples.map(to_sql_integer).transpose()?,
                    take.source_section_length_samples.map(to_sql_integer).transpose()?,
                    f64::from(take.volume),
                    f64::from(take.pan),
                    take.playback_rate,
                ],
            )?;
        }
        transaction.execute(
            "INSERT INTO audio_item_fades(item_id, fade_in_samples, fade_out_samples, fade_in_shape, fade_out_shape) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![to_sql_integer(item.id)?, item.fades.fade_in.length_samples(), item.fades.fade_out.length_samples(), legacy_fade_code(item.fades.fade_in.curve()), legacy_fade_code(item.fades.fade_out.curve())],
        )?;
        let in_parameters = native_fade_parameters(item.fades.fade_in.curve());
        let out_parameters = native_fade_parameters(item.fades.fade_out.curve());
        if in_parameters.is_some() || out_parameters.is_some() {
            transaction.execute("INSERT INTO audio_item_fade_curves(item_id, in_curvature, in_s, out_curvature, out_s) VALUES(?1, ?2, ?3, ?4, ?5)",
                params![to_sql_integer(item.id)?, in_parameters.map(|p| p.curvature()), in_parameters.map(|p| p.s_parameter()), out_parameters.map(|p| p.curvature()), out_parameters.map(|p| p.s_parameter())])?;
        }
    }

    for (position, item) in snapshot.midi_items.iter().enumerate() {
        let (source_offset_ticks, source_offset_negative) =
            signed_midi_offset_to_sql(item.source_offset_ticks)?;
        transaction.execute(
            "INSERT INTO items(id, track_id, position, start_tick, start_tick_subtick, length_ticks, name, source_offset_ticks, loop_source, source_loop_length_ticks, muted, active_take_name_is_empty, source_offset_negative) \
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                to_sql_integer(item.id)?,
                to_sql_integer(item.track_id)?,
                usize_to_sql(position)?,
                to_sql_integer(item.start_tick)?,
                i64::from(item.start_tick_subtick),
                to_sql_integer(item.length_ticks)?,
                if item.name.is_empty() {
                    EMPTY_ACTIVE_MIDI_TAKE_NAME
                } else {
                    item.name.as_str()
                },
                source_offset_ticks,
                item.loop_source,
                to_sql_integer(item.source_loop_length_ticks)?,
                item.muted,
                item.name.is_empty(),
                source_offset_negative,
            ],
        )?;
        for (position, note) in item.notes.iter().enumerate() {
            transaction.execute(
                "INSERT INTO midi_notes(id, item_id, position, pitch, channel, tick, duration, velocity, off_velocity) \
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    to_sql_integer(note.id)?,
                    to_sql_integer(item.id)?,
                    usize_to_sql(position)?,
                    i64::from(note.data.pitch),
                    i64::from(note.data.channel),
                    to_sql_integer(note.data.tick)?,
                    to_sql_integer(note.data.duration)?,
                    i64::from(note.data.velocity),
                    i64::from(note.data.off_velocity)
                ],
            )?;
        }
        for (position, controller) in item.controllers.iter().enumerate() {
            transaction.execute(
                "INSERT INTO midi_controllers(item_id, position, controller, channel, tick, value) \
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    to_sql_integer(item.id)?,
                    usize_to_sql(position)?,
                    i64::from(controller.controller),
                    i64::from(controller.channel),
                    to_sql_integer(controller.tick)?,
                    i64::from(controller.value)
                ],
            )?;
        }
        for (position, bend) in item.pitch_bends.iter().enumerate() {
            transaction.execute(
                "INSERT INTO midi_pitch_bends(item_id, position, channel, tick, value) \
                 VALUES(?1, ?2, ?3, ?4, ?5)",
                params![
                    to_sql_integer(item.id)?,
                    usize_to_sql(position)?,
                    i64::from(bend.channel),
                    to_sql_integer(bend.tick)?,
                    i64::from(bend.value)
                ],
            )?;
        }
        for (position, event) in item.program_changes.iter().enumerate() {
            transaction.execute(
                "INSERT INTO midi_program_changes(item_id, position, channel, tick, program) \
                 VALUES(?1, ?2, ?3, ?4, ?5)",
                params![
                    to_sql_integer(item.id)?,
                    usize_to_sql(position)?,
                    i64::from(event.channel),
                    to_sql_integer(event.tick)?,
                    i64::from(event.program)
                ],
            )?;
        }
        for (position, event) in item.poly_aftertouches.iter().enumerate() {
            transaction.execute(
                "INSERT INTO midi_poly_aftertouches(item_id, position, channel, tick, note, value) \
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    to_sql_integer(item.id)?,
                    usize_to_sql(position)?,
                    i64::from(event.channel),
                    to_sql_integer(event.tick)?,
                    i64::from(event.note),
                    i64::from(event.value)
                ],
            )?;
        }
        for (position, event) in item.channel_pressures.iter().enumerate() {
            transaction.execute(
                "INSERT INTO midi_channel_pressures(item_id, position, channel, tick, value) \
                 VALUES(?1, ?2, ?3, ?4, ?5)",
                params![
                    to_sql_integer(item.id)?,
                    usize_to_sql(position)?,
                    i64::from(event.channel),
                    to_sql_integer(event.tick)?,
                    i64::from(event.value)
                ],
            )?;
        }
        for (position, event) in item.opaque_events.iter().enumerate() {
            let (event_kind, meta_type, data) = match &event.kind {
                MidiOpaqueEventKind::Meta { kind, data } => (0_i64, Some(i64::from(*kind)), data),
                MidiOpaqueEventKind::SystemExclusive { status: 0xF0, data } => (1, None, data),
                MidiOpaqueEventKind::SystemExclusive { status: 0xF7, data } => (2, None, data),
                MidiOpaqueEventKind::SystemExclusive { .. } => {
                    return Err(StorageError::InvalidStoredData(
                        "unsupported MIDI SysEx status",
                    ));
                }
            };
            transaction.execute(
                "INSERT INTO midi_opaque_events(item_id, position, tick, source_order, event_kind, meta_type, data) \
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    to_sql_integer(item.id)?,
                    usize_to_sql(position)?,
                    to_sql_integer(event.tick)?,
                    i64::from(event.order),
                    event_kind,
                    meta_type,
                    data.as_slice(),
                ],
            )?;
        }
        for (position, event) in item.event_order.iter().enumerate() {
            let (event_kind, tick, channel, data1, data2, data3, duration) = match event.target {
                MidiEventOrderTarget::Note(note) => (
                    0_i64,
                    note.tick,
                    note.channel,
                    i64::from(note.pitch),
                    i64::from(note.velocity),
                    i64::from(note.off_velocity),
                    note.duration,
                ),
                MidiEventOrderTarget::Controller(controller) => (
                    1,
                    controller.tick,
                    controller.channel,
                    i64::from(controller.controller),
                    i64::from(controller.value),
                    0,
                    0,
                ),
                MidiEventOrderTarget::PitchBend(bend) => {
                    (2, bend.tick, bend.channel, i64::from(bend.value), 0, 0, 0)
                }
                MidiEventOrderTarget::ProgramChange(change) => (
                    3,
                    change.tick,
                    change.channel,
                    i64::from(change.program),
                    0,
                    0,
                    0,
                ),
                MidiEventOrderTarget::PolyAftertouch(aftertouch) => (
                    4,
                    aftertouch.tick,
                    aftertouch.channel,
                    i64::from(aftertouch.note),
                    i64::from(aftertouch.value),
                    0,
                    0,
                ),
                MidiEventOrderTarget::ChannelPressure(pressure) => (
                    5,
                    pressure.tick,
                    pressure.channel,
                    i64::from(pressure.value),
                    0,
                    0,
                    0,
                ),
            };
            transaction.execute(
                "INSERT INTO midi_event_order(item_id, position, source_order, event_kind, tick, channel, data1, data2, data3, duration, note_off_order) \
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    to_sql_integer(item.id)?,
                    usize_to_sql(position)?,
                    i64::from(event.order),
                    event_kind,
                    to_sql_integer(tick)?,
                    i64::from(channel),
                    data1,
                    data2,
                    data3,
                    to_sql_integer(duration)?,
                    event.note_off_order.map(i64::from),
                ],
            )?;
        }

        transaction.execute(
            "INSERT INTO midi_item_take_state(item_id, active_take_index) VALUES(?1, ?2)",
            params![to_sql_integer(item.id)?, i64::from(item.active_take_index)],
        )?;
        for take in &item.other_takes {
            write_midi_take(transaction, item.id, take)?;
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
    for marker in &snapshot.markers {
        transaction.execute(
            "INSERT INTO project_markers(id, position_ticks, name, marker_number, visible) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![
                to_sql_integer(marker.id)?,
                to_sql_integer(marker.position_ticks)?,
                marker.name,
                i64::from(marker.number),
                marker.visible
            ],
        )?;
    }
    for region in &snapshot.regions {
        transaction.execute(
            "INSERT INTO project_regions(id, region_number, start_ticks, end_ticks, name, visible, render_master_mix) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                to_sql_integer(region.id)?,
                i64::from(region.number),
                to_sql_integer(region.start_ticks)?,
                to_sql_integer(region.end_ticks)?,
                region.name,
                region.visible,
                region.render_master_mix
            ],
        )?;
        for (position, track_id) in region.render_track_ids.iter().copied().enumerate() {
            transaction.execute(
                "INSERT INTO project_region_render_tracks(region_id, track_id, position) VALUES(?1, ?2, ?3)",
                params![
                    to_sql_integer(region.id)?,
                    to_sql_integer(track_id)?,
                    usize_to_sql(position)?
                ],
            )?;
        }
    }
    Ok(())
}

fn write_midi_take(
    transaction: &Transaction<'_>,
    item_id: u64,
    take: &MidiItemTakeSnapshot,
) -> Result<(), StorageError> {
    let (source_offset_ticks, source_offset_negative) =
        signed_midi_offset_to_sql(take.source_offset_ticks)?;
    transaction.execute(
        "INSERT INTO midi_item_takes(item_id, take_index, name, source_offset_ticks, loop_source, source_loop_length_ticks, source_offset_negative) \
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            to_sql_integer(item_id)?,
            i64::from(take.index),
            take.name,
            source_offset_ticks,
            take.loop_source,
            to_sql_integer(take.source_loop_length_ticks)?,
            source_offset_negative,
        ],
    )?;
    let insert = |kind: i64,
                  position: usize,
                  note_id: Option<u64>,
                  target_kind: Option<i64>,
                  tick: u64,
                  channel: u8,
                  data1: i64,
                  data2: i64,
                  data3: i64,
                  duration: u64,
                  value: i64,
                  payload: Option<&[u8]>,
                  source_order: u32,
                  note_off_order: Option<u32>| {
        transaction.execute(
            "INSERT INTO midi_take_events(item_id, take_index, event_kind, position, note_id, target_kind, tick, channel, data1, data2, data3, duration, value, payload, source_order, note_off_order) \
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                to_sql_integer(item_id)?, i64::from(take.index), kind,
                usize_to_sql(position)?, note_id.map(to_sql_integer).transpose()?, target_kind,
                to_sql_integer(tick)?, i64::from(channel), data1, data2, data3,
                to_sql_integer(duration)?, value, payload, i64::from(source_order),
                note_off_order.map(i64::from),
            ],
        )?;
        Ok::<(), StorageError>(())
    };
    for (position, note) in take.notes.iter().enumerate() {
        insert(
            0,
            position,
            Some(note.id),
            None,
            note.data.tick,
            note.data.channel,
            i64::from(note.data.pitch),
            i64::from(note.data.velocity),
            i64::from(note.data.off_velocity),
            note.data.duration,
            0,
            None,
            0,
            None,
        )?;
    }
    for (position, event) in take.controllers.iter().enumerate() {
        insert(
            1,
            position,
            None,
            None,
            event.tick,
            event.channel,
            i64::from(event.controller),
            i64::from(event.value),
            0,
            0,
            0,
            None,
            0,
            None,
        )?;
    }
    for (position, event) in take.pitch_bends.iter().enumerate() {
        insert(
            2,
            position,
            None,
            None,
            event.tick,
            event.channel,
            0,
            0,
            0,
            0,
            i64::from(event.value),
            None,
            0,
            None,
        )?;
    }
    for (position, event) in take.program_changes.iter().enumerate() {
        insert(
            3,
            position,
            None,
            None,
            event.tick,
            event.channel,
            i64::from(event.program),
            0,
            0,
            0,
            0,
            None,
            0,
            None,
        )?;
    }
    for (position, event) in take.poly_aftertouches.iter().enumerate() {
        insert(
            4,
            position,
            None,
            None,
            event.tick,
            event.channel,
            i64::from(event.note),
            0,
            0,
            0,
            i64::from(event.value),
            None,
            0,
            None,
        )?;
    }
    for (position, event) in take.channel_pressures.iter().enumerate() {
        insert(
            5,
            position,
            None,
            None,
            event.tick,
            event.channel,
            0,
            0,
            0,
            0,
            i64::from(event.value),
            None,
            0,
            None,
        )?;
    }
    for (position, event) in take.opaque_events.iter().enumerate() {
        let (event_kind, data1, data) = match &event.kind {
            MidiOpaqueEventKind::Meta { kind, data } => (0, i64::from(*kind), data.as_slice()),
            MidiOpaqueEventKind::SystemExclusive { status: 0xF0, data } => (1, 0, data.as_slice()),
            MidiOpaqueEventKind::SystemExclusive { status: 0xF7, data } => (2, 0, data.as_slice()),
            MidiOpaqueEventKind::SystemExclusive { .. } => {
                return Err(StorageError::InvalidStoredData(
                    "unsupported MIDI SysEx status",
                ));
            }
        };
        insert(
            6,
            position,
            None,
            Some(event_kind),
            event.tick,
            0,
            data1,
            0,
            0,
            0,
            0,
            Some(data),
            event.order,
            None,
        )?;
    }
    for (position, event) in take.event_order.iter().enumerate() {
        let (target_kind, tick, channel, data1, data2, data3, duration, value) = match event.target
        {
            MidiEventOrderTarget::Note(note) => (
                0,
                note.tick,
                note.channel,
                i64::from(note.pitch),
                i64::from(note.velocity),
                i64::from(note.off_velocity),
                note.duration,
                0,
            ),
            MidiEventOrderTarget::Controller(event) => (
                1,
                event.tick,
                event.channel,
                i64::from(event.controller),
                i64::from(event.value),
                0,
                0,
                0,
            ),
            MidiEventOrderTarget::PitchBend(event) => (
                2,
                event.tick,
                event.channel,
                0,
                0,
                0,
                0,
                i64::from(event.value),
            ),
            MidiEventOrderTarget::ProgramChange(event) => (
                3,
                event.tick,
                event.channel,
                i64::from(event.program),
                0,
                0,
                0,
                0,
            ),
            MidiEventOrderTarget::PolyAftertouch(event) => (
                4,
                event.tick,
                event.channel,
                i64::from(event.note),
                0,
                0,
                0,
                i64::from(event.value),
            ),
            MidiEventOrderTarget::ChannelPressure(event) => (
                5,
                event.tick,
                event.channel,
                0,
                0,
                0,
                0,
                i64::from(event.value),
            ),
        };
        insert(
            7,
            position,
            None,
            Some(target_kind),
            tick,
            channel,
            data1,
            data2,
            data3,
            duration,
            value,
            None,
            event.order,
            event.note_off_order,
        )?;
    }
    Ok(())
}

fn write_arrangement_view_state(
    transaction: &Transaction<'_>,
    view_state: &ArrangementViewState,
) -> Result<(), StorageError> {
    validate_time_range(view_state.time_selection, false)?;
    validate_time_range(view_state.loop_points, true)?;
    if !(-100..=100).contains(&view_state.swing_amount_percent) {
        return Err(StorageError::InvalidStoredData("invalid Swing amount"));
    }
    transaction.execute("DELETE FROM arrangement_folder_compact", [])?;
    for folder in &view_state.folder_compact {
        if folder.mode > 2 {
            return Err(StorageError::InvalidStoredData(
                "invalid folder compact mode",
            ));
        }
        transaction.execute(
            "INSERT INTO arrangement_folder_compact(track_id, mode) VALUES(?1, ?2)",
            params![to_sql_integer(folder.track_id)?, folder.mode],
        )?;
    }
    transaction.execute("DELETE FROM arrangement_volume_lanes", [])?;
    transaction.execute("DELETE FROM arrangement_fx_lanes", [])?;
    transaction.execute(
        "INSERT INTO arrangement_view_meta(\
         singleton, state_version, repeat_enabled, snap_enabled, snap_grid, \
         midi_editor_snap_grid, swing_grid_enabled, swing_amount_percent) \
         VALUES(1, 1, ?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(singleton) DO UPDATE SET state_version = excluded.state_version, \
         repeat_enabled = excluded.repeat_enabled, snap_enabled = excluded.snap_enabled, \
         snap_grid = excluded.snap_grid, midi_editor_snap_grid = excluded.midi_editor_snap_grid, \
         swing_grid_enabled = excluded.swing_grid_enabled, \
         swing_amount_percent = excluded.swing_amount_percent",
        params![
            if view_state.repeat_enabled {
                1_i64
            } else {
                0_i64
            },
            if view_state.snap_enabled {
                1_i64
            } else {
                0_i64
            },
            view_state.snap_grid.storage_id(),
            view_state.midi_editor_snap_grid.storage_id(),
            if view_state.swing_grid_enabled {
                1_i64
            } else {
                0_i64
            },
            i64::from(view_state.swing_amount_percent),
        ],
    )?;
    transaction.execute(
        "INSERT INTO arrangement_time_ranges(\
         singleton, time_selection_start_seconds, time_selection_end_seconds, \
         loop_start_seconds, loop_end_seconds) VALUES(1, ?1, ?2, ?3, ?4) \
         ON CONFLICT(singleton) DO UPDATE SET \
         time_selection_start_seconds = excluded.time_selection_start_seconds, \
         time_selection_end_seconds = excluded.time_selection_end_seconds, \
         loop_start_seconds = excluded.loop_start_seconds, \
         loop_end_seconds = excluded.loop_end_seconds",
        params![
            view_state.time_selection.map(|range| range.start_seconds),
            view_state.time_selection.map(|range| range.end_seconds),
            view_state.loop_points.map(|range| range.start_seconds),
            view_state.loop_points.map(|range| range.end_seconds),
        ],
    )?;
    for lane in &view_state.volume_lanes {
        transaction.execute(
            "INSERT INTO arrangement_volume_lanes(track_id, visible) VALUES(?1, ?2)",
            params![
                to_sql_integer(lane.track_id)?,
                if lane.visible { 1_i64 } else { 0_i64 },
            ],
        )?;
    }
    for lane in &view_state.fx_lanes {
        if !lane.value_range.0.is_finite()
            || !lane.value_range.1.is_finite()
            || lane.value_range.0 >= lane.value_range.1
            || !lane.height.is_finite()
            || lane.height <= 0.0
        {
            return Err(StorageError::InvalidStoredData(
                "invalid FX automation lane view state",
            ));
        }
        transaction.execute(
            "INSERT INTO arrangement_fx_lanes(\
             track_id, chain_index, plugin_id, bundle_path, parameter_id, name, \
             value_min, value_max, stepped, height) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                to_sql_integer(lane.track_id)?,
                usize_to_sql(lane.chain_index)?,
                lane.plugin_id,
                lane.bundle_path,
                i64::from(lane.parameter_id),
                lane.name,
                lane.value_range.0,
                lane.value_range.1,
                if lane.stepped { 1_i64 } else { 0_i64 },
                f64::from(lane.height),
            ],
        )?;
    }
    Ok(())
}

fn validate_time_range(
    range: Option<crate::ArrangementTimeRangeState>,
    allow_zero_length: bool,
) -> Result<(), StorageError> {
    if range.is_some_and(|range| {
        !range.start_seconds.is_finite()
            || !range.end_seconds.is_finite()
            || if allow_zero_length {
                range.start_seconds > range.end_seconds
            } else {
                range.start_seconds >= range.end_seconds
            }
    }) {
        return Err(StorageError::InvalidStoredData(
            "invalid Arrangement time range",
        ));
    }
    Ok(())
}

fn time_range_from_columns(
    columns: (Option<f64>, Option<f64>),
    allow_zero_length: bool,
) -> Result<Option<crate::ArrangementTimeRangeState>, StorageError> {
    match columns {
        (None, None) => Ok(None),
        (Some(start_seconds), Some(end_seconds)) => {
            let range = crate::ArrangementTimeRangeState {
                start_seconds,
                end_seconds,
            };
            validate_time_range(Some(range), allow_zero_length)?;
            Ok(Some(range))
        }
        _ => Err(StorageError::InvalidStoredData(
            "incomplete Arrangement time range",
        )),
    }
}

fn read_tracks(connection: &Connection) -> Result<Vec<TrackSnapshot>, StorageError> {
    let mut sends = read_audio_sends(connection)?;
    let mut volume_automation = read_volume_automation(connection)?;
    let mut razor_edit_areas = read_razor_edit_areas(connection)?;
    let mut fx_chains = HashMap::<i64, Vec<TrackFxPluginSnapshot>>::new();
    let mut parameter_values = HashMap::<(i64, i64), Vec<TrackFxParameterValueSnapshot>>::new();
    let mut fx_statement = connection.prepare(
        "SELECT track_id, position, plugin_id, plugin_path, enabled, state \
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
                row.get::<_, Option<Vec<u8>>>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (track_id, position, plugin_id, bundle_path, enabled, state) in fx_rows {
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
                state,
                parameter_values: Vec::new(),
                parameter_automation: Vec::new(),
            });
    }

    let mut parameter_statement = connection.prepare(
        "SELECT track_id, position, parameter_id, value \
         FROM track_fx_parameter_values ORDER BY track_id, position, parameter_id",
    )?;
    let parameter_rows = parameter_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, f64>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (track_id, position, parameter_id, value) in parameter_rows {
        let parameter_id = u32::try_from(parameter_id)
            .map_err(|_| StorageError::InvalidStoredData("track FX parameter ID"))?;
        if !value.is_finite() {
            return Err(StorageError::InvalidStoredData("track FX parameter value"));
        }
        parameter_values
            .entry((track_id, position))
            .or_default()
            .push(TrackFxParameterValueSnapshot {
                parameter_id,
                value,
            });
    }
    for (track_id, chain) in &mut fx_chains {
        for (position, plugin) in chain.iter_mut().enumerate() {
            plugin.parameter_values = parameter_values
                .remove(&(*track_id, usize_to_sql(position)?))
                .unwrap_or_default();
        }
    }

    let mut parameter_automation =
        HashMap::<(i64, i64), Vec<TrackFxParameterAutomationLaneSnapshot>>::new();
    let mut automation_statement = connection.prepare(
        "SELECT track_id, position, parameter_id, point_position, sample, value \
         FROM track_fx_parameter_automation_points \
         ORDER BY track_id, position, parameter_id, point_position",
    )?;
    let automation_rows = automation_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, f64>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (track_id, position, parameter_id, point_position, sample, value) in automation_rows {
        let parameter_id = u32::try_from(parameter_id)
            .map_err(|_| StorageError::InvalidStoredData("track FX automation parameter ID"))?;
        let point_position = from_sql_u64(point_position)?;
        let sample = from_sql_u64(sample)?;
        if !value.is_finite() {
            return Err(StorageError::InvalidStoredData(
                "track FX parameter automation value",
            ));
        }
        let lanes = parameter_automation
            .entry((track_id, position))
            .or_default();
        let lane_index = match lanes.binary_search_by_key(&parameter_id, |lane| lane.parameter_id) {
            Ok(index) => index,
            Err(index) => {
                lanes.insert(
                    index,
                    TrackFxParameterAutomationLaneSnapshot {
                        parameter_id,
                        points: Vec::new(),
                    },
                );
                index
            }
        };
        let points = &mut lanes[lane_index].points;
        if usize::try_from(point_position).ok() != Some(points.len()) {
            return Err(StorageError::InvalidStoredData(
                "track FX automation point order",
            ));
        }
        points.push(TrackFxParameterAutomationPointSnapshot { sample, value });
    }
    for (track_id, chain) in &mut fx_chains {
        for (position, plugin) in chain.iter_mut().enumerate() {
            plugin.parameter_automation = parameter_automation
                .remove(&(*track_id, usize_to_sql(position)?))
                .unwrap_or_default();
        }
    }

    let mut statement = connection.prepare(
        "SELECT id, position, name, volume_db, pan, muted, solo, record_armed, instrument_id, instrument_path, instrument_state, is_bus, output_track_id, frozen_audio_item_id, main_send_enabled, is_folder, parent_track_id, phase_inverted \
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
                row.get::<_, Option<Vec<u8>>>(10)?,
                row.get::<_, bool>(11)?,
                row.get::<_, Option<i64>>(12)?,
                row.get::<_, Option<i64>>(13)?,
                row.get::<_, bool>(14)?,
                row.get::<_, bool>(15)?,
                row.get::<_, Option<i64>>(16)?,
                row.get::<_, bool>(17)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let tracks = rows
        .into_iter()
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
                instrument_state,
                is_bus,
                output_track_id,
                frozen_audio_item_id,
                main_send_enabled,
                is_folder,
                parent_track_id,
                phase_inverted,
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
                            state: instrument_state,
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
                    is_bus,
                    output_track_id: output_track_id.map(from_sql_u64).transpose()?,
                    phase_inverted,
                    main_send_enabled,
                    is_folder,
                    parent_track_id: parent_track_id.map(from_sql_u64).transpose()?,
                    sends: sends.remove(&id).unwrap_or_default(),
                    volume_db: volume_db as f32,
                    pan: pan as f32,
                    muted,
                    solo,
                    record_armed,
                    instrument,
                    fx_chain: fx_chains.remove(&id).unwrap_or_default(),
                    volume_automation: volume_automation.remove(&id).unwrap_or_default(),
                    razor_edit_areas: razor_edit_areas.remove(&id).unwrap_or_default(),
                    frozen_audio_item_id: frozen_audio_item_id.map(from_sql_u64).transpose()?,
                })
            },
        )
        .collect::<Result<Vec<_>, StorageError>>()?;
    if !sends.is_empty() {
        return Err(StorageError::InvalidStoredData(
            "audio sends reference a missing source",
        ));
    }
    if !razor_edit_areas.is_empty() {
        return Err(StorageError::InvalidStoredData(
            "razor edit areas reference a missing track",
        ));
    }
    Ok(tracks)
}

fn read_razor_edit_areas(
    connection: &Connection,
) -> Result<HashMap<i64, Vec<RazorEditArea>>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT track_id, position, start_sample, end_sample \
         FROM track_razor_edit_areas ORDER BY track_id, position",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut areas = HashMap::<i64, Vec<RazorEditArea>>::new();
    for (track_id, position, start_sample, end_sample) in rows {
        let position = from_sql_u64(position)?;
        let track_areas = areas.entry(track_id).or_default();
        if usize::try_from(position).ok() != Some(track_areas.len()) {
            return Err(StorageError::InvalidStoredData("razor edit area order"));
        }
        track_areas.push(
            RazorEditArea::new(from_sql_u64(start_sample)?, from_sql_u64(end_sample)?)
                .ok_or(StorageError::InvalidStoredData("razor edit area range"))?,
        );
    }
    Ok(areas)
}

fn read_volume_automation(
    connection: &Connection,
) -> Result<HashMap<i64, Vec<VolumeAutomationPoint>>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT track_id, sample, gain_db FROM track_volume_automation \
         ORDER BY track_id, position",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut points_by_track = HashMap::<i64, Vec<VolumeAutomationPoint>>::new();
    for (track_id, sample, gain_db) in rows {
        let sample = from_sql_u64(sample)?;
        let point = VolumeAutomationPoint::new(sample, gain_db as f32)
            .ok_or(StorageError::InvalidStoredData("track volume automation"))?;
        points_by_track.entry(track_id).or_default().push(point);
    }
    Ok(points_by_track)
}

fn legacy_fade_code(curve: aaadaw_core::FadeCurve) -> u8 {
    match curve {
        aaadaw_core::FadeCurve::Legacy(shape) => shape.code(),
        aaadaw_core::FadeCurve::Native(_) => 0,
    }
}

fn native_fade_parameters(
    curve: aaadaw_core::FadeCurve,
) -> Option<aaadaw_core::FadeCurveParameters> {
    match curve {
        aaadaw_core::FadeCurve::Legacy(_) => None,
        aaadaw_core::FadeCurve::Native(parameters) => Some(parameters),
    }
}

fn read_native_fade_curve(
    curvature: Option<f64>,
    s: Option<f64>,
) -> Result<Option<aaadaw_core::FadeCurve>, StorageError> {
    match (curvature, s) {
        (None, None) => Ok(None),
        (Some(curvature), Some(s)) => aaadaw_core::FadeCurveParameters::new(curvature, s)
            .map(|p| Some(aaadaw_core::FadeCurve::Native(p)))
            .map_err(|_| StorageError::Snapshot(SnapshotError::InvalidProjectData)),
        _ => Err(StorageError::Snapshot(SnapshotError::InvalidProjectData)),
    }
}

fn read_audio_items(connection: &Connection) -> Result<Vec<AudioItemSnapshot>, StorageError> {
    let mut curves = HashMap::new();
    let mut curve_statement = connection.prepare(
        "SELECT item_id, in_curvature, in_s, out_curvature, out_s FROM audio_item_fade_curves",
    )?;
    let curve_rows = curve_statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<f64>>(1)?,
            row.get::<_, Option<f64>>(2)?,
            row.get::<_, Option<f64>>(3)?,
            row.get::<_, Option<f64>>(4)?,
        ))
    })?;
    for row in curve_rows {
        let (id, in_curvature, in_s, out_curvature, out_s) = row?;
        let fade_in = read_native_fade_curve(in_curvature, in_s)?;
        let fade_out = read_native_fade_curve(out_curvature, out_s)?;
        if fade_in.is_none() && fade_out.is_none() {
            return Err(StorageError::Snapshot(SnapshotError::InvalidProjectData));
        }
        curves.insert(from_sql_u64(id)?, (fade_in, fade_out));
    }
    let mut fades = HashMap::new();
    let mut fade_statement = connection.prepare("SELECT item_id, fade_in_samples, fade_out_samples, fade_in_shape, fade_out_shape FROM audio_item_fades")?;
    let fade_rows = fade_statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, f64>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, u8>(3)?,
            row.get::<_, u8>(4)?,
        ))
    })?;
    for row in fade_rows {
        let (id, fade_in, fade_out, in_shape, out_shape) = row?;
        let invalid = || StorageError::Snapshot(SnapshotError::InvalidProjectData);
        let in_shape = aaadaw_core::FadeShape::from_code(in_shape).ok_or_else(invalid)?;
        let out_shape = aaadaw_core::FadeShape::from_code(out_shape).ok_or_else(invalid)?;
        let (in_curve, out_curve) = curves.remove(&from_sql_u64(id)?).unwrap_or((None, None));
        fades.insert(
            from_sql_u64(id)?,
            aaadaw_core::AudioItemFades {
                fade_in: aaadaw_core::AudioFade::with_curve(
                    fade_in,
                    in_curve.unwrap_or(aaadaw_core::FadeCurve::Legacy(in_shape)),
                )
                .map_err(|_| invalid())?,
                fade_out: aaadaw_core::AudioFade::with_curve(
                    fade_out,
                    out_curve.unwrap_or(aaadaw_core::FadeCurve::Legacy(out_shape)),
                )
                .map_err(|_| invalid())?,
            },
        );
    }
    if !curves.is_empty() {
        return Err(StorageError::Snapshot(SnapshotError::InvalidProjectData));
    }
    let mut other_takes = HashMap::<u64, Vec<aaadaw_core::AudioItemTakeSnapshot>>::new();
    let mut take_statement = connection.prepare(
        "SELECT item_id, take_index, media_ref, name, source_offset_samples, phase_inverted, \
         loop_source, source_section_start_samples, source_section_length_samples, take_volume, \
         take_pan, take_playback_rate FROM audio_item_takes ORDER BY item_id, take_index",
    )?;
    let take_rows = take_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, Option<i64>>(7)?,
                row.get::<_, Option<i64>>(8)?,
                row.get::<_, f64>(9)?,
                row.get::<_, f64>(10)?,
                row.get::<_, f64>(11)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (
        item_id,
        take_index,
        media_ref,
        name,
        source_offset_samples,
        phase_inverted_value,
        loop_source_value,
        source_section_start_samples,
        source_section_length_samples,
        volume,
        pan,
        playback_rate,
    ) in take_rows
    {
        let invalid = || StorageError::Snapshot(SnapshotError::InvalidProjectData);
        let phase_inverted = match phase_inverted_value {
            0 => false,
            1 => true,
            _ => return Err(invalid()),
        };
        let loop_source = match loop_source_value {
            0 => false,
            1 => true,
            _ => return Err(invalid()),
        };
        if !volume.is_finite()
            || !(0.0..=16.0).contains(&volume)
            || !pan.is_finite()
            || !(-1.0..=1.0).contains(&pan)
            || !playback_rate.is_finite()
            || playback_rate <= 0.0
        {
            return Err(invalid());
        }
        let source_section_start_samples =
            source_section_start_samples.map(from_sql_u64).transpose()?;
        let source_section_length_samples = source_section_length_samples
            .map(from_sql_u64)
            .transpose()?;
        if source_section_start_samples.is_some() != source_section_length_samples.is_some() {
            return Err(invalid());
        }
        other_takes.entry(from_sql_u64(item_id)?).or_default().push(
            aaadaw_core::AudioItemTakeSnapshot {
                index: u32::try_from(from_sql_u64(take_index)?).map_err(|_| invalid())?,
                media_ref,
                name,
                source_offset_samples,
                phase_inverted,
                loop_source,
                source_section_start_samples,
                source_section_length_samples,
                volume: volume as f32,
                pan: pan as f32,
                playback_rate,
            },
        );
    }
    let mut statement = connection.prepare(
        "SELECT id, track_id, position, media_ref, name, start_sample, signed_source_offset_samples, \
         length_samples, volume, take_volume, take_pan, take_playback_rate, muted, phase_inverted, loop_source, \
         source_section_start_samples, source_section_length_samples, active_take_index \
         FROM audio_items ORDER BY position",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, f64>(8)?,
                row.get::<_, f64>(9)?,
                row.get::<_, f64>(10)?,
                row.get::<_, f64>(11)?,
                row.get::<_, i64>(12)?,
                row.get::<_, i64>(13)?,
                row.get::<_, i64>(14)?,
                row.get::<_, Option<i64>>(15)?,
                row.get::<_, Option<i64>>(16)?,
                row.get::<_, i64>(17)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let items = rows
        .into_iter()
        .map(
            |(
                id,
                track_id,
                position,
                media_ref,
                name,
                start_sample,
                source_offset,
                length,
                volume,
                take_volume,
                take_pan,
                take_playback_rate,
                muted,
                phase_inverted_value,
                loop_source,
                source_section_start,
                source_section_length,
                active_take_index,
            )| {
                let _ = from_sql_u64(position)?;
                if !volume.is_finite() || !(0.0..=16.0).contains(&volume) {
                    return Err(StorageError::Snapshot(SnapshotError::InvalidProjectData));
                }
                if !take_volume.is_finite()
                    || !(0.0..=16.0).contains(&take_volume)
                    || !take_pan.is_finite()
                    || !(-1.0..=1.0).contains(&take_pan)
                    || !take_playback_rate.is_finite()
                    || take_playback_rate <= 0.0
                {
                    return Err(StorageError::Snapshot(SnapshotError::InvalidProjectData));
                }
                let loop_source = match loop_source {
                    0 => false,
                    1 => true,
                    _ => return Err(StorageError::Snapshot(SnapshotError::InvalidProjectData)),
                };
                let muted = match muted {
                    0 => false,
                    1 => true,
                    _ => return Err(StorageError::Snapshot(SnapshotError::InvalidProjectData)),
                };
                let phase_inverted = match phase_inverted_value {
                    0 => false,
                    1 => true,
                    _ => return Err(StorageError::Snapshot(SnapshotError::InvalidProjectData)),
                };
                let source_section_start_samples =
                    source_section_start.map(from_sql_u64).transpose()?;
                let source_section_length_samples =
                    source_section_length.map(from_sql_u64).transpose()?;
                if source_section_length_samples.is_some() != source_section_start_samples.is_some()
                {
                    return Err(StorageError::Snapshot(SnapshotError::InvalidProjectData));
                }
                Ok(AudioItemSnapshot {
                    fades: fades.remove(&from_sql_u64(id)?).unwrap_or_default(),
                    volume: volume as f32,
                    take_volume: take_volume as f32,
                    take_pan: take_pan as f32,
                    take_playback_rate,
                    id: from_sql_u64(id)?,
                    track_id: from_sql_u64(track_id)?,
                    media_ref,
                    name,
                    start_sample: from_sql_u64(start_sample)?,
                    source_offset_samples: source_offset,
                    length_samples: from_sql_u64(length)?,
                    muted,
                    phase_inverted,
                    loop_source,
                    source_section_start_samples,
                    source_section_length_samples,
                    active_take_index: u32::try_from(from_sql_u64(active_take_index)?)
                        .map_err(|_| StorageError::Snapshot(SnapshotError::InvalidProjectData))?,
                    other_takes: other_takes.remove(&from_sql_u64(id)?).unwrap_or_default(),
                })
            },
        )
        .collect::<Result<Vec<_>, StorageError>>()?;
    if !fades.is_empty() {
        return Err(StorageError::Snapshot(SnapshotError::InvalidProjectData));
    }
    if !other_takes.is_empty() {
        return Err(StorageError::Snapshot(SnapshotError::InvalidProjectData));
    }
    Ok(items)
}

struct StoredMidiTake {
    snapshot: MidiItemTakeSnapshot,
}

fn read_midi_takes(
    connection: &Connection,
) -> Result<HashMap<u64, (u32, Vec<MidiItemTakeSnapshot>)>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT item_id, take_index, name, source_offset_ticks, loop_source, source_loop_length_ticks, source_offset_negative \
         FROM midi_item_takes ORDER BY item_id, take_index",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut takes_by_item: HashMap<u64, Vec<StoredMidiTake>> = HashMap::new();
    for (item_id, index, name, source_offset, loop_source, loop_length, source_offset_negative) in
        rows
    {
        let loop_source = match loop_source {
            0 => false,
            1 => true,
            _ => {
                return Err(StorageError::InvalidStoredData(
                    "invalid MIDI take loop flag",
                ));
            }
        };
        takes_by_item
            .entry(from_sql_u64(item_id)?)
            .or_default()
            .push(StoredMidiTake {
                snapshot: MidiItemTakeSnapshot {
                    index: from_sql_u32(index)?,
                    name,
                    source_offset_ticks: from_sql_signed_midi_offset(
                        source_offset,
                        source_offset_negative,
                    )?,
                    loop_source,
                    source_loop_length_ticks: from_sql_u64(loop_length)?,
                    notes: Vec::new(),
                    controllers: Vec::new(),
                    pitch_bends: Vec::new(),
                    program_changes: Vec::new(),
                    poly_aftertouches: Vec::new(),
                    channel_pressures: Vec::new(),
                    opaque_events: Vec::new(),
                    event_order: Vec::new(),
                },
            });
    }

    let mut statement = connection.prepare(
        "SELECT item_id, take_index, event_kind, position, note_id, target_kind, tick, channel, data1, data2, data3, duration, value, payload, source_order, note_off_order \
         FROM midi_take_events ORDER BY item_id, take_index, event_kind, position",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, i64>(11)?,
                row.get::<_, i64>(12)?,
                row.get::<_, Option<Vec<u8>>>(13)?,
                row.get::<_, i64>(14)?,
                row.get::<_, Option<i64>>(15)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (
        item_id,
        take_index,
        kind,
        _position,
        note_id,
        target_kind,
        tick,
        channel,
        data1,
        data2,
        data3,
        duration,
        value,
        payload,
        source_order,
        note_off_order,
    ) in rows
    {
        let item_id = from_sql_u64(item_id)?;
        let take_index = from_sql_u32(take_index)?;
        let take = takes_by_item
            .get_mut(&item_id)
            .and_then(|takes| {
                takes
                    .iter_mut()
                    .find(|take| take.snapshot.index == take_index)
            })
            .ok_or(StorageError::InvalidStoredData(
                "MIDI take event references a missing take",
            ))?;
        let tick = from_sql_u64(tick)?;
        let channel = from_sql_u8(channel)?;
        let data1_u8 = || from_sql_u8(data1);
        let value_u8 = || from_sql_u8(value);
        let source_order = from_sql_u32(source_order)?;
        match kind {
            0 => take.snapshot.notes.push(MidiNoteSnapshot {
                id: from_sql_u64(
                    note_id.ok_or(StorageError::InvalidStoredData("MIDI take note has no ID"))?,
                )?,
                data: MidiNoteData {
                    pitch: from_sql_u8(data1)?,
                    channel,
                    tick,
                    duration: from_sql_u64(duration)?,
                    velocity: from_sql_u8(data2)?,
                    off_velocity: from_sql_u8(data3)?,
                },
            }),
            1 => take.snapshot.controllers.push(MidiControllerData {
                controller: data1_u8()?,
                channel,
                tick,
                value: from_sql_u8(data2)?,
            }),
            2 => take.snapshot.pitch_bends.push(MidiPitchBendData {
                channel,
                tick,
                value: from_sql_u16(value)?,
            }),
            3 => take.snapshot.program_changes.push(MidiProgramChangeData {
                channel,
                tick,
                program: data1_u8()?,
            }),
            4 => take
                .snapshot
                .poly_aftertouches
                .push(MidiPolyAftertouchData {
                    channel,
                    tick,
                    note: data1_u8()?,
                    value: value_u8()?,
                }),
            5 => take
                .snapshot
                .channel_pressures
                .push(MidiChannelPressureData {
                    channel,
                    tick,
                    value: value_u8()?,
                }),
            6 => {
                let event_kind = target_kind.ok_or(StorageError::InvalidStoredData(
                    "MIDI opaque take event has no kind",
                ))?;
                let data = payload.ok_or(StorageError::InvalidStoredData(
                    "MIDI opaque take event has no payload",
                ))?;
                let kind = match event_kind {
                    0 => MidiOpaqueEventKind::Meta {
                        kind: data1_u8()?,
                        data,
                    },
                    1 => MidiOpaqueEventKind::SystemExclusive { status: 0xF0, data },
                    2 => MidiOpaqueEventKind::SystemExclusive { status: 0xF7, data },
                    _ => {
                        return Err(StorageError::InvalidStoredData(
                            "invalid MIDI opaque take event kind",
                        ));
                    }
                };
                take.snapshot.opaque_events.push(MidiOpaqueEventData {
                    tick,
                    order: source_order,
                    kind,
                });
            }
            7 => {
                let target_kind = target_kind.ok_or(StorageError::InvalidStoredData(
                    "MIDI take event order has no target kind",
                ))?;
                let target = match target_kind {
                    0 => MidiEventOrderTarget::Note(MidiNoteData {
                        pitch: from_sql_u8(data1)?,
                        channel,
                        tick,
                        duration: from_sql_u64(duration)?,
                        velocity: from_sql_u8(data2)?,
                        off_velocity: from_sql_u8(data3)?,
                    }),
                    1 => MidiEventOrderTarget::Controller(MidiControllerData {
                        controller: data1_u8()?,
                        channel,
                        tick,
                        value: from_sql_u8(data2)?,
                    }),
                    2 => MidiEventOrderTarget::PitchBend(MidiPitchBendData {
                        channel,
                        tick,
                        value: from_sql_u16(value)?,
                    }),
                    3 => MidiEventOrderTarget::ProgramChange(MidiProgramChangeData {
                        channel,
                        tick,
                        program: data1_u8()?,
                    }),
                    4 => MidiEventOrderTarget::PolyAftertouch(MidiPolyAftertouchData {
                        channel,
                        tick,
                        note: data1_u8()?,
                        value: value_u8()?,
                    }),
                    5 => MidiEventOrderTarget::ChannelPressure(MidiChannelPressureData {
                        channel,
                        tick,
                        value: value_u8()?,
                    }),
                    _ => {
                        return Err(StorageError::InvalidStoredData(
                            "invalid MIDI take event-order target",
                        ));
                    }
                };
                take.snapshot.event_order.push(MidiEventOrderData {
                    target,
                    order: source_order,
                    note_off_order: note_off_order.map(from_sql_u32).transpose()?,
                });
            }
            _ => {
                return Err(StorageError::InvalidStoredData(
                    "invalid MIDI take event kind",
                ));
            }
        }
    }

    let mut statement = connection
        .prepare("SELECT item_id, active_take_index FROM midi_item_take_state ORDER BY item_id")?;
    let state_rows = statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut active_indices = HashMap::with_capacity(state_rows.len());
    for (item_id, index) in state_rows {
        active_indices.insert(from_sql_u64(item_id)?, from_sql_u32(index)?);
    }

    let mut result = HashMap::new();
    for (item_id, mut takes) in takes_by_item {
        takes.sort_by_key(|take| take.snapshot.index);
        let active_index =
            active_indices
                .remove(&item_id)
                .ok_or(StorageError::InvalidStoredData(
                    "MIDI alternate takes have no active-take state",
                ))?;
        result.insert(
            item_id,
            (
                active_index,
                takes.into_iter().map(|take| take.snapshot).collect(),
            ),
        );
    }
    for (item_id, active_index) in active_indices {
        result.insert(item_id, (active_index, Vec::new()));
    }
    Ok(result)
}

fn read_midi_items(connection: &Connection) -> Result<(Vec<MidiItemSnapshot>, bool), StorageError> {
    let mut takes_by_item = read_midi_takes(connection)?;
    let mut note_statement = connection.prepare(
        "SELECT item_id, id, position, pitch, channel, tick, duration, velocity, off_velocity \
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
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut notes_by_item: HashMap<u64, Vec<(u64, MidiNoteSnapshot)>> = HashMap::new();
    for (item_id, note_id, position, pitch, channel, tick, duration, velocity, off_velocity) in
        note_rows
    {
        let note = MidiNoteSnapshot {
            id: from_sql_u64(note_id)?,
            data: MidiNoteData {
                channel: from_sql_u8(channel)?,
                pitch: from_sql_u8(pitch)?,
                tick: from_sql_u64(tick)?,
                duration: from_sql_u64(duration)?,
                velocity: from_sql_u8(velocity)?,
                off_velocity: from_sql_u8(off_velocity)?,
            },
        };
        notes_by_item
            .entry(from_sql_u64(item_id)?)
            .or_default()
            .push((from_sql_u64(position)?, note));
    }

    let mut controller_statement = connection.prepare(
        "SELECT item_id, position, controller, channel, tick, value \
         FROM midi_controllers ORDER BY item_id, position",
    )?;
    let controller_rows = controller_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut controllers_by_item: HashMap<u64, Vec<(u64, MidiControllerData)>> = HashMap::new();
    for (item_id, position, controller, channel, tick, value) in controller_rows {
        controllers_by_item
            .entry(from_sql_u64(item_id)?)
            .or_default()
            .push((
                from_sql_u64(position)?,
                MidiControllerData {
                    channel: from_sql_u8(channel)?,
                    controller: from_sql_u8(controller)?,
                    tick: from_sql_u64(tick)?,
                    value: from_sql_u8(value)?,
                },
            ));
    }

    let mut bend_statement = connection.prepare(
        "SELECT item_id, position, channel, tick, value \
         FROM midi_pitch_bends ORDER BY item_id, position",
    )?;
    let bend_rows = bend_statement
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
    let mut bends_by_item: HashMap<u64, Vec<(u64, MidiPitchBendData)>> = HashMap::new();
    for (item_id, position, channel, tick, value) in bend_rows {
        bends_by_item
            .entry(from_sql_u64(item_id)?)
            .or_default()
            .push((
                from_sql_u64(position)?,
                MidiPitchBendData {
                    channel: from_sql_u8(channel)?,
                    tick: from_sql_u64(tick)?,
                    value: from_sql_u16(value)?,
                },
            ));
    }

    let mut program_statement = connection.prepare(
        "SELECT item_id, position, channel, tick, program \
         FROM midi_program_changes ORDER BY item_id, position",
    )?;
    let program_rows = program_statement
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
    let mut programs_by_item: HashMap<u64, Vec<(u64, MidiProgramChangeData)>> = HashMap::new();
    for (item_id, position, channel, tick, program) in program_rows {
        programs_by_item
            .entry(from_sql_u64(item_id)?)
            .or_default()
            .push((
                from_sql_u64(position)?,
                MidiProgramChangeData {
                    channel: from_sql_u8(channel)?,
                    tick: from_sql_u64(tick)?,
                    program: from_sql_u8(program)?,
                },
            ));
    }

    let mut poly_statement = connection.prepare(
        "SELECT item_id, position, channel, tick, note, value \
         FROM midi_poly_aftertouches ORDER BY item_id, position",
    )?;
    let poly_rows = poly_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut poly_by_item: HashMap<u64, Vec<(u64, MidiPolyAftertouchData)>> = HashMap::new();
    for (item_id, position, channel, tick, note, value) in poly_rows {
        poly_by_item
            .entry(from_sql_u64(item_id)?)
            .or_default()
            .push((
                from_sql_u64(position)?,
                MidiPolyAftertouchData {
                    channel: from_sql_u8(channel)?,
                    tick: from_sql_u64(tick)?,
                    note: from_sql_u8(note)?,
                    value: from_sql_u8(value)?,
                },
            ));
    }

    let mut pressure_statement = connection.prepare(
        "SELECT item_id, position, channel, tick, value \
         FROM midi_channel_pressures ORDER BY item_id, position",
    )?;
    let pressure_rows = pressure_statement
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
    let mut pressures_by_item: HashMap<u64, Vec<(u64, MidiChannelPressureData)>> = HashMap::new();
    for (item_id, position, channel, tick, value) in pressure_rows {
        pressures_by_item
            .entry(from_sql_u64(item_id)?)
            .or_default()
            .push((
                from_sql_u64(position)?,
                MidiChannelPressureData {
                    channel: from_sql_u8(channel)?,
                    tick: from_sql_u64(tick)?,
                    value: from_sql_u8(value)?,
                },
            ));
    }

    let mut opaque_statement = connection.prepare(
        "SELECT item_id, position, tick, source_order, event_kind, meta_type, data \
         FROM midi_opaque_events ORDER BY item_id, position",
    )?;
    let opaque_rows = opaque_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, Vec<u8>>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut opaque_by_item: HashMap<u64, Vec<(u64, MidiOpaqueEventData)>> = HashMap::new();
    for (item_id, position, tick, source_order, event_kind, meta_type, data) in opaque_rows {
        let kind = match (event_kind, meta_type) {
            (0, Some(meta_type)) => MidiOpaqueEventKind::Meta {
                kind: from_sql_u8(meta_type)?,
                data,
            },
            (1, None) => MidiOpaqueEventKind::SystemExclusive { status: 0xF0, data },
            (2, None) => MidiOpaqueEventKind::SystemExclusive { status: 0xF7, data },
            _ => {
                return Err(StorageError::InvalidStoredData(
                    "invalid MIDI opaque event kind",
                ));
            }
        };
        opaque_by_item
            .entry(from_sql_u64(item_id)?)
            .or_default()
            .push((
                from_sql_u64(position)?,
                MidiOpaqueEventData {
                    tick: from_sql_u64(tick)?,
                    order: from_sql_u32(source_order)?,
                    kind,
                },
            ));
    }

    let mut order_statement = connection.prepare(
        "SELECT item_id, position, source_order, event_kind, tick, channel, data1, data2, data3, duration, note_off_order \
         FROM midi_event_order ORDER BY item_id, position",
    )?;
    let order_rows = order_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, Option<i64>>(10)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut event_order_by_item: HashMap<u64, Vec<(u64, MidiEventOrderData)>> = HashMap::new();
    for (
        item_id,
        position,
        source_order,
        event_kind,
        tick,
        channel,
        data1,
        data2,
        data3,
        duration,
        note_off_order,
    ) in order_rows
    {
        let tick = from_sql_u64(tick)?;
        let channel = from_sql_u8(channel)?;
        let data2 = from_sql_u8(data2)?;
        let data3 = from_sql_u8(data3)?;
        let duration = from_sql_u64(duration)?;
        let target = match event_kind {
            0 => MidiEventOrderTarget::Note(MidiNoteData {
                pitch: u8::try_from(data1).map_err(|_| {
                    StorageError::InvalidStoredData("invalid MIDI note order pitch")
                })?,
                channel,
                tick,
                duration,
                velocity: data2,
                off_velocity: data3,
            }),
            1 => MidiEventOrderTarget::Controller(MidiControllerData {
                controller: u8::try_from(data1).map_err(|_| {
                    StorageError::InvalidStoredData("invalid MIDI controller order number")
                })?,
                channel,
                tick,
                value: data2,
            }),
            2 => MidiEventOrderTarget::PitchBend(MidiPitchBendData {
                channel,
                tick,
                value: u16::try_from(data1).map_err(|_| {
                    StorageError::InvalidStoredData("invalid MIDI pitch-bend order value")
                })?,
            }),
            3 => MidiEventOrderTarget::ProgramChange(MidiProgramChangeData {
                channel,
                tick,
                program: u8::try_from(data1).map_err(|_| {
                    StorageError::InvalidStoredData("invalid MIDI Program Change order value")
                })?,
            }),
            4 => MidiEventOrderTarget::PolyAftertouch(MidiPolyAftertouchData {
                channel,
                tick,
                note: u8::try_from(data1).map_err(|_| {
                    StorageError::InvalidStoredData("invalid MIDI poly-aftertouch order note")
                })?,
                value: data2,
            }),
            5 => MidiEventOrderTarget::ChannelPressure(MidiChannelPressureData {
                channel,
                tick,
                value: u8::try_from(data1).map_err(|_| {
                    StorageError::InvalidStoredData("invalid MIDI channel-pressure order value")
                })?,
            }),
            _ => {
                return Err(StorageError::InvalidStoredData(
                    "invalid MIDI event order kind",
                ));
            }
        };
        event_order_by_item
            .entry(from_sql_u64(item_id)?)
            .or_default()
            .push((
                from_sql_u64(position)?,
                MidiEventOrderData {
                    target,
                    order: from_sql_u32(source_order)?,
                    note_off_order: note_off_order.map(from_sql_u32).transpose()?,
                },
            ));
    }

    let mut item_statement = connection.prepare(
        "SELECT id, track_id, position, start_tick, start_tick_subtick, length_ticks, name, source_offset_ticks, loop_source, source_loop_length_ticks, muted, active_take_name_is_empty, source_offset_negative \
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
                row.get::<_, i64>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, i64>(11)?,
                row.get::<_, i64>(12)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut items = Vec::with_capacity(item_rows.len());
    for (
        id,
        track_id,
        position,
        start_tick,
        start_tick_subtick,
        length_ticks,
        name,
        source_offset_ticks,
        loop_source,
        source_loop_length_ticks,
        muted,
        active_take_name_is_empty,
        source_offset_negative,
    ) in item_rows
    {
        let muted = match muted {
            0 => false,
            1 => true,
            _ => return Err(StorageError::Snapshot(SnapshotError::InvalidProjectData)),
        };
        let loop_source = match loop_source {
            0 => false,
            1 => true,
            _ => return Err(StorageError::Snapshot(SnapshotError::InvalidProjectData)),
        };
        let id = from_sql_u64(id)?;
        let _ = from_sql_u64(position)?;
        let notes = notes_by_item.remove(&id).unwrap_or_default();
        let controllers = controllers_by_item.remove(&id).unwrap_or_default();
        let pitch_bends = bends_by_item.remove(&id).unwrap_or_default();
        let program_changes = programs_by_item.remove(&id).unwrap_or_default();
        let poly_aftertouches = poly_by_item.remove(&id).unwrap_or_default();
        let channel_pressures = pressures_by_item.remove(&id).unwrap_or_default();
        let opaque_events = opaque_by_item.remove(&id).unwrap_or_default();
        let event_order = event_order_by_item.remove(&id).unwrap_or_default();
        let name = match active_take_name_is_empty {
            0 => name,
            1 if name == EMPTY_ACTIVE_MIDI_TAKE_NAME
                && notes.is_empty()
                && controllers.is_empty()
                && pitch_bends.is_empty()
                && program_changes.is_empty()
                && poly_aftertouches.is_empty()
                && channel_pressures.is_empty()
                && opaque_events.is_empty()
                && event_order.is_empty() =>
            {
                String::new()
            }
            1 => {
                return Err(StorageError::InvalidStoredData(
                    "invalid empty active MIDI Take name marker",
                ));
            }
            _ => {
                return Err(StorageError::InvalidStoredData(
                    "invalid empty active MIDI Take flag",
                ));
            }
        };
        let (active_take_index, other_takes) = match takes_by_item.remove(&id) {
            Some((active_index, other_takes)) => (active_index, other_takes),
            None => (0, Vec::new()),
        };
        items.push(MidiItemSnapshot {
            id,
            track_id: from_sql_u64(track_id)?,
            name,
            active_take_index,
            other_takes,
            start_tick: from_sql_u64(start_tick)?,
            start_tick_subtick: from_sql_u8(start_tick_subtick)?,
            source_offset_ticks: from_sql_signed_midi_offset(
                source_offset_ticks,
                source_offset_negative,
            )?,
            loop_source,
            source_loop_length_ticks: from_sql_u64(source_loop_length_ticks)?,
            length_ticks: from_sql_u64(length_ticks)?,
            muted,
            notes: notes.into_iter().map(|(_, note)| note).collect(),
            controllers: controllers.into_iter().map(|(_, event)| event).collect(),
            pitch_bends: pitch_bends.into_iter().map(|(_, event)| event).collect(),
            program_changes: program_changes
                .into_iter()
                .map(|(_, event)| event)
                .collect(),
            poly_aftertouches: poly_aftertouches
                .into_iter()
                .map(|(_, event)| event)
                .collect(),
            channel_pressures: channel_pressures
                .into_iter()
                .map(|(_, event)| event)
                .collect(),
            opaque_events: opaque_events.into_iter().map(|(_, event)| event).collect(),
            event_order: event_order.into_iter().map(|(_, event)| event).collect(),
        });
    }
    Ok((
        items,
        !notes_by_item.is_empty()
            || !controllers_by_item.is_empty()
            || !bends_by_item.is_empty()
            || !programs_by_item.is_empty()
            || !poly_by_item.is_empty()
            || !pressures_by_item.is_empty()
            || !opaque_by_item.is_empty()
            || !event_order_by_item.is_empty()
            || !takes_by_item.is_empty(),
    ))
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

fn read_project_markers(
    connection: &Connection,
) -> Result<Vec<ProjectMarkerSnapshot>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT id, marker_number, position_ticks, name, visible FROM project_markers ORDER BY position_ticks, id",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(id, number, position_ticks, name, visible)| {
            let visible = match visible {
                0 => false,
                1 => true,
                _ => {
                    return Err(StorageError::InvalidStoredData(
                        "unsupported project marker visibility",
                    ));
                }
            };
            Ok(ProjectMarkerSnapshot {
                id: from_sql_u64(id)?,
                number: from_sql_u32(number)?,
                position_ticks: from_sql_u64(position_ticks)?,
                name,
                visible,
            })
        })
        .collect()
}

fn read_project_regions(
    connection: &Connection,
) -> Result<Vec<ProjectRegionSnapshot>, StorageError> {
    let mut render_tracks_by_region = HashMap::<u64, Vec<u64>>::new();
    {
        let mut statement = connection.prepare(
            "SELECT region_id, track_id FROM project_region_render_tracks ORDER BY region_id, position",
        )?;
        let rows =
            statement.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?;
        for row in rows {
            let (region_id, track_id) = row?;
            render_tracks_by_region
                .entry(from_sql_u64(region_id)?)
                .or_default()
                .push(from_sql_u64(track_id)?);
        }
    }
    let mut statement = connection.prepare(
        "SELECT id, region_number, start_ticks, end_ticks, name, visible, render_master_mix FROM project_regions ORDER BY start_ticks, id",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(
            |(id, number, start_ticks, end_ticks, name, visible, render_master_mix)| {
                let visible = match visible {
                    0 => false,
                    1 => true,
                    _ => {
                        return Err(StorageError::InvalidStoredData(
                            "unsupported project region visibility",
                        ));
                    }
                };
                let render_master_mix = match render_master_mix {
                    0 => false,
                    1 => true,
                    _ => {
                        return Err(StorageError::InvalidStoredData(
                            "unsupported project region Master render assignment",
                        ));
                    }
                };
                let id = from_sql_u64(id)?;
                Ok(ProjectRegionSnapshot {
                    id,
                    number: from_sql_u32(number)?,
                    start_ticks: from_sql_u64(start_ticks)?,
                    end_ticks: from_sql_u64(end_ticks)?,
                    name,
                    visible,
                    render_master_mix,
                    render_track_ids: render_tracks_by_region.remove(&id).unwrap_or_default(),
                })
            },
        )
        .collect()
}

fn tempo_curve_to_sql(curve: TempoCurve) -> i64 {
    match curve {
        TempoCurve::Step => 0,
        TempoCurve::Linear => 1,
        TempoCurve::Logarithmic => 2,
        TempoCurve::Bézier => 3,
    }
}

fn tempo_curve_from_sql(value: i64) -> Result<TempoCurve, StorageError> {
    match value {
        0 => Ok(TempoCurve::Step),
        1 => Ok(TempoCurve::Linear),
        2 => Ok(TempoCurve::Logarithmic),
        3 => Ok(TempoCurve::Bézier),
        _ => Err(StorageError::InvalidStoredData("unknown tempo curve")),
    }
}

fn to_sql_integer(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::IntegerOutOfRange(value))
}

fn signed_midi_offset_to_sql(value: i64) -> Result<(i64, i64), StorageError> {
    let magnitude = value.checked_abs().ok_or(StorageError::InvalidStoredData(
        "MIDI source offset is out of range",
    ))?;
    Ok((magnitude, i64::from(value < 0)))
}

fn from_sql_signed_midi_offset(magnitude: i64, negative: i64) -> Result<i64, StorageError> {
    if magnitude < 0 {
        return Err(StorageError::InvalidStoredData(
            "invalid MIDI source offset magnitude",
        ));
    }
    match negative {
        0 => Ok(magnitude),
        1 if magnitude > 0 => Ok(-magnitude),
        1 => Err(StorageError::InvalidStoredData(
            "invalid negative zero MIDI source offset",
        )),
        _ => Err(StorageError::InvalidStoredData(
            "invalid MIDI source offset sign",
        )),
    }
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

fn from_sql_u16(value: i64) -> Result<u16, StorageError> {
    u16::try_from(value).map_err(|_| StorageError::InvalidStoredData("integer is out of range"))
}

fn from_sql_u8(value: i64) -> Result<u8, StorageError> {
    u8::try_from(value).map_err(|_| StorageError::InvalidStoredData("MIDI value is out of range"))
}

#[cfg(test)]
mod audio_asset_copy_tests {
    use super::{ProjectStore, ResolvedAudioAsset};
    use std::io::{Cursor, Read};

    #[test]
    fn copying_audio_assets_preserves_embedded_content_and_external_links() {
        let directory = tempfile::tempdir().unwrap();
        let source_path = directory.path().join("unsaved.aaadaw");
        let destination_path = directory.path().join("saved.aaadaw");
        let linked_path = directory.path().join("linked.wav");
        std::fs::write(&linked_path, b"external audio").unwrap();

        let mut source = ProjectStore::open(&source_path).unwrap();
        source
            .import_audio_asset(
                "asset://embedded",
                "take.wav",
                Cursor::new(b"embedded audio"),
            )
            .unwrap();
        let linked_ref = source.link_external_audio_file(&linked_path).unwrap();

        let mut destination = ProjectStore::open(&destination_path).unwrap();
        destination.copy_audio_assets_from(&source).unwrap();

        let ResolvedAudioAsset::Embedded(mut reader) =
            destination.resolve_audio_asset("asset://embedded").unwrap()
        else {
            panic!("embedded asset should remain embedded");
        };
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"embedded audio");
        drop(reader);

        let ResolvedAudioAsset::LinkedFile {
            path,
            original_name,
        } = destination.resolve_audio_asset(&linked_ref).unwrap()
        else {
            panic!("linked asset should remain linked");
        };
        assert_eq!(path, linked_path.canonicalize().unwrap());
        assert_eq!(original_name, "linked.wav");

        destination.close().unwrap();
        source.close().unwrap();
    }
}

#[cfg(test)]
mod saved_snapshot_tests {
    use super::ProjectStore;
    use aaadaw_core::Project;

    #[test]
    fn empty_session_store_is_distinguishable_from_a_recoverable_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("session.aaadaw");
        let mut store = ProjectStore::open(&path).unwrap();

        assert!(!store.has_saved_snapshot().unwrap());
        store.save(&Project::new()).unwrap();
        assert!(store.has_saved_snapshot().unwrap());

        store.close().unwrap();
    }
}

#[cfg(test)]
mod loop_source_storage_tests {
    use super::ProjectStore;
    use aaadaw_core::{DawAction, MidiItemPosition, MidiNoteData, Project};
    use rusqlite::Connection;

    #[test]
    fn audio_item_name_round_trips_and_schema_53_migrates() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("audio-item-name.aaadaw");
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Audio".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertAudioItem {
                track_id: project.tracks()[0].id(),
                media_ref: "asset://source.wav".to_owned(),
                start_sample: 0,
                source_offset_samples: 0,
                length_samples: 4_800,
            })
            .unwrap();
        let item_id = project.audio_items()[0].id();
        project
            .apply(DawAction::SetAudioItemName {
                item_id,
                name: "Verse guitar".to_owned(),
            })
            .unwrap();

        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&project).unwrap();
        assert_eq!(
            store.load().unwrap().audio_items()[0].name(),
            "Verse guitar"
        );
        store
            .connection
            .execute_batch("ALTER TABLE audio_items DROP COLUMN name; PRAGMA user_version = 53;")
            .unwrap();
        store.close().unwrap();

        let store = ProjectStore::open(&path).unwrap();
        assert_eq!(
            store.schema_version().unwrap(),
            super::CURRENT_SCHEMA_VERSION
        );
        assert_eq!(store.load().unwrap().audio_items()[0].name(), "");
    }

    #[test]
    fn audio_take_playback_rate_round_trips_and_schema_56_migrates_to_unity() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("audio-take-rate.aaadaw");
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Audio".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertAudioItem {
                track_id: project.tracks()[0].id(),
                media_ref: "asset://source.wav".to_owned(),
                start_sample: 0,
                source_offset_samples: 0,
                length_samples: 48_000,
            })
            .unwrap();
        let item_id = project.audio_items()[0].id();
        project
            .apply(DawAction::SetAudioItemTakePlaybackRate {
                item_id,
                rate: 1.25,
            })
            .unwrap();

        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&project).unwrap();
        assert_eq!(
            store.load().unwrap().audio_items()[0].take_playback_rate(),
            1.25
        );
        store
            .connection
            .execute_batch(
                "ALTER TABLE audio_items DROP COLUMN take_playback_rate; PRAGMA user_version=56;",
            )
            .unwrap();
        store.close().unwrap();

        let store = ProjectStore::open(&path).unwrap();
        assert_eq!(
            store.schema_version().unwrap(),
            super::CURRENT_SCHEMA_VERSION
        );
        assert_eq!(
            store.load().unwrap().audio_items()[0].take_playback_rate(),
            1.0
        );
    }

    #[test]
    fn loop_source_metadata_round_trips_with_audio_items() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("loop-source.aaadaw");
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Loop source".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertLoopedAudioItem {
                track_id: project.tracks()[0].id(),
                media_ref: "asset://loop-source".to_owned(),
                start_sample: 10,
                source_offset_samples: 120,
                phase_inverted: false,
                source_section_start_samples: 100,
                length_samples: 4_800,
                muted: false,
                source_section_length_samples: 2_400,
            })
            .unwrap();
        let item_id = project.audio_items()[0].id();
        project
            .apply(DawAction::SetAudioItemLoopSource {
                item_id,
                enabled: false,
            })
            .unwrap();
        project
            .apply(DawAction::SetAudioItemMute {
                item_id,
                muted: true,
            })
            .unwrap();

        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&project).unwrap();
        store.close().unwrap();

        let store = ProjectStore::open(&path).unwrap();
        let loaded = store.load().unwrap();
        let item = &loaded.audio_items()[0];
        assert!(!item.is_loop_source());
        assert!(item.is_muted());
        assert_eq!(item.source_section_start_samples(), Some(100));
        assert_eq!(item.source_section_length_samples(), Some(2_400));
    }

    #[test]
    fn schema_43_audio_items_migrate_with_mute_disabled() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("schema-43-item-mute.aaadaw");
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Legacy".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertAudioItem {
                track_id: project.tracks()[0].id(),
                media_ref: "asset://legacy-mute".to_owned(),
                start_sample: 0,
                source_offset_samples: 0,
                length_samples: 4_800,
            })
            .unwrap();

        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&project).unwrap();
        store
            .connection
            .execute_batch(
                "ALTER TABLE audio_items DROP COLUMN muted;\
                 PRAGMA user_version = 43;",
            )
            .unwrap();
        store.close().unwrap();

        let store = ProjectStore::open(&path).unwrap();
        assert_eq!(
            store.schema_version().unwrap(),
            super::CURRENT_SCHEMA_VERSION
        );
        let loaded = store.load().unwrap();
        assert!(!loaded.audio_items()[0].is_muted());
    }

    #[test]
    fn midi_item_mute_round_trips_and_schema_44_defaults_to_unmuted() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("midi-item-mute.aaadaw");
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "MIDI".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertMidiItem {
                track_id: project.tracks()[0].id(),
                start_tick: 0,
                length_ticks: 960,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        project
            .apply(DawAction::SetMidiItemMute {
                item_id,
                muted: true,
            })
            .unwrap();
        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&project).unwrap();
        store
            .connection
            .execute_batch("ALTER TABLE items DROP COLUMN muted; PRAGMA user_version = 44;")
            .unwrap();
        store.close().unwrap();

        let store = ProjectStore::open(&path).unwrap();
        let loaded = store.load().unwrap();
        assert!(!loaded.midi_items()[0].is_muted());
        assert_eq!(
            store.schema_version().unwrap(),
            super::CURRENT_SCHEMA_VERSION
        );
    }

    #[test]
    fn schema_41_audio_items_migrate_with_looping_disabled() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("schema-41.aaadaw");
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Legacy".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertAudioItem {
                track_id: project.tracks()[0].id(),
                media_ref: "asset://legacy".to_owned(),
                start_sample: 0,
                source_offset_samples: 240,
                length_samples: 4_800,
            })
            .unwrap();

        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&project).unwrap();
        store
            .connection
            .execute_batch(
                "ALTER TABLE audio_items DROP COLUMN source_section_start_samples;\
                 ALTER TABLE audio_items DROP COLUMN source_section_length_samples;\
                 ALTER TABLE audio_items DROP COLUMN loop_source;\
                 PRAGMA user_version = 41;",
            )
            .unwrap();
        store.close().unwrap();

        let store = ProjectStore::open(&path).unwrap();
        assert_eq!(
            store.schema_version().unwrap(),
            super::CURRENT_SCHEMA_VERSION
        );
        let loaded = store.load().unwrap();
        let item = &loaded.audio_items()[0];
        assert!(!item.is_loop_source());
        assert_eq!(item.source_section_length_samples(), None);
    }

    #[test]
    fn schema_42_loop_source_migration_sets_section_start_to_existing_offset() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("schema-42-loop-source.aaadaw");
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Legacy loop".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertLoopedAudioItem {
                track_id: project.tracks()[0].id(),
                media_ref: "asset://legacy-loop".to_owned(),
                start_sample: 0,
                source_offset_samples: 240,
                phase_inverted: false,
                source_section_start_samples: 200,
                length_samples: 4_800,
                muted: false,
                source_section_length_samples: 1_200,
            })
            .unwrap();

        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&project).unwrap();
        store
            .connection
            .execute_batch(
                "ALTER TABLE audio_items DROP COLUMN source_section_start_samples;\
                 PRAGMA user_version = 42;",
            )
            .unwrap();
        store.close().unwrap();

        let store = ProjectStore::open(&path).unwrap();
        assert_eq!(
            store.schema_version().unwrap(),
            super::CURRENT_SCHEMA_VERSION
        );
        let loaded = store.load().unwrap();
        let item = &loaded.audio_items()[0];
        assert!(item.is_loop_source());
        assert_eq!(item.source_section_start_samples(), Some(240));
        assert_eq!(item.source_section_length_samples(), Some(1_200));
        assert_eq!(item.source_offset_samples(), 0);
    }

    #[test]
    fn schema_49_signed_source_offset_migrates_and_round_trips_negative_offsets() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("schema-49-signed-source-offset.aaadaw");
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Signed source".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertAudioItem {
                track_id: project.tracks()[0].id(),
                media_ref: "asset://signed-source".to_owned(),
                start_sample: 0,
                source_offset_samples: 120,
                length_samples: 4_800,
            })
            .unwrap();

        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&project).unwrap();
        drop(store);

        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "ALTER TABLE audio_items DROP COLUMN signed_source_offset_samples;
                 PRAGMA user_version = 48;",
            )
            .unwrap();
        drop(connection);

        let mut store = ProjectStore::open(&path).unwrap();
        assert_eq!(
            store.schema_version().unwrap(),
            super::CURRENT_SCHEMA_VERSION
        );
        let mut migrated = store.load().unwrap();
        assert_eq!(migrated.audio_items()[0].source_offset_samples(), 120);
        let migrated_columns: (i64, i64) = store
            .connection
            .query_row(
                "SELECT source_offset_samples, signed_source_offset_samples FROM audio_items",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(migrated_columns, (120, 120));

        let item = migrated.audio_items()[0].clone();
        migrated
            .apply(DawAction::SetAudioItemLoopSource {
                item_id: item.id(),
                enabled: false,
            })
            .unwrap();
        migrated
            .apply(DawAction::EditAudioItem {
                item_id: item.id(),
                media_ref: item.media_ref().to_owned(),
                start_sample: item.start_sample(),
                source_offset_samples: -7,
                length_samples: item.length_samples(),
            })
            .unwrap();
        store.save(&migrated).unwrap();
        drop(store);

        let store = ProjectStore::open(&path).unwrap();
        assert_eq!(
            store.load().unwrap().audio_items()[0].source_offset_samples(),
            -7
        );
        let negative_columns: (i64, i64) = store
            .connection
            .query_row(
                "SELECT source_offset_samples, signed_source_offset_samples FROM audio_items",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(negative_columns, (0, -7));
    }

    #[test]
    fn schema_50_makes_bounded_source_offsets_relative_to_the_section() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bounded-offset-v49.aaadaw");
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Bounded source".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertLoopedAudioItem {
                track_id: project.tracks()[0].id(),
                media_ref: "asset://bounded-source".to_owned(),
                start_sample: 0,
                source_offset_samples: 4_800,
                length_samples: 48_000,
                muted: false,
                phase_inverted: false,
                source_section_start_samples: 144_000,
                source_section_length_samples: 144_000,
            })
            .unwrap();

        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&project).unwrap();
        store
            .connection
            .execute_batch(
                "UPDATE audio_items
                 SET source_offset_samples = 148800,
                     signed_source_offset_samples = 148800;
                 PRAGMA user_version = 49;",
            )
            .unwrap();
        store.close().unwrap();

        let store = ProjectStore::open(&path).unwrap();
        assert_eq!(
            store.schema_version().unwrap(),
            super::CURRENT_SCHEMA_VERSION
        );
        let loaded = store.load().unwrap();
        let item = &loaded.audio_items()[0];
        assert_eq!(item.source_offset_samples(), 4_800);
        assert_eq!(item.source_section_start_samples(), Some(144_000));
        assert_eq!(item.source_section_length_samples(), Some(144_000));
        let columns: (i64, i64) = store
            .connection
            .query_row(
                "SELECT source_offset_samples, signed_source_offset_samples FROM audio_items",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(columns, (4_800, 4_800));
    }

    #[test]
    fn midi_loop_metadata_round_trips_through_snapshots_and_sqlite() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("midi-loop-source.aaadaw");
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "MIDI loop".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertMidiItem {
                track_id: project.tracks()[0].id(),
                start_tick: 0,
                length_ticks: 960,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        project
            .apply(DawAction::EditMidiItemPlacement {
                item_id,
                start_tick: 0,
                length_ticks: 1_500,
                source_offset_ticks: 240,
            })
            .unwrap();
        let exact_position = MidiItemPosition::new(1_080, 100).unwrap();
        project
            .apply(DawAction::SetMidiItemPosition {
                item_id,
                position: exact_position,
            })
            .unwrap();

        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&project).unwrap();
        store.close().unwrap();
        let store = ProjectStore::open(&path).unwrap();
        let loaded = store.load().unwrap();
        let item = &loaded.midi_items()[0];
        assert!(item.is_loop_source());
        assert_eq!(item.source_loop_length_ticks(), 960);
        assert_eq!(item.source_offset_ticks(), 240);
        assert_eq!(item.length_ticks(), 1_500);
        assert_eq!(item.start_position(), exact_position);
    }

    #[test]
    fn schema_55_midi_items_migrate_with_zero_fractional_ppq() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("midi-subtick-schema-55.aaadaw");
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Legacy MIDI".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertMidiItem {
                track_id: project.tracks()[0].id(),
                start_tick: 964,
                length_ticks: 960,
            })
            .unwrap();
        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&project).unwrap();
        store
            .connection
            .execute_batch(
                "ALTER TABLE items DROP COLUMN start_tick_subtick;
                 PRAGMA user_version = 55;",
            )
            .unwrap();
        store.close().unwrap();

        let store = ProjectStore::open(&path).unwrap();
        assert_eq!(
            store.schema_version().unwrap(),
            super::CURRENT_SCHEMA_VERSION
        );
        let loaded = store.load().unwrap();
        let item = &loaded.midi_items()[0];
        assert_eq!(item.start_tick(), 964);
        assert_eq!(item.start_position().subtick(), 0);
    }

    #[test]
    fn schema_50_midi_items_migrate_with_looping_and_event_extent() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("schema-50-midi-loop-source.aaadaw");
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Legacy MIDI".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertMidiItem {
                track_id: project.tracks()[0].id(),
                start_tick: 0,
                length_ticks: 1_300,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        project
            .apply(DawAction::AddMidiNotes {
                item_id,
                notes: vec![MidiNoteData {
                    channel: 0,
                    pitch: 60,
                    tick: 1_080,
                    duration: 120,
                    velocity: 100,
                    off_velocity: 0,
                }],
            })
            .unwrap();
        project
            .apply(DawAction::EditMidiItem {
                item_id,
                start_tick: 0,
                length_ticks: 480,
            })
            .unwrap();

        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&project).unwrap();
        store
            .connection
            .execute_batch(
                "ALTER TABLE items DROP COLUMN source_loop_length_ticks;
                 ALTER TABLE items DROP COLUMN loop_source;
                 UPDATE items SET length_ticks = 480;
                 PRAGMA user_version = 50;",
            )
            .unwrap();
        store.close().unwrap();

        let store = ProjectStore::open(&path).unwrap();
        let migrated = store.load().unwrap();
        let item = &migrated.midi_items()[0];
        assert!(item.is_loop_source());
        assert_eq!(item.source_loop_length_ticks(), 1_200);
        assert_eq!(item.length_ticks(), 480);
    }

    #[test]
    fn negative_offset_round_trips_with_disabled_bounded_source_section() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bounded-source-preroll.aaadaw");
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Bounded pre-roll".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertLoopedAudioItem {
                track_id: project.tracks()[0].id(),
                media_ref: "asset://bounded-preroll".to_owned(),
                start_sample: 0,
                source_offset_samples: 120,
                length_samples: 4_800,
                muted: false,
                phase_inverted: false,
                source_section_start_samples: 200,
                source_section_length_samples: 2_400,
            })
            .unwrap();
        let item_id = project.audio_items()[0].id();
        project
            .apply(DawAction::SetAudioItemLoopSource {
                item_id,
                enabled: false,
            })
            .unwrap();
        project
            .apply(DawAction::EditAudioItem {
                item_id,
                media_ref: "asset://bounded-preroll".to_owned(),
                start_sample: 0,
                source_offset_samples: -7,
                length_samples: 4_800,
            })
            .unwrap();

        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&project).unwrap();
        store.close().unwrap();
        let store = ProjectStore::open(&path).unwrap();
        let loaded = store.load().unwrap();
        let item = &loaded.audio_items()[0];
        assert!(!item.is_loop_source());
        assert_eq!(item.source_offset_samples(), -7);
        assert_eq!(item.source_section_start_samples(), Some(200));
        assert_eq!(item.source_section_length_samples(), Some(2_400));
    }
}

fn read_audio_sends(
    connection: &Connection,
) -> Result<HashMap<i64, Vec<aaadaw_core::AudioSendSnapshot>>, StorageError> {
    let mut statement = connection.prepare("SELECT id, source_track_id, destination_track_id, volume_db, pan, muted, phase_inverted, tap, midi_enabled FROM track_sends ORDER BY source_track_id, position")?;
    let mut rows = statement.query([])?;
    let mut sends: HashMap<i64, Vec<aaadaw_core::AudioSendSnapshot>> = HashMap::new();
    while let Some(row) = rows.next()? {
        sends
            .entry(row.get(1)?)
            .or_default()
            .push(aaadaw_core::AudioSendSnapshot {
                id: from_sql_u64(row.get(0)?)?,
                destination_track_id: from_sql_u64(row.get(2)?)?,
                parameters: aaadaw_core::AudioSendParameters {
                    volume_db: row.get::<_, f64>(3)? as f32,
                    pan: row.get::<_, f64>(4)? as f32,
                    muted: row.get(5)?,
                    phase_inverted: row.get(6)?,
                    midi_enabled: row.get(8)?,
                    tap: match row.get::<_, i64>(7)? {
                        0 => aaadaw_core::AudioSendTap::PostFader,
                        1 => aaadaw_core::AudioSendTap::PreFx,
                        3 => aaadaw_core::AudioSendTap::PreFader,
                        _ => return Err(StorageError::InvalidStoredData("invalid audio send tap")),
                    },
                },
            });
    }
    Ok(sends)
}
