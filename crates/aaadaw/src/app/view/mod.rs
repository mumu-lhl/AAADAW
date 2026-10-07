use super::{App, Message};
#[cfg(any(
    all(feature = "jack-backend", feature = "pipewire-backend"),
    all(
        feature = "jack-backend",
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos")
    ),
    all(
        feature = "pipewire-backend",
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos")
    )
))]
use aaadaw_app::PlaybackBackend;
use iced::widget::button;
#[cfg(feature = "audio-device")]
use iced::widget::text_input;
use iced::widget::{column, container, float, mouse_area, pane_grid, row, stack, text};
use iced::{Alignment, Element, Length};

mod arrangement;
mod fx_chain;
mod item_inspector;
mod media;
mod menu;
mod midi_editor;
mod mixer;
mod plugin_picker;
mod render;
mod settings;
mod tempo_map;
mod tokens;

const CLAP_PLUGIN_RISK: &str = "CLAP scanning runs in a helper process. Loaded plugins still run inside AAADAW with the app's privileges and can crash or stall it; runtime plugins are not sandboxed.";

#[cfg(any(feature = "audio-device", test))]
pub(super) fn playback_diagnostic_suffix(
    backend_name: &str,
    underrun_samples: u64,
    master_guarded_samples: u64,
    master_non_finite_samples: u64,
    jack_xruns: Option<u64>,
    callback_errors: u64,
    output_device_lost: bool,
) -> String {
    let mut diagnostics = String::new();
    if output_device_lost {
        diagnostics.push_str(" · output device unavailable; close playback and reopen it after selecting a default device");
    }
    if underrun_samples > 0 {
        diagnostics.push_str(&format!(" · stream underrun: {underrun_samples} samples"));
    }
    if let Some(jack_xruns) = jack_xruns.filter(|count| *count > 0) {
        diagnostics.push_str(&format!(" · JACK XRuns: {jack_xruns}"));
    }
    if master_guarded_samples > 0 {
        diagnostics.push_str(&format!(
            " · Master ceiling (total): {master_guarded_samples} samples"
        ));
    }
    if master_non_finite_samples > 0 {
        diagnostics.push_str(&format!(
            " · non-finite Master samples silenced (total): {master_non_finite_samples}"
        ));
    }
    if callback_errors > 0 {
        diagnostics.push_str(&format!(" · {backend_name} errors: {callback_errors}"));
    }
    diagnostics
}

