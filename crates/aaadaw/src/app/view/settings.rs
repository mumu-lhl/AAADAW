use super::super::{App, Message, commands};
use iced::widget::{button, column, container, row, rule, scrollable, text};
use iced::{Alignment, Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let mut bindings = column![].spacing(3);
    let mut category = None;
    for entry in commands::shortcut_entries(app) {
        if category != Some(entry.category) {
            category = Some(entry.category);
            bindings = bindings
                .push(text(entry.category).size(12))
                .push(rule::horizontal(1));
        }
        let action_id = entry.id.to_owned();
        let recording = app.shortcut_capture_id.as_deref() == Some(entry.id);
        let shown_binding = if recording {
            "Press a key…".to_owned()
        } else if entry.binding.is_empty() {
            "Unassigned".to_owned()
        } else {
            entry.binding
        };
        bindings = bindings.push(
            row![
                text(entry.label).width(Length::Fill),
                button(text(shown_binding))
                    .width(Length::Fixed(250.0))
                    .on_press(Message::StartShortcutCapture(action_id.clone())),
                button("Clear")
                    .width(Length::Fixed(48.0))
                    .on_press(Message::ClearShortcutBinding(action_id)),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }

    let content = column![
        text("Settings").size(22),
        text("Keyboard shortcuts").size(15),
        text("Select a binding, then press Ctrl/Cmd with a letter, optionally Shift, or Space."),
        scrollable(container(bindings).padding(iced::Padding::default().right(12.0)))
            .height(Length::Fill),
        text(app.shortcut_editor_feedback.clone()).size(12),
        row![
            button("Restore defaults").on_press(Message::ResetShortcutBindings),
            iced::widget::Space::new().width(Length::Fill),
            button("Save changes").on_press(Message::SaveShortcutBindings),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(8);

    container(container(content).padding([12, 16]))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
