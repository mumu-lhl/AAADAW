#[cfg(feature = "audio-device")]
use super::StereoPeakHold;
use super::commands::{self, CommandId, TrackCommand};
use super::fader;
use super::messages::SharedProjectSessionLock;
#[cfg(feature = "audio-device")]
use super::prepare_project_playback_file;
use super::project_io::{
    load_project_file, load_project_session, save_project_file,
    save_project_session_file_with_media,
};
use super::shortcut;
#[cfg(feature = "audio-device")]
use super::{ActiveRecording, SharedRecordingStart};
use super::{
    App, MainMenu, MainWorkspace, MenuNavigation, Message, MidiEditorTool, MobilePanel,
    PathPickerTarget, fx_chain_plugin_drag_event, keyboard_shortcut_event, menu_navigation_event,
    midi_editor_shortcut_event, midi_expression_context_menu_event, mobile_back_event,
    shortcut_message,
};
use crate::timeline::{SnapGrid, TimelineEvent};
#[cfg(all(feature = "jack-backend", feature = "pipewire-backend"))]
use aaadaw_app::PlaybackBackend;
use aaadaw_core::{
    DawAction, MidiControllerData, MidiNoteData, Project, RazorEditArea, TrackFxPlugin,
};
use aaadaw_media::{AudioStreamDecoder, AudioWaveform};
use aaadaw_storage::{ArrangementViewState, ProjectSessionLock, ProjectStore};
use iced::keyboard::{Key, Modifiers};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT_TEST_FILE: AtomicU64 = AtomicU64::new(0);

#[test]
fn transport_details_can_toggle_during_project_io() {
    let mut app = App {
        io_busy: true,
        ..App::default()
    };

    let _ = app.update(Message::ToggleTransportDetails);

    assert!(app.transport_details_open);
    let _ = app.update(Message::ToggleTransportDetails);
    assert!(!app.transport_details_open);
}

#[test]
fn mobile_midi_editor_uses_main_window_and_back_returns_to_arrangement() {
    let window_id = iced::window::Id::unique();
    let mut app = App {
        main_window_id: Some(window_id),
        midi_editor_window_id: Some(window_id),
        mobile_panel: MobilePanel::MidiEditor,
        mobile_panel_history: vec![MobilePanel::Editor],
        ..App::default()
    };

    let _ = app.update(Message::SelectMidiEditorTool(MidiEditorTool::Draw));
    assert_eq!(app.midi_editor_tool, MidiEditorTool::Draw);

    let _ = app.update(Message::CloseMidiEditor);

    assert_eq!(app.main_window_id, Some(window_id));
    assert_eq!(app.midi_editor_window_id, None);
    assert_eq!(app.mobile_panel, MobilePanel::Editor);
}

#[test]
#[cfg(feature = "audio-device")]
fn transport_details_can_open_while_playback_is_preparing() {
    let mut app = App::default();
    app.playback_busy = true;

    let _ = app.update(Message::ToggleTransportDetails);

    assert!(app.transport_details_open);
}

fn silent_test_waveform(sample_rate: u32, frame_count: u32) -> std::sync::Arc<AudioWaveform> {
    let data_len = frame_count * 2;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    bytes.resize(44 + data_len as usize, 0);
    let mut decoder = AudioStreamDecoder::from_reader(
        std::io::Cursor::new(bytes.clone()),
        Some(bytes.len() as u64),
        Some("wav"),
    )
    .unwrap();
    std::sync::Arc::new(AudioWaveform::decode(&mut decoder, 256).unwrap())
}

fn item_snap_test_app(audio_start_sample: u64, midi_start_tick: u64) -> App {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let tracks = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[0],
            media_ref: "asset://item-snap-test".to_owned(),
            start_sample: audio_start_sample,
            source_offset_samples: 120,
            length_samples: 24_000,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[1],
            start_tick: midi_start_tick,
            length_ticks: 960,
        })
        .unwrap();
    let audio_id = app.project.audio_items()[0].id();
    let midi_id = app.project.midi_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline
        .select_items(vec![(audio_id, tracks[0]), (midi_id, tracks[1])]);
    app.timeline.selected_track = Some(tracks[1]);
    app.timeline.selected_tracks = HashSet::from([tracks[0], tracks[1]]);
    app.timeline.edit_cursor_tick = 1_111;
    app
}

fn item_nudge_set_edge_test_app() -> (App, PathBuf) {
    let file_id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
    let project_path = std::env::temp_dir().join(format!(
        "aaadaw-ui-item-nudge-set-edges-{}-{file_id}.aaadaw",
        std::process::id()
    ));
    let media_ref = format!("asset://item-nudge-set-edges-{file_id}");
    let mut store = ProjectStore::open(&project_path).expect("project store should open");
    store
        .import_audio_asset(&media_ref, "set-edges.wav", std::io::Cursor::new([0_u8; 4]))
        .expect("test audio asset should import");
    store
        .set_audio_asset_metadata(
            &media_ref,
            &aaadaw_storage::AudioAssetMetadata {
                container: "wav".to_owned(),
                codec: "pcm".to_owned(),
                sample_rate: Some(44_100),
                channel_count: Some(1),
                bits_per_sample: Some(16),
                frame_count: Some(100_000),
                duration_nanos: None,
                byte_len: Some(4),
            },
        )
        .expect("test audio metadata should store");
    store.close().expect("project store should close");

    let mut app = App {
        project_path: Some(project_path.clone()),
        ..App::default()
    };
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let track_ids = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: track_ids[0],
            media_ref,
            start_sample: 96_000,
            source_offset_samples: 0,
            length_samples: 480_000,
        })
        .unwrap();
    let audio_id = app.project.audio_items()[0].id();
    app.project
        .apply(DawAction::SetAudioItemLoopSource {
            item_id: audio_id,
            enabled: true,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: track_ids[1],
            start_tick: 3_840,
            length_ticks: 19_200,
        })
        .unwrap();
    let midi_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::SetMidiItemLoopSource {
            item_id: midi_id,
            enabled: true,
        })
        .unwrap();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id: midi_id,
            notes: vec![MidiNoteData {
                pitch: 60,
                tick: 960,
                duration: 960,
                velocity: 96,
            }],
        })
        .unwrap();
    app.timeline.rebuild(&app.project);
    app.timeline
        .select_items(vec![(audio_id, track_ids[0]), (midi_id, track_ids[1])]);
    (app, project_path)
}

#[test]
fn item_nudge_moves_selected_audio_and_midi_together_and_undoes_as_one_edit() {
    let mut app = item_snap_test_app(0, 0);
    let audio_id = app.project.audio_items()[0].id();
    let midi_id = app.project.midi_items()[0].id();
    let original_revision = app.revision;
    let audio_source_offset = app.project.audio_items()[0].source_offset_samples();
    let audio_length = app.project.audio_items()[0].length_samples();
    let midi_length = app.project.midi_items()[0].length_ticks();
    let nudge_samples = (f64::from(app.project.settings().sample_rate()) / 1_000.0).round() as u64;
    let expected_midi_tick = app.project.tick_at_sample(nudge_samples).unwrap();

    let _ = app.update(Message::NudgeSelectedItems(1));

    assert_eq!(app.project.audio_items()[0].start_sample(), nudge_samples);
    assert_eq!(
        app.project.audio_items()[0].source_offset_samples(),
        audio_source_offset
    );
    assert_eq!(app.project.audio_items()[0].length_samples(), audio_length);
    assert_eq!(app.project.midi_items()[0].start_tick(), expected_midi_tick);
    assert_eq!(app.project.midi_items()[0].length_ticks(), midi_length);
    assert_eq!(app.revision, original_revision + 1);
    assert_eq!(app.timeline.selected_items.len(), 2);
    assert!(app.timeline.selected_items.contains(&audio_id));
    assert!(app.timeline.selected_items.contains(&midi_id));

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].start_sample(), 0);
    assert_eq!(app.project.midi_items()[0].start_tick(), 0);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items()[0].start_sample(), nudge_samples);
    assert_eq!(app.project.midi_items()[0].start_tick(), expected_midi_tick);

    let _ = app.update(Message::NudgeSelectedItems(-1));
    assert_eq!(app.project.audio_items()[0].start_sample(), 0);
    assert_eq!(app.project.midi_items()[0].start_tick(), 0);
}

#[test]
fn item_nudge_minutes_seconds_and_samples_units_match_native_snap_grids() {
    use super::item_nudge::{Target, Unit};

    let cases = [
        (Unit::Time, "0:00.250", false, 1, 72_960),
        (Unit::Time, "0:00.250", true, 1, 96_000),
        (Unit::Time, "0:00.250", true, -1, 48_000),
        (Unit::Samples, "125", false, 1, 61_085),
        (Unit::Samples, "125", true, 1, 61_085),
    ];

    for (unit, value, snap, direction, expected_sample) in cases {
        let mut app = item_snap_test_app(60_960, 5_242);
        let audio = &app.project.audio_items()[0];
        let audio_id = audio.id();
        let audio_track = audio.track_id();
        let original_offset = audio.source_offset_samples();
        let original_length = audio.length_samples();
        app.timeline.select_items(vec![(audio_id, audio_track)]);
        let original_selection = app.timeline.selected_items.clone();
        let revision_before = app.revision;

        let _ = app.update(Message::ItemNudgeUnitChanged(unit));
        let _ = app.update(Message::ItemNudgeTargetChanged(Target::Position));
        let _ = app.update(Message::ItemNudgeValueChanged(value.to_owned()));
        let _ = app.update(Message::ItemNudgeSnapToUnitChanged(snap));
        let _ = app.update(Message::NudgeSelectedItems(direction));

        let moved = &app.project.audio_items()[0];
        assert_eq!(moved.start_sample(), expected_sample, "{unit}, snap={snap}");
        assert_eq!(moved.source_offset_samples(), original_offset);
        assert_eq!(moved.length_samples(), original_length);
        assert_eq!(app.timeline.selected_items, original_selection);
        assert_eq!(app.revision, revision_before + 1);
        assert!(app.item_nudge_dialog.error.is_none(), "{unit}, snap={snap}");

        let _ = app.update(Message::Undo);
        assert_eq!(app.project.audio_items()[0].start_sample(), 60_960);
        assert_eq!(app.timeline.selected_items, original_selection);
        let _ = app.update(Message::Redo);
        assert_eq!(app.project.audio_items()[0].start_sample(), expected_sample);
    }
}

#[test]
fn item_nudge_grid_units_follow_arrangement_grid_and_native_snap_rounding() {
    use super::item_nudge::{Target, Unit};

    let cases = [(false, 1, 84_960), (true, 1, 96_000), (true, -1, 48_000)];
    for (snap, direction, expected_sample) in cases {
        let mut app = item_snap_test_app(60_960, 5_242);
        let item = &app.project.audio_items()[0];
        let item_id = item.id();
        let track_id = item.track_id();
        let original_offset = item.source_offset_samples();
        let original_length = item.length_samples();
        app.timeline.select_items(vec![(item_id, track_id)]);
        let selection = app.timeline.selected_items.clone();
        let revision_before = app.revision;

        let _ = app.update(Message::ItemNudgeUnitChanged(Unit::Grid));
        let _ = app.update(Message::ItemNudgeTargetChanged(Target::Position));
        let _ = app.update(Message::ItemNudgeValueChanged("1".to_owned()));
        let _ = app.update(Message::ItemNudgeSnapToUnitChanged(snap));
        let _ = app.update(Message::NudgeSelectedItems(direction));

        let moved = &app.project.audio_items()[0];
        assert_eq!(
            moved.start_sample(),
            expected_sample,
            "snap={snap}, direction={direction}"
        );
        assert_eq!(moved.source_offset_samples(), original_offset);
        assert_eq!(moved.length_samples(), original_length);
        assert_eq!(app.timeline.selected_items, selection);
        assert_eq!(app.revision, revision_before + 1);
        assert!(
            app.item_nudge_dialog.error.is_none(),
            "snap={snap}, direction={direction}"
        );

        let _ = app.update(Message::Undo);
        assert_eq!(app.project.audio_items()[0].start_sample(), 60_960);
        assert_eq!(app.timeline.selected_items, selection);
        let _ = app.update(Message::Redo);
        assert_eq!(app.project.audio_items()[0].start_sample(), expected_sample);
    }
}

#[test]
fn item_nudge_measures_beats_matches_native_bar_beat_and_snap_rounding() {
    use super::item_nudge::{Target, Unit};

    let cases = [
        ("1.0", false, 156_960),
        ("1.0", true, 192_000),
        ("0.1", false, 84_960),
    ];
    for (value, snap, expected_sample) in cases {
        let mut app = item_snap_test_app(60_960, 5_242);
        let item = &app.project.audio_items()[0];
        let item_id = item.id();
        let track_id = item.track_id();
        let original_offset = item.source_offset_samples();
        let original_length = item.length_samples();
        app.timeline.select_items(vec![(item_id, track_id)]);
        let selection = app.timeline.selected_items.clone();
        let revision_before = app.revision;

        let _ = app.update(Message::ItemNudgeUnitChanged(Unit::MeasuresBeats));
        assert_eq!(app.item_nudge_dialog.value, "1.0");
        let _ = app.update(Message::ItemNudgeTargetChanged(Target::Position));
        let _ = app.update(Message::ItemNudgeValueChanged(value.to_owned()));
        let _ = app.update(Message::ItemNudgeSnapToUnitChanged(snap));
        let _ = app.update(Message::NudgeSelectedItems(1));

        let moved = &app.project.audio_items()[0];
        assert_eq!(
            moved.start_sample(),
            expected_sample,
            "{value}, snap={snap}"
        );
        assert_eq!(moved.source_offset_samples(), original_offset);
        assert_eq!(moved.length_samples(), original_length);
        assert_eq!(app.timeline.selected_items, selection);
        assert_eq!(app.revision, revision_before + 1);
        assert!(
            app.item_nudge_dialog.error.is_none(),
            "{value}, snap={snap}"
        );

        let _ = app.update(Message::Undo);
        assert_eq!(app.project.audio_items()[0].start_sample(), 60_960);
        assert_eq!(app.timeline.selected_items, selection);
        let _ = app.update(Message::Redo);
        assert_eq!(app.project.audio_items()[0].start_sample(), expected_sample);
    }

    for (snap, expected_sample) in [(false, 132_960), (true, 144_000)] {
        let mut app = item_snap_test_app(60_960, 5_242);
        app.project
            .apply(DawAction::SetTimeSignatureMap {
                points: vec![aaadaw_core::MeterPointSnapshot {
                    start_tick: 0,
                    numerator: 3,
                    denominator: 4,
                }],
            })
            .unwrap();
        let item = &app.project.audio_items()[0];
        let item_id = item.id();
        let track_id = item.track_id();
        app.timeline.select_items(vec![(item_id, track_id)]);

        let _ = app.update(Message::ItemNudgeUnitChanged(Unit::MeasuresBeats));
        let _ = app.update(Message::ItemNudgeSnapToUnitChanged(snap));
        let _ = app.update(Message::NudgeSelectedItems(1));

        assert_eq!(
            app.project.audio_items()[0].start_sample(),
            expected_sample,
            "3/4 meter, snap={snap}"
        );
        assert!(app.item_nudge_dialog.error.is_none());
    }
}

#[test]
fn item_nudge_frames_matches_native_fractional_moves_snap_and_undo() {
    use super::item_nudge::{Target, Unit};

    let cases = [
        ("1", false, 1, 62_560),
        ("1", true, 1, 62_400),
        ("1", true, -1, 59_200),
        ("0.5", false, 1, 61_760),
        ("0.5", true, 1, 62_400),
    ];
    for (value, snap, direction, expected_sample) in cases {
        let mut app = item_snap_test_app(60_960, 5_242);
        let item = &app.project.audio_items()[0];
        let item_id = item.id();
        let track_id = item.track_id();
        let original_offset = item.source_offset_samples();
        let original_length = item.length_samples();
        app.timeline.select_items(vec![(item_id, track_id)]);
        let selection = app.timeline.selected_items.clone();
        let revision_before = app.revision;

        let _ = app.update(Message::ItemNudgeUnitChanged(Unit::Frames));
        assert_eq!(app.item_nudge_dialog.value, "1");
        let _ = app.update(Message::ItemNudgeTargetChanged(Target::Position));
        let _ = app.update(Message::ItemNudgeValueChanged(value.to_owned()));
        let _ = app.update(Message::ItemNudgeSnapToUnitChanged(snap));
        let _ = app.update(Message::NudgeSelectedItems(direction));

        let moved = &app.project.audio_items()[0];
        assert_eq!(
            moved.start_sample(),
            expected_sample,
            "{value}, snap={snap}"
        );
        assert_eq!(moved.source_offset_samples(), original_offset);
        assert_eq!(moved.length_samples(), original_length);
        assert_eq!(app.timeline.selected_items, selection);
        assert_eq!(app.revision, revision_before + 1);
        assert!(
            app.item_nudge_dialog.error.is_none(),
            "{value}, snap={snap}"
        );

        let _ = app.update(Message::Undo);
        assert_eq!(app.project.audio_items()[0].start_sample(), 60_960);
        assert_eq!(app.timeline.selected_items, selection);
        let _ = app.update(Message::Redo);
        assert_eq!(app.project.audio_items()[0].start_sample(), expected_sample);
    }

    let mut fractional = item_snap_test_app(48_000, 4_133);
    fractional
        .project
        .apply(DawAction::SetFrameRate {
            rate: aaadaw_core::FrameRate::Fps2997,
        })
        .unwrap();
    let item = &fractional.project.audio_items()[0];
    fractional
        .timeline
        .select_items(vec![(item.id(), item.track_id())]);
    let _ = fractional.update(Message::ItemNudgeUnitChanged(Unit::Frames));
    let _ = fractional.update(Message::ItemNudgeValueChanged("1".to_owned()));
    let _ = fractional.update(Message::ItemNudgeSnapToUnitChanged(true));
    let _ = fractional.update(Message::NudgeSelectedItems(1));
    assert_eq!(fractional.project.audio_items()[0].start_sample(), 49_650);
    assert!(fractional.item_nudge_dialog.error.is_none());

    let _ = fractional.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
    let previous_unit = fractional.item_nudge_dialog.unit;
    let _ = fractional.update(Message::ItemNudgeUnitChanged(Unit::Frames));
    assert_eq!(fractional.item_nudge_dialog.unit, previous_unit);
}

#[test]
fn item_nudge_set_applies_absolute_and_relative_positions_to_mixed_selection() {
    let mut app = item_snap_test_app(0, 0);
    let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
    let _ = app.update(Message::ItemNudgeValueChanged("0:02.000".to_owned()));
    let _ = app.update(Message::ApplyItemNudgeSet);

    let absolute_sample = (2.0 * f64::from(app.project.settings().sample_rate())) as u64;
    assert_eq!(app.project.audio_items()[0].start_sample(), absolute_sample);
    assert_eq!(
        app.project.midi_items()[0].start_tick(),
        app.project.tick_at_sample(absolute_sample).unwrap()
    );

    let _ = app.update(Message::ItemNudgeUnitChanged(
        super::item_nudge::Unit::Samples,
    ));
    let _ = app.update(Message::ItemNudgeRelativeSetChanged(true));
    let _ = app.update(Message::ItemNudgeValueChanged("-24000".to_owned()));
    let _ = app.update(Message::ApplyItemNudgeSet);

    let relative_sample = absolute_sample - 24_000;
    assert_eq!(app.project.audio_items()[0].start_sample(), relative_sample);
    assert_eq!(
        app.project.midi_items()[0].start_tick(),
        app.project.tick_at_sample(relative_sample).unwrap()
    );
}

#[test]
fn item_nudge_snap_aligns_the_first_item_and_preserves_selected_spacing() {
    let mut app = item_snap_test_app(12_013, 481);
    let first_track = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: first_track,
            media_ref: "asset://item-snap-test".to_owned(),
            start_sample: 72_026,
            source_offset_samples: 0,
            length_samples: 24_000,
        })
        .unwrap();
    let added_id = app.project.audio_items()[1].id();
    let audio_id = app.project.audio_items()[0].id();
    let midi_id = app.project.midi_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![
        (audio_id, first_track),
        (midi_id, app.project.tracks()[1].id()),
        (added_id, first_track),
    ]);
    let original_gap =
        app.project.audio_items()[1].start_sample() - app.project.audio_items()[0].start_sample();

    let _ = app.update(Message::ItemNudgeSnapToUnitChanged(true));
    let _ = app.update(Message::NudgeSelectedItems(1));

    assert_eq!(app.project.audio_items()[0].start_sample(), 12_048);
    assert_eq!(app.project.audio_items()[1].start_sample(), 72_061);
    assert_eq!(
        app.project.audio_items()[1].start_sample() - app.project.audio_items()[0].start_sample(),
        original_gap
    );
}

#[test]
fn item_nudge_set_uses_the_earliest_selected_audio_or_midi_item_as_its_anchor() {
    let mut app = item_snap_test_app(48_000, 0);
    let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
    let _ = app.update(Message::ItemNudgeValueChanged("0:02.000".to_owned()));
    let _ = app.update(Message::ApplyItemNudgeSet);

    let target_sample = 2 * u64::from(app.project.settings().sample_rate());
    assert_eq!(
        app.project.midi_items()[0].start_tick(),
        app.project.tick_at_sample(target_sample).unwrap()
    );
    assert_eq!(
        app.project.audio_items()[0].start_sample(),
        target_sample + 48_000
    );
}

#[test]
fn item_nudge_audio_boundary_targets_match_their_distinct_edge_semantics() {
    use super::item_nudge::Target;

    let cases = [
        (Target::LeftEdge, 48_000, 48_048, 23_952, 120),
        (Target::EndPosition, 48_000, 48_048, 24_000, 120),
        (Target::RightTrim, 48_000, 48_000, 24_048, 120),
    ];
    for (target, starting_sample, expected_start, expected_length, expected_offset) in cases {
        let mut app = item_snap_test_app(starting_sample, 0);
        let audio = &app.project.audio_items()[0];
        app.timeline
            .select_items(vec![(audio.id(), audio.track_id())]);
        let _ = app.update(Message::ItemNudgeTargetChanged(target));
        let _ = app.update(Message::ItemNudgeValueChanged("1".to_owned()));
        let _ = app.update(Message::NudgeSelectedItems(1));

        let edited = &app.project.audio_items()[0];
        assert_eq!(edited.start_sample(), expected_start, "target: {target}");
        assert_eq!(edited.length_samples(), expected_length, "target: {target}");
        assert_eq!(
            edited.source_offset_samples(),
            expected_offset,
            "target: {target}"
        );
        assert!(app.item_nudge_dialog.error.is_none(), "target: {target}");
    }
}

#[test]
fn item_nudge_edge_targets_support_snap_for_mixed_audio_midi_selection() {
    use super::item_nudge::Target;

    for snap_to_unit in [false, true] {
        for target in [
            Target::LeftTrim,
            Target::LeftEdge,
            Target::EndPosition,
            Target::RightTrim,
        ] {
            let (mut app, project_path) = item_nudge_set_edge_test_app();
            let original_audio = app.project.audio_items()[0].clone();
            let original_midi = app.project.midi_items()[0].clone();
            app.project
                .apply(DawAction::SetMidiControllers {
                    item_id: original_midi.id(),
                    controllers: vec![MidiControllerData {
                        controller: 1,
                        tick: 1_920,
                        value: 80,
                    }],
                })
                .unwrap();
            app.project
                .apply(DawAction::SetMidiPitchBends {
                    item_id: original_midi.id(),
                    pitch_bends: vec![aaadaw_core::MidiPitchBendData {
                        tick: 2_880,
                        value: 9_000,
                    }],
                })
                .unwrap();
            let original_midi = app.project.midi_items()[0].clone();
            let delta_samples = if snap_to_unit { 1_008_i128 } else { 989_i128 };
            let midi_start_sample = app
                .project
                .sample_at_tick(original_midi.start_tick())
                .unwrap();
            let midi_end_tick = original_midi.start_tick() + original_midi.length_ticks();
            let midi_end_sample = app.project.sample_at_tick(midi_end_tick).unwrap();
            let shifted_midi_start_tick = app
                .project
                .tick_at_sample((i128::from(midi_start_sample) + delta_samples) as u64)
                .unwrap();
            let shifted_midi_end_tick = app
                .project
                .tick_at_sample((i128::from(midi_end_sample) + delta_samples) as u64)
                .unwrap();
            let revision = app.revision;

            let _ = app.update(Message::ItemNudgeTargetChanged(target));
            let _ = app.update(Message::ItemNudgeValueChanged("20.6".to_owned()));
            let _ = app.update(Message::ItemNudgeSnapToUnitChanged(snap_to_unit));
            let _ = app.update(Message::NudgeSelectedItems(1));

            let edited_audio = app.project.audio_items()[0].clone();
            let edited_midi = app.project.midi_items()[0].clone();
            let (expected_audio_start, expected_audio_length, expected_audio_offset) = match target
            {
                Target::LeftTrim => (
                    original_audio.start_sample() + delta_samples as u64,
                    original_audio.length_samples() - delta_samples as u64,
                    if snap_to_unit { 926 } else { 909 },
                ),
                Target::LeftEdge => (
                    original_audio.start_sample() + delta_samples as u64,
                    original_audio.length_samples() - delta_samples as u64,
                    original_audio.source_offset_samples(),
                ),
                Target::EndPosition => (
                    original_audio.start_sample() + delta_samples as u64,
                    original_audio.length_samples(),
                    original_audio.source_offset_samples(),
                ),
                Target::RightTrim => (
                    original_audio.start_sample(),
                    original_audio.length_samples() + delta_samples as u64,
                    original_audio.source_offset_samples(),
                ),
                _ => unreachable!(),
            };
            assert_eq!(app.revision, revision + 1, "{target}, snap={snap_to_unit}");
            assert_eq!(
                (
                    edited_audio.start_sample(),
                    edited_audio.length_samples(),
                    edited_audio.source_offset_samples(),
                ),
                (
                    expected_audio_start,
                    expected_audio_length,
                    expected_audio_offset,
                ),
                "Audio: {target}, snap={snap_to_unit}"
            );
            match target {
                Target::LeftTrim | Target::LeftEdge => {
                    assert_eq!(edited_midi.start_tick(), shifted_midi_start_tick);
                    assert_eq!(
                        edited_midi.length_ticks(),
                        midi_end_tick - shifted_midi_start_tick
                    );
                    if target == Target::LeftTrim {
                        assert_ne!(
                            edited_midi.source_offset_ticks(),
                            original_midi.source_offset_ticks()
                        );
                    } else {
                        assert_eq!(
                            edited_midi.source_offset_ticks(),
                            original_midi.source_offset_ticks()
                        );
                    }
                }
                Target::EndPosition => {
                    assert_eq!(edited_midi.start_tick(), shifted_midi_start_tick);
                    assert_eq!(edited_midi.length_ticks(), original_midi.length_ticks());
                    assert_eq!(
                        edited_midi.source_offset_ticks(),
                        original_midi.source_offset_ticks()
                    );
                }
                Target::RightTrim => {
                    assert_eq!(edited_midi.start_tick(), original_midi.start_tick());
                    assert_eq!(
                        edited_midi.length_ticks(),
                        shifted_midi_end_tick - original_midi.start_tick()
                    );
                    assert_eq!(
                        edited_midi.source_offset_ticks(),
                        original_midi.source_offset_ticks()
                    );
                }
                _ => unreachable!(),
            }
            assert_eq!(edited_midi.notes(), original_midi.notes());
            assert_eq!(edited_midi.controllers(), original_midi.controllers());
            assert_eq!(edited_midi.pitch_bends(), original_midi.pitch_bends());
            assert_eq!(app.timeline.selected_items.len(), 2);
            assert!(
                app.item_nudge_dialog.error.is_none(),
                "{target}, snap={snap_to_unit}"
            );

            let _ = app.update(Message::Undo);
            assert_eq!(app.project.audio_items()[0], original_audio);
            assert_eq!(app.project.midi_items()[0], original_midi);
            let _ = app.update(Message::Redo);
            assert_eq!(app.project.audio_items()[0], edited_audio);
            assert_eq!(app.project.midi_items()[0], edited_midi);
            drop(app);
            let _ = std::fs::remove_file(project_path);
        }
    }
}

#[test]
fn item_nudge_nonloop_midi_left_trim_edits_the_source_at_its_boundary() {
    use super::item_nudge::Target;

    for (
        target,
        expected_start_delta,
        expected_length_delta,
        expected_source_delta,
        content_shift,
        expected_audio_start_delta,
    ) in [
        (
            Target::LeftTrim,
            480_i64,
            -480_i64,
            -480_i64,
            -480_i64,
            12_000_u64,
        ),
        (Target::LeftEdge, 480, -480, 0, 0, 12_000),
        (Target::EndPosition, 480, 0, 0, 0, 12_000),
        (Target::RightTrim, 0, 480, 480, 0, 0),
    ] {
        let (mut app, project_path) = item_nudge_set_edge_test_app();
        let audio_id = app.project.audio_items()[0].id();
        let midi_id = app.project.midi_items()[0].id();
        app.project
            .apply(DawAction::SetAudioItemLoopSource {
                item_id: audio_id,
                enabled: false,
            })
            .unwrap();
        app.project
            .apply(DawAction::SetMidiItemLoopSource {
                item_id: midi_id,
                enabled: false,
            })
            .unwrap();
        app.project
            .apply(DawAction::SetMidiControllers {
                item_id: midi_id,
                controllers: vec![MidiControllerData {
                    controller: 1,
                    tick: 1_920,
                    value: 80,
                }],
            })
            .unwrap();
        app.project
            .apply(DawAction::SetMidiPitchBends {
                item_id: midi_id,
                pitch_bends: vec![aaadaw_core::MidiPitchBendData {
                    tick: 2_880,
                    value: 9_000,
                }],
            })
            .unwrap();
        let original_audio = app.project.audio_items()[0].clone();
        let original_midi = app.project.midi_items()[0].clone();
        app.timeline.rebuild(&app.project);
        app.timeline.select_items(vec![
            (audio_id, original_audio.track_id()),
            (midi_id, original_midi.track_id()),
        ]);
        let revision = app.revision;

        let _ = app.update(Message::ItemNudgeTargetChanged(target));
        let _ = app.update(Message::ItemNudgeValueChanged("250".to_owned()));
        let _ = app.update(Message::NudgeSelectedItems(1));

        let edited_audio = app.project.audio_items()[0].clone();
        let edited_midi = app.project.midi_items()[0].clone();
        assert_eq!(app.revision, revision + 1, "{target}");
        assert_eq!(
            edited_midi.start_tick() as i64 - original_midi.start_tick() as i64,
            expected_start_delta,
            "{target} MIDI position"
        );
        assert_eq!(
            edited_midi.length_ticks() as i64 - original_midi.length_ticks() as i64,
            expected_length_delta,
            "{target} MIDI length"
        );
        assert_eq!(
            edited_midi.source_offset_ticks(),
            0,
            "{target} MIDI source offset"
        );
        assert_eq!(
            edited_midi.source_loop_length_ticks() as i64
                - original_midi.source_loop_length_ticks() as i64,
            expected_source_delta,
            "{target} MIDI source length"
        );
        assert_eq!(
            edited_midi.notes()[0].tick() as i64 - original_midi.notes()[0].tick() as i64,
            content_shift,
            "{target} MIDI note"
        );
        assert_eq!(
            edited_midi.controllers()[0].tick as i64 - original_midi.controllers()[0].tick as i64,
            content_shift,
            "{target} MIDI controller"
        );
        assert_eq!(
            edited_midi.pitch_bends()[0].tick as i64 - original_midi.pitch_bends()[0].tick as i64,
            content_shift,
            "{target} MIDI pitch bend"
        );
        assert_eq!(
            edited_audio.start_sample(),
            original_audio.start_sample() + expected_audio_start_delta
        );
        assert!(app.item_nudge_dialog.error.is_none(), "{target}");

        let _ = app.update(Message::Undo);
        assert_eq!(
            app.project.audio_items()[0],
            original_audio,
            "{target} undo audio"
        );
        assert_eq!(
            app.project.midi_items()[0],
            original_midi,
            "{target} undo MIDI"
        );
        let _ = app.update(Message::Redo);
        assert_eq!(
            app.project.audio_items()[0],
            edited_audio,
            "{target} redo audio"
        );
        assert_eq!(
            app.project.midi_items()[0],
            edited_midi,
            "{target} redo MIDI"
        );
        drop(app);
        let _ = std::fs::remove_file(project_path);
    }
}

#[test]
fn item_nudge_nonloop_midi_left_trim_uses_offset_until_source_zero() {
    use super::item_nudge::Target;

    for (direction, expected_offset, expected_source_delta, expected_content_shift) in [
        (1_i8, 672_u64, 0_i64, 0_i64),
        (-1_i8, 1_u64, 289_i64, 289_i64),
    ] {
        let (mut app, project_path) = item_nudge_set_edge_test_app();
        let audio_id = app.project.audio_items()[0].id();
        let midi_id = app.project.midi_items()[0].id();
        app.project
            .apply(DawAction::SetAudioItemLoopSource {
                item_id: audio_id,
                enabled: false,
            })
            .unwrap();
        app.project
            .apply(DawAction::SetMidiItemLoopSource {
                item_id: midi_id,
                enabled: false,
            })
            .unwrap();
        app.project
            .apply(DawAction::EditMidiItemPlacement {
                item_id: midi_id,
                start_tick: app.project.midi_items()[0].start_tick(),
                length_ticks: app.project.midi_items()[0].length_ticks(),
                source_offset_ticks: 192,
            })
            .unwrap();
        app.project
            .apply(DawAction::SetMidiControllers {
                item_id: midi_id,
                controllers: vec![MidiControllerData {
                    controller: 1,
                    tick: 1_920,
                    value: 80,
                }],
            })
            .unwrap();
        app.project
            .apply(DawAction::SetMidiPitchBends {
                item_id: midi_id,
                pitch_bends: vec![aaadaw_core::MidiPitchBendData {
                    tick: 2_880,
                    value: 9_000,
                }],
            })
            .unwrap();
        let original_audio = app.project.audio_items()[0].clone();
        let original_midi = app.project.midi_items()[0].clone();
        app.timeline.rebuild(&app.project);
        app.timeline.select_items(vec![
            (audio_id, original_audio.track_id()),
            (midi_id, original_midi.track_id()),
        ]);

        let _ = app.update(Message::ItemNudgeTargetChanged(Target::LeftTrim));
        let _ = app.update(Message::ItemNudgeValueChanged("250".to_owned()));
        let _ = app.update(Message::NudgeSelectedItems(direction));

        let edited = &app.project.midi_items()[0];
        assert_eq!(edited.source_offset_ticks(), expected_offset);
        assert_eq!(
            edited.source_loop_length_ticks() as i64
                - original_midi.source_loop_length_ticks() as i64,
            expected_source_delta
        );
        assert_eq!(
            edited.notes()[0].tick() as i64 - original_midi.notes()[0].tick() as i64,
            expected_content_shift
        );
        assert_eq!(
            edited.controllers()[0].tick as i64 - original_midi.controllers()[0].tick as i64,
            expected_content_shift
        );
        assert_eq!(
            edited.pitch_bends()[0].tick as i64 - original_midi.pitch_bends()[0].tick as i64,
            expected_content_shift
        );
        assert!(app.item_nudge_dialog.error.is_none());
        let edited = edited.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.project.midi_items()[0], original_midi);
        let _ = app.update(Message::Redo);
        assert_eq!(app.project.midi_items()[0], edited);
        drop(app);
        let _ = std::fs::remove_file(project_path);
    }
}

#[test]
fn item_nudge_nonloop_midi_left_trim_preserves_events_at_source_zero() {
    use super::item_nudge::Target;

    let (mut app, project_path) = item_nudge_set_edge_test_app();
    let audio_id = app.project.audio_items()[0].id();
    let midi_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::SetAudioItemLoopSource {
            item_id: audio_id,
            enabled: false,
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiItemLoopSource {
            item_id: midi_id,
            enabled: false,
        })
        .unwrap();
    let note_id = app.project.midi_items()[0].notes()[0].id();
    app.project
        .apply(DawAction::DeleteMidiNotes {
            item_id: midi_id,
            note_ids: vec![note_id],
        })
        .unwrap();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id: midi_id,
            notes: vec![MidiNoteData {
                pitch: 60,
                tick: 0,
                duration: 960,
                velocity: 96,
            }],
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiControllers {
            item_id: midi_id,
            controllers: vec![MidiControllerData {
                controller: 1,
                tick: 0,
                value: 80,
            }],
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiPitchBends {
            item_id: midi_id,
            pitch_bends: vec![aaadaw_core::MidiPitchBendData {
                tick: 240,
                value: 9_000,
            }],
        })
        .unwrap();
    let original_audio = app.project.audio_items()[0].clone();
    let original_midi = app.project.midi_items()[0].clone();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![
        (audio_id, original_audio.track_id()),
        (midi_id, original_midi.track_id()),
    ]);

    let _ = app.update(Message::ItemNudgeTargetChanged(Target::LeftTrim));
    let _ = app.update(Message::ItemNudgeValueChanged("250".to_owned()));
    let _ = app.update(Message::NudgeSelectedItems(1));

    let edited = app.project.midi_items()[0].clone();
    assert_eq!(edited.start_tick(), original_midi.start_tick() + 480);
    assert_eq!(edited.length_ticks(), original_midi.length_ticks() - 480);
    assert_eq!(edited.source_offset_ticks(), 480);
    assert_eq!(
        edited.source_loop_length_ticks(),
        original_midi.source_loop_length_ticks()
    );
    assert_eq!(edited.notes()[0].tick(), 0);
    assert_eq!(edited.notes()[0].duration(), 960);
    assert_eq!(edited.controllers()[0].tick, 0);
    assert_eq!(edited.pitch_bends()[0].tick, 240);
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0], original_midi);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.midi_items()[0], edited);
    drop(app);
    let _ = std::fs::remove_file(project_path);
}

#[test]
fn item_nudge_left_trim_and_contents_use_source_sample_frames() {
    use super::item_nudge::Target;

    let file_id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
    let project_path = std::env::temp_dir().join(format!(
        "aaadaw-ui-item-nudge-left-trim-{}-{file_id}.aaadaw",
        std::process::id()
    ));
    let media_ref = "asset://item-nudge-left-trim-test";
    let mut store = ProjectStore::open(&project_path).expect("project store should open");
    store
        .import_audio_asset(media_ref, "left-trim.wav", std::io::Cursor::new([0_u8; 4]))
        .expect("test asset should import");
    store
        .set_audio_asset_metadata(
            media_ref,
            &aaadaw_storage::AudioAssetMetadata {
                container: "wav".to_owned(),
                codec: "pcm".to_owned(),
                sample_rate: Some(44_100),
                channel_count: Some(1),
                bits_per_sample: Some(16),
                frame_count: Some(100_000),
                duration_nanos: None,
                byte_len: Some(4),
            },
        )
        .expect("test metadata should store");
    store.close().expect("project store should close");

    let mut app = App {
        project_path: Some(project_path.clone()),
        ..App::default()
    };
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: media_ref.to_owned(),
            start_sample: 48_000,
            source_offset_samples: 20_000,
            length_samples: 24_000,
        })
        .unwrap();
    let item_id = app.project.audio_items()[0].id();
    app.project
        .apply(DawAction::SetAudioItemLoopSource {
            item_id,
            enabled: false,
        })
        .unwrap();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(item_id, track_id)]);
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::LeftTrim));
    let _ = app.update(Message::ItemNudgeValueChanged("250".to_owned()));
    let revision = app.revision;

    let _ = app.update(Message::NudgeSelectedItems(1));

    let edited = &app.project.audio_items()[0];
    assert_eq!(app.revision, revision + 1);
    assert_eq!(edited.start_sample(), 60_000);
    assert_eq!(edited.length_samples(), 12_000);
    assert_eq!(edited.source_offset_samples(), 31_025);
    assert!(app.item_nudge_dialog.error.is_none());
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].start_sample(), 48_000);
    assert_eq!(app.project.audio_items()[0].source_offset_samples(), 20_000);

    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Contents));
    let revision = app.revision;
    let _ = app.update(Message::NudgeSelectedItems(1));
    let contents_right = &app.project.audio_items()[0];
    assert_eq!(app.revision, revision + 1);
    assert_eq!(contents_right.start_sample(), 48_000);
    assert_eq!(contents_right.length_samples(), 24_000);
    assert_eq!(contents_right.source_offset_samples(), 8_975);
    assert!(app.item_nudge_dialog.error.is_none());
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].source_offset_samples(), 20_000);

    let _ = app.update(Message::NudgeSelectedItems(-1));
    let contents_left = &app.project.audio_items()[0];
    assert_eq!(contents_left.start_sample(), 48_000);
    assert_eq!(contents_left.length_samples(), 24_000);
    assert_eq!(contents_left.source_offset_samples(), 31_025);
    let _ = app.update(Message::Undo);

    let _ = app.update(Message::ItemNudgeTargetChanged(Target::LeftTrim));
    let _ = app.update(Message::NudgeSelectedItems(-1));
    let left_trimmed = &app.project.audio_items()[0];
    assert_eq!(left_trimmed.start_sample(), 36_000);
    assert_eq!(left_trimmed.length_samples(), 36_000);
    assert_eq!(left_trimmed.source_offset_samples(), 8_975);

    app.project
        .apply(DawAction::EditAudioItem {
            item_id,
            media_ref: media_ref.to_owned(),
            start_sample: 36_000,
            source_offset_samples: 0,
            length_samples: 36_000,
        })
        .unwrap();
    let before = app.project.audio_items()[0].clone();
    let revision = app.revision;
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Contents));
    let _ = app.update(Message::NudgeSelectedItems(1));
    let nudged_before_source = &app.project.audio_items()[0];
    assert_eq!(app.revision, revision + 1);
    assert_eq!(nudged_before_source.start_sample(), before.start_sample());
    assert_eq!(
        nudged_before_source.length_samples(),
        before.length_samples()
    );
    assert_eq!(nudged_before_source.source_offset_samples(), -11_025);
    assert!(app.item_nudge_dialog.error.is_none());
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].source_offset_samples(), 0);

    app.project
        .apply(DawAction::EditAudioItem {
            item_id,
            media_ref: media_ref.to_owned(),
            start_sample: 36_000,
            source_offset_samples: 66_925,
            length_samples: 36_000,
        })
        .unwrap();
    let before = app.project.audio_items()[0].clone();
    let revision = app.revision;
    let _ = app.update(Message::NudgeSelectedItems(-1));
    let edited = &app.project.audio_items()[0];
    assert_eq!(app.revision, revision + 1);
    assert_eq!(edited.start_sample(), before.start_sample());
    assert_eq!(edited.length_samples(), before.length_samples());
    assert_eq!(edited.source_offset_samples(), 77_950);
    assert!(app.item_nudge_dialog.error.is_none());
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].source_offset_samples(), 66_925);
    drop(app);
    let _ = std::fs::remove_file(project_path);
}

#[test]
fn item_nudge_contents_wraps_inside_a_bounded_loop_source_section() {
    use super::item_nudge::Target;

    let file_id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
    let project_path = std::env::temp_dir().join(format!(
        "aaadaw-ui-item-nudge-loop-contents-{}-{file_id}.aaadaw",
        std::process::id()
    ));
    let media_ref = "asset://item-nudge-loop-contents-test";
    let mut store = ProjectStore::open(&project_path).expect("project store should open");
    store
        .import_audio_asset(
            media_ref,
            "loop-contents.wav",
            std::io::Cursor::new([0_u8; 4]),
        )
        .expect("test asset should import");
    store
        .set_audio_asset_metadata(
            media_ref,
            &aaadaw_storage::AudioAssetMetadata {
                container: "wav".to_owned(),
                codec: "pcm".to_owned(),
                sample_rate: Some(44_100),
                channel_count: Some(1),
                bits_per_sample: Some(16),
                frame_count: Some(100_000),
                duration_nanos: None,
                byte_len: Some(4),
            },
        )
        .expect("test metadata should store");
    store.close().expect("project store should close");

    let mut app = App {
        project_path: Some(project_path.clone()),
        ..App::default()
    };
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertLoopedAudioItem {
            track_id,
            media_ref: media_ref.to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            phase_inverted: false,
            source_section_start_samples: 80_000,
            length_samples: 24_000,
            muted: false,
            source_section_length_samples: 20_000,
        })
        .expect("bounded loop source should insert");
    let item_id = app.project.audio_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(item_id, track_id)]);
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Contents));
    let _ = app.update(Message::ItemNudgeValueChanged("250".to_owned()));
    let revision = app.revision;

    let _ = app.update(Message::NudgeSelectedItems(1));

    let item = &app.project.audio_items()[0];
    assert_eq!(app.revision, revision + 1);
    assert_eq!(item.start_sample(), 0);
    assert_eq!(item.length_samples(), 24_000);
    assert_eq!(item.source_offset_samples(), 8_975);
    assert_eq!(item.source_section_start_samples(), Some(80_000));
    assert_eq!(item.source_section_length_samples(), Some(20_000));
    assert!(app.item_nudge_dialog.error.is_none());
    let _ = app.update(Message::Undo);

    let _ = app.update(Message::NudgeSelectedItems(-1));
    let item = &app.project.audio_items()[0];
    assert_eq!(item.source_offset_samples(), 11_025);
    assert_eq!(item.source_section_start_samples(), Some(80_000));
    assert_eq!(item.source_section_length_samples(), Some(20_000));
    assert!(app.item_nudge_dialog.error.is_none());

    drop(app);
    let _ = std::fs::remove_file(project_path);
}

#[test]
fn item_nudge_contents_allows_negative_offset_in_disabled_bounded_section() {
    use super::item_nudge::Target;

    let file_id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
    let project_path = std::env::temp_dir().join(format!(
        "aaadaw-ui-item-nudge-disabled-section-{}-{file_id}.aaadaw",
        std::process::id()
    ));
    let media_ref = "asset://item-nudge-disabled-section-test";
    let mut store = ProjectStore::open(&project_path).expect("project store should open");
    store
        .import_audio_asset(
            media_ref,
            "disabled-section.wav",
            std::io::Cursor::new([0_u8; 4]),
        )
        .expect("test asset should import");
    store
        .set_audio_asset_metadata(
            media_ref,
            &aaadaw_storage::AudioAssetMetadata {
                container: "wav".to_owned(),
                codec: "pcm".to_owned(),
                sample_rate: Some(48_000),
                channel_count: Some(1),
                bits_per_sample: Some(16),
                frame_count: Some(480_000),
                duration_nanos: None,
                byte_len: Some(4),
            },
        )
        .expect("test metadata should store");
    store.close().expect("project store should close");

    let mut app = App {
        project_path: Some(project_path.clone()),
        ..App::default()
    };
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertLoopedAudioItem {
            track_id,
            media_ref: media_ref.to_owned(),
            start_sample: 0,
            source_offset_samples: 4_800,
            length_samples: 132_300,
            muted: false,
            phase_inverted: false,
            source_section_start_samples: 144_000,
            source_section_length_samples: 144_000,
        })
        .expect("bounded source should insert with section-relative offset");
    let item_id = app.project.audio_items()[0].id();
    app.project
        .apply(DawAction::SetAudioItemLoopSource {
            item_id,
            enabled: false,
        })
        .expect("bounded section should be disabled before nudge");
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(item_id, track_id)]);
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Contents));
    let _ = app.update(Message::ItemNudgeValueChanged("250".to_owned()));
    let revision = app.revision;

    let _ = app.update(Message::NudgeSelectedItems(1));

    let item = &app.project.audio_items()[0];
    assert_eq!(app.revision, revision + 1);
    assert_eq!(item.start_sample(), 0);
    assert_eq!(item.length_samples(), 132_300);
    assert_eq!(item.source_offset_samples(), -7_200);
    assert_eq!(item.source_section_start_samples(), Some(144_000));
    assert_eq!(item.source_section_length_samples(), Some(144_000));
    assert!(!item.is_loop_source());
    assert!(app.item_nudge_dialog.error.is_none());
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].source_offset_samples(), 4_800);

    drop(app);
    let _ = std::fs::remove_file(project_path);
}

#[test]
fn item_nudge_duplicate_offsets_copies_mixed_selection_and_selects_the_copies() {
    use super::item_nudge::Target;

    let mut app = item_snap_test_app(0, 0);
    let source_audio_id = app.project.audio_items()[0].id();
    let source_midi_id = app.project.midi_items()[0].id();
    let delta_samples = (f64::from(app.project.settings().sample_rate()) / 1_000.0).round() as u64;
    let expected_midi_tick = app.project.tick_at_sample(delta_samples).unwrap();
    let revision = app.revision;
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Duplicate));

    let _ = app.update(Message::NudgeSelectedItems(1));

    assert_eq!(app.revision, revision + 1);
    assert_eq!(app.project.audio_items().len(), 2);
    assert_eq!(app.project.midi_items().len(), 2);
    assert_eq!(app.project.audio_items()[0].start_sample(), 0);
    assert_eq!(app.project.audio_items()[1].start_sample(), delta_samples);
    assert_eq!(app.project.midi_items()[0].start_tick(), 0);
    assert_eq!(app.project.midi_items()[1].start_tick(), expected_midi_tick);
    assert_eq!(app.timeline.selected_items.len(), 2);
    assert!(!app.timeline.selected_items.contains(&source_audio_id));
    assert!(!app.timeline.selected_items.contains(&source_midi_id));
    assert!(
        app.timeline
            .selected_items
            .contains(&app.project.audio_items()[1].id())
    );
    assert!(
        app.timeline
            .selected_items
            .contains(&app.project.midi_items()[1].id())
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items().len(), 1);
    assert_eq!(app.project.midi_items().len(), 1);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items().len(), 2);
    assert_eq!(app.project.midi_items().len(), 2);
}

#[test]
fn item_nudge_set_duplicate_uses_absolute_target_and_copies_mixed_selection() {
    use super::item_nudge::Target;

    let mut app = item_snap_test_app(0, 0);
    let source_audio = app.project.audio_items()[0].clone();
    let source_midi = app.project.midi_items()[0].clone();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id: source_midi.id(),
            notes: vec![MidiNoteData {
                pitch: 60,
                tick: 120,
                duration: 240,
                velocity: 96,
            }],
        })
        .unwrap();
    let original_midi = app.project.midi_items()[0].clone();
    let revision = app.revision;
    let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Duplicate));
    let _ = app.update(Message::ItemNudgeSnapToUnitChanged(true));
    let _ = app.update(Message::ItemNudgeValueChanged("0:01.200".to_owned()));

    let _ = app.update(Message::ApplyItemNudgeSet);

    let duplicate_audio = &app.project.audio_items()[1];
    let duplicate_midi = &app.project.midi_items()[1];
    assert_eq!(app.revision, revision + 1);
    assert_eq!(app.project.audio_items().len(), 2);
    assert_eq!(app.project.midi_items().len(), 2);
    assert_eq!(app.project.audio_items()[0], source_audio);
    assert_eq!(app.project.midi_items()[0], original_midi);
    assert_eq!(duplicate_audio.start_sample(), 48_000);
    assert_eq!(
        duplicate_audio.length_samples(),
        source_audio.length_samples()
    );
    assert_eq!(
        duplicate_audio.source_offset_samples(),
        source_audio.source_offset_samples()
    );
    assert_eq!(duplicate_midi.start_tick(), 1_920);
    assert_eq!(duplicate_midi.length_ticks(), source_midi.length_ticks());
    assert_eq!(duplicate_midi.notes().len(), 1);
    assert_eq!(duplicate_midi.notes()[0].tick(), 120);
    assert_eq!(duplicate_midi.notes()[0].duration(), 240);
    assert_eq!(duplicate_midi.notes()[0].pitch(), 60);
    assert_eq!(duplicate_midi.notes()[0].velocity(), 96);
    assert_eq!(app.timeline.selected_items.len(), 2);
    assert!(app.timeline.selected_items.contains(&duplicate_audio.id()));
    assert!(app.timeline.selected_items.contains(&duplicate_midi.id()));
    assert!(!app.timeline.selected_items.contains(&source_audio.id()));
    assert!(!app.timeline.selected_items.contains(&source_midi.id()));
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items(), &[source_audio]);
    assert_eq!(app.project.midi_items(), &[original_midi]);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items().len(), 2);
    assert_eq!(app.project.midi_items().len(), 2);
    assert_eq!(app.project.audio_items()[1].start_sample(), 48_000);
    assert_eq!(app.project.midi_items()[1].start_tick(), 1_920);
}

#[test]
fn item_nudge_set_duplicate_keeps_absolute_target_when_relative_set_is_checked() {
    use super::item_nudge::Target;

    let mut app = item_snap_test_app(48_000, 1_920);
    let source_audio = app.project.audio_items()[0].clone();
    let source_midi = app.project.midi_items()[0].clone();
    let revision = app.revision;
    let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Duplicate));
    let _ = app.update(Message::ItemNudgeRelativeSetChanged(true));
    let _ = app.update(Message::ItemNudgeValueChanged("0:00.250".to_owned()));

    let _ = app.update(Message::ApplyItemNudgeSet);

    assert_eq!(app.revision, revision + 1);
    assert_eq!(app.project.audio_items()[0], source_audio);
    assert_eq!(app.project.midi_items()[0], source_midi);
    assert_eq!(app.project.audio_items()[1].start_sample(), 12_000);
    assert_eq!(app.project.midi_items()[1].start_tick(), 480);
    assert_eq!(app.timeline.selected_items.len(), 2);
    assert!(
        app.timeline
            .selected_items
            .contains(&app.project.audio_items()[1].id())
    );
    assert!(
        app.timeline
            .selected_items
            .contains(&app.project.midi_items()[1].id())
    );
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items(), &[source_audio]);
    assert_eq!(app.project.midi_items(), &[source_midi]);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items()[1].start_sample(), 12_000);
    assert_eq!(app.project.midi_items()[1].start_tick(), 480);
}

#[test]
fn item_nudge_set_boundary_targets_match_native_mixed_audio_and_midi_semantics() {
    use super::item_nudge::Target;

    let cases = [
        (
            Target::LeftTrim,
            "0:04.000",
            192_000,
            384_000,
            88_200,
            7_680,
            15_360,
            3_840,
        ),
        (
            Target::LeftEdge,
            "0:04.000",
            192_000,
            384_000,
            0,
            7_680,
            15_360,
            0,
        ),
        (
            Target::EndPosition,
            "0:14.000",
            192_000,
            480_000,
            0,
            7_680,
            19_200,
            0,
        ),
        (
            Target::RightTrim,
            "0:14.000",
            96_000,
            576_000,
            0,
            3_840,
            23_040,
            0,
        ),
    ];
    for (
        target,
        value,
        audio_start,
        audio_length,
        audio_offset,
        midi_start,
        midi_length,
        midi_offset,
    ) in cases
    {
        let (mut app, project_path) = item_nudge_set_edge_test_app();
        let original_audio = app.project.audio_items()[0].clone();
        let original_midi = app.project.midi_items()[0].clone();
        let revision = app.revision;
        let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
        let _ = app.update(Message::ItemNudgeTargetChanged(target));
        let _ = app.update(Message::ItemNudgeValueChanged(value.to_owned()));

        let _ = app.update(Message::ApplyItemNudgeSet);

        let edited_audio = &app.project.audio_items()[0];
        let edited_midi = &app.project.midi_items()[0];
        assert_eq!(app.revision, revision + 1, "target: {target}");
        assert_eq!(edited_audio.start_sample(), audio_start, "target: {target}");
        assert_eq!(
            edited_audio.length_samples(),
            audio_length,
            "target: {target}"
        );
        assert_eq!(
            edited_audio.source_offset_samples(),
            audio_offset,
            "target: {target}"
        );
        assert_eq!(
            edited_audio.is_loop_source(),
            original_audio.is_loop_source()
        );
        assert_eq!(edited_midi.start_tick(), midi_start, "target: {target}");
        assert_eq!(edited_midi.length_ticks(), midi_length, "target: {target}");
        assert_eq!(
            edited_midi.source_offset_ticks(),
            midi_offset,
            "target: {target}"
        );
        assert_eq!(edited_midi.is_loop_source(), original_midi.is_loop_source());
        assert_eq!(edited_midi.notes(), original_midi.notes());
        assert_eq!(app.timeline.selected_items.len(), 2);
        assert!(app.timeline.selected_items.contains(&edited_audio.id()));
        assert!(app.timeline.selected_items.contains(&edited_midi.id()));
        assert!(app.item_nudge_dialog.error.is_none(), "target: {target}");

        let after_audio = edited_audio.clone();
        let after_midi = edited_midi.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(
            app.project.audio_items()[0],
            original_audio,
            "target: {target}"
        );
        assert_eq!(
            app.project.midi_items()[0],
            original_midi,
            "target: {target}"
        );
        assert_eq!(
            app.timeline.selected_items.len(),
            2,
            "target: {target} Undo"
        );
        let _ = app.update(Message::Redo);
        assert_eq!(
            app.project.audio_items()[0],
            after_audio,
            "target: {target}"
        );
        assert_eq!(app.project.midi_items()[0], after_midi, "target: {target}");
        drop(app);
        let _ = std::fs::remove_file(project_path);
    }
}

#[test]
fn item_nudge_set_edge_targets_match_native_staggered_mixed_audio_midi_selection() {
    use super::item_nudge::Target;

    let cases = [
        (
            Target::LeftTrim,
            "0:04.000",
            (192_000, 384_000, 88_200),
            (7_680, 23_040, 15_360),
        ),
        (
            Target::LeftEdge,
            "0:04.000",
            (192_000, 384_000, 0),
            (7_680, 23_040, 0),
        ),
        (
            Target::EndPosition,
            "0:14.000",
            (192_000, 480_000, 0),
            (7_680, 19_200, 0),
        ),
        (
            Target::RightTrim,
            "0:14.000",
            (96_000, 576_000, 0),
            (11_520, 15_360, 0),
        ),
    ];

    for (target, value, expected_audio, expected_midi) in cases {
        let (mut app, project_path) = item_nudge_set_edge_test_app();
        let audio_id = app.project.audio_items()[0].id();
        let midi_id = app.project.midi_items()[0].id();
        let audio_track_id = app.project.tracks()[0].id();
        let midi_track_id = app.project.tracks()[1].id();
        app.project
            .apply(DawAction::EditMidiItemPlacement {
                item_id: midi_id,
                start_tick: 11_520,
                length_ticks: 19_200,
                source_offset_ticks: 0,
            })
            .unwrap();
        app.project
            .apply(DawAction::SetMidiControllers {
                item_id: midi_id,
                controllers: vec![aaadaw_core::MidiControllerData {
                    controller: 1,
                    tick: 2_880,
                    value: 80,
                }],
            })
            .unwrap();
        app.project
            .apply(DawAction::SetMidiPitchBends {
                item_id: midi_id,
                pitch_bends: vec![aaadaw_core::MidiPitchBendData {
                    tick: 3_840,
                    value: 9_000,
                }],
            })
            .unwrap();
        let original_audio = app.project.audio_items()[0].clone();
        let original_midi = app.project.midi_items()[0].clone();
        app.timeline.rebuild(&app.project);
        app.timeline
            .select_items(vec![(audio_id, audio_track_id), (midi_id, midi_track_id)]);
        let revision = app.revision;
        let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
        let _ = app.update(Message::ItemNudgeTargetChanged(target));
        let _ = app.update(Message::ItemNudgeValueChanged(value.to_owned()));

        let _ = app.update(Message::ApplyItemNudgeSet);

        let edited_audio = app.project.audio_items()[0].clone();
        let edited_midi = app.project.midi_items()[0].clone();
        assert_eq!(app.revision, revision + 1, "target: {target}");
        assert_eq!(
            (
                edited_audio.start_sample(),
                edited_audio.length_samples(),
                edited_audio.source_offset_samples(),
            ),
            expected_audio,
            "target: {target} Audio"
        );
        assert_eq!(
            (
                edited_midi.start_tick(),
                edited_midi.length_ticks(),
                edited_midi.source_offset_ticks(),
            ),
            expected_midi,
            "target: {target} MIDI"
        );
        assert_eq!(
            edited_midi.source_loop_length_ticks(),
            original_midi.source_loop_length_ticks(),
            "target: {target} MIDI source length"
        );
        assert_eq!(
            edited_midi.notes(),
            original_midi.notes(),
            "target: {target}"
        );
        assert_eq!(
            edited_midi.controllers(),
            original_midi.controllers(),
            "target: {target}"
        );
        assert_eq!(
            edited_midi.pitch_bends(),
            original_midi.pitch_bends(),
            "target: {target}"
        );
        assert_eq!(app.timeline.selected_items.len(), 2);
        assert!(app.item_nudge_dialog.error.is_none(), "target: {target}");

        let _ = app.update(Message::Undo);
        assert_eq!(
            app.project.audio_items()[0],
            original_audio,
            "target: {target}"
        );
        assert_eq!(
            app.project.midi_items()[0],
            original_midi,
            "target: {target}"
        );
        let _ = app.update(Message::Redo);
        assert_eq!(
            app.project.audio_items()[0],
            edited_audio,
            "target: {target}"
        );
        assert_eq!(app.project.midi_items()[0], edited_midi, "target: {target}");
        assert_eq!(
            app.timeline.selected_items.len(),
            2,
            "target: {target} Redo"
        );
        drop(app);
        let _ = std::fs::remove_file(project_path);
    }
}

#[test]
fn item_nudge_set_boundary_snap_rounds_the_absolute_edge_target() {
    use super::item_nudge::Target;

    let (mut app, project_path) = item_nudge_set_edge_test_app();
    let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::LeftEdge));
    let _ = app.update(Message::ItemNudgeSnapToUnitChanged(true));
    let _ = app.update(Message::ItemNudgeValueChanged("0:04.150".to_owned()));

    let _ = app.update(Message::ApplyItemNudgeSet);

    assert_eq!(app.project.audio_items()[0].start_sample(), 192_000);
    assert_eq!(app.project.audio_items()[0].length_samples(), 384_000);
    assert_eq!(app.project.midi_items()[0].start_tick(), 7_680);
    assert_eq!(app.project.midi_items()[0].length_ticks(), 15_360);
    assert!(app.item_nudge_dialog.error.is_none());
    drop(app);
    let _ = std::fs::remove_file(project_path);
}

#[test]
fn item_nudge_set_edge_targets_apply_absolute_edges_to_staggered_selection() {
    use super::item_nudge::Target;

    let cases = [
        (
            Target::LeftTrim,
            "0:04.000",
            false,
            (192_000, 384_000, 88_200),
            (192_000, 576_000, 88_200),
        ),
        (
            Target::LeftEdge,
            "0:04.000",
            false,
            (192_000, 384_000, 0),
            (192_000, 576_000, 0),
        ),
        (
            Target::EndPosition,
            "0:14.000",
            false,
            (192_000, 480_000, 0),
            (192_000, 480_000, 0),
        ),
        (
            Target::RightTrim,
            "0:14.000",
            false,
            (96_000, 576_000, 0),
            (288_000, 384_000, 0),
        ),
        (
            Target::EndPosition,
            "0:14.000",
            true,
            (0, 480_000, 0),
            (192_000, 480_000, 0),
        ),
    ];
    for (target, value, relative_set, first, last) in cases {
        let (mut app, project_path) = item_nudge_set_edge_test_app();
        let track_id = app.project.tracks()[0].id();
        let media_ref = app.project.audio_items()[0].media_ref().to_owned();
        app.project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref,
                start_sample: 288_000,
                source_offset_samples: 0,
                length_samples: 480_000,
            })
            .unwrap();
        let first_id = app.project.audio_items()[0].id();
        let last_id = app.project.audio_items()[1].id();
        let original = app.project.audio_items().to_vec();
        app.timeline.rebuild(&app.project);
        app.timeline
            .select_items(vec![(first_id, track_id), (last_id, track_id)]);
        let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
        let _ = app.update(Message::ItemNudgeTargetChanged(target));
        if relative_set {
            let _ = app.update(Message::ItemNudgeRelativeSetChanged(true));
        }
        let _ = app.update(Message::ItemNudgeValueChanged(value.to_owned()));

        let _ = app.update(Message::ApplyItemNudgeSet);

        assert!(app.item_nudge_dialog.error.is_none(), "target: {target}");
        assert_eq!(
            (
                app.project.audio_items()[0].start_sample(),
                app.project.audio_items()[0].length_samples(),
                app.project.audio_items()[0].source_offset_samples(),
            ),
            first
        );
        assert_eq!(
            (
                app.project.audio_items()[1].start_sample(),
                app.project.audio_items()[1].length_samples(),
                app.project.audio_items()[1].source_offset_samples(),
            ),
            last
        );
        assert_eq!(app.item_nudge_dialog.relative_set, relative_set);
        let edited = app.project.audio_items().to_vec();
        let _ = app.update(Message::Undo);
        assert_eq!(app.project.audio_items(), original, "target: {target}");
        let _ = app.update(Message::Redo);
        assert_eq!(app.project.audio_items(), edited, "target: {target}");
        drop(app);
        let _ = std::fs::remove_file(project_path);
    }
}

#[test]
fn item_nudge_set_edge_targets_match_native_staggered_looped_midi_selection() {
    use super::item_nudge::Target;

    let cases = [
        (
            Target::LeftTrim,
            "0:04.000",
            false,
            [(7_680, 15_360, 3_840), (7_680, 23_040, 15_360)],
        ),
        (
            Target::LeftEdge,
            "0:04.000",
            false,
            [(7_680, 15_360, 0), (7_680, 23_040, 0)],
        ),
        (
            Target::EndPosition,
            "0:14.000",
            false,
            [(7_680, 19_200, 0), (7_680, 19_200, 0)],
        ),
        (
            Target::RightTrim,
            "0:14.000",
            false,
            [(3_840, 23_040, 0), (11_520, 15_360, 0)],
        ),
        (
            Target::EndPosition,
            "0:14.000",
            true,
            [(0, 19_200, 0), (7_680, 19_200, 0)],
        ),
    ];

    for (target, value, relative_set, expected) in cases {
        let (mut app, project_path) = item_nudge_set_edge_test_app();
        let track_id = app.project.tracks()[1].id();
        app.project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick: 11_520,
                length_ticks: 19_200,
            })
            .unwrap();
        let last_id = app.project.midi_items()[1].id();
        app.project
            .apply(DawAction::SetMidiItemLoopSource {
                item_id: last_id,
                enabled: true,
            })
            .unwrap();
        app.project
            .apply(DawAction::AddMidiNotes {
                item_id: last_id,
                notes: vec![
                    MidiNoteData {
                        pitch: 60,
                        tick: 960,
                        duration: 960,
                        velocity: 96,
                    },
                    MidiNoteData {
                        pitch: 64,
                        tick: 2_880,
                        duration: 960,
                        velocity: 108,
                    },
                ],
            })
            .unwrap();
        let first_id = app.project.midi_items()[0].id();
        let original = app.project.midi_items().to_vec();
        let original_notes = original
            .iter()
            .map(|item| item.notes().to_vec())
            .collect::<Vec<_>>();
        app.timeline.rebuild(&app.project);
        app.timeline
            .select_items(vec![(first_id, track_id), (last_id, track_id)]);
        let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
        let _ = app.update(Message::ItemNudgeTargetChanged(target));
        if relative_set {
            let _ = app.update(Message::ItemNudgeRelativeSetChanged(true));
        }
        let _ = app.update(Message::ItemNudgeValueChanged(value.to_owned()));

        let _ = app.update(Message::ApplyItemNudgeSet);

        assert!(app.item_nudge_dialog.error.is_none(), "target: {target}");
        let actual = app
            .project
            .midi_items()
            .iter()
            .map(|item| {
                (
                    item.start_tick(),
                    item.length_ticks(),
                    item.source_offset_ticks(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "target: {target}");
        assert_eq!(
            app.project
                .midi_items()
                .iter()
                .map(|item| item.notes().to_vec())
                .collect::<Vec<_>>(),
            original_notes,
            "target: {target}"
        );
        let edited = app.project.midi_items().to_vec();
        let _ = app.update(Message::Undo);
        assert_eq!(app.project.midi_items(), original, "target: {target}");
        let _ = app.update(Message::Redo);
        assert_eq!(app.project.midi_items(), edited, "target: {target}");
        drop(app);
        let _ = std::fs::remove_file(project_path);
    }
}

#[test]
fn item_nudge_set_left_trim_matches_native_staggered_non_loop_midi_selection() {
    use super::item_nudge::Target;

    let (mut app, project_path) = item_nudge_set_edge_test_app();
    let track_id = app.project.tracks()[1].id();
    let first_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::SetMidiItemLoopSource {
            item_id: first_id,
            enabled: false,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 11_520,
            length_ticks: 19_200,
        })
        .unwrap();
    let second_id = app.project.midi_items()[1].id();
    app.project
        .apply(DawAction::SetMidiItemLoopSource {
            item_id: second_id,
            enabled: false,
        })
        .unwrap();
    for item_id in [first_id, second_id] {
        if item_id == first_id {
            app.project
                .apply(DawAction::AddMidiNotes {
                    item_id,
                    notes: vec![MidiNoteData {
                        pitch: 64,
                        tick: 2_880,
                        duration: 960,
                        velocity: 108,
                    }],
                })
                .unwrap();
        } else {
            app.project
                .apply(DawAction::AddMidiNotes {
                    item_id,
                    notes: vec![
                        MidiNoteData {
                            pitch: 60,
                            tick: 960,
                            duration: 960,
                            velocity: 96,
                        },
                        MidiNoteData {
                            pitch: 64,
                            tick: 2_880,
                            duration: 960,
                            velocity: 108,
                        },
                    ],
                })
                .unwrap();
        }
        app.project
            .apply(DawAction::SetMidiControllers {
                item_id,
                controllers: vec![MidiControllerData {
                    controller: 1,
                    tick: 1_920,
                    value: 80,
                }],
            })
            .unwrap();
        app.project
            .apply(DawAction::SetMidiPitchBends {
                item_id,
                pitch_bends: vec![aaadaw_core::MidiPitchBendData {
                    tick: 3_840,
                    value: 9_000,
                }],
            })
            .unwrap();
    }
    let original = app.project.midi_items().to_vec();
    app.timeline.rebuild(&app.project);
    app.timeline
        .select_items(vec![(first_id, track_id), (second_id, track_id)]);
    let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::LeftTrim));
    let _ = app.update(Message::ItemNudgeValueChanged("0:04.000".to_owned()));

    let revision = app.revision;
    let _ = app.update(Message::ApplyItemNudgeSet);

    let edited = app.project.midi_items().to_vec();
    assert_eq!(app.revision, revision + 1);
    assert!(app.item_nudge_dialog.error.is_none());
    assert_eq!(
        edited
            .iter()
            .map(|item| (
                item.start_tick(),
                item.length_ticks(),
                item.source_offset_ticks(),
                item.source_loop_length_ticks(),
            ))
            .collect::<Vec<_>>(),
        vec![(7_680, 15_360, 2_880, 18_240), (7_680, 23_040, 0, 23_040)]
    );
    assert_eq!(
        edited
            .iter()
            .map(|item| {
                item.notes()
                    .iter()
                    .map(|note| (note.id(), note.tick()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>(),
        vec![
            vec![
                (original[0].notes()[0].id(), 0),
                (original[0].notes()[1].id(), 1_920)
            ],
            vec![
                (original[1].notes()[0].id(), 4_800),
                (original[1].notes()[1].id(), 6_720)
            ],
        ]
    );
    assert_eq!(edited[0].controllers()[0].tick, 960);
    assert_eq!(edited[1].controllers()[0].tick, 5_760);
    assert_eq!(edited[0].pitch_bends()[0].tick, 2_880);
    assert_eq!(edited[1].pitch_bends()[0].tick, 7_680);
    assert_eq!(app.timeline.selected_items.len(), 2);

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items(), original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.midi_items(), edited);
    drop(app);
    let _ = std::fs::remove_file(project_path);
}

#[test]
fn item_nudge_set_edge_targets_match_native_staggered_non_loop_midi_selection() {
    use super::item_nudge::Target;

    let cases = [
        (
            Target::LeftEdge,
            "0:04.000",
            [(7_680, 15_360, 0, 19_200), (7_680, 23_040, 0, 19_200)],
            false,
        ),
        (
            Target::EndPosition,
            "0:14.000",
            [(7_680, 19_200, 0, 19_200), (7_680, 19_200, 0, 19_200)],
            false,
        ),
        (
            Target::RightTrim,
            "0:14.000",
            [(3_840, 23_040, 0, 23_040), (11_520, 15_360, 0, 15_360)],
            false,
        ),
        (
            Target::RightTrim,
            "0:14.000",
            [(7_680, 19_200, 2_880, 22_080), (7_680, 19_200, 0, 19_200)],
            true,
        ),
    ];

    for (target, value, expected, after_left_trim) in cases {
        let (mut app, project_path) = item_nudge_set_edge_test_app();
        let track_id = app.project.tracks()[1].id();
        let first_id = app.project.midi_items()[0].id();
        app.project
            .apply(DawAction::SetMidiItemLoopSource {
                item_id: first_id,
                enabled: false,
            })
            .unwrap();
        app.project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick: 11_520,
                length_ticks: 19_200,
            })
            .unwrap();
        let second_id = app.project.midi_items()[1].id();
        app.project
            .apply(DawAction::SetMidiItemLoopSource {
                item_id: second_id,
                enabled: false,
            })
            .unwrap();
        for item_id in [first_id, second_id] {
            app.project
                .apply(DawAction::AddMidiNotes {
                    item_id,
                    notes: vec![MidiNoteData {
                        pitch: 64,
                        tick: 2_880,
                        duration: 960,
                        velocity: 108,
                    }],
                })
                .unwrap();
            app.project
                .apply(DawAction::SetMidiControllers {
                    item_id,
                    controllers: vec![MidiControllerData {
                        controller: 1,
                        tick: 1_920,
                        value: 80,
                    }],
                })
                .unwrap();
            app.project
                .apply(DawAction::SetMidiPitchBends {
                    item_id,
                    pitch_bends: vec![aaadaw_core::MidiPitchBendData {
                        tick: 3_840,
                        value: 9_000,
                    }],
                })
                .unwrap();
        }
        if after_left_trim {
            app.project
                .apply(DawAction::EditMidiItemPlacement {
                    item_id: first_id,
                    start_tick: 7_680,
                    length_ticks: 15_360,
                    source_offset_ticks: 2_880,
                })
                .unwrap();
            app.project
                .apply(DawAction::SetMidiItemContents {
                    item_id: first_id,
                    source_offset_ticks: 2_880,
                    source_loop_length_ticks: 18_240,
                    content_shift_ticks: -960,
                })
                .unwrap();
            app.project
                .apply(DawAction::EditMidiItemPlacement {
                    item_id: second_id,
                    start_tick: 7_680,
                    length_ticks: 23_040,
                    source_offset_ticks: 0,
                })
                .unwrap();
            app.project
                .apply(DawAction::SetMidiItemContents {
                    item_id: second_id,
                    source_offset_ticks: 0,
                    source_loop_length_ticks: 23_040,
                    content_shift_ticks: 3_840,
                })
                .unwrap();
        }
        let original = app.project.midi_items().to_vec();
        let original_notes = original
            .iter()
            .map(|item| item.notes().to_vec())
            .collect::<Vec<_>>();
        let original_controllers = original
            .iter()
            .map(|item| item.controllers().to_vec())
            .collect::<Vec<_>>();
        let original_pitch_bends = original
            .iter()
            .map(|item| item.pitch_bends().to_vec())
            .collect::<Vec<_>>();
        app.timeline.rebuild(&app.project);
        app.timeline
            .select_items(vec![(first_id, track_id), (second_id, track_id)]);
        let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
        let _ = app.update(Message::ItemNudgeTargetChanged(target));
        let _ = app.update(Message::ItemNudgeValueChanged(value.to_owned()));

        let revision = app.revision;
        let _ = app.update(Message::ApplyItemNudgeSet);

        let edited = app.project.midi_items().to_vec();
        assert_eq!(app.revision, revision + 1, "target: {target}");
        assert!(app.item_nudge_dialog.error.is_none(), "target: {target}");
        assert_eq!(
            edited
                .iter()
                .map(|item| (
                    item.start_tick(),
                    item.length_ticks(),
                    item.source_offset_ticks(),
                    item.source_loop_length_ticks(),
                ))
                .collect::<Vec<_>>(),
            expected,
            "target: {target}"
        );
        assert_eq!(
            edited
                .iter()
                .map(|item| item.notes().to_vec())
                .collect::<Vec<_>>(),
            original_notes,
            "target: {target}"
        );
        assert_eq!(
            edited
                .iter()
                .map(|item| item.controllers().to_vec())
                .collect::<Vec<_>>(),
            original_controllers,
            "target: {target}"
        );
        assert_eq!(
            edited
                .iter()
                .map(|item| item.pitch_bends().to_vec())
                .collect::<Vec<_>>(),
            original_pitch_bends,
            "target: {target}"
        );
        assert_eq!(app.timeline.selected_items.len(), 2);

        let _ = app.update(Message::Undo);
        assert_eq!(app.project.midi_items(), original, "target: {target}");
        let _ = app.update(Message::Redo);
        assert_eq!(app.project.midi_items(), edited, "target: {target}");
        drop(app);
        let _ = std::fs::remove_file(project_path);
    }
}

#[test]
fn item_nudge_set_end_position_relative_set_applies_a_delta_from_the_last_edge() {
    use super::item_nudge::Target;

    let (mut app, project_path) = item_nudge_set_edge_test_app();
    let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::EndPosition));
    assert!(!app.item_nudge_dialog.relative_set);
    let _ = app.update(Message::ItemNudgeRelativeSetChanged(true));
    let _ = app.update(Message::ItemNudgeValueChanged("0:14.000".to_owned()));

    let _ = app.update(Message::ApplyItemNudgeSet);

    assert!(app.item_nudge_dialog.error.is_none());
    assert!(app.item_nudge_dialog.relative_set);
    assert_eq!(app.project.audio_items()[0].start_sample(), 192_000);
    assert_eq!(app.project.audio_items()[0].length_samples(), 480_000);
    assert_eq!(app.project.audio_items()[0].source_offset_samples(), 0);
    assert_eq!(app.project.midi_items()[0].start_tick(), 7_680);
    assert_eq!(app.project.midi_items()[0].length_ticks(), 19_200);
    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 0);
    drop(app);
    let _ = std::fs::remove_file(project_path);
}

#[test]
fn item_nudge_set_edge_targets_hide_relative_set_and_reject_atomic_overtrim() {
    use super::item_nudge::Target;

    let (mut app, project_path) = item_nudge_set_edge_test_app();
    let original_audio = app.project.audio_items()[0].clone();
    let original_midi = app.project.midi_items()[0].clone();
    let revision = app.revision;
    let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
    let _ = app.update(Message::ItemNudgeRelativeSetChanged(true));
    assert!(app.item_nudge_dialog.relative_set);
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::LeftTrim));
    assert!(!app.item_nudge_dialog.relative_set);
    let _ = app.update(Message::ItemNudgeRelativeSetChanged(true));
    assert!(!app.item_nudge_dialog.relative_set);
    let _ = app.update(Message::ItemNudgeValueChanged("0:12.000".to_owned()));

    let _ = app.update(Message::ApplyItemNudgeSet);

    assert_eq!(app.revision, revision);
    assert_eq!(app.project.audio_items()[0], original_audio);
    assert_eq!(app.project.midi_items()[0], original_midi);
    assert!(app.item_nudge_dialog.error.is_some());
    drop(app);
    let _ = std::fs::remove_file(project_path);
}

#[test]
fn item_nudge_without_selection_or_with_invalid_amount_does_not_edit_project() {
    let mut app = item_snap_test_app(0, 0);
    let original_revision = app.revision;
    app.timeline.clear_selected_items();
    let _ = app.update(Message::NudgeSelectedItems(1));
    assert_eq!(app.revision, original_revision);
    assert_eq!(app.project.audio_items()[0].start_sample(), 0);

    let _ = app.update(Message::SelectAllItems);
    let _ = app.update(Message::ItemNudgeValueChanged("-1".to_owned()));
    let _ = app.update(Message::NudgeSelectedItems(1));
    assert_eq!(app.revision, original_revision);
    assert_eq!(app.project.audio_items()[0].start_sample(), 0);
    assert_eq!(app.project.midi_items()[0].start_tick(), 0);
}

#[test]
fn item_nudge_edit_cursor_target_moves_cursor_without_changing_items() {
    let mut app = item_snap_test_app(0, 0);
    let starting_cursor = app.timeline.edit_cursor_tick;
    let start_sample = app.project.sample_at_tick(starting_cursor).unwrap();
    let delta_samples = (f64::from(app.project.settings().sample_rate()) / 1_000.0).round() as u64;
    let expected_tick = app
        .project
        .tick_at_sample(start_sample + delta_samples)
        .unwrap();

    let _ = app.update(Message::ItemNudgeTargetChanged(
        super::item_nudge::Target::EditCursor,
    ));
    let _ = app.update(Message::NudgeSelectedItems(1));

    assert_eq!(app.timeline.edit_cursor_tick, expected_tick);
    assert_eq!(app.project.audio_items()[0].start_sample(), 0);
    assert_eq!(app.project.midi_items()[0].start_tick(), 0);
}

#[test]
fn item_nudge_set_can_set_the_edit_cursor_to_a_project_time() {
    let mut app = item_snap_test_app(0, 0);
    let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
    let _ = app.update(Message::ItemNudgeTargetChanged(
        super::item_nudge::Target::EditCursor,
    ));
    let _ = app.update(Message::ItemNudgeValueChanged("0:01.000".to_owned()));
    let _ = app.update(Message::ApplyItemNudgeSet);

    let target_sample = u64::from(app.project.settings().sample_rate());
    assert_eq!(
        app.timeline.edit_cursor_tick,
        app.project.tick_at_sample(target_sample).unwrap()
    );
    assert_eq!(app.project.audio_items()[0].start_sample(), 0);
    assert_eq!(app.project.midi_items()[0].start_tick(), 0);
}

#[test]
fn item_nudge_set_contents_matches_native_midi_source_alignment_snap_and_undoes() {
    use super::item_nudge::Target;

    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let midi_track = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: midi_track,
            start_tick: 3_840,
            length_ticks: 9_600,
        })
        .unwrap();
    let midi_id = app.project.midi_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(midi_id, midi_track)]);
    let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
    let _ = app.update(Message::ItemNudgeRelativeSetChanged(true));
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Contents));
    let _ = app.update(Message::ItemNudgeValueChanged("0:03.000".to_owned()));

    let _ = app.update(Message::ApplyItemNudgeSet);

    let item = &app.project.midi_items()[0];
    assert_eq!(item.start_tick(), 3_840);
    assert_eq!(item.length_ticks(), 9_600);
    assert_eq!(item.source_loop_length_ticks(), 9_600);
    assert_eq!(item.source_offset_ticks(), 7_680);
    assert!(!app.item_nudge_dialog.relative_set);
    assert_eq!(app.timeline.selected_items.len(), 1);
    assert!(app.timeline.selected_items.contains(&midi_id));
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 0);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 7_680);

    let _ = app.update(Message::ItemNudgeSnapToUnitChanged(true));
    let _ = app.update(Message::ItemNudgeValueChanged("0:03.500".to_owned()));
    let _ = app.update(Message::ApplyItemNudgeSet);

    let item = &app.project.midi_items()[0];
    assert_eq!(item.start_tick(), 3_840);
    assert_eq!(item.length_ticks(), 9_600);
    assert_eq!(item.source_offset_ticks(), 5_760);
    assert!(app.item_nudge_dialog.snap_to_unit);
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 7_680);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 5_760);
}

#[test]
fn item_nudge_set_looped_midi_contents_matches_native_across_tempo_change() {
    use super::item_nudge::Target;

    let mut app = App::default();
    app.project
        .apply(DawAction::SetTempo {
            start_tick: 7_680,
            bpm: 60.0,
        })
        .unwrap();
    let sample_rate = f64::from(app.project.settings().sample_rate());
    let start_sample = app.project.sample_at_project_time(3.9).unwrap();
    let end_sample = app.project.sample_at_project_time(8.9).unwrap();
    let start_tick = app.project.tick_at_sample(start_sample).unwrap();
    let end_tick = app.project.tick_at_sample(end_sample).unwrap();
    assert_eq!(start_tick, 7_488);
    assert_eq!(end_tick - start_tick, 4_896);

    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick,
            length_ticks: end_tick - start_tick,
        })
        .unwrap();
    let item_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![
                MidiNoteData {
                    pitch: 60,
                    tick: 960,
                    duration: 960,
                    velocity: 96,
                },
                MidiNoteData {
                    pitch: 64,
                    tick: 2_880,
                    duration: 960,
                    velocity: 108,
                },
            ],
        })
        .unwrap();
    let original = app.project.midi_items()[0].clone();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(item_id, track_id)]);
    let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Contents));
    let _ = app.update(Message::ItemNudgeValueChanged("0:04.150".to_owned()));

    let revision = app.revision;
    let _ = app.update(Message::ApplyItemNudgeSet);

    let set_after = app.project.midi_items()[0].clone();
    assert_eq!(app.revision, revision + 1);
    assert_eq!(set_after.start_tick(), original.start_tick());
    assert_eq!(set_after.length_ticks(), original.length_ticks());
    assert_eq!(
        set_after.source_loop_length_ticks(),
        original.source_loop_length_ticks()
    );
    assert_eq!(set_after.source_offset_ticks(), 4_416);
    assert_eq!(set_after.notes(), original.notes());
    assert!(
        (app.project
            .midi_source_offset_samples(start_tick, 4_416)
            .unwrap()
            - 2.3 * sample_rate)
            .abs()
            < 1.0
    );
    assert_eq!(app.timeline.selected_items.len(), 1);
    assert!(app.timeline.selected_items.contains(&item_id));
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0], original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.midi_items()[0], set_after);
}

#[test]
fn item_nudge_set_looped_midi_contents_matches_native_linear_tempo_phase() {
    use super::item_nudge::Target;

    let mut app = App::default();
    app.project
        .apply(DawAction::SetTempo {
            start_tick: 7_680,
            bpm: 60.0,
        })
        .unwrap();
    app.project
        .apply(DawAction::SetTempoCurve {
            start_tick: 0,
            curve: aaadaw_core::TempoCurve::Linear,
        })
        .unwrap();
    let sample_rate = f64::from(app.project.settings().sample_rate());
    let sample_at = |seconds: f64| (seconds * sample_rate).round() as u64;
    let start_sample = sample_at(3.9);
    let end_sample = sample_at(8.9);
    let start_tick = app.project.tick_at_sample(start_sample).unwrap();
    let end_tick = app.project.tick_at_sample(end_sample).unwrap();
    let length_ticks = end_tick - start_tick;

    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick,
            length_ticks,
        })
        .unwrap();
    let item_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![
                MidiNoteData {
                    pitch: 60,
                    tick: 960,
                    duration: 960,
                    velocity: 96,
                },
                MidiNoteData {
                    pitch: 64,
                    tick: 2_880,
                    duration: 960,
                    velocity: 108,
                },
            ],
        })
        .unwrap();
    let original = app.project.midi_items()[0].clone();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(item_id, track_id)]);
    let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Contents));
    let _ = app.update(Message::ItemNudgeValueChanged("0:04.150".to_owned()));

    let revision = app.revision;
    let _ = app.update(Message::ApplyItemNudgeSet);

    let set_after = app.project.midi_items()[0].clone();
    assert_eq!(app.revision, revision + 1);
    assert_eq!(set_after.start_tick(), original.start_tick());
    assert_eq!(set_after.length_ticks(), original.length_ticks());
    assert_eq!(
        set_after.source_loop_length_ticks(),
        original.source_loop_length_ticks()
    );
    assert_eq!(set_after.source_loop_length_ticks(), 4_985);
    assert_eq!(set_after.source_offset_ticks(), 4_538);
    assert!((set_after.source_offset_ticks() as f64 - 4_537.985).abs() < 0.02);
    let source_offset_seconds = app
        .project
        .midi_source_offset_samples(start_tick, set_after.source_offset_ticks())
        .unwrap()
        / sample_rate;
    assert!((source_offset_seconds - 3.041_994_609).abs() < 0.000_03);
    assert_eq!(set_after.notes(), original.notes());
    assert_eq!(app.timeline.selected_items.len(), 1);
    assert!(app.timeline.selected_items.contains(&item_id));
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0], original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.midi_items()[0], set_after);
}

#[test]
fn item_nudge_set_contents_rebases_non_loop_midi_and_preserves_undoable_source_data() {
    use super::item_nudge::Target;
    use aaadaw_core::{MidiPitchBendData, NoteId};

    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 3_840,
            length_ticks: 9_600,
        })
        .unwrap();
    let item_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::SetMidiItemLoopSource {
            item_id,
            enabled: false,
        })
        .unwrap();
    app.project
        .apply(DawAction::EditMidiItemPlacement {
            item_id,
            start_tick: 3_840,
            length_ticks: 9_600,
            source_offset_ticks: 192,
        })
        .unwrap();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![
                MidiNoteData {
                    pitch: 60,
                    tick: 960,
                    duration: 1_920,
                    velocity: 96,
                },
                MidiNoteData {
                    pitch: 64,
                    tick: 4_800,
                    duration: 1_920,
                    velocity: 108,
                },
            ],
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![MidiControllerData {
                controller: 1,
                tick: 2_400,
                value: 80,
            }],
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![MidiPitchBendData {
                tick: 3_600,
                value: 9_000,
            }],
        })
        .unwrap();
    let original = app.project.midi_items()[0].clone();
    let original_note_ids = original
        .notes()
        .iter()
        .map(|note| note.id())
        .collect::<Vec<NoteId>>();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(item_id, track_id)]);
    let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Contents));
    let _ = app.update(Message::ItemNudgeValueChanged("0:03.000".to_owned()));

    let revision = app.revision;
    let _ = app.update(Message::ApplyItemNudgeSet);

    let set_after = app.project.midi_items()[0].clone();
    assert_eq!(app.revision, revision + 1);
    assert!(!set_after.is_loop_source());
    assert_eq!(set_after.start_tick(), 3_840);
    assert_eq!(set_after.length_ticks(), 9_600);
    assert_eq!(set_after.source_offset_ticks(), 0);
    assert_eq!(set_after.source_loop_length_ticks(), 11_520);
    assert_eq!(
        set_after
            .notes()
            .iter()
            .map(|note| (note.id(), note.tick(), note.duration()))
            .collect::<Vec<_>>(),
        vec![
            (original_note_ids[0], 2_880, 1_920),
            (original_note_ids[1], 6_720, 1_920),
        ]
    );
    assert_eq!(set_after.controllers()[0].tick, 4_320);
    assert_eq!(set_after.pitch_bends()[0].tick, 5_520);
    assert_eq!(app.timeline.selected_items.len(), 1);
    assert!(app.timeline.selected_items.contains(&item_id));
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0], original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.midi_items()[0], set_after);

    let _ = app.update(Message::Undo);
    let _ = app.update(Message::ItemNudgeValueChanged("0:01.000".to_owned()));
    let _ = app.update(Message::ApplyItemNudgeSet);
    let set_before_item_start = &app.project.midi_items()[0];
    assert!(!set_before_item_start.is_loop_source());
    assert_eq!(set_before_item_start.start_tick(), 3_840);
    assert_eq!(set_before_item_start.length_ticks(), 9_600);
    assert_eq!(set_before_item_start.source_offset_ticks(), 1_920);
    assert_eq!(set_before_item_start.source_loop_length_ticks(), 9_600);
    assert_eq!(set_before_item_start.notes(), original.notes());
    assert_eq!(set_before_item_start.controllers(), original.controllers());
    assert_eq!(set_before_item_start.pitch_bends(), original.pitch_bends());
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0], original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 1_920);
}

#[test]
fn item_nudge_set_contents_aligns_audio_loop_and_signed_source_offsets() {
    use super::item_nudge::Target;

    let file_id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
    let project_path = std::env::temp_dir().join(format!(
        "aaadaw-ui-item-nudge-audio-set-contents-{}-{file_id}.aaadaw",
        std::process::id()
    ));
    let media_ref = "asset://item-nudge-audio-set-contents-test";
    let mut store = ProjectStore::open(&project_path).expect("project store should open");
    store
        .import_audio_asset(
            media_ref,
            "audio-set-contents.wav",
            std::io::Cursor::new([0_u8; 4]),
        )
        .expect("test asset should import");
    store
        .set_audio_asset_metadata(
            media_ref,
            &aaadaw_storage::AudioAssetMetadata {
                container: "wav".to_owned(),
                codec: "pcm".to_owned(),
                sample_rate: Some(96_000),
                channel_count: Some(1),
                bits_per_sample: Some(16),
                frame_count: Some(384_000),
                duration_nanos: None,
                byte_len: Some(4),
            },
        )
        .expect("test metadata should store");
    store.close().expect("project store should close");

    let mut app = App {
        project_path: Some(project_path.clone()),
        ..App::default()
    };
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    let project_rate = u64::from(app.project.settings().sample_rate());
    app.project
        .apply(DawAction::InsertLoopedAudioItem {
            track_id,
            media_ref: media_ref.to_owned(),
            start_sample: project_rate * 2,
            source_offset_samples: 9_600,
            length_samples: project_rate * 5,
            muted: false,
            phase_inverted: false,
            source_section_start_samples: 0,
            source_section_length_samples: 384_000,
        })
        .unwrap();
    let item_id = app.project.audio_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(item_id, track_id)]);
    let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Contents));
    let _ = app.update(Message::ItemNudgeValueChanged("0:03.000".to_owned()));

    let _ = app.update(Message::ApplyItemNudgeSet);

    let item = &app.project.audio_items()[0];
    assert_eq!(item.source_offset_samples(), 288_000);
    assert_eq!(item.start_sample(), project_rate * 2);
    assert_eq!(item.length_samples(), project_rate * 5);
    assert!(item.is_loop_source());
    assert_eq!(app.timeline.selected_items.len(), 1);
    assert!(app.timeline.selected_items.contains(&item_id));
    assert!(app.item_nudge_dialog.error.is_none());
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].source_offset_samples(), 9_600);
    let _ = app.update(Message::Redo);
    assert_eq!(
        app.project.audio_items()[0].source_offset_samples(),
        288_000
    );

    let _ = app.update(Message::ItemNudgeSnapToUnitChanged(true));
    let _ = app.update(Message::ItemNudgeValueChanged("0:03.500".to_owned()));
    let _ = app.update(Message::ApplyItemNudgeSet);
    assert_eq!(
        app.project.audio_items()[0].source_offset_samples(),
        192_000
    );
    assert!(app.item_nudge_dialog.error.is_none());

    app.project
        .apply(DawAction::SetAudioItemLoopSource {
            item_id,
            enabled: false,
        })
        .unwrap();
    app.project
        .apply(DawAction::EditAudioItem {
            item_id,
            media_ref: media_ref.to_owned(),
            start_sample: project_rate * 2,
            source_offset_samples: 9_600,
            length_samples: project_rate * 5,
        })
        .unwrap();
    let _ = app.update(Message::ItemNudgeSnapToUnitChanged(false));
    let _ = app.update(Message::ItemNudgeValueChanged("0:03.000".to_owned()));
    let _ = app.update(Message::ApplyItemNudgeSet);
    assert_eq!(
        app.project.audio_items()[0].source_offset_samples(),
        -96_000
    );
    assert!(!app.project.audio_items()[0].is_loop_source());
    assert_eq!(
        app.project.audio_items()[0].start_sample(),
        project_rate * 2
    );
    assert_eq!(
        app.project.audio_items()[0].length_samples(),
        project_rate * 5
    );
    assert!(app.item_nudge_dialog.error.is_none());

    drop(app);
    let _ = std::fs::remove_file(project_path);
}

#[test]
fn item_nudge_contents_wraps_looped_midi_offsets_and_undoes_atomically() {
    use super::item_nudge::Target;

    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let midi_track = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: midi_track,
            start_tick: 0,
            length_ticks: 9_600,
        })
        .unwrap();
    let midi_id = app.project.midi_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(midi_id, midi_track)]);
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Contents));
    let _ = app.update(Message::ItemNudgeValueChanged("250".to_owned()));
    let revision = app.revision;

    let _ = app.update(Message::NudgeSelectedItems(1));
    let item = &app.project.midi_items()[0];
    assert_eq!(app.revision, revision + 1);
    assert_eq!(item.start_tick(), 0);
    assert_eq!(item.length_ticks(), 9_600);
    assert_eq!(item.source_offset_ticks(), 9_120);
    assert_eq!(item.source_loop_length_ticks(), 9_600);
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 0);
    let _ = app.update(Message::NudgeSelectedItems(-1));
    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 480);
    assert!(app.item_nudge_dialog.error.is_none());
}

#[test]
fn item_nudge_non_looped_midi_contents_matches_native_source_boundary_and_undoes() {
    use super::item_nudge::Target;
    use aaadaw_core::MidiPitchBendData;

    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 3_840,
            length_ticks: 9_600,
        })
        .unwrap();
    let item_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::SetMidiItemLoopSource {
            item_id,
            enabled: false,
        })
        .unwrap();
    app.project
        .apply(DawAction::EditMidiItemPlacement {
            item_id,
            start_tick: 3_840,
            length_ticks: 9_600,
            source_offset_ticks: 192,
        })
        .unwrap();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![
                MidiNoteData {
                    pitch: 60,
                    tick: 2_880,
                    duration: 1_920,
                    velocity: 96,
                },
                MidiNoteData {
                    pitch: 64,
                    tick: 6_720,
                    duration: 1_920,
                    velocity: 108,
                },
            ],
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![MidiControllerData {
                controller: 1,
                tick: 2_400,
                value: 80,
            }],
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![MidiPitchBendData {
                tick: 3_600,
                value: 9_000,
            }],
        })
        .unwrap();
    let original = app.project.midi_items()[0].clone();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(item_id, track_id)]);
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Contents));
    let _ = app.update(Message::ItemNudgeValueChanged("250".to_owned()));

    let _ = app.update(Message::NudgeSelectedItems(-1));
    let moved_left = app.project.midi_items()[0].clone();
    assert_eq!(moved_left.source_offset_ticks(), 672);
    assert_eq!(moved_left.source_loop_length_ticks(), 9_600);
    assert_eq!(moved_left.notes(), original.notes());
    assert_eq!(moved_left.controllers(), original.controllers());
    assert_eq!(moved_left.pitch_bends(), original.pitch_bends());
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0], original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 672);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0], original);

    let _ = app.update(Message::NudgeSelectedItems(1));
    let moved_right = app.project.midi_items()[0].clone();
    assert_eq!(moved_right.source_offset_ticks(), 0);
    assert_eq!(moved_right.source_loop_length_ticks(), 9_888);
    assert_eq!(
        moved_right
            .notes()
            .iter()
            .map(|note| (note.tick(), note.duration()))
            .collect::<Vec<_>>(),
        vec![(3_168, 1_920), (7_008, 1_920)]
    );
    assert_eq!(moved_right.controllers()[0].tick, 2_688);
    assert_eq!(moved_right.pitch_bends()[0].tick, 3_888);
    assert_eq!(app.timeline.selected_items.len(), 1);
    assert!(app.timeline.selected_items.contains(&item_id));
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0], original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.midi_items()[0], moved_right);

    let _ = app.update(Message::NudgeSelectedItems(-1));
    let moved_left_again = app.project.midi_items()[0].clone();
    assert_eq!(moved_left_again.source_offset_ticks(), 0);
    assert_eq!(moved_left_again.source_loop_length_ticks(), 9_408);
    assert_eq!(
        moved_left_again
            .notes()
            .iter()
            .map(|note| (note.tick(), note.duration()))
            .collect::<Vec<_>>(),
        vec![(2_688, 1_920), (6_528, 1_920)]
    );
    assert_eq!(moved_left_again.controllers()[0].tick, 2_208);
    assert_eq!(moved_left_again.pitch_bends()[0].tick, 3_408);
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0], moved_right);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.midi_items()[0], moved_left_again);
}

#[test]
fn item_nudge_non_looped_midi_contents_rebases_left_from_zero_offset() {
    use super::item_nudge::Target;
    use aaadaw_core::MidiPitchBendData;

    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 3_840,
            length_ticks: 9_600,
        })
        .unwrap();
    let item_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::SetMidiItemLoopSource {
            item_id,
            enabled: false,
        })
        .unwrap();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![
                MidiNoteData {
                    pitch: 60,
                    tick: 2_880,
                    duration: 1_920,
                    velocity: 96,
                },
                MidiNoteData {
                    pitch: 64,
                    tick: 6_720,
                    duration: 1_920,
                    velocity: 108,
                },
            ],
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![MidiControllerData {
                controller: 1,
                tick: 2_400,
                value: 80,
            }],
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![MidiPitchBendData {
                tick: 3_600,
                value: 9_000,
            }],
        })
        .unwrap();
    let original = app.project.midi_items()[0].clone();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(item_id, track_id)]);
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Contents));
    let _ = app.update(Message::ItemNudgeValueChanged("250".to_owned()));

    let _ = app.update(Message::NudgeSelectedItems(-1));

    let moved_left = app.project.midi_items()[0].clone();
    assert_eq!(moved_left.source_offset_ticks(), 0);
    assert_eq!(moved_left.source_loop_length_ticks(), 9_120);
    assert_eq!(
        moved_left
            .notes()
            .iter()
            .map(|note| (note.tick(), note.duration()))
            .collect::<Vec<_>>(),
        vec![(2_400, 1_920), (6_240, 1_920)]
    );
    assert_eq!(moved_left.controllers()[0].tick, 1_920);
    assert_eq!(moved_left.pitch_bends()[0].tick, 3_120);
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0], original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.midi_items()[0], moved_left);
}

#[test]
fn item_nudge_non_looped_midi_contents_clamps_events_at_source_zero_and_offsets_remainder() {
    use super::item_nudge::Target;
    use aaadaw_core::MidiPitchBendData;

    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 3_840,
            length_ticks: 9_600,
        })
        .unwrap();
    let item_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::SetMidiItemLoopSource {
            item_id,
            enabled: false,
        })
        .unwrap();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![
                MidiNoteData {
                    pitch: 60,
                    tick: 100,
                    duration: 300,
                    velocity: 96,
                },
                MidiNoteData {
                    pitch: 64,
                    tick: 6_720,
                    duration: 1_920,
                    velocity: 108,
                },
            ],
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![MidiControllerData {
                controller: 1,
                tick: 50,
                value: 80,
            }],
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiPitchBends {
            item_id,
            pitch_bends: vec![MidiPitchBendData {
                tick: 300,
                value: 9_000,
            }],
        })
        .unwrap();
    let original = app.project.midi_items()[0].clone();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(item_id, track_id)]);
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Contents));
    let _ = app.update(Message::ItemNudgeValueChanged("250".to_owned()));

    let _ = app.update(Message::NudgeSelectedItems(-1));

    let moved_left = app.project.midi_items()[0].clone();
    assert_eq!(moved_left.source_offset_ticks(), 430);
    assert_eq!(moved_left.source_loop_length_ticks(), 9_550);
    assert_eq!(
        moved_left
            .notes()
            .iter()
            .map(|note| (note.tick(), note.duration()))
            .collect::<Vec<_>>(),
        vec![(50, 300), (6_670, 1_920)]
    );
    assert_eq!(moved_left.controllers()[0].tick, 0);
    assert_eq!(moved_left.pitch_bends()[0].tick, 250);
    assert_eq!(app.timeline.selected_items.len(), 1);
    assert!(app.timeline.selected_items.contains(&item_id));
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0], original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.midi_items()[0], moved_left);
}

#[test]
fn item_nudge_midi_contents_matches_reaper_when_snap_to_unit_is_enabled() {
    use super::item_nudge::Target;

    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let midi_track = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: midi_track,
            start_tick: 0,
            length_ticks: 9_600,
        })
        .unwrap();
    let midi_id = app.project.midi_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(midi_id, midi_track)]);
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Contents));
    let _ = app.update(Message::ItemNudgeSnapToUnitChanged(true));
    let _ = app.update(Message::ItemNudgeValueChanged("250".to_owned()));

    let _ = app.update(Message::NudgeSelectedItems(1));

    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 9_120);
    assert!(app.item_nudge_dialog.error.is_none());
}

#[test]
fn item_nudge_midi_contents_matches_native_stepped_tempo_boundary() {
    use super::item_nudge::Target;

    let mut app = App::default();
    app.project
        .apply(DawAction::SetTempo {
            start_tick: 3_840,
            bpm: 60.0,
        })
        .unwrap();
    let _ = app.update(Message::AddTrack);
    let midi_track = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: midi_track,
            start_tick: 3_840,
            length_ticks: 4_800,
        })
        .unwrap();
    let midi_id = app.project.midi_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(midi_id, midi_track)]);
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Contents));
    let _ = app.update(Message::ItemNudgeValueChanged("250".to_owned()));

    let _ = app.update(Message::NudgeSelectedItems(1));

    assert_eq!(app.project.midi_items()[0].start_tick(), 3_840);
    assert_eq!(app.project.midi_items()[0].length_ticks(), 4_800);
    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 4_320);
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::NudgeSelectedItems(-1));

    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 0);
    assert!(app.item_nudge_dialog.error.is_none());
}

#[test]
fn item_nudge_midi_contents_uses_tempo_at_item_start_across_tempo_change() {
    use super::item_nudge::Target;

    let mut app = App::default();
    app.project
        .apply(DawAction::SetTempo {
            start_tick: 7_680,
            bpm: 60.0,
        })
        .unwrap();
    let _ = app.update(Message::AddTrack);
    let midi_track = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: midi_track,
            start_tick: 7_488,
            length_ticks: 4_896,
        })
        .unwrap();
    let midi_id = app.project.midi_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(midi_id, midi_track)]);
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Contents));
    let _ = app.update(Message::ItemNudgeValueChanged("250".to_owned()));

    let _ = app.update(Message::NudgeSelectedItems(1));

    assert_eq!(app.project.midi_items()[0].start_tick(), 7_488);
    assert_eq!(app.project.midi_items()[0].length_ticks(), 4_896);
    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 4_416);
    assert!(app.item_nudge_dialog.error.is_none());
}

#[test]
fn item_nudge_midi_contents_matches_native_linear_tempo_phase() {
    use super::item_nudge::Target;

    let mut app = App::default();
    app.project
        .apply(DawAction::SetTempo {
            start_tick: 7_680,
            bpm: 60.0,
        })
        .unwrap();
    app.project
        .apply(DawAction::SetTempoCurve {
            start_tick: 0,
            curve: aaadaw_core::TempoCurve::Linear,
        })
        .unwrap();
    let sample_rate = app.project.settings().sample_rate();
    let sample_at = |seconds: f64| (seconds * f64::from(sample_rate)).round() as u64;
    let start_tick = app.project.tick_at_sample(sample_at(3.9)).unwrap();
    let end_tick = app.project.tick_at_sample(sample_at(8.9)).unwrap();
    let length_ticks = end_tick - start_tick;
    let _ = app.update(Message::AddTrack);
    let midi_track = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: midi_track,
            start_tick,
            length_ticks,
        })
        .unwrap();
    let midi_id = app.project.midi_items()[0].id();
    let source_loop_length = app.project.midi_items()[0].source_loop_length_ticks();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(midi_id, midi_track)]);
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Contents));
    let _ = app.update(Message::ItemNudgeValueChanged("250".to_owned()));

    let _ = app.update(Message::NudgeSelectedItems(1));

    assert_eq!(app.project.midi_items()[0].start_tick(), start_tick);
    assert_eq!(app.project.midi_items()[0].length_ticks(), length_ticks);
    assert_eq!(source_loop_length, 4_985);
    // REAPER reports 4537.985 PPQ; the project stores the nearest integer phase.
    let source_phase = app.project.midi_items()[0].source_offset_ticks();
    assert_eq!(source_phase, 4_538);
    assert!((source_phase as f64 - 4_537.985).abs() < 0.02);
    let source_offset_seconds = app
        .project
        .midi_source_offset_samples(start_tick, source_phase)
        .unwrap()
        / f64::from(sample_rate);
    assert!((source_offset_seconds - 3.041_994_609).abs() < 0.000_03);
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::NudgeSelectedItems(-1));

    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 0);
    assert!(app.item_nudge_dialog.error.is_none());
}

#[test]
fn item_nudge_nonloop_midi_left_trim_matches_linear_tempo_event_edges() {
    use super::item_nudge::Target;

    let mut app = App::default();
    app.project
        .apply(DawAction::SetTempo {
            start_tick: 7_680,
            bpm: 60.0,
        })
        .unwrap();
    app.project
        .apply(DawAction::SetTempoCurve {
            start_tick: 0,
            curve: aaadaw_core::TempoCurve::Linear,
        })
        .unwrap();
    let sample_rate = app.project.settings().sample_rate();
    let sample_at = |seconds: f64| (seconds * f64::from(sample_rate)).round() as u64;
    let start_tick = app.project.tick_at_sample(sample_at(3.9)).unwrap();
    let end_tick = app.project.tick_at_sample(sample_at(8.9)).unwrap();
    let length_ticks = end_tick - start_tick;
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick,
            length_ticks,
        })
        .unwrap();
    let midi_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::SetMidiItemLoopSource {
            item_id: midi_id,
            enabled: false,
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiItemContents {
            item_id: midi_id,
            source_offset_ticks: 0,
            source_loop_length_ticks: 5_760,
            content_shift_ticks: 0,
        })
        .unwrap();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id: midi_id,
            notes: vec![
                MidiNoteData {
                    pitch: 60,
                    tick: 960,
                    duration: 960,
                    velocity: 96,
                },
                MidiNoteData {
                    pitch: 64,
                    tick: 2_880,
                    duration: 960,
                    velocity: 108,
                },
            ],
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiControllers {
            item_id: midi_id,
            controllers: vec![MidiControllerData {
                controller: 1,
                tick: 4_800,
                value: 80,
            }],
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiPitchBends {
            item_id: midi_id,
            pitch_bends: vec![aaadaw_core::MidiPitchBendData {
                tick: 4_500,
                value: 6_000,
            }],
        })
        .unwrap();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(midi_id, track_id)]);
    let original = app.project.midi_items()[0].clone();
    let _ = app.update(Message::ItemNudgeModeChanged(
        super::item_nudge::Mode::Nudge,
    ));
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::LeftTrim));
    let _ = app.update(Message::ItemNudgeValueChanged("250".to_owned()));

    let revision = app.revision;
    let _ = app.update(Message::NudgeSelectedItems(1));

    let edited = app.project.midi_items()[0].clone();
    assert_eq!(app.revision, revision + 1);
    assert_eq!(original.start_tick(), 6_119);
    assert_eq!(edited.start_tick(), 6_418);
    assert_eq!(original.length_ticks(), 4_985);
    assert_eq!(edited.length_ticks(), 4_686);
    assert_eq!(edited.source_offset_ticks(), 0);
    assert_eq!(edited.source_loop_length_ticks(), 5_462);
    assert_eq!(
        edited
            .notes()
            .iter()
            .map(|note| (note.tick(), note.duration()))
            .collect::<Vec<_>>(),
        vec![(662, 960), (2_582, 960)]
    );
    assert_eq!(
        edited
            .controllers()
            .iter()
            .map(|event| event.tick)
            .collect::<Vec<_>>(),
        vec![4_502]
    );
    assert_eq!(
        edited
            .pitch_bends()
            .iter()
            .map(|event| event.tick)
            .collect::<Vec<_>>(),
        vec![4_202]
    );
    assert_eq!(app.timeline.selected_items.len(), 1);
    assert!(app.timeline.selected_items.contains(&midi_id));
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0], original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.midi_items()[0], edited);
}

#[test]
fn item_nudge_mixed_contents_fails_atomically_when_audio_metadata_is_missing() {
    let mut app = item_snap_test_app(0, 0);
    let revision = app.revision;
    let _ = app.update(Message::ItemNudgeTargetChanged(
        super::item_nudge::Target::Contents,
    ));
    let _ = app.update(Message::NudgeSelectedItems(1));

    assert_eq!(app.project.audio_items()[0].start_sample(), 0);
    assert_eq!(app.project.audio_items()[0].source_offset_samples(), 120);
    assert_eq!(app.project.midi_items()[0].start_tick(), 0);
    assert!(
        app.item_nudge_dialog
            .error
            .as_deref()
            .is_some_and(|error| error.contains("audio media store"))
    );
    assert_eq!(app.revision, revision);

    let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
    let _ = app.update(Message::ItemNudgeValueChanged("0:03.000".to_owned()));
    let _ = app.update(Message::ApplyItemNudgeSet);
    assert_eq!(app.project.audio_items()[0].source_offset_samples(), 120);
    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 0);
    assert_eq!(app.revision, revision);
    assert!(
        app.item_nudge_dialog
            .error
            .as_deref()
            .is_some_and(|error| error.contains("audio media store"))
    );
}

#[test]
fn item_nudge_mixed_contents_moves_audio_and_midi_in_one_undoable_edit() {
    use super::item_nudge::Target;

    let file_id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
    let project_path = std::env::temp_dir().join(format!(
        "aaadaw-ui-item-nudge-mixed-contents-{}-{file_id}.aaadaw",
        std::process::id()
    ));
    let audio_ref = "asset://item-nudge-mixed-contents-test";
    let mut store = ProjectStore::open(&project_path).expect("project store should open");
    store
        .import_audio_asset(
            audio_ref,
            "mixed-contents.wav",
            std::io::Cursor::new([0_u8; 4]),
        )
        .expect("test asset should import");
    store
        .set_audio_asset_metadata(
            audio_ref,
            &aaadaw_storage::AudioAssetMetadata {
                container: "wav".to_owned(),
                codec: "pcm".to_owned(),
                sample_rate: Some(48_000),
                channel_count: Some(1),
                bits_per_sample: Some(16),
                frame_count: Some(480_000),
                duration_nanos: None,
                byte_len: Some(4),
            },
        )
        .expect("test metadata should store");
    store.close().expect("project store should close");

    let mut app = App {
        project_path: Some(project_path.clone()),
        ..App::default()
    };
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let audio_track = app.project.tracks()[0].id();
    let midi_track = app.project.tracks()[1].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: audio_track,
            media_ref: audio_ref.to_owned(),
            start_sample: 0,
            source_offset_samples: 120,
            length_samples: 24_000,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: midi_track,
            start_tick: 0,
            length_ticks: 9_600,
        })
        .unwrap();
    let audio_id = app.project.audio_items()[0].id();
    let midi_id = app.project.midi_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline
        .select_items(vec![(audio_id, audio_track), (midi_id, midi_track)]);
    let revision = app.revision;
    let _ = app.update(Message::ItemNudgeTargetChanged(Target::Contents));
    let _ = app.update(Message::ItemNudgeValueChanged("250".to_owned()));

    let _ = app.update(Message::NudgeSelectedItems(1));

    assert_eq!(app.revision, revision + 1);
    assert_eq!(
        app.project.audio_items()[0].source_offset_samples(),
        468_120
    );
    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 9_120);
    assert_eq!(app.project.audio_items()[0].start_sample(), 0);
    assert_eq!(app.project.midi_items()[0].start_tick(), 0);
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].source_offset_samples(), 120);
    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 0);
    let _ = app.update(Message::Redo);
    assert_eq!(
        app.project.audio_items()[0].source_offset_samples(),
        468_120
    );
    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 9_120);

    let before_set_revision = app.revision;
    let _ = app.update(Message::ItemNudgeModeChanged(super::item_nudge::Mode::Set));
    let _ = app.update(Message::ItemNudgeValueChanged("0:03.000".to_owned()));
    let _ = app.update(Message::ApplyItemNudgeSet);
    assert_eq!(app.revision, before_set_revision + 1);
    assert_eq!(
        app.project.audio_items()[0].source_offset_samples(),
        336_000
    );
    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 3_840);
    assert_eq!(app.project.audio_items()[0].start_sample(), 0);
    assert_eq!(app.project.midi_items()[0].start_tick(), 0);
    assert!(app.item_nudge_dialog.error.is_none());
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.project.audio_items()[0].source_offset_samples(),
        468_120
    );
    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 9_120);
    let _ = app.update(Message::Redo);
    assert_eq!(
        app.project.audio_items()[0].source_offset_samples(),
        336_000
    );
    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 3_840);

    drop(app);
    let _ = std::fs::remove_file(project_path);
}

#[test]
fn item_menu_and_action_list_share_the_native_nudge_set_command() {
    let app = App::default();
    let entry = commands::for_menu(&app, MainMenu::Item)
        .into_iter()
        .find(|entry| entry.id == CommandId::OpenItemNudgeDialog)
        .expect("Item menu should expose Nudge/set items");
    assert_eq!(entry.label, "Nudge/set items...");
    assert_eq!(entry.shortcut.as_deref(), Some("N"));
    assert_eq!(
        commands::find(&app, "Item edit: Nudge/Set..."),
        Some(CommandId::OpenItemNudgeDialog)
    );
    assert_eq!(
        commands::for_actions_menu(&app)
            .into_iter()
            .filter(|entry| entry.id == CommandId::OpenItemNudgeDialog)
            .count(),
        1
    );
}

#[test]
fn midi_item_name_draft_commits_as_one_undoable_project_action() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddMidiItem);
    let item_id = app.project.midi_items()[0].id();
    let original_revision = app.revision;

    let _ = app.update(Message::MidiItemNameChanged(
        item_id,
        "  Intro  ".to_owned(),
    ));
    assert_eq!(app.project.midi_items()[0].name(), "MIDI");
    assert_eq!(app.revision, original_revision);
    assert!(app.is_dirty());

    let _ = app.update(Message::CommitMidiItemName(item_id));
    assert_eq!(app.project.midi_items()[0].name(), "Intro");
    assert_eq!(app.revision, original_revision + 1);
    assert!(app.midi_item_name_edits.is_empty());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0].name(), "MIDI");

    let _ = app.update(Message::MidiItemNameChanged(item_id, "Outro".to_owned()));
    assert!(app.commit_pending_track_drafts());
    assert_eq!(app.project.midi_items()[0].name(), "Outro");
}

#[test]
fn project_markers_insert_navigate_and_undo_through_app_actions() {
    let mut app = App::default();
    app.timeline.edit_cursor_tick = 960;
    let _ = app.update(Message::AddProjectMarker);
    assert_eq!(app.project.markers().len(), 1);
    assert_eq!(app.project.markers()[0].position_ticks(), 960);
    let later_id = app.project.markers()[0].id();

    app.timeline.edit_cursor_tick = 240;
    let _ = app.update(Message::AddProjectMarker);
    assert_eq!(app.project.markers().len(), 2);
    assert_eq!(app.project.markers()[0].position_ticks(), 240);
    assert_eq!(app.project.markers()[1].id(), later_id);

    app.timeline.edit_cursor_tick = 0;
    let _ = app.update(Message::NextProjectMarker);
    assert_eq!(app.timeline.edit_cursor_tick, 240);
    let _ = app.update(Message::NextProjectMarker);
    assert_eq!(app.timeline.edit_cursor_tick, 960);
    let _ = app.update(Message::PreviousProjectMarker);
    assert_eq!(app.timeline.edit_cursor_tick, 240);
    let _ = app.update(Message::PreviousProjectMarker);
    assert_eq!(app.timeline.edit_cursor_tick, 0);

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.markers().len(), 1);
    assert_eq!(app.project.markers()[0].id(), later_id);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.markers().len(), 2);
}

#[test]
fn marker_manager_filters_selects_edits_and_deletes_with_undo() {
    let mut app = App::default();
    app.project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 1920,
            name: "Bridge".to_owned(),
        })
        .unwrap();
    let marker_id = app.project.markers()[0].id();

    let _ = app.update(Message::OpenMarkerManager);
    assert!(app.marker_manager_window_id.is_some());
    let manager = app.marker_manager.as_ref().unwrap();
    assert!(manager.show_regions);
    assert!(!manager.show_markers);

    let _ = app.update(Message::MarkerManagerShowMarkersChanged(true));
    let _ = app.update(Message::MarkerManagerFilterChanged("bridge".to_owned()));
    assert_eq!(app.marker_manager.as_ref().unwrap().filter, "bridge");
    let _ = app.update(Message::MarkerManagerSelectMarker(marker_id, false));
    assert_eq!(app.timeline.edit_cursor_tick, 1920);
    assert!(app.selected_render_markers.contains(&marker_id));

    let _ = app.update(Message::MarkerManagerSetMarkerVisibility(marker_id, false));
    assert!(!app.project.markers()[0].is_visible());
    let _ = app.update(Message::Undo);
    assert!(app.project.markers()[0].is_visible());

    let _ = app.update(Message::MarkerManagerEditMarker(marker_id));
    assert_eq!(app.marker_properties.as_ref().unwrap().name, "Bridge");
    let _ = app.update(Message::CloseMarkerProperties);

    let _ = app.update(Message::MarkerManagerDeleteMarker(marker_id));
    assert!(app.project.markers().is_empty());
    assert!(!app.selected_render_markers.contains(&marker_id));
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.markers()[0].id(), marker_id);
    assert_eq!(app.project.markers()[0].name(), "Bridge");
}

#[test]
fn project_marker_ruler_visibility_is_a_checked_view_action_and_undoable() {
    let mut app = App::default();
    let command = CommandId::ToggleProjectMarkersOnRuler;
    assert!(commands::is_enabled(&app, command));
    let entry = commands::for_menu(&app, MainMenu::View)
        .into_iter()
        .find(|entry| entry.id == command)
        .expect("ruler visibility should be available in the View menu");
    assert!(entry.checked);

    let _ = app.update(Message::ExecuteCommand(command));
    assert!(!app.project.show_project_markers());
    assert!(
        commands::for_menu(&app, MainMenu::View)
            .into_iter()
            .find(|entry| entry.id == command)
            .is_some_and(|entry| !entry.checked)
    );
    let _ = app.update(Message::Undo);
    assert!(app.project.show_project_markers());
}

#[test]
fn selected_marker_visibility_action_matches_native_mixed_selection_and_undo() {
    let mut app = App::default();
    let command = CommandId::ToggleSelectedProjectMarkers;
    assert!(!commands::is_enabled(&app, command));
    assert_eq!(
        commands::stable_id(command).as_deref(),
        Some("ruler.toggle-selected-project-markers")
    );
    assert!(
        commands::for_actions_menu(&app)
            .iter()
            .any(|entry| entry.id == command && !entry.enabled)
    );
    assert_eq!(
        commands::find(&app, "Ruler: Show/hide selected project markers"),
        None
    );

    for (position_ticks, name) in [(960, "Hidden"), (1_920, "Visible"), (2_880, "Other")] {
        app.project
            .apply(DawAction::InsertProjectMarker {
                position_ticks,
                name: name.to_owned(),
            })
            .unwrap();
    }
    let markers = app.project.markers();
    let hidden = markers[0].id();
    let selected_visible = markers[1].id();
    let unselected = markers[2].id();
    app.project
        .apply(DawAction::SetProjectMarkerVisibility {
            marker_id: hidden,
            visible: false,
        })
        .unwrap();
    app.selected_render_markers = HashSet::from([hidden, selected_visible]);
    assert!(commands::is_enabled(&app, command));
    assert_eq!(
        commands::find(&app, "Ruler: Show/hide selected project markers"),
        Some(command)
    );
    assert_eq!(app.project.markers()[2].id(), unselected);

    let _ = app.update(Message::ExecuteCommand(command));
    assert!(app.project.markers()[0].is_visible());
    assert!(app.project.markers()[1].is_visible());
    assert!(app.project.markers()[2].is_visible());

    let _ = app.update(Message::Undo);
    assert!(!app.project.markers()[0].is_visible());
    assert!(app.project.markers()[1].is_visible());
    assert!(app.project.markers()[2].is_visible());
    let _ = app.update(Message::Redo);
    assert!(app.project.markers()[0].is_visible());
    assert!(app.project.markers()[1].is_visible());
    assert!(app.project.markers()[2].is_visible());

    let _ = app.update(Message::ExecuteCommand(command));
    assert!(!app.project.markers()[0].is_visible());
    assert!(!app.project.markers()[1].is_visible());
    assert!(app.project.markers()[2].is_visible());
    let _ = app.update(Message::Undo);
    assert!(app.project.markers()[0].is_visible());
    assert!(app.project.markers()[1].is_visible());

    app.selected_render_markers.clear();
    app.io_busy = true;
    assert!(!commands::is_enabled(&app, command));
}

#[test]
fn selected_region_visibility_actions_batch_selection_with_one_undo_and_redo() {
    let mut app = App::default();
    let selected_regions_command = CommandId::ToggleSelectedProjectRegions;
    let selected_both_command = CommandId::ToggleSelectedProjectRegionsAndMarkers;
    assert_eq!(
        commands::stable_id(selected_regions_command).as_deref(),
        Some("ruler.toggle-selected-project-regions")
    );
    assert!(
        commands::for_actions_menu(&app)
            .iter()
            .any(|entry| entry.id == selected_both_command && !entry.enabled)
    );
    assert_eq!(
        commands::find(
            &app,
            "Ruler: Show/hide selected project regions and markers"
        ),
        None
    );

    for (start_ticks, end_ticks, name) in [
        (0, 960, "Hidden"),
        (960, 1_920, "Visible"),
        (1_920, 2_880, "Unselected"),
    ] {
        app.project
            .apply(DawAction::InsertProjectRegion {
                start_ticks,
                end_ticks,
                name: name.to_owned(),
            })
            .unwrap();
    }
    let regions = app.project.regions();
    let hidden = regions[0].id();
    let selected_visible = regions[1].id();
    assert_eq!(regions[2].name(), "Unselected");
    app.project
        .apply(DawAction::SetProjectRegionVisibility {
            region_id: hidden,
            visible: false,
        })
        .unwrap();
    app.selected_render_regions = HashSet::from([hidden, selected_visible]);

    assert!(commands::is_enabled(&app, selected_regions_command));
    let _ = app.update(Message::ExecuteCommand(selected_regions_command));
    assert!(app.project.regions()[0].is_visible());
    assert!(app.project.regions()[1].is_visible());
    assert!(app.project.regions()[2].is_visible());
    let _ = app.update(Message::Undo);
    assert!(!app.project.regions()[0].is_visible());
    assert!(app.project.regions()[1].is_visible());
    assert!(app.project.regions()[2].is_visible());

    app.project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 480,
            name: "Selected marker".to_owned(),
        })
        .unwrap();
    let marker_id = app.project.markers()[0].id();
    app.selected_render_markers = HashSet::from([marker_id]);
    assert!(commands::is_enabled(&app, selected_both_command));
    assert_eq!(
        commands::find(
            &app,
            "Ruler: Show/hide selected project regions and markers"
        ),
        Some(selected_both_command)
    );
    let _ = app.update(Message::ExecuteCommand(selected_both_command));
    assert!(app.project.regions()[0].is_visible());
    assert!(app.project.markers()[0].is_visible());
    let _ = app.update(Message::Undo);
    assert!(!app.project.regions()[0].is_visible());
    assert!(app.project.markers()[0].is_visible());
    let _ = app.update(Message::Redo);
    assert!(app.project.regions()[0].is_visible());
    assert!(app.project.markers()[0].is_visible());
}

#[test]
fn region_lane_view_actions_match_mixed_lane_undo_and_redo() {
    let mut app = App::default();
    let regions_command = CommandId::ToggleProjectRegionsOnRuler;
    let combined_command = CommandId::ToggleProjectRegionsAndMarkersOnRuler;
    let view_entries = commands::for_menu(&app, MainMenu::View);
    assert!(
        view_entries
            .iter()
            .any(|entry| entry.id == regions_command && entry.checked)
    );
    assert!(commands::toggle_state(&app, combined_command).unwrap());

    let _ = app.update(Message::ExecuteCommand(regions_command));
    assert!(!app.project.show_project_regions());
    let _ = app.update(Message::Undo);
    assert!(app.project.show_project_regions());

    let _ = app.update(Message::ExecuteCommand(combined_command));
    assert!(!app.project.show_project_regions());
    assert!(!app.project.show_project_markers());
    let _ = app.update(Message::Undo);
    assert!(app.project.show_project_regions());
    assert!(app.project.show_project_markers());

    let _ = app.update(Message::ExecuteCommand(regions_command));
    assert!(!app.project.show_project_regions());
    assert!(app.project.show_project_markers());
    let _ = app.update(Message::ExecuteCommand(combined_command));
    assert!(app.project.show_project_regions());
    assert!(app.project.show_project_markers());
    let _ = app.update(Message::Undo);
    assert!(!app.project.show_project_regions());
    assert!(app.project.show_project_markers());
    let _ = app.update(Message::Redo);
    assert!(app.project.show_project_regions());
    assert!(app.project.show_project_markers());
}

#[test]
fn marker_manager_delete_respects_project_edit_busy_guard() {
    let mut app = App::default();
    app.project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 960,
            name: "Verse".to_owned(),
        })
        .unwrap();
    let marker_id = app.project.markers()[0].id();
    let revision = app.revision;
    app.io_busy = true;

    app.delete_marker_from_manager(marker_id);

    assert_eq!(app.project.markers()[0].id(), marker_id);
    assert_eq!(app.revision, revision);
    assert!(app.status.contains("finish the current project operation"));
}

#[test]
fn insert_region_command_uses_a_non_empty_time_selection() {
    let mut app = App::default();
    let command = CommandId::InsertRegionFromTimeSelection;
    assert!(!commands::is_enabled(&app, command));

    let _ = app.update(Message::InsertRegionFromTimeSelection);
    assert!(app.project.regions().is_empty());
    assert!(app.status.contains("time selection"));

    let _ = app.update(Message::Timeline(TimelineEvent::SetTimeSelection {
        start_tick: 3_840,
        end_tick: 960,
    }));
    assert!(commands::is_enabled(&app, command));
    let _ = app.update(Message::ExecuteCommand(command));

    assert_eq!(app.project.regions().len(), 1);
    assert_eq!(app.project.regions()[0].start_ticks(), 960);
    assert_eq!(app.project.regions()[0].end_ticks(), 3_840);
    assert_eq!(app.project.regions()[0].number(), 1);
    assert_eq!(app.revision, 2);
    assert!(app.timeline.time_selection.is_some());
    let _ = app.update(Message::Undo);
    assert!(app.project.regions().is_empty());
}

#[test]
fn region_manager_selects_edits_filters_and_deletes_with_undo() {
    let mut app = App::default();
    app.project
        .apply(DawAction::InsertProjectRegion {
            start_ticks: 0,
            end_ticks: 1_920,
            name: "Verse".to_owned(),
        })
        .unwrap();
    let region_id = app.project.regions()[0].id();
    app.project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 960,
            name: "Cue".to_owned(),
        })
        .unwrap();
    let marker_id = app.project.markers()[0].id();

    let _ = app.update(Message::OpenMarkerManager);
    assert!(app.marker_manager.as_ref().unwrap().show_regions);
    assert!(!app.marker_manager.as_ref().unwrap().show_markers);
    let _ = app.update(Message::MarkerManagerShowMarkersChanged(true));
    let _ = app.update(Message::MarkerManagerSelectMarker(marker_id, false));
    assert!(app.selected_render_markers.contains(&marker_id));
    let _ = app.update(Message::MarkerManagerFilterChanged("r1".to_owned()));
    let _ = app.update(Message::MarkerManagerSelectRegion(region_id, false));
    assert_eq!(
        app.timeline.time_selection,
        Some(crate::timeline::TimeSelection {
            start_tick: 0,
            end_tick: 1_920,
        })
    );
    assert_eq!(app.timeline.edit_cursor_tick, 0);
    assert!(app.selected_render_regions.contains(&region_id));
    assert!(app.selected_render_markers.is_empty());

    let _ = app.update(Message::MarkerManagerEditRegion(region_id));
    assert_eq!(app.region_properties.as_ref().unwrap().number, "1");
    assert_eq!(app.region_properties.as_ref().unwrap().position, "1.1.00");
    assert_eq!(app.region_properties.as_ref().unwrap().length, "0.2.00");
    let _ = app.update(Message::RegionNameChanged("Verse A".to_owned()));
    let _ = app.update(Message::RegionNumberChanged("4".to_owned()));
    let _ = app.update(Message::RegionPositionChanged("1.2.00".to_owned()));
    let _ = app.update(Message::RegionLengthChanged("0.3.00".to_owned()));
    let _ = app.update(Message::ApplyRegionProperties);
    let updated = app
        .project
        .regions()
        .iter()
        .find(|region| region.id() == region_id)
        .unwrap();
    assert_eq!(updated.number(), 4);
    assert_eq!(updated.name(), "Verse A");
    assert_eq!(updated.start_ticks(), 960);
    assert_eq!(updated.end_ticks(), 3_840);
    assert!(app.region_properties_window_id.is_none());

    let _ = app.update(Message::Undo);
    let restored = app
        .project
        .regions()
        .iter()
        .find(|region| region.id() == region_id)
        .unwrap();
    assert_eq!(restored.number(), 1);
    assert_eq!(restored.name(), "Verse");
    assert_eq!(restored.start_ticks(), 0);
    assert_eq!(restored.end_ticks(), 1_920);

    let _ = app.update(Message::MarkerManagerDeleteRegion(region_id));
    assert!(app.project.regions().is_empty());
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.regions()[0].id(), region_id);
}

#[test]
fn region_cursor_target_matches_native_half_open_and_overlap_rules() {
    let mut app = App::default();
    for (start_ticks, end_ticks, name) in [
        (100, 300, "Earlier wide"),
        (140, 350, "Later but longer"),
        (400, 500, "Same start long"),
        (400, 450, "Same start short"),
        (600, 700, "Identical first"),
        (600, 700, "Identical second"),
    ] {
        app.project
            .apply(DawAction::InsertProjectRegion {
                start_ticks,
                end_ticks,
                name: name.to_owned(),
            })
            .unwrap();
    }
    let regions = app.project.regions();

    // The latest start wins even when that Region is longer overall.
    app.timeline.edit_cursor_tick = 140;
    assert_eq!(
        commands::project_region_at_cursor(&app),
        Some(regions[1].id())
    );
    // Region bounds are inclusive at Start and exclusive at End.
    app.timeline.edit_cursor_tick = 350;
    assert_eq!(commands::project_region_at_cursor(&app), None);
    app.timeline.edit_cursor_tick = 100;
    assert_eq!(
        commands::project_region_at_cursor(&app),
        Some(regions[0].id())
    );
    // Equal starts prefer the shorter interval; identical intervals retain
    // the earlier stable identity.
    app.timeline.edit_cursor_tick = 420;
    assert_eq!(
        commands::project_region_at_cursor(&app),
        Some(regions[3].id())
    );
    app.timeline.edit_cursor_tick = 650;
    assert_eq!(
        commands::project_region_at_cursor(&app),
        Some(regions[4].id())
    );
    app.timeline.edit_cursor_tick = 800;
    assert_eq!(commands::project_region_at_cursor(&app), None);
}

#[test]
fn region_cursor_actions_edit_delete_and_undo_with_project_busy_guard() {
    let mut app = App::default();
    app.project
        .apply(DawAction::InsertProjectRegion {
            start_ticks: 100,
            end_ticks: 300,
            name: "Verse".to_owned(),
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 150,
            name: "Cue".to_owned(),
        })
        .unwrap();
    let region_id = app.project.regions()[0].id();
    app.timeline.edit_cursor_tick = 150;

    let delete = CommandId::DeleteProjectRegionNearCursor;
    let edit = CommandId::EditProjectRegionNearCursor;
    assert_eq!(
        commands::find(&app, "delete region near cursor"),
        Some(delete)
    );
    assert_eq!(commands::find(&app, "edit nearby region"), Some(edit));
    assert_eq!(
        commands::stable_id(delete).as_deref(),
        Some("regions.delete-near-cursor")
    );
    assert_eq!(
        commands::stable_id(edit).as_deref(),
        Some("regions.edit-near-cursor")
    );
    assert!(commands::is_enabled(&app, delete));
    assert!(commands::is_enabled(&app, edit));

    let _ = app.update(Message::EditProjectRegionNearCursor);
    assert_eq!(app.region_properties.as_ref().unwrap().region_id, region_id);
    assert_eq!(app.region_properties.as_ref().unwrap().name, "Verse");
    let _ = app.update(Message::CloseRegionProperties);

    let _ = app.update(Message::DeleteProjectRegionNearCursor);
    assert!(app.project.regions().is_empty());
    assert_eq!(app.project.markers().len(), 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.regions()[0].id(), region_id);
    assert_eq!(app.project.markers().len(), 1);

    app.io_busy = true;
    assert!(!commands::is_enabled(&app, delete));
    assert!(!commands::is_enabled(&app, edit));
    let _ = app.update(Message::DeleteProjectRegionNearCursor);
    assert_eq!(app.project.regions()[0].id(), region_id);
}

#[test]
fn loop_points_and_time_selection_follow_reaper_link_copy_and_clear_actions() {
    let mut app = App::default();
    let range = |start_tick, end_tick| crate::timeline::TimeSelection {
        start_tick,
        end_tick,
    };
    let loop_range = |start_tick, end_tick| crate::timeline::LoopPoints {
        start_tick,
        end_tick,
    };
    assert!(app.timeline.loop_points_linked);
    assert_eq!(app.timeline.time_selection, None);
    assert_eq!(app.timeline.loop_points, None);

    let commands_to_check = [
        ("set loop start point", "loop-time.set-start"),
        ("set loop end point", "loop-time.set-end"),
        (
            "toggle loop points linked to time selection",
            "loop-time.toggle-link",
        ),
        (
            "move loop points to time selection",
            "loop-time.loop-to-time-selection",
        ),
        (
            "move time selection to loop points",
            "loop-time.time-selection-to-loop",
        ),
        (
            "remove loop points if unlinked",
            "loop-time.remove-loop-if-unlinked",
        ),
        ("remove loop points", "loop-time.remove-loop"),
        ("remove time selection", "loop-time.remove-time-selection"),
        (
            "remove time selection and loop points",
            "loop-time.remove-time-selection-and-loop",
        ),
    ];
    for (query, stable_id) in commands_to_check {
        let command = commands::find(&app, query)
            .unwrap_or_else(|| panic!("loop/time action should be searchable: {query}"));
        assert_eq!(commands::stable_id(command).as_deref(), Some(stable_id));
    }
    for (command, shortcut) in [
        (
            CommandId::LoopTime(super::commands::LoopTimeCommand::SetStartAtCursor),
            "Shift+{",
        ),
        (
            CommandId::LoopTime(super::commands::LoopTimeCommand::SetEndAtCursor),
            "Shift+}",
        ),
    ] {
        let entry = commands::for_actions_menu(&app)
            .into_iter()
            .find(|entry| entry.id == command)
            .expect("loop endpoint action should be in the Actions list");
        assert_eq!(entry.shortcut.as_deref(), Some(shortcut));
    }
    let clear_both = commands::for_actions_menu(&app)
        .into_iter()
        .find(|entry| {
            entry.id
                == CommandId::LoopTime(super::commands::LoopTimeCommand::RemoveTimeSelectionAndLoop)
        })
        .expect("Escape action should remain visible in the Actions list");
    assert_eq!(clear_both.shortcut.as_deref(), Some("Escape"));
    assert!(matches!(
        shortcut_message(Key::Character("{"), Modifiers::SHIFT, &HashMap::new(),),
        Some(Message::ExecuteCommand(CommandId::LoopTime(
            super::commands::LoopTimeCommand::SetStartAtCursor
        )))
    ));
    let run_action = |app: &mut App, query: &str| {
        let command = commands::find(app, query).expect("loop/time action should be registered");
        let _ = commands::dispatch(app, command);
    };

    let previous_revision = app.revision;
    let _ = app.update(Message::Timeline(TimelineEvent::SetTimeSelection {
        start_tick: 100,
        end_tick: 200,
    }));
    assert_ne!(app.revision, previous_revision);
    assert!(app.is_dirty());
    assert_eq!(app.timeline.time_selection, Some(range(100, 200)));
    assert_eq!(app.timeline.loop_points, Some(loop_range(100, 200)));

    run_action(&mut app, "toggle loop points linked to time selection");
    assert!(!app.timeline.loop_points_linked);
    assert_eq!(app.timeline.loop_points, Some(loop_range(100, 200)));

    let _ = app.update(Message::Timeline(TimelineEvent::SetTimeSelection {
        start_tick: 300,
        end_tick: 400,
    }));
    assert_eq!(app.timeline.time_selection, Some(range(300, 400)));
    assert_eq!(app.timeline.loop_points, Some(loop_range(100, 200)));
    run_action(&mut app, "move loop points to time selection");
    assert_eq!(app.timeline.loop_points, Some(loop_range(300, 400)));

    let _ = app.update(Message::Timeline(TimelineEvent::SetLoopPoints {
        start_tick: 500,
        end_tick: 600,
    }));
    run_action(&mut app, "move time selection to loop points");
    assert_eq!(app.timeline.time_selection, Some(range(500, 600)));

    run_action(&mut app, "remove loop points if unlinked");
    assert_eq!(app.timeline.loop_points, None);
    assert_eq!(app.timeline.time_selection, Some(range(500, 600)));

    run_action(&mut app, "toggle loop points linked to time selection");
    assert!(app.timeline.loop_points_linked);
    assert_eq!(app.timeline.loop_points, Some(loop_range(500, 600)));
    run_action(&mut app, "remove loop points if unlinked");
    assert_eq!(app.timeline.loop_points, Some(loop_range(500, 600)));
    run_action(&mut app, "remove loop points");
    assert_eq!(app.timeline.loop_points, None);
    assert_eq!(app.timeline.time_selection, None);

    let _ = app.update(Message::Timeline(TimelineEvent::SetLoopPoints {
        start_tick: 700,
        end_tick: 800,
    }));
    assert_eq!(app.timeline.time_selection, Some(range(700, 800)));
    run_action(&mut app, "remove time selection");
    assert_eq!(app.timeline.loop_points, None);
    assert_eq!(app.timeline.time_selection, None);

    run_action(&mut app, "toggle loop points linked to time selection");
    let _ = app.update(Message::Timeline(TimelineEvent::SetTimeSelection {
        start_tick: 900,
        end_tick: 1_000,
    }));
    let _ = app.update(Message::Timeline(TimelineEvent::ToggleLoopPointsLinked));
    let _ = app.update(Message::Timeline(TimelineEvent::SetLoopPoints {
        start_tick: 1_100,
        end_tick: 1_200,
    }));
    run_action(&mut app, "remove time selection and loop points");
    assert_eq!(app.timeline.loop_points, None);
    assert_eq!(app.timeline.time_selection, None);

    run_action(&mut app, "toggle loop points linked to time selection");
    let _ = app.update(Message::Timeline(TimelineEvent::SetTimeSelection {
        start_tick: 1_300,
        end_tick: 1_400,
    }));
    let _ = app.update(Message::Timeline(TimelineEvent::SetLoopPoints {
        start_tick: 1_500,
        end_tick: 1_600,
    }));
    let _ = app.update(Message::Escape);
    assert_eq!(app.timeline.time_selection, None);
    assert_eq!(app.timeline.loop_points, None);

    app.timeline.edit_cursor_tick = 500;
    run_action(&mut app, "set loop start point");
    assert_eq!(app.timeline.loop_points, Some(loop_range(500, 500)));
    assert_eq!(app.timeline.time_selection, None);
    app.timeline.edit_cursor_tick = 700;
    run_action(&mut app, "set loop end point");
    assert_eq!(app.timeline.loop_points, Some(loop_range(500, 700)));
    let _ = app.update(Message::Timeline(TimelineEvent::ClearLoopPoints));
    run_action(&mut app, "set loop end point");
    assert_eq!(app.timeline.loop_points, Some(loop_range(0, 700)));

    run_action(&mut app, "toggle loop points linked to time selection");
    let _ = app.update(Message::Timeline(TimelineEvent::SetTimeSelection {
        start_tick: 100,
        end_tick: 200,
    }));
    app.timeline.edit_cursor_tick = 150;
    run_action(&mut app, "set loop start point");
    assert_eq!(app.timeline.loop_points, Some(loop_range(150, 200)));
    assert_eq!(app.timeline.time_selection, Some(range(150, 200)));
    app.timeline.edit_cursor_tick = 175;
    run_action(&mut app, "set loop end point");
    assert_eq!(app.timeline.loop_points, Some(loop_range(150, 175)));
    assert_eq!(app.timeline.time_selection, Some(range(150, 175)));
}

#[test]
fn toggling_loop_time_link_schedules_global_save_without_dirtying_project() {
    let mut app = App::default();
    let command = CommandId::LoopTime(commands::LoopTimeCommand::ToggleLinked);
    let revision = app.revision;

    assert_eq!(commands::toggle_state(&app, command), Some(true));
    let _ = app.update(Message::ExecuteCommand(command));

    assert!(!app.timeline.loop_points_linked);
    assert_eq!(commands::toggle_state(&app, command), Some(false));
    assert!(app.loop_time_preferences_save_at.is_some());
    assert_eq!(app.revision, revision);
    assert_eq!(app.timeline.time_selection, None);
    assert_eq!(app.timeline.loop_points, None);
}

#[test]
fn loop_cursor_actions_jump_to_existing_boundaries_and_noop_without_a_loop() {
    let mut app = App::default();
    let go_to_start = CommandId::LoopTime(commands::LoopTimeCommand::GoToStart);
    let go_to_end = CommandId::LoopTime(commands::LoopTimeCommand::GoToEnd);
    app.timeline.edit_cursor_tick = 500;
    let _ = app.update(Message::ExecuteCommand(go_to_start));
    assert_eq!(app.timeline.edit_cursor_tick, 500);

    let _ = app.update(Message::Timeline(TimelineEvent::SetLoopPoints {
        start_tick: 120,
        end_tick: 360,
    }));
    let range = app.timeline.loop_points;
    let revision = app.revision;

    let _ = app.update(Message::ExecuteCommand(go_to_start));
    assert_eq!(app.timeline.edit_cursor_tick, 120);
    assert_eq!(app.timeline.loop_points, range);
    assert_eq!(app.revision, revision);

    let _ = app.update(Message::ExecuteCommand(go_to_end));
    assert_eq!(app.timeline.edit_cursor_tick, 360);
    assert_eq!(app.timeline.loop_points, range);
    assert_eq!(app.revision, revision);
}

#[test]
fn region_properties_reject_invalid_bounds_without_mutating_and_cancel_drafts() {
    let mut app = App::default();
    app.project
        .apply(DawAction::InsertProjectRegion {
            start_ticks: 0,
            end_ticks: 1_920,
            name: "Intro".to_owned(),
        })
        .unwrap();
    let region_id = app.project.regions()[0].id();
    let _ = app.update(Message::MarkerManagerEditRegion(region_id));
    let _ = app.update(Message::RegionNumberChanged("0".to_owned()));
    let _ = app.update(Message::ApplyRegionProperties);
    assert_eq!(app.project.regions()[0].number(), 1);
    assert!(app.region_properties_window_id.is_some());
    assert!(app.region_properties.as_ref().unwrap().error.is_some());

    let _ = app.update(Message::OpenMarkerManager);
    assert!(app.marker_manager_window_id.is_some());
    assert!(app.region_properties_window_id.is_some());
    let _ = app.update(Message::RegionNumberChanged("1".to_owned()));
    let _ = app.update(Message::RegionLengthChanged("0.0.00".to_owned()));
    let _ = app.update(Message::ApplyRegionProperties);
    assert_eq!(app.project.regions()[0].end_ticks(), 1_920);
    assert!(app.region_properties.as_ref().unwrap().error.is_some());

    let revision = app.revision;
    let _ = app.update(Message::RegionNameChanged("Discarded".to_owned()));
    let _ = app.update(Message::CloseRegionProperties);
    assert_eq!(app.project.regions()[0].name(), "Intro");
    assert_eq!(app.revision, revision);
    assert!(app.region_properties_window_id.is_none());
    assert!(app.marker_manager_window_id.is_some());
}

#[test]
fn ruler_region_edit_preserves_identity_and_is_one_undoable_action() {
    let mut app = App::default();
    app.project
        .apply(DawAction::InsertProjectRegion {
            start_ticks: 960,
            end_ticks: 3_840,
            name: "Verse".to_owned(),
        })
        .unwrap();
    let region = &app.project.regions()[0];
    let region_id = region.id();
    let number = region.number();

    let _ = app.update(Message::Timeline(TimelineEvent::MoveProjectRegion {
        region_id,
        original_start_ticks: 960,
        original_end_ticks: 3_840,
        start_ticks: 1_360,
        end_ticks: 4_240,
    }));

    let updated = &app.project.regions()[0];
    assert_eq!(updated.id(), region_id);
    assert_eq!(updated.number(), number);
    assert_eq!(updated.name(), "Verse");
    assert_eq!(updated.start_ticks(), 1_360);
    assert_eq!(updated.end_ticks(), 4_240);
    assert_eq!(app.timeline.edit_cursor_tick, 1_360);
    assert_eq!(
        app.timeline.time_selection,
        Some(crate::timeline::TimeSelection {
            start_tick: 1_360,
            end_tick: 4_240,
        })
    );

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.regions()[0].id(), region_id);
    assert_eq!(app.project.regions()[0].start_ticks(), 960);
    assert_eq!(app.project.regions()[0].end_ticks(), 3_840);
}

#[test]
fn ruler_region_edit_obeys_project_busy_guard() {
    let mut app = App::default();
    app.project
        .apply(DawAction::InsertProjectRegion {
            start_ticks: 960,
            end_ticks: 3_840,
            name: "Verse".to_owned(),
        })
        .unwrap();
    let region_id = app.project.regions()[0].id();
    let revision = app.revision;
    app.io_busy = true;

    let _ = app.update(Message::Timeline(TimelineEvent::MoveProjectRegion {
        region_id,
        original_start_ticks: 960,
        original_end_ticks: 3_840,
        start_ticks: 1_360,
        end_ticks: 4_240,
    }));

    assert_eq!(app.project.regions()[0].start_ticks(), 960);
    assert_eq!(app.project.regions()[0].end_ticks(), 3_840);
    assert_eq!(app.revision, revision);
    assert!(app.status.contains("before moving a region"));
}

#[test]
fn ruler_region_edit_rejects_a_stale_drag_after_another_edit() {
    let mut app = App::default();
    app.project
        .apply(DawAction::InsertProjectRegion {
            start_ticks: 960,
            end_ticks: 3_840,
            name: "Verse".to_owned(),
        })
        .unwrap();
    let region_id = app.project.regions()[0].id();
    app.project
        .apply(DawAction::UpdateProjectRegion {
            region_id,
            number: 1,
            start_ticks: 1_920,
            end_ticks: 4_800,
            name: "Verse".to_owned(),
        })
        .unwrap();
    let revision = app.revision;

    let _ = app.update(Message::Timeline(TimelineEvent::MoveProjectRegion {
        region_id,
        original_start_ticks: 960,
        original_end_ticks: 3_840,
        start_ticks: 1_360,
        end_ticks: 4_240,
    }));

    assert_eq!(app.project.regions()[0].start_ticks(), 1_920);
    assert_eq!(app.project.regions()[0].end_ticks(), 4_800);
    assert_eq!(app.revision, revision);
    assert!(app.status.contains("changed before the drag completed"));
}

#[test]
fn marker_manager_modifier_selects_multiple_render_bounds_and_keeps_them_after_close() {
    let mut app = App::default();
    for (position_ticks, name) in [(0, "One"), (960, "Two"), (1_920, "Three")] {
        app.project
            .apply(DawAction::InsertProjectMarker {
                position_ticks,
                name: name.to_owned(),
            })
            .expect("marker should be inserted");
    }
    app.project
        .apply(DawAction::InsertProjectRegion {
            start_ticks: 0,
            end_ticks: 1_920,
            name: "Whole".to_owned(),
        })
        .expect("region should be inserted");
    let first_marker = app.project.markers()[0].id();
    let second_marker = app.project.markers()[1].id();
    let third_marker = app.project.markers()[2].id();
    let region = app.project.regions()[0].id();

    let _ = app.update(Message::OpenMarkerManager);
    let _ = app.update(Message::MarkerManagerSelectMarker(first_marker, false));
    let manager_window = app.marker_manager_window_id.unwrap();
    let _ = app.update(Message::RuntimeKeyboardEvent(
        iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
            iced::keyboard::Modifiers::CTRL,
        )),
        iced::event::Status::Ignored,
        manager_window,
    ));
    let additive = app.keyboard_modifiers.command();
    let _ = app.update(Message::MarkerManagerSelectMarker(second_marker, additive));
    assert!(app.keyboard_modifiers.command());
    let _ = app.update(Message::RuntimeKeyboardEvent(
        iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
            iced::keyboard::Modifiers::NONE,
        )),
        iced::event::Status::Ignored,
        manager_window,
    ));
    assert_eq!(app.selected_render_markers.len(), 2);
    assert!(app.selected_render_markers.contains(&first_marker));
    assert!(app.selected_render_markers.contains(&second_marker));

    let _ = app.update(Message::MarkerManagerSelectRegion(region, true));
    assert!(app.selected_render_regions.contains(&region));
    assert_eq!(app.selected_render_markers.len(), 2);

    let _ = app.update(Message::CloseMarkerManager);
    assert!(app.marker_manager.is_none());
    assert!(app.selected_render_markers.contains(&first_marker));
    assert!(app.selected_render_markers.contains(&second_marker));
    assert!(app.selected_render_regions.contains(&region));

    let _ = app.update(Message::OpenMarkerManager);
    let _ = app.update(Message::MarkerManagerSelectMarker(third_marker, false));
    assert_eq!(app.selected_render_markers, HashSet::from([third_marker]));
    assert!(app.selected_render_regions.is_empty());
}

#[test]
fn marker_manager_keeps_global_undo_shortcut_available() {
    let window_id = iced::window::Id::unique();
    let mut app = App {
        marker_manager: Some(super::marker_manager::MarkerManager::default()),
        marker_manager_window_id: Some(window_id),
        ..App::default()
    };
    app.project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 960,
            name: String::new(),
        })
        .unwrap();
    let (marker_id, position_ticks) = {
        let marker = &app.project.markers()[0];
        (marker.id(), marker.position_ticks())
    };
    app.project
        .apply(DawAction::UpdateProjectMarker {
            marker_id,
            position_ticks,
            name: "Verse".to_owned(),
        })
        .unwrap();

    let _ = app.update(Message::RuntimeKeyboardEvent(
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: Key::Character("z".into()),
            modified_key: Key::Character("z".into()),
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::KeyZ),
            location: iced::keyboard::Location::Standard,
            modifiers: Modifiers::COMMAND,
            text: None,
            repeat: false,
        }),
        iced::event::Status::Ignored,
        window_id,
    ));

    assert_eq!(app.project.markers()[0].name(), "");
    assert_eq!(app.marker_manager_window_id, Some(window_id));
}

#[test]
fn marker_properties_edit_name_number_and_position_as_one_undoable_change() {
    let mut app = App::default();
    app.project
        .apply(DawAction::SetTempo {
            start_tick: 1_920,
            bpm: 60.0,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 960,
            name: String::new(),
        })
        .unwrap();
    let marker_id = app.project.markers()[0].id();
    assert_eq!(app.project.markers()[0].number(), 1);
    let revision_before = app.revision;

    let _ = app.update(Message::Timeline(TimelineEvent::EditProjectMarker {
        marker_id,
    }));
    assert_eq!(app.marker_properties.as_ref().unwrap().name, "");
    assert_eq!(app.marker_properties.as_ref().unwrap().position, "1.2.00");
    assert!(app.marker_properties_window_id.is_some());
    let _ = app.update(Message::MarkerNameChanged("discarded".to_owned()));
    let _ = app.update(Message::MarkerNumberChanged("8".to_owned()));
    let _ = app.update(Message::MarkerPositionChanged("1.3.50".to_owned()));
    let _ = app.update(Message::CloseMarkerProperties);
    assert_eq!(app.project.markers()[0].name(), "");
    assert_eq!(app.project.markers()[0].number(), 1);
    assert_eq!(app.project.markers()[0].position_ticks(), 960);
    assert_eq!(app.revision, revision_before);

    let _ = app.update(Message::Timeline(TimelineEvent::EditProjectMarker {
        marker_id,
    }));
    let _ = app.update(Message::MarkerNameChanged("Verse".to_owned()));
    let _ = app.update(Message::MarkerNumberChanged("8".to_owned()));
    let _ = app.update(Message::MarkerPositionChanged("1.3.50".to_owned()));
    let _ = app.update(Message::ApplyMarkerProperties);
    assert_eq!(app.project.markers()[0].name(), "Verse");
    assert_eq!(app.project.markers()[0].number(), 8);
    assert_eq!(app.project.markers()[0].id(), marker_id);
    assert_eq!(app.project.markers()[0].position_ticks(), 2_400);
    assert_eq!(app.timeline.edit_cursor_tick, 0);
    assert_eq!(app.revision, revision_before + 1);
    assert!(app.marker_properties_window_id.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.markers()[0].name(), "");
    assert_eq!(app.project.markers()[0].number(), 1);
    assert_eq!(app.project.markers()[0].id(), marker_id);
    assert_eq!(app.project.markers()[0].position_ticks(), 960);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.markers()[0].name(), "Verse");
    assert_eq!(app.project.markers()[0].number(), 8);
    assert_eq!(app.project.markers()[0].id(), marker_id);
    assert_eq!(app.project.markers()[0].position_ticks(), 2_400);
}

#[test]
fn marker_properties_keep_invalid_or_busy_edits_open_without_changing_project() {
    let mut app = App::default();
    app.project
        .apply(DawAction::InsertProjectMarker {
            position_ticks: 960,
            name: "Verse".to_owned(),
        })
        .unwrap();
    let marker_id = app.project.markers()[0].id();
    let _ = app.update(Message::Timeline(TimelineEvent::EditProjectMarker {
        marker_id,
    }));
    let _ = app.update(Message::MarkerNumberChanged("0".to_owned()));
    let _ = app.update(Message::ApplyMarkerProperties);
    assert_eq!(app.project.markers()[0].number(), 1);
    assert!(app.marker_properties_window_id.is_some());
    assert!(app.marker_properties.as_ref().unwrap().error.is_some());

    let _ = app.update(Message::MarkerNumberChanged("1".to_owned()));
    let _ = app.update(Message::MarkerNameChanged("\0".to_owned()));
    let _ = app.update(Message::ApplyMarkerProperties);
    assert_eq!(app.project.markers()[0].name(), "Verse");
    assert!(app.marker_properties_window_id.is_some());
    assert!(app.marker_properties.as_ref().unwrap().error.is_some());

    let _ = app.update(Message::MarkerNameChanged("Chorus".to_owned()));
    let _ = app.update(Message::MarkerPositionChanged("not a position".to_owned()));
    let _ = app.update(Message::ApplyMarkerProperties);
    assert_eq!(app.project.markers()[0].name(), "Verse");
    assert_eq!(app.project.markers()[0].position_ticks(), 960);
    assert!(app.marker_properties_window_id.is_some());
    assert!(app.marker_properties.as_ref().unwrap().error.is_some());

    let _ = app.update(Message::MarkerPositionChanged("1.3.00".to_owned()));
    app.io_busy = true;
    let _ = app.update(Message::ApplyMarkerProperties);
    assert_eq!(app.project.markers()[0].name(), "Verse");
    assert_eq!(app.project.markers()[0].position_ticks(), 960);
    assert!(app.marker_properties_window_id.is_some());
    assert!(app.marker_properties.as_ref().unwrap().error.is_some());
}

#[test]
fn deleting_nearby_marker_matches_reaper_radius_and_nearest_selection() {
    let mut app = App::default();
    app.timeline.edit_cursor_tick = 0;
    let _ = app.update(Message::AddProjectMarker);
    let first_id = app.project.markers()[0].id();

    app.timeline.edit_cursor_tick = 1_920;
    let _ = app.update(Message::AddProjectMarker);
    let second_id = app.project.markers()[1].id();

    app.timeline.edit_cursor_tick = 3_840;
    let _ = app.update(Message::AddProjectMarker);
    let third_id = app.project.markers()[2].id();

    // The closest marker wins, including a marker behind the cursor.
    app.timeline.edit_cursor_tick = 190;
    let command = CommandId::DeleteProjectMarkerNearCursor;
    assert!(commands::is_enabled(&app, command));
    assert_eq!(
        commands::find(&app, "delete marker near cursor"),
        Some(command)
    );
    assert_eq!(
        commands::nearest_project_marker_to_delete(&app),
        Some(first_id)
    );
    let _ = app.update(Message::DeleteProjectMarkerNearCursor);
    assert_eq!(app.project.markers().len(), 2);
    assert_eq!(app.project.markers()[0].id(), second_id);
    assert_eq!(app.project.markers()[1].id(), third_id);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.markers().len(), 3);

    // REAPER resolves equal-distance markers in favor of the earlier one.
    app.timeline.edit_cursor_tick = 960;
    assert_eq!(
        commands::nearest_project_marker_to_delete(&app),
        Some(first_id)
    );
    let _ = app.update(Message::DeleteProjectMarkerNearCursor);
    assert_eq!(app.project.markers()[0].id(), second_id);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.markers()[0].id(), first_id);

    // A marker exactly at the cursor qualifies at distance zero.
    app.timeline.edit_cursor_tick = 1_920;
    assert!(commands::is_enabled(&app, command));
    assert_eq!(
        commands::nearest_project_marker_to_delete(&app),
        Some(second_id)
    );
    let _ = app.update(Message::DeleteProjectMarkerNearCursor);
    assert_eq!(app.project.markers().len(), 2);
    assert_eq!(app.project.markers()[0].id(), first_id);
    assert_eq!(app.project.markers()[1].id(), third_id);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.markers().len(), 3);

    // When both sides are in range, compare their absolute time distances.
    app.timeline.edit_cursor_tick = 1_440;
    assert_eq!(
        commands::nearest_project_marker_to_delete(&app),
        Some(second_id)
    );
    let _ = app.update(Message::DeleteProjectMarkerNearCursor);
    assert_eq!(app.project.markers().len(), 2);
    assert_eq!(app.project.markers()[0].id(), first_id);
    assert_eq!(app.project.markers()[1].id(), third_id);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.markers().len(), 3);

    let _ = app.update(Message::DeleteProjectMarkerNearCursor);
    assert_eq!(app.project.markers().len(), 2);
    assert_eq!(app.project.markers()[0].id(), first_id);
    assert_eq!(app.project.markers()[1].id(), third_id);

    // Both remaining markers are exactly one second from the cursor and fall
    // outside the strict 1-second radius.
    app.timeline.edit_cursor_tick = 1_920;
    assert!(!commands::is_enabled(&app, command));
    assert_eq!(commands::find(&app, "delete marker near cursor"), None);

    // A one-tick move brings the later marker inside the radius.
    app.timeline.edit_cursor_tick = 1_921;
    assert!(commands::is_enabled(&app, command));
    assert_eq!(
        commands::nearest_project_marker_to_delete(&app),
        Some(third_id)
    );
    let _ = app.update(Message::DeleteProjectMarkerNearCursor);
    assert_eq!(app.project.markers().len(), 1);
    assert_eq!(app.project.markers()[0].id(), first_id);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.markers()[1].id(), third_id);

    // The past side of the radius is symmetric, with the same strict bound.
    app.timeline.edit_cursor_tick = 5_759;
    assert_eq!(
        commands::nearest_project_marker_to_delete(&app),
        Some(third_id)
    );
    app.timeline.edit_cursor_tick = 5_760;
    assert!(!commands::is_enabled(&app, command));
}

#[test]
fn numbered_marker_slots_match_visible_numbers_and_duplicate_order() {
    let mut app = App::default();
    for (number, position_ticks, name) in [
        (2, 0, "M2"),
        (1, 960, "M1-later"),
        (1, 1_920, "M1-duplicate"),
    ] {
        app.apply_action(
            DawAction::InsertNumberedProjectMarker {
                number,
                position_ticks,
                name: name.to_owned(),
            },
            "Project marker inserted",
        );
    }
    let first_number_one_id = app.project.markers()[1].id();
    let duplicate_number_one_id = app.project.markers()[2].id();
    app.timeline.edit_cursor_tick = 3_840;
    let _ = app.update(Message::MoveProjectMarkerSlot(1));
    let moved = app
        .project
        .markers()
        .iter()
        .find(|marker| marker.id() == first_number_one_id)
        .unwrap();
    assert_eq!(moved.position_ticks(), 3_840);
    assert_eq!(moved.number(), 1);
    assert_eq!(moved.name(), "M1-later");
    assert_eq!(
        app.project
            .markers()
            .iter()
            .find(|marker| marker.id() == duplicate_number_one_id)
            .unwrap()
            .position_ticks(),
        1_920
    );
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.project
            .markers()
            .iter()
            .find(|marker| marker.id() == first_number_one_id)
            .unwrap()
            .position_ticks(),
        960
    );

    app.timeline.edit_cursor_tick = 4_800;
    let _ = app.update(Message::MoveProjectMarkerSlot(3));
    let inserted = app
        .project
        .markers()
        .iter()
        .find(|marker| marker.number() == 3)
        .unwrap();
    assert_eq!(inserted.position_ticks(), 4_800);
}

#[test]
fn ruler_marker_move_is_one_undoable_edit_and_respects_busy_guard() {
    let mut app = App::default();
    app.apply_action(
        DawAction::InsertProjectMarker {
            position_ticks: 960,
            name: "Verse".to_owned(),
        },
        "Project marker inserted",
    );
    let marker_id = app.project.markers()[0].id();

    app.io_busy = true;
    app.handle_timeline_view_event(crate::timeline::TimelineEvent::MoveProjectMarker {
        marker_id,
        position_ticks: 1_920,
    });
    assert_eq!(app.project.markers()[0].position_ticks(), 960);
    assert!(app.status.contains("before moving a marker"));
    app.io_busy = false;

    app.handle_timeline_view_event(crate::timeline::TimelineEvent::MoveProjectMarker {
        marker_id,
        position_ticks: 1_920,
    });
    assert_eq!(app.project.markers()[0].id(), marker_id);
    assert_eq!(app.project.markers()[0].position_ticks(), 1_920);
    assert_eq!(app.project.markers()[0].name(), "Verse");
    assert_eq!(app.timeline.selected_project_marker, Some(marker_id));

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.markers()[0].id(), marker_id);
    assert_eq!(app.project.markers()[0].position_ticks(), 960);
    assert_eq!(app.project.markers()[0].name(), "Verse");
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.markers()[0].position_ticks(), 1_920);
}

#[test]
fn track_header_selection_uses_runtime_modifiers_and_keeps_a_primary_track() {
    let mut app = App::default();
    for _ in 0..3 {
        let _ = app.update(Message::AddTrack);
    }
    let tracks = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    app.timeline.selected_track = None;
    app.timeline.selected_tracks.clear();
    let window_id = iced::window::Id::unique();
    let _ = app.update(Message::RuntimeKeyboardEvent(
        iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(Modifiers::CTRL)),
        iced::event::Status::Ignored,
        window_id,
    ));
    let modifiers = app.keyboard_modifiers;
    assert_eq!(modifiers, Modifiers::CTRL);
    assert_eq!(app.timeline.modifiers, Modifiers::CTRL);
    let _ = app.update(Message::Timeline(TimelineEvent::SelectTrackWithModifiers {
        track_id: tracks[0],
        modifiers,
    }));
    let _ = app.update(Message::Timeline(TimelineEvent::SelectTrackWithModifiers {
        track_id: tracks[2],
        modifiers,
    }));

    assert_eq!(app.timeline.selected_track, Some(tracks[2]));
    assert_eq!(
        app.timeline.selected_tracks,
        HashSet::from([tracks[0], tracks[2]])
    );
    assert_eq!(app.selected_track_id(), Some(tracks[2]));
}

#[test]
fn left_click_in_midi_editor_dismisses_expression_context_menus_globally() {
    let midi_editor_window_id = iced::window::Id::unique();
    let other_window_id = iced::window::Id::unique();
    let left_click =
        iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left));
    let right_click = iced::Event::Mouse(iced::mouse::Event::ButtonPressed(
        iced::mouse::Button::Right,
    ));

    assert!(matches!(
        midi_expression_context_menu_event(
            left_click.clone(),
            iced::event::Status::Ignored,
            midi_editor_window_id,
        ),
        Some(Message::DismissMidiExpressionContextMenus(window_id))
            if window_id == midi_editor_window_id
    ));
    assert!(
        midi_expression_context_menu_event(
            right_click,
            iced::event::Status::Ignored,
            midi_editor_window_id,
        )
        .is_none()
    );

    let mut app = App {
        midi_editor_window_id: Some(midi_editor_window_id),
        ..App::default()
    };
    let _ = app.update(Message::DismissMidiExpressionContextMenus(other_window_id));
    assert_eq!(app.midi_expression_context_menu_epoch, 0);
    let _ = app.update(Message::DismissMidiExpressionContextMenus(
        midi_editor_window_id,
    ));
    assert_eq!(app.midi_expression_context_menu_epoch, 1);
}

#[test]
fn passive_pointer_cleanup_events_keep_main_menu_open() {
    let main_window_id = iced::window::Id::unique();
    let mut app = App {
        main_window_id: Some(main_window_id),
        ..App::default()
    };
    let _ = app.update(Message::ToggleMainMenu(MainMenu::Actions));

    let dismissal = midi_expression_context_menu_event(
        iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)),
        iced::event::Status::Ignored,
        main_window_id,
    )
    .expect("a left click should dismiss MIDI expression context menus");
    let _ = app.update(dismissal);
    let release_cleanup = fx_chain_plugin_drag_event(
        iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
            iced::mouse::Button::Left,
        )),
        iced::event::Status::Ignored,
        main_window_id,
    )
    .expect("a left button release should finish FX drag cleanup");
    let _ = app.update(release_cleanup);

    assert_eq!(app.active_menu, Some(MainMenu::Actions));
}

#[cfg(feature = "audio-device")]
#[test]
fn dirty_saved_project_can_start_recording_without_saving_again() {
    let directory = tempfile::tempdir().expect("test directory should be created");
    let project_path = directory.path().join("recording.aaadaw");
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    save_project_file(project_path.clone(), app.project.snapshot(), false)
        .expect("project should be saved before recording");
    let saved_revision = app.revision;
    let _ = app.update(Message::ProjectSaved(
        project_path,
        saved_revision,
        Ok(()),
        None,
        SharedProjectSessionLock::new(None),
    ));
    let _ = app.update(Message::ToggleRecordArm(track_id));

    assert!(
        app.is_dirty(),
        "arming the track should dirty the saved project"
    );

    let _ = app.update(Message::StartRecording);

    assert!(
        app.recording_starting,
        "recording was rejected: {}",
        app.status
    );
    assert!(app.playback_busy);
    assert_eq!(app.status, "Preparing playback at sample 0…");
}

#[cfg(feature = "audio-device")]
#[test]
fn unsaved_new_track_can_start_recording_without_saving_again() {
    let directory = tempfile::tempdir().expect("test directory should be created");
    let project_path = directory.path().join("recording.aaadaw");
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    save_project_file(project_path.clone(), app.project.snapshot(), false)
        .expect("project should be saved before recording");
    let saved_revision = app.revision;
    let _ = app.update(Message::ProjectSaved(
        project_path,
        saved_revision,
        Ok(()),
        None,
        SharedProjectSessionLock::new(None),
    ));

    let _ = app.update(Message::AddTrack);
    let unsaved_track_id = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .next_back()
        .expect("new track should be available");
    let _ = app.update(Message::ToggleRecordArm(unsaved_track_id));
    let _ = app.update(Message::StartRecording);

    assert!(
        app.recording_starting,
        "recording was rejected: {}",
        app.status
    );
    assert!(app.playback_busy);
}

#[cfg(feature = "audio-device")]
#[test]
fn recovery_recreates_a_newly_armed_track_from_manifest_metadata() {
    let directory = tempfile::tempdir().expect("test directory should be created");
    let project_path = directory.path().join("recording.aaadaw");
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    app.project_path = Some(project_path.clone());
    let candidate = aaadaw_app::RecordingRecoveryCandidate {
        manifest_path: directory.path().join("recording.recovery.json"),
        manifest: aaadaw_app::RecordingRecoveryManifest {
            version: 1,
            project_path,
            sample_rate: 48_000,
            track_ids: vec![u64::MAX],
            track_names: vec!["New Audio Track".to_owned()],
            start_sample: Some(24_000),
            start_sample_is_estimate: false,
            stem: "recording".to_owned(),
            segments: Vec::new(),
            finalized: false,
            discarded_frames: 0,
            discarded_tail_bytes: 0,
        },
        segment_paths: vec![directory.path().join("recovered.wav")],
        recorded_frames: 4,
        discarded_frames: 0,
        discarded_tail_bytes: 0,
    };

    let _task = app.recording_recovery_prepared(Ok(candidate));

    let restored_track = app
        .project
        .tracks()
        .last()
        .expect("missing recorded track should be restored");
    assert_eq!(restored_track.name(), "New Audio Track");
    assert_eq!(
        app.record_import_tracks
            .as_ref()
            .expect("recovery import should be prepared")
            .track_ids,
        [restored_track.id()]
    );
}

#[cfg(feature = "audio-device")]
#[test]
fn unavailable_recording_input_reports_failure_without_inserting_an_item() {
    let mut app = App::default();
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Armed take".to_owned(),
        })
        .unwrap();
    app.recording_starting = true;
    app.recording_tracks = vec![app.project.tracks()[0].id()];
    let input_error =
        "JACK has 0 physical audio input port(s); stereo recording needs two".to_owned();
    let result: Result<ActiveRecording, String> = Err(input_error.clone());

    let _ = app.update(Message::RecordingStarted(SharedRecordingStart(
        std::sync::Arc::new(std::sync::Mutex::new(Some(result))),
    )));

    assert!(!app.recording_starting);
    assert!(app.recording_tracks.is_empty());
    assert!(app.project.audio_items().is_empty());
    assert!(app.status.contains("Recording could not start"));
    assert!(app.status.contains(&input_error));
}

#[cfg(all(
    feature = "cpal-backend",
    any(target_os = "windows", target_os = "macos")
))]
#[test]
fn cpal_output_enumeration_failures_are_visible_and_clear_loading_state() {
    let mut app = App::default();
    app.cpal_output_devices_loading = true;
    app.cpal_output_devices
        .push(aaadaw_engine::CpalOutputDeviceInfo {
            id: "cpal:stale".to_owned(),
            name: "Stale output".to_owned(),
        });

    app.finish_cpal_output_device_enumeration(Err("endpoint service stopped".to_owned()));

    assert!(!app.cpal_output_devices_loading);
    assert!(app.cpal_output_devices.is_empty());
    assert_eq!(
        app.cpal_output_devices_error.as_deref(),
        Some("endpoint service stopped")
    );
    assert_eq!(
        app.audio_settings_feedback,
        "System audio output devices could not be listed: endpoint service stopped"
    );
}

#[cfg(all(
    feature = "cpal-backend",
    any(target_os = "windows", target_os = "macos")
))]
#[test]
fn cpal_input_enumeration_failures_are_visible_and_clear_loading_state() {
    let mut app = App::default();
    app.cpal_input_devices_loading = true;
    app.audio_settings.cpal_input_device_id = Some("cpal:stored-input".to_owned());
    app.cpal_input_devices
        .push(aaadaw_engine::CpalInputDeviceInfo {
            id: "cpal:stale-input".to_owned(),
            name: "Stale input".to_owned(),
        });

    app.finish_cpal_input_device_enumeration(Err("endpoint service stopped".to_owned()));

    assert!(!app.cpal_input_devices_loading);
    assert!(app.cpal_input_devices.is_empty());
    assert_eq!(
        app.cpal_input_devices_error.as_deref(),
        Some("endpoint service stopped")
    );
    assert_eq!(
        app.audio_settings.cpal_input_device_id.as_deref(),
        Some("cpal:stored-input")
    );
    assert_eq!(
        app.audio_settings_feedback,
        "System audio input devices could not be listed: endpoint service stopped"
    );
}

#[cfg(all(feature = "jack-backend", feature = "pipewire-backend"))]
#[test]
fn saved_playback_backend_restores_only_when_available_in_this_build() {
    let mut app = App::default();
    app.restore_playback_backend(Some(super::audio_config::PlaybackBackendSetting::PipeWire));
    assert_eq!(app.selected_playback_backend(), PlaybackBackend::PipeWire);

    app.restore_playback_backend(Some(super::audio_config::PlaybackBackendSetting::Cpal));
    assert_eq!(app.selected_playback_backend(), PlaybackBackend::PipeWire);
}

#[test]
fn first_new_track_is_selected_but_later_tracks_do_not_change_selection() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let first_track_id = app.project.tracks()[0].id();
    assert_eq!(app.timeline.selected_track, Some(first_track_id));

    app.timeline.selected_track = None;
    let _ = app.update(Message::AddTrack);
    assert_eq!(app.project.tracks().len(), 2);
    assert_eq!(app.timeline.selected_track, None);
}

#[test]
#[cfg(feature = "audio-device")]
fn resetting_live_track_meters_preserves_peak_hold_and_clip_history() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.track_peak_levels.insert(track_id, [0.75, 0.5]);
    app.master_peak_level = [0.9, 0.8];
    app.input_peak_level = [0.65, 0.4];
    app.track_peak_holds
        .entry(track_id)
        .or_default()
        .observe([0.75, 1.05]);
    app.input_peak_hold.observe([0.65, 1.1]);
    app.master_peak_hold.observe([0.9, 0.8]);
    app.master_guard_ticks_remaining = 5;

    app.reset_track_meters();

    assert!(app.track_peak_levels.is_empty());
    assert_eq!(app.input_peak_level, [0.0; 2]);
    assert_eq!(app.master_peak_level, [0.0; 2]);
    assert_eq!(app.track_peak_holds[&track_id].levels, [0.75, 1.05]);
    assert_eq!(app.track_peak_holds[&track_id].clipped, [false, true]);
    assert_eq!(app.input_peak_hold.levels, [0.65, 1.1]);
    assert_eq!(app.input_peak_hold.clipped, [false, true]);
    assert_eq!(app.master_peak_hold.levels, [0.9, 0.8]);
    assert_eq!(app.master_guard_ticks_remaining, 0);
}

#[test]
#[cfg(feature = "audio-device")]
fn android_route_loss_recovery_only_resumes_active_playback() {
    assert_eq!(
        App::route_loss_resume_sample(true, true, 48_000),
        Some(48_000)
    );
    assert_eq!(App::route_loss_resume_sample(true, false, 48_000), None);
    assert_eq!(App::route_loss_resume_sample(false, true, 48_000), None);
}

#[test]
#[cfg(feature = "audio-device")]
fn clearing_track_and_master_meter_history_resets_peak_values_and_clip_latches() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.track_peak_levels.insert(track_id, [0.75, 1.05]);
    app.track_peak_holds
        .entry(track_id)
        .or_default()
        .observe([0.75, 1.05]);
    app.master_peak_level = [0.9, 1.05];
    app.master_peak_hold.observe([0.9, 1.05]);
    app.input_peak_level = [0.8, 1.05];
    app.input_peak_hold.observe([0.8, 1.05]);

    let _ = app.update(Message::ClearTrackMeter(track_id));
    let _ = app.update(Message::ClearInputMeter);
    let _ = app.update(Message::ClearMasterMeter);

    assert!(!app.track_peak_levels.contains_key(&track_id));
    assert_eq!(app.track_peak_holds.get(&track_id), None);
    assert_eq!(app.master_peak_level, [0.0; 2]);
    assert_eq!(app.master_peak_hold, StereoPeakHold::default());
    assert_eq!(app.input_peak_level, [0.0; 2]);
    assert_eq!(app.input_peak_hold, StereoPeakHold::default());
}

#[test]
#[cfg(feature = "audio-device")]
fn output_device_loss_stops_transport_and_clears_track_meters() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.track_peak_levels.insert(track_id, [0.9, 0.8]);
    app.master_peak_level = [0.95, 0.85];
    app.master_guard_ticks_remaining = 4;
    app.playback_playing = true;
    app.playback_paused = true;

    let _ = app.handle_playback_device_lost();

    assert!(!app.playback_playing);
    assert!(!app.playback_paused);
    assert!(app.track_peak_levels.is_empty());
    assert_eq!(app.master_peak_level, [0.0; 2]);
    assert_eq!(app.master_guard_ticks_remaining, 0);
    assert!(app.status.contains("output device unavailable"));
}

#[test]
fn switching_arrange_and_mixer_preserves_track_selection_and_transport_position() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.timeline.selected_track = Some(track_id);
    app.timeline.edit_cursor_tick = 1_920;
    #[cfg(feature = "audio-device")]
    {
        app.playhead_sample = 24_000;
    }

    let _ = app.update(Message::ShowMainWorkspace(MainWorkspace::Mixer));
    assert_eq!(app.main_workspace, MainWorkspace::Mixer);
    assert_eq!(app.timeline.selected_track, Some(track_id));
    assert_eq!(app.timeline.edit_cursor_tick, 1_920);
    assert_eq!(app.project.tracks()[0].id(), track_id);
    #[cfg(feature = "audio-device")]
    assert_eq!(app.playhead_sample, 24_000);

    let _ = app.update(Message::ShowMainWorkspace(MainWorkspace::Arrangement));
    assert_eq!(app.main_workspace, MainWorkspace::Arrangement);
    assert_eq!(app.timeline.selected_track, Some(track_id));
    assert_eq!(app.timeline.edit_cursor_tick, 1_920);
    #[cfg(feature = "audio-device")]
    assert_eq!(app.playhead_sample, 24_000);
}

#[test]
fn mobile_utility_routes_keep_editor_state_and_use_one_main_window() {
    let mut app = App {
        main_window_size: Some(iced::Size::new(420.0, 800.0)),
        shell_profile: super::ShellProfile::Touch,
        ..App::default()
    };
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.timeline.selected_track = Some(track_id);
    app.timeline.edit_cursor_tick = 1_920;
    app.clap_plugin_scan.plugins = vec![
        aaadaw_app::ClapPluginDescriptor {
            entry_path: std::path::PathBuf::from("/plugins/test-effect.clap"),
            plugin_id: "org.example.test-effect".to_owned(),
            name: "Test Effect".to_owned(),
            vendor: Some("Example".to_owned()),
            features: vec!["audio-effect".to_owned()],
        },
        aaadaw_app::ClapPluginDescriptor {
            entry_path: std::path::PathBuf::from("/plugins/test-instrument.clap"),
            plugin_id: "org.example.test-instrument".to_owned(),
            name: "Test Instrument".to_owned(),
            vendor: Some("Example".to_owned()),
            features: vec!["instrument".to_owned()],
        },
    ];

    let _ = app.update(Message::OpenSettings);
    assert_eq!(app.mobile_panel, MobilePanel::Settings);
    assert_eq!(app.settings_window_id, None);
    let _ = app.update(Message::Escape);
    assert_eq!(app.mobile_panel, MobilePanel::Editor);

    let _ = app.update(Message::OpenTempoMap);
    assert_eq!(app.mobile_panel, MobilePanel::TimeMap);
    assert_eq!(app.tempo_map_window_id, None);
    let _ = app.update(Message::MobileNavigateBack);
    let _ = app.update(Message::OpenMeterMap);
    assert_eq!(app.mobile_panel, MobilePanel::TimeMap);
    assert_eq!(app.time_map_tab, super::TimeMapTab::Meter);
    let _ = app.update(Message::MobileNavigateBack);

    let _ = app.update(Message::ToggleMediaBrowserPanel);
    assert_eq!(app.mobile_panel, MobilePanel::MediaBrowser);
    let _ = app.update(Message::MobileNavigateBack);

    let _ = app.update(Message::OpenTrackFxChain(track_id));
    assert_eq!(app.mobile_panel, MobilePanel::FxChain);
    assert_eq!(app.fx_chain_window_id, None);
    let _ = app.update(Message::OpenPluginPicker);
    assert_eq!(app.mobile_panel, MobilePanel::PluginPicker);
    assert_eq!(app.plugin_picker_window_id, None);

    let _ = app.update(Message::OpenClapPluginSettings);
    assert_eq!(app.mobile_panel, MobilePanel::Settings);
    assert_eq!(app.settings_category, super::SettingsCategory::ClapPlugins);
    let _ = app.update(Message::MobileNavigateBack);
    assert_eq!(app.mobile_panel, MobilePanel::PluginPicker);
    assert_eq!(app.plugin_picker_track_id, Some(track_id));

    let _ = app.update(Message::AddScannedPlugin(
        "org.example.test-effect".to_owned(),
    ));
    assert_eq!(app.mobile_panel, MobilePanel::FxChain);
    assert_eq!(app.project.tracks()[0].fx_chain().len(), 1);
    assert_eq!(app.fx_chain_selected_index, Some(0));
    assert_eq!(app.fx_chain_window_id, None);
    assert_eq!(app.plugin_picker_window_id, None);

    let _ = app.update(Message::MobileNavigateBack);
    let _ = app.update(Message::OpenTrackInstrumentPicker(track_id));
    assert_eq!(app.mobile_panel, MobilePanel::PluginPicker);
    let _ = app.update(Message::SelectScannedInstrument(
        "org.example.test-instrument".to_owned(),
    ));
    assert_eq!(app.mobile_panel, MobilePanel::Editor);
    assert_eq!(app.plugin_picker_window_id, None);
    assert_eq!(
        app.project.tracks()[0]
            .instrument()
            .map(|instrument| instrument.plugin_id()),
        Some("org.example.test-instrument")
    );

    assert_eq!(app.timeline.selected_track, Some(track_id));
    assert_eq!(app.timeline.edit_cursor_tick, 1_920);
}

#[test]
fn mobile_settings_shortcut_capture_and_system_back_are_handled() {
    let main_window_id = iced::window::Id::unique();
    let mut app = App {
        main_window_id: Some(main_window_id),
        main_window_size: Some(iced::Size::new(420.0, 800.0)),
        shell_profile: super::ShellProfile::Touch,
        ..App::default()
    };
    let _ = app.update(Message::OpenSettings);
    let _ = app.update(Message::StartShortcutCapture("edit.undo".to_owned()));

    let key = Key::Character("k".into());
    let key_event = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::KeyK),
        location: iced::keyboard::Location::Standard,
        modifiers: Modifiers::COMMAND,
        text: None,
        repeat: false,
    });
    assert!(matches!(
        keyboard_shortcut_event(
            key_event,
            iced::event::Status::Captured,
            main_window_id,
            Some(main_window_id),
            None,
            app.shortcut_capture_id.as_deref(),
        ),
        Some(Message::ShortcutCaptureKey { action_id, key, modifiers })
            if action_id == "edit.undo" && key == "K" && modifiers == Modifiers::COMMAND
    ));

    let _ = app.update(Message::Escape);
    assert!(app.shortcut_capture_id.is_none());
    assert_eq!(app.mobile_panel, MobilePanel::Settings);

    let _ = app.update(Message::StartShortcutCapture("edit.undo".to_owned()));
    let _ = app.update(Message::MobileNavigateBack);
    assert!(app.shortcut_capture_id.is_none());
    assert_eq!(app.mobile_panel, MobilePanel::Editor);

    let _ = app.update(Message::OpenSettings);
    let _ = app.update(Message::StartShortcutCapture("edit.undo".to_owned()));
    let _ = app.update(Message::ToggleMediaBrowserPanel);
    assert!(app.shortcut_capture_id.is_none());
    assert_eq!(app.mobile_panel, MobilePanel::MediaBrowser);
    let _ = app.update(Message::MobileNavigateBack);

    let _ = app.update(Message::OpenSettings);
    let back_event = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: Key::Named(iced::keyboard::key::Named::BrowserBack),
        modified_key: Key::Named(iced::keyboard::key::Named::BrowserBack),
        physical_key: iced::keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        location: iced::keyboard::Location::Standard,
        modifiers: Modifiers::NONE,
        text: None,
        repeat: false,
    });
    assert!(matches!(
        mobile_back_event(&back_event, main_window_id, Some(main_window_id), true,),
        Some(Message::MobileNavigateBack)
    ));
    assert!(mobile_back_event(&back_event, main_window_id, Some(main_window_id), false,).is_none());
}

#[test]
fn arrangement_automation_add_move_delete_selection_and_undo() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    let expected_sample = app.project.sample_at_tick(960).unwrap();
    let _ = app.update(Message::Timeline(
        super::super::timeline::TimelineEvent::InsertVolumeAutomationAt {
            track_index: 0,
            tick: 960,
            gain_db: -12.0,
        },
    ));
    assert_eq!(app.project.tracks()[0].volume_automation().len(), 1);
    assert_eq!(
        app.project.tracks()[0].volume_automation()[0].sample(),
        expected_sample
    );
    assert!(app.timeline.volume_automation_tracks.contains(&track_id));
    let point = aaadaw_core::VolumeAutomationPoint::new(expected_sample + 24_000, -6.0).unwrap();
    let _ = app.update(Message::Timeline(
        super::super::timeline::TimelineEvent::SelectVolumeAutomationPoint { track_id, index: 0 },
    ));
    let _ = app.update(Message::Timeline(
        super::super::timeline::TimelineEvent::SetVolumeAutomation(track_id, vec![point]),
    ));
    assert_eq!(app.project.tracks()[0].volume_automation(), &[point]);
    let _ = app.update(Message::Timeline(
        super::super::timeline::TimelineEvent::DeleteVolumeAutomationPoint { track_id, index: 0 },
    ));
    assert!(app.project.tracks()[0].volume_automation().is_empty());
    assert!(app.project.undo().unwrap());
    assert_eq!(app.project.tracks()[0].volume_automation(), &[point]);
}

#[test]
fn delete_by_focus_removes_selected_volume_automation_point_and_undoes() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    let sample = app.project.sample_at_tick(960).unwrap();
    let point = aaadaw_core::VolumeAutomationPoint::new(sample, -6.0).unwrap();
    let _ = app.update(Message::Timeline(
        super::super::timeline::TimelineEvent::SetVolumeAutomation(track_id, vec![point]),
    ));
    let _ = app.update(Message::Timeline(
        super::super::timeline::TimelineEvent::SelectVolumeAutomationPoint { track_id, index: 0 },
    ));

    let revision = app.revision;
    let _ = app.update(Message::ExecuteCommand(CommandId::DeleteByFocus));

    assert!(app.project.tracks()[0].volume_automation().is_empty());
    assert_eq!(app.project.tracks().len(), 1);
    assert_eq!(app.revision, revision + 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks()[0].volume_automation(), &[point]);
}

#[test]
fn arrangement_fx_automation_edits_points_undoes_and_redoes() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::SetTrackFxChain {
            track_id,
            plugins: vec![TrackFxPlugin::new("org.example.eq", "/plugins/eq.clap").unwrap()],
        })
        .unwrap();
    app.timeline.rebuild(&app.project);
    app.timeline
        .handle(super::super::timeline::TimelineEvent::ToggleFxAutomation {
            track_id,
            chain_index: 0,
            parameter_id: 7,
            name: "Frequency".to_owned(),
            value_range: (20.0, 20_000.0),
            stepped: false,
        });
    app.timeline
        .handle(super::super::timeline::TimelineEvent::ToggleFxAutomation {
            track_id,
            chain_index: 0,
            parameter_id: 8,
            name: "Q".to_owned(),
            value_range: (0.1, 10.0),
            stepped: false,
        });
    assert_eq!(app.timeline.fx_automation_lanes.len(), 2);

    let first = aaadaw_core::FxParameterAutomationPoint::new(
        app.project.sample_at_tick(960).unwrap(),
        440.0,
    )
    .unwrap();
    let second = aaadaw_core::FxParameterAutomationPoint::new(
        app.project.sample_at_tick(1920).unwrap(),
        880.0,
    )
    .unwrap();
    let update = |points, selected_point| {
        Message::Timeline(super::super::timeline::TimelineEvent::SetFxAutomation {
            track_id,
            chain_index: 0,
            parameter_id: 7,
            points,
            selected_point,
        })
    };
    let _ = app.update(update(vec![first], Some(0)));
    let _ = app.update(update(vec![first, second], Some(1)));
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0]
            .parameter_automation_for(7)
            .unwrap()
            .points(),
        &[first, second]
    );

    let moved = aaadaw_core::FxParameterAutomationPoint::new(
        app.project.sample_at_tick(2880).unwrap(),
        1_200.0,
    )
    .unwrap();
    let _ = app.update(update(vec![first, moved], Some(1)));
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0]
            .parameter_automation_for(7)
            .unwrap()
            .points(),
        &[first, moved]
    );
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0]
            .parameter_automation_for(7)
            .unwrap()
            .points(),
        &[first, second]
    );
    let _ = app.update(Message::Redo);
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0]
            .parameter_automation_for(7)
            .unwrap()
            .points(),
        &[first, moved]
    );
    assert!(app.timeline.fx_automation_lanes.contains(&(track_id, 0, 8)));

    let _ = app.update(Message::Timeline(
        super::super::timeline::TimelineEvent::DeleteFxAutomationPoint {
            track_id,
            chain_index: 0,
            parameter_id: 7,
            index: 0,
        },
    ));
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0]
            .parameter_automation_for(7)
            .unwrap()
            .points(),
        &[moved]
    );
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0]
            .parameter_automation_for(7)
            .unwrap()
            .points(),
        &[first, moved]
    );
    let _ = app.update(Message::Redo);
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0]
            .parameter_automation_for(7)
            .unwrap()
            .points(),
        &[moved]
    );
}

#[test]
fn tempo_map_editor_adds_edits_curves_deletes_and_undoes_as_one_action() {
    let mut app = App::default();
    app.timeline.edit_cursor_tick = 960;
    app.refresh_tempo_map_edits();

    let _ = app.update(Message::AddTempoPoint);
    assert_eq!(app.tempo_map_edits.len(), 2);
    let _ = app.update(Message::TempoPointBpmChanged(1, "90".to_owned()));
    let _ = app.update(Message::CycleTempoCurve(0));
    let previous_revision = app.revision;
    let _ = app.update(Message::ApplyTempoMap);

    assert_eq!(app.revision, previous_revision + 1);
    #[cfg(feature = "audio-device")]
    assert!(app.playback_graph_dirty);
    assert_eq!(app.project.tempo_at_tick(960), 90.0);
    assert_eq!(
        app.project.tempo_points().collect::<Vec<_>>(),
        vec![
            (0, 120.0, aaadaw_core::TempoCurve::Linear),
            (960, 90.0, aaadaw_core::TempoCurve::Step),
        ]
    );
    assert!(app.project.undo().unwrap());
    assert_eq!(
        app.project.tempo_points().collect::<Vec<_>>(),
        vec![(0, 120.0, aaadaw_core::TempoCurve::Step)]
    );
    assert!(app.project.redo().unwrap());
    assert_eq!(app.project.tempo_at_tick(960), 90.0);

    let _ = app.update(Message::TempoPointTickChanged(1, "1920".to_owned()));
    let _ = app.update(Message::ApplyTempoMap);
    assert_eq!(app.project.tempo_points().count(), 2);
    assert_eq!(app.project.tempo_at_tick(1920), 90.0);
    assert!(app.project.undo().unwrap());
    assert_eq!(app.project.tempo_at_tick(960), 90.0);
    assert!(app.project.redo().unwrap());
    assert_eq!(app.project.tempo_at_tick(1920), 90.0);

    app.project.undo().unwrap();
    app.project.undo().unwrap();
    app.refresh_tempo_map_edits();
    app.timeline.edit_cursor_tick = 960;
    let _ = app.update(Message::AddTempoPoint);
    let _ = app.update(Message::DeleteTempoPoint(0));
    assert_eq!(
        app.tempo_map_edits.len(),
        2,
        "the initial point is protected"
    );
    let _ = app.update(Message::DeleteTempoPoint(1));
    let _ = app.update(Message::ApplyTempoMap);
    assert_eq!(app.project.tempo_points().count(), 1);
}

#[test]
fn meter_map_editor_applies_bar_aligned_changes_as_one_undoable_map() {
    let mut app = App::default();
    app.timeline.edit_cursor_tick = 2880;
    app.refresh_meter_map_edits();

    let _ = app.update(Message::AddMeterPoint);
    let _ = app.update(Message::MeterPointNumeratorChanged(0, "3".to_owned()));
    let _ = app.update(Message::MeterPointNumeratorChanged(1, "7".to_owned()));
    let _ = app.update(Message::MeterPointDenominatorChanged(1, "8".to_owned()));
    let revision = app.revision;
    let _ = app.update(Message::ApplyMeterMap);

    assert_eq!(app.revision, revision + 1);
    assert_eq!(
        app.project.time_signature_map(),
        vec![
            aaadaw_core::MeterPointSnapshot {
                start_tick: 0,
                numerator: 3,
                denominator: 4,
            },
            aaadaw_core::MeterPointSnapshot {
                start_tick: 2880,
                numerator: 7,
                denominator: 8,
            },
        ]
    );
    assert!(app.project.undo().unwrap());
    assert!(app.project.redo().unwrap());
    assert_eq!(
        app.project.time_signature_at_tick(2880),
        aaadaw_core::TimeSignature::new(7, 8).unwrap()
    );
}

#[test]
fn new_project_is_in_file_menu_and_prompts_before_discarding_dirty_work() {
    let mut app = App::default();
    let new_project = commands::for_menu(&app, MainMenu::File)
        .into_iter()
        .find(|entry| entry.id == CommandId::NewProject)
        .expect("File menu should expose New project");
    assert!(new_project.enabled);
    assert_eq!(new_project.label, "New project");

    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    assert!(commands::is_enabled(&app, CommandId::NewProject));
    let _ = app.update(Message::NewProject);
    assert_eq!(app.project.tracks()[0].id(), track_id);
    assert_eq!(
        app.pending_project_transition,
        Some(super::PendingProjectTransition::NewProject)
    );
    assert_eq!(app.status, "Save changes to the current project?");
    let _ = app.update(Message::DiscardProjectChanges);
    assert!(app.project.tracks().is_empty());

    app.saved_revision = app.revision;
    app.project_path = Some(std::path::PathBuf::from("saved.aaadaw"));
    app.project_path_query = "saved.aaadaw".to_owned();
    let _ = app.update(Message::NewProject);
    assert!(app.project.tracks().is_empty());
    assert!(app.project_path.is_none());
    assert!(app.project_path_query.is_empty());
    assert_eq!(app.revision, 0);
    assert_eq!(app.saved_revision, 0);
    assert!(app.status.starts_with("New project created"));
}

#[test]
fn close_project_matches_native_menu_and_preserves_unsaved_work_until_confirmed() {
    let mut app = App::default();
    let file_menu = commands::for_menu(&app, MainMenu::File);
    let close_project = file_menu
        .iter()
        .find(|entry| entry.id == CommandId::CloseProject)
        .expect("File menu should expose Close project");
    assert_eq!(close_project.label, "Close project");
    assert!(close_project.enabled);
    assert!(close_project.separator_before);
    let action = commands::matching_actions_menu(&app, "close current project tab")
        .into_iter()
        .find(|entry| entry.id == CommandId::CloseProject)
        .expect("Action List should expose the native close-action name");
    assert_eq!(action.category, "");
    assert_eq!(action.label, "Close current project tab");
    let close_binding = commands::shortcut_entries(&app)
        .into_iter()
        .find(|entry| entry.id == "file.close-project")
        .expect("Close project should have a configurable shortcut entry");
    assert_eq!(close_binding.default_binding, "Ctrl+F4");
    let bindings = app
        .shortcut_bindings
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(
        commands::from_shortcut(
            &Key::Named(iced::keyboard::key::Named::F4),
            Modifiers::CTRL,
            &bindings,
            &app.action_macros,
        ),
        Some(CommandId::CloseProject)
    );
    drop(bindings);

    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project_path = Some(std::path::PathBuf::from("current.aaadaw"));
    app.project_path_query = "current.aaadaw".to_owned();
    let _ = app.update(Message::ExecuteCommand(CommandId::CloseProject));
    assert_eq!(app.project.tracks()[0].id(), track_id);
    assert_eq!(
        app.pending_project_transition,
        Some(super::PendingProjectTransition::CloseProject)
    );
    let _ = app.update(Message::CancelProjectTransition);
    assert!(app.is_dirty());
    assert_eq!(
        app.project_path.as_deref(),
        Some(std::path::Path::new("current.aaadaw"))
    );

    let _ = app.update(Message::ExecuteCommand(CommandId::CloseProject));
    let _ = app.update(Message::DiscardProjectChanges);
    assert!(app.project.tracks().is_empty());
    assert!(app.project_path.is_none());
    assert!(app.project_path_query.is_empty());
    assert_eq!(app.revision, 0);
    assert_eq!(app.saved_revision, 0);
    assert_eq!(app.pending_project_transition, None);
    assert_eq!(app.status, "Project closed");
}

#[test]
fn save_project_as_matches_native_ctrl_alt_s_and_opens_the_save_picker() {
    let window_id = iced::window::Id::unique();
    let mut app = App {
        main_window_id: Some(window_id),
        ..App::default()
    };
    let save_as = commands::for_menu(&app, MainMenu::File)
        .into_iter()
        .find(|entry| entry.id == CommandId::SaveProjectAs)
        .expect("File menu should expose Save project as");
    assert_eq!(save_as.label, "Save project as...");
    let binding = commands::shortcut_entries(&app)
        .into_iter()
        .find(|entry| entry.id == "file.save-project-as")
        .expect("Save project as should have a configurable shortcut entry");
    assert_eq!(binding.default_binding, "Ctrl+Alt+S");
    let bindings = app
        .shortcut_bindings
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(
        commands::from_shortcut(
            &Key::Character("s"),
            Modifiers::CTRL.union(Modifiers::ALT),
            &bindings,
            &app.action_macros,
        ),
        Some(CommandId::SaveProjectAs)
    );
    drop(bindings);

    let _ = app.update(Message::ExecuteCommand(CommandId::SaveProjectAs));
    assert!(app.path_picker_busy);
    assert_eq!(app.active_menu, None);
}

#[test]
fn save_new_project_version_matches_native_action_and_shortcut() {
    let mut app = App::default();
    let file_menu = commands::for_menu(&app, MainMenu::File);
    let save_as_index = file_menu
        .iter()
        .position(|entry| entry.id == CommandId::SaveProjectAs)
        .expect("File menu should expose Save project as");
    let save_version = file_menu
        .iter()
        .find(|entry| entry.id == CommandId::SaveProjectNewVersion)
        .expect("File menu should expose Save new version of project");
    assert_eq!(save_version.label, "Save new version of project");
    assert_eq!(save_version.shortcut.as_deref(), Some("Ctrl+Alt+Shift+S"));
    assert_eq!(
        file_menu[save_as_index + 1].id,
        CommandId::SaveProjectNewVersion,
        "REAPER places the new-version action after Save project as"
    );

    let action = app
        .action_list_entries()
        .into_iter()
        .find(|entry| entry.id == CommandId::SaveProjectNewVersion)
        .expect("Action List should expose Save new version of project");
    assert_eq!(action.category, "File");
    assert_eq!(
        action.label,
        "Save new version of project (automatically increment project name)"
    );
    let binding = commands::shortcut_entries(&app)
        .into_iter()
        .find(|entry| entry.id == "file.save-project-new-version")
        .expect("Save new version should have a configurable shortcut entry");
    assert_eq!(binding.default_binding, "Ctrl+Alt+Shift+S");
    let bindings = app
        .shortcut_bindings
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(
        commands::from_shortcut(
            &Key::Character("s"),
            Modifiers::CTRL
                .union(Modifiers::ALT)
                .union(Modifiers::SHIFT),
            &bindings,
            &app.action_macros,
        ),
        Some(CommandId::SaveProjectNewVersion)
    );
    drop(bindings);

    let _ = app.update(Message::ExecuteCommand(CommandId::SaveProjectNewVersion));
    assert!(app.path_picker_busy);
    assert_eq!(app.active_menu, None);
}

#[test]
fn save_new_project_version_dispatches_to_a_non_overwriting_incremented_path() {
    let directory = tempfile::tempdir().expect("test directory should be created");
    let current = directory.path().join("Mix.aaadaw");
    let expected = directory.path().join("Mix_001.aaadaw");
    let mut app = App {
        project_path_query: current.to_string_lossy().into_owned(),
        project_path: Some(current),
        ..App::default()
    };

    let _ = app.update(Message::ExecuteCommand(CommandId::SaveProjectNewVersion));

    assert!(app.io_busy);
    assert_eq!(app.status, format!("Saving {}…", expected.display()));
}

#[test]
fn file_menu_matches_native_save_all_and_close_all_actions_for_one_project() {
    let app = App::default();
    let file_menu = commands::for_menu(&app, MainMenu::File);
    let save_version_index = file_menu
        .iter()
        .position(|entry| entry.id == CommandId::SaveProjectNewVersion)
        .expect("File menu should expose Save new version of project");
    let save_all = file_menu
        .iter()
        .find(|entry| entry.id == CommandId::SaveAllProjects)
        .expect("File menu should expose Save all projects");
    let close_project_index = file_menu
        .iter()
        .position(|entry| entry.id == CommandId::CloseProject)
        .expect("File menu should expose Close project");
    let close_all = file_menu
        .iter()
        .find(|entry| entry.id == CommandId::CloseAllProjects)
        .expect("File menu should expose Close all projects");

    assert_eq!(save_all.label, "Save all projects");
    assert!(save_all.enabled);
    assert_eq!(save_all.shortcut, None);
    assert_eq!(
        file_menu[save_version_index + 1].id,
        CommandId::SaveAllProjects
    );
    assert_eq!(close_all.label, "Close all projects");
    assert!(close_all.enabled);
    assert_eq!(close_all.shortcut, None);
    assert_eq!(
        file_menu[close_project_index + 1].id,
        CommandId::CloseAllProjects
    );

    for (id, label) in [
        (CommandId::SaveAllProjects, "Save all projects"),
        (CommandId::CloseAllProjects, "Close all projects"),
    ] {
        let action = app
            .action_list_entries()
            .into_iter()
            .find(|entry| entry.id == id)
            .expect("Action List should expose the native File action");
        assert_eq!(action.category, "File");
        assert_eq!(action.label, label);
        assert_eq!(action.shortcut, None);
    }
}

#[test]
fn save_all_projects_routes_through_the_active_project_save() {
    let directory = tempfile::tempdir().expect("test directory should be created");
    let current = directory.path().join("Mix.aaadaw");
    let mut app = App {
        project_path_query: current.to_string_lossy().into_owned(),
        project_path: Some(current.clone()),
        ..App::default()
    };

    let _ = app.update(Message::ExecuteCommand(CommandId::SaveAllProjects));

    assert!(app.io_busy);
    assert_eq!(app.status, format!("Saving {}…", current.display()));
}

#[test]
fn close_all_projects_routes_through_the_guarded_single_project_close() {
    let mut app = App {
        project_path_query: "current.aaadaw".to_owned(),
        project_path: Some(std::path::PathBuf::from("current.aaadaw")),
        ..App::default()
    };
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();

    let _ = app.update(Message::ExecuteCommand(CommandId::CloseAllProjects));

    assert_eq!(app.project.tracks()[0].id(), track_id);
    assert_eq!(
        app.pending_project_transition,
        Some(super::PendingProjectTransition::CloseProject)
    );
    let _ = app.update(Message::CancelProjectTransition);
    assert!(app.is_dirty());
    assert_eq!(
        app.project_path.as_deref(),
        Some(std::path::Path::new("current.aaadaw"))
    );

    let _ = app.update(Message::ExecuteCommand(CommandId::CloseAllProjects));
    let _ = app.update(Message::DiscardProjectChanges);
    assert!(app.project.tracks().is_empty());
    assert!(app.project_path.is_none());
    assert_eq!(app.pending_project_transition, None);
}

#[test]
fn quit_matches_native_file_menu_and_uses_dirty_close_confirmation() {
    let window_id = iced::window::Id::unique();
    let mut app = App {
        main_window_id: Some(window_id),
        ..App::default()
    };
    let file_menu = commands::for_menu(&app, MainMenu::File);
    let quit = file_menu
        .last()
        .expect("Quit should be the last File command");
    assert_eq!(quit.id, CommandId::Quit);
    assert_eq!(quit.label, "Quit");
    assert!(quit.enabled);
    assert!(quit.separator_before);
    let binding = commands::shortcut_entries(&app)
        .into_iter()
        .find(|entry| entry.id == "file.quit")
        .expect("Quit should have a configurable shortcut entry");
    assert_eq!(binding.default_binding, "Ctrl+Q");

    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    let _ = app.update(Message::ExecuteCommand(CommandId::Quit));
    assert_eq!(app.project.tracks()[0].id(), track_id);
    assert_eq!(
        app.pending_project_transition,
        Some(super::PendingProjectTransition::CloseMainWindow(window_id))
    );
    let _ = app.update(Message::CancelProjectTransition);
    assert!(app.is_dirty());
    assert_eq!(app.pending_project_transition, None);

    let _ = app.update(Message::ExecuteCommand(CommandId::Quit));
    let _ = app.update(Message::DiscardProjectChanges);
    assert_eq!(app.pending_project_transition, None);
}

#[test]
fn dirty_main_window_close_offers_save_discard_or_cancel_without_closing() {
    let window_id = iced::window::Id::unique();
    let mut app = App {
        main_window_id: Some(window_id),
        revision: 1,
        ..App::default()
    };

    let _ = app.update(Message::WindowCloseRequested(window_id));

    assert_eq!(
        app.pending_project_transition,
        Some(super::PendingProjectTransition::CloseMainWindow(window_id))
    );
    assert!(app.is_dirty());

    let _ = app.update(Message::CancelProjectTransition);
    assert_eq!(app.pending_project_transition, None);
    assert!(app.is_dirty());
}

#[test]
fn new_project_status_keeps_temporary_storage_failures_visible() {
    assert_eq!(
        super::new_project_status(Some("permission denied")),
        "New project created, but temporary media storage is unavailable: permission denied"
    );
}

#[test]
fn cancelling_open_picker_after_discard_choice_keeps_current_project() {
    let mut app = App {
        revision: 1,
        ..App::default()
    };
    let _ = app.update(Message::OpenProject);
    let _ = app.update(Message::DiscardProjectChanges);
    assert!(app.path_picker_busy);
    assert!(app.is_dirty());

    let _ = app.path_picked(PathPickerTarget::OpenProject, Ok(None));

    assert!(!app.path_picker_busy);
    assert_eq!(app.pending_project_transition, None);
    assert!(app.is_dirty());
}

#[test]
fn cancelling_save_picker_or_failing_save_does_not_continue_close() {
    let window_id = iced::window::Id::unique();
    let mut app = App {
        main_window_id: Some(window_id),
        revision: 1,
        ..App::default()
    };
    let _ = app.update(Message::WindowCloseRequested(window_id));
    let _ = app.update(Message::SaveBeforeProjectTransition);
    assert!(app.path_picker_busy);

    let _ = app.path_picked(PathPickerTarget::SaveProject, Ok(None));
    assert_eq!(
        app.pending_project_transition,
        Some(super::PendingProjectTransition::CloseMainWindow(window_id))
    );
    assert!(app.is_dirty());

    app.io_busy = true;
    let _ = app.update(Message::ProjectSaved(
        std::path::PathBuf::from("failed.aaadaw"),
        1,
        Err("permission denied".to_owned()),
        None,
        SharedProjectSessionLock::new(None),
    ));
    assert_eq!(
        app.pending_project_transition,
        Some(super::PendingProjectTransition::CloseMainWindow(window_id))
    );
    assert!(app.is_dirty());
    assert!(app.main_window_id.is_some());
}

#[test]
fn successful_save_continues_the_pending_transition() {
    let window_id = iced::window::Id::unique();
    let path = std::path::PathBuf::from("saved.aaadaw");
    let mut app = App {
        main_window_id: Some(window_id),
        pending_project_transition: Some(super::PendingProjectTransition::CloseMainWindow(
            window_id,
        )),
        io_busy: true,
        revision: 1,
        ..App::default()
    };

    let _ = app.update(Message::ProjectSaved(
        path,
        1,
        Ok(()),
        None,
        SharedProjectSessionLock::new(None),
    ));

    assert_eq!(app.saved_revision, 1);
    assert!(!app.is_dirty());
    assert_eq!(app.pending_project_transition, None);
}

#[test]
fn successful_save_before_close_project_clears_the_open_project() {
    let mut app = App {
        pending_project_transition: Some(super::PendingProjectTransition::CloseProject),
        io_busy: true,
        revision: 1,
        ..App::default()
    };

    let _ = app.update(Message::ProjectSaved(
        std::path::PathBuf::from("saved.aaadaw"),
        1,
        Ok(()),
        None,
        SharedProjectSessionLock::new(None),
    ));

    assert!(!app.is_dirty());
    assert_eq!(app.pending_project_transition, None);
    assert!(app.project.tracks().is_empty());
    assert!(app.project_path.is_none());
    assert_eq!(app.revision, 0);
    assert_eq!(app.saved_revision, 0);
    assert_eq!(app.status, "Project closed");
}

#[test]
fn successful_save_continues_open_by_picking_a_project() {
    let mut app = App {
        pending_project_transition: Some(super::PendingProjectTransition::OpenProject),
        io_busy: true,
        revision: 1,
        ..App::default()
    };

    let _ = app.update(Message::ProjectSaved(
        std::path::PathBuf::from("saved.aaadaw"),
        1,
        Ok(()),
        None,
        SharedProjectSessionLock::new(None),
    ));

    assert!(!app.is_dirty());
    assert!(app.path_picker_busy);
    assert_eq!(
        app.pending_project_transition,
        Some(super::PendingProjectTransition::OpenProject)
    );
}

#[test]
fn app_entrypoint_has_iced_result_signature() {
    let run: fn() -> iced::Result = crate::app::run;
    let _ = run;
}

#[test]
fn view_builder_has_app_view_signature() {
    let builder: for<'a> fn(&'a App) -> iced::Element<'a, Message> = super::view::view;
    let _ = builder;
}

#[test]
fn playback_diagnostics_distinguish_stream_underruns_from_backend_errors() {
    assert_eq!(
        super::view::playback_diagnostic_suffix("JACK", 0, 0, 0, Some(0), 0, false),
        ""
    );
    assert_eq!(
        super::view::playback_diagnostic_suffix("JACK", 256, 0, 0, Some(0), 0, false),
        " · stream underrun: 256 samples"
    );
    assert_eq!(
        super::view::playback_diagnostic_suffix("PipeWire", 0, 0, 0, None, 2, false),
        " · PipeWire errors: 2"
    );
    assert_eq!(
        super::view::playback_diagnostic_suffix("JACK", 256, 3, 1, Some(2), 2, false),
        " · stream underrun: 256 samples · JACK XRuns: 2 · Master ceiling (total): 3 samples · non-finite Master samples silenced (total): 1 · JACK errors: 2"
    );
    assert_eq!(
        super::view::playback_diagnostic_suffix("JACK", 0, 0, 0, Some(2), 0, false),
        " · JACK XRuns: 2"
    );
    assert_eq!(
        super::view::playback_diagnostic_suffix("PipeWire", 256, 0, 0, None, 2, false),
        " · stream underrun: 256 samples · PipeWire errors: 2"
    );
    assert_eq!(
        super::view::playback_diagnostic_suffix("WASAPI", 0, 0, 0, None, 1, true),
        " · output device unavailable; close playback and reopen it after selecting a default device · WASAPI errors: 1"
    );
}

#[test]
fn audio_asset_maintenance_requires_a_saved_project_snapshot() {
    let mut app = App {
        project_path: Some(std::path::PathBuf::from("project.aaadaw")),
        revision: 1,
        ..App::default()
    };

    let _ = app.update(Message::RunAudioAssetManagement(
        aaadaw_app::AudioAssetManagementOperation::ScanSources,
    ));
    assert!(!app.audio_asset_management_busy);
    assert_eq!(
        app.status,
        "Save the project before scanning or packing audio assets"
    );
}

#[test]
fn menus_and_media_browser_panel_stay_available_during_jobs() {
    let mut app = App {
        audio_asset_management_busy: true,
        ..App::default()
    };
    let _ = app.update(Message::ToggleMainMenu(MainMenu::File));
    assert_eq!(app.active_menu, Some(MainMenu::File));
    let _ = app.update(Message::ExecuteCommand(CommandId::ToggleMediaBrowserPanel));
    assert!(app.media_panel_dock.open);
    assert_eq!(app.active_menu, None);
    let _ = app.update(Message::ToggleMainMenu(MainMenu::File));
    assert_eq!(app.active_menu, Some(MainMenu::File));
    let _ = app.update(Message::ToggleMainMenu(MainMenu::File));
    assert_eq!(app.active_menu, None);
}

#[test]
fn menus_and_context_targets_survive_keyboard_input_and_background_ticks() {
    let window_id = iced::window::Id::unique();
    let mut app = App {
        main_window_id: Some(window_id),
        ..App::default()
    };
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = app.project.midi_items()[0].id();
    let _ = app.update(Message::ToggleMainMenu(MainMenu::Actions));
    app.timeline.context_track = Some(track_id);
    app.timeline.context_item = Some(item_id);
    app.timeline.context_item_position = Some((20.0, 30.0));

    let _ = app.update(Message::ActionQueryChanged(
        "no matching command".to_owned(),
    ));
    assert_eq!(app.active_menu, Some(MainMenu::Actions));
    assert_eq!(app.timeline.context_track, None);
    assert_eq!(app.timeline.context_item, None);
    let _ = app.update(Message::RunActionQuery);
    assert_eq!(app.active_menu, Some(MainMenu::Actions));
    app.timeline.context_track = Some(track_id);
    app.timeline.context_item = Some(item_id);
    app.timeline.context_item_position = Some((20.0, 30.0));
    let _ = app.update(Message::BackgroundTick);
    let _ = app.update(Message::MeterTick);
    let _ = app.update(Message::RuntimeKeyboardEvent(
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: Key::Character("a".into()),
            modified_key: Key::Character("a".into()),
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::KeyA),
            location: iced::keyboard::Location::Standard,
            modifiers: Modifiers::NONE,
            text: Some("a".into()),
            repeat: false,
        }),
        iced::event::Status::Captured,
        window_id,
    ));
    let _ = app.update(Message::ShortcutPressed(test_shortcut_input(
        Key::Character("not-a-bound-shortcut".into()),
        Modifiers::NONE,
    )));
    let _ = app.update(Message::AudioImportFinished(
        Err("import failed".to_owned()),
    ));
    #[cfg(feature = "audio-device")]
    let _ = app.update(Message::RecordingStarted(SharedRecordingStart(
        std::sync::Arc::new(std::sync::Mutex::new(Some(Err(
            "recording failed".to_owned()
        )))),
    )));

    assert_eq!(app.active_menu, Some(MainMenu::Actions));
    assert_eq!(app.timeline.context_track, Some(track_id));
    assert_eq!(app.timeline.context_item, Some(item_id));
    assert_eq!(app.timeline.context_item_position, Some((20.0, 30.0)));

    let _ = app.update(Message::DismissMainMenu);
    assert_eq!(app.active_menu, None);
    assert_eq!(app.timeline.context_track, None);
    assert_eq!(app.timeline.context_item, None);

    let _ = app.update(Message::ToggleMainMenu(MainMenu::Actions));
    let _ = app.update(Message::ActionQueryChanged("add track".to_owned()));
    let _ = app.update(Message::RunActionQuery);
    assert_eq!(app.project.tracks().len(), 2);
    assert_eq!(app.active_menu, None);
}

#[test]
fn full_menu_bar_switches_sections_and_escape_dismisses_it_before_time_selection() {
    let mut app = App::default();
    let menus = [
        MainMenu::File,
        MainMenu::Edit,
        MainMenu::View,
        MainMenu::Insert,
        MainMenu::Item,
        MainMenu::Track,
        MainMenu::Options,
        MainMenu::Actions,
        MainMenu::Help,
    ];
    for menu in menus {
        let _ = app.update(Message::ToggleMainMenu(menu));
        assert_eq!(app.active_menu, Some(menu));
        let _ = app.update(Message::ToggleMainMenu(menu));
        assert_eq!(app.active_menu, None);
    }
    let _ = app.update(Message::ToggleMainMenu(MainMenu::File));
    let _ = app.update(Message::ToggleMainMenu(MainMenu::Edit));
    assert_eq!(app.active_menu, Some(MainMenu::Edit));
    assert!(matches!(
        shortcut_message(
            Key::Named(iced::keyboard::key::Named::Escape),
            Modifiers::NONE,
            &HashMap::new(),
        ),
        Some(Message::Escape)
    ));
    let _ = app.update(Message::Escape);
    assert_eq!(app.active_menu, None);

    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.timeline.selected_track = Some(track_id);
    let _ = app.update(Message::Timeline(
        crate::timeline::TimelineEvent::SetTimeSelection {
            start_tick: 240,
            end_tick: 960,
        },
    ));
    assert_eq!(
        app.timeline.loop_points,
        Some(crate::timeline::LoopPoints {
            start_tick: 240,
            end_tick: 960,
        })
    );
    app.timeline.edit_cursor_tick = 720;
    let _ = app.update(Message::ToggleMainMenu(MainMenu::Item));
    let _ = app.update(Message::Escape);
    assert_eq!(app.active_menu, None);
    assert!(app.timeline.time_selection.is_some());

    let _ = app.update(Message::Escape);
    assert_eq!(app.timeline.time_selection, None);
    assert_eq!(app.timeline.loop_points, None);
    assert_eq!(app.timeline.edit_cursor_tick, 720);
    assert_eq!(app.timeline.selected_track, Some(track_id));
}

#[test]
fn escape_dismisses_the_arrangement_automation_point_menu() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.timeline.context_automation_point =
        Some(crate::timeline::AutomationPointContext::Volume { track_id, index: 0 });
    app.timeline.context_automation_position = Some((40.0, 70.0));

    let _ = app.update(Message::Escape);

    assert_eq!(app.timeline.context_automation_point, None);
    assert_eq!(app.timeline.context_automation_position, None);

    app.timeline.context_automation_point =
        Some(crate::timeline::AutomationPointContext::Volume { track_id, index: 0 });
    app.timeline.context_automation_position = Some((40.0, 70.0));
    let _ = app.update(Message::Timeline(
        crate::timeline::TimelineEvent::SelectEmpty(240),
    ));
    assert_eq!(app.timeline.context_automation_point, None);
    assert_eq!(app.timeline.context_automation_position, None);
}

#[test]
fn loading_a_project_clears_the_previous_time_selection() {
    let mut app = App::default();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("loaded.aaadaw");
    let lock = ProjectSessionLock::acquire(&path).unwrap();
    let _ = app.update(Message::Timeline(
        crate::timeline::TimelineEvent::SetTimeSelection {
            start_tick: 240,
            end_tick: 960,
        },
    ));

    let _ = app.update(Message::ProjectLoaded(
        path,
        std::sync::Arc::new(std::sync::Mutex::new(Some(Ok((
            Project::new(),
            None,
            lock,
        ))))),
    ));

    assert_eq!(app.timeline.time_selection, None);
    assert_eq!(app.timeline.loop_points, None);
    assert!(app.timeline.loop_points_linked);
}

#[test]
fn fx_chain_reordering_moves_plugin_state_and_automation_as_one_undoable_edit() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    let selected_plugin = TrackFxPlugin::new("vendor.comp", "/plugins/comp.clap")
        .unwrap()
        .with_enabled(false)
        .with_state(Some(vec![1, 2, 3]));
    app.apply_action(
        DawAction::SetTrackFxChain {
            track_id,
            plugins: vec![
                TrackFxPlugin::new("vendor.eq", "/plugins/eq.clap").unwrap(),
                selected_plugin.clone(),
                TrackFxPlugin::new("vendor.limit", "/plugins/limit.clap").unwrap(),
            ],
        },
        "FX chain configured",
    );
    app.apply_action(
        DawAction::SetTrackFxParameterAutomation {
            track_id,
            chain_index: 1,
            parameter_id: 7,
            points: vec![
                aaadaw_core::FxParameterAutomationPoint::new(100, 0.25).unwrap(),
                aaadaw_core::FxParameterAutomationPoint::new(200, 0.75).unwrap(),
            ],
        },
        "Automation configured",
    );
    app.fx_chain_track_id = Some(track_id);
    app.fx_chain_selected_index = Some(1);

    #[cfg(feature = "audio-device")]
    {
        app.playback_graph_dirty = false;
    }
    let _ = app.update(Message::ReorderFxChainPlugin { from: 1, to: 2 });
    #[cfg(feature = "audio-device")]
    assert!(app.playback_graph_dirty);

    let chain = app.project.tracks()[0].fx_chain();
    assert_eq!(chain[0].plugin_id(), "vendor.eq");
    assert_eq!(chain[1].plugin_id(), "vendor.limit");
    assert_eq!(chain[2].plugin_id(), "vendor.comp");
    let selected_instance_id = selected_plugin.instance_id();
    assert_eq!(chain[2].state(), Some(&[1, 2, 3][..]));
    assert!(!chain[2].is_enabled());
    assert_eq!(
        chain[2].parameter_automation_for(7).unwrap().points().len(),
        2
    );
    assert_eq!(app.fx_chain_selected_index, Some(2));
    assert_eq!(
        app.selected_fx_chain_instance_id(),
        Some(selected_instance_id)
    );

    let _ = app.update(Message::Undo);
    let chain = app.project.tracks()[0].fx_chain();
    assert_eq!(
        chain
            .iter()
            .map(TrackFxPlugin::plugin_id)
            .collect::<Vec<_>>(),
        ["vendor.eq", "vendor.comp", "vendor.limit"]
    );
    assert_eq!(chain[1].state(), Some(&[1, 2, 3][..]));
    assert_eq!(
        chain[1].parameter_automation_for(7).unwrap().points().len(),
        2
    );
    assert_eq!(app.fx_chain_selected_index, Some(1));
    assert_eq!(
        app.selected_fx_chain_instance_id(),
        Some(selected_instance_id)
    );

    let _ = app.update(Message::Redo);
    let chain = app.project.tracks()[0].fx_chain();
    assert_eq!(
        chain
            .iter()
            .map(TrackFxPlugin::plugin_id)
            .collect::<Vec<_>>(),
        ["vendor.eq", "vendor.limit", "vendor.comp"]
    );
    assert_eq!(chain[2].state(), Some(&[1, 2, 3][..]));
    assert_eq!(
        chain[2].parameter_automation_for(7).unwrap().points().len(),
        2
    );
    assert_eq!(app.fx_chain_selected_index, Some(2));
    assert_eq!(
        app.selected_fx_chain_instance_id(),
        Some(selected_instance_id)
    );
}

#[test]
fn fx_chain_drag_reorder_uses_the_same_undoable_move_action() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.apply_action(
        DawAction::SetTrackFxChain {
            track_id,
            plugins: vec![
                TrackFxPlugin::new("vendor.eq", "/plugins/eq.clap").unwrap(),
                TrackFxPlugin::new("vendor.comp", "/plugins/comp.clap").unwrap(),
            ],
        },
        "FX chain configured",
    );
    app.fx_chain_track_id = Some(track_id);
    app.fx_chain_selected_index = Some(0);

    let _ = app.update(Message::BeginFxChainPluginDrag(0));
    let _ = app.update(Message::HoverFxChainPluginDragTarget(1));
    let _ = app.update(Message::FinishFxChainPluginDrag);

    assert_eq!(
        app.project.tracks()[0]
            .fx_chain()
            .iter()
            .map(TrackFxPlugin::plugin_id)
            .collect::<Vec<_>>(),
        ["vendor.comp", "vendor.eq"]
    );
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].plugin_id(),
        "vendor.eq"
    );
}

#[test]
fn fx_chain_drag_release_outside_a_handle_cancels_and_clears_the_source() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.apply_action(
        DawAction::SetTrackFxChain {
            track_id,
            plugins: vec![
                TrackFxPlugin::new("vendor.eq", "/plugins/eq.clap").unwrap(),
                TrackFxPlugin::new("vendor.comp", "/plugins/comp.clap").unwrap(),
            ],
        },
        "FX chain configured",
    );
    app.fx_chain_track_id = Some(track_id);

    let _ = app.update(Message::BeginFxChainPluginDrag(0));
    let _ = app.update(Message::LeaveFxChainPluginDragTarget(0));
    let _ = app.update(Message::FinishFxChainPluginDrag);

    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].plugin_id(),
        "vendor.eq"
    );
    assert_eq!(app.fx_chain_drag_index, None);
    assert_eq!(app.fx_chain_drag_target_index, None);

    let _ = app.update(Message::BeginFxChainPluginDrag(1));
    let _ = app.update(Message::HoverFxChainPluginDragTarget(0));
    let _ = app.update(Message::FinishFxChainPluginDrag);
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].plugin_id(),
        "vendor.comp"
    );
}

#[test]
fn fx_chain_reordering_waits_for_pending_parameter_updates() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.apply_action(
        DawAction::SetTrackFxChain {
            track_id,
            plugins: vec![
                TrackFxPlugin::new("vendor.eq", "/plugins/eq.clap").unwrap(),
                TrackFxPlugin::new("vendor.comp", "/plugins/comp.clap").unwrap(),
            ],
        },
        "FX chain configured",
    );
    app.fx_chain_track_id = Some(track_id);
    app.fx_chain_selected_index = Some(1);
    app.pending_fx_parameter_sync = Some(aaadaw_core::FxParameterChange {
        track_id,
        chain_index: 1,
        parameter_id: 7,
        value: 0.5,
    });

    let _ = app.update(Message::ReorderFxChainPlugin { from: 1, to: 0 });

    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].plugin_id(),
        "vendor.eq"
    );
    assert_eq!(
        app.project.tracks()[0].fx_chain()[1].plugin_id(),
        "vendor.comp"
    );
    assert!(app.pending_fx_parameter_sync.is_some());
    assert!(app.status.contains("parameter update to finish"));
}

#[cfg(feature = "audio-device")]
#[test]
fn fx_chain_reordering_waits_for_an_active_automation_take() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.apply_action(
        DawAction::SetTrackFxChain {
            track_id,
            plugins: vec![
                TrackFxPlugin::new("vendor.eq", "/plugins/eq.clap").unwrap(),
                TrackFxPlugin::new("vendor.comp", "/plugins/comp.clap").unwrap(),
            ],
        },
        "FX chain configured",
    );
    app.fx_chain_track_id = Some(track_id);
    app.fx_chain_selected_index = Some(1);
    app.fx_automation_write_target = Some((track_id, 1, 7));

    let _ = app.update(Message::ReorderFxChainPlugin { from: 1, to: 0 });

    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].plugin_id(),
        "vendor.eq"
    );
    assert_eq!(
        app.project.tracks()[0].fx_chain()[1].plugin_id(),
        "vendor.comp"
    );
    assert_eq!(app.fx_automation_write_target, Some((track_id, 1, 7)));
    assert!(app.status.contains("automation take"));
}

#[test]
fn fx_chain_reordering_rejects_invalid_positions_and_tracks_shifted_selection() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.apply_action(
        DawAction::SetTrackFxChain {
            track_id,
            plugins: ["eq", "comp", "limit"]
                .into_iter()
                .map(|name| TrackFxPlugin::new(name, format!("/plugins/{name}.clap")).unwrap())
                .collect(),
        },
        "FX chain configured",
    );
    app.fx_chain_track_id = Some(track_id);
    app.fx_chain_selected_index = Some(1);
    let revision_before = app.revision;

    let _ = app.update(Message::ReorderFxChainPlugin {
        from: 0,
        to: usize::MAX,
    });
    assert_eq!(app.revision, revision_before);
    assert_eq!(app.fx_chain_selected_index, Some(1));

    let _ = app.update(Message::ReorderFxChainPlugin { from: 2, to: 0 });
    assert_eq!(app.project.tracks()[0].fx_chain()[0].plugin_id(), "limit");
    assert_eq!(app.project.tracks()[0].fx_chain()[1].plugin_id(), "eq");
    assert_eq!(app.project.tracks()[0].fx_chain()[2].plugin_id(), "comp");
    assert_eq!(app.fx_chain_selected_index, Some(2));
}

#[test]
fn automation_lane_view_changes_mark_the_project_dirty_for_saving() {
    let mut app = App::default();
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "FX".to_owned(),
        })
        .unwrap();
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::SetTrackFxChain {
            track_id,
            plugins: vec![TrackFxPlugin::new("vendor.eq", "/plugins/eq.clap").unwrap()],
        })
        .unwrap();
    app.timeline.rebuild(&app.project);

    app.fx_chain_track_id = Some(track_id);
    app.fx_chain_selected_index = Some(0);
    let _ = app.update(Message::FxAutomationLaneToggled {
        parameter_id: 7,
        name: "Mix".to_owned(),
        min_value: 0.0,
        max_value: 1.0,
        stepped: false,
    });
    assert!(app.is_dirty());

    let _ = app.update(Message::Timeline(
        crate::timeline::TimelineEvent::ResizeFxAutomationLane {
            track_id,
            chain_index: 0,
            parameter_id: 7,
            height: 48.0,
        },
    ));
    assert_eq!(
        app.timeline.arrangement_view_state(&app.project).fx_lanes[0].height,
        48.0
    );
    assert_eq!(app.revision, 2);
}

#[test]
fn cancelled_picker_preserves_path_and_clears_busy_state() {
    let mut app = App {
        path_picker_busy: true,
        audio_file_path_query: "previous.wav".to_owned(),
        revision: 4,
        ..App::default()
    };

    let _ = app.path_picked(PathPickerTarget::ImportAudio, Ok(None));

    assert_eq!(app.audio_file_path_query, "previous.wav");
    assert_eq!(app.revision, 4);
    assert!(!app.path_picker_busy);
}

#[test]
fn cancelling_save_dialog_clears_the_wait_for_dialog_status() {
    for trigger in [
        Message::SaveProject,
        Message::PickPath(PathPickerTarget::SaveProject),
    ] {
        let temporary_dir = tempfile::tempdir().unwrap();
        let session_media_path = temporary_dir.path().join("session.aaadaw");
        std::fs::write(&session_media_path, b"").unwrap();
        let session_media_dir = super::UnsavedSessionMedia {
            directory: temporary_dir.keep(),
            lock: Some(super::ProjectSessionLock::acquire(&session_media_path).unwrap()),
        };
        let mut app = App {
            session_media_dir: Some(session_media_dir),
            ..App::default()
        };
        assert_eq!(app.media_store_path(), Some(session_media_path.clone()));
        let _ = app.update(trigger);
        assert!(app.path_picker_busy);

        let _ = app.update(Message::SaveProject);
        assert_eq!(app.status, "Wait for the file dialog to finish");

        let _ = app.update(Message::PathPicked(PathPickerTarget::SaveProject, Ok(None)));
        assert!(!app.path_picker_busy);
        assert_eq!(app.status, "");
        assert_eq!(app.media_store_path(), Some(session_media_path.clone()));
        assert!(session_media_path.is_file());
    }
}

#[test]
fn failed_picker_reports_error_and_clears_busy_state() {
    let mut app = App {
        path_picker_busy: true,
        audio_file_path_query: "previous.wav".to_owned(),
        revision: 4,
        ..App::default()
    };

    let _ = app.path_picked(
        PathPickerTarget::ImportAudio,
        Err("dialog unavailable".to_owned()),
    );

    assert_eq!(app.audio_file_path_query, "previous.wav");
    assert_eq!(app.revision, 4);
    assert!(!app.path_picker_busy);
    assert_eq!(app.status, "File dialog failed: dialog unavailable");
}

#[test]
fn native_picker_results_fill_the_requested_path_fields() {
    let mut app = App {
        path_picker_busy: true,
        ..App::default()
    };
    let audio_path = std::path::PathBuf::from("/tmp/example.wav");
    let _ = app.path_picked(PathPickerTarget::ImportAudio, Ok(Some(audio_path.clone())));
    assert_eq!(app.audio_file_path_query, audio_path.to_string_lossy());
    assert!(!app.path_picker_busy);

    app.path_picker_busy = true;
    let replacement_path = std::path::PathBuf::from("/tmp/replacement.wav");
    let _ = app.path_picked(
        PathPickerTarget::RelinkAudio,
        Ok(Some(replacement_path.clone())),
    );
    assert_eq!(
        app.relink_source_path_query,
        replacement_path.to_string_lossy()
    );
    assert!(!app.path_picker_busy);
}

#[test]
fn saving_waits_for_an_unsaved_session_recovery_snapshot() {
    let mut app = App {
        project_path: Some(PathBuf::from("project.aaadaw")),
        unsaved_session_snapshot_busy: true,
        ..App::default()
    };

    let _ = app.update(Message::SaveProject);

    assert!(!app.io_busy);
    assert_eq!(
        app.status,
        "Wait for the unsaved session recovery snapshot to finish"
    );
}

#[test]
fn insert_menu_audio_picker_runs_the_import_path_and_keeps_save_guard() {
    let mut app = App {
        path_picker_busy: true,
        ..App::default()
    };
    let audio_path = std::path::PathBuf::from("/tmp/menu-audio.wav");

    let _ = app.path_picked(
        PathPickerTarget::ImportAudioToProject,
        Ok(Some(audio_path.clone())),
    );

    assert_eq!(app.audio_file_path_query, audio_path.to_string_lossy());
    assert!(!app.path_picker_busy);
    assert!(!app.import_busy);
    assert_eq!(app.status, "Temporary project media storage is unavailable");
}

#[test]
fn keyboard_shortcuts_and_menu_hints_share_command_definitions() {
    assert!(matches!(
        shortcut_message(Key::Character("z"), Modifiers::COMMAND, &HashMap::new()),
        Some(Message::ExecuteCommand(CommandId::Undo))
    ));
    assert!(matches!(
        shortcut_message(
            Key::Character("z"),
            Modifiers::COMMAND | Modifiers::SHIFT,
            &HashMap::new(),
        ),
        Some(Message::ExecuteCommand(CommandId::Redo))
    ));
    assert!(matches!(
        shortcut_message(Key::Character("s"), Modifiers::COMMAND, &HashMap::new()),
        Some(Message::ExecuteCommand(CommandId::SaveProject))
    ));
    assert!(matches!(
        shortcut_message(Key::Character("o"), Modifiers::COMMAND, &HashMap::new()),
        Some(Message::ExecuteCommand(CommandId::OpenProject))
    ));
    assert!(matches!(
        shortcut_message(Key::Character("d"), Modifiers::COMMAND, &HashMap::new()),
        Some(Message::ExecuteCommand(CommandId::DuplicateSelectedItem))
    ));
    assert!(matches!(
        shortcut_message(
            Key::Named(iced::keyboard::key::Named::Delete),
            Modifiers::NONE,
            &HashMap::new()
        ),
        Some(Message::ExecuteCommand(CommandId::DeleteByFocus))
    ));
    assert!(
        shortcut_message(
            Key::Named(iced::keyboard::key::Named::Backspace),
            Modifiers::NONE,
            &HashMap::new()
        )
        .is_none()
    );
    assert!(matches!(
        shortcut_message(Key::Character("s"), Modifiers::NONE, &HashMap::new()),
        Some(Message::ExecuteCommand(
            CommandId::SplitSelectedItemsAtCursor
        ))
    ));
    assert!(shortcut_message(Key::Character("x"), Modifiers::COMMAND, &HashMap::new()).is_none());
    let undo_event = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: Key::Character("z".into()),
        modified_key: Key::Character("z".into()),
        physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::KeyZ),
        location: iced::keyboard::Location::Standard,
        modifiers: Modifiers::COMMAND,
        text: None,
        repeat: false,
    });
    let main_window_id = iced::window::Id::unique();
    assert!(
        keyboard_shortcut_event(
            undo_event.clone(),
            iced::event::Status::Captured,
            main_window_id,
            Some(main_window_id),
            None,
            None,
        )
        .is_none()
    );
    assert!(matches!(
        keyboard_shortcut_event(
            undo_event,
            iced::event::Status::Ignored,
            main_window_id,
            Some(main_window_id),
            None,
            None,
        ),
        Some(Message::ShortcutPressed(input))
            if input.logical_key == Key::Character("z".into()) && input.modifiers == Modifiers::COMMAND
    ));
    let app = App::default();
    let undo = commands::for_menu(&app, MainMenu::Edit)
        .into_iter()
        .find(|entry| entry.id == CommandId::Undo)
        .expect("Edit menu should expose Undo");
    let redo = commands::for_menu(&app, MainMenu::Edit)
        .into_iter()
        .find(|entry| entry.id == CommandId::Redo)
        .expect("Edit menu should expose Redo");
    assert_eq!(
        undo.shortcut.as_deref(),
        Some(commands::format_shortcut_label("Mod+Z").as_str())
    );
    assert_eq!(
        redo.shortcut.as_deref(),
        Some(commands::format_shortcut_label("Mod+Shift+Z; Mod+Y").as_str())
    );
    let escape_event = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: Key::Named(iced::keyboard::key::Named::Escape),
        modified_key: Key::Named(iced::keyboard::key::Named::Escape),
        physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Escape),
        location: iced::keyboard::Location::Standard,
        modifiers: Modifiers::NONE,
        text: None,
        repeat: false,
    });
    assert!(matches!(
        keyboard_shortcut_event(
            escape_event,
            iced::event::Status::Captured,
            main_window_id,
            Some(main_window_id),
            None,
            None,
        ),
        Some(Message::Escape)
    ));
    #[cfg(feature = "audio-device")]
    assert!(matches!(
        shortcut_message(
            Key::Named(iced::keyboard::key::Named::Space),
            Modifiers::NONE,
            &HashMap::new(),
        ),
        Some(Message::ExecuteCommand(CommandId::TogglePlayStop))
    ));
    #[cfg(not(feature = "audio-device"))]
    assert!(
        shortcut_message(
            Key::Named(iced::keyboard::key::Named::Space),
            Modifiers::NONE,
            &HashMap::new(),
        )
        .is_none()
    );
}

#[test]
fn midi_editor_shortcuts_use_configured_commands_only_when_unconsumed() {
    let midi_editor_window_id = iced::window::Id::unique();
    let other_window_id = iced::window::Id::unique();
    let event = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: Key::Character("z".into()),
        modified_key: Key::Character("z".into()),
        physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::KeyZ),
        location: iced::keyboard::Location::Standard,
        modifiers: Modifiers::COMMAND,
        text: None,
        repeat: false,
    });

    assert!(matches!(
        midi_editor_shortcut_event(
            event.clone(),
            iced::event::Status::Ignored,
            midi_editor_window_id,
            Some(midi_editor_window_id),
        ),
        Some(Message::ShortcutPressed(input))
            if input.logical_key == Key::Character("z".into()) && input.modifiers == Modifiers::COMMAND
    ));
    assert!(
        midi_editor_shortcut_event(
            event.clone(),
            iced::event::Status::Captured,
            midi_editor_window_id,
            Some(midi_editor_window_id),
        )
        .is_none()
    );
    assert!(
        midi_editor_shortcut_event(
            event,
            iced::event::Status::Ignored,
            other_window_id,
            Some(midi_editor_window_id),
        )
        .is_none()
    );
}

#[cfg(feature = "audio-device")]
#[test]
fn midi_editor_stop_shortcut_reaches_the_transport_command() {
    let midi_editor_window_id = iced::window::Id::unique();
    let event = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: Key::Named(iced::keyboard::key::Named::Space),
        modified_key: Key::Named(iced::keyboard::key::Named::Space),
        physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Space),
        location: iced::keyboard::Location::Standard,
        modifiers: Modifiers::SHIFT,
        text: None,
        repeat: false,
    });

    let shortcut = midi_editor_shortcut_event(
        event.clone(),
        iced::event::Status::Ignored,
        midi_editor_window_id,
        Some(midi_editor_window_id),
    )
    .expect("unconsumed Shift+Space from the MIDI window should reach shortcuts");
    assert!(matches!(
        &shortcut,
        Message::ShortcutPressed(input)
            if input.logical_key == Key::Named(iced::keyboard::key::Named::Space) && input.modifiers == Modifiers::SHIFT
    ));
    let mut app = App::default();
    *app.shortcut_bindings.write().unwrap() = commands::validate_bindings(&HashMap::from([(
        "transport.stop-playback".into(),
        "Shift+Space".into(),
    )]))
    .unwrap();
    let _ = app.update(shortcut);
    assert!(app.status.contains("output is not open"));
    assert!(
        midi_editor_shortcut_event(
            event,
            iced::event::Status::Captured,
            midi_editor_window_id,
            Some(midi_editor_window_id),
        )
        .is_none()
    );
}

#[test]
fn midi_editor_escape_is_left_for_the_editor_gesture_to_handle() {
    let window_id = iced::window::Id::unique();
    let event = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: Key::Named(iced::keyboard::key::Named::Escape),
        modified_key: Key::Named(iced::keyboard::key::Named::Escape),
        physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Escape),
        location: iced::keyboard::Location::Standard,
        modifiers: Modifiers::NONE,
        text: None,
        repeat: false,
    });
    assert!(
        midi_editor_shortcut_event(
            event,
            iced::event::Status::Captured,
            window_id,
            Some(window_id),
        )
        .is_none()
    );
}

#[test]
fn midi_editor_feedback_stays_local_to_that_window() {
    let mut app = App {
        status: "Main window status".to_owned(),
        ..App::default()
    };
    let _ = app.update(Message::MidiEditorFeedback("Resize rejected".to_owned()));
    assert_eq!(app.status, "Main window status");
    assert_eq!(app.midi_editor_feedback.as_deref(), Some("Resize rejected"));
}

#[test]
fn wav_render_command_is_discoverable_queueable_and_cancellable_from_the_file_menu() {
    let mut app = App {
        project_path: Some(std::path::PathBuf::from("session.aaadaw")),
        ..App::default()
    };
    let export = commands::for_menu(&app, MainMenu::File)
        .into_iter()
        .find(|entry| entry.id == CommandId::ExportWav)
        .expect("File menu should expose WAV rendering");
    assert!(export.enabled);
    assert_eq!(export.label, "Render...");
    assert_eq!(export.shortcut.as_deref(), Some("Ctrl+Alt+R"));

    let action = app
        .action_list_entries()
        .into_iter()
        .find(|entry| entry.id == CommandId::ExportWav)
        .expect("Action List should expose native Render project to disk action");
    assert_eq!(action.category, "File");
    assert_eq!(action.label, "Render project to disk...");
    assert_eq!(action.shortcut.as_deref(), Some("Ctrl+Alt+R"));
    let binding = commands::shortcut_entries(&app)
        .into_iter()
        .find(|entry| entry.id == "audio.export-wav")
        .expect("Render action should have a configurable shortcut entry");
    assert_eq!(binding.default_binding, "Ctrl+Alt+R");
    let bindings = app
        .shortcut_bindings
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(
        commands::from_shortcut(
            &Key::Character("r"),
            Modifiers::CTRL.union(Modifiers::ALT),
            &bindings,
            &app.action_macros,
        ),
        Some(CommandId::ExportWav)
    );

    app.offline_render_busy = true;
    let file_commands = commands::for_menu(&app, MainMenu::File);
    assert!(
        file_commands
            .iter()
            .find(|entry| entry.id == CommandId::ExportWav)
            .is_some_and(|entry| entry.enabled)
    );
    assert!(
        file_commands
            .iter()
            .find(|entry| entry.id == CommandId::CancelOfflineRender)
            .is_some_and(|entry| entry.enabled)
    );

    #[cfg(feature = "audio-device")]
    {
        app.offline_render_busy = false;
        app.recording_starting = true;
        assert!(
            commands::for_menu(&app, MainMenu::File)
                .into_iter()
                .find(|entry| entry.id == CommandId::ExportWav)
                .is_some_and(|entry| !entry.enabled)
        );
    }
}

#[test]
fn render_command_opens_a_reusable_render_window() {
    let mut app = App {
        project_path: Some(std::path::PathBuf::from("session.aaadaw")),
        ..App::default()
    };

    let _ = app.update(Message::ExecuteCommand(CommandId::ExportWav));
    let first_window = app
        .render_window_id
        .expect("render command should open the render window");
    assert_eq!(app.window_title(first_window), "Render project to WAV");

    let _ = app.update(Message::ExecuteCommand(CommandId::ExportWav));
    assert_eq!(app.render_window_id, Some(first_window));

    let _ = app.update(Message::WindowClosed(first_window));
    assert_eq!(app.render_window_id, None);
}

#[test]
fn main_window_title_tracks_project_name_and_unsaved_changes() {
    let main_window_id = iced::window::Id::unique();
    let mut app = App {
        main_window_id: Some(main_window_id),
        ..App::default()
    };

    assert_eq!(app.window_title(main_window_id), "New project — AAADAW");

    let _ = app.update(Message::AddTrack);
    assert_eq!(app.window_title(main_window_id), "New project  * — AAADAW");

    app.project_path = Some(PathBuf::from("/projects/session.aaadaw"));
    app.saved_revision = app.revision;
    assert_eq!(app.window_title(main_window_id), "session.aaadaw — AAADAW");

    let _ = app.update(Message::AddTrack);
    assert_eq!(
        app.window_title(main_window_id),
        "session.aaadaw  * — AAADAW"
    );
}

#[test]
fn wav_export_options_default_to_pcm24_and_dither_only_applies_to_integer_formats() {
    let mut app = App::default();
    assert_eq!(
        app.wav_export_options,
        aaadaw_app::WavExportOptions::default()
    );
    assert_eq!(
        app.wav_export_options.sample_format,
        aaadaw_app::WavSampleFormat::Pcm24
    );
    assert!(!app.wav_export_options.dither);

    let _ = app.update(Message::SetWavDither(true));
    assert!(app.wav_export_options.dither);
    let _ = app.update(Message::SetWavSampleFormat(
        aaadaw_app::WavSampleFormat::Float32,
    ));
    assert_eq!(
        app.wav_export_options.sample_format,
        aaadaw_app::WavSampleFormat::Float32
    );
    assert!(!app.wav_export_options.dither);

    let _ = app.update(Message::SetWavDither(true));
    assert!(!app.wav_export_options.dither);
}

#[test]
fn shortcut_capture_formats_keys_and_supports_clear_and_cancel() {
    assert_eq!(
        commands::capture_binding("x", Modifiers::COMMAND).unwrap(),
        "Mod+X"
    );
    assert_eq!(
        commands::capture_binding("z", Modifiers::COMMAND | Modifiers::SHIFT).unwrap(),
        "Mod+Shift+Z"
    );
    assert_eq!(
        commands::capture_binding("Space", Modifiers::NONE).unwrap(),
        "Space"
    );
    assert_eq!(
        commands::capture_binding("s", Modifiers::NONE).unwrap(),
        "S"
    );
    assert_eq!(
        commands::capture_binding("Delete", Modifiers::NONE).unwrap(),
        "Delete"
    );

    let main_window_id = iced::window::Id::unique();
    let settings_window_id = iced::window::Id::unique();
    let make_key_event = |key: Key, modifiers| {
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::KeyK),
            location: iced::keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        })
    };
    assert!(matches!(
        keyboard_shortcut_event(
            make_key_event(Key::Character("k".into()), Modifiers::COMMAND),
            iced::event::Status::Captured,
            settings_window_id,
            Some(main_window_id),
            Some(settings_window_id),
            Some("edit.undo"),
        ),
        Some(Message::ShortcutCaptureKey { action_id, key, modifiers })
            if action_id == "edit.undo" && key == "K" && modifiers == Modifiers::COMMAND
    ));
    assert!(matches!(
        keyboard_shortcut_event(
            make_key_event(
                Key::Named(iced::keyboard::key::Named::Backspace),
                Modifiers::NONE,
            ),
            iced::event::Status::Captured,
            settings_window_id,
            Some(main_window_id),
            Some(settings_window_id),
            Some("edit.undo"),
        ),
        Some(Message::ClearShortcutBinding(action_id)) if action_id == "edit.undo"
    ));
    assert!(matches!(
        keyboard_shortcut_event(
            make_key_event(
                Key::Named(iced::keyboard::key::Named::Delete),
                Modifiers::NONE,
            ),
            iced::event::Status::Captured,
            settings_window_id,
            Some(main_window_id),
            Some(settings_window_id),
            Some("edit.undo"),
        ),
        Some(Message::ShortcutCaptureKey { action_id, key, modifiers })
            if action_id == "edit.undo" && key == "StandardDelete" && modifiers == Modifiers::NONE
    ));
    assert!(matches!(
        keyboard_shortcut_event(
            make_key_event(
                Key::Named(iced::keyboard::key::Named::Escape),
                Modifiers::NONE
            ),
            iced::event::Status::Captured,
            settings_window_id,
            Some(main_window_id),
            Some(settings_window_id),
            Some("edit.undo"),
        ),
        Some(Message::CancelShortcutCapture)
    ));
    assert!(
        keyboard_shortcut_event(
            make_key_event(Key::Character("k".into()), Modifiers::COMMAND),
            iced::event::Status::Ignored,
            main_window_id,
            Some(main_window_id),
            Some(settings_window_id),
            Some("edit.undo"),
        )
        .is_some()
    );
    assert!(
        keyboard_shortcut_event(
            make_key_event(
                Key::Named(iced::keyboard::key::Named::Delete),
                Modifiers::NONE,
            ),
            iced::event::Status::Captured,
            main_window_id,
            Some(main_window_id),
            Some(settings_window_id),
            None,
        )
        .is_none()
    );
}

#[test]
fn settings_menu_opens_one_window_and_accepts_duplicate_shortcut_bindings() {
    let mut app = App::default();
    assert!(
        commands::for_menu(&app, MainMenu::Options)
            .iter()
            .any(|entry| entry.id == CommandId::OpenSettings)
    );
    assert!(
        commands::for_menu(&app, MainMenu::File)
            .iter()
            .all(|entry| entry.id != CommandId::OpenSettings)
    );
    assert_eq!(
        commands::find(&app, "settings"),
        Some(CommandId::OpenSettings)
    );

    let _ = app.update(Message::ExecuteCommand(CommandId::OpenSettings));
    let settings_window_id = app.settings_window_id;
    assert!(settings_window_id.is_some());
    let _ = app.update(Message::ExecuteCommand(CommandId::OpenSettings));
    assert_eq!(app.settings_window_id, settings_window_id);

    let _ = app.update(Message::StartShortcutCapture("edit.undo".to_owned()));
    let _ = app.update(Message::ShortcutCaptureKey {
        action_id: "edit.undo".to_owned(),
        key: "s".to_owned(),
        modifiers: Modifiers::COMMAND,
    });
    assert_eq!(app.shortcut_capture_id, None);
    assert_eq!(
        app.shortcut_binding_edits
            .get("edit.undo")
            .map(String::as_str),
        Some("Mod+S")
    );
    assert!(
        app.shortcut_editor_feedback
            .contains(&commands::format_shortcut_label("Mod+S"))
    );
    let validated = commands::validate_bindings(&app.shortcut_binding_edits).unwrap();
    assert_eq!(
        commands::from_shortcut(
            &Key::Character("s"),
            Modifiers::COMMAND,
            &validated,
            &app.action_macros,
        ),
        Some(CommandId::Undo)
    );

    let _ = app.update(Message::ClearShortcutBinding("edit.undo".to_owned()));
    assert!(app.shortcut_capture_id.is_none());
    assert_eq!(
        app.shortcut_binding_edits
            .get("edit.undo")
            .map(String::as_str),
        Some("")
    );
    assert_eq!(
        commands::shortcut_entries(&app)
            .into_iter()
            .find(|entry| entry.id == "edit.undo")
            .unwrap()
            .binding,
        ""
    );
}

#[test]
fn options_menu_exposes_preferences_and_updates_the_loop_link_checkmark() {
    let mut app = App::default();
    let options = commands::for_menu(&app, MainMenu::Options);
    let preferences = options
        .iter()
        .find(|entry| entry.id == CommandId::OpenSettings)
        .expect("Preferences should be available from Options");
    assert_eq!(preferences.label, "Preferences...");
    let expected_shortcut = commands::format_shortcut_label("Mod+P");
    assert_eq!(
        preferences.shortcut.as_deref(),
        Some(expected_shortcut.as_str())
    );
    assert!(matches!(
        shortcut_message(Key::Character("p"), Modifiers::COMMAND, &HashMap::new()),
        Some(Message::ExecuteCommand(CommandId::OpenSettings))
    ));

    let link = CommandId::LoopTime(commands::LoopTimeCommand::ToggleLinked);
    let link_entry = options
        .iter()
        .find(|entry| entry.id == link)
        .expect("loop link setting should be available from Options");
    assert_eq!(link_entry.label, "Loop points linked to time selection");
    assert!(link_entry.checked);

    let revision = app.revision;
    let _ = app.update(Message::ExecuteCommand(link));
    let link_entry = commands::for_menu(&app, MainMenu::Options)
        .into_iter()
        .find(|entry| entry.id == link)
        .expect("loop link setting should remain available");
    assert!(!link_entry.checked);
    assert!(!app.timeline.loop_points_linked);
    assert!(app.loop_time_preferences_save_at.is_some());
    assert_eq!(app.revision, revision);
}

#[test]
fn help_menu_opens_the_action_list_and_a_single_about_window() {
    let mut app = App::default();
    let help = commands::for_menu(&app, MainMenu::Help);
    assert_eq!(help.len(), 2);
    assert_eq!(help[0].id, CommandId::OpenActionList);
    assert_eq!(help[0].label, "Action list and shortcuts...");
    assert_eq!(help[1].id, CommandId::OpenAbout);
    assert_eq!(help[1].label, "About AAADAW...");
    assert_eq!(help[1].shortcut.as_deref(), Some("Ctrl+F1"));
    assert!(matches!(
        shortcut_message(
            Key::Named(iced::keyboard::key::Named::F1),
            Modifiers::CTRL,
            &HashMap::new()
        ),
        Some(Message::ExecuteCommand(CommandId::OpenAbout))
    ));

    let _ = app.update(Message::ExecuteCommand(CommandId::OpenAbout));
    let about_window_id = app.about_window_id;
    assert!(about_window_id.is_some());
    let _ = app.update(Message::ExecuteCommand(CommandId::OpenAbout));
    assert_eq!(app.about_window_id, about_window_id);
    let _ = app.update(Message::WindowClosed(about_window_id.unwrap()));
    assert_eq!(app.about_window_id, None);

    let _ = app.update(Message::ExecuteCommand(help[0].id));
    assert!(app.action_list_window_id.is_some());
}

#[test]
fn documented_first_project_shortcuts_match_action_defaults() {
    let app = App::default();
    let shortcuts = commands::shortcut_entries(&app);
    for (action_id, expected) in [
        ("file.new-project", "Mod+N"),
        ("file.open-project", "Mod+O"),
        ("file.save-project", "Mod+S"),
        ("edit.undo", "Mod+Z"),
        ("edit.redo", "Mod+Shift+Z; Mod+Y"),
        ("item.duplicate", "Mod+D"),
        ("edit.delete-focused", "Delete"),
        ("item.split-at-cursor", "S"),
    ] {
        let entry = shortcuts
            .iter()
            .find(|entry| entry.id == action_id)
            .unwrap_or_else(|| panic!("documented action {action_id} should exist"));
        assert_eq!(
            entry.default_binding,
            commands::format_shortcut_label(expected),
            "{action_id}"
        );
    }

    #[cfg(feature = "audio-device")]
    {
        let playback = shortcuts
            .iter()
            .find(|entry| entry.id == "transport.toggle-playback")
            .expect("playback shortcut should exist in audio builds");
        assert_eq!(playback.default_binding, "Enter; Ctrl+Space");

        let stop = shortcuts
            .iter()
            .find(|entry| entry.id == "transport.stop-playback")
            .expect("stop shortcut should exist in audio builds");
        assert_eq!(stop.default_binding, "");
    }
}

#[cfg(feature = "audio-device")]
#[test]
fn stop_shortcut_resolves_to_a_distinct_transport_command() {
    let key = Key::Named(iced::keyboard::key::Named::Space);
    assert_eq!(
        commands::from_shortcut(&key, Modifiers::NONE, &HashMap::new(), &[]),
        Some(CommandId::TogglePlayStop)
    );
}

#[test]
fn settings_categories_preserve_edits_and_actions_restore_individual_defaults() {
    let mut app = App {
        shortcut_binding_edits: HashMap::from([
            ("file.new-project".to_owned(), "Mod+P".to_owned()),
            ("file.save-project-as".to_owned(), "Mod+A".to_owned()),
            ("edit.undo".to_owned(), "Mod+U".to_owned()),
        ]),
        shortcut_bindings: std::sync::Arc::new(std::sync::RwLock::new(HashMap::from([
            ("file.new-project".to_owned(), "Mod+P".to_owned()),
            ("file.save-project-as".to_owned(), "Mod+A".to_owned()),
            ("edit.undo".to_owned(), "Mod+U".to_owned()),
        ]))),
        ..App::default()
    };
    let _ = app.update(Message::StartShortcutCapture("edit.undo".to_owned()));
    let _ = app.update(Message::SelectSettingsCategory(
        super::SettingsCategory::KeyboardShortcuts,
    ));
    assert_eq!(
        app.settings_category,
        super::SettingsCategory::KeyboardShortcuts
    );
    assert_eq!(
        app.shortcut_binding_edits.get("file.new-project").unwrap(),
        "Mod+P"
    );
    assert!(app.shortcut_capture_id.is_none());

    let _ = app.update(Message::SelectSettingsCategory(
        super::SettingsCategory::ClapPlugins,
    ));
    assert_eq!(app.settings_category, super::SettingsCategory::ClapPlugins);
    assert_eq!(
        app.shortcut_binding_edits.get("file.new-project").unwrap(),
        "Mod+P"
    );
    let _ = app.update(Message::SelectSettingsCategory(
        super::SettingsCategory::KeyboardShortcuts,
    ));

    let _ = app.update(Message::RestoreShortcutDefault(
        "file.new-project".to_owned(),
    ));
    assert!(!app.shortcut_binding_edits.contains_key("file.new-project"));
    assert_eq!(
        app.shortcut_binding_edits
            .get("file.save-project-as")
            .unwrap(),
        "Mod+A"
    );
    assert_eq!(
        app.shortcut_binding_edits.get("edit.undo").unwrap(),
        "Mod+U"
    );

    let _ = app.update(Message::RestoreShortcutDefault(
        "file.save-project-as".to_owned(),
    ));
    assert!(
        !app.shortcut_binding_edits
            .contains_key("file.save-project-as")
    );
    assert_eq!(
        commands::shortcut_entries(&app)
            .into_iter()
            .find(|entry| entry.id == "file.save-project-as")
            .unwrap()
            .binding,
        commands::format_shortcut_label("Ctrl+Alt+S")
    );
    assert_eq!(
        commands::shortcut_entries(&app)
            .into_iter()
            .find(|entry| entry.id == "file.new-project")
            .unwrap()
            .binding,
        commands::format_shortcut_label("Mod+N")
    );
    assert_eq!(
        commands::shortcut_entries(&app)
            .into_iter()
            .find(|entry| entry.id == "file.save-project-as")
            .unwrap()
            .binding,
        commands::format_shortcut_label("Ctrl+Alt+S")
    );
}

#[test]
fn plugin_picker_settings_action_opens_the_clap_settings_category() {
    let mut app = App::default();

    let _ = app.update(Message::OpenClapPluginSettings);

    assert_eq!(app.settings_category, super::SettingsCategory::ClapPlugins);
    assert!(app.settings_window_id.is_some());
}

#[test]
fn clap_plugin_scan_results_update_settings_without_mutating_project_state() {
    let mut app = App {
        clap_plugin_scan_busy: true,
        ..App::default()
    };
    let project = app.project.snapshot();
    let revision = app.revision;
    let report = aaadaw_app::ClapPluginScanReport {
        plugins: vec![aaadaw_app::ClapPluginDescriptor {
            entry_path: std::path::PathBuf::from("/plugins/test.clap"),
            plugin_id: "org.example.test".to_owned(),
            name: "Test Instrument".to_owned(),
            vendor: Some("Example".to_owned()),
            features: vec!["instrument".to_owned()],
        }],
        errors: vec![],
        entries_checked: 1,
    };

    let _ = app.update(Message::ClapPluginsScanned(Ok(report)));

    assert!(!app.clap_plugin_scan_busy);
    assert_eq!(app.clap_plugin_scan.plugins.len(), 1);
    assert!(app.clap_plugin_scan.plugins[0].is_instrument());
    assert_eq!(app.revision, revision);
    assert_eq!(app.project.snapshot(), project);
}

#[test]
fn cached_clap_scan_results_remain_usable_when_refresh_fails() {
    let directory = tempfile::tempdir().unwrap();
    let cache_path = directory.path().join("clap-scan-cache.json");
    let previous = test_clap_scan_report("Cached Instrument");
    let cached_paths = vec![std::path::PathBuf::from("/previous/plugins")];
    super::clap_plugin_cache::save_to(&cache_path, &cached_paths, &previous).unwrap();
    let mut app = App {
        clap_plugin_cache_path: Some(cache_path),
        clap_plugin_paths: vec![directory.path().to_path_buf()],
        ..App::default()
    };

    assert_eq!(app.restore_cached_clap_plugin_scan(), None);
    assert!(app.clap_plugin_scan_is_cached);
    assert_eq!(app.clap_plugin_scan_paths, cached_paths);
    assert_eq!(app.clap_plugin_scan, previous);

    let _task = app.start_clap_plugin_scan(false);
    assert!(app.clap_plugin_scan_busy);
    let _ = app.update(Message::ClapPluginsScanned(Err("worker failed".to_owned())));

    assert!(!app.clap_plugin_scan_busy);
    assert!(app.clap_plugin_scan_is_cached);
    assert_eq!(app.clap_plugin_scan.plugins[0].name, "Cached Instrument");
    assert!(app.clap_plugin_settings_feedback.contains("cached results"));
}

#[test]
fn startup_clap_scan_loads_cached_results_before_starting_refresh() {
    let directory = tempfile::tempdir().unwrap();
    let cache_path = directory.path().join("clap-scan-cache.json");
    let cached = test_clap_scan_report("Startup Instrument");
    super::clap_plugin_cache::save_to(
        &cache_path,
        &[std::path::PathBuf::from("/previous/plugins")],
        &cached,
    )
    .unwrap();
    let mut app = App {
        clap_plugin_paths: vec![directory.path().to_path_buf()],
        ..App::default()
    };

    let _task = app.initialize_clap_plugin_scan(Some(cache_path), Vec::new());

    assert!(app.clap_plugin_scan_busy);
    assert!(app.clap_plugin_scan_is_cached);
    assert_eq!(app.clap_plugin_scan, cached);
    assert!(
        app.clap_plugin_settings_feedback
            .contains("cached results remain available")
    );
}

#[test]
fn successful_clap_refresh_replaces_the_cache_and_marks_results_fresh() {
    let directory = tempfile::tempdir().unwrap();
    let cache_path = directory.path().join("clap-scan-cache.json");
    let search_path = directory.path().join("plugins");
    std::fs::create_dir(&search_path).unwrap();
    let fresh = test_clap_scan_report("Fresh Instrument");
    let mut app = App {
        clap_plugin_cache_path: Some(cache_path.clone()),
        clap_plugin_paths: vec![search_path.clone()],
        clap_plugin_scan_busy: true,
        ..App::default()
    };

    let _ = app.update(Message::ClapPluginsScanned(Ok(fresh.clone())));

    assert!(!app.clap_plugin_scan_busy);
    assert!(!app.clap_plugin_scan_is_cached);
    assert_eq!(app.clap_plugin_scan, fresh);
    let cached = super::clap_plugin_cache::load_from(&cache_path)
        .unwrap()
        .unwrap();
    assert_eq!(cached.report, fresh);
    assert_eq!(cached.search_paths, vec![search_path]);
}

fn test_clap_scan_report(name: &str) -> aaadaw_app::ClapPluginScanReport {
    aaadaw_app::ClapPluginScanReport {
        plugins: vec![aaadaw_app::ClapPluginDescriptor {
            entry_path: std::path::PathBuf::from("/plugins/test.clap"),
            plugin_id: "org.example.test".to_owned(),
            name: name.to_owned(),
            vendor: Some("Example".to_owned()),
            features: vec!["instrument".to_owned()],
        }],
        errors: Vec::new(),
        entries_checked: 1,
    }
}

#[test]
fn track_fx_add_button_opens_a_scanned_plugin_picker_and_chain_edits_use_actions() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();

    let _ = app.update(Message::OpenTrackFxChain(track_id));
    let chain_window_id = app.fx_chain_window_id.expect("FX chain window should open");
    assert_eq!(app.fx_chain_track_id, Some(track_id));
    assert_ne!(app.main_window_id, Some(chain_window_id));

    app.clap_plugin_scan.plugins = vec![aaadaw_app::ClapPluginDescriptor {
        entry_path: std::path::PathBuf::from("/plugins/test.clap"),
        plugin_id: "org.example.test-effect".to_owned(),
        name: "Test Effect".to_owned(),
        vendor: Some("Example".to_owned()),
        features: vec!["audio-effect".to_owned()],
    }];
    let _ = app.update(Message::OpenPluginPicker);
    let picker_window_id = app
        .plugin_picker_window_id
        .expect("Add should open a separate plugin picker");
    assert_ne!(picker_window_id, chain_window_id);
    assert_eq!(app.plugin_picker_track_id, Some(track_id));

    let _ = app.update(Message::AddScannedPlugin(
        "org.example.test-effect".to_owned(),
    ));
    let track = app
        .project
        .tracks()
        .iter()
        .find(|track| track.id() == track_id)
        .unwrap();
    assert_eq!(track.fx_chain().len(), 1);
    assert_eq!(track.fx_chain()[0].plugin_id(), "org.example.test-effect");
    assert_eq!(track.fx_chain()[0].bundle_path(), "/plugins/test.clap");
    assert!(track.fx_chain()[0].is_enabled());
    assert_eq!(app.plugin_picker_window_id, None);
    assert_eq!(app.fx_chain_selected_index, Some(0));

    let _ = app.update(Message::ToggleFxChainPlugin(0));
    assert!(!app.project.tracks()[0].fx_chain()[0].is_enabled());
    let _ = app.update(Message::RemoveSelectedFxPlugin);
    assert!(app.project.tracks()[0].fx_chain().is_empty());
}

#[test]
fn fx_parameter_slider_commits_or_cancels_one_undoable_gesture() {
    let mut app = App::default();
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Guitar".to_owned(),
        })
        .unwrap();
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::SetTrackFxChain {
            track_id,
            plugins: vec![TrackFxPlugin::new("org.example.eq", "/plugins/eq.clap").unwrap()],
        })
        .unwrap();
    app.fx_chain_track_id = Some(track_id);
    app.fx_chain_selected_index = Some(0);
    app.fx_chain_parameters = vec![
        aaadaw_engine::ClapParameterInfo {
            id: 12,
            name: "Gain".to_owned(),
            min_value: -24.0,
            max_value: 24.0,
            default_value: 0.0,
            value: 0.0,
            display_value: "0 dB".to_owned(),
            stepped: false,
            read_only: false,
        },
        aaadaw_engine::ClapParameterInfo {
            id: 13,
            name: "Oscillator mode".to_owned(),
            min_value: 0.0,
            max_value: 3.0,
            default_value: 1.0,
            value: 0.0,
            display_value: "Sine".to_owned(),
            stepped: true,
            read_only: false,
        },
    ];

    let _ = app.update(Message::FxParameterChanged(12, 6.0));
    let _ = app.update(Message::FxParameterChanged(12, 12.0));
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].parameter_value(12),
        None
    );
    let _ = app.update(Message::FxParameterEnded(12));
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].parameter_value(12),
        Some(12.0)
    );

    let _ = app.update(Message::Undo);
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].parameter_value(12),
        Some(0.0)
    );
    let _ = app.update(Message::Redo);
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].parameter_value(12),
        Some(12.0)
    );
    let revision = app.revision;
    let _ = app.update(Message::FxParameterChanged(12, 18.0));
    let _ = app.update(Message::CancelFxParameterGesture(12));
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].parameter_value(12),
        Some(12.0)
    );
    assert!(app.fx_parameter_gesture.is_none());
    assert_eq!(app.revision, revision);

    let _ = app.update(Message::FxParameterValueTextChanged(
        12,
        "-3.125".to_owned(),
    ));
    let _ = app.update(Message::CommitFxParameterValue(12));
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].parameter_value(12),
        Some(-3.125)
    );

    let _ = app.update(Message::ResetFxParameterValue(12));
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].parameter_value(12),
        Some(0.0)
    );

    let _ = app.update(Message::FxParameterValueTextChanged(12, "2.5".to_owned()));
    let _ = app.update(Message::FxParameterChanged(12, 8.0));
    assert_eq!(
        app.fx_parameter_value_edits.get(&12).map(String::as_str),
        Some("2.5")
    );
    let _ = app.update(Message::FxParameterEnded(12));
    let _ = app.update(Message::CommitFxParameterValue(12));
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].parameter_value(12),
        Some(2.5)
    );

    let _ = app.update(Message::FxParameterValueTextChanged(12, "999".to_owned()));
    let _ = app.update(Message::CommitFxParameterValue(12));
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].parameter_value(12),
        Some(2.5)
    );

    let _ = app.update(Message::FxParameterChanged(12, 15.0));
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].parameter_value(12),
        Some(2.5)
    );

    let _ = app.update(Message::FxParameterChanged(13, 2.0));
    let _ = app.update(Message::FxParameterEnded(13));
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].parameter_value(13),
        Some(2.0)
    );
    assert_eq!(app.fx_chain_parameters[1].display_value, "2");
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].parameter_value(13),
        Some(0.0)
    );
    assert_eq!(app.fx_chain_parameters[1].display_value, "0");
    let _ = app.update(Message::Redo);
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].parameter_value(13),
        Some(2.0)
    );

    let _ = app.update(Message::FxParameterValueTextChanged(13, "1.5".to_owned()));
    let _ = app.update(Message::CommitFxParameterValue(13));
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].parameter_value(13),
        Some(2.0)
    );
    assert_eq!(
        app.status,
        "Stepped parameters require a whole-number value"
    );
    let _ = app.update(Message::FxParameterValueTextChanged(13, "4".to_owned()));
    let _ = app.update(Message::CommitFxParameterValue(13));
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].parameter_value(13),
        Some(2.0)
    );
    let _ = app.update(Message::FxParameterChanged(13, 4.0));
    let _ = app.update(Message::FxParameterEnded(13));
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].parameter_value(13),
        Some(2.0)
    );

    let _ = app.update(Message::FxParameterValueTextChanged(13, "3".to_owned()));
    let _ = app.update(Message::CommitFxParameterValue(13));
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].parameter_value(13),
        Some(3.0)
    );
    let _ = app.update(Message::ResetFxParameterValue(13));
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].parameter_value(13),
        Some(1.0)
    );
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].parameter_value(13),
        Some(3.0)
    );
    let _ = app.update(Message::Redo);
    assert_eq!(
        app.project.tracks()[0].fx_chain()[0].parameter_value(13),
        Some(1.0)
    );
}

#[test]
fn track_context_instrument_picker_assigns_only_instruments_through_undoable_actions() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.clap_plugin_scan.plugins = vec![
        aaadaw_app::ClapPluginDescriptor {
            entry_path: std::path::PathBuf::from("/plugins/test-synth.clap"),
            plugin_id: "org.example.test-synth".to_owned(),
            name: "Test Synth".to_owned(),
            vendor: Some("Example".to_owned()),
            features: vec!["instrument".to_owned()],
        },
        aaadaw_app::ClapPluginDescriptor {
            entry_path: std::path::PathBuf::from("/plugins/test-effect.clap"),
            plugin_id: "org.example.test-effect".to_owned(),
            name: "Test Effect".to_owned(),
            vendor: Some("Example".to_owned()),
            features: vec!["audio-effect".to_owned()],
        },
    ];

    let _ = app.update(Message::OpenTrackInstrumentPicker(track_id));
    assert_eq!(app.plugin_picker_instrument_track_id, Some(track_id));
    let _ = app.update(Message::SelectScannedInstrument(
        "org.example.test-effect".to_owned(),
    ));
    assert!(app.project.tracks()[0].instrument().is_none());
    assert!(app.status.contains("no longer in the scan results"));

    let _ = app.update(Message::SelectScannedInstrument(
        "org.example.test-synth".to_owned(),
    ));
    let instrument = app.project.tracks()[0]
        .instrument()
        .expect("selected instrument should be assigned");
    assert_eq!(instrument.plugin_id(), "org.example.test-synth");
    assert_eq!(instrument.bundle_path(), "/plugins/test-synth.clap");
    assert_eq!(app.plugin_picker_window_id, None);

    let _ = app.update(Message::Undo);
    assert!(app.project.tracks()[0].instrument().is_none());
    let _ = app.update(Message::Redo);
    assert_eq!(
        app.project.tracks()[0]
            .instrument()
            .map(|instrument| instrument.plugin_id()),
        Some("org.example.test-synth")
    );

    let _ = app.update(Message::ClearTrackInstrument(track_id));
    assert!(app.project.tracks()[0].instrument().is_none());
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.project.tracks()[0]
            .instrument()
            .map(|instrument| instrument.plugin_id()),
        Some("org.example.test-synth")
    );
}

#[test]
fn configurable_shortcuts_drive_dispatch_and_menu_hints_with_duplicate_bindings() {
    let bindings = HashMap::from([
        ("edit.undo".to_owned(), "Mod+U".to_owned()),
        ("file.save-project".to_owned(), "Mod+Shift+S".to_owned()),
    ]);
    let bindings = commands::validate_bindings(&bindings).unwrap();
    assert_eq!(bindings.get("edit.undo").map(String::as_str), Some("Mod+U"));
    assert!(matches!(
        shortcut_message(Key::Character("u"), Modifiers::COMMAND, &bindings),
        Some(Message::ExecuteCommand(CommandId::Undo))
    ));
    assert!(shortcut_message(Key::Character("z"), Modifiers::COMMAND, &bindings).is_none());

    let app = App::default();
    *app.shortcut_bindings
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = bindings;
    let undo = commands::for_menu(&app, MainMenu::Edit)
        .into_iter()
        .find(|entry| entry.id == CommandId::Undo)
        .unwrap();
    assert_eq!(
        undo.shortcut.as_deref(),
        Some(commands::format_shortcut_label("Mod+U").as_str())
    );

    let conflicting = HashMap::from([
        ("edit.undo".to_owned(), "Mod+X".to_owned()),
        ("file.save-project".to_owned(), "Mod+X".to_owned()),
    ]);
    let duplicate = commands::validate_bindings(&conflicting).unwrap();
    assert_eq!(duplicate["edit.undo"], "Mod+X");
    assert_eq!(duplicate["file.save-project"], "Mod+X");
    assert!(
        commands::validate_bindings(&HashMap::from([(
            "file.typo".to_owned(),
            "Mod+T".to_owned()
        )]))
        .is_err()
    );
    assert!(
        commands::validate_bindings(&HashMap::from([(
            "edit.undo".to_owned(),
            "Mod+Nope".to_owned()
        )]))
        .is_err()
    );
}

#[test]
fn shortcut_alternatives_dispatch_named_keys_and_allow_action_duplicates() {
    assert!(
        commands::validate_bindings(&HashMap::from([(
            "edit.undo".to_owned(),
            "Escape".to_owned(),
        )]))
        .is_err()
    );
    assert!(
        commands::validate_bindings(&HashMap::from([(
            "edit.undo".to_owned(),
            "Alt+Escape".to_owned(),
        )]))
        .is_ok()
    );
    let bindings = commands::validate_bindings(&HashMap::from([(
        "edit.undo".to_owned(),
        "Ctrl+Alt+F12; Shift+Home; Alt+7".to_owned(),
    )]))
    .unwrap();
    for (key, modifiers) in [
        (
            Key::Named(iced::keyboard::key::Named::F12),
            Modifiers::CTRL | Modifiers::ALT,
        ),
        (
            Key::Named(iced::keyboard::key::Named::Home),
            Modifiers::SHIFT,
        ),
        (Key::Character("7"), Modifiers::ALT),
    ] {
        assert!(matches!(
            shortcut_message(key, modifiers, &bindings),
            Some(Message::ExecuteCommand(CommandId::Undo))
        ));
    }
    // Existing factory defaults do not make otherwise valid user chords fail.
    assert!(
        commands::validate_bindings(&HashMap::from([(
            "edit.undo".to_owned(),
            "F1; Backspace".to_owned(),
        )]))
        .is_ok()
    );
    assert!(
        commands::validate_bindings(&HashMap::from([
            ("edit.undo".to_owned(), "Alt+F1; Alt+F2".to_owned()),
            ("file.save-project".to_owned(), "Alt+F2".to_owned()),
        ]))
        .is_ok()
    );
}

#[test]
fn shortcut_named_key_events_reach_project_actions_without_losing_focus_guards() {
    let mut app = App::default();
    let window = iced::window::Id::unique();
    app.main_window_id = Some(window);
    *app.shortcut_bindings.write().unwrap() = commands::validate_bindings(&HashMap::from([(
        "track.add".to_owned(),
        "Alt+F1".to_owned(),
    )]))
    .unwrap();
    let event = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: Key::Named(iced::keyboard::key::Named::F1),
        modified_key: Key::Named(iced::keyboard::key::Named::F1),
        physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::F1),
        location: iced::keyboard::Location::Standard,
        modifiers: Modifiers::ALT,
        text: None,
        repeat: false,
    });
    assert!(
        keyboard_shortcut_event(
            event.clone(),
            iced::event::Status::Captured,
            window,
            Some(window),
            None,
            None
        )
        .is_none()
    );
    assert!(
        keyboard_shortcut_event(
            event.clone(),
            iced::event::Status::Ignored,
            iced::window::Id::unique(),
            Some(window),
            None,
            None
        )
        .is_none()
    );
    let message = keyboard_shortcut_event(
        event,
        iced::event::Status::Ignored,
        window,
        Some(window),
        None,
        None,
    )
    .unwrap();
    let _ = app.update(message);
    assert_eq!(app.project.tracks().len(), 1);
    let _ = app.update(Message::Undo);
    assert!(app.project.tracks().is_empty());
}

#[test]
fn focused_track_name_editor_routes_native_text_escape_and_undo_keys() {
    let window = iced::window::Id::unique();
    let mut app = App {
        main_window_id: Some(window),
        ..App::default()
    };
    let _ = app.update(Message::AddTrack);
    let first_track = app.project.tracks()[0].id();
    app.timeline
        .handle(crate::timeline::TimelineEvent::SetTimeSelection {
            start_tick: 240,
            end_tick: 960,
        });
    let selected_ranges = (app.timeline.time_selection, app.timeline.loop_points);
    app.begin_track_draft(first_track, super::TrackDraftField::Name);
    app.track_name_edits.insert(first_track, "draft".to_owned());

    let space = test_input_event(test_shortcut_input(
        Key::Character(" ".into()),
        Modifiers::NONE,
    ));
    assert!(
        keyboard_shortcut_event(
            space.clone(),
            iced::event::Status::Captured,
            window,
            Some(window),
            None,
            None,
        )
        .is_none()
    );
    let _ = app.update(Message::RuntimeKeyboardEvent(
        space,
        iced::event::Status::Captured,
        window,
    ));
    assert_eq!(app.project.tracks().len(), 1);
    assert_eq!(app.timeline.time_selection, selected_ranges.0);

    let escape = test_input_event(test_shortcut_input(
        Key::Named(iced::keyboard::key::Named::Escape),
        Modifiers::NONE,
    ));
    let _ = app.update(Message::RuntimeKeyboardEvent(
        escape,
        iced::event::Status::Captured,
        window,
    ));
    assert!(app.active_track_draft.is_none());
    assert_eq!(app.timeline.time_selection, selected_ranges.0);
    assert_eq!(app.timeline.loop_points, selected_ranges.1);

    let _ = app.update(Message::AddTrack);
    let second_track = app.project.tracks()[1].id();
    app.begin_track_draft(second_track, super::TrackDraftField::Name);
    app.track_name_edits
        .insert(second_track, "draft".to_owned());
    let undo = test_input_event(test_shortcut_input(
        Key::Character("z".into()),
        Modifiers::CTRL,
    ));
    let _ = app.update(Message::RuntimeKeyboardEvent(
        undo,
        iced::event::Status::Ignored,
        window,
    ));
    assert_eq!(app.project.tracks().len(), 1);
    assert_eq!(app.project.tracks()[0].id(), first_track);
    assert_eq!(app.timeline.time_selection, selected_ranges.0);
    assert_eq!(app.timeline.loop_points, selected_ranges.1);
}

#[test]
fn action_list_filter_focus_contains_global_shortcuts_and_escape_closes_the_window() {
    let main_window = iced::window::Id::unique();
    let action_list_window = iced::window::Id::unique();
    let mut app = App {
        main_window_id: Some(main_window),
        action_list_window_id: Some(action_list_window),
        ..App::default()
    };
    let _ = app.update(Message::AddTrack);
    app.action_list.set_query("undo".to_owned());

    let space = test_input_event(test_shortcut_input(
        Key::Character(" ".into()),
        Modifiers::NONE,
    ));
    let _ = app.update(Message::RuntimeKeyboardEvent(
        space,
        iced::event::Status::Captured,
        action_list_window,
    ));
    assert_eq!(app.action_list.query, "undo");
    assert_eq!(app.project.tracks().len(), 1);

    let undo = test_input_event(test_shortcut_input(
        Key::Character("z".into()),
        Modifiers::CTRL,
    ));
    let _ = app.update(Message::RuntimeKeyboardEvent(
        undo.clone(),
        iced::event::Status::Ignored,
        action_list_window,
    ));
    assert_eq!(app.action_list.query, "undo");
    assert_eq!(app.project.tracks().len(), 1);

    let _ = app.update(Message::RuntimeKeyboardEvent(
        undo,
        iced::event::Status::Ignored,
        main_window,
    ));
    assert!(app.project.tracks().is_empty());

    let escape = test_input_event(test_shortcut_input(
        Key::Named(iced::keyboard::key::Named::Escape),
        Modifiers::NONE,
    ));
    let _ = app.update(Message::RuntimeKeyboardEvent(
        escape,
        iced::event::Status::Captured,
        action_list_window,
    ));
    assert!(app.action_list_window_id.is_none());
}

#[test]
fn action_list_filter_enter_focuses_results_before_running_selected_action() {
    let main_window = iced::window::Id::unique();
    let action_list_window = iced::window::Id::unique();
    let mut app = App {
        main_window_id: Some(main_window),
        action_list_window_id: Some(action_list_window),
        ..App::default()
    };
    let _ = app.update(Message::AddTrack);
    app.action_list.set_query("undo".to_owned());
    app.action_list.select("edit.undo".to_owned());

    let enter = test_input_event(test_shortcut_input(
        Key::Named(iced::keyboard::key::Named::Enter),
        Modifiers::NONE,
    ));
    let _ = app.update(Message::RuntimeKeyboardEvent(
        enter.clone(),
        iced::event::Status::Captured,
        action_list_window,
    ));
    let _ = app.update(Message::ActionListFilterSubmitted);
    assert_eq!(app.project.tracks().len(), 1);

    let _ = app.update(Message::RuntimeKeyboardEvent(
        enter,
        iced::event::Status::Ignored,
        action_list_window,
    ));
    assert!(app.project.tracks().is_empty());
}

#[test]
fn action_list_result_navigation_selects_visible_actions_and_clamps_at_bounds() {
    let main_window = iced::window::Id::unique();
    let action_list_window = iced::window::Id::unique();
    let mut app = App {
        main_window_id: Some(main_window),
        action_list_window_id: Some(action_list_window),
        ..App::default()
    };
    app.action_list.set_query("item".to_owned());
    app.action_list.results_viewport_height = 65.0;
    let visible_ids = app
        .action_list_entries()
        .into_iter()
        .filter_map(|entry| commands::stable_id(entry.id))
        .collect::<Vec<_>>();
    assert!(visible_ids.len() > 8);

    let down = test_input_event(test_shortcut_input(
        Key::Named(iced::keyboard::key::Named::ArrowDown),
        Modifiers::NONE,
    ));
    let up = test_input_event(test_shortcut_input(
        Key::Named(iced::keyboard::key::Named::ArrowUp),
        Modifiers::NONE,
    ));
    let home = test_input_event(test_shortcut_input(
        Key::Named(iced::keyboard::key::Named::Home),
        Modifiers::NONE,
    ));
    let end = test_input_event(test_shortcut_input(
        Key::Named(iced::keyboard::key::Named::End),
        Modifiers::NONE,
    ));
    let _ = app.update(Message::RuntimeKeyboardEvent(
        down.clone(),
        iced::event::Status::Captured,
        action_list_window,
    ));
    assert!(app.action_list.selected.is_none());

    let _ = app.update(Message::ActionListFilterSubmitted);
    assert_eq!(app.action_list.selected.as_ref(), Some(&visible_ids[0]));
    let _ = app.update(Message::RuntimeKeyboardEvent(
        end,
        iced::event::Status::Ignored,
        action_list_window,
    ));
    assert_eq!(app.action_list.selected.as_ref(), visible_ids.last());
    assert!(app.action_list.results_scroll_offset > 0.0);
    let _ = app.update(Message::RuntimeKeyboardEvent(
        home,
        iced::event::Status::Ignored,
        action_list_window,
    ));
    assert_eq!(app.action_list.selected.as_ref(), Some(&visible_ids[0]));
    let _ = app.update(Message::RuntimeKeyboardEvent(
        down.clone(),
        iced::event::Status::Ignored,
        action_list_window,
    ));
    assert_eq!(app.action_list.selected.as_ref(), Some(&visible_ids[1]));
    let _ = app.update(Message::RuntimeKeyboardEvent(
        up.clone(),
        iced::event::Status::Ignored,
        action_list_window,
    ));
    assert_eq!(app.action_list.selected.as_ref(), Some(&visible_ids[0]));
    let _ = app.update(Message::RuntimeKeyboardEvent(
        up.clone(),
        iced::event::Status::Ignored,
        action_list_window,
    ));
    assert_eq!(app.action_list.selected.as_ref(), Some(&visible_ids[0]));

    for _ in 1..visible_ids.len() {
        let _ = app.update(Message::RuntimeKeyboardEvent(
            down.clone(),
            iced::event::Status::Ignored,
            action_list_window,
        ));
    }
    assert_eq!(app.action_list.selected.as_ref(), visible_ids.last());
    let offset_at_end = app.action_list.results_scroll_offset;
    let _ = app.update(Message::RuntimeKeyboardEvent(
        down,
        iced::event::Status::Ignored,
        action_list_window,
    ));
    assert_eq!(app.action_list.selected.as_ref(), visible_ids.last());
    assert_eq!(app.action_list.results_scroll_offset, offset_at_end);
    for _ in 0..6 {
        let _ = app.update(Message::RuntimeKeyboardEvent(
            up.clone(),
            iced::event::Status::Ignored,
            action_list_window,
        ));
    }
    assert!(app.action_list.results_scroll_offset < offset_at_end);
}

#[test]
fn shortcut_add_capture_preserves_existing_bindings_and_duplicate_candidates() {
    let mut app = App::default();
    let _ = app.update(Message::AddShortcutBinding("edit.undo".to_owned()));
    let _ = app.update(Message::ShortcutCaptureKey {
        action_id: "edit.undo".to_owned(),
        key: "F12".to_owned(),
        modifiers: Modifiers::ALT,
    });
    let saved = app.shortcut_binding_edits.clone();
    assert_eq!(saved["edit.undo"], "Mod+Z; Alt+F12");
    for (key, modifiers) in [
        (Key::Character("z"), Modifiers::COMMAND),
        (Key::Named(iced::keyboard::key::Named::F12), Modifiers::ALT),
    ] {
        assert!(matches!(
            shortcut_message(key, modifiers, &saved),
            Some(Message::ExecuteCommand(CommandId::Undo))
        ));
    }
    let _ = app.update(Message::AddShortcutBinding("file.save-project".to_owned()));
    let _ = app.update(Message::ShortcutCaptureKey {
        action_id: "file.save-project".to_owned(),
        key: "F12".to_owned(),
        modifiers: Modifiers::ALT,
    });
    assert_ne!(app.shortcut_binding_edits, saved);
    assert_eq!(app.shortcut_binding_edits["edit.undo"], "Mod+Z; Alt+F12");
    assert!(app.shortcut_binding_edits["file.save-project"].contains("Alt+F12"));
    assert!(app.shortcut_capture_id.is_none());
}

#[test]
fn removed_workspace_shortcuts_do_not_discard_other_saved_bindings() {
    let bindings = HashMap::from([
        ("view.media".to_owned(), "Mod+M".to_owned()),
        ("file.save-project".to_owned(), "Mod+Shift+S".to_owned()),
    ]);
    let validated = commands::validate_bindings(&bindings).unwrap();
    assert!(!validated.contains_key("view.media"));
    assert_eq!(
        validated.get("file.save-project").map(String::as_str),
        Some("Mod+Shift+S")
    );
}

#[test]
fn media_browser_dock_toggles_and_resizes_without_replacing_arrangement() {
    let mut app = App::default();
    assert!(!app.media_panel_dock.open);
    assert!(app.media_panel_dock.mixer_open);
    assert_eq!(app.media_panel_dock.panes.as_ref().unwrap().len(), 2);
    let toggle = CommandId::ToggleMediaBrowserPanel;
    assert!(commands::is_enabled(&app, toggle));
    assert!(
        commands::for_menu(&app, MainMenu::View)
            .iter()
            .any(|entry| entry.id == toggle)
    );

    let _ = app.update(Message::ExecuteCommand(toggle));
    assert!(app.media_panel_dock.open);
    assert_eq!(app.media_panel_dock.panes.as_ref().unwrap().len(), 3);
    let split = app.media_panel_dock.split.unwrap();
    let _ = app.update(Message::MediaPanelResized(split, 0.63));
    assert!((app.media_panel_dock.main_ratio - 0.63).abs() < f32::EPSILON);

    let _ = app.update(Message::ToggleMediaBrowserPanel);
    assert!(!app.media_panel_dock.open);
    let _ = app.update(Message::ToggleMediaBrowserPanel);
    assert!(app.media_panel_dock.open);
    assert!((app.media_panel_dock.main_ratio - 0.63).abs() < f32::EPSILON);
}

#[test]
fn view_menu_contains_the_media_browser_panel_and_no_workspace_pages() {
    let app = App::default();
    let entries = commands::for_menu(&app, MainMenu::View);
    assert!(entries.iter().any(|entry| {
        entry.id == CommandId::ToggleMediaBrowserPanel && entry.label == "Toggle Media Browser"
    }));
    assert!(
        entries
            .iter()
            .all(|entry| { !["Arrangement", "Media", "Project"].contains(&entry.label.as_str()) })
    );
}

#[test]
fn offline_jobs_panel_is_available_from_view_menu_and_action_search() {
    let mut app = App::default();
    let toggle = CommandId::ToggleOfflineJobsPanel;
    assert!(commands::is_enabled(&app, toggle));
    assert!(
        commands::for_menu(&app, MainMenu::View)
            .iter()
            .any(|entry| entry.id == toggle && entry.label == "Toggle Offline Jobs")
    );
    assert!(
        commands::for_actions_menu(&app)
            .iter()
            .any(|entry| entry.id == toggle)
    );

    let _ = app.update(Message::ExecuteCommand(toggle));
    assert!(app.offline_jobs_panel_open);
    let _ = app.update(Message::Escape);
    assert!(!app.offline_jobs_panel_open);
}

#[test]
fn native_open_render_queue_action_opens_the_offline_jobs_panel_idempotently() {
    let mut app = App::default();
    let file_menu = commands::for_menu(&app, MainMenu::File);
    let render_index = file_menu
        .iter()
        .position(|entry| entry.id == CommandId::ExportWav)
        .expect("File menu should expose Render");
    let queue = file_menu
        .iter()
        .find(|entry| entry.id == CommandId::OpenRenderQueue)
        .expect("File menu should expose Queued Renders");
    let queue_index = file_menu
        .iter()
        .position(|entry| entry.id == CommandId::OpenRenderQueue)
        .unwrap();
    assert_eq!(queue.label, "Queued Renders");
    assert!(queue.enabled);
    assert_eq!(queue.shortcut, None);
    assert_eq!(queue_index, render_index + 1);

    let action = app
        .action_list_entries()
        .into_iter()
        .find(|entry| entry.id == CommandId::OpenRenderQueue)
        .expect("Action List should expose the native File action");
    assert_eq!(action.category, "File");
    assert_eq!(action.label, "Open render queue");
    assert_eq!(action.shortcut, None);
    assert_eq!(
        commands::matching_actions_menu(&app, "render queue")
            .first()
            .map(|entry| entry.id),
        Some(CommandId::OpenRenderQueue)
    );

    let _ = app.update(Message::ExecuteCommand(CommandId::OpenRenderQueue));
    assert!(app.offline_jobs_panel_open);
    let _ = app.update(Message::ExecuteCommand(CommandId::OpenRenderQueue));
    assert!(
        app.offline_jobs_panel_open,
        "Open render queue must not toggle closed"
    );
}

#[test]
fn native_export_project_midi_action_opens_the_export_settings_window() {
    let mut app = App::default();
    let export = commands::for_menu(&app, MainMenu::File)
        .into_iter()
        .find(|entry| entry.id == CommandId::ExportProjectMidi)
        .expect("File menu should expose Export project MIDI");
    assert_eq!(export.label, "Export project MIDI...");
    assert!(export.enabled);
    assert_eq!(export.shortcut, None);

    let action = app
        .action_list_entries()
        .into_iter()
        .find(|entry| entry.id == CommandId::ExportProjectMidi)
        .expect("Action List should expose the native MIDI export action");
    assert_eq!(action.category, "File");
    assert_eq!(action.label, "Export project MIDI...");
    assert_eq!(action.shortcut, None);

    let _ = app.update(Message::ExecuteCommand(CommandId::ExportProjectMidi));
    assert!(app.midi_export_window_id.is_some());
    assert!(app.midi_export_dialog.is_some());
    assert!(!app.path_picker_busy);
    assert_eq!(app.active_menu, None);

    let _ = app.update(Message::Escape);
    assert!(app.midi_export_window_id.is_none());
    assert!(app.midi_export_dialog.is_none());
}

#[test]
fn midi_export_marker_options_are_inert_for_an_empty_project() {
    let mut app = App::default();
    let _ = app.update(Message::OpenMidiExportDialog);

    let _ = app.update(Message::MidiExportMarkersChanged(true));
    let _ = app.update(Message::MidiExportMarkerTypeChanged(
        aaadaw_core::MidiMarkerType::Cues,
    ));
    let _ = app.update(Message::MidiExportHashOnlyChanged(true));

    let dialog = app.midi_export_dialog.as_ref().unwrap();
    assert!(!dialog.export_markers);
    assert_eq!(dialog.marker_type, aaadaw_core::MidiMarkerType::Markers);
    assert!(!dialog.only_markers_starting_with_hash);
    assert!(!dialog.export_hidden_markers);
}

#[test]
fn track_controls_and_undo_change_project_only_through_actions() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();

    let _ = app.update(Message::ToggleMute(track_id));
    let _ = app.update(Message::ToggleRecordArm(track_id));
    let _ = app.update(Message::TrackVolumeTextChanged(track_id, "-3.0".to_owned()));
    let _ = app.update(Message::CommitTrackVolumeText(track_id));
    assert!(app.project.tracks()[0].is_muted());
    assert!(app.project.tracks()[0].is_record_armed());
    assert_eq!(app.project.tracks()[0].volume_db(), -3.0);

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks()[0].volume_db(), 0.0);
    let _ = app.update(Message::Undo);
    assert!(!app.project.tracks()[0].is_record_armed());
    let _ = app.update(Message::Undo);
    assert!(!app.project.tracks()[0].is_muted());
}

#[test]
fn save_commits_pending_track_field_edits_as_one_undoable_action() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("track-fields.aaadaw");
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project_path = Some(path.clone());
    app.project_path_query = path.to_string_lossy().into_owned();
    app.saved_revision = app.revision;
    let original_revision = app.revision;

    let _ = app.update(Message::TrackNameChanged(track_id, "Lead Vox".to_owned()));
    let _ = app.update(Message::TrackVolumeTextChanged(track_id, "-6.0".to_owned()));
    let _ = app.update(Message::TrackPanTextChanged(track_id, "0.5".to_owned()));
    assert!(
        app.is_dirty(),
        "uncommitted field edits must mark the project dirty"
    );

    let _ = app.update(Message::SaveProject);

    let track = &app.project.tracks()[0];
    assert_eq!(track.name(), "Lead Vox");
    assert_eq!(track.volume_db(), -6.0);
    assert_eq!(track.pan(), 0.5);
    assert!(app.track_name_edits.is_empty());
    assert!(app.track_volume_edits.is_empty());
    assert!(app.track_pan_edits.is_empty());
    assert_eq!(app.revision, original_revision + 1);

    let revision = app.revision;
    let _ = app.update(Message::ProjectSaved(
        path,
        revision,
        Ok(()),
        None,
        SharedProjectSessionLock::new(None),
    ));
    assert!(!app.is_dirty());

    let _ = app.update(Message::Undo);
    let track = &app.project.tracks()[0];
    assert_eq!(track.name(), "Audio 1");
    assert_eq!(track.volume_db(), 0.0);
    assert_eq!(track.pan(), 0.0);
}

#[test]
fn save_rejects_invalid_track_draft_without_applying_other_pending_fields() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("invalid-track-fields.aaadaw");
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project_path = Some(path.clone());
    app.project_path_query = path.to_string_lossy().into_owned();
    app.saved_revision = app.revision;
    let original_revision = app.revision;

    let _ = app.update(Message::TrackNameChanged(track_id, "Lead Vox".to_owned()));
    let _ = app.update(Message::TrackVolumeTextChanged(
        track_id,
        "too loud".to_owned(),
    ));
    let _ = app.update(Message::SaveProject);

    assert_eq!(app.project.tracks()[0].name(), "Audio 1");
    assert_eq!(app.project.tracks()[0].volume_db(), 0.0);
    assert_eq!(app.revision, original_revision);
    assert!(!app.io_busy);
    assert!(app.is_dirty());
    assert_eq!(app.track_name_edits.get(&track_id).unwrap(), "Lead Vox");
    assert_eq!(app.track_volume_edits.get(&track_id).unwrap(), "too loud");
    assert!(
        app.track_draft_errors
            .contains_key(&(track_id, super::TrackDraftField::Volume))
    );
}

#[test]
fn escape_cancels_the_active_track_field_draft() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.saved_revision = app.revision;
    let original_revision = app.revision;

    let _ = app.update(Message::TrackPanTextChanged(track_id, "0.75".to_owned()));
    assert!(app.is_dirty());
    let _ = app.update(Message::Escape);

    assert!(app.track_pan_edits.is_empty());
    assert_eq!(app.project.tracks()[0].pan(), 0.0);
    assert_eq!(app.revision, original_revision);
    assert!(!app.is_dirty());
}

#[test]
fn track_mix_slider_reports_when_it_discards_a_text_draft() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();

    let _ = app.update(Message::TrackVolumeTextChanged(track_id, "-9".to_owned()));
    let _ = app.update(Message::PreviewTrackVolume(track_id, -3.0));

    assert!(!app.track_volume_edits.contains_key(&track_id));
    assert!(app.status.contains("draft discarded by slider adjustment"));
}

#[test]
fn item_drag_obeys_project_busy_and_jack_edit_guards() {
    assert_eq!(
        super::item_drag_edit_guard_status(false, false, false, false, true, false),
        Some("Close audio output before editing the project")
    );

    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = app.project.midi_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.import_busy = true;
    app.timeline
        .handle(crate::timeline::TimelineEvent::BeginItemDrag {
            item_id,
            pointer_delta_ticks: 240,
            target_track_index: Some(0),
            range: false,
            ignore_snap: false,
            copy: false,
        });
    assert!(app.timeline.drag_preview().is_some());

    let _ = app.update(Message::Timeline(
        crate::timeline::TimelineEvent::EndItemDrag,
    ));

    assert!(app.timeline.drag_preview().is_none());
    assert_eq!(app.project.midi_items()[0].start_tick(), 0);
    assert_eq!(app.status, "Wait for audio import to finish or cancel it");
}

#[test]
fn stopped_or_paused_open_playback_does_not_lock_project_editing() {
    let stopped = super::playback_prevents_project_edits(true, false);
    let playing = super::playback_prevents_project_edits(true, true);

    assert!(!stopped);
    assert!(playing);
    assert!(!super::playback_prevents_project_edits(false, false));
    assert_eq!(
        super::item_drag_edit_guard_status(false, false, false, false, stopped, false),
        None
    );
    assert_eq!(
        super::item_drag_edit_guard_status(false, false, false, false, playing, false),
        Some("Close audio output before editing the project")
    );
}

#[cfg(feature = "audio-device")]
#[test]
fn stopped_edit_cursor_sets_next_playback_position_without_seeking_active_transport() {
    let mut app = App::default();
    let stopped_tick = 1_920;
    let stopped_sample = app.project.sample_at_tick(stopped_tick).unwrap();

    let _ = app.update(Message::Timeline(
        super::timeline::TimelineEvent::SelectEmpty(stopped_tick),
    ));

    assert_eq!(app.timeline.edit_cursor_tick, stopped_tick);
    assert_eq!(app.playhead_sample, stopped_sample);
    assert_eq!(app.playback_start_sample, stopped_sample);

    app.playback_paused = true;
    app.playhead_sample = 12_345;
    app.playback_start_sample = 6_789;
    let paused_tick = 2_880;
    let _ = app.update(Message::Timeline(
        super::timeline::TimelineEvent::SetEditCursor(paused_tick),
    ));

    assert_eq!(app.timeline.edit_cursor_tick, paused_tick);
    assert_eq!(app.playhead_sample, 12_345);
    assert_eq!(app.playback_start_sample, 6_789);

    app.playback_paused = false;
    app.playback_playing = true;
    app.playhead_sample = 23_456;
    app.playback_start_sample = 18_765;
    let playing_tick = 3_840;
    let _ = app.update(Message::Timeline(
        super::timeline::TimelineEvent::SetEditCursor(playing_tick),
    ));

    assert_eq!(app.timeline.edit_cursor_tick, playing_tick);
    assert_eq!(app.playhead_sample, 23_456);
    assert_eq!(app.playback_start_sample, 18_765);
}

#[test]
fn reimport_requires_a_saved_project_and_changed_embedded_source() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "embedded-source".to_owned(),
            start_sample: 120,
            source_offset_samples: 8,
            length_samples: 1_000,
        })
        .unwrap();
    let item_id = app.project.audio_items()[0].id();
    app.project_path = Some(std::path::PathBuf::from("project.aaadaw"));
    app.audio_asset_source_statuses.insert(
        "embedded-source".to_owned(),
        aaadaw_app::AudioAssetSourceStatusEntry {
            media_ref: "embedded-source".to_owned(),
            status: aaadaw_app::AudioAssetSourceStatus::Changed,
            is_external_link: false,
        },
    );
    assert!(app.is_dirty());

    let _ = app.update(Message::ReimportAudioItem(item_id));

    assert!(!app.import_busy);
    assert!(app.status.contains("Save the project"));
    assert_eq!(app.project.audio_items()[0].media_ref(), "embedded-source");
}

#[test]
fn action_search_dispatches_supported_commands() {
    let mut app = App::default();
    let _ = app.update(Message::ActionQueryChanged("add track".to_owned()));
    let _ = app.update(Message::RunActionQuery);
    assert_eq!(app.project.tracks().len(), 1);

    let _ = app.update(Message::ActionQueryChanged("undo".to_owned()));
    let _ = app.update(Message::RunActionQuery);
    assert!(app.project.tracks().is_empty());

    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::ActionQueryChanged("insert MIDI item".to_owned()));
    let _ = app.update(Message::RunActionQuery);
    assert_eq!(app.project.midi_items().len(), 1);
    let _ = app.update(Message::ActionQueryChanged("undo".to_owned()));
    let _ = app.update(Message::RunActionQuery);
    assert!(app.project.midi_items().is_empty());
}

#[test]
fn menu_keyboard_uses_native_alt_mnemonics_and_accepts_enter_while_actions_search_has_focus() {
    let window_id = iced::window::Id::unique();
    let f10 = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: Key::Named(iced::keyboard::key::Named::F10),
        modified_key: Key::Named(iced::keyboard::key::Named::F10),
        physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::F10),
        location: iced::keyboard::Location::Standard,
        modifiers: Modifiers::NONE,
        text: None,
        repeat: false,
    });
    assert!(
        menu_navigation_event(
            &f10,
            iced::event::Status::Ignored,
            window_id,
            Some(window_id),
            None,
        )
        .is_none()
    );

    let mut app = App {
        main_window_id: Some(window_id),
        ..App::default()
    };
    for (key, expected) in [
        ("a", MainMenu::Actions),
        ("e", MainMenu::Edit),
        ("f", MainMenu::File),
        ("h", MainMenu::Help),
        ("i", MainMenu::Insert),
        ("o", MainMenu::Options),
        ("v", MainMenu::View),
    ] {
        let event = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: Key::Character(key.into()),
            modified_key: Key::Character(key.into()),
            physical_key: iced::keyboard::key::Physical::Unidentified(
                iced::keyboard::key::NativeCode::Unidentified,
            ),
            location: iced::keyboard::Location::Standard,
            modifiers: Modifiers::ALT,
            text: None,
            repeat: false,
        });
        let message = menu_navigation_event(
            &event,
            iced::event::Status::Ignored,
            window_id,
            Some(window_id),
            None,
        )
        .unwrap();
        assert!(matches!(
            message,
            Message::MenuKeyboard(MenuNavigation::OpenMenu(menu)) if menu == expected
        ));
        let _ = app.update(message);
        assert_eq!(app.active_menu, Some(expected));
    }

    let file = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: Key::Character("f".into()),
        modified_key: Key::Character("f".into()),
        physical_key: iced::keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        location: iced::keyboard::Location::Standard,
        modifiers: Modifiers::ALT,
        text: None,
        repeat: false,
    });
    let _ = app.update(
        menu_navigation_event(
            &file,
            iced::event::Status::Ignored,
            window_id,
            Some(window_id),
            app.active_menu,
        )
        .unwrap(),
    );
    assert_eq!(app.active_menu, Some(MainMenu::File));

    assert!(
        menu_navigation_event(
            &file,
            iced::event::Status::Captured,
            window_id,
            Some(window_id),
            None,
        )
        .is_none()
    );

    let right = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: Key::Named(iced::keyboard::key::Named::ArrowRight),
        modified_key: Key::Named(iced::keyboard::key::Named::ArrowRight),
        physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::ArrowRight),
        location: iced::keyboard::Location::Standard,
        modifiers: Modifiers::NONE,
        text: None,
        repeat: false,
    });
    assert!(matches!(
        menu_navigation_event(
            &right,
            iced::event::Status::Ignored,
            window_id,
            Some(window_id),
            Some(MainMenu::File),
        ),
        Some(Message::MenuKeyboard(MenuNavigation::NextMenu))
    ));

    let enter = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: Key::Named(iced::keyboard::key::Named::Enter),
        modified_key: Key::Named(iced::keyboard::key::Named::Enter),
        physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Enter),
        location: iced::keyboard::Location::Standard,
        modifiers: Modifiers::NONE,
        text: None,
        repeat: false,
    });
    assert!(matches!(
        menu_navigation_event(
            &enter,
            iced::event::Status::Captured,
            window_id,
            Some(window_id),
            Some(MainMenu::Actions),
        ),
        Some(Message::RunActionQuery)
    ));
}

#[test]
fn action_search_enter_executes_the_selected_visible_match() {
    let window_id = iced::window::Id::unique();
    let mut app = App {
        main_window_id: Some(window_id),
        ..App::default()
    };
    let _ = app.update(Message::ToggleMainMenu(MainMenu::Actions));
    let _ = app.update(Message::ActionQueryChanged("track".to_owned()));

    let _ = app.update(Message::RuntimeKeyboardEvent(
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: Key::Named(iced::keyboard::key::Named::Enter),
            modified_key: Key::Named(iced::keyboard::key::Named::Enter),
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Enter),
            location: iced::keyboard::Location::Standard,
            modifiers: Modifiers::NONE,
            text: None,
            repeat: false,
        }),
        iced::event::Status::Captured,
        window_id,
    ));

    assert_eq!(app.project.tracks().len(), 1);
    assert_eq!(app.active_menu, None);
}

#[test]
fn keyboard_navigation_switches_menus_and_wraps_enabled_commands() {
    let mut app = App::default();
    let _ = app.update(Message::MenuKeyboard(MenuNavigation::OpenMenu(
        MainMenu::File,
    )));
    assert_eq!(app.active_menu, Some(MainMenu::File));
    let enabled_commands = commands::for_menu(&app, MainMenu::File)
        .into_iter()
        .filter(|entry| entry.enabled)
        .map(|entry| entry.id)
        .collect::<Vec<_>>();
    let first = app.menu_selected_command;
    assert!(first.is_some());
    assert_eq!(first, enabled_commands.first().copied());

    let _ = app.update(Message::MenuKeyboard(MenuNavigation::PreviousCommand));
    assert_eq!(app.menu_selected_command, enabled_commands.last().copied());
    let _ = app.update(Message::MenuKeyboard(MenuNavigation::NextCommand));
    assert_eq!(app.menu_selected_command, first);

    let _ = app.update(Message::MenuKeyboard(MenuNavigation::NextCommand));
    assert_ne!(app.menu_selected_command, first);
    let _ = app.update(Message::MenuKeyboard(MenuNavigation::PreviousCommand));
    assert_eq!(app.menu_selected_command, first);

    let _ = app.update(Message::MenuKeyboard(MenuNavigation::NextMenu));
    assert_eq!(app.active_menu, Some(MainMenu::Edit));
    let _ = app.update(Message::MenuKeyboard(MenuNavigation::PreviousMenu));
    assert_eq!(app.active_menu, Some(MainMenu::File));
    for menu in [
        MainMenu::Edit,
        MainMenu::View,
        MainMenu::Insert,
        MainMenu::Item,
        MainMenu::Track,
        MainMenu::Options,
        MainMenu::Actions,
        MainMenu::Help,
    ] {
        let _ = app.update(Message::MenuKeyboard(MenuNavigation::NextMenu));
        assert_eq!(app.active_menu, Some(menu));
    }
    let _ = app.update(Message::Escape);
    assert_eq!(app.active_menu, None);
    assert_eq!(app.menu_selected_command, None);
}

#[test]
fn action_search_keyboard_selection_scrolls_past_the_visible_rows() {
    let app = App::default();
    let entries = super::commands::matching_actions_menu(&app, "");
    let selected = entries
        .iter()
        .rfind(|entry| entry.enabled)
        .expect("Actions menu has an enabled command")
        .id;

    assert!(
        super::actions_menu_selection_scroll_offset(&entries, selected, 0.0)
            .expect("selected command is in the Actions results")
            > 0.0
    );
}

#[test]
fn action_search_keeps_visible_rows_stationary_and_scrolls_at_viewport_edges() {
    let app = App::default();
    let entries = super::commands::matching_actions_menu(&app, "");
    let ids = entries
        .iter()
        .filter(|entry| entry.enabled)
        .map(|entry| entry.id)
        .collect::<Vec<_>>();
    let first = ids[0];
    let middle = ids[3];
    let last = *ids.last().expect("Actions menu has enabled commands");

    assert_eq!(
        super::actions_menu_selection_scroll_offset(&entries, first, 0.0),
        None
    );
    assert_eq!(
        super::actions_menu_selection_scroll_offset(&entries, middle, 0.0),
        None
    );
    assert_eq!(
        super::actions_menu_selection_scroll_offset(&entries, first, 150.0),
        Some(20.0)
    );
    let max_offset = super::actions_menu_selection_scroll_offset(&entries, last, 0.0)
        .expect("last result extends below the viewport");
    assert!(max_offset > 0.0);
    assert_eq!(
        super::actions_menu_selection_scroll_offset(&entries, last, max_offset),
        None
    );
}

#[test]
fn menu_search_and_navigation_stay_available_during_a_file_dialog() {
    let window_id = iced::window::Id::unique();
    let mut app = App {
        main_window_id: Some(window_id),
        path_picker_busy: true,
        ..App::default()
    };
    let _ = app.update(Message::ToggleMainMenu(MainMenu::Actions));
    let _ = app.update(Message::ActionQueryChanged(String::new()));
    let first = app.menu_selected_command;
    assert!(first.is_some());

    let _ = app.update(Message::RuntimeKeyboardEvent(
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: Key::Named(iced::keyboard::key::Named::ArrowDown),
            modified_key: Key::Named(iced::keyboard::key::Named::ArrowDown),
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::ArrowDown),
            location: iced::keyboard::Location::Standard,
            modifiers: Modifiers::NONE,
            text: None,
            repeat: false,
        }),
        iced::event::Status::Captured,
        window_id,
    ));
    assert_ne!(app.menu_selected_command, first);

    let _ = app.update(Message::RuntimeKeyboardEvent(
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: Key::Named(iced::keyboard::key::Named::Enter),
            modified_key: Key::Named(iced::keyboard::key::Named::Enter),
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Enter),
            location: iced::keyboard::Location::Standard,
            modifiers: Modifiers::NONE,
            text: None,
            repeat: false,
        }),
        iced::event::Status::Captured,
        window_id,
    ));
    assert!(app.project.tracks().is_empty());
    assert_eq!(app.active_menu, Some(MainMenu::Actions));
    assert_eq!(app.status, "Wait for the file dialog to finish");
}

#[test]
fn saved_macros_run_ordered_commands_from_actions_search_and_shortcuts() {
    let mut app = App::default();
    app.action_macros.push(super::action_macros::ActionMacro {
        id: 7,
        name: "Mixer then arrangement".to_owned(),
        steps: vec![
            "view.mixer-workspace".to_owned(),
            "view.arrangement-workspace".to_owned(),
        ],
    });
    *app.shortcut_bindings
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) =
        HashMap::from([(commands::macro_id(7), "Mod+M".to_owned())]);
    assert_eq!(
        commands::find(&app, "Mixer then arrangement"),
        Some(CommandId::Macro(7))
    );
    let _ = app.update(Message::ActionQueryChanged(
        "Mixer then arrangement".to_owned(),
    ));
    let _ = app.update(Message::RunActionQuery);
    assert_eq!(app.main_workspace, MainWorkspace::Arrangement);

    let _ = app.update(Message::ShortcutPressed(test_shortcut_input(
        Key::Character("m".into()),
        Modifiers::COMMAND,
    )));
    assert_eq!(app.main_workspace, MainWorkspace::Arrangement);

    app.action_macros[0].steps = vec![
        "view.mixer-workspace".to_owned(),
        "not-a-real-action".to_owned(),
    ];
    let _ = app.update(Message::ExecuteCommand(CommandId::Macro(7)));
    assert_eq!(app.main_workspace, MainWorkspace::Mixer);
    assert!(app.status.contains("unknown action at step 2"));
}

#[test]
fn malformed_macro_config_blocks_create_and_delete_overwrites() {
    let mut app = App {
        action_macro_config_error: Some("invalid macro config".to_owned()),
        action_macro_name: "Do not overwrite".to_owned(),
        action_macro_steps: vec!["view.mixer-workspace".to_owned()],
        ..App::default()
    };
    let _ = app.update(Message::SaveActionMacro);
    assert!(app.action_macros.is_empty());
    assert!(
        app.action_macro_feedback
            .contains("Fix the action macro config")
    );

    app.action_macros.push(super::action_macros::ActionMacro {
        id: 11,
        name: "Loaded before failure".to_owned(),
        steps: vec!["view.mixer-workspace".to_owned()],
    });
    let _ = app.update(Message::DeleteActionMacro(11));
    assert_eq!(app.action_macros.len(), 1);
    assert!(
        app.action_macro_feedback
            .contains("Fix the action macro config")
    );
}

#[cfg(feature = "audio-device")]
#[test]
fn midi_panic_without_an_open_output_reports_the_missing_backend() {
    let mut app = App::default();

    let panic_command = commands::for_actions_menu(&app)
        .into_iter()
        .find(|entry| entry.id == CommandId::PanicMidi)
        .expect("Actions menu should expose MIDI Panic");
    assert_eq!(panic_command.label, "MIDI Panic · release all notes");
    assert!(!panic_command.enabled);

    let _ = commands::dispatch(&mut app, CommandId::PanicMidi);

    assert!(app.status.contains("output is not open"));
}

#[test]
fn action_search_does_not_run_commands_that_are_disabled_in_the_menu() {
    let mut app = App::default();
    let _ = app.update(Message::ActionQueryChanged("import audio".to_owned()));
    let _ = app.update(Message::RunActionQuery);

    assert!(!app.path_picker_busy);
    assert_eq!(
        app.status,
        "Unknown action. Search for a command or choose one from the Actions menu."
    );
}

#[test]
fn action_search_routes_track_commands_through_the_existing_messages() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let first_id = app.project.tracks()[0].id();
    let second_id = app.project.tracks()[1].id();
    app.timeline.selected_track = Some(second_id);

    let _ = app.update(Message::ActionQueryChanged(
        "move selected track up".to_owned(),
    ));
    let _ = app.update(Message::RunActionQuery);
    assert_eq!(app.project.tracks()[0].id(), second_id);

    let _ = app.update(Message::ActionQueryChanged(
        "mute selected track".to_owned(),
    ));
    let _ = app.update(Message::RunActionQuery);
    assert!(app.project.tracks()[0].is_muted());

    let _ = app.update(Message::ActionQueryChanged(
        "delete selected track".to_owned(),
    ));
    let _ = app.update(Message::RunActionQuery);
    assert_eq!(app.project.tracks().len(), 1);
    assert_eq!(app.project.tracks()[0].id(), first_id);
}

#[test]
fn saving_an_untitled_project_opens_the_save_dialog() {
    let mut app = App::default();

    let _ = app.update(Message::SaveProject);

    assert!(app.path_picker_busy);
    assert_eq!(app.status, "");
}

#[test]
fn opening_missing_path_does_not_create_a_project_file() {
    let file_id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "aaadaw-ui-missing-{}-{file_id}.aaadaw",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);

    assert!(load_project_file(path.clone()).is_err());
    #[cfg(feature = "audio-device")]
    assert!(prepare_project_playback_file(path.clone(), Project::new().snapshot(), 0).is_err());
    assert!(!path.exists());
}

#[test]
fn an_open_desktop_project_excludes_an_mcp_writer_until_released() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("locked-session.aaadaw");
    let store = ProjectStore::open(&path).unwrap();
    store.close().unwrap();

    let (_, _, session_lock) = load_project_session(path.clone()).unwrap();
    let error = ProjectSessionLock::acquire(&path).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);

    drop(session_lock);
    ProjectSessionLock::acquire(path).unwrap();
}

#[cfg(unix)]
#[test]
fn saving_a_new_project_publishes_it_with_the_session_identity_lock_held() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("new-session.aaadaw");
    let mut session_lock = ProjectSessionLock::acquire(&path).unwrap();
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Lead Vocal".to_owned(),
        })
        .unwrap();

    save_project_session_file_with_media(
        path.clone(),
        project.snapshot(),
        ArrangementViewState::default(),
        false,
        Some(&mut session_lock),
        None,
    )
    .unwrap();

    let persisted = ProjectStore::load_read_only(&path).unwrap();
    assert_eq!(persisted.tracks()[0].name(), "Lead Vocal");
    let hard_link = directory.path().join("new-session-link.aaadaw");
    std::fs::hard_link(&path, &hard_link).unwrap();
    let error = ProjectSessionLock::acquire(&hard_link).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
    assert_eq!(
        std::fs::read_dir(directory.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with(".aaadaw-save-"))
            .count(),
        0
    );
}

#[cfg(unix)]
#[test]
fn first_save_migrates_unsaved_audio_assets_before_publishing_the_project() {
    use std::io::{Cursor, Read};

    let directory = tempfile::tempdir().unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    let source_path = source_dir.path().join("session.aaadaw");
    let destination = directory.path().join("first-save.aaadaw");
    let mut source_store = ProjectStore::open(&source_path).unwrap();
    source_store
        .import_audio_asset(
            "asset://unsaved-take",
            "take.wav",
            Cursor::new(b"audio created before first save"),
        )
        .unwrap();
    source_store.close().unwrap();

    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Unsaved audio".to_owned(),
        })
        .unwrap();
    let track_id = project.tracks()[0].id();
    project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://unsaved-take".to_owned(),
            start_sample: 120,
            source_offset_samples: 0,
            length_samples: 960,
        })
        .unwrap();
    let mut session_lock = ProjectSessionLock::acquire(&destination).unwrap();

    save_project_session_file_with_media(
        destination.clone(),
        project.snapshot(),
        ArrangementViewState::default(),
        false,
        Some(&mut session_lock),
        Some(source_path.clone()),
    )
    .unwrap();

    let destination_store = ProjectStore::open(&destination).unwrap();
    let persisted = destination_store.load().unwrap();
    assert_eq!(persisted.audio_items().len(), 1);
    assert_eq!(
        persisted.audio_items()[0].media_ref(),
        "asset://unsaved-take"
    );
    let aaadaw_storage::ResolvedAudioAsset::Embedded(mut reader) = destination_store
        .resolve_audio_asset("asset://unsaved-take")
        .unwrap()
    else {
        panic!("the migrated take should remain embedded");
    };
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"audio created before first save");
    drop(reader);
    destination_store.close().unwrap();

    let source_store = ProjectStore::open(source_path).unwrap();
    let aaadaw_storage::ResolvedAudioAsset::Embedded(mut reader) = source_store
        .resolve_audio_asset("asset://unsaved-take")
        .unwrap()
    else {
        panic!("the source take must remain available until migration succeeds");
    };
    let mut source_bytes = Vec::new();
    reader.read_to_end(&mut source_bytes).unwrap();
    assert_eq!(source_bytes, b"audio created before first save");
    drop(reader);
    source_store.close().unwrap();
}

#[test]
fn abandoned_unsaved_sessions_are_scanned_and_recovered_but_live_sessions_are_skipped() {
    let root = tempfile::tempdir().unwrap();
    let active_session = super::create_unsaved_project_session_dir_in(root.path()).unwrap();
    let active_directory = active_session.directory.clone();

    let abandoned_directory = root.path().join("session-abandoned");
    std::fs::create_dir(&abandoned_directory).unwrap();
    let abandoned_path = abandoned_directory.join("session.aaadaw");
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Recovered track".to_owned(),
        })
        .unwrap();
    super::project_io::save_unsaved_session_snapshot(
        abandoned_path.clone(),
        project.snapshot(),
        ArrangementViewState::default(),
    )
    .unwrap();

    let candidates = super::project_io::scan_unsaved_session_recoveries(
        root.path().to_path_buf(),
        active_directory,
    )
    .unwrap();
    assert_eq!(candidates, vec![abandoned_path.clone()]);

    let live_lock = ProjectSessionLock::acquire(&abandoned_path).unwrap();
    assert!(
        super::project_io::scan_unsaved_session_recoveries(
            root.path().to_path_buf(),
            active_session.directory.clone(),
        )
        .unwrap()
        .is_empty()
    );
    drop(live_lock);

    let (recovered, view_state, lock) = load_project_session(abandoned_path.clone()).unwrap();
    assert_eq!(recovered.tracks()[0].name(), "Recovered track");
    assert_eq!(view_state, Some(ArrangementViewState::default()));

    let mut app = App {
        session_media_dir: Some(active_session),
        unsaved_session_recovery_candidates: vec![abandoned_path.clone()],
        ..App::default()
    };
    let result = std::sync::Arc::new(std::sync::Mutex::new(Some(Ok((
        recovered, view_state, lock,
    )))));
    let _ = app.update(Message::UnsavedSessionRecovered(
        abandoned_path.clone(),
        result,
    ));
    assert_eq!(app.media_store_path(), Some(abandoned_path));
    assert_eq!(app.project.tracks()[0].name(), "Recovered track");
    assert!(app.is_dirty());
}

#[test]
fn edits_in_an_unsaved_session_schedule_a_recovery_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let session = super::create_unsaved_project_session_dir_in(directory.path()).unwrap();
    let mut app = App {
        session_media_dir: Some(session),
        ..App::default()
    };

    let _ = app.update(Message::AddTrack);

    assert!(app.unsaved_session_snapshot_at.is_some());
    assert_eq!(app.revision, 1);
}

#[cfg(unix)]
#[test]
fn project_session_round_trip_restores_arrangement_view_state() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("automation-view.aaadaw");
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Automated".to_owned(),
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
        volume_lanes: vec![aaadaw_storage::VolumeAutomationLaneViewState {
            track_id: track_id.value(),
            visible: false,
        }],
        fx_lanes: vec![aaadaw_storage::FxAutomationLaneViewState {
            track_id: track_id.value(),
            chain_index: 0,
            plugin_id: "vendor.eq".to_owned(),
            bundle_path: "/plugins/eq.clap".to_owned(),
            parameter_id: 7,
            name: "Mix".to_owned(),
            value_range: (0.0, 1.0),
            stepped: false,
            height: 48.0,
        }],
        time_selection: Some(aaadaw_storage::ArrangementTimeRangeState {
            start_seconds: 0.5,
            end_seconds: 1.0,
        }),
        loop_points: Some(aaadaw_storage::ArrangementTimeRangeState {
            start_seconds: 1.5,
            end_seconds: 2.0,
        }),
    };
    let mut session_lock = ProjectSessionLock::acquire(&path).unwrap();
    save_project_session_file_with_media(
        path.clone(),
        project.snapshot(),
        view_state.clone(),
        false,
        Some(&mut session_lock),
        None,
    )
    .unwrap();
    drop(session_lock);

    let (loaded_project, loaded_view_state, _session_lock) = load_project_session(path).unwrap();
    assert_eq!(loaded_project.tracks()[0].id(), track_id);
    assert_eq!(loaded_view_state, Some(view_state));

    let mut timeline = crate::timeline::TimelineState::default();
    timeline.replace_project(&loaded_project, loaded_view_state.as_ref());
    let row = timeline.row_layout(0).unwrap();
    assert_eq!(row.fx_lane_count, 1);
    assert_eq!(row.height, crate::timeline::TIMELINE_ROW_HEIGHT + 48.0);
    assert_eq!(
        timeline.time_selection,
        Some(crate::timeline::TimeSelection {
            start_tick: 960,
            end_tick: 1_920,
        })
    );
    assert_eq!(
        timeline.loop_points,
        Some(crate::timeline::LoopPoints {
            start_tick: 2_880,
            end_tick: 3_840,
        })
    );
}

#[cfg(feature = "audio-device")]
#[test]
fn jack_feature_prepares_offline_graph_without_opening_device() {
    let file_id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "aaadaw-ui-playback-{}-{file_id}.aaadaw",
        std::process::id()
    ));
    let store = ProjectStore::open(&path).expect("empty project store should open");
    store.close().expect("empty project store should close");

    let prepared = prepare_project_playback_file(path.clone(), Project::new().snapshot(), 0)
        .expect("empty project should prepare without a JACK device");
    assert_eq!(prepared.feeder_count(), 0);
    drop(prepared);

    let _ = std::fs::remove_file(&path);
    for suffix in ["-wal", "-shm"] {
        let sidecar = format!("{}{suffix}", path.display());
        let _ = std::fs::remove_file(sidecar);
    }
}

#[cfg(feature = "audio-device")]
#[test]
fn missing_clap_effect_is_skipped_without_failing_playback_preparation() {
    let file_id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
    let project_path = std::env::temp_dir().join(format!(
        "aaadaw-ui-fx-playback-{}-{file_id}.aaadaw",
        std::process::id()
    ));
    let store = ProjectStore::open(&project_path).expect("empty project store should open");
    store.close().expect("empty project store should close");

    let mut app = App::default();
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Effects".to_owned(),
        })
        .expect("track should be created");
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::SetTrackFxChain {
            track_id,
            plugins: vec![
                TrackFxPlugin::new("org.example.missing", "/missing/test.clap")
                    .expect("plugin reference should be valid"),
            ],
        })
        .expect("effect should be assigned");

    let mut prepared =
        prepare_project_playback_file(project_path.clone(), app.project.snapshot(), 0)
            .expect("empty media graph should prepare");
    let owners = app
        .install_track_fx_processors(&mut prepared)
        .expect("a missing effect should not block playback preparation");
    assert!(owners.is_empty());
    assert!(
        app.clap_plugin_warnings
            .iter()
            .any(|warning| warning.contains("org.example.missing") && warning.contains("skipped"))
    );
    assert!(app.clap_effect_owners.is_empty());

    drop(prepared);
    let _ = std::fs::remove_file(&project_path);
    for suffix in ["-wal", "-shm"] {
        let sidecar = format!("{}{suffix}", project_path.display());
        let _ = std::fs::remove_file(sidecar);
    }
}

#[test]
fn time_selection_split_without_selected_items_splits_all_tracks_once() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let track_ids = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    for track_id in &track_ids {
        app.project
            .apply(DawAction::InsertMidiItem {
                track_id: *track_id,
                start_tick: 0,
                length_ticks: 3_840,
            })
            .unwrap();
    }
    app.timeline.rebuild(&app.project);
    app.timeline
        .handle(crate::timeline::TimelineEvent::SetTimeSelection {
            start_tick: 960,
            end_tick: 2_880,
        });
    assert!(app.timeline.selected_items.is_empty());
    assert!(commands::is_enabled(
        &app,
        CommandId::SplitSelectedItemsAtTimeSelection
    ));
    let before = app.project.snapshot();
    let revision = app.revision;

    let _ = app.update(Message::SplitSelectedItemsAtTimeSelection);

    assert_eq!(app.revision, revision + 1);
    assert_eq!(
        app.project
            .midi_items()
            .iter()
            .map(|item| (item.start_tick(), item.length_ticks()))
            .collect::<Vec<_>>(),
        [
            (0, 960),
            (960, 1_920),
            (2_880, 960),
            (0, 960),
            (960, 1_920),
            (2_880, 960)
        ]
    );
    assert_eq!(app.timeline.selected_items.len(), 2);
    assert!(app.timeline.selected_items.iter().all(|item_id| {
        app.project.midi_items().iter().any(|item| {
            item.id() == *item_id && item.start_tick() == 960 && item.length_ticks() == 1_920
        })
    }));
    let after = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after);
}

#[test]
fn razor_area_split_is_track_scoped_consumes_areas_and_undoes_as_one_edit() {
    let file_id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
    let project_path = std::env::temp_dir().join(format!(
        "aaadaw-ui-razor-split-{}-{file_id}.aaadaw",
        std::process::id()
    ));
    let media_ref = "asset://razor-split-test";
    let mut store = ProjectStore::open(&project_path).expect("project store should open");
    store
        .import_audio_asset(
            media_ref,
            "razor-split-test.wav",
            std::io::Cursor::new([0_u8; 4]),
        )
        .expect("test asset should import");
    store
        .set_audio_asset_metadata(
            media_ref,
            &aaadaw_storage::AudioAssetMetadata {
                container: "wav".to_owned(),
                codec: "pcm".to_owned(),
                sample_rate: Some(44_100),
                channel_count: Some(1),
                bits_per_sample: Some(16),
                frame_count: Some(100_000),
                duration_nanos: None,
                byte_len: Some(4),
            },
        )
        .expect("test metadata should store");
    store.close().expect("project store should close");

    let mut app = App {
        project_path: Some(project_path.clone()),
        ..App::default()
    };
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let track_ids = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    let item_end = app.project.sample_at_tick(3_840).unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: track_ids[0],
            media_ref: media_ref.to_owned(),
            start_sample: 0,
            source_offset_samples: 5_000,
            length_samples: item_end,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: track_ids[0],
            start_tick: 0,
            length_ticks: 3_840,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: track_ids[1],
            start_tick: 0,
            length_ticks: 3_840,
        })
        .unwrap();
    let area = RazorEditArea::new(
        app.project.sample_at_tick(960).unwrap(),
        app.project.sample_at_tick(2_880).unwrap(),
    )
    .unwrap();
    app.project
        .apply(DawAction::SetTrackRazorEditAreas {
            track_areas: vec![(track_ids[0], vec![area])],
        })
        .unwrap();
    app.timeline.rebuild(&app.project);
    assert!(app.timeline.selected_items.is_empty());
    assert!(commands::is_enabled(
        &app,
        CommandId::SplitSelectedItemsAtTimeSelection
    ));
    let before = app.project.snapshot();
    let revision = app.revision;

    let _ = app.update(Message::SplitSelectedItemsAtTimeSelection);

    assert_eq!(app.revision, revision + 1);
    assert!(app.project.tracks()[0].razor_edit_areas().is_empty());
    assert_eq!(app.project.tracks()[1].razor_edit_areas(), &[]);
    let audio = app.project.audio_items();
    assert_eq!(audio.len(), 3);
    let first_cut = app.project.sample_at_tick(960).unwrap();
    let second_cut = app.project.sample_at_tick(2_880).unwrap();
    assert_eq!(audio[0].length_samples(), first_cut);
    assert_eq!(audio[1].start_sample(), first_cut);
    assert_eq!(audio[1].length_samples(), second_cut - first_cut);
    assert_eq!(
        audio[1].source_offset_samples(),
        i64::try_from(5_000 + (first_cut * 44_100 + 24_000) / 48_000).unwrap()
    );
    assert_eq!(audio[2].start_sample(), second_cut);
    assert_eq!(
        audio[2].source_offset_samples(),
        i64::try_from(5_000 + (second_cut * 44_100 + 24_000) / 48_000).unwrap()
    );
    assert_eq!(
        app.project
            .midi_items()
            .iter()
            .map(|item| (item.track_id(), item.start_tick(), item.length_ticks()))
            .collect::<Vec<_>>(),
        [
            (track_ids[0], 0, 960),
            (track_ids[0], 960, 1_920),
            (track_ids[0], 2_880, 960),
            (track_ids[1], 0, 3_840),
        ]
    );
    assert_eq!(app.timeline.selected_items.len(), 2);
    let after = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after);

    let _ = std::fs::remove_file(&project_path);
    for suffix in ["-wal", "-shm"] {
        let sidecar = format!("{}{suffix}", project_path.display());
        let _ = std::fs::remove_file(sidecar);
    }
}

#[cfg(feature = "audio-device")]
#[test]
fn missing_clap_instrument_is_skipped_without_failing_playback_preparation() {
    let file_id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
    let project_path = std::env::temp_dir().join(format!(
        "aaadaw-ui-instrument-playback-{}-{file_id}.aaadaw",
        std::process::id()
    ));
    let store = ProjectStore::open(&project_path).expect("empty project store should open");
    store.close().expect("empty project store should close");

    let mut app = App::default();
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "MIDI".to_owned(),
        })
        .expect("track should be created");
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::SetTrackInstrument {
            track_id,
            instrument: aaadaw_core::TrackInstrument::new(
                "org.example.missing-synth",
                "/missing/test-synth.clap",
            ),
        })
        .expect("instrument should be assigned");

    let mut prepared =
        prepare_project_playback_file(project_path.clone(), app.project.snapshot(), 0)
            .expect("empty media graph should prepare");
    let owners = app
        .install_track_instrument_processors(&mut prepared)
        .expect("a missing instrument should not block playback preparation");
    assert!(owners.is_empty());
    assert!(app.clap_plugin_warnings.iter().any(|warning| {
        warning.contains("org.example.missing-synth") && warning.contains("silent")
    }));
    assert!(app.clap_instrument_owners.is_empty());

    drop(prepared);
    let _ = std::fs::remove_file(&project_path);
    for suffix in ["-wal", "-shm"] {
        let sidecar = format!("{}{suffix}", project_path.display());
        let _ = std::fs::remove_file(sidecar);
    }
}

#[test]
fn midi_items_follow_the_edit_cursor_and_require_a_track() {
    let mut app = App::default();
    let _ = app.update(Message::AddMidiItem);
    assert!(app.project.midi_items().is_empty());
    assert_eq!(
        app.status,
        "Edit failed: add a track before creating a MIDI item"
    );

    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddMidiItem);
    app.timeline.edit_cursor_tick = 9_600;
    let _ = app.update(Message::AddMidiItem);
    assert_eq!(app.project.midi_items()[0].start_tick(), 0);
    assert_eq!(app.project.midi_items()[1].start_tick(), 9_600);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items().len(), 1);
}

#[test]
fn midi_item_is_inserted_on_the_selected_track_at_the_edit_cursor() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    app.project
        .apply(DawAction::CreateTrack {
            index: 1,
            name: "MIDI 2".to_owned(),
        })
        .expect("second track should be created");
    let selected_track = app.project.tracks()[1].id();
    app.timeline.selected_track = Some(selected_track);
    app.timeline.edit_cursor_tick = 9_600;

    let _ = app.update(Message::AddMidiItem);

    let midi_item = &app.project.midi_items()[0];
    assert_eq!(midi_item.track_id(), selected_track);
    assert_eq!(midi_item.start_tick(), 9_600);
    assert_eq!(midi_item.length_ticks(), 3_840);
}

#[test]
fn midi_item_uses_the_active_time_selection_range() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    app.timeline.edit_cursor_tick = 9_600;
    app.timeline.time_selection = Some(super::super::timeline::TimeSelection {
        start_tick: 4_800,
        end_tick: 7_200,
    });

    let _ = app.update(Message::AddMidiItem);

    let midi_item = &app.project.midi_items()[0];
    assert_eq!(midi_item.start_tick(), 9_600);
    assert_eq!(midi_item.length_ticks(), 2_400);
}

#[test]
fn midi_items_cannot_be_inserted_on_bus_tracks() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    app.project
        .apply(DawAction::CreateBusTrack {
            index: 1,
            name: "Bus 1".to_owned(),
        })
        .expect("bus should be created");
    let bus_id = app.project.tracks()[1].id();
    app.timeline.selected_track = Some(bus_id);

    assert!(!commands::is_enabled(&app, CommandId::AddMidiItem));
    let _ = app.update(Message::AddMidiItem);

    assert!(app.project.midi_items().is_empty());
    assert_eq!(
        app.status,
        "Select a regular track before creating a MIDI item"
    );
}

#[test]
fn midi_item_creation_and_note_editing_use_undoable_actions() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddMidiItem);
    let midi_item = &app.project.midi_items()[0];
    let item_id = midi_item.id();
    assert_eq!(midi_item.start_tick(), 0);
    assert_eq!(midi_item.length_ticks(), 3_840);
    let _ = app.update(Message::NudgeMidiItem(item_id, 1));
    assert_eq!(app.project.midi_items()[0].start_tick(), 960);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0].start_tick(), 0);

    let _ = app.update(Message::AddMidiNote(item_id));
    let note = &app.project.midi_items()[0].notes()[0];
    let note_id = note.id();
    assert_eq!(
        (note.pitch(), note.tick(), note.duration(), note.velocity()),
        (60, 0, 960, 100)
    );
    for _ in 0..3 {
        let _ = app.update(Message::AddMidiNote(item_id));
    }
    assert_eq!(
        app.project.midi_items()[0]
            .notes()
            .iter()
            .map(|note| note.tick())
            .collect::<Vec<_>>(),
        [0, 960, 1_920, 2_880]
    );
    let revision = app.revision;
    let _ = app.update(Message::AddMidiNote(item_id));
    assert_eq!(app.project.midi_items()[0].notes().len(), 4);
    assert_eq!(app.revision, revision);

    app.project
        .apply(DawAction::EditMidiNote {
            item_id,
            note_id,
            data: MidiNoteData {
                pitch: 60,
                tick: 40,
                duration: 960,
                velocity: 100,
            },
        })
        .expect("test note should move off-grid");
    let _ = app.update(Message::QuantizeMidiItem(item_id));
    assert_eq!(app.project.midi_items()[0].notes()[0].tick(), 0);
    let _ = app.update(Message::NudgeMidiNote(item_id, note_id, 1));
    assert_eq!(app.project.midi_items()[0].notes()[0].tick(), 240);

    let _ = app.update(Message::AdjustMidiNotePitch(item_id, note_id, 1));
    assert_eq!(app.project.midi_items()[0].notes()[0].pitch(), 61);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0].notes()[0].pitch(), 60);

    assert_eq!(app.project.midi_items()[0].notes()[0].tick(), 240);
    let _ = app.update(Message::AdjustMidiNoteVelocity(item_id, note_id, 5));
    assert_eq!(app.project.midi_items()[0].notes()[0].velocity(), 105);

    let _ = app.update(Message::DeleteMidiNote(item_id, note_id));
    assert_eq!(app.project.midi_items()[0].notes().len(), 3);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0].notes().len(), 4);
    assert_eq!(app.project.midi_items()[0].notes()[0].id(), note_id);
    let _ = app.update(Message::DeleteMidiItem(item_id));
    assert!(app.project.midi_items().is_empty());
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0].notes().len(), 4);
}

#[test]
fn piano_roll_copy_drag_adds_fresh_notes_and_undoes_as_one_action() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddMidiItem);
    let item_id = app.project.midi_items()[0].id();
    let _ = app.update(Message::OpenMidiEditor(item_id));
    let _ = app.update(Message::AddMidiNoteAt(
        item_id,
        MidiNoteData {
            pitch: 60,
            tick: 240,
            duration: 240,
            velocity: 90,
        },
    ));
    let source_id = app.project.midi_items()[0].notes()[0].id();
    let source = app.project.midi_items()[0].notes()[0].clone();
    app.midi_editor_selected_notes = HashSet::from([source_id]);
    let _ = app.update(Message::SetPianoRollCursor(item_id, 360));
    assert!(app.midi_editor_selected_notes.is_empty());

    let _ = app.update(Message::CopyDragMidiNotes(
        item_id,
        vec![MidiNoteData {
            pitch: 61,
            tick: 480,
            duration: 240,
            velocity: 90,
        }],
    ));

    let notes = app.project.midi_items()[0].notes();
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0], source);
    assert_eq!(notes[1].pitch(), 61);
    assert_eq!(notes[1].tick(), 480);
    assert_ne!(notes[1].id(), source_id);
    assert_eq!(
        app.midi_editor_selected_notes,
        HashSet::from([notes[1].id()])
    );
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.project.midi_items()[0].notes(),
        std::slice::from_ref(&source)
    );
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.midi_items()[0].notes().len(), 2);
    assert_eq!(app.project.midi_items()[0].notes()[0], source);
}

#[test]
fn midi_editor_fit_uses_only_visible_notes_after_start_trim() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddMidiItem);
    let item_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::EditMidiItem {
            item_id,
            start_tick: 0,
            length_ticks: 5_000,
        })
        .unwrap();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![
                MidiNoteData {
                    pitch: 20,
                    tick: 300,
                    duration: 240,
                    velocity: 90,
                },
                MidiNoteData {
                    pitch: 64,
                    tick: 1_200,
                    duration: 240,
                    velocity: 100,
                },
                MidiNoteData {
                    pitch: 120,
                    tick: 0,
                    duration: 120,
                    velocity: 100,
                },
            ],
        })
        .unwrap();
    app.project
        .apply(DawAction::TrimMidiItemStart {
            item_id,
            start_tick: 480,
            length_ticks: 3_360,
            source_offset_ticks: 480,
        })
        .unwrap();

    let _ = app.update(Message::OpenMidiEditor(item_id));

    assert_eq!(app.midi_editor_origin_tick, 480);
    assert_eq!(app.midi_editor_high_pitch, 66);
    assert_eq!(app.midi_editor_pitch_rows, 5);
}

#[test]
fn piano_roll_copy_paste_and_velocity_edits_are_grouped_undoable_actions() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddMidiItem);
    let item_id = app.project.midi_items()[0].id();
    let _ = app.update(Message::OpenMidiEditor(item_id));
    let _ = app.update(Message::AddMidiNoteAt(
        item_id,
        MidiNoteData {
            pitch: 60,
            tick: 240,
            duration: 240,
            velocity: 90,
        },
    ));
    let _ = app.update(Message::AddMidiNoteAt(
        item_id,
        MidiNoteData {
            pitch: 64,
            tick: 960,
            duration: 480,
            velocity: 110,
        },
    ));
    let source_ids = app.project.midi_items()[0]
        .notes()
        .iter()
        .map(|note| note.id())
        .collect::<Vec<_>>();

    let _ = app.update(Message::CopyMidiNotes(item_id, source_ids.clone()));
    assert_eq!(
        app.midi_editor_feedback.as_deref(),
        Some("Copied 2 MIDI notes")
    );
    let _ = app.update(Message::PasteMidiNotes(item_id));
    assert!(app.midi_editor_feedback.is_none());
    let notes = app.project.midi_items()[0].notes();
    assert_eq!(notes.len(), 4);
    assert_eq!(notes[2].tick(), 1_440);
    assert_eq!(notes[3].tick(), 2_160);
    assert_ne!(notes[2].id(), source_ids[0]);
    assert_eq!(
        app.midi_editor_selected_notes,
        HashSet::from([notes[2].id(), notes[3].id()])
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0].notes().len(), 2);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.midi_items()[0].notes().len(), 4);
    let _ = app.update(Message::PasteMidiNotes(item_id));
    assert_eq!(app.project.midi_items()[0].notes()[4].tick(), 2_640);
    assert_eq!(app.project.midi_items()[0].notes()[5].tick(), 3_360);

    let pasted = app.project.midi_items()[0].notes()[4..]
        .iter()
        .map(|note| {
            (
                note.id(),
                MidiNoteData {
                    pitch: note.pitch(),
                    tick: note.tick(),
                    duration: note.duration(),
                    velocity: 100,
                },
            )
        })
        .collect::<Vec<_>>();
    let _ = app.update(Message::EditMidiNotes(item_id, pasted));
    assert!(
        app.project.midi_items()[0].notes()[4..]
            .iter()
            .all(|note| note.velocity() == 100)
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0].notes()[4].velocity(), 90);
    assert_eq!(app.project.midi_items()[0].notes()[5].velocity(), 110);
    let _ = app.update(Message::Redo);
    assert!(
        app.project.midi_items()[0].notes()[4..]
            .iter()
            .all(|note| note.velocity() == 100)
    );
}

#[test]
fn piano_roll_copying_a_late_phrase_pastes_after_it_instead_of_near_item_start() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddMidiItem);
    let item_id = app.project.midi_items()[0].id();
    let _ = app.update(Message::OpenMidiEditor(item_id));
    let _ = app.update(Message::AddMidiNoteAt(
        item_id,
        MidiNoteData {
            pitch: 60,
            tick: 2_880,
            duration: 120,
            velocity: 96,
        },
    ));
    let source = &app.project.midi_items()[0].notes()[0];
    let _ = app.update(Message::CopyMidiNotes(item_id, vec![source.id()]));

    let _ = app.update(Message::PasteMidiNotes(item_id));

    let notes = app.project.midi_items()[0].notes();
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[1].tick(), 3_120);
}

#[test]
fn piano_roll_paste_uses_the_caret_and_duplicate_does_not_replace_the_clipboard() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddMidiItem);
    let item_id = app.project.midi_items()[0].id();
    let _ = app.update(Message::OpenMidiEditor(item_id));
    let _ = app.update(Message::AddMidiNoteAt(
        item_id,
        MidiNoteData {
            pitch: 60,
            tick: 2_880,
            duration: 120,
            velocity: 96,
        },
    ));
    let source_id = app.project.midi_items()[0].notes()[0].id();
    let _ = app.update(Message::CopyMidiNotes(item_id, vec![source_id]));
    assert_eq!(app.midi_editor_edit_cursor_tick, Some(3_120));

    let _ = app.update(Message::SetPianoRollCursor(item_id, 1_920));
    let _ = app.update(Message::PasteMidiNotes(item_id));
    assert_eq!(app.project.midi_items()[0].notes()[1].tick(), 1_920);

    let clipboard = app.midi_note_clipboard.notes.clone();
    let _ = app.update(Message::SelectMidiNotes(HashSet::from([source_id])));
    let _ = app.update(Message::DuplicateMidiNotes(item_id));
    assert_eq!(app.project.midi_items()[0].notes()[2].tick(), 3_120);
    assert_eq!(app.midi_note_clipboard.notes, clipboard);

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0].notes().len(), 2);
}

#[test]
fn piano_roll_cross_item_paste_uses_the_copied_phrase_position_when_no_caret_is_set() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddMidiItem);
    let source_item_id = app.project.midi_items()[0].id();
    let _ = app.update(Message::AddMidiItem);
    let target_item_id = app.project.midi_items()[1].id();
    let _ = app.update(Message::AddMidiItem);
    let explicit_target_item_id = app.project.midi_items()[2].id();
    let _ = app.update(Message::OpenMidiEditor(source_item_id));
    let _ = app.update(Message::AddMidiNoteAt(
        source_item_id,
        MidiNoteData {
            pitch: 60,
            tick: 2_880,
            duration: 120,
            velocity: 96,
        },
    ));
    let source_id = app.project.midi_items()[0].notes()[0].id();
    let _ = app.update(Message::CopyMidiNotes(source_item_id, vec![source_id]));
    let _ = app.update(Message::OpenMidiEditor(target_item_id));
    assert_eq!(app.midi_editor_edit_cursor_tick, None);

    let _ = app.update(Message::PasteMidiNotes(target_item_id));

    assert_eq!(app.project.midi_items()[1].notes().len(), 1);
    assert_eq!(app.project.midi_items()[1].notes()[0].tick(), 3_120);
    assert_eq!(app.midi_editor_edit_cursor_tick, Some(3_360));

    let _ = app.update(Message::OpenMidiEditor(explicit_target_item_id));
    let _ = app.update(Message::SetPianoRollCursor(explicit_target_item_id, 1_920));
    let _ = app.update(Message::PasteMidiNotes(explicit_target_item_id));
    assert_eq!(app.project.midi_items()[2].notes()[0].tick(), 1_920);
}

#[test]
fn piano_roll_paste_rejection_is_visible_as_editor_local_feedback() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddMidiItem);
    let item_id = app.project.midi_items()[0].id();
    let _ = app.update(Message::OpenMidiEditor(item_id));
    app.midi_editor_edit_cursor_tick = Some(3_600);
    app.midi_note_clipboard.notes = vec![MidiNoteData {
        pitch: 60,
        tick: 0,
        duration: 480,
        velocity: 96,
    }];
    app.status = "Main window status".to_owned();

    let _ = app.update(Message::PasteMidiNotes(item_id));

    assert_eq!(
        app.status,
        "Paste rejected: notes would extend beyond the MIDI item"
    );
    assert_eq!(
        app.midi_editor_feedback.as_deref(),
        Some("Paste rejected: notes would extend beyond the MIDI item")
    );
    assert!(app.project.midi_items()[0].notes().is_empty());
}

#[test]
fn piano_roll_paste_uses_its_own_grid_division_and_respects_shared_snap_off() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddMidiItem);
    let item_id = app.project.midi_items()[0].id();
    let _ = app.update(Message::OpenMidiEditor(item_id));
    let _ = app.update(Message::Timeline(TimelineEvent::SetMidiEditorSnapGrid(
        SnapGrid::EighthTriplet,
    )));
    app.midi_editor_edit_cursor_tick = Some(190);
    app.midi_note_clipboard.notes = vec![MidiNoteData {
        pitch: 60,
        tick: 0,
        duration: 120,
        velocity: 96,
    }];

    let _ = app.update(Message::PasteMidiNotes(item_id));
    assert_eq!(app.project.midi_items()[0].notes()[0].tick(), 320);

    let _ = app.update(Message::Timeline(TimelineEvent::ToggleMidiEditorSnap));
    app.midi_editor_edit_cursor_tick = Some(510);
    app.midi_note_clipboard.last_paste = None;
    let _ = app.update(Message::PasteMidiNotes(item_id));
    assert_eq!(app.project.midi_items()[0].notes()[1].tick(), 510);
}

#[test]
fn piano_roll_navigation_zoom_keeps_its_anchor_and_fit_frames_the_notes() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddMidiItem);
    let item_id = app.project.midi_items()[0].id();
    let _ = app.update(Message::AddMidiNoteAt(
        item_id,
        MidiNoteData {
            pitch: 60,
            tick: 960,
            duration: 240,
            velocity: 96,
        },
    ));
    let _ = app.update(Message::AddMidiNoteAt(
        item_id,
        MidiNoteData {
            pitch: 64,
            tick: 2_880,
            duration: 480,
            velocity: 96,
        },
    ));
    let _ = app.update(Message::OpenMidiEditor(item_id));

    assert_eq!(app.midi_editor_origin_tick, 720);
    assert_eq!(app.midi_editor_high_pitch, 66);
    assert!((app.midi_editor_pixels_per_beat - (884.0 / 3.0)).abs() < f32::EPSILON);

    let window_id = app.midi_editor_window_id.unwrap();
    let _ = app.update(Message::WindowResized(
        window_id,
        iced::Size::new(720.0, 420.0),
    ));
    let _ = app.update(Message::FitPianoRollToNotes(item_id));
    assert!((app.midi_editor_pixels_per_beat - (604.0 / 3.0)).abs() < f32::EPSILON);

    app.midi_editor_origin_tick = 1_000;
    app.midi_editor_pixels_per_beat = 96.0;
    let pointer_offset = 192.0_f64;
    let tick_before = app.midi_editor_origin_tick as f64
        + pointer_offset / f64::from(app.midi_editor_pixels_per_beat)
            * f64::from(app.project.settings().ppq());
    let _ = app.update(Message::PianoRollZoomAt(2.0, pointer_offset as f32));
    let tick_after = app.midi_editor_origin_tick as f64
        + pointer_offset / f64::from(app.midi_editor_pixels_per_beat)
            * f64::from(app.project.settings().ppq());
    assert!((tick_before - tick_after).abs() < 1.0);

    let _ = app.update(Message::PianoRollPanPixels(96.0));
    assert_eq!(app.midi_editor_origin_tick, 2_440);
}

#[test]
fn piano_roll_fit_includes_long_phrases_and_wide_pitch_ranges() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 100_000,
        })
        .expect("long MIDI source should be inserted");
    let item_id = app.project.midi_items()[0].id();
    for note in [
        MidiNoteData {
            pitch: 12,
            tick: 0,
            duration: 960,
            velocity: 96,
        },
        MidiNoteData {
            pitch: 110,
            tick: 40_000,
            duration: 960,
            velocity: 96,
        },
    ] {
        let _ = app.update(Message::AddMidiNoteAt(item_id, note));
    }

    let _ = app.update(Message::OpenMidiEditor(item_id));

    assert!(app.midi_editor_pixels_per_beat < 24.0);
    assert_eq!(app.midi_editor_high_pitch, 112);
    assert_eq!(app.midi_editor_pitch_rows, 103);
    assert!(app.midi_editor_pitch_row_height >= 2.0);
}

#[cfg(feature = "audio-device")]
#[test]
fn piano_roll_playhead_matches_arrangement_and_follow_keeps_it_in_view() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddMidiItem);
    let item_id = app.project.midi_items()[0].id();
    let _ = app.update(Message::OpenMidiEditor(item_id));
    let playhead_tick = 12_000;
    app.midi_editor_pixels_per_beat = 96.0;
    app.playhead_sample = app.project.sample_at_tick(playhead_tick).unwrap();
    app.playback_playing = true;
    app.midi_editor_follow_playhead = true;

    let item = &app.project.midi_items()[0];
    assert_eq!(app.midi_editor_playhead_tick(item), Some(playhead_tick));

    app.follow_midi_editor_playhead();

    assert_eq!(app.midi_editor_origin_tick, playhead_tick - 6_630);
}

#[test]
fn piano_roll_insert_move_resize_and_delete_use_project_actions() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddMidiItem);
    let item_id = app.project.midi_items()[0].id();

    let _ = app.update(Message::AddMidiNoteAt(
        item_id,
        MidiNoteData {
            pitch: 72,
            tick: 480,
            duration: 240,
            velocity: 96,
        },
    ));
    let note = &app.project.midi_items()[0].notes()[0];
    let note_id = note.id();
    assert_eq!((note.pitch(), note.tick(), note.duration()), (72, 480, 240));

    let _ = app.update(Message::EditMidiNotes(
        item_id,
        vec![(
            note_id,
            MidiNoteData {
                pitch: 74,
                tick: 720,
                duration: 480,
                velocity: 96,
            },
        )],
    ));
    let note = &app.project.midi_items()[0].notes()[0];
    assert_eq!((note.pitch(), note.tick(), note.duration()), (74, 720, 480));
    let _ = app.update(Message::Undo);
    let note = &app.project.midi_items()[0].notes()[0];
    assert_eq!((note.pitch(), note.tick(), note.duration()), (72, 480, 240));
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.midi_items()[0].notes()[0].tick(), 720);

    let _ = app.update(Message::DeleteMidiNotes(item_id, vec![note_id]));
    assert!(app.project.midi_items()[0].notes().is_empty());
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0].notes()[0].id(), note_id);
}

#[test]
fn track_rename_is_undoable() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();

    assert!(!app.track_name_edits.contains_key(&track_id));
    let _ = app.update(Message::Timeline(
        super::super::timeline::TimelineEvent::SelectTrack(track_id),
    ));
    assert_eq!(
        app.timeline.delete_focus,
        super::super::timeline::DeleteFocusTarget::TrackPanel
    );
    let _ = app.update(Message::BeginTrackNameEdit(track_id));
    assert_eq!(app.track_name_edits.get(&track_id).unwrap(), "Audio 1");
    assert_eq!(
        app.timeline.delete_focus,
        super::super::timeline::DeleteFocusTarget::TrackPanel
    );
    let _ = app.update(Message::TrackNameChanged(track_id, "Lead Vox".to_owned()));
    let _ = app.update(Message::CommitTrackName(track_id));
    assert_eq!(app.project.tracks()[0].name(), "Lead Vox");
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks()[0].name(), "Audio 1");
    let revision = app.revision;
    let _ = app.update(Message::TrackNameChanged(track_id, "   ".to_owned()));
    let _ = app.update(Message::CommitTrackName(track_id));
    assert_eq!(app.project.tracks()[0].name(), "Audio 1");
    assert_eq!(app.revision, revision);
}

#[test]
fn mixer_track_rename_uses_shared_undoable_project_action() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let selected_track_id = app.timeline.selected_track.unwrap();
    let renamed_track = app
        .project
        .tracks()
        .iter()
        .find(|track| track.id() != selected_track_id)
        .unwrap();
    let renamed_track_id = renamed_track.id();
    let original_name = renamed_track.name().to_owned();
    let _ = app.update(Message::ShowMainWorkspace(MainWorkspace::Mixer));

    let _ = app.update(Message::TrackNameChanged(
        renamed_track_id,
        "Bass Bus".to_owned(),
    ));
    let _ = app.update(Message::CommitTrackName(renamed_track_id));
    assert_eq!(
        app.project
            .tracks()
            .iter()
            .find(|track| track.id() == renamed_track_id)
            .unwrap()
            .name(),
        "Bass Bus"
    );
    assert_eq!(app.timeline.selected_track, Some(selected_track_id));

    let _ = app.update(Message::ShowMainWorkspace(MainWorkspace::Arrangement));
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.project
            .tracks()
            .iter()
            .find(|track| track.id() == renamed_track_id)
            .unwrap()
            .name(),
        original_name
    );
    let _ = app.update(Message::Redo);
    assert_eq!(
        app.project
            .tracks()
            .iter()
            .find(|track| track.id() == renamed_track_id)
            .unwrap()
            .name(),
        "Bass Bus"
    );
    assert_eq!(app.timeline.selected_track, Some(selected_track_id));
}

#[test]
fn track_context_menu_selects_its_target_and_closes_after_an_action() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();

    let _ = app.update(Message::Timeline(
        crate::timeline::TimelineEvent::OpenTrackContextMenu(track_id),
    ));
    assert_eq!(app.timeline.context_track, Some(track_id));
    assert_eq!(app.timeline.selected_track, Some(track_id));

    let _ = app.update(Message::Timeline(
        crate::timeline::TimelineEvent::ToggleTrackContextMenu(track_id),
    ));
    assert!(app.timeline.context_track.is_none());
    let _ = app.update(Message::Timeline(
        crate::timeline::TimelineEvent::ToggleTrackContextMenu(track_id),
    ));
    assert_eq!(app.timeline.context_track, Some(track_id));

    let _ = app.update(Message::ToggleMute(track_id));
    assert!(app.timeline.context_track.is_none());
    assert!(app.project.tracks()[0].is_muted());
}

#[test]
fn item_context_menu_selects_unselected_targets_and_preserves_existing_multiselection() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    for start_tick in [0, 1_920, 3_840] {
        app.project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick,
                length_ticks: 960,
            })
            .unwrap();
    }
    app.timeline.rebuild(&app.project);
    let item_ids = app
        .project
        .midi_items()
        .iter()
        .map(|item| item.id())
        .collect::<Vec<_>>();
    app.timeline
        .selected_items
        .extend([item_ids[0], item_ids[1]]);
    app.timeline.selected_item = Some(item_ids[1]);
    app.timeline
        .handle(crate::timeline::TimelineEvent::SetTimeSelection {
            start_tick: 100,
            end_tick: 500,
        });
    let selection = app.timeline.selected_items.clone();

    let _ = app.update(Message::Timeline(
        crate::timeline::TimelineEvent::OpenItemContextMenu {
            item_id: item_ids[1],
            x: 120.0,
            y: 40.0,
        },
    ));
    assert_eq!(app.timeline.context_item, Some(item_ids[1]));
    assert_eq!(app.timeline.context_item_position, Some((120.0, 40.0)));
    assert_eq!(app.timeline.selected_items, selection);
    assert!(
        commands::for_menu(&app, MainMenu::Item)
            .iter()
            .any(|entry| entry.id == CommandId::DeleteSelectedItems && entry.enabled)
    );

    let _ = app.update(Message::Escape);
    assert_eq!(app.timeline.context_item, None);
    assert_eq!(
        app.timeline.time_selection,
        Some(crate::timeline::TimeSelection {
            start_tick: 100,
            end_tick: 500,
        })
    );
    assert_eq!(app.timeline.selected_items, selection);

    let _ = app.update(Message::Timeline(
        crate::timeline::TimelineEvent::OpenItemContextMenu {
            item_id: item_ids[2],
            x: 20.0,
            y: 8.0,
        },
    ));
    assert_eq!(
        app.timeline.selected_items,
        [item_ids[2]].into_iter().collect()
    );
    assert_eq!(app.timeline.selected_item, Some(item_ids[2]));
    let _ = app.update(Message::Timeline(
        crate::timeline::TimelineEvent::SelectEmpty(4_000),
    ));
    assert_eq!(app.timeline.context_item, None);
    assert_eq!(app.timeline.context_item_position, None);
}

#[test]
fn track_menu_context_menu_and_action_search_share_command_definitions() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let selected_id = app.project.tracks()[1].id();
    app.timeline.selected_track = Some(selected_id);

    let menu_entries = commands::for_menu(&app, MainMenu::Track);
    let context_entries = commands::for_track_context(&app, selected_id);
    for context_entry in context_entries {
        let CommandId::Track { track_id, command } = context_entry.id else {
            panic!("track context entry should target the clicked track");
        };
        let menu_entry = menu_entries
            .iter()
            .find(|entry| entry.id == CommandId::SelectedTrack(command))
            .expect("the Track menu should expose the same command");
        assert_eq!(context_entry.label, menu_entry.label);
        assert_eq!(track_id, selected_id);
    }
    assert_eq!(
        commands::find(&app, "move selected track up"),
        Some(CommandId::SelectedTrack(TrackCommand::MoveUp))
    );
}

#[test]
fn item_menu_exposes_native_remove_and_select_all_availability_for_an_empty_project() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let entries = commands::for_menu(&app, MainMenu::Item);
    let remove_items = entries
        .first()
        .expect("Item menu should start with Remove items");
    assert_eq!(remove_items.id, CommandId::DeleteSelectedItems);
    assert_eq!(remove_items.label, "Remove items");
    assert_eq!(remove_items.shortcut, None);
    assert!(!remove_items.enabled);

    let track_id = app.project.tracks()[0].id();
    let _ = app.update(Message::Timeline(
        super::super::timeline::TimelineEvent::SelectTrack(track_id),
    ));
    let delete_by_focus = commands::shortcut_entries(&app)
        .into_iter()
        .find(|entry| entry.id == "edit.delete-focused")
        .expect("REAPER's focus-sensitive Delete action should be registered");
    assert_eq!(
        delete_by_focus.label,
        "Remove items/tracks/envelope points (depending on focus)"
    );
    assert_eq!(delete_by_focus.default_binding, "Delete");

    let delete_by_focus_no_prompting = commands::shortcut_entries(&app)
        .into_iter()
        .find(|entry| entry.id == "edit.delete-focused-no-prompting")
        .expect("REAPER's no-prompting focus-sensitive Delete action should be registered");
    assert_eq!(
        delete_by_focus_no_prompting.label,
        "Remove items/tracks/envelope points (depending on focus) - no prompting"
    );
    assert!(delete_by_focus_no_prompting.default_binding.is_empty());
    assert_eq!(
        commands::find(&app, "delete by focus without prompting"),
        Some(CommandId::DeleteByFocusNoPrompting)
    );

    let trim_items = entries
        .get(1)
        .expect("Trim items should follow Remove items");
    assert_eq!(trim_items.id, CommandId::TrimItemsToSelectedArea);
    assert_eq!(trim_items.label, "Trim items to selected area");
    assert!(trim_items.enabled);

    let copy_loop = entries.get(2).expect("Copy loop should follow Trim items");
    assert_eq!(copy_loop.id, CommandId::CopyLoopOfSelectedArea);
    assert_eq!(copy_loop.label, "Copy loop of selected area of items");
    assert_eq!(copy_loop.shortcut.as_deref(), Some("Ctrl+Alt+L"));
    assert!(copy_loop.enabled);

    let enclose = entries
        .get(3)
        .expect("Enclose media items should follow Copy loop");
    assert_eq!(enclose.id, CommandId::EncloseMediaItemsInRazorEdit);
    assert_eq!(enclose.label, "Enclose media items in razor edit");
    assert!(enclose.enabled);

    let select_all = entries
        .get(4)
        .expect("Select all should follow the item razor-edit section");
    assert_eq!(select_all.id, CommandId::SelectAllItems);
    assert_eq!(select_all.label, "Select all");
    assert!(select_all.enabled);

    let edit_entries = commands::for_menu(&app, MainMenu::Edit);
    let paste = edit_entries
        .iter()
        .find(|entry| entry.id == CommandId::PasteItems)
        .expect("Edit menu should expose Paste items/tracks");
    assert_eq!(paste.label, "Paste");
    assert_eq!(paste.shortcut.as_deref(), Some("Ctrl+V"));
    assert!(!paste.enabled);

    let revision = app.revision;
    let _ = app.update(Message::ExecuteCommand(CommandId::SelectAllItems));
    assert!(app.project.audio_items().is_empty());
    assert!(app.project.midi_items().is_empty());
    assert!(app.timeline.selected_items.is_empty());
    assert_eq!(app.revision, revision);
}

#[test]
fn copy_loop_of_selected_audio_area_pastes_source_sections_on_one_undo_step() {
    let directory = tempfile::tempdir().unwrap();
    let project_path = directory.path().join("copy-loop.aaadaw");
    let media_ref = "asset://copy-loop-test";
    let mut store = ProjectStore::open(&project_path).unwrap();
    store
        .import_audio_asset(media_ref, "copy-loop.wav", std::io::Cursor::new([0_u8; 4]))
        .unwrap();
    store
        .set_audio_asset_metadata(
            media_ref,
            &aaadaw_storage::AudioAssetMetadata {
                container: "wav".to_owned(),
                codec: "pcm".to_owned(),
                sample_rate: Some(48_000),
                channel_count: Some(1),
                bits_per_sample: Some(16),
                frame_count: Some(600_000),
                duration_nanos: Some(12_500_000_000),
                byte_len: Some(4),
            },
        )
        .unwrap();
    store.close().unwrap();

    let mut app = App {
        project_path: Some(project_path),
        ..App::default()
    };
    app.project
        .apply(DawAction::SetProjectSampleRate {
            sample_rate: 44_100,
        })
        .unwrap();
    for _ in 0..3 {
        let _ = app.update(Message::AddTrack);
    }
    let track_ids = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    let full_length = app.project.sample_at_tick(19_200).unwrap();
    let partial_start = app.project.sample_at_tick(7_680).unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: track_ids[0],
            media_ref: media_ref.to_owned(),
            start_sample: 0,
            source_offset_samples: 128,
            length_samples: full_length,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: track_ids[1],
            media_ref: media_ref.to_owned(),
            start_sample: 0,
            source_offset_samples: 256,
            length_samples: full_length,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: track_ids[2],
            media_ref: media_ref.to_owned(),
            start_sample: partial_start,
            source_offset_samples: 512,
            length_samples: full_length,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: track_ids[2],
            start_tick: 0,
            length_ticks: 19_200,
        })
        .unwrap();
    let full_first_id = app.project.audio_items()[0].id();
    let full_second_id = app.project.audio_items()[1].id();
    let partial_id = app.project.audio_items()[2].id();
    let midi_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::SetAudioItemMute {
            item_id: full_first_id,
            muted: true,
        })
        .unwrap();
    app.project
        .apply(DawAction::SetAudioItemPhase {
            item_id: full_second_id,
            inverted: true,
        })
        .unwrap();
    app.timeline.select_items(vec![
        (full_first_id, track_ids[0]),
        (full_second_id, track_ids[1]),
        (partial_id, track_ids[2]),
        (midi_id, track_ids[2]),
    ]);
    app.timeline.time_selection = Some(crate::timeline::TimeSelection {
        start_tick: 5_760,
        end_tick: 13_440,
    });
    let selected_before = app.timeline.selected_items.clone();
    let revision_before = app.revision;
    let source_delta = u64::try_from(
        (u128::from(app.project.sample_at_tick(5_760).unwrap()) * 48_000
            + u128::from(app.project.settings().sample_rate()) / 2)
            / u128::from(app.project.settings().sample_rate()),
    )
    .unwrap();
    let selection_length =
        app.project.sample_at_tick(13_440).unwrap() - app.project.sample_at_tick(5_760).unwrap();

    let _ = app.update(Message::ExecuteCommand(CommandId::CopyLoopOfSelectedArea));

    let clipboard = app
        .audio_item_clipboard
        .clone()
        .expect("copy action should populate the audio item clipboard");
    assert_eq!(clipboard.items.len(), 2);
    assert_eq!(clipboard.items[0].track_offset, 0);
    assert_eq!(clipboard.items[1].track_offset, 1);
    assert_eq!(clipboard.items[0].source_offset_samples, 0);
    assert_eq!(
        clipboard.items[0].source_section_start_samples,
        128 + source_delta
    );
    assert_eq!(clipboard.items[1].source_offset_samples, 0);
    assert_eq!(
        clipboard.items[1].source_section_start_samples,
        256 + source_delta
    );
    assert!(clipboard.items[0].muted);
    assert!(!clipboard.items[1].muted);
    assert!(!clipboard.items[0].phase_inverted);
    assert!(clipboard.items[1].phase_inverted);
    assert_eq!(clipboard.items[0].length_samples, selection_length);
    assert_eq!(
        clipboard.items[0].source_section_length_samples,
        u64::try_from((u128::from(selection_length) * 48_000 + 44_100 / 2) / 44_100).unwrap()
    );
    assert_eq!(clipboard.span_samples, selection_length);
    assert_eq!(clipboard.project_sample_rate, 44_100);
    assert_eq!(app.timeline.selected_items, selected_before);
    assert_eq!(app.revision, revision_before);

    app.project
        .apply(DawAction::SetProjectSampleRate {
            sample_rate: 48_000,
        })
        .unwrap();
    app.timeline.selected_track = Some(track_ids[0]);
    app.timeline.edit_cursor_tick = 57_600;
    let paste_start = app.project.sample_at_tick(57_600).unwrap();
    let paste_length =
        u64::try_from((u128::from(selection_length) * 48_000 + 44_100 / 2) / 44_100).unwrap();
    let _ = app.update(Message::ExecuteCommand(CommandId::PasteItems));

    let pasted = app
        .project
        .audio_items()
        .iter()
        .filter(|item| item.start_sample() == paste_start)
        .collect::<Vec<_>>();
    assert_eq!(pasted.len(), 2);
    assert_eq!(pasted[0].track_id(), track_ids[0]);
    assert_eq!(pasted[1].track_id(), track_ids[1]);
    assert_eq!(pasted[0].source_offset_samples(), 0);
    assert_eq!(
        pasted[0].source_section_start_samples(),
        Some(clipboard.items[0].source_section_start_samples)
    );
    assert_eq!(pasted[1].source_offset_samples(), 0);
    assert_eq!(pasted[0].length_samples(), paste_length);
    assert!(pasted[0].is_muted());
    assert!(!pasted[1].is_muted());
    assert!(!pasted[0].is_phase_inverted());
    assert!(pasted[1].is_phase_inverted());
    assert!(pasted.iter().all(|item| item.is_loop_source()));
    assert_eq!(
        pasted[0].source_section_length_samples(),
        Some(clipboard.items[0].source_section_length_samples)
    );
    assert_eq!(app.revision, revision_before + 1);
    assert_eq!(app.timeline.selected_items.len(), 2);
    assert!(commands::is_enabled(&app, CommandId::PasteItems));

    let _ = app.update(Message::Undo);
    assert!(
        app.project
            .audio_items()
            .iter()
            .all(|item| item.start_sample() != paste_start)
    );
    assert_eq!(app.audio_item_clipboard.as_ref(), Some(&clipboard));

    let _ = app.update(Message::Redo);
    let restored_paste = app
        .project
        .audio_items()
        .iter()
        .filter(|item| item.start_sample() == paste_start)
        .collect::<Vec<_>>();
    assert_eq!(restored_paste.len(), 2);
    assert!(restored_paste[0].is_muted());
    assert!(!restored_paste[1].is_muted());
    assert!(!restored_paste[0].is_phase_inverted());
    assert!(restored_paste[1].is_phase_inverted());
    let _ = app.update(Message::Undo);
    assert!(
        app.project
            .audio_items()
            .iter()
            .all(|item| item.start_sample() != paste_start)
    );

    app.timeline.selected_track = Some(track_ids[1]);
    app.timeline.edit_cursor_tick = 76_800;
    let second_paste_start = app.project.sample_at_tick(76_800).unwrap();
    let _ = app.update(Message::ExecuteCommand(CommandId::PasteItems));
    let second_paste = app
        .project
        .audio_items()
        .iter()
        .filter(|item| item.start_sample() == second_paste_start)
        .collect::<Vec<_>>();
    assert_eq!(second_paste.len(), 2);
    assert_eq!(second_paste[0].track_id(), track_ids[1]);
    assert_eq!(second_paste[1].track_id(), track_ids[2]);
    assert!(!second_paste[0].is_phase_inverted());
    assert!(second_paste[1].is_phase_inverted());

    app.project_generation = app.project_generation.saturating_add(1);
    assert!(!commands::is_enabled(&app, CommandId::PasteItems));
}

#[test]
fn enclose_selected_audio_and_midi_in_razor_areas_and_support_undo() {
    let mut app = App::default();
    for _ in 0..3 {
        let _ = app.update(Message::AddTrack);
    }
    let track_ids = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    let sample_0 = app.project.sample_at_tick(0).unwrap();
    let sample_960 = app.project.sample_at_tick(960).unwrap();
    let sample_1920 = app.project.sample_at_tick(1_920).unwrap();
    let sample_5760 = app.project.sample_at_tick(5_760).unwrap();
    let sample_7680 = app.project.sample_at_tick(7_680).unwrap();
    let sample_11520 = app.project.sample_at_tick(11_520).unwrap();
    let sample_13440 = app.project.sample_at_tick(13_440).unwrap();
    let sample_23040 = app.project.sample_at_tick(23_040).unwrap();
    let old_area = RazorEditArea::new(sample_0, sample_960).unwrap();
    app.project
        .apply(DawAction::SetTrackRazorEditAreas {
            track_areas: vec![(track_ids[2], vec![old_area])],
        })
        .unwrap();

    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: track_ids[0],
            media_ref: "asset://razor-test".to_owned(),
            start_sample: sample_1920,
            source_offset_samples: 0,
            length_samples: sample_5760 - sample_1920,
        })
        .unwrap();
    let first_audio = app.project.audio_items().last().unwrap().id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: track_ids[0],
            media_ref: "asset://razor-test".to_owned(),
            start_sample: sample_11520,
            source_offset_samples: 0,
            length_samples: sample_13440 - sample_11520,
        })
        .unwrap();
    let second_audio = app.project.audio_items().last().unwrap().id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: track_ids[1],
            media_ref: "asset://razor-test".to_owned(),
            start_sample: sample_7680,
            source_offset_samples: 0,
            length_samples: sample_11520 - sample_7680,
        })
        .unwrap();
    let other_track_audio = app.project.audio_items().last().unwrap().id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: track_ids[0],
            start_tick: 19_200,
            length_ticks: 3_840,
        })
        .unwrap();
    let midi_id = app.project.midi_items()[0].id();
    let selected_ids = [first_audio, second_audio, other_track_audio, midi_id];
    app.timeline.select_items(
        selected_ids
            .iter()
            .map(|item_id| {
                let track_id = app
                    .project
                    .audio_items()
                    .iter()
                    .find(|item| item.id() == *item_id)
                    .map(|item| item.track_id())
                    .or_else(|| {
                        app.project
                            .midi_items()
                            .iter()
                            .find(|item| item.id() == *item_id)
                            .map(|item| item.track_id())
                    })
                    .unwrap();
                (*item_id, track_id)
            })
            .collect(),
    );
    app.timeline.rebuild(&app.project);
    let action_revision = app.revision;
    let expected_area = RazorEditArea::new(sample_1920, sample_23040).unwrap();

    let _ = app.update(Message::ExecuteCommand(
        CommandId::EncloseMediaItemsInRazorEdit,
    ));

    assert!(app.timeline.selected_items.is_empty());
    assert_eq!(app.project.tracks()[0].razor_edit_areas(), &[expected_area]);
    assert_eq!(app.project.tracks()[1].razor_edit_areas(), &[expected_area]);
    assert!(app.project.tracks()[2].razor_edit_areas().is_empty());
    assert_eq!(app.revision, action_revision + 1);

    let _ = app.update(Message::Undo);
    assert!(app.project.tracks()[0].razor_edit_areas().is_empty());
    assert!(app.project.tracks()[1].razor_edit_areas().is_empty());
    assert_eq!(app.project.tracks()[2].razor_edit_areas(), &[old_area]);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.tracks()[0].razor_edit_areas(), &[expected_area]);
    assert_eq!(app.project.tracks()[1].razor_edit_areas(), &[expected_area]);

    let revision_before_clear = app.revision;
    let _ = app.update(Message::ExecuteCommand(
        CommandId::EncloseMediaItemsInRazorEdit,
    ));
    assert!(
        app.project
            .tracks()
            .iter()
            .all(|track| track.razor_edit_areas().is_empty())
    );
    assert_eq!(app.revision, revision_before_clear + 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks()[0].razor_edit_areas(), &[expected_area]);
    assert_eq!(app.project.tracks()[1].razor_edit_areas(), &[expected_area]);
}

#[test]
fn trim_items_to_selected_area_clips_audio_and_midi_in_one_undoable_edit() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    let selection_start_tick = 5_760;
    let selection_end_tick = 13_440;
    let audio_selection_start = app
        .project
        .sample_at_tick(selection_start_tick)
        .expect("selection start should map to a sample");
    let audio_selection_end = app
        .project
        .sample_at_tick(selection_end_tick)
        .expect("selection end should map to a sample");
    let audio_item_end = app
        .project
        .sample_at_tick(19_200)
        .expect("item end should map to a sample");
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://trim-test".to_owned(),
            start_sample: 0,
            source_offset_samples: 128,
            length_samples: audio_item_end,
        })
        .expect("test audio item should be inserted");
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 19_200,
        })
        .expect("test MIDI item should be inserted");
    let audio_id = app.project.audio_items()[0].id();
    let midi_id = app.project.midi_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline.handle(TimelineEvent::SelectItem {
        item_id: Some(audio_id),
        additive: false,
        range: false,
    });
    app.timeline.handle(TimelineEvent::SelectItem {
        item_id: Some(midi_id),
        additive: true,
        range: false,
    });
    let _ = app.update(Message::Timeline(TimelineEvent::SetTimeSelection {
        start_tick: selection_start_tick,
        end_tick: selection_end_tick,
    }));

    let revision = app.revision;
    let _ = app.update(Message::ExecuteCommand(CommandId::TrimItemsToSelectedArea));

    let audio = &app.project.audio_items()[0];
    assert_eq!(audio.start_sample(), audio_selection_start);
    assert_eq!(
        audio.length_samples(),
        audio_selection_end - audio_selection_start
    );
    assert_eq!(
        audio.source_offset_samples(),
        i64::try_from(128 + audio_selection_start).unwrap()
    );
    let midi = &app.project.midi_items()[0];
    assert_eq!(midi.start_tick(), selection_start_tick);
    assert_eq!(
        midi.length_ticks(),
        selection_end_tick - selection_start_tick
    );
    assert_eq!(midi.source_offset_ticks(), selection_start_tick);
    assert!(app.timeline.selected_items.contains(&audio_id));
    assert!(app.timeline.selected_items.contains(&midi_id));
    assert_eq!(app.revision, revision + 1);

    let _ = app.update(Message::Undo);
    let audio = &app.project.audio_items()[0];
    assert_eq!(audio.id(), audio_id);
    assert_eq!(audio.start_sample(), 0);
    assert_eq!(audio.length_samples(), audio_item_end);
    assert_eq!(audio.source_offset_samples(), 128);
    let midi = &app.project.midi_items()[0];
    assert_eq!(midi.id(), midi_id);
    assert_eq!(midi.start_tick(), 0);
    assert_eq!(midi.length_ticks(), 19_200);
    assert_eq!(midi.source_offset_ticks(), 0);
}

#[test]
fn trim_items_to_selected_area_matches_native_overlap_and_endpoint_rules() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    let ranges = [
        (0, 3_840),
        (0, 5_760),
        (0, 7_680),
        (5_760, 13_440),
        (11_520, 19_200),
        (13_440, 19_200),
        (15_360, 19_200),
    ];
    let mut ids = Vec::with_capacity(ranges.len());
    for (start_tick, end_tick) in ranges {
        app.project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick,
                length_ticks: end_tick - start_tick,
            })
            .expect("test MIDI item should be inserted");
        ids.push(app.project.midi_items().last().unwrap().id());
    }
    app.timeline.rebuild(&app.project);
    for (index, item_id) in ids.iter().copied().enumerate() {
        app.timeline.handle(TimelineEvent::SelectItem {
            item_id: Some(item_id),
            additive: index != 0,
            range: false,
        });
    }
    let _ = app.update(Message::Timeline(TimelineEvent::SetTimeSelection {
        start_tick: 5_760,
        end_tick: 13_440,
    }));

    let revision = app.revision;
    let _ = app.update(Message::TrimItemsToSelectedArea);

    assert_eq!(app.project.midi_items().len(), 5);
    assert!(
        app.project
            .midi_items()
            .iter()
            .any(|item| item.id() == ids[0])
    );
    for item_id in [ids[2], ids[3], ids[4], ids[6]] {
        assert!(
            app.project
                .midi_items()
                .iter()
                .any(|item| item.id() == item_id)
        );
    }
    let left_overlap = app
        .project
        .midi_items()
        .iter()
        .find(|item| item.id() == ids[2])
        .unwrap();
    assert_eq!(left_overlap.start_tick(), 5_760);
    assert_eq!(left_overlap.length_ticks(), 1_920);
    assert_eq!(left_overlap.source_offset_ticks(), 5_760);
    let interior = app
        .project
        .midi_items()
        .iter()
        .find(|item| item.id() == ids[3])
        .unwrap();
    assert_eq!(interior.start_tick(), 5_760);
    assert_eq!(interior.length_ticks(), 7_680);
    let right_overlap = app
        .project
        .midi_items()
        .iter()
        .find(|item| item.id() == ids[4])
        .unwrap();
    assert_eq!(right_overlap.start_tick(), 11_520);
    assert_eq!(right_overlap.length_ticks(), 1_920);
    assert_eq!(app.revision, revision + 1);

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items().len(), 7);
    assert!(ids.iter().all(|item_id| {
        app.project
            .midi_items()
            .iter()
            .any(|item| item.id() == *item_id)
    }));
}

#[test]
fn trim_items_to_selected_area_without_a_selection_is_a_noop() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .expect("test MIDI item should be inserted");
    let item_id = app.project.midi_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline.handle(TimelineEvent::SelectItem {
        item_id: Some(item_id),
        additive: false,
        range: false,
    });
    let revision = app.revision;

    let _ = app.update(Message::TrimItemsToSelectedArea);

    assert_eq!(app.project.midi_items()[0].id(), item_id);
    assert_eq!(app.project.midi_items()[0].start_tick(), 0);
    assert_eq!(app.project.midi_items()[0].length_ticks(), 960);
    assert_eq!(app.revision, revision);

    let _ = app.update(Message::Timeline(TimelineEvent::SetTimeSelection {
        start_tick: 120,
        end_tick: 480,
    }));
    app.timeline.clear_selected_items();
    let revision = app.revision;
    let _ = app.update(Message::TrimItemsToSelectedArea);
    assert_eq!(app.project.midi_items()[0].start_tick(), 0);
    assert_eq!(app.project.midi_items()[0].length_ticks(), 960);
    assert_eq!(app.revision, revision);
}

#[test]
fn item_menu_remove_items_deletes_selected_media_and_is_undoable() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .expect("test MIDI item should be inserted");
    let item_id = app.project.midi_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline
        .handle(crate::timeline::TimelineEvent::SelectItem {
            item_id: Some(item_id),
            additive: false,
            range: false,
        });

    let entries = commands::for_menu(&app, MainMenu::Item);
    assert_eq!(entries[0].id, CommandId::DeleteSelectedItems);
    assert!(entries[0].enabled);
    assert_eq!(
        commands::find(&app, "remove items"),
        Some(CommandId::DeleteSelectedItems)
    );
    assert_eq!(
        commands::find(&app, "delete selected items"),
        Some(CommandId::DeleteSelectedItems)
    );
    let revision = app.revision;
    let _ = app.update(Message::ExecuteCommand(CommandId::DeleteSelectedItems));
    assert!(app.project.midi_items().is_empty());
    assert_eq!(app.revision, revision + 1);

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0].id(), item_id);
}

#[test]
fn toolbar_and_track_controls_share_command_availability() {
    let mut app = App::default();
    assert!(!commands::is_enabled(&app, CommandId::AddMidiItem));

    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.timeline.selected_track = Some(track_id);
    app.io_busy = true;

    for command in [
        CommandId::AddTrack,
        CommandId::AddMidiItem,
        CommandId::Track {
            track_id,
            command: TrackCommand::ToggleMute,
        },
        CommandId::Track {
            track_id,
            command: TrackCommand::ToggleSolo,
        },
    ] {
        assert!(!commands::is_enabled(&app, command));
    }

    let track_menu = commands::for_menu(&app, MainMenu::Track);
    for command in [TrackCommand::ToggleMute, TrackCommand::ToggleSolo] {
        let entry = track_menu
            .iter()
            .find(|entry| entry.id == CommandId::SelectedTrack(command))
            .expect("Track menu should expose each track control");
        assert_eq!(entry.enabled, commands::is_enabled(&app, entry.id));
        assert!(!entry.enabled);
    }
}

#[test]
fn track_reorder_is_undoable() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let first_id = app.project.tracks()[0].id();
    let second_id = app.project.tracks()[1].id();

    let _ = app.update(Message::MoveTrack(second_id, -1));
    assert_eq!(app.project.tracks()[0].id(), second_id);
    assert_eq!(app.project.tracks()[1].id(), first_id);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks()[0].id(), first_id);
    assert_eq!(app.project.tracks()[1].id(), second_id);
}

#[test]
fn track_pan_adjustment_is_undoable() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();

    let _ = app.update(Message::PreviewTrackPan(track_id, 0.25));
    assert_eq!(app.project.tracks()[0].pan(), 0.0);
    let _ = app.update(Message::CommitTrackPan(track_id));
    assert_eq!(app.project.tracks()[0].pan(), 0.0);
    app.track_mix_commit_at = Some(Instant::now() - Duration::from_secs(1));
    let _ = app.update(Message::BackgroundTick);
    assert_eq!(app.project.tracks()[0].pan(), 0.25);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks()[0].pan(), 0.0);
}

#[test]
fn resetting_track_pan_is_undoable() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::SetTrackPan { track_id, pan: 0.4 })
        .expect("track pan should be set");

    let _ = app.update(Message::ResetTrackPan(track_id));
    assert_eq!(app.project.tracks()[0].pan(), 0.0);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks()[0].pan(), 0.4);
}

#[test]
fn resetting_track_volume_is_undoable() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::SetTrackVolume {
            track_id,
            volume_db: -4.0,
        })
        .expect("track volume should be set");

    let _ = app.update(Message::ResetTrackVolume(track_id));
    assert_eq!(app.project.tracks()[0].volume_db(), 0.0);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks()[0].volume_db(), -4.0);
}

#[test]
fn reset_button_preserves_a_recent_mix_gesture_as_a_separate_undo_step() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::SetTrackVolume {
            track_id,
            volume_db: -4.0,
        })
        .expect("track volume should be set");

    let _ = app.update(Message::PreviewTrackVolume(track_id, -8.0));
    let _ = app.update(Message::CommitTrackVolume(track_id));
    let _ = app.update(Message::ResetTrackVolume(track_id));
    assert_eq!(app.project.tracks()[0].volume_db(), 0.0);

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks()[0].volume_db(), -8.0);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks()[0].volume_db(), -4.0);
}

#[test]
fn dragging_track_volume_commits_one_undoable_action() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();

    let _ = app.update(Message::PreviewTrackVolume(track_id, -3.2));
    let _ = app.update(Message::PreviewTrackVolume(track_id, -5.7));
    assert_eq!(app.project.tracks()[0].volume_db(), 0.0);

    let _ = app.update(Message::CommitTrackVolume(track_id));
    assert_eq!(app.project.tracks()[0].volume_db(), 0.0);
    app.track_mix_commit_at = Some(Instant::now() - Duration::from_secs(1));
    let _ = app.update(Message::BackgroundTick);
    assert_eq!(app.project.tracks()[0].volume_db(), -5.7);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks()[0].volume_db(), 0.0);
    let _ = app.update(Message::Undo);
    assert!(app.project.tracks().is_empty());
}

#[test]
fn master_gesture_commits_once_and_undo_restores_both_controls() {
    let mut app = App::default();
    let _ = app.update(Message::PreviewMasterVolume(-3.0));
    let _ = app.update(Message::PreviewMasterVolume(-12.0));
    assert_eq!(app.project.master_mix().volume_db(), 0.0);
    let _ = app.update(Message::CommitMasterMix);
    assert!(commands::is_enabled(&app, commands::CommandId::Undo));
    assert!(!commands::is_enabled(&app, commands::CommandId::Redo));
    app.master_mix_commit_at = Some(Instant::now() - Duration::from_secs(1));
    let _ = app.update(Message::BackgroundTick);
    assert_eq!(app.project.master_mix().volume_db(), -12.0);
    let _ = app.update(Message::PreviewMasterPan(0.5));
    let _ = app.update(Message::CommitMasterMix);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.master_mix().pan(), 0.0);
    assert_eq!(app.project.master_mix().volume_db(), -12.0);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.master_mix(), aaadaw_core::MasterMix::default());
    assert!(!app.project.can_undo());
}

#[test]
fn master_cancel_and_double_click_do_not_commit_first_click_preview() {
    let mut app = App::default();
    app.project
        .apply(DawAction::SetMasterMix {
            mix: aaadaw_core::MasterMix::new(-6.0, 0.5).unwrap(),
        })
        .unwrap();
    let _ = app.update(Message::PreviewMasterVolume(-20.0));
    let _ = app.update(Message::CancelMasterMix);
    assert_eq!(app.project.master_mix().volume_db(), -6.0);
    let _ = app.update(Message::PreviewMasterPan(-0.8));
    let _ = app.update(Message::Escape);
    assert_eq!(app.project.master_mix().pan(), 0.5);
    let _ = app.update(Message::PreviewMasterVolume(-20.0));
    let _ = app.update(Message::CommitMasterMix);
    let _ = app.update(Message::ResetMasterVolume);
    assert_eq!(app.project.master_mix().volume_db(), 0.0);
    assert_eq!(app.project.master_mix().pan(), 0.5);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.master_mix().volume_db(), -6.0);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.master_mix(), aaadaw_core::MasterMix::default());
}

#[test]
fn factory_fader_range_and_silent_endpoint_preserve_undo_and_finite_storage() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let id = app.project.tracks()[0].id();
    let _ = app.update(Message::TrackVolumeTextChanged(id, "12".into()));
    let _ = app.update(Message::CommitTrackVolumeText(id));
    assert_eq!(app.project.tracks()[0].volume_db(), 12.0);
    let _ = app.update(Message::TrackVolumeTextChanged(id, "20".into()));
    let _ = app.update(Message::CommitTrackVolumeText(id));
    assert_eq!(app.project.tracks()[0].volume_db(), 20.0);
    let _ = app.update(Message::TrackVolumeTextChanged(id, "-inf".into()));
    let _ = app.update(Message::CommitTrackVolumeText(id));
    assert_eq!(app.project.tracks()[0].volume_db(), fader::SILENCE_DB);
    assert!(app.project.tracks()[0].volume_db().is_finite());
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks()[0].volume_db(), 20.0);
    let _ = app.update(Message::PreviewMasterVolume(fader::from_position(0.0)));
    let _ = app.update(Message::CommitMasterMix);
    app.master_mix_commit_at = Some(Instant::now() - Duration::from_secs(1));
    let _ = app.update(Message::BackgroundTick);
    assert_eq!(app.project.master_mix().channel_gains(), [0.0, 0.0]);
    assert_eq!(
        Project::from_snapshot(app.project.snapshot())
            .unwrap()
            .master_mix()
            .channel_gains(),
        [0.0, 0.0]
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.master_mix().volume_db(), 0.0);
}

#[test]
fn right_click_cancels_a_track_mix_slider_gesture() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::SetTrackVolume {
            track_id,
            volume_db: -6.0,
        })
        .expect("track volume should be set");
    let revision = app.revision;

    let _ = app.update(Message::PreviewTrackVolume(track_id, -2.0));
    let _ = app.update(Message::CancelTrackMixGesture);

    assert_eq!(app.project.tracks()[0].volume_db(), -6.0);
    assert!(app.track_mix_gesture.is_none());
    assert_eq!(app.revision, revision);
}

#[test]
fn double_click_track_mix_reset_coalesces_the_pending_click_into_one_undo_step() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::SetTrackVolume {
            track_id,
            volume_db: -4.0,
        })
        .expect("track volume should be set");

    let _ = app.update(Message::PreviewTrackVolume(track_id, -8.0));
    let _ = app.update(Message::CommitTrackVolume(track_id));
    assert_eq!(app.project.tracks()[0].volume_db(), -4.0);
    let _ = app.update(Message::ResetTrackVolumeByDoubleClick(track_id));
    assert_eq!(app.project.tracks()[0].volume_db(), 0.0);

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks()[0].volume_db(), -4.0);
}

#[test]
fn track_delete_is_undoable() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();

    let _ = app.update(Message::DeleteTrack(track_id));
    assert!(app.project.tracks().is_empty());
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks().len(), 1);
    assert_eq!(app.project.tracks()[0].id(), track_id);
}

#[test]
fn delete_by_focus_prefers_selected_items_and_deletes_selected_tracks_atomically() {
    let mut app = App::default();
    let main_window_id = iced::window::Id::unique();
    app.main_window_id = Some(main_window_id);
    let focused_key_event = |key, physical_key| {
        Message::RuntimeKeyboardEvent(
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(key),
                modified_key: iced::keyboard::Key::Named(key),
                physical_key: iced::keyboard::key::Physical::Code(physical_key),
                location: iced::keyboard::Location::Standard,
                modifiers: Modifiers::NONE,
                text: None,
                repeat: false,
            }),
            iced::event::Status::Ignored,
            main_window_id,
        )
    };
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let parent = app.project.tracks()[0].id();
    let child = app.project.tracks()[1].id();
    let _ = app.update(Message::RoutingChange(DawAction::SetTrackFolder {
        track_id: parent,
        enabled: true,
    }));
    let _ = app.update(Message::RoutingChange(DawAction::SetTrackParent {
        track_id: child,
        parent: Some(parent),
    }));
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: child,
            start_tick: 0,
            length_ticks: 960,
        })
        .expect("test MIDI item should be inserted");
    let item_id = app.project.midi_items()[0].id();
    app.timeline.rebuild(&app.project);
    let _ = app.update(Message::Timeline(
        super::super::timeline::TimelineEvent::SelectTrackWithModifiers {
            track_id: parent,
            modifiers: Modifiers::NONE,
        },
    ));
    let _ = app.update(Message::Timeline(
        super::super::timeline::TimelineEvent::SelectTrackWithModifiers {
            track_id: child,
            modifiers: Modifiers::CTRL,
        },
    ));
    let _ = app.update(Message::Timeline(
        super::super::timeline::TimelineEvent::SelectItem {
            item_id: Some(item_id),
            additive: false,
            range: false,
        },
    ));

    let revision = app.revision;
    let _ = app.update(Message::ExecuteCommand(CommandId::DeleteByFocusNoPrompting));

    assert!(app.project.midi_items().is_empty());
    assert_eq!(app.project.tracks().len(), 2);
    assert_eq!(app.revision, revision + 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0].id(), item_id);

    let _ = app.update(Message::Timeline(
        super::super::timeline::TimelineEvent::SelectItem {
            item_id: Some(item_id),
            additive: false,
            range: false,
        },
    ));
    let _ = app.update(Message::Timeline(
        super::super::timeline::TimelineEvent::SelectTrackWithModifiers {
            track_id: parent,
            modifiers: Modifiers::NONE,
        },
    ));
    let _ = app.update(Message::Timeline(
        super::super::timeline::TimelineEvent::SelectTrackWithModifiers {
            track_id: child,
            modifiers: Modifiers::CTRL,
        },
    ));
    let revision = app.revision;
    let _ = app.update(Message::ExecuteCommand(CommandId::DeleteByFocus));

    assert_eq!(app.project.tracks().len(), 2);
    assert_eq!(app.project.midi_items().len(), 1);
    assert_eq!(app.revision, revision);
    assert_eq!(
        app.pending_focused_track_deletion.as_deref(),
        Some(&[parent, child][..])
    );
    let _ = app.update(focused_key_event(
        iced::keyboard::key::Named::Escape,
        iced::keyboard::key::Code::Escape,
    ));
    assert_eq!(app.project.tracks().len(), 2);
    assert_eq!(app.revision, revision);

    let _ = app.update(Message::ExecuteCommand(CommandId::DeleteByFocus));
    assert!(app.pending_focused_track_deletion.is_some());
    let _ = app.update(focused_key_event(
        iced::keyboard::key::Named::Enter,
        iced::keyboard::key::Code::Enter,
    ));
    assert!(app.project.tracks().is_empty());
    assert_eq!(app.revision, revision + 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks().len(), 2);
    assert_eq!(app.project.tracks()[0].id(), parent);
    assert_eq!(app.project.tracks()[1].id(), child);
    assert_eq!(app.project.tracks()[1].parent_track(), Some(parent));
    assert_eq!(app.project.midi_items()[0].id(), item_id);

    let _ = app.update(Message::Timeline(
        super::super::timeline::TimelineEvent::SelectTrackWithModifiers {
            track_id: parent,
            modifiers: Modifiers::NONE,
        },
    ));
    let _ = app.update(Message::Timeline(
        super::super::timeline::TimelineEvent::SelectTrackWithModifiers {
            track_id: child,
            modifiers: Modifiers::CTRL,
        },
    ));
    let revision = app.revision;
    let _ = app.update(Message::ExecuteCommand(CommandId::DeleteByFocusNoPrompting));
    assert!(app.pending_focused_track_deletion.is_none());
    assert!(app.project.tracks().is_empty());
    assert_eq!(app.revision, revision + 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks().len(), 2);
}

#[test]
fn bus_track_creation_and_routing_are_available_as_undoable_actions() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let source = app.project.tracks()[0].id();
    let _ = app.update(Message::AddBusTrack);
    let bus = app.project.tracks()[1].id();
    assert!(app.project.tracks()[1].is_bus());

    let _ = app.update(Message::SetTrackOutput(source, Some(bus)));
    assert_eq!(app.project.tracks()[0].output_track(), Some(bus));
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks()[0].output_track(), None);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.tracks()[0].output_track(), Some(bus));
}

#[test]
fn audio_timeline_exact_position_edit_preserves_source_and_is_undoable() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://exact-position".to_owned(),
            start_sample: 240,
            source_offset_samples: 120,
            length_samples: 960,
        })
        .expect("source item should be inserted");
    let item_id = app.project.audio_items()[0].id();

    let _ = app.update(Message::AudioItemStartSampleChanged(
        item_id,
        "12345".to_owned(),
    ));
    let _ = app.update(Message::CommitAudioItemStartSample(item_id));
    let item = &app.project.audio_items()[0];
    assert_eq!(item.start_sample(), 12_345);
    assert_eq!(item.source_offset_samples(), 120);
    assert_eq!(item.length_samples(), 960);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].start_sample(), 240);
}

#[test]
fn mixed_item_drag_moves_as_one_undoable_action_and_preserves_content() {
    let mut app = App::default();
    app.timeline
        .handle(TimelineEvent::SetSnapGrid(SnapGrid::Sixteenth));
    for _ in 0..3 {
        let _ = app.update(Message::AddTrack);
    }
    let tracks = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[0],
            media_ref: "asset://drag-audio".to_owned(),
            start_sample: app.project.sample_at_tick(480).unwrap(),
            source_offset_samples: 173,
            length_samples: 12_000,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[1],
            start_tick: 960,
            length_ticks: 3_840,
        })
        .unwrap();
    let audio_id = app.project.audio_items()[0].id();
    let midi_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id: midi_id,
            notes: vec![MidiNoteData {
                pitch: 67,
                tick: 480,
                duration: 720,
                velocity: 94,
            }],
        })
        .unwrap();
    let audio_before = app.project.audio_items()[0].clone();
    let midi_notes_before = app.project.midi_items()[0].notes().to_vec();
    app.timeline.rebuild(&app.project);
    app.timeline
        .handle(crate::timeline::TimelineEvent::SelectItem {
            item_id: Some(audio_id),
            additive: false,
            range: false,
        });
    app.timeline
        .handle(crate::timeline::TimelineEvent::SelectItem {
            item_id: Some(midi_id),
            additive: true,
            range: false,
        });
    app.timeline
        .handle(crate::timeline::TimelineEvent::BeginItemDrag {
            item_id: audio_id,
            pointer_delta_ticks: 150,
            target_track_index: Some(1),
            range: false,
            ignore_snap: false,
            copy: false,
        });

    app.finish_item_drag();

    let audio_after = &app.project.audio_items()[0];
    let midi_after = &app.project.midi_items()[0];
    assert_eq!(audio_after.track_id(), tracks[1]);
    assert_eq!(
        audio_after.start_sample(),
        app.project.sample_at_tick(720).unwrap()
    );
    assert_eq!(audio_after.media_ref(), audio_before.media_ref());
    assert_eq!(audio_after.source_offset_samples(), 173);
    assert_eq!(audio_after.length_samples(), 12_000);
    assert_eq!(midi_after.track_id(), tracks[2]);
    assert_eq!(midi_after.start_tick(), 1_200);
    assert_eq!(midi_after.length_ticks(), 3_840);
    assert_eq!(midi_after.notes(), midi_notes_before);
    assert_eq!(app.revision, 4);

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0], audio_before);
    assert_eq!(app.project.midi_items()[0].track_id(), tracks[1]);
    assert_eq!(app.project.midi_items()[0].start_tick(), 960);
    assert_eq!(app.project.midi_items()[0].notes(), midi_notes_before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items()[0].track_id(), tracks[1]);
    assert_eq!(
        app.project.audio_items()[0].start_sample(),
        app.project.sample_at_tick(720).unwrap()
    );
    assert_eq!(app.project.midi_items()[0].track_id(), tracks[2]);
    assert_eq!(app.project.midi_items()[0].start_tick(), 1_200);
    assert_eq!(app.project.midi_items()[0].notes(), midi_notes_before);
}

#[test]
fn mixed_item_copy_drag_duplicates_audio_and_midi_as_one_undoable_action() {
    let mut app = App::default();
    app.timeline
        .handle(TimelineEvent::SetSnapGrid(SnapGrid::Sixteenth));
    for _ in 0..3 {
        let _ = app.update(Message::AddTrack);
    }
    let tracks = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[0],
            media_ref: "asset://copy-drag".to_owned(),
            start_sample: app.project.sample_at_tick(480).unwrap(),
            source_offset_samples: 97,
            length_samples: 8_000,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[1],
            start_tick: 960,
            length_ticks: 1_920,
        })
        .unwrap();
    let audio_id = app.project.audio_items()[0].id();
    let midi_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id: midi_id,
            notes: vec![MidiNoteData {
                pitch: 67,
                tick: 120,
                duration: 360,
                velocity: 88,
            }],
        })
        .unwrap();
    let audio_before = app.project.audio_items()[0].clone();
    let midi_before = app.project.midi_items()[0].clone();
    app.timeline.rebuild(&app.project);
    app.timeline
        .handle(crate::timeline::TimelineEvent::SelectItem {
            item_id: Some(audio_id),
            additive: false,
            range: false,
        });
    app.timeline
        .handle(crate::timeline::TimelineEvent::SelectItem {
            item_id: Some(midi_id),
            additive: true,
            range: false,
        });
    app.timeline
        .handle(crate::timeline::TimelineEvent::BeginItemDrag {
            item_id: audio_id,
            pointer_delta_ticks: 240,
            target_track_index: Some(1),
            range: false,
            ignore_snap: false,
            copy: true,
        });
    let revision_before_copy = app.revision;

    app.finish_item_drag();

    assert_eq!(app.project.audio_items().len(), 2);
    assert_eq!(app.project.midi_items().len(), 2);
    assert_eq!(app.project.audio_items()[0], audio_before);
    assert_eq!(app.project.midi_items()[0], midi_before);
    let audio_copy = &app.project.audio_items()[1];
    assert_eq!(audio_copy.track_id(), tracks[1]);
    assert_eq!(
        audio_copy.start_sample(),
        app.project.sample_at_tick(720).unwrap()
    );
    assert_eq!(audio_copy.media_ref(), audio_before.media_ref());
    assert_eq!(
        audio_copy.source_offset_samples(),
        audio_before.source_offset_samples()
    );
    assert_eq!(audio_copy.length_samples(), audio_before.length_samples());
    assert_ne!(audio_copy.id(), audio_before.id());
    let midi_copy = &app.project.midi_items()[1];
    assert_eq!(midi_copy.track_id(), tracks[2]);
    assert_eq!(midi_copy.start_tick(), 1_200);
    assert_eq!(midi_copy.notes()[0].pitch(), 67);
    assert_ne!(midi_copy.id(), midi_before.id());
    assert_ne!(midi_copy.notes()[0].id(), midi_before.notes()[0].id());
    assert_eq!(app.revision, revision_before_copy + 1);

    let _ = app.update(Message::Undo);
    assert_eq!(
        app.project.audio_items(),
        std::slice::from_ref(&audio_before)
    );
    assert_eq!(app.project.midi_items(), std::slice::from_ref(&midi_before));
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items().len(), 2);
    assert_eq!(app.project.midi_items().len(), 2);
    assert_eq!(app.project.audio_items()[0], audio_before);
    assert_eq!(app.project.midi_items()[0], midi_before);
}

#[test]
fn audio_item_edge_trim_preserves_source_content_and_is_one_undoable_edit() {
    let mut app = App::default();
    app.timeline
        .handle(TimelineEvent::SetSnapGrid(SnapGrid::Sixteenth));
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    let start = app.project.sample_at_tick(480).unwrap();
    let end = app.project.sample_at_tick(1_440).unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://trim-test".to_owned(),
            start_sample: start,
            source_offset_samples: 256,
            length_samples: end - start,
        })
        .unwrap();
    app.audio_waveforms.insert(
        "asset://trim-test".to_owned(),
        silent_test_waveform(44_100, 100_000),
    );
    let item_id = app.project.audio_items()[0].id();
    let original = app.project.audio_items()[0].clone();
    app.timeline.rebuild(&app.project);

    app.timeline
        .handle(crate::timeline::TimelineEvent::BeginItemTrim {
            item_id,
            edge: crate::timeline::ItemTrimEdge::Start,
            target_tick: 700,
            ignore_snap: true,
        });
    assert_eq!(app.timeline.item_trim_preview().unwrap().start_tick, 700);
    app.timeline
        .handle(crate::timeline::TimelineEvent::CancelItemTrim);
    assert!(app.timeline.item_trim_preview().is_none());
    assert_eq!(app.project.audio_items()[0], original);

    app.timeline
        .handle(crate::timeline::TimelineEvent::BeginItemTrim {
            item_id,
            edge: crate::timeline::ItemTrimEdge::Start,
            target_tick: 650,
            ignore_snap: false,
        });
    let preview = app.timeline.item_trim_preview().unwrap();
    assert!(preview.valid);
    assert_eq!(preview.start_tick, 720);
    app.finish_item_trim();

    let trimmed = &app.project.audio_items()[0];
    let trimmed_start = app.project.sample_at_tick(720).unwrap();
    let trimmed_source_offset = trimmed.source_offset_samples();
    assert_eq!(trimmed.start_sample(), trimmed_start);
    assert_eq!(trimmed.source_offset_samples(), 5_768);
    assert_eq!(trimmed.start_sample() + trimmed.length_samples(), end);
    assert_eq!(app.revision, 2);

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0], original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items()[0].start_sample(), trimmed_start);

    app.timeline
        .handle(crate::timeline::TimelineEvent::BeginItemTrim {
            item_id,
            edge: crate::timeline::ItemTrimEdge::End,
            target_tick: 1_200,
            ignore_snap: false,
        });
    app.finish_item_trim();
    let right_trimmed = &app.project.audio_items()[0];
    let right_trimmed_start = right_trimmed.start_sample();
    let right_trimmed_offset = right_trimmed.source_offset_samples();
    assert_eq!(right_trimmed_start, trimmed_start);
    assert_eq!(right_trimmed_offset, trimmed_source_offset);
    assert_eq!(
        right_trimmed.start_sample() + right_trimmed.length_samples(),
        app.project.sample_at_tick(1_200).unwrap()
    );

    app.timeline
        .handle(crate::timeline::TimelineEvent::BeginItemTrim {
            item_id,
            edge: crate::timeline::ItemTrimEdge::Start,
            target_tick: 600,
            ignore_snap: true,
        });
    app.finish_item_trim();
    let restored_left = app.project.audio_items()[0].clone();
    let restored_start = app.project.sample_at_tick(600).unwrap();
    assert_eq!(restored_left.start_sample(), restored_start);
    assert_eq!(restored_left.source_offset_samples(), 3_012);
    assert_eq!(
        restored_left.source_offset_samples()
            + i64::try_from(
                super::scale_project_samples_to_source_frames_toward_zero(
                    i128::from(trimmed_start - restored_start),
                    44_100,
                    48_000,
                )
                .unwrap()
            )
            .unwrap(),
        right_trimmed_offset
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].start_sample(), trimmed_start);
    assert_eq!(
        app.project.audio_items()[0].source_offset_samples(),
        right_trimmed_offset
    );
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items()[0], restored_left);

    app.timeline
        .handle(crate::timeline::TimelineEvent::BeginItemTrim {
            item_id,
            edge: crate::timeline::ItemTrimEdge::End,
            target_tick: 1_300,
            ignore_snap: true,
        });
    app.finish_item_trim();
    let restored_right = app.project.audio_items()[0].clone();
    assert_eq!(
        restored_right.start_sample() + restored_right.length_samples(),
        app.project.sample_at_tick(1_300).unwrap()
    );
    assert_eq!(restored_right.source_offset_samples(), 3_012);
}

#[test]
fn invalid_audio_item_trim_changes_neither_project_nor_history() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://trim-invalid".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 1_000,
        })
        .unwrap();
    let item_id = app.project.audio_items()[0].id();
    let original = app.project.audio_items()[0].clone();
    app.timeline.rebuild(&app.project);
    app.timeline
        .handle(crate::timeline::TimelineEvent::BeginItemTrim {
            item_id,
            edge: crate::timeline::ItemTrimEdge::Start,
            target_tick: 100,
            ignore_snap: true,
        });
    assert!(!app.timeline.item_trim_preview().unwrap().valid);
    let revision = app.revision;
    app.finish_item_trim();
    assert_eq!(app.project.audio_items()[0], original);
    assert_eq!(app.revision, revision);
    assert!(app.status.contains("at least one sample"));
}

#[test]
fn midi_item_edge_trim_keeps_hidden_events_and_is_undoable() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 960,
            length_ticks: 1_920,
        })
        .unwrap();
    let item_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 64,
                tick: 1_200,
                duration: 480,
                velocity: 96,
            }],
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![MidiControllerData {
                controller: 1,
                tick: 1_680,
                value: 80,
            }],
        })
        .unwrap();
    let original = app.project.midi_items()[0].clone();
    app.timeline.rebuild(&app.project);

    app.handle_timeline_view_event(crate::timeline::TimelineEvent::BeginItemTrim {
        item_id,
        edge: crate::timeline::ItemTrimEdge::End,
        target_tick: 2_640,
        ignore_snap: true,
    });
    let preview = app.timeline.item_trim_preview().unwrap();
    assert!(preview.valid);
    assert_eq!(preview.end_tick, 2_640);
    app.finish_item_trim();

    let trimmed = &app.project.midi_items()[0];
    assert_eq!(trimmed.start_tick(), 960);
    assert_eq!(trimmed.length_ticks(), 1_680);
    assert_eq!(trimmed.notes(), original.notes());
    assert_eq!(trimmed.controllers(), original.controllers());
    assert_eq!(app.status, "MIDI item trimmed");

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0], original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.midi_items()[0].length_ticks(), 1_680);
    assert_eq!(app.project.midi_items()[0].notes(), original.notes());
    assert_eq!(
        app.project.midi_items()[0].controllers(),
        original.controllers()
    );
}

#[test]
fn midi_item_start_trim_preserves_content_position_and_can_expand_back() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 960,
            length_ticks: 1_920,
        })
        .unwrap();
    let item_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 64,
                tick: 1_200,
                duration: 240,
                velocity: 96,
            }],
        })
        .unwrap();
    let original = app.project.midi_items()[0].clone();
    app.timeline.rebuild(&app.project);

    app.handle_timeline_view_event(crate::timeline::TimelineEvent::BeginItemTrim {
        item_id,
        edge: crate::timeline::ItemTrimEdge::Start,
        target_tick: 480,
        ignore_snap: true,
    });
    assert!(!app.timeline.item_trim_preview().unwrap().valid);
    app.timeline
        .handle(crate::timeline::TimelineEvent::CancelItemTrim);

    app.handle_timeline_view_event(crate::timeline::TimelineEvent::BeginItemTrim {
        item_id,
        edge: crate::timeline::ItemTrimEdge::Start,
        target_tick: 1_440,
        ignore_snap: true,
    });
    assert!(app.timeline.item_trim_preview().unwrap().valid);
    app.finish_item_trim();
    let trimmed = &app.project.midi_items()[0];
    assert_eq!(trimmed.start_tick(), 1_440);
    assert_eq!(trimmed.length_ticks(), 1_440);
    assert_eq!(trimmed.source_offset_ticks(), 480);
    assert_eq!(trimmed.project_tick_at_content_tick(1_200), Some(2_160));
    assert_eq!(trimmed.notes(), original.notes());

    app.handle_timeline_view_event(crate::timeline::TimelineEvent::BeginItemTrim {
        item_id,
        edge: crate::timeline::ItemTrimEdge::Start,
        target_tick: 960,
        ignore_snap: true,
    });
    assert!(app.timeline.item_trim_preview().unwrap().valid);
    app.finish_item_trim();
    let expanded = &app.project.midi_items()[0];
    assert_eq!(expanded.start_tick(), 960);
    assert_eq!(expanded.source_offset_ticks(), 0);
    assert_eq!(expanded.project_tick_at_content_tick(1_200), Some(2_160));
}

#[test]
fn audio_item_trim_can_extend_past_source_eof_and_keeps_the_requested_length() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    let start = app.project.sample_at_tick(480).unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://trim-boundary".to_owned(),
            start_sample: start,
            source_offset_samples: 0,
            length_samples: 24_000,
        })
        .unwrap();
    let item_id = app.project.audio_items()[0].id();
    let original = app.project.audio_items()[0].clone();
    app.audio_waveforms.insert(
        "asset://trim-boundary".to_owned(),
        silent_test_waveform(48_000, 30_000),
    );
    app.timeline.rebuild(&app.project);

    app.handle_timeline_view_event(crate::timeline::TimelineEvent::BeginItemTrim {
        item_id,
        edge: crate::timeline::ItemTrimEdge::End,
        target_tick: 2_000,
        ignore_snap: true,
    });
    assert!(app.timeline.item_trim_preview().unwrap().valid);
    let revision = app.revision;
    app.finish_item_trim();

    let trimmed = &app.project.audio_items()[0];
    assert_ne!(trimmed, &original);
    assert_eq!(trimmed.start_sample(), original.start_sample());
    assert_eq!(
        trimmed.source_offset_samples(),
        original.source_offset_samples()
    );
    assert_eq!(
        trimmed.length_samples(),
        app.project.sample_at_tick(2_000).unwrap() - start
    );
    assert_eq!(app.revision, revision + 1);
}

#[test]
fn audio_left_trim_at_source_eof_keeps_the_new_source_range_valid() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://trim-eof".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .unwrap();
    let item_id = app.project.audio_items()[0].id();
    app.audio_waveforms.insert(
        "asset://trim-eof".to_owned(),
        silent_test_waveform(44_100, 44_100),
    );
    app.timeline.rebuild(&app.project);

    app.handle_timeline_view_event(crate::timeline::TimelineEvent::BeginItemTrim {
        item_id,
        edge: crate::timeline::ItemTrimEdge::Start,
        target_tick: 4,
        ignore_snap: true,
    });
    assert!(app.timeline.item_trim_preview().unwrap().valid);
    app.finish_item_trim();

    let trimmed = &app.project.audio_items()[0];
    let source_length = super::scale_project_samples_to_source_frames_ceil(
        i128::from(trimmed.length_samples()),
        44_100,
        48_000,
    )
    .unwrap();
    assert_eq!(
        i128::from(trimmed.source_offset_samples()) + source_length,
        44_100
    );
}

#[test]
fn audio_trim_maps_project_time_into_source_sample_rate() {
    let inward =
        super::scale_project_samples_to_source_frames_toward_zero(6_000, 44_100, 48_000).unwrap();
    let outward =
        super::scale_project_samples_to_source_frames_toward_zero(-6_000, 44_100, 48_000).unwrap();
    assert_eq!(inward, 5_512);
    assert_eq!(outward, -5_512);
    assert_eq!(inward + outward, 0);
    assert_eq!(
        super::scale_project_samples_to_source_frames_ceil(6_000, 44_100, 48_000).unwrap(),
        5_513
    );
    assert_eq!(
        super::scale_project_samples_to_source_frames_toward_zero(-3_000, 44_100, 48_000).unwrap(),
        -2_756
    );
}

#[test]
fn invalid_item_drop_does_not_change_project_or_create_history() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let source_track = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: source_track,
            media_ref: "asset://edge-item".to_owned(),
            start_sample: 0,
            source_offset_samples: 20,
            length_samples: 1_000,
        })
        .unwrap();
    let item_id = app.project.audio_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline
        .handle(crate::timeline::TimelineEvent::SelectItem {
            item_id: Some(item_id),
            additive: false,
            range: false,
        });
    app.timeline
        .handle(crate::timeline::TimelineEvent::BeginItemDrag {
            item_id,
            pointer_delta_ticks: -10,
            target_track_index: Some(1),
            range: false,
            ignore_snap: false,
            copy: false,
        });
    let revision_before_drop = app.revision;

    app.finish_item_drag();

    let item_after = &app.project.audio_items()[0];
    assert_eq!(item_after.track_id(), source_track);
    assert_eq!(item_after.start_sample(), 0);
    assert_eq!(item_after.source_offset_samples(), 20);
    assert_eq!(item_after.length_samples(), 1_000);
    assert_eq!(app.revision, revision_before_drop);
    assert_eq!(
        app.status,
        "Drop rejected: item would leave the project bounds"
    );
    assert!(app.project.undo().unwrap());
    assert!(app.project.audio_items().is_empty());
}

#[test]
fn audio_timeline_duplicate_preserves_source_offset_and_is_undoable() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://duplicate-test".to_owned(),
            start_sample: 240,
            source_offset_samples: 120,
            length_samples: 960,
        })
        .expect("source item should be inserted");
    let source_id = app.project.audio_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline.selected_item = Some(source_id);
    app.timeline.selected_items.insert(source_id);

    let _ = app.update(Message::ExecuteCommand(CommandId::DuplicateSelectedItem));
    let duplicate = &app.project.audio_items()[1];
    assert_eq!(duplicate.start_sample(), 1_200);
    assert_eq!(duplicate.media_ref(), "asset://duplicate-test");
    assert_eq!(duplicate.source_offset_samples(), 120);
    assert_eq!(duplicate.length_samples(), 960);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items().len(), 1);
    assert_eq!(app.project.audio_items()[0].id(), source_id);
}

#[test]
fn midi_item_duplicate_is_available_in_shared_command_surfaces_and_undoable() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 240,
            length_ticks: 960,
        })
        .expect("source MIDI item should be inserted");
    let source_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id: source_id,
            notes: vec![MidiNoteData {
                pitch: 60,
                tick: 120,
                duration: 240,
                velocity: 90,
            }],
        })
        .unwrap();
    app.timeline.rebuild(&app.project);
    app.timeline.selected_item = Some(source_id);
    app.timeline.selected_items.insert(source_id);

    let command = CommandId::DuplicateSelectedItem;
    assert!(commands::is_enabled(&app, command));
    assert_eq!(commands::find(&app, "duplicate item"), Some(command));
    assert!(
        commands::for_menu(&app, MainMenu::Item)
            .iter()
            .any(|entry| entry.id == command && entry.enabled)
    );
    assert_eq!(app.project.midi_items()[0].start_tick(), 240);
    let source_note_id = app.project.midi_items()[0].notes()[0].id();

    let _ = app.update(Message::ExecuteCommand(command));
    assert_eq!(app.project.midi_items().len(), 2);
    assert_eq!(app.project.midi_items()[1].start_tick(), 1_200);
    assert_ne!(app.project.midi_items()[1].notes()[0].id(), source_note_id);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items().len(), 1);
}

#[test]
fn duplicate_selected_audio_and_midi_items_is_one_undoable_action() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://multi-duplicate-test".to_owned(),
            start_sample: 240,
            source_offset_samples: 120,
            length_samples: 960,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 240,
            length_ticks: 960,
        })
        .unwrap();
    let audio_id = app.project.audio_items()[0].id();
    let midi_id = app.project.midi_items()[0].id();
    let audio_start_tick = app.project.tick_at_sample(240).unwrap();
    let audio_end_tick = app.project.tick_at_sample(1_200).unwrap();
    let midi_start_tick = 240;
    let midi_end_tick = midi_start_tick + 960;
    let group_start_tick = audio_start_tick.min(midi_start_tick);
    let group_end_tick = audio_end_tick.max(midi_end_tick);
    let group_offset = group_end_tick - group_start_tick;
    app.timeline.rebuild(&app.project);
    app.timeline.selected_item = Some(audio_id);
    app.timeline.selected_items.extend([audio_id, midi_id]);
    let revision_before_duplicate = app.revision;
    assert!(commands::is_enabled(&app, CommandId::DuplicateSelectedItem));

    let _ = app.update(Message::ExecuteCommand(CommandId::DuplicateSelectedItem));

    assert_eq!(app.project.audio_items().len(), 2);
    assert_eq!(app.project.midi_items().len(), 2);
    assert_eq!(
        app.project.midi_items()[1].start_tick(),
        midi_start_tick + group_offset
    );
    assert_eq!(
        app.project.audio_items()[1].start_sample(),
        app.project
            .sample_at_tick(audio_start_tick + group_offset)
            .unwrap()
    );
    assert_eq!(app.revision, revision_before_duplicate + 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items().len(), 1);
    assert_eq!(app.project.midi_items().len(), 1);
}

#[test]
fn select_all_items_action_selects_audio_and_midi_without_editing_the_project() {
    assert!(commands::is_enabled(
        &App::default(),
        CommandId::SelectAllItems
    ));
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let tracks = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[0],
            media_ref: "asset://select-all-items".to_owned(),
            start_sample: 24_000,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[1],
            start_tick: 960,
            length_ticks: 3_840,
        })
        .unwrap();
    let audio_id = app.project.audio_items()[0].id();
    let midi_id = app.project.midi_items()[0].id();
    app.timeline.selected_items.insert(audio_id);

    let command = CommandId::SelectAllItems;
    assert!(commands::is_enabled(&app, command));
    assert_eq!(commands::find(&app, "select all items"), Some(command));
    assert_eq!(
        commands::stable_id(command).as_deref(),
        Some("item.select-all")
    );
    app.action_list.set_query("select all items".to_owned());
    let entries = app.action_list_entries();
    assert_eq!(
        entries
            .iter()
            .filter_map(|entry| commands::stable_id(entry.id))
            .collect::<HashSet<_>>(),
        HashSet::from([
            "item.select-all".to_owned(),
            "item.select-all-in-track".to_owned(),
            "item.select-all-in-time-selection".to_owned(),
            "item.select-all-on-selected-tracks-in-time-selection".to_owned(),
            "item.unselect-all".to_owned(),
        ])
    );
    assert!(entries.iter().all(|entry| entry.shortcut.is_none()));
    app.action_list.select("item.select-all".to_owned());
    let revision_before = app.revision;
    let can_undo_before = app.project.can_undo();
    let can_redo_before = app.project.can_redo();

    let _ = app.update(Message::ActionListRun(false));

    assert_eq!(
        app.timeline.selected_items,
        HashSet::from([audio_id, midi_id])
    );
    assert_eq!(app.timeline.selected_item, Some(midi_id));
    assert_eq!(app.revision, revision_before);
    assert_eq!(app.project.can_undo(), can_undo_before);
    assert_eq!(app.project.can_redo(), can_redo_before);
    assert_eq!(app.project.audio_items().len(), 1);
    assert_eq!(app.project.midi_items().len(), 1);
}

#[test]
fn item_selection_undo_and_redo_stay_in_order_with_project_edits() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let tracks = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[0],
            media_ref: "asset://selection-history-audio".to_owned(),
            start_sample: 24_000,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[1],
            start_tick: 960,
            length_ticks: 3_840,
        })
        .unwrap();
    let audio_id = app.project.audio_items()[0].id();
    let midi_id = app.project.midi_items()[0].id();
    app.timeline.select_items(vec![(audio_id, tracks[0])]);

    let project_history_cursor = app.project.history_cursor();
    let _ = app.update(Message::SelectAllItems);
    assert_eq!(app.project.history_cursor(), project_history_cursor);
    assert_eq!(
        app.timeline.selected_items,
        HashSet::from([audio_id, midi_id])
    );

    let _ = app.update(Message::AddTrack);
    assert_eq!(app.project.tracks().len(), 3);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks().len(), 2);
    assert_eq!(
        app.timeline.selected_items,
        HashSet::from([audio_id, midi_id])
    );

    let _ = app.update(Message::Undo);
    assert_eq!(app.timeline.selected_items, HashSet::from([audio_id]));
    assert_eq!(app.timeline.selected_item, Some(audio_id));
    let _ = app.update(Message::Redo);
    assert_eq!(
        app.timeline.selected_items,
        HashSet::from([audio_id, midi_id])
    );
    assert_eq!(app.project.tracks().len(), 2);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.tracks().len(), 3);
}

#[test]
fn no_op_item_selection_does_not_discard_redo_history() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://selection-redo-audio".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .unwrap();

    let _ = app.update(Message::SelectAllItems);
    let _ = app.update(Message::Undo);
    assert!(app.can_redo_action());

    let _ = app.update(Message::UnselectAllItems);
    assert!(app.can_redo_action());

    let _ = app.update(Message::Redo);
    let item_id = app.project.audio_items()[0].id();
    assert_eq!(app.timeline.selected_items, HashSet::from([item_id]));

    let mut project_redo = App::default();
    let _ = project_redo.update(Message::AddTrack);
    let _ = project_redo.update(Message::AddTrack);
    let _ = project_redo.update(Message::Undo);
    assert!(project_redo.project.can_redo());
    let _ = project_redo.update(Message::UnselectAllItems);
    assert!(project_redo.project.can_redo());
    let _ = project_redo.update(Message::Redo);
    assert_eq!(project_redo.project.tracks().len(), 2);
}

#[test]
fn changed_item_selection_discards_project_redo_history() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://selection-branch-audio".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .unwrap();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::Undo);
    assert!(app.project.can_redo());

    let _ = app.update(Message::SelectAllItems);

    assert!(!app.project.can_redo());
    assert!(!app.can_redo_action());
    assert_eq!(app.project.tracks().len(), 1);
    assert_eq!(app.timeline.selected_items.len(), 1);
}

#[test]
fn invert_item_selection_action_selects_the_complement_of_audio_and_midi_items() {
    assert!(!commands::is_enabled(
        &App::default(),
        CommandId::InvertItemSelection
    ));
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let tracks = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[0],
            media_ref: "asset://invert-selection".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[0],
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[1],
            start_tick: 960,
            length_ticks: 960,
        })
        .unwrap();
    let audio_id = app.project.audio_items()[0].id();
    let midi_ids = app
        .project
        .midi_items()
        .iter()
        .map(|item| item.id())
        .collect::<Vec<_>>();
    app.timeline.rebuild(&app.project);
    app.timeline.selected_item = Some(audio_id);
    app.timeline.selected_items.insert(audio_id);

    let command = CommandId::InvertItemSelection;
    assert!(commands::is_enabled(&app, command));
    assert_eq!(commands::find(&app, "invert selection"), Some(command));
    assert_eq!(
        commands::stable_id(command).as_deref(),
        Some("item.invert-selection")
    );
    app.action_list.set_query("invert selection".to_owned());
    app.action_list.select("item.invert-selection".to_owned());
    assert_eq!(
        app.action_list_entries()
            .iter()
            .filter_map(|entry| commands::stable_id(entry.id))
            .collect::<Vec<_>>(),
        vec!["item.invert-selection".to_owned()]
    );
    let revision_before = app.revision;
    let can_undo_before = app.project.can_undo();
    let can_redo_before = app.project.can_redo();

    let _ = app.update(Message::ExecuteCommand(command));

    assert_eq!(
        app.timeline.selected_items,
        midi_ids.iter().copied().collect::<HashSet<_>>()
    );
    assert_eq!(app.timeline.selected_item, midi_ids.last().copied());
    assert_eq!(app.revision, revision_before);
    assert_eq!(app.project.can_undo(), can_undo_before);
    assert_eq!(app.project.can_redo(), can_redo_before);

    app.timeline.selected_items.clear();
    app.timeline.selected_items.insert(audio_id);
    app.timeline.selected_item = Some(audio_id);
    app.action_list.select("item.invert-selection".to_owned());
    let _ = app.update(Message::ActionListRun(false));
    assert_eq!(
        app.timeline.selected_items,
        midi_ids.iter().copied().collect::<HashSet<_>>()
    );
    assert_eq!(
        app.timeline.selected_item,
        Some(midi_ids.last().copied().unwrap())
    );
    let _ = app.update(Message::ExecuteCommand(command));
    assert_eq!(app.timeline.selected_items, HashSet::from([audio_id]));
    assert_eq!(app.timeline.selected_item, Some(audio_id));
    assert_eq!(app.revision, revision_before);
}

#[test]
fn adjacent_non_overlapping_item_actions_navigate_within_the_active_track() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let tracks = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    let audio_start = app.project.sample_at_tick(960).unwrap();
    let audio_end = app.project.sample_at_tick(1_920).unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[0],
            media_ref: "asset://adjacent-selection".to_owned(),
            start_sample: audio_start,
            source_offset_samples: 0,
            length_samples: audio_end - audio_start,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[0],
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[0],
            start_tick: 1_440,
            length_ticks: 240,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[0],
            start_tick: 2_500,
            length_ticks: 960,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[0],
            start_tick: 1_920,
            length_ticks: 480,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[1],
            start_tick: 1_920,
            length_ticks: 480,
        })
        .unwrap();
    let active_id = app.project.audio_items()[0].id();
    let previous_id = app
        .project
        .midi_items()
        .iter()
        .find(|item| item.track_id() == tracks[0] && item.start_tick() == 0)
        .unwrap()
        .id();
    let next_id = app
        .project
        .midi_items()
        .iter()
        .find(|item| item.track_id() == tracks[0] && item.start_tick() == 1_920)
        .unwrap()
        .id();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(active_id, tracks[0])]);
    let revision_before = app.revision;
    let can_undo_before = app.project.can_undo();
    let can_redo_before = app.project.can_redo();

    let next = CommandId::SelectNextAdjacentNonOverlappingItem;
    assert!(commands::is_enabled(&app, next));
    app.action_list.set_query("select next adjacent".to_owned());
    app.action_list
        .select("item.select-next-adjacent".to_owned());
    let _ = app.update(Message::ActionListRun(false));
    assert_eq!(app.timeline.selected_items, HashSet::from([next_id]));
    assert_eq!(app.timeline.selected_item, Some(next_id));
    assert!(commands::is_enabled(&app, next));

    app.timeline.select_items(vec![(active_id, tracks[0])]);
    let previous = CommandId::SelectPreviousAdjacentNonOverlappingItem;
    assert!(commands::is_enabled(&app, previous));
    app.action_list
        .set_query("select previous adjacent".to_owned());
    app.action_list
        .select("item.select-previous-adjacent".to_owned());
    let _ = app.update(Message::ActionListRun(false));
    assert_eq!(app.timeline.selected_items, HashSet::from([previous_id]));
    assert_eq!(app.timeline.selected_item, Some(previous_id));
    assert!(commands::is_enabled(&app, previous));

    assert_eq!(app.revision, revision_before);
    assert_eq!(app.project.can_undo(), can_undo_before);
    assert_eq!(app.project.can_redo(), can_redo_before);
}

#[test]
fn adjacent_item_actions_require_touching_boundaries_and_map_each_selection() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let tracks = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    let audio_start = app.project.sample_at_tick(960).unwrap();
    let audio_end = app.project.sample_at_tick(1_920).unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[0],
            media_ref: "asset://touching-boundary-selection".to_owned(),
            start_sample: audio_start,
            source_offset_samples: 0,
            length_samples: audio_end - audio_start,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[0],
            start_tick: 1_920,
            length_ticks: 480,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[0],
            start_tick: 2_400,
            length_ticks: 480,
        })
        .unwrap();
    let audio_id = app.project.audio_items()[0].id();
    let touching_id = app
        .project
        .midi_items()
        .iter()
        .find(|item| item.start_tick() == 1_920)
        .unwrap()
        .id();
    let gap_id = app
        .project
        .midi_items()
        .iter()
        .find(|item| item.start_tick() == 2_400)
        .unwrap()
        .id();
    app.timeline.rebuild(&app.project);
    app.timeline
        .select_items(vec![(audio_id, tracks[0]), (gap_id, tracks[0])]);
    let revision_before = app.revision;
    let can_undo_before = app.project.can_undo();
    let can_redo_before = app.project.can_redo();

    let next = CommandId::SelectNextAdjacentNonOverlappingItem;
    assert!(commands::is_enabled(&app, next));
    let _ = app.update(Message::ExecuteCommand(next));
    assert_eq!(app.timeline.selected_items, HashSet::from([touching_id]));
    assert_eq!(app.timeline.selected_item, Some(touching_id));

    app.timeline.select_items(vec![(audio_id, tracks[0])]);
    let _ = app.update(Message::ExecuteCommand(next));
    assert_eq!(app.timeline.selected_items, HashSet::from([touching_id]));

    app.timeline.select_items(vec![(gap_id, tracks[0])]);
    assert!(commands::is_enabled(&app, next));
    let _ = app.update(Message::ExecuteCommand(next));
    assert!(app.timeline.selected_items.is_empty());
    assert_eq!(app.timeline.selected_item, None);

    app.timeline.select_items(vec![(touching_id, tracks[0])]);
    let previous = CommandId::SelectPreviousAdjacentNonOverlappingItem;
    assert!(commands::is_enabled(&app, previous));
    let _ = app.update(Message::ExecuteCommand(previous));
    assert_eq!(app.timeline.selected_items, HashSet::from([audio_id]));

    assert_eq!(app.revision, revision_before);
    assert_eq!(app.project.can_undo(), can_undo_before);
    assert_eq!(app.project.can_redo(), can_redo_before);
}

#[test]
fn adjacent_track_navigation_uses_first_overlapping_item_and_preserves_track_selection() {
    let mut app = App::default();
    for _ in 0..4 {
        let _ = app.update(Message::AddTrack);
    }
    let tracks = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    let audio_start = app.project.sample_at_tick(960).unwrap();
    let audio_end = app.project.sample_at_tick(1_920).unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[0],
            media_ref: "asset://adjacent-track-previous-distractor".to_owned(),
            start_sample: audio_start,
            source_offset_samples: 0,
            length_samples: audio_end - audio_start,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[2],
            media_ref: "asset://adjacent-track-source".to_owned(),
            start_sample: audio_start,
            source_offset_samples: 0,
            length_samples: audio_end - audio_start,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[2],
            start_tick: 2_880,
            length_ticks: 480,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[3],
            start_tick: 960,
            length_ticks: 240,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[3],
            media_ref: "asset://adjacent-track-later-overlap".to_owned(),
            start_sample: app.project.sample_at_tick(1_440).unwrap(),
            source_offset_samples: 0,
            length_samples: app.project.sample_at_tick(2_400).unwrap()
                - app.project.sample_at_tick(1_440).unwrap(),
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[3],
            start_tick: 2_880,
            length_ticks: 240,
        })
        .unwrap();
    let source_audio = app.project.audio_items()[1].id();
    let source_midi = app.project.midi_items()[0].id();
    let previous_track_distractor = app.project.audio_items()[0].id();
    let next_track_first = app.project.midi_items()[1].id();
    let next_track_later = app.project.audio_items()[2].id();
    app.timeline.rebuild(&app.project);
    app.timeline.selected_track = Some(tracks[2]);
    app.timeline.selected_tracks = HashSet::from([tracks[2]]);
    app.timeline
        .select_items(vec![(source_midi, tracks[2]), (source_audio, tracks[2])]);
    app.timeline.selected_track = Some(tracks[2]);
    app.timeline.selected_tracks = HashSet::from([tracks[2]]);
    app.timeline.edit_cursor_tick = 777;
    let previous = CommandId::SelectItemOnPreviousTrack;
    assert!(commands::is_enabled(&app, previous));
    app.action_list
        .select("item.select-previous-track".to_owned());
    let _ = app.update(Message::ActionListRun(false));
    assert!(app.timeline.selected_items.is_empty());
    assert_eq!(app.timeline.edit_cursor_tick, 777);
    assert_eq!(app.timeline.selected_track, Some(tracks[2]));
    assert_eq!(app.timeline.selected_tracks, HashSet::from([tracks[2]]));

    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[1],
            start_tick: 960,
            length_ticks: 240,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[1],
            start_tick: 1_440,
            length_ticks: 480,
        })
        .unwrap();
    let previous_track_first = app
        .project
        .midi_items()
        .iter()
        .find(|item| item.track_id() == tracks[1] && item.start_tick() == 960)
        .unwrap()
        .id();
    let next_track_midi = app
        .project
        .midi_items()
        .iter()
        .find(|item| item.track_id() == tracks[3] && item.start_tick() == 2_880)
        .unwrap()
        .id();
    app.timeline.rebuild(&app.project);
    let revision_before = app.revision;
    let can_undo_before = app.project.can_undo();
    let can_redo_before = app.project.can_redo();
    app.timeline
        .select_items(vec![(source_midi, tracks[2]), (source_audio, tracks[2])]);
    app.timeline.selected_track = Some(tracks[2]);
    app.timeline.selected_tracks = HashSet::from([tracks[2]]);
    let _ = app.update(Message::ExecuteCommand(previous));
    assert_eq!(
        app.timeline.selected_items,
        HashSet::from([previous_track_first])
    );
    assert_eq!(app.timeline.edit_cursor_tick, 960);
    assert_eq!(app.timeline.selected_track, Some(tracks[2]));
    assert_eq!(app.timeline.selected_tracks, HashSet::from([tracks[2]]));
    assert_ne!(previous_track_first, previous_track_distractor);

    app.timeline.select_items(vec![(source_audio, tracks[2])]);
    app.timeline.selected_track = Some(tracks[2]);
    app.timeline.selected_tracks = HashSet::from([tracks[2]]);
    let next = CommandId::SelectItemOnNextTrack;
    assert!(commands::is_enabled(&app, next));
    app.action_list.select("item.select-next-track".to_owned());
    let _ = app.update(Message::ActionListRun(false));
    assert_eq!(
        app.timeline.selected_items,
        HashSet::from([next_track_first])
    );
    assert_eq!(app.timeline.edit_cursor_tick, 960);
    assert_eq!(app.timeline.selected_track, Some(tracks[2]));
    assert_eq!(app.timeline.selected_tracks, HashSet::from([tracks[2]]));
    assert_ne!(next_track_first, next_track_later);

    app.timeline.select_items(vec![(source_midi, tracks[2])]);
    app.timeline.selected_track = Some(tracks[2]);
    app.timeline.selected_tracks = HashSet::from([tracks[2]]);
    let _ = app.update(Message::ExecuteCommand(next));
    assert_eq!(
        app.timeline.selected_items,
        HashSet::from([next_track_midi])
    );
    assert_eq!(app.timeline.edit_cursor_tick, 2_880);
    assert_eq!(app.timeline.selected_track, Some(tracks[2]));
    assert_eq!(app.timeline.selected_tracks, HashSet::from([tracks[2]]));

    assert_eq!(app.revision, revision_before);
    assert_eq!(app.project.can_undo(), can_undo_before);
    assert_eq!(app.project.can_redo(), can_redo_before);
}

#[test]
fn item_edge_navigation_actions_move_to_nearest_audio_or_midi_boundary() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    let audio_start = app.project.sample_at_tick(960).unwrap();
    let audio_end = app.project.sample_at_tick(1_920).unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://item-edge-navigation".to_owned(),
            start_sample: audio_start,
            source_offset_samples: 0,
            length_samples: audio_end - audio_start,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 2_880,
            length_ticks: 960,
        })
        .unwrap();
    let selected_item = app.project.audio_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(selected_item, track_id)]);
    app.timeline.edit_cursor_tick = 2_500;
    let revision_before = app.revision;
    let can_undo_before = app.project.can_undo();
    let can_redo_before = app.project.can_redo();

    let left = CommandId::MoveEditCursorToPreviousItemEdge;
    assert!(commands::is_enabled(&app, left));
    app.action_list.set_query("nearest item edge".to_owned());
    app.action_list
        .select("item.navigate-cursor-left-edge".to_owned());
    let left_entry = app
        .action_list_entries()
        .into_iter()
        .find(|entry| entry.id == left)
        .expect("left edge navigation action is searchable");
    assert!(left_entry.shortcut.is_none());
    let _ = app.update(Message::ActionListRun(false));
    assert_eq!(app.timeline.edit_cursor_tick, 1_920);
    assert_eq!(app.timeline.selected_items, HashSet::from([selected_item]));

    app.timeline.edit_cursor_tick = 2_500;
    let right = CommandId::MoveEditCursorToNextItemEdge;
    assert!(commands::is_enabled(&app, right));
    app.action_list
        .select("item.navigate-cursor-right-edge".to_owned());
    let _ = app.update(Message::ActionListRun(false));
    assert_eq!(app.timeline.edit_cursor_tick, 2_880);
    assert_eq!(app.timeline.selected_items, HashSet::from([selected_item]));

    app.timeline.edit_cursor_tick = 0;
    assert!(!commands::is_enabled(&app, left));
    app.timeline.edit_cursor_tick = 3_840;
    assert!(!commands::is_enabled(&app, right));
    assert_eq!(app.revision, revision_before);
    assert_eq!(app.project.can_undo(), can_undo_before);
    assert_eq!(app.project.can_redo(), can_redo_before);
}

#[test]
fn selected_item_cursor_actions_use_outer_audio_midi_bounds() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let tracks = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    let audio_start = app.project.sample_at_tick(960).unwrap();
    let audio_end = app.project.sample_at_tick(1_920).unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[1],
            media_ref: "asset://selected-item-cursor-audio".to_owned(),
            start_sample: audio_start,
            source_offset_samples: 0,
            length_samples: audio_end - audio_start,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[0],
            start_tick: 2_880,
            length_ticks: 480,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[0],
            start_tick: 0,
            length_ticks: 480,
        })
        .unwrap();
    let audio_id = app.project.audio_items()[0].id();
    let selected_midi_id = app
        .project
        .midi_items()
        .iter()
        .find(|item| item.track_id() == tracks[0])
        .unwrap()
        .id();
    app.timeline.rebuild(&app.project);
    app.timeline
        .select_items(vec![(selected_midi_id, tracks[0]), (audio_id, tracks[1])]);
    app.timeline.selected_track = Some(tracks[1]);
    app.timeline.selected_tracks = HashSet::from([tracks[0], tracks[1]]);
    app.timeline.edit_cursor_tick = 5_000;
    let selected_items = app.timeline.selected_items.clone();
    let revision_before = app.revision;
    let can_undo_before = app.project.can_undo();
    let can_redo_before = app.project.can_redo();

    let start = CommandId::MoveEditCursorToStartOfItems;
    assert!(commands::is_enabled(&app, start));
    app.action_list
        .set_query("move cursor to start of items".to_owned());
    app.action_list
        .select("item.navigate-cursor-selected-start".to_owned());
    let start_entry = app
        .action_list_entries()
        .into_iter()
        .find(|entry| entry.id == start)
        .expect("selected item start action is searchable");
    assert!(start_entry.shortcut.is_none());
    let _ = app.update(Message::ActionListRun(false));
    assert_eq!(app.timeline.edit_cursor_tick, 960);
    assert_eq!(app.timeline.selected_items, selected_items);
    assert_eq!(app.timeline.selected_track, Some(tracks[1]));
    assert_eq!(
        app.timeline.selected_tracks,
        HashSet::from([tracks[0], tracks[1]])
    );

    let end = CommandId::MoveEditCursorToEndOfItems;
    assert!(commands::is_enabled(&app, end));
    app.action_list
        .set_query("move cursor to end of items".to_owned());
    app.action_list
        .select("item.navigate-cursor-selected-end".to_owned());
    let end_entry = app
        .action_list_entries()
        .into_iter()
        .find(|entry| entry.id == end)
        .expect("selected item end action is searchable");
    assert!(end_entry.shortcut.is_none());
    let _ = app.update(Message::ActionListRun(false));
    assert_eq!(app.timeline.edit_cursor_tick, 3_360);
    assert_eq!(app.timeline.selected_items, selected_items);
    assert_eq!(app.timeline.selected_track, Some(tracks[1]));
    assert_eq!(
        app.timeline.selected_tracks,
        HashSet::from([tracks[0], tracks[1]])
    );

    app.timeline.clear_selected_items();
    app.timeline.edit_cursor_tick = 1_111;
    assert!(!commands::is_enabled(&app, start));
    assert!(!commands::is_enabled(&app, end));
    let _ = app.update(Message::ExecuteCommand(start));
    let _ = app.update(Message::ExecuteCommand(end));
    assert_eq!(app.timeline.edit_cursor_tick, 1_111);
    assert!(app.timeline.selected_items.is_empty());
    assert_eq!(app.revision, revision_before);
    assert_eq!(app.project.can_undo(), can_undo_before);
    assert_eq!(app.project.can_redo(), can_redo_before);
}

#[test]
fn item_snap_actions_match_audio_and_midi_grid_directions_with_grouped_undo() {
    let cases = [
        (
            CommandId::SnapSelectedItemsLeft,
            "item.snap-left",
            "snap items left",
            60_960,
            5_242,
            48_000,
            4_800,
        ),
        (
            CommandId::SnapSelectedItemsRight,
            "item.snap-right",
            "snap items right",
            60_960,
            5_242,
            72_000,
            5_760,
        ),
        (
            CommandId::SnapSelectedItemsToNearestPoint,
            "item.snap-nearest",
            "snap items to nearest snap point",
            60_000,
            5_280,
            48_000,
            4_800,
        ),
    ];

    for (command, stable_id, label, original_audio, original_midi, target_audio, target_midi) in
        cases
    {
        let mut app = item_snap_test_app(original_audio, original_midi);
        let audio_id = app.project.audio_items()[0].id();
        let midi_id = app.project.midi_items()[0].id();
        let selected_items = app.timeline.selected_items.clone();
        let selected_track = app.timeline.selected_track;
        let selected_tracks = app.timeline.selected_tracks.clone();
        let cursor_tick = app.timeline.edit_cursor_tick;
        let revision_before = app.revision;
        assert!(commands::is_enabled(&app, command));
        assert_eq!(commands::stable_id(command).as_deref(), Some(stable_id));
        assert_eq!(commands::find(&app, label), Some(command));
        app.action_list.set_query(label.to_owned());
        app.action_list.select(stable_id.to_owned());
        let entry = app
            .action_list_entries()
            .into_iter()
            .find(|entry| entry.id == command)
            .expect("item snap action is searchable");
        assert!(entry.shortcut.is_none());

        let _ = app.update(Message::ActionListRun(false));

        assert_eq!(app.project.audio_items()[0].id(), audio_id);
        assert_eq!(app.project.audio_items()[0].start_sample(), target_audio);
        assert_eq!(app.project.audio_items()[0].source_offset_samples(), 120);
        assert_eq!(app.project.audio_items()[0].length_samples(), 24_000);
        assert_eq!(app.project.midi_items()[0].id(), midi_id);
        assert_eq!(app.project.midi_items()[0].start_tick(), target_midi);
        assert_eq!(app.project.midi_items()[0].length_ticks(), 960);
        assert_eq!(app.timeline.selected_items, selected_items);
        assert_eq!(app.timeline.selected_track, selected_track);
        assert_eq!(app.timeline.selected_tracks, selected_tracks);
        assert_eq!(app.timeline.edit_cursor_tick, cursor_tick);
        assert_eq!(app.revision, revision_before + 1);

        let _ = app.update(Message::Undo);
        assert_eq!(app.project.audio_items()[0].start_sample(), original_audio);
        assert_eq!(app.project.midi_items()[0].start_tick(), original_midi);
        assert_eq!(app.timeline.selected_items, selected_items);
        let _ = app.update(Message::Redo);
        assert_eq!(app.project.audio_items()[0].start_sample(), target_audio);
        assert_eq!(app.project.midi_items()[0].start_tick(), target_midi);
    }
}

#[test]
fn item_snap_nearest_uses_sample_grid_and_ignores_snap_toggle() {
    let mut app = item_snap_test_app(60_960, 5_242);
    let audio_id = app.project.audio_items()[0].id();
    let midi_id = app.project.midi_items()[0].id();
    app.timeline.snap_enabled = false;

    let command = CommandId::SnapSelectedItemsToNearestPoint;
    assert!(commands::is_enabled(&app, command));
    app.action_list.select("item.snap-nearest".to_owned());
    let _ = app.update(Message::ActionListRun(false));

    assert_eq!(app.project.audio_items()[0].id(), audio_id);
    assert_eq!(app.project.audio_items()[0].start_sample(), 72_000);
    assert_eq!(app.project.midi_items()[0].id(), midi_id);
    assert_eq!(app.project.midi_items()[0].start_tick(), 4_800);
}

#[test]
fn item_snap_exact_grid_empty_selection_and_busy_states_do_not_edit() {
    let mut app = item_snap_test_app(48_000, 4_800);
    let audio_id = app.project.audio_items()[0].id();
    let midi_id = app.project.midi_items()[0].id();
    let revision_before = app.revision;
    let can_undo_before = app.project.can_undo();
    let can_redo_before = app.project.can_redo();
    app.action_list.select("item.snap-nearest".to_owned());

    let _ = app.update(Message::ActionListRun(false));

    assert_eq!(app.project.audio_items()[0].start_sample(), 48_000);
    assert_eq!(app.project.midi_items()[0].start_tick(), 4_800);
    assert_eq!(app.revision, revision_before);
    assert_eq!(app.project.can_undo(), can_undo_before);
    assert_eq!(app.project.can_redo(), can_redo_before);

    app.timeline.clear_selected_items();
    for command in [
        CommandId::SnapSelectedItemsLeft,
        CommandId::SnapSelectedItemsRight,
        CommandId::SnapSelectedItemsToNearestPoint,
    ] {
        assert!(!commands::is_enabled(&app, command));
    }
    let _ = app.update(Message::SnapSelectedItemsLeft);
    let _ = app.update(Message::SnapSelectedItemsRight);
    let _ = app.update(Message::SnapSelectedItemsToNearestPoint);
    assert_eq!(app.project.audio_items()[0].id(), audio_id);
    assert_eq!(app.project.midi_items()[0].id(), midi_id);
    assert_eq!(app.revision, revision_before);

    app.timeline.select_items(vec![
        (audio_id, app.project.audio_items()[0].track_id()),
        (midi_id, app.project.midi_items()[0].track_id()),
    ]);
    app.io_busy = true;
    assert!(!commands::is_enabled(
        &app,
        CommandId::SnapSelectedItemsToNearestPoint
    ));
    let _ = app.update(Message::SnapSelectedItemsToNearestPoint);
    assert_eq!(app.project.audio_items()[0].start_sample(), 48_000);
    assert_eq!(app.project.midi_items()[0].start_tick(), 4_800);
    assert_eq!(app.revision, revision_before);
}

#[test]
fn select_all_items_in_track_action_selects_all_selected_tracks() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let tracks = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[0],
            media_ref: "asset://select-track-items".to_owned(),
            start_sample: 24_000,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[0],
            start_tick: 960,
            length_ticks: 3_840,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[1],
            media_ref: "asset://select-track-items-other".to_owned(),
            start_sample: 48_000,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .unwrap();
    let first_audio = app.project.audio_items()[0].id();
    let other_track_audio = app.project.audio_items()[1].id();
    let first_track_midi = app.project.midi_items()[0].id();
    app.timeline.selected_track = Some(tracks[0]);
    app.timeline.selected_tracks = HashSet::from([tracks[0], tracks[1]]);
    app.timeline.selected_items.insert(other_track_audio);

    let command = CommandId::SelectAllItemsInTrack;
    assert!(commands::is_enabled(&app, command));
    assert_eq!(
        commands::find(&app, "select all items in track"),
        Some(command)
    );
    assert_eq!(
        commands::stable_id(command).as_deref(),
        Some("item.select-all-in-track")
    );
    app.action_list
        .set_query("select all items in track".to_owned());
    let entries = app.action_list_entries();
    let entry = entries
        .iter()
        .find(|entry| entry.id == command)
        .expect("track action is searchable");
    assert!(entry.shortcut.is_none());
    app.action_list
        .select("item.select-all-in-track".to_owned());

    let _ = app.update(Message::ActionListRun(false));

    assert_eq!(
        app.timeline.selected_items,
        HashSet::from([first_audio, first_track_midi, other_track_audio])
    );
    assert_eq!(app.timeline.selected_item, Some(other_track_audio));
}

#[test]
fn select_all_items_in_time_selection_action_selects_overlapping_audio_and_midi() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let tracks = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[0],
            media_ref: "asset://time-selection-overlap".to_owned(),
            start_sample: 24_000,
            source_offset_samples: 0,
            length_samples: 24_000,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[1],
            start_tick: 2_700,
            length_ticks: 300,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[1],
            media_ref: "asset://time-selection-end-boundary".to_owned(),
            start_sample: 72_000,
            source_offset_samples: 0,
            length_samples: 24_000,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[2],
            start_tick: 2_880,
            length_ticks: 960,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[2],
            media_ref: "asset://time-selection-start-boundary".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 24_000,
        })
        .unwrap();

    let overlapping_audio = app.project.audio_items()[0].id();
    let end_boundary_audio = app.project.audio_items()[1].id();
    let start_boundary_audio = app.project.audio_items()[2].id();
    let overlapping_midi = app.project.midi_items()[0].id();
    let end_boundary_midi = app.project.midi_items()[1].id();
    app.timeline.time_selection = Some(crate::timeline::TimeSelection {
        start_tick: 960,
        end_tick: 2_880,
    });
    app.timeline.selected_items =
        HashSet::from([end_boundary_audio, end_boundary_midi, start_boundary_audio]);
    app.timeline.selected_item = Some(end_boundary_midi);

    let command = CommandId::SelectAllItemsInTimeSelection;
    assert!(commands::is_enabled(&app, command));
    assert_eq!(
        commands::find(&app, "select all items in current time selection"),
        Some(command)
    );
    assert_eq!(
        commands::stable_id(command).as_deref(),
        Some("item.select-all-in-time-selection")
    );
    app.action_list
        .set_query("select all items in current time selection".to_owned());
    let entries = app.action_list_entries();
    let entry = entries
        .iter()
        .find(|entry| entry.id == command)
        .expect("time-selection action is searchable");
    assert!(entry.shortcut.is_none());
    app.action_list
        .select("item.select-all-in-time-selection".to_owned());
    let revision_before = app.revision;
    let can_undo_before = app.project.can_undo();
    let can_redo_before = app.project.can_redo();

    let _ = app.update(Message::ActionListRun(false));

    assert_eq!(
        app.timeline.selected_items,
        HashSet::from([overlapping_audio, overlapping_midi])
    );
    assert_eq!(app.timeline.selected_item, Some(overlapping_midi));
    assert_eq!(app.revision, revision_before);
    assert_eq!(app.project.can_undo(), can_undo_before);
    assert_eq!(app.project.can_redo(), can_redo_before);

    app.timeline.time_selection = None;
    assert!(!commands::is_enabled(&app, command));
    app.timeline.time_selection = Some(crate::timeline::TimeSelection {
        start_tick: 5_000,
        end_tick: 6_000,
    });
    assert!(!commands::is_enabled(&app, command));
}

#[test]
fn select_all_items_on_selected_tracks_in_time_selection_action_respects_multi_track_scope() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let tracks = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[0],
            media_ref: "asset://selected-track-first".to_owned(),
            start_sample: 24_000,
            source_offset_samples: 0,
            length_samples: 24_000,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[1],
            start_tick: 1_200,
            length_ticks: 600,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[2],
            media_ref: "asset://selected-track-last".to_owned(),
            start_sample: 48_000,
            source_offset_samples: 0,
            length_samples: 12_000,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[2],
            start_tick: 4_000,
            length_ticks: 500,
        })
        .unwrap();

    let first_audio = app.project.audio_items()[0].id();
    let unselected_track_midi = app.project.midi_items()[0].id();
    let selected_track_audio = app.project.audio_items()[1].id();
    let out_of_range_midi = app.project.midi_items()[1].id();
    app.timeline.time_selection = Some(crate::timeline::TimeSelection {
        start_tick: 1_000,
        end_tick: 3_000,
    });
    app.timeline.selected_tracks = HashSet::from([tracks[0], tracks[2]]);
    app.timeline.selected_track = Some(tracks[2]);
    app.timeline.selected_items = HashSet::from([unselected_track_midi, out_of_range_midi]);

    let command = CommandId::SelectAllItemsOnSelectedTracksInTimeSelection;
    assert!(commands::is_enabled(&app, command));
    assert_eq!(
        commands::find(
            &app,
            "select all items on selected tracks in current time selection"
        ),
        Some(command)
    );
    assert_eq!(
        commands::stable_id(command).as_deref(),
        Some("item.select-all-on-selected-tracks-in-time-selection")
    );
    app.action_list
        .set_query("select all items on selected tracks in current time selection".to_owned());
    let entries = app.action_list_entries();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].shortcut.is_none());
    app.action_list
        .select("item.select-all-on-selected-tracks-in-time-selection".to_owned());
    let revision_before = app.revision;
    let can_undo_before = app.project.can_undo();
    let can_redo_before = app.project.can_redo();

    let _ = app.update(Message::ActionListRun(false));

    assert_eq!(
        app.timeline.selected_items,
        HashSet::from([first_audio, selected_track_audio])
    );
    assert_eq!(app.timeline.selected_item, Some(selected_track_audio));
    assert_eq!(app.revision, revision_before);
    assert_eq!(app.project.can_undo(), can_undo_before);
    assert_eq!(app.project.can_redo(), can_redo_before);

    app.timeline.selected_tracks.clear();
    app.timeline.selected_track = None;
    assert!(!commands::is_enabled(&app, command));
}

#[test]
fn unselect_all_items_action_clears_item_selection_and_preserves_other_view_state() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let tracks = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[0],
            media_ref: "asset://unselect-items".to_owned(),
            start_sample: 24_000,
            source_offset_samples: 0,
            length_samples: 24_000,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[1],
            start_tick: 1_200,
            length_ticks: 960,
        })
        .unwrap();
    let audio_id = app.project.audio_items()[0].id();
    let midi_id = app.project.midi_items()[0].id();
    app.timeline
        .select_items(vec![(audio_id, tracks[0]), (midi_id, tracks[1])]);
    app.timeline.selected_track = Some(tracks[1]);
    app.timeline.selected_tracks = HashSet::from([tracks[0], tracks[1]]);
    app.timeline.time_selection = Some(crate::timeline::TimeSelection {
        start_tick: 960,
        end_tick: 2_400,
    });

    let command = CommandId::UnselectAllItems;
    assert!(commands::is_enabled(&app, command));
    assert_eq!(commands::find(&app, "unselect all items"), Some(command));
    assert_eq!(
        commands::stable_id(command).as_deref(),
        Some("item.unselect-all")
    );
    app.action_list.set_query("unselect all items".to_owned());
    let entries = app.action_list_entries();
    let entry = entries
        .iter()
        .find(|entry| entry.id == command)
        .expect("unselect action is searchable");
    assert!(entry.shortcut.is_none());
    app.action_list.select("item.unselect-all".to_owned());
    let revision_before = app.revision;
    let can_undo_before = app.project.can_undo();
    let can_redo_before = app.project.can_redo();
    let time_selection_before = app.timeline.time_selection;
    let selected_track_before = app.timeline.selected_track;
    let selected_tracks_before = app.timeline.selected_tracks.clone();

    let _ = app.update(Message::ActionListRun(false));

    assert!(app.timeline.selected_items.is_empty());
    assert_eq!(app.timeline.selected_item, None);
    assert_eq!(app.timeline.time_selection, time_selection_before);
    assert_eq!(app.timeline.selected_track, selected_track_before);
    assert_eq!(app.timeline.selected_tracks, selected_tracks_before);
    assert_eq!(app.revision, revision_before);
    assert_eq!(app.project.can_undo(), can_undo_before);
    assert_eq!(app.project.can_redo(), can_redo_before);
    assert!(!commands::is_enabled(&app, command));
}

#[test]
fn select_item_under_mouse_action_uses_current_hover_and_replaces_selection() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let tracks = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    for (track_id, start_tick) in [(tracks[0], 240), (tracks[1], 1_200)] {
        app.project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick,
                length_ticks: 480,
            })
            .unwrap();
    }
    let hovered_id = app.project.midi_items()[0].id();
    let previously_selected_id = app.project.midi_items()[1].id();
    app.timeline.rebuild(&app.project);
    app.timeline
        .select_items(vec![(previously_selected_id, tracks[1])]);
    app.timeline.hovered_item = Some(hovered_id);

    let command = CommandId::SelectItemUnderMouseCursor;
    assert!(commands::is_enabled(&app, command));
    assert_eq!(
        commands::find(&app, "select item under mouse cursor"),
        Some(command)
    );
    assert_eq!(
        commands::stable_id(command).as_deref(),
        Some("item.select-under-mouse")
    );
    app.action_list
        .set_query("select item under mouse cursor".to_owned());
    let entry = app
        .action_list_entries()
        .into_iter()
        .find(|entry| entry.id == command)
        .expect("under-mouse action is searchable");
    assert!(entry.shortcut.is_none());
    app.action_list.select("item.select-under-mouse".to_owned());
    let revision_before = app.revision;
    let can_undo_before = app.project.can_undo();
    let can_redo_before = app.project.can_redo();

    let _ = app.update(Message::ActionListRun(false));

    assert_eq!(app.timeline.selected_items, HashSet::from([hovered_id]));
    assert_eq!(app.timeline.selected_item, Some(hovered_id));
    assert_eq!(app.timeline.selected_track, Some(tracks[0]));
    assert_eq!(app.revision, revision_before);
    assert_eq!(app.project.can_undo(), can_undo_before);
    assert_eq!(app.project.can_redo(), can_redo_before);

    app.timeline.hovered_item = None;
    assert!(!commands::is_enabled(&app, command));
}

#[test]
fn select_item_under_mouse_leaving_other_items_selected_adds_without_toggling() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let tracks = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    for (track_id, start_tick) in [(tracks[0], 240), (tracks[1], 1_200)] {
        app.project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick,
                length_ticks: 480,
            })
            .unwrap();
    }
    let previously_selected_id = app.project.midi_items()[0].id();
    let hovered_id = app.project.midi_items()[1].id();
    app.timeline.rebuild(&app.project);
    app.timeline
        .select_items(vec![(previously_selected_id, tracks[0])]);
    app.timeline.hovered_item = Some(hovered_id);

    let command = CommandId::AddItemUnderMouseToSelection;
    assert!(commands::is_enabled(&app, command));
    assert_eq!(
        commands::find(
            &app,
            "select item under mouse cursor (leaving other items selected)"
        ),
        Some(command)
    );
    assert_eq!(
        commands::stable_id(command).as_deref(),
        Some("item.add-under-mouse-to-selection")
    );
    app.action_list
        .set_query("leaving other items selected".to_owned());
    let entry = app
        .action_list_entries()
        .into_iter()
        .find(|entry| entry.id == command)
        .expect("add-under-mouse action is searchable");
    assert!(entry.shortcut.is_none());
    app.action_list
        .select("item.add-under-mouse-to-selection".to_owned());
    let revision_before = app.revision;
    let can_undo_before = app.project.can_undo();
    let can_redo_before = app.project.can_redo();

    for _ in 0..2 {
        let _ = app.update(Message::ActionListRun(false));
        assert_eq!(
            app.timeline.selected_items,
            HashSet::from([previously_selected_id, hovered_id])
        );
        assert_eq!(app.timeline.selected_item, Some(hovered_id));
    }
    assert_eq!(app.revision, revision_before);
    assert_eq!(app.project.can_undo(), can_undo_before);
    assert_eq!(app.project.can_redo(), can_redo_before);
}

#[test]
fn toggle_selection_of_item_under_mouse_adds_and_removes_the_hovered_item() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let tracks = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    for (track_id, start_tick) in [(tracks[0], 240), (tracks[1], 1_200)] {
        app.project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick,
                length_ticks: 480,
            })
            .unwrap();
    }
    let retained_id = app.project.midi_items()[0].id();
    let hovered_id = app.project.midi_items()[1].id();
    app.timeline.rebuild(&app.project);
    app.timeline
        .select_items(vec![(retained_id, tracks[0]), (hovered_id, tracks[1])]);
    app.timeline.hovered_item = Some(hovered_id);

    let command = CommandId::ToggleSelectionOfItemUnderMouse;
    assert!(commands::is_enabled(&app, command));
    assert_eq!(
        commands::find(&app, "toggle selection of item under mouse cursor"),
        Some(command)
    );
    assert_eq!(
        commands::stable_id(command).as_deref(),
        Some("item.toggle-under-mouse-selection")
    );
    app.action_list
        .set_query("toggle selection of item under mouse cursor".to_owned());
    let entry = app
        .action_list_entries()
        .into_iter()
        .find(|entry| entry.id == command)
        .expect("toggle-under-mouse action is searchable");
    assert!(entry.shortcut.is_none());
    app.action_list
        .select("item.toggle-under-mouse-selection".to_owned());
    let revision_before = app.revision;
    let can_undo_before = app.project.can_undo();
    let can_redo_before = app.project.can_redo();

    let _ = app.update(Message::ActionListRun(false));

    assert_eq!(app.timeline.selected_items, HashSet::from([retained_id]));
    assert_eq!(app.timeline.selected_item, Some(retained_id));

    let _ = app.update(Message::ActionListRun(false));

    assert_eq!(
        app.timeline.selected_items,
        HashSet::from([retained_id, hovered_id])
    );
    assert_eq!(app.timeline.selected_item, Some(hovered_id));
    assert_eq!(app.revision, revision_before);
    assert_eq!(app.project.can_undo(), can_undo_before);
    assert_eq!(app.project.can_redo(), can_redo_before);
}

#[test]
fn duplicate_selected_audio_group_preserves_sample_spacing() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    for (media_ref, start_sample, length_samples) in
        [("asset://first", 241, 10), ("asset://second", 300, 20)]
    {
        app.project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: media_ref.to_owned(),
                start_sample,
                source_offset_samples: 0,
                length_samples,
            })
            .unwrap();
    }
    let selected = app
        .project
        .audio_items()
        .iter()
        .map(|item| item.id())
        .collect::<Vec<_>>();
    app.timeline.rebuild(&app.project);
    app.timeline.selected_item = selected.first().copied();
    app.timeline.selected_items.extend(selected);

    let _ = app.update(Message::ExecuteCommand(CommandId::DuplicateSelectedItem));

    assert_eq!(app.project.audio_items().len(), 4);
    assert_eq!(app.project.audio_items()[2].start_sample(), 320);
    assert_eq!(app.project.audio_items()[3].start_sample(), 379);
}

#[test]
fn audio_timeline_delete_is_undoable() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://delete-test".to_owned(),
            start_sample: 32,
            source_offset_samples: 4,
            length_samples: 256,
        })
        .expect("test item should be inserted");
    let item_id = app.project.audio_items()[0].id();

    let _ = app.update(Message::DeleteAudioItem(item_id));
    assert!(app.project.audio_items().is_empty());
    let _ = app.update(Message::Undo);
    let item = &app.project.audio_items()[0];
    assert_eq!(item.id(), item_id);
    assert_eq!(item.media_ref(), "asset://delete-test");
    assert_eq!(item.start_sample(), 32);
    assert_eq!(item.source_offset_samples(), 4);
    assert_eq!(item.length_samples(), 256);
}

#[test]
fn deleting_selected_audio_and_midi_items_is_one_undoable_menu_action() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let tracks = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: tracks[0],
            media_ref: "asset://menu-delete".to_owned(),
            start_sample: 0,
            source_offset_samples: 64,
            length_samples: 512,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id: tracks[1],
            start_tick: 960,
            length_ticks: 3_840,
        })
        .unwrap();
    let audio_id = app.project.audio_items()[0].id();
    let midi_id = app.project.midi_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline
        .handle(crate::timeline::TimelineEvent::SelectItem {
            item_id: Some(audio_id),
            additive: false,
            range: false,
        });
    app.timeline
        .handle(crate::timeline::TimelineEvent::SelectItem {
            item_id: Some(midi_id),
            additive: true,
            range: false,
        });
    let _ = app.update(Message::ToggleMainMenu(MainMenu::Item));

    let _ = app.update(Message::DeleteSelectedItems);

    assert!(app.project.audio_items().is_empty());
    assert!(app.project.midi_items().is_empty());
    assert_eq!(app.active_menu, None);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].id(), audio_id);
    assert_eq!(app.project.audio_items()[0].source_offset_samples(), 64);
    assert_eq!(app.project.midi_items()[0].id(), midi_id);
    assert_eq!(app.project.midi_items()[0].track_id(), tracks[1]);
}

#[test]
fn audio_timeline_nudge_is_undoable_and_cannot_cross_sample_zero() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://nudge-test".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 256,
        })
        .expect("test item should be inserted");
    let item_id = app.project.audio_items()[0].id();

    let _ = app.update(Message::NudgeAudioItem(item_id, 1, 1_000));
    assert_eq!(app.project.audio_items()[0].start_sample(), 48_000);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].start_sample(), 0);
    let _ = app.update(Message::NudgeAudioItem(item_id, 1, 10));
    assert_eq!(app.project.audio_items()[0].start_sample(), 480);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].start_sample(), 0);
    let _ = app.update(Message::NudgeAudioItem(item_id, -1, 10));
    assert_eq!(app.project.audio_items()[0].start_sample(), 0);
}

#[test]
fn completed_audio_import_places_item_through_project_action() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.import_busy = true;

    let _ = app.update(Message::AudioImportFinished(Ok(
        DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://test-audio".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 128,
        },
    )));

    assert_eq!(app.project.audio_items().len(), 1);
    assert_eq!(app.project.audio_items()[0].start_sample(), 0);
    assert_eq!(app.project.audio_items()[0].length_samples(), 128);
    assert_eq!(app.revision, 2);
    assert!(!app.import_busy);
}

#[test]
fn unsaved_session_can_start_audio_import_into_its_temporary_store() {
    let root = tempfile::tempdir().unwrap();
    let session = super::create_unsaved_project_session_dir_in(root.path()).unwrap();
    let store_path = session.store_path();
    let source_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/first-project/melody.wav");
    let mut app = App {
        session_media_dir: Some(session),
        audio_file_path_query: source_path.to_string_lossy().into_owned(),
        ..App::default()
    };
    let _ = app.update(Message::AddTrack);

    let _task = app.start_audio_import();

    assert!(app.project_path.is_none());
    assert_eq!(app.media_store_path(), Some(store_path.clone()));
    assert!(store_path.is_file());
    assert!(app.import_busy);
    assert!(app.status.starts_with("Starting import of "));
}

#[test]
fn unsaved_session_can_queue_project_rendering() {
    let root = tempfile::tempdir().unwrap();
    let session = super::create_unsaved_project_session_dir_in(root.path()).unwrap();
    let store_path = session.store_path();
    let destination = root.path().join("render.wav");
    let mut app = App {
        session_media_dir: Some(session),
        ..App::default()
    };
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Render content".to_owned(),
        })
        .expect("track should be created for rendering");
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .expect("render should have a non-empty project range");

    let _task = app.start_offline_render(destination);

    assert!(app.project_path.is_none());
    assert_eq!(app.media_store_path(), Some(store_path));
    assert!(app.offline_render_busy);
    assert!(!app.io_busy);
    assert_eq!(app.status, "Render render.wav…");
}

#[cfg(feature = "audio-device")]
#[test]
fn unsaved_session_can_start_recording_preparation() {
    let root = tempfile::tempdir().unwrap();
    let session = super::create_unsaved_project_session_dir_in(root.path()).unwrap();
    let store_path = session.store_path();
    let mut app = App {
        session_media_dir: Some(session),
        ..App::default()
    };
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    let _ = app.update(Message::ToggleRecordArm(track_id));

    let _task = app.update(Message::StartRecording);

    assert!(app.project_path.is_none());
    assert_eq!(app.media_store_path(), Some(store_path));
    assert!(app.recording_starting);
    assert!(app.playback_busy);
}

#[test]
fn audio_import_targets_the_selected_track_and_edit_cursor_sample() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let first_track = app.project.tracks()[0].id();
    let _ = app.update(Message::AddTrack);
    let selected_track = app.project.tracks()[1].id();
    app.timeline.selected_track = Some(selected_track);
    app.timeline.edit_cursor_tick = 1_440;
    let expected_sample = app.project.sample_at_tick(1_440).unwrap();

    let captured = app.audio_import_placement().unwrap();
    app.timeline.selected_track = Some(first_track);
    app.timeline.edit_cursor_tick = 0;

    assert_eq!(captured, (selected_track, expected_sample));
}

#[test]
fn audio_import_falls_back_to_first_audio_track_and_rejects_bus_only_projects() {
    let mut app = App::default();
    let _ = app.update(Message::AddBusTrack);
    let _ = app.update(Message::AddTrack);
    let first_audio_track = app.project.tracks()[1].id();
    app.timeline.selected_track = None;
    assert_eq!(
        app.audio_import_placement().unwrap(),
        (first_audio_track, 0)
    );

    let mut bus_only = App::default();
    let _ = bus_only.update(Message::AddBusTrack);
    assert_eq!(
        bus_only.audio_import_placement().unwrap_err(),
        "Select an audio track before importing audio"
    );
    bus_only.timeline.selected_track = None;
    assert_eq!(
        bus_only.audio_import_placement().unwrap_err(),
        "Add an audio track before importing audio"
    );
    assert_eq!(
        App::default().audio_import_placement().unwrap_err(),
        "Add an audio track before importing audio"
    );
}

#[test]
fn recorded_take_places_one_shared_asset_on_all_captured_armed_tracks_in_one_undo_step() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let first_track = app.project.tracks()[0].id();
    let _ = app.update(Message::AddTrack);
    let second_track = app.project.tracks()[1].id();
    let _ = app.update(Message::ToggleRecordArm(first_track));
    let _ = app.update(Message::ToggleRecordArm(second_track));
    let source = std::env::temp_dir().join(format!(
        "aaadaw-recorded-take-test-{}-{}.wav",
        std::process::id(),
        NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let project_path = source.with_extension("aaadaw");
    app.project_path = Some(project_path.clone());
    let recording_start_sample = super::audio_config::apply_recording_offset(
        96_000,
        app.project.settings().sample_rate(),
        500,
    )
    .expect("positive calibration offset should fit the project timeline");
    app.record_import_tracks = Some(super::RecordImportTarget {
        track_ids: vec![first_track, second_track],
        recreated_track_ids: Vec::new(),
        source_paths: vec![source.clone()],
        next_segment_index: 0,
        next_start_sample: recording_start_sample,
        imported_actions: Vec::new(),
        project_path,
        sample_rate: app.project.settings().sample_rate(),
        recovery_manifest_path: source.with_extension("recovery.json"),
        project_generation: app.project_generation,
        recovery_discarded_frames: 0,
        recovery_discarded_tail_bytes: 0,
        recovery_start_sample_is_estimate: false,
    });
    app.import_busy = true;

    let _ = app.finish_audio_import(Ok(DawAction::InsertAudioItem {
        track_id: first_track,
        media_ref: "asset://recorded-take".to_owned(),
        start_sample: recording_start_sample,
        source_offset_samples: 0,
        length_samples: 48_000,
    }));

    assert_eq!(app.project.audio_items().len(), 2);
    assert!(app.project.audio_items().iter().all(|item| {
        item.start_sample() == recording_start_sample && item.media_ref() == "asset://recorded-take"
    }));
    let _ = app.update(Message::Undo);
    assert!(app.project.audio_items().is_empty());
    let _ = app.update(Message::ProjectSaved(
        app.project_path.clone().expect("project path is set"),
        app.revision,
        Ok(()),
        None,
        SharedProjectSessionLock::new(None),
    ));
    assert_eq!(app.pending_recording_cleanup.len(), 1);
    let imported_revision = app.pending_recording_cleanup[0].saved_revision;
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items().len(), 2);
    assert!(app.record_import_tracks.is_none());
    assert_eq!(app.pending_recording_cleanup.len(), 1);
    assert_eq!(
        app.pending_recording_cleanup[0].saved_revision,
        imported_revision
    );
    let _ = app.update(Message::ProjectSaved(
        app.project_path.clone().expect("project path is set"),
        app.revision,
        Ok(()),
        None,
        SharedProjectSessionLock::new(None),
    ));
    let manifest_path = app.pending_recording_cleanup[0].manifest_path.clone();
    assert_eq!(
        app.pending_recording_cleanup[0].media_refs,
        ["asset://recorded-take"]
    );
    let _ = app.update(Message::RecordingRecoveryCleaned(Ok(vec![manifest_path])));
    assert!(app.pending_recording_cleanup.is_empty());
}

#[test]
fn invalid_recording_offset_edit_keeps_the_last_applied_value() {
    let mut app = App::default();
    app.audio_settings.recording_offset_us = -250;
    app.audio_recording_offset_query = Some("5000.001".to_owned());

    let _ = app.update(Message::ApplyRecordingOffset);

    assert_eq!(app.audio_settings.recording_offset_us, -250);
    assert!(
        app.audio_settings_feedback
            .contains("up to 3 decimal places")
    );
}

#[test]
fn recovered_recording_keeps_the_calibrated_anchor_for_import() {
    let directory = tempfile::tempdir().expect("test directory should be created");
    let project_path = directory.path().join("recording.aaadaw");
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Calibrated take".to_owned(),
        })
        .expect("track should be created");
    project
        .apply(DawAction::CreateTrack {
            index: 1,
            name: "Second armed track".to_owned(),
        })
        .expect("second track should be created");
    save_project_file(project_path.clone(), project.snapshot(), false)
        .expect("project should be saved before recording");
    let track_id = project.tracks()[0].id();
    let second_track_id = project.tracks()[1].id();
    let sample_rate = project.settings().sample_rate();
    let provisional_start = super::audio_config::apply_recording_offset(96_000, sample_rate, 500)
        .expect("calibrated provisional position should fit the project timeline");
    let capture_transport_sample =
        super::audio_config::apply_recording_offset(96_512, sample_rate, 500)
            .expect("calibrated capture position should fit the project timeline");
    let capture_anchor = aaadaw_app::CaptureTimelineAnchor::new(
        10_000,
        capture_transport_sample,
        sample_rate,
        sample_rate,
    )
    .expect("capture clock should map to the project sample rate");
    let capture_start = capture_anchor
        .project_sample_at(10_024)
        .expect("first callback should map to a project sample");

    let (mut producer, consumer, control) = aaadaw_app::audio_capture_stream(16);
    let worker = aaadaw_app::AudioRecordingWorker::start_recoverable(
        &project_path,
        sample_rate,
        vec![track_id.value(), second_track_id.value()],
        consumer,
        control.clone(),
    )
    .expect("recoverable recording should start");
    let manifest_path = worker
        .recovery_manifest_path()
        .expect("recording should create recovery metadata")
        .to_path_buf();
    worker
        .set_start_sample(provisional_start)
        .expect("corrected provisional anchor should be durable before capture");
    worker
        .set_capture_timeline_anchor(capture_anchor)
        .expect("capture clock anchor should be queued before capture starts");
    control.start();
    producer.push_planar_at(10_024, &[0.25, 0.5], &[-0.25, -0.5]);
    control.fail();
    assert!(matches!(
        worker.finish(),
        Err(aaadaw_app::AudioRecordingError::CaptureFailed { .. })
    ));

    let candidate = aaadaw_app::scan_recording_recoveries(&project_path)
        .expect("interrupted take should be discoverable")
        .pop()
        .expect("interrupted take should have a recovery candidate");
    let candidate = aaadaw_app::recover_recording_candidate(candidate)
        .expect("interrupted audio should be prepared for import");
    assert_eq!(candidate.manifest.start_sample, Some(capture_start));
    assert!(!candidate.manifest.start_sample_is_estimate);

    let mut app = App {
        project,
        project_path: Some(project_path.clone()),
        ..App::default()
    };
    let _task = app.recording_recovery_prepared(Ok(candidate));
    assert_eq!(
        app.record_import_tracks
            .as_ref()
            .expect("recovery should start importing the take")
            .next_start_sample,
        capture_start
    );
    let _task = app.finish_audio_import(Ok(DawAction::InsertAudioItem {
        track_id,
        media_ref: "asset://recovered-calibrated-take".to_owned(),
        start_sample: 0,
        source_offset_samples: 0,
        length_samples: 48_000,
    }));
    assert_eq!(app.project.audio_items().len(), 2);
    assert!(app.project.audio_items().iter().all(|item| {
        item.start_sample() == capture_start
            && item.media_ref() == "asset://recovered-calibrated-take"
    }));
    let _ = app.update(Message::Undo);
    assert!(app.project.audio_items().is_empty());
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items().len(), 2);
    assert!(app.project.audio_items().iter().all(|item| {
        item.start_sample() == capture_start
            && item.media_ref() == "asset://recovered-calibrated-take"
    }));

    aaadaw_app::discard_recording_recovery(&manifest_path)
        .expect("test recovery sources should be cleaned up");
}

#[test]
fn segmented_recording_import_places_contiguous_segments_in_one_undo_step() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let first_track = app.project.tracks()[0].id();
    let _ = app.update(Message::AddTrack);
    let second_track = app.project.tracks()[1].id();
    let source_paths = vec![
        std::env::temp_dir().join("aaadaw-segment-one.wav"),
        std::env::temp_dir().join("aaadaw-segment-two.wav"),
    ];
    app.record_import_tracks = Some(super::RecordImportTarget {
        track_ids: vec![first_track, second_track],
        recreated_track_ids: Vec::new(),
        source_paths,
        next_segment_index: 0,
        next_start_sample: 4_800,
        imported_actions: Vec::new(),
        project_path: std::env::temp_dir().join("aaadaw-segmented-test.aaadaw"),
        sample_rate: app.project.settings().sample_rate(),
        recovery_manifest_path: std::env::temp_dir().join("aaadaw-segmented-test.recovery.json"),
        project_generation: app.project_generation,
        recovery_discarded_frames: 0,
        recovery_discarded_tail_bytes: 0,
        recovery_start_sample_is_estimate: true,
    });
    app.import_busy = true;

    let _ = app.finish_audio_import(Ok(DawAction::InsertAudioItem {
        track_id: first_track,
        media_ref: "asset://segment-one".to_owned(),
        start_sample: 4_800,
        source_offset_samples: 0,
        length_samples: 2_400,
    }));
    let target = app
        .record_import_tracks
        .as_ref()
        .expect("next segment is pending");
    assert_eq!(target.next_segment_index, 1);
    assert_eq!(target.next_start_sample, 7_200);

    let _ = app.finish_audio_import(Ok(DawAction::InsertAudioItem {
        track_id: first_track,
        media_ref: "asset://segment-two".to_owned(),
        start_sample: 7_200,
        source_offset_samples: 0,
        length_samples: 1_200,
    }));
    assert_eq!(app.project.audio_items().len(), 4);
    assert!(app.project.audio_items().iter().any(|item| {
        item.track_id() == first_track
            && item.start_sample() == 4_800
            && item.media_ref() == "asset://segment-one"
    }));
    assert!(app.project.audio_items().iter().any(|item| {
        item.track_id() == first_track
            && item.start_sample() == 7_200
            && item.media_ref() == "asset://segment-two"
    }));
    assert!(app.project.audio_items().iter().any(|item| {
        item.track_id() == second_track
            && item.start_sample() == 4_800
            && item.media_ref() == "asset://segment-one"
    }));
    assert!(app.project.audio_items().iter().any(|item| {
        item.track_id() == second_track
            && item.start_sample() == 7_200
            && item.media_ref() == "asset://segment-two"
    }));
    assert!(app.status.contains("start position is approximate"));
    assert!(app.record_import_tracks.is_none());
    let _ = app.update(Message::Undo);
    assert!(app.project.audio_items().is_empty());
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items().len(), 4);
}

#[test]
fn failed_recording_import_keeps_recovery_sources_for_retry() {
    let file_id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
    let project_path = std::env::temp_dir().join(format!(
        "aaadaw-recording-retry-{}-{file_id}.aaadaw",
        std::process::id()
    ));
    let manifest_path =
        project_path.with_file_name(format!(".aaadaw-recording-retry-{file_id}.recovery.json"));
    let source_path = project_path.with_file_name(format!(".aaadaw-recording-retry-{file_id}.wav"));
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Recorded".to_owned(),
        })
        .expect("track should be created");
    save_project_file(project_path.clone(), project.snapshot(), false)
        .expect("project should be saved before recording");
    std::fs::write(&manifest_path, b"recoverable metadata")
        .expect("recovery manifest should be written");
    std::fs::write(&source_path, b"recorded segment").expect("recorded source should be written");

    let mut app = App {
        project,
        project_path: Some(project_path.clone()),
        ..App::default()
    };
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::CreateTrack {
            index: 1,
            name: "Recovered track".to_owned(),
        })
        .expect("unsaved recovery track should be created");
    let recreated_track_id = app.project.tracks()[1].id();
    app.record_import_tracks = Some(super::RecordImportTarget {
        track_ids: vec![recreated_track_id],
        recreated_track_ids: vec![recreated_track_id],
        source_paths: vec![source_path.clone()],
        next_segment_index: 0,
        next_start_sample: 0,
        imported_actions: Vec::new(),
        project_path: project_path.clone(),
        sample_rate: app.project.settings().sample_rate(),
        recovery_manifest_path: manifest_path.clone(),
        project_generation: app.project_generation,
        recovery_discarded_frames: 0,
        recovery_discarded_tail_bytes: 0,
        recovery_start_sample_is_estimate: false,
    });
    app.import_busy = true;
    let _ = app.finish_audio_import(Err("simulated interrupted import".to_owned()));

    assert!(source_path.is_file());
    assert!(manifest_path.is_file());
    assert!(app.record_import_tracks.is_none());
    assert_eq!(app.project.tracks().len(), 1);
    assert_eq!(app.project.tracks()[0].id(), track_id);
    assert!(app.status.contains("simulated interrupted import"));

    for path in [&project_path, &manifest_path, &source_path] {
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn failed_recovery_placement_removes_recreated_tracks() {
    let directory = tempfile::tempdir().expect("test directory should be created");
    let project_path = directory.path().join("recording.aaadaw");
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Saved track".to_owned(),
        })
        .expect("saved track should be created");
    save_project_file(project_path.clone(), project.snapshot(), false)
        .expect("project should be saved");
    let invalid_track_id = {
        let index = project.tracks().len();
        project
            .apply(DawAction::CreateTrack {
                index,
                name: "Missing track".to_owned(),
            })
            .expect("temporary track should be created");
        let track_id = project.tracks()[index].id();
        project
            .apply(DawAction::DeleteTrack { track_id })
            .expect("temporary track should be removed");
        track_id
    };
    project
        .apply(DawAction::CreateTrack {
            index: project.tracks().len(),
            name: "Restored track".to_owned(),
        })
        .expect("recreated track should be created");
    let recreated_track_id = project.tracks()[1].id();
    let mut app = App {
        project,
        project_path: Some(project_path.clone()),
        ..App::default()
    };
    app.record_import_tracks = Some(super::RecordImportTarget {
        track_ids: vec![invalid_track_id],
        recreated_track_ids: vec![recreated_track_id],
        source_paths: vec![directory.path().join("recovered.wav")],
        next_segment_index: 0,
        next_start_sample: 0,
        imported_actions: Vec::new(),
        project_path,
        sample_rate: app.project.settings().sample_rate(),
        recovery_manifest_path: directory.path().join("recording.recovery.json"),
        project_generation: app.project_generation,
        recovery_discarded_frames: 0,
        recovery_discarded_tail_bytes: 0,
        recovery_start_sample_is_estimate: false,
    });
    app.import_busy = true;

    let _ = app.finish_audio_import(Ok(DawAction::InsertAudioItem {
        track_id: invalid_track_id,
        media_ref: "asset://failed-recovery-placement".to_owned(),
        start_sample: 0,
        source_offset_samples: 0,
        length_samples: 48_000,
    }));

    assert_eq!(app.project.tracks().len(), 1);
    assert_eq!(app.project.tracks()[0].name(), "Saved track");
    assert!(app.project.audio_items().is_empty());
    assert!(app.status.contains("Action failed"));
}

#[test]
fn save_as_target_overrides_the_current_project_path() {
    let current = std::path::Path::new("/projects/current.aaadaw");
    let selected = std::path::PathBuf::from("/projects/dialog.aaadaw");

    let resolved = super::project_io::resolve_save_target(
        Some(current),
        "/projects/typed.aaadaw",
        Some(selected.clone()),
    );

    assert_eq!(resolved, Some((selected, false)));
}

#[test]
fn save_as_allows_overwrite_only_for_the_current_file() {
    let current = std::path::Path::new("/projects/current.aaadaw");
    let selected_current = std::path::PathBuf::from("/projects/current.aaadaw");
    let selected_other = std::path::PathBuf::from("/projects/other.aaadaw");

    assert_eq!(
        super::project_io::resolve_save_target(Some(current), "", None),
        Some((current.to_path_buf(), true))
    );
    assert_eq!(
        super::project_io::resolve_save_target(
            Some(current),
            "/projects/typed.aaadaw",
            Some(selected_current.clone()),
        ),
        Some((selected_current, true))
    );
    assert_eq!(
        super::project_io::resolve_save_target(
            Some(current),
            "/projects/typed.aaadaw",
            Some(selected_other.clone()),
        ),
        Some((selected_other, false))
    );
    assert_eq!(
        super::project_io::resolve_save_target(None, "/projects/typed.aaadaw", None),
        Some((std::path::PathBuf::from("/projects/typed.aaadaw"), false))
    );
}

#[test]
fn next_project_version_path_matches_reaper_padding_and_skips_existing_versions() {
    let directory = tempfile::tempdir().unwrap();
    let current = directory.path().join("Mix.aaadaw");
    let first = directory.path().join("Mix_001.aaadaw");
    let second = directory.path().join("Mix_002.aaadaw");
    let third = directory.path().join("Mix_003.aaadaw");

    assert_eq!(
        super::project_io::next_project_version_path(&current),
        Ok(first.clone())
    );
    std::fs::write(&first, b"occupied").unwrap();
    assert_eq!(
        super::project_io::next_project_version_path(&current),
        Ok(second.clone())
    );
    assert_eq!(
        super::project_io::next_project_version_path(&first),
        Ok(second.clone())
    );
    std::fs::write(&second, b"occupied").unwrap();
    assert_eq!(
        super::project_io::next_project_version_path(&current),
        Ok(third)
    );
}

#[test]
fn saving_a_new_project_version_preserves_the_source_and_copies_current_state() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("Mix.aaadaw");
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    save_project_file(source.clone(), app.project.snapshot(), false)
        .expect("the source project should be saved");
    let _ = app.update(Message::AddTrack);

    let version = super::project_io::next_project_version_path(&source).unwrap();
    let mut version_lock = ProjectSessionLock::acquire(&version).unwrap();
    save_project_session_file_with_media(
        version.clone(),
        app.project.snapshot(),
        app.timeline.arrangement_view_state(&app.project),
        false,
        Some(&mut version_lock),
        Some(source.clone()),
    )
    .expect("the new version should be saved without replacing the source");

    let original = load_project_file(source.clone()).unwrap();
    let saved_version = load_project_file(version.clone()).unwrap();
    assert!(source.is_file());
    assert!(version.is_file());
    assert_eq!(original.tracks().len(), 1);
    assert_eq!(saved_version.tracks().len(), 2);
}

#[test]
fn dirty_project_cannot_be_replaced_by_open() {
    let mut app = App {
        project_path_query: "/projects/another.aaadaw".to_owned(),
        revision: 1,
        ..App::default()
    };

    let _ = super::project_io::open_project(&mut app);

    assert!(!app.io_busy);
    assert_eq!(app.status, "Save current project before opening another");
}

#[test]
fn dirty_project_open_command_does_not_open_picker() {
    let mut app = App {
        project_path_query: "/projects/another.aaadaw".to_owned(),
        revision: 1,
        ..App::default()
    };

    let _ = app.update(Message::OpenProject);

    assert!(!app.io_busy);
    assert!(!app.path_picker_busy);
    assert_eq!(
        app.pending_project_transition,
        Some(super::PendingProjectTransition::OpenProject)
    );
    assert_eq!(app.status, "Save changes to the current project?");
}

#[test]
fn project_file_save_and_open_round_trip_core_snapshot() {
    let file_id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("aaadaw-ui-{}-{file_id}.aaadaw", std::process::id()));
    let mut project = Project::new();
    project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Persisted".to_owned(),
        })
        .expect("track should be created");
    let expected = project.snapshot();

    save_project_file(path.clone(), expected.clone(), false).expect("new project file should save");
    assert!(save_project_file(path.clone(), Project::new().snapshot(), false).is_err());
    let loaded = load_project_file(path.clone()).expect("project should open");
    assert_eq!(loaded.snapshot(), expected);

    let _ = std::fs::remove_file(&path);
    for suffix in ["-wal", "-shm"] {
        let sidecar = format!("{}{suffix}", path.display());
        let _ = std::fs::remove_file(sidecar);
    }
}

#[test]
fn split_item_commands_are_selection_aware_and_share_menu_search_definitions() {
    let mut app = App::default();
    let cursor = CommandId::SplitSelectedItemsAtCursor;
    let time_selection = CommandId::SplitSelectedItemsAtTimeSelection;
    assert!(commands::is_enabled(&app, cursor));
    assert!(!commands::is_enabled(&app, time_selection));

    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 3_840,
        })
        .unwrap();
    let item_id = app.project.midi_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline.selected_item = Some(item_id);
    app.timeline.selected_items.insert(item_id);
    app.timeline.edit_cursor_tick = 960;
    assert!(commands::is_enabled(&app, cursor));
    assert!(!commands::is_enabled(&app, time_selection));

    app.timeline
        .handle(crate::timeline::TimelineEvent::SetTimeSelection {
            start_tick: 960,
            end_tick: 2_880,
        });
    assert!(commands::is_enabled(&app, time_selection));
    let item_menu = commands::for_menu(&app, MainMenu::Item);
    for command in [cursor, time_selection] {
        let entry = item_menu
            .iter()
            .find(|entry| entry.id == command)
            .expect("Item menu should list the split command");
        assert_eq!(entry.enabled, commands::is_enabled(&app, command));
        assert!(entry.enabled);
    }
    let split_at_cursor = item_menu
        .iter()
        .find(|entry| entry.id == cursor)
        .expect("Item menu should expose Split items at cursor");
    assert_eq!(split_at_cursor.label, "Split items at cursor");
    assert_eq!(split_at_cursor.shortcut.as_deref(), Some("S"));
    let cursor_action = commands::matching_actions_menu(&app, "split items at edit or play cursor")
        .into_iter()
        .find(|entry| entry.id == cursor)
        .expect("Action List should use the native edit/play cursor label");
    assert_eq!(
        cursor_action.label,
        "Split items at edit or play cursor (select right)"
    );
    assert_eq!(cursor_action.shortcut.as_deref(), Some("S"));
    let split_at_time_selection = item_menu
        .iter()
        .find(|entry| entry.id == time_selection)
        .expect("Item menu should expose Split items at time selection");
    assert_eq!(
        split_at_time_selection.label,
        "Split items at time selection"
    );
    assert_eq!(split_at_time_selection.shortcut.as_deref(), Some("Shift+S"));
    assert_eq!(commands::find(&app, "split at cursor"), Some(cursor));
    assert_eq!(
        commands::find(&app, "split at time selection"),
        Some(time_selection)
    );

    app.timeline.edit_cursor_tick = 0;
    app.timeline.time_selection = None;
    assert!(commands::is_enabled(&app, cursor));
    app.io_busy = true;
    app.timeline.edit_cursor_tick = 960;
    assert!(!commands::is_enabled(&app, cursor));
}

#[test]
fn time_selection_splits_mixed_items_once_and_preserves_audio_and_midi_content() {
    let file_id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
    let project_path = std::env::temp_dir().join(format!(
        "aaadaw-ui-split-{}-{file_id}.aaadaw",
        std::process::id()
    ));
    let mut store = ProjectStore::open(&project_path).expect("project store should open");
    let media_ref = "asset://split-test";
    store
        .import_audio_asset(media_ref, "split-test.wav", std::io::Cursor::new([0_u8; 4]))
        .expect("test asset should import");
    store
        .set_audio_asset_metadata(
            media_ref,
            &aaadaw_storage::AudioAssetMetadata {
                container: "wav".to_owned(),
                codec: "pcm".to_owned(),
                sample_rate: Some(44_100),
                channel_count: Some(1),
                bits_per_sample: Some(16),
                frame_count: Some(100_000),
                duration_nanos: None,
                byte_len: Some(4),
            },
        )
        .expect("test metadata should store");
    store.close().expect("project store should close");
    let mut app = App {
        project_path: Some(project_path.clone()),
        ..App::default()
    };
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    let audio_end = app.project.sample_at_tick(3_840).unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: media_ref.to_owned(),
            start_sample: 0,
            source_offset_samples: 5_000,
            length_samples: audio_end,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 3_840,
        })
        .unwrap();
    let _ = app.update(Message::AddTrack);
    let unselected_track_id = app.project.tracks()[1].id();
    let unselected_item_length = app.project.sample_at_tick(3_840).unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: unselected_track_id,
            media_ref: media_ref.to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: unselected_item_length,
        })
        .unwrap();
    let midi_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id: midi_id,
            notes: vec![
                MidiNoteData {
                    pitch: 60,
                    tick: 0,
                    duration: 1_200,
                    velocity: 100,
                },
                MidiNoteData {
                    pitch: 64,
                    tick: 1_600,
                    duration: 1_600,
                    velocity: 90,
                },
            ],
        })
        .unwrap();
    let audio_id = app.project.audio_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline.selected_item = Some(midi_id);
    app.timeline.selected_items.extend([audio_id, midi_id]);
    app.timeline
        .handle(crate::timeline::TimelineEvent::SetTimeSelection {
            start_tick: 960,
            end_tick: 2_880,
        });
    let before = app.project.snapshot();
    let revision = app.revision;

    let _ = app.update(Message::ExecuteCommand(
        CommandId::SplitSelectedItemsAtTimeSelection,
    ));

    assert_eq!(app.revision, revision + 1);
    let cut_samples = [
        app.project.sample_at_tick(960).unwrap(),
        app.project.sample_at_tick(2_880).unwrap(),
    ];
    let audio = app.project.audio_items();
    assert_eq!(audio.len(), 4);
    let split_audio = audio
        .iter()
        .filter(|item| item.track_id() == track_id)
        .collect::<Vec<_>>();
    assert_eq!(split_audio.len(), 3);
    assert_eq!(
        split_audio
            .iter()
            .map(|item| (
                item.start_sample(),
                item.source_offset_samples(),
                item.length_samples()
            ))
            .collect::<Vec<_>>(),
        [
            (0, 5_000, cut_samples[0]),
            (
                cut_samples[0],
                i64::try_from(5_000 + (cut_samples[0] * 44_100 + 24_000) / 48_000).unwrap(),
                cut_samples[1] - cut_samples[0]
            ),
            (
                cut_samples[1],
                i64::try_from(5_000 + (cut_samples[1] * 44_100 + 24_000) / 48_000).unwrap(),
                audio_end - cut_samples[1]
            ),
        ]
    );
    let unselected_audio = audio
        .iter()
        .find(|item| item.track_id() == unselected_track_id)
        .expect("unselected track item should remain intact");
    assert_eq!(unselected_audio.start_sample(), 0);
    assert_eq!(unselected_audio.length_samples(), audio_end);
    assert!(!app.timeline.selected_items.contains(&unselected_audio.id()));
    let midi = app.project.midi_items();
    assert_eq!(
        midi.iter()
            .map(|item| (item.start_tick(), item.length_ticks()))
            .collect::<Vec<_>>(),
        [(0, 960), (960, 1_920), (2_880, 960)]
    );
    assert_eq!(
        midi.iter()
            .map(|item| {
                item.notes()
                    .iter()
                    .map(|note| (note.pitch(), note.tick(), note.duration()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>(),
        [
            vec![(60, 0, 960)],
            vec![(60, 0, 240), (64, 640, 1_280)],
            vec![(64, 0, 320)],
        ]
    );
    assert_eq!(app.timeline.selected_items.len(), 2);
    assert!(app.timeline.selected_items.contains(&split_audio[1].id()));
    assert!(app.timeline.selected_items.contains(&midi[1].id()));
    assert!(!app.timeline.selected_items.contains(&split_audio[0].id()));
    assert!(!app.timeline.selected_items.contains(&split_audio[2].id()));
    assert!(!app.timeline.selected_items.contains(&midi[0].id()));
    assert!(!app.timeline.selected_items.contains(&midi[2].id()));
    assert_eq!(
        app.timeline.time_selection,
        Some(crate::timeline::TimeSelection {
            start_tick: 960,
            end_tick: 2_880,
        })
    );

    let after = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after);
    let _ = std::fs::remove_file(&project_path);
    for suffix in ["-wal", "-shm"] {
        let sidecar = format!("{}{suffix}", project_path.display());
        let _ = std::fs::remove_file(sidecar);
    }
}

#[cfg(feature = "audio-device")]
#[test]
fn standby_input_completion_clears_pending_state_during_other_background_jobs() {
    let mut app = App {
        standby_monitor_starting: true,
        io_busy: true,
        import_busy: true,
        audio_asset_management_busy: true,
        recording_recovery_busy: true,
        playback_busy: true,
        ..App::default()
    };

    let _ = app.update(Message::StandbyInputClosed);

    assert!(!app.standby_monitor_starting);
}

#[test]
fn cursor_split_runs_from_action_search_and_boundary_noops_do_not_change_history() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 480,
            length_ticks: 1_920,
        })
        .unwrap();
    let item_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 60,
                tick: 0,
                duration: 1_440,
                velocity: 100,
            }],
        })
        .unwrap();
    app.timeline.rebuild(&app.project);
    app.timeline.selected_item = Some(item_id);
    app.timeline.selected_items.insert(item_id);
    app.timeline.edit_cursor_tick = 1_440;
    let before = app.project.snapshot();
    let revision = app.revision;

    let _ = app.update(Message::ActionQueryChanged("split at cursor".to_owned()));
    let _ = app.update(Message::RunActionQuery);

    assert_eq!(app.project.midi_items().len(), 2);
    assert_eq!(app.project.midi_items()[0].length_ticks(), 960);
    assert_eq!(app.project.midi_items()[0].notes()[0].duration(), 960);
    assert_eq!(app.project.midi_items()[1].start_tick(), 1_440);
    assert_eq!(app.project.midi_items()[1].notes()[0].tick(), 0);
    assert_eq!(app.project.midi_items()[1].notes()[0].duration(), 480);
    assert_eq!(
        app.timeline.selected_items,
        HashSet::from([app.project.midi_items()[1].id()])
    );
    assert_eq!(app.revision, revision + 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);

    app.timeline.edit_cursor_tick = 480;
    let before_noop = app.project.snapshot();
    let revision = app.revision;
    assert!(commands::is_enabled(
        &app,
        CommandId::SplitSelectedItemsAtCursor
    ));
    let _ = app.update(Message::SplitSelectedItemsAtCursor);
    assert_eq!(app.project.snapshot(), before_noop);
    assert_eq!(app.revision, revision);
}

#[test]
fn cursor_split_without_selection_splits_all_tracks_and_selects_right_segments_once() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let track_ids = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    for track_id in &track_ids {
        app.project
            .apply(DawAction::InsertMidiItem {
                track_id: *track_id,
                start_tick: 0,
                length_ticks: 3_840,
            })
            .unwrap();
    }
    app.timeline.rebuild(&app.project);
    app.timeline.edit_cursor_tick = 1_920;
    assert!(app.timeline.selected_items.is_empty());
    let before = app.project.snapshot();
    let revision = app.revision;

    let _ = app.update(Message::SplitSelectedItemsAtCursor);

    assert_eq!(app.project.midi_items().len(), 4);
    for track_id in track_ids {
        let fragments = app
            .project
            .midi_items()
            .iter()
            .filter(|item| item.track_id() == track_id)
            .collect::<Vec<_>>();
        assert_eq!(fragments.len(), 2);
        assert_eq!(
            (fragments[0].start_tick(), fragments[0].length_ticks()),
            (0, 1_920)
        );
        assert_eq!(
            (fragments[1].start_tick(), fragments[1].length_ticks()),
            (1_920, 1_920)
        );
        assert!(!app.timeline.selected_items.contains(&fragments[0].id()));
        assert!(app.timeline.selected_items.contains(&fragments[1].id()));
    }
    assert_eq!(app.timeline.selected_items.len(), 2);
    assert_eq!(app.revision, revision + 1);
    let after = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after);

    app.timeline.selected_items.clear();
    app.timeline.selected_item = None;
    app.timeline.edit_cursor_tick = 1_920;
    let before_boundary = app.project.snapshot();
    let revision = app.revision;
    let _ = app.update(Message::SplitSelectedItemsAtCursor);
    assert_eq!(app.project.snapshot(), before_boundary);
    assert_eq!(app.revision, revision);
}

#[test]
fn cursor_split_without_selection_preserves_audio_source_rate_and_selects_right_item() {
    let file_id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
    let project_path = std::env::temp_dir().join(format!(
        "aaadaw-ui-cursor-split-{}-{file_id}.aaadaw",
        std::process::id()
    ));
    let media_ref = "asset://cursor-split-test";
    let mut store = ProjectStore::open(&project_path).expect("project store should open");
    store
        .import_audio_asset(
            media_ref,
            "cursor-split-test.wav",
            std::io::Cursor::new([0_u8; 4]),
        )
        .expect("test asset should import");
    store
        .set_audio_asset_metadata(
            media_ref,
            &aaadaw_storage::AudioAssetMetadata {
                container: "wav".to_owned(),
                codec: "pcm".to_owned(),
                sample_rate: Some(44_100),
                channel_count: Some(1),
                bits_per_sample: Some(16),
                frame_count: Some(100_000),
                duration_nanos: None,
                byte_len: Some(4),
            },
        )
        .expect("test audio source metadata should store");
    store.close().expect("project store should close");

    let mut app = App {
        project_path: Some(project_path.clone()),
        ..App::default()
    };
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    let item_end = app.project.sample_at_tick(3_840).unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: media_ref.to_owned(),
            start_sample: 0,
            source_offset_samples: 5_000,
            length_samples: item_end,
        })
        .unwrap();
    app.timeline.rebuild(&app.project);
    app.timeline.edit_cursor_tick = 1_920;
    let before = app.project.snapshot();
    let revision = app.revision;

    let _ = app.update(Message::SplitSelectedItemsAtCursor);

    let items = app.project.audio_items();
    assert_eq!(items.len(), 2);
    assert_eq!(
        (items[0].start_sample(), items[0].length_samples()),
        (0, 48_000)
    );
    assert_eq!(items[0].source_offset_samples(), 5_000);
    assert_eq!(
        (items[1].start_sample(), items[1].length_samples()),
        (48_000, 48_000)
    );
    assert_eq!(items[1].source_offset_samples(), 49_100);
    assert_eq!(app.timeline.selected_items, HashSet::from([items[1].id()]));
    assert_eq!(app.revision, revision + 1);
    let after = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after);

    drop(app);
    let _ = std::fs::remove_file(&project_path);
    for suffix in ["-wal", "-shm"] {
        let sidecar = format!("{}{suffix}", project_path.display());
        let _ = std::fs::remove_file(sidecar);
    }
}

#[test]
fn heal_splits_only_merges_selected_contiguous_audio_sections_undoably() {
    let file_id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
    let project_path = std::env::temp_dir().join(format!(
        "aaadaw-ui-heal-splits-{}-{file_id}.aaadaw",
        std::process::id()
    ));
    let media_ref = "asset://heal-splits-test";
    let mut store = ProjectStore::open(&project_path).expect("project store should open");
    store
        .import_audio_asset(
            media_ref,
            "heal-splits-test.wav",
            std::io::Cursor::new([0_u8; 4]),
        )
        .expect("test asset should import");
    store
        .set_audio_asset_metadata(
            media_ref,
            &aaadaw_storage::AudioAssetMetadata {
                container: "wav".to_owned(),
                codec: "pcm".to_owned(),
                sample_rate: Some(44_100),
                channel_count: Some(1),
                bits_per_sample: Some(16),
                frame_count: Some(100_000),
                duration_nanos: None,
                byte_len: Some(4),
            },
        )
        .expect("test audio source metadata should store");
    let other_media_ref = "asset://heal-splits-other-test";
    store
        .import_audio_asset(
            other_media_ref,
            "heal-splits-other-test.wav",
            std::io::Cursor::new([0_u8; 4]),
        )
        .expect("second test asset should import");
    store
        .set_audio_asset_metadata(
            other_media_ref,
            &aaadaw_storage::AudioAssetMetadata {
                container: "wav".to_owned(),
                codec: "pcm".to_owned(),
                sample_rate: Some(44_100),
                channel_count: Some(1),
                bits_per_sample: Some(16),
                frame_count: Some(100_000),
                duration_nanos: None,
                byte_len: Some(4),
            },
        )
        .expect("second test source metadata should store");
    store.close().expect("project store should close");

    let mut app = App {
        project_path: Some(project_path.clone()),
        ..App::default()
    };
    assert!(!commands::is_enabled(&app, CommandId::HealSplitsInItems));
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: media_ref.to_owned(),
            start_sample: 0,
            source_offset_samples: 5_000,
            length_samples: 48_000,
        })
        .unwrap();
    let left_id = app.project.audio_items()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: media_ref.to_owned(),
            start_sample: 48_000,
            source_offset_samples: 49_100,
            length_samples: 48_000,
        })
        .unwrap();
    let right_id = app.project.audio_items()[1].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 1_920,
        })
        .unwrap();
    let midi_left = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 1_920,
            length_ticks: 1_920,
        })
        .unwrap();
    let midi_right = app.project.midi_items()[1].id();
    app.timeline.rebuild(&app.project);
    let item_menu = commands::for_menu(&app, MainMenu::Item);
    let menu_entry = item_menu
        .iter()
        .find(|entry| entry.id == CommandId::HealSplitsInItems)
        .expect("Item menu should expose Heal splits in items");
    assert!(!menu_entry.enabled);
    assert_eq!(menu_entry.label, "Heal splits in items");
    assert_eq!(menu_entry.shortcut, None);

    let empty_before = app.project.snapshot();
    let empty_revision = app.revision;
    let _ = app.update(Message::HealSplitsInItems);
    assert_eq!(app.project.snapshot(), empty_before);
    assert_eq!(app.revision, empty_revision);

    app.timeline
        .select_items(vec![(midi_left, track_id), (midi_right, track_id)]);
    assert!(commands::is_enabled(&app, CommandId::HealSplitsInItems));
    let midi_before = app.project.snapshot();
    let midi_revision = app.revision;
    let _ = app.update(Message::HealSplitsInItems);
    assert_eq!(app.project.snapshot(), midi_before);
    assert_eq!(app.revision, midi_revision);

    app.timeline.select_items(vec![(right_id, track_id)]);
    assert!(commands::is_enabled(&app, CommandId::HealSplitsInItems));
    let partial_before = app.project.snapshot();
    let partial_revision = app.revision;
    let _ = app.update(Message::HealSplitsInItems);
    assert_eq!(app.project.snapshot(), partial_before);
    assert_eq!(app.revision, partial_revision);

    app.project
        .apply(DawAction::EditAudioItem {
            item_id: right_id,
            media_ref: other_media_ref.to_owned(),
            start_sample: 48_000,
            source_offset_samples: 49_100,
            length_samples: 48_000,
        })
        .unwrap();
    app.timeline
        .select_items(vec![(left_id, track_id), (right_id, track_id)]);
    let different_sources_before = app.project.snapshot();
    let different_sources_revision = app.revision;
    let _ = app.update(Message::HealSplitsInItems);
    assert_eq!(app.project.snapshot(), different_sources_before);
    assert_eq!(app.revision, different_sources_revision);

    app.project
        .apply(DawAction::EditAudioItem {
            item_id: right_id,
            media_ref: media_ref.to_owned(),
            start_sample: 48_000,
            source_offset_samples: 49_000,
            length_samples: 48_000,
        })
        .unwrap();
    app.timeline
        .select_items(vec![(left_id, track_id), (right_id, track_id)]);
    let discontinuous_before = app.project.snapshot();
    let discontinuous_revision = app.revision;
    let _ = app.update(Message::HealSplitsInItems);
    assert_eq!(app.project.snapshot(), discontinuous_before);
    assert_eq!(app.revision, discontinuous_revision);

    app.project
        .apply(DawAction::EditAudioItem {
            item_id: right_id,
            media_ref: media_ref.to_owned(),
            start_sample: 48_001,
            source_offset_samples: 49_100,
            length_samples: 48_000,
        })
        .unwrap();
    let gap_before = app.project.snapshot();
    let gap_revision = app.revision;
    let _ = app.update(Message::HealSplitsInItems);
    assert_eq!(app.project.snapshot(), gap_before);
    assert_eq!(app.revision, gap_revision);

    app.project
        .apply(DawAction::EditAudioItem {
            item_id: right_id,
            media_ref: media_ref.to_owned(),
            start_sample: 48_000,
            source_offset_samples: 49_100,
            length_samples: 48_000,
        })
        .unwrap();
    app.timeline.select_items(vec![
        (left_id, track_id),
        (right_id, track_id),
        (midi_left, track_id),
        (midi_right, track_id),
    ]);
    let before = app.project.snapshot();
    let revision = app.revision;
    let _ = app.update(Message::HealSplitsInItems);

    assert_eq!(app.project.audio_items().len(), 1);
    let healed = &app.project.audio_items()[0];
    assert_eq!(healed.id(), left_id);
    assert_eq!(healed.start_sample(), 0);
    assert_eq!(healed.source_offset_samples(), 5_000);
    assert_eq!(healed.length_samples(), 96_000);
    assert_eq!(app.project.midi_items().len(), 2);
    assert_eq!(
        app.timeline.selected_items,
        HashSet::from([left_id, midi_left, midi_right])
    );
    assert_eq!(app.revision, revision + 1);
    let after = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after);

    let _ = app.update(Message::Undo);
    app.project
        .apply(DawAction::EditAudioItem {
            item_id: left_id,
            media_ref: media_ref.to_owned(),
            start_sample: 0,
            source_offset_samples: 5_000,
            length_samples: 24_000,
        })
        .unwrap();
    app.project
        .apply(DawAction::EditAudioItem {
            item_id: right_id,
            media_ref: media_ref.to_owned(),
            start_sample: 24_000,
            source_offset_samples: 27_050,
            length_samples: 24_000,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: media_ref.to_owned(),
            start_sample: 48_000,
            source_offset_samples: 49_100,
            length_samples: 48_000,
        })
        .unwrap();
    let third_id = app.project.audio_items()[2].id();
    app.timeline.select_items(vec![
        (left_id, track_id),
        (right_id, track_id),
        (third_id, track_id),
    ]);
    let before_chain = app.project.snapshot();
    let revision = app.revision;
    let _ = app.update(Message::HealSplitsInItems);
    assert_eq!(app.project.audio_items().len(), 1);
    assert_eq!(app.project.audio_items()[0].id(), left_id);
    assert_eq!(app.project.audio_items()[0].length_samples(), 96_000);
    assert_eq!(app.timeline.selected_items, HashSet::from([left_id]));
    assert_eq!(app.revision, revision + 1);
    let after_chain = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before_chain);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after_chain);

    drop(app);
    let _ = std::fs::remove_file(&project_path);
    for suffix in ["-wal", "-shm"] {
        let sidecar = format!("{}{suffix}", project_path.display());
        let _ = std::fs::remove_file(sidecar);
    }
}

fn test_shortcut_input(logical_key: Key, modifiers: Modifiers) -> shortcut::ShortcutInput {
    shortcut::ShortcutInput {
        logical_key,
        modifiers,
        physical_key: iced::keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        location: iced::keyboard::Location::Standard,
    }
}

fn test_input_event(input: shortcut::ShortcutInput) -> iced::Event {
    iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: input.logical_key.clone(),
        modified_key: input.logical_key,
        physical_key: input.physical_key,
        location: input.location,
        modifiers: input.modifiers,
        text: None,
        repeat: false,
    })
}

#[test]
fn main_keyboard_preserves_keypad_identity_through_project_actions() {
    use iced::keyboard::{
        Location,
        key::{Code, Named, Physical},
    };
    let window = iced::window::Id::unique();
    let mut app = App {
        main_window_id: Some(window),
        ..App::default()
    };
    *app.shortcut_bindings.write().unwrap() = commands::validate_bindings(&HashMap::from([
        ("track.add".into(), "Ctrl+Standard7".into()),
        ("edit.undo".into(), "Ctrl+NumPad7".into()),
        ("markers.add-or-move.07".into(), String::new()),
    ]))
    .unwrap();
    let mut input = test_shortcut_input(Key::Character("7".into()), Modifiers::CTRL);
    input.physical_key = Physical::Code(Code::Digit7);
    let _ = app.update(Message::RuntimeKeyboardEvent(
        test_input_event(input.clone()),
        iced::event::Status::Ignored,
        window,
    ));
    assert_eq!(app.project.tracks().len(), 1);
    input.logical_key = Key::Named(Named::Home); // NumLock off
    input.physical_key = Physical::Code(Code::Numpad7);
    input.location = Location::Numpad;
    let _ = app.update(Message::RuntimeKeyboardEvent(
        test_input_event(input),
        iced::event::Status::Ignored,
        window,
    ));
    assert!(app.project.tracks().is_empty());
}

#[test]
fn f10_custom_bindings_and_main_window_capture_precede_menu_navigation() {
    use iced::keyboard::key::Named;
    let window = iced::window::Id::unique();
    for chord in ["Ctrl+F10", "Alt+F10", "F10"] {
        let mut app = App {
            main_window_id: Some(window),
            ..App::default()
        };
        *app.shortcut_bindings.write().unwrap() =
            commands::validate_bindings(&HashMap::from([("track.add".into(), chord.into())]))
                .unwrap();
        let modifiers = if chord.starts_with("Ctrl") {
            Modifiers::CTRL
        } else if chord.starts_with("Alt") {
            Modifiers::ALT
        } else {
            Modifiers::NONE
        };
        let input = test_shortcut_input(Key::Named(Named::F10), modifiers);
        let _ = app.update(Message::RuntimeKeyboardEvent(
            test_input_event(input),
            iced::event::Status::Ignored,
            window,
        ));
        assert_eq!(app.project.tracks().len(), 1, "{chord}");
        assert!(app.active_menu.is_none());
    }
    let mut app = App {
        main_window_id: Some(window),
        shell_profile: super::ShellProfile::Touch,
        ..App::default()
    };
    let _ = app.update(Message::AddShortcutBinding("edit.undo".into()));
    let input = test_shortcut_input(Key::Named(Named::F10), Modifiers::NONE);
    let _ = app.update(Message::RuntimeKeyboardEvent(
        test_input_event(input),
        iced::event::Status::Ignored,
        window,
    ));
    assert_eq!(app.shortcut_binding_edits["edit.undo"], "Mod+Z; F10");
    assert!(app.active_menu.is_none());
}

#[test]
fn narrow_linux_window_keeps_desktop_profile() {
    let app = App {
        main_window_size: Some(iced::Size::new(420.0, 640.0)),
        shell_profile: super::ShellProfile::Desktop,
        ..App::default()
    };
    assert!(!app.is_mobile_main_window());
}

#[test]
fn busy_project_keeps_action_list_input_available_without_allowing_project_edits() {
    let window = iced::window::Id::unique();
    let mut app = App {
        action_list_window_id: Some(window),
        import_busy: true,
        ..App::default()
    };
    app.action_list.select("track.add".into());
    let _ = app.update(Message::ActionListRun(false));
    assert!(app.project.tracks().is_empty());
    app.action_list.capture = Some(super::action_list::CaptureMode::Find);
    let input = test_shortcut_input(
        Key::Named(iced::keyboard::key::Named::Escape),
        Modifiers::NONE,
    );
    let _ = app.update(Message::RuntimeKeyboardEvent(
        test_input_event(input),
        iced::event::Status::Ignored,
        window,
    ));
    assert!(app.action_list.capture.is_none());
    assert_eq!(app.action_list_window_id, Some(window));
}

#[test]
fn unidentified_logical_key_keeps_known_numpad_five_dispatch() {
    use iced::keyboard::{
        Location,
        key::{Code, Physical},
    };
    let window = iced::window::Id::unique();
    let mut app = App {
        main_window_id: Some(window),
        ..App::default()
    };
    *app.shortcut_bindings.write().unwrap() = commands::validate_bindings(&HashMap::from([(
        "track.add".into(),
        "Ctrl+NumPad5".into(),
    )]))
    .unwrap();
    let mut input = test_shortcut_input(Key::Unidentified, Modifiers::CTRL);
    input.physical_key = Physical::Code(Code::Numpad5);
    input.location = Location::Numpad;
    let _ = app.update(Message::RuntimeKeyboardEvent(
        test_input_event(input),
        iced::event::Status::Ignored,
        window,
    ));
    assert_eq!(app.project.tracks().len(), 1);
}

#[test]
fn action_list_and_custom_f10_respect_pending_save_confirmation() {
    let main = iced::window::Id::unique();
    let list = iced::window::Id::unique();
    let mut app = App {
        main_window_id: Some(main),
        action_list_window_id: Some(list),
        ..App::default()
    };
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::NewProject);
    assert!(app.pending_project_transition.is_some());
    app.action_list.select("track.add".into());
    let _ = app.update(Message::ActionListRun(false));
    assert_eq!(app.project.tracks().len(), 1);
    *app.shortcut_bindings.write().unwrap() =
        commands::validate_bindings(&HashMap::from([("track.add".into(), "F10".into())])).unwrap();
    let event = test_input_event(test_shortcut_input(
        Key::Named(iced::keyboard::key::Named::F10),
        Modifiers::NONE,
    ));
    let _ = app.update(Message::RuntimeKeyboardEvent(
        event,
        iced::event::Status::Ignored,
        main,
    ));
    assert_eq!(app.project.tracks().len(), 1);
    assert!(app.pending_project_transition.is_some());
}

#[test]
fn action_list_run_close_closes_even_when_undo_has_no_effect() {
    let window = iced::window::Id::unique();
    let mut app = App {
        action_list_window_id: Some(window),
        ..App::default()
    };
    app.action_list.select("edit.undo".into());
    let _ = app.update(Message::ActionListRun(true));
    assert!(app.action_list_window_id.is_none());
    assert!(app.project.tracks().is_empty());
}

#[test]
fn desktop_mixer_dock_and_media_share_state_and_keep_independent_ratios() {
    let mut app = App::default();
    assert!(app.media_panel_dock.mixer_open);
    let _ = app.update(Message::AddTrack);
    let id = app.project.tracks()[0].id();
    let _ = app.update(Message::ToggleMediaBrowserPanel);
    let media = app.media_panel_dock.split.unwrap();
    let mixer = app.media_panel_dock.mixer_split.unwrap();
    assert_ne!(media, mixer);
    let _ = app.update(Message::MediaPanelResized(media, 0.63));
    let _ = app.update(Message::MediaPanelResized(mixer, 0.48));
    let _ = app.update(Message::ToggleMixerPanel);
    assert!(!app.media_panel_dock.mixer_open);
    assert_eq!(app.media_panel_dock.panes.as_ref().unwrap().len(), 2);
    let _ = app.update(Message::ToggleMixerPanel);
    assert_eq!(app.media_panel_dock.panes.as_ref().unwrap().len(), 3);
    assert_eq!(app.media_panel_dock.main_ratio, 0.63);
    assert_eq!(app.media_panel_dock.arrange_ratio, 0.48);
    assert_eq!(app.project.tracks()[0].id(), id);
    assert!(app.desktop_layout_save_at.is_some());
}

#[test]
fn new_factory_shortcuts_preserve_existing_custom_maps_and_effective_labels() {
    let bindings =
        commands::validate_bindings(&HashMap::from([("edit.undo".into(), "Mod+M".into())]))
            .unwrap();
    let input = test_shortcut_input(Key::Character("m".into()), Modifiers::COMMAND);
    assert_eq!(
        commands::from_shortcut_input(&input, &bindings, &[]),
        Some(CommandId::Undo)
    );
    assert!(commands::binding_for_id("view.toggle-mixer", &bindings).is_empty());
    assert_eq!(commands::binding_for_id("edit.undo", &bindings), "Mod+M");
}

#[test]
fn busy_project_view_menu_and_shortcut_toggle_mixer_without_editing_project() {
    let main = iced::window::Id::unique();
    let mut app = App {
        main_window_id: Some(main),
        io_busy: true,
        ..App::default()
    };
    let _ = app.update(Message::ExecuteCommand(CommandId::ToggleMixerPanel));
    assert!(!app.media_panel_dock.mixer_open);
    let input = test_shortcut_input(Key::Character("m".into()), Modifiers::COMMAND);
    let _ = app.update(Message::RuntimeKeyboardEvent(
        test_input_event(input),
        iced::event::Status::Ignored,
        main,
    ));
    assert!(app.media_panel_dock.mixer_open);
    assert!(app.project.tracks().is_empty());
}

#[test]
fn yielded_factory_binding_is_consistent_between_labels_and_dispatch() {
    use iced::keyboard::{
        Location,
        key::{Code, Physical},
    };
    let bindings = commands::validate_bindings(&HashMap::from([(
        "edit.undo".into(),
        "Shift+NumPadDivide".into(),
    )]))
    .unwrap();
    assert!(commands::binding_for_id("actions.show-list", &bindings).is_empty());
    let mut input = test_shortcut_input(Key::Character("/".into()), Modifiers::SHIFT);
    assert_eq!(commands::from_shortcut_input(&input, &bindings, &[]), None);
    input.physical_key = Physical::Code(Code::NumpadDivide);
    input.location = Location::Numpad;
    assert_eq!(
        commands::from_shortcut_input(&input, &bindings, &[]),
        Some(CommandId::Undo)
    );
}

#[test]
fn ordinary_track_routing_is_available_as_an_undoable_action() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let source = app.project.tracks()[0].id();
    let _ = app.update(Message::AddTrack);
    let bus = app.project.tracks()[1].id();
    assert!(!app.project.tracks()[1].is_bus());

    let _ = app.update(Message::SetTrackOutput(source, Some(bus)));
    assert_eq!(app.project.tracks()[0].output_track(), Some(bus));
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks()[0].output_track(), None);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.tracks()[0].output_track(), Some(bus));
}

#[test]
fn routing_send_drafts_commit_atomically_and_invalid_values_preserve_project() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let source = app.project.tracks()[0].id();
    let destination = app.project.tracks()[1].id();
    app.routing_track_id = Some(source);
    let _ = app.update(Message::RoutingChange(DawAction::CreateAudioSend {
        track_id: source,
        destination,
        parameters: Default::default(),
    }));
    let id = app.project.tracks()[0].sends()[0].id();
    let before = app.project.snapshot();
    let _ = app.update(Message::RoutingSendDraft(id, false, "-6".into()));
    let _ = app.update(Message::RoutingSendDraft(id, true, "NaN".into()));
    let _ = app.update(Message::CommitRoutingSend(id));
    assert_eq!(app.project.snapshot(), before);
    assert!(app.routing_send_drafts.contains_key(&id));
    let _ = app.update(Message::RoutingSendDraft(id, true, "0.5".into()));
    let _ = app.update(Message::CommitRoutingSend(id));
    let parameters = app.project.tracks()[0].sends()[0].parameters();
    assert_eq!((parameters.volume_db, parameters.pan), (-6.0, 0.5));
    assert!(!app.routing_send_drafts.contains_key(&id));
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    let _ = app.update(Message::RoutingChange(DawAction::DeleteTrack {
        track_id: destination,
    }));
    assert_eq!(app.project.snapshot(), before);
}

#[test]
fn touch_track_routing_uses_panel_navigation_and_close_keeps_project() {
    let mut app = App {
        shell_profile: super::ShellProfile::Touch,
        ..Default::default()
    };
    let _ = app.update(Message::AddTrack);
    let source = app.project.tracks()[0].id();
    let before = app.project.snapshot();
    let _ = app.update(Message::OpenTrackRouting(source));
    assert_eq!(app.mobile_panel, MobilePanel::Routing);
    assert!(app.routing_window_id.is_none());
    let _ = app.update(Message::CloseTrackRouting);
    assert_eq!(app.mobile_panel, MobilePanel::Editor);
    assert_eq!(app.project.snapshot(), before);
}

#[test]
fn folder_registry_and_routing_parent_edits_share_undoable_project_state() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddTrack);
    let parent = app.project.tracks()[0].id();
    let child = app.project.tracks()[1].id();
    let _ = app.update(Message::ExecuteCommand(commands::CommandId::Track {
        track_id: parent,
        command: commands::TrackCommand::ToggleFolder,
    }));
    assert!(app.project.tracks()[0].is_folder());
    let _ = app.update(Message::RoutingChange(DawAction::SetTrackParent {
        track_id: child,
        parent: Some(parent),
    }));
    assert_eq!(app.project.folder_depth(child), 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks()[1].parent_track(), None);
    let _ = app.update(Message::Undo);
    assert!(!app.project.tracks()[0].is_folder());
}

#[test]
fn folder_compact_registered_command_changes_only_persisted_view_state() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let folder = app.project.tracks()[0].id();
    let _ = app.update(Message::RoutingChange(DawAction::SetTrackFolder {
        track_id: folder,
        enabled: true,
    }));
    let before = app.project.snapshot();
    let view_before = app.timeline.arrangement_view_state(&app.project);
    let revision = app.revision;
    let _ = app.update(Message::ExecuteCommand(commands::CommandId::Track {
        track_id: folder,
        command: commands::TrackCommand::CycleFolderCompact,
    }));
    assert_eq!(app.project.snapshot(), before);
    assert_ne!(
        app.timeline.arrangement_view_state(&app.project),
        view_before
    );
    assert!(app.revision > revision);
}

#[cfg(feature = "audio-device")]
#[test]
fn transport_factory_bindings_separate_stop_and_pause_and_preserve_custom_maps() {
    use iced::keyboard::key::Named;
    for (key, modifiers, expected) in [
        (Named::Space, Modifiers::NONE, CommandId::TogglePlayStop),
        (Named::Enter, Modifiers::NONE, CommandId::TogglePlayback),
        (Named::Space, Modifiers::CTRL, CommandId::TogglePlayback),
    ] {
        assert_eq!(
            commands::from_shortcut(&Key::Named(key), modifiers, &HashMap::new(), &[]),
            Some(expected)
        );
    }
    assert_eq!(
        commands::from_shortcut(
            &Key::Named(Named::Space),
            Modifiers::SHIFT,
            &HashMap::new(),
            &[]
        ),
        None
    );
    let custom = commands::validate_bindings(&HashMap::from([
        ("transport.toggle-playback".into(), "Space".into()),
        ("transport.stop-playback".into(), "Shift+Space".into()),
    ]))
    .unwrap();
    assert_eq!(
        commands::from_shortcut(&Key::Named(Named::Space), Modifiers::NONE, &custom, &[]),
        Some(CommandId::TogglePlayback)
    );
    assert_eq!(
        commands::from_shortcut(&Key::Named(Named::Space), Modifiers::SHIFT, &custom, &[]),
        Some(CommandId::StopPlayback)
    );
    assert_eq!(commands::binding_for_id("transport.play-stop", &custom), "");
}

#[cfg(feature = "audio-device")]
#[test]
fn transport_stop_commands_cancel_recording_setup_through_busy_guards() {
    for command in [CommandId::TogglePlayStop, CommandId::StopPlayback] {
        let mut app = App {
            recording_starting: true,
            playback_busy: true,
            ..App::default()
        };
        let _ = app.update(Message::ExecuteCommand(command));
        assert!(app.recording_cancel_requested, "{command:?}");
        assert!(app.status.contains("Cancelling input setup"));
        assert!(app.project.tracks().is_empty());
    }
}

#[test]
fn track_phase_command_and_undo_share_domain_state() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track = app.project.tracks()[0].id();
    app.timeline.select_track_only(track);
    let command = CommandId::SelectedTrack(commands::TrackCommand::TogglePhase);
    assert_eq!(
        commands::stable_id(command).as_deref(),
        Some("track.toggle-phase")
    );
    assert_eq!(commands::toggle_state(&app, command), Some(false));
    let _ = app.update(Message::ExecuteCommand(command));
    assert!(app.project.tracks()[0].is_phase_inverted());
    assert_eq!(commands::toggle_state(&app, command), Some(true));
    let _ = app.update(Message::Undo);
    assert!(!app.project.tracks()[0].is_phase_inverted());
    let _ = app.update(Message::Redo);
    assert!(app.project.tracks()[0].is_phase_inverted());
}

#[test]
fn item_fade_preview_commits_once_and_cancel_preserves_project_revision() {
    use crate::timeline::TimelineEvent;
    use aaadaw_core::{AudioFade, AudioItemFades, FadeShape};
    let mut app = App::default();
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".into(),
        })
        .unwrap();
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://constant".into(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .unwrap();
    let item_id = app.project.audio_items()[0].id();
    let original = app.project.snapshot();
    let revision = app.revision;
    let first = AudioItemFades {
        fade_in: AudioFade::new(12_000.5, FadeShape::Smooth).unwrap(),
        ..AudioItemFades::default()
    };
    let last = AudioItemFades {
        fade_in: AudioFade::new(24_000.25, FadeShape::Smooth).unwrap(),
        ..first
    };
    let _ = app.update(Message::Timeline(TimelineEvent::PreviewItemFades {
        item_id,
        fades: first,
    }));
    let _ = app.update(Message::Timeline(TimelineEvent::PreviewItemFades {
        item_id,
        fades: last,
    }));
    assert_eq!(app.project.snapshot(), original);
    assert_eq!(app.revision, revision);
    let _ = app.update(Message::Timeline(TimelineEvent::CommitItemFades));
    assert_eq!(app.project.audio_items()[0].fades(), last);
    assert_eq!(app.revision, revision + 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items()[0].fades(), last);
    let revision = app.revision;
    let _ = app.update(Message::Timeline(TimelineEvent::PreviewItemFades {
        item_id,
        fades: first,
    }));
    let _ = app.update(Message::Escape);
    assert!(app.timeline.item_fade_preview().is_none());
    assert_eq!(app.project.audio_items()[0].fades(), last);
    assert_eq!(app.revision, revision);
    let _ = app.update(Message::Timeline(TimelineEvent::CommitItemFades));
    assert_eq!(app.revision, revision);
}

#[test]
fn curve_menu_converts_legacy_smooth_without_changing_lengths_and_is_undoable() {
    use crate::timeline::{ItemFadeEdge, TimelineEvent};
    use aaadaw_core::{AudioFade, AudioItemFades, FadeCurve, FadeShape};
    let mut app = App::default();
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".into(),
        })
        .unwrap();
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://constant".into(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .unwrap();
    let item_id = app.project.audio_items()[0].id();
    let original = AudioItemFades {
        fade_in: AudioFade::new(12_000.25, FadeShape::Smooth).unwrap(),
        fade_out: AudioFade::new(6_000.5, FadeShape::FastStart).unwrap(),
    };
    app.project
        .apply(DawAction::SetAudioItemFades {
            item_id,
            fades: original,
        })
        .unwrap();
    let message = Message::Timeline(TimelineEvent::SetFadeCurvePreset {
        item_id,
        edge: ItemFadeEdge::In,
        shape: FadeShape::Smooth,
    });
    let _ = app.update(message.clone());
    let current = app.project.audio_items()[0].fades();
    assert_eq!(current.fade_in.length_samples(), 12_000.25);
    assert_eq!(
        current.fade_in.curve(),
        FadeCurve::current_preset(FadeShape::Smooth)
    );
    assert_eq!(current.fade_out, original.fade_out);
    let revision = app.revision;
    let _ = app.update(message);
    assert_eq!(app.revision, revision);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].fades(), original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items()[0].fades(), current);
    let _ = app.update(Message::Timeline(TimelineEvent::OpenFadeMenu {
        item_id,
        edge: ItemFadeEdge::Out,
        x: 1.0,
        y: 1.0,
    }));
    let _ = app.update(Message::Escape);
    assert!(app.timeline.context_fade.is_none());
    assert_eq!(app.project.audio_items()[0].fades(), current);
}

#[test]
fn item_properties_stage_fades_preserve_precision_and_apply_one_history_entry() {
    use super::item_properties::{ItemProperties, ItemPropertyField};
    use aaadaw_core::{AudioFade, AudioItemFades, FadeCurve, FadeShape};
    let mut app = App::default();
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".into(),
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: app.project.tracks()[0].id(),
            media_ref: "asset://owned".into(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48000,
        })
        .unwrap();
    let id = app.project.audio_items()[0].id();
    let original = AudioItemFades {
        fade_in: AudioFade::new(12000.25, FadeShape::Smooth).unwrap(),
        fade_out: AudioFade::new(6000.5, FadeShape::FastStart).unwrap(),
    };
    app.project
        .apply(DawAction::SetAudioItemFades {
            item_id: id,
            fades: original,
        })
        .unwrap();
    app.item_properties = Some(ItemProperties::new(id, original, 1.0, 0.0, 48000));
    let revision = app.revision;
    let _ = app.update(Message::ApplyItemProperties(false));
    assert_eq!(app.project.audio_items()[0].fades(), original);
    assert_eq!(app.revision, revision);
    let _ = app.update(Message::ItemPropertyFieldChanged(
        ItemPropertyField::InCurvature,
        "-2".into(),
    ));
    let _ = app.update(Message::ItemPropertyFieldChanged(
        ItemPropertyField::OutLength,
        "0:00.1255".into(),
    ));
    let _ = app.update(Message::ItemPropertyFieldChanged(
        ItemPropertyField::TakeVolume,
        "-6.02dB".into(),
    ));
    let _ = app.update(Message::ItemPropertyFieldChanged(
        ItemPropertyField::TakePan,
        "25%L".into(),
    ));
    assert_eq!(app.project.audio_items()[0].fades(), original);
    let _ = app.update(Message::ApplyItemProperties(false));
    let applied = app.project.audio_items()[0].fades();
    assert_eq!(applied.fade_in.length_samples(), 12000.25);
    assert!(
        matches!(applied.fade_in.curve(), FadeCurve::Native(p) if p.curvature() == -1.0 && p.s_parameter() == 0.5)
    );
    assert_eq!(applied.fade_out.length_samples(), 6024.0);
    assert_eq!(applied.fade_out.curve(), original.fade_out.curve());
    assert_eq!(app.project.audio_items()[0].volume(), 1.0);
    assert!((app.project.audio_items()[0].take_volume() - 0.5).abs() < 0.001);
    assert_eq!(app.project.audio_items()[0].take_pan(), -0.25);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].fades(), original);
    assert_eq!(app.project.audio_items()[0].take_volume(), 1.0);
    assert_eq!(app.project.audio_items()[0].take_pan(), 0.0);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items()[0].fades(), applied);
    assert!((app.project.audio_items()[0].take_volume() - 0.5).abs() < 0.001);
    assert_eq!(app.project.audio_items()[0].take_pan(), -0.25);
    let _ = app.update(Message::ItemPropertyFieldChanged(
        ItemPropertyField::InLength,
        "NaN".into(),
    ));
    let _ = app.update(Message::ApplyItemProperties(true));
    assert!(app.item_properties.as_ref().unwrap().error.is_some());
    assert_eq!(app.project.audio_items()[0].fades(), applied);
    let _ = app.update(Message::CloseItemProperties);
    assert!(app.item_properties.is_none());
    assert_eq!(app.project.audio_items()[0].fades(), applied);
    let both = AudioItemFades {
        fade_in: AudioFade::new(19200.0, FadeShape::FastStart).unwrap(),
        fade_out: AudioFade::new(28800.0, FadeShape::FastStart).unwrap(),
    };
    let mut draft = ItemProperties::new(id, both, 1.0, 0.0, 48000);
    draft.edit(ItemPropertyField::InLength, "0:00.800".into());
    let adjusted = draft.fades(both, 48000, 48000).unwrap();
    assert_eq!(adjusted.fade_in.length_samples(), 38400.0);
    assert_eq!(adjusted.fade_out.length_samples(), 9600.0);
    draft.edit(ItemPropertyField::OutLength, "0:02.000".into());
    let adjusted = draft.fades(both, 48000, 48000).unwrap();
    assert_eq!(adjusted.fade_in.length_samples(), 0.0);
    assert_eq!(adjusted.fade_out.length_samples(), 48000.0);
    draft.edit(ItemPropertyField::InLength, "0:00.600".into());
    draft.edit(ItemPropertyField::OutLength, "0:00.600".into());
    let adjusted = draft.fades(both, 48000, 48000).unwrap();
    assert_eq!(adjusted.fade_in.length_samples(), 19200.0);
    assert_eq!(adjusted.fade_out.length_samples(), 28800.0);
}

#[test]
fn item_properties_toggle_loop_source_preserves_section_and_undoes() {
    use super::item_properties::{ItemProperties, TimeUnit};

    let mut app = App::default();
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".into(),
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertLoopedAudioItem {
            track_id: app.project.tracks()[0].id(),
            media_ref: "asset://item-properties-loop".into(),
            start_sample: 0,
            source_offset_samples: 120,
            length_samples: 4_800,
            muted: false,
            phase_inverted: false,
            source_section_start_samples: 100,
            source_section_length_samples: 240,
        })
        .unwrap();
    let item = app.project.audio_items()[0].clone();
    let mut draft = ItemProperties::from_item(&item, 48_000, TimeUnit::Samples);
    draft.set_loop_source(false);
    app.item_properties = Some(draft);

    let _ = app.update(Message::ApplyItemProperties(false));
    let item = &app.project.audio_items()[0];
    assert!(!item.is_loop_source());
    assert_eq!(item.source_section_start_samples(), Some(100));
    assert_eq!(item.source_section_length_samples(), Some(240));
    let _ = app.update(Message::Undo);
    assert!(app.project.audio_items()[0].is_loop_source());
    let _ = app.update(Message::Redo);
    assert!(!app.project.audio_items()[0].is_loop_source());
}

#[test]
fn item_properties_mute_and_phase_apply_atomically_with_undo() {
    use super::item_properties::{ItemProperties, TimeUnit};

    let mut app = App::default();
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".into(),
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: app.project.tracks()[0].id(),
            media_ref: "asset://item-properties-mute".into(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 4_800,
        })
        .unwrap();
    let item = app.project.audio_items()[0].clone();
    let mut draft = ItemProperties::from_item(&item, 48_000, TimeUnit::Samples);
    draft.set_muted(true);
    app.item_properties = Some(draft);
    let _ = app.update(Message::ItemPropertyPhaseChanged(true));

    let _ = app.update(Message::ApplyItemProperties(false));
    assert!(app.project.audio_items()[0].is_muted());
    assert!(app.project.audio_items()[0].is_phase_inverted());
    assert_eq!(app.project.audio_items()[0].volume(), 1.0);
    let _ = app.update(Message::Undo);
    assert!(!app.project.audio_items()[0].is_muted());
    assert!(!app.project.audio_items()[0].is_phase_inverted());
    let _ = app.update(Message::Redo);
    assert!(app.project.audio_items()[0].is_muted());
    assert!(app.project.audio_items()[0].is_phase_inverted());
    let _ = app.update(Message::ItemPropertyPhaseChanged(false));
    let _ = app.update(Message::CloseItemProperties);
    assert!(app.project.audio_items()[0].is_phase_inverted());
}

#[test]
fn midi_item_properties_opens_applies_mute_and_keeps_notes_undoable() {
    use super::item_properties::{ItemPropertyField, TimeUnit};

    let mut app = App {
        item_properties_unit: TimeUnit::Time,
        ..App::default()
    };
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "MIDI".into(),
        })
        .unwrap();
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = app.project.midi_items()[0].id();
    app.project
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
    app.timeline.rebuild(&app.project);
    app.timeline.selected_item = Some(item_id);
    app.timeline.selected_items.insert(item_id);

    assert!(commands::is_enabled(&app, CommandId::OpenItemProperties));
    assert!(commands::is_enabled(&app, CommandId::ToggleItemProperties));
    let _ = app.update(Message::ToggleItemProperties);
    assert!(app.item_properties.is_none());
    assert!(app.midi_item_properties.is_some());
    assert!(app.item_properties_window_id.is_some());

    let expected_start = app
        .project
        .tick_at_sample_position(app.project.sample_at_project_time(1.0).unwrap() as f64)
        .unwrap()
        .round() as u64;
    let expected_end = app
        .project
        .tick_at_sample_position(app.project.sample_at_project_time(1.75).unwrap() as f64)
        .unwrap()
        .round() as u64;
    let expected_source_offset = app
        .project
        .tick_at_sample_position(app.project.sample_at_project_time(0.25).unwrap() as f64)
        .unwrap()
        .round() as u64;
    let revision = app.revision;
    let _ = app.update(Message::ItemPropertyFieldChanged(
        ItemPropertyField::Position,
        "0:01.000".into(),
    ));
    let _ = app.update(Message::ItemPropertyFieldChanged(
        ItemPropertyField::Length,
        "0:00.750".into(),
    ));
    let _ = app.update(Message::ItemPropertyFieldChanged(
        ItemPropertyField::SourceOffset,
        "0:00.250".into(),
    ));
    let _ = app.update(Message::ItemPropertyMuteChanged(true));
    let _ = app.update(Message::MidiItemPropertyNameChanged("   ".into()));
    let _ = app.update(Message::ApplyItemProperties(false));
    assert_eq!(app.revision, revision);
    assert_eq!(app.project.midi_items()[0].name(), "MIDI");
    assert!(!app.project.midi_items()[0].is_muted());
    assert_eq!(app.project.midi_items()[0].start_tick(), 0);
    assert_eq!(app.project.midi_items()[0].length_ticks(), 960);
    assert!(app.midi_item_properties.as_ref().unwrap().error.is_some());

    let _ = app.update(Message::MidiItemPropertyNameChanged("Intro".into()));
    let _ = app.update(Message::ApplyItemProperties(false));
    assert!(app.project.midi_items()[0].is_muted());
    assert_eq!(app.project.midi_items()[0].name(), "Intro");
    assert_eq!(app.project.midi_items()[0].start_tick(), expected_start);
    assert_eq!(
        app.project.midi_items()[0].length_ticks(),
        expected_end - expected_start
    );
    assert_eq!(
        app.project.midi_items()[0].source_offset_ticks(),
        expected_source_offset
    );
    assert_eq!(app.project.midi_items()[0].notes().len(), 1);
    assert_eq!(app.revision, revision + 1);

    let _ = app.update(Message::Undo);
    assert!(!app.project.midi_items()[0].is_muted());
    assert_eq!(app.project.midi_items()[0].start_tick(), 0);
    assert_eq!(app.project.midi_items()[0].length_ticks(), 960);
    assert_eq!(app.project.midi_items()[0].source_offset_ticks(), 0);
    assert_eq!(app.project.midi_items()[0].name(), "MIDI");
    assert_eq!(app.project.midi_items()[0].notes().len(), 1);
    let _ = app.update(Message::Redo);
    assert!(app.project.midi_items()[0].is_muted());
    assert_eq!(app.project.midi_items()[0].start_tick(), expected_start);
    assert_eq!(
        app.project.midi_items()[0].length_ticks(),
        expected_end - expected_start
    );
    assert_eq!(
        app.project.midi_items()[0].source_offset_ticks(),
        expected_source_offset
    );
    assert_eq!(app.project.midi_items()[0].name(), "Intro");

    let _ = app.update(Message::MidiItemPropertyNameChanged("Unapplied".into()));
    let _ = app.update(Message::ItemPropertyMuteChanged(false));
    let _ = app.update(Message::CloseItemProperties);
    assert!(app.midi_item_properties.is_none());
    assert!(app.project.midi_items()[0].is_muted());
    assert_eq!(app.project.midi_items()[0].name(), "Intro");
}

#[test]
fn midi_item_properties_toggle_loop_source_preserves_source_length_and_undoes() {
    let mut app = App::default();
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "MIDI loop source".into(),
        })
        .unwrap();
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::EditMidiItemPlacement {
            item_id,
            start_tick: 0,
            length_ticks: 1_920,
            source_offset_ticks: 0,
        })
        .unwrap();
    app.timeline.rebuild(&app.project);
    app.timeline.selected_item = Some(item_id);
    app.timeline.selected_items.insert(item_id);

    let _ = app.update(Message::ToggleItemProperties);
    assert!(app.midi_item_properties.is_some());
    assert!(app.project.midi_items()[0].is_loop_source());
    assert_eq!(app.project.midi_items()[0].source_loop_length_ticks(), 960);

    let _ = app.update(Message::ItemPropertyLoopSourceChanged(false));
    let _ = app.update(Message::ApplyItemProperties(false));

    let item = &app.project.midi_items()[0];
    assert!(!item.is_loop_source());
    assert_eq!(item.source_loop_length_ticks(), 960);
    assert_eq!(item.length_ticks(), 1_920);
    let _ = app.update(Message::Undo);
    assert!(app.project.midi_items()[0].is_loop_source());
    assert_eq!(app.project.midi_items()[0].source_loop_length_ticks(), 960);
    let _ = app.update(Message::Redo);
    assert!(!app.project.midi_items()[0].is_loop_source());
    assert_eq!(app.project.midi_items()[0].source_loop_length_ticks(), 960);
}

#[test]
fn midi_item_properties_edit_loop_source_length_preserves_events_and_undoes() {
    let mut app = App::default();
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "MIDI loop source".into(),
        })
        .unwrap();
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let item_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![MidiNoteData {
                pitch: 60,
                tick: 120,
                duration: 240,
                velocity: 96,
            }],
        })
        .unwrap();
    let original = app.project.midi_items()[0].clone();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(item_id, track_id)]);

    let _ = app.update(Message::ToggleItemProperties);
    assert_eq!(
        app.midi_item_properties
            .as_ref()
            .unwrap()
            .source_loop_length,
        "960"
    );
    let _ = app.update(Message::MidiItemPropertySourceLoopLengthChanged(
        "1920".into(),
    ));
    let _ = app.update(Message::ApplyItemProperties(false));

    let extended = app.project.midi_items()[0].clone();
    assert_eq!(extended.source_loop_length_ticks(), 1_920);
    assert_eq!(extended.length_ticks(), original.length_ticks());
    assert_eq!(extended.notes(), original.notes());
    assert_eq!(app.timeline.selected_items, [item_id].into());
    assert!(app.midi_item_properties.as_ref().unwrap().error.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items()[0], original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.midi_items()[0], extended);

    let _ = app.update(Message::MidiItemPropertySourceLoopLengthChanged(
        "200".into(),
    ));
    let _ = app.update(Message::ApplyItemProperties(false));
    assert_eq!(
        app.project.midi_items()[0].source_loop_length_ticks(),
        1_920
    );
    assert_eq!(
        app.midi_item_properties.as_ref().unwrap().error.as_deref(),
        Some("Source length cannot end before stored MIDI events")
    );
}

#[test]
fn item_toggle_mute_action_matches_action_list_and_is_undoable() {
    let mut app = App::default();
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".into(),
        })
        .unwrap();
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://item-action-mute".into(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 4_800,
        })
        .unwrap();
    let item_id = app.project.audio_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(item_id, track_id)]);
    assert!(commands::is_enabled(
        &app,
        CommandId::ToggleMuteSelectedItems
    ));

    let revision = app.revision;
    let _ = app.update(Message::ExecuteCommand(CommandId::ToggleMuteSelectedItems));
    assert!(app.project.audio_items()[0].is_muted());
    assert_eq!(app.revision, revision + 1);
    let _ = app.update(Message::Undo);
    assert!(!app.project.audio_items()[0].is_muted());
    let _ = app.update(Message::Redo);
    assert!(app.project.audio_items()[0].is_muted());
}

#[test]
fn item_toggle_mute_action_handles_mixed_audio_and_midi_selection_in_one_step() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://mixed-item-mute".into(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 4_800,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let audio_id = app.project.audio_items()[0].id();
    let midi_id = app.project.midi_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline
        .select_items(vec![(audio_id, track_id), (midi_id, track_id)]);

    assert!(commands::is_enabled(
        &app,
        CommandId::ToggleMuteSelectedItems
    ));
    let revision = app.revision;
    let _ = app.update(Message::ExecuteCommand(CommandId::ToggleMuteSelectedItems));
    assert!(app.project.audio_items()[0].is_muted());
    assert!(app.project.midi_items()[0].is_muted());
    assert_eq!(app.revision, revision + 1);
    let _ = app.update(Message::Undo);
    assert!(!app.project.audio_items()[0].is_muted());
    assert!(!app.project.midi_items()[0].is_muted());
    let _ = app.update(Message::Redo);
    assert!(app.project.audio_items()[0].is_muted());
    assert!(app.project.midi_items()[0].is_muted());
}

#[test]
fn item_properties_batch_mute_edits_mixed_selection_in_one_undo_step() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://batch-item-properties".into(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 4_800,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://batch-item-properties".into(),
            start_sample: 4_800,
            source_offset_samples: 0,
            length_samples: 4_800,
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 960,
        })
        .unwrap();
    let audio_id = app.project.audio_items()[0].id();
    let second_audio_id = app.project.audio_items()[1].id();
    let midi_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::SetAudioItemMute {
            item_id: audio_id,
            muted: true,
        })
        .unwrap();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![
        (audio_id, track_id),
        (second_audio_id, track_id),
        (midi_id, track_id),
    ]);

    let _ = app.update(Message::OpenItemProperties);
    let draft = app
        .batch_item_properties
        .as_ref()
        .expect("multiple selected items should open batch properties");
    assert_eq!(draft.item_ids.len(), 3);
    assert!(draft.item_ids.contains(&audio_id));
    assert!(draft.item_ids.contains(&second_audio_id));
    assert!(draft.item_ids.contains(&midi_id));
    assert_eq!(
        draft.muted, None,
        "different mute values should display mixed"
    );
    assert!(app.item_properties.is_none());
    assert!(app.midi_item_properties.is_none());

    app.io_busy = true;
    let _ = app.update(Message::BatchItemPropertyMuteChanged(true));
    assert_eq!(
        app.batch_item_properties.as_ref().unwrap().muted,
        Some(true),
        "property drafts remain editable while project I/O is busy"
    );
    app.io_busy = false;

    let revision = app.revision;
    let _ = app.update(Message::ApplyItemProperties(false));
    assert!(app.project.audio_items()[0].is_muted());
    assert!(app.project.audio_items()[1].is_muted());
    assert!(app.project.midi_items()[0].is_muted());
    assert_eq!(app.revision, revision + 1);
    assert!(
        !app.batch_item_properties
            .as_ref()
            .expect("Apply should keep the batch dialog open")
            .has_changes()
    );

    let _ = app.update(Message::Undo);
    assert!(app.project.audio_items()[0].is_muted());
    assert!(!app.project.audio_items()[1].is_muted());
    assert!(!app.project.midi_items()[0].is_muted());
    let _ = app.update(Message::Redo);
    assert!(app.project.audio_items()[0].is_muted());
    assert!(app.project.audio_items()[1].is_muted());
    assert!(app.project.midi_items()[0].is_muted());

    let revision_before_cancel = app.revision;
    let _ = app.update(Message::BatchItemPropertyMuteChanged(false));
    let _ = app.update(Message::CloseItemProperties);
    assert!(app.batch_item_properties.is_none());
    assert_eq!(app.revision, revision_before_cancel);
    assert!(app.project.audio_items().iter().all(|item| item.is_muted()));
    assert!(app.project.midi_items()[0].is_muted());
}

#[test]
fn item_properties_batch_mute_only_blocks_changed_midi_items_during_playback() {
    fn selected_items(audio_muted: bool, midi_muted: bool) -> App {
        let mut app = App::default();
        let _ = app.update(Message::AddTrack);
        let track_id = app.project.tracks()[0].id();
        app.project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: "asset://batch-item-playback".into(),
                start_sample: 0,
                source_offset_samples: 0,
                length_samples: 4_800,
            })
            .unwrap();
        app.project
            .apply(DawAction::InsertMidiItem {
                track_id,
                start_tick: 0,
                length_ticks: 960,
            })
            .unwrap();
        let audio_id = app.project.audio_items()[0].id();
        let midi_id = app.project.midi_items()[0].id();
        app.project
            .apply(DawAction::SetAudioItemMute {
                item_id: audio_id,
                muted: audio_muted,
            })
            .unwrap();
        app.project
            .apply(DawAction::SetMidiItemMute {
                item_id: midi_id,
                muted: midi_muted,
            })
            .unwrap();
        app.timeline.rebuild(&app.project);
        app.timeline
            .select_items(vec![(audio_id, track_id), (midi_id, track_id)]);
        let _ = app.update(Message::OpenItemProperties);
        app
    }

    let mut no_op = selected_items(false, false);
    assert!(no_op.batch_item_properties.as_ref().unwrap().muted == Some(false));
    let revision = no_op.revision;
    let _ = no_op.apply_batch_item_properties_with_playback(true, true);
    assert!(no_op.batch_item_properties.is_none());
    assert_eq!(no_op.revision, revision);

    let mut audio_only = selected_items(false, true);
    let _ = audio_only.update(Message::BatchItemPropertyMuteChanged(true));
    let revision = audio_only.revision;
    let _ = audio_only.apply_batch_item_properties_with_playback(false, true);
    assert!(audio_only.project.audio_items()[0].is_muted());
    assert!(audio_only.project.midi_items()[0].is_muted());
    assert_eq!(audio_only.revision, revision + 1);

    let mut midi_change = selected_items(false, false);
    let _ = midi_change.update(Message::BatchItemPropertyMuteChanged(true));
    let revision = midi_change.revision;
    let _ = midi_change.apply_batch_item_properties_with_playback(false, true);
    assert!(!midi_change.project.audio_items()[0].is_muted());
    assert!(!midi_change.project.midi_items()[0].is_muted());
    assert_eq!(midi_change.revision, revision);
    assert!(
        midi_change
            .batch_item_properties
            .as_ref()
            .unwrap()
            .error
            .is_some()
    );
}

#[test]
fn item_properties_cannot_apply_a_draft_to_another_project_generation() {
    use super::item_properties::{ItemProperties, ItemPropertyField};
    let mut app = App::default();
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".into(),
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: app.project.tracks()[0].id(),
            media_ref: "asset://owned".into(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48000,
        })
        .unwrap();
    let item = &app.project.audio_items()[0];
    app.item_properties = Some(ItemProperties::new(
        item.id(),
        item.fades(),
        item.take_volume(),
        item.take_pan(),
        48000,
    ));
    let _ = app.update(Message::ItemPropertyFieldChanged(
        ItemPropertyField::InLength,
        "0.25".into(),
    ));
    app.project_generation += 1;
    let _ = app.update(Message::ApplyItemProperties(false));
    assert!(app.item_properties.is_none());
    assert_eq!(
        app.project.audio_items()[0].fades(),
        aaadaw_core::AudioItemFades::default()
    );
}

#[test]
fn factory_f2_toggles_item_properties_and_respects_custom_binding() {
    use iced::keyboard::key::Named;
    assert_eq!(
        commands::from_shortcut(
            &Key::Named(Named::F2),
            Modifiers::NONE,
            &HashMap::new(),
            &[]
        ),
        Some(CommandId::ToggleItemProperties)
    );
    let custom = HashMap::from([("track.add".to_owned(), "F2".to_owned())]);
    assert_eq!(
        commands::from_shortcut(&Key::Named(Named::F2), Modifiers::NONE, &custom, &[]),
        Some(CommandId::AddTrack)
    );
    let mut app = App::default();
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".into(),
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: app.project.tracks()[0].id(),
            media_ref: "asset://owned".into(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48000,
        })
        .unwrap();
    app.timeline.selected_item = Some(app.project.audio_items()[0].id());
    let _ = app.update(Message::ToggleItemProperties);
    assert!(app.item_properties_window_id.is_some());
    let window_id = app.item_properties_window_id.unwrap();
    let _ = app.update(Message::RuntimeKeyboardEvent(
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: Key::Named(Named::F2),
            modified_key: Key::Named(Named::F2),
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::F2),
            location: iced::keyboard::Location::Standard,
            modifiers: Modifiers::NONE,
            text: None,
            repeat: false,
        }),
        iced::event::Status::Ignored,
        window_id,
    ));
    assert!(app.item_properties_window_id.is_none());
    assert!(app.item_properties.is_none());
}

#[test]
fn item_properties_apply_placement_and_fades_atomically_and_keep_oversized_requests_on_shortening()
{
    use super::item_properties::{ItemProperties, ItemPropertyField as Field};
    use aaadaw_core::{AudioFade, AudioItemFades, FadeShape};
    let mut app = App::default();
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".into(),
        })
        .unwrap();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: app.project.tracks()[0].id(),
            media_ref: "asset://owned".into(),
            start_sample: 17,
            source_offset_samples: 23,
            length_samples: 48000,
        })
        .unwrap();
    let id = app.project.audio_items()[0].id();
    let original_fades = AudioItemFades {
        fade_in: AudioFade::new(38400.0, FadeShape::Smooth).unwrap(),
        fade_out: AudioFade::new(9600.0, FadeShape::SlowStart).unwrap(),
    };
    app.project
        .apply(DawAction::SetAudioItemFades {
            item_id: id,
            fades: original_fades,
        })
        .unwrap();
    let original = app.project.snapshot();
    app.item_properties = Some(ItemProperties::from_item(
        &app.project.audio_items()[0],
        48000,
        super::item_properties::TimeUnit::Time,
    ));
    for (field, value) in [
        (Field::Position, "0:00.500"),
        (Field::Length, "0:00.250"),
        (Field::SourceOffset, "0:00.125"),
        (Field::InCurvature, "0.25"),
    ] {
        let _ = app.update(Message::ItemPropertyFieldChanged(field, value.into()));
    }
    assert_eq!(app.project.snapshot(), original);
    let _ = app.update(Message::ApplyItemProperties(false));
    let item = &app.project.audio_items()[0];
    assert_eq!(
        (
            item.start_sample(),
            item.length_samples(),
            item.source_offset_samples()
        ),
        (24000, 12000, 6000)
    );
    assert_eq!(item.fades().fade_in.length_samples(), 38400.0);
    assert_eq!(item.fades().fade_out, original_fades.fade_out);
    assert_eq!(item.fades().fade_in.curve().parameters().curvature(), 0.25);
    assert_eq!(item.fades().fade_in.curve().parameters().s_parameter(), 0.5);
    let applied = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), applied);
    let _ = app.update(Message::ItemPropertyFieldChanged(
        Field::Length,
        "0:00.000".into(),
    ));
    let _ = app.update(Message::ItemPropertyFieldChanged(
        Field::OutLength,
        "0:00.100".into(),
    ));
    let _ = app.update(Message::ApplyItemProperties(false));
    assert!(app.item_properties.as_ref().unwrap().error.is_some());
    assert_eq!(app.project.snapshot(), applied);
    let _ = app.update(Message::ItemPropertyFieldChanged(
        Field::Length,
        "1e100".into(),
    ));
    let _ = app.update(Message::ApplyItemProperties(false));
    assert_eq!(app.project.snapshot(), applied);
    let _ = app.update(Message::ItemPropertyFieldChanged(
        Field::Length,
        "0:00.250".into(),
    ));
    let _ = app.update(Message::ItemPropertyFieldChanged(
        Field::OutLength,
        "0:00.150".into(),
    ));
    let _ = app.update(Message::ApplyItemProperties(false));
    let item = &app.project.audio_items()[0];
    assert_eq!(item.fades().fade_out.length_samples(), 7200.0);
    assert_eq!(item.fades().fade_in.length_samples(), 4800.0);
}