pub(super) fn view_for_window(app: &App, window_id: iced::window::Id) -> Element<'_, Message> {
    if app.settings_window_id == Some(window_id) {
        settings::view(app)
    } else if app.render_window_id == Some(window_id) {
        render::view(app)
    } else if app.tempo_map_window_id == Some(window_id) {
        tempo_map::view(app)
    } else if app.fx_chain_window_id == Some(window_id) {
        fx_chain::view(app)
    } else if app.plugin_picker_window_id == Some(window_id) {
        plugin_picker::view(app)
    } else if app.midi_editor_window_id == Some(window_id) {
        midi_editor::view(app)
    } else {
        view(app)
    }
}

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let toolbar = menu::bar(app);

    let workspace: Element<'_, Message> = match app.main_workspace {
        super::MainWorkspace::Arrangement if app.media_panel_dock.open => docked_arrangement(app),
        super::MainWorkspace::Arrangement => arrangement::view(app),
        super::MainWorkspace::Mixer => mixer::view(app),
    };
    let status_text = app.status.clone();

    let mut content = column![toolbar]
        .spacing(tokens::SECTION_GAP)
        .padding(tokens::SPACING_LG)
        .height(Length::Fill);
    let visible_recoveries = app
        .recording_recovery_candidates
        .iter()
        .filter(|candidate| candidate_needs_recovery(app, candidate))
        .collect::<Vec<_>>();
    if let Some(candidate) = visible_recoveries.first() {
        let seconds =
            candidate.recorded_frames as f64 / f64::from(candidate.manifest.sample_rate.max(1));
        let description = if candidate.discarded_tail_bytes > 0 {
            format!(
                "Incomplete take found: {:.1}s of audio. The final {} byte(s) are incomplete and will be dropped.",
                seconds, candidate.discarded_tail_bytes
            )
        } else {
            format!("Incomplete take found: {:.1}s of audio.", seconds)
        };
        let description = if candidate.discarded_frames > 0 {
            format!(
                "{description} {} previously finalized frame(s) are missing or unreadable.",
                candidate.discarded_frames
            )
        } else {
            description
        };
        let count = visible_recoveries.len();
        let mut notice = row![text(description).size(12)].spacing(tokens::SECTION_GAP);
        if count > 1 {
            notice = notice.push(text(format!("{count} takes")).size(11));
        }
        notice = notice
            .push(
                button(text("Recover").size(12))
                    .padding([tokens::SPACING_XS, tokens::SPACING_SM])
                    .on_press(Message::RecoverRecording(candidate.manifest_path.clone())),
            )
            .push(
                button(text("Discard").size(12))
                    .padding([tokens::SPACING_XS, tokens::SPACING_SM])
                    .on_press(Message::DiscardRecording(candidate.manifest_path.clone())),
            )
            .align_y(Alignment::Center);
        content = content.push(
            container(notice)
                .width(Length::Fill)
                .padding(tokens::PANEL_PADDING),
        );
    }
    let transport = container(
        row![
            text("Transport").size(14),
            button(
                text(format!(
                    "{:.2} BPM",
                    app.project.tempo_at_tick(app.timeline.edit_cursor_tick)
                ))
                .size(12)
            )
            .padding([tokens::SPACING_XS, tokens::SPACING_SM])
            .on_press(Message::OpenTempoMap),
            button(
                text(format!(
                    "{}/{}",
                    app.project
                        .time_signature_at_tick(app.timeline.edit_cursor_tick)
                        .numerator(),
                    app.project
                        .time_signature_at_tick(app.timeline.edit_cursor_tick)
                        .denominator()
                ))
                .size(12)
            )
            .padding([tokens::SPACING_XS, tokens::SPACING_SM])
            .on_press(Message::OpenMeterMap),
            playback_controls(app),
            iced::widget::Space::new().width(Length::Fill),
            time_selection_readout(app),
        ]
        .spacing(tokens::SECTION_GAP)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(tokens::PANEL_PADDING)
    .style(iced::widget::container::rounded_box);
    let offline_job_count = usize::from(app.offline_render_busy) + app.offline_job_queue.len();
    let status_row = row![
        text(status_text).width(Length::Fill),
        button(text(format!("Jobs · {offline_job_count}")).size(11))
            .padding([tokens::SPACING_XS, tokens::SPACING_SM])
            .style(button::secondary)
            .on_press(Message::ToggleOfflineJobsPanel),
    ]
    .spacing(tokens::SECTION_GAP)
    .align_y(Alignment::Center);
    content = content.push(workspace).push(status_row).push(transport);
    let base: Element<'_, Message> = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    let mut layered = base;
    if app.offline_jobs_panel_open {
        let popup = float(menu::offline_jobs_panel(app)).translate(|bounds, viewport| {
            let max_x = (viewport.x + viewport.width - bounds.width).max(viewport.x);
            let target_x = max_x;
            let target_y = (viewport.y + viewport.height - bounds.height - 76.0).max(viewport.y);
            iced::Vector::new(target_x - bounds.x, target_y - bounds.y)
        });
        layered = stack![layered, popup]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }
    layered = if let Some(active_menu) = app.active_menu {
        let anchor_x = menu::anchor_x(active_menu);
        let popup = float(menu::dropdown(app, active_menu)).translate(move |bounds, viewport| {
            let max_x = (viewport.x + viewport.width - bounds.width).max(viewport.x);
            let max_y = (viewport.y + viewport.height - bounds.height).max(viewport.y);
            let target_x = (viewport.x + anchor_x).clamp(viewport.x, max_x);
            let target_y = (viewport.y + menu::bar_bottom()).min(max_y);
            iced::Vector::new(target_x - bounds.x, target_y - bounds.y)
        });
        stack![layered, popup]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        layered
    };
    mouse_area(layered)
        .on_press(Message::DismissMainMenu)
        .into()
}

