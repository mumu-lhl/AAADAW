use super::{App, MainMenu, Message, PathPickerTarget, WorkspacePage};
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Alignment, Element, Length};

mod arrangement;
mod item_inspector;
mod media;
mod project;

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let project_name = app
        .project_path
        .as_ref()
        .and_then(|path| path.file_name())
        .map_or_else(
            || "New project".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
    let file_label = if app.active_menu == Some(MainMenu::File) {
        "File ▴"
    } else {
        "File ▾"
    };
    let edit_label = if app.active_menu == Some(MainMenu::Edit) {
        "Edit ▴"
    } else {
        "Edit ▾"
    };
    let track_label = if app.active_menu == Some(MainMenu::Track) {
        "Track ▴"
    } else {
        "Track ▾"
    };
    let toolbar = row![
        text("AAADAW").size(24),
        button(file_label)
            .style(menu_button_style(app.active_menu == Some(MainMenu::File)))
            .on_press(Message::ToggleMainMenu(MainMenu::File)),
        button(edit_label)
            .style(menu_button_style(app.active_menu == Some(MainMenu::Edit)))
            .on_press(Message::ToggleMainMenu(MainMenu::Edit)),
        button(track_label)
            .style(menu_button_style(app.active_menu == Some(MainMenu::Track)))
            .on_press(Message::ToggleMainMenu(MainMenu::Track)),
        text(project_name.clone()).width(Length::Fill),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let menu_panel: Option<Element<'_, Message>> = match app.active_menu {
        Some(MainMenu::File) => Some(
            column![
                text("Project files").size(16),
                row![
                    text_input("Project file path", &app.project_path_query)
                        .on_input(Message::ProjectPathChanged)
                        .width(Length::Fill),
                    utility_button("Open path", Message::OpenProject),
                    utility_button("Save path", Message::SaveProject),
                ]
                .spacing(8),
                row![
                    utility_button("Open…", Message::PickPath(PathPickerTarget::OpenProject)),
                    utility_button("Save as…", Message::PickPath(PathPickerTarget::SaveProject)),
                    text(if app.is_dirty() {
                        "Unsaved changes"
                    } else if app.project_path.is_some() {
                        "Saved"
                    } else {
                        "New project"
                    }),
                ]
                .spacing(8),
            ]
            .spacing(8)
            .into(),
        ),
        Some(MainMenu::Edit) => Some(
            row![
                utility_button("Undo", Message::Undo),
                utility_button("Redo", Message::Redo),
            ]
            .spacing(8)
            .into(),
        ),
        Some(MainMenu::Track) => Some(
            row![utility_button("Add track", Message::AddTrack)]
                .spacing(8)
                .into(),
        ),
        None => None,
    };

    let workspace_tabs = row![
        button(if app.active_workspace == WorkspacePage::Arrangement {
            "● Arrangement"
        } else {
            "Arrangement"
        })
        .style(workspace_button_style(
            app.active_workspace == WorkspacePage::Arrangement,
        ))
        .on_press(Message::SelectWorkspace(WorkspacePage::Arrangement)),
        button(if app.active_workspace == WorkspacePage::Media {
            "● Media"
        } else {
            "Media"
        })
        .style(workspace_button_style(
            app.active_workspace == WorkspacePage::Media,
        ))
        .on_press(Message::SelectWorkspace(WorkspacePage::Media)),
        button(if app.active_workspace == WorkspacePage::Project {
            "● Project"
        } else {
            "Project"
        })
        .style(workspace_button_style(
            app.active_workspace == WorkspacePage::Project,
        ))
        .on_press(Message::SelectWorkspace(WorkspacePage::Project)),
    ]
    .spacing(8);

    let workspace: Element<'_, Message> = match app.active_workspace {
        WorkspacePage::Arrangement => arrangement::view(app),
        WorkspacePage::Media => media::view(app),
        WorkspacePage::Project => project::view(app),
    };
    let project_state = if app.is_dirty() {
        format!("{project_name} · Unsaved changes")
    } else {
        project_name
    };
    let status_text = if app.status.is_empty() {
        project_state
    } else {
        app.status.clone()
    };

    let mut content = column![toolbar]
        .spacing(12)
        .padding(14)
        .height(Length::Fill);
    if let Some(menu_panel) = menu_panel {
        content = content.push(
            container(menu_panel)
                .padding(10)
                .style(iced::widget::container::rounded_box),
        );
    }
    let transport = container(
        row![text("Transport").size(14), playback_controls(app)]
            .spacing(12)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(8)
    .style(iced::widget::container::rounded_box);
    content = content
        .push(workspace_tabs)
        .push(workspace)
        .push(text(status_text))
        .push(transport);
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn menu_button_style(
    active: bool,
) -> fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    if active {
        iced::widget::button::warning
    } else {
        iced::widget::button::secondary
    }
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

fn utility_button<'a>(label: &'a str, message: Message) -> iced::widget::Button<'a, Message> {
    button(label)
        .style(iced::widget::button::secondary)
        .on_press(message)
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
