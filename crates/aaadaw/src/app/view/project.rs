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
    let project_workspace = column![
        text("Project tools").size(24),
        text("Search and run supported project commands."),
        action_search,
    ]
    .spacing(10);

    container(scrollable(
        container(project_workspace).padding([0.0, 24.0]),
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(14)
    .into()
}