fn candidate_needs_recovery(app: &App, candidate: &aaadaw_app::RecordingRecoveryCandidate) -> bool {
    !app.pending_recording_cleanup.iter().any(|cleanup| {
        cleanup.manifest_path == candidate.manifest_path
            && cleanup.media_refs.iter().all(|media_ref| {
                app.project
                    .audio_items()
                    .iter()
                    .any(|item| item.media_ref() == media_ref)
            })
    })
}

fn docked_arrangement(app: &App) -> Element<'_, Message> {
    let panes = app
        .media_panel_dock
        .panes
        .as_ref()
        .expect("the Media Browser dock initializes its pane grid before opening");
    pane_grid(panes, |_pane, content, _is_maximized| {
        let body: Element<'_, Message> = match content {
            super::MainPane::Arrangement => arrangement::view(app),
            super::MainPane::MediaBrowser => media::dock_view(app),
        };
        pane_grid::Content::new(body)
    })
    .spacing(3)
    .on_resize(10, |event| {
        Message::MediaPanelResized(event.split, event.ratio)
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn time_selection_readout(app: &App) -> Element<'_, Message> {
    let Some(selection) = app.timeline.time_selection else {
        return iced::widget::Space::new().width(Length::Shrink).into();
    };
    let format_tick = |tick| {
        app.project
            .musical_position_at_tick(tick)
            .map(|position| {
                format!(
                    "{}.{}.{}",
                    position.measure(),
                    position.beat(),
                    position.tick_in_beat()
                )
            })
            .unwrap_or_else(|_| format!("{tick} ticks"))
    };
    text(format!(
        "Sel {}–{}",
        format_tick(selection.start_tick),
        format_tick(selection.end_tick)
    ))
    .size(12)
    .into()
}

#[cfg(feature = "audio-device")]
fn playback_controls(app: &App) -> Element<'_, Message> {
    let backend_name = app.selected_playback_backend().name();
    let playback_available = app.selected_playback_backend().is_available();
    let armed = app
        .project
        .tracks()
        .iter()
        .any(|track| track.is_record_armed());
    let playback_state = if !playback_available {
        "Native audio output is unavailable in this build".to_owned()
    } else if app.recording.is_some() {
        format!(
            "Recording · {:.2}s",
            app.playhead_sample as f64 / app.project.settings().sample_rate() as f64
        )
    } else if app.recording_starting {
        if app.pending_recording.is_some() {
            format!("Starting {backend_name} transport…")
        } else {
            format!("Connecting {backend_name} input…")
        }
    } else if app.recording_stopping {
        "Finalizing take…".to_owned()
    } else if app.playback_busy {
        format!("Preparing {backend_name}…")
    } else if app.playback.is_none() {
        format!(
            "{} · {backend_name} closed",
            if armed { "Armed" } else { "Stopped" }
        )
    } else {
        let seconds = app.playhead_sample as f64 / app.project.settings().sample_rate() as f64;
        format!(
            "{} · {seconds:.2}s",
            if app.playback_playing {
                "Playing"
            } else if app.playback_paused {
                "Paused"
            } else if armed {
                "Armed · Stopped"
            } else {
                "Stopped"
            },
        )
    };
    let playback_state = if let Some(playback) = app.playback.as_ref() {
        let stats = playback.stats();
        format!(
            "{playback_state}{}",
            playback_diagnostic_suffix(
                backend_name,
                stats.underrun_samples,
                stats.master_guarded_samples,
                stats.master_non_finite_samples,
                stats.jack_xruns,
                stats.callback_errors,
                stats.output_device_lost,
            )
        )
    } else {
        playback_state
    };
    let can_record = playback_available;
    let controls = row![
        button(if app.playback_playing {
            "Pause"
        } else {
            "Play"
        })
        .on_press_maybe(playback_available.then_some(if app.playback_playing {
            Message::TogglePlayback
        } else {
            Message::StartPlayback
        })),
        button(if app.recording_starting {
            "Cancel Input"
        } else if app.recording.is_some() {
            "Stop Recording"
        } else {
            "Stop"
        })
        .on_press_maybe(if app.recording.is_some() || app.recording_starting {
            Some(Message::StopRecording)
        } else {
            playback_available.then_some(Message::StopPlayback)
        }),
        button("MIDI Panic")
            .style(iced::widget::button::danger)
            .on_press_maybe(
                (playback_available && app.playback.is_some()).then_some(Message::PanicMidi)
            ),
        button(if app.recording_starting {
            "Connecting…"
        } else if app.recording.is_some() {
            "Recording"
        } else if !can_record {
            "Record unavailable"
        } else {
            "Record"
        })
        .style(iced::widget::button::danger)
        .on_press_maybe(if app.recording.is_some() || app.recording_starting {
            Some(Message::StopRecording)
        } else if can_record {
            Some(Message::StartRecording)
        } else {
            None
        }),
        button("Restart").on_press_maybe(playback_available.then_some(Message::RestartPlayback),),
        text_input("Sample", &app.seek_sample_query)
            .on_input(Message::SeekSampleChanged)
            .width(100),
        button("Seek").on_press_maybe(playback_available.then_some(Message::SeekToSample)),
        button(text(format!("Close {backend_name}"))).on_press(Message::ClosePlayback),
        text(playback_state),
    ];
    #[cfg(any(
        all(feature = "jack-backend", feature = "pipewire-backend"),
        all(
            feature = "jack-backend",
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos")
        ),
        all(
            feature = "pipewire-backend",
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos")
        )
    ))]
    let controls = {
        let controls = controls;
        #[cfg(feature = "jack-backend")]
        let controls = controls.push(
            button(
                if app.selected_playback_backend() == PlaybackBackend::Jack {
                    "● JACK"
                } else {
                    "JACK"
                },
            )
            .on_press(Message::SelectPlaybackBackend(PlaybackBackend::Jack)),
        );
        #[cfg(feature = "pipewire-backend")]
        let controls = controls.push(
            button(
                if app.selected_playback_backend() == PlaybackBackend::PipeWire {
                    "● PipeWire"
                } else {
                    "PipeWire"
                },
            )
            .on_press(Message::SelectPlaybackBackend(PlaybackBackend::PipeWire)),
        );
        #[cfg(all(
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos")
        ))]
        let controls = controls.push(
            button(
                if app.selected_playback_backend() == PlaybackBackend::Cpal {
                    format!("● {}", PlaybackBackend::Cpal.name())
                } else {
                    PlaybackBackend::Cpal.name().to_owned()
                },
            )
            .on_press(Message::SelectPlaybackBackend(PlaybackBackend::Cpal)),
        );
        controls
    };
    controls.spacing(8).align_y(Alignment::Center).into()
}

#[cfg(not(feature = "audio-device"))]
fn playback_controls(_app: &App) -> Element<'_, Message> {
    text("Enable an audio backend for playback").into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn clap_risk_notice_clearly_describes_the_process_boundary() {
        assert!(super::CLAP_PLUGIN_RISK.contains("app's privileges"));
        assert!(super::CLAP_PLUGIN_RISK.contains("crash or stall"));
        assert!(super::CLAP_PLUGIN_RISK.contains("not sandboxed"));
    }
}
