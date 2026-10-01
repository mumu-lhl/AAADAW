#[cfg(feature = "jack-backend")]
use super::prepare_project_playback_file;
use super::project_io::{load_project_file, save_project_file};
use super::{
    App, MainMenu, Message, PathPickerTarget, WorkspacePage, keyboard_shortcut_event,
    shortcut_message,
};
use aaadaw_core::{DawAction, MidiNoteData, Project};
#[cfg(feature = "jack-backend")]
use aaadaw_storage::ProjectStore;
use iced::keyboard::{Key, Modifiers};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEST_FILE: AtomicU64 = AtomicU64::new(0);

#[test]
fn default_workspace_is_arrangement() {
    let app = App::default();
    assert_eq!(app.active_workspace, WorkspacePage::Arrangement);
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
fn top_menus_toggle_and_workspace_navigation_stays_available_during_jobs() {
    let mut app = App {
        audio_asset_management_busy: true,
        ..App::default()
    };
    let _ = app.update(Message::ToggleMainMenu(MainMenu::File));
    assert_eq!(app.active_menu, Some(MainMenu::File));
    let _ = app.update(Message::SelectWorkspace(WorkspacePage::Media));
    assert_eq!(app.active_workspace, WorkspacePage::Media);
    let _ = app.update(Message::SelectWorkspace(WorkspacePage::Project));
    assert_eq!(app.active_workspace, WorkspacePage::Project);
    let _ = app.update(Message::ToggleMainMenu(MainMenu::File));
    assert_eq!(app.active_menu, None);
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
fn keyboard_shortcuts_route_to_existing_app_messages() {
    assert!(matches!(
        shortcut_message(Key::Character("z"), Modifiers::COMMAND),
        Some(Message::Undo)
    ));
    assert!(matches!(
        shortcut_message(Key::Character("z"), Modifiers::COMMAND | Modifiers::SHIFT),
        Some(Message::Redo)
    ));
    assert!(matches!(
        shortcut_message(Key::Character("s"), Modifiers::COMMAND),
        Some(Message::SaveProject)
    ));
    assert!(matches!(
        shortcut_message(Key::Character("o"), Modifiers::COMMAND),
        Some(Message::OpenProject)
    ));
    assert!(shortcut_message(Key::Character("x"), Modifiers::COMMAND).is_none());
    let undo_event = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: Key::Character("z".into()),
        modified_key: Key::Character("z".into()),
        physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::KeyZ),
        location: iced::keyboard::Location::Standard,
        modifiers: Modifiers::COMMAND,
        text: None,
        repeat: false,
    });
    assert!(
        keyboard_shortcut_event(
            undo_event.clone(),
            iced::event::Status::Captured,
            iced::window::Id::unique()
        )
        .is_none()
    );
    assert!(matches!(
        keyboard_shortcut_event(
            undo_event,
            iced::event::Status::Ignored,
            iced::window::Id::unique()
        ),
        Some(Message::Undo)
    ));
    #[cfg(feature = "jack-backend")]
    assert!(matches!(
        shortcut_message(
            Key::Named(iced::keyboard::key::Named::Space),
            Modifiers::NONE
        ),
        Some(Message::TogglePlayback)
    ));
    #[cfg(not(feature = "jack-backend"))]
    assert!(
        shortcut_message(
            Key::Named(iced::keyboard::key::Named::Space),
            Modifiers::NONE
        )
        .is_none()
    );
}

#[test]
fn track_controls_and_undo_change_project_only_through_actions() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();

    let _ = app.update(Message::ToggleMute(track_id));
    let _ = app.update(Message::AdjustVolume(track_id, -3.0));
    assert!(app.project.tracks()[0].is_muted());
    assert_eq!(app.project.tracks()[0].volume_db(), -3.0);

    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks()[0].volume_db(), 0.0);
    let _ = app.update(Message::Undo);
    assert!(!app.project.tracks()[0].is_muted());
}

#[test]
fn item_drag_obeys_project_busy_and_jack_edit_guards() {
    assert_eq!(
        super::item_drag_edit_guard_status(false, false, false, false, true, false),
        Some("Close JACK output before editing the project")
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
fn action_search_dispatches_supported_commands() {
    let mut app = App::default();
    let _ = app.update(Message::ActionQueryChanged("add track".to_owned()));
    let _ = app.update(Message::RunActionQuery);
    assert_eq!(app.project.tracks().len(), 1);

    let _ = app.update(Message::ActionQueryChanged("undo".to_owned()));
    let _ = app.update(Message::RunActionQuery);
    assert!(app.project.tracks().is_empty());
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
    #[cfg(feature = "jack-backend")]
    assert!(prepare_project_playback_file(path.clone(), Project::new().snapshot(), 0).is_err());
    assert!(!path.exists());
}

#[cfg(feature = "jack-backend")]
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

#[test]
fn midi_items_append_after_existing_items_and_require_a_track() {
    let mut app = App::default();
    let _ = app.update(Message::AddMidiItem);
    assert!(app.project.midi_items().is_empty());
    assert_eq!(
        app.status,
        "Edit failed: add a track before creating a MIDI item"
    );

    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::AddMidiItem);
    let _ = app.update(Message::AddMidiItem);
    assert_eq!(app.project.midi_items()[0].start_tick(), 0);
    assert_eq!(app.project.midi_items()[1].start_tick(), 3_840);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.midi_items().len(), 1);
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
fn track_rename_is_undoable() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();

    let _ = app.update(Message::BeginTrackNameEdit(track_id));
    assert_eq!(app.track_name_edits.get(&track_id).unwrap(), "Audio 1");
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

    let _ = app.update(Message::AdjustPan(track_id, 0.25));
    assert_eq!(app.project.tracks()[0].pan(), 0.25);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.tracks()[0].pan(), 0.0);
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

    let _ = app.update(Message::DuplicateAudioItem(source_id));
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
