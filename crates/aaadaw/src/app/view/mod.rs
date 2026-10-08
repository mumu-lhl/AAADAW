use super::{App, Message};
#[cfg(any(
    all(feature = "jack-backend", feature = "pipewire-backend"),
    all(
        feature = "jack-backend",
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos", target_os = "android")
    ),
    all(
        feature = "pipewire-backend",
        feature = "cpal-backend",
        any(target_os = "windows", target_os = "macos", target_os = "android")
    )
))]
use aaadaw_app::PlaybackBackend;
use iced::widget::button;
#[cfg(feature = "audio-device")]
use iced::widget::text_input;
use iced::widget::{
    column, container, float, mouse_area, pane_grid, responsive, row, scrollable, stack, text,
};
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

const CLAP_PLUGIN_RISK: &str = "Scanning and instrument hosting run in helper processes. Audio effects run inside AAADAW with the app's privileges and may crash or stall it; audio effects are not sandboxed.";

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
    responsive(move |size| {
        if size.width < 720.0 {
            mobile_view(app, size.width, size.height)
        } else {
            desktop_view(app)
        }
    })
    .into()
}

fn desktop_view(app: &App) -> Element<'_, Message> {
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
    let transport = container(transport_view(app))
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
    // Keep the click-to-dismiss surface behind floating panels. Wrapping the
    // finished stack would also put the menu popup inside the dismiss surface,
    // so clicks in Actions search or on a menu command would close it before
    // the popup could handle the interaction.
    let mut layered: Element<'_, Message> =
        mouse_area(base).on_press(Message::DismissMainMenu).into();
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
    if let Some(transition) = app.pending_project_transition {
        let description = match transition {
            super::PendingProjectTransition::NewProject => {
                "Creating a new project will replace the current project."
            }
            super::PendingProjectTransition::OpenProject => {
                "Opening another project will replace the current project."
            }
            super::PendingProjectTransition::CloseMainWindow(_) => {
                "Closing AAADAW will discard unsaved changes."
            }
        };
        let dialog = container(
            column![
                text("Save changes before continuing?").size(18),
                text(format!(
                    "The current project has unsaved changes. {description}"
                ))
                .size(13),
                row![
                    button(text("Save")).on_press(Message::SaveBeforeProjectTransition),
                    button(text("Don't Save"))
                        .style(button::danger)
                        .on_press(Message::DiscardProjectChanges),
                    button(text("Cancel"))
                        .style(button::secondary)
                        .on_press(Message::CancelProjectTransition),
                ]
                .spacing(tokens::SECTION_GAP)
                .align_y(Alignment::Center),
            ]
            .spacing(tokens::SECTION_GAP),
        )
        .width(Length::Fixed(480.0))
        .padding(tokens::SPACING_LG)
        .style(iced::widget::container::rounded_box);
        let backdrop = mouse_area(
            container(dialog)
                .width(Length::Fill)
                .height(Length::Fill)
                .center(Length::Fill)
                .style(|_| iced::widget::container::Style {
                    background: Some(iced::Background::Color(iced::Color::from_rgba8(
                        0, 0, 0, 0.66,
                    ))),
                    ..iced::widget::container::Style::default()
                }),
        )
        .on_press(Message::DismissMainMenu);
        return stack![layered, backdrop]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }
    layered
}

