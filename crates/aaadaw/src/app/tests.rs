use super::commands::{self, CommandId, TrackCommand};
#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
use super::prepare_project_playback_file;
use super::project_io::{load_project_file, save_project_file};
use super::{
    App, MainMenu, Message, PathPickerTarget, WorkspacePage, keyboard_shortcut_event,
    shortcut_message,
};
use aaadaw_core::{DawAction, MidiNoteData, Project};
use aaadaw_storage::ProjectStore;
use iced::keyboard::{Key, Modifiers};
use std::collections::HashMap;
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
    let _ = app.update(Message::ExecuteCommand(CommandId::Workspace(
        WorkspacePage::Media,
    )));
    assert_eq!(app.active_workspace, WorkspacePage::Media);
    let _ = app.update(Message::ExecuteCommand(CommandId::Workspace(
        WorkspacePage::Project,
    )));
    assert_eq!(app.active_workspace, WorkspacePage::Project);
    assert_eq!(app.active_menu, None);
    let _ = app.update(Message::ToggleMainMenu(MainMenu::File));
    assert_eq!(app.active_menu, Some(MainMenu::File));
    let _ = app.update(Message::ToggleMainMenu(MainMenu::File));
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
        MainMenu::Actions,
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
    app.timeline.edit_cursor_tick = 720;
    let _ = app.update(Message::ToggleMainMenu(MainMenu::Item));
    let _ = app.update(Message::Escape);
    assert_eq!(app.active_menu, None);
    assert!(app.timeline.time_selection.is_some());

    let _ = app.update(Message::Escape);
    assert_eq!(app.timeline.time_selection, None);
    assert_eq!(app.timeline.edit_cursor_tick, 720);
    assert_eq!(app.timeline.selected_track, Some(track_id));
}

#[test]
fn loading_a_project_clears_the_previous_time_selection() {
    let mut app = App::default();
    let _ = app.update(Message::Timeline(
        crate::timeline::TimelineEvent::SetTimeSelection {
            start_tick: 240,
            end_tick: 960,
        },
    ));

    let _ = app.update(Message::ProjectLoaded(
        std::path::PathBuf::from("loaded.aaadaw"),
        std::sync::Arc::new(std::sync::Mutex::new(Some(Ok(Project::new())))),
    ));

    assert_eq!(app.timeline.time_selection, None);
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
    assert_eq!(app.status, "Save the project before importing audio");
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
        Some(Message::ShortcutPressed(key, modifiers))
            if key == "z" && modifiers == Modifiers::COMMAND
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
    assert_eq!(undo.shortcut.as_deref(), Some("Ctrl/Cmd+Z"));
    assert_eq!(
        redo.shortcut.as_deref(),
        Some("Ctrl/Cmd+Shift+Z, Ctrl/Cmd+Y")
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
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    assert!(matches!(
        shortcut_message(
            Key::Named(iced::keyboard::key::Named::Space),
            Modifiers::NONE,
            &HashMap::new(),
        ),
        Some(Message::ExecuteCommand(CommandId::TogglePlayback))
    ));
    #[cfg(not(any(feature = "jack-backend", feature = "pipewire-backend")))]
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
    assert!(commands::capture_binding("x", Modifiers::NONE).is_err());

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
            if action_id == "edit.undo" && key == "k" && modifiers == Modifiers::COMMAND
    ));
    for key in [
        Key::Named(iced::keyboard::key::Named::Backspace),
        Key::Named(iced::keyboard::key::Named::Delete),
    ] {
        assert!(matches!(
            keyboard_shortcut_event(
                make_key_event(key, Modifiers::NONE),
                iced::event::Status::Captured,
                settings_window_id,
                Some(main_window_id),
                Some(settings_window_id),
                Some("edit.undo"),
            ),
            Some(Message::ClearShortcutBinding(action_id)) if action_id == "edit.undo"
        ));
    }
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
}

