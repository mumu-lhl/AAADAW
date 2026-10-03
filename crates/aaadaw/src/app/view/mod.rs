use super::{App, Message};
#[cfg(all(feature = "jack-backend", feature = "pipewire-backend"))]
use aaadaw_app::PlaybackBackend;
use iced::widget::button;
#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
use iced::widget::text_input;
use iced::widget::{column, container, float, mouse_area, pane_grid, row, stack, text};
use iced::{Alignment, Element, Length};

mod arrangement;
mod fx_chain;
mod item_inspector;
mod media;
mod menu;
mod midi_editor;
mod plugin_picker;
mod settings;
mod tokens;

const CLAP_IN_PROCESS_RISK: &str = "CLAP plugins run inside AAADAW with the app's privileges. A plugin can crash or stall the app; plugins are not sandboxed.";

pub(super) fn view_for_window(app: &App, window_id: iced::window::Id) -> Element<'_, Message> {
    if app.settings_window_id == Some(window_id) {
        settings::view(app)
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

    let arrangement: Element<'_, Message> = if app.media_panel_dock.open {
        docked_arrangement(app)
    } else {
        arrangement::view(app)
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
    content = content
        .push(arrangement)
        .push(text(status_text))
        .push(transport);
    let base: Element<'_, Message> = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    let layered = if let Some(active_menu) = app.active_menu {
        let anchor_x = menu::anchor_x(active_menu);
        let popup = float(menu::dropdown(app, active_menu)).translate(move |bounds, viewport| {
            let max_x = (viewport.x + viewport.width - bounds.width).max(viewport.x);
            let max_y = (viewport.y + viewport.height - bounds.height).max(viewport.y);
            let target_x = (viewport.x + anchor_x).clamp(viewport.x, max_x);
            let target_y = (viewport.y + menu::bar_bottom()).min(max_y);
            iced::Vector::new(target_x - bounds.x, target_y - bounds.y)
        });
        stack![base, popup]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        base
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

#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
fn playback_controls(app: &App) -> Element<'_, Message> {
    let backend_name = app.selected_playback_backend().name();
    let armed = app
        .project
        .tracks()
        .iter()
        .any(|track| track.is_record_armed());
    let playback_state = if app.recording.is_some() {
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
        let callback_errors = app
            .playback
            .as_ref()
            .map_or(0, |playback| playback.stats().callback_errors);
        format!(
            "{} · {seconds:.2}s{}",
            if app.playback_playing {
                "Playing"
            } else if armed {
                "Armed · Stopped"
            } else {
                "Stopped"
            },
            if callback_errors > 0 {
                format!(" · {backend_name} errors: {callback_errors}")
            } else {
                String::new()
            },
        )
    };
    let controls = row![
        button("Play").on_press(Message::StartPlayback),
        button(if app.recording_starting {
            "Cancel Input"
        } else if app.recording.is_some() {
            "Stop Recording"
        } else {
            "Stop"
        })
        .on_press(if app.recording.is_some() || app.recording_starting {
            Message::StopRecording
        } else {
            Message::StopPlayback
        }),
        button(if app.recording_starting {
            "Connecting…"
        } else if app.recording.is_some() {
            "Recording"
        } else {
            "Record"
        })
        .style(iced::widget::button::danger)
        .on_press(if app.recording.is_some() || app.recording_starting {
            Message::StopRecording
        } else {
            Message::StartRecording
        }),
        button("Restart").on_press(Message::RestartPlayback),
        text_input("Sample", &app.seek_sample_query)
            .on_input(Message::SeekSampleChanged)
            .width(100),
        button("Seek").on_press(Message::SeekToSample),
        button(text(format!("Close {backend_name}"))).on_press(Message::ClosePlayback),
        text(playback_state),
    ];
    #[cfg(all(feature = "jack-backend", feature = "pipewire-backend"))]
    let controls = controls
        .push(
            button(
                if app.selected_playback_backend() == PlaybackBackend::Jack {
                    "● JACK"
                } else {
                    "JACK"
                },
            )
            .on_press(Message::SelectPlaybackBackend(PlaybackBackend::Jack)),
        )
        .push(
            button(
                if app.selected_playback_backend() == PlaybackBackend::PipeWire {
                    "● PipeWire"
                } else {
                    "PipeWire"
                },
            )
            .on_press(Message::SelectPlaybackBackend(PlaybackBackend::PipeWire)),
        );
    controls.spacing(8).align_y(Alignment::Center).into()
}

#[cfg(not(any(feature = "jack-backend", feature = "pipewire-backend")))]
fn playback_controls(_app: &App) -> Element<'_, Message> {
    text("Enable jack-backend or pipewire-backend for audio output").into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn clap_risk_notice_clearly_describes_the_process_boundary() {
        assert!(super::CLAP_IN_PROCESS_RISK.contains("app's privileges"));
        assert!(super::CLAP_IN_PROCESS_RISK.contains("crash or stall"));
        assert!(super::CLAP_IN_PROCESS_RISK.contains("not sandboxed"));
    }
}