fn mobile_view(app: &App, viewport_width: f32, viewport_height: f32) -> Element<'_, Message> {
    use super::{MainMenu, MainWorkspace};

    let menus = [
        MainMenu::File,
        MainMenu::Edit,
        MainMenu::View,
        MainMenu::Insert,
        MainMenu::Item,
        MainMenu::Track,
        MainMenu::Actions,
    ]
    .into_iter()
    .map(|menu_id| -> Element<'_, Message> {
        button(text(menu_id.label()).size(14))
            .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
            .padding([tokens::SPACING_LG as u16, tokens::SPACING_MD as u16])
            .style(if app.active_menu == Some(menu_id) {
                button::primary
            } else {
                button::secondary
            })
            .on_press(Message::ToggleMainMenu(menu_id))
            .into()
    });
    let menu_row = scrollable(row(menus).spacing(tokens::SPACING_XS))
        .direction(iced::widget::scrollable::Direction::Horizontal(
            iced::widget::scrollable::Scrollbar::default(),
        ))
        .height(Length::Shrink);

    let workspace_switcher = row![
        button("Arrange")
            .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
            .padding([tokens::SPACING_LG as u16; 2])
            .style(if app.main_workspace == MainWorkspace::Arrangement {
                button::primary
            } else {
                button::secondary
            })
            .on_press(Message::ShowMainWorkspace(MainWorkspace::Arrangement)),
        button("Mixer")
            .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
            .padding([tokens::SPACING_LG as u16; 2])
            .style(if app.main_workspace == MainWorkspace::Mixer {
                button::primary
            } else {
                button::secondary
            })
            .on_press(Message::ShowMainWorkspace(MainWorkspace::Mixer)),
        button(text(format!(
            "Jobs · {}",
            usize::from(app.offline_render_busy) + app.offline_job_queue.len()
        )))
        .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
        .padding([tokens::SPACING_LG as u16, tokens::SPACING_MD as u16])
        .on_press(Message::ToggleOfflineJobsPanel),
    ]
    .spacing(tokens::SPACING_SM);

    let workspace: Element<'_, Message> = match app.mobile_panel {
        super::MobilePanel::Editor => match app.main_workspace {
            MainWorkspace::Arrangement => arrangement::mobile_view(app),
            MainWorkspace::Mixer => mixer::mobile_view(app),
        },
        super::MobilePanel::MediaBrowser => media::mobile_view(app),
        super::MobilePanel::Settings => settings::view(app),
        super::MobilePanel::TimeMap => tempo_map::mobile_view(app),
        super::MobilePanel::FxChain => fx_chain::mobile_view(app),
        super::MobilePanel::PluginPicker => plugin_picker::mobile_view(app),
    };
    let panel_navigation: Element<'_, Message> = if app.mobile_panel == super::MobilePanel::Editor {
        workspace_switcher.into()
    } else {
        row![
            button("← Back")
                .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
                .padding([tokens::SPACING_SM, tokens::SPACING_MD])
                .on_press(Message::MobileNavigateBack),
            text(app.mobile_panel.title()).size(14).width(Length::Fill),
        ]
        .spacing(tokens::SPACING_SM)
        .align_y(Alignment::Center)
        .into()
    };
    let transport = container(
        column![
            row![
                text(format!(
                    "{:.2} BPM",
                    app.project.tempo_at_tick(app.timeline.edit_cursor_tick)
                ))
                .size(13),
                iced::widget::Space::new().width(Length::Fill),
                mobile_playback_controls(app),
            ]
            .align_y(Alignment::Center),
            row![
                text(format!("{}", app.timeline.edit_cursor_tick)).size(12),
                iced::widget::Space::new().width(Length::Fill),
                text(app.status.clone()).size(11).width(Length::Fill),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(tokens::SPACING_XS),
    )
    .width(Length::Fill)
    .padding(tokens::PANEL_PADDING)
    .style(iced::widget::container::rounded_box);

    let mut content = column![menu_row, panel_navigation]
        .spacing(tokens::SPACING_SM)
        .padding(tokens::SPACING_SM)
        .height(Length::Fill);
    if let Some(candidate) = app
        .recording_recovery_candidates
        .iter()
        .find(|candidate| candidate_needs_recovery(app, candidate))
    {
        let notice = row![
            text("Incomplete recording found")
                .size(12)
                .width(Length::Fill),
            button("Recover")
                .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
                .padding([tokens::SPACING_MD; 2])
                .on_press(Message::RecoverRecording(candidate.manifest_path.clone())),
            button("Discard")
                .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
                .padding([tokens::SPACING_MD; 2])
                .on_press(Message::DiscardRecording(candidate.manifest_path.clone())),
        ]
        .spacing(tokens::SPACING_XS)
        .align_y(Alignment::Center);
        content = content.push(container(notice).width(Length::Fill));
    }
    content = content.push(workspace).push(transport);
    let base: Element<'_, Message> = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    let mut layered = base;
    if let Some(active_menu) = app.active_menu {
        let popup = float(menu::mobile_dropdown(
            app,
            active_menu,
            (viewport_width - tokens::SPACING_LG * 2.0).max(1.0),
            (viewport_height - tokens::SPACING_LG * 2.0).max(1.0),
        ))
        .translate(center_popup);
        layered = stack![layered, popup]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }
    if app.offline_jobs_panel_open {
        let popup = float(menu::mobile_offline_jobs_panel(
            app,
            (viewport_width - tokens::SPACING_LG * 2.0).max(1.0),
            (viewport_height - tokens::SPACING_LG * 2.0).max(1.0),
        ))
        .translate(center_popup);
        layered = stack![layered, popup]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }
    mouse_area(layered)
        .on_press(Message::DismissMainMenu)
        .into()
}

fn center_popup(bounds: iced::Rectangle, viewport: iced::Rectangle) -> iced::Vector {
    let max_x = (viewport.x + viewport.width - bounds.width).max(viewport.x);
    let max_y = (viewport.y + viewport.height - bounds.height).max(viewport.y);
    let target_x = (viewport.x + (viewport.width - bounds.width) / 2.0).clamp(viewport.x, max_x);
    let target_y = (viewport.y + (viewport.height - bounds.height) / 2.0).clamp(viewport.y, max_y);
    iced::Vector::new(target_x - bounds.x, target_y - bounds.y)
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

fn transport_readout_tick(app: &App) -> u64 {
    #[cfg(feature = "audio-device")]
    if app.playback_playing || app.playback_paused || app.recording.is_some() {
        return app
            .project
            .tick_at_sample(app.playhead_sample)
            .unwrap_or(app.timeline.edit_cursor_tick);
    }
    app.timeline.edit_cursor_tick
}

fn transport_time_readout(project: &aaadaw_core::Project, tick: u64) -> String {
    project
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
}

fn transport_view(app: &App) -> Element<'_, Message> {
    responsive(move |size| {
        let readout = text(transport_time_readout(
            &app.project,
            transport_readout_tick(app),
        ))
        .size(16);
        let status = text(playback_status_label(app)).size(12);
        let details_button = button(if app.transport_details_open {
            "Details ▲"
        } else {
            "Details ▼"
        })
        .padding([tokens::SPACING_XS, tokens::SPACING_SM])
        .style(button::secondary)
        .on_press(Message::ToggleTransportDetails);
        let controls = playback_controls(app);
        let primary: Element<'_, Message> = if size.width < 720.0 {
            column![
                row![controls, details_button]
                    .spacing(tokens::SECTION_GAP)
                    .align_y(Alignment::Center),
                row![readout, status]
                    .spacing(tokens::SECTION_GAP)
                    .align_y(Alignment::Center),
            ]
            .spacing(tokens::SECTION_GAP)
            .into()
        } else {
            row![
                controls,
                readout,
                status.width(Length::Fill),
                details_button
            ]
            .spacing(tokens::SECTION_GAP)
            .align_y(Alignment::Center)
            .into()
        };
        let tempo = button(
            text(format!(
                "{:.2} BPM",
                app.project.tempo_at_tick(app.timeline.edit_cursor_tick)
            ))
            .size(12),
        )
        .padding([tokens::SPACING_XS, tokens::SPACING_SM])
        .on_press(Message::OpenTempoMap);
        let signature = app
            .project
            .time_signature_at_tick(app.timeline.edit_cursor_tick);
        let meter = button(
            text(format!(
                "{}/{}",
                signature.numerator(),
                signature.denominator()
            ))
            .size(12),
        )
        .padding([tokens::SPACING_XS, tokens::SPACING_SM])
        .on_press(Message::OpenMeterMap);
        let timing = row![tempo, meter, time_selection_readout(app)]
            .spacing(tokens::SECTION_GAP)
            .align_y(Alignment::Center);
        let main: Element<'_, Message> = if size.width < 980.0 {
            column![primary, timing].spacing(tokens::SECTION_GAP).into()
        } else {
            row![primary, timing]
                .spacing(tokens::SECTION_GAP)
                .align_y(Alignment::Center)
                .into()
        };
        let content: Element<'_, Message> = if app.transport_details_open {
            column![main, transport_details(app)]
                .spacing(tokens::SECTION_GAP)
                .into()
        } else {
            column![main].into()
        };
        container(content).width(Length::Fill).into()
    })
    .into()
}

