use super::super::{App, Message, commands};
use iced::widget::{button, column, container, row, rule, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let action_search = row![
        text_input("Search actions…", &app.action_query)
            .on_input(Message::ActionQueryChanged)
            .on_submit(Message::RunActionQuery)
            .width(Length::Fill),
        button("Run").on_press(Message::RunActionQuery),
    ]
    .spacing(8);
    let mut shortcut_rows = column![].spacing(4);
    let mut category = None;
    for entry in commands::shortcut_entries(app) {
        if category != Some(entry.category) {
            category = Some(entry.category);
            shortcut_rows = shortcut_rows
                .push(text(entry.category).size(11))
                .push(rule::horizontal(1));
        }
        let id = entry.id.to_owned();
        shortcut_rows = shortcut_rows.push(
            row![
                text(entry.label).width(Length::Fill),
                text_input("Unassigned", &entry.binding)
                    .on_input(move |binding| Message::ShortcutBindingChanged(id.clone(), binding))
                    .width(Length::Fixed(180.0)),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        );
    }
    let project_workspace = column![
        text("Project tools").size(24),
        text("Run supported commands or change their keyboard shortcuts."),
        action_search,
        row![
            text("Keyboard shortcuts").size(18).width(Length::Fill),
            button("Reset").on_press(Message::ResetShortcutBindings),
            button("Save").on_press(Message::SaveShortcutBindings),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        shortcut_rows,
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
