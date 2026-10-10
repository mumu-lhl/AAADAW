use super::super::{App, Message, commands};
use iced::widget::{button, column, container, row, rule, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let mut actions = column![
        row![
            text("Shortcut").width(Length::Fixed(140.0)),
            text("Description").width(Length::Fill),
            text("State").width(Length::Fixed(48.0)),
        ]
        .spacing(4),
        rule::horizontal(1)
    ]
    .spacing(1);
    for entry in app.action_list_entries() {
        let Some(id) = commands::stable_id(entry.id) else {
            continue;
        };
        let selected = app.action_list.selected.as_ref() == Some(&id);
        let state =
            commands::toggle_state(app, entry.id).map_or("", |on| if on { "on" } else { "off" });
        actions = actions.push(
            button(
                row![
                    text(entry.shortcut.unwrap_or_default())
                        .size(12)
                        .width(Length::Fixed(140.0)),
                    text(format!("{}: {}", entry.category, entry.label))
                        .size(12)
                        .width(Length::Fill),
                    text(state).size(12).width(Length::Fixed(48.0)),
                ]
                .spacing(4),
            )
            .width(Length::Fill)
            .padding([2, 0])
            .style(if selected {
                button::primary
            } else {
                button::text
            })
            .on_press(Message::ActionListSelect(id)),
        );
    }
    let selected = app.action_list_selected_entry();
    // REAPER enables Run for a selected row even when the action has no effect
    // in the current project (for example Undo in an empty history).
    let can_run = selected.is_some();
    let mut shortcuts = column![].spacing(2);
    if let Some(id) = selected
        .as_ref()
        .and_then(|entry| commands::stable_id(entry.id))
    {
        for (index, binding) in app.action_list_bindings(&id).iter().enumerate() {
            shortcuts = shortcuts.push(
                button(text(commands::format_shortcut_label(&binding.config_label())).size(12))
                    .padding([2, 4])
                    .width(Length::Fill)
                    .style(if app.action_list.selected_binding == Some(index) {
                        button::primary
                    } else {
                        button::text
                    })
                    .on_press(Message::ActionListSelectBinding(index)),
            );
        }
    }
    let capture = app.action_list.capture.is_some();
    let feedback = if capture {
        format!(
            "{}{}Press a shortcut; Escape cancels",
            app.action_list.feedback,
            if app.action_list.feedback.is_empty() {
                ""
            } else {
                "; "
            }
        )
    } else {
        app.action_list.feedback.clone()
    };
    container(
        column![
            row![
                text("Filter").size(12),
                text_input("", &app.action_list.query)
                    .on_input(Message::ActionListQueryChanged)
                    .size(12),
                button(text("Clear").size(12))
                    .on_press(Message::ActionListQueryChanged(String::new())),
                button(text("Find shortcut...").size(12)).on_press(Message::ActionListFindShortcut),
                text("Section: Main").size(12)
            ]
            .spacing(6)
            .align_y(Alignment::Center),
            scrollable(actions).height(Length::Fill),
            text("Shortcuts for selected action").size(12),
            row![
                scrollable(shortcuts)
                    .height(Length::Fixed(78.0))
                    .width(Length::Fill),
                column![
                    button(text("Add...").size(12)).on_press_maybe(
                        (selected.is_some() && !capture).then_some(Message::ActionListAddBinding)
                    ),
                    button(text("Delete").size(12)).on_press_maybe(
                        (app.action_list.selected_binding.is_some() && !capture)
                            .then_some(Message::ActionListDeleteBinding)
                    ),
                ]
                .spacing(4),
                column![
                    button(text("New action...").size(12)).on_press(Message::OpenActionMacroEditor),
                    row![
                        button(text("Run").size(12)).on_press_maybe(
                            (can_run && !capture).then_some(Message::ActionListRun(false))
                        ),
                        button(text("Run/close").size(12)).on_press_maybe(
                            (can_run && !capture).then_some(Message::ActionListRun(true))
                        ),
                        button(text("Close").size(12)).on_press(Message::CloseActionList),
                    ]
                    .spacing(4),
                ]
                .spacing(4),
            ]
            .spacing(8),
            text(feedback).size(11),
        ]
        .spacing(6),
    )
    .padding(12)
    .height(Length::Fill)
    .width(Length::Fill)
    .into()
}

pub(super) fn input_view(app: &App) -> Element<'_, Message> {
    let label = app
        .action_list
        .input_draft
        .map(|binding| commands::format_shortcut_label(&binding.config_label()))
        .unwrap_or_default();
    let action = app
        .action_list
        .input_action
        .as_ref()
        .and_then(|id| commands::label_for_id(app, id))
        .unwrap_or_default();
    container(
        column![
            text(action).size(12),
            text("Press a key or key combination").size(12),
            container(text(label).size(16))
                .padding(8)
                .width(Length::Fill)
                .style(container::bordered_box),
            text(&app.action_list.feedback).size(11),
            iced::widget::Space::new().height(Length::Fill),
            row![
                iced::widget::Space::new().width(Length::Fill),
                button(text("OK").size(12)).on_press_maybe(
                    app.action_list
                        .input_draft
                        .is_some()
                        .then_some(Message::ActionInputConfirm)
                ),
                button(text("Cancel").size(12)).on_press(Message::ActionInputCancel),
            ]
            .spacing(6)
        ]
        .spacing(8),
    )
    .padding(12)
    .height(Length::Fill)
    .width(Length::Fill)
    .into()
}