#[cfg(feature = "audio-device")]
fn mobile_playback_controls(app: &App) -> Element<'_, Message> {
    let available = app.selected_playback_backend().is_available();
    row![
        button(if app.playback_playing {
            "Pause"
        } else {
            "Play"
        })
        .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
        .padding([tokens::SPACING_LG, tokens::SPACING_MD])
        .on_press_maybe(available.then_some(if app.playback_playing {
            Message::TogglePlayback
        } else {
            Message::StartPlayback
        })),
        button("Stop")
            .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
            .padding([tokens::SPACING_LG, tokens::SPACING_MD])
            .on_press_maybe(available.then_some(Message::StopPlayback)),
        button(if app.recording.is_some() {
            "End rec"
        } else {
            "Record"
        })
        .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
        .padding([tokens::SPACING_LG, tokens::SPACING_MD])
        .style(button::danger)
        .on_press_maybe(if app.recording.is_some() {
            Some(Message::StopRecording)
        } else if available {
            Some(Message::StartRecording)
        } else {
            None
        }),
    ]
    .spacing(tokens::SPACING_XS)
    .into()
}

#[cfg(not(feature = "audio-device"))]
fn mobile_playback_controls(_app: &App) -> Element<'static, Message> {
    text("Audio off").size(11).into()
}

