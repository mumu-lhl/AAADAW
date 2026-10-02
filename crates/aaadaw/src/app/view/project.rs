use super::super::{App, Message};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let action_search = row![
        text_input("Search actions…", &app.action_query)
            .on_input(Message::ActionQueryChanged)
            .on_submit(Message::RunActionQuery)
            .width(Length::Fill),
        button("Run").on_press(Message::RunActionQuery),
    ]
    .spacing(8);
    #[cfg(feature = "jack-backend")]
    let shortcut_help = text(
        "Ctrl/Cmd+Z undo · Ctrl/Cmd+Shift+Z redo · Ctrl/Cmd+S save · Ctrl/Cmd+O open · Space play/stop",
    );
    #[cfg(not(feature = "jack-backend"))]
    let shortcut_help =
        text("Ctrl/Cmd+Z undo · Ctrl/Cmd+Shift+Z redo · Ctrl/Cmd+S save · Ctrl/Cmd+O open");
    let project_workspace = column![
        text("Project tools").size(24),
        text("Run supported commands or use the keyboard shortcuts."),
        action_search,
        text("Keyboard shortcuts").size(18),
        shortcut_help,
    ]
    .spacing(14);

    container(scrollable(project_workspace))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(14)
        .into()
}
