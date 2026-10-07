use super::commands::{self, CommandId, TrackCommand};
use super::messages::SharedProjectSessionLock;
#[cfg(feature = "audio-device")]
use super::prepare_project_playback_file;
use super::project_io::{
    load_project_file, load_project_session, save_project_file, save_project_session_file,
};
#[cfg(feature = "audio-device")]
use super::{ActiveRecording, SharedRecordingStart};
use super::{
    App, MainMenu, MainWorkspace, Message, PathPickerTarget, keyboard_shortcut_event,
    shortcut_message,
};
#[cfg(all(feature = "jack-backend", feature = "pipewire-backend"))]
use aaadaw_app::PlaybackBackend;
use aaadaw_core::{DawAction, MidiNoteData, Project, TrackFxPlugin};
use aaadaw_storage::{ArrangementViewState, ProjectSessionLock, ProjectStore};
use iced::keyboard::{Key, Modifiers};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT_TEST_FILE: AtomicU64 = AtomicU64::new(0);

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
fn resetting_track_meters_clears_the_visible_peak_values() {
    let mut app = App::default();
    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    app.track_peak_levels.insert(track_id, [0.75, 0.5]);
    app.master_peak_level = [0.9, 0.8];
    app.master_guard_ticks_remaining = 5;

    app.reset_track_meters();

    assert!(app.track_peak_levels.is_empty());
    assert_eq!(app.master_peak_level, [0.0; 2]);
    assert_eq!(app.master_guard_ticks_remaining, 0);
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

    app.handle_playback_device_lost();

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
fn new_project_is_in_file_menu_and_cannot_discard_dirty_work() {
    let mut app = App::default();
    let new_project = commands::for_menu(&app, MainMenu::File)
        .into_iter()
        .find(|entry| entry.id == CommandId::NewProject)
        .expect("File menu should expose New project");
    assert!(new_project.enabled);
    assert_eq!(new_project.label, "New project");

    let _ = app.update(Message::AddTrack);
    let track_id = app.project.tracks()[0].id();
    assert!(!commands::is_enabled(&app, CommandId::NewProject));
    let _ = app.update(Message::NewProject);
    assert_eq!(app.project.tracks()[0].id(), track_id);
    assert_eq!(
        app.status,
        "Save the current project before creating a new one"
    );

    app.saved_revision = app.revision;
    app.project_path = Some(std::path::PathBuf::from("saved.aaadaw"));
    app.project_path_query = "saved.aaadaw".to_owned();
    let _ = app.update(Message::NewProject);
    assert!(app.project.tracks().is_empty());
    assert!(app.project_path.is_none());
    assert!(app.project_path_query.is_empty());
    assert_eq!(app.revision, 0);
    assert_eq!(app.saved_revision, 0);
    assert_eq!(app.status, "New project created");
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
        let mut app = App::default();
        let _ = app.update(trigger);
        assert!(app.path_picker_busy);

        let _ = app.update(Message::SaveProject);
        assert_eq!(app.status, "Wait for the file dialog to finish");

        let _ = app.update(Message::PathPicked(PathPickerTarget::SaveProject, Ok(None)));
        assert!(!app.path_picker_busy);
        assert_eq!(app.status, "");
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
        Some(Message::ExecuteCommand(CommandId::DeleteSelectedItems))
    ));
    assert!(matches!(
        shortcut_message(
            Key::Named(iced::keyboard::key::Named::Backspace),
            Modifiers::NONE,
            &HashMap::new()
        ),
        Some(Message::ExecuteCommand(CommandId::DeleteSelectedItems))
    ));
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
    #[cfg(feature = "audio-device")]
    assert!(matches!(
        shortcut_message(
            Key::Named(iced::keyboard::key::Named::Space),
            Modifiers::NONE,
            &HashMap::new(),
        ),
        Some(Message::ExecuteCommand(CommandId::TogglePlayback))
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
    assert_eq!(export.label, "Render project to WAV…");

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
        "Delete/Backspace"
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
            if action_id == "edit.undo" && key == "k" && modifiers == Modifiers::COMMAND
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
            if action_id == "edit.undo" && key == "Delete" && modifiers == Modifiers::NONE
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
fn documented_first_project_shortcuts_match_action_defaults() {
    let app = App::default();
    let shortcuts = commands::shortcut_entries(&app);
    for (action_id, expected) in [
        ("file.new-project", "Ctrl/Cmd+N"),
        ("file.open-project", "Ctrl/Cmd+O"),
        ("file.save-project", "Ctrl/Cmd+S"),
        ("edit.undo", "Ctrl/Cmd+Z"),
        ("edit.redo", "Ctrl/Cmd+Shift+Z, Ctrl/Cmd+Y"),
        ("item.duplicate", "Ctrl/Cmd+D"),
        ("item.delete-selected", "Delete/Backspace"),
        ("item.split-at-cursor", "S"),
    ] {
        let entry = shortcuts
            .iter()
            .find(|entry| entry.id == action_id)
            .unwrap_or_else(|| panic!("documented action {action_id} should exist"));
        assert_eq!(entry.default_binding, expected, "{action_id}");
    }

    #[cfg(feature = "audio-device")]
    {
        let playback = shortcuts
            .iter()
            .find(|entry| entry.id == "transport.toggle-playback")
            .expect("playback shortcut should exist in audio builds");
        assert_eq!(playback.default_binding, "Space");
    }
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
        ""
    );
    assert_eq!(
        commands::shortcut_entries(&app)
            .into_iter()
            .find(|entry| entry.id == "file.new-project")
            .unwrap()
            .binding,
        "Ctrl/Cmd+N"
    );
    assert_eq!(
        commands::shortcut_entries(&app)
            .into_iter()
            .find(|entry| entry.id == "file.save-project-as")
            .unwrap()
            .binding,
        ""
    );
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

    let _ = app.update(Message::ShortcutPressed("m".to_owned(), Modifiers::COMMAND));
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

    save_project_session_file(
        path.clone(),
        project.snapshot(),
        ArrangementViewState::default(),
        false,
        Some(&mut session_lock),
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
    };
    let mut session_lock = ProjectSessionLock::acquire(&path).unwrap();
    save_project_session_file(
        path.clone(),
        project.snapshot(),
        view_state.clone(),
        false,
        Some(&mut session_lock),
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
    let _ = app.update(Message::PasteMidiNotes(item_id));
    let notes = app.project.midi_items()[0].notes();
    assert_eq!(notes.len(), 4);
    assert_eq!(notes[2].tick(), 1_200);
    assert_eq!(notes[3].tick(), 1_920);
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
    assert_eq!(app.project.midi_items()[0].notes()[4].tick(), 2_400);
    assert_eq!(app.project.midi_items()[0].notes()[5].tick(), 3_120);

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
fn audio_item_edge_trim_preserves_source_content_and_is_one_undoable_edit() {
    let mut app = App::default();
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
    assert_eq!(trimmed.source_offset_samples(), 256 + trimmed_start - start);
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
    assert_eq!(right_trimmed.start_sample(), trimmed_start);
    assert_eq!(right_trimmed.source_offset_samples(), trimmed_source_offset);
    assert_eq!(
        right_trimmed.start_sample() + right_trimmed.length_samples(),
        app.project.sample_at_tick(1_200).unwrap()
    );
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
            target_tick: 0,
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
    app.record_import_tracks = Some(super::RecordImportTarget {
        track_ids: vec![track_id],
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
    assert!(app.status.contains("simulated interrupted import"));

    for path in [&project_path, &manifest_path, &source_path] {
        let _ = std::fs::remove_file(path);
    }
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
