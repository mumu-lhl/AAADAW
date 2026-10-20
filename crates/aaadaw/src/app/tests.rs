#[cfg(feature = "audio-device")]
use super::StereoPeakHold;
use super::commands::{self, CommandId, TrackCommand};
use super::fader;
use super::messages::SharedProjectSessionLock;
use super::project_io::{
    load_project_file, load_project_session, save_project_file,
    save_project_session_file_with_media,
};
use super::shortcut;
#[cfg(feature = "audio-device")]
use super::{ActiveRecording, SharedRecordingStart};
use super::{
    App, ItemSnapDirection, MainMenu, MainWorkspace, MenuNavigation, Message, MidiEditorMode,
    MidiEditorTool, MidiHumanizeScope, MidiQuantizeOperation, MidiQuantizeScope,
    MidiTransposeScope, MobilePanel, PathPickerTarget, fx_chain_plugin_drag_event,
    keyboard_shortcut_event, menu_navigation_event, midi_editor_shortcut_event,
    midi_expression_context_menu_event, midi_item_swing_snap_target, mobile_back_event,
    shortcut_message,
};
#[cfg(feature = "audio-device")]
use super::{prepare_project_playback_file, prepare_project_playback_file_with_loop_points};
use crate::timeline::{SnapGrid, TimelineEvent};
#[cfg(all(feature = "jack-backend", feature = "pipewire-backend"))]
use aaadaw_app::PlaybackBackend;
use aaadaw_core::{
    DawAction, ItemId, MidiChannelPressureData, MidiControllerData, MidiFileFormat,
    MidiFileProjectImport, MidiImportedMarker, MidiImportedMeterPoint, MidiImportedTempoPoint,
    MidiImportedTrack, MidiItemPosition, MidiItemTakeSnapshot, MidiNoteData, MidiOpaqueEventData,
    MidiOpaqueEventKind, MidiPitchBendData, MidiPolyAftertouchData, MidiProgramChangeData, Project,
    RazorEditArea, TrackFxPlugin, TrackId,
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
fn audio_take_actions_switch_with_undo_and_require_multiple_takes() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://audio-take-actions".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .unwrap();
    let item_id = app.project.audio_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(item_id, track_id)]);

    assert!(!commands::is_enabled(
        &app,
        CommandId::SwitchItemsToNextTake
    ));
    let _ = app.update(Message::ExecuteCommand(CommandId::DuplicateActiveTake));
    let item = &app.project.audio_items()[0];
    assert_eq!(item.take_count(), 2);
    assert_eq!(item.active_take_index(), 1);
    assert!(commands::is_enabled(&app, CommandId::SwitchItemsToNextTake));

    let _ = app.update(Message::ExecuteCommand(CommandId::SwitchItemsToNextTake));
    assert_eq!(app.project.audio_items()[0].active_take_index(), 0);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].active_take_index(), 1);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items()[0].active_take_index(), 0);

    let _ = app.update(Message::Undo);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].take_count(), 1);
    assert!(!commands::is_enabled(
        &app,
        CommandId::SwitchItemsToPreviousTake
    ));
}

#[test]
fn midi_take_action_list_switches_and_adds_empty_takes_with_undo() {
    let mut app = App::default();
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
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id,
            notes: vec![
                MidiNoteData {
                    pitch: 60,
                    channel: 0,
                    tick: 0,
                    duration: 480,
                    velocity: 100,
                    off_velocity: 0,
                },
                MidiNoteData {
                    pitch: 64,
                    channel: 0,
                    tick: 960,
                    duration: 480,
                    velocity: 100,
                    off_velocity: 0,
                },
            ],
        })
        .unwrap();
    let mut snapshot = app.project.snapshot();
    let item = &mut snapshot.midi_items[0];
    let note_a = item.notes[0].clone();
    let note_b = item.notes[1].clone();
    item.notes = vec![note_a];
    item.name = "Take A".to_owned();
    item.other_takes = vec![MidiItemTakeSnapshot {
        index: 1,
        name: "Take B".to_owned(),
        source_offset_ticks: 0,
        loop_source: false,
        source_loop_length_ticks: item.length_ticks,
        notes: vec![note_b],
        controllers: Vec::new(),
        pitch_bends: Vec::new(),
        program_changes: Vec::new(),
        poly_aftertouches: Vec::new(),
        channel_pressures: Vec::new(),
        opaque_events: Vec::new(),
        event_order: Vec::new(),
    }];
    app.project = Project::from_snapshot(snapshot).unwrap();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(item_id, track_id)]);
    assert!(commands::is_enabled(&app, CommandId::SwitchItemsToNextTake));

    let before = app.project.snapshot();
    let cursor = app.project.history_cursor();
    let _ = app.update(Message::SwitchMidiItemTake(item_id, true));
    let item = &app.project.midi_items()[0];
    assert_eq!(item.active_take_index(), 1);
    assert_eq!(item.name(), "Take B");
    assert_eq!(item.notes()[0].pitch(), 64);
    assert_eq!(app.project.history_cursor(), cursor + 1);
    let after = app.project.snapshot();

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after);

    let _ = app.update(Message::ExecuteCommand(
        CommandId::SwitchItemsToPreviousTake,
    ));
    assert_eq!(app.project.midi_items()[0].active_take_index(), 0);
    assert_eq!(app.project.midi_items()[0].name(), "Take A");

    let duplicate = commands::shortcut_entries(&app)
        .into_iter()
        .find(|entry| entry.id == "take.duplicate-active")
        .expect("REAPER's Duplicate active take action should be registered");
    assert!(duplicate.default_binding.is_empty());
    assert!(commands::is_enabled(&app, CommandId::DuplicateActiveTake));
    let before_duplicate = app.project.snapshot();
    let cursor = app.project.history_cursor();
    let _ = app.update(Message::ExecuteCommand(CommandId::DuplicateActiveTake));
    let item = &app.project.midi_items()[0];
    assert_eq!(app.project.history_cursor(), cursor + 1);
    assert_eq!(item.active_take_index(), 2);
    assert_eq!(
        item.take_names()
            .into_iter()
            .map(|(_, name)| name)
            .collect::<Vec<_>>(),
        ["Take A", "Take B", "Take A"]
    );
    let after_duplicate = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before_duplicate);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after_duplicate);
    let _ = app.update(Message::Undo);

    for (id, command) in [
        (
            "take.add-empty-before-active",
            CommandId::AddEmptyMidiTakeBeforeActive,
        ),
        (
            "take.add-empty-after-active",
            CommandId::AddEmptyMidiTakeAfterActive,
        ),
    ] {
        let action = commands::shortcut_entries(&app)
            .into_iter()
            .find(|entry| entry.id == id)
            .expect("the REAPER empty-take action should be registered");
        assert!(action.default_binding.is_empty());
        assert!(commands::is_enabled(&app, command));
    }

    let before_insert = app.project.snapshot();
    let cursor = app.project.history_cursor();
    let _ = app.update(Message::ExecuteCommand(
        CommandId::AddEmptyMidiTakeBeforeActive,
    ));
    let item = &app.project.midi_items()[0];
    assert_eq!(app.project.history_cursor(), cursor + 1);
    assert_eq!(item.active_take_index(), 1);
    assert_eq!(item.name(), "Take A");
    assert!(item.take_snapshots()[0].name.is_empty());
    assert_eq!(item.take_snapshots()[1].name, "Take A");
    assert_eq!(item.take_snapshots()[2].name, "Take B");
    let after_insert = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before_insert);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after_insert);

    let _ = app.update(Message::ExecuteCommand(
        CommandId::AddEmptyMidiTakeAfterActive,
    ));
    let item = &app.project.midi_items()[0];
    assert_eq!(item.active_take_index(), 1);
    assert_eq!(item.take_snapshots()[1].name, "Take A");
    assert!(item.take_snapshots()[2].name.is_empty());
    assert_eq!(item.take_snapshots()[3].name, "Take B");

    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 3_840,
            length_ticks: 3_840,
        })
        .unwrap();
    let second_item_id = app.project.midi_items()[1].id();
    app.timeline.rebuild(&app.project);
    app.timeline
        .select_items(vec![(item_id, track_id), (second_item_id, track_id)]);
    let before_multi = app.project.snapshot();
    let cursor = app.project.history_cursor();
    let _ = app.update(Message::AddEmptyMidiTakes(false));
    assert_eq!(app.project.history_cursor(), cursor + 1);
    assert_eq!(app.project.midi_items()[0].take_count(), 5);
    assert_eq!(app.project.midi_items()[0].active_take_index(), 1);
    assert_eq!(app.project.midi_items()[1].take_count(), 2);
    assert_eq!(app.project.midi_items()[1].active_take_index(), 0);
    let after_multi = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before_multi);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after_multi);

    app.timeline.select_items(vec![(item_id, track_id)]);
    let before_delete = app.project.snapshot();
    let cursor = app.project.history_cursor();
    assert!(commands::is_enabled(&app, CommandId::DeleteActiveTake));
    let _ = app.update(Message::ExecuteCommand(CommandId::DeleteActiveTake));
    assert_eq!(app.project.history_cursor(), cursor + 1);
    let item = &app.project.midi_items()[0];
    assert_eq!(item.take_count(), 4);
    assert_eq!(item.active_take_index(), 0);
    assert!(item.name().is_empty());
    let after_delete = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before_delete);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after_delete);
}

#[test]
fn deleting_mixed_selected_audio_and_midi_takes_is_one_undoable_action() {
    let mut app = App::default();
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Mixed take deletion".to_owned(),
        })
        .unwrap();
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://delete-take-a".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .unwrap();
    let audio_id = app.project.audio_items()[0].id();
    app.project
        .apply(DawAction::DuplicateAudioItemTake { item_id: audio_id })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 1_920,
        })
        .unwrap();
    let midi_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::DuplicateMidiItemTake { item_id: midi_id })
        .unwrap();
    app.timeline.rebuild(&app.project);
    app.timeline
        .select_items(vec![(audio_id, track_id), (midi_id, track_id)]);

    let before = app.project.snapshot();
    let cursor = app.project.history_cursor();
    assert!(commands::is_enabled(&app, CommandId::DeleteActiveTake));
    let _ = app.update(Message::ExecuteCommand(CommandId::DeleteActiveTake));
    assert_eq!(app.project.history_cursor(), cursor + 1);
    assert_eq!(app.project.audio_items().len(), 1);
    assert_eq!(app.project.audio_items()[0].take_count(), 1);
    assert_eq!(app.project.audio_items()[0].active_take_index(), 0);
    assert_eq!(app.project.midi_items().len(), 1);
    assert_eq!(app.project.midi_items()[0].take_count(), 1);
    assert_eq!(app.project.midi_items()[0].active_take_index(), 0);
    let after = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after);
}

#[test]
fn imploding_selected_audio_items_keeps_track_order_all_takes_and_one_step_history() {
    let mut app = App::default();
    for (index, name) in ["Implode", "Other"].into_iter().enumerate() {
        app.project
            .apply(DawAction::CreateTrack {
                index,
                name: name.to_owned(),
            })
            .expect("track should be created");
    }
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://late-active".to_owned(),
            start_sample: 48_000,
            source_offset_samples: 900,
            length_samples: 72_000,
        })
        .expect("late source should be inserted");
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://base-active".to_owned(),
            start_sample: 0,
            source_offset_samples: 100,
            length_samples: 48_000,
        })
        .expect("base item should be inserted");
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://middle-old".to_owned(),
            start_sample: 48_000,
            source_offset_samples: 500,
            length_samples: 60_000,
        })
        .expect("middle source should be inserted");
    let late_id = app.project.audio_items()[0].id();
    let base_id = app.project.audio_items()[1].id();
    let middle_id = app.project.audio_items()[2].id();

    let mut base_alternate = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == base_id)
        .unwrap()
        .active_take_snapshot();
    base_alternate.media_ref = "asset://base-alternate".to_owned();
    base_alternate.name = "Base alternate".to_owned();
    base_alternate.phase_inverted = true;
    base_alternate.volume = 0.5;
    base_alternate.pan = -0.25;
    base_alternate.playback_rate = 0.9;
    app.project
        .apply(DawAction::PasteAudioItemAsTake {
            item_id: base_id,
            take: base_alternate,
        })
        .expect("base should retain its existing take");

    let mut middle_active = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == middle_id)
        .unwrap()
        .active_take_snapshot();
    middle_active.media_ref = "asset://middle-active".to_owned();
    middle_active.name = "Middle active".to_owned();
    middle_active.volume = 0.7;
    middle_active.pan = 0.3;
    middle_active.playback_rate = 1.1;
    app.project
        .apply(DawAction::PasteAudioItemAsTake {
            item_id: middle_id,
            take: middle_active,
        })
        .expect("middle source should have an active alternate");

    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![
        (late_id, track_id),
        (middle_id, track_id),
        (base_id, track_id),
    ]);
    let before = app.project.snapshot();
    let history_cursor = app.project.history_cursor();
    let menu_entry = commands::for_menu(&app, MainMenu::Item)
        .into_iter()
        .find(|entry| entry.id == CommandId::ImplodeAudioItemsOnSameTrackIntoTakes)
        .expect("Item menu should expose the same-track Take action");
    assert!(menu_entry.enabled);
    let action = commands::shortcut_entries(&app)
        .into_iter()
        .find(|entry| entry.id == "take.implode-same-track")
        .expect("REAPER's 40543 action should be registered");
    assert!(action.default_binding.is_empty());

    let _ = app.update(Message::ExecuteCommand(
        CommandId::ImplodeAudioItemsOnSameTrackIntoTakes,
    ));

    assert_eq!(app.project.history_cursor(), history_cursor + 1);
    assert_eq!(app.status, "Selected audio items imploded into takes");
    assert_eq!(app.project.audio_items().len(), 1);
    let target = &app.project.audio_items()[0];
    assert_eq!(target.id(), base_id);
    assert_eq!(target.start_sample(), 0);
    assert_eq!(target.length_samples(), 48_000);
    assert_eq!(target.take_count(), 5);
    assert_eq!(target.active_take_index(), 1);
    assert_eq!(target.media_ref(), "asset://base-alternate");
    assert_eq!(
        target
            .take_snapshots()
            .iter()
            .map(|take| (take.index, take.name.as_str(), take.media_ref.as_str()))
            .collect::<Vec<_>>(),
        [
            (0, "", "asset://base-active"),
            (1, "Base alternate", "asset://base-alternate"),
            (2, "", "asset://middle-old"),
            (3, "Middle active", "asset://middle-active"),
            (4, "", "asset://late-active"),
        ]
    );
    let middle_active = &target.take_snapshots()[3];
    assert_eq!(middle_active.source_offset_samples, 500);
    assert_eq!(middle_active.volume, 0.7);
    assert_eq!(middle_active.pan, 0.3);
    assert_eq!(middle_active.playback_rate, 1.1);
    assert_eq!(app.timeline.selected_items, HashSet::from([base_id]));
    assert_eq!(app.timeline.selected_item, Some(base_id));

    let after = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    assert!(app.timeline.selected_items.is_empty());
    assert_eq!(app.timeline.selected_item, None);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after);
    assert_eq!(app.timeline.selected_items, HashSet::from([base_id]));
    assert_eq!(app.timeline.selected_item, Some(base_id));
}

#[test]
fn implode_same_track_takes_rejects_single_and_cross_track_selections() {
    let mut app = App::default();
    for (index, name) in ["First", "Second"].into_iter().enumerate() {
        app.project
            .apply(DawAction::CreateTrack {
                index,
                name: name.to_owned(),
            })
            .expect("track should be created");
    }
    let first_track = app.project.tracks()[0].id();
    let second_track = app.project.tracks()[1].id();
    for (track_id, media_ref) in [
        (first_track, "asset://first"),
        (second_track, "asset://second"),
    ] {
        app.project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: media_ref.to_owned(),
                start_sample: 0,
                source_offset_samples: 0,
                length_samples: 48_000,
            })
            .expect("audio item should be inserted");
    }
    let first_item = app.project.audio_items()[0].id();
    let second_item = app.project.audio_items()[1].id();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(first_item, first_track)]);
    assert!(!commands::is_enabled(
        &app,
        CommandId::ImplodeAudioItemsOnSameTrackIntoTakes
    ));

    app.timeline
        .select_items(vec![(first_item, first_track), (second_item, second_track)]);
    assert!(!commands::is_enabled(
        &app,
        CommandId::ImplodeAudioItemsOnSameTrackIntoTakes
    ));
    let before = app.project.snapshot();
    let _ = app.update(Message::ExecuteCommand(
        CommandId::ImplodeAudioItemsOnSameTrackIntoTakes,
    ));
    assert_eq!(
        app.status,
        "Select at least two audio items on the same track"
    );
    assert_eq!(app.project.snapshot(), before);
}

#[test]
fn imploding_selected_audio_items_across_tracks_rebases_each_take_and_keeps_top_track_target() {
    let file_id = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
    let project_path = std::env::temp_dir().join(format!(
        "aaadaw-ui-cross-track-implode-{}-{file_id}.aaadaw",
        std::process::id()
    ));
    let source_rates = [
        ("asset://cross-target", 48_000),
        ("asset://cross-target-alt", 48_000),
        ("asset://cross-middle", 44_100),
        ("asset://cross-middle-alt", 48_000),
        ("asset://cross-bottom", 48_000),
    ];
    let mut store = ProjectStore::open(&project_path).expect("project media store should open");
    for (media_ref, sample_rate) in source_rates {
        store
            .import_audio_asset(
                media_ref,
                "cross-track.wav",
                std::io::Cursor::new([0_u8; 4]),
            )
            .expect("test source should import");
        store
            .set_audio_asset_metadata(
                media_ref,
                &aaadaw_storage::AudioAssetMetadata {
                    container: "wav".to_owned(),
                    codec: "pcm".to_owned(),
                    sample_rate: Some(sample_rate),
                    channel_count: Some(1),
                    bits_per_sample: Some(16),
                    frame_count: Some(500_000),
                    duration_nanos: None,
                    byte_len: Some(4),
                },
            )
            .expect("source metadata should be stored");
    }
    store.close().expect("project media store should close");

    let mut app = App {
        project_path: Some(project_path.clone()),
        ..App::default()
    };
    for (index, name) in ["Top", "Middle", "Bottom"].into_iter().enumerate() {
        app.project
            .apply(DawAction::CreateTrack {
                index,
                name: name.to_owned(),
            })
            .expect("track should be created");
    }
    let track_ids = app
        .project
        .tracks()
        .iter()
        .map(|track| track.id())
        .collect::<Vec<_>>();
    let top_track = track_ids[0];
    let middle_track = track_ids[1];
    let bottom_track = track_ids[2];

    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: top_track,
            media_ref: "asset://cross-target".to_owned(),
            start_sample: 48_000,
            source_offset_samples: 6_000,
            length_samples: 48_000,
        })
        .expect("target should be inserted");
    let target_id = app.project.audio_items()[0].id();
    let mut target_alternate = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == target_id)
        .unwrap()
        .active_take_snapshot();
    target_alternate.media_ref = "asset://cross-target-alt".to_owned();
    target_alternate.name = "Target alternate".to_owned();
    target_alternate.source_offset_samples = 18_000;
    target_alternate.playback_rate = 0.9;
    app.project
        .apply(DawAction::PasteAudioItemAsTake {
            item_id: target_id,
            take: target_alternate,
        })
        .expect("target should keep its alternate take");

    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: middle_track,
            media_ref: "asset://cross-middle".to_owned(),
            start_sample: 24_000,
            source_offset_samples: 11_025,
            length_samples: 48_000,
        })
        .expect("middle donor should be inserted");
    let middle_id = app.project.audio_items()[1].id();
    app.project
        .apply(DawAction::SetAudioItemTakePlaybackRate {
            item_id: middle_id,
            rate: 1.1,
        })
        .expect("middle source rate should be set");
    let mut middle_alternate = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == middle_id)
        .unwrap()
        .active_take_snapshot();
    middle_alternate.media_ref = "asset://cross-middle-alt".to_owned();
    middle_alternate.name = "Middle alternate".to_owned();
    middle_alternate.source_offset_samples = 24_000;
    middle_alternate.playback_rate = 0.8;
    app.project
        .apply(DawAction::PasteAudioItemAsTake {
            item_id: middle_id,
            take: middle_alternate,
        })
        .expect("middle donor should keep its second take");

    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: bottom_track,
            media_ref: "asset://cross-bottom".to_owned(),
            start_sample: 72_000,
            source_offset_samples: 18_000,
            length_samples: 48_000,
        })
        .expect("bottom donor should be inserted");
    let bottom_id = app.project.audio_items()[2].id();
    app.project
        .apply(DawAction::SetAudioItemTakePlaybackRate {
            item_id: bottom_id,
            rate: 1.2,
        })
        .expect("bottom source rate should be set");

    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![
        (bottom_id, bottom_track),
        (middle_id, middle_track),
        (target_id, top_track),
    ]);
    assert!(commands::audio_items_for_cross_track_implode(&app).is_some());
    assert!(!commands::project_edit_busy(&app));
    let before = app.project.snapshot();
    let history_cursor = app.project.history_cursor();
    let menu_entry = commands::for_menu(&app, MainMenu::Item)
        .into_iter()
        .find(|entry| entry.id == CommandId::ImplodeAudioItemsAcrossTracksIntoTakes)
        .expect("Item menu should expose cross-track implode");
    assert!(menu_entry.enabled);
    let keymap_action = commands::shortcut_entries(&app)
        .into_iter()
        .find(|entry| entry.id == "take.implode-across-tracks")
        .expect("REAPER action 40438 should be registered");
    assert!(keymap_action.default_binding.is_empty());

    let _ = app.update(Message::ExecuteCommand(
        CommandId::ImplodeAudioItemsAcrossTracksIntoTakes,
    ));

    assert_eq!(app.project.history_cursor(), history_cursor + 1);
    assert_eq!(app.project.audio_items().len(), 1);
    let target = &app.project.audio_items()[0];
    assert_eq!(target.id(), target_id);
    assert_eq!(target.track_id(), top_track);
    assert_eq!(target.start_sample(), 48_000);
    assert_eq!(target.length_samples(), 48_000);
    assert_eq!(target.active_take_index(), 1);
    assert_eq!(target.take_count(), 5);
    let takes = target.take_snapshots();
    assert_eq!(takes[0].media_ref, "asset://cross-target");
    assert_eq!(takes[1].name, "Target alternate");
    assert_eq!(takes[1].source_offset_samples, 18_000);
    assert_eq!(takes[2].source_offset_samples, 35_280);
    assert_eq!(takes[2].playback_rate, 1.1);
    assert_eq!(takes[3].media_ref, "asset://cross-middle-alt");
    assert_eq!(takes[3].source_offset_samples, 43_200);
    assert_eq!(takes[3].playback_rate, 0.8);
    assert_eq!(takes[4].source_offset_samples, -10_800);
    assert_eq!(takes[4].playback_rate, 1.2);
    assert_eq!(app.timeline.selected_items, HashSet::from([target_id]));
    assert_eq!(app.timeline.selected_item, Some(target_id));

    let after = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    assert!(app.timeline.selected_items.is_empty());
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after);
    assert_eq!(app.timeline.selected_items, HashSet::from([target_id]));
}

#[test]
fn cross_track_implode_rejects_single_track_selections_and_accepts_loop_sources() {
    let mut app = App::default();
    for (index, name) in ["First", "Second"].into_iter().enumerate() {
        app.project
            .apply(DawAction::CreateTrack {
                index,
                name: name.to_owned(),
            })
            .expect("track should be created");
    }
    let first_track = app.project.tracks()[0].id();
    let second_track = app.project.tracks()[1].id();
    for (track_id, media_ref) in [
        (first_track, "asset://first"),
        (first_track, "asset://same-track"),
        (second_track, "asset://second"),
    ] {
        app.project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: media_ref.to_owned(),
                start_sample: 0,
                source_offset_samples: 0,
                length_samples: 48_000,
            })
            .expect("audio item should be inserted");
    }
    let first_item = app.project.audio_items()[0].id();
    let same_track_item = app.project.audio_items()[1].id();
    let second_item = app.project.audio_items()[2].id();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![
        (first_item, first_track),
        (same_track_item, first_track),
    ]);
    assert!(!commands::is_enabled(
        &app,
        CommandId::ImplodeAudioItemsAcrossTracksIntoTakes
    ));

    app.timeline
        .select_items(vec![(first_item, first_track), (second_item, second_track)]);
    assert!(commands::is_enabled(
        &app,
        CommandId::ImplodeAudioItemsAcrossTracksIntoTakes
    ));
}

#[test]
fn cross_track_implode_requires_overlapping_selected_items() {
    let mut app = App::default();
    for (index, name) in ["Top", "Bottom"].into_iter().enumerate() {
        app.project
            .apply(DawAction::CreateTrack {
                index,
                name: name.to_owned(),
            })
            .expect("track should be created");
    }
    let top_track = app.project.tracks()[0].id();
    let bottom_track = app.project.tracks()[1].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: top_track,
            media_ref: "asset://abutting-top".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .expect("top audio item should be inserted");
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: bottom_track,
            media_ref: "asset://abutting-bottom".to_owned(),
            start_sample: 48_000,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .expect("bottom audio item should be inserted");
    let items = app.project.audio_items();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![
        (items[0].id(), top_track),
        (items[1].id(), bottom_track),
    ]);

    assert!(commands::audio_items_for_cross_track_implode(&app).is_none());
    assert!(!commands::is_enabled(
        &app,
        CommandId::ImplodeAudioItemsAcrossTracksIntoTakes
    ));
}

#[test]
fn exploding_selected_audio_takes_in_place_preserves_order_and_is_one_undo_step() {
    fn insert_multitake(
        app: &mut App,
        track_id: TrackId,
        media_ref: &str,
        start_sample: u64,
        length_samples: u64,
        names: &[&str],
        active_take_index: u32,
    ) -> ItemId {
        app.project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: media_ref.to_owned(),
                start_sample,
                source_offset_samples: 0,
                length_samples,
            })
            .expect("audio item should be inserted");
        let item_id = app.project.audio_items().last().unwrap().id();
        for _ in 1..names.len() {
            app.project
                .apply(DawAction::DuplicateAudioItemTake { item_id })
                .expect("take should be duplicated");
        }
        for (index, name) in names.iter().enumerate() {
            app.project
                .apply(DawAction::SetAudioItemActiveTake {
                    item_id,
                    take_index: index as u32,
                })
                .expect("take should be selected for setup");
            app.project
                .apply(DawAction::SetAudioItemName {
                    item_id,
                    name: (*name).to_owned(),
                })
                .expect("take should be named");
            app.project
                .apply(DawAction::SetAudioItemTakePlaybackRate {
                    item_id,
                    rate: 0.8 + index as f64 * 0.2,
                })
                .expect("take rate should be set");
        }
        app.project
            .apply(DawAction::SetAudioItemActiveTake {
                item_id,
                take_index: active_take_index,
            })
            .expect("requested take should become active");
        item_id
    }

    let mut app = App::default();
    for (index, name) in ["Top", "Bottom"].into_iter().enumerate() {
        app.project
            .apply(DawAction::CreateTrack {
                index,
                name: name.to_owned(),
            })
            .expect("track should be created");
    }
    let top_track = app.project.tracks()[0].id();
    let bottom_track = app.project.tracks()[1].id();
    let top_id = insert_multitake(
        &mut app,
        top_track,
        "asset://explode-top",
        48_000,
        48_000,
        &["Top A", "Top B", "Top C"],
        1,
    );
    let bottom_id = insert_multitake(
        &mut app,
        bottom_track,
        "asset://explode-bottom",
        0,
        72_000,
        &["Bottom A", "Bottom B"],
        0,
    );
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id: bottom_track,
            media_ref: "asset://explode-single".to_owned(),
            start_sample: 96_000,
            source_offset_samples: 0,
            length_samples: 24_000,
        })
        .expect("single-take control should be inserted");
    let single_id = app.project.audio_items().last().unwrap().id();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![
        (bottom_id, bottom_track),
        (single_id, bottom_track),
        (top_id, top_track),
    ]);
    let before = app.project.snapshot();
    let history_cursor = app.project.history_cursor();
    assert!(commands::is_enabled(
        &app,
        CommandId::ExplodeAudioItemTakesInPlace
    ));
    let action = commands::shortcut_entries(&app)
        .into_iter()
        .find(|entry| entry.id == "take.explode-in-place")
        .expect("REAPER action 40642 should be registered");
    assert!(action.default_binding.is_empty());

    let _ = app.update(Message::ExecuteCommand(
        CommandId::ExplodeAudioItemTakesInPlace,
    ));

    assert_eq!(app.project.history_cursor(), history_cursor + 1);
    assert_eq!(app.status, "Selected item takes exploded in place");
    assert_eq!(app.project.audio_items().len(), 6);
    let names = app
        .project
        .audio_items()
        .iter()
        .map(|item| item.name())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        ["Top A", "Top B", "Top C", "Bottom A", "Bottom B", "",]
    );
    assert_eq!(
        app.project
            .audio_items()
            .iter()
            .map(|item| item.track_id())
            .collect::<Vec<_>>(),
        [
            top_track,
            top_track,
            top_track,
            bottom_track,
            bottom_track,
            bottom_track
        ]
    );
    assert_eq!(app.project.audio_items()[5].id(), single_id);
    assert!(
        app.project
            .audio_items()
            .iter()
            .all(|item| { item.take_count() == 1 && item.active_take_index() == 0 })
    );
    assert!(app.timeline.selected_items.is_empty());
    let after = app.project.snapshot();

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    assert!(app.timeline.selected_items.is_empty());
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after);
    assert!(app.timeline.selected_items.is_empty());
}

#[test]
fn exploding_mixed_audio_and_midi_takes_is_one_undo_step() {
    let mut app = App::default();
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Mixed takes".into(),
        })
        .unwrap();
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://mixed-take-a".into(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .unwrap();
    let audio_id = app.project.audio_items()[0].id();
    app.project
        .apply(DawAction::DuplicateAudioItemTake { item_id: audio_id })
        .unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 1_920,
        })
        .unwrap();
    let midi_id = app.project.midi_items()[0].id();
    let mut snapshot = app.project.snapshot();
    let midi = &mut snapshot.midi_items[0];
    midi.active_take_index = 1;
    midi.name = "MIDI take B".into();
    midi.other_takes = vec![MidiItemTakeSnapshot {
        index: 0,
        name: "MIDI take A".into(),
        source_offset_ticks: 0,
        loop_source: false,
        source_loop_length_ticks: midi.length_ticks,
        notes: Vec::new(),
        controllers: Vec::new(),
        pitch_bends: Vec::new(),
        program_changes: Vec::new(),
        poly_aftertouches: Vec::new(),
        channel_pressures: Vec::new(),
        opaque_events: Vec::new(),
        event_order: Vec::new(),
    }];
    app.project = Project::from_snapshot(snapshot).unwrap();
    app.timeline.rebuild(&app.project);
    app.timeline
        .select_items(vec![(audio_id, track_id), (midi_id, track_id)]);
    let before = app.project.snapshot();
    let history_cursor = app.project.history_cursor();

    assert!(commands::is_enabled(
        &app,
        CommandId::ExplodeAudioItemTakesInPlace
    ));
    let _ = app.update(Message::ExecuteCommand(
        CommandId::ExplodeAudioItemTakesInPlace,
    ));

    assert_eq!(app.project.history_cursor(), history_cursor + 1);
    assert_eq!(app.status, "Selected item takes exploded in place");
    assert_eq!(app.project.audio_items().len(), 2);
    assert_eq!(app.project.midi_items().len(), 2);
    assert!(
        app.project
            .audio_items()
            .iter()
            .all(|item| item.take_count() == 1)
    );
    assert!(
        app.project
            .midi_items()
            .iter()
            .all(|item| item.take_count() == 1)
    );
    assert!(app.timeline.selected_items.is_empty());
    let after = app.project.snapshot();

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after);
    assert!(app.timeline.selected_items.is_empty());
}

#[test]
fn exploding_only_selected_single_take_items_clears_selection_without_history() {
    let mut app = App::default();
    app.project
        .apply(DawAction::CreateTrack {
            index: 0,
            name: "Audio".to_owned(),
        })
        .expect("track should be created");
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://single-take".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .expect("single-take item should be inserted");
    let item_id = app.project.audio_items()[0].id();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(item_id, track_id)]);
    let before = app.project.snapshot();
    let history_cursor = app.project.history_cursor();

    let _ = app.update(Message::ExecuteCommand(
        CommandId::ExplodeAudioItemTakesInPlace,
    ));

    assert_eq!(app.project.snapshot(), before);
    assert_eq!(app.project.history_cursor(), history_cursor);
    assert!(app.timeline.selected_items.is_empty());
}

#[test]
fn item_properties_take_selector_lists_ordered_names_and_tracks_undo_redo() {
    use super::item_properties::{ItemProperties, TimeUnit};

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
            media_ref: "asset://take-properties-selector".into(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .unwrap();
    let item_id = app.project.audio_items()[0].id();
    app.project
        .apply(DawAction::DuplicateAudioItemTake { item_id })
        .unwrap();
    app.project
        .apply(DawAction::SetAudioItemName {
            item_id,
            name: "Take B".into(),
        })
        .unwrap();
    app.project
        .apply(DawAction::SetAudioItemTakeMix {
            item_id,
            volume: 0.5,
            pan: 0.25,
        })
        .unwrap();
    app.project
        .apply(DawAction::SetAudioItemTakePlaybackRate {
            item_id,
            rate: 1.25,
        })
        .unwrap();
    app.project
        .apply(DawAction::SetAudioItemLoopSource {
            item_id,
            enabled: true,
        })
        .unwrap();
    app.project
        .apply(DawAction::SetAudioItemPhase {
            item_id,
            inverted: true,
        })
        .unwrap();
    app.project
        .apply(DawAction::SetAudioItemActiveTake {
            item_id,
            take_index: 0,
        })
        .unwrap();
    app.project
        .apply(DawAction::SetAudioItemName {
            item_id,
            name: "Take A".into(),
        })
        .unwrap();

    let item = &app.project.audio_items()[0];
    let draft = ItemProperties::from_item(item, 48_000, TimeUnit::Time);
    assert_eq!(draft.active_take.index, 0);
    assert_eq!(
        draft
            .take_choices
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["1: Take A", "2: Take B"]
    );
    app.item_properties = Some(draft);
    app.item_properties_generation = app.project_generation;

    let history_cursor = app.project.history_cursor();
    let _ = app.update(Message::ItemPropertyTakeChanged(99));
    assert_eq!(app.project.history_cursor(), history_cursor);
    let _ = app.update(Message::ItemPropertyTakeChanged(1));
    assert_eq!(app.project.audio_items()[0].active_take_index(), 0);
    assert_eq!(app.item_properties.as_ref().unwrap().active_take.index, 1);
    assert_eq!(app.item_properties.as_ref().unwrap().name, "Take B");
    assert_eq!(app.item_properties.as_ref().unwrap().take_rate, "1.25");
    assert!(app.item_properties.as_ref().unwrap().loop_source);
    assert!(app.item_properties.as_ref().unwrap().phase_inverted);
    assert_eq!(app.item_properties.as_ref().unwrap().fields[9], "-6.02dB");
    assert_eq!(app.item_properties.as_ref().unwrap().fields[10], "25%R");
    assert_eq!(app.project.history_cursor(), history_cursor);

    let _ = app.update(Message::ApplyItemProperties(false));
    assert_eq!(app.project.audio_items()[0].active_take_index(), 1);
    assert_eq!(app.project.audio_items()[0].name(), "Take B");
    assert_eq!(app.project.audio_items()[0].take_volume(), 0.5);
    assert_eq!(app.project.audio_items()[0].take_pan(), 0.25);
    assert_eq!(app.project.audio_items()[0].take_playback_rate(), 1.25);
    assert!(app.project.audio_items()[0].is_loop_source());
    assert!(app.project.audio_items()[0].is_phase_inverted());
    assert_eq!(app.project.history_cursor(), history_cursor + 1);
    assert_eq!(app.item_properties.as_ref().unwrap().active_take.index, 1);

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].active_take_index(), 0);
    assert_eq!(app.item_properties.as_ref().unwrap().active_take.index, 0);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items()[0].active_take_index(), 1);
    assert_eq!(app.item_properties.as_ref().unwrap().active_take.index, 1);
}

#[test]
fn switching_selected_audio_takes_wraps_skips_single_takes_and_uses_one_undo_step() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    for (media_ref, start_sample) in [
        ("asset://two-take-item", 0),
        ("asset://single-take-item", 96_000),
        ("asset://three-take-item", 192_000),
    ] {
        app.project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: media_ref.to_owned(),
                start_sample,
                source_offset_samples: 0,
                length_samples: 48_000,
            })
            .expect("audio item should be inserted");
    }
    let first_id = app.project.audio_items()[0].id();
    let single_id = app.project.audio_items()[1].id();
    let last_id = app.project.audio_items()[2].id();
    app.project
        .apply(DawAction::DuplicateAudioItemTake { item_id: first_id })
        .expect("two-take item should be created");
    app.project
        .apply(DawAction::SetAudioItemActiveTake {
            item_id: first_id,
            take_index: 0,
        })
        .expect("first take should be active");
    app.project
        .apply(DawAction::DuplicateAudioItemTake { item_id: last_id })
        .expect("second take should be created");
    app.project
        .apply(DawAction::DuplicateAudioItemTake { item_id: last_id })
        .expect("third take should be created");
    assert_eq!(
        app.project
            .audio_items()
            .iter()
            .map(|item| item.active_take_index())
            .collect::<Vec<_>>(),
        [0, 0, 2]
    );

    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![
        (first_id, track_id),
        (single_id, track_id),
        (last_id, track_id),
    ]);
    assert!(commands::is_enabled(&app, CommandId::SwitchItemsToNextTake));
    assert!(commands::is_enabled(
        &app,
        CommandId::SwitchItemsToPreviousTake
    ));

    let before = app.project.snapshot();
    let history_cursor = app.project.history_cursor();
    let _ = app.update(Message::ExecuteCommand(CommandId::SwitchItemsToNextTake));
    let after_next = app.project.snapshot();
    assert_eq!(app.project.history_cursor(), history_cursor + 1);
    assert_eq!(
        app.project
            .audio_items()
            .iter()
            .map(|item| item.active_take_index())
            .collect::<Vec<_>>(),
        [0, 0, 0]
    );
    assert_eq!(app.status, "Selected active takes changed");

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after_next);

    let next_history_cursor = app.project.history_cursor();
    let _ = app.update(Message::ExecuteCommand(
        CommandId::SwitchItemsToPreviousTake,
    ));
    assert_eq!(app.project.history_cursor(), next_history_cursor + 1);
    assert_eq!(
        app.project
            .audio_items()
            .iter()
            .map(|item| item.active_take_index())
            .collect::<Vec<_>>(),
        [1, 0, 2]
    );
    let after_previous = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), after_next);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after_previous);

    let previous_history_cursor = app.project.history_cursor();
    let _ = app.update(Message::ExecuteCommand(
        CommandId::SwitchItemsToPreviousTake,
    ));
    assert_eq!(app.project.history_cursor(), previous_history_cursor + 1);
    assert_eq!(
        app.project
            .audio_items()
            .iter()
            .map(|item| item.active_take_index())
            .collect::<Vec<_>>(),
        [1, 0, 1]
    );
    let after_second_previous = app.project.snapshot();

    let shared_slot_history_cursor = app.project.history_cursor();
    let _ = app.update(Message::ExecuteCommand(CommandId::SwitchItemsToNextTake));
    assert_eq!(app.project.history_cursor(), shared_slot_history_cursor + 1);
    assert_eq!(
        app.project
            .audio_items()
            .iter()
            .map(|item| item.active_take_index())
            .collect::<Vec<_>>(),
        [1, 0, 2]
    );
    let after_shared_slot_next = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), after_second_previous);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after_shared_slot_next);
}

#[test]
fn duplicating_multiple_selected_audio_and_midi_takes_is_one_undoable_transaction() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    for (media_ref, start_sample) in [
        ("asset://take-b", 0),
        ("asset://take-a", 96_000),
        ("asset://take-b", 192_000),
    ] {
        app.project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: media_ref.to_owned(),
                start_sample,
                source_offset_samples: 0,
                length_samples: 48_000,
            })
            .expect("audio item should be inserted");
    }
    let first_id = app.project.audio_items()[0].id();
    let single_id = app.project.audio_items()[1].id();
    let last_id = app.project.audio_items()[2].id();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 3_840,
        })
        .expect("MIDI item should be inserted");
    let midi_item_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id: midi_item_id,
            notes: vec![MidiNoteData {
                pitch: 60,
                channel: 0,
                tick: 120,
                duration: 240,
                velocity: 100,
                off_velocity: 0,
            }],
        })
        .expect("MIDI source note should be inserted");
    for item_id in [first_id, last_id] {
        let mut take = app
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == item_id)
            .expect("multi-take target should exist")
            .active_take_snapshot();
        take.media_ref = "asset://take-a".to_owned();
        take.name = "Take A".to_owned();
        app.project
            .apply(DawAction::PasteAudioItemAsTake { item_id, take })
            .expect("Take A should be appended");
    }
    app.project
        .apply(DawAction::SetAudioItemActiveTake {
            item_id: first_id,
            take_index: 0,
        })
        .expect("the first item's Take B should be selected at index 0");
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![
        (first_id, track_id),
        (single_id, track_id),
        (last_id, track_id),
        (midi_item_id, track_id),
    ]);
    let before = app.project.snapshot();
    let history_cursor = app.project.history_cursor();
    assert!(commands::is_enabled(&app, CommandId::DuplicateActiveTake));

    let _ = app.update(Message::ExecuteCommand(CommandId::DuplicateActiveTake));
    assert_eq!(app.project.history_cursor(), history_cursor + 1);
    assert_eq!(app.status, "Selected active takes duplicated");
    let first = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == first_id)
        .unwrap();
    assert_eq!(first.take_count(), 3);
    assert_eq!(first.active_take_index(), 2);
    assert_eq!(first.media_ref(), "asset://take-b");
    assert_eq!(
        first
            .other_takes()
            .iter()
            .map(|take| (take.index(), take.media_ref()))
            .collect::<Vec<_>>(),
        [(0, "asset://take-b"), (1, "asset://take-a")]
    );
    let single = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == single_id)
        .unwrap();
    assert_eq!(single.take_count(), 2);
    assert_eq!(single.active_take_index(), 1);
    assert_eq!(single.media_ref(), "asset://take-a");
    let last = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == last_id)
        .unwrap();
    assert_eq!(last.take_count(), 3);
    assert_eq!(last.active_take_index(), 2);
    assert_eq!(last.media_ref(), "asset://take-a");
    assert_eq!(
        last.other_takes()
            .iter()
            .map(|take| (take.index(), take.media_ref()))
            .collect::<Vec<_>>(),
        [(0, "asset://take-b"), (1, "asset://take-a")]
    );
    let midi = &app.project.midi_items()[0];
    assert_eq!(midi.take_count(), 2);
    assert_eq!(midi.active_take_index(), 1);
    let midi_takes = midi.take_snapshots();
    assert_eq!(midi_takes[0].notes[0].data, midi_takes[1].notes[0].data);
    assert_ne!(midi_takes[0].notes[0].id, midi_takes[1].notes[0].id);

    let after = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after);
}

#[test]
fn moving_active_audio_take_to_top_is_registered_and_undoable() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://take-a".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .expect("first take item should be inserted");
    let item_id = app.project.audio_items()[0].id();
    let mut second_take = app.project.audio_items()[0].active_take_snapshot();
    second_take.media_ref = "asset://take-b".to_owned();
    second_take.name = "Take B".to_owned();
    app.project
        .apply(DawAction::PasteAudioItemAsTake {
            item_id,
            take: second_take,
        })
        .expect("second take should be appended and active");
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(item_id, track_id)]);

    let action = commands::shortcut_entries(&app)
        .into_iter()
        .find(|entry| entry.id == "take.move-active-to-top")
        .expect("REAPER's Move active takes to top action should be registered");
    assert!(action.default_binding.is_empty());
    let menu_entry = commands::for_menu(&app, MainMenu::Item)
        .into_iter()
        .find(|entry| entry.id == CommandId::MoveActiveTakesToTop)
        .expect("Item menu should expose Move active takes to top");
    assert_eq!(menu_entry.label, "Move active takes to top");
    assert!(menu_entry.enabled);
    assert_eq!(
        commands::find(&app, "move active take to top"),
        Some(CommandId::MoveActiveTakesToTop)
    );

    let _ = app.update(Message::ExecuteCommand(CommandId::MoveActiveTakesToTop));
    let item = &app.project.audio_items()[0];
    assert_eq!(item.take_count(), 2);
    assert_eq!(item.active_take_index(), 0);
    assert_eq!(item.media_ref(), "asset://take-b");
    assert_eq!(item.other_takes()[0].index(), 1);
    assert_eq!(item.other_takes()[0].media_ref(), "asset://take-a");

    let _ = app.update(Message::Undo);
    let item = &app.project.audio_items()[0];
    assert_eq!(item.active_take_index(), 1);
    assert_eq!(item.media_ref(), "asset://take-b");
    assert_eq!(item.other_takes()[0].index(), 0);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items()[0].active_take_index(), 0);
    assert_eq!(app.project.audio_items()[0].media_ref(), "asset://take-b");
    let history_cursor = app.project.history_cursor();
    let _ = app.update(Message::MoveSelectedItemActiveTakesToTop);
    assert_eq!(app.project.history_cursor(), history_cursor);
    assert_eq!(app.status, "Active take is already at the top");
}

#[test]
fn moving_multiple_selected_audio_items_to_top_is_one_undoable_transaction() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    for (media_ref, start_sample) in [
        ("asset://take-b-first", 0),
        ("asset://single-take", 48_000),
        ("asset://take-a-second", 96_000),
        ("asset://take-b-already-top", 144_000),
    ] {
        app.project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: media_ref.to_owned(),
                start_sample,
                source_offset_samples: 0,
                length_samples: 48_000,
            })
            .expect("audio item should be inserted");
    }
    let first_id = app.project.audio_items()[0].id();
    let single_id = app.project.audio_items()[1].id();
    let second_id = app.project.audio_items()[2].id();
    let already_top_id = app.project.audio_items()[3].id();
    for (item_id, media_ref, name, active_index) in [
        (first_id, "asset://take-a-first", "Take A", None),
        (second_id, "asset://take-b-second", "Take B", None),
        (
            already_top_id,
            "asset://take-a-already-top",
            "Take A",
            Some(0),
        ),
    ] {
        let mut take = app
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == item_id)
            .expect("target audio item should exist")
            .active_take_snapshot();
        take.media_ref = media_ref.to_owned();
        take.name = name.to_owned();
        app.project
            .apply(DawAction::PasteAudioItemAsTake { item_id, take })
            .expect("second take should be appended and active");
        if let Some(take_index) = active_index {
            app.project
                .apply(DawAction::SetAudioItemActiveTake {
                    item_id,
                    take_index,
                })
                .expect("already-top item should select its first take");
        }
    }
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![
        (first_id, track_id),
        (single_id, track_id),
        (second_id, track_id),
        (already_top_id, track_id),
    ]);
    let before = app.project.snapshot();
    let history_cursor = app.project.history_cursor();
    assert!(commands::is_enabled(&app, CommandId::MoveActiveTakesToTop));

    let _ = app.update(Message::ExecuteCommand(CommandId::MoveActiveTakesToTop));
    let moved_first = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == first_id)
        .unwrap();
    assert_eq!(moved_first.active_take_index(), 0);
    assert_eq!(moved_first.media_ref(), "asset://take-a-first");
    assert_eq!(moved_first.other_takes()[0].index(), 1);
    assert_eq!(
        moved_first.other_takes()[0].media_ref(),
        "asset://take-b-first"
    );
    let moved_second = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == second_id)
        .unwrap();
    assert_eq!(moved_second.active_take_index(), 0);
    assert_eq!(moved_second.media_ref(), "asset://take-b-second");
    assert_eq!(moved_second.other_takes()[0].index(), 1);
    assert_eq!(
        moved_second.other_takes()[0].media_ref(),
        "asset://take-a-second"
    );
    let single = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == single_id)
        .unwrap();
    assert_eq!(single.take_count(), 1);
    assert_eq!(single.media_ref(), "asset://single-take");
    let already_top = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == already_top_id)
        .unwrap();
    assert_eq!(already_top.active_take_index(), 0);
    assert_eq!(already_top.media_ref(), "asset://take-b-already-top");
    assert_eq!(app.project.history_cursor(), history_cursor + 1);
    assert_eq!(app.status, "Active takes moved to top");

    let moved = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), moved);
}

#[test]
fn moving_selected_audio_and_midi_takes_to_top_is_one_undoable_transaction() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://move-take-a".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .unwrap();
    let audio_id = app.project.audio_items()[0].id();
    let mut audio_take = app.project.audio_items()[0].active_take_snapshot();
    audio_take.media_ref = "asset://move-take-b".to_owned();
    audio_take.name = "Take B".to_owned();
    audio_take.source_offset_samples = 1_920;
    app.project
        .apply(DawAction::PasteAudioItemAsTake {
            item_id: audio_id,
            take: audio_take,
        })
        .unwrap();

    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 3_840,
        })
        .unwrap();
    let midi_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id: midi_id,
            notes: vec![MidiNoteData {
                pitch: 64,
                channel: 2,
                tick: 720,
                duration: 240,
                velocity: 92,
                off_velocity: 17,
            }],
        })
        .unwrap();
    let mut snapshot = app.project.snapshot();
    let item = &mut snapshot.midi_items[0];
    item.name = "Take C".to_owned();
    item.active_take_index = 2;
    item.source_offset_ticks = 375;
    item.other_takes = vec![
        MidiItemTakeSnapshot {
            index: 0,
            name: "Take A".to_owned(),
            source_offset_ticks: 125,
            loop_source: true,
            source_loop_length_ticks: 1_920,
            notes: vec![aaadaw_core::MidiNoteSnapshot {
                id: 2,
                data: MidiNoteData {
                    pitch: 60,
                    channel: 0,
                    tick: 120,
                    duration: 240,
                    velocity: 100,
                    off_velocity: 0,
                },
            }],
            controllers: Vec::new(),
            pitch_bends: Vec::new(),
            program_changes: Vec::new(),
            poly_aftertouches: Vec::new(),
            channel_pressures: Vec::new(),
            opaque_events: Vec::new(),
            event_order: Vec::new(),
        },
        MidiItemTakeSnapshot {
            index: 1,
            name: "Take B".to_owned(),
            source_offset_ticks: 250,
            loop_source: false,
            source_loop_length_ticks: 3_840,
            notes: vec![aaadaw_core::MidiNoteSnapshot {
                id: 3,
                data: MidiNoteData {
                    pitch: 67,
                    channel: 1,
                    tick: 480,
                    duration: 360,
                    velocity: 84,
                    off_velocity: 0,
                },
            }],
            controllers: Vec::new(),
            pitch_bends: vec![MidiPitchBendData {
                channel: 1,
                tick: 600,
                value: 9_000,
            }],
            program_changes: Vec::new(),
            poly_aftertouches: Vec::new(),
            channel_pressures: Vec::new(),
            opaque_events: Vec::new(),
            event_order: Vec::new(),
        },
    ];
    app.project = Project::from_snapshot(snapshot).unwrap();

    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 3_840,
            length_ticks: 3_840,
        })
        .unwrap();
    let already_top_midi_id = app.project.midi_items()[1].id();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id: already_top_midi_id,
            notes: vec![MidiNoteData {
                pitch: 72,
                channel: 3,
                tick: 960,
                duration: 360,
                velocity: 88,
                off_velocity: 0,
            }],
        })
        .unwrap();
    let mut snapshot = app.project.snapshot();
    let next_note_id = snapshot
        .midi_items
        .iter()
        .flat_map(|item| {
            item.notes
                .iter()
                .chain(item.other_takes.iter().flat_map(|take| take.notes.iter()))
                .map(|note| note.id)
        })
        .max()
        .unwrap()
        + 1;
    let item = snapshot
        .midi_items
        .iter_mut()
        .find(|item| item.id == already_top_midi_id.value())
        .unwrap();
    item.name = "Take B".to_owned();
    item.active_take_index = 0;
    item.source_offset_ticks = 250;
    item.other_takes = vec![MidiItemTakeSnapshot {
        index: 1,
        name: "Take A".to_owned(),
        source_offset_ticks: 125,
        loop_source: false,
        source_loop_length_ticks: 3_840,
        notes: vec![aaadaw_core::MidiNoteSnapshot {
            id: next_note_id,
            data: MidiNoteData {
                pitch: 67,
                channel: 1,
                tick: 480,
                duration: 240,
                velocity: 84,
                off_velocity: 0,
            },
        }],
        controllers: Vec::new(),
        pitch_bends: Vec::new(),
        program_changes: Vec::new(),
        poly_aftertouches: Vec::new(),
        channel_pressures: Vec::new(),
        opaque_events: Vec::new(),
        event_order: Vec::new(),
    }];
    app.project = Project::from_snapshot(snapshot).unwrap();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![
        (audio_id, track_id),
        (midi_id, track_id),
        (already_top_midi_id, track_id),
    ]);
    let before = app.project.snapshot();
    let history_cursor = app.project.history_cursor();
    assert!(commands::is_enabled(&app, CommandId::MoveActiveTakesToTop));

    let _ = app.update(Message::ExecuteCommand(CommandId::MoveActiveTakesToTop));
    assert_eq!(app.project.history_cursor(), history_cursor + 1);
    let audio = &app.project.audio_items()[0];
    assert_eq!(audio.active_take_index(), 0);
    assert_eq!(audio.media_ref(), "asset://move-take-b");
    assert_eq!(audio.other_takes()[0].index(), 1);
    assert_eq!(audio.other_takes()[0].media_ref(), "asset://move-take-a");
    let midi = &app.project.midi_items()[0];
    assert_eq!(midi.active_take_index(), 0);
    let takes = midi.take_snapshots();
    assert_eq!(
        takes
            .iter()
            .map(|take| take.name.as_str())
            .collect::<Vec<_>>(),
        ["Take C", "Take A", "Take B"]
    );
    assert_eq!(takes[0].source_offset_ticks, 375);
    assert_eq!(takes[1].source_offset_ticks, 125);
    assert!(takes[1].loop_source);
    assert_eq!(takes[2].pitch_bends[0].value, 9_000);
    let already_top = app
        .project
        .midi_items()
        .iter()
        .find(|item| item.id() == already_top_midi_id)
        .unwrap();
    assert_eq!(already_top.active_take_index(), 0);
    assert_eq!(
        already_top
            .take_snapshots()
            .iter()
            .map(|take| take.name.as_str())
            .collect::<Vec<_>>(),
        ["Take B", "Take A"]
    );
    let after = app.project.snapshot();

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after);
}

#[test]
fn cropping_audio_item_to_active_take_is_registered_and_undoable() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://take-a".to_owned(),
            start_sample: 12_000,
            source_offset_samples: 0,
            length_samples: 96_000,
        })
        .expect("first take item should be inserted");
    let item_id = app.project.audio_items()[0].id();
    let mut second_take = app.project.audio_items()[0].active_take_snapshot();
    second_take.media_ref = "asset://take-b".to_owned();
    second_take.name = "Take B".to_owned();
    second_take.source_offset_samples = 12_345;
    second_take.volume = 0.73;
    second_take.pan = -0.4;
    second_take.playback_rate = 0.8;
    app.project
        .apply(DawAction::PasteAudioItemAsTake {
            item_id,
            take: second_take,
        })
        .expect("second take should be appended and active");
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(item_id, track_id)]);

    let action = commands::shortcut_entries(&app)
        .into_iter()
        .find(|entry| entry.id == "take.crop-active")
        .expect("REAPER's Crop to active take action should be registered");
    assert!(action.default_binding.is_empty());
    let menu_entry = commands::for_menu(&app, MainMenu::Item)
        .into_iter()
        .find(|entry| entry.id == CommandId::CropAudioItemToActiveTake)
        .expect("Item menu should expose Crop to active take in items");
    assert_eq!(menu_entry.label, "Crop to active take in items");
    assert!(menu_entry.enabled);

    let before = app.project.snapshot();
    let _ = app.update(Message::ExecuteCommand(
        CommandId::CropAudioItemToActiveTake,
    ));
    let item = &app.project.audio_items()[0];
    assert_eq!(item.take_count(), 1);
    assert_eq!(item.active_take_index(), 0);
    assert_eq!(item.media_ref(), "asset://take-b");
    assert_eq!(item.name(), "Take B");
    let active_take = item.active_take_snapshot();
    assert_eq!(active_take.source_offset_samples, 12_345);
    assert_eq!(active_take.volume, 0.73);
    assert_eq!(active_take.pan, -0.4);
    assert_eq!(active_take.playback_rate, 0.8);
    assert_eq!(item.start_sample(), 12_000);
    assert_eq!(item.length_samples(), 96_000);
    assert!(!commands::is_enabled(
        &app,
        CommandId::CropAudioItemToActiveTake
    ));

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items()[0].take_count(), 1);
    assert_eq!(app.project.audio_items()[0].media_ref(), "asset://take-b");
}

#[test]
fn cropping_multiple_selected_audio_items_is_one_undoable_transaction() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    for (media_ref, start_sample) in [
        ("asset://take-b-first", 0),
        ("asset://take-b-second", 96_000),
        ("asset://single-take", 192_000),
    ] {
        app.project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: media_ref.to_owned(),
                start_sample,
                source_offset_samples: 0,
                length_samples: 48_000,
            })
            .expect("audio item should be inserted");
    }
    let first_id = app.project.audio_items()[0].id();
    let second_id = app.project.audio_items()[1].id();
    let single_id = app.project.audio_items()[2].id();
    for (item_id, name, make_base_take_active) in
        [(first_id, "Take A", false), (second_id, "Take A", true)]
    {
        let mut take = app
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == item_id)
            .expect("target audio item should exist")
            .active_take_snapshot();
        take.media_ref = format!("asset://{name}-{}", item_id.value());
        take.name = name.to_owned();
        app.project
            .apply(DawAction::PasteAudioItemAsTake { item_id, take })
            .expect("second take should be appended");
        if make_base_take_active {
            app.project
                .apply(DawAction::SetAudioItemActiveTake {
                    item_id,
                    take_index: 0,
                })
                .expect("base take should become active");
        }
    }
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![
        (first_id, track_id),
        (second_id, track_id),
        (single_id, track_id),
    ]);
    let before = app.project.snapshot();
    let history_cursor = app.project.history_cursor();
    assert!(commands::is_enabled(
        &app,
        CommandId::CropAudioItemToActiveTake
    ));

    let _ = app.update(Message::ExecuteCommand(
        CommandId::CropAudioItemToActiveTake,
    ));
    let items = app.project.audio_items();
    assert_eq!(
        items
            .iter()
            .find(|item| item.id() == first_id)
            .unwrap()
            .take_count(),
        1
    );
    assert_eq!(
        items
            .iter()
            .find(|item| item.id() == first_id)
            .unwrap()
            .active_take_index(),
        0
    );
    assert!(
        items
            .iter()
            .find(|item| item.id() == first_id)
            .unwrap()
            .media_ref()
            .starts_with("asset://Take A-")
    );
    assert_eq!(
        items
            .iter()
            .find(|item| item.id() == second_id)
            .unwrap()
            .take_count(),
        1
    );
    assert_eq!(
        items
            .iter()
            .find(|item| item.id() == second_id)
            .unwrap()
            .media_ref(),
        "asset://take-b-second"
    );
    assert_eq!(
        items
            .iter()
            .find(|item| item.id() == single_id)
            .unwrap()
            .take_count(),
        1
    );
    assert_eq!(app.project.history_cursor(), history_cursor + 1);

    let cropped = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), cropped);
}

#[test]
fn cropping_mixed_selected_audio_and_midi_takes_is_one_undoable_transaction() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://crop-take-a".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .unwrap();
    let audio_id = app.project.audio_items()[0].id();
    let mut audio_take = app.project.audio_items()[0].active_take_snapshot();
    audio_take.media_ref = "asset://crop-take-b".to_owned();
    audio_take.name = "Take B".to_owned();
    audio_take.source_offset_samples = 12_345;
    app.project
        .apply(DawAction::PasteAudioItemAsTake {
            item_id: audio_id,
            take: audio_take,
        })
        .unwrap();

    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 0,
            length_ticks: 3_840,
        })
        .unwrap();
    let midi_id = app.project.midi_items()[0].id();
    app.project
        .apply(DawAction::AddMidiNotes {
            item_id: midi_id,
            notes: vec![MidiNoteData {
                pitch: 72,
                channel: 3,
                tick: 720,
                duration: 360,
                velocity: 93,
                off_velocity: 11,
            }],
        })
        .unwrap();
    let mut snapshot = app.project.snapshot();
    let item = &mut snapshot.midi_items[0];
    item.name = "Take C".to_owned();
    item.active_take_index = 2;
    item.source_offset_ticks = 375;
    item.loop_source = true;
    item.source_loop_length_ticks = 1_920;
    item.other_takes = vec![
        MidiItemTakeSnapshot {
            index: 0,
            name: "Take A".to_owned(),
            source_offset_ticks: 125,
            loop_source: false,
            source_loop_length_ticks: 1_920,
            notes: Vec::new(),
            controllers: Vec::new(),
            pitch_bends: Vec::new(),
            program_changes: Vec::new(),
            poly_aftertouches: Vec::new(),
            channel_pressures: Vec::new(),
            opaque_events: Vec::new(),
            event_order: Vec::new(),
        },
        MidiItemTakeSnapshot {
            index: 1,
            name: "Take B".to_owned(),
            source_offset_ticks: 250,
            loop_source: false,
            source_loop_length_ticks: 3_840,
            notes: Vec::new(),
            controllers: Vec::new(),
            pitch_bends: vec![MidiPitchBendData {
                channel: 1,
                tick: 600,
                value: 9_000,
            }],
            program_changes: Vec::new(),
            poly_aftertouches: Vec::new(),
            channel_pressures: Vec::new(),
            opaque_events: Vec::new(),
            event_order: Vec::new(),
        },
    ];
    app.project = Project::from_snapshot(snapshot).unwrap();
    app.project
        .apply(DawAction::InsertMidiItem {
            track_id,
            start_tick: 3_840,
            length_ticks: 1_920,
        })
        .unwrap();
    let single_midi_id = app.project.midi_items()[1].id();
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![
        (audio_id, track_id),
        (midi_id, track_id),
        (single_midi_id, track_id),
    ]);

    let before = app.project.snapshot();
    let history_cursor = app.project.history_cursor();
    assert!(commands::is_enabled(
        &app,
        CommandId::CropAudioItemToActiveTake
    ));
    let _ = app.update(Message::ExecuteCommand(
        CommandId::CropAudioItemToActiveTake,
    ));

    let audio = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == audio_id)
        .unwrap();
    assert_eq!(audio.take_count(), 1);
    assert_eq!(audio.active_take_index(), 0);
    assert_eq!(audio.media_ref(), "asset://crop-take-b");
    assert_eq!(audio.active_take_snapshot().source_offset_samples, 12_345);
    let midi = app
        .project
        .midi_items()
        .iter()
        .find(|item| item.id() == midi_id)
        .unwrap();
    assert_eq!(midi.take_count(), 1);
    assert_eq!(midi.active_take_index(), 0);
    assert_eq!(midi.name(), "Take C");
    assert_eq!(midi.source_offset_ticks(), 375);
    assert!(midi.is_loop_source());
    assert_eq!(midi.notes()[0].pitch(), 72);
    assert_eq!(midi.pitch_bends().len(), 0);
    let single_midi = app
        .project
        .midi_items()
        .iter()
        .find(|item| item.id() == single_midi_id)
        .unwrap();
    assert_eq!(single_midi.take_count(), 1);
    assert_eq!(app.project.history_cursor(), history_cursor + 1);

    let after = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after);
}

#[test]
fn copied_audio_take_pastes_into_selected_item_without_removing_source_item() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    for (media_ref, start_sample) in [("asset://copied-take", 0), ("asset://target", 96_000)] {
        app.project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: media_ref.to_owned(),
                start_sample,
                source_offset_samples: 0,
                length_samples: 48_000,
            })
            .expect("test audio item should be inserted");
    }
    let source_item_id = app.project.audio_items()[0].id();
    let target_item_id = app.project.audio_items()[1].id();
    app.timeline.rebuild(&app.project);
    let shortcuts = commands::shortcut_entries(&app);
    let copy_action = shortcuts
        .iter()
        .find(|entry| entry.id == "edit.copy-items")
        .expect("REAPER's Edit: Copy items/tracks action should be registered");
    assert_eq!(copy_action.default_binding, "Ctrl+C");
    let paste_as_takes = shortcuts
        .iter()
        .find(|entry| entry.id == "take.paste-as-takes")
        .expect("REAPER's Take: Paste as takes in items action should be registered");
    assert!(paste_as_takes.default_binding.is_empty());
    let item_menu_entry = commands::for_menu(&app, MainMenu::Item)
        .into_iter()
        .find(|entry| entry.id == CommandId::PasteAudioItemAsTake)
        .expect("Item menu should expose Paste as takes in items");
    assert_eq!(item_menu_entry.label, "Paste as takes in items");
    assert!(!item_menu_entry.enabled);
    app.timeline.select_items(vec![(source_item_id, track_id)]);

    assert!(commands::is_enabled(&app, CommandId::CopyItems));
    let _ = app.update(Message::ExecuteCommand(CommandId::CopyItems));
    assert_eq!(app.project.audio_items().len(), 2);
    app.timeline.select_items(vec![(target_item_id, track_id)]);
    assert!(commands::is_enabled(&app, CommandId::PasteAudioItemAsTake));
    assert_eq!(
        commands::find(&app, "paste as takes in items"),
        Some(CommandId::PasteAudioItemAsTake)
    );
    let _ = app.update(Message::ExecuteCommand(CommandId::PasteAudioItemAsTake));

    let target = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == target_item_id)
        .unwrap();
    assert_eq!(target.take_count(), 2);
    assert_eq!(target.active_take_index(), 1);
    assert_eq!(target.media_ref(), "asset://copied-take");
    assert_eq!(
        app.project.audio_items()[0].media_ref(),
        "asset://copied-take"
    );
    assert_eq!(app.project.audio_items().len(), 2);

    let _ = app.update(Message::Undo);
    let target = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == target_item_id)
        .unwrap();
    assert_eq!(target.take_count(), 1);
    assert_eq!(target.media_ref(), "asset://target");
    let _ = app.update(Message::Redo);
    let target = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == target_item_id)
        .unwrap();
    assert_eq!(target.take_count(), 2);
    assert_eq!(target.active_take_index(), 1);
    assert_eq!(target.media_ref(), "asset://copied-take");
}

#[test]
fn copied_audio_take_pastes_into_multiple_selected_items_as_one_transaction() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    for (media_ref, start_sample) in [
        ("asset://take-a", 0),
        ("asset://take-a-single", 96_000),
        ("asset://take-b", 192_000),
        ("asset://clipboard-b", 288_000),
    ] {
        app.project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: media_ref.to_owned(),
                start_sample,
                source_offset_samples: 0,
                length_samples: 48_000,
            })
            .expect("audio item should be inserted");
    }
    let first_id = app.project.audio_items()[0].id();
    let single_id = app.project.audio_items()[1].id();
    let last_id = app.project.audio_items()[2].id();
    let source_id = app.project.audio_items()[3].id();

    let mut first_alternate = app.project.audio_items()[0].active_take_snapshot();
    first_alternate.media_ref = "asset://take-b".to_owned();
    first_alternate.name = "Take B".to_owned();
    app.project
        .apply(DawAction::PasteAudioItemAsTake {
            item_id: first_id,
            take: first_alternate,
        })
        .expect("first target should receive its alternate take");
    app.project
        .apply(DawAction::MoveAudioItemActiveTakeToTop { item_id: first_id })
        .expect("Take B should become active at index zero");

    let mut last_alternate = app.project.audio_items()[2].active_take_snapshot();
    last_alternate.media_ref = "asset://take-a".to_owned();
    last_alternate.name = "Take A".to_owned();
    app.project
        .apply(DawAction::PasteAudioItemAsTake {
            item_id: last_id,
            take: last_alternate,
        })
        .expect("last target should receive Take A at index one");

    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(source_id, track_id)]);
    assert!(commands::is_enabled(&app, CommandId::CopyItems));
    let _ = app.update(Message::ExecuteCommand(CommandId::CopyItems));
    let history_cursor = app.project.history_cursor();
    app.timeline.select_items(vec![
        (last_id, track_id),
        (first_id, track_id),
        (single_id, track_id),
    ]);
    assert!(commands::is_enabled(&app, CommandId::PasteAudioItemAsTake));
    let menu_entry = commands::for_menu(&app, MainMenu::Item)
        .into_iter()
        .find(|entry| entry.id == CommandId::PasteAudioItemAsTake)
        .expect("Item menu should expose multi-selection Paste as takes");
    assert!(menu_entry.enabled);

    let before = app.project.snapshot();
    let _ = app.update(Message::ExecuteCommand(CommandId::PasteAudioItemAsTake));
    assert_eq!(app.project.history_cursor(), history_cursor + 1);
    assert_eq!(app.status, "Take pasted into selected audio items");
    assert_eq!(app.project.audio_items().len(), 4);

    let first = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == first_id)
        .unwrap();
    assert_eq!(first.take_count(), 3);
    assert_eq!(first.active_take_index(), 2);
    assert_eq!(first.media_ref(), "asset://clipboard-b");
    assert_eq!(
        first
            .other_takes()
            .iter()
            .map(|take| (take.index(), take.media_ref()))
            .collect::<Vec<_>>(),
        [(0, "asset://take-b"), (1, "asset://take-a")]
    );
    let single = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == single_id)
        .unwrap();
    assert_eq!(single.take_count(), 2);
    assert_eq!(single.active_take_index(), 1);
    assert_eq!(single.media_ref(), "asset://clipboard-b");
    let last = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == last_id)
        .unwrap();
    assert_eq!(last.take_count(), 3);
    assert_eq!(last.active_take_index(), 2);
    assert_eq!(last.media_ref(), "asset://clipboard-b");
    assert_eq!(
        last.other_takes()
            .iter()
            .map(|take| (take.index(), take.media_ref()))
            .collect::<Vec<_>>(),
        [(0, "asset://take-b"), (1, "asset://take-a")]
    );
    let source = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == source_id)
        .unwrap();
    assert_eq!(source.take_count(), 1);
    assert_eq!(source.media_ref(), "asset://clipboard-b");

    let after = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after);
}

#[test]
fn multiple_copied_audio_takes_paste_in_order_to_each_selected_item() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    for (media_ref, start_sample) in [
        ("asset://target-a", 0),
        ("asset://target-b", 96_000),
        ("asset://source-a", 192_000),
        ("asset://source-b", 288_000),
    ] {
        app.project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: media_ref.to_owned(),
                start_sample,
                source_offset_samples: 0,
                length_samples: 48_000,
            })
            .expect("audio item should be inserted");
    }
    let target_ids = [
        app.project.audio_items()[0].id(),
        app.project.audio_items()[1].id(),
    ];
    let source_ids = [
        app.project.audio_items()[2].id(),
        app.project.audio_items()[3].id(),
    ];

    app.timeline
        .select_items(vec![(source_ids[1], track_id), (source_ids[0], track_id)]);
    assert!(commands::is_enabled(&app, CommandId::CopyItems));
    let _ = app.update(Message::ExecuteCommand(CommandId::CopyItems));
    let clipboard = app
        .audio_take_clipboard
        .as_ref()
        .expect("copy should populate the active-take clipboard");
    assert_eq!(clipboard.takes.len(), 2);
    assert_eq!(clipboard.takes[0].media_ref, "asset://source-a");
    assert_eq!(clipboard.takes[1].media_ref, "asset://source-b");

    app.timeline
        .select_items(vec![(target_ids[1], track_id), (target_ids[0], track_id)]);
    assert!(commands::is_enabled(&app, CommandId::PasteAudioItemAsTake));
    let history_cursor = app.project.history_cursor();
    let before = app.project.snapshot();
    let _ = app.update(Message::ExecuteCommand(CommandId::PasteAudioItemAsTake));
    assert_eq!(app.project.history_cursor(), history_cursor + 1);
    assert_eq!(app.status, "Takes pasted into selected audio items");
    assert_eq!(app.project.audio_items().len(), 4);

    for (target_id, original_source) in [
        (target_ids[0], "asset://target-a"),
        (target_ids[1], "asset://target-b"),
    ] {
        let target = app
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == target_id)
            .expect("target should remain in the project");
        assert_eq!(target.take_count(), 3);
        assert_eq!(target.active_take_index(), 2);
        assert_eq!(target.media_ref(), "asset://source-b");
        assert_eq!(
            target
                .other_takes()
                .iter()
                .map(|take| take.media_ref())
                .collect::<Vec<_>>(),
            [original_source, "asset://source-a"]
        );
    }

    let after = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after);

    app.timeline.select_items(vec![(target_ids[0], track_id)]);
    let single_target_before = app.project.history_cursor();
    let _ = app.update(Message::ExecuteCommand(CommandId::PasteAudioItemAsTake));
    assert_eq!(app.project.history_cursor(), single_target_before + 1);
    assert_eq!(app.status, "Takes pasted into audio item");
    let target = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == target_ids[0])
        .expect("single selected target should remain in the project");
    assert_eq!(target.take_count(), 5);
    assert_eq!(target.active_take_index(), 4);
    assert_eq!(target.media_ref(), "asset://source-b");
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), after);
}

#[test]
fn delete_active_take_action_keeps_the_other_take_and_undo_redo_restore_it() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.project
        .apply(DawAction::InsertAudioItem {
            track_id,
            media_ref: "asset://take-a".to_owned(),
            start_sample: 0,
            source_offset_samples: 0,
            length_samples: 48_000,
        })
        .expect("test audio item should be inserted");
    let item_id = app.project.audio_items()[0].id();
    let mut second_take = app.project.audio_items()[0].active_take_snapshot();
    second_take.media_ref = "asset://take-b".to_owned();
    second_take.name = "Take B".to_owned();
    app.project
        .apply(DawAction::PasteAudioItemAsTake {
            item_id,
            take: second_take,
        })
        .expect("second take should be appended");
    app.project
        .apply(DawAction::SetAudioItemActiveTake {
            item_id,
            take_index: 0,
        })
        .expect("first take should be active before deletion");
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![(item_id, track_id)]);

    let shortcut = commands::shortcut_entries(&app)
        .into_iter()
        .find(|entry| entry.id == "take.delete-active")
        .expect("REAPER's Delete active take action should be registered");
    assert!(shortcut.default_binding.is_empty());
    let menu_entry = commands::for_menu(&app, MainMenu::Item)
        .into_iter()
        .find(|entry| entry.id == CommandId::DeleteActiveTake)
        .expect("Item menu should expose Delete active take from items");
    assert_eq!(menu_entry.label, "Delete active take from items");
    assert!(menu_entry.enabled);
    assert_eq!(
        commands::find(&app, "delete active take from items"),
        Some(CommandId::DeleteActiveTake)
    );

    let _ = app.update(Message::ExecuteCommand(CommandId::DeleteActiveTake));
    let item = &app.project.audio_items()[0];
    assert_eq!(item.take_count(), 1);
    assert_eq!(item.active_take_index(), 0);
    assert_eq!(item.media_ref(), "asset://take-b");
    assert_eq!(item.name(), "Take B");

    let _ = app.update(Message::Undo);
    let item = &app.project.audio_items()[0];
    assert_eq!(item.take_count(), 2);
    assert_eq!(item.active_take_index(), 0);
    assert_eq!(item.media_ref(), "asset://take-a");
    let _ = app.update(Message::Redo);
    let item = &app.project.audio_items()[0];
    assert_eq!(item.take_count(), 1);
    assert_eq!(item.media_ref(), "asset://take-b");
}

#[test]
fn deleting_active_takes_from_multiple_selected_audio_items_is_one_undoable_transaction() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    for start_sample in [0, 48_000, 96_000] {
        app.project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: "asset://take-a".to_owned(),
                start_sample,
                source_offset_samples: 0,
                length_samples: 48_000,
            })
            .expect("audio item should be inserted");
    }
    let first_id = app.project.audio_items()[0].id();
    let last_id = app.project.audio_items()[1].id();
    let single_id = app.project.audio_items()[2].id();
    for (item_id, media_ref, take_index) in [
        (first_id, "asset://take-b", 0),
        (last_id, "asset://take-b", 1),
    ] {
        let mut take = app
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == item_id)
            .expect("multi-take target should exist")
            .active_take_snapshot();
        take.media_ref = media_ref.to_owned();
        take.name = "Take B".to_owned();
        app.project
            .apply(DawAction::PasteAudioItemAsTake { item_id, take })
            .expect("second take should be appended");
        app.project
            .apply(DawAction::SetAudioItemActiveTake {
                item_id,
                take_index,
            })
            .expect("target active take should be selected");
    }
    app.timeline.rebuild(&app.project);
    app.timeline.select_items(vec![
        (first_id, track_id),
        (last_id, track_id),
        (single_id, track_id),
    ]);
    let before = app.project.snapshot();
    let history_cursor = app.project.history_cursor();
    assert!(commands::is_enabled(&app, CommandId::DeleteActiveTake));

    let _ = app.update(Message::ExecuteCommand(CommandId::DeleteActiveTake));
    assert_eq!(app.project.history_cursor(), history_cursor + 1);
    assert_eq!(app.status, "Selected active takes deleted");
    assert_eq!(app.project.audio_items().len(), 2);
    let remaining_first = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == first_id)
        .expect("first multi-take item should remain");
    assert_eq!(remaining_first.take_count(), 1);
    assert_eq!(remaining_first.active_take_index(), 0);
    assert_eq!(remaining_first.media_ref(), "asset://take-b");
    let remaining_last = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == last_id)
        .expect("last multi-take item should remain");
    assert_eq!(remaining_last.take_count(), 1);
    assert_eq!(remaining_last.active_take_index(), 0);
    assert_eq!(remaining_last.media_ref(), "asset://take-a");
    assert!(
        app.project
            .audio_items()
            .iter()
            .all(|item| item.id() != single_id)
    );

    let after = app.project.snapshot();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.snapshot(), after);
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
    let mut app = App {
        playback_busy: true,
        ..App::default()
    };

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
                channel: 0,
                pitch: 60,
                tick: 960,
                duration: 960,
                velocity: 96,
                off_velocity: 0,
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
    let expected_midi_tick = app
        .project
        .tick_at_sample_position(nudge_samples as f64)
        .unwrap();
    let expected_midi_position = MidiItemPosition::from_total_subticks(
        (expected_midi_tick * f64::from(aaadaw_core::MIDI_ITEM_SUBTICKS_PER_TICK)).round() as u128,
    )
    .unwrap();

    let _ = app.update(Message::NudgeSelectedItems(1));

    assert_eq!(app.project.audio_items()[0].start_sample(), nudge_samples);
    assert_eq!(
        app.project.audio_items()[0].source_offset_samples(),
        audio_source_offset
    );
    assert_eq!(app.project.audio_items()[0].length_samples(), audio_length);
    assert_eq!(
        app.project.midi_items()[0].start_position(),
        expected_midi_position
    );
    assert_eq!(app.project.midi_items()[0].length_ticks(), midi_length);
    assert_eq!(app.revision, original_revision + 1);
    assert_eq!(app.timeline.selected_items.len(), 2);
    assert!(app.timeline.selected_items.contains(&audio_id));
    assert!(app.timeline.selected_items.contains(&midi_id));

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].start_sample(), 0);
    assert_eq!(
        app.project.midi_items()[0].start_position(),
        MidiItemPosition::new(0, 0).unwrap()
    );
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items()[0].start_sample(), nudge_samples);
    assert_eq!(
        app.project.midi_items()[0].start_position(),
        expected_midi_position
    );

    let _ = app.update(Message::NudgeSelectedItems(-1));
    assert_eq!(app.project.audio_items()[0].start_sample(), 0);
    assert_eq!(
        app.project.midi_items()[0].start_position(),
        MidiItemPosition::new(0, 0).unwrap()
    );
}

#[test]
fn item_nudge_keyboard_selectors_expose_only_native_mode_options() {
    use super::item_nudge::{Mode, NoteValue, Target, Unit};

    let app = App::default();
    let dialog = &app.item_nudge_dialog;

    assert_eq!(dialog.mode_options.options(), &Mode::ALL);
    assert_eq!(dialog.target_options.options(), &Target::ALL);
    assert_eq!(dialog.nudge_unit_options.options(), &Unit::NUDGE_OPTIONS);
    assert_eq!(dialog.set_unit_options.options(), &Unit::SET_OPTIONS);
    assert_eq!(
        dialog.note_value_options.options(),
        &NoteValue::ALL,
        "the keyboard list must retain every supported REAPER note value"
    );
}

#[test]
fn item_nudge_combo_consumes_escape_before_closing_the_dialog() {
    let window_id = iced::window::Id::unique();
    let mut app = App {
        item_nudge_window_id: Some(window_id),
        ..App::default()
    };
    let escape = || {
        test_input_event(test_shortcut_input(
            Key::Named(iced::keyboard::key::Named::Escape),
            Modifiers::NONE,
        ))
    };

    let _ = app.update(Message::RuntimeKeyboardEvent(
        escape(),
        iced::event::Status::Captured,
        window_id,
    ));
    assert_eq!(app.item_nudge_window_id, Some(window_id));

    let _ = app.update(Message::RuntimeKeyboardEvent(
        escape(),
        iced::event::Status::Ignored,
        window_id,
    ));
    assert_eq!(app.item_nudge_window_id, None);
}

#[test]
fn item_nudge_item_units_use_the_first_item_length_and_selected_item_envelope() {
    use super::item_nudge::Unit;

    let mut app = item_snap_test_app(48_000, 7_680);
    let audio_start = app.project.audio_items()[0].start_sample();
    let audio_length = app.project.audio_items()[0].length_samples();
    let midi_start_tick = app.project.midi_items()[0].start_tick();
    let midi_start = app.project.sample_at_tick(midi_start_tick).unwrap();
    let selection = app.timeline.selected_items.clone();

    let _ = app.update(Message::ItemNudgeUnitChanged(Unit::ItemLengths));
    let _ = app.update(Message::ItemNudgeValueChanged("1".to_owned()));
    let _ = app.update(Message::NudgeSelectedItems(1));

    let length_delta = i128::from(audio_length);
    assert_eq!(
        app.project.audio_items()[0].start_sample(),
        (i128::from(audio_start) + length_delta) as u64
    );
    assert_eq!(
        app.project.midi_items()[0].start_tick(),
        app.project
            .tick_at_sample((i128::from(midi_start) + length_delta) as u64)
            .unwrap()
    );
    assert_eq!(app.timeline.selected_items, selection);
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].start_sample(), audio_start);
    assert_eq!(app.project.midi_items()[0].start_tick(), midi_start_tick);

    let midi_end = app
        .project
        .sample_at_tick(midi_start_tick + app.project.midi_items()[0].length_ticks())
        .unwrap();
    let selection_delta = i128::from(midi_end - audio_start);
    let _ = app.update(Message::ItemNudgeUnitChanged(Unit::ItemSelections));
    let _ = app.update(Message::ItemNudgeValueChanged("1".to_owned()));
    let _ = app.update(Message::NudgeSelectedItems(1));

    assert_eq!(
        app.project.audio_items()[0].start_sample(),
        (i128::from(audio_start) + selection_delta) as u64
    );
    assert_eq!(
        app.project.midi_items()[0].start_tick(),
        app.project
            .tick_at_sample((i128::from(midi_start) + selection_delta) as u64)
            .unwrap()
    );
    assert_eq!(app.timeline.selected_items, selection);
    assert!(app.item_nudge_dialog.error.is_none());
    let moved_audio = app.project.audio_items()[0].start_sample();
    let moved_midi = app.project.midi_items()[0].start_tick();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].start_sample(), audio_start);
    assert_eq!(app.project.midi_items()[0].start_tick(), midi_start_tick);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items()[0].start_sample(), moved_audio);
    assert_eq!(app.project.midi_items()[0].start_tick(), moved_midi);
}

#[test]
fn item_nudge_note_values_move_items_by_tempo_mapped_note_lengths() {
    use super::item_nudge::{NoteValue, Unit};

    let mut app = item_snap_test_app(48_000, 3_840);
    let audio_start = app.project.audio_items()[0].start_sample();
    let audio_length = app.project.audio_items()[0].length_samples();
    let midi_start_tick = app.project.midi_items()[0].start_tick();
    let midi_start = app.project.sample_at_tick(midi_start_tick).unwrap();
    let selection = app.timeline.selected_items.clone();

    let _ = app.update(Message::ItemNudgeUnitChanged(Unit::Notes));
    let _ = app.update(Message::ItemNudgeValueChanged("1".to_owned()));
    let _ = app.update(Message::NudgeSelectedItems(1));

    let quarter_note_delta = app
        .project
        .sample_at_tick(midi_start_tick + u64::from(app.project.settings().ppq()))
        .unwrap()
        - midi_start;
    assert_eq!(
        app.project.audio_items()[0].start_sample(),
        audio_start + quarter_note_delta
    );
    assert_eq!(
        app.project.midi_items()[0].start_tick(),
        midi_start_tick + u64::from(app.project.settings().ppq())
    );
    assert_eq!(app.project.audio_items()[0].length_samples(), audio_length);
    assert_eq!(app.timeline.selected_items, selection);
    assert!(app.item_nudge_dialog.error.is_none());

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].start_sample(), audio_start);
    assert_eq!(app.project.midi_items()[0].start_tick(), midi_start_tick);
    let _ = app.update(Message::Redo);
    assert_eq!(
        app.project.audio_items()[0].start_sample(),
        audio_start + quarter_note_delta
    );
    assert_eq!(
        app.project.midi_items()[0].start_tick(),
        midi_start_tick + u64::from(app.project.settings().ppq())
    );
    let _ = app.update(Message::Undo);

    let _ = app.update(Message::ItemNudgeNoteValueChanged(
        NoteValue::QuarterTriplet,
    ));
    let _ = app.update(Message::NudgeSelectedItems(1));
    let expected_delta = app
        .project
        .sample_at_tick_position(
            app.project
                .tick_at_sample_position(midi_start as f64)
                .unwrap()
                + f64::from(app.project.settings().ppq()) * (2.0 / 3.0),
        )
        .unwrap()
        .round() as u64
        - midi_start;
    assert_eq!(
        app.project.audio_items()[0].start_sample(),
        audio_start + expected_delta
    );
    assert_eq!(app.timeline.selected_items, selection);
    assert!(app.item_nudge_dialog.error.is_none());
    let triplet_audio = app.project.audio_items()[0].start_sample();
    let triplet_midi = app.project.midi_items()[0].start_tick();
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].start_sample(), audio_start);
    assert_eq!(app.project.midi_items()[0].start_tick(), midi_start_tick);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items()[0].start_sample(), triplet_audio);
    assert_eq!(app.project.midi_items()[0].start_tick(), triplet_midi);
}

#[test]
fn item_nudge_note_values_use_the_tempo_at_the_selected_item_position() {
    use super::item_nudge::Unit;

    let mut app = item_snap_test_app(48_000, 1_920);
    app.project
        .apply(DawAction::SetTempo {
            start_tick: 1_920,
            bpm: 60.0,
        })
        .unwrap();
    app.timeline.rebuild(&app.project);
    let audio_start = app.project.audio_items()[0].start_sample();
    let midi_start_tick = app.project.midi_items()[0].start_tick();
    let selected = app.timeline.selected_items.clone();
    let quarter_later_sample = app.project.sample_at_tick(2_880).unwrap();
    assert_eq!(quarter_later_sample - audio_start, 48_000);

    let _ = app.update(Message::ItemNudgeUnitChanged(Unit::Notes));
    let _ = app.update(Message::ItemNudgeValueChanged("1".to_owned()));
    let _ = app.update(Message::NudgeSelectedItems(1));

    assert_eq!(
        app.project.audio_items()[0].start_sample(),
        quarter_later_sample
    );
    assert_eq!(app.project.midi_items()[0].start_tick(), 2_880);
    assert_eq!(app.timeline.selected_items, selected);
    assert!(app.item_nudge_dialog.error.is_none());
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].start_sample(), audio_start);
    assert_eq!(app.project.midi_items()[0].start_tick(), midi_start_tick);
    let _ = app.update(Message::Redo);
    assert_eq!(
        app.project.audio_items()[0].start_sample(),
        quarter_later_sample
    );
    assert_eq!(app.project.midi_items()[0].start_tick(), 2_880);
}

#[test]
fn item_nudge_note_snap_uses_the_selected_note_value_grid() {
    use super::item_nudge::{NoteValue, Unit};

    let mut app = item_snap_test_app(60_960, 3_840);
    let audio_id = app.project.audio_items()[0].id();
    let audio_track = app.project.tracks()[0].id();
    app.timeline.select_items(vec![(audio_id, audio_track)]);
    let audio_start = app.project.audio_items()[0].start_sample();
    let audio_length = app.project.audio_items()[0].length_samples();
    let selection = app.timeline.selected_items.clone();

    let _ = app.update(Message::ItemNudgeUnitChanged(Unit::Notes));
    let _ = app.update(Message::ItemNudgeNoteValueChanged(
        NoteValue::QuarterTriplet,
    ));
    let _ = app.update(Message::ItemNudgeSnapToUnitChanged(true));
    let _ = app.update(Message::NudgeSelectedItems(1));

    let snapped_sample = app.project.sample_at_tick(3_200).unwrap();
    assert_eq!(snapped_sample, 80_000);
    assert_eq!(app.project.audio_items()[0].start_sample(), snapped_sample);
    assert_eq!(app.project.audio_items()[0].length_samples(), audio_length);
    assert_eq!(app.timeline.selected_items, selection);
    assert!(app.item_nudge_dialog.error.is_none());
    let moved_sample = app.project.audio_items()[0].start_sample();

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.audio_items()[0].start_sample(), audio_start);
    let _ = app.update(Message::Redo);
    assert_eq!(app.project.audio_items()[0].start_sample(), moved_sample);
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
    assert_eq!(fractional.item_nudge_dialog.unit, Unit::Frames);
    let _ = fractional.update(Message::ItemNudgeUnitChanged(Unit::Frames));
    assert_eq!(fractional.item_nudge_dialog.unit, Unit::Frames);
}

#[test]
fn item_nudge_pixels_follow_arrangement_zoom_and_native_snap_rounding() {
    use super::item_nudge::Unit;

    let cases = [
        (0.25, "1.25", false, 1_i8),
        (0.5, "1.25", true, 1_i8),
        (0.5, "0.5", true, -1_i8),
    ];
    for (pixels_per_tick, value, snap, direction) in cases {
        let mut app = item_snap_test_app(60_960, 5_242);
        app.timeline.origin_tick = 0;
        app.timeline.pixels_per_tick = pixels_per_tick;
        let item = &app.project.audio_items()[0];
        let track_id = item.track_id();
        let first_sample = item.start_sample();
        let original_length = item.length_samples();
        let original_offset = item.source_offset_samples();
        let original_selection = app.timeline.selected_items.clone();
        let first_tick = app.project.tick_at_sample(first_sample).unwrap();
        let first_pixel = first_tick as f64 * f64::from(pixels_per_tick);
        let target_pixel = first_pixel + f64::from(direction) * value.parse::<f64>().unwrap();
        let revision_before = app.revision;

        let _ = app.update(Message::ItemNudgeUnitChanged(Unit::Pixels));
        assert_eq!(app.item_nudge_dialog.value, "0.5");
        let _ = app.update(Message::ItemNudgeValueChanged(value.to_owned()));
        let _ = app.update(Message::ItemNudgeSnapToUnitChanged(snap));
        let _ = app.update(Message::NudgeSelectedItems(direction));

        let moved = &app.project.audio_items()[0];
        let moved_tick = app.project.tick_at_sample(moved.start_sample()).unwrap();
        let moved_pixel = moved_tick as f64 * f64::from(pixels_per_tick);
        let expected_pixel = if snap {
            target_pixel.floor()
        } else {
            target_pixel
        };
        assert!(
            (moved_pixel - expected_pixel).abs() <= f64::from(pixels_per_tick) * 0.51,
            "{pixels_per_tick}px/tick, {value}px, snap={snap}, moved={moved_pixel}, expected={expected_pixel}"
        );
        assert_ne!(moved.start_sample(), first_sample);
        assert_eq!(moved.length_samples(), original_length);
        assert_eq!(moved.source_offset_samples(), original_offset);
        assert_eq!(moved.track_id(), track_id);
        assert_eq!(app.timeline.selected_items, original_selection);
        assert_eq!(app.revision, revision_before + 1);
        assert!(app.item_nudge_dialog.error.is_none());
        let moved_sample = moved.start_sample();

        let _ = app.update(Message::Undo);
        assert_eq!(app.project.audio_items()[0].start_sample(), first_sample);
        assert_eq!(app.timeline.selected_items, original_selection);
        let _ = app.update(Message::Redo);
        assert_eq!(app.project.audio_items()[0].start_sample(), moved_sample);
        assert_eq!(app.project.audio_items()[0].track_id(), track_id);
        assert_eq!(app.timeline.selected_items, original_selection);
    }
}

#[test]
fn item_nudge_set_pixels_use_absolute_screen_positions_and_snap_to_pixel() {
    use super::item_nudge::{Mode, Unit};

    for (snap, expected_pixel) in [(false, 200.5), (true, 201.0)] {
        let mut app = item_snap_test_app(60_960, 5_242);
        app.timeline.origin_tick = 0;
        app.timeline.pixels_per_tick = 0.5;
        let item = &app.project.audio_items()[0];
        let track_id = item.track_id();
        let original_sample = item.start_sample();
        let original_length = item.length_samples();
        let original_offset = item.source_offset_samples();
        let selection = app.timeline.selected_items.clone();
        let revision_before = app.revision;

        let _ = app.update(Message::ItemNudgeModeChanged(Mode::Set));
        let _ = app.update(Message::ItemNudgeUnitChanged(Unit::Pixels));
        let _ = app.update(Message::ItemNudgeValueChanged("200.5".to_owned()));
        let _ = app.update(Message::ItemNudgeSnapToUnitChanged(snap));
        let _ = app.update(Message::ApplyItemNudgeSet);

        let moved = &app.project.audio_items()[0];
        let moved_tick = app.project.tick_at_sample(moved.start_sample()).unwrap();
        let moved_pixel = moved_tick as f64 * f64::from(app.timeline.pixels_per_tick);
        assert!((moved_pixel - expected_pixel).abs() <= 0.26);
        assert_ne!(moved.start_sample(), original_sample);
        assert_eq!(moved.length_samples(), original_length);
        assert_eq!(moved.source_offset_samples(), original_offset);
        assert_eq!(app.timeline.selected_items, selection);
        assert_eq!(app.revision, revision_before + 1);
        assert!(app.item_nudge_dialog.error.is_none());

        let moved_sample = moved.start_sample();
        let _ = app.update(Message::Undo);
        assert_eq!(app.project.audio_items()[0].start_sample(), original_sample);
        assert_eq!(app.timeline.selected_items, selection);
        let _ = app.update(Message::Redo);
        assert_eq!(app.project.audio_items()[0].start_sample(), moved_sample);
        assert_eq!(app.timeline.selected_items, selection);
        assert_eq!(app.project.audio_items()[0].track_id(), track_id);
    }
}

#[test]
fn item_nudge_set_frames_and_measures_beats_match_native_positions_and_snap() {
    use super::item_nudge::{Mode, Unit};

    let cases = [
        (Unit::Frames, "60", false, 96_000),
        (Unit::Frames, "60.5", true, 97_600),
        (Unit::MeasuresBeats, "3.1.25", false, 198_000),
        (Unit::MeasuresBeats, "3.1.75", true, 216_000),
    ];
    for (unit, value, snap, expected_sample) in cases {
        let mut app = item_snap_test_app(44_100, 5_242);
        app.timeline.snap_grid = crate::timeline::SnapGrid::Quarter;
        let item = &app.project.audio_items()[0];
        let track_id = item.track_id();
        let original_length = item.length_samples();
        let original_offset = item.source_offset_samples();
        let selection = app.timeline.selected_items.clone();

        let _ = app.update(Message::ItemNudgeModeChanged(Mode::Set));
        let _ = app.update(Message::ItemNudgeUnitChanged(unit));
        let _ = app.update(Message::ItemNudgeValueChanged(value.to_owned()));
        let _ = app.update(Message::ItemNudgeSnapToUnitChanged(snap));
        let _ = app.update(Message::ApplyItemNudgeSet);

        let moved = &app.project.audio_items()[0];
        assert_eq!(moved.start_sample(), expected_sample, "{unit}, {value}");
        assert_eq!(moved.length_samples(), original_length);
        assert_eq!(moved.source_offset_samples(), original_offset);
        assert_eq!(app.timeline.selected_items, selection);
        assert!(app.item_nudge_dialog.error.is_none(), "{unit}, {value}");

        let _ = app.update(Message::Undo);
        assert_eq!(app.project.audio_items()[0].start_sample(), 44_100);
        assert_eq!(app.timeline.selected_items, selection);
        let _ = app.update(Message::Redo);
        assert_eq!(app.project.audio_items()[0].start_sample(), expected_sample);
        assert_eq!(app.timeline.selected_items, selection);
        assert_eq!(app.project.audio_items()[0].track_id(), track_id);
    }
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
fn item_nudge_sample_delta_preserves_fractional_midi_item_position() {
    use super::item_nudge::{Mode, Unit};

    let mut app = item_snap_test_app(96_020, 3_840);
    let midi_id = app.project.midi_items()[0].id();
    let original_position = MidiItemPosition::new(3_840, 160).unwrap();
    app.project
        .apply(DawAction::SetMidiItemPosition {
            item_id: midi_id,
            position: original_position,
        })
        .unwrap();
    app.timeline.rebuild(&app.project);

    let _ = app.update(Message::ItemNudgeModeChanged(Mode::Nudge));
    let _ = app.update(Message::ItemNudgeUnitChanged(Unit::Samples));
    let _ = app.update(Message::ItemNudgeValueChanged("480".into()));
    let _ = app.update(Message::NudgeSelectedItems(1));

    let expected_position = MidiItemPosition::new(3_860, 0).unwrap();
    assert_eq!(
        app.project.midi_items()[0].start_position(),
        expected_position
    );
    assert_eq!(app.project.audio_items()[0].start_sample(), 96_500);
    assert!(app.item_nudge_dialog.error.is_none());
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.project.midi_items()[0].start_position(),
        original_position
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
                        channel: 0,
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
                        channel: 0,
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
                    channel: 0,
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
                    channel: 0,
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
                    channel: 0,
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
                    channel: 0,
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
                channel: 0,
                pitch: 60,
                tick: 0,
                duration: 960,
                velocity: 96,
                off_velocity: 0,
            }],
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiControllers {
            item_id: midi_id,
            controllers: vec![MidiControllerData {
                channel: 0,
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
                channel: 0,
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
                channel: 0,
                pitch: 60,
                tick: 120,
                duration: 240,
                velocity: 96,
                off_velocity: 0,
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
                    channel: 0,
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
                    channel: 0,
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
                        channel: 0,
                        pitch: 60,
                        tick: 960,
                        duration: 960,
                        velocity: 96,
                        off_velocity: 0,
                    },
                    MidiNoteData {
                        channel: 0,
                        pitch: 64,
                        tick: 2_880,
                        duration: 960,
                        velocity: 108,
                        off_velocity: 0,
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
                        channel: 0,
                        pitch: 64,
                        tick: 2_880,
                        duration: 960,
                        velocity: 108,
                        off_velocity: 0,
                    }],
                })
                .unwrap();
        } else {
            app.project
                .apply(DawAction::AddMidiNotes {
                    item_id,
                    notes: vec![
                        MidiNoteData {
                            channel: 0,
                            pitch: 60,
                            tick: 960,
                            duration: 960,
                            velocity: 96,
                            off_velocity: 0,
                        },
                        MidiNoteData {
                            channel: 0,
                            pitch: 64,
                            tick: 2_880,
                            duration: 960,
                            velocity: 108,
                            off_velocity: 0,
                        },
                    ],
                })
                .unwrap();
        }
        app.project
            .apply(DawAction::SetMidiControllers {
                item_id,
                controllers: vec![MidiControllerData {
                    channel: 0,
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
                    channel: 0,
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
                        channel: 0,
                        pitch: 64,
                        tick: 2_880,
                        duration: 960,
                        velocity: 108,
                        off_velocity: 0,
                    }],
                })
                .unwrap();
            app.project
                .apply(DawAction::SetMidiControllers {
                    item_id,
                    controllers: vec![MidiControllerData {
                        channel: 0,
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
                        channel: 0,
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
fn item_nudge_edit_cursor_notes_snap_uses_note_grid_without_moving_items() {
    use super::item_nudge::{NoteValue, Target, Unit};

    for (snap, expected_tick) in [(false, 3_080), (true, 3_200)] {
        let mut app = item_snap_test_app(0, 0);
        app.timeline.edit_cursor_tick = 2_440;
        let audio_start = app.project.audio_items()[0].start_sample();
        let midi_start = app.project.midi_items()[0].start_tick();
        let selection = app.timeline.selected_items.clone();

        let _ = app.update(Message::ItemNudgeTargetChanged(Target::EditCursor));
        let _ = app.update(Message::ItemNudgeUnitChanged(Unit::Notes));
        let _ = app.update(Message::ItemNudgeNoteValueChanged(
            NoteValue::QuarterTriplet,
        ));
        let _ = app.update(Message::ItemNudgeSnapToUnitChanged(snap));
        let _ = app.update(Message::NudgeSelectedItems(1));

        assert_eq!(app.timeline.edit_cursor_tick, expected_tick, "snap={snap}");
        assert_eq!(app.project.audio_items()[0].start_sample(), audio_start);
        assert_eq!(app.project.midi_items()[0].start_tick(), midi_start);
        assert_eq!(app.timeline.selected_items, selection);
        assert!(app.item_nudge_dialog.error.is_none());
    }
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
                    channel: 0,
                    pitch: 60,
                    tick: 960,
                    duration: 960,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 64,
                    tick: 2_880,
                    duration: 960,
                    velocity: 108,
                    off_velocity: 0,
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
                    channel: 0,
                    pitch: 60,
                    tick: 960,
                    duration: 960,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 64,
                    tick: 2_880,
                    duration: 960,
                    velocity: 108,
                    off_velocity: 0,
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
                    channel: 0,
                    pitch: 60,
                    tick: 960,
                    duration: 1_920,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 64,
                    tick: 4_800,
                    duration: 1_920,
                    velocity: 108,
                    off_velocity: 0,
                },
            ],
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![MidiControllerData {
                channel: 0,
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
                channel: 0,
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
                    channel: 0,
                    pitch: 60,
                    tick: 2_880,
                    duration: 1_920,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 64,
                    tick: 6_720,
                    duration: 1_920,
                    velocity: 108,
                    off_velocity: 0,
                },
            ],
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![MidiControllerData {
                channel: 0,
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
                channel: 0,
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
                    channel: 0,
                    pitch: 60,
                    tick: 2_880,
                    duration: 1_920,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 64,
                    tick: 6_720,
                    duration: 1_920,
                    velocity: 108,
                    off_velocity: 0,
                },
            ],
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![MidiControllerData {
                channel: 0,
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
                channel: 0,
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
                    channel: 0,
                    pitch: 60,
                    tick: 100,
                    duration: 300,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 64,
                    tick: 6_720,
                    duration: 1_920,
                    velocity: 108,
                    off_velocity: 0,
                },
            ],
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiControllers {
            item_id,
            controllers: vec![MidiControllerData {
                channel: 0,
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
                channel: 0,
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
                    channel: 0,
                    pitch: 60,
                    tick: 960,
                    duration: 960,
                    velocity: 96,
                    off_velocity: 0,
                },
                MidiNoteData {
                    channel: 0,
                    pitch: 64,
                    tick: 2_880,
                    duration: 960,
                    velocity: 108,
                    off_velocity: 0,
                },
            ],
        })
        .unwrap();
    app.project
        .apply(DawAction::SetMidiControllers {
            item_id: midi_id,
            controllers: vec![MidiControllerData {
                channel: 0,
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
                channel: 0,
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

    app.timeline.set_edit_cursor_tick(0);
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

#[cfg(feature = "audio-device")]
#[test]
fn repeat_action_uses_reaper_default_key_and_toggles_transport_state() {
    let mut app = App::default();
    let command = commands::find(&app, "toggle repeat").expect("repeat action should exist");
    assert_eq!(
        commands::stable_id(command).as_deref(),
        Some("transport.toggle-repeat")
    );
    assert_eq!(commands::toggle_state(&app, command), Some(false));
    app.action_list.set_query("toggle repeat".to_owned());
    let entry = app
        .action_list_entries()
        .into_iter()
        .find(|entry| entry.id == command)
        .expect("repeat action should appear in Action List");
    assert_eq!(entry.shortcut.as_deref(), Some("R"));

    let revision = app.revision;
    let _ = app.update(Message::ExecuteCommand(command));
    assert!(app.repeat_enabled);
    assert!(app.project_arrangement_view_state().repeat_enabled);
    assert!(app.loop_time_preferences_save_at.is_none());
    assert_eq!(commands::toggle_state(&app, command), Some(true));
    let _ = app.update(Message::ExecuteCommand(command));
    assert!(!app.repeat_enabled);
    assert!(!app.project_arrangement_view_state().repeat_enabled);
    assert!(app.loop_time_preferences_save_at.is_none());
    assert_eq!(commands::toggle_state(&app, command), Some(false));
    assert_eq!(app.revision, revision);
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
    assert_eq!(app.status, "Preparing playback at sample 0â€¦");
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
    assert_eq!(app.status, format!("Saving {}â€¦", expected.display()));
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
    assert_eq!(app.status, format!("Saving {}â€¦", current.display()));
}

#[test]
fn recent_projects_submenu_opens_the_selected_path_through_project_transition_guards() {
    let directory = tempfile::tempdir().expect("test directory should be created");
    let recent_path = directory.path().join("Mix.aaadaw");
    let older_path = directory.path().join("Older mix.aaadaw");
    std::fs::write(&recent_path, b"project fixture").unwrap();
    std::fs::write(&older_path, b"older project fixture").unwrap();
    let mut app = App {
        recent_project_paths: vec![recent_path.clone(), older_path],
        ..App::default()
    };

    let file_menu = commands::for_menu(&app, MainMenu::File);
    let save_all_index = file_menu
        .iter()
        .position(|entry| entry.id == CommandId::SaveAllProjects)
        .expect("File menu should expose Save all projects");
    assert_eq!(
        file_menu[save_all_index + 1].id,
        CommandId::OpenProjectTemplates
    );
    assert_eq!(
        file_menu[save_all_index + 2].id,
        CommandId::OpenRecentProjects
    );
    assert_eq!(
        file_menu[save_all_index + 3].id,
        CommandId::CloseProject,
        "Recent projects belongs before the close-project section"
    );

    let _ = app.update(Message::ToggleMainMenu(MainMenu::File));
    let _ = app.update(Message::ToggleRecentProjectsMenu);
    assert!(app.recent_projects_menu_open);
    let recent_entries = commands::for_menu(&app, MainMenu::File);
    assert!(
        recent_entries
            .iter()
            .all(|entry| !matches!(entry.id, CommandId::OpenRecentProject(_)))
    );
    let recent_entry = commands::recent_project_entries(&app)
        .into_iter()
        .find(|entry| entry.id == CommandId::OpenRecentProject(0))
        .expect("cascading submenu should list the recent path");
    assert!(recent_entry.label.contains("Mix.aaadaw"));
    assert!(
        commands::recent_project_entries(&app)
            .iter()
            .any(|entry| entry.id == CommandId::OpenRecentProject(1))
    );
    assert!(
        commands::for_mobile_menu(&app, MainMenu::File)
            .iter()
            .any(|entry| entry.id == CommandId::OpenRecentProject(0))
    );

    app.menu_selected_command = Some(CommandId::OpenRecentProjects);
    let _ = app.update(Message::MenuKeyboard(MenuNavigation::NextMenu));
    assert!(
        app.recent_projects_menu_open,
        "Right should enter the submenu"
    );
    let _ = app.update(Message::MenuKeyboard(MenuNavigation::NextCommand));
    assert_eq!(
        app.menu_selected_command,
        Some(CommandId::OpenRecentProject(0))
    );
    let _ = app.update(Message::MenuKeyboard(MenuNavigation::NextCommand));
    assert_eq!(
        app.menu_selected_command,
        Some(CommandId::OpenRecentProject(1))
    );
    let _ = app.update(Message::MenuKeyboard(MenuNavigation::PreviousCommand));
    assert_eq!(
        app.menu_selected_command,
        Some(CommandId::OpenRecentProject(0))
    );
    let _ = app.update(Message::MenuKeyboard(MenuNavigation::LastCommand));
    assert_eq!(
        app.menu_selected_command,
        Some(CommandId::OpenRecentProject(1))
    );
    let _ = app.update(Message::MenuKeyboard(MenuNavigation::FirstCommand));
    assert_eq!(
        app.menu_selected_command,
        Some(CommandId::OpenRecentProject(0))
    );
    let _ = app.update(Message::MenuKeyboard(MenuNavigation::PreviousMenu));
    assert!(
        !app.recent_projects_menu_open,
        "Left should return to the parent item"
    );
    assert_eq!(
        app.menu_selected_command,
        Some(CommandId::OpenRecentProjects)
    );

    let _ = app.update(Message::MenuKeyboard(MenuNavigation::NextMenu));
    let _ = app.update(Message::MenuKeyboard(MenuNavigation::NextCommand));
    let _ = app.update(Message::MenuKeyboard(MenuNavigation::Activate));
    assert!(app.io_busy);
    assert_eq!(app.project_path_query, recent_path.to_string_lossy());
    assert_eq!(app.status, format!("Opening {}â€¦", recent_path.display()));
    assert_eq!(
        app.pending_project_transition,
        Some(super::PendingProjectTransition::OpenProject)
    );

    let mut dirty_app = App {
        recent_project_paths: vec![recent_path.clone()],
        revision: 1,
        ..App::default()
    };
    let _ = dirty_app.update(Message::ExecuteCommand(CommandId::OpenRecentProject(0)));
    assert_eq!(
        dirty_app.pending_project_transition,
        Some(super::PendingProjectTransition::OpenProject)
    );
    assert_eq!(
        dirty_app.pending_project_path_to_open,
        Some(recent_path.clone())
    );
    assert_eq!(dirty_app.status, "Save changes to the current project?");

    let _ = dirty_app.update(Message::CancelProjectTransition);
    assert_eq!(dirty_app.pending_project_path_to_open, None);
    assert!(dirty_app.is_dirty());

    let project_lock = aaadaw_storage::ProjectSessionLock::acquire(&recent_path)
        .expect("the current recent project should be lockable");
    let mut same_project_app = App {
        recent_project_paths: vec![recent_path.clone()],
        project_path: Some(recent_path.clone()),
        project_lock: Some(project_lock),
        revision: 1,
        ..App::default()
    };
    let _ = same_project_app.update(Message::ExecuteCommand(CommandId::OpenRecentProject(0)));
    assert_eq!(same_project_app.pending_project_transition, None);
    assert_eq!(same_project_app.pending_project_path_to_open, None);
    assert_eq!(
        same_project_app.status,
        format!("{} is already open", recent_path.display())
    );
    assert!(same_project_app.is_dirty());
}

#[test]
fn project_templates_open_isolated_working_copies_and_never_overwrite_the_template() {
    let directory = tempfile::tempdir().expect("test directory should be created");
    let template_path = directory.path().join("First Project.aaadaw");
    save_project_file(template_path.clone(), Project::new().snapshot(), false)
        .expect("test template should be a valid project file");
    let original_template = std::fs::read(&template_path).unwrap();
    let mut app = App {
        project_template_paths: vec![template_path.clone()],
        ..App::default()
    };

    let file_menu = commands::for_menu(&app, MainMenu::File);
    let templates_index = file_menu
        .iter()
        .position(|entry| entry.id == CommandId::OpenProjectTemplates)
        .expect("File menu should expose Project templates");
    assert_eq!(
        file_menu[templates_index + 1].id,
        CommandId::OpenRecentProjects,
        "Project templates belongs immediately before Recent projects"
    );
    let _ = app.update(Message::ToggleMainMenu(MainMenu::File));
    let _ = app.update(Message::ToggleProjectTemplatesMenu);
    assert!(app.project_templates_menu_open);
    assert!(
        commands::for_menu(&app, MainMenu::File)
            .iter()
            .all(|entry| !matches!(entry.id, CommandId::OpenProjectTemplate(_)))
    );
    let template_entries = commands::project_template_entries(&app);
    assert_eq!(template_entries[0].id, CommandId::SaveProjectAsTemplate);
    assert_eq!(template_entries[1].id, CommandId::ShowProjectTemplatesPath);
    assert_eq!(template_entries[2].id, CommandId::ProjectTemplateHeader);
    assert!(!template_entries[2].enabled);
    assert_eq!(template_entries[3].id, CommandId::OpenProjectTemplate(0));
    assert_eq!(template_entries[3].label, "First Project");
    assert!(
        commands::for_mobile_menu(&app, MainMenu::File)
            .iter()
            .any(|entry| entry.id == CommandId::OpenProjectTemplate(0)),
        "touch File menus should expose templates inline"
    );

    app.menu_selected_command = Some(CommandId::OpenProjectTemplates);
    let _ = app.update(Message::MenuKeyboard(MenuNavigation::NextMenu));
    assert_eq!(
        app.menu_selected_command,
        Some(CommandId::OpenProjectTemplates)
    );
    let _ = app.update(Message::MenuKeyboard(MenuNavigation::NextCommand));
    assert_eq!(
        app.menu_selected_command,
        Some(CommandId::SaveProjectAsTemplate)
    );
    let _ = app.update(Message::MenuKeyboard(MenuNavigation::NextCommand));
    assert_eq!(
        app.menu_selected_command,
        Some(CommandId::ShowProjectTemplatesPath)
    );
    let _ = app.update(Message::MenuKeyboard(MenuNavigation::NextCommand));
    assert_eq!(
        app.menu_selected_command,
        Some(CommandId::OpenProjectTemplate(0))
    );
    let _ = app.update(Message::MenuKeyboard(MenuNavigation::FirstCommand));
    assert_eq!(
        app.menu_selected_command,
        Some(CommandId::SaveProjectAsTemplate)
    );
    let _ = app.update(Message::MenuKeyboard(MenuNavigation::LastCommand));
    assert_eq!(
        app.menu_selected_command,
        Some(CommandId::OpenProjectTemplate(0))
    );
    let _ = app.update(Message::MenuKeyboard(MenuNavigation::PreviousMenu));
    assert!(!app.project_templates_menu_open);
    assert_eq!(
        app.menu_selected_command,
        Some(CommandId::OpenProjectTemplates)
    );

    let _ = app.update(Message::OpenProjectTemplate(template_path.clone()));
    assert_eq!(
        app.pending_project_transition,
        Some(super::PendingProjectTransition::OpenProject)
    );
    assert!(app.io_busy);
    let (working_directory, working_path) =
        super::project_templates::create_working_copy(&template_path).unwrap();
    let _ = app.update(Message::ProjectTemplateCopied(
        template_path.clone(),
        Ok((working_directory, working_path.clone())),
    ));
    assert!(app.io_busy);
    let working_directory = working_path.parent().unwrap().to_path_buf();
    let _ = app.update(Message::Escape);
    assert_eq!(
        app.pending_project_transition,
        Some(super::PendingProjectTransition::OpenProject)
    );
    assert!(
        app.pending_project_template_working_copy.is_some(),
        "Escape must not delete a working copy while project loading is in flight"
    );
    assert!(working_directory.exists());
    let loaded_project = load_project_session(working_path.clone())
        .expect("working copy should open as an independent project session");
    let _ = app.update(Message::ProjectLoaded(
        working_path.clone(),
        std::sync::Arc::new(std::sync::Mutex::new(Some(Ok(loaded_project)))),
    ));
    assert_eq!(app.project_path.as_deref(), Some(working_path.as_path()));
    assert_eq!(
        app.project_template_source.as_deref(),
        Some(template_path.as_path())
    );
    assert_eq!(
        app.project_template_working_copy
            .as_ref()
            .map(|working_copy| working_copy.directory.as_path()),
        working_path.parent()
    );
    assert_eq!(
        app.status,
        "Created project from template First Project.aaadaw"
    );
    assert!(app.recent_project_paths.is_empty());

    drop(super::project_io::save_project(
        &mut app,
        Some(template_path.clone()),
    ));
    assert!(!app.io_busy, "saving onto the source should be rejected");
    assert!(app.status.contains("outside the ProjectTemplates folder"));
    assert_eq!(std::fs::read(&template_path).unwrap(), original_template);
    let _ = app.update(Message::SaveProject);
    assert!(
        app.path_picker_busy,
        "ordinary Save from a template project must route through Save As"
    );
    let _ = app.update(Message::PathPicked(PathPickerTarget::SaveProject, Ok(None)));
    assert!(!app.path_picker_busy);
}

#[test]
fn empty_project_templates_menu_has_native_disabled_header_and_empty_state() {
    let app = App::default();
    let entries = commands::project_template_entries(&app);

    assert_eq!(entries[0].id, CommandId::SaveProjectAsTemplate);
    assert_eq!(entries[0].label, "Save project as template...");
    assert_eq!(entries[1].id, CommandId::ShowProjectTemplatesPath);
    assert_eq!(entries[2].id, CommandId::ProjectTemplateHeader);
    assert_eq!(entries[2].label, "Load project template:");
    assert!(entries[2].separator_before);
    assert!(!entries[2].enabled);
    assert_eq!(entries[3].id, CommandId::NoProjectTemplates);
    assert_eq!(entries[3].label, "(no project templates)");
    assert!(!entries[3].enabled);
}

#[test]
fn saving_a_project_as_template_creates_a_valid_non_overwritable_copy() {
    let directory = tempfile::tempdir().expect("test directory should be created");
    let template_path = directory.path().join("Blank Project.aaadaw");
    super::project_io::save_project_template(
        template_path.clone(),
        Project::new().snapshot(),
        ArrangementViewState::default(),
        None,
    )
    .expect("a new project template should be saved");
    assert!(template_path.is_file());
    assert!(load_project_file(template_path.clone()).is_ok());

    let original = std::fs::read(&template_path).unwrap();
    let error = super::project_io::save_project_template(
        template_path.clone(),
        Project::new().snapshot(),
        ArrangementViewState::default(),
        None,
    )
    .expect_err("existing templates must never be overwritten");
    assert!(error.contains("already exists"));
    assert_eq!(std::fs::read(template_path).unwrap(), original);
}

#[test]
fn confirmed_project_template_overwrite_replaces_the_file_atomically() {
    let directory = tempfile::tempdir().expect("test directory should be created");
    let template_path = directory.path().join("Blank Project.aaadaw");
    let mut original_app = App::default();
    let _ = original_app.update(Message::AddTrack);
    super::project_io::save_project_template(
        template_path.clone(),
        original_app.project.snapshot(),
        ArrangementViewState::default(),
        None,
    )
    .expect("the original template should be saved");
    assert_eq!(
        load_project_file(template_path.clone())
            .unwrap()
            .tracks()
            .len(),
        1
    );

    let mut replacement_app = App::default();
    let _ = replacement_app.update(Message::AddTrack);
    let _ = replacement_app.update(Message::AddTrack);
    super::project_io::overwrite_project_template(
        template_path.clone(),
        replacement_app.project.snapshot(),
        ArrangementViewState::default(),
        None,
    )
    .expect("an explicitly confirmed template can be replaced");

    assert_eq!(load_project_file(template_path).unwrap().tracks().len(), 2);
}

#[test]
fn failed_project_template_overwrite_preserves_the_existing_file() {
    let directory = tempfile::tempdir().expect("test directory should be created");
    let template_path = directory.path().join("Blank Project.aaadaw");
    super::project_io::save_project_template(
        template_path.clone(),
        Project::new().snapshot(),
        ArrangementViewState::default(),
        None,
    )
    .expect("the original template should be saved");
    let original_bytes = std::fs::read(&template_path).unwrap();
    let invalid_media_source = directory.path().join("invalid-source.aaadaw");
    std::fs::write(&invalid_media_source, b"not an AAADAW project").unwrap();

    let error = super::project_io::overwrite_project_template(
        template_path.clone(),
        Project::new().snapshot(),
        ArrangementViewState::default(),
        Some(invalid_media_source),
    )
    .expect_err("a failed write must not replace the existing template");

    assert!(!error.is_empty());
    assert_eq!(std::fs::read(&template_path).unwrap(), original_bytes);
    assert!(load_project_file(template_path).is_ok());
}

#[test]
fn project_template_overwrite_prompt_is_modal_and_escape_cancels_without_writing() {
    let directory = tempfile::tempdir().expect("test directory should be created");
    let template_path = directory.path().join("Existing.aaadaw");
    let original = b"existing template file";
    std::fs::write(&template_path, original).unwrap();
    let mut app = App::default();

    let _ = app.request_project_template_save(template_path.clone(), directory.path());
    assert_eq!(
        app.pending_project_template_overwrite.as_deref(),
        Some(template_path.as_path())
    );
    assert!(!app.io_busy);

    let _ = app.update(Message::AddTrack);
    assert!(app.project.tracks().is_empty());
    let _ = app.update(Message::Escape);

    assert!(app.pending_project_template_overwrite.is_none());
    assert_eq!(app.status, "Project template overwrite cancelled");
    assert_eq!(std::fs::read(template_path).unwrap(), original);
}

#[test]
fn project_template_overwrite_refuses_to_replace_the_open_project_file() {
    let directory = tempfile::tempdir().expect("test directory should be created");
    let template_path = directory.path().join("Existing.aaadaw");
    let original = b"existing template file";
    std::fs::write(&template_path, original).unwrap();
    let mut app = App {
        project_path: Some(template_path.clone()),
        pending_project_template_overwrite: Some(template_path.clone()),
        ..App::default()
    };

    let _ = app.update(Message::ConfirmProjectTemplateOverwrite);

    assert!(app.pending_project_template_overwrite.is_none());
    assert_eq!(
        app.status,
        "Cannot overwrite a project template while it is open"
    );
    assert_eq!(std::fs::read(template_path).unwrap(), original);
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
        " Â· stream underrun: 256 samples"
    );
    assert_eq!(
        super::view::playback_diagnostic_suffix("PipeWire", 0, 0, 0, None, 2, false),
        " Â· PipeWire errors: 2"
    );
    assert_eq!(
        super::view::playback_diagnostic_suffix("JACK", 256, 3, 1, Some(2), 2, false),
        " Â· stream underrun: 256 samples Â· JACK XRuns: 2 Â· Master ceiling (total): 3 samples Â· non-finite Master samples silenced (total): 1 Â· JACK errors: 2"
    );
    assert_eq!(
        super::view::playback_diagnostic_suffix("JACK", 0, 0, 0, Some(2), 0, false),
        " Â· JACK XRuns: 2"
    );
    assert_eq!(
        super::view::playback_diagnostic_suffix("PipeWire", 256, 0, 0, None, 2, false),
        " Â· stream underrun: 256 samples Â· PipeWire errors: 2"
    );
    assert_eq!(
        super::view::playback_diagnostic_suffix("WASAPI", 0, 0, 0, None, 1, true),
        " Â· output device unavailable; close playback and reopen it after selecting a default device Â· WASAPI errors: 1"
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
        app.selected_fx_chain_iç¯|Û†òµë(š+my×€¢7WW#£¦Ç•öÖ–F•öWfVçE÷&÷W'F–W5öVF—B€¢“À¢Ö–F”WfVçE&÷W'F–W4VF—C£¥&VÆF—fR„Ö–F”WfVçE&÷W'F–W5&VÆF—fUfÇVR°¢÷W&F÷#¢Ö–F”WfVçE&÷W'F–W4÷W&F÷#£¤FBÀ¢÷W&æC¢SãÀ¢Ò’À¢À¢#rÀ¢’À¢æöæP¢“° ¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢5óƒCÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWB÷&–v–æÅöæ÷FW2Ò°¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢#CÀ¢GW&F–öã¢#À¢fVÆö6—G“¢“À¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢"À¢—F6ƒ¢cBÀ¢F–6³¢CƒÀ¢GW&F–öã¢#CÀ¢fVÆö6—G“¢À¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢÷&–v–æÅöæ÷FW2çFõ÷fV2‚’À¢Ò¢æW‡V7B‚'FW7BÔ”D’æ÷FW26†÷VÆB&R–ç6W'FVB"“°¢ÆWBæ÷FUö–G2Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Ğ¢ææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂæ÷FRæ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWB÷&–v–æÅö6öçG&öÆÆW'2Ò°¢Ö–F”6öçG&öÆÆW$FF°¢6öçG&öÆÆW#¢rÀ¢6†ææVÃ¢"À¢F–6³¢CƒÀ¢fÇVS¢“À¢ÒÀ¢Ö–F”6öçG&öÆÆW$FF°¢6öçG&öÆÆW#¢À¢6†ææVÃ¢RÀ¢F–6³¢“cÀ¢fÇVS¢À¢ÒÀ¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F”6öçG&öÆÆW'2°¢—FVÕö–BÀ¢6öçG&öÆÆW'3¢÷&–v–æÅö6öçG&öÆÆW'2çFõ÷fV2‚’À¢Ò¢æW‡V7B‚'FW7B6öçG&öÆÆW'26†÷VÆB&R–ç6W'FVB"“°¢ÆWB÷&–v–æÅ÷—F6…ö&VæG2Ò°¢Ö–F•—F6„&VæDFF°¢6†ææVÃ¢"À¢F–6³¢CƒÀ¢fÇVS¢•óÀ¢ÒÀ¢Ö–F•—F6„&VæDFF°¢6†ææVÃ¢RÀ¢F–6³¢“cÀ¢fÇVS¢óÀ¢ÒÀ¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F•—F6„&VæG2°¢—FVÕö–BÀ¢—F6…ö&VæG3¢÷&–v–æÅ÷—F6…ö&VæG2çFõ÷fV2‚’À¢Ò¢æW‡V7B‚'FW7B—F6‚&VæG26†÷VÆB&R–ç6W'FVB"“°¢æÖ–F•öVF—F÷%ö—FVÕö–BÒ6öÖR†—FVÕö–B“° ¢æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Ò†6…6WC£¦g&öÕö—FW"†æ÷FUö–G2“°¢ÆWB†—7F÷'•ö&Vf÷&Uöæ÷FW2Òç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F”WfVçE&÷W'F–W2†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F”WfVçE&÷W'F–W5÷6—F–öâ€¢"³ãã"çFõö÷væVB‚’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F”WfVçE&÷W'F–W4ÆVæwF‚‚"£""çFõö÷væVB‚’’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F”WfVçE&÷W'F–W46†ææVÂ‚"³"çFõö÷væVB‚’’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F”WfVçE&÷W'F–W5&ÖWFW"‚"³ó""çFõö÷væVB‚’’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F”WfVçE&÷W'F–W5fÇVR‚"³"çFõö÷væVB‚’’“°¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”Ö–F”WfVçE&÷W'F–W2“°¢ÆWBVF—FVEöæ÷FW2Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Ğ¢ææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂÖ–F”æ÷FTFF°¢6†ææVÃ¢æ÷FRæ6†ææVÂ‚’À¢—F6ƒ¢æ÷FRç—F6‚‚’À¢F–6³¢æ÷FRçF–6²‚’À¢GW&F–öã¢æ÷FRæGW&F–öâ‚’À¢fVÆö6—G“¢æ÷FRçfVÆö6—G’‚’À¢öfe÷fVÆö6—G“¢À¢Ò¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢76W'EöW€¢VF—FVEöæ÷FW2À¢°¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢sBÀ¢F–6³¢ó#À¢GW&F–öã¢#CÀ¢fVÆö6—G“¢À¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢2À¢—F6ƒ¢s‚À¢F–6³¢óCCÀ¢GW&F–öã¢CƒÀ¢fVÆö6—G“¢À¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ğ¢“°¢76W'EöW†ç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚’Â†—7F÷'•ö&Vf÷&Uöæ÷FW2²“°¢ÆWBòÒçWFFR„ÖW76vS£¤6æ6VÄÖ–F”WfVçE&÷W'F–W2“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Ğ¢ææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂÖ–F”æ÷FTFF°¢6†ææVÃ¢æ÷FRæ6†ææVÂ‚’À¢—F6ƒ¢æ÷FRç—F6‚‚’À¢F–6³¢æ÷FRçF–6²‚’À¢GW&F–öã¢æ÷FRæGW&F–öâ‚’À¢fVÆö6—G“¢æ÷FRçfVÆö6—G’‚’À¢öfe÷fVÆö6—G“¢À¢Ò¢æ6öÆÆV7C££ÅfV3Åóãâ‚’À¢÷&–v–æÅöæ÷FW0¢“° ¢æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ6ÆV"‚“°¢æÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2Ò†6…6WC£¦g&öÒ†÷&–v–æÅö6öçG&öÆÆW'2“°¢ÆWB†—7F÷'•ö&Vf÷&Uö6öçG&öÆÆW'2Òç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F”WfVçE&÷W'F–W2†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F”WfVçE&÷W'F–W5÷6—F–öâ€¢"³ãã"çFõö÷væVB‚’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F”WfVçE&÷W'F–W46†ææVÂ‚"Ó"çFõö÷væVB‚’’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F”WfVçE&÷W'F–W5&ÖWFW"‚"³"çFõö÷væVB‚’’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F”WfVçE&÷W'F–W5fÇVR‚"³"çFõö÷væVB‚’’“°¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”Ö–F”WfVçE&÷W'F–W2“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ6öçG&öÆÆW'2‚’À¢°¢Ö–F”6öçG&öÆÆW$FF°¢6öçG&öÆÆW#¢‚À¢6†ææVÃ¢À¢F–6³¢óCCÀ¢fÇVS¢À¢ÒÀ¢Ö–F”6öçG&öÆÆW$FF°¢6öçG&öÆÆW#¢À¢6†ææVÃ¢BÀ¢F–6³¢ó“#À¢fÇVS¢À¢ÒÀ¢Ğ¢“°¢76W'EöW†ç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚’Â†—7F÷'•ö&Vf÷&Uö6öçG&öÆÆW'2²“°¢ÆWBòÒçWFFR„ÖW76vS£¤6æ6VÄÖ–F”WfVçE&÷W'F–W2“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ6öçG&öÆÆW'2‚’À¢÷&–v–æÅö6öçG&öÆÆW'0¢“° ¢æÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ6ÆV"‚“°¢æÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG2Ò†6…6WC£¦g&öÒ†÷&–v–æÅ÷—F6…ö&VæG2“°¢ÆWB†—7F÷'•ö&Vf÷&U÷—F6…ö&VæG2Òç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F”WfVçE&÷W'F–W2†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F”WfVçE&÷W'F–W5fÇVR‚"ó""çFõö÷væVB‚’’“°¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”Ö–F”WfVçE&÷W'F–W2“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç—F6…ö&VæG2‚’À¢°¢Ö–F•—F6„&VæDFF°¢fÇVS¢EóSÀ¢âæ÷&–v–æÅ÷—F6…ö&VæG5³Ğ¢ÒÀ¢Ö–F•—F6„&VæDFF°¢fÇVS¢UóÀ¢âæ÷&–v–æÅ÷—F6…ö&VæG5³Ğ¢ÒÀ¢Ğ¢“°¢76W'EöW†ç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚’Â†—7F÷'•ö&Vf÷&U÷—F6…ö&VæG2²“°¢ÆWBòÒçWFFR„ÖW76vS£¤6æ6VÄÖ–F”WfVçE&÷W'F–W2“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç—F6…ö&VæG2‚’À¢÷&–v–æÅ÷—F6…ö&VæG0¢“°§Ğ ¢5·FW7EĞ¦fâÖ–F•öVF—F÷%÷VçF—¦U÷÷6—F–öç5öæEöVæG5ö6†ævW5ööæÇ•÷6VÆV7FVEöæ÷FW2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢5óƒCÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2°¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢#À¢GW&F–öã¢CÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢"À¢—F6ƒ¢cBÀ¢F–6³¢sÀ¢GW&F–öã¢“À¢fVÆö6—G“¢À¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢crÀ¢F–6³¢3SÀ¢GW&F–öã¢CÀ¢fVÆö6—G“¢ƒÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢ÒÀ¢Ò¢æW‡V7B‚'FW7Bæ÷FW26†÷VÆB&R–ç6W'FVB"“°¢ÆWB6VÆV7FVEö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Òæ–B‚“°¢æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Ò†6…6WC£¦g&öÒ…·6VÆV7FVEö–EÒ“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VçF—¦U6VÆV7FVDÖ–F”æ÷FW4æDVæG2†—FVÕö–B’“° ¢ÆWBæ÷FW2Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚“°¢76W'EöW‚†æ÷FW5³ÒçF–6²‚’Âæ÷FW5³ÒæGW&F–öâ‚’’Âƒ#ÂC’“°¢76W'EöW€¢†æ÷FW5³ÒçF–6²‚’Âæ÷FW5³ÒæGW&F–öâ‚’Âæ÷FW5³Òæ6†ææVÂ‚’’À¢ƒ#Â#Â"¢“°¢76W'EöW‚†æ÷FW5³%ÒçF–6²‚’Âæ÷FW5³%ÒæGW&F–öâ‚’’Âƒ3SÂC’“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW€¢€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒçF–6²‚’À¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒæGW&F–öâ‚¢’À¢ƒsÂ“¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW€¢€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒçF–6²‚’À¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒæGW&F–öâ‚¢’À¢ƒ#Â#¢“°§Ğ ¢5·FW7EĞ¦fâÖ–F•÷VçF—¦UöF–Æöu÷&VÖVÖ&W'5÷6WGF–æw5ögFW%ö6æ6VÅöæEö6öÖÖ—G5÷6VÆV7FVEöæ÷FW2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢5óƒCÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2°¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢sÀ¢GW&F–öã¢“À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢"À¢—F6ƒ¢cBÀ¢F–6³¢3SÀ¢GW&F–öã¢ƒÀ¢fVÆö6—G“¢À¢öfe÷fVÆö6—G“¢À¢ÒÀ¢ÒÀ¢Ò¢æW‡V7B‚'FW7Bæ÷FW26†÷VÆB&R–ç6W'FVB"“°¢ÆWB6VÆV7FVEö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Òæ–B‚“°¢æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Ò†6…6WC£¦g&öÒ…·6VÆV7FVEö–EÒ“° ¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F•VçF—¦TF–Æör†—FVÕö–B’“°¢ÆWBF–ÆörÒ ¢æÖ–F•÷VçF—¦UöF–Æöp¢æW‡V7B‚'VçF—¦RF–Æör6†÷VÆB÷Vâ"“°¢76W'EöW†F–Æörç66÷RÂÖ–F•VçF—¦U66÷S£¥6VÆV7FVDæ÷FW2“°¢76W'EöW†F–Æöræ÷W&F–öâÂÖ–F•VçF—¦T÷W&F–öã£¥÷6—F–öâ“°¢76W'EöW†F–Æörç7G&VæwF…÷W&6VçBÂ“°¢76W'B‚F–Æöræ'—72“°¢76W'B†F–Æöræf—…ö÷fW&Æ2“°¢ÆWBòÒçWFFR„ÖW76vS£¤6æ6VÄÖ–F•VçF—¦R“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒçF–6²‚’Âs“° ¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F•VçF—¦TF–Æör†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦U66÷R€¢Ö–F•VçF—¦U66÷S£¥6VÆV7FVDWfVçG2À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦T÷W&F–öâ€¢Ö–F•VçF—¦T÷W&F–öã£¥÷6—F–öäæDæ÷FTVæBÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦U7G&VæwF‚ƒc’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦T'—72‡G'VR’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦Tf—„÷fW&Æ2†fÇ6R’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6æ6VÄÖ–F•VçF—¦R“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F•VçF—¦TF–Æör†—FVÕö–B’“°¢ÆWB&VÖVÖ&W&VBÒ ¢æÖ–F•÷VçF—¦UöF–Æöp¢æW‡V7B‚'VçF—¦RF–Æör6†÷VÆB&V÷Vâv—F‚&VÖVÖ&W&VB6WGF–æw2"“°¢76W'EöW‡&VÖVÖ&W&VBç66÷RÂÖ–F•VçF—¦U66÷S£¥6VÆV7FVDWfVçG2“°¢76W'EöW€¢&VÖVÖ&W&VBæ÷W&F–öâÀ¢Ö–F•VçF—¦T÷W&F–öã£¥÷6—F–öäæDæ÷FTVæ@¢“°¢76W'EöW‡&VÖVÖ&W&VBç7G&VæwF…÷W&6VçBÂc“°¢76W'B‡&VÖVÖ&W&VBæ'—72“°¢76W'B‚&VÖVÖ&W&VBæf—…ö÷fW&Æ2“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦U66÷R€¢Ö–F•VçF—¦U66÷S£¥6VÆV7FVDæ÷FW2À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦T÷W&F–öâ€¢Ö–F•VçF—¦T÷W&F–öã£¥÷6—F–öâÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦U7G&VæwF‚ƒS’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦T'—72†fÇ6R’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦Tf—„÷fW&Æ2‡G'VR’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6öÖÖ—DÖ–F•VçF—¦R“°¢76W'B†æÖ–F•÷VçF—¦UöF–Æöræ—5÷6öÖR‚’“°¢ÆWBæ÷FW2Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚“°¢76W'EöW‚†æ÷FW5³ÒçF–6²‚’Âæ÷FW5³ÒæGW&F–öâ‚’’ÂƒCRÂ“’“°¢76W'EöW†æ÷FW5³ÒçF–6²‚’Â3S“° ¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦T÷W&F–öâ€¢Ö–F•VçF—¦T÷W&F–öã£¥÷6—F–öäæDæ÷FTVæBÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦U7G&VæwF‚ƒ’“°¢ÆWBòÒçWFFR„ÖW76vS£¤W66R“°¢76W'B†æÖ–F•÷VçF—¦UöF–Æöræ—5öæöæR‚’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒçF–6²‚’ÂCR“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒçF–6²‚’Âs“°§Ğ ¢5·FW7EĞ¦fâÖ–F•÷VçF—¦UöF–Æöuö'—75÷6¶—5ö6öÖÖ—EöæEööµ÷v—F†÷WE÷&V6÷&F–æuöåöVF—B‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢5óƒCÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2´Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢À¢GW&F–öã¢ƒÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÕÒÀ¢Ò¢æW‡V7B‚'FW7Bæ÷FR6†÷VÆB&R–ç6W'FVB"“°¢ÆWB&Vf÷&RÒç&ö¦V7Bç6æ6†÷B‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F•VçF—¦TF–Æör†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦U66÷R„Ö–F•VçF—¦U66÷S£¤ÆÄæ÷FW2’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦T'—72‡G'VR’“°¢76W'B†æÖ–F•÷VçF—¦UöF–Æöræ—5÷6öÖR‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6öÖÖ—DÖ–F•VçF—¦R“°¢76W'B†æÖ–F•÷VçF—¦UöF–Æöræ—5÷6öÖR‚’“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&Vf÷&R“°¢ÆWBòÒçWFFR„ÖW76vS£¤66WDÖ–F•VçF—¦R“°¢76W'B†æÖ–F•÷VçF—¦UöF–Æöræ—5öæöæR‚’“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&Vf÷&R“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F•VçF—¦TF–Æör†—FVÕö–B’“°¢ÆWB&VÖVÖ&W&VBÒ ¢æÖ–F•÷VçF—¦UöF–Æöp¢æW‡V7B‚&66WFVBVçF—¦R6WGF–æw26†÷VÆB&R&VÖVÖ&W&VB"“°¢76W'EöW‡&VÖVÖ&W&VBç66÷RÂÖ–F•VçF—¦U66÷S£¤ÆÄæ÷FW2“°¢76W'B‡&VÖVÖ&W&VBæ'—72“°¢ÆWBòÒçWFFR„ÖW76vS£¤6æ6VÄÖ–F•VçF—¦R“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚’æ—5öV×G’‚’“°§Ğ ¢5·FW7EĞ¦fâÖ–F•÷VçF—¦UöF–ÆöuöÆ–W5öÆÅöæ÷FW5öæEöæ÷FUöVæG5ö5ööæU÷VæFö&ÆUöVF—B‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢5óƒCÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2°¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢À¢GW&F–öã¢#À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢"À¢—F6ƒ¢cBÀ¢F–6³¢3SÀ¢GW&F–öã¢ƒÀ¢fVÆö6—G“¢À¢öfe÷fVÆö6—G“¢À¢ÒÀ¢ÒÀ¢Ò¢æW‡V7B‚'FW7Bæ÷FW26†÷VÆB&R–ç6W'FVB"“°¢ÆWB6VÆV7FVEö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Òæ–B‚“°¢æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Ò†6…6WC£¦g&öÒ…·6VÆV7FVEö–EÒ“° ¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F•VçF—¦TF–Æör†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦U66÷R„Ö–F•VçF—¦U66÷S£¤ÆÄæ÷FW2’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦T÷W&F–öâ€¢Ö–F•VçF—¦T÷W&F–öã£¥÷6—F–öäæDæ÷FTVæBÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤66WDÖ–F•VçF—¦R“° ¢76W'B†æÖ–F•÷VçF—¦UöF–Æöræ—5öæöæR‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F•VçF—¦TF–Æör†—FVÕö–B’“°¢ÆWB&VÖVÖ&W&VBÒ ¢æÖ–F•÷VçF—¦UöF–Æöp¢æW‡V7B‚&66WFVBVçF—¦R6WGF–æw26†÷VÆB&R&VÖVÖ&W&VB"“°¢76W'EöW‡&VÖVÖ&W&VBç66÷RÂÖ–F•VçF—¦U66÷S£¤ÆÄæ÷FW2“°¢76W'EöW€¢&VÖVÖ&W&VBæ÷W&F–öâÀ¢Ö–F•VçF—¦T÷W&F–öã£¥÷6—F–öäæDæ÷FTVæ@¢“°¢76W'EöW‡&VÖVÖ&W&VBç7G&VæwF…÷W&6VçBÂ“°¢ÆWBòÒçWFFR„ÖW76vS£¤6æ6VÄÖ–F•VçF—¦R“°¢ÆWBæ÷FW2Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚“°¢76W'EöW‚†æ÷FW5³ÒçF–6²‚’Âæ÷FW5³ÒæGW&F–öâ‚’’Âƒ#Â#’“°¢76W'EöW‚†æ÷FW5³ÒçF–6²‚’Âæ÷FW5³ÒæGW&F–öâ‚’’Âƒ3cÂ#’“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢ÆWBæ÷FW2Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚“°¢76W'EöW‚†æ÷FW5³ÒçF–6²‚’Âæ÷FW5³ÒæGW&F–öâ‚’’ÂƒÂ#’“°¢76W'EöW‚†æ÷FW5³ÒçF–6²‚’Âæ÷FW5³ÒæGW&F–öâ‚’’Âƒ3SÂƒ’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢ÆWBæ÷FW2Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚“°¢76W'EöW‚†æ÷FW5³ÒçF–6²‚’Âæ÷FW5³ÒæGW&F–öâ‚’’Âƒ#Â#’“°¢76W'EöW‚†æ÷FW5³ÒçF–6²‚’Âæ÷FW5³ÒæGW&F–öâ‚’’Âƒ3cÂ#’“°§Ğ ¢5·FW7EĞ¦fâ–æÆ–æUöÖ–F•öw&÷WöVF—Eö—5ööæU÷VæFö&ÆU÷G&ç67F–öâ‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢%óCÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢²ƒÂc’Âƒ3SÂc"’ÂƒSSÂcB’Âƒ“ÂcR•Ğ¢æ–çFõö—FW"‚¢æÖ‡Â‡F–6²Â—F6‚—ÂÖ–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6‚À¢F–6²À¢GW&F–öã¢#À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢Ò¢æ6öÆÆV7B‚’À¢Ò¢æW‡V7B‚'FW7BÔ”D’æ÷FW26†÷VÆB&R–ç6W'FVB"“°¢ÆWB&Vf÷&RÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWBVF—G2Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Ğ¢ææ÷FW2‚¢æ—FW"‚¢æf–ÇFW%öÖ‡Ææ÷FWÂ°¢ÆWBF–6²ÒÖF6‚æ÷FRç—F6‚‚’°¢cÓâƒCÀ¢c"Óâó“À¢cRÓâócCÀ¢òÓâ&WGW&âæöæRÀ¢Ó°¢6öÖR‚€¢æ÷FRæ–B‚’À¢Ö–F”æ÷FTFF°¢6†ææVÃ¢æ÷FRæ6†ææVÂ‚’À¢—F6ƒ¢æ÷FRç—F6‚‚’À¢F–6²À¢GW&F–öã¢æ÷FRæGW&F–öâ‚’À¢fVÆö6—G“¢æ÷FRçfVÆö6—G’‚’À¢öfe÷fVÆö6—G“¢æ÷FRæöfe÷fVÆö6—G’‚’À¢ÒÀ¢’¢Ò¢æ6öÆÆV7C££ÅfV3Åóãâ‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤VF—DÖ–F”æ÷FW2†—FVÕö–BÂVF—G2’“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Ğ¢ææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂ†æ÷FRç—F6‚‚’Âæ÷FRçF–6²‚’Âæ÷FRæGW&F–öâ‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚’À¢°¢ƒcÂƒCÂ#’À¢ƒc"Âó“Â#’À¢ƒcBÂSSÂ#’À¢ƒcRÂócCÂ#¢Ğ¢“°¢ÆWBgFW"Òç&ö¦V7Bç6æ6†÷B‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&Vf÷&R“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’ÂgFW"“°§Ğ ¢5·FW7EĞ¦fâ–æÆ–æUöÖ–F•öw&÷Wö6÷•÷6VÆV7G5ö6÷–W5öæEö—5ööæU÷VæFö&ÆU÷G&ç67F–öâ‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢5óƒCÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢²ƒƒCÂc’Âƒó“Âc"’ÂƒSSÂcB’ÂƒócCÂcR•Ğ¢æ–çFõö—FW"‚¢æÖ‡Â‡F–6²Â—F6‚—ÂÖ–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6‚À¢F–6²À¢GW&F–öã¢#À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢Ò¢æ6öÆÆV7B‚’À¢Ò¢æW‡V7B‚'FW7BÔ”D’æ÷FW26†÷VÆB&R–ç6W'FVB"“°¢çF–ÖVÆ–æRæ–æÆ–æUöÖ–F•ö—FV×2æ–ç6W'B†—FVÕö–B“°¢ÆWB6÷W&6U÷6VÆV7FVEö–G2Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Ğ¢ææ÷FW2‚¢æ—FW"‚¢æf–ÇFW"‡Ææ÷FWÂÖF6†W2†æ÷FRç—F6‚‚’ÂcÂc"ÂcR’¢æÖ‡Ææ÷FWÂæ÷FRæ–B‚’¢æ6öÆÆV7C££Ä†6…6WCÅóãâ‚“°¢çF–ÖVÆ–æP¢æ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öà¢æ–ç6W'B†—FVÕö–BÂ6÷W&6U÷6VÆV7FVEö–G2æ6ÆöæR‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F”VF—F÷"†—FVÕö–B’“°¢æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Ò6÷W&6U÷6VÆV7FVEö–G2æ6ÆöæR‚“°¢ÆWB&Vf÷&RÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWB÷&–v–æÅö–G2Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Ğ¢ææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂæ÷FRæ–B‚’¢æ6öÆÆV7C££Ä†6…6WCÅóãâ‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤6÷”G&tÖ–F”æ÷FW2€¢—FVÕö–BÀ¢²ƒ“cÂc’Âƒó#Âc"’ÂƒóscÂcR•Ğ¢æ–çFõö—FW"‚¢æÖ‡Â‡F–6²Â—F6‚—ÂÖ–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6‚À¢F–6²À¢GW&F–öã¢#À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢Ò¢æ6öÆÆV7B‚’À¢’“° ¢ÆWBæ÷FW2Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚“°¢76W'EöW†æ÷FW2æÆVâ‚’Âr“°¢76W'EöW€¢æ÷FW0¢æ—FW"‚¢æÖ‡Ææ÷FWÂ†æ÷FRç—F6‚‚’Âæ÷FRçF–6²‚’Âæ÷FRæGW&F–öâ‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚’À¢°¢ƒcÂƒCÂ#’À¢ƒc"Âó“Â#’À¢ƒcBÂSSÂ#’À¢ƒcRÂócCÂ#’À¢ƒcÂ“cÂ#’À¢ƒc"Âó#Â#’À¢ƒcRÂóscÂ#’À¢Ğ¢“°¢ÆWB6÷–VEö–G2Òæ÷FW0¢æ—FW"‚¢æÖ‡Ææ÷FWÂæ÷FRæ–B‚’¢æf–ÇFW"‡Æ–GÂ÷&–v–æÅö–G2æ6öçF–ç2†–B’¢æ6öÆÆV7C££Ä†6…6WCÅóãâ‚“°¢76W'EöW†6÷–VEö–G2æÆVâ‚’Â2“°¢76W'EöW†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Â6÷–VEö–G2“°¢76W'EöW€¢çF–ÖVÆ–æRæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öâævWB‚f—FVÕö–B’À¢6öÖR‚f6÷–VEö–G2¢“°¢ÆWBgFW"Òç&ö¦V7Bç6æ6†÷B‚“° ¢ÆWB6VÆV7FVEö6÷’Ò†6…6WC£¦g&öÒ…²¦6÷–VEö–G2æ—FW"‚’ææW‡B‚’çVçw&‚•Ò“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR€¢F–ÖVÆ–æTWfVçC£¥6WD–æÆ–æTÖ–F”æ÷FU6VÆV7F–öâ°¢—FVÕö–BÀ¢æ÷FUö–G3¢6VÆV7FVEö6÷’æ—FW"‚’æ6÷–VB‚’æ6öÆÆV7B‚’À¢ÒÀ¢’“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’ÂgFW"“°¢76W'EöW€¢çF–ÖVÆ–æRæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öâævWB‚f—FVÕö–B’À¢6öÖR‚g6VÆV7FVEö6÷’¢“°¢76W'EöW†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Â6VÆV7FVEö6÷’“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’ÂgFW"“°¢76W'EöW€¢çF–ÖVÆ–æRæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öâævWB‚f—FVÕö–B’À¢6öÖR‚f6÷–VEö–G2¢“°¢76W'EöW†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Â6÷–VEö–G2“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&Vf÷&R“°¢76W'EöW€¢çF–ÖVÆ–æRæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öâævWB‚f—FVÕö–B’À¢6öÖR‚g6÷W&6U÷6VÆV7FVEö–G2¢“°¢76W'EöW†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Â6÷W&6U÷6VÆV7FVEö–G2“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’ÂgFW"“°¢76W'EöW€¢çF–ÖVÆ–æRæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öâævWB‚f—FVÕö–B’À¢6öÖR‚f6÷–VEö–G2¢“°¢76W'EöW†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Â6÷–VEö–G2“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’ÂgFW"“°¢76W'EöW€¢çF–ÖVÆ–æRæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öâævWB‚f—FVÕö–B’À¢6öÖR‚g6VÆV7FVEö6÷’¢“°¢76W'EöW†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Â6VÆV7FVEö6÷’“°§Ğ ¢5·FW7EĞ¦fâ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öåö—5÷VæFö&ÆU÷v—F†÷WEö6†æv–æu÷&ö¦V7Eö6öçFVçB‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢²ƒÂc’Âƒ3SÂcB•Ğ¢æ–çFõö—FW"‚¢æÖ‡Â‡F–6²Â—F6‚—ÂÖ–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6‚À¢F–6²À¢GW&F–öã¢#À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢Ò¢æ6öÆÆV7B‚’À¢Ò¢æW‡V7B‚'FW7BÔ”D’æ÷FW26†÷VÆB&R–ç6W'FVB"“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRæ–æÆ–æUöÖ–F•ö—FV×2æ–ç6W'B†—FVÕö–B“°¢ÆWBæ÷FUö–G2Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Ğ¢ææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂ†æ÷FRç—F6‚‚’Âæ÷FRæ–B‚’’¢æ6öÆÆV7C££Ä†6„ÖÅòÂóãâ‚“°¢ÆWBf—'7Eö–BÒæ÷FUö–G5²ccÓ°¢ÆWB6V6öæEö–BÒæ÷FUö–G5²ccEÓ°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F”VF—F÷"†—FVÕö–B’“°¢ÆWB&ö¦V7E÷6æ6†÷BÒç&ö¦V7Bç6æ6†÷B‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR€¢F–ÖVÆ–æTWfVçC£¥6WD–æÆ–æTÖ–F”æ÷FU6VÆV7F–öâ°¢—FVÕö–BÀ¢æ÷FUö–G3¢fV2¶f—'7Eö–EÒÀ¢ÒÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR€¢F–ÖVÆ–æTWfVçC£¥6WD–æÆ–æTÖ–F”æ÷FU6VÆV7F–öâ°¢—FVÕö–BÀ¢æ÷FUö–G3¢fV2¶f—'7Eö–BÂ6V6öæEö–EÒÀ¢ÒÀ¢’“°¢ÆWB6VÆV7FVEö&÷F‚Ò†6…6WC£¦g&öÒ…¶f—'7Eö–BÂ6V6öæEö–EÒ“°¢76W'EöW€¢çF–ÖVÆ–æRæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öå²f—FVÕö–EÒÀ¢6VÆV7FVEö&÷F€¢“°¢76W'EöW†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Â6VÆV7FVEö&÷F‚“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&ö¦V7E÷6æ6†÷B“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW€¢çF–ÖVÆ–æRæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öå²f—FVÕö–EÒÀ¢†6…6WC£¦g&öÒ…¶f—'7Eö–EÒ¢“°¢76W'EöW†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Â†6…6WC£¦g&öÒ…¶f—'7Eö–EÒ’“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&ö¦V7E÷6æ6†÷B“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B€¢çF–ÖVÆ–æP¢æ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öà¢æ6öçF–ç5ö¶W’‚f—FVÕö–B¢“°¢76W'B†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ—5öV×G’‚’“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&ö¦V7E÷6æ6†÷B“° ¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW€¢çF–ÖVÆ–æRæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öå²f—FVÕö–EÒÀ¢†6…6WC£¦g&öÒ…¶f—'7Eö–EÒ¢“°¢76W'EöW†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Â†6…6WC£¦g&öÒ…¶f—'7Eö–EÒ’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW€¢çF–ÖVÆ–æRæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öå²f—FVÕö–EÒÀ¢6VÆV7FVEö&÷F€¢“°¢76W'EöW†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Â6VÆV7FVEö&÷F‚“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&ö¦V7E÷6æ6†÷B“°§Ğ ¢5·FW7EĞ¦fâ–æõ÷&öÆÅöæ÷FU÷6VÆV7F–öåö—5÷VæFö&ÆUöæE÷6†&VE÷v—F…ö–æÆ–æUö'&ævVÖVçB‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢²ƒÂc’Âƒ3SÂcB•Ğ¢æ–çFõö—FW"‚¢æÖ‡Â‡F–6²Â—F6‚—ÂÖ–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6‚À¢F–6²À¢GW&F–öã¢#À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢Ò¢æ6öÆÆV7B‚’À¢Ò¢æW‡V7B‚'FW7BÔ”D’æ÷FW26†÷VÆB&R–ç6W'FVB"“°¢ÆWB6öçG&öÆÆW"ÒÖ–F”6öçG&öÆÆW$FF°¢6öçG&öÆÆW#¢À¢6†ææVÃ¢À¢F–6³¢SÀ¢fÇVS¢cBÀ¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F”6öçG&öÆÆW'2°¢—FVÕö–BÀ¢6öçG&öÆÆW'3¢fV2¶6öçG&öÆÆW%ÒÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’6öçG&öÆÆW"6†÷VÆB&R–ç6W'FVB"“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRæ–æÆ–æUöÖ–F•ö—FV×2æ–ç6W'B†—FVÕö–B“°¢ÆWBæ÷FUö–G2Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Ğ¢ææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂ†æ÷FRç—F6‚‚’Âæ÷FRæ–B‚’’¢æ6öÆÆV7C££Ä†6„ÖÅòÂóãâ‚“°¢ÆWBf—'7Eö–BÒæ÷FUö–G5²ccÓ°¢ÆWB6V6öæEö–BÒæ÷FUö–G5²ccEÓ°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F”VF—F÷"†—FVÕö–B’“°¢æÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ–ç6W'B†6öçG&öÆÆW"“°¢ÆWB&ö¦V7E÷6æ6†÷BÒç&ö¦V7Bç6æ6†÷B‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¥6VÆV7DÖ–F”æ÷FW2„†6…6WC£¦g&öÒ…¶f—'7Eö–EÒ’’“°¢76W'B†æÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ—5öV×G’‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6VÆV7DÖ–F”æ÷FW2„†6…6WC£¦g&öÒ…°¢f—'7Eö–BÂ6V6öæEö–BÀ¢Ò’’“°¢ÆWB6VÆV7FVEö&÷F‚Ò†6…6WC£¦g&öÒ…¶f—'7Eö–BÂ6V6öæEö–EÒ“°¢76W'EöW†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Â6VÆV7FVEö&÷F‚“°¢76W'EöW€¢çF–ÖVÆ–æRæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öå²f—FVÕö–EÒÀ¢6VÆV7FVEö&÷F€¢“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&ö¦V7E÷6æ6†÷B“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Â†6…6WC£¦g&öÒ…¶f—'7Eö–EÒ’“°¢76W'B†æÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ—5öV×G’‚’“°¢76W'EöW€¢çF–ÖVÆ–æRæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öå²f—FVÕö–EÒÀ¢†6…6WC£¦g&öÒ…¶f—'7Eö–EÒ¢“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&ö¦V7E÷6æ6†÷B“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ—5öV×G’‚’“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2À¢†6…6WC£¦g&öÒ…¶6öçG&öÆÆW%Ò¢“°¢76W'B€¢çF–ÖVÆ–æP¢æ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öà¢æ6öçF–ç5ö¶W’‚f—FVÕö–B¢“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&ö¦V7E÷6æ6†÷B“° ¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Â†6…6WC£¦g&öÒ…¶f—'7Eö–EÒ’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Â6VÆV7FVEö&÷F‚“°¢76W'EöW€¢çF–ÖVÆ–æRæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öå²f—FVÕö–EÒÀ¢6VÆV7FVEö&÷F€¢“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&ö¦V7E÷6æ6†÷B“°§Ğ ¢5·FW7EĞ¦fâWfVçEöÆ—7E÷&÷u÷6VÆV7F–öå÷VæFöW5ö7&÷75öWfVçE÷G—W5÷v—F†÷WE÷&ö¦V7EöVF—G2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2´Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢#À¢GW&F–öã¢#CÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÕÒÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’æ÷FR6†÷VÆB&R–ç6W'FVB"“°¢ÆWBæ÷FUö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Òæ–B‚“°¢ÆWB6öçG&öÆÆW"ÒÖ–F”6öçG&öÆÆW$FF°¢6öçG&öÆÆW#¢rÀ¢6†ææVÃ¢À¢F–6³¢3cÀ¢fÇVS¢ƒÀ¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F”6öçG&öÆÆW'2°¢—FVÕö–BÀ¢6öçG&öÆÆW'3¢fV2¶6öçG&öÆÆW%ÒÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’6öçG&öÆÆW"6†÷VÆB&R–ç6W'FVB"“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F”VF—F÷"†—FVÕö–B’“°¢æÖ–F•öVF—F÷%öÖöFRÒÖ–F”VF—F÷$ÖöFS£¤WfVçDÆ—7C°¢ÆWB&ö¦V7E÷6æ6†÷BÒç&ö¦V7Bç6æ6†÷B‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¥6VÆV7DÖ–F”VF—F÷$WfVçE6VÆV7F–öâ€¢7WW#£¤Ö–F”VF—F÷$WfVçDÆ—7D7W'6÷#£¤æ÷FR†æ÷FUö–B’À¢fÇ6RÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6VÆV7DÖ–F”VF—F÷$WfVçE6VÆV7F–öâ€¢7WW#£¤Ö–F”VF—F÷$WfVçDÆ—7D7W'6÷#£¤6öçG&öÆÆW"†6öçG&öÆÆW"’À¢G'VRÀ¢’“°¢76W'EöW†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Â†6…6WC£¦g&öÒ…¶æ÷FUö–EÒ’“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2À¢†6…6WC£¦g&öÒ…¶6öçG&öÆÆW%Ò¢“°¢76W'EöW€¢æÖ–F•öVF—F÷%öWfVçEöÆ—7Eö7W'6÷"À¢6öÖR‡7WW#£¤Ö–F”VF—F÷$WfVçDÆ—7D7W'6÷#£¤6öçG&öÆÆW"†6öçG&öÆÆW"’¢“°¢76W'EöW€¢çF–ÖVÆ–æRæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öå²f—FVÕö–EÒÀ¢†6…6WC£¦g&öÒ…¶æ÷FUö–EÒ¢“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&ö¦V7E÷6æ6†÷B“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Â†6…6WC£¦g&öÒ…¶æ÷FUö–EÒ’“°¢76W'B†æÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ—5öV×G’‚’“°¢76W'EöW€¢æÖ–F•öVF—F÷%öWfVçEöÆ—7Eö7W'6÷"À¢6öÖR‡7WW#£¤Ö–F”VF—F÷$WfVçDÆ—7D7W'6÷#£¤æ÷FR†æ÷FUö–B’¢“°¢76W'EöW€¢çF–ÖVÆ–æRæ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öå²f—FVÕö–EÒÀ¢†6…6WC£¦g&öÒ…¶æ÷FUö–EÒ¢“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&ö¦V7E÷6æ6†÷B“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ—5öV×G’‚’“°¢76W'B†æÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2æ—5öV×G’‚’“°¢76W'B†æÖ–F•öVF—F÷%öWfVçEöÆ—7Eö7W'6÷"æ—5öæöæR‚’“°¢76W'B€¢çF–ÖVÆ–æP¢æ–æÆ–æUöÖ–F•öæ÷FU÷6VÆV7F–öà¢æ6öçF–ç5ö¶W’‚f—FVÕö–B¢“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&ö¦V7E÷6æ6†÷B“° ¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Â†6…6WC£¦g&öÒ…¶æ÷FUö–EÒ’“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2À¢†6…6WC£¦g&öÒ…¶6öçG&öÆÆW%Ò¢“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&ö¦V7E÷6æ6†÷B“°§Ğ ¢5·FW7EĞ¦fâÖ–F•÷VçF—¦UöF–Æöuöf—…ö÷fW&Æ5öFVfVÇG5ööåöæEö6åö&UöF—6&ÆVB‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢EóÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2°¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢À¢GW&F–öã¢#À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢ƒÀ¢GW&F–öã¢#À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢33À¢GW&F–öã¢#À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢ÒÀ¢Ò¢æW‡V7B‚&÷fW&Æ–æræ÷FW26†÷VÆB&R–ç6W'FVB"“° ¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F•VçF—¦TF–Æör†—FVÕö–B’“°¢76W'B€¢æÖ–F•÷VçF—¦UöF–Æöp¢æW‡V7B‚&F–Æör6†÷VÆB÷Vâ"¢æf—…ö÷fW&Æ0¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦U66÷R„Ö–F•VçF—¦U66÷S£¤ÆÄæ÷FW2’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦T÷W&F–öâ€¢Ö–F•VçF—¦T÷W&F–öã£¥÷6—F–öäæDæ÷FTÆVæwF‚À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤66WDÖ–F•VçF—¦R“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Ğ¢ææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂ†æ÷FRçF–6²‚’Âæ÷FRæGW&F–öâ‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚’À¢²ƒ#Â#’Âƒ#CÂ#’Âƒ3cÂ#•Ğ¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Ğ¢ææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂ†æ÷FRçF–6²‚’Âæ÷FRæGW&F–öâ‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚’À¢²ƒÂ#’ÂƒƒÂ#’Âƒ33Â#•Ğ¢“° ¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F•VçF—¦TF–Æör†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦U66÷R„Ö–F•VçF—¦U66÷S£¤ÆÄæ÷FW2’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦T÷W&F–öâ€¢Ö–F•VçF—¦T÷W&F–öã£¥÷6—F–öäæDæ÷FTÆVæwF‚À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦Tf—„÷fW&Æ2†fÇ6R’“°¢ÆWBòÒçWFFR„ÖW76vS£¤66WDÖ–F•VçF—¦R“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Ğ¢ææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂ†æ÷FRçF–6²‚’Âæ÷FRæGW&F–öâ‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚’À¢²ƒ#Â#C’Âƒ#CÂ#’Âƒ3cÂ#•Ğ¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Ğ¢ææ÷FW2‚¢æ—FW"‚¢ç¦—…²ƒÂ#’ÂƒƒÂ#’Âƒ33Â#•Ò¢æÆÂ‡Â†æ÷FRÂW‡V7FVB—Â†æ÷FRçF–6²‚’Âæ÷FRæGW&F–öâ‚’’ÓÒW‡V7FVB¢“°§Ğ ¢5·FW7EĞ¦fâÖ–F•÷VçF—¦UöF–Æöuö¶VW5÷6VÆV7FVEöæ÷FUööfe÷v—F…ö—G5÷VçF—¦VEöæ÷FR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢EóÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2°¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢À¢GW&F–öã¢#À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢ƒÀ¢GW&F–öã¢#À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢ÒÀ¢Ò¢æW‡V7B‚&÷fW&Æ–æræ÷FW26†÷VÆB&R–ç6W'FVB"“°¢ÆWB6VÆV7FVEöæ÷FUö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Òæ–B‚“°¢æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Ò†6…6WC£¦g&öÒ…·6VÆV7FVEöæ÷FUö–EÒ“° ¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F•VçF—¦TF–Æör†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦T÷W&F–öâ€¢Ö–F•VçF—¦T÷W&F–öã£¥÷6—F–öäæDæ÷FTÆVæwF‚À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦Tf—„÷fW&Æ2†fÇ6R’“°¢ÆWBòÒçWFFR„ÖW76vS£¤66WDÖ–F•VçF—¦R“° ¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Ğ¢ææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂ†æ÷FRçF–6²‚’Âæ÷FRæGW&F–öâ‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚’À¢²ƒÂ#’Âƒ#CÂ#•Ğ¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Ğ¢ææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂ†æ÷FRçF–6²‚’Âæ÷FRæGW&F–öâ‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚’À¢²ƒÂ#’ÂƒƒÂ#•Ğ¢“°§Ğ ¢5·FW7EĞ¦fâÖ–F•÷VçF—¦UöF–Æöu÷VçF—¦W5ö×VÇF—ÆU÷6VÆV7FVEöæ÷FW5÷F‡&÷Vv…ö÷fW&Æö6†–â‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢EóÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2°¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢À¢GW&F–öã¢#À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢ƒÀ¢GW&F–öã¢#À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢33À¢GW&F–öã¢SÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢CÀ¢GW&F–öã¢3À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢S3À¢GW&F–öã¢#À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢c#À¢GW&F–öã¢CÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢ƒÀ¢GW&F–öã¢ƒÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢ÒÀ¢Ò¢æW‡V7B‚&÷fW&Æ–æræ÷FW26†÷VÆB&R–ç6W'FVB"“°¢ÆWBæ÷FW2Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚“°¢æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Ò†6…6WC£¦g&öÒ…¶æ÷FW5³Òæ–B‚’Âæ÷FW5³5Òæ–B‚•Ò“° ¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F•VçF—¦TF–Æör†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦T÷W&F–öâ€¢Ö–F•VçF—¦T÷W&F–öã£¥÷6—F–öäæDæ÷FTÆVæwF‚À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦Tf—„÷fW&Æ2†fÇ6R’“°¢ÆWBòÒçWFFR„ÖW76vS£¤66WDÖ–F•VçF—¦R“° ¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Ğ¢ææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂ†æ÷FRçF–6²‚’Âæ÷FRæGW&F–öâ‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚’À¢°¢ƒÂ#’À¢ƒ#CÂ#’À¢ƒ33ÂS’À¢ƒ3cÂ3c’À¢ƒS3Â#’À¢ƒc#ÂC’À¢ƒƒÂƒ’À¢Ğ¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Ğ¢ææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂ†æ÷FRçF–6²‚’Âæ÷FRæGW&F–öâ‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚’À¢°¢ƒÂ#’À¢ƒƒÂ#’À¢ƒ33ÂS’À¢ƒCÂ3’À¢ƒS3Â#’À¢ƒc#ÂC’À¢ƒƒÂƒ’À¢Ğ¢“°§Ğ ¢5·FW7EĞ¦fâÖ–F•÷VçF—¦UöF–Æöu÷W6W5÷F†Uö7F—fU÷7v–æuöw&–Eöf÷%öæ÷FW5öæEöÆÅöWfVçG2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢EóÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2°¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢sÀ¢GW&F–öã¢“À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢c"À¢F–6³¢3SÀ¢GW&F–öã¢ƒÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢ÒÀ¢Ò¢æW‡V7B‚'FW7Bæ÷FW26†÷VÆB&R–ç6W'FVB"“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F”6öçG&öÆÆW'2°¢—FVÕö–BÀ¢6öçG&öÆÆW'3¢fV2´Ö–F”6öçG&öÆÆW$FF°¢6öçG&öÆÆW#¢À¢6†ææVÃ¢À¢F–6³¢CsRÀ¢fÇVS¢cBÀ¢ÕÒÀ¢Ò¢æW‡V7B‚'FW7B6öçG&öÆÆW"6†÷VÆB&R–ç6W'FVB"“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F•—F6„&VæG2°¢—FVÕö–BÀ¢—F6…ö&VæG3¢fV2´Ö–F•—F6„&VæDFF°¢6†ææVÃ¢À¢F–6³¢ƒ3RÀ¢fÇVS¢…óÀ¢ÕÒÀ¢Ò¢æW‡V7B‚'FW7B—F6‚&VæB6†÷VÆB&R–ç6W'FVB"“°¢ÆWB6VÆV7FVEö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Òæ–B‚“°¢æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Ò†6…6WC£¦g&öÒ…·6VÆV7FVEö–EÒ“°¢ÆWBòÒçWFFR„ÖW76vS£¥6æ7v–ætw&–D6†ævVB‡G'VR’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6æ7v–ætÖ÷VçD6†ævVBƒS’“° ¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F•VçF—¦TF–Æör†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¤66WDÖ–F•VçF—¦R“°¢ÆWB—FVÒÒfç&ö¦V7BæÖ–F•ö—FV×2‚•³Ó°¢76W'EöW†—FVÒææ÷FW2‚•³ÒçF–6²‚’ÂS“°¢76W'EöW†—FVÒææ÷FW2‚•³ÒçF–6²‚’Â3S“°¢76W'EöW†—FVÒæ6öçG&öÆÆW'2‚•³ÒçF–6²ÂCsR“°¢76W'EöW†—FVÒç—F6…ö&VæG2‚•³ÒçF–6²Âƒ3R“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“° ¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F•VçF—¦TF–Æör†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦U66÷R„Ö–F•VçF—¦U66÷S£¤ÆÄWfVçG2’“°¢ÆWBòÒçWFFR„ÖW76vS£¤66WDÖ–F•VçF—¦R“°¢ÆWB—FVÒÒfç&ö¦V7BæÖ–F•ö—FV×2‚•³Ó°¢76W'EöW€¢—FVÒææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂ†æ÷FRçF–6²‚’Âæ÷FRæGW&F–öâ‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚’À¢²ƒSÂ“’Âƒ3“Âƒ•Ğ¢“°¢76W'EöW†—FVÒæ6öçG&öÆÆW'2‚•³ÒçF–6²ÂCƒ“°¢76W'EöW†—FVÒç—F6…ö&VæG2‚•³ÒçF–6²Âƒs“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢ÆWB—FVÒÒfç&ö¦V7BæÖ–F•ö—FV×2‚•³Ó°¢76W'EöW†—FVÒææ÷FW2‚•³ÒçF–6²‚’Âs“°¢76W'EöW†—FVÒææ÷FW2‚•³ÒçF–6²‚’Â3S“°¢76W'EöW†—FVÒæ6öçG&öÆÆW'2‚•³ÒçF–6²ÂCsR“°¢76W'EöW†—FVÒç—F6…ö&VæG2‚•³ÒçF–6²Âƒ3R“°§Ğ ¢5·FW7EĞ¦fâÖ–F•÷VçF—¦UöF–ÆöuöÆ–W5öæ÷FUöÆVæwF…ööæÇ•÷Fõ÷6VÆV7FVEöæ÷FW2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢5óƒCÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2°¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢À¢GW&F–öã¢ƒÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢"À¢—F6ƒ¢cBÀ¢F–6³¢3SÀ¢GW&F–öã¢ƒÀ¢fVÆö6—G“¢À¢öfe÷fVÆö6—G“¢À¢ÒÀ¢ÒÀ¢Ò¢æW‡V7B‚'FW7Bæ÷FW26†÷VÆB&R–ç6W'FVB"“°¢ÆWB6VÆV7FVEö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Òæ–B‚“°¢æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Ò†6…6WC£¦g&öÒ…·6VÆV7FVEö–EÒ“° ¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F•VçF—¦TF–Æör†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦T÷W&F–öâ€¢Ö–F•VçF—¦T÷W&F–öã£¤æ÷FTÆVæwF„öæÇ’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤66WDÖ–F•VçF—¦R“° ¢76W'B†æÖ–F•÷VçF—¦UöF–Æöræ—5öæöæR‚’“°¢ÆWBæ÷FW2Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚“°¢76W'EöW‚†æ÷FW5³ÒçF–6²‚’Âæ÷FW5³ÒæGW&F–öâ‚’’ÂƒÂ#C’“°¢76W'EöW‚†æ÷FW5³ÒçF–6²‚’Âæ÷FW5³ÒæGW&F–öâ‚’’Âƒ3SÂƒ’“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒæGW&F–öâ‚’Âƒ“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒæGW&F–öâ‚’Â#C“°§Ğ ¢5·FW7EĞ¦fâÖ–F•÷VçF—¦UöF–Æöu÷&÷WFW5÷÷6—F–öåöæEöæ÷FUöVæEööæÇ•ö÷W&F–öç2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢5óƒCÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2°¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢À¢GW&F–öã¢ƒÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢"À¢—F6ƒ¢cBÀ¢F–6³¢3SÀ¢GW&F–öã¢ƒÀ¢fVÆö6—G“¢À¢öfe÷fVÆö6—G“¢À¢ÒÀ¢ÒÀ¢Ò¢æW‡V7B‚'FW7Bæ÷FW26†÷VÆB&R–ç6W'FVB"“°¢ÆWB6VÆV7FVEö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Òæ–B‚“°¢æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Ò†6…6WC£¦g&öÒ…·6VÆV7FVEö–EÒ“° ¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F•VçF—¦TF–Æör†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦T÷W&F–öâ€¢Ö–F•VçF—¦T÷W&F–öã£¥÷6—F–öäæDæ÷FTÆVæwF‚À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤66WDÖ–F•VçF—¦R“°¢76W'EöW€¢€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒçF–6²‚’À¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒæGW&F–öâ‚’À¢’À¢ƒ#Â#C¢“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒçF–6²‚’Â3S“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“° ¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F•VçF—¦TF–Æör†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦T÷W&F–öâ€¢Ö–F•VçF—¦T÷W&F–öã£¤æ÷FTVæDöæÇ’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤66WDÖ–F•VçF—¦R“°¢76W'EöW€¢€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒçF–6²‚’À¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒæGW&F–öâ‚’À¢’À¢ƒÂC¢“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒçF–6²‚’Â3S“°§Ğ ¢5·FW7EĞ¦fâÖ–F•÷VçF—¦UöF–ÆöuöÆÅöWfVçG5öÖF6†W5öÖ–F•ö6öÆÆ—6–öå÷'VÆW5ö–åööæUöVF—B‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢EóÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2°¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢À¢GW&F–öã¢ƒÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢c"À¢F–6³¢3SÀ¢GW&F–öã¢ƒÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢ÒÀ¢Ò¢æW‡V7B‚'FW7Bæ÷FW26†÷VÆB&R–ç6W'FVB"“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F”6öçG&öÆÆW'2°¢—FVÕö–BÀ¢6öçG&öÆÆW'3¢fV2°¢Ö–F”6öçG&öÆÆW$FF°¢6öçG&öÆÆW#¢À¢6†ææVÃ¢À¢F–6³¢CsRÀ¢fÇVS¢cBÀ¢ÒÀ¢Ö–F”6öçG&öÆÆW$FF°¢6öçG&öÆÆW#¢À¢6†ææVÃ¢À¢F–6³¢CƒRÀ¢fÇVS¢cRÀ¢ÒÀ¢ÒÀ¢Ò¢æW‡V7B‚'FW7B6öçG&öÆÆW"6†÷VÆB&R–ç6W'FVB"“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F•—F6„&VæG2°¢—FVÕö–BÀ¢—F6…ö&VæG3¢fV2°¢Ö–F•—F6„&VæDFF°¢6†ææVÃ¢À¢F–6³¢ƒ3RÀ¢fÇVS¢…óÀ¢ÒÀ¢Ö–F•—F6„&VæDFF°¢6†ææVÃ¢À¢F–6³¢ƒSÀ¢fÇVS¢…ó“"À¢ÒÀ¢ÒÀ¢Ò¢æW‡V7B‚'FW7B—F6‚&VæB6†÷VÆB&R–ç6W'FVB"“° ¢ÆWB6VÆV7FVE÷&öw&Õö6†ævRÒÖ–F•&öw&Ô6†ævTFF°¢6†ææVÃ¢À¢F–6³¢CsRÀ¢&öw&Ó¢"À¢Ó°¢ÆWB6öÆÆ–F–æu÷Vç6VÆV7FVE÷&öw&Õö6†ævRÒÖ–F•&öw&Ô6†ævTFF°¢F–6³¢CƒÀ¢&öw&Ó¢2À¢âç6VÆV7FVE÷&öw&Õö6†ævP¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F•&öw&Ô6†ævW2°¢—FVÕö–BÀ¢&öw&Õö6†ævW3¢fV2·6VÆV7FVE÷&öw&Õö6†ævRÂ6öÆÆ–F–æu÷Vç6VÆV7FVE÷&öw&Õö6†ævUÒÀ¢Ò¢æW‡V7B‚'FW7B&öw&Ò6†ævW26†÷VÆB&R–ç6W'FVB"“°¢ÆWB6VÆV7FVEö6†ææVÅ÷&W77W&RÒÖ–F”6†ææVÅ&W77W&TFF°¢6†ææVÃ¢À¢F–6³¢CsRÀ¢fÇVS¢cÀ¢Ó°¢ÆWB6öÆÆ–F–æu÷Vç6VÆV7FVEö6†ææVÅ÷&W77W&RÒÖ–F”6†ææVÅ&W77W&TFF°¢F–6³¢CƒÀ¢fÇVS¢sÀ¢âç6VÆV7FVEö6†ææVÅ÷&W77W&P¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F”6†ææVÅ&W77W&W2°¢—FVÕö–BÀ¢6†ææVÅ÷&W77W&W3¢fV2°¢6VÆV7FVEö6†ææVÅ÷&W77W&RÀ¢6öÆÆ–F–æu÷Vç6VÆV7FVEö6†ææVÅ÷&W77W&RÀ¢ÒÀ¢Ò¢æW‡V7B‚'FW7B6†ææVÂ&W77W&RWfVçG26†÷VÆB&R–ç6W'FVB"“°¢ÆWB6VÆV7FVE÷öÇ•ögFW'F÷V6‚ÒÖ–F•öÇ”gFW'F÷V6„FF°¢6†ææVÃ¢À¢F–6³¢3SRÀ¢æ÷FS¢cÀ¢fÇVS¢ƒÀ¢Ó°¢ÆWB6öÆÆ–F–æu÷Vç6VÆV7FVE÷öÇ•ögFW'F÷V6‚ÒÖ–F•öÇ”gFW'F÷V6„FF°¢F–6³¢3cÀ¢fÇVS¢“À¢âç6VÆV7FVE÷öÇ•ögFW'F÷V6€¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F•öÇ”gFW'F÷V6†W2°¢—FVÕö–BÀ¢öÇ•ögFW'F÷V6†W3¢fV2°¢6VÆV7FVE÷öÇ•ögFW'F÷V6‚À¢6öÆÆ–F–æu÷Vç6VÆV7FVE÷öÇ•ögFW'F÷V6‚À¢ÒÀ¢Ò¢æW‡V7B‚'FW7BöÇ’gFW'F÷V6‚WfVçG26†÷VÆB&R–ç6W'FVB"“°¢æÖ–F•öVF—F÷%ö—FVÕö–BÒ6öÖR†—FVÕö–B“°¢æÖ–F•öVF—F÷%öÖöFRÒÖ–F”VF—F÷$ÖöFS£¤WfVçDÆ—7C°¢ÆWBòÒçWFFR„ÖW76vS£¥6VÆV7DÖ–F•&öw&Ô6†ævTWfVçB€¢—FVÕö–BÀ¢6VÆV7FVE÷&öw&Õö6†ævRÀ¢fÇ6RÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6VÆV7DÖ–F”6†ææVÅ&W77W&TWfVçB€¢—FVÕö–BÀ¢6VÆV7FVEö6†ææVÅ÷&W77W&RÀ¢G'VRÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6VÆV7DÖ–F•öÇ”gFW'F÷V6„WfVçB€¢—FVÕö–BÀ¢6VÆV7FVE÷öÇ•ögFW'F÷V6‚À¢G'VRÀ¢’“° ¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F•VçF—¦TF–Æör†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦U66÷R„Ö–F•VçF—¦U66÷S£¤ÆÄWfVçG2’“°¢ÆWBòÒçWFFR„ÖW76vS£¤66WDÖ–F•VçF—¦R“° ¢ÆWB—FVÒÒfç&ö¦V7BæÖ–F•ö—FV×2‚•³Ó°¢76W'EöW€¢—FVÒææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂ†æ÷FRçF–6²‚’Âæ÷FRæGW&F–öâ‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚’À¢²ƒ#Âƒ’Âƒ3cÂƒ•Ğ¢“°¢76W'EöW†—FVÒæ6öçG&öÆÆW'2‚’æÆVâ‚’Â“°¢76W'EöW†—FVÒæ6öçG&öÆÆW'2‚•³ÒçF–6²ÂCƒ“°¢76W'EöW†—FVÒæ6öçG&öÆÆW'2‚•³ÒçfÇVRÂcR“°¢76W'EöW†—FVÒç—F6…ö&VæG2‚’æÆVâ‚’Â“°¢76W'EöW†—FVÒç—F6…ö&VæG2‚•³ÒçF–6²ÂƒC“°¢76W'EöW†—FVÒç—F6…ö&VæG2‚•³ÒçfÇVRÂ…ó“"“°¢76W'EöW€¢—FVÒç&öw&Õö6†ævW2‚’À¢´Ö–F•&öw&Ô6†ævTFF°¢F–6³¢CƒÀ¢âç6VÆV7FVE÷&öw&Õö6†ævP¢ÕĞ¢“°¢76W'EöW€¢—FVÒæ6†ææVÅ÷&W77W&W2‚’À¢´Ö–F”6†ææVÅ&W77W&TFF°¢F–6³¢CƒÀ¢âç6VÆV7FVEö6†ææVÅ÷&W77W&P¢ÕĞ¢“°¢76W'EöW€¢—FVÒçöÇ•ögFW'F÷V6†W2‚’À¢°¢Ö–F•öÇ”gFW'F÷V6„FF°¢F–6³¢3cÀ¢âç6VÆV7FVE÷öÇ•ögFW'F÷V6€¢ÒÀ¢Ö–F•öÇ”gFW'F÷V6„FF°¢F–6³¢3cÀ¢âæ6öÆÆ–F–æu÷Vç6VÆV7FVE÷öÇ•ögFW'F÷V6€¢ÒÀ¢Ğ¢“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2À¢†6…6WC£¦g&öÒ…´Ö–F•&öw&Ô6†ævTFF°¢F–6³¢CƒÀ¢âç6VÆV7FVE÷&öw&Õö6†ævP¢ÕÒ¢“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W2À¢†6…6WC£¦g&öÒ…´Ö–F”6†ææVÅ&W77W&TFF°¢F–6³¢CƒÀ¢âç6VÆV7FVEö6†ææVÅ÷&W77W&P¢ÕÒ¢“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2À¢†6…6WC£¦g&öÒ…´Ö–F•öÇ”gFW'F÷V6„FF°¢F–6³¢3cÀ¢âç6VÆV7FVE÷öÇ•ögFW'F÷V6€¢ÕÒ¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢ÆWB—FVÒÒfç&ö¦V7BæÖ–F•ö—FV×2‚•³Ó°¢76W'EöW†—FVÒææ÷FW2‚•³ÒçF–6²‚’Â“°¢76W'EöW†—FVÒæ6öçG&öÆÆW'2‚’æÆVâ‚’Â"“°¢76W'EöW†—FVÒæ6öçG&öÆÆW'2‚•³ÒçF–6²ÂCsR“°¢76W'EöW†—FVÒæ6öçG&öÆÆW'2‚•³ÒçF–6²ÂCƒR“°¢76W'EöW†—FVÒç—F6…ö&VæG2‚’æÆVâ‚’Â"“°¢76W'EöW†—FVÒç—F6…ö&VæG2‚•³ÒçF–6²Âƒ3R“°¢76W'EöW†—FVÒç—F6…ö&VæG2‚•³ÒçF–6²ÂƒS“°¢76W'EöW€¢—FVÒç&öw&Õö6†ævW2‚’À¢·6VÆV7FVE÷&öw&Õö6†ævRÂ6öÆÆ–F–æu÷Vç6VÆV7FVE÷&öw&Õö6†ævUĞ¢“°¢76W'EöW€¢—FVÒæ6†ææVÅ÷&W77W&W2‚’À¢°¢6VÆV7FVEö6†ææVÅ÷&W77W&RÀ¢6öÆÆ–F–æu÷Vç6VÆV7FVEö6†ææVÅ÷&W77W&P¢Ğ¢“°¢76W'EöW€¢—FVÒçöÇ•ögFW'F÷V6†W2‚’À¢°¢6VÆV7FVE÷öÇ•ögFW'F÷V6‚À¢6öÆÆ–F–æu÷Vç6VÆV7FVE÷öÇ•ögFW'F÷V6€¢Ğ¢“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2À¢†6…6WC£¦g&öÒ…·6VÆV7FVE÷&öw&Õö6†ævUÒ¢“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W2À¢†6…6WC£¦g&öÒ…·6VÆV7FVEö6†ææVÅ÷&W77W&UÒ¢“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2À¢†6…6WC£¦g&öÒ…·6VÆV7FVE÷öÇ•ögFW'F÷V6…Ò¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢ÆWB—FVÒÒfç&ö¦V7BæÖ–F•ö—FV×2‚•³Ó°¢76W'EöW†—FVÒææ÷FW2‚•³ÒçF–6²‚’Â#“°¢76W'EöW†—FVÒæ6öçG&öÆÆW'2‚’æÆVâ‚’Â“°¢76W'EöW†—FVÒæ6öçG&öÆÆW'2‚•³ÒçF–6²ÂCƒ“°¢76W'EöW†—FVÒç—F6…ö&VæG2‚’æÆVâ‚’Â“°¢76W'EöW†—FVÒç—F6…ö&VæG2‚•³ÒçF–6²ÂƒC“°¢76W'EöW€¢—FVÒç&öw&Õö6†ævW2‚’À¢´Ö–F•&öw&Ô6†ævTFF°¢F–6³¢CƒÀ¢âç6VÆV7FVE÷&öw&Õö6†ævP¢ÕĞ¢“°¢76W'EöW€¢—FVÒæ6†ææVÅ÷&W77W&W2‚’À¢´Ö–F”6†ææVÅ&W77W&TFF°¢F–6³¢CƒÀ¢âç6VÆV7FVEö6†ææVÅ÷&W77W&P¢ÕĞ¢“°¢76W'EöW€¢—FVÒçöÇ•ögFW'F÷V6†W2‚’À¢°¢Ö–F•öÇ”gFW'F÷V6„FF°¢F–6³¢3cÀ¢âç6VÆV7FVE÷öÇ•ögFW'F÷V6€¢ÒÀ¢Ö–F•öÇ”gFW'F÷V6„FF°¢F–6³¢3cÀ¢âæ6öÆÆ–F–æu÷Vç6VÆV7FVE÷öÇ•ögFW'F÷V6€¢ÒÀ¢Ğ¢“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2À¢†6…6WC£¦g&öÒ…´Ö–F•&öw&Ô6†ævTFF°¢F–6³¢CƒÀ¢âç6VÆV7FVE÷&öw&Õö6†ævP¢ÕÒ¢“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W2À¢†6…6WC£¦g&öÒ…´Ö–F”6†ææVÅ&W77W&TFF°¢F–6³¢CƒÀ¢âç6VÆV7FVEö6†ææVÅ÷&W77W&P¢ÕÒ¢“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2À¢†6…6WC£¦g&öÒ…´Ö–F•öÇ”gFW'F÷V6„FF°¢F–6³¢3cÀ¢âç6VÆV7FVE÷öÇ•ögFW'F÷V6€¢ÕÒ¢“°§Ğ ¢5·FW7EĞ¦fâÖ–F•÷VçF—¦UöF–Æöu÷6VÆV7FVEöWfVçG5÷&W6W'fW5÷Vç6VÆV7FVEöWfVçG5öæE÷6VÆV7F–öâ‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢EóÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2°¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢sÀ¢GW&F–öã¢ƒÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢c"À¢F–6³¢3SÀ¢GW&F–öã¢ƒÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cBÀ¢F–6³¢SSÀ¢GW&F–öã¢ƒÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢ÒÀ¢Ò¢æW‡V7B‚'FW7Bæ÷FW26†÷VÆB&R–ç6W'FVB"“°¢ÆWB6VÆV7FVEöæ÷FUö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Òæ–B‚“°¢ÆWB6VÆV7FVEö6öçG&öÆÆW"ÒÖ–F”6öçG&öÆÆW$FF°¢6öçG&öÆÆW#¢À¢6†ææVÃ¢À¢F–6³¢CsRÀ¢fÇVS¢cBÀ¢Ó°¢ÆWB6öÆÆ–F–æu÷Vç6VÆV7FVEö6öçG&öÆÆW"ÒÖ–F”6öçG&öÆÆW$FF°¢F–6³¢CƒÀ¢fÇVS¢cRÀ¢âç6VÆV7FVEö6öçG&öÆÆW ¢Ó°¢ÆWBVçF÷V6†VEö6öçG&öÆÆW"ÒÖ–F”6öçG&öÆÆW$FF°¢6öçG&öÆÆW#¢rÀ¢6†ææVÃ¢À¢F–6³¢s3À¢fÇVS¢“bÀ¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F”6öçG&öÆÆW'2°¢—FVÕö–BÀ¢6öçG&öÆÆW'3¢fV2°¢6VÆV7FVEö6öçG&öÆÆW"À¢6öÆÆ–F–æu÷Vç6VÆV7FVEö6öçG&öÆÆW"À¢VçF÷V6†VEö6öçG&öÆÆW"À¢ÒÀ¢Ò¢æW‡V7B‚'FW7B6öçG&öÆÆW'26†÷VÆB&R–ç6W'FVB"“°¢ÆWB6VÆV7FVE÷—F6…ö&VæBÒÖ–F•—F6„&VæDFF°¢6†ææVÃ¢À¢F–6³¢ƒ3RÀ¢fÇVS¢…óÀ¢Ó°¢ÆWB6öÆÆ–F–æu÷Vç6VÆV7FVE÷—F6…ö&VæBÒÖ–F•—F6„&VæDFF°¢F–6³¢ƒCÀ¢fÇVS¢…ó“"À¢âç6VÆV7FVE÷—F6…ö&Væ@¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F•—F6„&VæG2°¢—FVÕö–BÀ¢—F6…ö&VæG3¢fV2·6VÆV7FVE÷—F6…ö&VæBÂ6öÆÆ–F–æu÷Vç6VÆV7FVE÷—F6…ö&VæEÒÀ¢Ò¢æW‡V7B‚'FW7B—F6‚&VæG26†÷VÆB&R–ç6W'FVB"“°¢ÆWB6VÆV7FVE÷&öw&Õö6†ævRÒÖ–F•&öw&Ô6†ævTFF°¢6†ææVÃ¢"À¢F–6³¢#3RÀ¢&öw&Ó¢"À¢Ó°¢ÆWB6öÆÆ–F–æu÷Vç6VÆV7FVE÷&öw&Õö6†ævRÒÖ–F•&öw&Ô6†ævTFF°¢F–6³¢#CÀ¢&öw&Ó¢2À¢âç6VÆV7FVE÷&öw&Õö6†ævP¢Ó°¢ÆWBVçF÷V6†VE÷&öw&Õö6†ævRÒÖ–F•&öw&Ô6†ævTFF°¢6†ææVÃ¢2À¢F–6³¢s3À¢&öw&Ó¢BÀ¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F•&öw&Ô6†ævW2°¢—FVÕö–BÀ¢&öw&Õö6†ævW3¢fV2°¢6VÆV7FVE÷&öw&Õö6†ævRÀ¢6öÆÆ–F–æu÷Vç6VÆV7FVE÷&öw&Õö6†ævRÀ¢VçF÷V6†VE÷&öw&Õö6†ævRÀ¢ÒÀ¢Ò¢æW‡V7B‚'FW7B&öw&Ò6†ævW26†÷VÆB&R–ç6W'FVB"“°¢ÆWB6VÆV7FVE÷öÇ•ögFW'F÷V6‚ÒÖ–F•öÇ”gFW'F÷V6„FF°¢6†ææVÃ¢BÀ¢F–6³¢3SRÀ¢æ÷FS¢cÀ¢fÇVS¢ƒÀ¢Ó°¢ÆWB6öÆÆ–F–æu÷Vç6VÆV7FVE÷öÇ•ögFW'F÷V6‚ÒÖ–F•öÇ”gFW'F÷V6„FF°¢F–6³¢3cÀ¢fÇVS¢“À¢âç6VÆV7FVE÷öÇ•ögFW'F÷V6€¢Ó°¢ÆWBVçF÷V6†VE÷öÇ•ögFW'F÷V6‚ÒÖ–F•öÇ”gFW'F÷V6„FF°¢6†ææVÃ¢RÀ¢F–6³¢s3À¢æ÷FS¢cBÀ¢fÇVS¢sÀ¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F•öÇ”gFW'F÷V6†W2°¢—FVÕö–BÀ¢öÇ•ögFW'F÷V6†W3¢fV2°¢6VÆV7FVE÷öÇ•ögFW'F÷V6‚À¢6öÆÆ–F–æu÷Vç6VÆV7FVE÷öÇ•ögFW'F÷V6‚À¢VçF÷V6†VE÷öÇ•ögFW'F÷V6‚À¢ÒÀ¢Ò¢æW‡V7B‚'FW7BöÇ’gFW'F÷V6‚WfVçG26†÷VÆB&R–ç6W'FVB"“°¢ÆWB6VÆV7FVEö6†ææVÅ÷&W77W&RÒÖ–F”6†ææVÅ&W77W&TFF°¢6†ææVÃ¢bÀ¢F–6³¢CsRÀ¢fÇVS¢cÀ¢Ó°¢ÆWB6öÆÆ–F–æu÷Vç6VÆV7FVEö6†ææVÅ÷&W77W&RÒÖ–F”6†ææVÅ&W77W&TFF°¢F–6³¢CƒÀ¢fÇVS¢sÀ¢âç6VÆV7FVEö6†ææVÅ÷&W77W&P¢Ó°¢ÆWBVçF÷V6†VEö6†ææVÅ÷&W77W&RÒÖ–F”6†ææVÅ&W77W&TFF°¢6†ææVÃ¢rÀ¢F–6³¢s3À¢fÇVS¢sRÀ¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F”6†ææVÅ&W77W&W2°¢—FVÕö–BÀ¢6†ææVÅ÷&W77W&W3¢fV2°¢6VÆV7FVEö6†ææVÅ÷&W77W&RÀ¢6öÆÆ–F–æu÷Vç6VÆV7FVEö6†ææVÅ÷&W77W&RÀ¢VçF÷V6†VEö6†ææVÅ÷&W77W&RÀ¢ÒÀ¢Ò¢æW‡V7B‚'FW7B6†ææVÂ&W77W&RWfVçG26†÷VÆB&R–ç6W'FVB"“°¢ÆWB6VÆV7FVEö÷VUöWfVçBÒÖ–F”÷VTWfVçDFF°¢F–6³¢CsRÀ¢÷&FW#¢#À¢¶–æC¢Ö–F”÷VTWfVçD¶–æC£¤ÖWF°¢¶–æC¢ƒÀ¢FF¢fV2³†fbÂƒÂ"w2uÒÀ¢ÒÀ¢Ó°¢ÆWBVç6VÆV7FVEö÷VUöWfVçBÒÖ–F”÷VTWfVçDFF°¢F–6³¢CƒÀ¢÷&FW#¢À¢¶–æC¢Ö–F”÷VTWfVçD¶–æC£¥7—7FVÔW†6ÇW6—fR°¢7FGW3¢„crÀ¢FF¢fV2³ƒC2Âƒ"Âƒ3EÒÀ¢ÒÀ¢Ó°¢ÆWB6V6öæE÷6VÆV7FVEö÷VUöWfVçBÒÖ–F”÷VTWfVçDFF°¢F–6³¢S“RÀ¢÷&FW#¢3À¢¶–æC¢Ö–F”÷VTWfVçD¶–æC£¥7—7FVÔW†6ÇW6—fR°¢7FGW3¢„cÀ¢FF¢fV2³ƒC2ÂƒSeÒÀ¢ÒÀ¢Ó°¢ÆWBVç6VÆV7FVEö6öÆÆ–F–æu÷7—6W…öWfVçBÒÖ–F”÷VTWfVçDFF°¢F–6³¢cÀ¢÷&FW#¢CÀ¢¶–æC¢Ö–F”÷VTWfVçD¶–æC£¥7—7FVÔW†6ÇW6—fR°¢7FGW3¢„cÀ¢FF¢fV2³ƒSRÂƒceÒÀ¢ÒÀ¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F”÷VTWfVçG2°¢—FVÕö–BÀ¢WfVçG3¢fV2°¢6VÆV7FVEö÷VUöWfVçBæ6ÆöæR‚’À¢Vç6VÆV7FVEö÷VUöWfVçBæ6ÆöæR‚’À¢6V6öæE÷6VÆV7FVEö÷VUöWfVçBæ6ÆöæR‚’À¢Vç6VÆV7FVEö6öÆÆ–F–æu÷7—6W…öWfVçBæ6ÆöæR‚’À¢ÒÀ¢Ò¢æW‡V7B‚'FW7B÷VRWfVçG26†÷VÆB&R–ç6W'FVB"“° ¢æÖ–F•öVF—F÷%ö—FVÕö–BÒ6öÖR†—FVÕö–B“°¢ÆWBòÒçWFFR„ÖW76vS£¥6VÆV7DÖ–F”æ÷FW2„†6…6WC£¦g&öÒ…·6VÆV7FVEöæ÷FUö–EÒ’’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6VÆV7DÖ–F”6öçG&öÆÆW$WfVçB€¢—FVÕö–BÀ¢6VÆV7FVEö6öçG&öÆÆW"À¢G'VRÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6VÆV7DÖ–F•—F6„&VæDWfVçB€¢—FVÕö–BÀ¢6VÆV7FVE÷—F6…ö&VæBÀ¢G'VRÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6VÆV7DÖ–F•&öw&Ô6†ævTWfVçB€¢—FVÕö–BÀ¢6VÆV7FVE÷&öw&Õö6†ævRÀ¢G'VRÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6VÆV7DÖ–F•öÇ”gFW'F÷V6„WfVçB€¢—FVÕö–BÀ¢6VÆV7FVE÷öÇ•ögFW'F÷V6‚À¢G'VRÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6VÆV7DÖ–F”6†ææVÅ&W77W&TWfVçB€¢—FVÕö–BÀ¢6VÆV7FVEö6†ææVÅ÷&W77W&RÀ¢G'VRÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6VÆV7DÖ–F”÷VTWfVçB†—FVÕö–BÂÂG'VR’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6VÆV7DÖ–F”÷VTWfVçB†—FVÕö–BÂ"ÂG'VR’“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F•VçF—¦TF–Æör†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦U66÷R€¢Ö–F•VçF—¦U66÷S£¥6VÆV7FVDWfVçG2À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤66WDÖ–F•VçF—¦R“° ¢ÆWB—FVÒÒfç&ö¦V7BæÖ–F•ö—FV×2‚•³Ó°¢76W'EöW€¢—FVÒææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂæ÷FRçF–6²‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚’À¢³#Â3SÂSSĞ¢“°¢76W'EöW€¢—FVÒæ6öçG&öÆÆW'2‚’À¢°¢Ö–F”6öçG&öÆÆW$FF°¢F–6³¢CƒÀ¢âç6VÆV7FVEö6öçG&öÆÆW ¢ÒÀ¢VçF÷V6†VEö6öçG&öÆÆW"À¢Ğ¢“°¢76W'EöW€¢—FVÒç—F6…ö&VæG2‚’À¢´Ö–F•—F6„&VæDFF°¢F–6³¢ƒCÀ¢âç6VÆV7FVE÷—F6…ö&Væ@¢ÕĞ¢“°¢76W'EöW€¢—FVÒç&öw&Õö6†ævW2‚’À¢°¢Ö–F•&öw&Ô6†ævTFF°¢F–6³¢#CÀ¢âç6VÆV7FVE÷&öw&Õö6†ævP¢ÒÀ¢VçF÷V6†VE÷&öw&Õö6†ævRÀ¢Ğ¢“°¢76W'EöW€¢—FVÒçöÇ•ögFW'F÷V6†W2‚’À¢°¢Ö–F•öÇ”gFW'F÷V6„FF°¢F–6³¢3cÀ¢âç6VÆV7FVE÷öÇ•ögFW'F÷V6€¢ÒÀ¢Ö–F•öÇ”gFW'F÷V6„FF°¢F–6³¢3cÀ¢âæ6öÆÆ–F–æu÷Vç6VÆV7FVE÷öÇ•ögFW'F÷V6€¢ÒÀ¢VçF÷V6†VE÷öÇ•ögFW'F÷V6‚À¢Ğ¢“°¢76W'EöW€¢—FVÒæ6†ææVÅ÷&W77W&W2‚’À¢°¢Ö–F”6†ææVÅ&W77W&TFF°¢F–6³¢CƒÀ¢âç6VÆV7FVEö6†ææVÅ÷&W77W&P¢ÒÀ¢VçF÷V6†VEö6†ææVÅ÷&W77W&RÀ¢Ğ¢“°¢76W'EöW€¢—FVÒæ÷VUöWfVçG2‚’À¢°¢Ö–F”÷VTWfVçDFF°¢F–6³¢CƒÀ¢âçVç6VÆV7FVEö÷VUöWfVçBæ6ÆöæR‚¢ÒÀ¢Ö–F”÷VTWfVçDFF°¢F–6³¢CƒÀ¢âç6VÆV7FVEö÷VUöWfVçBæ6ÆöæR‚¢ÒÀ¢Ö–F”÷VTWfVçDFF°¢F–6³¢cÀ¢âç6V6öæE÷6VÆV7FVEö÷VUöWfVçBæ6ÆöæR‚¢ÒÀ¢Vç6VÆV7FVEö6öÆÆ–F–æu÷7—6W…öWfVçBæ6ÆöæR‚’À¢Ğ¢“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVEö6öçG&öÆÆW'2À¢†6…6WC£¦g&öÒ…´Ö–F”6öçG&öÆÆW$FF°¢F–6³¢CƒÀ¢âç6VÆV7FVEö6öçG&öÆÆW ¢ÕÒ¢“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVE÷—F6…ö&VæG2À¢†6…6WC£¦g&öÒ…´Ö–F•—F6„&VæDFF°¢F–6³¢ƒCÀ¢âç6VÆV7FVE÷—F6…ö&Væ@¢ÕÒ¢“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVE÷&öw&Õö6†ævW2À¢†6…6WC£¦g&öÒ…´Ö–F•&öw&Ô6†ævTFF°¢F–6³¢#CÀ¢âç6VÆV7FVE÷&öw&Õö6†ævP¢ÕÒ¢“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVE÷öÇ•ögFW'F÷V6†W2À¢†6…6WC£¦g&öÒ…´Ö–F•öÇ”gFW'F÷V6„FF°¢F–6³¢3cÀ¢âç6VÆV7FVE÷öÇ•ögFW'F÷V6€¢ÕÒ¢“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVEö6†ææVÅ÷&W77W&W2À¢†6…6WC£¦g&öÒ…´Ö–F”6†ææVÅ&W77W&TFF°¢F–6³¢CƒÀ¢âç6VÆV7FVEö6†ææVÅ÷&W77W&P¢ÕÒ¢“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG2À¢†6…6WC£¦g&öÒ…³Â%Ò¢“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢ÆWB—FVÒÒfç&ö¦V7BæÖ–F•ö—FV×2‚•³Ó°¢76W'EöW†—FVÒææ÷FW2‚•³ÒçF–6²‚’Âs“°¢76W'EöW€¢—FVÒæ6öçG&öÆÆW'2‚’À¢°¢6VÆV7FVEö6öçG&öÆÆW"À¢6öÆÆ–F–æu÷Vç6VÆV7FVEö6öçG&öÆÆW"À¢VçF÷V6†VEö6öçG&öÆÆW"À¢Ğ¢“°¢76W'EöW€¢—FVÒç—F6…ö&VæG2‚’À¢·6VÆV7FVE÷—F6…ö&VæBÂ6öÆÆ–F–æu÷Vç6VÆV7FVE÷—F6…ö&VæEĞ¢“°¢76W'EöW€¢—FVÒç&öw&Õö6†ævW2‚’À¢°¢6VÆV7FVE÷&öw&Õö6†ævRÀ¢6öÆÆ–F–æu÷Vç6VÆV7FVE÷&öw&Õö6†ævRÀ¢VçF÷V6†VE÷&öw&Õö6†ævRÀ¢Ğ¢“°¢76W'EöW€¢—FVÒçöÇ•ögFW'F÷V6†W2‚’À¢°¢6VÆV7FVE÷öÇ•ögFW'F÷V6‚À¢6öÆÆ–F–æu÷Vç6VÆV7FVE÷öÇ•ögFW'F÷V6‚À¢VçF÷V6†VE÷öÇ•ögFW'F÷V6‚À¢Ğ¢“°¢76W'EöW€¢—FVÒæ6†ææVÅ÷&W77W&W2‚’À¢°¢6VÆV7FVEö6†ææVÅ÷&W77W&RÀ¢6öÆÆ–F–æu÷Vç6VÆV7FVEö6†ææVÅ÷&W77W&RÀ¢VçF÷V6†VEö6†ææVÅ÷&W77W&RÀ¢Ğ¢“°¢76W'EöW€¢—FVÒæ÷VUöWfVçG2‚’À¢°¢6VÆV7FVEö÷VUöWfVçBæ6ÆöæR‚’À¢Vç6VÆV7FVEö÷VUöWfVçBæ6ÆöæR‚’À¢6V6öæE÷6VÆV7FVEö÷VUöWfVçBæ6ÆöæR‚’À¢Vç6VÆV7FVEö6öÆÆ–F–æu÷7—6W…öWfVçBæ6ÆöæR‚’À¢Ğ¢“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG2À¢†6…6WC£¦g&öÒ…³Â%Ò¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ÷VUöWfVçG2‚’À¢°¢Ö–F”÷VTWfVçDFF°¢F–6³¢CƒÀ¢âçVç6VÆV7FVEö÷VUöWfVç@¢ÒÀ¢Ö–F”÷VTWfVçDFF°¢F–6³¢CƒÀ¢âç6VÆV7FVEö÷VUöWfVç@¢ÒÀ¢Ö–F”÷VTWfVçDFF°¢F–6³¢cÀ¢âç6V6öæE÷6VÆV7FVEö÷VUöWfVç@¢ÒÀ¢Vç6VÆV7FVEö6öÆÆ–F–æu÷7—6W…öWfVçBÀ¢Ğ¢“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG2À¢†6…6WC£¦g&öÒ…³Â%Ò¢“°§Ğ ¢5·FW7EĞ¦fâÖ–F•÷VçF—¦UöF–Æöuö6å÷VçF—¦UööæÇ•÷F†U÷6VÆV7FVEö÷VUöWfVçB‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢EóÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWB÷VUöWfVçBÒÖ–F”÷VTWfVçDFF°¢F–6³¢CsRÀ¢÷&FW#¢rÀ¢¶–æC¢Ö–F”÷VTWfVçD¶–æC£¥7—7FVÔW†6ÇW6—fR°¢7FGW3¢„cÀ¢FF¢fV2³ƒC2Âƒ"Âƒ3EÒÀ¢ÒÀ¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F”÷VTWfVçG2°¢—FVÕö–BÀ¢WfVçG3¢fV2¶÷VUöWfVçBæ6ÆöæR‚•ÒÀ¢Ò¢æW‡V7B‚'FW7B7—4W‚WfVçB6†÷VÆB&R–ç6W'FVB"“°¢æÖ–F•öVF—F÷%ö—FVÕö–BÒ6öÖR†—FVÕö–B“°¢æÖ–F•öVF—F÷%öÖöFRÒÖ–F”VF—F÷$ÖöFS£¤WfVçDÆ—7C°¢ÆWBòÒçWFFR„ÖW76vS£¥6VÆV7DÖ–F”÷VTWfVçB†—FVÕö–BÂÂfÇ6R’“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F•VçF—¦TF–Æör†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WDÖ–F•VçF—¦U66÷R€¢Ö–F•VçF—¦U66÷S£¥6VÆV7FVDWfVçG2À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤66WDÖ–F•VçF—¦R“° ¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ÷VUöWfVçG2‚’À¢´Ö–F”÷VTWfVçDFF°¢F–6³¢CƒÀ¢âæ÷VUöWfVçBæ6ÆöæR‚¢ÕĞ¢“°¢76W'EöW†æÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG2Â†6…6WC£¦g&öÒ…³Ò’“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ÷VUöWfVçG2‚’À¢7FC£§6Æ–6S£¦g&öÕ÷&Vb‚f÷VUöWfVçB¢“°¢76W'EöW†æÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG2Â†6…6WC£¦g&öÒ…³Ò’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ÷VUöWfVçG2‚•³ÒçF–6²ÂCƒ“°¢76W'EöW†æÖ–F•öVF—F÷%÷6VÆV7FVEö÷VUöWfVçG2Â†6…6WC£¦g&öÒ…³Ò’“°§Ğ ¢5·FW7EĞ¦fâÖ–F•÷VçF—¦U÷66÷Uö÷F–öç5öföÆÆ÷u÷&VW%öÖVçUö÷&FW"‚’°¢76W'EöW€¢Ö–F•VçF—¦U66÷S£¤õD”ôå2À¢°¢Ö–F•VçF—¦U66÷S£¤ÆÄæ÷FW2À¢Ö–F•VçF—¦U66÷S£¥6VÆV7FVDæ÷FW2À¢Ö–F•VçF—¦U66÷S£¤ÆÄWfVçG2À¢Ö–F•VçF—¦U66÷S£¥6VÆV7FVDWfVçG2À¢Ğ¢“°¢76W'EöW€¢Ö–F•VçF—¦U66÷S£¥6VÆV7FVDWfVçG2çFõ÷7G&–ær‚’À¢%6VÆV7FVBWfVçG2 ¢“°§Ğ ¢5·FW7EĞ¦fâÖ–F•öVF—F÷%÷VçF—¦Uöw&–Eö6öçfW'6–öåöÖF6†W5öWfW'•ö×W6–6ÅöF—f—6–öâ‚’°¢ÆWB×WB&ö¦V7BÒ&ö¦V7C£¦æWr‚“°¢&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢%–æò"çFõö÷væVB‚’À¢Ò¢æW‡V7B‚'FW7BG&6²6†÷VÆB&R7&VFVB"“°¢ÆWBG&6µö–BÒ&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢#óÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒ&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2´Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢À¢GW&F–öã¢À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÕÒÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’æ÷FR6†÷VÆB&R–ç6W'FVB"“°¢ÆWBæ÷FUö–BÒ&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Òæ–B‚“°¢ÆWBÒ&ö¦V7Bç6WGF–æw2‚’ç‚“° ¢f÷"w&–B–â6æw&–C£¤ÄÂ°¢ÆWB–çFW'fÂÒw&–@¢çF–6µö–çFW'fÂ‡¢æW‡V7B‚'7W÷'FVB&ö¦V7B†2â–çFW'fÂf÷"WfW'’w&–B"“°¢&ö¦V7@¢æÇ’„Ft7F–öã£¤VF—DÖ–F”æ÷FR°¢—FVÕö–BÀ¢æ÷FUö–BÀ¢FF¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢–çFW'fÂ²À¢GW&F–öã¢À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ò¢æW‡V7B‚&Ö÷f–ærF†RFW7Bæ÷FRöfbÖw&–B6†÷VÆB7V66VVB"“°¢&ö¦V7@¢æÇ’„Ft7F–öã£¥VçF—¦TÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FUö–G3¢fV2¶æ÷FUö–EÒÀ¢w&–C¢7WW#£¦Ö–F•öVF—F÷%öw&–Eög&7F–öâ†w&–B’À¢7G&VæwFƒ¢ãÀ¢Ò¢æW‡V7B‚'VçF—¦–ærv—F‚F†R6VÆV7FVBÔ”D’w&–B6†÷VÆB7V66VVB"“°¢76W'EöW€¢&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒçF–6²‚’À¢–çFW'fÂÀ¢&w&–B6öçfW'6–öâ6†÷VÆB&W6W'fR¶w&–GÒ–çFW'fÂ¶–çFW'fÇÒ ¢“°¢Ğ§Ğ ¢5·FW7EĞ¦fâ–æõ÷&öÆÅö6÷•öG&uöFG5ög&W6…öæ÷FW5öæE÷VæFöW5ö5ööæUö7F–öâ‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”—FVÒ“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F”VF—F÷"†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”æ÷FTB€¢—FVÕö–BÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢#CÀ¢GW&F–öã¢#CÀ¢fVÆö6—G“¢“À¢öfe÷fVÆö6—G“¢3rÀ¢ÒÀ¢’“°¢ÆWB6÷W&6Uö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Òæ–B‚“°¢ÆWB6÷W&6RÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Òæ6ÆöæR‚“°¢æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2Ò†6…6WC£¦g&öÒ…·6÷W&6Uö–EÒ“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WE–æõ&öÆÄ7W'6÷"†—FVÕö–BÂ3c’“°¢76W'B†æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2æ—5öV×G’‚’“° ¢ÆWBòÒçWFFR„ÖW76vS£¤6÷”G&tÖ–F”æ÷FW2€¢—FVÕö–BÀ¢fV2´Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢CƒÀ¢GW&F–öã¢#CÀ¢fVÆö6—G“¢“À¢öfe÷fVÆö6—G“¢3rÀ¢ÕÒÀ¢’“° ¢ÆWBæ÷FW2Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚“°¢76W'EöW†æ÷FW2æÆVâ‚’Â"“°¢76W'EöW†æ÷FW5³ÒÂ6÷W&6R“°¢76W'EöW†æ÷FW5³Òç—F6‚‚’Âc“°¢76W'EöW†æ÷FW5³ÒçF–6²‚’ÂCƒ“°¢76W'EöW†æ÷FW5³Òæöfe÷fVÆö6—G’‚’Â3r“°¢76W'EöæR†æ÷FW5³Òæ–B‚’Â6÷W&6Uö–B“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2À¢†6…6WC£¦g&öÒ…¶æ÷FW5³Òæ–B‚•Ò¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚’À¢7FC£§6Æ–6S£¦g&öÕ÷&Vb‚g6÷W&6R¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚’æÆVâ‚’Â"“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒÂ6÷W&6R“°§Ğ ¢5·FW7EĞ¦fâÖ–F•öVF—F÷%öf—E÷W6W5ööæÇ•÷f—6–&ÆUöæ÷FW5ögFW%÷7F'E÷G&–Ò‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”—FVÒ“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤VF—DÖ–F”—FVÒ°¢—FVÕö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢UóÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2°¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢#À¢F–6³¢3À¢GW&F–öã¢#CÀ¢fVÆö6—G“¢“À¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cBÀ¢F–6³¢ó#À¢GW&F–öã¢#CÀ¢fVÆö6—G“¢À¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢#À¢F–6³¢À¢GW&F–öã¢#À¢fVÆö6—G“¢À¢öfe÷fVÆö6—G“¢À¢ÒÀ¢ÒÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥G&–ÔÖ–F”—FVÕ7F'B°¢—FVÕö–BÀ¢7F'E÷F–6³¢CƒÀ¢ÆVæwF…÷F–6·3¢5ó3cÀ¢6÷W&6Uööfg6WE÷F–6·3¢CƒÀ¢Ò¢çVçw&‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F”VF—F÷"†—FVÕö–B’“° ¢76W'EöW†æÖ–F•öVF—F÷%ö÷&–v–å÷F–6²ÂCƒ“°¢76W'EöW†æÖ–F•öVF—F÷%ö†–v…÷—F6‚Âcb“°¢76W'EöW†æÖ–F•öVF—F÷%÷—F6…÷&÷w2ÂR“°§Ğ ¢5·FW7EĞ¦fâ–æõ÷&öÆÅö6÷•÷7FUöæE÷fVÆö6—G•öVF—G5ö&Uöw&÷WVE÷VæFö&ÆUö7F–öç2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”—FVÒ“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F”VF—F÷"†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”æ÷FTB€¢—FVÕö–BÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢#CÀ¢GW&F–öã¢#CÀ¢fVÆö6—G“¢“À¢öfe÷fVÆö6—G“¢3rÀ¢ÒÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”æ÷FTB€¢—FVÕö–BÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cBÀ¢F–6³¢“cÀ¢GW&F–öã¢CƒÀ¢fVÆö6—G“¢À¢öfe÷fVÆö6—G“¢#bÀ¢ÒÀ¢’“°¢ÆWB6÷W&6Uö–G2Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Ğ¢ææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂæ÷FRæ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤6÷”Ö–F”æ÷FW2†—FVÕö–BÂ6÷W&6Uö–G2æ6ÆöæR‚’’“°¢76W'EöW€¢æÖ–F•öVF—F÷%öfVVF&6²æ5öFW&Vb‚’À¢6öÖR‚$6÷–VB"Ô”D’æ÷FW2"¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥7FTÖ–F”æ÷FW2†—FVÕö–B’“°¢76W'B†æÖ–F•öVF—F÷%öfVVF&6²æ—5öæöæR‚’“°¢ÆWBæ÷FW2Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚“°¢76W'EöW†æ÷FW2æÆVâ‚’ÂB“°¢76W'EöW†æ÷FW5³%ÒçF–6²‚’ÂóCC“°¢76W'EöW†æ÷FW5³5ÒçF–6²‚’Â%óc“°¢76W'EöW†æ÷FW5³%Òæöfe÷fVÆö6—G’‚’Â3r“°¢76W'EöW†æ÷FW5³5Òæöfe÷fVÆö6—G’‚’Â#b“°¢76W'EöæR†æ÷FW5³%Òæ–B‚’Â6÷W&6Uö–G5³Ò“°¢76W'EöW€¢æÖ–F•öVF—F÷%÷6VÆV7FVEöæ÷FW2À¢†6…6WC£¦g&öÒ…¶æ÷FW5³%Òæ–B‚’Âæ÷FW5³5Òæ–B‚•Ò¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚’æÆVâ‚’Â"“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚’æÆVâ‚’ÂB“°¢ÆWBòÒçWFFR„ÖW76vS£¥7FTÖ–F”æ÷FW2†—FVÕö–B’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³EÒçF–6²‚’Â%ócC“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³UÒçF–6²‚’Â5ó3c“° ¢ÆWB7FVBÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³BâåĞ¢æ—FW"‚¢æÖ‡Ææ÷FWÂ°¢€¢æ÷FRæ–B‚’À¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢æ÷FRç—F6‚‚’À¢F–6³¢æ÷FRçF–6²‚’À¢GW&F–öã¢æ÷FRæGW&F–öâ‚’À¢fVÆö6—G“¢À¢öfe÷fVÆö6—G“¢æ÷FRæöfe÷fVÆö6—G’‚’À¢ÒÀ¢¢Ò¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤VF—DÖ–F”æ÷FW2†—FVÕö–BÂ7FVB’“°¢76W'B€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³BâåĞ¢æ—FW"‚¢æÆÂ‡Ææ÷FWÂæ÷FRçfVÆö6—G’‚’ÓÒ¢“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³EÒæöfe÷fVÆö6—G’‚’Â3r“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³UÒæöfe÷fVÆö6—G’‚’Â#b“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³EÒçfVÆö6—G’‚’Â““°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³UÒçfVÆö6—G’‚’Â“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³EÒæöfe÷fVÆö6—G’‚’Â3r“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³UÒæöfe÷fVÆö6—G’‚’Â#b“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'B€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³BâåĞ¢æ—FW"‚¢æÆÂ‡Ææ÷FWÂæ÷FRçfVÆö6—G’‚’ÓÒ¢“°§Ğ ¢5·FW7EĞ¦fâ–æõ÷&öÆÅö6÷––æuööÆFU÷‡&6U÷7FW5ögFW%ö—Eö–ç7FVEööeöæV%ö—FVÕ÷7F'B‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢çF–ÖVÆ–æRæÖ–F•öVF—F÷%÷6æöw&–BÒ6æw&–C£¥6—‡FVVçFƒ°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”—FVÒ“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F”VF—F÷"†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”æ÷FTB€¢—FVÕö–BÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢%óƒƒÀ¢GW&F–öã¢#À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢’“°¢ÆWB6÷W&6RÒfç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Ó°¢ÆWBòÒçWFFR„ÖW76vS£¤6÷”Ö–F”æ÷FW2†—FVÕö–BÂfV2·6÷W&6Ræ–B‚•Ò’“° ¢ÆWBòÒçWFFR„ÖW76vS£¥7FTÖ–F”æ÷FW2†—FVÕö–B’“° ¢ÆWBæ÷FW2Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚“°¢76W'EöW†æ÷FW2æÆVâ‚’Â"“°¢76W'EöW†æ÷FW5³ÒçF–6²‚’Â5ó#“°§Ğ ¢5·FW7EĞ¦fâ–æõ÷&öÆÅ÷7FU÷W6W5÷F†Uö6&WEöæEöGWÆ–6FUöFöW5öæ÷E÷&WÆ6U÷F†Uö6Æ—&ö&B‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢çF–ÖVÆ–æRæÖ–F•öVF—F÷%÷6æöw&–BÒ6æw&–C£¥6—‡FVVçFƒ°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”—FVÒ“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F”VF—F÷"†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”æ÷FTB€¢—FVÕö–BÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢%óƒƒÀ¢GW&F–öã¢#À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢’“°¢ÆWB6÷W&6Uö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤6÷”Ö–F”æ÷FW2†—FVÕö–BÂfV2·6÷W&6Uö–EÒ’“°¢76W'EöW†æÖ–F•öVF—F÷%öVF—Eö7W'6÷%÷F–6²Â6öÖRƒ5ó#’“° ¢ÆWBòÒçWFFR„ÖW76vS£¥6WE–æõ&öÆÄ7W'6÷"†—FVÕö–BÂó“#’“°¢ÆWBòÒçWFFR„ÖW76vS£¥7FTÖ–F”æ÷FW2†—FVÕö–B’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒçF–6²‚’Âó“#“° ¢ÆWB6Æ—&ö&BÒæÖ–F•öæ÷FUö6Æ—&ö&Bææ÷FW2æ6ÆöæR‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥6VÆV7DÖ–F”æ÷FW2„†6…6WC£¦g&öÒ…·6÷W&6Uö–EÒ’’“°¢ÆWBòÒçWFFR„ÖW76vS£¤GWÆ–6FTÖ–F”æ÷FW2†—FVÕö–B’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³%ÒçF–6²‚’Â5ó#“°¢76W'EöW†æÖ–F•öæ÷FUö6Æ—&ö&Bææ÷FW2Â6Æ—&ö&B“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚’æÆVâ‚’Â"“°§Ğ ¢5·FW7EĞ¦fâ–æõ÷&öÆÅö7&÷75ö—FVÕ÷7FU÷W6W5÷F†Uö6÷–VE÷‡&6U÷÷6—F–öå÷v†Våöæõö6&WEö—5÷6WB‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢çF–ÖVÆ–æRæÖ–F•öVF—F÷%÷6æöw&–BÒ6æw&–C£¥6—‡FVVçFƒ°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”—FVÒ“°¢ÆWB6÷W&6Uö—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”—FVÒ“°¢ÆWBF&vWEö—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”—FVÒ“°¢ÆWBW‡Æ–6—E÷F&vWEö—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³%Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F”VF—F÷"‡6÷W&6Uö—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”æ÷FTB€¢6÷W&6Uö—FVÕö–BÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢%óƒƒÀ¢GW&F–öã¢#À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢’“°¢ÆWB6÷W&6Uö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤6÷”Ö–F”æ÷FW2‡6÷W&6Uö—FVÕö–BÂfV2·6÷W&6Uö–EÒ’“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F”VF—F÷"‡F&vWEö—FVÕö–B’“°¢76W'EöW†æÖ–F•öVF—F÷%öVF—Eö7W'6÷%÷F–6²ÂæöæR“° ¢ÆWBòÒçWFFR„ÖW76vS£¥7FTÖ–F”æ÷FW2‡F&vWEö—FVÕö–B’“° ¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚’æÆVâ‚’Â“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒçF–6²‚’Â5ó#“°¢76W'EöW†æÖ–F•öVF—F÷%öVF—Eö7W'6÷%÷F–6²Â6öÖRƒ5ó3c’“° ¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F”VF—F÷"†W‡Æ–6—E÷F&vWEö—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6WE–æõ&öÆÄ7W'6÷"†W‡Æ–6—E÷F&vWEö—FVÕö–BÂó“#’“°¢ÆWBòÒçWFFR„ÖW76vS£¥7FTÖ–F”æ÷FW2†W‡Æ–6—E÷F&vWEö—FVÕö–B’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³%Òææ÷FW2‚•³ÒçF–6²‚’Âó“#“°§Ğ ¢5·FW7EĞ¦fâ–æõ÷&öÆÅ÷7FU÷&V¦V7F–öåö—5÷f—6–&ÆUö5öVF—F÷%öÆö6ÅöfVVF&6²‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”—FVÒ“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F”VF—F÷"†—FVÕö–B’“°¢æÖ–F•öVF—F÷%öVF—Eö7W'6÷%÷F–6²Ò6öÖRƒ5óc“°¢æÖ–F•öæ÷FUö6Æ—&ö&Bææ÷FW2ÒfV2´Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢À¢GW&F–öã¢CƒÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÕÓ°¢ç7FGW2Ò$Ö–âv–æF÷r7FGW2"çFõö÷væVB‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¥7FTÖ–F”æ÷FW2†—FVÕö–B’“° ¢76W'EöW€¢ç7FGW2À¢%7FR&V¦V7FVC¢æ÷FW2v÷VÆBW‡FVæB&W–öæBF†RÔ”D’—FVÒ ¢“°¢76W'EöW€¢æÖ–F•öVF—F÷%öfVVF&6²æ5öFW&Vb‚’À¢6öÖR‚%7FR&V¦V7FVC¢æ÷FW2v÷VÆBW‡FVæB&W–öæBF†RÔ”D’—FVÒ"¢“°¢76W'B†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚’æ—5öV×G’‚’“°§Ğ ¢5·FW7EĞ¦fâ–æõ÷&öÆÅ÷7FU÷W6W5ö—G5ö÷våöw&–EöF—f—6–öåöæE÷&W7V7G5÷6†&VE÷6æööfb‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”—FVÒ“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F”VF—F÷"†—FVÕö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR…F–ÖVÆ–æTWfVçC£¥6WDÖ–F”VF—F÷%6æw&–B€¢6æw&–C£¤V–v‡F…G&—ÆWBÀ¢’’“°¢æÖ–F•öVF—F÷%öVF—Eö7W'6÷%÷F–6²Ò6öÖRƒ““°¢æÖ–F•öæ÷FUö6Æ—&ö&Bææ÷FW2ÒfV2´Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢À¢GW&F–öã¢#À¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÕÓ° ¢ÆWBòÒçWFFR„ÖW76vS£¥7FTÖ–F”æ÷FW2†—FVÕö–B’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒçF–6²‚’Â3#“° ¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR…F–ÖVÆ–æTWfVçC£¥FövvÆTÖ–F”VF—F÷%6æ’“°¢æÖ–F•öVF—F÷%öVF—Eö7W'6÷%÷F–6²Ò6öÖRƒS“°¢æÖ–F•öæ÷FUö6Æ—&ö&BæÆ7E÷7FRÒæöæS°¢ÆWBòÒçWFFR„ÖW76vS£¥7FTÖ–F”æ÷FW2†—FVÕö–B’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒçF–6²‚’ÂS“°§Ğ ¢5·FW7EĞ¦fâ–æõ÷&öÆÅöæf–vF–öå÷¦ööÕö¶VW5ö—G5öæ6†÷%öæEöf—Eög&ÖW5÷F†Uöæ÷FW2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”—FVÒ“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”æ÷FTB€¢—FVÕö–BÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢“cÀ¢GW&F–öã¢#CÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”æ÷FTB€¢—FVÕö–BÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cBÀ¢F–6³¢%óƒƒÀ¢GW&F–öã¢CƒÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F”VF—F÷"†—FVÕö–B’“° ¢76W'EöW†æÖ–F•öVF—F÷%ö÷&–v–å÷F–6²Âs#“°¢76W'EöW†æÖ–F•öVF—F÷%ö†–v…÷—F6‚Âcb“°¢76W'B‚†æÖ–F•öVF—F÷%÷—†VÇ5÷W%ö&VBÒƒƒƒBãò2ã’’æ'2‚’Âc3#£¤U4”Äôâ“° ¢ÆWBv–æF÷uö–BÒæÖ–F•öVF—F÷%÷v–æF÷uö–BçVçw&‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥v–æF÷u&W6—¦VB€¢v–æF÷uö–BÀ¢–6VC£¥6—¦S£¦æWrƒs#ãÂC#ã’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤f—E–æõ&öÆÅFôæ÷FW2†—FVÕö–B’“°¢76W'B‚†æÖ–F•öVF—F÷%÷—†VÇ5÷W%ö&VBÒƒcBãò2ã’’æ'2‚’Âc3#£¤U4”Äôâ“° ¢æÖ–F•öVF—F÷%ö÷&–v–å÷F–6²Òó°¢æÖ–F•öVF—F÷%÷—†VÇ5÷W%ö&VBÒ“bã°¢ÆWBö–çFW%ööfg6WBÒ“"ãöccC°¢ÆWBF–6µö&Vf÷&RÒæÖ–F•öVF—F÷%ö÷&–v–å÷F–6²2cc@¢²ö–çFW%ööfg6WBòccC£¦g&öÒ†æÖ–F•öVF—F÷%÷—†VÇ5÷W%ö&VB¢¢ccC£¦g&öÒ†ç&ö¦V7Bç6WGF–æw2‚’ç‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥–æõ&öÆÅ¦ööÔBƒ"ãÂö–çFW%ööfg6WB2c3"’“°¢ÆWBF–6µögFW"ÒæÖ–F•öVF—F÷%ö÷&–v–å÷F–6²2cc@¢²ö–çFW%ööfg6WBòccC£¦g&öÒ†æÖ–F•öVF—F÷%÷—†VÇ5÷W%ö&VB¢¢ccC£¦g&öÒ†ç&ö¦V7Bç6WGF–æw2‚’ç‚’“°¢76W'B‚‡F–6µö&Vf÷&RÒF–6µögFW"’æ'2‚’Âã“° ¢ÆWBòÒçWFFR„ÖW76vS£¥–æõ&öÆÅå—†VÇ2ƒ“bã’“°¢76W'EöW†æÖ–F•öVF—F÷%ö÷&–v–å÷F–6²Â%óCC“°§Ğ ¢5·FW7EĞ¦fâ–æõ÷&öÆÅöf—Eö–æ6ÇVFW5öÆöæu÷‡&6W5öæE÷v–FU÷—F6…÷&ævW2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢óÀ¢Ò¢æW‡V7B‚&ÆöærÔ”D’6÷W&6R6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢f÷"æ÷FR–â°¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢"À¢F–6³¢À¢GW&F–öã¢“cÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢À¢F–6³¢CóÀ¢GW&F–öã¢“cÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ò°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”æ÷FTB†—FVÕö–BÂæ÷FR’“°¢Ğ ¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F”VF—F÷"†—FVÕö–B’“° ¢76W'B†æÖ–F•öVF—F÷%÷—†VÇ5÷W%ö&VBÂ#Bã“°¢76W'EöW†æÖ–F•öVF—F÷%ö†–v…÷—F6‚Â"“°¢76W'EöW†æÖ–F•öVF—F÷%÷—F6…÷&÷w2Â2“°¢76W'B†æÖ–F•öVF—F÷%÷—F6…÷&÷uö†V–v‡BãÒ"ã“°§Ğ ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ğ¢5·FW7EĞ¦fâ–æõ÷&öÆÅ÷Æ–†VEöÖF6†W5ö'&ævVÖVçEöæEöföÆÆ÷uö¶VW5ö—Eö–å÷f–Wr‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”—FVÒ“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VäÖ–F”VF—F÷"†—FVÕö–B’“°¢ÆWBÆ–†VE÷F–6²Ò%ó°¢æÖ–F•öVF—F÷%÷—†VÇ5÷W%ö&VBÒ“bã°¢çÆ–†VE÷6×ÆRÒç&ö¦V7Bç6×ÆUöE÷F–6²‡Æ–†VE÷F–6²’çVçw&‚“°¢çÆ–&6µ÷Æ––ærÒG'VS°¢æÖ–F•öVF—F÷%öföÆÆ÷u÷Æ–†VBÒG'VS° ¢ÆWB—FVÒÒfç&ö¦V7BæÖ–F•ö—FV×2‚•³Ó°¢76W'EöW†æÖ–F•öVF—F÷%÷Æ–†VE÷F–6²†—FVÒ’Â6öÖR‡Æ–†VE÷F–6²’“° ¢æföÆÆ÷uöÖ–F•öVF—F÷%÷Æ–†VB‚“° ¢76W'EöW†æÖ–F•öVF—F÷%ö÷&–v–å÷F–6²ÂÆ–†VE÷F–6²Òeóc3“°§Ğ ¢5·FW7EĞ¦fâ–æõ÷&öÆÅö–ç6W'EöÖ÷fU÷&W6—¦UöæEöFVÆWFU÷W6U÷&ö¦V7Eö7F–öç2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”—FVÒ“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤FDÖ–F”æ÷FTB€¢—FVÕö–BÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢s"À¢F–6³¢CƒÀ¢GW&F–öã¢#CÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢’“°¢ÆWBæ÷FRÒfç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Ó°¢ÆWBæ÷FUö–BÒæ÷FRæ–B‚“°¢76W'EöW‚†æ÷FRç—F6‚‚’Âæ÷FRçF–6²‚’Âæ÷FRæGW&F–öâ‚’’Âƒs"ÂCƒÂ#C’“° ¢ÆWBòÒçWFFR„ÖW76vS£¤VF—DÖ–F”æ÷FW2€¢—FVÕö–BÀ¢fV2²€¢æ÷FUö–BÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢sBÀ¢F–6³¢s#À¢GW&F–öã¢CƒÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÒÀ¢•ÒÀ¢’“°¢ÆWBæ÷FRÒfç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Ó°¢76W'EöW‚†æ÷FRç—F6‚‚’Âæ÷FRçF–6²‚’Âæ÷FRæGW&F–öâ‚’’ÂƒsBÂs#ÂCƒ’“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢ÆWBæ÷FRÒfç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Ó°¢76W'EöW‚†æ÷FRç—F6‚‚’Âæ÷FRçF–6²‚’Âæ÷FRæGW&F–öâ‚’’Âƒs"ÂCƒÂ#C’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒçF–6²‚’Âs#“° ¢ÆWBòÒçWFFR„ÖW76vS£¤FVÆWFTÖ–F”æ÷FW2†—FVÕö–BÂfV2¶æ÷FUö–EÒ’“°¢76W'B†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚’æ—5öV×G’‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Òæ–B‚’Âæ÷FUö–B“°§Ğ ¢5·FW7EĞ¦fâG&6µ÷&VæÖUö—5÷VæFö&ÆR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“° ¢76W'B‚çG&6µöæÖUöVF—G2æ6öçF–ç5ö¶W’‚gG&6µö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR€¢7WW#£§7WW#£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6VÆV7EG&6²‡G&6µö–B’À¢’“°¢76W'EöW€¢çF–ÖVÆ–æRæFVÆWFUöfö7W2À¢7WW#£§7WW#£§F–ÖVÆ–æS£¤FVÆWFTfö7W5F&vWC£¥G&6µæVÀ¢“°¢ÆWBòÒçWFFR„ÖW76vS£¤&Vv–åG&6´æÖTVF—B‡G&6µö–B’“°¢76W'EöW†çG&6µöæÖUöVF—G2ævWB‚gG&6µö–B’çVçw&‚’Â$VF–ò"“°¢76W'EöW€¢çF–ÖVÆ–æRæFVÆWFUöfö7W2À¢7WW#£§7WW#£§F–ÖVÆ–æS£¤FVÆWFTfö7W5F&vWC£¥G&6µæVÀ¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥G&6´æÖT6†ævVB‡G&6µö–BÂ$ÆVBf÷‚"çFõö÷væVB‚’’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6öÖÖ—EG&6´æÖR‡G&6µö–B’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒææÖR‚’Â$ÆVBf÷‚"“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒææÖR‚’Â$VF–ò"“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¥G&6´æÖT6†ævVB‡G&6µö–BÂ""çFõö÷væVB‚’’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6öÖÖ—EG&6´æÖR‡G&6µö–B’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒææÖR‚’Â$VF–ò"“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ“°§Ğ ¢5·FW7EĞ¦fâÖ—†W%÷G&6µ÷&VæÖU÷W6W5÷6†&VE÷VæFö&ÆU÷&ö¦V7Eö7F–öâ‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWB6VÆV7FVE÷G&6µö–BÒçF–ÖVÆ–æRç6VÆV7FVE÷G&6²çVçw&‚“°¢ÆWB&VæÖVE÷G&6²Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–æB‡ÇG&6·ÂG&6²æ–B‚’Ò6VÆV7FVE÷G&6µö–B¢çVçw&‚“°¢ÆWB&VæÖVE÷G&6µö–BÒ&VæÖVE÷G&6²æ–B‚“°¢ÆWB÷&–v–æÅöæÖRÒ&VæÖVE÷G&6²ææÖR‚’çFõö÷væVB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥6†÷tÖ–åv÷&·76R„Ö–åv÷&·76S£¤Ö—†W"’“° ¢ÆWBòÒçWFFR„ÖW76vS£¥G&6´æÖT6†ævVB€¢&VæÖVE÷G&6µö–BÀ¢$&72'W2"çFõö÷væVB‚’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6öÖÖ—EG&6´æÖR‡&VæÖVE÷G&6µö–B’“°¢76W'EöW€¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–æB‡ÇG&6·ÂG&6²æ–B‚’ÓÒ&VæÖVE÷G&6µö–B¢çVçw&‚¢ææÖR‚’À¢$&72'W2 ¢“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Â6öÖR‡6VÆV7FVE÷G&6µö–B’“° ¢ÆWBòÒçWFFR„ÖW76vS£¥6†÷tÖ–åv÷&·76R„Ö–åv÷&·76S£¤'&ævVÖVçB’“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW€¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–æB‡ÇG&6·ÂG&6²æ–B‚’ÓÒ&VæÖVE÷G&6µö–B¢çVçw&‚¢ææÖR‚’À¢÷&–v–æÅöæÖP¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW€¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æf–æB‡ÇG&6·ÂG&6²æ–B‚’ÓÒ&VæÖVE÷G&6µö–B¢çVçw&‚¢ææÖR‚’À¢$&72'W2 ¢“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Â6öÖR‡6VÆV7FVE÷G&6µö–B’“°§Ğ ¢5·FW7EĞ¦fâG&6µö6öçFW‡EöÖVçU÷6VÆV7G5ö—G5÷F&vWEöæEö6Æ÷6W5ögFW%öåö7F–öâ‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR€¢7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤÷VåG&6´6öçFW‡DÖVçR‡G&6µö–B’À¢’“°¢76W'EöW†çF–ÖVÆ–æRæ6öçFW‡E÷G&6²Â6öÖR‡G&6µö–B’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Â6öÖR‡G&6µö–B’“° ¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR€¢7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥FövvÆUG&6´6öçFW‡DÖVçR‡G&6µö–B’À¢’“°¢76W'B†çF–ÖVÆ–æRæ6öçFW‡E÷G&6²æ—5öæöæR‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR€¢7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥FövvÆUG&6´6öçFW‡DÖVçR‡G&6µö–B’À¢’“°¢76W'EöW†çF–ÖVÆ–æRæ6öçFW‡E÷G&6²Â6öÖR‡G&6µö–B’“° ¢ÆWBòÒçWFFR„ÖW76vS£¥FövvÆT×WFR‡G&6µö–B’“°¢76W'B†çF–ÖVÆ–æRæ6öçFW‡E÷G&6²æ—5öæöæR‚’“°¢76W'B†ç&ö¦V7BçG&6·2‚•³Òæ—5ö×WFVB‚’“°§Ğ ¢5·FW7EĞ¦fâ—FVÕö6öçFW‡EöÖVçU÷6VÆV7G5÷Vç6VÆV7FVE÷F&vWG5öæE÷&W6W'fW5öW†—7F–æuö×VÇF—6VÆV7F–öâ‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢f÷"7F'E÷F–6²–â³Âó“#Â5óƒCÒ°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6²À¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢çVçw&‚“°¢Ğ¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢ÆWB—FVÕö–G2Ò ¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æÖ‡Æ—FV×Â—FVÒæ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢çF–ÖVÆ–æP¢ç6VÆV7FVEö—FV×0¢æW‡FVæB…¶—FVÕö–G5³ÒÂ—FVÕö–G5³ÕÒ“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÒ6öÖR†—FVÕö–G5³Ò“°¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6WEF–ÖU6VÆV7F–öâ°¢7F'E÷F–6³¢À¢VæE÷F–6³¢SÀ¢Ò“°¢ÆWB6VÆV7F–öâÒçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6ÆöæR‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR€¢7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤÷Vä—FVÔ6öçFW‡DÖVçR°¢—FVÕö–C¢—FVÕö–G5³ÒÀ¢ƒ¢#ãÀ¢“¢CãÀ¢ÒÀ¢’“°¢76W'EöW†çF–ÖVÆ–æRæ6öçFW‡Eö—FVÒÂ6öÖR†—FVÕö–G5³Ò’“°¢76W'EöW†çF–ÖVÆ–æRæ6öçFW‡Eö—FVÕ÷÷6—F–öâÂ6öÖR‚ƒ#ãÂCã’’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â6VÆV7F–öâ“°¢76W'B€¢6öÖÖæG3£¦f÷%öÖVçR‚fÂÖ–äÖVçS£¤—FVÒ¢æ—FW"‚¢æç’‡ÆVçG'—ÂVçG'’æ–BÓÒ6öÖÖæD–C£¤FVÆWFU6VÆV7FVD—FV×2bbVçG'’æVæ&ÆVB¢“° ¢ÆWBòÒçWFFR„ÖW76vS£¤W66R“°¢76W'EöW†çF–ÖVÆ–æRæ6öçFW‡Eö—FVÒÂæöæR“°¢76W'EöW€¢çF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâÀ¢6öÖR†7&FS£§F–ÖVÆ–æS£¥F–ÖU6VÆV7F–öâ°¢7F'E÷F–6³¢À¢VæE÷F–6³¢SÀ¢Ò¢“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â6VÆV7F–öâ“° ¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR€¢7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤÷Vä—FVÔ6öçFW‡DÖVçR°¢—FVÕö–C¢—FVÕö–G5³%ÒÀ¢ƒ¢#ãÀ¢“¢‚ãÀ¢ÒÀ¢’“°¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2À¢¶—FVÕö–G5³%ÕÒæ–çFõö—FW"‚’æ6öÆÆV7B‚¢“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR†—FVÕö–G5³%Ò’“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR€¢7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6VÆV7DV×G’ƒEó’À¢’“°¢76W'EöW†çF–ÖVÆ–æRæ6öçFW‡Eö—FVÒÂæöæR“°¢76W'EöW†çF–ÖVÆ–æRæ6öçFW‡Eö—FVÕ÷÷6—F–öâÂæöæR“°§Ğ ¢5·FW7EĞ¦fâG&6µöÖVçUö6öçFW‡EöÖVçUöæEö7F–öå÷6V&6…÷6†&Uö6öÖÖæEöFVf–æ—F–öç2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWB6VÆV7FVEö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Ò6öÖR‡6VÆV7FVEö–B“° ¢ÆWBÖVçUöVçG&–W2Ò6öÖÖæG3£¦f÷%öÖVçR‚fÂÖ–äÖVçS£¥G&6²“°¢ÆWB6öçFW‡EöVçG&–W2Ò6öÖÖæG3£¦f÷%÷G&6µö6öçFW‡B‚fÂ6VÆV7FVEö–B“°¢f÷"6öçFW‡EöVçG'’–â6öçFW‡EöVçG&–W2°¢ÆWB6öÖÖæD–C£¥G&6²²G&6µö–BÂ6öÖÖæBÒÒ6öçFW‡EöVçG'’æ–BVÇ6R°¢æ–2‚'G&6²6öçFW‡BVçG'’6†÷VÆBF&vWBF†R6Æ–6¶VBG&6²"“°¢Ó°¢ÆWBÖVçUöVçG'’ÒÖVçUöVçG&–W0¢æ—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒ6öÖÖæD–C£¥6VÆV7FVEG&6²†6öÖÖæB’¢æW‡V7B‚'F†RG&6²ÖVçR6†÷VÆBW‡÷6RF†R6ÖR6öÖÖæB"“°¢76W'EöW†6öçFW‡EöVçG'’æÆ&VÂÂÖVçUöVçG'’æÆ&VÂ“°¢76W'EöW‡G&6µö–BÂ6VÆV7FVEö–B“°¢Ğ¢76W'EöW€¢6öÖÖæG3£¦f–æB‚fÂ&Ö÷fR6VÆV7FVBG&6²W"’À¢6öÖR„6öÖÖæD–C£¥6VÆV7FVEG&6²…G&6´6öÖÖæC£¤Ö÷fUW’¢“°§Ğ ¢5·FW7EĞ¦fâ—FVÕöÖVçUöW‡÷6W5öæF—fU÷&VÖ÷fUöæE÷6VÆV7EöÆÅöf–Æ&–Æ—G•öf÷%öåöV×G•÷&ö¦V7B‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBVçG&–W2Ò6öÖÖæG3£¦f÷%öÖVçR‚fÂÖ–äÖVçS£¤—FVÒ“°¢ÆWB&VÖ÷fUö—FV×2ÒVçG&–W0¢æf—'7B‚¢æW‡V7B‚$—FVÒÖVçR6†÷VÆB7F'Bv—F‚&VÖ÷fR—FV×2"“°¢76W'EöW‡&VÖ÷fUö—FV×2æ–BÂ6öÖÖæD–C£¤FVÆWFU6VÆV7FVD—FV×2“°¢76W'EöW‡&VÖ÷fUö—FV×2æÆ&VÂÂ%&VÖ÷fR—FV×2"“°¢76W'EöW‡&VÖ÷fUö—FV×2ç6†÷'F7WBÂæöæR“°¢76W'B‚&VÖ÷fUö—FV×2æVæ&ÆVB“° ¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR€¢7WW#£§7WW#£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6VÆV7EG&6²‡G&6µö–B’À¢’“°¢ÆWBFVÆWFUö'•öfö7W2Ò6öÖÖæG3£§6†÷'F7WEöVçG&–W2‚f¢æ–çFõö—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒ&VF—BæFVÆWFRÖfö7W6VB"¢æW‡V7B‚%$TU"w2fö7W2×6Vç6—F—fRFVÆWFR7F–öâ6†÷VÆB&R&Vv—7FW&VB"“°¢76W'EöW€¢FVÆWFUö'•öfö7W2æÆ&VÂÀ¢%&VÖ÷fR—FV×2÷G&6·2öVçfVÆ÷Rö–çG2†FWVæF–æröâfö7W2’ ¢“°¢76W'EöW†FVÆWFUö'•öfö7W2æFVfVÇEö&–æF–ærÂ$FVÆWFR"“° ¢ÆWBFVÆWFUö'•öfö7W5öæõ÷&ö×F–ærÒ6öÖÖæG3£§6†÷'F7WEöVçG&–W2‚f¢æ–çFõö—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒ&VF—BæFVÆWFRÖfö7W6VBÖæò×&ö×F–ær"¢æW‡V7B‚%$TU"w2æò×&ö×F–ærfö7W2×6Vç6—F—fRFVÆWFR7F–öâ6†÷VÆB&R&Vv—7FW&VB"“°¢76W'EöW€¢FVÆWFUö'•öfö7W5öæõ÷&ö×F–æræÆ&VÂÀ¢%&VÖ÷fR—FV×2÷G&6·2öVçfVÆ÷Rö–çG2†FWVæF–æröâfö7W2’Òæò&ö×F–ær ¢“°¢76W'B†FVÆWFUö'•öfö7W5öæõ÷&ö×F–æræFVfVÇEö&–æF–æræ—5öV×G’‚’“°¢76W'EöW€¢6öÖÖæG3£¦f–æB‚fÂ&FVÆWFR'’fö7W2v—F†÷WB&ö×F–ær"’À¢6öÖR„6öÖÖæD–C£¤FVÆWFT'”fö7W4æõ&ö×F–ær¢“° ¢ÆWBG&–Õö—FV×2ÒVçG&–W0¢ævWBƒ¢æW‡V7B‚%G&–Ò—FV×26†÷VÆBföÆÆ÷r&VÖ÷fR—FV×2"“°¢76W'EöW‡G&–Õö—FV×2æ–BÂ6öÖÖæD–C£¥G&–Ô—FV×5Fõ6VÆV7FVD&V“°¢76W'EöW‡G&–Õö—FV×2æÆ&VÂÂ%G&–Ò—FV×2Fò6VÆV7FVB&V"“°¢76W'B‡G&–Õö—FV×2æVæ&ÆVB“° ¢ÆWB6÷•öÆö÷ÒVçG&–W2ævWBƒ"’æW‡V7B‚$6÷’Æö÷6†÷VÆBföÆÆ÷rG&–Ò—FV×2"“°¢76W'EöW†6÷•öÆö÷æ–BÂ6öÖÖæD–C£¤6÷”Æö÷öe6VÆV7FVD&V“°¢76W'EöW†6÷•öÆö÷æÆ&VÂÂ$6÷’Æö÷öb6VÆV7FVB&Vöb—FV×2"“°¢76W'EöW†6÷•öÆö÷ç6†÷'F7WBæ5öFW&Vb‚’Â6öÖR‚$7G&Â´ÇB´Â"’“°¢76W'B†6÷•öÆö÷æVæ&ÆVB“° ¢ÆWBVæ6Æ÷6RÒVçG&–W0¢ævWBƒ2¢æW‡V7B‚$Væ6Æ÷6RÖVF–—FV×26†÷VÆBföÆÆ÷r6÷’Æö÷"“°¢76W'EöW†Væ6Æ÷6Ræ–BÂ6öÖÖæD–C£¤Væ6Æ÷6TÖVF–—FV×4–å&¦÷$VF—B“°¢76W'EöW†Væ6Æ÷6RæÆ&VÂÂ$Væ6Æ÷6RÖVF–—FV×2–â&¦÷"VF—B"“°¢76W'B†Væ6Æ÷6RæVæ&ÆVB“° ¢ÆWB6VÆV7EöÆÂÒVçG&–W0¢ævWBƒB¢æW‡V7B‚%6VÆV7BÆÂ6†÷VÆBföÆÆ÷rF†R—FVÒ&¦÷"ÖVF—B6V7F–öâ"“°¢76W'EöW‡6VÆV7EöÆÂæ–BÂ6öÖÖæD–C£¥6VÆV7DÆÄ—FV×2“°¢76W'EöW‡6VÆV7EöÆÂæÆ&VÂÂ%6VÆV7BÆÂ"“°¢76W'B‡6VÆV7EöÆÂæVæ&ÆVB“° ¢ÆWBVF—EöVçG&–W2Ò6öÖÖæG3£¦f÷%öÖVçR‚fÂÖ–äÖVçS£¤VF—B“°¢ÆWB7FRÒVF—EöVçG&–W0¢æ—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒ6öÖÖæD–C£¥7FT—FV×2¢æW‡V7B‚$VF—BÖVçR6†÷VÆBW‡÷6R7FR—FV×2÷G&6·2"“°¢76W'EöW‡7FRæÆ&VÂÂ%7FR"“°¢76W'EöW‡7FRç6†÷'F7WBæ5öFW&Vb‚’Â6öÖR‚$7G&Âµb"’“°¢76W'B‚7FRæVæ&ÆVB“° ¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¥6VÆV7DÆÄ—FV×2’“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚’æ—5öV×G’‚’“°¢76W'B†ç&ö¦V7BæÖ–F•ö—FV×2‚’æ—5öV×G’‚’“°¢76W'B†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ—5öV×G’‚’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ“°§Ğ ¢5·FW7EĞ¦fâ6÷•öÆö÷ööe÷6VÆV7FVEöVF–õö&V÷7FW5÷6÷W&6U÷6V7F–öç5ööåööæU÷VæFõ÷7FW‚’°¢ÆWBF—&V7F÷'’ÒFV×f–ÆS£§FV×F—"‚’çVçw&‚“°¢ÆWB&ö¦V7E÷F‚ÒF—&V7F÷'’çF‚‚’æ¦ö–â‚&6÷’ÖÆö÷æFr"“°¢ÆWBÖVF–÷&VbÒ&76WC¢òö6÷’ÖÆö÷×FW7B#°¢ÆWB×WB7F÷&RÒ&ö¦V7E7F÷&S£¦÷Vâ‚g&ö¦V7E÷F‚’çVçw&‚“°¢7F÷&P¢æ–×÷'EöVF–õö76WB†ÖVF–÷&VbÂ&6÷’ÖÆö÷çvb"Â7FC£¦–ó£¤7W'6÷#£¦æWr…³÷Sƒ²EÒ’¢çVçw&‚“°¢7F÷&P¢ç6WEöVF–õö76WEöÖWFFF€¢ÖVF–÷&VbÀ¢fFu÷7F÷&vS£¤VF–ô76WDÖWFFF°¢6öçF–æW#¢'vb"çFõö÷væVB‚’À¢6öFV3¢'6Ò"çFõö÷væVB‚’À¢6×ÆU÷&FS¢6öÖRƒC…ó’À¢6†ææVÅö6÷VçC¢6öÖRƒ’À¢&—G5÷W%÷6×ÆS¢6öÖRƒb’À¢g&ÖUö6÷VçC¢6öÖRƒcó’À¢GW&F–öåöææ÷3¢6öÖRƒ%óSóó’À¢'—FUöÆVã¢6öÖRƒB’À¢ÒÀ¢¢çVçw&‚“°¢7F÷&Ræ6Æ÷6R‚’çVçw&‚“° ¢ÆWB×WBÒ°¢&ö¦V7E÷Fƒ¢6öÖR‡&ö¦V7E÷F‚’À¢âä£¦FVfVÇB‚¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WE&ö¦V7E6×ÆU&FR°¢6×ÆU÷&FS¢CEóÀ¢Ò¢çVçw&‚“°¢f÷"ò–ââã2°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢Ğ¢ÆWBG&6µö–G2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWBgVÆÅöÆVæwF‚Òç&ö¦V7Bç6×ÆUöE÷F–6²ƒ•ó#’çVçw&‚“°¢ÆWB'F–Å÷7F'BÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒuócƒ’çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6µö–G5³ÒÀ¢ÖVF–÷&Vc¢ÖVF–÷&VbçFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢#‚À¢ÆVæwF…÷6×ÆW3¢gVÆÅöÆVæwF‚À¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6µö–G5³ÒÀ¢ÖVF–÷&Vc¢ÖVF–÷&VbçFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢#SbÀ¢ÆVæwF…÷6×ÆW3¢gVÆÅöÆVæwF‚À¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6µö–G5³%ÒÀ¢ÖVF–÷&Vc¢ÖVF–÷&VbçFõö÷væVB‚’À¢7F'E÷6×ÆS¢'F–Å÷7F'BÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢S"À¢ÆVæwF…÷6×ÆW3¢gVÆÅöÆVæwF‚À¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6µö–G5³%ÒÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢•ó#À¢Ò¢çVçw&‚“°¢ÆWBgVÆÅöf—'7Eö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBgVÆÅ÷6V6öæEö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWB'F–Åö–BÒç&ö¦V7BæVF–õö—FV×2‚•³%Òæ–B‚“°¢ÆWBÖ–F•ö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDVF–ô—FVÕF¶UÆ–&6µ&FR°¢—FVÕö–C¢gVÆÅöf—'7Eö–BÀ¢&FS¢ã#RÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDVF–ô—FVÔ×WFR°¢—FVÕö–C¢gVÆÅöf—'7Eö–BÀ¢×WFVC¢G'VRÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDVF–ô—FVÕföÇVÖR°¢—FVÕö–C¢gVÆÅöf—'7Eö–BÀ¢föÇVÖS¢ãbÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDVF–ô—FVÕF¶TÖ—‚°¢—FVÕö–C¢gVÆÅöf—'7Eö–BÀ¢föÇVÖS¢ã#RÀ¢ã¢Óã2À¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDVF–ô—FVÔæÖR°¢—FVÕö–C¢gVÆÅöf—'7Eö–BÀ¢æÖS¢$ÆVB‡&6R"çFõö÷væVB‚’À¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDVF–ô—FVÕ†6R°¢—FVÕö–C¢gVÆÅ÷6V6öæEö–BÀ¢–çfW'FVC¢G'VRÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDVF–ô—FVÕföÇVÖR°¢—FVÕö–C¢gVÆÅ÷6V6öæEö–BÀ¢föÇVÖS¢ã‚À¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDVF–ô—FVÕF¶TÖ—‚°¢—FVÕö–C¢gVÆÅ÷6V6öæEö–BÀ¢föÇVÖS¢ãcRÀ¢ã¢ãCRÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDVF–ô—FVÔæÖR°¢—FVÕö–C¢gVÆÅ÷6V6öæEö–BÀ¢æÖS¢$&72Æö÷"çFõö÷væVB‚’À¢Ò¢çVçw&‚“°¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2°¢†gVÆÅöf—'7Eö–BÂG&6µö–G5³Ò’À¢†gVÆÅ÷6V6öæEö–BÂG&6µö–G5³Ò’À¢‡'F–Åö–BÂG&6µö–G5³%Ò’À¢†Ö–F•ö–BÂG&6µö–G5³%Ò’À¢Ò“°¢çF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâÒ6öÖR†7&FS£§F–ÖVÆ–æS£¥F–ÖU6VÆV7F–öâ°¢7F'E÷F–6³¢UóscÀ¢VæE÷F–6³¢5óCCÀ¢Ò“°¢ÆWB6VÆV7FVEö&Vf÷&RÒçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6ÆöæR‚“°¢ÆWB&Wf—6–öåö&Vf÷&RÒç&Wf—6–öã°¢ÆWB6VÆV7F–öå÷7F'E÷6×ÆRÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒUósc’çVçw&‚“°¢ÆWB6÷W&6UöFVÇFÒ7WW#£§&W66ÆU÷6×ÆW5öE÷F¶U÷&FR€¢6VÆV7F–öå÷7F'E÷6×ÆRÀ¢ç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚’À¢C…óÀ¢ã#RÀ¢¢çVçw&‚“°¢ÆWBVæ—G•÷6÷W&6UöFVÇFÒ7WW#£§&W66ÆU÷6×ÆW2€¢6VÆV7F–öå÷7F'E÷6×ÆRÀ¢ç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚’À¢C…óÀ¢¢çVçw&‚“°¢ÆWB6VÆV7F–öåöÆVæwF‚Ğ¢ç&ö¦V7Bç6×ÆUöE÷F–6²ƒ5óCC’çVçw&‚’Òç&ö¦V7Bç6×ÆUöE÷F–6²ƒUósc’çVçw&‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¤6÷”Æö÷öe6VÆV7FVD&V’“° ¢ÆWB6Æ—&ö&BÒ ¢æVF–õö—FVÕö6Æ—&ö&@¢æ6ÆöæR‚¢æW‡V7B‚&6÷’7F–öâ6†÷VÆB÷VÆFRF†RVF–ò—FVÒ6Æ—&ö&B"“°¢76W'EöW†6Æ—&ö&Bæ—FV×2æÆVâ‚’Â"“°¢76W'EöW†6Æ—&ö&Bæ—FV×5³ÒçG&6µööfg6WBÂ“°¢76W'EöW†6Æ—&ö&Bæ—FV×5³ÒçG&6µööfg6WBÂ“°¢76W'EöW†6Æ—&ö&Bæ—FV×5³Òç6÷W&6Uööfg6WE÷6×ÆW2Â“°¢76W'EöW€¢6Æ—&ö&Bæ—FV×5³Òç6÷W&6U÷6V7F–öå÷7F'E÷6×ÆW2À¢#‚²6÷W&6UöFVÇF¢“°¢76W'EöW†6Æ—&ö&Bæ—FV×5³Òç6÷W&6Uööfg6WE÷6×ÆW2Â“°¢76W'EöW€¢6Æ—&ö&Bæ—FV×5³Òç6÷W&6U÷6V7F–öå÷7F'E÷6×ÆW2À¢#Sb²Væ—G•÷6÷W&6UöFVÇF¢“°¢76W'B†6Æ—&ö&Bæ—FV×5³Òæ×WFVB“°¢76W'B‚6Æ—&ö&Bæ—FV×5³Òæ×WFVB“°¢76W'B‚6Æ—&ö&Bæ—FV×5³Òç†6Uö–çfW'FVB“°¢76W'B†6Æ—&ö&Bæ—FV×5³Òç†6Uö–çfW'FVB“°¢76W'EöW†6Æ—&ö&Bæ—FV×5³ÒçföÇVÖRÂãb“°¢76W'EöW†6Æ—&ö&Bæ—FV×5³ÒçF¶U÷föÇVÖRÂã#R“°¢76W'EöW†6Æ—&ö&Bæ—FV×5³ÒçF¶U÷âÂÓã2“°¢76W'EöW†6Æ—&ö&Bæ—FV×5³ÒçF¶U÷Æ–&6µ÷&FRÂã#R“°¢76W'EöW†6Æ—&ö&Bæ—FV×5³ÒææÖRÂ$ÆVB‡&6RÆö÷"“°¢76W'EöW†6Æ—&ö&Bæ—FV×5³ÒçföÇVÖRÂã‚“°¢76W'EöW†6Æ—&ö&Bæ—FV×5³ÒçF¶U÷föÇVÖRÂãcR“°¢76W'EöW†6Æ—&ö&Bæ—FV×5³ÒçF¶U÷âÂãCR“°¢76W'EöW†6Æ—&ö&Bæ—FV×5³ÒçF¶U÷Æ–&6µ÷&FRÂã“°¢76W'EöW†6Æ—&ö&Bæ—FV×5³ÒææÖRÂ$&72Æö÷"“°¢76W'EöW†6Æ—&ö&Bæ—FV×5³ÒæÆVæwF…÷6×ÆW2Â6VÆV7F–öåöÆVæwF‚“°¢76W'EöW€¢6Æ—&ö&Bæ—FV×5³Òç6÷W&6U÷6V7F–öåöÆVæwF…÷6×ÆW2À¢7WW#£§&W66ÆU÷6×ÆW5öE÷F¶U÷&FR‡6VÆV7F–öåöÆVæwF‚ÂCEóÂC…óÂã#R’çVçw&‚¢“°¢76W'EöW€¢6Æ—&ö&Bæ—FV×5³Òç6÷W&6U÷6V7F–öåöÆVæwF…÷6×ÆW2À¢7WW#£§&W66ÆU÷6×ÆW2‡6VÆV7F–öåöÆVæwF‚ÂCEóÂC…ó’çVçw&‚¢“°¢76W'EöW†6Æ—&ö&Bç7å÷6×ÆW2Â6VÆV7F–öåöÆVæwF‚“°¢76W'EöW†6Æ—&ö&Bç&ö¦V7E÷6×ÆU÷&FRÂCEó“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â6VÆV7FVEö&Vf÷&R“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“° ¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WE&ö¦V7E6×ÆU&FR°¢6×ÆU÷&FS¢C…óÀ¢Ò¢çVçw&‚“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Ò6öÖR‡G&6µö–G5³Ò“°¢ÆWB7FU÷7F'BÒSuóc°¢ç6WEöVF—Eö7W'6÷%÷÷6—F–öâ†7&FS£§F–ÖVÆ–æS£¤VF—D7W'6÷%÷6—F–öã£¥6×ÆR‡7FU÷7F'B’“°¢ÆWB7FUöÆVæwF‚Ğ¢ScC£§G'•ög&öÒ‚‡S#ƒ£¦g&öÒ‡6VÆV7F–öåöÆVæwF‚’¢C…ó²CEóò"’òCEó’çVçw&‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¥7FT—FV×2’“° ¢ÆWB7FVBÒ ¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â—FVÒç7F'E÷6×ÆR‚’ÓÒ7FU÷7F'B¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢76W'EöW‡7FVBæÆVâ‚’Â"“°¢76W'EöW‡7FVE³ÒçG&6µö–B‚’ÂG&6µö–G5³Ò“°¢76W'EöW‡7FVE³ÒçG&6µö–B‚’ÂG&6µö–G5³Ò“°¢76W'EöW‡7FVE³Òç6÷W&6Uööfg6WE÷6×ÆW2‚’Â“°¢76W'EöW€¢7FVE³Òç6÷W&6U÷6V7F–öå÷7F'E÷6×ÆW2‚’À¢6öÖR†6Æ—&ö&Bæ—FV×5³Òç6÷W&6U÷6V7F–öå÷7F'E÷6×ÆW2¢“°¢76W'EöW‡7FVE³Òç6÷W&6Uööfg6WE÷6×ÆW2‚’Â“°¢76W'EöW‡7FVE³ÒæÆVæwF…÷6×ÆW2‚’Â7FUöÆVæwF‚“°¢76W'B‡7FVE³Òæ—5ö×WFVB‚’“°¢76W'B‚7FVE³Òæ—5ö×WFVB‚’“°¢76W'B‚7FVE³Òæ—5÷†6Uö–çfW'FVB‚’“°¢76W'B‡7FVE³Òæ—5÷†6Uö–çfW'FVB‚’“°¢76W'EöW‡7FVE³ÒçföÇVÖR‚’Â6Æ—&ö&Bæ—FV×5³ÒçföÇVÖR“°¢76W'EöW‡7FVE³ÒçF¶U÷föÇVÖR‚’Â6Æ—&ö&Bæ—FV×5³ÒçF¶U÷föÇVÖR“°¢76W'EöW‡7FVE³ÒçF¶U÷â‚’Â6Æ—&ö&Bæ—FV×5³ÒçF¶U÷â“°¢76W'EöW‡7FVE³ÒçF¶U÷Æ–&6µ÷&FR‚’Âã#R“°¢76W'EöW‡7FVE³ÒææÖR‚’Â6Æ—&ö&Bæ—FV×5³ÒææÖR“°¢76W'EöW‡7FVE³ÒçföÇVÖR‚’Â6Æ—&ö&Bæ—FV×5³ÒçföÇVÖR“°¢76W'EöW‡7FVE³ÒçF¶U÷föÇVÖR‚’Â6Æ—&ö&Bæ—FV×5³ÒçF¶U÷föÇVÖR“°¢76W'EöW‡7FVE³ÒçF¶U÷â‚’Â6Æ—&ö&Bæ—FV×5³ÒçF¶U÷â“°¢76W'EöW‡7FVE³ÒçF¶U÷Æ–&6µ÷&FR‚’Âã“°¢76W'EöW‡7FVE³ÒææÖR‚’Â6Æ—&ö&Bæ—FV×5³ÒææÖR“°¢76W'B‡7FVBæ—FW"‚’æÆÂ‡Æ—FV×Â—FVÒæ—5öÆö÷÷6÷W&6R‚’’“°¢76W'EöW€¢7FVE³Òç6÷W&6U÷6V7F–öåöÆVæwF…÷6×ÆW2‚’À¢6öÖR†6Æ—&ö&Bæ—FV×5³Òç6÷W&6U÷6V7F–öåöÆVæwF…÷6×ÆW2¢“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R²“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æÆVâ‚’Â"“°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæD–C£¥7FT—FV×2’“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B€¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æÆÂ‡Æ—FV×Â—FVÒç7F'E÷6×ÆR‚’Ò7FU÷7F'B¢“°¢76W'EöW†æVF–õö—FVÕö6Æ—&ö&Bæ5÷&Vb‚’Â6öÖR‚f6Æ—&ö&B’“° ¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢ÆWB&W7F÷&VE÷7FRÒ ¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â—FVÒç7F'E÷6×ÆR‚’ÓÒ7FU÷7F'B¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢76W'EöW‡&W7F÷&VE÷7FRæÆVâ‚’Â"“°¢76W'B‡&W7F÷&VE÷7FU³Òæ—5ö×WFVB‚’“°¢76W'B‚&W7F÷&VE÷7FU³Òæ—5ö×WFVB‚’“°¢76W'B‚&W7F÷&VE÷7FU³Òæ—5÷†6Uö–çfW'FVB‚’“°¢76W'B‡&W7F÷&VE÷7FU³Òæ—5÷†6Uö–çfW'FVB‚’“°¢76W'EöW‡&W7F÷&VE÷7FU³ÒçföÇVÖR‚’Â6Æ—&ö&Bæ—FV×5³ÒçföÇVÖR“°¢76W'EöW€¢&W7F÷&VE÷7FU³ÒçF¶U÷föÇVÖR‚’À¢6Æ—&ö&Bæ—FV×5³ÒçF¶U÷föÇVÖP¢“°¢76W'EöW‡&W7F÷&VE÷7FU³ÒçF¶U÷â‚’Â6Æ—&ö&Bæ—FV×5³ÒçF¶U÷â“°¢76W'EöW‡&W7F÷&VE÷7FU³ÒçF¶U÷Æ–&6µ÷&FR‚’Âã#R“°¢76W'EöW‡&W7F÷&VE÷7FU³ÒææÖR‚’Â6Æ—&ö&Bæ—FV×5³ÒææÖR“°¢76W'EöW‡&W7F÷&VE÷7FU³ÒçföÇVÖR‚’Â6Æ—&ö&Bæ—FV×5³ÒçföÇVÖR“°¢76W'EöW€¢&W7F÷&VE÷7FU³ÒçF¶U÷föÇVÖR‚’À¢6Æ—&ö&Bæ—FV×5³ÒçF¶U÷föÇVÖP¢“°¢76W'EöW‡&W7F÷&VE÷7FU³ÒçF¶U÷â‚’Â6Æ—&ö&Bæ—FV×5³ÒçF¶U÷â“°¢76W'EöW‡&W7F÷&VE÷7FU³ÒææÖR‚’Â6Æ—&ö&Bæ—FV×5³ÒææÖR“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B€¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æÆÂ‡Æ—FV×Â—FVÒç7F'E÷6×ÆR‚’Ò7FU÷7F'B¢“° ¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Ò6öÖR‡G&6µö–G5³Ò“°¢çF–ÖVÆ–æRç6WEöVF—Eö7W'6÷%÷F–6²ƒseóƒ“°¢ÆWB6V6öæE÷7FU÷7F'BÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒseóƒ’çVçw&‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¥7FT—FV×2’“°¢ÆWB6V6öæE÷7FRÒ ¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â—FVÒç7F'E÷6×ÆR‚’ÓÒ6V6öæE÷7FU÷7F'B¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢76W'EöW‡6V6öæE÷7FRæÆVâ‚’Â"“°¢76W'EöW‡6V6öæE÷7FU³ÒçG&6µö–B‚’ÂG&6µö–G5³Ò“°¢76W'EöW‡6V6öæE÷7FU³ÒçG&6µö–B‚’ÂG&6µö–G5³%Ò“°¢76W'B‚6V6öæE÷7FU³Òæ—5÷†6Uö–çfW'FVB‚’“°¢76W'B‡6V6öæE÷7FU³Òæ—5÷†6Uö–çfW'FVB‚’“°¢76W'EöW‡6V6öæE÷7FU³ÒçföÇVÖR‚’Â6Æ—&ö&Bæ—FV×5³ÒçföÇVÖR“°¢76W'EöW€¢6V6öæE÷7FU³ÒçF¶U÷föÇVÖR‚’À¢6Æ—&ö&Bæ—FV×5³ÒçF¶U÷föÇVÖP¢“°¢76W'EöW‡6V6öæE÷7FU³ÒçF¶U÷â‚’Â6Æ—&ö&Bæ—FV×5³ÒçF¶U÷â“°¢76W'EöW‡6V6öæE÷7FU³ÒææÖR‚’Â6Æ—&ö&Bæ—FV×5³ÒææÖR“°¢76W'EöW‡6V6öæE÷7FU³ÒçföÇVÖR‚’Â6Æ—&ö&Bæ—FV×5³ÒçföÇVÖR“°¢76W'EöW€¢6V6öæE÷7FU³ÒçF¶U÷föÇVÖR‚’À¢6Æ—&ö&Bæ—FV×5³ÒçF¶U÷föÇVÖP¢“°¢76W'EöW‡6V6öæE÷7FU³ÒçF¶U÷â‚’Â6Æ—&ö&Bæ—FV×5³ÒçF¶U÷â“°¢76W'EöW‡6V6öæE÷7FU³ÒææÖR‚’Â6Æ—&ö&Bæ—FV×5³ÒææÖR“° ¢ç&ö¦V7EövVæW&F–öâÒç&ö¦V7EövVæW&F–öâç6GW&F–æuöFBƒ“°¢76W'B‚6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæD–C£¥7FT—FV×2’“°§Ğ ¢5·FW7EĞ¦fâ6÷•öÆö÷÷7FUö7&VFW5öÖ—76–æuöFW7F–æF–öå÷G&6·5÷v—F…÷VæFõöæE÷&VFò‚’°¢ÆWBF—&V7F÷'’ÒFV×f–ÆS£§FV×F—"‚’çVçw&‚“°¢ÆWB&ö¦V7E÷F‚ÒF—&V7F÷'’çF‚‚’æ¦ö–â‚&6÷’ÖÆö÷×G&6²Öw&÷wF‚æFr"“°¢ÆWBÖVF–÷&VbÒ&76WC¢òö6÷’ÖÆö÷×G&6²Öw&÷wF‚#°¢ÆWB×WB7F÷&RÒ&ö¦V7E7F÷&S£¦÷Vâ‚g&ö¦V7E÷F‚’çVçw&‚“°¢7F÷&P¢æ–×÷'EöVF–õö76WB†ÖVF–÷&VbÂ&6÷’ÖÆö÷çvb"Â7FC£¦–ó£¤7W'6÷#£¦æWr…³÷Sƒ²EÒ’¢çVçw&‚“°¢7F÷&P¢ç6WEöVF–õö76WEöÖWFFF€¢ÖVF–÷&VbÀ¢fFu÷7F÷&vS£¤VF–ô76WDÖWFFF°¢6öçF–æW#¢'vb"çFõö÷væVB‚’À¢6öFV3¢'6Ò"çFõö÷væVB‚’À¢6×ÆU÷&FS¢6öÖRƒC…ó’À¢6†ææVÅö6÷VçC¢6öÖRƒ’À¢&—G5÷W%÷6×ÆS¢6öÖRƒb’À¢g&ÖUö6÷VçC¢6öÖRƒcó’À¢GW&F–öåöææ÷3¢6öÖRƒ%óSóó’À¢'—FUöÆVã¢6öÖRƒB’À¢ÒÀ¢¢çVçw&‚“°¢7F÷&Ræ6Æ÷6R‚’çVçw&‚“° ¢ÆWB×WBÒ°¢&ö¦V7E÷Fƒ¢6öÖR‡&ö¦V7E÷F‚’À¢âä£¦FVfVÇB‚¢Ó°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWB6÷W&6U÷G&6·2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWB6÷W&6UöÆVæwF‚Òç&ö¦V7Bç6×ÆUöE÷F–6²ƒ•ó#’çVçw&‚“°¢f÷"G&6µö–B–âg6÷W&6U÷G&6·2°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢§G&6µö–BÀ¢ÖVF–÷&Vc¢ÖVF–÷&VbçFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢6÷W&6UöÆVæwF‚À¢Ò¢çVçw&‚“°¢Ğ¢ÆWB6÷W&6Uö—FVÕö–G2Ò ¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æÖ‡Æ—FV×Â†—FVÒæ–B‚’Â—FVÒçG&6µö–B‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡6÷W&6Uö—FVÕö–G2“°¢çF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâÒ6öÖR†7&FS£§F–ÖVÆ–æS£¥F–ÖU6VÆV7F–öâ°¢7F'E÷F–6³¢UóscÀ¢VæE÷F–6³¢5óCCÀ¢Ò“°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¤6÷”Æö÷öe6VÆV7FVD&V’“°¢ÆWB6Æ—&ö&BÒ ¢æVF–õö—FVÕö6Æ—&ö&@¢æ6ÆöæR‚¢æW‡V7B‚&6÷’6†÷VÆB&WF–â&÷F‚6÷W&6RG&6·2"“°¢76W'EöW†6Æ—&ö&Bæ—FV×2æÆVâ‚’Â"“°¢76W'EöW†6Æ—&ö&Bæ—FV×5³ÒçG&6µööfg6WBÂ“°¢76W'EöW†6Æ—&ö&Bæ—FV×5³ÒçG&6µööfg6WBÂ“° ¢æÇ•ö7F–öâ€¢Ft7F–öã£¤FVÆWFUG&6²°¢G&6µö–C¢6÷W&6U÷G&6·5³ÒÀ¢ÒÀ¢%6÷W&6RG&6²FVÆWFVB"À¢“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Ò6öÖR‡6÷W&6U÷G&6·5³Ò“°¢çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²ÒSuóc°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæD–C£¥7FT—FV×2’“°¢ÆWBf—'7E÷7FU÷7F'BÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒSuóc’çVçw&‚“°¢ÆWB&Wf—6–öåö&Vf÷&Uöf—'7E÷7FRÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¥7FT—FV×2’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â"“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’Â2“°¢ÆWBf—'7E÷7FRÒ ¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â—FVÒç7F'E÷6×ÆR‚’ÓÒf—'7E÷7FU÷7F'B¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢76W'EöW†f—'7E÷7FRæÆVâ‚’Â"“°¢76W'EöW†f—'7E÷7FU³ÒçG&6µö–B‚’Â6÷W&6U÷G&6·5³Ò“°¢76W'EöW†f—'7E÷7FU³ÒçG&6µö–B‚’Âç&ö¦V7BçG&6·2‚•³Òæ–B‚’“°¢76W'EöæR†f—'7E÷7FU³ÒçG&6µö–B‚’Â6÷W&6U÷G&6·5³Ò“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&Uöf—'7E÷7FR²“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æÆVâ‚’Â"“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’Â“°¢76W'EöW†æVF–õö—FVÕö6Æ—&ö&Bæ5÷&Vb‚’Â6öÖR‚f6Æ—&ö&B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â"“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’Â2“° ¢ÆWBG…ö–BÒç&Wf—6–öã°¢æÇ•ö7F–öâ€¢Ft7F–öã£¤&F6…G&ç67F–öâ°¢G…ö–BÀ¢7F–öç3¢ ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢ç&Wb‚¢æÖ‡ÇG&6·ÂFt7F–öã£¤FVÆWFUG&6²°¢G&6µö–C¢G&6²æ–B‚’À¢Ò¢æ6öÆÆV7B‚’À¢ÒÀ¢$ÆÂG&6·2FVÆWFVB"À¢“°¢76W'B†ç&ö¦V7BçG&6·2‚’æ—5öV×G’‚’“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚’æ—5öV×G’‚’“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6²ÒæöæS°¢çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²ÒCEó°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæD–C£¥7FT—FV×2’“°¢ÆWBV×G•÷&ö¦V7E÷7FU÷7F'BÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒCEó’çVçw&‚“°¢ÆWB&Wf—6–öåö&Vf÷&UöV×G•÷&ö¦V7E÷7FRÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¥7FT—FV×2’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â"“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’Â"“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚’æ—FW"‚’æÆÂ‡Æ—FV×Â°¢—FVÒç7F'E÷6×ÆR‚’ÓÒV×G•÷&ö¦V7E÷7FU÷7F'@¢bb ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æç’‡ÇG&6·ÂG&6²æ–B‚’ÓÒ—FVÒçG&6µö–B‚’¢Ò’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&UöV×G•÷&ö¦V7E÷7FR²“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B†ç&ö¦V7BçG&6·2‚’æ—5öV×G’‚’“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚’æ—5öV×G’‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â"“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’Â"“°§Ğ ¢5·FW7EĞ¦fâVæ6Æ÷6U÷6VÆV7FVEöVF–õöæEöÖ–F•ö–å÷&¦÷%ö&V5öæE÷7W÷'E÷VæFò‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢f÷"ò–ââã2°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢Ğ¢ÆWBG&6µö–G2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWB6×ÆUóÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒ’çVçw&‚“°¢ÆWB6×ÆUó“cÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒ“c’çVçw&‚“°¢ÆWB6×ÆUó“#Òç&ö¦V7Bç6×ÆUöE÷F–6²ƒó“#’çVçw&‚“°¢ÆWB6×ÆUóSscÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒUósc’çVçw&‚“°¢ÆWB6×ÆUóscƒÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒuócƒ’çVçw&‚“°¢ÆWB6×ÆUóS#Òç&ö¦V7Bç6×ÆUöE÷F–6²ƒóS#’çVçw&‚“°¢ÆWB6×ÆUó3CCÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒ5óCC’çVçw&‚“°¢ÆWB6×ÆUó#3CÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒ#5óC’çVçw&‚“°¢ÆWBöÆEö&VÒ&¦÷$VF—D&V£¦æWr‡6×ÆUóÂ6×ÆUó“c’çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WEG&6µ&¦÷$VF—D&V2°¢G&6µö&V3¢fV2²‡G&6µö–G5³%ÒÂfV2¶öÆEö&VÒ•ÒÀ¢Ò¢çVçw&‚“° ¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6µö–G5³ÒÀ¢ÖVF–÷&Vc¢&76WC¢ò÷&¦÷"×FW7B"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢6×ÆUó“#À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢6×ÆUóSscÒ6×ÆUó“#À¢Ò¢çVçw&‚“°¢ÆWBf—'7EöVF–òÒç&ö¦V7BæVF–õö—FV×2‚’æÆ7B‚’çVçw&‚’æ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6µö–G5³ÒÀ¢ÖVF–÷&Vc¢&76WC¢ò÷&¦÷"×FW7B"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢6×ÆUóS#À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢6×ÆUó3CCÒ6×ÆUóS#À¢Ò¢çVçw&‚“°¢ÆWB6V6öæEöVF–òÒç&ö¦V7BæVF–õö—FV×2‚’æÆ7B‚’çVçw&‚’æ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6µö–G5³ÒÀ¢ÖVF–÷&Vc¢&76WC¢ò÷&¦÷"×FW7B"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢6×ÆUóscƒÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢6×ÆUóS#Ò6×ÆUóscƒÀ¢Ò¢çVçw&‚“°¢ÆWB÷F†W%÷G&6µöVF–òÒç&ö¦V7BæVF–õö—FV×2‚’æÆ7B‚’çVçw&‚’æ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6µö–G5³ÒÀ¢7F'E÷F–6³¢•ó#À¢ÆVæwF…÷F–6·3¢5óƒCÀ¢Ò¢çVçw&‚“°¢ÆWBÖ–F•ö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWB6VÆV7FVEö–G2Ò¶f—'7EöVF–òÂ6V6öæEöVF–òÂ÷F†W%÷G&6µöVF–òÂÖ–F•ö–EÓ°¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2€¢6VÆV7FVEö–G0¢æ—FW"‚¢æÖ‡Æ—FVÕö–GÂ°¢ÆWBG&6µö–BÒ ¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ¦—FVÕö–B¢æÖ‡Æ—FV×Â—FVÒçG&6µö–B‚’¢æ÷%öVÇ6R‡ÇÂ°¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ¦—FVÕö–B¢æÖ‡Æ—FV×Â—FVÒçG&6µö–B‚’¢Ò¢çVçw&‚“°¢‚¦—FVÕö–BÂG&6µö–B¢Ò¢æ6öÆÆV7B‚’À¢“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢ÆWB7F–öå÷&Wf—6–öâÒç&Wf—6–öã°¢ÆWBW‡V7FVEö&VÒ&¦÷$VF—D&V£¦æWr‡6×ÆUó“#Â6×ÆUó#3C’çVçw&‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB€¢6öÖÖæD–C£¤Væ6Æ÷6TÖVF–—FV×4–å&¦÷$VF—BÀ¢’“° ¢76W'B†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ—5öV×G’‚’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òç&¦÷%öVF—Eö&V2‚’Âe¶W‡V7FVEö&VÒ“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òç&¦÷%öVF—Eö&V2‚’Âe¶W‡V7FVEö&VÒ“°¢76W'B†ç&ö¦V7BçG&6·2‚•³%Òç&¦÷%öVF—Eö&V2‚’æ—5öV×G’‚’“°¢76W'EöW†ç&Wf—6–öâÂ7F–öå÷&Wf—6–öâ²“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B†ç&ö¦V7BçG&6·2‚•³Òç&¦÷%öVF—Eö&V2‚’æ—5öV×G’‚’“°¢76W'B†ç&ö¦V7BçG&6·2‚•³Òç&¦÷%öVF—Eö&V2‚’æ—5öV×G’‚’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³%Òç&¦÷%öVF—Eö&V2‚’Âe¶öÆEö&VÒ“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òç&¦÷%öVF—Eö&V2‚’Âe¶W‡V7FVEö&VÒ“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òç&¦÷%öVF—Eö&V2‚’Âe¶W‡V7FVEö&VÒ“° ¢ÆWB&Wf—6–öåö&Vf÷&Uö6ÆV"Òç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB€¢6öÖÖæD–C£¤Væ6Æ÷6TÖVF–—FV×4–å&¦÷$VF—BÀ¢’“°¢76W'B€¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÆÂ‡ÇG&6·ÂG&6²ç&¦÷%öVF—Eö&V2‚’æ—5öV×G’‚’¢“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&Uö6ÆV"²“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òç&¦÷%öVF—Eö&V2‚’Âe¶W‡V7FVEö&VÒ“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òç&¦÷%öVF—Eö&V2‚’Âe¶W‡V7FVEö&VÒ“°§Ğ ¢5·FW7EĞ¦fâG&–Õö—FV×5÷Fõ÷6VÆV7FVEö&Vö6Æ—5öVF–õöæEöÖ–F•ö–åööæU÷VæFö&ÆUöVF—B‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWB6VÆV7F–öå÷7F'E÷F–6²ÒUósc°¢ÆWB6VÆV7F–öåöVæE÷F–6²Ò5óCC°¢ÆWBVF–õ÷6VÆV7F–öå÷7F'BÒ ¢ç&ö¦V7@¢ç6×ÆUöE÷F–6²‡6VÆV7F–öå÷7F'E÷F–6²¢æW‡V7B‚'6VÆV7F–öâ7F'B6†÷VÆBÖFò6×ÆR"“°¢ÆWBVF–õ÷6VÆV7F–öåöVæBÒ ¢ç&ö¦V7@¢ç6×ÆUöE÷F–6²‡6VÆV7F–öåöVæE÷F–6²¢æW‡V7B‚'6VÆV7F–öâVæB6†÷VÆBÖFò6×ÆR"“°¢ÆWBVF–õö—FVÕöVæBÒ ¢ç&ö¦V7@¢ç6×ÆUöE÷F–6²ƒ•ó#¢æW‡V7B‚&—FVÒVæB6†÷VÆBÖFò6×ÆR"“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢ò÷G&–Ò×FW7B"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢#‚À¢ÆVæwF…÷6×ÆW3¢VF–õö—FVÕöVæBÀ¢Ò¢æW‡V7B‚'FW7BVF–ò—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢•ó#À¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWBVF–õö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBÖ–F•ö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRæ†æFÆR…F–ÖVÆ–æTWfVçC£¥6VÆV7D—FVÒ°¢—FVÕö–C¢6öÖR†VF–õö–B’À¢FF—F—fS¢fÇ6RÀ¢&ævS¢fÇ6RÀ¢Ò“°¢çF–ÖVÆ–æRæ†æFÆR…F–ÖVÆ–æTWfVçC£¥6VÆV7D—FVÒ°¢—FVÕö–C¢6öÖR†Ö–F•ö–B’À¢FF—F—fS¢G'VRÀ¢&ævS¢fÇ6RÀ¢Ò“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR…F–ÖVÆ–æTWfVçC£¥6WEF–ÖU6VÆV7F–öâ°¢7F'E÷F–6³¢6VÆV7F–öå÷7F'E÷F–6²À¢VæE÷F–6³¢6VÆV7F–öåöVæE÷F–6²À¢Ò’“° ¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¥G&–Ô—FV×5Fõ6VÆV7FVD&V’“° ¢ÆWBVF–òÒfç&ö¦V7BæVF–õö—FV×2‚•³Ó°¢76W'EöW†VF–òç7F'E÷6×ÆR‚’ÂVF–õ÷6VÆV7F–öå÷7F'B“°¢76W'EöW€¢VF–òæÆVæwF…÷6×ÆW2‚’À¢VF–õ÷6VÆV7F–öåöVæBÒVF–õ÷6VÆV7F–öå÷7F'@¢“°¢76W'EöW€¢VF–òç6÷W&6Uööfg6WE÷6×ÆW2‚’À¢“cC£§G'•ög&öÒƒ#‚²VF–õ÷6VÆV7F–öå÷7F'B’çVçw&‚¢“°¢ÆWBÖ–F’Òfç&ö¦V7BæÖ–F•ö—FV×2‚•³Ó°¢76W'EöW†Ö–F’ç7F'E÷F–6²‚’Â6VÆV7F–öå÷7F'E÷F–6²“°¢76W'EöW€¢Ö–F’æÆVæwF…÷F–6·2‚’À¢6VÆV7F–öåöVæE÷F–6²Ò6VÆV7F–öå÷7F'E÷F–6°¢“°¢76W'EöW†Ö–F’ç6÷W&6Uööfg6WE÷F–6·2‚’Â6VÆV7F–öå÷7F'E÷F–6²“°¢76W'B†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2‚fVF–õö–B’“°¢76W'B†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2‚fÖ–F•ö–B’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢ÆWBVF–òÒfç&ö¦V7BæVF–õö—FV×2‚•³Ó°¢76W'EöW†VF–òæ–B‚’ÂVF–õö–B“°¢76W'EöW†VF–òç7F'E÷6×ÆR‚’Â“°¢76W'EöW†VF–òæÆVæwF…÷6×ÆW2‚’ÂVF–õö—FVÕöVæB“°¢76W'EöW†VF–òç6÷W&6Uööfg6WE÷6×ÆW2‚’Â#‚“°¢ÆWBÖ–F’Òfç&ö¦V7BæÖ–F•ö—FV×2‚•³Ó°¢76W'EöW†Ö–F’æ–B‚’ÂÖ–F•ö–B“°¢76W'EöW†Ö–F’ç7F'E÷F–6²‚’Â“°¢76W'EöW†Ö–F’æÆVæwF…÷F–6·2‚’Â•ó#“°¢76W'EöW†Ö–F’ç6÷W&6Uööfg6WE÷F–6·2‚’Â“°§Ğ ¢5·FW7EĞ¦fâG&–Õö—FV×5÷Fõ÷6VÆV7FVEö&VöÖF6†W5öæF—fUö÷fW&ÆöæEöVæGö–çE÷'VÆW2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWB&ævW2Ò°¢ƒÂ5óƒC’À¢ƒÂUósc’À¢ƒÂuócƒ’À¢ƒUóscÂ5óCC’À¢ƒóS#Â•ó#’À¢ƒ5óCCÂ•ó#’À¢ƒUó3cÂ•ó#’À¢Ó°¢ÆWB×WB–G2ÒfV3£§v—F…ö66—G’‡&ævW2æÆVâ‚’“°¢f÷"‡7F'E÷F–6²ÂVæE÷F–6²’–â&ævW2°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6²À¢ÆVæwF…÷F–6·3¢VæE÷F–6²Ò7F'E÷F–6²À¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢–G2çW6‚†ç&ö¦V7BæÖ–F•ö—FV×2‚’æÆ7B‚’çVçw&‚’æ–B‚’“°¢Ğ¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢f÷"†–æFW‚Â—FVÕö–B’–â–G2æ—FW"‚’æ6÷–VB‚’æVçVÖW&FR‚’°¢çF–ÖVÆ–æRæ†æFÆR…F–ÖVÆ–æTWfVçC£¥6VÆV7D—FVÒ°¢—FVÕö–C¢6öÖR†—FVÕö–B’À¢FF—F—fS¢–æFW‚ÒÀ¢&ævS¢fÇ6RÀ¢Ò“°¢Ğ¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR…F–ÖVÆ–æTWfVçC£¥6WEF–ÖU6VÆV7F–öâ°¢7F'E÷F–6³¢UóscÀ¢VæE÷F–6³¢5óCCÀ¢Ò’“° ¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¥G&–Ô—FV×5Fõ6VÆV7FVD&V“° ¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚’æÆVâ‚’ÂR“°¢76W'B€¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æç’‡Æ—FV×Â—FVÒæ–B‚’ÓÒ–G5³Ò¢“°¢f÷"—FVÕö–B–â¶–G5³%ÒÂ–G5³5ÒÂ–G5³EÒÂ–G5³eÕÒ°¢76W'B€¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æç’‡Æ—FV×Â—FVÒæ–B‚’ÓÒ—FVÕö–B¢“°¢Ğ¢ÆWBÆVgEö÷fW&ÆÒ ¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ–G5³%Ò¢çVçw&‚“°¢76W'EöW†ÆVgEö÷fW&Æç7F'E÷F–6²‚’ÂUósc“°¢76W'EöW†ÆVgEö÷fW&ÆæÆVæwF…÷F–6·2‚’Âó“#“°¢76W'EöW†ÆVgEö÷fW&Æç6÷W&6Uööfg6WE÷F–6·2‚’ÂUósc“°¢ÆWB–çFW&–÷"Ò ¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ–G5³5Ò¢çVçw&‚“°¢76W'EöW†–çFW&–÷"ç7F'E÷F–6²‚’ÂUósc“°¢76W'EöW†–çFW&–÷"æÆVæwF…÷F–6·2‚’Âuócƒ“°¢ÆWB&–v‡Eö÷fW&ÆÒ ¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒæ–B‚’ÓÒ–G5³EÒ¢çVçw&‚“°¢76W'EöW‡&–v‡Eö÷fW&Æç7F'E÷F–6²‚’ÂóS#“°¢76W'EöW‡&–v‡Eö÷fW&ÆæÆVæwF…÷F–6·2‚’Âó“#“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚’æÆVâ‚’Âr“°¢76W'B†–G2æ—FW"‚’æÆÂ‡Æ—FVÕö–GÂ°¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æç’‡Æ—FV×Â—FVÒæ–B‚’ÓÒ¦—FVÕö–B¢Ò’“°§Ğ ¢5·FW7EĞ¦fâG&–Õö—FV×5÷Fõ÷6VÆV7FVEö&V÷v—F†÷WEö÷6VÆV7F–öåö—5ööæö÷‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRæ†æFÆR…F–ÖVÆ–æTWfVçC£¥6VÆV7D—FVÒ°¢—FVÕö–C¢6öÖR†—FVÕö–B’À¢FF—F—fS¢fÇ6RÀ¢&ævS¢fÇ6RÀ¢Ò“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã° ¢ÆWBòÒçWFFR„ÖW76vS£¥G&–Ô—FV×5Fõ6VÆV7FVD&V“° ¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚’Â—FVÕö–B“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’Â“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒæÆVæwF…÷F–6·2‚’Â“c“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ“° ¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR…F–ÖVÆ–æTWfVçC£¥6WEF–ÖU6VÆV7F–öâ°¢7F'E÷F–6³¢#À¢VæE÷F–6³¢CƒÀ¢Ò’“°¢çF–ÖVÆ–æRæ6ÆV%÷6VÆV7FVEö—FV×2‚“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¥G&–Ô—FV×5Fõ6VÆV7FVD&V“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’Â“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒæÆVæwF…÷F–6·2‚’Â“c“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ“°§Ğ ¢5·FW7EĞ¦fâ—FVÕöÖVçU÷&VÖ÷fUö—FV×5öFVÆWFW5÷6VÆV7FVEöÖVF–öæEö—5÷VæFö&ÆR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6VÆV7D—FVÒ°¢—FVÕö–C¢6öÖR†—FVÕö–B’À¢FF—F—fS¢fÇ6RÀ¢&ævS¢fÇ6RÀ¢Ò“° ¢ÆWBVçG&–W2Ò6öÖÖæG3£¦f÷%öÖVçR‚fÂÖ–äÖVçS£¤—FVÒ“°¢76W'EöW†VçG&–W5³Òæ–BÂ6öÖÖæD–C£¤FVÆWFU6VÆV7FVD—FV×2“°¢76W'B†VçG&–W5³ÒæVæ&ÆVB“°¢76W'EöW€¢6öÖÖæG3£¦f–æB‚fÂ'&VÖ÷fR—FV×2"’À¢6öÖR„6öÖÖæD–C£¤FVÆWFU6VÆV7FVD—FV×2¢“°¢76W'EöW€¢6öÖÖæG3£¦f–æB‚fÂ&FVÆWFR6VÆV7FVB—FV×2"’À¢6öÖR„6öÖÖæD–C£¤FVÆWFU6VÆV7FVD—FV×2¢“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¤FVÆWFU6VÆV7FVD—FV×2’“°¢76W'B†ç&ö¦V7BæÖ–F•ö—FV×2‚’æ—5öV×G’‚’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚’Â—FVÕö–B“°§Ğ ¢5·FW7EĞ¦fâFööÆ&%öæE÷G&6µö6öçG&öÇ5÷6†&Uö6öÖÖæEöf–Æ&–Æ—G’‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢76W'B‚6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæD–C£¤FDÖ–F”—FVÒ’“° ¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Ò6öÖR‡G&6µö–B“°¢æ–õö'W7’ÒG'VS° ¢f÷"6öÖÖæB–â°¢6öÖÖæD–C£¤FEG&6²À¢6öÖÖæD–C£¤FDÖ–F”—FVÒÀ¢6öÖÖæD–C£¥G&6²°¢G&6µö–BÀ¢6öÖÖæC¢G&6´6öÖÖæC£¥FövvÆT×WFRÀ¢ÒÀ¢6öÖÖæD–C£¥G&6²°¢G&6µö–BÀ¢6öÖÖæC¢G&6´6öÖÖæC£¥FövvÆU6öÆòÀ¢ÒÀ¢Ò°¢76W'B‚6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°¢Ğ ¢ÆWBG&6µöÖVçRÒ6öÖÖæG3£¦f÷%öÖVçR‚fÂÖ–äÖVçS£¥G&6²“°¢f÷"6öÖÖæB–âµG&6´6öÖÖæC£¥FövvÆT×WFRÂG&6´6öÖÖæC£¥FövvÆU6öÆõÒ°¢ÆWBVçG'’ÒG&6µöÖVçP¢æ—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒ6öÖÖæD–C£¥6VÆV7FVEG&6²†6öÖÖæB’¢æW‡V7B‚%G&6²ÖVçR6†÷VÆBW‡÷6RV6‚G&6²6öçG&öÂ"“°¢76W'EöW†VçG'’æVæ&ÆVBÂ6öÖÖæG3£¦—5öVæ&ÆVB‚fÂVçG'’æ–B’“°¢76W'B‚VçG'’æVæ&ÆVB“°¢Ğ§Ğ ¢5·FW7EĞ¦fâG&6µ÷&V÷&FW%ö—5÷VæFö&ÆR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBf—'7Eö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWB6V6öæEö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤Ö÷fUG&6²‡6V6öæEö–BÂÓ’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òæ–B‚’Â6V6öæEö–B“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òæ–B‚’Âf—'7Eö–B“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òæ–B‚’Âf—'7Eö–B“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òæ–B‚’Â6V6öæEö–B“°§Ğ ¢5·FW7EĞ¦fâG&6µ÷åöF§W7FÖVçEö—5÷VæFö&ÆR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¥&Wf–WuG&6µâ‡G&6µö–BÂã#R’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òçâ‚’Âã“°¢ÆWBòÒçWFFR„ÖW76vS£¤6öÖÖ—EG&6µâ‡G&6µö–B’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òçâ‚’Âã“°¢çG&6µöÖ—…ö6öÖÖ—EöBÒ6öÖR„–ç7FçC£¦æ÷r‚’ÒGW&F–öã£¦g&öÕ÷6V72ƒ’“°¢ÆWBòÒçWFFR„ÖW76vS£¤&6¶w&÷VæEF–6²“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òçâ‚’Âã#R“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òçâ‚’Âã“°§Ğ ¢5·FW7EĞ¦fâ&W6WGF–æu÷G&6µ÷åö—5÷VæFö&ÆR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WEG&6µâ²G&6µö–BÂã¢ãBÒ¢æW‡V7B‚'G&6²â6†÷VÆB&R6WB"“° ¢ÆWBòÒçWFFR„ÖW76vS£¥&W6WEG&6µâ‡G&6µö–B’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òçâ‚’Âã“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òçâ‚’ÂãB“°§Ğ ¢5·FW7EĞ¦fâ&W6WGF–æu÷G&6µ÷föÇVÖUö—5÷VæFö&ÆR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WEG&6µföÇVÖR°¢G&6µö–BÀ¢föÇVÖUöF#¢ÓBãÀ¢Ò¢æW‡V7B‚'G&6²föÇVÖR6†÷VÆB&R6WB"“° ¢ÆWBòÒçWFFR„ÖW76vS£¥&W6WEG&6µföÇVÖR‡G&6µö–B’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒçföÇVÖUöF"‚’Âã“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒçföÇVÖUöF"‚’ÂÓBã“°§Ğ ¢5·FW7EĞ¦fâ&W6WEö'WGFöå÷&W6W'fW5ö÷&V6VçEöÖ—…övW7GW&Uö5ö÷6W&FU÷VæFõ÷7FW‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WEG&6µföÇVÖR°¢G&6µö–BÀ¢föÇVÖUöF#¢ÓBãÀ¢Ò¢æW‡V7B‚'G&6²föÇVÖR6†÷VÆB&R6WB"“° ¢ÆWBòÒçWFFR„ÖW76vS£¥&Wf–WuG&6µföÇVÖR‡G&6µö–BÂÓ‚ã’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6öÖÖ—EG&6µföÇVÖR‡G&6µö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&W6WEG&6µföÇVÖR‡G&6µö–B’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒçföÇVÖUöF"‚’Âã“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒçföÇVÖUöF"‚’ÂÓ‚ã“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒçföÇVÖUöF"‚’ÂÓBã“°§Ğ ¢5·FW7EĞ¦fâG&vv–æu÷G&6µ÷föÇVÖUö6öÖÖ—G5ööæU÷VæFö&ÆUö7F–öâ‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¥&Wf–WuG&6µföÇVÖR‡G&6µö–BÂÓ2ã"’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&Wf–WuG&6µföÇVÖR‡G&6µö–BÂÓRãr’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒçföÇVÖUöF"‚’Âã“° ¢ÆWBòÒçWFFR„ÖW76vS£¤6öÖÖ—EG&6µföÇVÖR‡G&6µö–B’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒçföÇVÖUöF"‚’Âã“°¢çG&6µöÖ—…ö6öÖÖ—EöBÒ6öÖR„–ç7FçC£¦æ÷r‚’ÒGW&F–öã£¦g&öÕ÷6V72ƒ’“°¢ÆWBòÒçWFFR„ÖW76vS£¤&6¶w&÷VæEF–6²“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒçföÇVÖUöF"‚’ÂÓRãr“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒçföÇVÖUöF"‚’Âã“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B†ç&ö¦V7BçG&6·2‚’æ—5öV×G’‚’“°§Ğ ¢5·FW7EĞ¦fâÖ7FW%övW7GW&Uö6öÖÖ—G5ööæ6UöæE÷VæFõ÷&W7F÷&W5ö&÷F…ö6öçG&öÇ2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥&Wf–WtÖ7FW%föÇVÖR‚Ó2ã’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&Wf–WtÖ7FW%föÇVÖR‚Ó"ã’“°¢76W'EöW†ç&ö¦V7BæÖ7FW%öÖ—‚‚’çföÇVÖUöF"‚’Âã“°¢ÆWBòÒçWFFR„ÖW76vS£¤6öÖÖ—DÖ7FW$Ö—‚“°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæG3£¤6öÖÖæD–C£¥VæFò’“°¢76W'B‚6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæG3£¤6öÖÖæD–C£¥&VFò’“°¢æÖ7FW%öÖ—…ö6öÖÖ—EöBÒ6öÖR„–ç7FçC£¦æ÷r‚’ÒGW&F–öã£¦g&öÕ÷6V72ƒ’“°¢ÆWBòÒçWFFR„ÖW76vS£¤&6¶w&÷VæEF–6²“°¢76W'EöW†ç&ö¦V7BæÖ7FW%öÖ—‚‚’çföÇVÖUöF"‚’ÂÓ"ã“°¢ÆWBòÒçWFFR„ÖW76vS£¥&Wf–WtÖ7FW%âƒãR’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6öÖÖ—DÖ7FW$Ö—‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæÖ7FW%öÖ—‚‚’çâ‚’Âã“°¢76W'EöW†ç&ö¦V7BæÖ7FW%öÖ—‚‚’çföÇVÖUöF"‚’ÂÓ"ã“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæÖ7FW%öÖ—‚‚’ÂFuö6÷&S£¤Ö7FW$Ö—ƒ£¦FVfVÇB‚’“°¢76W'B‚ç&ö¦V7Bæ6å÷VæFò‚’“°§Ğ ¢5·FW7EĞ¦fâÖ7FW%ö6æ6VÅöæEöF÷V&ÆUö6Æ–6µöFõöæ÷Eö6öÖÖ—Eöf—'7Eö6Æ–6µ÷&Wf–Wr‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ7FW$Ö—‚°¢Ö—ƒ¢Fuö6÷&S£¤Ö7FW$Ö—ƒ£¦æWr‚ÓbãÂãR’çVçw&‚’À¢Ò¢çVçw&‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥&Wf–WtÖ7FW%föÇVÖR‚Ó#ã’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6æ6VÄÖ7FW$Ö—‚“°¢76W'EöW†ç&ö¦V7BæÖ7FW%öÖ—‚‚’çföÇVÖUöF"‚’ÂÓbã“°¢ÆWBòÒçWFFR„ÖW76vS£¥&Wf–WtÖ7FW%â‚Óã‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤W66R“°¢76W'EöW†ç&ö¦V7BæÖ7FW%öÖ—‚‚’çâ‚’ÂãR“°¢ÆWBòÒçWFFR„ÖW76vS£¥&Wf–WtÖ7FW%föÇVÖR‚Ó#ã’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6öÖÖ—DÖ7FW$Ö—‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥&W6WDÖ7FW%föÇVÖR“°¢76W'EöW†ç&ö¦V7BæÖ7FW%öÖ—‚‚’çföÇVÖUöF"‚’Âã“°¢76W'EöW†ç&ö¦V7BæÖ7FW%öÖ—‚‚’çâ‚’ÂãR“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæÖ7FW%öÖ—‚‚’çföÇVÖUöF"‚’ÂÓbã“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæÖ7FW%öÖ—‚‚’ÂFuö6÷&S£¤Ö7FW$Ö—ƒ£¦FVfVÇB‚’“°§Ğ ¢5·FW7EĞ¦fâf7F÷'•öfFW%÷&ævUöæE÷6–ÆVçEöVæGö–çE÷&W6W'fU÷VæFõöæEöf–æ—FU÷7F÷&vR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWB–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥G&6µföÇVÖUFW‡D6†ævVB†–BÂ#""æ–çFò‚’’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6öÖÖ—EG&6µföÇVÖUFW‡B†–B’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒçföÇVÖUöF"‚’Â"ã“°¢ÆWBòÒçWFFR„ÖW76vS£¥G&6µföÇVÖUFW‡D6†ævVB†–BÂ##"æ–çFò‚’’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6öÖÖ—EG&6µföÇVÖUFW‡B†–B’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒçföÇVÖUöF"‚’Â#ã“°¢ÆWBòÒçWFFR„ÖW76vS£¥G&6µföÇVÖUFW‡D6†ævVB†–BÂ"Ö–æb"æ–çFò‚’’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6öÖÖ—EG&6µföÇVÖUFW‡B†–B’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒçföÇVÖUöF"‚’ÂfFW#£¥4”ÄTä4UôD"“°¢76W'B†ç&ö¦V7BçG&6·2‚•³ÒçföÇVÖUöF"‚’æ—5öf–æ—FR‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒçföÇVÖUöF"‚’Â#ã“°¢ÆWBòÒçWFFR„ÖW76vS£¥&Wf–WtÖ7FW%föÇVÖR†fFW#£¦g&öÕ÷÷6—F–öâƒã’’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6öÖÖ—DÖ7FW$Ö—‚“°¢æÖ7FW%öÖ—…ö6öÖÖ—EöBÒ6öÖR„–ç7FçC£¦æ÷r‚’ÒGW&F–öã£¦g&öÕ÷6V72ƒ’“°¢ÆWBòÒçWFFR„ÖW76vS£¤&6¶w&÷VæEF–6²“°¢76W'EöW†ç&ö¦V7BæÖ7FW%öÖ—‚‚’æ6†ææVÅöv–ç2‚’Â³ãÂãÒ“°¢76W'EöW€¢&ö¦V7C£¦g&öÕ÷6æ6†÷B†ç&ö¦V7Bç6æ6†÷B‚’¢çVçw&‚¢æÖ7FW%öÖ—‚‚¢æ6†ææVÅöv–ç2‚’À¢³ãÂãĞ¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæÖ7FW%öÖ—‚‚’çföÇVÖUöF"‚’Âã“°§Ğ ¢5·FW7EĞ¦fâ&–v‡Eö6Æ–6µö6æ6VÇ5ö÷G&6µöÖ—…÷6Æ–FW%övW7GW&R‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WEG&6µföÇVÖR°¢G&6µö–BÀ¢föÇVÖUöF#¢ÓbãÀ¢Ò¢æW‡V7B‚'G&6²föÇVÖR6†÷VÆB&R6WB"“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã° ¢ÆWBòÒçWFFR„ÖW76vS£¥&Wf–WuG&6µföÇVÖR‡G&6µö–BÂÓ"ã’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6æ6VÅG&6´Ö—„vW7GW&R“° ¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒçföÇVÖUöF"‚’ÂÓbã“°¢76W'B†çG&6µöÖ—…övW7GW&Ræ—5öæöæR‚’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ“°§Ğ ¢5·FW7EĞ¦fâF÷V&ÆUö6Æ–6µ÷G&6µöÖ—…÷&W6WEö6öÆW66W5÷F†U÷VæF–æuö6Æ–6µö–çFõööæU÷VæFõ÷7FW‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WEG&6µföÇVÖR°¢G&6µö–BÀ¢föÇVÖUöF#¢ÓBãÀ¢Ò¢æW‡V7B‚'G&6²föÇVÖR6†÷VÆB&R6WB"“° ¢ÆWBòÒçWFFR„ÖW76vS£¥&Wf–WuG&6µföÇVÖR‡G&6µö–BÂÓ‚ã’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6öÖÖ—EG&6µföÇVÖR‡G&6µö–B’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒçföÇVÖUöF"‚’ÂÓBã“°¢ÆWBòÒçWFFR„ÖW76vS£¥&W6WEG&6µföÇVÖT'”F÷V&ÆT6Æ–6²‡G&6µö–B’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒçföÇVÖUöF"‚’Âã“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒçföÇVÖUöF"‚’ÂÓBã“°§Ğ ¢5·FW7EĞ¦fâG&6µöFVÆWFUö—5÷VæFö&ÆR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤FVÆWFUG&6²‡G&6µö–B’“°¢76W'B†ç&ö¦V7BçG&6·2‚’æ—5öV×G’‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òæ–B‚’ÂG&6µö–B“°§Ğ ¢5·FW7EĞ¦fâFVÆWFUö'•öfö7W5÷&VfW'5÷6VÆV7FVEö—FV×5öæEöFVÆWFW5÷6VÆV7FVE÷G&6·5öFöÖ–6ÆÇ’‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBÖ–å÷v–æF÷uö–BÒ–6VC£§v–æF÷s£¤–C£§Væ—VR‚“°¢æÖ–å÷v–æF÷uö–BÒ6öÖR†Ö–å÷v–æF÷uö–B“°¢ÆWBfö7W6VEö¶W•öWfVçBÒÆ¶W’Â‡—6–6Åö¶W—Â°¢ÖW76vS£¥'VçF–ÖT¶W–&ö&DWfVçB€¢–6VC£¤WfVçC£¤¶W–&ö&B†–6VC£¦¶W–&ö&C£¤WfVçC£¤¶W•&W76VB°¢¶W“¢–6VC£¦¶W–&ö&C£¤¶W“£¤æÖVB†¶W’’À¢ÖöF–f–VEö¶W“¢–6VC£¦¶W–&ö&C£¤¶W“£¤æÖVB†¶W’’À¢‡—6–6Åö¶W“¢–6VC£¦¶W–&ö&C£¦¶W“£¥‡—6–6Ã£¤6öFR‡‡—6–6Åö¶W’’À¢Æö6F–öã¢–6VC£¦¶W–&ö&C£¤Æö6F–öã£¥7FæF&BÀ¢ÖöF–f–W'3¢ÖöF–f–W'3£¤äôäRÀ¢FW‡C¢æöæRÀ¢&WVC¢fÇ6RÀ¢Ò’À¢–6VC£¦WfVçC£¥7FGW3£¤–væ÷&VBÀ¢Ö–å÷v–æF÷uö–BÀ¢¢Ó°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWB&VçBÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWB6†–ÆBÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥&÷WF–æt6†ævR„Ft7F–öã£¥6WEG&6´föÆFW"°¢G&6µö–C¢&VçBÀ¢Væ&ÆVC¢G'VRÀ¢Ò’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&÷WF–æt6†ævR„Ft7F–öã£¥6WEG&6µ&VçB°¢G&6µö–C¢6†–ÆBÀ¢&VçC¢6öÖR‡&VçB’À¢Ò’“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢6†–ÆBÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢æW‡V7B‚'FW7BÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR€¢7WW#£§7WW#£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6VÆV7EG&6µv—F„ÖöF–f–W'2°¢G&6µö–C¢&VçBÀ¢ÖöF–f–W'3¢ÖöF–f–W'3£¤äôäRÀ¢ÒÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR€¢7WW#£§7WW#£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6VÆV7EG&6µv—F„ÖöF–f–W'2°¢G&6µö–C¢6†–ÆBÀ¢ÖöF–f–W'3¢ÖöF–f–W'3£¤5E$ÂÀ¢ÒÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR€¢7WW#£§7WW#£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6VÆV7D—FVÒ°¢—FVÕö–C¢6öÖR†—FVÕö–B’À¢FF—F—fS¢fÇ6RÀ¢&ævS¢fÇ6RÀ¢ÒÀ¢’“° ¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¤FVÆWFT'”fö7W4æõ&ö×F–ær’“° ¢76W'B†ç&ö¦V7BæÖ–F•ö—FV×2‚’æ—5öV×G’‚’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â"“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚’Â—FVÕö–B“° ¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR€¢7WW#£§7WW#£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6VÆV7D—FVÒ°¢—FVÕö–C¢6öÖR†—FVÕö–B’À¢FF—F—fS¢fÇ6RÀ¢&ævS¢fÇ6RÀ¢ÒÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR€¢7WW#£§7WW#£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6VÆV7EG&6µv—F„ÖöF–f–W'2°¢G&6µö–C¢&VçBÀ¢ÖöF–f–W'3¢ÖöF–f–W'3£¤äôäRÀ¢ÒÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR€¢7WW#£§7WW#£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6VÆV7EG&6µv—F„ÖöF–f–W'2°¢G&6µö–C¢6†–ÆBÀ¢ÖöF–f–W'3¢ÖöF–f–W'3£¤5E$ÂÀ¢ÒÀ¢’“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¤FVÆWFT'”fö7W2’“° ¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â"“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚’æÆVâ‚’Â“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ“°¢76W'EöW€¢çVæF–æuöfö7W6VE÷G&6µöFVÆWF–öâæ5öFW&Vb‚’À¢6öÖR‚e·&VçBÂ6†–ÆEÕ²âåÒ¢“°¢ÆWBòÒçWFFR†fö7W6VEö¶W•öWfVçB€¢–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤W66RÀ¢–6VC£¦¶W–&ö&C£¦¶W“£¤6öFS£¤W66RÀ¢’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â"“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ“° ¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¤FVÆWFT'”fö7W2’“°¢76W'B†çVæF–æuöfö7W6VE÷G&6µöFVÆWF–öâæ—5÷6öÖR‚’“°¢ÆWBòÒçWFFR†fö7W6VEö¶W•öWfVçB€¢–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤VçFW"À¢–6VC£¦¶W–&ö&C£¦¶W“£¤6öFS£¤VçFW"À¢’“°¢76W'B†ç&ö¦V7BçG&6·2‚’æ—5öV×G’‚’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â"“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òæ–B‚’Â&VçB“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òæ–B‚’Â6†–ÆB“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òç&VçE÷G&6²‚’Â6öÖR‡&VçB’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚’Â—FVÕö–B“° ¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR€¢7WW#£§7WW#£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6VÆV7EG&6µv—F„ÖöF–f–W'2°¢G&6µö–C¢&VçBÀ¢ÖöF–f–W'3¢ÖöF–f–W'3£¤äôäRÀ¢ÒÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR€¢7WW#£§7WW#£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6VÆV7EG&6µv—F„ÖöF–f–W'2°¢G&6µö–C¢6†–ÆBÀ¢ÖöF–f–W'3¢ÖöF–f–W'3£¤5E$ÂÀ¢ÒÀ¢’“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¤FVÆWFT'”fö7W4æõ&ö×F–ær’“°¢76W'B†çVæF–æuöfö7W6VE÷G&6µöFVÆWF–öâæ—5öæöæR‚’“°¢76W'B†ç&ö¦V7BçG&6·2‚’æ—5öV×G’‚’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â"“°§Ğ ¢5·FW7EĞ¦fâ'W5÷G&6µö7&VF–öåöæE÷&÷WF–æuö&Uöf–Æ&ÆUö5÷VæFö&ÆUö7F–öç2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWB6÷W&6RÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FD'W5G&6²“°¢ÆWB'W2Òç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢76W'B†ç&ö¦V7BçG&6·2‚•³Òæ—5ö'W2‚’“° ¢ÆWBòÒçWFFR„ÖW76vS£¥6WEG&6´÷WGWB‡6÷W&6RÂ6öÖR†'W2’’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òæ÷WGWE÷G&6²‚’Â6öÖR†'W2’“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òæ÷WGWE÷G&6²‚’ÂæöæR“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òæ÷WGWE÷G&6²‚’Â6öÖR†'W2’“°§Ğ ¢5·FW7EĞ¦fâVF–õ÷F–ÖVÆ–æUöW†7E÷÷6—F–öåöVF—E÷&W6W'fW5÷6÷W&6UöæEö—5÷VæFö&ÆR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢òöW†7B×÷6—F–öâ"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢#CÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢#À¢ÆVæwF…÷6×ÆW3¢“cÀ¢Ò¢æW‡V7B‚'6÷W&6R—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤VF–ô—FVÕ7F'E6×ÆT6†ævVB€¢—FVÕö–BÀ¢##3CR"çFõö÷væVB‚’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6öÖÖ—DVF–ô—FVÕ7F'E6×ÆR†—FVÕö–B’“°¢ÆWB—FVÒÒfç&ö¦V7BæVF–õö—FV×2‚•³Ó°¢76W'EöW†—FVÒç7F'E÷6×ÆR‚’Â%ó3CR“°¢76W'EöW†—FVÒç6÷W&6Uööfg6WE÷6×ÆW2‚’Â#“°¢76W'EöW†—FVÒæÆVæwF…÷6×ÆW2‚’Â“c“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òç7F'E÷6×ÆR‚’Â#C“°§Ğ ¢5·FW7EĞ¦fâÖ—†VEö—FVÕöG&uöÖ÷fW5ö5ööæU÷VæFö&ÆUö7F–öåöæE÷&W6W'fW5ö6öçFVçB‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢çF–ÖVÆ–æP¢æ†æFÆR…F–ÖVÆ–æTWfVçC£¥6WE6æw&–B…6æw&–C£¥6—‡FVVçF‚’“°¢f÷"ò–ââã2°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢Ğ¢ÆWBG&6·2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢ÖVF–÷&Vc¢&76WC¢òöG&rÖVF–ò"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢ç&ö¦V7Bç6×ÆUöE÷F–6²ƒCƒ’çVçw&‚’À¢6÷W&6Uööfg6WE÷6×ÆW3¢s2À¢ÆVæwF…÷6×ÆW3¢%óÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢“cÀ¢ÆVæwF…÷F–6·3¢5óƒCÀ¢Ò¢çVçw&‚“°¢ÆWBVF–õö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBÖ–F•ö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–C¢Ö–F•ö–BÀ¢æ÷FW3¢fV2´Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢crÀ¢F–6³¢CƒÀ¢GW&F–öã¢s#À¢fVÆö6—G“¢“BÀ¢öfe÷fVÆö6—G“¢À¢ÕÒÀ¢Ò¢çVçw&‚“°¢ÆWBVF–õö&Vf÷&RÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ6ÆöæR‚“°¢ÆWBÖ–F•öæ÷FW5ö&Vf÷&RÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚’çFõ÷fV2‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6VÆV7D—FVÒ°¢—FVÕö–C¢6öÖR†VF–õö–B’À¢FF—F—fS¢fÇ6RÀ¢&ævS¢fÇ6RÀ¢Ò“°¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6VÆV7D—FVÒ°¢—FVÕö–C¢6öÖR†Ö–F•ö–B’À¢FF—F—fS¢G'VRÀ¢&ævS¢fÇ6RÀ¢Ò“°¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤&Vv–ä—FVÔG&r°¢—FVÕö–C¢VF–õö–BÀ¢ö–çFW%öFVÇF÷F–6·3¢SÀ¢F&vWE÷G&6µö–æFWƒ¢6öÖRƒ’À¢&ævS¢fÇ6RÀ¢–væ÷&U÷6æ¢fÇ6RÀ¢6÷“¢fÇ6RÀ¢Ò“° ¢æf–æ—6…ö—FVÕöG&r‚“° ¢ÆWBVF–õögFW"Òfç&ö¦V7BæVF–õö—FV×2‚•³Ó°¢ÆWBÖ–F•ögFW"Òfç&ö¦V7BæÖ–F•ö—FV×2‚•³Ó°¢76W'EöW†VF–õögFW"çG&6µö–B‚’ÂG&6·5³Ò“°¢76W'EöW€¢VF–õögFW"ç7F'E÷6×ÆR‚’À¢ç&ö¦V7Bç6×ÆUöE÷F–6²ƒs#’çVçw&‚¢“°¢76W'EöW†VF–õögFW"æÖVF–÷&Vb‚’ÂVF–õö&Vf÷&RæÖVF–÷&Vb‚’“°¢76W'EöW†VF–õögFW"ç6÷W&6Uööfg6WE÷6×ÆW2‚’Âs2“°¢76W'EöW†VF–õögFW"æÆVæwF…÷6×ÆW2‚’Â%ó“°¢76W'EöW†Ö–F•ögFW"çG&6µö–B‚’ÂG&6·5³%Ò“°¢76W'EöW†Ö–F•ögFW"ç7F'E÷F–6²‚’Âó#“°¢76W'EöW†Ö–F•ögFW"æÆVæwF…÷F–6·2‚’Â5óƒC“°¢76W'EöW†Ö–F•ögFW"ææ÷FW2‚’ÂÖ–F•öæ÷FW5ö&Vf÷&R“°¢76W'EöW†ç&Wf—6–öâÂB“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒÂVF–õö&Vf÷&R“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒçG&6µö–B‚’ÂG&6·5³Ò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’Â“c“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚’ÂÖ–F•öæ÷FW5ö&Vf÷&R“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒçG&6µö–B‚’ÂG&6·5³Ò“°¢76W'EöW€¢ç&ö¦V7BæVF–õö—FV×2‚•³Òç7F'E÷6×ÆR‚’À¢ç&ö¦V7Bç6×ÆUöE÷F–6²ƒs#’çVçw&‚¢“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒçG&6µö–B‚’ÂG&6·5³%Ò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’Âó#“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚’ÂÖ–F•öæ÷FW5ö&Vf÷&R“°§Ğ ¢5·FW7EĞ¦fâÖ—†VEö—FVÕö6÷•öG&uöGWÆ–6FW5öVF–õöæEöÖ–F•ö5ööæU÷VæFö&ÆUö7F–öâ‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢çF–ÖVÆ–æP¢æ†æFÆR…F–ÖVÆ–æTWfVçC£¥6WE6æw&–B…6æw&–C£¥6—‡FVVçF‚’“°¢f÷"ò–ââã2°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢Ğ¢ÆWBG&6·2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢ÖVF–÷&Vc¢&76WC¢òö6÷’ÖG&r"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢ç&ö¦V7Bç6×ÆUöE÷F–6²ƒCƒ’çVçw&‚’À¢6÷W&6Uööfg6WE÷6×ÆW3¢“rÀ¢ÆVæwF…÷6×ÆW3¢…óÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢“cÀ¢ÆVæwF…÷F–6·3¢ó“#À¢Ò¢çVçw&‚“°¢ÆWBVF–õö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBÖ–F•ö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–C¢Ö–F•ö–BÀ¢æ÷FW3¢fV2´Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢crÀ¢F–6³¢#À¢GW&F–öã¢3cÀ¢fVÆö6—G“¢ƒ‚À¢öfe÷fVÆö6—G“¢À¢ÕÒÀ¢Ò¢çVçw&‚“°¢ÆWBVF–õö&Vf÷&RÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ6ÆöæR‚“°¢ÆWBÖ–F•ö&Vf÷&RÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ6ÆöæR‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6VÆV7D—FVÒ°¢—FVÕö–C¢6öÖR†VF–õö–B’À¢FF—F—fS¢fÇ6RÀ¢&ævS¢fÇ6RÀ¢Ò“°¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6VÆV7D—FVÒ°¢—FVÕö–C¢6öÖR†Ö–F•ö–B’À¢FF—F—fS¢G'VRÀ¢&ævS¢fÇ6RÀ¢Ò“°¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤&Vv–ä—FVÔG&r°¢—FVÕö–C¢VF–õö–BÀ¢ö–çFW%öFVÇF÷F–6·3¢#CÀ¢F&vWE÷G&6µö–æFWƒ¢6öÖRƒ’À¢&ævS¢fÇ6RÀ¢–væ÷&U÷6æ¢fÇ6RÀ¢6÷“¢G'VRÀ¢Ò“°¢ÆWB&Wf—6–öåö&Vf÷&Uö6÷’Òç&Wf—6–öã° ¢æf–æ—6…ö—FVÕöG&r‚“° ¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’Â"“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚’æÆVâ‚’Â"“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒÂVF–õö&Vf÷&R“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒÂÖ–F•ö&Vf÷&R“°¢ÆWBVF–õö6÷’Òfç&ö¦V7BæVF–õö—FV×2‚•³Ó°¢76W'EöW†VF–õö6÷’çG&6µö–B‚’ÂG&6·5³Ò“°¢76W'EöW€¢VF–õö6÷’ç7F'E÷6×ÆR‚’À¢ç&ö¦V7Bç6×ÆUöE÷F–6²ƒs#’çVçw&‚¢“°¢76W'EöW†VF–õö6÷’æÖVF–÷&Vb‚’ÂVF–õö&Vf÷&RæÖVF–÷&Vb‚’“°¢76W'EöW€¢VF–õö6÷’ç6÷W&6Uööfg6WE÷6×ÆW2‚’À¢VF–õö&Vf÷&Rç6÷W&6Uööfg6WE÷6×ÆW2‚¢“°¢76W'EöW†VF–õö6÷’æÆVæwF…÷6×ÆW2‚’ÂVF–õö&Vf÷&RæÆVæwF…÷6×ÆW2‚’“°¢76W'EöæR†VF–õö6÷’æ–B‚’ÂVF–õö&Vf÷&Ræ–B‚’“°¢ÆWBÖ–F•ö6÷’Òfç&ö¦V7BæÖ–F•ö—FV×2‚•³Ó°¢76W'EöW†Ö–F•ö6÷’çG&6µö–B‚’ÂG&6·5³%Ò“°¢76W'EöW†Ö–F•ö6÷’ç7F'E÷F–6²‚’Âó#“°¢76W'EöW†Ö–F•ö6÷’ææ÷FW2‚•³Òç—F6‚‚’Âcr“°¢76W'EöæR†Ö–F•ö6÷’æ–B‚’ÂÖ–F•ö&Vf÷&Ræ–B‚’“°¢76W'EöæR†Ö–F•ö6÷’ææ÷FW2‚•³Òæ–B‚’ÂÖ–F•ö&Vf÷&Rææ÷FW2‚•³Òæ–B‚’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&Uö6÷’²“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW€¢ç&ö¦V7BæVF–õö—FV×2‚’À¢7FC£§6Æ–6S£¦g&öÕ÷&Vb‚fVF–õö&Vf÷&R¢“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚’Â7FC£§6Æ–6S£¦g&öÕ÷&Vb‚fÖ–F•ö&Vf÷&R’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’Â"“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚’æÆVâ‚’Â"“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒÂVF–õö&Vf÷&R“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒÂÖ–F•ö&Vf÷&R“°§Ğ ¢5·FW7EĞ¦fâVF–õö—FVÕöVFvU÷G&–Õ÷&W6W'fW5÷6÷W&6Uö6öçFVçEöæEö—5ööæU÷VæFö&ÆUöVF—B‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢çF–ÖVÆ–æP¢æ†æFÆR…F–ÖVÆ–æTWfVçC£¥6WE6æw&–B…6æw&–C£¥6—‡FVVçF‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWB7F'BÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒCƒ’çVçw&‚“°¢ÆWBVæBÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒóCC’çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢ò÷G&–Ò×FW7B"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢7F'BÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢#SbÀ¢ÆVæwF…÷6×ÆW3¢VæBÒ7F'BÀ¢Ò¢çVçw&‚“°¢æVF–õ÷vfVf÷&×2æ–ç6W'B€¢&76WC¢ò÷G&–Ò×FW7B"çFõö÷væVB‚’À¢6–ÆVçE÷FW7E÷vfVf÷&ÒƒCEóÂó’À¢“°¢ÆWB—FVÕö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWB÷&–v–æÂÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ6ÆöæR‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“° ¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤&Vv–ä—FVÕG&–Ò°¢—FVÕö–BÀ¢VFvS¢7&FS£§F–ÖVÆ–æS£¤—FVÕG&–ÔVFvS£¥7F'BÀ¢F&vWE÷F–6³¢sÀ¢–væ÷&U÷6æ¢G'VRÀ¢Ò“°¢76W'EöW†çF–ÖVÆ–æRæ—FVÕ÷G&–Õ÷&Wf–Wr‚’çVçw&‚’ç7F'E÷F–6²Âs“°¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤6æ6VÄ—FVÕG&–Ò“°¢76W'B†çF–ÖVÆ–æRæ—FVÕ÷G&–Õ÷&Wf–Wr‚’æ—5öæöæR‚’“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒÂ÷&–v–æÂ“° ¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤&Vv–ä—FVÕG&–Ò°¢—FVÕö–BÀ¢VFvS¢7&FS£§F–ÖVÆ–æS£¤—FVÕG&–ÔVFvS£¥7F'BÀ¢F&vWE÷F–6³¢cSÀ¢–væ÷&U÷6æ¢fÇ6RÀ¢Ò“°¢ÆWB&Wf–WrÒçF–ÖVÆ–æRæ—FVÕ÷G&–Õ÷&Wf–Wr‚’çVçw&‚“°¢76W'B‡&Wf–WrçfÆ–B“°¢76W'EöW‡&Wf–Wrç7F'E÷F–6²Âs#“°¢æf–æ—6…ö—FVÕ÷G&–Ò‚“° ¢ÆWBG&–ÖÖVBÒfç&ö¦V7BæVF–õö—FV×2‚•³Ó°¢ÆWBG&–ÖÖVE÷7F'BÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒs#’çVçw&‚“°¢ÆWBG&–ÖÖVE÷6÷W&6Uööfg6WBÒG&–ÖÖVBç6÷W&6Uööfg6WE÷6×ÆW2‚“°¢76W'EöW‡G&–ÖÖVBç7F'E÷6×ÆR‚’ÂG&–ÖÖVE÷7F'B“°¢76W'EöW‡G&–ÖÖVBç6÷W&6Uööfg6WE÷6×ÆW2‚’ÂUósc‚“°¢76W'EöW‡G&–ÖÖVBç7F'E÷6×ÆR‚’²G&–ÖÖVBæÆVæwF…÷6×ÆW2‚’ÂVæB“°¢76W'EöW†ç&Wf—6–öâÂ"“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒÂ÷&–v–æÂ“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òç7F'E÷6×ÆR‚’ÂG&–ÖÖVE÷7F'B“° ¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤&Vv–ä—FVÕG&–Ò°¢—FVÕö–BÀ¢VFvS¢7&FS£§F–ÖVÆ–æS£¤—FVÕG&–ÔVFvS£¤VæBÀ¢F&vWE÷F–6³¢ó#À¢–væ÷&U÷6æ¢fÇ6RÀ¢Ò“°¢æf–æ—6…ö—FVÕ÷G&–Ò‚“°¢ÆWB&–v‡E÷G&–ÖÖVBÒfç&ö¦V7BæVF–õö—FV×2‚•³Ó°¢ÆWB&–v‡E÷G&–ÖÖVE÷7F'BÒ&–v‡E÷G&–ÖÖVBç7F'E÷6×ÆR‚“°¢ÆWB&–v‡E÷G&–ÖÖVEööfg6WBÒ&–v‡E÷G&–ÖÖVBç6÷W&6Uööfg6WE÷6×ÆW2‚“°¢76W'EöW‡&–v‡E÷G&–ÖÖVE÷7F'BÂG&–ÖÖVE÷7F'B“°¢76W'EöW‡&–v‡E÷G&–ÖÖVEööfg6WBÂG&–ÖÖVE÷6÷W&6Uööfg6WB“°¢76W'EöW€¢&–v‡E÷G&–ÖÖVBç7F'E÷6×ÆR‚’²&–v‡E÷G&–ÖÖVBæÆVæwF…÷6×ÆW2‚’À¢ç&ö¦V7Bç6×ÆUöE÷F–6²ƒó#’çVçw&‚¢“° ¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤&Vv–ä—FVÕG&–Ò°¢—FVÕö–BÀ¢VFvS¢7&FS£§F–ÖVÆ–æS£¤—FVÕG&–ÔVFvS£¥7F'BÀ¢F&vWE÷F–6³¢cÀ¢–væ÷&U÷6æ¢G'VRÀ¢Ò“°¢æf–æ—6…ö—FVÕ÷G&–Ò‚“°¢ÆWB&W7F÷&VEöÆVgBÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ6ÆöæR‚“°¢ÆWB&W7F÷&VE÷7F'BÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒc’çVçw&‚“°¢76W'EöW‡&W7F÷&VEöÆVgBç7F'E÷6×ÆR‚’Â&W7F÷&VE÷7F'B“°¢76W'EöW‡&W7F÷&VEöÆVgBç6÷W&6Uööfg6WE÷6×ÆW2‚’Â5ó"“°¢76W'EöW€¢&W7F÷&VEöÆVgBç6÷W&6Uööfg6WE÷6×ÆW2‚¢²“cC£§G'•ög&öÒ€¢7WW#£§66ÆU÷&ö¦V7E÷6×ÆW5÷Fõ÷6÷W&6Uög&ÖW5÷F÷v&E÷¦W&ò€¢“#ƒ£¦g&öÒ‡G&–ÖÖVE÷7F'BÒ&W7F÷&VE÷7F'B’À¢CEóÀ¢C…óÀ¢¢çVçw&‚¢¢çVçw&‚’À¢&–v‡E÷G&–ÖÖVEööfg6W@¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òç7F'E÷6×ÆR‚’ÂG&–ÖÖVE÷7F'B“°¢76W'EöW€¢ç&ö¦V7BæVF–õö—FV×2‚•³Òç6÷W&6Uööfg6WE÷6×ÆW2‚’À¢&–v‡E÷G&–ÖÖVEööfg6W@¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒÂ&W7F÷&VEöÆVgB“° ¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤&Vv–ä—FVÕG&–Ò°¢—FVÕö–BÀ¢VFvS¢7&FS£§F–ÖVÆ–æS£¤—FVÕG&–ÔVFvS£¤VæBÀ¢F&vWE÷F–6³¢ó3À¢–væ÷&U÷6æ¢G'VRÀ¢Ò“°¢æf–æ—6…ö—FVÕ÷G&–Ò‚“°¢ÆWB&W7F÷&VE÷&–v‡BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ6ÆöæR‚“°¢76W'EöW€¢&W7F÷&VE÷&–v‡Bç7F'E÷6×ÆR‚’²&W7F÷&VE÷&–v‡BæÆVæwF…÷6×ÆW2‚’À¢ç&ö¦V7Bç6×ÆUöE÷F–6²ƒó3’çVçw&‚¢“°¢76W'EöW‡&W7F÷&VE÷&–v‡Bç6÷W&6Uööfg6WE÷6×ÆW2‚’Â5ó"“°§Ğ ¢5·FW7EĞ¦fâ–çfÆ–EöVF–õö—FVÕ÷G&–Õö6†ævW5öæV—F†W%÷&ö¦V7Eöæ÷%ö†—7F÷'’‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢ò÷G&–ÒÖ–çfÆ–B"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢óÀ¢Ò¢çVçw&‚“°¢ÆWB—FVÕö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWB÷&–v–æÂÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ6ÆöæR‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤&Vv–ä—FVÕG&–Ò°¢—FVÕö–BÀ¢VFvS¢7&FS£§F–ÖVÆ–æS£¤—FVÕG&–ÔVFvS£¥7F'BÀ¢F&vWE÷F–6³¢À¢–væ÷&U÷6æ¢G'VRÀ¢Ò“°¢76W'B‚çF–ÖVÆ–æRæ—FVÕ÷G&–Õ÷&Wf–Wr‚’çVçw&‚’çfÆ–B“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢æf–æ—6…ö—FVÕ÷G&–Ò‚“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒÂ÷&–v–æÂ“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ“°¢76W'B†ç7FGW2æ6öçF–ç2‚&BÆV7BöæR6×ÆR"’“°§Ğ ¢5·FW7EĞ¦fâÖ–F•ö—FVÕöVFvU÷G&–Õö¶VW5ö†–FFVåöWfVçG5öæEö—5÷VæFö&ÆR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢“cÀ¢ÆVæwF…÷F–6·3¢ó“#À¢Ò¢çVçw&‚“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2´Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cBÀ¢F–6³¢ó#À¢GW&F–öã¢CƒÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÕÒÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F”6öçG&öÆÆW'2°¢—FVÕö–BÀ¢6öçG&öÆÆW'3¢fV2´Ö–F”6öçG&öÆÆW$FF°¢6†ææVÃ¢À¢6öçG&öÆÆW#¢À¢F–6³¢ócƒÀ¢fÇVS¢ƒÀ¢ÕÒÀ¢Ò¢çVçw&‚“°¢ÆWB÷&–v–æÂÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ6ÆöæR‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“° ¢æ†æFÆU÷F–ÖVÆ–æU÷f–WuöWfVçB†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤&Vv–ä—FVÕG&–Ò°¢—FVÕö–BÀ¢VFvS¢7&FS£§F–ÖVÆ–æS£¤—FVÕG&–ÔVFvS£¤VæBÀ¢F&vWE÷F–6³¢%ócCÀ¢–væ÷&U÷6æ¢G'VRÀ¢Ò“°¢ÆWB&Wf–WrÒçF–ÖVÆ–æRæ—FVÕ÷G&–Õ÷&Wf–Wr‚’çVçw&‚“°¢76W'B‡&Wf–WrçfÆ–B“°¢76W'EöW‡&Wf–WræVæE÷F–6²Â%ócC“°¢æf–æ—6…ö—FVÕ÷G&–Ò‚“° ¢ÆWBG&–ÖÖVBÒfç&ö¦V7BæÖ–F•ö—FV×2‚•³Ó°¢76W'EöW‡G&–ÖÖVBç7F'E÷F–6²‚’Â“c“°¢76W'EöW‡G&–ÖÖVBæÆVæwF…÷F–6·2‚’Âócƒ“°¢76W'EöW‡G&–ÖÖVBææ÷FW2‚’Â÷&–v–æÂææ÷FW2‚’“°¢76W'EöW‡G&–ÖÖVBæ6öçG&öÆÆW'2‚’Â÷&–v–æÂæ6öçG&öÆÆW'2‚’“°¢76W'EöW†ç7FGW2Â$Ô”D’—FVÒG&–ÖÖVB"“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒÂ÷&–v–æÂ“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒæÆVæwF…÷F–6·2‚’Âócƒ“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚’Â÷&–v–æÂææ÷FW2‚’“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ6öçG&öÆÆW'2‚’À¢÷&–v–æÂæ6öçG&öÆÆW'2‚¢“°§Ğ ¢5·FW7EĞ¦fâÖ–F•ö—FVÕ÷7F'E÷G&–Õ÷&W6W'fW5ö6öçFVçE÷÷6—F–öåöæEö6åöW‡æEö&6²‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢“cÀ¢ÆVæwF…÷F–6·3¢ó“#À¢Ò¢çVçw&‚“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2´Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cBÀ¢F–6³¢ó#À¢GW&F–öã¢#CÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÕÒÀ¢Ò¢çVçw&‚“°¢ÆWB÷&–v–æÂÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ6ÆöæR‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“° ¢æ†æFÆU÷F–ÖVÆ–æU÷f–WuöWfVçB†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤&Vv–ä—FVÕG&–Ò°¢—FVÕö–BÀ¢VFvS¢7&FS£§F–ÖVÆ–æS£¤—FVÕG&–ÔVFvS£¥7F'BÀ¢F&vWE÷F–6³¢CƒÀ¢–væ÷&U÷6æ¢G'VRÀ¢Ò“°¢76W'B‚çF–ÖVÆ–æRæ—FVÕ÷G&–Õ÷&Wf–Wr‚’çVçw&‚’çfÆ–B“°¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤6æ6VÄ—FVÕG&–Ò“° ¢æ†æFÆU÷F–ÖVÆ–æU÷f–WuöWfVçB†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤&Vv–ä—FVÕG&–Ò°¢—FVÕö–BÀ¢VFvS¢7&FS£§F–ÖVÆ–æS£¤—FVÕG&–ÔVFvS£¥7F'BÀ¢F&vWE÷F–6³¢óCCÀ¢–væ÷&U÷6æ¢G'VRÀ¢Ò“°¢76W'B†çF–ÖVÆ–æRæ—FVÕ÷G&–Õ÷&Wf–Wr‚’çVçw&‚’çfÆ–B“°¢æf–æ—6…ö—FVÕ÷G&–Ò‚“°¢ÆWBG&–ÖÖVBÒfç&ö¦V7BæÖ–F•ö—FV×2‚•³Ó°¢76W'EöW‡G&–ÖÖVBç7F'E÷F–6²‚’ÂóCC“°¢76W'EöW‡G&–ÖÖVBæÆVæwF…÷F–6·2‚’ÂóCC“°¢76W'EöW‡G&–ÖÖVBç6÷W&6Uööfg6WE÷F–6·2‚’ÂCƒ“°¢76W'EöW‡G&–ÖÖVBç&ö¦V7E÷F–6µöEö6öçFVçE÷F–6²ƒó#’Â6öÖRƒ%óc’“°¢76W'EöW‡G&–ÖÖVBææ÷FW2‚’Â÷&–v–æÂææ÷FW2‚’“° ¢æ†æFÆU÷F–ÖVÆ–æU÷f–WuöWfVçB†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤&Vv–ä—FVÕG&–Ò°¢—FVÕö–BÀ¢VFvS¢7&FS£§F–ÖVÆ–æS£¤—FVÕG&–ÔVFvS£¥7F'BÀ¢F&vWE÷F–6³¢“cÀ¢–væ÷&U÷6æ¢G'VRÀ¢Ò“°¢76W'B†çF–ÖVÆ–æRæ—FVÕ÷G&–Õ÷&Wf–Wr‚’çVçw&‚’çfÆ–B“°¢æf–æ—6…ö—FVÕ÷G&–Ò‚“°¢ÆWBW‡æFVBÒfç&ö¦V7BæÖ–F•ö—FV×2‚•³Ó°¢76W'EöW†W‡æFVBç7F'E÷F–6²‚’Â“c“°¢76W'EöW†W‡æFVBç6÷W&6Uööfg6WE÷F–6·2‚’Â“°¢76W'EöW†W‡æFVBç&ö¦V7E÷F–6µöEö6öçFVçE÷F–6²ƒó#’Â6öÖRƒ%óc’“°§Ğ ¢5·FW7EĞ¦fâVF–õö—FVÕ÷G&–Õö6åöW‡FVæE÷7E÷6÷W&6UöVöeöæEö¶VW5÷F†U÷&WVW7FVEöÆVæwF‚‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWB7F'BÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒCƒ’çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢ò÷G&–ÒÖ&÷VæF'’"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢7F'BÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢#EóÀ¢Ò¢çVçw&‚“°¢ÆWB—FVÕö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWB÷&–v–æÂÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ6ÆöæR‚“°¢æVF–õ÷vfVf÷&×2æ–ç6W'B€¢&76WC¢ò÷G&–ÒÖ&÷VæF'’"çFõö÷væVB‚’À¢6–ÆVçE÷FW7E÷vfVf÷&ÒƒC…óÂ3ó’À¢“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“° ¢æ†æFÆU÷F–ÖVÆ–æU÷f–WuöWfVçB†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤&Vv–ä—FVÕG&–Ò°¢—FVÕö–BÀ¢VFvS¢7&FS£§F–ÖVÆ–æS£¤—FVÕG&–ÔVFvS£¤VæBÀ¢F&vWE÷F–6³¢%óÀ¢–væ÷&U÷6æ¢G'VRÀ¢Ò“°¢76W'B†çF–ÖVÆ–æRæ—FVÕ÷G&–Õ÷&Wf–Wr‚’çVçw&‚’çfÆ–B“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢æf–æ—6…ö—FVÕ÷G&–Ò‚“° ¢ÆWBG&–ÖÖVBÒfç&ö¦V7BæVF–õö—FV×2‚•³Ó°¢76W'EöæR‡G&–ÖÖVBÂf÷&–v–æÂ“°¢76W'EöW‡G&–ÖÖVBç7F'E÷6×ÆR‚’Â÷&–v–æÂç7F'E÷6×ÆR‚’“°¢76W'EöW€¢G&–ÖÖVBç6÷W&6Uööfg6WE÷6×ÆW2‚’À¢÷&–v–æÂç6÷W&6Uööfg6WE÷6×ÆW2‚¢“°¢76W'EöW€¢G&–ÖÖVBæÆVæwF…÷6×ÆW2‚’À¢ç&ö¦V7Bç6×ÆUöE÷F–6²ƒ%ó’çVçw&‚’Ò7F'@¢“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²“°§Ğ ¢5·FW7EĞ¦fâVF–õöÆVgE÷G&–ÕöE÷6÷W&6UöVöeö¶VW5÷F†UöæWu÷6÷W&6U÷&ævU÷fÆ–B‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢ò÷G&–ÒÖVöb"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò¢çVçw&‚“°¢ÆWB—FVÕö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢æVF–õ÷vfVf÷&×2æ–ç6W'B€¢&76WC¢ò÷G&–ÒÖVöb"çFõö÷væVB‚’À¢6–ÆVçE÷FW7E÷vfVf÷&ÒƒCEóÂCEó’À¢“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“° ¢æ†æFÆU÷F–ÖVÆ–æU÷f–WuöWfVçB†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤&Vv–ä—FVÕG&–Ò°¢—FVÕö–BÀ¢VFvS¢7&FS£§F–ÖVÆ–æS£¤—FVÕG&–ÔVFvS£¥7F'BÀ¢F&vWE÷F–6³¢BÀ¢–væ÷&U÷6æ¢G'VRÀ¢Ò“°¢76W'B†çF–ÖVÆ–æRæ—FVÕ÷G&–Õ÷&Wf–Wr‚’çVçw&‚’çfÆ–B“°¢æf–æ—6…ö—FVÕ÷G&–Ò‚“° ¢ÆWBG&–ÖÖVBÒfç&ö¦V7BæVF–õö—FV×2‚•³Ó°¢ÆWB6÷W&6UöÆVæwF‚Ò7WW#£§66ÆU÷&ö¦V7E÷6×ÆW5÷Fõ÷6÷W&6Uög&ÖW5ö6V–Â€¢“#ƒ£¦g&öÒ‡G&–ÖÖVBæÆVæwF…÷6×ÆW2‚’’À¢CEóÀ¢C…óÀ¢¢çVçw&‚“°¢76W'EöW€¢“#ƒ£¦g&öÒ‡G&–ÖÖVBç6÷W&6Uööfg6WE÷6×ÆW2‚’’²6÷W&6UöÆVæwF‚À¢CEó ¢“°§Ğ ¢5·FW7EĞ¦fâVF–õ÷G&–ÕöÖ5÷&ö¦V7E÷F–ÖUö–çFõ÷6÷W&6U÷6×ÆU÷&FR‚’°¢ÆWB–çv&BĞ¢7WW#£§66ÆU÷&ö¦V7E÷6×ÆW5÷Fõ÷6÷W&6Uög&ÖW5÷F÷v&E÷¦W&òƒeóÂCEóÂC…ó’çVçw&‚“°¢ÆWB÷WGv&BĞ¢7WW#£§66ÆU÷&ö¦V7E÷6×ÆW5÷Fõ÷6÷W&6Uög&ÖW5÷F÷v&E÷¦W&ò‚ÓeóÂCEóÂC…ó’çVçw&‚“°¢76W'EöW†–çv&BÂUóS"“°¢76W'EöW†÷WGv&BÂÓUóS"“°¢76W'EöW†–çv&B²÷WGv&BÂ“°¢76W'EöW€¢7WW#£§66ÆU÷&ö¦V7E÷6×ÆW5÷Fõ÷6÷W&6Uög&ÖW5ö6V–ÂƒeóÂCEóÂC…ó’çVçw&‚’À¢UóS0¢“°¢76W'EöW€¢7WW#£§66ÆU÷&ö¦V7E÷6×ÆW5÷Fõ÷6÷W&6Uög&ÖW5÷F÷v&E÷¦W&ò‚Ó5óÂCEóÂC…ó’çVçw&‚’À¢Ó%ósS`¢“°§Ğ ¢5·FW7EĞ¦fâ–çfÆ–Eö—FVÕöG&÷öFöW5öæ÷Eö6†ævU÷&ö¦V7Eö÷%ö7&VFUö†—7F÷'’‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWB6÷W&6U÷G&6²Òç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢6÷W&6U÷G&6²À¢ÖVF–÷&Vc¢&76WC¢òöVFvRÖ—FVÒ"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢#À¢ÆVæwF…÷6×ÆW3¢óÀ¢Ò¢çVçw&‚“°¢ÆWB—FVÕö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6VÆV7D—FVÒ°¢—FVÕö–C¢6öÖR†—FVÕö–B’À¢FF—F—fS¢fÇ6RÀ¢&ævS¢fÇ6RÀ¢Ò“°¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¤&Vv–ä—FVÔG&r°¢—FVÕö–BÀ¢ö–çFW%öFVÇF÷F–6·3¢ÓÀ¢F&vWE÷G&6µö–æFWƒ¢6öÖRƒ’À¢&ævS¢fÇ6RÀ¢–væ÷&U÷6æ¢fÇ6RÀ¢6÷“¢fÇ6RÀ¢Ò“°¢ÆWB&Wf—6–öåö&Vf÷&UöG&÷Òç&Wf—6–öã° ¢æf–æ—6…ö—FVÕöG&r‚“° ¢ÆWB—FVÕögFW"Òfç&ö¦V7BæVF–õö—FV×2‚•³Ó°¢76W'EöW†—FVÕögFW"çG&6µö–B‚’Â6÷W&6U÷G&6²“°¢76W'EöW†—FVÕögFW"ç7F'E÷6×ÆR‚’Â“°¢76W'EöW†—FVÕögFW"ç6÷W&6Uööfg6WE÷6×ÆW2‚’Â#“°¢76W'EöW†—FVÕögFW"æÆVæwF…÷6×ÆW2‚’Âó“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&UöG&÷“°¢76W'EöW€¢ç7FGW2À¢$G&÷&V¦V7FVC¢—FVÒv÷VÆBÆVfRF†R&ö¦V7B&÷VæG2 ¢“°¢76W'B†ç&ö¦V7BçVæFò‚’çVçw&‚’“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚’æ—5öV×G’‚’“°§Ğ ¢5·FW7EĞ¦fâVF–õ÷F–ÖVÆ–æUöGWÆ–6FU÷&W6W'fW5÷6÷W&6Uööfg6WEöæEö—5÷VæFö&ÆR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢òöGWÆ–6FR×FW7B"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢#CÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢#À¢ÆVæwF…÷6×ÆW3¢“cÀ¢Ò¢æW‡V7B‚'6÷W&6R—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB6÷W&6Uö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÒ6öÖR‡6÷W&6Uö–B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ–ç6W'B‡6÷W&6Uö–B“° ¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¤GWÆ–6FU6VÆV7FVD—FVÒ’“°¢ÆWBGWÆ–6FRÒfç&ö¦V7BæVF–õö—FV×2‚•³Ó°¢76W'EöW†GWÆ–6FRç7F'E÷6×ÆR‚’Âó#“°¢76W'EöW†GWÆ–6FRæÖVF–÷&Vb‚’Â&76WC¢òöGWÆ–6FR×FW7B"“°¢76W'EöW†GWÆ–6FRç6÷W&6Uööfg6WE÷6×ÆW2‚’Â#“°¢76W'EöW†GWÆ–6FRæÆVæwF…÷6×ÆW2‚’Â“c“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’Â“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚’Â6÷W&6Uö–B“°§Ğ ¢5·FW7EĞ¦fâÖ–F•ö—FVÕöGWÆ–6FUö—5öf–Æ&ÆUö–å÷6†&VEö6öÖÖæE÷7W&f6W5öæE÷VæFö&ÆR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢#CÀ¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢æW‡V7B‚'6÷W&6RÔ”D’—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB6÷W&6Uö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–C¢6÷W&6Uö–BÀ¢æ÷FW3¢fV2´Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢#À¢GW&F–öã¢#CÀ¢fVÆö6—G“¢“À¢öfe÷fVÆö6—G“¢À¢ÕÒÀ¢Ò¢çVçw&‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÒ6öÖR‡6÷W&6Uö–B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ–ç6W'B‡6÷W&6Uö–B“° ¢ÆWB6öÖÖæBÒ6öÖÖæD–C£¤GWÆ–6FU6VÆV7FVD—FVÓ°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°¢76W'EöW†6öÖÖæG3£¦f–æB‚fÂ&GWÆ–6FR—FVÒ"’Â6öÖR†6öÖÖæB’“°¢76W'B€¢6öÖÖæG3£¦f÷%öÖVçR‚fÂÖ–äÖVçS£¤—FVÒ¢æ—FW"‚¢æç’‡ÆVçG'—ÂVçG'’æ–BÓÒ6öÖÖæBbbVçG'’æVæ&ÆVB¢“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’Â#C“°¢ÆWB6÷W&6Uöæ÷FUö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Òæ–B‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB†6öÖÖæB’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚’æÆVâ‚’Â"“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’Âó#“°¢76W'EöæR†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Òæ–B‚’Â6÷W&6Uöæ÷FUö–B“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚’æÆVâ‚’Â“°§Ğ ¢5·FW7EĞ¦fâGWÆ–6FU÷6VÆV7FVEöVF–õöæEöÖ–F•ö—FV×5ö—5ööæU÷VæFö&ÆUö7F–öâ‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢òö×VÇF’ÖGWÆ–6FR×FW7B"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢#CÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢#À¢ÆVæwF…÷6×ÆW3¢“cÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢#CÀ¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢çVçw&‚“°¢ÆWBVF–õö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBÖ–F•ö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWBVF–õ÷7F'E÷F–6²Òç&ö¦V7BçF–6µöE÷6×ÆRƒ#C’çVçw&‚“°¢ÆWBVF–õöVæE÷F–6²Òç&ö¦V7BçF–6µöE÷6×ÆRƒó#’çVçw&‚“°¢ÆWBÖ–F•÷7F'E÷F–6²Ò#C°¢ÆWBÖ–F•öVæE÷F–6²ÒÖ–F•÷7F'E÷F–6²²“c°¢ÆWBw&÷W÷7F'E÷F–6²ÒVF–õ÷7F'E÷F–6²æÖ–â†Ö–F•÷7F'E÷F–6²“°¢ÆWBw&÷WöVæE÷F–6²ÒVF–õöVæE÷F–6²æÖ‚†Ö–F•öVæE÷F–6²“°¢ÆWBw&÷Wööfg6WBÒw&÷WöVæE÷F–6²Òw&÷W÷7F'E÷F–6³°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÒ6öÖR†VF–õö–B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æW‡FVæB…¶VF–õö–BÂÖ–F•ö–EÒ“°¢ÆWB&Wf—6–öåö&Vf÷&UöGWÆ–6FRÒç&Wf—6–öã°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæD–C£¤GWÆ–6FU6VÆV7FVD—FVÒ’“° ¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¤GWÆ–6FU6VÆV7FVD—FVÒ’“° ¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’Â"“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚’æÆVâ‚’Â"“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’À¢Ö–F•÷7F'E÷F–6²²w&÷Wööfg6W@¢“°¢76W'EöW€¢ç&ö¦V7BæVF–õö—FV×2‚•³Òç7F'E÷6×ÆR‚’À¢ç&ö¦V7@¢ç6×ÆUöE÷F–6²†VF–õ÷7F'E÷F–6²²w&÷Wööfg6WB¢çVçw&‚¢“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&UöGWÆ–6FR²“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’Â“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚’æÆVâ‚’Â“°§Ğ ¢5·FW7EĞ¦fâ6VÆV7EöÆÅö—FV×5ö7F–öå÷6VÆV7G5öVF–õöæEöÖ–F•÷v—F†÷WEöVF—F–æu÷F†U÷&ö¦V7B‚’°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB€¢d£¦FVfVÇB‚’À¢6öÖÖæD–C£¥6VÆV7DÆÄ—FV×0¢’“°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6·2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢ÖVF–÷&Vc¢&76WC¢ò÷6VÆV7BÖÆÂÖ—FV×2"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢#EóÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢“cÀ¢ÆVæwF…÷F–6·3¢5óƒCÀ¢Ò¢çVçw&‚“°¢ÆWBVF–õö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBÖ–F•ö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ–ç6W'B†VF–õö–B“° ¢ÆWB6öÖÖæBÒ6öÖÖæD–C£¥6VÆV7DÆÄ—FV×3°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°¢76W'EöW†6öÖÖæG3£¦f–æB‚fÂ'6VÆV7BÆÂ—FV×2"’Â6öÖR†6öÖÖæB’“°¢76W'EöW€¢6öÖÖæG3£§7F&ÆUö–B†6öÖÖæB’æ5öFW&Vb‚’À¢6öÖR‚&—FVÒç6VÆV7BÖÆÂ"¢“°¢æ7F–öåöÆ—7Bç6WE÷VW'’‚'6VÆV7BÆÂ—FV×2"çFõö÷væVB‚’“°¢ÆWBVçG&–W2Òæ7F–öåöÆ—7EöVçG&–W2‚“°¢76W'EöW€¢VçG&–W0¢æ—FW"‚¢æf–ÇFW%öÖ‡ÆVçG'—Â6öÖÖæG3£§7F&ÆUö–B†VçG'’æ–B’¢æ6öÆÆV7C££Ä†6…6WCÅóãâ‚’À¢†6…6WC£¦g&öÒ…°¢&—FVÒç6VÆV7BÖÆÂ"çFõö÷væVB‚’À¢&—FVÒç6VÆV7BÖÆÂÖ–â×G&6²"çFõö÷væVB‚’À¢&—FVÒç6VÆV7BÖÆÂÖ–â×F–ÖR×6VÆV7F–öâ"çFõö÷væVB‚’À¢&—FVÒç6VÆV7BÖÆÂÖöâ×6VÆV7FVB×G&6·2Ö–â×F–ÖR×6VÆV7F–öâ"çFõö÷væVB‚’À¢&—FVÒçVç6VÆV7BÖÆÂ"çFõö÷væVB‚’À¢Ò¢“°¢76W'B†VçG&–W2æ—FW"‚’æÆÂ‡ÆVçG'—ÂVçG'’ç6†÷'F7WBæ—5öæöæR‚’’“°¢æ7F–öåöÆ—7Bç6VÆV7B‚&—FVÒç6VÆV7BÖÆÂ"çFõö÷væVB‚’“°¢ÆWB&Wf—6–öåö&Vf÷&RÒç&Wf—6–öã°¢ÆWB6å÷VæFõö&Vf÷&RÒç&ö¦V7Bæ6å÷VæFò‚“°¢ÆWB6å÷&VFõö&Vf÷&RÒç&ö¦V7Bæ6å÷&VFò‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“° ¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2À¢†6…6WC£¦g&öÒ…¶VF–õö–BÂÖ–F•ö–EÒ¢“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR†Ö–F•ö–B’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’Â“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚’æÆVâ‚’Â“°§Ğ ¢5·FW7EĞ¦fâ—FVÕ÷6VÆV7F–öå÷VæFõöæE÷&VFõ÷7F•ö–åö÷&FW%÷v—F…÷&ö¦V7EöVF—G2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6·2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢ÖVF–÷&Vc¢&76WC¢ò÷6VÆV7F–öâÖ†—7F÷'’ÖVF–ò"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢#EóÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢“cÀ¢ÆVæwF…÷F–6·3¢5óƒCÀ¢Ò¢çVçw&‚“°¢ÆWBVF–õö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBÖ–F•ö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2²†VF–õö–BÂG&6·5³Ò•Ò“° ¢ÆWB&ö¦V7Eö†—7F÷'•ö7W'6÷"Òç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥6VÆV7DÆÄ—FV×2“°¢76W'EöW†ç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚’Â&ö¦V7Eö†—7F÷'•ö7W'6÷"“°¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2À¢†6…6WC£¦g&öÒ…¶VF–õö–BÂÖ–F•ö–EÒ¢“° ¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â2“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â"“°¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2À¢†6…6WC£¦g&öÒ…¶VF–õö–BÂÖ–F•ö–EÒ¢“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â†6…6WC£¦g&öÒ…¶VF–õö–EÒ’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR†VF–õö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2À¢†6…6WC£¦g&öÒ…¶VF–õö–BÂÖ–F•ö–EÒ¢“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â"“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â2“°§Ğ ¢5·FW7EĞ¦fâæõö÷ö—FVÕ÷6VÆV7F–öåöFöW5öæ÷EöF—66&E÷&VFõö†—7F÷'’‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢ò÷6VÆV7F–öâ×&VFòÖVF–ò"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò¢çVçw&‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¥6VÆV7DÆÄ—FV×2“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B†æ6å÷&VFõö7F–öâ‚’“° ¢ÆWBòÒçWFFR„ÖW76vS£¥Vç6VÆV7DÆÄ—FV×2“°¢76W'B†æ6å÷&VFõö7F–öâ‚’“° ¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢ÆWB—FVÕö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â†6…6WC£¦g&öÒ…¶—FVÕö–EÒ’“° ¢ÆWB×WB&ö¦V7E÷&VFòÒ£¦FVfVÇB‚“°¢ÆWBòÒ&ö¦V7E÷&VFòçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒ&ö¦V7E÷&VFòçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒ&ö¦V7E÷&VFòçWFFR„ÖW76vS£¥VæFò“°¢76W'B‡&ö¦V7E÷&VFòç&ö¦V7Bæ6å÷&VFò‚’“°¢ÆWBòÒ&ö¦V7E÷&VFòçWFFR„ÖW76vS£¥Vç6VÆV7DÆÄ—FV×2“°¢76W'B‡&ö¦V7E÷&VFòç&ö¦V7Bæ6å÷&VFò‚’“°¢ÆWBòÒ&ö¦V7E÷&VFòçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW‡&ö¦V7E÷&VFòç&ö¦V7BçG&6·2‚’æÆVâ‚’Â"“°§Ğ ¢5·FW7EĞ¦fâ6†ævVEö—FVÕ÷6VÆV7F–öåöF—66&G5÷&ö¦V7E÷&VFõö†—7F÷'’‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢ò÷6VÆV7F–öâÖ'&æ6‚ÖVF–ò"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò¢çVçw&‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B†ç&ö¦V7Bæ6å÷&VFò‚’“° ¢ÆWBòÒçWFFR„ÖW76vS£¥6VÆV7DÆÄ—FV×2“° ¢76W'B‚ç&ö¦V7Bæ6å÷&VFò‚’“°¢76W'B‚æ6å÷&VFõö7F–öâ‚’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æÆVâ‚’Â“°§Ğ ¢5·FW7EĞ¦fâ–çfW'Eö—FVÕ÷6VÆV7F–öåö7F–öå÷6VÆV7G5÷F†Uö6ö×ÆVÖVçEööeöVF–õöæEöÖ–F•ö—FV×2‚’°¢76W'B‚6öÖÖæG3£¦—5öVæ&ÆVB€¢d£¦FVfVÇB‚’À¢6öÖÖæD–C£¤–çfW'D—FVÕ6VÆV7F–öà¢’“°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6·2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢ÖVF–÷&Vc¢&76WC¢òö–çfW'B×6VÆV7F–öâ"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢“cÀ¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢çVçw&‚“°¢ÆWBVF–õö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBÖ–F•ö–G2Ò ¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æÖ‡Æ—FV×Â—FVÒæ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“° ¢ÆWB6öÖÖæBÒ6öÖÖæD–C£¤–çfW'D—FVÕ6VÆV7F–öã°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°¢76W'EöW†6öÖÖæG3£¦f–æB‚fÂ&–çfW'B6VÆV7F–öâ"’Â6öÖR†6öÖÖæB’“°¢76W'EöW€¢6öÖÖæG3£§7F&ÆUö–B†6öÖÖæB’æ5öFW&Vb‚’À¢6öÖR‚&—FVÒæ–çfW'B×6VÆV7F–öâ"¢“°¢æ7F–öåöÆ—7Bç6WE÷VW'’‚&–çfW'B6VÆV7F–öâ"çFõö÷væVB‚’“°¢æ7F–öåöÆ—7Bç6VÆV7B‚&—FVÒæ–çfW'B×6VÆV7F–öâ"çFõö÷væVB‚’“°¢76W'EöW€¢æ7F–öåöÆ—7EöVçG&–W2‚¢æ—FW"‚¢æf–ÇFW%öÖ‡ÆVçG'—Â6öÖÖæG3£§7F&ÆUö–B†VçG'’æ–B’¢æ6öÆÆV7C££ÅfV3Åóãâ‚’À¢fV2²&—FVÒæ–çfW'B×6VÆV7F–öâ"çFõö÷væVB‚•Ğ¢“°¢ÆWB&Wf—6–öåö&Vf÷&RÒç&Wf—6–öã°¢ÆWB6å÷VæFõö&Vf÷&RÒç&ö¦V7Bæ6å÷VæFò‚“°¢ÆWB6å÷&VFõö&Vf÷&RÒç&ö¦V7Bæ6å÷&VFò‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB†6öÖÖæB’“°¢ÆWBÆÅö—FV×2Ò7FC£¦—FW#£¦öæ6R†VF–õö–B¢æ6†–â†Ö–F•ö–G2æ—FW"‚’æ6÷–VB‚’¢æ6öÆÆV7C££Ä†6…6WCÅóãâ‚“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2ÂÆÅö—FV×2“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂÖ–F•ö–G2æÆ7B‚’æ6÷–VB‚’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ—5öV×G’‚’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂæöæR“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2ÂÆÅö—FV×2“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂÖ–F•ö–G2æÆ7B‚’æ6÷–VB‚’“° ¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6ÆV"‚“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ–ç6W'B†VF–õö–B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÒ6öÖR†VF–õö–B“°¢æ7F–öåöÆ—7Bç6VÆV7B‚&—FVÒæ–çfW'B×6VÆV7F–öâ"çFõö÷væVB‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“° ¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2À¢Ö–F•ö–G2æ—FW"‚’æ6÷–VB‚’æ6öÆÆV7C££Ä†6…6WCÅóãâ‚¢“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂÖ–F•ö–G2æÆ7B‚’æ6÷–VB‚’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â†6…6WC£¦g&öÒ…¶VF–õö–EÒ’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR†VF–õö–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2À¢Ö–F•ö–G2æ—FW"‚’æ6÷–VB‚’æ6öÆÆV7C££Ä†6…6WCÅóãâ‚¢“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂÖ–F•ö–G2æÆ7B‚’æ6÷–VB‚’“° ¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB†6öÖÖæB’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â†6…6WC£¦g&öÒ…¶VF–õö–EÒ’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR†VF–õö–B’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°§Ğ ¢5·FW7EĞ¦fâF¦6VçEöæöåö÷fW&Æ–æuö—FVÕö7F–öç5öæf–vFU÷v—F†–å÷F†Uö7F—fU÷G&6²‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6·2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWBVF–õ÷7F'BÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒ“c’çVçw&‚“°¢ÆWBVF–õöVæBÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒó“#’çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢ÖVF–÷&Vc¢&76WC¢òöF¦6VçB×6VÆV7F–öâ"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢VF–õ÷7F'BÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢VF–õöVæBÒVF–õ÷7F'BÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢óCCÀ¢ÆVæwF…÷F–6·3¢#CÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢%óSÀ¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢ó“#À¢ÆVæwF…÷F–6·3¢CƒÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢ó“#À¢ÆVæwF…÷F–6·3¢CƒÀ¢Ò¢çVçw&‚“°¢ÆWB7F—fUö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWB&Wf–÷W5ö–BÒ ¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒçG&6µö–B‚’ÓÒG&6·5³Òbb—FVÒç7F'E÷F–6²‚’ÓÒ¢çVçw&‚¢æ–B‚“°¢ÆWBæW‡Eö–BÒ ¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒçG&6µö–B‚’ÓÒG&6·5³Òbb—FVÒç7F'E÷F–6²‚’ÓÒó“#¢çVçw&‚¢æ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2²†7F—fUö–BÂG&6·5³Ò•Ò“°¢ÆWB&Wf—6–öåö&Vf÷&RÒç&Wf—6–öã°¢ÆWB6å÷VæFõö&Vf÷&RÒç&ö¦V7Bæ6å÷VæFò‚“°¢ÆWB6å÷&VFõö&Vf÷&RÒç&ö¦V7Bæ6å÷&VFò‚“° ¢ÆWBæW‡BÒ6öÖÖæD–C£¥6VÆV7DæW‡DF¦6VçDæöä÷fW&Æ–æt—FVÓ°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂæW‡B’“°¢æ7F–öåöÆ—7Bç6WE÷VW'’‚'6VÆV7BæW‡BF¦6VçB"çFõö÷væVB‚’“°¢æ7F–öåöÆ—7@¢ç6VÆV7B‚&—FVÒç6VÆV7BÖæW‡BÖF¦6VçB"çFõö÷væVB‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â†6…6WC£¦g&öÒ…¶æW‡Eö–EÒ’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR†æW‡Eö–B’“°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂæW‡B’“° ¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2²†7F—fUö–BÂG&6·5³Ò•Ò“°¢ÆWB&Wf–÷W2Ò6öÖÖæD–C£¥6VÆV7E&Wf–÷W4F¦6VçDæöä÷fW&Æ–æt—FVÓ°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ&Wf–÷W2’“°¢æ7F–öåöÆ—7@¢ç6WE÷VW'’‚'6VÆV7B&Wf–÷W2F¦6VçB"çFõö÷væVB‚’“°¢æ7F–öåöÆ—7@¢ç6VÆV7B‚&—FVÒç6VÆV7B×&Wf–÷W2ÖF¦6VçB"çFõö÷væVB‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â†6…6WC£¦g&öÒ…·&Wf–÷W5ö–EÒ’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR‡&Wf–÷W5ö–B’“°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ&Wf–÷W2’“° ¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“°§Ğ ¢5·FW7EĞ¦fâF¦6VçEö—FVÕö7F–öç5÷&WV—&U÷F÷V6†–æuö&÷VæF&–W5öæEöÖöV6…÷6VÆV7F–öâ‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6·2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWBVF–õ÷7F'BÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒ“c’çVçw&‚“°¢ÆWBVF–õöVæBÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒó“#’çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢ÖVF–÷&Vc¢&76WC¢ò÷F÷V6†–ærÖ&÷VæF'’×6VÆV7F–öâ"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢VF–õ÷7F'BÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢VF–õöVæBÒVF–õ÷7F'BÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢ó“#À¢ÆVæwF…÷F–6·3¢CƒÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢%óCÀ¢ÆVæwF…÷F–6·3¢CƒÀ¢Ò¢çVçw&‚“°¢ÆWBVF–õö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBF÷V6†–æuö–BÒ ¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒç7F'E÷F–6²‚’ÓÒó“#¢çVçw&‚¢æ–B‚“°¢ÆWBvö–BÒ ¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒç7F'E÷F–6²‚’ÓÒ%óC¢çVçw&‚¢æ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æP¢ç6VÆV7Eö—FV×2‡fV2²†VF–õö–BÂG&6·5³Ò’Â†vö–BÂG&6·5³Ò•Ò“°¢ÆWB&Wf—6–öåö&Vf÷&RÒç&Wf—6–öã°¢ÆWB6å÷VæFõö&Vf÷&RÒç&ö¦V7Bæ6å÷VæFò‚“°¢ÆWB6å÷&VFõö&Vf÷&RÒç&ö¦V7Bæ6å÷&VFò‚“° ¢ÆWBæW‡BÒ6öÖÖæD–C£¥6VÆV7DæW‡DF¦6VçDæöä÷fW&Æ–æt—FVÓ°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂæW‡B’“°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB†æW‡B’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â†6…6WC£¦g&öÒ…·F÷V6†–æuö–EÒ’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR‡F÷V6†–æuö–B’“° ¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2²†VF–õö–BÂG&6·5³Ò•Ò“°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB†æW‡B’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â†6…6WC£¦g&öÒ…·F÷V6†–æuö–EÒ’“° ¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2²†vö–BÂG&6·5³Ò•Ò“°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂæW‡B’“°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB†æW‡B’“°¢76W'B†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ—5öV×G’‚’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂæöæR“° ¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2²‡F÷V6†–æuö–BÂG&6·5³Ò•Ò“°¢ÆWB&Wf–÷W2Ò6öÖÖæD–C£¥6VÆV7E&Wf–÷W4F¦6VçDæöä÷fW&Æ–æt—FVÓ°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ&Wf–÷W2’“°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB‡&Wf–÷W2’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â†6…6WC£¦g&öÒ…¶VF–õö–EÒ’“° ¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“°§Ğ ¢5·FW7EĞ¦fâF¦6VçE÷G&6µöæf–vF–öå÷W6W5öf—'7Eö÷fW&Æ–æuö—FVÕöæE÷&W6W'fW5÷G&6µ÷6VÆV7F–öâ‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢f÷"ò–ââãB°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢Ğ¢ÆWBG&6·2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWBVF–õ÷7F'BÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒ“c’çVçw&‚“°¢ÆWBVF–õöVæBÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒó“#’çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢ÖVF–÷&Vc¢&76WC¢òöF¦6VçB×G&6²×&Wf–÷W2ÖF—7G&7F÷""çFõö÷væVB‚’À¢7F'E÷6×ÆS¢VF–õ÷7F'BÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢VF–õöVæBÒVF–õ÷7F'BÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6·5³%ÒÀ¢ÖVF–÷&Vc¢&76WC¢òöF¦6VçB×G&6²×6÷W&6R"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢VF–õ÷7F'BÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢VF–õöVæBÒVF–õ÷7F'BÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³%ÒÀ¢7F'E÷F–6³¢%óƒƒÀ¢ÆVæwF…÷F–6·3¢CƒÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³5ÒÀ¢7F'E÷F–6³¢“cÀ¢ÆVæwF…÷F–6·3¢#CÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6·5³5ÒÀ¢ÖVF–÷&Vc¢&76WC¢òöF¦6VçB×G&6²ÖÆFW"Ö÷fW&Æ"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢ç&ö¦V7Bç6×ÆUöE÷F–6²ƒóCC’çVçw&‚’À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢ç&ö¦V7Bç6×ÆUöE÷F–6²ƒ%óC’çVçw&‚¢Òç&ö¦V7Bç6×ÆUöE÷F–6²ƒóCC’çVçw&‚’À¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³5ÒÀ¢7F'E÷F–6³¢%óƒƒÀ¢ÆVæwF…÷F–6·3¢#CÀ¢Ò¢çVçw&‚“°¢ÆWB6÷W&6UöVF–òÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWB6÷W&6UöÖ–F’Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWB&Wf–÷W5÷G&6µöF—7G&7F÷"Òç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBæW‡E÷G&6µöf—'7BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWBæW‡E÷G&6µöÆFW"Òç&ö¦V7BæVF–õö—FV×2‚•³%Òæ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Ò6öÖR‡G&6·5³%Ò“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Ò†6…6WC£¦g&öÒ…·G&6·5³%ÕÒ“°¢çF–ÖVÆ–æP¢ç6VÆV7Eö—FV×2‡fV2²‡6÷W&6UöÖ–F’ÂG&6·5³%Ò’Â‡6÷W&6UöVF–òÂG&6·5³%Ò•Ò“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Ò6öÖR‡G&6·5³%Ò“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Ò†6…6WC£¦g&öÒ…·G&6·5³%ÕÒ“°¢çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²Òsss°¢ÆWB&Wf–÷W2Ò6öÖÖæD–C£¥6VÆV7D—FVÔöå&Wf–÷W5G&6³°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ&Wf–÷W2’“°¢æ7F–öåöÆ—7@¢ç6VÆV7B‚&—FVÒç6VÆV7B×&Wf–÷W2×G&6²"çFõö÷væVB‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'B†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ—5öV×G’‚’“°¢76W'EöW†çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²Âssr“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Â6öÖR‡G&6·5³%Ò’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Â†6…6WC£¦g&öÒ…·G&6·5³%ÕÒ’“° ¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢“cÀ¢ÆVæwF…÷F–6·3¢#CÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢óCCÀ¢ÆVæwF…÷F–6·3¢CƒÀ¢Ò¢çVçw&‚“°¢ÆWB&Wf–÷W5÷G&6µöf—'7BÒ ¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒçG&6µö–B‚’ÓÒG&6·5³Òbb—FVÒç7F'E÷F–6²‚’ÓÒ“c¢çVçw&‚¢æ–B‚“°¢ÆWBæW‡E÷G&6µöÖ–F’Ò ¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒçG&6µö–B‚’ÓÒG&6·5³5Òbb—FVÒç7F'E÷F–6²‚’ÓÒ%óƒƒ¢çVçw&‚¢æ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢ÆWB&Wf—6–öåö&Vf÷&RÒç&Wf—6–öã°¢ÆWB6å÷VæFõö&Vf÷&RÒç&ö¦V7Bæ6å÷VæFò‚“°¢ÆWB6å÷&VFõö&Vf÷&RÒç&ö¦V7Bæ6å÷&VFò‚“°¢çF–ÖVÆ–æP¢ç6VÆV7Eö—FV×2‡fV2²‡6÷W&6UöÖ–F’ÂG&6·5³%Ò’Â‡6÷W&6UöVF–òÂG&6·5³%Ò•Ò“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Ò6öÖR‡G&6·5³%Ò“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Ò†6…6WC£¦g&öÒ…·G&6·5³%ÕÒ“°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB‡&Wf–÷W2’“°¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2À¢†6…6WC£¦g&öÒ…·&Wf–÷W5÷G&6µöf—'7EÒ¢“°¢76W'EöW†çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²Â“c“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Â6öÖR‡G&6·5³%Ò’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Â†6…6WC£¦g&öÒ…·G&6·5³%ÕÒ’“°¢76W'EöæR‡&Wf–÷W5÷G&6µöf—'7BÂ&Wf–÷W5÷G&6µöF—7G&7F÷"“° ¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2²‡6÷W&6UöVF–òÂG&6·5³%Ò•Ò“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Ò6öÖR‡G&6·5³%Ò“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Ò†6…6WC£¦g&öÒ…·G&6·5³%ÕÒ“°¢ÆWBæW‡BÒ6öÖÖæD–C£¥6VÆV7D—FVÔöäæW‡EG&6³°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂæW‡B’“°¢æ7F–öåöÆ—7Bç6VÆV7B‚&—FVÒç6VÆV7BÖæW‡B×G&6²"çFõö÷væVB‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2À¢†6…6WC£¦g&öÒ…¶æW‡E÷G&6µöf—'7EÒ¢“°¢76W'EöW†çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²Â“c“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Â6öÖR‡G&6·5³%Ò’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Â†6…6WC£¦g&öÒ…·G&6·5³%ÕÒ’“°¢76W'EöæR†æW‡E÷G&6µöf—'7BÂæW‡E÷G&6µöÆFW"“° ¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2²‡6÷W&6UöÖ–F’ÂG&6·5³%Ò•Ò“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Ò6öÖR‡G&6·5³%Ò“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Ò†6…6WC£¦g&öÒ…·G&6·5³%ÕÒ“°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB†æW‡B’“°¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2À¢†6…6WC£¦g&öÒ…¶æW‡E÷G&6µöÖ–F•Ò¢“°¢76W'EöW†çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²Â%óƒƒ“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Â6öÖR‡G&6·5³%Ò’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Â†6…6WC£¦g&öÒ…·G&6·5³%ÕÒ’“° ¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“°§Ğ ¢5·FW7EĞ¦fâ—FVÕöVFvUöæf–vF–öåö7F–öç5öÖ÷fU÷FõöæV&W7EöVF–õö÷%öÖ–F•ö&÷VæF'’‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWBVF–õ÷7F'BÒ#ó°¢ÆWBVF–õöVæBÒ3UóC°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢òö—FVÒÖVFvRÖæf–vF–öâ"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢VF–õ÷7F'BÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢VF–õöVæBÒVF–õ÷7F'BÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢%óƒƒÀ¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢çVçw&‚“°¢ÆWB6VÆV7FVEö—FVÒÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2²‡6VÆV7FVEö—FVÒÂG&6µö–B•Ò“°¢çF–ÖVÆ–æRç6WEöVF—Eö7W'6÷%÷F–6²ƒ%óS“°¢ÆWB&Wf—6–öåö&Vf÷&RÒç&Wf—6–öã°¢ÆWB6å÷VæFõö&Vf÷&RÒç&ö¦V7Bæ6å÷VæFò‚“°¢ÆWB6å÷&VFõö&Vf÷&RÒç&ö¦V7Bæ6å÷&VFò‚“° ¢ÆWBÆVgBÒ6öÖÖæD–C£¤Ö÷fTVF—D7W'6÷%Fõ&Wf–÷W4—FVÔVFvS°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂÆVgB’“°¢æ7F–öåöÆ—7Bç6WE÷VW'’‚&æV&W7B—FVÒVFvR"çFõö÷væVB‚’“°¢æ7F–öåöÆ—7@¢ç6VÆV7B‚&—FVÒææf–vFRÖ7W'6÷"ÖÆVgBÖVFvR"çFõö÷væVB‚’“°¢ÆWBÆVgEöVçG'’Ò ¢æ7F–öåöÆ—7EöVçG&–W2‚¢æ–çFõö—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒÆVgB¢æW‡V7B‚&ÆVgBVFvRæf–vF–öâ7F–öâ—26V&6†&ÆR"“°¢76W'B†ÆVgEöVçG'’ç6†÷'F7WBæ—5öæöæR‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'EöW€¢çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²À¢ç&ö¦V7BçF–6µöE÷6×ÆR†VF–õöVæB’çVçw&‚¢“°¢76W'EöW€¢çF–ÖVÆ–æRæ7W'6÷%÷÷6—F–öâ‚’À¢7&FS£§F–ÖVÆ–æS£¤VF—D7W'6÷%÷6—F–öã£¥6×ÆR†VF–õöVæB¢“°¢76W'EöW†çF–ÖVÆ–æRæVF—Eö7W'6÷%÷6×ÆR‚fç&ö¦V7B’Âö²†VF–õöVæB’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â†6…6WC£¦g&öÒ…·6VÆV7FVEö—FVÕÒ’“° ¢çF–ÖVÆ–æRç6WEöVF—Eö7W'6÷%÷F–6²ƒ%óS“°¢ÆWB&–v‡BÒ6öÖÖæD–C£¤Ö÷fTVF—D7W'6÷%FôæW‡D—FVÔVFvS°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ&–v‡B’“°¢æ7F–öåöÆ—7@¢ç6VÆV7B‚&—FVÒææf–vFRÖ7W'6÷"×&–v‡BÖVFvR"çFõö÷væVB‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'EöW†çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²Â%óƒƒ“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â†6…6WC£¦g&öÒ…·6VÆV7FVEö—FVÕÒ’“° ¢çF–ÖVÆ–æRç6WEöVF—Eö7W'6÷%÷F–6²ƒ“°¢76W'B‚6öÖÖæG3£¦—5öVæ&ÆVB‚fÂÆVgB’“°¢çF–ÖVÆ–æRç6WEöVF—Eö7W'6÷%÷F–6²ƒ5óƒC“°¢76W'B‚6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ&–v‡B’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“°§Ğ ¢5·FW7EĞ¦fâ6VÆV7FVEö—FVÕö7W'6÷%ö7F–öç5÷W6Uö÷WFW%öVF–õöÖ–F•ö&÷VæG2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6·2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWBVF–õ÷7F'BÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒ“c’çVçw&‚“°¢ÆWBVF–õöVæBÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒó“#’çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢ÖVF–÷&Vc¢&76WC¢ò÷6VÆV7FVBÖ—FVÒÖ7W'6÷"ÖVF–ò"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢VF–õ÷7F'BÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢VF–õöVæBÒVF–õ÷7F'BÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢%óƒƒÀ¢ÆVæwF…÷F–6·3¢CƒÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢CƒÀ¢Ò¢çVçw&‚“°¢ÆWBVF–õö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWB6VÆV7FVEöÖ–F•ö–BÒ ¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒçG&6µö–B‚’ÓÒG&6·5³Ò¢çVçw&‚¢æ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æP¢ç6VÆV7Eö—FV×2‡fV2²‡6VÆV7FVEöÖ–F•ö–BÂG&6·5³Ò’Â†VF–õö–BÂG&6·5³Ò•Ò“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Ò6öÖR‡G&6·5³Ò“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Ò†6…6WC£¦g&öÒ…·G&6·5³ÒÂG&6·5³ÕÒ“°¢çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²ÒUó°¢ÆWB6VÆV7FVEö—FV×2ÒçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6ÆöæR‚“°¢ÆWB&Wf—6–öåö&Vf÷&RÒç&Wf—6–öã°¢ÆWB6å÷VæFõö&Vf÷&RÒç&ö¦V7Bæ6å÷VæFò‚“°¢ÆWB6å÷&VFõö&Vf÷&RÒç&ö¦V7Bæ6å÷&VFò‚“° ¢ÆWB7F'BÒ6öÖÖæD–C£¤Ö÷fTVF—D7W'6÷%Fõ7F'Död—FV×3°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ7F'B’“°¢æ7F–öåöÆ—7@¢ç6WE÷VW'’‚&Ö÷fR7W'6÷"Fò7F'Böb—FV×2"çFõö÷væVB‚’“°¢æ7F–öåöÆ—7@¢ç6VÆV7B‚&—FVÒææf–vFRÖ7W'6÷"×6VÆV7FVB×7F'B"çFõö÷væVB‚’“°¢ÆWB7F'EöVçG'’Ò ¢æ7F–öåöÆ—7EöVçG&–W2‚¢æ–çFõö—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒ7F'B¢æW‡V7B‚'6VÆV7FVB—FVÒ7F'B7F–öâ—26V&6†&ÆR"“°¢76W'B‡7F'EöVçG'’ç6†÷'F7WBæ—5öæöæR‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'EöW†çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²Â“c“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â6VÆV7FVEö—FV×2“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Â6öÖR‡G&6·5³Ò’“°¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2À¢†6…6WC£¦g&öÒ…·G&6·5³ÒÂG&6·5³ÕÒ¢“° ¢ÆWBVæBÒ6öÖÖæD–C£¤Ö÷fTVF—D7W'6÷%FôVæDöd—FV×3°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂVæB’“°¢æ7F–öåöÆ—7@¢ç6WE÷VW'’‚&Ö÷fR7W'6÷"FòVæBöb—FV×2"çFõö÷væVB‚’“°¢æ7F–öåöÆ—7@¢ç6VÆV7B‚&—FVÒææf–vFRÖ7W'6÷"×6VÆV7FVBÖVæB"çFõö÷væVB‚’“°¢ÆWBVæEöVçG'’Ò ¢æ7F–öåöÆ—7EöVçG&–W2‚¢æ–çFõö—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒVæB¢æW‡V7B‚'6VÆV7FVB—FVÒVæB7F–öâ—26V&6†&ÆR"“°¢76W'B†VæEöVçG'’ç6†÷'F7WBæ—5öæöæR‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'EöW†çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²Â5ó3c“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â6VÆV7FVEö—FV×2“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Â6öÖR‡G&6·5³Ò’“°¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2À¢†6…6WC£¦g&öÒ…·G&6·5³ÒÂG&6·5³ÕÒ¢“° ¢çF–ÖVÆ–æRæ6ÆV%÷6VÆV7FVEö—FV×2‚“°¢çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²Òó°¢76W'B‚6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ7F'B’“°¢76W'B‚6öÖÖæG3£¦—5öVæ&ÆVB‚fÂVæB’“°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB‡7F'B’“°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB†VæB’“°¢76W'EöW†çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²Âó“°¢76W'B†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ—5öV×G’‚’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“°§Ğ ¢5·FW7EĞ¦fâ6VÆV7FVEöVF–õö—FVÕö7W'6÷%ö7F–öç5÷&W6W'fU÷6×ÆUöW†7Eö&÷VæF&–W2‚’°¢ÆWBF—&V7F÷'’ÒFV×f–ÆS£§FV×F—"‚’çVçw&‚“°¢ÆWB&ö¦V7E÷F‚ÒF—&V7F÷'’çF‚‚’æ¦ö–â‚'6×ÆRÖW†7BÖ7W'6÷"æFr"“°¢ÆWBÖVF–÷&VbÒ&76WC¢ò÷6×ÆRÖW†7BÖ—FVÒÖ7W'6÷"#°¢ÆWB×WB7F÷&RÒ&ö¦V7E7F÷&S£¦÷Vâ‚g&ö¦V7E÷F‚’çVçw&‚“°¢7F÷&P¢æ–×÷'EöVF–õö76WB€¢ÖVF–÷&VbÀ¢'6×ÆRÖW†7Bçvb"À¢7FC£¦–ó£¤7W'6÷#£¦æWr…³÷Sƒ²EÒ’À¢¢çVçw&‚“°¢7F÷&P¢ç6WEöVF–õö76WEöÖWFFF€¢ÖVF–÷&VbÀ¢fFu÷7F÷&vS£¤VF–ô76WDÖWFFF°¢6öçF–æW#¢'vb"çFõö÷væVB‚’À¢6öFV3¢'6Ò"çFõö÷væVB‚’À¢6×ÆU÷&FS¢6öÖRƒC…ó’À¢6†ææVÅö6÷VçC¢6öÖRƒ’À¢&—G5÷W%÷6×ÆS¢6öÖRƒb’À¢g&ÖUö6÷VçC¢6öÖRƒcó’À¢GW&F–öåöææ÷3¢6öÖRƒó#Sóó’À¢'—FUöÆVã¢6öÖRƒB’À¢ÒÀ¢¢çVçw&‚“°¢7F÷&Ræ6Æ÷6R‚’çVçw&‚“° ¢ÆWB×WBÒ°¢&ö¦V7E÷Fƒ¢6öÖR‡&ö¦V7E÷F‚’À¢âä£¦FVfVÇB‚¢Ó°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWB7F'E÷6×ÆRÒ#ó°¢ÆWBVæE÷6×ÆRÒ3UóC°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢ÖVF–÷&VbçFõö÷væVB‚’À¢7F'E÷6×ÆRÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢VæE÷6×ÆRÒ7F'E÷6×ÆRÀ¢Ò¢çVçw&‚“°¢ÆWB—FVÕö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2²†—FVÕö–BÂG&6µö–B•Ò“° ¢æ7F–öåöÆ—7@¢ç6WE÷VW'’‚&Ö÷fR7W'6÷"Fò7F'Böb—FV×2"çFõö÷væVB‚’“°¢æ7F–öåöÆ—7@¢ç6VÆV7B‚&—FVÒææf–vFRÖ7W'6÷"×6VÆV7FVB×7F'B"çFõö÷væVB‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'EöW€¢çF–ÖVÆ–æRæ7W'6÷%÷÷6—F–öâ‚’À¢7&FS£§F–ÖVÆ–æS£¤VF—D7W'6÷%÷6—F–öã£¥6×ÆR‡7F'E÷6×ÆR¢“°¢76W'EöW€¢çF–ÖVÆ–æRæVF—Eö7W'6÷%÷6×ÆR‚fç&ö¦V7B’À¢ö²‡7F'E÷6×ÆR¢“°¢76W'EöæR€¢ç&ö¦V7@¢ç6×ÆUöE÷F–6²†çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²¢çVçw&‚’À¢7F'E÷6×ÆRÀ¢'F†R–çFVvW"F–6²Æ&VÂÆöæRv÷VÆBVçF—¦RF†—2&÷VæF'’ ¢“°¢ÆWBW†7E÷7F'E÷F–6²Ò ¢ç&ö¦V7@¢çF–6µöE÷6×ÆU÷÷6—F–öâ‡7F'E÷6×ÆR2ccB¢çVçw&‚“°¢76W'B‚†çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6µ÷÷6—F–öâ‚fç&ö¦V7B’ÒW†7E÷7F'E÷F–6²’æ'2‚’ÂRÓ’“°¢76W'EöW†æVF–õö–×÷'E÷Æ6VÖVçB‚’çVçw&‚’ãÂ7F'E÷6×ÆR“°¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ğ¢°¢76W'EöW†çÆ–†VE÷6×ÆRÂ7F'E÷6×ÆR“°¢76W'EöW†çÆ–&6µ÷7F'E÷6×ÆRÂ7F'E÷6×ÆR“°¢Ğ ¢æ7F–öåöÆ—7@¢ç6WE÷VW'’‚&Ö÷fR7W'6÷"FòVæBöb—FV×2"çFõö÷væVB‚’“°¢æ7F–öåöÆ—7@¢ç6VÆV7B‚&—FVÒææf–vFRÖ7W'6÷"×6VÆV7FVBÖVæB"çFõö÷væVB‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'EöW€¢çF–ÖVÆ–æRæ7W'6÷%÷÷6—F–öâ‚’À¢7&FS£§F–ÖVÆ–æS£¤VF—D7W'6÷%÷6—F–öã£¥6×ÆR†VæE÷6×ÆR¢“°¢76W'EöW€¢çF–ÖVÆ–æRæVF—Eö7W'6÷%÷6×ÆR‚fç&ö¦V7B’À¢ö²†VæE÷6×ÆR¢“° ¢ÆWB7Æ—E÷6×ÆRÒ#uó#°¢ç6WEöVF—Eö7W'6÷%÷÷6—F–öâ†7&FS£§F–ÖVÆ–æS£¤VF—D7W'6÷%÷6—F–öã£¥6×ÆR‡7Æ—E÷6×ÆR’“°¢ÆWBòÒçWFFR„ÖW76vS£¥7Æ—E6VÆV7FVD—FV×4D7W'6÷"“°¢ÆWB×WB7Æ—Eö—FV×2Ò ¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æÖ‡Æ—FV×Â†—FVÒç7F'E÷6×ÆR‚’Â—FVÒæVæE÷6×ÆR‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢7Æ—Eö—FV×2ç6÷'E÷Vç7F&ÆR‚“°¢76W'EöW€¢7Æ—Eö—FV×2À¢fV2²‡7F'E÷6×ÆRÂ7Æ—E÷6×ÆR’Â‡7Æ—E÷6×ÆRÂVæE÷6×ÆR•ÒÀ¢&7W'6÷"Ö&6VB7Æ—GF–ær×W7BW6RF†RW†7B6×ÆR&÷VæF'’ ¢“° ¢æ†æFÆU÷F–ÖVÆ–æU÷f–WuöWfVçB†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6WDVF—D7W'6÷"ƒóCC’“°¢76W'EöW€¢çF–ÖVÆ–æRæ7W'6÷%÷÷6—F–öâ‚’À¢7&FS£§F–ÖVÆ–æS£¤VF—D7W'6÷%÷6—F–öã£¥F–6²ƒóCC¢“°¢76W'EöW€¢çF–ÖVÆ–æRæVF—Eö7W'6÷%÷6×ÆR‚fç&ö¦V7B’À¢ç&ö¦V7Bç6×ÆUöE÷F–6²ƒóCC¢“°§Ğ ¢5·FW7EĞ¦fâ—FVÕ÷6æö7F–öç5öÖF6…öVF–õöæEöÖ–F•öw&–EöF—&V7F–öç5÷v—F…öw&÷WVE÷VæFò‚’°¢ÆWB66W2Ò°¢€¢6öÖÖæD–C£¥6æ6VÆV7FVD—FV×4ÆVgBÀ¢&—FVÒç6æÖÆVgB"À¢'6æ—FV×2ÆVgB"À¢có“cÀ¢Uó#C"À¢C…óÀ¢EóƒÀ¢’À¢€¢6öÖÖæD–C£¥6æ6VÆV7FVD—FV×5&–v‡BÀ¢&—FVÒç6æ×&–v‡B"À¢'6æ—FV×2&–v‡B"À¢có“cÀ¢Uó#C"À¢s%óÀ¢UóscÀ¢’À¢€¢6öÖÖæD–C£¥6æ6VÆV7FVD—FV×5FôæV&W7Eö–çBÀ¢&—FVÒç6æÖæV&W7B"À¢'6æ—FV×2FòæV&W7B6æö–çB"À¢cóÀ¢Uó#ƒÀ¢C…óÀ¢EóƒÀ¢’À¢Ó° ¢f÷"†6öÖÖæBÂ7F&ÆUö–BÂÆ&VÂÂ÷&–v–æÅöVF–òÂ÷&–v–æÅöÖ–F’ÂF&vWEöVF–òÂF&vWEöÖ–F’’–à¢66W0¢°¢ÆWB×WBÒ—FVÕ÷6æ÷FW7Eö†÷&–v–æÅöVF–òÂ÷&–v–æÅöÖ–F’“°¢ÆWBVF–õö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBÖ–F•ö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWB6VÆV7FVEö—FV×2ÒçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6ÆöæR‚“°¢ÆWB6VÆV7FVE÷G&6²ÒçF–ÖVÆ–æRç6VÆV7FVE÷G&6³°¢ÆWB6VÆV7FVE÷G&6·2ÒçF–ÖVÆ–æRç6VÆV7FVE÷G&6·2æ6ÆöæR‚“°¢ÆWB7W'6÷%÷F–6²ÒçF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6³°¢ÆWB&Wf—6–öåö&Vf÷&RÒç&Wf—6–öã°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°¢76W'EöW†6öÖÖæG3£§7F&ÆUö–B†6öÖÖæB’æ5öFW&Vb‚’Â6öÖR‡7F&ÆUö–B’“°¢76W'EöW†6öÖÖæG3£¦f–æB‚fÂÆ&VÂ’Â6öÖR†6öÖÖæB’“°¢æ7F–öåöÆ—7Bç6WE÷VW'’†Æ&VÂçFõö÷væVB‚’“°¢æ7F–öåöÆ—7Bç6VÆV7B‡7F&ÆUö–BçFõö÷væVB‚’“°¢ÆWBVçG'’Ò ¢æ7F–öåöÆ—7EöVçG&–W2‚¢æ–çFõö—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒ6öÖÖæB¢æW‡V7B‚&—FVÒ6æ7F–öâ—26V&6†&ÆR"“°¢76W'B†VçG'’ç6†÷'F7WBæ—5öæöæR‚’“° ¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“° ¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚’ÂVF–õö–B“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òç7F'E÷6×ÆR‚’ÂF&vWEöVF–ò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òç6÷W&6Uööfg6WE÷6×ÆW2‚’Â#“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒæÆVæwF…÷6×ÆW2‚’Â#Eó“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚’ÂÖ–F•ö–B“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’ÂF&vWEöÖ–F’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒæÆVæwF…÷F–6·2‚’Â“c“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â6VÆV7FVEö—FV×2“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Â6VÆV7FVE÷G&6²“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Â6VÆV7FVE÷G&6·2“°¢76W'EöW†çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²Â7W'6÷%÷F–6²“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R²“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òç7F'E÷6×ÆR‚’Â÷&–v–æÅöVF–ò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’Â÷&–v–æÅöÖ–F’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â6VÆV7FVEö—FV×2“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òç7F'E÷6×ÆR‚’ÂF&vWEöVF–ò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’ÂF&vWEöÖ–F’“°¢Ğ§Ğ ¢5·FW7EĞ¦fâ—FVÕ÷6æöæV&W7E÷W6W5÷6×ÆUöw&–EöæEö–væ÷&W5÷6æ÷FövvÆR‚’°¢ÆWB×WBÒ—FVÕ÷6æ÷FW7Eöƒcó“cÂUó#C"“°¢ÆWBVF–õö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBÖ–F•ö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç6æöVæ&ÆVBÒfÇ6S° ¢ÆWB6öÖÖæBÒ6öÖÖæD–C£¥6æ6VÆV7FVD—FV×5FôæV&W7Eö–çC°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°¢æ7F–öåöÆ—7Bç6VÆV7B‚&—FVÒç6æÖæV&W7B"çFõö÷væVB‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“° ¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚’ÂVF–õö–B“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òç7F'E÷6×ÆR‚’Âs%ó“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚’ÂÖ–F•ö–B“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’ÂEóƒ“°§Ğ ¢5·FW7EĞ¦fâ—FVÕ÷6æöW†7Eöw&–EöV×G•÷6VÆV7F–öåöæEö'W7•÷7FFW5öFõöæ÷EöVF—B‚’°¢ÆWB×WBÒ—FVÕ÷6æ÷FW7EöƒC…óÂEóƒ“°¢ÆWBVF–õö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBÖ–F•ö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWB&Wf—6–öåö&Vf÷&RÒç&Wf—6–öã°¢ÆWB6å÷VæFõö&Vf÷&RÒç&ö¦V7Bæ6å÷VæFò‚“°¢ÆWB6å÷&VFõö&Vf÷&RÒç&ö¦V7Bæ6å÷&VFò‚“°¢æ7F–öåöÆ—7Bç6VÆV7B‚&—FVÒç6æÖæV&W7B"çFõö÷væVB‚’“° ¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“° ¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òç7F'E÷6×ÆR‚’ÂC…ó“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’ÂEóƒ“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“° ¢çF–ÖVÆ–æRæ6ÆV%÷6VÆV7FVEö—FV×2‚“°¢f÷"6öÖÖæB–â°¢6öÖÖæD–C£¥6æ6VÆV7FVD—FV×4ÆVgBÀ¢6öÖÖæD–C£¥6æ6VÆV7FVD—FV×5&–v‡BÀ¢6öÖÖæD–C£¥6æ6VÆV7FVD—FV×5FôæV&W7Eö–çBÀ¢Ò°¢76W'B‚6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°¢Ğ¢ÆWBòÒçWFFR„ÖW76vS£¥6æ6VÆV7FVD—FV×4ÆVgB“°¢ÆWBòÒçWFFR„ÖW76vS£¥6æ6VÆV7FVD—FV×5&–v‡B“°¢ÆWBòÒçWFFR„ÖW76vS£¥6æ6VÆV7FVD—FV×5FôæV&W7Eö–çB“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚’ÂVF–õö–B“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚’ÂÖ–F•ö–B“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“° ¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2°¢†VF–õö–BÂç&ö¦V7BæVF–õö—FV×2‚•³ÒçG&6µö–B‚’’À¢†Ö–F•ö–BÂç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒçG&6µö–B‚’’À¢Ò“°¢æ–õö'W7’ÒG'VS°¢76W'B‚6öÖÖæG3£¦—5öVæ&ÆVB€¢fÀ¢6öÖÖæD–C£¥6æ6VÆV7FVD—FV×5FôæV&W7Eö–ç@¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6æ6VÆV7FVD—FV×5FôæV&W7Eö–çB“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òç7F'E÷6×ÆR‚’ÂC…ó“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’ÂEóƒ“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°§Ğ ¢5·FW7EĞ¦fâ—FVÕ÷6æö7F–öç5öÖF6…÷&VW%÷7v–æu÷ö–çG5öf÷%öVF–õöæEöÖ–F’‚’°¢ÆWB66W2Ò°¢€¢'÷6—F—fR7v–ærÆVgB"À¢s%óÀ¢%óƒƒÀ¢SÀ¢6öÖÖæD–C£¥6æ6VÆV7FVD—FV×4ÆVgBÀ¢C…óÀ¢ó“#À¢’À¢€¢'÷6—F—fR7v–ær&–v‡B"À¢s%óÀ¢%óƒƒÀ¢SÀ¢6öÖÖæD–C£¥6æ6VÆV7FVD—FV×5&–v‡BÀ¢s…óÀ¢5ó#À¢’À¢€¢'V'FW"×7G&VæwF‚7v–æræV&W7B"À¢c5óÀ¢%óS#À¢#RÀ¢6öÖÖæD–C£¥6æ6VÆV7FVD—FV×5FôæV&W7Eö–çBÀ¢sUóÀ¢5óÀ¢’À¢€¢&gVÆÂ7v–æræV&W7B"À¢s%óÀ¢%óƒƒÀ¢À¢6öÖÖæD–C£¥6æ6VÆV7FVD—FV×5FôæV&W7Eö–çBÀ¢ƒEóÀ¢5ó3cÀ¢’À¢€¢&æVvF—fR7v–æræV&W7B"À¢s%óÀ¢%óƒƒÀ¢ÓSÀ¢6öÖÖæD–C£¥6æ6VÆV7FVD—FV×5FôæV&W7Eö–çBÀ¢ceóÀ¢%ócCÀ¢’À¢€¢&ÖV7W&R7F'B—26¶—VB"À¢“eóÀ¢5óƒCÀ¢SÀ¢6öÖÖæD–C£¥6æ6VÆV7FVD—FV×5FôæV&W7Eö–çBÀ¢s…óÀ¢5ó#À¢’À¢Ó° ¢f÷"†æÖRÂVF–õ÷7F'BÂÖ–F•÷7F'BÂ7v–ærÂ6öÖÖæBÂVF–õ÷F&vWBÂÖ–F•÷F&vWB’–â66W2°¢ÆWB×WBÒ—FVÕ÷6æ÷FW7Eö†VF–õ÷7F'BÂÖ–F•÷7F'B“°¢ÆWBVF–õö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBÖ–F•ö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWB6VÆV7FVEö—FV×2ÒçF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6ÆöæR‚“°¢ÆWB6VÆV7FVE÷G&6·2ÒçF–ÖVÆ–æRç6VÆV7FVE÷G&6·2æ6ÆöæR‚“°¢ÆWBVF—Eö7W'6÷"ÒçF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6³°¢ÆWBòÒçWFFR„ÖW76vS£¥6æ7v–ætw&–D6†ævVB‡G'VR’“°¢ÆWBòÒçWFFR„ÖW76vS£¥6æ7v–ætÖ÷VçD6†ævVB‡7v–ær’“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR…F–ÖVÆ–æTWfVçC£¥FövvÆU6æ’“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢æ7F–öåöÆ—7@¢ç6VÆV7B†6öÖÖæG3£§7F&ÆUö–B†6öÖÖæB’æW‡V7B‚'6æ7F–öâ†27F&ÆR&Vv—7G'’”B"’“° ¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“° ¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚’ÂVF–õö–BÂ'¶æÖWÒ"“°¢76W'EöW€¢ç&ö¦V7BæVF–õö—FV×2‚•³Òç7F'E÷6×ÆR‚’À¢VF–õ÷F&vWBÀ¢'¶æÖWÒVF–òF&vWB ¢“°¢76W'EöW€¢ç&ö¦V7BæVF–õö—FV×2‚•³ÒæÆVæwF…÷6×ÆW2‚’À¢#EóÀ¢'¶æÖWÒ ¢“°¢76W'EöW€¢ç&ö¦V7BæVF–õö—FV×2‚•³Òç6÷W&6Uööfg6WE÷6×ÆW2‚’À¢#À¢'¶æÖWÒ ¢“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚’ÂÖ–F•ö–BÂ'¶æÖWÒ"“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’À¢Ö–F•÷F&vWBÀ¢'¶æÖWÒÔ”D’F&vWB ¢“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒæÆVæwF…÷F–6·2‚’Â“cÂ'¶æÖWÒ"“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â6VÆV7FVEö—FV×2Â'¶æÖWÒ"“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Â6VÆV7FVE÷G&6·2Â'¶æÖWÒ"“°¢76W'EöW†çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²ÂVF—Eö7W'6÷"Â'¶æÖWÒ"“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²Â'¶æÖWÒ"“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW€¢ç&ö¦V7BæVF–õö—FV×2‚•³Òç7F'E÷6×ÆR‚’À¢VF–õ÷7F'BÀ¢'¶æÖWÒ ¢“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’À¢Ö–F•÷7F'BÀ¢'¶æÖWÒ ¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW€¢ç&ö¦V7BæVF–õö—FV×2‚•³Òç7F'E÷6×ÆR‚’À¢VF–õ÷F&vWBÀ¢'¶æÖWÒ ¢“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’À¢Ö–F•÷F&vWBÀ¢'¶æÖWÒ ¢“°¢Ğ§Ğ ¢5·FW7EĞ¦fâÖ–F•ö—FVÕ÷6æ÷&WF–ç5÷&VW%ög&7F–öæÅ÷7v–æu÷‚’°¢ÆWB&ö¦V7BÒ&ö¦V7C£¦æWr‚“°¢ÆWB7G&–v‡BÒÖ–F”—FVÕ÷6—F–öã£¦æWrƒ“cÂ’çVçw&‚“° ¢76W'EöW€¢Ö–F•ö—FVÕ÷7v–æu÷6æ÷F&vWB‚g&ö¦V7BÂ7G&–v‡BÂ“cÂ—FVÕ6æF—&V7F–öã£¥&–v‡BÂÂ’çVçw&‚’À¢Ö–F”—FVÕ÷6—F–öã£¦æWrƒ“cBÂc’çVçw&‚’À¢%$TU"7F–öâCƒ2Æ6W2³R7v–ærB“cBã‚ ¢“°¢76W'EöW€¢Ö–F•ö—FVÕ÷7v–æu÷6æ÷F&vWB‚g&ö¦V7BÂ7G&–v‡BÂ“cÂ—FVÕ6æF—&V7F–öã£¤æV&W7BÂÓÂ¢çVçw&‚’À¢Ö–F”—FVÕ÷6—F–öã£¦æWrƒ“SRÂC’çVçw&‚’À¢%$TU"7F–öâCƒBÆ6W2ÓR7v–ærB“SRã" ¢“°§Ğ ¢5·FW7EĞ¦fâ6æöw&–E÷6WGF–æw5ö6öÖÖæEö—5÷&WW6&ÆUöæEö6Æ×5÷7v–æuöÖ÷VçB‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBVçG'’Ò ¢æ7F–öåöÆ—7EöVçG&–W2‚¢æ–çFõö—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒ6öÖÖæD–C£¤÷Vå6æw&–E6WGF–æw2¢æW‡V7B‚'6æöw&–B6WGF–æw2—2f–Æ&ÆR–âF†R7F–öâÆ—7B"“°¢76W'EöW†VçG'’æÆ&VÂÂ%6æöw&–B6WGF–æw2âââ"“°¢76W'EöW†VçG'’ç6†÷'F7WBæ5öFW&Vb‚’Â6öÖR‚$ÇB´Â"’“°¢76W'EöW€¢6öÖÖæG3£§7F&ÆUö–B„6öÖÖæD–C£¥FövvÆU7v–ætw&–B’æ5öFW&Vb‚’À¢6öÖR‚&w&–BçFövvÆR×7v–ær"¢“° ¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¤÷Vå6æw&–E6WGF–æw2’“°¢ÆWBv–æF÷uö–BÒ ¢ç6æöw&–E÷6WGF–æw5÷v–æF÷uö–@¢æW‡V7B‚'6æöw&–B6WGF–æw26öÖÖæB6†÷VÆB÷Vâ—G2v–æF÷r"“°¢76W'EöW†çv–æF÷u÷F—FÆR‡v–æF÷uö–B’Â%6æôw&–B6WGF–æw2"“°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¤÷Vå6æw&–E6WGF–æw2’“°¢76W'EöW†ç6æöw&–E÷6WGF–æw5÷v–æF÷uö–BÂ6öÖR‡v–æF÷uö–B’“° ¢ÆWB–æ—F–Å÷&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¥FövvÆU7v–ætw&–B’“°¢76W'B†çF–ÖVÆ–æRç6æ÷7v–æuöVæ&ÆVB‚’“°¢76W'EöW†ç&Wf—6–öâÂ–æ—F–Å÷&Wf—6–öâ²“°¢ÆWBòÒçWFFR„ÖW76vS£¥6æ7v–ætÖ÷VçD6†ævVBƒS’“°¢76W'EöW†çF–ÖVÆ–æRç6æ÷7v–æuöÖ÷VçE÷W&6VçB‚’Â“°¢76W'EöW†ç&Wf—6–öâÂ–æ—F–Å÷&Wf—6–öâ²"“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR…F–ÖVÆ–æTWfVçC£¥6WE6æw&–B€¢7&FS£§F–ÖVÆ–æS£¥6æw&–C£¤V–v‡F‚À¢’’“°¢76W'EöW†çF–ÖVÆ–æRç6æöw&–BÂ7&FS£§F–ÖVÆ–æS£¥6æw&–C£¤V–v‡F‚“°¢76W'EöW†ç&Wf—6–öâÂ–æ—F–Å÷&Wf—6–öâ²2“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR…F–ÖVÆ–æTWfVçC£¥6WDÖ–F”VF—F÷%6æw&–B€¢7&FS£§F–ÖVÆ–æS£¥6æw&–C£¥6—‡FVVçF‚À¢’’“°¢76W'EöW€¢çF–ÖVÆ–æRæÖ–F•öVF—F÷%÷6æöw&–BÀ¢7&FS£§F–ÖVÆ–æS£¥6æw&–C£¥6—‡FVVçF€¢“°¢76W'EöW†ç&Wf—6–öâÂ–æ—F–Å÷&Wf—6–öâ²B“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR…F–ÖVÆ–æTWfVçC£¥FövvÆU6æ’“°¢76W'B‚çF–ÖVÆ–æRç6æöVæ&ÆVB“°¢76W'EöW†ç&Wf—6–öâÂ–æ—F–Å÷&Wf—6–öâ²R“°¢ÆWBòÒçWFFR„ÖW76vS£¥v–æF÷t6Æ÷6VB‡v–æF÷uö–B’“°¢76W'EöW†ç6æöw&–E÷6WGF–æw5÷v–æF÷uö–BÂæöæR“°§Ğ ¢5·FW7EĞ¦fâ6VÆV7EöÆÅö—FV×5ö–å÷G&6µö7F–öå÷6VÆV7G5öÆÅ÷6VÆV7FVE÷G&6·2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6·2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢ÖVF–÷&Vc¢&76WC¢ò÷6VÆV7B×G&6²Ö—FV×2"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢#EóÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢“cÀ¢ÆVæwF…÷F–6·3¢5óƒCÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢ÖVF–÷&Vc¢&76WC¢ò÷6VÆV7B×G&6²Ö—FV×2Ö÷F†W""çFõö÷væVB‚’À¢7F'E÷6×ÆS¢C…óÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò¢çVçw&‚“°¢ÆWBf—'7EöVF–òÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWB÷F†W%÷G&6µöVF–òÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBf—'7E÷G&6µöÖ–F’Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Ò6öÖR‡G&6·5³Ò“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Ò†6…6WC£¦g&öÒ…·G&6·5³ÒÂG&6·5³ÕÒ“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ–ç6W'B†÷F†W%÷G&6µöVF–ò“° ¢ÆWB6öÖÖæBÒ6öÖÖæD–C£¥6VÆV7DÆÄ—FV×4–åG&6³°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°¢76W'EöW€¢6öÖÖæG3£¦f–æB‚fÂ'6VÆV7BÆÂ—FV×2–âG&6²"’À¢6öÖR†6öÖÖæB¢“°¢76W'EöW€¢6öÖÖæG3£§7F&ÆUö–B†6öÖÖæB’æ5öFW&Vb‚’À¢6öÖR‚&—FVÒç6VÆV7BÖÆÂÖ–â×G&6²"¢“°¢æ7F–öåöÆ—7@¢ç6WE÷VW'’‚'6VÆV7BÆÂ—FV×2–âG&6²"çFõö÷væVB‚’“°¢ÆWBVçG&–W2Òæ7F–öåöÆ—7EöVçG&–W2‚“°¢ÆWBVçG'’ÒVçG&–W0¢æ—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒ6öÖÖæB¢æW‡V7B‚'G&6²7F–öâ—26V&6†&ÆR"“°¢76W'B†VçG'’ç6†÷'F7WBæ—5öæöæR‚’“°¢æ7F–öåöÆ—7@¢ç6VÆV7B‚&—FVÒç6VÆV7BÖÆÂÖ–â×G&6²"çFõö÷væVB‚’“° ¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“° ¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2À¢†6…6WC£¦g&öÒ…¶f—'7EöVF–òÂf—'7E÷G&6µöÖ–F’Â÷F†W%÷G&6µöVF–õÒ¢“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR†÷F†W%÷G&6µöVF–ò’“°§Ğ ¢5·FW7EĞ¦fâ6VÆV7EöÆÅö—FV×5ö–å÷F–ÖU÷6VÆV7F–öåö7F–öå÷6VÆV7G5ö÷fW&Æ–æuöVF–õöæEöÖ–F’‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6·2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢ÖVF–÷&Vc¢&76WC¢ò÷F–ÖR×6VÆV7F–öâÖ÷fW&Æ"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢#EóÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢#EóÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢%ósÀ¢ÆVæwF…÷F–6·3¢3À¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢ÖVF–÷&Vc¢&76WC¢ò÷F–ÖR×6VÆV7F–öâÖVæBÖ&÷VæF'’"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢s%óÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢#EóÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³%ÒÀ¢7F'E÷F–6³¢%óƒƒÀ¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6·5³%ÒÀ¢ÖVF–÷&Vc¢&76WC¢ò÷F–ÖR×6VÆV7F–öâ×7F'BÖ&÷VæF'’"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢#EóÀ¢Ò¢çVçw&‚“° ¢ÆWB÷fW&Æ–æuöVF–òÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBVæEö&÷VæF'•öVF–òÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWB7F'Eö&÷VæF'•öVF–òÒç&ö¦V7BæVF–õö—FV×2‚•³%Òæ–B‚“°¢ÆWB÷fW&Æ–æuöÖ–F’Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWBVæEö&÷VæF'•öÖ–F’Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâÒ6öÖR†7&FS£§F–ÖVÆ–æS£¥F–ÖU6VÆV7F–öâ°¢7F'E÷F–6³¢“cÀ¢VæE÷F–6³¢%óƒƒÀ¢Ò“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Ğ¢†6…6WC£¦g&öÒ…¶VæEö&÷VæF'•öVF–òÂVæEö&÷VæF'•öÖ–F’Â7F'Eö&÷VæF'•öVF–õÒ“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÒ6öÖR†VæEö&÷VæF'•öÖ–F’“° ¢ÆWB6öÖÖæBÒ6öÖÖæD–C£¥6VÆV7DÆÄ—FV×4–åF–ÖU6VÆV7F–öã°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°¢76W'EöW€¢6öÖÖæG3£¦f–æB‚fÂ'6VÆV7BÆÂ—FV×2–â7W'&VçBF–ÖR6VÆV7F–öâ"’À¢6öÖR†6öÖÖæB¢“°¢76W'EöW€¢6öÖÖæG3£§7F&ÆUö–B†6öÖÖæB’æ5öFW&Vb‚’À¢6öÖR‚&—FVÒç6VÆV7BÖÆÂÖ–â×F–ÖR×6VÆV7F–öâ"¢“°¢æ7F–öåöÆ—7@¢ç6WE÷VW'’‚'6VÆV7BÆÂ—FV×2–â7W'&VçBF–ÖR6VÆV7F–öâ"çFõö÷væVB‚’“°¢ÆWBVçG&–W2Òæ7F–öåöÆ—7EöVçG&–W2‚“°¢ÆWBVçG'’ÒVçG&–W0¢æ—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒ6öÖÖæB¢æW‡V7B‚'F–ÖR×6VÆV7F–öâ7F–öâ—26V&6†&ÆR"“°¢76W'B†VçG'’ç6†÷'F7WBæ—5öæöæR‚’“°¢æ7F–öåöÆ—7@¢ç6VÆV7B‚&—FVÒç6VÆV7BÖÆÂÖ–â×F–ÖR×6VÆV7F–öâ"çFõö÷væVB‚’“°¢ÆWB&Wf—6–öåö&Vf÷&RÒç&Wf—6–öã°¢ÆWB6å÷VæFõö&Vf÷&RÒç&ö¦V7Bæ6å÷VæFò‚“°¢ÆWB6å÷&VFõö&Vf÷&RÒç&ö¦V7Bæ6å÷&VFò‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“° ¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2À¢†6…6WC£¦g&öÒ…¶÷fW&Æ–æuöVF–òÂ÷fW&Æ–æuöÖ–F•Ò¢“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR†÷fW&Æ–æuöÖ–F’’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“° ¢çF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâÒæöæS°¢76W'B‚6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°¢çF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâÒ6öÖR†7&FS£§F–ÖVÆ–æS£¥F–ÖU6VÆV7F–öâ°¢7F'E÷F–6³¢UóÀ¢VæE÷F–6³¢eóÀ¢Ò“°¢76W'B‚6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°§Ğ ¢5·FW7EĞ¦fâ6VÆV7EöÆÅö—FV×5ööå÷6VÆV7FVE÷G&6·5ö–å÷F–ÖU÷6VÆV7F–öåö7F–öå÷&W7V7G5ö×VÇF•÷G&6µ÷66÷R‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6·2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢ÖVF–÷&Vc¢&76WC¢ò÷6VÆV7FVB×G&6²Öf—'7B"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢#EóÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢#EóÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢ó#À¢ÆVæwF…÷F–6·3¢cÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6·5³%ÒÀ¢ÖVF–÷&Vc¢&76WC¢ò÷6VÆV7FVB×G&6²ÖÆ7B"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢C…óÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢%óÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³%ÒÀ¢7F'E÷F–6³¢EóÀ¢ÆVæwF…÷F–6·3¢SÀ¢Ò¢çVçw&‚“° ¢ÆWBf—'7EöVF–òÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBVç6VÆV7FVE÷G&6µöÖ–F’Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWB6VÆV7FVE÷G&6µöVF–òÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWB÷WEööe÷&ævUöÖ–F’Òç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâÒ6öÖR†7&FS£§F–ÖVÆ–æS£¥F–ÖU6VÆV7F–öâ°¢7F'E÷F–6³¢óÀ¢VæE÷F–6³¢5óÀ¢Ò“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Ò†6…6WC£¦g&öÒ…·G&6·5³ÒÂG&6·5³%ÕÒ“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Ò6öÖR‡G&6·5³%Ò“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Ò†6…6WC£¦g&öÒ…·Vç6VÆV7FVE÷G&6µöÖ–F’Â÷WEööe÷&ævUöÖ–F•Ò“° ¢ÆWB6öÖÖæBÒ6öÖÖæD–C£¥6VÆV7DÆÄ—FV×4öå6VÆV7FVEG&6·4–åF–ÖU6VÆV7F–öã°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°¢76W'EöW€¢6öÖÖæG3£¦f–æB€¢fÀ¢'6VÆV7BÆÂ—FV×2öâ6VÆV7FVBG&6·2–â7W'&VçBF–ÖR6VÆV7F–öâ ¢’À¢6öÖR†6öÖÖæB¢“°¢76W'EöW€¢6öÖÖæG3£§7F&ÆUö–B†6öÖÖæB’æ5öFW&Vb‚’À¢6öÖR‚&—FVÒç6VÆV7BÖÆÂÖöâ×6VÆV7FVB×G&6·2Ö–â×F–ÖR×6VÆV7F–öâ"¢“°¢æ7F–öåöÆ—7@¢ç6WE÷VW'’‚'6VÆV7BÆÂ—FV×2öâ6VÆV7FVBG&6·2–â7W'&VçBF–ÖR6VÆV7F–öâ"çFõö÷væVB‚’“°¢ÆWBVçG&–W2Òæ7F–öåöÆ—7EöVçG&–W2‚“°¢76W'EöW†VçG&–W2æÆVâ‚’Â“°¢76W'B†VçG&–W5³Òç6†÷'F7WBæ—5öæöæR‚’“°¢æ7F–öåöÆ—7@¢ç6VÆV7B‚&—FVÒç6VÆV7BÖÆÂÖöâ×6VÆV7FVB×G&6·2Ö–â×F–ÖR×6VÆV7F–öâ"çFõö÷væVB‚’“°¢ÆWB&Wf—6–öåö&Vf÷&RÒç&Wf—6–öã°¢ÆWB6å÷VæFõö&Vf÷&RÒç&ö¦V7Bæ6å÷VæFò‚“°¢ÆWB6å÷&VFõö&Vf÷&RÒç&ö¦V7Bæ6å÷&VFò‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“° ¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2À¢†6…6WC£¦g&öÒ…¶f—'7EöVF–òÂ6VÆV7FVE÷G&6µöVF–õÒ¢“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR‡6VÆV7FVE÷G&6µöVF–ò’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“° ¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2æ6ÆV"‚“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6²ÒæöæS°¢76W'B‚6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°§Ğ ¢5·FW7EĞ¦fâVç6VÆV7EöÆÅö—FV×5ö7F–öåö6ÆV'5ö—FVÕ÷6VÆV7F–öåöæE÷&W6W'fW5ö÷F†W%÷f–Wu÷7FFR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6·2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢ÖVF–÷&Vc¢&76WC¢ò÷Vç6VÆV7BÖ—FV×2"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢#EóÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢#EóÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢ó#À¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢çVçw&‚“°¢ÆWBVF–õö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBÖ–F•ö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æP¢ç6VÆV7Eö—FV×2‡fV2²†VF–õö–BÂG&6·5³Ò’Â†Ö–F•ö–BÂG&6·5³Ò•Ò“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Ò6öÖR‡G&6·5³Ò“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Ò†6…6WC£¦g&öÒ…·G&6·5³ÒÂG&6·5³ÕÒ“°¢çF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâÒ6öÖR†7&FS£§F–ÖVÆ–æS£¥F–ÖU6VÆV7F–öâ°¢7F'E÷F–6³¢“cÀ¢VæE÷F–6³¢%óCÀ¢Ò“° ¢ÆWB6öÖÖæBÒ6öÖÖæD–C£¥Vç6VÆV7DÆÄ—FV×3°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°¢76W'EöW†6öÖÖæG3£¦f–æB‚fÂ'Vç6VÆV7BÆÂ—FV×2"’Â6öÖR†6öÖÖæB’“°¢76W'EöW€¢6öÖÖæG3£§7F&ÆUö–B†6öÖÖæB’æ5öFW&Vb‚’À¢6öÖR‚&—FVÒçVç6VÆV7BÖÆÂ"¢“°¢æ7F–öåöÆ—7Bç6WE÷VW'’‚'Vç6VÆV7BÆÂ—FV×2"çFõö÷væVB‚’“°¢ÆWBVçG&–W2Òæ7F–öåöÆ—7EöVçG&–W2‚“°¢ÆWBVçG'’ÒVçG&–W0¢æ—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒ6öÖÖæB¢æW‡V7B‚'Vç6VÆV7B7F–öâ—26V&6†&ÆR"“°¢76W'B†VçG'’ç6†÷'F7WBæ—5öæöæR‚’“°¢æ7F–öåöÆ—7Bç6VÆV7B‚&—FVÒçVç6VÆV7BÖÆÂ"çFõö÷væVB‚’“°¢ÆWB&Wf—6–öåö&Vf÷&RÒç&Wf—6–öã°¢ÆWB6å÷VæFõö&Vf÷&RÒç&ö¦V7Bæ6å÷VæFò‚“°¢ÆWB6å÷&VFõö&Vf÷&RÒç&ö¦V7Bæ6å÷&VFò‚“°¢ÆWBF–ÖU÷6VÆV7F–öåö&Vf÷&RÒçF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öã°¢ÆWB6VÆV7FVE÷G&6µö&Vf÷&RÒçF–ÖVÆ–æRç6VÆV7FVE÷G&6³°¢ÆWB6VÆV7FVE÷G&6·5ö&Vf÷&RÒçF–ÖVÆ–æRç6VÆV7FVE÷G&6·2æ6ÆöæR‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“° ¢76W'B†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ—5öV×G’‚’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂæöæR“°¢76W'EöW†çF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâÂF–ÖU÷6VÆV7F–öåö&Vf÷&R“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Â6VÆV7FVE÷G&6µö&Vf÷&R“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Â6VÆV7FVE÷G&6·5ö&Vf÷&R“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“°¢76W'B‚6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°§Ğ ¢5·FW7EĞ¦fâ6VÆV7Eö—FVÕ÷VæFW%öÖ÷W6Uö7F–öå÷6VÆV7G5ö—FVÕö6ÆV'5öV×G•öÆæUöæEöæö÷5ö÷WG6–FUö'&ævR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6·2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢f÷"‡G&6µö–BÂ7F'E÷F–6²’–â²‡G&6·5³ÒÂ#C’Â‡G&6·5³ÒÂó#•Ò°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6²À¢ÆVæwF…÷F–6·3¢CƒÀ¢Ò¢çVçw&‚“°¢Ğ¢ÆWB†÷fW&VEö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWB&Wf–÷W6Ç•÷6VÆV7FVEö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æP¢ç6VÆV7Eö—FV×2‡fV2²‡&Wf–÷W6Ç•÷6VÆV7FVEö–BÂG&6·5³Ò•Ò“°¢çF–ÖVÆ–æRç—†VÇ5÷W%÷F–6²Òã°¢ÆWB&÷rÒçF–ÖVÆ–æRç&÷uöÆ–÷WBƒ’æW‡V7B‚&f—'7BG&6²†2&÷r"“°¢çF–ÖVÆ–æP¢æ†æFÆR…F–ÖVÆ–æTWfVçC£¥ö–çFW$Ö÷fVB†–6VC£¥ö–çC£¦æWr€¢#rãÀ¢&÷rçF÷²#ãÀ¢’’“°¢76W'EöW†çF–ÖVÆ–æRæ†÷fW&VEö—FVÒÂ6öÖR††÷fW&VEö–B’“° ¢ÆWB6öÖÖæBÒ6öÖÖæD–C£¥6VÆV7D—FVÕVæFW$Ö÷W6T7W'6÷#°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°¢76W'EöW€¢6öÖÖæG3£¦f–æB‚fÂ'6VÆV7B—FVÒVæFW"Ö÷W6R7W'6÷""’À¢6öÖR†6öÖÖæB¢“°¢76W'EöW€¢6öÖÖæG3£§7F&ÆUö–B†6öÖÖæB’æ5öFW&Vb‚’À¢6öÖR‚&—FVÒç6VÆV7B×VæFW"ÖÖ÷W6R"¢“°¢æ7F–öåöÆ—7@¢ç6WE÷VW'’‚'6VÆV7B—FVÒVæFW"Ö÷W6R7W'6÷""çFõö÷væVB‚’“°¢ÆWBVçG'’Ò ¢æ7F–öåöÆ—7EöVçG&–W2‚¢æ–çFõö—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒ6öÖÖæB¢æW‡V7B‚'VæFW"ÖÖ÷W6R7F–öâ—26V&6†&ÆR"“°¢76W'B€¢VçG'’æVæ&ÆVBÀ¢'F†RæF—fR7F–öâÆ—7B'Vâ&VÖ–ç2f–Æ&ÆR ¢“°¢76W'B†VçG'’ç6†÷'F7WBæ—5öæöæR‚’“°¢æ7F–öåöÆ—7Bç6VÆV7B‚&—FVÒç6VÆV7B×VæFW"ÖÖ÷W6R"çFõö÷væVB‚’“°¢ÆWB&Wf—6–öåö&Vf÷&RÒç&Wf—6–öã°¢ÆWB6å÷VæFõö&Vf÷&RÒç&ö¦V7Bæ6å÷VæFò‚“°¢ÆWB6å÷&VFõö&Vf÷&RÒç&ö¦V7Bæ6å÷&VFò‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“° ¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â†6…6WC£¦g&öÒ…¶†÷fW&VEö–EÒ’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR††÷fW&VEö–B’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Â6öÖR‡G&6·5³Ò’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“° ¢çF–ÖVÆ–æP¢æ†æFÆR…F–ÖVÆ–æTWfVçC£¥ö–çFW$Ö÷fVB†–6VC£¥ö–çC£¦æWr€¢CãÀ¢&÷rçF÷²#ãÀ¢’’“°¢76W'EöW†çF–ÖVÆ–æRæ†÷fW&VEö—FVÒÂæöæR“°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'B†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ—5öV×G’‚’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂæöæR“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â†6…6WC£¦g&öÒ…¶†÷fW&VEö–EÒ’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR††÷fW&VEö–B’“° ¢çF–ÖVÆ–æRæ†æFÆR…F–ÖVÆ–æTWfVçC£¥6WD†÷fW&VD—FVÒ„æöæR’“°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â†6…6WC£¦g&öÒ…¶†÷fW&VEö–EÒ’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR††÷fW&VEö–B’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“°§Ğ ¢5·FW7EĞ¦fâ6VÆV7Eö—FVÕ÷VæFW%öÖ÷W6UöÆVf–æuö÷F†W%ö—FV×5÷6VÆV7FVEöFG5÷v—F†÷WE÷FövvÆ–ær‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6·2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢f÷"‡G&6µö–BÂ7F'E÷F–6²’–â²‡G&6·5³ÒÂ#C’Â‡G&6·5³ÒÂó#•Ò°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6²À¢ÆVæwF…÷F–6·3¢CƒÀ¢Ò¢çVçw&‚“°¢Ğ¢ÆWB&Wf–÷W6Ç•÷6VÆV7FVEö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWB†÷fW&VEö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æP¢ç6VÆV7Eö—FV×2‡fV2²‡&Wf–÷W6Ç•÷6VÆV7FVEö–BÂG&6·5³Ò•Ò“°¢çF–ÖVÆ–æRç6VÆV7E÷G&6µööæÇ’‡G&6·5³Ò“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Â†6…6WC£¦g&öÒ…·G&6·5³ÕÒ’“°¢çF–ÖVÆ–æRæ†÷fW&VEö—FVÒÒ6öÖR††÷fW&VEö–B“° ¢ÆWB6öÖÖæBÒ6öÖÖæD–C£¤FD—FVÕVæFW$Ö÷W6UFõ6VÆV7F–öã°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°¢76W'EöW€¢6öÖÖæG3£¦f–æB€¢fÀ¢'6VÆV7B—FVÒVæFW"Ö÷W6R7W'6÷"†ÆVf–ær÷F†W"—FV×26VÆV7FVB’ ¢’À¢6öÖR†6öÖÖæB¢“°¢76W'EöW€¢6öÖÖæG3£§7F&ÆUö–B†6öÖÖæB’æ5öFW&Vb‚’À¢6öÖR‚&—FVÒæFB×VæFW"ÖÖ÷W6R×Fò×6VÆV7F–öâ"¢“°¢æ7F–öåöÆ—7@¢ç6WE÷VW'’‚&ÆVf–ær÷F†W"—FV×26VÆV7FVB"çFõö÷væVB‚’“°¢ÆWBVçG'’Ò ¢æ7F–öåöÆ—7EöVçG&–W2‚¢æ–çFõö—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒ6öÖÖæB¢æW‡V7B‚&FB×VæFW"ÖÖ÷W6R7F–öâ—26V&6†&ÆR"“°¢76W'B†VçG'’æVæ&ÆVB“°¢76W'B†VçG'’ç6†÷'F7WBæ—5öæöæR‚’“°¢æ7F–öåöÆ—7@¢ç6VÆV7B‚&—FVÒæFB×VæFW"ÖÖ÷W6R×Fò×6VÆV7F–öâ"çFõö÷væVB‚’“°¢ÆWB&Wf—6–öåö&Vf÷&RÒç&Wf—6–öã°¢ÆWB6å÷VæFõö&Vf÷&RÒç&ö¦V7Bæ6å÷VæFò‚“°¢ÆWB6å÷&VFõö&Vf÷&RÒç&ö¦V7Bæ6å÷&VFò‚“° ¢f÷"ò–ââã"°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2À¢†6…6WC£¦g&öÒ…·&Wf–÷W6Ç•÷6VÆV7FVEö–BÂ†÷fW&VEö–EÒ¢“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR††÷fW&VEö–B’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Â6öÖR‡G&6·5³Ò’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Â†6…6WC£¦g&öÒ…·G&6·5³ÕÒ’“°¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7F–öåöæ6†÷"‚’À¢6öÖR‡&Wf–÷W6Ç•÷6VÆV7FVEö–B¢“°¢Ğ¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“° ¢çF–ÖVÆ–æRæ†æFÆR…F–ÖVÆ–æTWfVçC£¥6WD†÷fW&VD—FVÒ„æöæR’“°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2À¢†6…6WC£¦g&öÒ…·&Wf–÷W6Ç•÷6VÆV7FVEö–BÂ†÷fW&VEö–EÒ¢“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR††÷fW&VEö–B’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Â6öÖR‡G&6·5³Ò’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Â†6…6WC£¦g&öÒ…·G&6·5³ÕÒ’“°¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7F–öåöæ6†÷"‚’À¢6öÖR‡&Wf–÷W6Ç•÷6VÆV7FVEö–B¢“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“°§Ğ ¢5·FW7EĞ¦fâFövvÆU÷6VÆV7F–öåööeö—FVÕ÷VæFW%öÖ÷W6UöFG5öæE÷&VÖ÷fW5÷F†Uö†÷fW&VEö—FVÒ‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6·2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢f÷"‡G&6µö–BÂ7F'E÷F–6²’–â²‡G&6·5³ÒÂ#C’Â‡G&6·5³ÒÂó#•Ò°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6²À¢ÆVæwF…÷F–6·3¢CƒÀ¢Ò¢çVçw&‚“°¢Ğ¢ÆWB&WF–æVEö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWB†÷fW&VEö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æP¢ç6VÆV7Eö—FV×2‡fV2²††÷fW&VEö–BÂG&6·5³Ò’Â‡&WF–æVEö–BÂG&6·5³Ò•Ò“°¢çF–ÖVÆ–æRç6VÆV7E÷G&6µööæÇ’‡G&6·5³Ò“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Â†6…6WC£¦g&öÒ…·G&6·5³ÕÒ’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7F–öåöæ6†÷"‚’Â6öÖR‡&WF–æVEö–B’“°¢çF–ÖVÆ–æRæ†÷fW&VEö—FVÒÒ6öÖR††÷fW&VEö–B“° ¢ÆWB6öÖÖæBÒ6öÖÖæD–C£¥FövvÆU6VÆV7F–öäöd—FVÕVæFW$Ö÷W6S°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°¢76W'EöW€¢6öÖÖæG3£¦f–æB‚fÂ'FövvÆR6VÆV7F–öâöb—FVÒVæFW"Ö÷W6R7W'6÷""’À¢6öÖR†6öÖÖæB¢“°¢76W'EöW€¢6öÖÖæG3£§7F&ÆUö–B†6öÖÖæB’æ5öFW&Vb‚’À¢6öÖR‚&—FVÒçFövvÆR×VæFW"ÖÖ÷W6R×6VÆV7F–öâ"¢“°¢æ7F–öåöÆ—7@¢ç6WE÷VW'’‚'FövvÆR6VÆV7F–öâöb—FVÒVæFW"Ö÷W6R7W'6÷""çFõö÷væVB‚’“°¢ÆWBVçG'’Ò ¢æ7F–öåöÆ—7EöVçG&–W2‚¢æ–çFõö—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒ6öÖÖæB¢æW‡V7B‚'FövvÆR×VæFW"ÖÖ÷W6R7F–öâ—26V&6†&ÆR"“°¢76W'B†VçG'’æVæ&ÆVB“°¢76W'B†VçG'’ç6†÷'F7WBæ—5öæöæR‚’“°¢æ7F–öåöÆ—7@¢ç6VÆV7B‚&—FVÒçFövvÆR×VæFW"ÖÖ÷W6R×6VÆV7F–öâ"çFõö÷væVB‚’“°¢ÆWB&Wf—6–öåö&Vf÷&RÒç&Wf—6–öã°¢ÆWB6å÷VæFõö&Vf÷&RÒç&ö¦V7Bæ6å÷VæFò‚“°¢ÆWB6å÷&VFõö&Vf÷&RÒç&ö¦V7Bæ6å÷&VFò‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“° ¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â†6…6WC£¦g&öÒ…·&WF–æVEö–EÒ’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR‡&WF–æVEö–B’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Â6öÖR‡G&6·5³Ò’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Â†6…6WC£¦g&öÒ…·G&6·5³ÕÒ’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7F–öåöæ6†÷"‚’Â6öÖR‡&WF–æVEö–B’“° ¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“° ¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2À¢†6…6WC£¦g&öÒ…·&WF–æVEö–BÂ†÷fW&VEö–EÒ¢“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR††÷fW&VEö–B’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Â6öÖR‡G&6·5³Ò’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Â†6…6WC£¦g&öÒ…·G&6·5³ÕÒ’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7F–öåöæ6†÷"‚’Â6öÖR‡&WF–æVEö–B’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“° ¢çF–ÖVÆ–æRæ†æFÆR…F–ÖVÆ–æTWfVçC£¥6WD†÷fW&VD—FVÒ„æöæR’“°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2À¢†6…6WC£¦g&öÒ…·&WF–æVEö–BÂ†÷fW&VEö–EÒ¢“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR††÷fW&VEö–B’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Â6öÖR‡G&6·5³Ò’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Â†6…6WC£¦g&öÒ…·G&6·5³ÕÒ’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7F–öåöæ6†÷"‚’Â6öÖR‡&WF–æVEö–B’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“° ¢çF–ÖVÆ–æRæ†÷fW&VEö—FVÒÒ6öÖR‡&WF–æVEö–B“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â†6…6WC£¦g&öÒ…¶†÷fW&VEö–EÒ’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR††÷fW&VEö–B’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7F–öåöæ6†÷"‚’Â6öÖR‡&WF–æVEö–B’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“° ¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2²††÷fW&VEö–BÂG&6·5³Ò•Ò“°¢çF–ÖVÆ–æRæ†÷fW&VEö—FVÒÒ6öÖR††÷fW&VEö–B“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'B†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ—5öV×G’‚’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂæöæR“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7F–öåöæ6†÷"‚’Â6öÖR††÷fW&VEö–B’“°§Ğ ¢5·FW7EĞ¦fâ†÷fW%÷6VÆV7F–öåö7F–öç5÷6VÆV7EöFEöæE÷FövvÆUöVF–õö—FV×2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6·2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢f÷"‡G&6µö–BÂ7F'E÷6×ÆR’–â²‡G&6·5³ÒÂ’Â‡G&6·5³ÒÂC…ó•Ò°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢òö†÷fW"×6VÆV7F–öâÖVF–ò×FW7B"çFõö÷væVB‚’À¢7F'E÷6×ÆRÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢#EóÀ¢Ò¢çVçw&‚“°¢Ğ¢ÆWB&Wf–÷W6Ç•÷6VÆV7FVEö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWB†÷fW&VEö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æP¢ç6VÆV7Eö—FV×2‡fV2²‡&Wf–÷W6Ç•÷6VÆV7FVEö–BÂG&6·5³Ò•Ò“°¢çF–ÖVÆ–æRç6VÆV7E÷G&6µööæÇ’‡G&6·5³Ò“°¢çF–ÖVÆ–æRæ†÷fW&VEö—FVÒÒ6öÖR††÷fW&VEö–B“° ¢ÆWB&Wf—6–öåö&Vf÷&RÒç&Wf—6–öã°¢ÆWB6å÷VæFõö&Vf÷&RÒç&ö¦V7Bæ6å÷VæFò‚“°¢ÆWB6å÷&VFõö&Vf÷&RÒç&ö¦V7Bæ6å÷&VFò‚“° ¢ÆWB6VÆV7Eö6öÖÖæBÒ6öÖÖæD–C£¥6VÆV7D—FVÕVæFW$Ö÷W6T7W'6÷#°¢æ7F–öåöÆ—7@¢ç6WE÷VW'’‚'6VÆV7B—FVÒVæFW"Ö÷W6R7W'6÷""çFõö÷væVB‚’“°¢ÆWB6VÆV7EöVçG'’Ò ¢æ7F–öåöÆ—7EöVçG&–W2‚¢æ–çFõö—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒ6VÆV7Eö6öÖÖæB¢æW‡V7B‚&VF–ò6VÆV7B×VæFW"ÖÖ÷W6R7F–öâ—26V&6†&ÆR"“°¢76W'B‡6VÆV7EöVçG'’æVæ&ÆVB“°¢76W'B‡6VÆV7EöVçG'’ç6†÷'F7WBæ—5öæöæR‚’“°¢æ7F–öåöÆ—7Bç6VÆV7B‚&—FVÒç6VÆV7B×VæFW"ÖÖ÷W6R"çFõö÷væVB‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â†6…6WC£¦g&öÒ…¶†÷fW&VEö–EÒ’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR††÷fW&VEö–B’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Â6öÖR‡G&6·5³Ò’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“° ¢çF–ÖVÆ–æP¢ç6VÆV7Eö—FV×2‡fV2²‡&Wf–÷W6Ç•÷6VÆV7FVEö–BÂG&6·5³Ò•Ò“°¢çF–ÖVÆ–æRç6VÆV7E÷G&6µööæÇ’‡G&6·5³Ò“°¢çF–ÖVÆ–æRæ†÷fW&VEö—FVÒÒ6öÖR††÷fW&VEö–B“° ¢f÷"†6öÖÖæBÂ7F&ÆUö–BÂVW'’ÂW‡V7FVE÷6VÆV7FVB’–â°¢€¢6öÖÖæD–C£¤FD—FVÕVæFW$Ö÷W6UFõ6VÆV7F–öâÀ¢&—FVÒæFB×VæFW"ÖÖ÷W6R×Fò×6VÆV7F–öâ"À¢&ÆVf–ær÷F†W"—FV×26VÆV7FVB"À¢†6…6WC£¦g&öÒ…·&Wf–÷W6Ç•÷6VÆV7FVEö–BÂ†÷fW&VEö–EÒ’À¢’À¢€¢6öÖÖæD–C£¥FövvÆU6VÆV7F–öäöd—FVÕVæFW$Ö÷W6RÀ¢&—FVÒçFövvÆR×VæFW"ÖÖ÷W6R×6VÆV7F–öâ"À¢'FövvÆR6VÆV7F–öâöb—FVÒVæFW"Ö÷W6R7W'6÷""À¢†6…6WC£¦g&öÒ…·&Wf–÷W6Ç•÷6VÆV7FVEö–EÒ’À¢’À¢Ò°¢æ7F–öåöÆ—7Bç6WE÷VW'’‡VW'’çFõö÷væVB‚’“°¢ÆWBVçG'’Ò ¢æ7F–öåöÆ—7EöVçG&–W2‚¢æ–çFõö—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒ6öÖÖæB¢æW‡V7B‚&VF–ò†÷fW"×6VÆV7F–öâ7F–öâ—26V&6†&ÆR"“°¢76W'B†VçG'’æVæ&ÆVB“°¢76W'B†VçG'’ç6†÷'F7WBæ—5öæöæR‚’“°¢æ7F–öåöÆ—7Bç6VÆV7B‡7F&ÆUö–BçFõö÷væVB‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2ÂW‡V7FVE÷6VÆV7FVB“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Â6öÖR‡G&6·5³Ò’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVE÷G&6·2Â†6…6WC£¦g&öÒ…·G&6·5³ÕÒ’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷VæFò‚’Â6å÷VæFõö&Vf÷&R“°¢76W'EöW†ç&ö¦V7Bæ6å÷&VFò‚’Â6å÷&VFõö&Vf÷&R“°¢Ğ§Ğ ¢5·FW7EĞ¦fâFövvÆ–æuööfe÷&–Ö'•ö—FVÕö6†ö÷6W5öf—'7E÷&VÖ–æ–æuö—FVÕö–å÷&ö¦V7Eö÷&FW"‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6·2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢f÷"‡G&6µö–BÂ7F'E÷F–6²’–â²‡G&6·5³ÒÂ#C’Â‡G&6·5³ÒÂs#’Â‡G&6·5³ÒÂó#•Ò°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6²À¢ÆVæwF…÷F–6·3¢CƒÀ¢Ò¢çVçw&‚“°¢Ğ¢ÆWB—FV×2Ò ¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æÖ‡Æ—FV×Â†—FVÒæ–B‚’Â—FVÒçG&6µö–B‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ÆWBf—'7Eö–BÒ—FV×5³Òã°¢ÆWB6V6öæEö–BÒ—FV×5³Òã°¢ÆWBF†—&Eö–BÒ—FV×5³%Òã°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æP¢ç6VÆV7Eö—FV×2‡fV2¶—FV×5³ÒÂ—FV×5³%ÒÂ—FV×5³ÕÒ“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR†f—'7Eö–B’“°¢çF–ÖVÆ–æRæ†÷fW&VEö—FVÒÒ6öÖR†f—'7Eö–B“°¢æ7F–öåöÆ—7@¢ç6WE÷VW'’‚'FövvÆR6VÆV7F–öâöb—FVÒVæFW"Ö÷W6R7W'6÷""çFõö÷væVB‚’“°¢æ7F–öåöÆ—7@¢ç6VÆV7B‚&—FVÒçFövvÆR×VæFW"ÖÖ÷W6R×6VÆV7F–öâ"çFõö÷væVB‚’“° ¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“° ¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2À¢†6…6WC£¦g&öÒ…·6V6öæEö–BÂF†—&Eö–EÒ¢“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÂ6öÖR‡6V6öæEö–B’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7F–öåöæ6†÷"‚’Â6öÖR†f—'7Eö–B’“°§Ğ ¢5·FW7EĞ¦fâGWÆ–6FU÷6VÆV7FVEöVF–õöw&÷W÷&W6W'fW5÷6×ÆU÷76–ær‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢f÷"†ÖVF–÷&VbÂ7F'E÷6×ÆRÂÆVæwF…÷6×ÆW2’–à¢²‚&76WC¢òöf—'7B"Â#CÂ’Â‚&76WC¢ò÷6V6öæB"Â3Â#•Ğ¢°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢ÖVF–÷&VbçFõö÷væVB‚’À¢7F'E÷6×ÆRÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW2À¢Ò¢çVçw&‚“°¢Ğ¢ÆWB6VÆV7FVBÒ ¢ç&ö¦V7@¢æVF–õö—FV×2‚¢æ—FW"‚¢æÖ‡Æ—FV×Â—FVÒæ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÒ6VÆV7FVBæf—'7B‚’æ6÷–VB‚“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æW‡FVæB‡6VÆV7FVB“° ¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¤GWÆ–6FU6VÆV7FVD—FVÒ’“° ¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’ÂB“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³%Òç7F'E÷6×ÆR‚’Â3#“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³5Òç7F'E÷6×ÆR‚’Â3s’“°§Ğ ¢5·FW7EĞ¦fâVF–õ÷F–ÖVÆ–æUöFVÆWFUö—5÷VæFö&ÆR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢òöFVÆWFR×FW7B"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢3"À¢6÷W&6Uööfg6WE÷6×ÆW3¢BÀ¢ÆVæwF…÷6×ÆW3¢#SbÀ¢Ò¢æW‡V7B‚'FW7B—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤FVÆWFTVF–ô—FVÒ†—FVÕö–B’“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚’æ—5öV×G’‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢ÆWB—FVÒÒfç&ö¦V7BæVF–õö—FV×2‚•³Ó°¢76W'EöW†—FVÒæ–B‚’Â—FVÕö–B“°¢76W'EöW†—FVÒæÖVF–÷&Vb‚’Â&76WC¢òöFVÆWFR×FW7B"“°¢76W'EöW†—FVÒç7F'E÷6×ÆR‚’Â3"“°¢76W'EöW†—FVÒç6÷W&6Uööfg6WE÷6×ÆW2‚’ÂB“°¢76W'EöW†—FVÒæÆVæwF…÷6×ÆW2‚’Â#Sb“°§Ğ ¢5·FW7EĞ¦fâFVÆWF–æu÷6VÆV7FVEöVF–õöæEöÖ–F•ö—FV×5ö—5ööæU÷VæFö&ÆUöÖVçUö7F–öâ‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6·2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢ÖVF–÷&Vc¢&76WC¢òöÖVçRÖFVÆWFR"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢cBÀ¢ÆVæwF…÷6×ÆW3¢S"À¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢G&6·5³ÒÀ¢7F'E÷F–6³¢“cÀ¢ÆVæwF…÷F–6·3¢5óƒCÀ¢Ò¢çVçw&‚“°¢ÆWBVF–õö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBÖ–F•ö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6VÆV7D—FVÒ°¢—FVÕö–C¢6öÖR†VF–õö–B’À¢FF—F—fS¢fÇ6RÀ¢&ævS¢fÇ6RÀ¢Ò“°¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6VÆV7D—FVÒ°¢—FVÕö–C¢6öÖR†Ö–F•ö–B’À¢FF—F—fS¢G'VRÀ¢&ævS¢fÇ6RÀ¢Ò“°¢ÆWBòÒçWFFR„ÖW76vS£¥FövvÆTÖ–äÖVçR„Ö–äÖVçS£¤—FVÒ’“° ¢ÆWBòÒçWFFR„ÖW76vS£¤FVÆWFU6VÆV7FVD—FV×2“° ¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚’æ—5öV×G’‚’“°¢76W'B†ç&ö¦V7BæÖ–F•ö—FV×2‚’æ—5öV×G’‚’“°¢76W'EöW†æ7F—fUöÖVçRÂæöæR“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚’ÂVF–õö–B“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òç6÷W&6Uööfg6WE÷6×ÆW2‚’ÂcB“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚’ÂÖ–F•ö–B“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒçG&6µö–B‚’ÂG&6·5³Ò“°§Ğ ¢5·FW7EĞ¦fâVF–õ÷F–ÖVÆ–æUöçVFvUö—5÷VæFö&ÆUöæEö6ææ÷Eö7&÷75÷6×ÆU÷¦W&ò‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢òöçVFvR×FW7B"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢#SbÀ¢Ò¢æW‡V7B‚'FW7B—FVÒ6†÷VÆB&R–ç6W'FVB"“°¢ÆWB—FVÕö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¤çVFvTVF–ô—FVÒ†—FVÕö–BÂÂó’“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òç7F'E÷6×ÆR‚’ÂC…ó“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òç7F'E÷6×ÆR‚’Â“°¢ÆWBòÒçWFFR„ÖW76vS£¤çVFvTVF–ô—FVÒ†—FVÕö–BÂÂ’“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òç7F'E÷6×ÆR‚’ÂCƒ“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òç7F'E÷6×ÆR‚’Â“°¢ÆWBòÒçWFFR„ÖW76vS£¤çVFvTVF–ô—FVÒ†—FVÕö–BÂÓÂ’“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òç7F'E÷6×ÆR‚’Â“°§Ğ ¢5·FW7EĞ¦fâ6ö×ÆWFVEöVF–õö–×÷'E÷Æ6W5ö—FVÕ÷F‡&÷Vv…÷&ö¦V7Eö7F–öâ‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢æ–×÷'Eö'W7’ÒG'VS° ¢ÆWBòÒçWFFR„ÖW76vS£¤VF–ô–×÷'Df–æ—6†VB„ö²€¢Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢ò÷FW7BÖVF–ò"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢#‚À¢ÒÀ¢’’“° ¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’Â“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òç7F'E÷6×ÆR‚’Â“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒæÆVæwF…÷6×ÆW2‚’Â#‚“°¢76W'EöW†ç&Wf—6–öâÂ"“°¢76W'B‚æ–×÷'Eö'W7’“°§Ğ ¢5·FW7EĞ¦fâVç6fVE÷6W76–öåö6å÷7F'EöVF–õö–×÷'Eö–çFõö—G5÷FV×÷&'•÷7F÷&R‚’°¢ÆWB&ö÷BÒFV×f–ÆS£§FV×F—"‚’çVçw&‚“°¢ÆWB6W76–öâÒ7WW#£¦7&VFU÷Vç6fVE÷&ö¦V7E÷6W76–öåöF—%ö–â‡&ö÷BçF‚‚’’çVçw&‚“°¢ÆWB7F÷&U÷F‚Ò6W76–öâç7F÷&U÷F‚‚“°¢ÆWB6÷W&6U÷F‚Ğ¢F„'Vc£¦g&öÒ†Vçb‚$4$tõôÔä”dU5EôD•""’’æ¦ö–â‚"ââòââöW†×ÆW2öf—'7B×&ö¦V7BöÖVÆöG’çvb"“°¢ÆWB×WBÒ°¢6W76–öåöÖVF–öF—#¢6öÖR‡6W76–öâ’À¢VF–õöf–ÆU÷F…÷VW'“¢6÷W&6U÷F‚çFõ÷7G&–æuöÆ÷77’‚’æ–çFõö÷væVB‚’À¢âä£¦FVfVÇB‚¢Ó°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“° ¢ÆWB÷F6²Òç7F'EöVF–õö–×÷'B‚“° ¢76W'B†ç&ö¦V7E÷F‚æ—5öæöæR‚’“°¢76W'EöW†æÖVF–÷7F÷&U÷F‚‚’Â6öÖR‡7F÷&U÷F‚æ6ÆöæR‚’’“°¢76W'B‡7F÷&U÷F‚æ—5öf–ÆR‚’“°¢76W'B†æ–×÷'Eö'W7’“°¢76W'B†ç7FGW2ç7F'G5÷v—F‚‚%7F'F–ær–×÷'Böb"’“°§Ğ ¢5·FW7EĞ¦fâVç6fVE÷6W76–öåö6å÷VWVU÷&ö¦V7E÷&VæFW&–ær‚’°¢ÆWB&ö÷BÒFV×f–ÆS£§FV×F—"‚’çVçw&‚“°¢ÆWB6W76–öâÒ7WW#£¦7&VFU÷Vç6fVE÷&ö¦V7E÷6W76–öåöF—%ö–â‡&ö÷BçF‚‚’’çVçw&‚“°¢ÆWB7F÷&U÷F‚Ò6W76–öâç7F÷&U÷F‚‚“°¢ÆWBFW7F–æF–öâÒ&ö÷BçF‚‚’æ¦ö–â‚'&VæFW"çvb"“°¢ÆWB×WBÒ°¢6W76–öåöÖVF–öF—#¢6öÖR‡6W76–öâ’À¢âä£¦FVfVÇB‚¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢%&VæFW"6öçFVçB"çFõö÷væVB‚’À¢Ò¢æW‡V7B‚'G&6²6†÷VÆB&R7&VFVBf÷"&VæFW&–ær"“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢æW‡V7B‚'&VæFW"6†÷VÆB†fRæöâÖV×G’&ö¦V7B&ævR"“° ¢ÆWB÷F6²Òç7F'EööffÆ–æU÷&VæFW"†FW7F–æF–öâ“° ¢76W'B†ç&ö¦V7E÷F‚æ—5öæöæR‚’“°¢76W'EöW†æÖVF–÷7F÷&U÷F‚‚’Â6öÖR‡7F÷&U÷F‚’“°¢76W'B†æöffÆ–æU÷&VæFW%ö'W7’“°¢76W'B‚æ–õö'W7’“°¢76W'EöW†ç7FGW2Â%&VæFW"&VæFW"çvn(
b"“°§Ğ ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ğ¢5·FW7EĞ¦fâVç6fVE÷6W76–öåö6å÷7F'E÷&V6÷&F–æu÷&W&F–öâ‚’°¢ÆWB&ö÷BÒFV×f–ÆS£§FV×F—"‚’çVçw&‚“°¢ÆWB6W76–öâÒ7WW#£¦7&VFU÷Vç6fVE÷&ö¦V7E÷6W76–öåöF—%ö–â‡&ö÷BçF‚‚’’çVçw&‚“°¢ÆWB7F÷&U÷F‚Ò6W76–öâç7F÷&U÷F‚‚“°¢ÆWB×WBÒ°¢6W76–öåöÖVF–öF—#¢6öÖR‡6W76–öâ’À¢âä£¦FVfVÇB‚¢Ó°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥FövvÆU&V6÷&D&Ò‡G&6µö–B’“° ¢ÆWB÷F6²ÒçWFFR„ÖW76vS£¥7F'E&V6÷&F–ær“° ¢76W'B†ç&ö¦V7E÷F‚æ—5öæöæR‚’“°¢76W'EöW†æÖVF–÷7F÷&U÷F‚‚’Â6öÖR‡7F÷&U÷F‚’“°¢76W'B†ç&V6÷&F–æu÷7F'F–ær“°¢76W'B†çÆ–&6µö'W7’“°§Ğ ¢5·FW7EĞ¦fâVF–õö–×÷'E÷F&vWG5÷F†U÷6VÆV7FVE÷G&6µöæEöVF—Eö7W'6÷%÷6×ÆR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBf—'7E÷G&6²Òç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWB6VÆV7FVE÷G&6²Òç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Ò6öÖR‡6VÆV7FVE÷G&6²“°¢çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²ÒóCC°¢ÆWBW‡V7FVE÷6×ÆRÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒóCC’çVçw&‚“° ¢ÆWB6GW&VBÒæVF–õö–×÷'E÷Æ6VÖVçB‚’çVçw&‚“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6²Ò6öÖR†f—'7E÷G&6²“°¢çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²Ò° ¢76W'EöW†6GW&VBÂ‡6VÆV7FVE÷G&6²ÂW‡V7FVE÷6×ÆR’“°§Ğ ¢5·FW7EĞ¦fâVF–õö–×÷'EöfÆÇ5ö&6µ÷Fõöf—'7EöVF–õ÷G&6µöæE÷&V¦V7G5ö'W5ööæÇ•÷&ö¦V7G2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FD'W5G&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBf—'7EöVF–õ÷G&6²Òç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç6VÆV7FVE÷G&6²ÒæöæS°¢76W'EöW€¢æVF–õö–×÷'E÷Æ6VÖVçB‚’çVçw&‚’À¢†f—'7EöVF–õ÷G&6²Â¢“° ¢ÆWB×WB'W5ööæÇ’Ò£¦FVfVÇB‚“°¢ÆWBòÒ'W5ööæÇ’çWFFR„ÖW76vS£¤FD'W5G&6²“°¢76W'EöW€¢'W5ööæÇ’æVF–õö–×÷'E÷Æ6VÖVçB‚’çVçw&öW'"‚’À¢%6VÆV7BâVF–òG&6²&Vf÷&R–×÷'F–ærVF–ò ¢“°¢'W5ööæÇ’çF–ÖVÆ–æRç6VÆV7FVE÷G&6²ÒæöæS°¢76W'EöW€¢'W5ööæÇ’æVF–õö–×÷'E÷Æ6VÖVçB‚’çVçw&öW'"‚’À¢$FBâVF–òG&6²&Vf÷&R–×÷'F–ærVF–ò ¢“°¢76W'EöW€¢£¦FVfVÇB‚’æVF–õö–×÷'E÷Æ6VÖVçB‚’çVçw&öW'"‚’À¢$FBâVF–òG&6²&Vf÷&R–×÷'F–ærVF–ò ¢“°§Ğ ¢5·FW7EĞ¦fâ&V6÷&FVE÷F¶U÷Æ6W5ööæU÷6†&VEö76WEööåöÆÅö6GW&VEö&ÖVE÷G&6·5ö–åööæU÷VæFõ÷7FW‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBf—'7E÷G&6²Òç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWB6V6öæE÷G&6²Òç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥FövvÆU&V6÷&D&Ò†f—'7E÷G&6²’“°¢ÆWBòÒçWFFR„ÖW76vS£¥FövvÆU&V6÷&D&Ò‡6V6öæE÷G&6²’“°¢ÆWB6÷W&6RÒ7FC£¦Vçc£§FV×öF—"‚’æ¦ö–â†f÷&ÖB€¢&Fr×&V6÷&FVB×F¶R×FW7B×·Ò×·Òçvb"À¢7FC£§&ö6W73£¦–B‚’À¢äU…EõDU5Eôd”ÄRæfWF6…öFBƒÂ÷&FW&–æs£¥&VÆ†VB¢’“°¢ÆWB&ö¦V7E÷F‚Ò6÷W&6Rçv—F…öW‡FVç6–öâ‚&Fr"“°¢ç&ö¦V7E÷F‚Ò6öÖR‡&ö¦V7E÷F‚æ6ÆöæR‚’“°¢ÆWB&V6÷&F–æu÷7F'E÷6×ÆRÒ7WW#£¦VF–õö6öæf–s£¦Ç•÷&V6÷&F–æuööfg6WB€¢“eóÀ¢ç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚’À¢SÀ¢¢æW‡V7B‚'÷6—F—fR6Æ–'&F–öâöfg6WB6†÷VÆBf—BF†R&ö¦V7BF–ÖVÆ–æR"“°¢ç&V6÷&Eö–×÷'E÷G&6·2Ò6öÖR‡7WW#£¥&V6÷&D–×÷'EF&vWB°¢G&6µö–G3¢fV2¶f—'7E÷G&6²Â6V6öæE÷G&6µÒÀ¢&V7&VFVE÷G&6µö–G3¢fV3£¦æWr‚’À¢6÷W&6U÷F‡3¢fV2·6÷W&6Ræ6ÆöæR‚•ÒÀ¢æW‡E÷6VvÖVçEö–æFWƒ¢À¢æW‡E÷7F'E÷6×ÆS¢&V6÷&F–æu÷7F'E÷6×ÆRÀ¢–×÷'FVEö7F–öç3¢fV3£¦æWr‚’À¢&ö¦V7E÷F‚À¢6×ÆU÷&FS¢ç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚’À¢&V6÷fW'•öÖæ–fW7E÷Fƒ¢6÷W&6Rçv—F…öW‡FVç6–öâ‚'&V6÷fW'’æ§6öâ"’À¢&ö¦V7EövVæW&F–öã¢ç&ö¦V7EövVæW&F–öâÀ¢&V6÷fW'•öF—66&FVEög&ÖW3¢À¢&V6÷fW'•öF—66&FVE÷F–Åö'—FW3¢À¢&V6÷fW'•÷7F'E÷6×ÆUö—5öW7F–ÖFS¢fÇ6RÀ¢Ò“°¢æ–×÷'Eö'W7’ÒG'VS° ¢ÆWBòÒæf–æ—6…öVF–õö–×÷'B„ö²„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢f—'7E÷G&6²À¢ÖVF–÷&Vc¢&76WC¢ò÷&V6÷&FVB×F¶R"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢&V6÷&F–æu÷7F'E÷6×ÆRÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò’“° ¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’Â"“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚’æ—FW"‚’æÆÂ‡Æ—FV×Â°¢—FVÒç7F'E÷6×ÆR‚’ÓÒ&V6÷&F–æu÷7F'E÷6×ÆRbb—FVÒæÖVF–÷&Vb‚’ÓÒ&76WC¢ò÷&V6÷&FVB×F¶R ¢Ò’“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚’æ—5öV×G’‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&ö¦V7E6fVB€¢ç&ö¦V7E÷F‚æ6ÆöæR‚’æW‡V7B‚'&ö¦V7BF‚—26WB"’À¢ç&Wf—6–öâÀ¢ö²‚‚’’À¢æöæRÀ¢6†&VE&ö¦V7E6W76–öäÆö6³£¦æWr„æöæR’À¢’“°¢76W'EöW†çVæF–æu÷&V6÷&F–æuö6ÆVçWæÆVâ‚’Â“°¢ÆWB–×÷'FVE÷&Wf—6–öâÒçVæF–æu÷&V6÷&F–æuö6ÆVçW³Òç6fVE÷&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’Â"“°¢76W'B†ç&V6÷&Eö–×÷'E÷G&6·2æ—5öæöæR‚’“°¢76W'EöW†çVæF–æu÷&V6÷&F–æuö6ÆVçWæÆVâ‚’Â“°¢76W'EöW€¢çVæF–æu÷&V6÷&F–æuö6ÆVçW³Òç6fVE÷&Wf—6–öâÀ¢–×÷'FVE÷&Wf—6–öà¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥&ö¦V7E6fVB€¢ç&ö¦V7E÷F‚æ6ÆöæR‚’æW‡V7B‚'&ö¦V7BF‚—26WB"’À¢ç&Wf—6–öâÀ¢ö²‚‚’’À¢æöæRÀ¢6†&VE&ö¦V7E6W76–öäÆö6³£¦æWr„æöæR’À¢’“°¢ÆWBÖæ–fW7E÷F‚ÒçVæF–æu÷&V6÷&F–æuö6ÆVçW³ÒæÖæ–fW7E÷F‚æ6ÆöæR‚“°¢76W'EöW€¢çVæF–æu÷&V6÷&F–æuö6ÆVçW³ÒæÖVF–÷&Vg2À¢²&76WC¢ò÷&V6÷&FVB×F¶R%Ğ¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥&V6÷&F–æu&V6÷fW'”6ÆVæVB„ö²‡fV2¶Öæ–fW7E÷F…Ò’’“°¢76W'B†çVæF–æu÷&V6÷&F–æuö6ÆVçWæ—5öV×G’‚’“°§Ğ ¢5·FW7EĞ¦fâ–çfÆ–E÷&V6÷&F–æuööfg6WEöVF—Eö¶VW5÷F†UöÆ7EöÆ–VE÷fÇVR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢æVF–õ÷6WGF–æw2ç&V6÷&F–æuööfg6WE÷W2ÒÓ#S°¢æVF–õ÷&V6÷&F–æuööfg6WE÷VW'’Ò6öÖR‚#Sã"çFõö÷væVB‚’“° ¢ÆWBòÒçWFFR„ÖW76vS£¤Ç•&V6÷&F–ætöfg6WB“° ¢76W'EöW†æVF–õ÷6WGF–æw2ç&V6÷&F–æuööfg6WE÷W2ÂÓ#S“°¢76W'B€¢æVF–õ÷6WGF–æw5öfVVF&6°¢æ6öçF–ç2‚'WFò2FV6–ÖÂÆ6W2"¢“°§Ğ ¢5·FW7EĞ¦fâ&V6÷fW&VE÷&V6÷&F–æuö¶VW5÷F†Uö6Æ–'&FVEöæ6†÷%öf÷%ö–×÷'B‚’°¢ÆWBF—&V7F÷'’ÒFV×f–ÆS£§FV×F—"‚’æW‡V7B‚'FW7BF—&V7F÷'’6†÷VÆB&R7&VFVB"“°¢ÆWB&ö¦V7E÷F‚ÒF—&V7F÷'’çF‚‚’æ¦ö–â‚'&V6÷&F–æræFr"“°¢ÆWB×WB&ö¦V7BÒ&ö¦V7C£¦æWr‚“°¢&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢$6Æ–'&FVBF¶R"çFõö÷væVB‚’À¢Ò¢æW‡V7B‚'G&6²6†÷VÆB&R7&VFVB"“°¢&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢%6V6öæB&ÖVBG&6²"çFõö÷væVB‚’À¢Ò¢æW‡V7B‚'6V6öæBG&6²6†÷VÆB&R7&VFVB"“°¢6fU÷&ö¦V7Eöf–ÆR‡&ö¦V7E÷F‚æ6ÆöæR‚’Â&ö¦V7Bç6æ6†÷B‚’ÂfÇ6R¢æW‡V7B‚'&ö¦V7B6†÷VÆB&R6fVB&Vf÷&R&V6÷&F–ær"“°¢ÆWBG&6µö–BÒ&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWB6V6öæE÷G&6µö–BÒ&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWB6×ÆU÷&FRÒ&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚“°¢ÆWB&÷f—6–öæÅ÷7F'BÒ7WW#£¦VF–õö6öæf–s£¦Ç•÷&V6÷&F–æuööfg6WBƒ“eóÂ6×ÆU÷&FRÂS¢æW‡V7B‚&6Æ–'&FVB&÷f—6–öæÂ÷6—F–öâ6†÷VÆBf—BF†R&ö¦V7BF–ÖVÆ–æR"“°¢ÆWB6GW&U÷G&ç7÷'E÷6×ÆRĞ¢7WW#£¦VF–õö6öæf–s£¦Ç•÷&V6÷&F–æuööfg6WBƒ“eóS"Â6×ÆU÷&FRÂS¢æW‡V7B‚&6Æ–'&FVB6GW&R÷6—F–öâ6†÷VÆBf—BF†R&ö¦V7BF–ÖVÆ–æR"“°¢ÆWB6GW&Uöæ6†÷"ÒFuö£¤6GW&UF–ÖVÆ–æTæ6†÷#£¦æWr€¢óÀ¢6GW&U÷G&ç7÷'E÷6×ÆRÀ¢6×ÆU÷&FRÀ¢6×ÆU÷&FRÀ¢¢æW‡V7B‚&6GW&R6Æö6²6†÷VÆBÖFòF†R&ö¦V7B6×ÆR&FR"“°¢ÆWB6GW&U÷7F'BÒ6GW&Uöæ6†÷ ¢ç&ö¦V7E÷6×ÆUöBƒó#B¢æW‡V7B‚&f—'7B6ÆÆ&6²6†÷VÆBÖFò&ö¦V7B6×ÆR"“° ¢ÆWB†×WB&öGV6W"Â6öç7VÖW"Â6öçG&öÂ’ÒFuö£¦VF–õö6GW&U÷7G&VÒƒb“°¢ÆWBv÷&¶W"ÒFuö£¤VF–õ&V6÷&F–æuv÷&¶W#£§7F'E÷&V6÷fW&&ÆR€¢g&ö¦V7E÷F‚À¢6×ÆU÷&FRÀ¢fV2·G&6µö–BçfÇVR‚’Â6V6öæE÷G&6µö–BçfÇVR‚•ÒÀ¢6öç7VÖW"À¢6öçG&öÂæ6ÆöæR‚’À¢¢æW‡V7B‚'&V6÷fW&&ÆR&V6÷&F–ær6†÷VÆB7F'B"“°¢ÆWBÖæ–fW7E÷F‚Òv÷&¶W ¢ç&V6÷fW'•öÖæ–fW7E÷F‚‚¢æW‡V7B‚'&V6÷&F–ær6†÷VÆB7&VFR&V6÷fW'’ÖWFFF"¢çFõ÷F…ö'Vb‚“°¢v÷&¶W ¢ç6WE÷7F'E÷6×ÆR‡&÷f—6–öæÅ÷7F'B¢æW‡V7B‚&6÷'&V7FVB&÷f—6–öæÂæ6†÷"6†÷VÆB&RGW&&ÆR&Vf÷&R6GW&R"“°¢v÷&¶W ¢ç6WEö6GW&U÷F–ÖVÆ–æUöæ6†÷"†6GW&Uöæ6†÷"¢æW‡V7B‚&6GW&R6Æö6²æ6†÷"6†÷VÆB&RVWVVB&Vf÷&R6GW&R7F'G2"“°¢6öçG&öÂç7F'B‚“°¢&öGV6W"çW6…÷Ææ%öBƒó#BÂe³ã#RÂãUÒÂe²Óã#RÂÓãUÒ“°¢6öçG&öÂæf–Â‚“°¢76W'B†ÖF6†W2€¢v÷&¶W"æf–æ—6‚‚’À¢W'"†Fuö£¤VF–õ&V6÷&F–ætW'&÷#£¤6GW&Tf–ÆVB²ââÒ¢’“° ¢ÆWB6æF–FFRÒFuö£§66å÷&V6÷&F–æu÷&V6÷fW&–W2‚g&ö¦V7E÷F‚¢æW‡V7B‚&–çFW''WFVBF¶R6†÷VÆB&RF—66÷fW&&ÆR"¢ç÷‚¢æW‡V7B‚&–çFW''WFVBF¶R6†÷VÆB†fR&V6÷fW'’6æF–FFR"“°¢ÆWB6æF–FFRÒFuö£§&V6÷fW%÷&V6÷&F–æuö6æF–FFR†6æF–FFR¢æW‡V7B‚&–çFW''WFVBVF–ò6†÷VÆB&R&W&VBf÷"–×÷'B"“°¢76W'EöW†6æF–FFRæÖæ–fW7Bç7F'E÷6×ÆRÂ6öÖR†6GW&U÷7F'B’“°¢76W'B‚6æF–FFRæÖæ–fW7Bç7F'E÷6×ÆUö—5öW7F–ÖFR“° ¢ÆWB×WBÒ°¢&ö¦V7BÀ¢&ö¦V7E÷Fƒ¢6öÖR‡&ö¦V7E÷F‚æ6ÆöæR‚’’À¢âä£¦FVfVÇB‚¢Ó°¢ÆWB÷F6²Òç&V6÷&F–æu÷&V6÷fW'•÷&W&VB„ö²†6æF–FFR’“°¢76W'EöW€¢ç&V6÷&Eö–×÷'E÷G&6·0¢æ5÷&Vb‚¢æW‡V7B‚'&V6÷fW'’6†÷VÆB7F'B–×÷'F–ærF†RF¶R"¢ææW‡E÷7F'E÷6×ÆRÀ¢6GW&U÷7F'@¢“°¢ÆWB÷F6²Òæf–æ—6…öVF–õö–×÷'B„ö²„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢ò÷&V6÷fW&VBÖ6Æ–'&FVB×F¶R"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò’“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’Â"“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚’æ—FW"‚’æÆÂ‡Æ—FV×Â°¢—FVÒç7F'E÷6×ÆR‚’ÓÒ6GW&U÷7F'@¢bb—FVÒæÖVF–÷&Vb‚’ÓÒ&76WC¢ò÷&V6÷fW&VBÖ6Æ–'&FVB×F¶R ¢Ò’“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚’æ—5öV×G’‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’Â"“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚’æ—FW"‚’æÆÂ‡Æ—FV×Â°¢—FVÒç7F'E÷6×ÆR‚’ÓÒ6GW&U÷7F'@¢bb—FVÒæÖVF–÷&Vb‚’ÓÒ&76WC¢ò÷&V6÷fW&VBÖ6Æ–'&FVB×F¶R ¢Ò’“° ¢Fuö£¦F—66&E÷&V6÷&F–æu÷&V6÷fW'’‚fÖæ–fW7E÷F‚¢æW‡V7B‚'FW7B&V6÷fW'’6÷W&6W26†÷VÆB&R6ÆVæVBW"“°§Ğ ¢5·FW7EĞ¦fâ6VvÖVçFVE÷&V6÷&F–æuö–×÷'E÷Æ6W5ö6öçF–wV÷W5÷6VvÖVçG5ö–åööæU÷VæFõ÷7FW‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBf—'7E÷G&6²Òç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWB6V6öæE÷G&6²Òç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWB6÷W&6U÷F‡2ÒfV2°¢7FC£¦Vçc£§FV×öF—"‚’æ¦ö–â‚&Fr×6VvÖVçBÖöæRçvb"’À¢7FC£¦Vçc£§FV×öF—"‚’æ¦ö–â‚&Fr×6VvÖVçB×Gvòçvb"’À¢Ó°¢ç&V6÷&Eö–×÷'E÷G&6·2Ò6öÖR‡7WW#£¥&V6÷&D–×÷'EF&vWB°¢G&6µö–G3¢fV2¶f—'7E÷G&6²Â6V6öæE÷G&6µÒÀ¢&V7&VFVE÷G&6µö–G3¢fV3£¦æWr‚’À¢6÷W&6U÷F‡2À¢æW‡E÷6VvÖVçEö–æFWƒ¢À¢æW‡E÷7F'E÷6×ÆS¢EóƒÀ¢–×÷'FVEö7F–öç3¢fV3£¦æWr‚’À¢&ö¦V7E÷Fƒ¢7FC£¦Vçc£§FV×öF—"‚’æ¦ö–â‚&Fr×6VvÖVçFVB×FW7BæFr"’À¢6×ÆU÷&FS¢ç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚’À¢&V6÷fW'•öÖæ–fW7E÷Fƒ¢7FC£¦Vçc£§FV×öF—"‚’æ¦ö–â‚&Fr×6VvÖVçFVB×FW7Bç&V6÷fW'’æ§6öâ"’À¢&ö¦V7EövVæW&F–öã¢ç&ö¦V7EövVæW&F–öâÀ¢&V6÷fW'•öF—66&FVEög&ÖW3¢À¢&V6÷fW'•öF—66&FVE÷F–Åö'—FW3¢À¢&V6÷fW'•÷7F'E÷6×ÆUö—5öW7F–ÖFS¢G'VRÀ¢Ò“°¢æ–×÷'Eö'W7’ÒG'VS° ¢ÆWBòÒæf–æ—6…öVF–õö–×÷'B„ö²„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢f—'7E÷G&6²À¢ÖVF–÷&Vc¢&76WC¢ò÷6VvÖVçBÖöæR"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢EóƒÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢%óCÀ¢Ò’“°¢ÆWBF&vWBÒ ¢ç&V6÷&Eö–×÷'E÷G&6·0¢æ5÷&Vb‚¢æW‡V7B‚&æW‡B6VvÖVçB—2VæF–ær"“°¢76W'EöW‡F&vWBææW‡E÷6VvÖVçEö–æFW‚Â“°¢76W'EöW‡F&vWBææW‡E÷7F'E÷6×ÆRÂuó#“° ¢ÆWBòÒæf–æ—6…öVF–õö–×÷'B„ö²„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢f—'7E÷G&6²À¢ÖVF–÷&Vc¢&76WC¢ò÷6VvÖVçB×Gvò"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢uó#À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢ó#À¢Ò’“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’ÂB“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚’æ—FW"‚’æç’‡Æ—FV×Â°¢—FVÒçG&6µö–B‚’ÓÒf—'7E÷G&6°¢bb—FVÒç7F'E÷6×ÆR‚’ÓÒEóƒ ¢bb—FVÒæÖVF–÷&Vb‚’ÓÒ&76WC¢ò÷6VvÖVçBÖöæR ¢Ò’“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚’æ—FW"‚’æç’‡Æ—FV×Â°¢—FVÒçG&6µö–B‚’ÓÒf—'7E÷G&6°¢bb—FVÒç7F'E÷6×ÆR‚’ÓÒuó# ¢bb—FVÒæÖVF–÷&Vb‚’ÓÒ&76WC¢ò÷6VvÖVçB×Gvò ¢Ò’“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚’æ—FW"‚’æç’‡Æ—FV×Â°¢—FVÒçG&6µö–B‚’ÓÒ6V6öæE÷G&6°¢bb—FVÒç7F'E÷6×ÆR‚’ÓÒEóƒ ¢bb—FVÒæÖVF–÷&Vb‚’ÓÒ&76WC¢ò÷6VvÖVçBÖöæR ¢Ò’“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚’æ—FW"‚’æç’‡Æ—FV×Â°¢—FVÒçG&6µö–B‚’ÓÒ6V6öæE÷G&6°¢bb—FVÒç7F'E÷6×ÆR‚’ÓÒuó# ¢bb—FVÒæÖVF–÷&Vb‚’ÓÒ&76WC¢ò÷6VvÖVçB×Gvò ¢Ò’“°¢76W'B†ç7FGW2æ6öçF–ç2‚'7F'B÷6—F–öâ—2&÷†–ÖFR"’“°¢76W'B†ç&V6÷&Eö–×÷'E÷G&6·2æ—5öæöæR‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚’æ—5öV×G’‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’ÂB“°§Ğ ¢5·FW7EĞ¦fâf–ÆVE÷&V6÷&F–æuö–×÷'Eö¶VW5÷&V6÷fW'•÷6÷W&6W5öf÷%÷&WG'’‚’°¢ÆWBf–ÆUö–BÒäU…EõDU5Eôd”ÄRæfWF6…öFBƒÂ÷&FW&–æs£¥&VÆ†VB“°¢ÆWB&ö¦V7E÷F‚Ò7FC£¦Vçc£§FV×öF—"‚’æ¦ö–â†f÷&ÖB€¢&Fr×&V6÷&F–ær×&WG'’×·Ò×¶f–ÆUö–GÒæFr"À¢7FC£§&ö6W73£¦–B‚¢’“°¢ÆWBÖæ–fW7E÷F‚Ğ¢&ö¦V7E÷F‚çv—F…öf–ÆUöæÖR†f÷&ÖB‚"æFr×&V6÷&F–ær×&WG'’×¶f–ÆUö–GÒç&V6÷fW'’æ§6öâ"’“°¢ÆWB6÷W&6U÷F‚Ò&ö¦V7E÷F‚çv—F…öf–ÆUöæÖR†f÷&ÖB‚"æFr×&V6÷&F–ær×&WG'’×¶f–ÆUö–GÒçvb"’“°¢ÆWB×WB&ö¦V7BÒ&ö¦V7C£¦æWr‚“°¢&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢%&V6÷&FVB"çFõö÷væVB‚’À¢Ò¢æW‡V7B‚'G&6²6†÷VÆB&R7&VFVB"“°¢6fU÷&ö¦V7Eöf–ÆR‡&ö¦V7E÷F‚æ6ÆöæR‚’Â&ö¦V7Bç6æ6†÷B‚’ÂfÇ6R¢æW‡V7B‚'&ö¦V7B6†÷VÆB&R6fVB&Vf÷&R&V6÷&F–ær"“°¢7FC£¦g3£§w&—FR‚fÖæ–fW7E÷F‚Â"'&V6÷fW&&ÆRÖWFFF"¢æW‡V7B‚'&V6÷fW'’Öæ–fW7B6†÷VÆB&Rw&—GFVâ"“°¢7FC£¦g3£§w&—FR‚g6÷W&6U÷F‚Â"'&V6÷&FVB6VvÖVçB"’æW‡V7B‚'&V6÷&FVB6÷W&6R6†÷VÆB&Rw&—GFVâ"“° ¢ÆWB×WBÒ°¢&ö¦V7BÀ¢&ö¦V7E÷Fƒ¢6öÖR‡&ö¦V7E÷F‚æ6ÆöæR‚’’À¢âä£¦FVfVÇB‚¢Ó°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢%&V6÷fW&VBG&6²"çFõö÷væVB‚’À¢Ò¢æW‡V7B‚'Vç6fVB&V6÷fW'’G&6²6†÷VÆB&R7&VFVB"“°¢ÆWB&V7&VFVE÷G&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&V6÷&Eö–×÷'E÷G&6·2Ò6öÖR‡7WW#£¥&V6÷&D–×÷'EF&vWB°¢G&6µö–G3¢fV2·&V7&VFVE÷G&6µö–EÒÀ¢&V7&VFVE÷G&6µö–G3¢fV2·&V7&VFVE÷G&6µö–EÒÀ¢6÷W&6U÷F‡3¢fV2·6÷W&6U÷F‚æ6ÆöæR‚•ÒÀ¢æW‡E÷6VvÖVçEö–æFWƒ¢À¢æW‡E÷7F'E÷6×ÆS¢À¢–×÷'FVEö7F–öç3¢fV3£¦æWr‚’À¢&ö¦V7E÷Fƒ¢&ö¦V7E÷F‚æ6ÆöæR‚’À¢6×ÆU÷&FS¢ç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚’À¢&V6÷fW'•öÖæ–fW7E÷Fƒ¢Öæ–fW7E÷F‚æ6ÆöæR‚’À¢&ö¦V7EövVæW&F–öã¢ç&ö¦V7EövVæW&F–öâÀ¢&V6÷fW'•öF—66&FVEög&ÖW3¢À¢&V6÷fW'•öF—66&FVE÷F–Åö'—FW3¢À¢&V6÷fW'•÷7F'E÷6×ÆUö—5öW7F–ÖFS¢fÇ6RÀ¢Ò“°¢æ–×÷'Eö'W7’ÒG'VS°¢ÆWBòÒæf–æ—6…öVF–õö–×÷'B„W'"‚'6–×VÆFVB–çFW''WFVB–×÷'B"çFõö÷væVB‚’’“° ¢76W'B‡6÷W&6U÷F‚æ—5öf–ÆR‚’“°¢76W'B†Öæ–fW7E÷F‚æ—5öf–ÆR‚’“°¢76W'B†ç&V6÷&Eö–×÷'E÷G&6·2æ—5öæöæR‚’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òæ–B‚’ÂG&6µö–B“°¢76W'B†ç7FGW2æ6öçF–ç2‚'6–×VÆFVB–çFW''WFVB–×÷'B"’“° ¢f÷"F‚–â²g&ö¦V7E÷F‚ÂfÖæ–fW7E÷F‚Âg6÷W&6U÷F…Ò°¢ÆWBòÒ7FC£¦g3£§&VÖ÷fUöf–ÆR‡F‚“°¢Ğ§Ğ ¢5·FW7EĞ¦fâf–ÆVE÷&V6÷fW'•÷Æ6VÖVçE÷&VÖ÷fW5÷&V7&VFVE÷G&6·2‚’°¢ÆWBF—&V7F÷'’ÒFV×f–ÆS£§FV×F—"‚’æW‡V7B‚'FW7BF—&V7F÷'’6†÷VÆB&R7&VFVB"“°¢ÆWB&ö¦V7E÷F‚ÒF—&V7F÷'’çF‚‚’æ¦ö–â‚'&V6÷&F–æræFr"“°¢ÆWB×WB&ö¦V7BÒ&ö¦V7C£¦æWr‚“°¢&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢%6fVBG&6²"çFõö÷væVB‚’À¢Ò¢æW‡V7B‚'6fVBG&6²6†÷VÆB&R7&VFVB"“°¢6fU÷&ö¦V7Eöf–ÆR‡&ö¦V7E÷F‚æ6ÆöæR‚’Â&ö¦V7Bç6æ6†÷B‚’ÂfÇ6R¢æW‡V7B‚'&ö¦V7B6†÷VÆB&R6fVB"“°¢ÆWB–çfÆ–E÷G&6µö–BÒ°¢ÆWB–æFW‚Ò&ö¦V7BçG&6·2‚’æÆVâ‚“°¢&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFW‚À¢æÖS¢$Ö—76–ærG&6²"çFõö÷væVB‚’À¢Ò¢æW‡V7B‚'FV×÷&'’G&6²6†÷VÆB&R7&VFVB"“°¢ÆWBG&6µö–BÒ&ö¦V7BçG&6·2‚•¶–æFW…Òæ–B‚“°¢&ö¦V7@¢æÇ’„Ft7F–öã£¤FVÆWFUG&6²²G&6µö–BÒ¢æW‡V7B‚'FV×÷&'’G&6²6†÷VÆB&R&VÖ÷fVB"“°¢G&6µö–@¢Ó°¢&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢&ö¦V7BçG&6·2‚’æÆVâ‚’À¢æÖS¢%&W7F÷&VBG&6²"çFõö÷væVB‚’À¢Ò¢æW‡V7B‚'&V7&VFVBG&6²6†÷VÆB&R7&VFVB"“°¢ÆWB&V7&VFVE÷G&6µö–BÒ&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWB×WBÒ°¢&ö¦V7BÀ¢&ö¦V7E÷Fƒ¢6öÖR‡&ö¦V7E÷F‚æ6ÆöæR‚’’À¢âä£¦FVfVÇB‚¢Ó°¢ç&V6÷&Eö–×÷'E÷G&6·2Ò6öÖR‡7WW#£¥&V6÷&D–×÷'EF&vWB°¢G&6µö–G3¢fV2¶–çfÆ–E÷G&6µö–EÒÀ¢&V7&VFVE÷G&6µö–G3¢fV2·&V7&VFVE÷G&6µö–EÒÀ¢6÷W&6U÷F‡3¢fV2¶F—&V7F÷'’çF‚‚’æ¦ö–â‚'&V6÷fW&VBçvb"•ÒÀ¢æW‡E÷6VvÖVçEö–æFWƒ¢À¢æW‡E÷7F'E÷6×ÆS¢À¢–×÷'FVEö7F–öç3¢fV3£¦æWr‚’À¢&ö¦V7E÷F‚À¢6×ÆU÷&FS¢ç&ö¦V7Bç6WGF–æw2‚’ç6×ÆU÷&FR‚’À¢&V6÷fW'•öÖæ–fW7E÷Fƒ¢F—&V7F÷'’çF‚‚’æ¦ö–â‚'&V6÷&F–ærç&V6÷fW'’æ§6öâ"’À¢&ö¦V7EövVæW&F–öã¢ç&ö¦V7EövVæW&F–öâÀ¢&V6÷fW'•öF—66&FVEög&ÖW3¢À¢&V6÷fW'•öF—66&FVE÷F–Åö'—FW3¢À¢&V6÷fW'•÷7F'E÷6×ÆUö—5öW7F–ÖFS¢fÇ6RÀ¢Ò“°¢æ–×÷'Eö'W7’ÒG'VS° ¢ÆWBòÒæf–æ—6…öVF–õö–×÷'B„ö²„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢–çfÆ–E÷G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢òöf–ÆVB×&V6÷fW'’×Æ6VÖVçB"çFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò’“° ¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³ÒææÖR‚’Â%6fVBG&6²"“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚’æ—5öV×G’‚’“°¢76W'B†ç7FGW2æ6öçF–ç2‚$7F–öâf–ÆVB"’“°§Ğ ¢5·FW7EĞ¦fâ6fUö5÷F&vWEö÷fW'&–FW5÷F†Uö7W'&VçE÷&ö¦V7E÷F‚‚’°¢ÆWB7W'&VçBÒ7FC£§Fƒ£¥Fƒ£¦æWr‚"÷&ö¦V7G2ö7W'&VçBæFr"“°¢ÆWB6VÆV7FVBÒ7FC£§Fƒ£¥F„'Vc£¦g&öÒ‚"÷&ö¦V7G2öF–ÆöræFr"“° ¢ÆWB&W6öÇfVBÒ7WW#£§&ö¦V7Eö–ó£§&W6öÇfU÷6fU÷F&vWB€¢6öÖR†7W'&VçB’À¢"÷&ö¦V7G2÷G—VBæFr"À¢6öÖR‡6VÆV7FVBæ6ÆöæR‚’’À¢“° ¢76W'EöW‡&W6öÇfVBÂ6öÖR‚‡6VÆV7FVBÂfÇ6R’’“°§Ğ ¢5·FW7EĞ¦fâ6fUö5öÆÆ÷w5ö÷fW'w&—FUööæÇ•öf÷%÷F†Uö7W'&VçEöf–ÆR‚’°¢ÆWB7W'&VçBÒ7FC£§Fƒ£¥Fƒ£¦æWr‚"÷&ö¦V7G2ö7W'&VçBæFr"“°¢ÆWB6VÆV7FVEö7W'&VçBÒ7FC£§Fƒ£¥F„'Vc£¦g&öÒ‚"÷&ö¦V7G2ö7W'&VçBæFr"“°¢ÆWB6VÆV7FVEö÷F†W"Ò7FC£§Fƒ£¥F„'Vc£¦g&öÒ‚"÷&ö¦V7G2ö÷F†W"æFr"“° ¢76W'EöW€¢7WW#£§&ö¦V7Eö–ó£§&W6öÇfU÷6fU÷F&vWB…6öÖR†7W'&VçB’Â""ÂæöæR’À¢6öÖR‚†7W'&VçBçFõ÷F…ö'Vb‚’ÂG'VR’¢“°¢76W'EöW€¢7WW#£§&ö¦V7Eö–ó£§&W6öÇfU÷6fU÷F&vWB€¢6öÖR†7W'&VçB’À¢"÷&ö¦V7G2÷G—VBæFr"À¢6öÖR‡6VÆV7FVEö7W'&VçBæ6ÆöæR‚’’À¢’À¢6öÖR‚‡6VÆV7FVEö7W'&VçBÂG'VR’¢“°¢76W'EöW€¢7WW#£§&ö¦V7Eö–ó£§&W6öÇfU÷6fU÷F&vWB€¢6öÖR†7W'&VçB’À¢"÷&ö¦V7G2÷G—VBæFr"À¢6öÖR‡6VÆV7FVEö÷F†W"æ6ÆöæR‚’’À¢’À¢6öÖR‚‡6VÆV7FVEö÷F†W"ÂfÇ6R’¢“°¢76W'EöW€¢7WW#£§&ö¦V7Eö–ó£§&W6öÇfU÷6fU÷F&vWB„æöæRÂ"÷&ö¦V7G2÷G—VBæFr"ÂæöæR’À¢6öÖR‚‡7FC£§Fƒ£¥F„'Vc£¦g&öÒ‚"÷&ö¦V7G2÷G—VBæFr"’ÂfÇ6R’¢“°§Ğ ¢5·FW7EĞ¦fâæW‡E÷&ö¦V7E÷fW'6–öå÷F…öÖF6†W5÷&VW%÷FF–æuöæE÷6¶—5öW†—7F–æu÷fW'6–öç2‚’°¢ÆWBF—&V7F÷'’ÒFV×f–ÆS£§FV×F—"‚’çVçw&‚“°¢ÆWB7W'&VçBÒF—&V7F÷'’çF‚‚’æ¦ö–â‚$Ö—‚æFr"“°¢ÆWBf—'7BÒF—&V7F÷'’çF‚‚’æ¦ö–â‚$Ö—…óæFr"“°¢ÆWB6V6öæBÒF—&V7F÷'’çF‚‚’æ¦ö–â‚$Ö—…ó"æFr"“°¢ÆWBF†—&BÒF—&V7F÷'’çF‚‚’æ¦ö–â‚$Ö—…ó2æFr"“° ¢76W'EöW€¢7WW#£§&ö¦V7Eö–ó£¦æW‡E÷&ö¦V7E÷fW'6–öå÷F‚‚f7W'&VçB’À¢ö²†f—'7Bæ6ÆöæR‚’¢“°¢7FC£¦g3£§w&—FR‚ff—'7BÂ"&ö67W–VB"’çVçw&‚“°¢76W'EöW€¢7WW#£§&ö¦V7Eö–ó£¦æW‡E÷&ö¦V7E÷fW'6–öå÷F‚‚f7W'&VçB’À¢ö²‡6V6öæBæ6ÆöæR‚’¢“°¢76W'EöW€¢7WW#£§&ö¦V7Eö–ó£¦æW‡E÷&ö¦V7E÷fW'6–öå÷F‚‚ff—'7B’À¢ö²‡6V6öæBæ6ÆöæR‚’¢“°¢7FC£¦g3£§w&—FR‚g6V6öæBÂ"&ö67W–VB"’çVçw&‚“°¢76W'EöW€¢7WW#£§&ö¦V7Eö–ó£¦æW‡E÷&ö¦V7E÷fW'6–öå÷F‚‚f7W'&VçB’À¢ö²‡F†—&B¢“°§Ğ ¢5·FW7EĞ¦fâ6f–æuööæWu÷&ö¦V7E÷fW'6–öå÷&W6W'fW5÷F†U÷6÷W&6UöæEö6÷–W5ö7W'&VçE÷7FFR‚’°¢ÆWBF—&V7F÷'’ÒFV×f–ÆS£§FV×F—"‚’çVçw&‚“°¢ÆWB6÷W&6RÒF—&V7F÷'’çF‚‚’æ¦ö–â‚$Ö—‚æFr"“°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢6fU÷&ö¦V7Eöf–ÆR‡6÷W&6Ræ6ÆöæR‚’Âç&ö¦V7Bç6æ6†÷B‚’ÂfÇ6R¢æW‡V7B‚'F†R6÷W&6R&ö¦V7B6†÷VÆB&R6fVB"“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“° ¢ÆWBfW'6–öâÒ7WW#£§&ö¦V7Eö–ó£¦æW‡E÷&ö¦V7E÷fW'6–öå÷F‚‚g6÷W&6R’çVçw&‚“°¢ÆWB×WBfW'6–öåöÆö6²Ò&ö¦V7E6W76–öäÆö6³£¦7V—&R‚gfW'6–öâ’çVçw&‚“°¢6fU÷&ö¦V7E÷6W76–öåöf–ÆU÷v—F…öÖVF–€¢fW'6–öâæ6ÆöæR‚’À¢ç&ö¦V7Bç6æ6†÷B‚’À¢çF–ÖVÆ–æRæ'&ævVÖVçE÷f–Wu÷7FFR‚fç&ö¦V7B’À¢fÇ6RÀ¢6öÖR‚f×WBfW'6–öåöÆö6²’À¢6öÖR‡6÷W&6Ræ6ÆöæR‚’’À¢¢æW‡V7B‚'F†RæWrfW'6–öâ6†÷VÆB&R6fVBv—F†÷WB&WÆ6–ærF†R6÷W&6R"“° ¢ÆWB÷&–v–æÂÒÆöE÷&ö¦V7Eöf–ÆR‡6÷W&6Ræ6ÆöæR‚’’çVçw&‚“°¢ÆWB6fVE÷fW'6–öâÒÆöE÷&ö¦V7Eöf–ÆR‡fW'6–öâæ6ÆöæR‚’’çVçw&‚“°¢76W'B‡6÷W&6Ræ—5öf–ÆR‚’“°¢76W'B‡fW'6–öâæ—5öf–ÆR‚’“°¢76W'EöW†÷&–v–æÂçG&6·2‚’æÆVâ‚’Â“°¢76W'EöW‡6fVE÷fW'6–öâçG&6·2‚’æÆVâ‚’Â"“°§Ğ ¢5·FW7EĞ¦fâF—'G•÷&ö¦V7Eö6ææ÷Eö&U÷&WÆ6VEö'•ö÷Vâ‚’°¢ÆWB×WBÒ°¢&ö¦V7E÷F…÷VW'“¢"÷&ö¦V7G2öæ÷F†W"æFr"çFõö÷væVB‚’À¢&Wf—6–öã¢À¢âä£¦FVfVÇB‚¢Ó° ¢ÆWBòÒ7WW#£§&ö¦V7Eö–ó£¦÷Vå÷&ö¦V7B‚f×WB“° ¢76W'B‚æ–õö'W7’“°¢76W'EöW†ç7FGW2Â%6fR7W'&VçB&ö¦V7B&Vf÷&R÷Væ–æræ÷F†W""“°§Ğ ¢5·FW7EĞ¦fâF—'G•÷&ö¦V7Eö÷Våö6öÖÖæEöFöW5öæ÷Eö÷Vå÷–6¶W"‚’°¢ÆWB×WBÒ°¢&ö¦V7E÷F…÷VW'“¢"÷&ö¦V7G2öæ÷F†W"æFr"çFõö÷væVB‚’À¢&Wf—6–öã¢À¢âä£¦FVfVÇB‚¢Ó° ¢ÆWBòÒçWFFR„ÖW76vS£¤÷Vå&ö¦V7B“° ¢76W'B‚æ–õö'W7’“°¢76W'B‚çF…÷–6¶W%ö'W7’“°¢76W'EöW€¢çVæF–æu÷&ö¦V7E÷G&ç6—F–öâÀ¢6öÖR‡7WW#£¥VæF–æu&ö¦V7EG&ç6—F–öã£¤÷Vå&ö¦V7B¢“°¢76W'EöW†ç7FGW2Â%6fR6†ævW2FòF†R7W'&VçB&ö¦V7Cò"“°§Ğ ¢5·FW7EĞ¦fâ&ö¦V7Eöf–ÆU÷6fUöæEö÷Vå÷&÷VæE÷G&—ö6÷&U÷6æ6†÷B‚’°¢ÆWBf–ÆUö–BÒäU…EõDU5Eôd”ÄRæfWF6…öFBƒÂ÷&FW&–æs£¥&VÆ†VB“°¢ÆWBF‚Ğ¢7FC£¦Vçc£§FV×öF—"‚’æ¦ö–â†f÷&ÖB‚&Fr×V’×·Ò×¶f–ÆUö–GÒæFr"Â7FC£§&ö6W73£¦–B‚’’“°¢ÆWB×WB&ö¦V7BÒ&ö¦V7C£¦æWr‚“°¢&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢%W'6—7FVB"çFõö÷væVB‚’À¢Ò¢æW‡V7B‚'G&6²6†÷VÆB&R7&VFVB"“°¢ÆWBW‡V7FVBÒ&ö¦V7Bç6æ6†÷B‚“° ¢6fU÷&ö¦V7Eöf–ÆR‡F‚æ6ÆöæR‚’ÂW‡V7FVBæ6ÆöæR‚’ÂfÇ6R’æW‡V7B‚&æWr&ö¦V7Bf–ÆR6†÷VÆB6fR"“°¢76W'B‡6fU÷&ö¦V7Eöf–ÆR‡F‚æ6ÆöæR‚’Â&ö¦V7C£¦æWr‚’ç6æ6†÷B‚’ÂfÇ6R’æ—5öW'"‚’“°¢ÆWBÆöFVBÒÆöE÷&ö¦V7Eöf–ÆR‡F‚æ6ÆöæR‚’’æW‡V7B‚'&ö¦V7B6†÷VÆB÷Vâ"“°¢76W'EöW†ÆöFVBç6æ6†÷B‚’ÂW‡V7FVB“° ¢ÆWBòÒ7FC£¦g3£§&VÖ÷fUöf–ÆR‚gF‚“°¢f÷"7Vff—‚–â²"×vÂ"Â"×6†Ò%Ò°¢ÆWB6–FV6"Òf÷&ÖB‚'·×·7Vff—‡Ò"ÂF‚æF—7Æ’‚’“°¢ÆWBòÒ7FC£¦g3£§&VÖ÷fUöf–ÆR‡6–FV6"“°¢Ğ§Ğ ¢5·FW7EĞ¦fâ7Æ—Eö—FVÕö6öÖÖæG5ö&U÷6VÆV7F–öåöv&UöæE÷6†&UöÖVçU÷6V&6…öFVf–æ—F–öç2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWB7W'6÷"Ò6öÖÖæD–C£¥7Æ—E6VÆV7FVD—FV×4D7W'6÷#°¢ÆWBF–ÖU÷6VÆV7F–öâÒ6öÖÖæD–C£¥7Æ—E6VÆV7FVD—FV×4EF–ÖU6VÆV7F–öã°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ7W'6÷"’“°¢76W'B‚6öÖÖæG3£¦—5öVæ&ÆVB‚fÂF–ÖU÷6VÆV7F–öâ’“° ¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢5óƒCÀ¢Ò¢çVçw&‚“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÒ6öÖR†—FVÕö–B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ–ç6W'B†—FVÕö–B“°¢çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²Ò“c°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ7W'6÷"’“°¢76W'B‚6öÖÖæG3£¦—5öVæ&ÆVB‚fÂF–ÖU÷6VÆV7F–öâ’“° ¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6WEF–ÖU6VÆV7F–öâ°¢7F'E÷F–6³¢“cÀ¢VæE÷F–6³¢%óƒƒÀ¢Ò“°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂF–ÖU÷6VÆV7F–öâ’“°¢ÆWB—FVÕöÖVçRÒ6öÖÖæG3£¦f÷%öÖVçR‚fÂÖ–äÖVçS£¤—FVÒ“°¢f÷"6öÖÖæB–â¶7W'6÷"ÂF–ÖU÷6VÆV7F–öåÒ°¢ÆWBVçG'’Ò—FVÕöÖVçP¢æ—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒ6öÖÖæB¢æW‡V7B‚$—FVÒÖVçR6†÷VÆBÆ—7BF†R7Æ—B6öÖÖæB"“°¢76W'EöW†VçG'’æVæ&ÆVBÂ6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæB’“°¢76W'B†VçG'’æVæ&ÆVB“°¢Ğ¢ÆWB7Æ—EöEö7W'6÷"Ò—FVÕöÖVçP¢æ—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒ7W'6÷"¢æW‡V7B‚$—FVÒÖVçR6†÷VÆBW‡÷6R7Æ—B—FV×2B7W'6÷""“°¢76W'EöW‡7Æ—EöEö7W'6÷"æÆ&VÂÂ%7Æ—B—FV×2B7W'6÷""“°¢76W'EöW‡7Æ—EöEö7W'6÷"ç6†÷'F7WBæ5öFW&Vb‚’Â6öÖR‚%2"’“°¢ÆWB7W'6÷%ö7F–öâÒ6öÖÖæG3£¦ÖF6†–æuö7F–öç5öÖVçR‚fÂ'7Æ—B—FV×2BVF—B÷"Æ’7W'6÷""¢æ–çFõö—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒ7W'6÷"¢æW‡V7B‚$7F–öâÆ—7B6†÷VÆBW6RF†RæF—fRVF—B÷Æ’7W'6÷"Æ&VÂ"“°¢76W'EöW€¢7W'6÷%ö7F–öâæÆ&VÂÀ¢%7Æ—B—FV×2BVF—B÷"Æ’7W'6÷"‡6VÆV7B&–v‡B’ ¢“°¢76W'EöW†7W'6÷%ö7F–öâç6†÷'F7WBæ5öFW&Vb‚’Â6öÖR‚%2"’“°¢ÆWB7Æ—EöE÷F–ÖU÷6VÆV7F–öâÒ—FVÕöÖVçP¢æ—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒF–ÖU÷6VÆV7F–öâ¢æW‡V7B‚$—FVÒÖVçR6†÷VÆBW‡÷6R7Æ—B—FV×2BF–ÖR6VÆV7F–öâ"“°¢76W'EöW€¢7Æ—EöE÷F–ÖU÷6VÆV7F–öâæÆ&VÂÀ¢%7Æ—B—FV×2BF–ÖR6VÆV7F–öâ ¢“°¢76W'EöW‡7Æ—EöE÷F–ÖU÷6VÆV7F–öâç6†÷'F7WBæ5öFW&Vb‚’Â6öÖR‚%6†–gBµ2"’“°¢76W'EöW†6öÖÖæG3£¦f–æB‚fÂ'7Æ—BB7W'6÷""’Â6öÖR†7W'6÷"’“°¢76W'EöW€¢6öÖÖæG3£¦f–æB‚fÂ'7Æ—BBF–ÖR6VÆV7F–öâ"’À¢6öÖR‡F–ÖU÷6VÆV7F–öâ¢“° ¢çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²Ò°¢çF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâÒæöæS°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ7W'6÷"’“°¢æ–õö'W7’ÒG'VS°¢çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²Ò“c°¢76W'B‚6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ7W'6÷"’“°§Ğ ¢5·FW7EĞ¦fâF–ÖU÷6VÆV7F–öå÷7Æ—G5öÖ—†VEö—FV×5ööæ6UöæE÷&W6W'fW5öVF–õöæEöÖ–F•ö6öçFVçB‚’°¢ÆWBf–ÆUö–BÒäU…EõDU5Eôd”ÄRæfWF6…öFBƒÂ÷&FW&–æs£¥&VÆ†VB“°¢ÆWB&ö¦V7E÷F‚Ò7FC£¦Vçc£§FV×öF—"‚’æ¦ö–â†f÷&ÖB€¢&Fr×V’×7Æ—B×·Ò×¶f–ÆUö–GÒæFr"À¢7FC£§&ö6W73£¦–B‚¢’“°¢ÆWB×WB7F÷&RÒ&ö¦V7E7F÷&S£¦÷Vâ‚g&ö¦V7E÷F‚’æW‡V7B‚'&ö¦V7B7F÷&R6†÷VÆB÷Vâ"“°¢ÆWBÖVF–÷&VbÒ&76WC¢ò÷7Æ—B×FW7B#°¢7F÷&P¢æ–×÷'EöVF–õö76WB†ÖVF–÷&VbÂ'7Æ—B×FW7Bçvb"Â7FC£¦–ó£¤7W'6÷#£¦æWr…³÷Sƒ²EÒ’¢æW‡V7B‚'FW7B76WB6†÷VÆB–×÷'B"“°¢7F÷&P¢ç6WEöVF–õö76WEöÖWFFF€¢ÖVF–÷&VbÀ¢fFu÷7F÷&vS£¤VF–ô76WDÖWFFF°¢6öçF–æW#¢'vb"çFõö÷væVB‚’À¢6öFV3¢'6Ò"çFõö÷væVB‚’À¢6×ÆU÷&FS¢6öÖRƒCEó’À¢6†ææVÅö6÷VçC¢6öÖRƒ’À¢&—G5÷W%÷6×ÆS¢6öÖRƒb’À¢g&ÖUö6÷VçC¢6öÖRƒó’À¢GW&F–öåöææ÷3¢æöæRÀ¢'—FUöÆVã¢6öÖRƒB’À¢ÒÀ¢¢æW‡V7B‚'FW7BÖWFFF6†÷VÆB7F÷&R"“°¢7F÷&Ræ6Æ÷6R‚’æW‡V7B‚'&ö¦V7B7F÷&R6†÷VÆB6Æ÷6R"“°¢ÆWB×WBÒ°¢&ö¦V7E÷Fƒ¢6öÖR‡&ö¦V7E÷F‚æ6ÆöæR‚’’À¢âä£¦FVfVÇB‚¢Ó°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWBVF–õöVæBÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒ5óƒC’çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢ÖVF–÷&VbçFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢UóÀ¢ÆVæwF…÷6×ÆW3¢VF–õöVæBÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢5óƒCÀ¢Ò¢çVçw&‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBVç6VÆV7FVE÷G&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWBVç6VÆV7FVEö—FVÕöÆVæwF‚Òç&ö¦V7Bç6×ÆUöE÷F–6²ƒ5óƒC’çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢Vç6VÆV7FVE÷G&6µö–BÀ¢ÖVF–÷&Vc¢ÖVF–÷&VbçFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢Vç6VÆV7FVEö—FVÕöÆVæwF‚À¢Ò¢çVçw&‚“°¢ÆWBÖ–F•ö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–C¢Ö–F•ö–BÀ¢æ÷FW3¢fV2°¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢À¢GW&F–öã¢ó#À¢fVÆö6—G“¢À¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cBÀ¢F–6³¢ócÀ¢GW&F–öã¢ócÀ¢fVÆö6—G“¢“À¢öfe÷fVÆö6—G“¢À¢ÒÀ¢ÒÀ¢Ò¢çVçw&‚“°¢ÆWBVF–õö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÒ6öÖR†Ö–F•ö–B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æW‡FVæB…¶VF–õö–BÂÖ–F•ö–EÒ“°¢çF–ÖVÆ–æP¢æ†æFÆR†7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC£¥6WEF–ÖU6VÆV7F–öâ°¢7F'E÷F–6³¢“cÀ¢VæE÷F–6³¢%óƒƒÀ¢Ò“°¢ÆWB&Vf÷&RÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã° ¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB€¢6öÖÖæD–C£¥7Æ—E6VÆV7FVD—FV×4EF–ÖU6VÆV7F–öâÀ¢’“° ¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²“°¢ÆWB7WE÷6×ÆW2Ò°¢ç&ö¦V7Bç6×ÆUöE÷F–6²ƒ“c’çVçw&‚’À¢ç&ö¦V7Bç6×ÆUöE÷F–6²ƒ%óƒƒ’çVçw&‚’À¢Ó°¢ÆWBVF–òÒç&ö¦V7BæVF–õö—FV×2‚“°¢76W'EöW†VF–òæÆVâ‚’ÂB“°¢ÆWB7Æ—EöVF–òÒVF–ğ¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â—FVÒçG&6µö–B‚’ÓÒG&6µö–B¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢76W'EöW‡7Æ—EöVF–òæÆVâ‚’Â2“°¢76W'EöW€¢7Æ—EöVF–ğ¢æ—FW"‚¢æÖ‡Æ—FV×Â€¢—FVÒç7F'E÷6×ÆR‚’À¢—FVÒç6÷W&6Uööfg6WE÷6×ÆW2‚’À¢—FVÒæÆVæwF…÷6×ÆW2‚¢’¢æ6öÆÆV7C££ÅfV3Åóãâ‚’À¢°¢ƒÂUóÂ7WE÷6×ÆW5³Ò’À¢€¢7WE÷6×ÆW5³ÒÀ¢“cC£§G'•ög&öÒƒUó²†7WE÷6×ÆW5³Ò¢CEó²#Eó’òC…ó’çVçw&‚’À¢7WE÷6×ÆW5³ÒÒ7WE÷6×ÆW5³Ğ¢’À¢€¢7WE÷6×ÆW5³ÒÀ¢“cC£§G'•ög&öÒƒUó²†7WE÷6×ÆW5³Ò¢CEó²#Eó’òC…ó’çVçw&‚’À¢VF–õöVæBÒ7WE÷6×ÆW5³Ğ¢’À¢Ğ¢“°¢ÆWBVç6VÆV7FVEöVF–òÒVF–ğ¢æ—FW"‚¢æf–æB‡Æ—FV×Â—FVÒçG&6µö–B‚’ÓÒVç6VÆV7FVE÷G&6µö–B¢æW‡V7B‚'Vç6VÆV7FVBG&6²—FVÒ6†÷VÆB&VÖ–â–çF7B"“°¢76W'EöW‡Vç6VÆV7FVEöVF–òç7F'E÷6×ÆR‚’Â“°¢76W'EöW‡Vç6VÆV7FVEöVF–òæÆVæwF…÷6×ÆW2‚’ÂVF–õöVæB“°¢76W'B‚çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2‚gVç6VÆV7FVEöVF–òæ–B‚’’“°¢ÆWBÖ–F’Òç&ö¦V7BæÖ–F•ö—FV×2‚“°¢76W'EöW€¢Ö–F’æ—FW"‚¢æÖ‡Æ—FV×Â†—FVÒç7F'E÷F–6²‚’Â—FVÒæÆVæwF…÷F–6·2‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚’À¢²ƒÂ“c’Âƒ“cÂó“#’Âƒ%óƒƒÂ“c•Ğ¢“°¢76W'EöW€¢Ö–F’æ—FW"‚¢æÖ‡Æ—FV×Â°¢—FVÒææ÷FW2‚¢æ—FW"‚¢æÖ‡Ææ÷FWÂ†æ÷FRç—F6‚‚’Âæ÷FRçF–6²‚’Âæ÷FRæGW&F–öâ‚’’¢æ6öÆÆV7C££ÅfV3Åóãâ‚¢Ò¢æ6öÆÆV7C££ÅfV3Åóãâ‚’À¢°¢fV2²ƒcÂÂ“c•ÒÀ¢fV2²ƒcÂÂ#C’ÂƒcBÂcCÂó#ƒ•ÒÀ¢fV2²ƒcBÂÂ3#•ÒÀ¢Ğ¢“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æÆVâ‚’Â"“°¢76W'B†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2‚g7Æ—EöVF–õ³Òæ–B‚’’“°¢76W'B†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2‚fÖ–F•³Òæ–B‚’’“°¢76W'B‚çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2‚g7Æ—EöVF–õ³Òæ–B‚’’“°¢76W'B‚çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2‚g7Æ—EöVF–õ³%Òæ–B‚’’“°¢76W'B‚çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2‚fÖ–F•³Òæ–B‚’’“°¢76W'B‚çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2‚fÖ–F•³%Òæ–B‚’’“°¢76W'EöW€¢çF–ÖVÆ–æRçF–ÖU÷6VÆV7F–öâÀ¢6öÖR†7&FS£§F–ÖVÆ–æS£¥F–ÖU6VÆV7F–öâ°¢7F'E÷F–6³¢“cÀ¢VæE÷F–6³¢%óƒƒÀ¢Ò¢“° ¢ÆWBgFW"Òç&ö¦V7Bç6æ6†÷B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&Vf÷&R“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’ÂgFW"“°¢ÆWBòÒ7FC£¦g3£§&VÖ÷fUöf–ÆR‚g&ö¦V7E÷F‚“°¢f÷"7Vff—‚–â²"×vÂ"Â"×6†Ò%Ò°¢ÆWB6–FV6"Òf÷&ÖB‚'·×·7Vff—‡Ò"Â&ö¦V7E÷F‚æF—7Æ’‚’“°¢ÆWBòÒ7FC£¦g3£§&VÖ÷fUöf–ÆR‡6–FV6"“°¢Ğ§Ğ ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ğ¢5·FW7EĞ¦fâ7FæF'•ö–çWEö6ö×ÆWF–öåö6ÆV'5÷VæF–æu÷7FFUöGW&–æuö÷F†W%ö&6¶w&÷VæEö¦ö'2‚’°¢ÆWB×WBÒ°¢7FæF'•öÖöæ—F÷%÷7F'F–æs¢G'VRÀ¢–õö'W7“¢G'VRÀ¢–×÷'Eö'W7“¢G'VRÀ¢VF–õö76WEöÖævVÖVçEö'W7“¢G'VRÀ¢&V6÷&F–æu÷&V6÷fW'•ö'W7“¢G'VRÀ¢Æ–&6µö'W7“¢G'VRÀ¢âä£¦FVfVÇB‚¢Ó° ¢ÆWBòÒçWFFR„ÖW76vS£¥7FæF'”–çWD6Æ÷6VB“° ¢76W'B‚ç7FæF'•öÖöæ—F÷%÷7F'F–ær“°§Ğ ¢5·FW7EĞ¦fâ7W'6÷%÷7Æ—E÷'Vç5ög&öÕö7F–öå÷6V&6…öæEö&÷VæF'•öæö÷5öFõöæ÷Eö6†ævUö†—7F÷'’‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢CƒÀ¢ÆVæwF…÷F–6·3¢ó“#À¢Ò¢çVçw&‚“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2´Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢À¢GW&F–öã¢óCCÀ¢fVÆö6—G“¢À¢öfe÷fVÆö6—G“¢À¢ÕÒÀ¢Ò¢çVçw&‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÒ6öÖR†—FVÕö–B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ–ç6W'B†—FVÕö–B“°¢çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²ÒóCC°¢ÆWB&Vf÷&RÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã° ¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öåVW'”6†ævVB‚'7Æ—BB7W'6÷""çFõö÷væVB‚’’“°¢ÆWBòÒçWFFR„ÖW76vS£¥'Vä7F–öåVW'’“° ¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚’æÆVâ‚’Â"“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒæÆVæwF…÷F–6·2‚’Â“c“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒæGW&F–öâ‚’Â“c“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’ÂóCC“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒçF–6²‚’Â“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³ÒæGW&F–öâ‚’ÂCƒ“°¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2À¢†6…6WC£¦g&öÒ…¶ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚•Ò¢“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&Vf÷&R“° ¢çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²ÒCƒ°¢ÆWB&Vf÷&Uöæö÷Òç&ö¦V7Bç6æ6†÷B‚“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB€¢fÀ¢6öÖÖæD–C£¥7Æ—E6VÆV7FVD—FV×4D7W'6÷ ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥7Æ—E6VÆV7FVD—FV×4D7W'6÷"“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&Vf÷&Uöæö÷“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ“°§Ğ ¢5·FW7EĞ¦fâ7W'6÷%÷7Æ—E÷&W6W'fW5ö×VÇF•÷F¶UöÖ–F•öæE÷6VÆV7G5÷F†U÷&–v‡E÷6VvÖVçB‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢5óƒCÀ¢Ò¢çVçw&‚“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2°¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢“cÀ¢GW&F–öã¢ó“#À¢fVÆö6—G“¢À¢öfe÷fVÆö6—G“¢3rÀ¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cBÀ¢F–6³¢ó#À¢GW&F–öã¢“cÀ¢fVÆö6—G“¢“"À¢öfe÷fVÆö6—G“¢À¢ÒÀ¢ÒÀ¢Ò¢çVçw&‚“°¢ÆWB×WB6æ6†÷BÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWB—FVÒÒf×WB6æ6†÷BæÖ–F•ö—FV×5³Ó°¢ÆWBF¶Uööæ÷FRÒ—FVÒææ÷FW5³Òæ6ÆöæR‚“°¢—FVÒææ÷FW2ÒfV2¶—FVÒææ÷FW5³Òæ6ÆöæR‚•Ó°¢—FVÒæ7F—fU÷F¶Uö–æFW‚Ò°¢—FVÒææÖRÒ%F¶R""æ–çFò‚“°¢—FVÒæ÷F†W%÷F¶W2ÒfV2´Ö–F”—FVÕF¶U6æ6†÷B°¢–æFWƒ¢À¢æÖS¢%F¶R"æ–çFò‚’À¢6÷W&6Uööfg6WE÷F–6·3¢À¢Æö÷÷6÷W&6S¢fÇ6RÀ¢6÷W&6UöÆö÷öÆVæwF…÷F–6·3¢—FVÒæÆVæwF…÷F–6·2À¢æ÷FW3¢fV2·F¶Uööæ÷FUÒÀ¢6öçG&öÆÆW'3¢fV3£¦æWr‚’À¢—F6…ö&VæG3¢fV3£¦æWr‚’À¢&öw&Õö6†ævW3¢fV3£¦æWr‚’À¢öÇ•ögFW'F÷V6†W3¢fV3£¦æWr‚’À¢6†ææVÅ÷&W77W&W3¢fV3£¦æWr‚’À¢÷VUöWfVçG3¢fV3£¦æWr‚’À¢WfVçEö÷&FW#¢fV3£¦æWr‚’À¢ÕÓ°¢ç&ö¦V7BÒ&ö¦V7C£¦g&öÕ÷6æ6†÷B‡6æ6†÷B’çVçw&‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2²†—FVÕö–BÂG&6µö–B•Ò“°¢çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²Òó“#°¢ÆWB&Vf÷&RÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã° ¢ÆWBòÒçWFFR„ÖW76vS£¥7Æ—E6VÆV7FVD—FV×4D7W'6÷"“° ¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²“°¢ÆWB—FV×2Òç&ö¦V7BæÖ–F•ö—FV×2‚“°¢76W'EöW†—FV×2æÆVâ‚’Â"“°¢76W'B†—FV×2æ—FW"‚’æÆÂ‡Æ—FV×Â°¢—FVÒçF¶Uö6÷VçB‚’ÓÒ"bb—FVÒæ7F—fU÷F¶Uö–æFW‚‚’ÓÒbb—FVÒææÖR‚’ÓÒ%F¶R" ¢Ò’“°¢76W'EöW†—FV×5³ÒçF¶U÷6æ6†÷G2‚•³Òææ÷FW5³ÒæFFæGW&F–öâÂ“c“°¢76W'EöW†—FV×5³ÒçF¶U÷6æ6†÷G2‚•³Òææ÷FW5³ÒæFFçF–6²Â“°¢76W'EöW†—FV×5³ÒçF¶U÷6æ6†÷G2‚•³Òææ÷FW5³ÒæFFçF–6²Â“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â†6…6WC£¦g&öÒ…¶—FV×5³Òæ–B‚•Ò’“°¢ÆWBgFW"Òç&ö¦V7Bç6æ6†÷B‚“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&Vf÷&R“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’ÂgFW"“°§Ğ ¢5·FW7EĞ¦fâ7W'6÷%÷7Æ—E÷v—F†÷WE÷6VÆV7F–öå÷7Æ—G5öÆÅ÷G&6·5öæE÷6VÆV7G5÷&–v‡E÷6VvÖVçG5ööæ6R‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–G2Ò ¢ç&ö¦V7@¢çG&6·2‚¢æ—FW"‚¢æÖ‡ÇG&6·ÂG&6²æ–B‚’¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢f÷"G&6µö–B–âgG&6µö–G2°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–C¢§G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢5óƒCÀ¢Ò¢çVçw&‚“°¢Ğ¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²Òó“#°¢76W'B†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ—5öV×G’‚’“°¢ÆWB&Vf÷&RÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã° ¢ÆWBòÒçWFFR„ÖW76vS£¥7Æ—E6VÆV7FVD—FV×4D7W'6÷"“° ¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚’æÆVâ‚’ÂB“°¢f÷"G&6µö–B–âG&6µö–G2°¢ÆWBg&vÖVçG2Ò ¢ç&ö¦V7@¢æÖ–F•ö—FV×2‚¢æ—FW"‚¢æf–ÇFW"‡Æ—FV×Â—FVÒçG&6µö–B‚’ÓÒG&6µö–B¢æ6öÆÆV7C££ÅfV3Åóãâ‚“°¢76W'EöW†g&vÖVçG2æÆVâ‚’Â"“°¢76W'EöW€¢†g&vÖVçG5³Òç7F'E÷F–6²‚’Âg&vÖVçG5³ÒæÆVæwF…÷F–6·2‚’’À¢ƒÂó“#¢“°¢76W'EöW€¢†g&vÖVçG5³Òç7F'E÷F–6²‚’Âg&vÖVçG5³ÒæÆVæwF…÷F–6·2‚’’À¢ƒó“#Âó“#¢“°¢76W'B‚çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2‚fg&vÖVçG5³Òæ–B‚’’“°¢76W'B†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6öçF–ç2‚fg&vÖVçG5³Òæ–B‚’’“°¢Ğ¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æÆVâ‚’Â"“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²“°¢ÆWBgFW"Òç&ö¦V7Bç6æ6†÷B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&Vf÷&R“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’ÂgFW"“° ¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ6ÆV"‚“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÒæöæS°¢çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²Òó“#°¢ÆWB&Vf÷&Uö&÷VæF'’Òç&ö¦V7Bç6æ6†÷B‚“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¥7Æ—E6VÆV7FVD—FV×4D7W'6÷"“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&Vf÷&Uö&÷VæF'’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ“°§Ğ ¢5·FW7EĞ¦fâ7W'6÷%÷7Æ—E÷v—F†÷WE÷6VÆV7F–öå÷&W6W'fW5öVF–õ÷6÷W&6U÷&FUöæE÷6VÆV7G5÷&–v‡Eö—FVÒ‚’°¢ÆWBf–ÆUö–BÒäU…EõDU5Eôd”ÄRæfWF6…öFBƒÂ÷&FW&–æs£¥&VÆ†VB“°¢ÆWB&ö¦V7E÷F‚Ò7FC£¦Vçc£§FV×öF—"‚’æ¦ö–â†f÷&ÖB€¢&Fr×V’Ö7W'6÷"×7Æ—B×·Ò×¶f–ÆUö–GÒæFr"À¢7FC£§&ö6W73£¦–B‚¢’“°¢ÆWBÖVF–÷&VbÒ&76WC¢òö7W'6÷"×7Æ—B×FW7B#°¢ÆWB×WB7F÷&RÒ&ö¦V7E7F÷&S£¦÷Vâ‚g&ö¦V7E÷F‚’æW‡V7B‚'&ö¦V7B7F÷&R6†÷VÆB÷Vâ"“°¢7F÷&P¢æ–×÷'EöVF–õö76WB€¢ÖVF–÷&VbÀ¢&7W'6÷"×7Æ—B×FW7Bçvb"À¢7FC£¦–ó£¤7W'6÷#£¦æWr…³÷Sƒ²EÒ’À¢¢æW‡V7B‚'FW7B76WB6†÷VÆB–×÷'B"“°¢7F÷&P¢ç6WEöVF–õö76WEöÖWFFF€¢ÖVF–÷&VbÀ¢fFu÷7F÷&vS£¤VF–ô76WDÖWFFF°¢6öçF–æW#¢'vb"çFõö÷væVB‚’À¢6öFV3¢'6Ò"çFõö÷væVB‚’À¢6×ÆU÷&FS¢6öÖRƒCEó’À¢6†ææVÅö6÷VçC¢6öÖRƒ’À¢&—G5÷W%÷6×ÆS¢6öÖRƒb’À¢g&ÖUö6÷VçC¢6öÖRƒó’À¢GW&F–öåöææ÷3¢æöæRÀ¢'—FUöÆVã¢6öÖRƒB’À¢ÒÀ¢¢æW‡V7B‚'FW7BVF–ò6÷W&6RÖWFFF6†÷VÆB7F÷&R"“°¢7F÷&Ræ6Æ÷6R‚’æW‡V7B‚'&ö¦V7B7F÷&R6†÷VÆB6Æ÷6R"“° ¢ÆWB×WBÒ°¢&ö¦V7E÷Fƒ¢6öÖR‡&ö¦V7E÷F‚æ6ÆöæR‚’’À¢âä£¦FVfVÇB‚¢Ó°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWB—FVÕöVæBÒç&ö¦V7Bç6×ÆUöE÷F–6²ƒ5óƒC’çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢ÖVF–÷&VbçFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢UóÀ¢ÆVæwF…÷6×ÆW3¢—FVÕöVæBÀ¢Ò¢çVçw&‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRæVF—Eö7W'6÷%÷F–6²Òó“#°¢ÆWB&Vf÷&RÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã° ¢ÆWBòÒçWFFR„ÖW76vS£¥7Æ—E6VÆV7FVD—FV×4D7W'6÷"“° ¢ÆWB—FV×2Òç&ö¦V7BæVF–õö—FV×2‚“°¢76W'EöW†—FV×2æÆVâ‚’Â"“°¢76W'EöW€¢†—FV×5³Òç7F'E÷6×ÆR‚’Â—FV×5³ÒæÆVæwF…÷6×ÆW2‚’’À¢ƒÂC…ó¢“°¢76W'EöW†—FV×5³Òç6÷W&6Uööfg6WE÷6×ÆW2‚’ÂUó“°¢76W'EöW€¢†—FV×5³Òç7F'E÷6×ÆR‚’Â—FV×5³ÒæÆVæwF…÷6×ÆW2‚’’À¢ƒC…óÂC…ó¢“°¢76W'EöW†—FV×5³Òç6÷W&6Uööfg6WE÷6×ÆW2‚’ÂC•ó“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â†6…6WC£¦g&öÒ…¶—FV×5³Òæ–B‚•Ò’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²“°¢ÆWBgFW"Òç&ö¦V7Bç6æ6†÷B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&Vf÷&R“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’ÂgFW"“° ¢G&÷†“°¢ÆWBòÒ7FC£¦g3£§&VÖ÷fUöf–ÆR‚g&ö¦V7E÷F‚“°¢f÷"7Vff—‚–â²"×vÂ"Â"×6†Ò%Ò°¢ÆWB6–FV6"Òf÷&ÖB‚'·×·7Vff—‡Ò"Â&ö¦V7E÷F‚æF—7Æ’‚’“°¢ÆWBòÒ7FC£¦g3£§&VÖ÷fUöf–ÆR‡6–FV6"“°¢Ğ§Ğ ¢5·FW7EĞ¦fâ†VÅ÷7Æ—G5ööæÇ•öÖW&vW5÷6VÆV7FVEö6öçF–wV÷W5öVF–õ÷6V7F–öç5÷VæFö&Ç’‚’°¢ÆWBf–ÆUö–BÒäU…EõDU5Eôd”ÄRæfWF6…öFBƒÂ÷&FW&–æs£¥&VÆ†VB“°¢ÆWB&ö¦V7E÷F‚Ò7FC£¦Vçc£§FV×öF—"‚’æ¦ö–â†f÷&ÖB€¢&Fr×V’Ö†VÂ×7Æ—G2×·Ò×¶f–ÆUö–GÒæFr"À¢7FC£§&ö6W73£¦–B‚¢’“°¢ÆWBÖVF–÷&VbÒ&76WC¢òö†VÂ×7Æ—G2×FW7B#°¢ÆWB×WB7F÷&RÒ&ö¦V7E7F÷&S£¦÷Vâ‚g&ö¦V7E÷F‚’æW‡V7B‚'&ö¦V7B7F÷&R6†÷VÆB÷Vâ"“°¢7F÷&P¢æ–×÷'EöVF–õö76WB€¢ÖVF–÷&VbÀ¢&†VÂ×7Æ—G2×FW7Bçvb"À¢7FC£¦–ó£¤7W'6÷#£¦æWr…³÷Sƒ²EÒ’À¢¢æW‡V7B‚'FW7B76WB6†÷VÆB–×÷'B"“°¢7F÷&P¢ç6WEöVF–õö76WEöÖWFFF€¢ÖVF–÷&VbÀ¢fFu÷7F÷&vS£¤VF–ô76WDÖWFFF°¢6öçF–æW#¢'vb"çFõö÷væVB‚’À¢6öFV3¢'6Ò"çFõö÷væVB‚’À¢6×ÆU÷&FS¢6öÖRƒCEó’À¢6†ææVÅö6÷VçC¢6öÖRƒ’À¢&—G5÷W%÷6×ÆS¢6öÖRƒb’À¢g&ÖUö6÷VçC¢6öÖRƒó’À¢GW&F–öåöææ÷3¢æöæRÀ¢'—FUöÆVã¢6öÖRƒB’À¢ÒÀ¢¢æW‡V7B‚'FW7BVF–ò6÷W&6RÖWFFF6†÷VÆB7F÷&R"“°¢ÆWB÷F†W%öÖVF–÷&VbÒ&76WC¢òö†VÂ×7Æ—G2Ö÷F†W"×FW7B#°¢7F÷&P¢æ–×÷'EöVF–õö76WB€¢÷F†W%öÖVF–÷&VbÀ¢&†VÂ×7Æ—G2Ö÷F†W"×FW7Bçvb"À¢7FC£¦–ó£¤7W'6÷#£¦æWr…³÷Sƒ²EÒ’À¢¢æW‡V7B‚'6V6öæBFW7B76WB6†÷VÆB–×÷'B"“°¢7F÷&P¢ç6WEöVF–õö76WEöÖWFFF€¢÷F†W%öÖVF–÷&VbÀ¢fFu÷7F÷&vS£¤VF–ô76WDÖWFFF°¢6öçF–æW#¢'vb"çFõö÷væVB‚’À¢6öFV3¢'6Ò"çFõö÷væVB‚’À¢6×ÆU÷&FS¢6öÖRƒCEó’À¢6†ææVÅö6÷VçC¢6öÖRƒ’À¢&—G5÷W%÷6×ÆS¢6öÖRƒb’À¢g&ÖUö6÷VçC¢6öÖRƒó’À¢GW&F–öåöææ÷3¢æöæRÀ¢'—FUöÆVã¢6öÖRƒB’À¢ÒÀ¢¢æW‡V7B‚'6V6öæBFW7B6÷W&6RÖWFFF6†÷VÆB7F÷&R"“°¢7F÷&Ræ6Æ÷6R‚’æW‡V7B‚'&ö¦V7B7F÷&R6†÷VÆB6Æ÷6R"“° ¢ÆWB×WBÒ°¢&ö¦V7E÷Fƒ¢6öÖR‡&ö¦V7E÷F‚æ6ÆöæR‚’’À¢âä£¦FVfVÇB‚¢Ó°¢76W'B‚6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæD–C£¤†VÅ7Æ—G4–ä—FV×2’“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢ÖVF–÷&VbçFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢UóÀ¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò¢çVçw&‚“°¢ÆWBÆVgEö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢ÖVF–÷&VbçFõö÷væVB‚’À¢7F'E÷6×ÆS¢C…óÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢C•óÀ¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò¢çVçw&‚“°¢ÆWB&–v‡Eö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢ó“#À¢Ò¢çVçw&‚“°¢ÆWBÖ–F•öÆVgBÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢ó“#À¢ÆVæwF…÷F–6·3¢ó“#À¢Ò¢çVçw&‚“°¢ÆWBÖ–F•÷&–v‡BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢ÆWB—FVÕöÖVçRÒ6öÖÖæG3£¦f÷%öÖVçR‚fÂÖ–äÖVçS£¤—FVÒ“°¢ÆWBÖVçUöVçG'’Ò—FVÕöÖVçP¢æ—FW"‚¢æf–æB‡ÆVçG'—ÂVçG'’æ–BÓÒ6öÖÖæD–C£¤†VÅ7Æ—G4–ä—FV×2¢æW‡V7B‚$—FVÒÖVçR6†÷VÆBW‡÷6R†VÂ7Æ—G2–â—FV×2"“°¢76W'B‚ÖVçUöVçG'’æVæ&ÆVB“°¢76W'EöW†ÖVçUöVçG'’æÆ&VÂÂ$†VÂ7Æ—G2–â—FV×2"“°¢76W'EöW†ÖVçUöVçG'’ç6†÷'F7WBÂæöæR“° ¢ÆWBV×G•ö&Vf÷&RÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWBV×G•÷&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤†VÅ7Æ—G4–ä—FV×2“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’ÂV×G•ö&Vf÷&R“°¢76W'EöW†ç&Wf—6–öâÂV×G•÷&Wf—6–öâ“° ¢çF–ÖVÆ–æP¢ç6VÆV7Eö—FV×2‡fV2²†Ö–F•öÆVgBÂG&6µö–B’Â†Ö–F•÷&–v‡BÂG&6µö–B•Ò“°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæD–C£¤†VÅ7Æ—G4–ä—FV×2’“°¢ÆWBÖ–F•ö&Vf÷&RÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWBÖ–F•÷&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤†VÅ7Æ—G4–ä—FV×2“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’ÂÖ–F•ö&Vf÷&R“°¢76W'EöW†ç&Wf—6–öâÂÖ–F•÷&Wf—6–öâ“° ¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2²‡&–v‡Eö–BÂG&6µö–B•Ò“°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæD–C£¤†VÅ7Æ—G4–ä—FV×2’“°¢ÆWB'F–Åö&Vf÷&RÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWB'F–Å÷&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤†VÅ7Æ—G4–ä—FV×2“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â'F–Åö&Vf÷&R“°¢76W'EöW†ç&Wf—6–öâÂ'F–Å÷&Wf—6–öâ“° ¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤VF—DVF–ô—FVÒ°¢—FVÕö–C¢&–v‡Eö–BÀ¢ÖVF–÷&Vc¢÷F†W%öÖVF–÷&VbçFõö÷væVB‚’À¢7F'E÷6×ÆS¢C…óÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢C•óÀ¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò¢çVçw&‚“°¢çF–ÖVÆ–æP¢ç6VÆV7Eö—FV×2‡fV2²†ÆVgEö–BÂG&6µö–B’Â‡&–v‡Eö–BÂG&6µö–B•Ò“°¢ÆWBF–ffW&VçE÷6÷W&6W5ö&Vf÷&RÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWBF–ffW&VçE÷6÷W&6W5÷&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤†VÅ7Æ—G4–ä—FV×2“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’ÂF–ffW&VçE÷6÷W&6W5ö&Vf÷&R“°¢76W'EöW†ç&Wf—6–öâÂF–ffW&VçE÷6÷W&6W5÷&Wf—6–öâ“° ¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤VF—DVF–ô—FVÒ°¢—FVÕö–C¢&–v‡Eö–BÀ¢ÖVF–÷&Vc¢ÖVF–÷&VbçFõö÷væVB‚’À¢7F'E÷6×ÆS¢C…óÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢C•óÀ¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò¢çVçw&‚“°¢çF–ÖVÆ–æP¢ç6VÆV7Eö—FV×2‡fV2²†ÆVgEö–BÂG&6µö–B’Â‡&–v‡Eö–BÂG&6µö–B•Ò“°¢ÆWBF—66öçF–çV÷W5ö&Vf÷&RÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWBF—66öçF–çV÷W5÷&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤†VÅ7Æ—G4–ä—FV×2“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’ÂF—66öçF–çV÷W5ö&Vf÷&R“°¢76W'EöW†ç&Wf—6–öâÂF—66öçF–çV÷W5÷&Wf—6–öâ“° ¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤VF—DVF–ô—FVÒ°¢—FVÕö–C¢&–v‡Eö–BÀ¢ÖVF–÷&Vc¢ÖVF–÷&VbçFõö÷væVB‚’À¢7F'E÷6×ÆS¢C…óÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢C•óÀ¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò¢çVçw&‚“°¢ÆWBvö&Vf÷&RÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWBv÷&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤†VÅ7Æ—G4–ä—FV×2“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Âvö&Vf÷&R“°¢76W'EöW†ç&Wf—6–öâÂv÷&Wf—6–öâ“° ¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤VF—DVF–ô—FVÒ°¢—FVÕö–C¢&–v‡Eö–BÀ¢ÖVF–÷&Vc¢ÖVF–÷&VbçFõö÷væVB‚’À¢7F'E÷6×ÆS¢C…óÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢C•óÀ¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò¢çVçw&‚“°¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2°¢†ÆVgEö–BÂG&6µö–B’À¢‡&–v‡Eö–BÂG&6µö–B’À¢†Ö–F•öÆVgBÂG&6µö–B’À¢†Ö–F•÷&–v‡BÂG&6µö–B’À¢Ò“°¢ÆWB&Vf÷&RÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤†VÅ7Æ—G4–ä—FV×2“° ¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’Â“°¢ÆWB†VÆVBÒfç&ö¦V7BæVF–õö—FV×2‚•³Ó°¢76W'EöW††VÆVBæ–B‚’ÂÆVgEö–B“°¢76W'EöW††VÆVBç7F'E÷6×ÆR‚’Â“°¢76W'EöW††VÆVBç6÷W&6Uööfg6WE÷6×ÆW2‚’ÂUó“°¢76W'EöW††VÆVBæÆVæwF…÷6×ÆW2‚’Â“eó“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚’æÆVâ‚’Â"“°¢76W'EöW€¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2À¢†6…6WC£¦g&öÒ…¶ÆVgEö–BÂÖ–F•öÆVgBÂÖ–F•÷&–v‡EÒ¢“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²“°¢ÆWBgFW"Òç&ö¦V7Bç6æ6†÷B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&Vf÷&R“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’ÂgFW"“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤VF—DVF–ô—FVÒ°¢—FVÕö–C¢ÆVgEö–BÀ¢ÖVF–÷&Vc¢ÖVF–÷&VbçFõö÷væVB‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢UóÀ¢ÆVæwF…÷6×ÆW3¢#EóÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤VF—DVF–ô—FVÒ°¢—FVÕö–C¢&–v‡Eö–BÀ¢ÖVF–÷&Vc¢ÖVF–÷&VbçFõö÷væVB‚’À¢7F'E÷6×ÆS¢#EóÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢#uóSÀ¢ÆVæwF…÷6×ÆW3¢#EóÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢ÖVF–÷&VbçFõö÷væVB‚’À¢7F'E÷6×ÆS¢C…óÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢C•óÀ¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò¢çVçw&‚“°¢ÆWBF†—&Eö–BÒç&ö¦V7BæVF–õö—FV×2‚•³%Òæ–B‚“°¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2°¢†ÆVgEö–BÂG&6µö–B’À¢‡&–v‡Eö–BÂG&6µö–B’À¢‡F†—&Eö–BÂG&6µö–B’À¢Ò“°¢ÆWB&Vf÷&Uö6†–âÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤†VÅ7Æ—G4–ä—FV×2“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚’æÆVâ‚’Â“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚’ÂÆVgEö–B“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒæÆVæwF…÷6×ÆW2‚’Â“eó“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â†6…6WC£¦g&öÒ…¶ÆVgEö–EÒ’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²“°¢ÆWBgFW%ö6†–âÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&Vf÷&Uö6†–â“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’ÂgFW%ö6†–â“° ¢G&÷†“°¢ÆWBòÒ7FC£¦g3£§&VÖ÷fUöf–ÆR‚g&ö¦V7E÷F‚“°¢f÷"7Vff—‚–â²"×vÂ"Â"×6†Ò%Ò°¢ÆWB6–FV6"Òf÷&ÖB‚'·×·7Vff—‡Ò"Â&ö¦V7E÷F‚æF—7Æ’‚’“°¢ÆWBòÒ7FC£¦g3£§&VÖ÷fUöf–ÆR‡6–FV6"“°¢Ğ§Ğ ¦fâFW7E÷6†÷'F7WEö–çWB†Æöv–6Åö¶W“¢¶W’ÂÖöF–f–W'3¢ÖöF–f–W'2’Óâ6†÷'F7WC£¥6†÷'F7WD–çWB°¢6†÷'F7WC£¥6†÷'F7WD–çWB°¢Æöv–6Åö¶W’À¢ÖöF–f–W'2À¢‡—6–6Åö¶W“¢–6VC£¦¶W–&ö&C£¦¶W“£¥‡—6–6Ã£¥Væ–FVçF–f–VB€¢–6VC£¦¶W–&ö&C£¦¶W“£¤æF—fT6öFS£¥Væ–FVçF–f–VBÀ¢’À¢Æö6F–öã¢–6VC£¦¶W–&ö&C£¤Æö6F–öã£¥7FæF&BÀ¢Ğ§Ğ ¦fâFW7Eö–çWEöWfVçB†–çWC¢6†÷'F7WC£¥6†÷'F7WD–çWB’Óâ–6VC£¤WfVçB°¢–6VC£¤WfVçC£¤¶W–&ö&B†–6VC£¦¶W–&ö&C£¤WfVçC£¤¶W•&W76VB°¢¶W“¢–çWBæÆöv–6Åö¶W’æ6ÆöæR‚’À¢ÖöF–f–VEö¶W“¢–çWBæÆöv–6Åö¶W’À¢‡—6–6Åö¶W“¢–çWBç‡—6–6Åö¶W’À¢Æö6F–öã¢–çWBæÆö6F–öâÀ¢ÖöF–f–W'3¢–çWBæÖöF–f–W'2À¢FW‡C¢æöæRÀ¢&WVC¢fÇ6RÀ¢Ò§Ğ ¢5·FW7EĞ¦fâÖ–åö¶W–&ö&E÷&W6W'fW5ö¶W—Eö–FVçF—G•÷F‡&÷Vv…÷&ö¦V7Eö7F–öç2‚’°¢W6R–6VC£¦¶W–&ö&C£§°¢Æö6F–öâÀ¢¶W“£§´6öFRÂæÖVBÂ‡—6–6ÇÒÀ¢Ó°¢ÆWBv–æF÷rÒ–6VC£§v–æF÷s£¤–C£§Væ—VR‚“°¢ÆWB×WBÒ°¢Ö–å÷v–æF÷uö–C¢6öÖR‡v–æF÷r’À¢âä£¦FVfVÇB‚¢Ó°¢¦ç6†÷'F7WEö&–æF–æw2çw&—FR‚’çVçw&‚’Ò6öÖÖæG3£§fÆ–FFUö&–æF–æw2‚d†6„Ö£¦g&öÒ…°¢‚'G&6²æFB"æ–çFò‚’Â$7G&Âµ7FæF&Cr"æ–çFò‚’’À¢‚&VF—BçVæFò"æ–çFò‚’Â$7G&Â´çVÕCr"æ–çFò‚’’À¢‚&Ö&¶W'2æFBÖ÷"ÖÖ÷fRãr"æ–çFò‚’Â7G&–æs£¦æWr‚’’À¢Ò’¢çVçw&‚“°¢ÆWB×WB–çWBÒFW7E÷6†÷'F7WEö–çWB„¶W“£¤6†&7FW"‚#r"æ–çFò‚’’ÂÖöF–f–W'3£¤5E$Â“°¢–çWBç‡—6–6Åö¶W’Ò‡—6–6Ã£¤6öFR„6öFS£¤F–v—Cr“°¢ÆWBòÒçWFFR„ÖW76vS£¥'VçF–ÖT¶W–&ö&DWfVçB€¢FW7Eö–çWEöWfVçB†–çWBæ6ÆöæR‚’’À¢–6VC£¦WfVçC£¥7FGW3£¤–væ÷&VBÀ¢v–æF÷rÀ¢’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â“°¢–çWBæÆöv–6Åö¶W’Ò¶W“£¤æÖVB„æÖVC£¤†öÖR“²òòçVÔÆö6²öf`¢–çWBç‡—6–6Åö¶W’Ò‡—6–6Ã£¤6öFR„6öFS£¤çV×Cr“°¢–çWBæÆö6F–öâÒÆö6F–öã£¤çV×C°¢ÆWBòÒçWFFR„ÖW76vS£¥'VçF–ÖT¶W–&ö&DWfVçB€¢FW7Eö–çWEöWfVçB†–çWB’À¢–6VC£¦WfVçC£¥7FGW3£¤–væ÷&VBÀ¢v–æF÷rÀ¢’“°¢76W'B†ç&ö¦V7BçG&6·2‚’æ—5öV×G’‚’“°§Ğ ¢5·FW7EĞ¦fâcö7W7FöÕö&–æF–æw5öæEöÖ–å÷v–æF÷uö6GW&U÷&V6VFUöÖVçUöæf–vF–öâ‚’°¢W6R–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC°¢ÆWBv–æF÷rÒ–6VC£§v–æF÷s£¤–C£§Væ—VR‚“°¢f÷"6†÷&B–â²$7G&Â´c"Â$ÇB´c"Â$c%Ò°¢ÆWB×WBÒ°¢Ö–å÷v–æF÷uö–C¢6öÖR‡v–æF÷r’À¢âä£¦FVfVÇB‚¢Ó°¢¦ç6†÷'F7WEö&–æF–æw2çw&—FR‚’çVçw&‚’Ğ¢6öÖÖæG3£§fÆ–FFUö&–æF–æw2‚d†6„Ö£¦g&öÒ…²‚'G&6²æFB"æ–çFò‚’Â6†÷&Bæ–çFò‚’•Ò’¢çVçw&‚“°¢ÆWBÖöF–f–W'2Ò–b6†÷&Bç7F'G5÷v—F‚‚$7G&Â"’°¢ÖöF–f–W'3£¤5E$À¢ÒVÇ6R–b6†÷&Bç7F'G5÷v—F‚‚$ÇB"’°¢ÖöF–f–W'3£¤Å@¢ÒVÇ6R°¢ÖöF–f–W'3£¤äôäP¢Ó°¢ÆWB–çWBÒFW7E÷6†÷'F7WEö–çWB„¶W“£¤æÖVB„æÖVC£¤c’ÂÖöF–f–W'2“°¢ÆWBòÒçWFFR„ÖW76vS£¥'VçF–ÖT¶W–&ö&DWfVçB€¢FW7Eö–çWEöWfVçB†–çWB’À¢–6VC£¦WfVçC£¥7FGW3£¤–væ÷&VBÀ¢v–æF÷rÀ¢’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’ÂÂ'¶6†÷&GÒ"“°¢76W'B†æ7F—fUöÖVçRæ—5öæöæR‚’“°¢Ğ¢ÆWB×WBÒ°¢Ö–å÷v–æF÷uö–C¢6öÖR‡v–æF÷r’À¢6†VÆÅ÷&öf–ÆS¢7WW#£¥6†VÆÅ&öf–ÆS£¥F÷V6‚À¢âä£¦FVfVÇB‚¢Ó°¢ÆWBòÒçWFFR„ÖW76vS£¤FE6†÷'F7WD&–æF–ær‚&VF—BçVæFò"æ–çFò‚’’“°¢ÆWB–çWBÒFW7E÷6†÷'F7WEö–çWB„¶W“£¤æÖVB„æÖVC£¤c’ÂÖöF–f–W'3£¤äôäR“°¢ÆWBòÒçWFFR„ÖW76vS£¥'VçF–ÖT¶W–&ö&DWfVçB€¢FW7Eö–çWEöWfVçB†–çWB’À¢–6VC£¦WfVçC£¥7FGW3£¤–væ÷&VBÀ¢v–æF÷rÀ¢’“°¢76W'EöW†ç6†÷'F7WEö&–æF–æuöVF—G5²&VF—BçVæFò%ÒÂ$ÖöBµ£²c"“°¢76W'B†æ7F—fUöÖVçRæ—5öæöæR‚’“°§Ğ ¢5·FW7EĞ¦fâæ'&÷uöÆ–çW…÷v–æF÷uö¶VW5öFW6·F÷÷&öf–ÆR‚’°¢ÆWBÒ°¢Ö–å÷v–æF÷u÷6—¦S¢6öÖR†–6VC£¥6—¦S£¦æWrƒC#ãÂcCã’’À¢6†VÆÅ÷&öf–ÆS¢7WW#£¥6†VÆÅ&öf–ÆS£¤FW6·F÷À¢âä£¦FVfVÇB‚¢Ó°¢76W'B‚æ—5öÖö&–ÆUöÖ–å÷v–æF÷r‚’“°§Ğ ¢5·FW7EĞ¦fâ'W7•÷&ö¦V7Eö¶VW5ö7F–öåöÆ—7Eö–çWEöf–Æ&ÆU÷v—F†÷WEöÆÆ÷v–æu÷&ö¦V7EöVF—G2‚’°¢ÆWBv–æF÷rÒ–6VC£§v–æF÷s£¤–C£§Væ—VR‚“°¢ÆWB×WBÒ°¢7F–öåöÆ—7E÷v–æF÷uö–C¢6öÖR‡v–æF÷r’À¢–×÷'Eö'W7“¢G'VRÀ¢âä£¦FVfVÇB‚¢Ó°¢æ7F–öåöÆ—7Bç6VÆV7B‚'G&6²æFB"æ–çFò‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'B†ç&ö¦V7BçG&6·2‚’æ—5öV×G’‚’“°¢æ7F–öåöÆ—7Bæ6GW&RÒ6öÖR‡7WW#£¦7F–öåöÆ—7C£¤6GW&TÖöFS£¤f–æB“°¢ÆWB–çWBÒFW7E÷6†÷'F7WEö–çWB€¢¶W“£¤æÖVB†–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤W66R’À¢ÖöF–f–W'3£¤äôäRÀ¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥'VçF–ÖT¶W–&ö&DWfVçB€¢FW7Eö–çWEöWfVçB†–çWB’À¢–6VC£¦WfVçC£¥7FGW3£¤–væ÷&VBÀ¢v–æF÷rÀ¢’“°¢76W'B†æ7F–öåöÆ—7Bæ6GW&Ræ—5öæöæR‚’“°¢76W'EöW†æ7F–öåöÆ—7E÷v–æF÷uö–BÂ6öÖR‡v–æF÷r’“°§Ğ ¢5·FW7EĞ¦fâVæ–FVçF–f–VEöÆöv–6Åö¶W•ö¶VW5ö¶æ÷våöçV×Eöf—fUöF—7F6‚‚’°¢W6R–6VC£¦¶W–&ö&C£§°¢Æö6F–öâÀ¢¶W“£§´6öFRÂ‡—6–6ÇÒÀ¢Ó°¢ÆWBv–æF÷rÒ–6VC£§v–æF÷s£¤–C£§Væ—VR‚“°¢ÆWB×WBÒ°¢Ö–å÷v–æF÷uö–C¢6öÖR‡v–æF÷r’À¢âä£¦FVfVÇB‚¢Ó°¢¦ç6†÷'F7WEö&–æF–æw2çw&—FR‚’çVçw&‚’Ò6öÖÖæG3£§fÆ–FFUö&–æF–æw2‚d†6„Ö£¦g&öÒ…²€¢'G&6²æFB"æ–çFò‚’À¢$7G&Â´çVÕCR"æ–çFò‚’À¢•Ò’¢çVçw&‚“°¢ÆWB×WB–çWBÒFW7E÷6†÷'F7WEö–çWB„¶W“£¥Væ–FVçF–f–VBÂÖöF–f–W'3£¤5E$Â“°¢–çWBç‡—6–6Åö¶W’Ò‡—6–6Ã£¤6öFR„6öFS£¤çV×CR“°¢–çWBæÆö6F–öâÒÆö6F–öã£¤çV×C°¢ÆWBòÒçWFFR„ÖW76vS£¥'VçF–ÖT¶W–&ö&DWfVçB€¢FW7Eö–çWEöWfVçB†–çWB’À¢–6VC£¦WfVçC£¥7FGW3£¤–væ÷&VBÀ¢v–æF÷rÀ¢’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â“°§Ğ ¢5·FW7EĞ¦fâ7F–öåöÆ—7EöæEö7W7FöÕöc÷&W7V7E÷VæF–æu÷6fUö6öæf—&ÖF–öâ‚’°¢ÆWBÖ–âÒ–6VC£§v–æF÷s£¤–C£§Væ—VR‚“°¢ÆWBÆ—7BÒ–6VC£§v–æF÷s£¤–C£§Væ—VR‚“°¢ÆWB×WBÒ°¢Ö–å÷v–æF÷uö–C¢6öÖR†Ö–â’À¢7F–öåöÆ—7E÷v–æF÷uö–C¢6öÖR†Æ—7B’À¢âä£¦FVfVÇB‚¢Ó°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤æWu&ö¦V7B“°¢76W'B†çVæF–æu÷&ö¦V7E÷G&ç6—F–öâæ—5÷6öÖR‚’“°¢æ7F–öåöÆ—7Bç6VÆV7B‚'G&6²æFB"æ–çFò‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ†fÇ6R’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â“°¢¦ç6†÷'F7WEö&–æF–æw2çw&—FR‚’çVçw&‚’Ğ¢6öÖÖæG3£§fÆ–FFUö&–æF–æw2‚d†6„Ö£¦g&öÒ…²‚'G&6²æFB"æ–çFò‚’Â$c"æ–çFò‚’•Ò’’çVçw&‚“°¢ÆWBWfVçBÒFW7Eö–çWEöWfVçB‡FW7E÷6†÷'F7WEö–çWB€¢¶W“£¤æÖVB†–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC£¤c’À¢ÖöF–f–W'3£¤äôäRÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¥'VçF–ÖT¶W–&ö&DWfVçB€¢WfVçBÀ¢–6VC£¦WfVçC£¥7FGW3£¤–væ÷&VBÀ¢Ö–âÀ¢’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚’æÆVâ‚’Â“°¢76W'B†çVæF–æu÷&ö¦V7E÷G&ç6—F–öâæ—5÷6öÖR‚’“°§Ğ ¢5·FW7EĞ¦fâ7F–öåöÆ—7E÷'Våö6Æ÷6Uö6Æ÷6W5öWfVå÷v†Vå÷VæFõö†5öæõöVffV7B‚’°¢ÆWBv–æF÷rÒ–6VC£§v–æF÷s£¤–C£§Væ—VR‚“°¢ÆWB×WBÒ°¢7F–öåöÆ—7E÷v–æF÷uö–C¢6öÖR‡v–æF÷r’À¢âä£¦FVfVÇB‚¢Ó°¢æ7F–öåöÆ—7Bç6VÆV7B‚&VF—BçVæFò"æ–çFò‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤7F–öäÆ—7E'Vâ‡G'VR’“°¢76W'B†æ7F–öåöÆ—7E÷v–æF÷uö–Bæ—5öæöæR‚’“°¢76W'B†ç&ö¦V7BçG&6·2‚’æ—5öV×G’‚’“°§Ğ ¢5·FW7EĞ¦fâFW6·F÷öÖ—†W%öFö6µöæEöÖVF–÷6†&U÷7FFUöæEö¶VWö–æFWVæFVçE÷&F–÷2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢76W'B†æÖVF–÷æVÅöFö6²æÖ—†W%ö÷Vâ“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWB–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥FövvÆTÖVF–'&÷w6W%æVÂ“°¢ÆWBÖVF–ÒæÖVF–÷æVÅöFö6²ç7Æ—BçVçw&‚“°¢ÆWBÖ—†W"ÒæÖVF–÷æVÅöFö6²æÖ—†W%÷7Æ—BçVçw&‚“°¢76W'EöæR†ÖVF–ÂÖ—†W"“°¢ÆWBòÒçWFFR„ÖW76vS£¤ÖVF–æVÅ&W6—¦VB†ÖVF–Âãc2’“°¢ÆWBòÒçWFFR„ÖW76vS£¤ÖVF–æVÅ&W6—¦VB†Ö—†W"ÂãC‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥FövvÆTÖ—†W%æVÂ“°¢76W'B‚æÖVF–÷æVÅöFö6²æÖ—†W%ö÷Vâ“°¢76W'EöW†æÖVF–÷æVÅöFö6²çæW2æ5÷&Vb‚’çVçw&‚’æÆVâ‚’Â"“°¢ÆWBòÒçWFFR„ÖW76vS£¥FövvÆTÖ—†W%æVÂ“°¢76W'EöW†æÖVF–÷æVÅöFö6²çæW2æ5÷&Vb‚’çVçw&‚’æÆVâ‚’Â2“°¢76W'EöW†æÖVF–÷æVÅöFö6²æÖ–å÷&F–òÂãc2“°¢76W'EöW†æÖVF–÷æVÅöFö6²æ'&ævU÷&F–òÂãC‚“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òæ–B‚’Â–B“°¢76W'B†æFW6·F÷öÆ–÷WE÷6fUöBæ—5÷6öÖR‚’“°§Ğ ¢5·FW7EĞ¦fâæWuöf7F÷'•÷6†÷'F7WG5÷&W6W'fUöW†—7F–æuö7W7FöÕöÖ5öæEöVffV7F—fUöÆ&VÇ2‚’°¢ÆWB&–æF–æw2Ğ¢6öÖÖæG3£§fÆ–FFUö&–æF–æw2‚d†6„Ö£¦g&öÒ…²‚&VF—BçVæFò"æ–çFò‚’Â$ÖöB´Ò"æ–çFò‚’•Ò’¢çVçw&‚“°¢ÆWB–çWBÒFW7E÷6†÷'F7WEö–çWB„¶W“£¤6†&7FW"‚&Ò"æ–çFò‚’’ÂÖöF–f–W'3£¤4ôÔÔäB“°¢76W'EöW€¢6öÖÖæG3£¦g&öÕ÷6†÷'F7WEö–çWB‚f–çWBÂf&–æF–æw2ÂeµÒ’À¢6öÖR„6öÖÖæD–C£¥VæFò¢“°¢76W'B†6öÖÖæG3£¦&–æF–æuöf÷%ö–B‚'f–WrçFövvÆRÖÖ—†W""Âf&–æF–æw2’æ—5öV×G’‚’“°¢76W'EöW†6öÖÖæG3£¦&–æF–æuöf÷%ö–B‚&VF—BçVæFò"Âf&–æF–æw2’Â$ÖöB´Ò"“°§Ğ ¢5·FW7EĞ¦fâ'W7•÷&ö¦V7E÷f–WuöÖVçUöæE÷6†÷'F7WE÷FövvÆUöÖ—†W%÷v—F†÷WEöVF—F–æu÷&ö¦V7B‚’°¢ÆWBÖ–âÒ–6VC£§v–æF÷s£¤–C£§Væ—VR‚“°¢ÆWB×WBÒ°¢Ö–å÷v–æF÷uö–C¢6öÖR†Ö–â’À¢–õö'W7“¢G'VRÀ¢âä£¦FVfVÇB‚¢Ó°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¥FövvÆTÖ—†W%æVÂ’“°¢76W'B‚æÖVF–÷æVÅöFö6²æÖ—†W%ö÷Vâ“°¢ÆWB–çWBÒFW7E÷6†÷'F7WEö–çWB„¶W“£¤6†&7FW"‚&Ò"æ–çFò‚’’ÂÖöF–f–W'3£¤4ôÔÔäB“°¢ÆWBòÒçWFFR„ÖW76vS£¥'VçF–ÖT¶W–&ö&DWfVçB€¢FW7Eö–çWEöWfVçB†–çWB’À¢–6VC£¦WfVçC£¥7FGW3£¤–væ÷&VBÀ¢Ö–âÀ¢’“°¢76W'B†æÖVF–÷æVÅöFö6²æÖ—†W%ö÷Vâ“°¢76W'B†ç&ö¦V7BçG&6·2‚’æ—5öV×G’‚’“°§Ğ ¢5·FW7EĞ¦fâ––VÆFVEöf7F÷'•ö&–æF–æuö—5ö6öç6—7FVçEö&WGvVVåöÆ&VÇ5öæEöF—7F6‚‚’°¢W6R–6VC£¦¶W–&ö&C£§°¢Æö6F–öâÀ¢¶W“£§´6öFRÂ‡—6–6ÇÒÀ¢Ó°¢ÆWB&–æF–æw2Ò6öÖÖæG3£§fÆ–FFUö&–æF–æw2‚d†6„Ö£¦g&öÒ…²€¢&VF—BçVæFò"æ–çFò‚’À¢%6†–gB´çVÕDF—f–FR"æ–çFò‚’À¢•Ò’¢çVçw&‚“°¢76W'B†6öÖÖæG3£¦&–æF–æuöf÷%ö–B‚&7F–öç2ç6†÷rÖÆ—7B"Âf&–æF–æw2’æ—5öV×G’‚’“°¢ÆWB×WB–çWBÒFW7E÷6†÷'F7WEö–çWB„¶W“£¤6†&7FW"‚"ò"æ–çFò‚’’ÂÖöF–f–W'3£¥4„”eB“°¢76W'EöW†6öÖÖæG3£¦g&öÕ÷6†÷'F7WEö–çWB‚f–çWBÂf&–æF–æw2ÂeµÒ’ÂæöæR“°¢–çWBç‡—6–6Åö¶W’Ò‡—6–6Ã£¤6öFR„6öFS£¤çV×DF—f–FR“°¢–çWBæÆö6F–öâÒÆö6F–öã£¤çV×C°¢76W'EöW€¢6öÖÖæG3£¦g&öÕ÷6†÷'F7WEö–çWB‚f–çWBÂf&–æF–æw2ÂeµÒ’À¢6öÖR„6öÖÖæD–C£¥VæFò¢“°§Ğ ¢5·FW7EĞ¦fâ÷&F–æ'•÷G&6µ÷&÷WF–æuö—5öf–Æ&ÆUö5öå÷VæFö&ÆUö7F–öâ‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWB6÷W&6RÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWB'W2Òç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢76W'B‚ç&ö¦V7BçG&6·2‚•³Òæ—5ö'W2‚’“° ¢ÆWBòÒçWFFR„ÖW76vS£¥6WEG&6´÷WGWB‡6÷W&6RÂ6öÖR†'W2’’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òæ÷WGWE÷G&6²‚’Â6öÖR†'W2’“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òæ÷WGWE÷G&6²‚’ÂæöæR“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òæ÷WGWE÷G&6²‚’Â6öÖR†'W2’“°§Ğ ¢5·FW7EĞ¦fâ&÷WF–æu÷6VæEöG&gG5ö6öÖÖ—EöFöÖ–6ÆÇ•öæEö–çfÆ–E÷fÇVW5÷&W6W'fU÷&ö¦V7B‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWB6÷W&6RÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWBFW7F–æF–öâÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&÷WF–æu÷G&6µö–BÒ6öÖR‡6÷W&6R“°¢ÆWBòÒçWFFR„ÖW76vS£¥&÷WF–æt6†ævR„Ft7F–öã£¤7&VFTVF–õ6VæB°¢G&6µö–C¢6÷W&6RÀ¢FW7F–æF–öâÀ¢&ÖWFW'3¢FVfVÇC£¦FVfVÇB‚’À¢Ò’“°¢ÆWB–BÒç&ö¦V7BçG&6·2‚•³Òç6VæG2‚•³Òæ–B‚“°¢ÆWB&Vf÷&RÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥&÷WF–æu6VæDG&gB†–BÂfÇ6RÂ"Ób"æ–çFò‚’’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&÷WF–æu6VæDG&gB†–BÂG'VRÂ$æâ"æ–çFò‚’’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6öÖÖ—E&÷WF–æu6VæB†–B’“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&Vf÷&R“°¢76W'B†ç&÷WF–æu÷6VæEöG&gG2æ6öçF–ç5ö¶W’‚f–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&÷WF–æu6VæDG&gB†–BÂG'VRÂ#ãR"æ–çFò‚’’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6öÖÖ—E&÷WF–æu6VæB†–B’“°¢ÆWB&ÖWFW'2Òç&ö¦V7BçG&6·2‚•³Òç6VæG2‚•³Òç&ÖWFW'2‚“°¢76W'EöW‚‡&ÖWFW'2çföÇVÖUöF"Â&ÖWFW'2çâ’Â‚ÓbãÂãR’“°¢76W'B‚ç&÷WF–æu÷6VæEöG&gG2æ6öçF–ç5ö¶W’‚f–B’“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&Vf÷&R“°¢ÆWBòÒçWFFR„ÖW76vS£¥&÷WF–æt6†ævR„Ft7F–öã£¤FVÆWFUG&6²°¢G&6µö–C¢FW7F–æF–öâÀ¢Ò’“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&Vf÷&R“°§Ğ ¢5·FW7EĞ¦fâ&V6V—fUöÆ—7E÷&VÖ÷fÅöFVÆWFW5ööæÇ•÷F†E÷6VæEöæE÷VæFõ÷&W7F÷&W5ö—B‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBf—'7E÷6VæFW"Òç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWB6V6öæE÷6VæFW"Òç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWB&V6V—fW"Òç&ö¦V7BçG&6·2‚•³%Òæ–B‚“°¢f÷"G&6µö–B–â¶f—'7E÷6VæFW"Â6V6öæE÷6VæFW%Ò°¢ÆWBòÒçWFFR„ÖW76vS£¥&÷WF–æt6†ævR„Ft7F–öã£¤7&VFTVF–õ6VæB°¢G&6µö–BÀ¢FW7F–æF–öã¢&V6V—fW"À¢&ÖWFW'3¢FVfVÇC£¦FVfVÇB‚’À¢Ò’“°¢Ğ¢ÆWBf—'7E÷6VæBÒç&ö¦V7BçG&6·2‚•³Òç6VæG2‚•³Òæ–B‚“°¢ÆWB&Vf÷&RÒç&ö¦V7Bç6æ6†÷B‚“°¢ç&÷WF–æu÷G&6µö–BÒ6öÖR‡&V6V—fW"“° ¢òòF†—2—2F†R7F–öâv—&VBFò&VÖ÷fR–âF†R&V6V—fW"w2&V6V—fRÆ—7Bà¢ÆWBòÒçWFFR„ÖW76vS£¥&÷WF–æt6†ævR„Ft7F–öã£¤FVÆWFTVF–õ6VæB°¢G&6µö–C¢f—'7E÷6VæFW"À¢6VæEö–C¢f—'7E÷6VæBÀ¢Ò’“°¢76W'B†ç&ö¦V7BçG&6·2‚•³Òç6VæG2‚’æ—5öV×G’‚’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òç6VæG2‚’æÆVâ‚’Â“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³%Òæ–B‚’Â&V6V—fW"“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&Vf÷&R“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'B†ç&ö¦V7BçG&6·2‚•³Òç6VæG2‚’æ—5öV×G’‚’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òç6VæG2‚’æÆVâ‚’Â“°§Ğ ¢5·FW7EĞ¦fâVæFõ÷6†÷'F7WE÷v÷&·5÷v†–ÆU÷F†U÷&÷WF–æu÷v–æF÷uö†5öfö7W2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWB6÷W&6RÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWBFW7F–æF–öâÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥&÷WF–æt6†ævR„Ft7F–öã£¤7&VFTVF–õ6VæB°¢G&6µö–C¢6÷W&6RÀ¢FW7F–æF–öâÀ¢&ÖWFW'3¢FVfVÇC£¦FVfVÇB‚’À¢Ò’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òç6VæG2‚’æÆVâ‚’Â“°¢ÆWBv–æF÷uö–BÒ–6VC£§v–æF÷s£¤–C£§Væ—VR‚“°¢ç&÷WF–æu÷v–æF÷uö–BÒ6öÖR‡v–æF÷uö–B“° ¢ÆWBòÒçWFFR„ÖW76vS£¥'VçF–ÖT¶W–&ö&DWfVçB€¢FW7Eö–çWEöWfVçB‡FW7E÷6†÷'F7WEö–çWB€¢¶W“£¤6†&7FW"‚'¢"æ–çFò‚’’À¢ÖöF–f–W'3£¤5E$ÂÀ¢’’À¢–6VC£¦WfVçC£¥7FGW3£¤–væ÷&VBÀ¢v–æF÷uö–BÀ¢’“° ¢76W'B†ç&ö¦V7BçG&6·2‚•³Òç6VæG2‚’æ—5öV×G’‚’“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òæ–B‚’ÂFW7F–æF–öâ“°§Ğ ¢5·FW7EĞ¦fâF÷V6…÷G&6µ÷&÷WF–æu÷W6W5÷æVÅöæf–vF–öåöæEö6Æ÷6Uö¶VW5÷&ö¦V7B‚’°¢ÆWB×WBÒ°¢6†VÆÅ÷&öf–ÆS¢7WW#£¥6†VÆÅ&öf–ÆS£¥F÷V6‚À¢âäFVfVÇC£¦FVfVÇB‚¢Ó°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWB6÷W&6RÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWB&Vf÷&RÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷VåG&6µ&÷WF–ær‡6÷W&6R’“°¢76W'EöW†æÖö&–ÆU÷æVÂÂÖö&–ÆUæVÃ£¥&÷WF–ær“°¢76W'B†ç&÷WF–æu÷v–æF÷uö–Bæ—5öæöæR‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6Æ÷6UG&6µ&÷WF–ær“°¢76W'EöW†æÖö&–ÆU÷æVÂÂÖö&–ÆUæVÃ£¤VF—F÷"“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&Vf÷&R“°§Ğ ¢5·FW7EĞ¦fâföÆFW%÷&Vv—7G'•öæE÷&÷WF–æu÷&VçEöVF—G5÷6†&U÷VæFö&ÆU÷&ö¦V7E÷7FFR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWB&VçBÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWB6†–ÆBÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB†6öÖÖæG3£¤6öÖÖæD–C£¥G&6²°¢G&6µö–C¢&VçBÀ¢6öÖÖæC¢6öÖÖæG3£¥G&6´6öÖÖæC£¥FövvÆTföÆFW"À¢Ò’“°¢76W'B†ç&ö¦V7BçG&6·2‚•³Òæ—5öföÆFW"‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&÷WF–æt6†ævR„Ft7F–öã£¥6WEG&6µ&VçB°¢G&6µö–C¢6†–ÆBÀ¢&VçC¢6öÖR‡&VçB’À¢Ò’“°¢76W'EöW†ç&ö¦V7BæföÆFW%öFWF‚†6†–ÆB’Â“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BçG&6·2‚•³Òç&VçE÷G&6²‚’ÂæöæR“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B‚ç&ö¦V7BçG&6·2‚•³Òæ—5öföÆFW"‚’“°§Ğ ¢5·FW7EĞ¦fâföÆFW%ö6ö×7E÷&Vv—7FW&VEö6öÖÖæEö6†ævW5ööæÇ•÷W'6—7FVE÷f–Wu÷7FFR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBföÆFW"Òç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥&÷WF–æt6†ævR„Ft7F–öã£¥6WEG&6´föÆFW"°¢G&6µö–C¢föÆFW"À¢Væ&ÆVC¢G'VRÀ¢Ò’“°¢ÆWB&Vf÷&RÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWBf–Wuö&Vf÷&RÒçF–ÖVÆ–æRæ'&ævVÖVçE÷f–Wu÷7FFR‚fç&ö¦V7B“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB†6öÖÖæG3£¤6öÖÖæD–C£¥G&6²°¢G&6µö–C¢föÆFW"À¢6öÖÖæC¢6öÖÖæG3£¥G&6´6öÖÖæC£¤7–6ÆTföÆFW$6ö×7BÀ¢Ò’“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&Vf÷&R“°¢76W'EöæR€¢çF–ÖVÆ–æRæ'&ævVÖVçE÷f–Wu÷7FFR‚fç&ö¦V7B’À¢f–Wuö&Vf÷&P¢“°¢76W'B†ç&Wf—6–öââ&Wf—6–öâ“°§Ğ ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ğ¢5·FW7EĞ¦fâG&ç7÷'Eöf7F÷'•ö&–æF–æw5÷6W&FU÷7F÷öæE÷W6UöæE÷&W6W'fUö7W7FöÕöÖ2‚’°¢W6R–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC°¢f÷"†¶W’ÂÖöF–f–W'2ÂW‡V7FVB’–â°¢„æÖVC£¥76RÂÖöF–f–W'3£¤äôäRÂ6öÖÖæD–C£¥FövvÆUÆ•7F÷’À¢„æÖVC£¤VçFW"ÂÖöF–f–W'3£¤äôäRÂ6öÖÖæD–C£¥FövvÆUÆ–&6²’À¢„æÖVC£¥76RÂÖöF–f–W'3£¤5E$ÂÂ6öÖÖæD–C£¥FövvÆUÆ–&6²’À¢Ò°¢76W'EöW€¢6öÖÖæG3£¦g&öÕ÷6†÷'F7WB‚d¶W“£¤æÖVB†¶W’’ÂÖöF–f–W'2Âd†6„Ö£¦æWr‚’ÂeµÒ’À¢6öÖR†W‡V7FVB¢“°¢Ğ¢76W'EöW€¢6öÖÖæG3£¦g&öÕ÷6†÷'F7WB€¢d¶W“£¤æÖVB„æÖVC£¥76R’À¢ÖöF–f–W'3£¥4„”eBÀ¢d†6„Ö£¦æWr‚’À¢eµĞ¢’À¢æöæP¢“°¢ÆWB7W7FöÒÒ6öÖÖæG3£§fÆ–FFUö&–æF–æw2‚d†6„Ö£¦g&öÒ…°¢‚'G&ç7÷'BçFövvÆR×Æ–&6²"æ–çFò‚’Â%76R"æ–çFò‚’’À¢‚'G&ç7÷'Bç7F÷×Æ–&6²"æ–çFò‚’Â%6†–gBµ76R"æ–çFò‚’’À¢Ò’¢çVçw&‚“°¢76W'EöW€¢6öÖÖæG3£¦g&öÕ÷6†÷'F7WB‚d¶W“£¤æÖVB„æÖVC£¥76R’ÂÖöF–f–W'3£¤äôäRÂf7W7FöÒÂeµÒ’À¢6öÖR„6öÖÖæD–C£¥FövvÆUÆ–&6²¢“°¢76W'EöW€¢6öÖÖæG3£¦g&öÕ÷6†÷'F7WB‚d¶W“£¤æÖVB„æÖVC£¥76R’ÂÖöF–f–W'3£¥4„”eBÂf7W7FöÒÂeµÒ’À¢6öÖR„6öÖÖæD–C£¥7F÷Æ–&6²¢“°¢76W'EöW†6öÖÖæG3£¦&–æF–æuöf÷%ö–B‚'G&ç7÷'BçÆ’×7F÷"Âf7W7FöÒ’Â""“°§Ğ ¢5¶6fr†fVGW&RÒ&VF–òÖFWf–6R"•Ğ¢5·FW7EĞ¦fâG&ç7÷'E÷7F÷ö6öÖÖæG5ö6æ6VÅ÷&V6÷&F–æu÷6WGW÷F‡&÷Vv…ö'W7•öwV&G2‚’°¢f÷"6öÖÖæB–â´6öÖÖæD–C£¥FövvÆUÆ•7F÷Â6öÖÖæD–C£¥7F÷Æ–&6µÒ°¢ÆWB×WBÒ°¢&V6÷&F–æu÷7F'F–æs¢G'VRÀ¢Æ–&6µö'W7“¢G'VRÀ¢âä£¦FVfVÇB‚¢Ó°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB†6öÖÖæB’“°¢76W'B†ç&V6÷&F–æuö6æ6VÅ÷&WVW7FVBÂ'¶6öÖÖæC£÷Ò"“°¢76W'B†ç7FGW2æ6öçF–ç2‚$6æ6VÆÆ–ær–çWB6WGW"’“°¢76W'B†ç&ö¦V7BçG&6·2‚’æ—5öV×G’‚’“°¢Ğ§Ğ ¢5·FW7EĞ¦fâG&6µ÷†6Uö6öÖÖæEöæE÷VæFõ÷6†&UöFöÖ–å÷7FFR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6²Òç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç6VÆV7E÷G&6µööæÇ’‡G&6²“°¢ÆWB6öÖÖæBÒ6öÖÖæD–C£¥6VÆV7FVEG&6²†6öÖÖæG3£¥G&6´6öÖÖæC£¥FövvÆU†6R“°¢76W'EöW€¢6öÖÖæG3£§7F&ÆUö–B†6öÖÖæB’æ5öFW&Vb‚’À¢6öÖR‚'G&6²çFövvÆR×†6R"¢“°¢76W'EöW†6öÖÖæG3£§FövvÆU÷7FFR‚fÂ6öÖÖæB’Â6öÖR†fÇ6R’“°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB†6öÖÖæB’“°¢76W'B†ç&ö¦V7BçG&6·2‚•³Òæ—5÷†6Uö–çfW'FVB‚’“°¢76W'EöW†6öÖÖæG3£§FövvÆU÷7FFR‚fÂ6öÖÖæB’Â6öÖR‡G'VR’“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B‚ç&ö¦V7BçG&6·2‚•³Òæ—5÷†6Uö–çfW'FVB‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'B†ç&ö¦V7BçG&6·2‚•³Òæ—5÷†6Uö–çfW'FVB‚’“°§Ğ ¢5·FW7EĞ¦fâ—FVÕöfFU÷&Wf–Wuö6öÖÖ—G5ööæ6UöæEö6æ6VÅ÷&W6W'fW5÷&ö¦V7E÷&Wf—6–öâ‚’°¢W6R7&FS£§F–ÖVÆ–æS£¥F–ÖVÆ–æTWfVçC°¢W6RFuö6÷&S£§´VF–ôfFRÂVF–ô—FVÔfFW2ÂfFU6†WÓ°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢$VF–ò"æ–çFò‚’À¢Ò¢çVçw&‚“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢òö6öç7FçB"æ–çFò‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò¢çVçw&‚“°¢ÆWB—FVÕö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWB÷&–v–æÂÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBf—'7BÒVF–ô—FVÔfFW2°¢fFUö–ã¢VF–ôfFS£¦æWrƒ%óãRÂfFU6†S£¥6Öö÷F‚’çVçw&‚’À¢âäVF–ô—FVÔfFW3£¦FVfVÇB‚¢Ó°¢ÆWBÆ7BÒVF–ô—FVÔfFW2°¢fFUö–ã¢VF–ôfFS£¦æWrƒ#Eóã#RÂfFU6†S£¥6Öö÷F‚’çVçw&‚’À¢âæf—'7@¢Ó°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR…F–ÖVÆ–æTWfVçC£¥&Wf–Wt—FVÔfFW2°¢—FVÕö–BÀ¢fFW3¢f—'7BÀ¢Ò’“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR…F–ÖVÆ–æTWfVçC£¥&Wf–Wt—FVÔfFW2°¢—FVÕö–BÀ¢fFW3¢Æ7BÀ¢Ò’“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â÷&–v–æÂ“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR…F–ÖVÆ–æTWfVçC£¤6öÖÖ—D—FVÔfFW2’“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒæfFW2‚’ÂÆ7B“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â÷&–v–æÂ“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒæfFW2‚’ÂÆ7B“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR…F–ÖVÆ–æTWfVçC£¥&Wf–Wt—FVÔfFW2°¢—FVÕö–BÀ¢fFW3¢f—'7BÀ¢Ò’“°¢ÆWBòÒçWFFR„ÖW76vS£¤W66R“°¢76W'B†çF–ÖVÆ–æRæ—FVÕöfFU÷&Wf–Wr‚’æ—5öæöæR‚’“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒæfFW2‚’ÂÆ7B“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR…F–ÖVÆ–æTWfVçC£¤6öÖÖ—D—FVÔfFW2’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ“°§Ğ ¢5·FW7EĞ¦fâ7W'fUöÖVçUö6öçfW'G5öÆVv7•÷6Öö÷F…÷v—F†÷WEö6†æv–æuöÆVæwF‡5öæEö—5÷VæFö&ÆR‚’°¢W6R7&FS£§F–ÖVÆ–æS£§´—FVÔfFTVFvRÂF–ÖVÆ–æTWfVçGÓ°¢W6RFuö6÷&S£§´VF–ôfFRÂVF–ô—FVÔfFW2ÂfFT7W'fRÂfFU6†WÓ°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢$VF–ò"æ–çFò‚’À¢Ò¢çVçw&‚“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢òö6öç7FçB"æ–çFò‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò¢çVçw&‚“°¢ÆWB—FVÕö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWB÷&–v–æÂÒVF–ô—FVÔfFW2°¢fFUö–ã¢VF–ôfFS£¦æWrƒ%óã#RÂfFU6†S£¥6Öö÷F‚’çVçw&‚’À¢fFUö÷WC¢VF–ôfFS£¦æWrƒeóãRÂfFU6†S£¤f7E7F'B’çVçw&‚’À¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDVF–ô—FVÔfFW2°¢—FVÕö–BÀ¢fFW3¢÷&–v–æÂÀ¢Ò¢çVçw&‚“°¢ÆWBÖW76vRÒÖW76vS£¥F–ÖVÆ–æR…F–ÖVÆ–æTWfVçC£¥6WDfFT7W'fU&W6WB°¢—FVÕö–BÀ¢VFvS¢—FVÔfFTVFvS£¤–âÀ¢6†S¢fFU6†S£¥6Öö÷F‚À¢Ò“°¢ÆWBòÒçWFFR†ÖW76vRæ6ÆöæR‚’“°¢ÆWB7W'&VçBÒç&ö¦V7BæVF–õö—FV×2‚•³ÒæfFW2‚“°¢76W'EöW†7W'&VçBæfFUö–âæÆVæwF…÷6×ÆW2‚’Â%óã#R“°¢76W'EöW€¢7W'&VçBæfFUö–âæ7W'fR‚’À¢fFT7W'fS£¦7W'&VçE÷&W6WB„fFU6†S£¥6Öö÷F‚¢“°¢76W'EöW†7W'&VçBæfFUö÷WBÂ÷&–v–æÂæfFUö÷WB“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR†ÖW76vR“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒæfFW2‚’Â÷&–v–æÂ“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒæfFW2‚’Â7W'&VçB“°¢ÆWBòÒçWFFR„ÖW76vS£¥F–ÖVÆ–æR…F–ÖVÆ–æTWfVçC£¤÷VäfFTÖVçR°¢—FVÕö–BÀ¢VFvS¢—FVÔfFTVFvS£¤÷WBÀ¢ƒ¢ãÀ¢“¢ãÀ¢Ò’“°¢ÆWBòÒçWFFR„ÖW76vS£¤W66R“°¢76W'B†çF–ÖVÆ–æRæ6öçFW‡EöfFRæ—5öæöæR‚’“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒæfFW2‚’Â7W'&VçB“°§Ğ ¢5·FW7EĞ¦fâ—FVÕ÷&÷W'F–W5÷7FvUöfFW5÷&W6W'fU÷&V6—6–öåöæEöÇ•ööæUö†—7F÷'•öVçG'’‚’°¢W6R7WW#£¦—FVÕ÷&÷W'F–W3£§´—FVÕ&÷W'F–W2Â—FVÕ&÷W'G”f–VÆGÓ°¢W6RFuö6÷&S£§´VF–ôfFRÂVF–ô—FVÔfFW2ÂfFT7W'fRÂfFU6†WÓ°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢$VF–ò"æ–çFò‚’À¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢ç&ö¦V7BçG&6·2‚•³Òæ–B‚’À¢ÖVF–÷&Vc¢&76WC¢òö÷væVB"æ–çFò‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢CƒÀ¢Ò¢çVçw&‚“°¢ÆWB–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWB÷&–v–æÂÒVF–ô—FVÔfFW2°¢fFUö–ã¢VF–ôfFS£¦æWrƒ#ã#RÂfFU6†S£¥6Öö÷F‚’çVçw&‚’À¢fFUö÷WC¢VF–ôfFS£¦æWrƒcãRÂfFU6†S£¤f7E7F'B’çVçw&‚’À¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDVF–ô—FVÔfFW2°¢—FVÕö–C¢–BÀ¢fFW3¢÷&–v–æÂÀ¢Ò¢çVçw&‚“°¢æ—FVÕ÷&÷W'F–W2Ò6öÖR„—FVÕ&÷W'F–W3£¦æWr†–BÂ÷&–v–æÂÂãÂãÂCƒ’“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”—FVÕ&÷W'F–W2†fÇ6R’“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒæfFW2‚’Â÷&–v–æÂ“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G”f–VÆD6†ævVB€¢—FVÕ&÷W'G”f–VÆC£¤–ä7W'fGW&RÀ¢"Ó""æ–çFò‚’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G”f–VÆD6†ævVB€¢—FVÕ&÷W'G”f–VÆC£¤÷WDÆVæwF‚À¢#£ã#SR"æ–çFò‚’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G”f–VÆD6†ævVB€¢—FVÕ&÷W'G”f–VÆC£¥F¶UföÇVÖRÀ¢"Óbã&D""æ–çFò‚’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G”f–VÆD6†ævVB€¢—FVÕ&÷W'G”f–VÆC£¥F¶UâÀ¢##RTÂ"æ–çFò‚’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G•F¶U&FT6†ævVB‚#ã#R"æ–çFò‚’’“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒæfFW2‚’Â÷&–v–æÂ“°¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”—FVÕ&÷W'F–W2†fÇ6R’“°¢ÆWBÆ–VBÒç&ö¦V7BæVF–õö—FV×2‚•³ÒæfFW2‚“°¢76W'EöW†Æ–VBæfFUö–âæÆVæwF…÷6×ÆW2‚’Â#ã#R“°¢76W'B€¢ÖF6†W2†Æ–VBæfFUö–âæ7W'fR‚’ÂfFT7W'fS£¤æF—fR‡’–bæ7W'fGW&R‚’ÓÒÓãbbç5÷&ÖWFW"‚’ÓÒãR¢“°¢76W'EöW†Æ–VBæfFUö÷WBæÆVæwF…÷6×ÆW2‚’Âc#Bã“°¢76W'EöW†Æ–VBæfFUö÷WBæ7W'fR‚’Â÷&–v–æÂæfFUö÷WBæ7W'fR‚’“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒçföÇVÖR‚’Âã“°¢76W'B‚†ç&ö¦V7BæVF–õö—FV×2‚•³ÒçF¶U÷föÇVÖR‚’ÒãR’æ'2‚’Âã“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒçF¶U÷â‚’ÂÓã#R“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒçF¶U÷Æ–&6µ÷&FR‚’Âã#R“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒæfFW2‚’Â÷&–v–æÂ“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒçF¶U÷föÇVÖR‚’Âã“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒçF¶U÷â‚’Âã“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒçF¶U÷Æ–&6µ÷&FR‚’Âã“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒæfFW2‚’ÂÆ–VB“°¢76W'B‚†ç&ö¦V7BæVF–õö—FV×2‚•³ÒçF¶U÷föÇVÖR‚’ÒãR’æ'2‚’Âã“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒçF¶U÷â‚’ÂÓã#R“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒçF¶U÷Æ–&6µ÷&FR‚’Âã#R“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G”f–VÆD6†ævVB€¢—FVÕ&÷W'G”f–VÆC£¤–äÆVæwF‚À¢$æâ"æ–çFò‚’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”—FVÕ&÷W'F–W2‡G'VR’“°¢76W'B†æ—FVÕ÷&÷W'F–W2æ5÷&Vb‚’çVçw&‚’æW'&÷"æ—5÷6öÖR‚’“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒæfFW2‚’ÂÆ–VB“°¢ÆWBòÒçWFFR„ÖW76vS£¤6Æ÷6T—FVÕ&÷W'F–W2“°¢76W'B†æ—FVÕ÷&÷W'F–W2æ—5öæöæR‚’“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒæfFW2‚’ÂÆ–VB“°¢ÆWB&÷F‚ÒVF–ô—FVÔfFW2°¢fFUö–ã¢VF–ôfFS£¦æWrƒ“#ãÂfFU6†S£¤f7E7F'B’çVçw&‚’À¢fFUö÷WC¢VF–ôfFS£¦æWrƒ#ƒƒãÂfFU6†S£¤f7E7F'B’çVçw&‚’À¢Ó°¢ÆWB×WBG&gBÒ—FVÕ&÷W'F–W3£¦æWr†–BÂ&÷F‚ÂãÂãÂCƒ“°¢G&gBæVF—B„—FVÕ&÷W'G”f–VÆC£¤–äÆVæwF‚Â#£ãƒ"æ–çFò‚’“°¢ÆWBF§W7FVBÒG&gBæfFW2†&÷F‚ÂCƒÂCƒ’çVçw&‚“°¢76W'EöW†F§W7FVBæfFUö–âæÆVæwF…÷6×ÆW2‚’Â3ƒCã“°¢76W'EöW†F§W7FVBæfFUö÷WBæÆVæwF…÷6×ÆW2‚’Â“cã“°¢G&gBæVF—B„—FVÕ&÷W'G”f–VÆC£¤÷WDÆVæwF‚Â#£"ã"æ–çFò‚’“°¢ÆWBF§W7FVBÒG&gBæfFW2†&÷F‚ÂCƒÂCƒ’çVçw&‚“°¢76W'EöW†F§W7FVBæfFUö–âæÆVæwF…÷6×ÆW2‚’Âã“°¢76W'EöW†F§W7FVBæfFUö÷WBæÆVæwF…÷6×ÆW2‚’ÂCƒã“°¢G&gBæVF—B„—FVÕ&÷W'G”f–VÆC£¤–äÆVæwF‚Â#£ãc"æ–çFò‚’“°¢G&gBæVF—B„—FVÕ&÷W'G”f–VÆC£¤÷WDÆVæwF‚Â#£ãc"æ–çFò‚’“°¢ÆWBF§W7FVBÒG&gBæfFW2†&÷F‚ÂCƒÂCƒ’çVçw&‚“°¢76W'EöW†F§W7FVBæfFUö–âæÆVæwF…÷6×ÆW2‚’Â“#ã“°¢76W'EöW†F§W7FVBæfFUö÷WBæÆVæwF…÷6×ÆW2‚’Â#ƒƒã“°§Ğ ¢5·FW7EĞ¦fâ—FVÕ÷&÷W'F–W5÷&VgW6U÷F¶U÷&FUö6†ævW5÷v†–ÆUöVF–õö÷WGWEö—5ö7F—fR‚’°¢W6R7WW#£¦—FVÕ÷&÷W'F–W3£§´—FVÕ&÷W'F–W2ÂF–ÖUVæ—GÓ° ¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢$VF–ò"æ–çFò‚’À¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢ç&ö¦V7BçG&6·2‚•³Òæ–B‚’À¢ÖVF–÷&Vc¢&76WC¢òö7F—fRÖ÷WGWB×F¶R×&FR"æ–çFò‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢C…óÀ¢Ò¢çVçw&‚“°¢ÆWB—FVÒÒfç&ö¦V7BæVF–õö—FV×2‚•³Ó°¢ÆWB×WBG&gBÒ—FVÕ&÷W'F–W3£¦g&öÕö—FVÒ†—FVÒÂC…óÂF–ÖUVæ—C£¥F–ÖR“°¢G&gBç6WE÷F¶U÷&FR‚#ã#R"æ–çFò‚’“°¢æ—FVÕ÷&÷W'F–W2Ò6öÖR†G&gB“°¢æ—FVÕ÷&÷W'F–W5övVæW&F–öâÒç&ö¦V7EövVæW&F–öã°¢ÆWB&Vf÷&RÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã° ¢ÆWBòÒæÇ•ö—FVÕ÷&÷W'F–W5÷v—F…÷Æ–&6²†fÇ6RÂG'VR“° ¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â&Vf÷&R“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒçF¶U÷Æ–&6µ÷&FR‚’Âã“°¢76W'EöW€¢æ—FVÕ÷&÷W'F–W2æ5÷&Vb‚’çVçw&‚’æW'&÷"æ5öFW&Vb‚’À¢6öÖR‚$6Æ÷6RVF–ò÷WGWB&Vf÷&RVF—F–ær—FVÒÆ6VÖVçBÂ6÷W&6RÆö÷–ærÂ÷"F¶R&FR"¢“°§Ğ ¢5·FW7EĞ¦fâ—FVÕ÷&÷W'F–W5÷FövvÆUöÆö÷÷6÷W&6U÷&W6W'fW5÷6V7F–öåöæE÷VæFöW2‚’°¢W6R7WW#£¦—FVÕ÷&÷W'F–W3£§´—FVÕ&÷W'F–W2ÂF–ÖUVæ—GÓ° ¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢$VF–ò"æ–çFò‚’À¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÆö÷VDVF–ô—FVÒ°¢G&6µö–C¢ç&ö¦V7BçG&6·2‚•³Òæ–B‚’À¢ÖVF–÷&Vc¢&76WC¢òö—FVÒ×&÷W'F–W2ÖÆö÷"æ–çFò‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢#À¢ÆVæwF…÷6×ÆW3¢EóƒÀ¢×WFVC¢fÇ6RÀ¢†6Uö–çfW'FVC¢fÇ6RÀ¢6÷W&6U÷6V7F–öå÷7F'E÷6×ÆW3¢À¢6÷W&6U÷6V7F–öåöÆVæwF…÷6×ÆW3¢#CÀ¢Ò¢çVçw&‚“°¢ÆWB—FVÒÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ6ÆöæR‚“°¢ÆWB×WBG&gBÒ—FVÕ&÷W'F–W3£¦g&öÕö—FVÒ‚f—FVÒÂC…óÂF–ÖUVæ—C£¥6×ÆW2“°¢G&gBç6WEöÆö÷÷6÷W&6R†fÇ6R“°¢æ—FVÕ÷&÷W'F–W2Ò6öÖR†G&gB“° ¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”—FVÕ&÷W'F–W2†fÇ6R’“°¢ÆWB—FVÒÒfç&ö¦V7BæVF–õö—FV×2‚•³Ó°¢76W'B‚—FVÒæ—5öÆö÷÷6÷W&6R‚’“°¢76W'EöW†—FVÒç6÷W&6U÷6V7F–öå÷7F'E÷6×ÆW2‚’Â6öÖRƒ’“°¢76W'EöW†—FVÒç6÷W&6U÷6V7F–öåöÆVæwF…÷6×ÆW2‚’Â6öÖRƒ#C’“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5öÆö÷÷6÷W&6R‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'B‚ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5öÆö÷÷6÷W&6R‚’“°§Ğ ¢5·FW7EĞ¦fâ—FVÕ÷&÷W'F–W5ö×WFUöæE÷†6UöÇ•öFöÖ–6ÆÇ•÷v—F…÷VæFò‚’°¢W6R7WW#£¦—FVÕ÷&÷W'F–W3£§´—FVÕ&÷W'F–W2ÂF–ÖUVæ—GÓ° ¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢$VF–ò"æ–çFò‚’À¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢ç&ö¦V7BçG&6·2‚•³Òæ–B‚’À¢ÖVF–÷&Vc¢&76WC¢òö—FVÒ×&÷W'F–W2Ö×WFR"æ–çFò‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢EóƒÀ¢Ò¢çVçw&‚“°¢ÆWB—FVÒÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ6ÆöæR‚“°¢ÆWB×WBG&gBÒ—FVÕ&÷W'F–W3£¦g&öÕö—FVÒ‚f—FVÒÂC…óÂF–ÖUVæ—C£¥6×ÆW2“°¢G&gBç6WEö×WFVB‡G'VR“°¢æ—FVÕ÷&÷W'F–W2Ò6öÖR†G&gB“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G•†6T6†ævVB‡G'VR’“° ¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”—FVÕ&÷W'F–W2†fÇ6R’“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5÷†6Uö–çfW'FVB‚’“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒçföÇVÖR‚’Âã“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B‚ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'B‚ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5÷†6Uö–çfW'FVB‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5÷†6Uö–çfW'FVB‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G•†6T6†ævVB†fÇ6R’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6Æ÷6T—FVÕ&÷W'F–W2“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5÷†6Uö–çfW'FVB‚’“°§Ğ ¢5·FW7EĞ¦fâVF–õö—FVÕ÷&÷W'F–W5÷&VæÖUöæE÷&W7F÷&U÷F†U÷6÷W&6UöÆ&VÅö–åööæU÷VæFõ÷7FW‚’°¢W6R7WW#£¦—FVÕ÷&÷W'F–W3£§´—FVÕ&÷W'F–W2ÂF–ÖUVæ—GÓ° ¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢$VF–ò"æ–çFò‚’À¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢ç&ö¦V7BçG&6·2‚•³Òæ–B‚’À¢ÖVF–÷&Vc¢&76WC¢òö—FVÒ×&÷W'F–W2ÖæÖR"æ–çFò‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢EóƒÀ¢Ò¢çVçw&‚“°¢ÆWB—FVÒÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ6ÆöæR‚“°¢ÆWB×WBG&gBÒ—FVÕ&÷W'F–W3£¦g&öÕö—FVÒ‚f—FVÒÂC…óÂF–ÖUVæ—C£¥6×ÆW2“°¢G&gBç6WEöæÖR‚$¶–6²6Æ÷6R"çFõö÷væVB‚’“°¢æ—FVÕ÷&÷W'F–W2Ò6öÖR†G&gB“°¢ÆWB–æ—F–Å÷&Wf—6–öâÒç&Wf—6–öã° ¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”—FVÕ&÷W'F–W2†fÇ6R’“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒææÖR‚’Â$¶–6²6Æ÷6R"“°¢76W'EöW†ç&Wf—6–öâÂ–æ—F–Å÷&Wf—6–öâ²“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒææÖR‚’Â""“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒææÖR‚’Â$¶–6²6Æ÷6R"“° ¢ÆWB×WBG&gBĞ¢—FVÕ&÷W'F–W3£¦g&öÕö—FVÒ‚fç&ö¦V7BæVF–õö—FV×2‚•³ÒÂC…óÂF–ÖUVæ—C£¥6×ÆW2“°¢G&gBç6WEöæÖR…7G&–æs£¦æWr‚’“°¢æ—FVÕ÷&÷W'F–W2Ò6öÖR†G&gB“°¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”—FVÕ&÷W'F–W2†fÇ6R’“°¢76W'EöW†ç&ö¦V7BæVF–õö—FV×2‚•³ÒææÖR‚’Â""“°§Ğ ¢5·FW7EĞ¦fâÖ–F•ö—FVÕ÷&÷W'F–W5÷F¶U÷6VÆV7F÷%öÆ—7G5÷F¶W5öæE÷7v—F6†W5÷v—F…÷VæFõ÷&VFò‚’°¢W6R7WW#£¦—FVÕ÷&÷W'F–W3£¥F–ÖUVæ—C° ¢ÆWB×WBÒ°¢—FVÕ÷&÷W'F–W5÷Væ—C¢F–ÖUVæ—C£¥F–ÖRÀ¢âä£¦FVfVÇB‚¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢$Ô”D’F¶W2"æ–çFò‚’À¢Ò¢çVçw&‚“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢5óƒCÀ¢Ò¢çVçw&‚“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2°¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢À¢GW&F–öã¢CƒÀ¢fVÆö6—G“¢À¢öfe÷fVÆö6—G“¢À¢ÒÀ¢Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cBÀ¢F–6³¢“cÀ¢GW&F–öã¢CƒÀ¢fVÆö6—G“¢“"À¢öfe÷fVÆö6—G“¢À¢ÒÀ¢ÒÀ¢Ò¢çVçw&‚“°¢ÆWB×WB6æ6†÷BÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWB—FVÒÒf×WB6æ6†÷BæÖ–F•ö—FV×5³Ó°¢ÆWBF¶UöÒ—FVÒææ÷FW5³Òæ6ÆöæR‚“°¢ÆWBF¶Uö"Ò—FVÒææ÷FW5³Òæ6ÆöæR‚“°¢—FVÒææ÷FW2ÒfV2·F¶Uö%Ó°¢—FVÒææÖRÒ$Ô”D’F¶R""æ–çFò‚“°¢—FVÒæ7F—fU÷F¶Uö–æFW‚Ò°¢—FVÒæ÷F†W%÷F¶W2ÒfV2´Ö–F”—FVÕF¶U6æ6†÷B°¢–æFWƒ¢À¢æÖS¢$Ô”D’F¶R"æ–çFò‚’À¢6÷W&6Uööfg6WE÷F–6·3¢#À¢Æö÷÷6÷W&6S¢G'VRÀ¢6÷W&6UöÆö÷öÆVæwF…÷F–6·3¢“cÀ¢æ÷FW3¢fV2·F¶UöÒÀ¢6öçG&öÆÆW'3¢fV3£¦æWr‚’À¢—F6…ö&VæG3¢fV3£¦æWr‚’À¢&öw&Õö6†ævW3¢fV3£¦æWr‚’À¢öÇ•ögFW'F÷V6†W3¢fV3£¦æWr‚’À¢6†ææVÅ÷&W77W&W3¢fV3£¦æWr‚’À¢÷VUöWfVçG3¢fV3£¦æWr‚’À¢WfVçEö÷&FW#¢fV3£¦æWr‚’À¢ÕÓ°¢ç&ö¦V7BÒ&ö¦V7C£¦g&öÕ÷6æ6†÷B‡6æ6†÷B’çVçw&‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÒ6öÖR†—FVÕö–B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ–ç6W'B†—FVÕö–B“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷Vä—FVÕ&÷W'F–W2“° ¢ÆWBG&gBÒæÖ–F•ö—FVÕ÷&÷W'F–W2æ5÷&Vb‚’çVçw&‚“°¢76W'EöW†G&gBæ7F—fU÷F¶Ræ–æFW‚Â“°¢76W'EöW€¢G&g@¢çF¶Uö6†ö–6W0¢æ—FW"‚¢æÖ…Fõ7G&–æs£§Fõ÷7G&–ær¢æ6öÆÆV7C££ÅfV3Åóãâ‚’À¢²#¢Ô”D’F¶R"Â##¢Ô”D’F¶R"%Ğ¢“°¢ÆWB7W'6÷"Òç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G•F¶T6†ævVBƒ“’’“°¢76W'EöW†ç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚’Â7W'6÷"“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G•F¶T6†ævVBƒ’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ7F—fU÷F¶Uö–æFW‚‚’Â“°¢76W'EöW€¢æÖ–F•ö—FVÕ÷&÷W'F–W2æ5÷&Vb‚’çVçw&‚’æ7F—fU÷F¶Ræ–æFW‚À¢ ¢“°¢76W'EöW€¢æÖ–F•ö—FVÕ÷&÷W'F–W2æ5÷&Vb‚’çVçw&‚’ææÖRÀ¢$Ô”D’F¶R ¢“°¢76W'B†æÖ–F•ö—FVÕ÷&÷W'F–W2æ5÷&Vb‚’çVçw&‚’æÆö÷÷6÷W&6R“°¢76W'EöW€¢æÖ–F•ö—FVÕ÷&÷W'F–W0¢æ5÷&Vb‚¢çVçw&‚¢ç6÷W&6UöÆö÷öÆVæwF‚À¢#“c ¢“°¢76W'EöW†ç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚’Â7W'6÷"“° ¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”—FVÕ&÷W'F–W2†fÇ6R’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ7F—fU÷F¶Uö–æFW‚‚’Â“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒææÖR‚’Â$Ô”D’F¶R"“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Òç—F6‚‚’Âc“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç6÷W&6Uööfg6WE÷F–6·2‚’Â#“°¢76W'B†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ—5öÆö÷÷6÷W&6R‚’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç6÷W&6UöÆö÷öÆVæwF…÷F–6·2‚’Â“c“°¢76W'EöW†ç&ö¦V7Bæ†—7F÷'•ö7W'6÷"‚’Â7W'6÷"²“°¢76W'EöW€¢æÖ–F•ö—FVÕ÷&÷W'F–W2æ5÷&Vb‚’çVçw&‚’æ7F—fU÷F¶Ræ–æFW‚À¢ ¢“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ7F—fU÷F¶Uö–æFW‚‚’Â“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Òç—F6‚‚’ÂcB“°¢76W'EöW€¢æÖ–F•ö—FVÕ÷&÷W'F–W2æ5÷&Vb‚’çVçw&‚’æ7F—fU÷F¶Ræ–æFW‚À¢¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ7F—fU÷F¶Uö–æFW‚‚’Â“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚•³Òç—F6‚‚’Âc“°¢76W'EöW€¢æÖ–F•ö—FVÕ÷&÷W'F–W2æ5÷&Vb‚’çVçw&‚’æ7F—fU÷F¶Ræ–æFW‚À¢ ¢“°§Ğ ¢5·FW7EĞ¦fâÖ–F•ö—FVÕ÷&÷W'F–W5ö÷Vç5öÆ–W5ö×WFUöæEö¶VW5öæ÷FW5÷VæFö&ÆR‚’°¢W6R7WW#£¦—FVÕ÷&÷W'F–W3£§´—FVÕ&÷W'G”f–VÆBÂF–ÖUVæ—GÓ° ¢ÆWB×WBÒ°¢—FVÕ÷&÷W'F–W5÷Væ—C¢F–ÖUVæ—C£¥F–ÖRÀ¢âä£¦FVfVÇB‚¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢$Ô”D’"æ–çFò‚’À¢Ò¢çVçw&‚“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢çVçw&‚“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2´Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢À¢GW&F–öã¢CƒÀ¢fVÆö6—G“¢À¢öfe÷fVÆö6—G“¢À¢ÕÒÀ¢Ò¢çVçw&‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÒ6öÖR†—FVÕö–B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ–ç6W'B†—FVÕö–B“° ¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæD–C£¤÷Vä—FVÕ&÷W'F–W2’“°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB‚fÂ6öÖÖæD–C£¥FövvÆT—FVÕ&÷W'F–W2’“°¢ÆWBòÒçWFFR„ÖW76vS£¥FövvÆT—FVÕ&÷W'F–W2“°¢76W'B†æ—FVÕ÷&÷W'F–W2æ—5öæöæR‚’“°¢76W'B†æÖ–F•ö—FVÕ÷&÷W'F–W2æ—5÷6öÖR‚’“°¢76W'B†æ—FVÕ÷&÷W'F–W5÷v–æF÷uö–Bæ—5÷6öÖR‚’“° ¢ÆWBW‡V7FVE÷7F'BÒ ¢ç&ö¦V7@¢çF–6µöE÷6×ÆU÷÷6—F–öâ†ç&ö¦V7Bç6×ÆUöE÷&ö¦V7E÷F–ÖRƒã’çVçw&‚’2ccB¢çVçw&‚¢ç&÷VæB‚’2ScC°¢ÆWBW‡V7FVEöVæBÒ ¢ç&ö¦V7@¢çF–6µöE÷6×ÆU÷÷6—F–öâ†ç&ö¦V7Bç6×ÆUöE÷&ö¦V7E÷F–ÖRƒãsR’çVçw&‚’2ccB¢çVçw&‚¢ç&÷VæB‚’2ScC°¢ÆWBW‡V7FVE÷6÷W&6Uööfg6WBÒ ¢ç&ö¦V7@¢çF–6µöE÷6×ÆU÷÷6—F–öâ†ç&ö¦V7Bç6×ÆUöE÷&ö¦V7E÷F–ÖRƒã#R’çVçw&‚’2ccB¢çVçw&‚¢ç&÷VæB‚’2ScC°¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G”f–VÆD6†ævVB€¢—FVÕ&÷W'G”f–VÆC£¥÷6—F–öâÀ¢#£ã"æ–çFò‚’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G”f–VÆD6†ævVB€¢—FVÕ&÷W'G”f–VÆC£¤ÆVæwF‚À¢#£ãsS"æ–çFò‚’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G”f–VÆD6†ævVB€¢—FVÕ&÷W'G”f–VÆC£¥6÷W&6Töfg6WBÀ¢#£ã#S"æ–çFò‚’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G”×WFT6†ævVB‡G'VR’“°¢ÆWBòÒçWFFR„ÖW76vS£¤Ö–F”—FVÕ&÷W'G”æÖT6†ævVB‚""æ–çFò‚’’“°¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”—FVÕ&÷W'F–W2†fÇ6R’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒææÖR‚’Â$Ô”D’"“°¢76W'B‚ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’Â“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒæÆVæwF…÷F–6·2‚’Â“c“°¢76W'B†æÖ–F•ö—FVÕ÷&÷W'F–W2æ5÷&Vb‚’çVçw&‚’æW'&÷"æ—5÷6öÖR‚’“° ¢ÆWBòÒçWFFR„ÖW76vS£¤Ö–F”—FVÕ&÷W'G”æÖT6†ævVB‚$–çG&ò"æ–çFò‚’’“°¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”—FVÕ&÷W'F–W2†fÇ6R’“°¢76W'B†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒææÖR‚’Â$–çG&ò"“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’ÂW‡V7FVE÷7F'B“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒæÆVæwF…÷F–6·2‚’À¢W‡V7FVEöVæBÒW‡V7FVE÷7F'@¢“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç6÷W&6Uööfg6WE÷F–6·2‚’À¢W‡V7FVE÷6÷W&6Uööfg6W@¢“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚’æÆVâ‚’Â“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B‚ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’Â“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒæÆVæwF…÷F–6·2‚’Â“c“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç6÷W&6Uööfg6WE÷F–6·2‚’Â“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒææÖR‚’Â$Ô”D’"“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òææ÷FW2‚’æÆVâ‚’Â“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'B†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷F–6²‚’ÂW‡V7FVE÷7F'B“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒæÆVæwF…÷F–6·2‚’À¢W‡V7FVEöVæBÒW‡V7FVE÷7F'@¢“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç6÷W&6Uööfg6WE÷F–6·2‚’À¢W‡V7FVE÷6÷W&6Uööfg6W@¢“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒææÖR‚’Â$–çG&ò"“° ¢ÆWBòÒçWFFR„ÖW76vS£¤Ö–F”—FVÕ&÷W'G”æÖT6†ævVB‚%VæÆ–VB"æ–çFò‚’’“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G”×WFT6†ævVB†fÇ6R’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6Æ÷6T—FVÕ&÷W'F–W2“°¢76W'B†æÖ–F•ö—FVÕ÷&÷W'F–W2æ—5öæöæR‚’“°¢76W'B†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒææÖR‚’Â$–çG&ò"“°§Ğ ¢5·FW7EĞ¦fâÖ–F•ö—FVÕ÷&÷W'F–W5öÇ•÷&W6W'fW5ög&7F–öæÅ÷÷÷6—F–öåöæE÷VæFöW2‚’°¢W6R7WW#£¦—FVÕ÷&÷W'F–W3£§´—FVÕ&÷W'G”f–VÆBÂF–ÖUVæ—GÓ° ¢ÆWB×WBÒ°¢—FVÕ÷&÷W'F–W5÷Væ—C¢F–ÖUVæ—C£¥6×ÆW2À¢âä£¦FVfVÇB‚¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢$Ô”D’"æ–çFò‚’À¢Ò¢çVçw&‚“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢5óƒCÀ¢ÆVæwF…÷F–6·3¢ó“#À¢Ò¢çVçw&‚“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ÆWB÷&–v–æÅ÷÷6—F–öâÒÖ–F”—FVÕ÷6—F–öã£¦æWrƒ5óƒCÂc’çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F”—FVÕ÷6—F–öâ°¢—FVÕö–BÀ¢÷6—F–öã¢÷&–v–æÅ÷÷6—F–öâÀ¢Ò¢çVçw&‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÒ6öÖR†—FVÕö–B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ–ç6W'B†—FVÕö–B“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã° ¢ÆWBòÒçWFFR„ÖW76vS£¥FövvÆT—FVÕ&÷W'F–W2“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G”f–VÆD6†ævVB€¢—FVÕ&÷W'G”f–VÆC£¥÷6—F–öâÀ¢#“cS#"æ–çFò‚’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”—FVÕ&÷W'F–W2†fÇ6R’“° ¢ÆWBÖ÷fVE÷÷6—F–öâÒÖ–F”—FVÕ÷6—F–öã£¦æWrƒ5óƒcÂc’çVçw&‚“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷÷6—F–öâ‚’ÂÖ÷fVE÷÷6—F–öâ“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷÷6—F–öâ‚’À¢÷&–v–æÅ÷÷6—F–öà¢“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç7F'E÷÷6—F–öâ‚’ÂÖ÷fVE÷÷6—F–öâ“°§Ğ ¢5·FW7EĞ¦fâÖ–F•ö—FVÕ÷&÷W'F–W5÷FövvÆUöÆö÷÷6÷W&6U÷&W6W'fW5÷6÷W&6UöÆVæwF…öæE÷VæFöW2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢$Ô”D’Æö÷6÷W&6R"æ–çFò‚’À¢Ò¢çVçw&‚“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢çVçw&‚“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤VF—DÖ–F”—FVÕÆ6VÖVçB°¢—FVÕö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢ó“#À¢6÷W&6Uööfg6WE÷F–6·3¢À¢Ò¢çVçw&‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÒ6öÖR†—FVÕö–B“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FV×2æ–ç6W'B†—FVÕö–B“° ¢ÆWBòÒçWFFR„ÖW76vS£¥FövvÆT—FVÕ&÷W'F–W2“°¢76W'B†æÖ–F•ö—FVÕ÷&÷W'F–W2æ—5÷6öÖR‚’“°¢76W'B†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ—5öÆö÷÷6÷W&6R‚’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç6÷W&6UöÆö÷öÆVæwF…÷F–6·2‚’Â“c“° ¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G”Æö÷6÷W&6T6†ævVB†fÇ6R’“°¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”—FVÕ&÷W'F–W2†fÇ6R’“° ¢ÆWB—FVÒÒfç&ö¦V7BæÖ–F•ö—FV×2‚•³Ó°¢76W'B‚—FVÒæ—5öÆö÷÷6÷W&6R‚’“°¢76W'EöW†—FVÒç6÷W&6UöÆö÷öÆVæwF…÷F–6·2‚’Â“c“°¢76W'EöW†—FVÒæÆVæwF…÷F–6·2‚’Âó“#“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ—5öÆö÷÷6÷W&6R‚’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç6÷W&6UöÆö÷öÆVæwF…÷F–6·2‚’Â“c“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'B‚ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ—5öÆö÷÷6÷W&6R‚’“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç6÷W&6UöÆö÷öÆVæwF…÷F–6·2‚’Â“c“°§Ğ ¢5·FW7EĞ¦fâÖ–F•ö—FVÕ÷&÷W'F–W5öVF—EöÆö÷÷6÷W&6UöÆVæwF…÷&W6W'fW5öWfVçG5öæE÷VæFöW2‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢$Ô”D’Æö÷6÷W&6R"æ–çFò‚’À¢Ò¢çVçw&‚“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢çVçw&‚“°¢ÆWB—FVÕö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤FDÖ–F”æ÷FW2°¢—FVÕö–BÀ¢æ÷FW3¢fV2´Ö–F”æ÷FTFF°¢6†ææVÃ¢À¢—F6ƒ¢cÀ¢F–6³¢#À¢GW&F–öã¢#CÀ¢fVÆö6—G“¢“bÀ¢öfe÷fVÆö6—G“¢À¢ÕÒÀ¢Ò¢çVçw&‚“°¢ÆWB÷&–v–æÂÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ6ÆöæR‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2²†—FVÕö–BÂG&6µö–B•Ò“° ¢ÆWBòÒçWFFR„ÖW76vS£¥FövvÆT—FVÕ&÷W'F–W2“°¢76W'EöW€¢æÖ–F•ö—FVÕ÷&÷W'F–W0¢æ5÷&Vb‚¢çVçw&‚¢ç6÷W&6UöÆö÷öÆVæwF‚À¢#“c ¢“°¢ÆWBòÒçWFFR„ÖW76vS£¤Ö–F”—FVÕ&÷W'G•6÷W&6TÆö÷ÆVæwF„6†ævVB€¢#“#"æ–çFò‚’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”—FVÕ&÷W'F–W2†fÇ6R’“° ¢ÆWBW‡FVæFVBÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ6ÆöæR‚“°¢76W'EöW†W‡FVæFVBç6÷W&6UöÆö÷öÆVæwF…÷F–6·2‚’Âó“#“°¢76W'EöW†W‡FVæFVBæÆVæwF…÷F–6·2‚’Â÷&–v–æÂæÆVæwF…÷F–6·2‚’“°¢76W'EöW†W‡FVæFVBææ÷FW2‚’Â÷&–v–æÂææ÷FW2‚’“°¢76W'EöW†çF–ÖVÆ–æRç6VÆV7FVEö—FV×2Â¶—FVÕö–EÒæ–çFò‚’“°¢76W'B†æÖ–F•ö—FVÕ÷&÷W'F–W2æ5÷&Vb‚’çVçw&‚’æW'&÷"æ—5öæöæR‚’“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒÂ÷&–v–æÂ“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7BæÖ–F•ö—FV×2‚•³ÒÂW‡FVæFVB“° ¢ÆWBòÒçWFFR„ÖW76vS£¤Ö–F”—FVÕ&÷W'G•6÷W&6TÆö÷ÆVæwF„6†ævVB€¢##"æ–çFò‚’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”—FVÕ&÷W'F–W2†fÇ6R’“°¢76W'EöW€¢ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òç6÷W&6UöÆö÷öÆVæwF…÷F–6·2‚’À¢ó“# ¢“°¢76W'EöW€¢æÖ–F•ö—FVÕ÷&÷W'F–W2æ5÷&Vb‚’çVçw&‚’æW'&÷"æ5öFW&Vb‚’À¢6öÖR‚%6÷W&6RÆVæwF‚6ææ÷BVæB&Vf÷&R7F÷&VBÔ”D’WfVçG2"¢“°§Ğ ¢5·FW7EĞ¦fâ—FVÕ÷FövvÆUö×WFUö7F–öåöÖF6†W5ö7F–öåöÆ—7EöæEö—5÷VæFö&ÆR‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢$VF–ò"æ–çFò‚’À¢Ò¢çVçw&‚“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢òö—FVÒÖ7F–öâÖ×WFR"æ–çFò‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢EóƒÀ¢Ò¢çVçw&‚“°¢ÆWB—FVÕö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2²†—FVÕö–BÂG&6µö–B•Ò“°¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB€¢fÀ¢6öÖÖæD–C£¥FövvÆT×WFU6VÆV7FVD—FV×0¢’“° ¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¥FövvÆT×WFU6VÆV7FVD—FV×2’“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B‚ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5ö×WFVB‚’“°§Ğ ¢5·FW7EĞ¦fâ—FVÕ÷FövvÆUö×WFUö7F–öåö†æFÆW5öÖ—†VEöVF–õöæEöÖ–F•÷6VÆV7F–öåö–åööæU÷7FW‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢òöÖ—†VBÖ—FVÒÖ×WFR"æ–çFò‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢EóƒÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢çVçw&‚“°¢ÆWBVF–õö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBÖ–F•ö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æP¢ç6VÆV7Eö—FV×2‡fV2²†VF–õö–BÂG&6µö–B’Â†Ö–F•ö–BÂG&6µö–B•Ò“° ¢76W'B†6öÖÖæG3£¦—5öVæ&ÆVB€¢fÀ¢6öÖÖæD–C£¥FövvÆT×WFU6VÆV7FVD—FV×0¢’“°¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤W†V7WFT6öÖÖæB„6öÖÖæD–C£¥FövvÆT×WFU6VÆV7FVD—FV×2’“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'B†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B‚ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'B‚ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'B†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ—5ö×WFVB‚’“°§Ğ ¢5·FW7EĞ¦fâ—FVÕ÷&÷W'F–W5ö&F6…ö×WFUöVF—G5öÖ—†VE÷6VÆV7F–öåö–åööæU÷VæFõ÷7FW‚’°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢òö&F6‚Ö—FVÒ×&÷W'F–W2"æ–çFò‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢EóƒÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢òö&F6‚Ö—FVÒ×&÷W'F–W2"æ–çFò‚’À¢7F'E÷6×ÆS¢EóƒÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢EóƒÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢çVçw&‚“°¢ÆWBVF–õö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWB6V6öæEöVF–õö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBÖ–F•ö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDVF–ô—FVÔ×WFR°¢—FVÕö–C¢VF–õö–BÀ¢×WFVC¢G'VRÀ¢Ò¢çVçw&‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æRç6VÆV7Eö—FV×2‡fV2°¢†VF–õö–BÂG&6µö–B’À¢‡6V6öæEöVF–õö–BÂG&6µö–B’À¢†Ö–F•ö–BÂG&6µö–B’À¢Ò“° ¢ÆWBòÒçWFFR„ÖW76vS£¤÷Vä—FVÕ&÷W'F–W2“°¢ÆWBG&gBÒ ¢æ&F6…ö—FVÕ÷&÷W'F–W0¢æ5÷&Vb‚¢æW‡V7B‚&×VÇF—ÆR6VÆV7FVB—FV×26†÷VÆB÷Vâ&F6‚&÷W'F–W2"“°¢76W'EöW†G&gBæ—FVÕö–G2æÆVâ‚’Â2“°¢76W'B†G&gBæ—FVÕö–G2æ6öçF–ç2‚fVF–õö–B’“°¢76W'B†G&gBæ—FVÕö–G2æ6öçF–ç2‚g6V6öæEöVF–õö–B’“°¢76W'B†G&gBæ—FVÕö–G2æ6öçF–ç2‚fÖ–F•ö–B’“°¢76W'EöW€¢G&gBæ×WFVBÂæöæRÀ¢&F–ffW&VçB×WFRfÇVW26†÷VÆBF—7Æ’Ö—†VB ¢“°¢76W'B†æ—FVÕ÷&÷W'F–W2æ—5öæöæR‚’“°¢76W'B†æÖ–F•ö—FVÕ÷&÷W'F–W2æ—5öæöæR‚’“° ¢æ–õö'W7’ÒG'VS°¢ÆWBòÒçWFFR„ÖW76vS£¤&F6„—FVÕ&÷W'G”×WFT6†ævVB‡G'VR’“°¢76W'EöW€¢æ&F6…ö—FVÕ÷&÷W'F–W2æ5÷&Vb‚’çVçw&‚’æ×WFVBÀ¢6öÖR‡G'VR’À¢'&÷W'G’G&gG2&VÖ–âVF—F&ÆRv†–ÆR&ö¦V7B’ôò—2'W7’ ¢“°¢æ–õö'W7’ÒfÇ6S° ¢ÆWB&Wf—6–öâÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”—FVÕ&÷W'F–W2†fÇ6R’“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'B†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öâ²“°¢76W'B€¢æ&F6…ö—FVÕ÷&÷W'F–W0¢æ5÷&Vb‚¢æW‡V7B‚$Ç’6†÷VÆB¶VWF†R&F6‚F–Æör÷Vâ"¢æ†5ö6†ævW2‚¢“° ¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'B‚ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'B‚ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'B†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ—5ö×WFVB‚’“° ¢ÆWB&Wf—6–öåö&Vf÷&Uö6æ6VÂÒç&Wf—6–öã°¢ÆWBòÒçWFFR„ÖW76vS£¤&F6„—FVÕ&÷W'G”×WFT6†ævVB†fÇ6R’“°¢ÆWBòÒçWFFR„ÖW76vS£¤6Æ÷6T—FVÕ&÷W'F–W2“°¢76W'B†æ&F6…ö—FVÕ÷&÷W'F–W2æ—5öæöæR‚’“°¢76W'EöW†ç&Wf—6–öâÂ&Wf—6–öåö&Vf÷&Uö6æ6VÂ“°¢76W'B†ç&ö¦V7BæVF–õö—FV×2‚’æ—FW"‚’æÆÂ‡Æ—FV×Â—FVÒæ—5ö×WFVB‚’’“°¢76W'B†ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ—5ö×WFVB‚’“°§Ğ ¢5·FW7EĞ¦fâ—FVÕ÷&÷W'F–W5ö&F6…ö×WFUööæÇ•ö&Æö6·5ö6†ævVEöÖ–F•ö—FV×5öGW&–æu÷Æ–&6²‚’°¢fâ6VÆV7FVEö—FV×2†VF–õö×WFVC¢&ööÂÂÖ–F•ö×WFVC¢&ööÂ’Óâ°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ÆWBòÒçWFFR„ÖW76vS£¤FEG&6²“°¢ÆWBG&6µö–BÒç&ö¦V7BçG&6·2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–BÀ¢ÖVF–÷&Vc¢&76WC¢òö&F6‚Ö—FVÒ×Æ–&6²"æ–çFò‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢EóƒÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DÖ–F”—FVÒ°¢G&6µö–BÀ¢7F'E÷F–6³¢À¢ÆVæwF…÷F–6·3¢“cÀ¢Ò¢çVçw&‚“°¢ÆWBVF–õö–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWBÖ–F•ö–BÒç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ–B‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDVF–ô—FVÔ×WFR°¢—FVÕö–C¢VF–õö–BÀ¢×WFVC¢VF–õö×WFVBÀ¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDÖ–F”—FVÔ×WFR°¢—FVÕö–C¢Ö–F•ö–BÀ¢×WFVC¢Ö–F•ö×WFVBÀ¢Ò¢çVçw&‚“°¢çF–ÖVÆ–æRç&V'V–ÆB‚fç&ö¦V7B“°¢çF–ÖVÆ–æP¢ç6VÆV7Eö—FV×2‡fV2²†VF–õö–BÂG&6µö–B’Â†Ö–F•ö–BÂG&6µö–B•Ò“°¢ÆWBòÒçWFFR„ÖW76vS£¤÷Vä—FVÕ&÷W'F–W2“°¢ ¢Ğ ¢ÆWB×WBæõö÷Ò6VÆV7FVEö—FV×2†fÇ6RÂfÇ6R“°¢76W'B†æõö÷æ&F6…ö—FVÕ÷&÷W'F–W2æ5÷&Vb‚’çVçw&‚’æ×WFVBÓÒ6öÖR†fÇ6R’“°¢ÆWB&Wf—6–öâÒæõö÷ç&Wf—6–öã°¢ÆWBòÒæõö÷æÇ•ö&F6…ö—FVÕ÷&÷W'F–W5÷v—F…÷Æ–&6²‡G'VRÂG'VR“°¢76W'B†æõö÷æ&F6…ö—FVÕ÷&÷W'F–W2æ—5öæöæR‚’“°¢76W'EöW†æõö÷ç&Wf—6–öâÂ&Wf—6–öâ“° ¢ÆWB×WBVF–õööæÇ’Ò6VÆV7FVEö—FV×2†fÇ6RÂG'VR“°¢ÆWBòÒVF–õööæÇ’çWFFR„ÖW76vS£¤&F6„—FVÕ&÷W'G”×WFT6†ævVB‡G'VR’“°¢ÆWB&Wf—6–öâÒVF–õööæÇ’ç&Wf—6–öã°¢ÆWBòÒVF–õööæÇ’æÇ•ö&F6…ö—FVÕ÷&÷W'F–W5÷v—F…÷Æ–&6²†fÇ6RÂG'VR“°¢76W'B†VF–õööæÇ’ç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'B†VF–õööæÇ’ç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'EöW†VF–õööæÇ’ç&Wf—6–öâÂ&Wf—6–öâ²“° ¢ÆWB×WBÖ–F•ö6†ævRÒ6VÆV7FVEö—FV×2†fÇ6RÂfÇ6R“°¢ÆWBòÒÖ–F•ö6†ævRçWFFR„ÖW76vS£¤&F6„—FVÕ&÷W'G”×WFT6†ævVB‡G'VR’“°¢ÆWB&Wf—6–öâÒÖ–F•ö6†ævRç&Wf—6–öã°¢ÆWBòÒÖ–F•ö6†ævRæÇ•ö&F6…ö—FVÕ÷&÷W'F–W5÷v—F…÷Æ–&6²†fÇ6RÂG'VR“°¢76W'B‚Ö–F•ö6†ævRç&ö¦V7BæVF–õö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'B‚Ö–F•ö6†ævRç&ö¦V7BæÖ–F•ö—FV×2‚•³Òæ—5ö×WFVB‚’“°¢76W'EöW†Ö–F•ö6†ævRç&Wf—6–öâÂ&Wf—6–öâ“°¢76W'B€¢Ö–F•ö6†ævP¢æ&F6…ö—FVÕ÷&÷W'F–W0¢æ5÷&Vb‚¢çVçw&‚¢æW'&÷ ¢æ—5÷6öÖR‚¢“°§Ğ ¢5·FW7EĞ¦fâ—FVÕ÷&÷W'F–W5ö6ææ÷EöÇ•ööG&gE÷Fõöæ÷F†W%÷&ö¦V7EövVæW&F–öâ‚’°¢W6R7WW#£¦—FVÕ÷&÷W'F–W3£§´—FVÕ&÷W'F–W2Â—FVÕ&÷W'G”f–VÆGÓ°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢$VF–ò"æ–çFò‚’À¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢ç&ö¦V7BçG&6·2‚•³Òæ–B‚’À¢ÖVF–÷&Vc¢&76WC¢òö÷væVB"æ–çFò‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢CƒÀ¢Ò¢çVçw&‚“°¢ÆWB—FVÒÒfç&ö¦V7BæVF–õö—FV×2‚•³Ó°¢æ—FVÕ÷&÷W'F–W2Ò6öÖR„—FVÕ&÷W'F–W3£¦æWr€¢—FVÒæ–B‚’À¢—FVÒæfFW2‚’À¢—FVÒçF¶U÷föÇVÖR‚’À¢—FVÒçF¶U÷â‚’À¢CƒÀ¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G”f–VÆD6†ævVB€¢—FVÕ&÷W'G”f–VÆC£¤–äÆVæwF‚À¢#ã#R"æ–çFò‚’À¢’“°¢ç&ö¦V7EövVæW&F–öâ³Ò°¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”—FVÕ&÷W'F–W2†fÇ6R’“°¢76W'B†æ—FVÕ÷&÷W'F–W2æ—5öæöæR‚’“°¢76W'EöW€¢ç&ö¦V7BæVF–õö—FV×2‚•³ÒæfFW2‚’À¢Fuö6÷&S£¤VF–ô—FVÔfFW3£¦FVfVÇB‚¢“°§Ğ ¢5·FW7EĞ¦fâf7F÷'•öc%÷FövvÆW5ö—FVÕ÷&÷W'F–W5öæE÷&W7V7G5ö7W7FöÕö&–æF–ær‚’°¢W6R–6VC£¦¶W–&ö&C£¦¶W“£¤æÖVC°¢76W'EöW€¢6öÖÖæG3£¦g&öÕ÷6†÷'F7WB€¢d¶W“£¤æÖVB„æÖVC£¤c"’À¢ÖöF–f–W'3£¤äôäRÀ¢d†6„Ö£¦æWr‚’À¢eµĞ¢’À¢6öÖR„6öÖÖæD–C£¥FövvÆT—FVÕ&÷W'F–W2¢“°¢ÆWB7W7FöÒÒ†6„Ö£¦g&öÒ…²‚'G&6²æFB"çFõö÷væVB‚’Â$c""çFõö÷væVB‚’•Ò“°¢76W'EöW€¢6öÖÖæG3£¦g&öÕ÷6†÷'F7WB‚d¶W“£¤æÖVB„æÖVC£¤c"’ÂÖöF–f–W'3£¤äôäRÂf7W7FöÒÂeµÒ’À¢6öÖR„6öÖÖæD–C£¤FEG&6²¢“°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢$VF–ò"æ–çFò‚’À¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢ç&ö¦V7BçG&6·2‚•³Òæ–B‚’À¢ÖVF–÷&Vc¢&76WC¢òö÷væVB"æ–çFò‚’À¢7F'E÷6×ÆS¢À¢6÷W&6Uööfg6WE÷6×ÆW3¢À¢ÆVæwF…÷6×ÆW3¢CƒÀ¢Ò¢çVçw&‚“°¢çF–ÖVÆ–æRç6VÆV7FVEö—FVÒÒ6öÖR†ç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚’“°¢ÆWBòÒçWFFR„ÖW76vS£¥FövvÆT—FVÕ&÷W'F–W2“°¢76W'B†æ—FVÕ÷&÷W'F–W5÷v–æF÷uö–Bæ—5÷6öÖR‚’“°¢ÆWBv–æF÷uö–BÒæ—FVÕ÷&÷W'F–W5÷v–æF÷uö–BçVçw&‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥'VçF–ÖT¶W–&ö&DWfVçB€¢–6VC£¤WfVçC£¤¶W–&ö&B†–6VC£¦¶W–&ö&C£¤WfVçC£¤¶W•&W76VB°¢¶W“¢¶W“£¤æÖVB„æÖVC£¤c"’À¢ÖöF–f–VEö¶W“¢¶W“£¤æÖVB„æÖVC£¤c"’À¢‡—6–6Åö¶W“¢–6VC£¦¶W–&ö&C£¦¶W“£¥‡—6–6Ã£¤6öFR†–6VC£¦¶W–&ö&C£¦¶W“£¤6öFS£¤c"’À¢Æö6F–öã¢–6VC£¦¶W–&ö&C£¤Æö6F–öã£¥7FæF&BÀ¢ÖöF–f–W'3¢ÖöF–f–W'3£¤äôäRÀ¢FW‡C¢æöæRÀ¢&WVC¢fÇ6RÀ¢Ò’À¢–6VC£¦WfVçC£¥7FGW3£¤–væ÷&VBÀ¢v–æF÷uö–BÀ¢’“°¢76W'B†æ—FVÕ÷&÷W'F–W5÷v–æF÷uö–Bæ—5öæöæR‚’“°¢76W'B†æ—FVÕ÷&÷W'F–W2æ—5öæöæR‚’“°§Ğ ¢5·FW7EĞ¦fâ—FVÕ÷&÷W'F–W5öÇ•÷Æ6VÖVçEöæEöfFW5öFöÖ–6ÆÇ•öæEö¶VWö÷fW'6—¦VE÷&WVW7G5ööå÷6†÷'FVæ–ær‚§°¢W6R7WW#£¦—FVÕ÷&÷W'F–W3£§´—FVÕ&÷W'F–W2Â—FVÕ&÷W'G”f–VÆB2f–VÆGÓ°¢W6RFuö6÷&S£§´VF–ôfFRÂVF–ô—FVÔfFW2ÂfFU6†WÓ°¢ÆWB×WBÒ£¦FVfVÇB‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤7&VFUG&6²°¢–æFWƒ¢À¢æÖS¢$VF–ò"æ–çFò‚’À¢Ò¢çVçw&‚“°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¤–ç6W'DVF–ô—FVÒ°¢G&6µö–C¢ç&ö¦V7BçG&6·2‚•³Òæ–B‚’À¢ÖVF–÷&Vc¢&76WC¢òö÷væVB"æ–çFò‚’À¢7F'E÷6×ÆS¢rÀ¢6÷W&6Uööfg6WE÷6×ÆW3¢#2À¢ÆVæwF…÷6×ÆW3¢CƒÀ¢Ò¢çVçw&‚“°¢ÆWB–BÒç&ö¦V7BæVF–õö—FV×2‚•³Òæ–B‚“°¢ÆWB÷&–v–æÅöfFW2ÒVF–ô—FVÔfFW2°¢fFUö–ã¢VF–ôfFS£¦æWrƒ3ƒCãÂfFU6†S£¥6Öö÷F‚’çVçw&‚’À¢fFUö÷WC¢VF–ôfFS£¦æWrƒ“cãÂfFU6†S£¥6Æ÷u7F'B’çVçw&‚’À¢Ó°¢ç&ö¦V7@¢æÇ’„Ft7F–öã£¥6WDVF–ô—FVÔfFW2°¢—FVÕö–C¢–BÀ¢fFW3¢÷&–v–æÅöfFW2À¢Ò¢çVçw&‚“°¢ÆWB÷&–v–æÂÒç&ö¦V7Bç6æ6†÷B‚“°¢æ—FVÕ÷&÷W'F–W2Ò6öÖR„—FVÕ&÷W'F–W3£¦g&öÕö—FVÒ€¢fç&ö¦V7BæVF–õö—FV×2‚•³ÒÀ¢CƒÀ¢7WW#£¦—FVÕ÷&÷W'F–W3£¥F–ÖUVæ—C£¥F–ÖRÀ¢’“°¢f÷"†f–VÆBÂfÇVR’–â°¢„f–VÆC£¥÷6—F–öâÂ#£ãS"’À¢„f–VÆC£¤ÆVæwF‚Â#£ã#S"’À¢„f–VÆC£¥6÷W&6Töfg6WBÂ#£ã#R"’À¢„f–VÆC£¤–ä7W'fGW&RÂ#ã#R"’À¢Ò°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G”f–VÆD6†ævVB†f–VÆBÂfÇVRæ–çFò‚’’“°¢Ğ¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â÷&–v–æÂ“°¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”—FVÕ&÷W'F–W2†fÇ6R’“°¢ÆWB—FVÒÒfç&ö¦V7BæVF–õö—FV×2‚•³Ó°¢76W'EöW€¢€¢—FVÒç7F'E÷6×ÆR‚’À¢—FVÒæÆVæwF…÷6×ÆW2‚’À¢—FVÒç6÷W&6Uööfg6WE÷6×ÆW2‚¢’À¢ƒ#CÂ#Âc¢“°¢76W'EöW†—FVÒæfFW2‚’æfFUö–âæÆVæwF…÷6×ÆW2‚’Â3ƒCã“°¢76W'EöW†—FVÒæfFW2‚’æfFUö÷WBÂ÷&–v–æÅöfFW2æfFUö÷WB“°¢76W'EöW†—FVÒæfFW2‚’æfFUö–âæ7W'fR‚’ç&ÖWFW'2‚’æ7W'fGW&R‚’Âã#R“°¢76W'EöW†—FVÒæfFW2‚’æfFUö–âæ7W'fR‚’ç&ÖWFW'2‚’ç5÷&ÖWFW"‚’ÂãR“°¢ÆWBÆ–VBÒç&ö¦V7Bç6æ6†÷B‚“°¢ÆWBòÒçWFFR„ÖW76vS£¥VæFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’Â÷&–v–æÂ“°¢ÆWBòÒçWFFR„ÖW76vS£¥&VFò“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’ÂÆ–VB“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G”f–VÆD6†ævVB€¢f–VÆC£¤ÆVæwF‚À¢#£ã"æ–çFò‚’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G”f–VÆD6†ævVB€¢f–VÆC£¤÷WDÆVæwF‚À¢#£ã"æ–çFò‚’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”—FVÕ&÷W'F–W2†fÇ6R’“°¢76W'B†æ—FVÕ÷&÷W'F–W2æ5÷&Vb‚’çVçw&‚’æW'&÷"æ—5÷6öÖR‚’“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’ÂÆ–VB“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G”f–VÆD6†ævVB€¢f–VÆC£¤ÆVæwF‚À¢#S"æ–çFò‚’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”—FVÕ&÷W'F–W2†fÇ6R’“°¢76W'EöW†ç&ö¦V7Bç6æ6†÷B‚’ÂÆ–VB“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G”f–VÆD6†ævVB€¢f–VÆC£¤ÆVæwF‚À¢#£ã#S"æ–çFò‚’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤—FVÕ&÷W'G”f–VÆD6†ævVB€¢f–VÆC£¤÷WDÆVæwF‚À¢#£ãS"æ–çFò‚’À¢’“°¢ÆWBòÒçWFFR„ÖW76vS£¤Ç”—FVÕ&÷W'F–W2†fÇ6R’“°¢ÆWB—FVÒÒfç&ö¦V7BæVF–õö—FV×2‚•³Ó°¢76W'EöW†—FVÒæfFW2‚’æfFUö÷WBæÆVæwF…÷6×ÆW2‚’Âs#ã“°¢76W'EöW†—FVÒæfFW2‚’æfFUö–âæÆVæwF…÷6×ÆW2‚’ÂCƒã“°§Ğ 