#[cfg(feature = "audio-device")]
fn playback_status_label(app: &App) -> String {
    if app.recording_starting {
        "Connecting audio input…".to_owned()
    } else if app.recording_stopping {
        "Finalizing recording…".to_owned()
    } else if app.recording.is_some() {
        "Recording".to_owned()
    } else if app.playback_busy {
        format!("Preparing {}…", app.selected_playback_backend().name())
    } else if !app.selected_playback_backend().is_available() {
        "Audio unavailable in this build".to_owned()
    } else if app.playback.is_none() {
        "Output closed".to_owned()
    } else if app.playback_playing {
        "Playing".to_owned()
    } else if app.playback_paused {
        "Paused".to_owned()
    } else if app
        .project
        .tracks()
        .iter()
        .any(|track| track.is_record_armed())
    {
        "Stopped · Armed".to_owned()
    } else {
        "Stopped · Ready".to_owned()
    }
}

#[cfg(not(feature = "audio-device"))]
fn playback_status_label(_app: &App) -> String {
    "Audio unavailable in this build".to_owned()
}

#[cfg(feature = "audio-device")]
fn playback_controls(app: &App) -> Element<'_, Message> {
    let playback_available = app.selected_playback_backend().is_available();
    let preparation_busy = app.playback_busy || app.recording_starting || app.recording_stopping;
    let can_play = playback_available && !preparation_busy && app.recording.is_none();
    let can_stop = app.recording.is_some()
        || app.recording_starting
        || (!app.playback_busy && app.playback.is_some());
    let can_record = playback_available && !preparation_busy && app.recording.is_none();
    row![
        button(if app.playback_playing {
            "Pause"
        } else {
            "Play"
        })
        .on_press_maybe(can_play.then_some(if app.playback_playing {
            Message::TogglePlayback
        } else {
            Message::StartPlayback
        })),
        button("Stop").on_press_maybe(if app.recording.is_some() || app.recording_starting {
            Some(Message::StopRecording)
        } else {
            can_stop.then_some(Message::StopPlayback)
        }),
        button(if app.recording_starting {
            "Cancel Rec"
        } else if app.recording.is_some() {
            "Stop Rec"
        } else if !playback_available {
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
        button("Loop unavailable")
            .style(button::secondary)
            .on_press_maybe(None::<Message>),
    ]
    .spacing(tokens::SPACING_XS)
    .align_y(Alignment::Center)
    .into()
}

#[cfg(not(feature = "audio-device"))]
fn playback_controls(_app: &App) -> Element<'_, Message> {
    row![
        button("Play").on_press_maybe(None::<Message>),
        button("Stop").on_press_maybe(None::<Message>),
        button("Record unavailable")
            .style(button::danger)
            .on_press_maybe(None::<Message>),
        button("Loop unavailable")
            .style(button::secondary)
            .on_press_maybe(None::<Message>),
    ]
    .spacing(tokens::SPACING_XS)
    .align_y(Alignment::Center)
    .into()
}