#[test]
fn settings_menu_opens_one_settings_window_and_shortcut_conflicts_keep_previous_binding() {
    let mut app = App::default();
    assert!(
        commands::for_menu(&app, MainMenu::File)
            .iter()
            .any(|entry| entry.id == CommandId::OpenSettings)
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

    let original = app.shortcut_binding_edits.get("edit.undo").cloned();
    let _ = app.update(Message::StartShortcutCapture("edit.undo".to_owned()));
    let _ = app.update(Message::ShortcutCaptureKey {
        action_id: "edit.undo".to_owned(),
        key: "s".to_owned(),
        modifiers: Modifiers::COMMAND,
    });
    assert_eq!(
        app.shortcut_binding_edits.get("edit.undo").cloned(),
        original
    );
    assert!(app.shortcut_capture_id.is_some());
    assert!(app.shortcut_editor_feedback.contains("Save project"));

    let _ = app.update(Message::ShortcutCaptureKey {
        action_id: "edit.undo".to_owned(),
        key: "u".to_owned(),
        modifiers: Modifiers::COMMAND,
    });
    assert_eq!(
        app.shortcut_binding_edits
            .get("edit.undo")
            .map(String::as_str),
        Some("Mod+U")
    );
    assert!(app.shortcut_editor_feedback.contains("Ctrl/Cmd+U"));

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
fn configurable_shortcuts_drive_dispatch_and_menu_hints_with_conflict_checks() {
    let bindings = HashMap::from([
        ("edit.undo".to_owned(), "Ctrl+U".to_owned()),
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
    assert_eq!(undo.shortcut.as_deref(), Some("Ctrl/Cmd+U"));

    let conflicting = HashMap::from([
        ("edit.undo".to_owned(), "Mod+X".to_owned()),
        ("file.save-project".to_owned(), "Mod+X".to_owned()),
    ]);
    assert!(commands::validate_bindings(&conflicting).is_err());
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
fn media_browser_dock_toggles_resizes_and_survives_workspace_changes() {
    let mut app = App::default();
    assert!(!app.media_panel_dock.open);
    assert!(app.media_panel_dock.panes.is_none());
    let toggle = CommandId::ToggleMediaBrowserPanel;
    assert!(commands::is_enabled(&app, toggle));
    assert!(
        commands::for_menu(&app, MainMenu::View)
            .iter()
            .any(|entry| entry.id == toggle)
    );

    let _ = app.update(Message::ExecuteCommand(toggle));
    assert!(app.media_panel_dock.open);
    assert_eq!(app.media_panel_dock.panes.as_ref().unwrap().len(), 2);
    let split = app.media_panel_dock.split.unwrap();
    let _ = app.update(Message::MediaPanelResized(split, 0.63));
    assert!((app.media_panel_dock.main_ratio - 0.63).abs() < f32::EPSILON);

    let _ = app.update(Message::ExecuteCommand(CommandId::Workspace(
        WorkspacePage::Media,
    )));
    assert!(app.media_panel_dock.open);
    let _ = app.update(Message::ExecuteCommand(CommandId::Workspace(
        WorkspacePage::Arrangement,
    )));
    let _ = app.update(Message::ToggleMediaBrowserPanel);
    assert!(!app.media_panel_dock.open);
    let _ = app.update(Message::ToggleMediaBrowserPanel);
    assert!(app.media_panel_dock.open);
    assert!((app.media_panel_dock.main_ratio - 0.63).abs() < f32::EPSILON);
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

    let _ = app.update(Message::AddTrack);
    let _ = app.update(Message::ActionQueryChanged("insert MIDI item".to_owned()));
    let _ = app.update(Message::RunActionQuery);
    assert_eq!(app.project.midi_items().len(), 1);
    let _ = app.update(Message::ActionQueryChanged("undo".to_owned()));
    let _ = app.update(Message::RunActionQuery);
    assert!(app.project.midi_items().is_empty());
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
    #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
    assert!(prepare_project_playback_file(path.clone(), Project::new().snapshot(), 0).is_err());
    assert!(!path.exists());
}

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
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

    let command = CommandId::DuplicateSelectedMidiItem;
    assert!(commands::is_enabled(&app, command));
    assert_eq!(
        commands::find(&app, "duplicate selected midi item"),
        Some(command)
    );
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
fn dirty_project_open_command_does_not_open_picker() {
    let mut app = App {
        project_path_query: "/projects/another.aaadaw".to_owned(),
        revision: 1,
        ..App::default()
    };

    let _ = app.update(Message::OpenProject);

    assert!(!app.io_busy);
    assert!(!app.path_picker_busy);
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

#[test]
fn split_item_commands_are_selection_aware_and_share_menu_search_definitions() {
    let mut app = App::default();
    let cursor = CommandId::SplitSelectedItemsAtCursor;
    let time_selection = CommandId::SplitSelectedItemsAtTimeSelection;
    assert!(!commands::is_enabled(&app, cursor));
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
    assert_eq!(commands::find(&app, "split at cursor"), Some(cursor));
    assert_eq!(
        commands::find(&app, "split at time selection"),
        Some(time_selection)
    );

    app.timeline.edit_cursor_tick = 0;
    app.timeline.time_selection = None;
    assert!(!commands::is_enabled(&app, cursor));
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
    assert_eq!(audio.len(), 3);
    assert_eq!(
        audio
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
                5_000 + (cut_samples[0] * 44_100 + 24_000) / 48_000,
                cut_samples[1] - cut_samples[0]
            ),
            (
                cut_samples[1],
                5_000 + (cut_samples[1] * 44_100 + 24_000) / 48_000,
                audio_end - cut_samples[1]
            ),
        ]
    );
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
    assert_eq!(app.timeline.selected_items.len(), 6);
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
    assert_eq!(app.revision, revision + 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.project.snapshot(), before);

    app.timeline.edit_cursor_tick = 480;
    let before_noop = app.project.snapshot();
    let revision = app.revision;
    assert!(!commands::is_enabled(
        &app,
        CommandId::SplitSelectedItemsAtCursor
    ));
    let _ = app.update(Message::SplitSelectedItemsAtCursor);
    assert_eq!(app.project.snapshot(), before_noop);
    assert_eq!(app.revision, revision);
}
