use super::{App, Message};
#[cfg(all(feature = "jack-backend", feature = "pipewire-backend"))]
use aaadaw_app::PlaybackBackend;
#[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
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