#[cfg(feature = "audio-device")]
fn transport_details(app: &App) -> Element<'_, Message> {
    let backend_name = app.selected_playback_backend().name();
    let sample = app.playhead_sample;
    let seconds = sample as f64 / f64::from(app.project.settings().sample_rate().max(1));
    let busy = app.playback_busy || app.recording_starting || app.recording_stopping;
    let controls = row![
        button("Restart").on_press_maybe(
            (app.selected_playback_backend().is_available() && !busy)
                .then_some(Message::RestartPlayback)
        ),
        text_input("Sample", &app.seek_sample_query)
            .on_input(Message::SeekSampleChanged)
            .width(120),
        button("Seek").on_press_maybe(
            (app.selected_playback_backend().is_available() && !busy)
                .then_some(Message::SeekToSample)
        ),
        button("MIDI Panic")
            .style(button::danger)
            .on_press_maybe((!busy && app.playback.is_some()).then_some(Message::PanicMidi)),
        button(text(format!("Close {backend_name}")))
            .on_press_maybe((!busy && app.playback.is_some()).then_some(Message::ClosePlayback)),
        button("Audio Settings")
            .style(button::secondary)
            .on_press(Message::OpenSettings),
    ]
    .spacing(tokens::SPACING_XS)
    .align_y(Alignment::Center);
    #[cfg(any(
        all(feature = "jack-backend", feature = "pipewire-backend"),
        all(
            feature = "jack-backend",
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos", target_os = "android")
        ),
        all(
            feature = "pipewire-backend",
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos", target_os = "android")
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
            .on_press_maybe(
                (!busy && app.playback.is_none())
                    .then_some(Message::SelectPlaybackBackend(PlaybackBackend::Jack)),
            ),
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
            .on_press_maybe(
                (!busy && app.playback.is_none())
                    .then_some(Message::SelectPlaybackBackend(PlaybackBackend::PipeWire)),
            ),
        );
        #[cfg(all(
            feature = "cpal-backend",
            any(target_os = "windows", target_os = "macos", target_os = "android")
        ))]
        let controls = controls.push(
            button(
                if app.selected_playback_backend() == PlaybackBackend::Cpal {
                    format!("● {}", PlaybackBackend::Cpal.name())
                } else {
                    PlaybackBackend::Cpal.name().to_owned()
                },
            )
            .on_press_maybe(
                (!busy && app.playback.is_none())
                    .then_some(Message::SelectPlaybackBackend(PlaybackBackend::Cpal)),
            ),
        );
        controls
    };
    let mut content = column![
        row![
            text(format!("{seconds:.3}s · {sample} samples")).size(12),
            text(format!("Output: {backend_name}")).size(12),
        ]
        .spacing(tokens::SECTION_GAP)
        .align_y(Alignment::Center),
        controls,
    ]
    .spacing(tokens::SPACING_XS);
    if let Some(playback) = app.playback.as_ref() {
        let stats = playback.stats();
        let diagnostics = playback_diagnostic_suffix(
            backend_name,
            stats.underrun_samples,
            stats.master_guarded_samples,
            stats.master_non_finite_samples,
            stats.jack_xruns,
            stats.callback_errors,
            stats.output_device_lost,
        );
        if !diagnostics.is_empty() {
            content = content.push(text(diagnostics).size(11).width(Length::Fill));
        }
    }
    container(content).width(Length::Fill).into()
}

#[cfg(not(feature = "audio-device"))]
fn transport_details(_app: &App) -> Element<'_, Message> {
    row![
        text("Audio backend support is disabled in this build."),
        button("Audio Settings")
            .style(button::secondary)
            .on_press(Message::OpenSettings),
    ]
    .spacing(tokens::SECTION_GAP)
    .align_y(Alignment::Center)
    .into()
}

#[cfg(test)]
mod tests {
    use aaadaw_core::Project;

    #[test]
    fn transport_time_readout_uses_bars_beats_and_ticks() {
        let project = Project::new();

        assert_eq!(super::transport_time_readout(&project, 960), "1.2.0");
    }

    #[cfg(feature = "audio-device")]
    #[test]
    fn playback_preparation_has_a_clear_status_label() {
        let mut app = super::App::default();
        app.playback_busy = true;

        assert!(super::playback_status_label(&app).starts_with("Preparing "));
    }

    #[test]
    fn clap_risk_notice_clearly_describes_the_process_boundary() {
        assert!(super::CLAP_PLUGIN_RISK.contains("app's privileges"));
        assert!(super::CLAP_PLUGIN_RISK.contains("crash or stall"));
        assert!(super::CLAP_PLUGIN_RISK.contains("instrument hosting run in helper processes"));
        assert!(super::CLAP_PLUGIN_RISK.contains("audio effects are not sandboxed"));
    }
}
