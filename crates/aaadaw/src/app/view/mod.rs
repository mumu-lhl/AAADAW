use super::commands::CommandId;
use super::{App, Message, WorkspacePage};
#[cfg(feature = "jack-backend")]
use iced::widget::text_input;
use iced::widget::{button, column, container, float, mouse_area, row, stack, text};
use iced::{Alignment, Element, Length};

mod arrangement;
mod item_inspector;
mod media;
mod menu;
mod project;
mod tokens;

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let toolbar = menu::bar(app);

    let workspace_tabs = row![
        button(if app.active_workspace == WorkspacePage::Arrangement {
            "● Arrangement"
        } else {
            "Arrangement"
        })
        .style(workspace_button_style(
            app.active_workspace == WorkspacePage::Arrangement,
        ))
        .on_press(Message::ExecuteCommand(CommandId::Workspace(
            WorkspacePage::Arrangement,
        ))),
        button(if app.active_workspace == WorkspacePage::Media {
            "● Media"
        } else {
            "Media"
        })
        .style(workspace_button_style(
            app.active_workspace == WorkspacePage::Media,
        ))
        .on_press(Message::ExecuteCommand(CommandId::Workspace(
            WorkspacePage::Media,
        ))),
        button(if app.active_workspace == WorkspacePage::Project {
            "● Project"
        } else {
            "Project"
        })
        .style(workspace_button_style(
            app.active_workspace == WorkspacePage::Project,
        ))
        .on_press(Message::ExecuteCommand(CommandId::Workspace(
            WorkspacePage::Project,
        ))),
    ]
    .spacing(tokens::SPACING_MD);

    let workspace: Element<'_, Message> = match app.active_workspace {
        WorkspacePage::Arrangement => arrangement::view(app),
        WorkspacePage::Media => media::view(app),
        WorkspacePage::Project => project::view(app),
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
        .push(workspace_tabs)
        .push(workspace)
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

fn workspace_button_style(
    active: bool,
) -> fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    if active {
        iced::widget::button::primary
    } else {
        iced::widget::button::secondary
    }
}

#[cfg(feature = "jack-backend")]
fn playback_controls(app: &App) -> Element<'_, Message> {
    let playback_state = if app.playback_busy {
        "Preparing JACK…".to_owned()
    } else if app.playback.is_none() {
        "JACK closed".to_owned()
    } else {
        let seconds = app.playhead_sample as f64 / app.project.settings().sample_rate() as f64;
        format!(
            "{} · {seconds:.2}s",
            if app.playback_playing {
                "Playing"
            } else {
                "Stopped"
            }
        )
    };
    row![
        button("Play").on_press(Message::StartPlayback),
        button("Stop").on_press(Message::StopPlayback),
        button("Restart").on_press(Message::RestartPlayback),
        text_input("Sample", &app.seek_sample_query)
            .on_input(Message::SeekSampleChanged)
            .width(100),
        button("Seek").on_press(Message::SeekToSample),
        button("Close JACK").on_press(Message::ClosePlayback),
        text(playback_state),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

#[cfg(not(feature = "jack-backend"))]
fn playback_controls(_app: &App) -> Element<'_, Message> {
    text("JACK: enable jack-backend").into()
}
