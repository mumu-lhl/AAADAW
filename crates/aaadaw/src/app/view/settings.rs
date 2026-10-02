use super::super::{App, Message, commands};
use iced::widget::{button, column, container, row, rule, scrollable, text};
use iced::{Alignment, Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let entries = commands::shortcut_entries(app);
    let categories = entries.iter().map(|entry| entry.category).fold(
        Vec::<&str>::new(),
        |mut categories, category| {
            if !categories.contains(&category) {
                categories.push(category);
            }
            categories
        },
    );
    let active_category = categories
        .iter()
        .copied()
        .find(|category| *category == app.settings_category)
        .or_else(|| categories.first().copied())
        .unwrap_or("File");

    let mut navigation = column![text("Keyboard shortcuts").size(13)].spacing(4);
    for category in categories {
        let selected = category == active_category;
        navigation = navigation.push(
            button(text(category))
                .width(Length::Fill)
                .style(if selected {
                    button::primary
                } else {
                    button::secondary
                })
                .on_press(Message::SelectSettingsCategory(category.to_owned())),
        );
    }

    let mut bindings = column![].spacing(5);
    for entry in entries
        .into_iter()
        .filter(|entry| entry.category == active_category)
    {
        let action_id = entry.id.to_owned();
        let recording = app.shortcut_capture_id.as_deref() == Some(entry.id);
        let shown_binding = if recording {
            "Press a key…".to_owned()
        } else if entry.binding.is_empty() {
            "Unassigned".to_owned()
        } else {
            entry.binding
        };
        let default_binding = if entry.default_binding.is_empty() {
            "Default: Unassigned".to_owned()
        } else {
            format!("Default: {}", entry.default_binding)
        };
        bindings = bindings.push(
            row![
                column![text(entry.label).size(13), text(default_binding).size(10)]
                    .width(Length::Fill)
                    .spacing(2),
                button(text(shown_binding))
                    .width(Length::Fixed(150.0))
                    .on_press(Message::StartShortcutCapture(action_id.clone())),
                button("Clear")
                    .width(Length::Fixed(48.0))
                    .on_press(Message::ClearShortcutBinding(action_id.clone())),
                button("Default")
                    .width(Length::Fixed(68.0))
                    .on_press(Message::RestoreShortcutDefault(action_id)),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    }

    let details = column![
        text(format!("Keyboard shortcuts · {active_category}")).size(17),
        text("Select a binding, then press Ctrl/Cmd with a letter, optionally Shift, or Space.")
            .size(12),
        rule::horizontal(1),
        scrollable(container(bindings).padding(iced::Padding::default().right(12.0)))
            .height(Length::Fill),
        text(app.shortcut_editor_feedback.clone()).size(12),
        row![
            button("Restore all defaults").on_press(Message::ResetShortcutBindings),
            iced::widget::Space::new().width(Length::Fill),
            button("Save changes").on_press(Message::SaveShortcutBindings),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(8)
    .width(Length::Fill);

    let content = row![
        container(navigation)
            .width(Length::Fixed(154.0))
            .height(Length::Fill)
            .padding(iced::Padding::default().right(10.0)),
        rule::vertical(1),
        details,
    ]
    .spacing(12)
    .height(Length::Fill);

    container(container(content).padding([12, 14]))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
