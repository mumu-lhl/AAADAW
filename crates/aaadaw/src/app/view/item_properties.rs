use super::super::{
    App, Message,
    item_properties::{ItemPropertyField, TimeUnit},
};
use iced::widget::{button, column, container, pick_list, row, text, text_input};
use iced::{Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let Some(draft) = &app.item_properties else {
        return text("No item selected").into();
    };
    let field = |id: ItemPropertyField, width| {
        text_input("", &draft.fields[id.index()])
            .on_input(move |value| Message::ItemPropertyFieldChanged(id, value))
            .on_submit(Message::ApplyItemProperties(true))
            .width(width)
    };
    let content = column![
        row![
            text("Display:").width(120),
            pick_list(
                [
                    TimeUnit::Time,
                    TimeUnit::Beats,
                    TimeUnit::Hmsf,
                    TimeUnit::Samples
                ],
                Some(draft.unit),
                Message::ItemPropertiesUnitChanged
            )
        ]
        .spacing(6),
        row![
            text("Position:").width(120),
            field(ItemPropertyField::Position, 130)
        ]
        .spacing(6),
        row![
            text("Length:").width(120),
            field(ItemPropertyField::Length, 130)
        ]
        .spacing(6),
        row![
            text("Start in source:").width(120),
            field(ItemPropertyField::SourceOffset, 130)
        ]
        .spacing(6),
        row![
            text("Fade in:").width(80),
            field(ItemPropertyField::InLength, 130)
        ]
        .spacing(6),
        row![
            text("Curve:").width(80),
            field(ItemPropertyField::InCurvature, 80),
            field(ItemPropertyField::InS, 80)
        ]
        .spacing(6),
        row![
            text("Fade out:").width(80),
            field(ItemPropertyField::OutLength, 130)
        ]
        .spacing(6),
        row![
            text("Curve:").width(80),
            field(ItemPropertyField::OutCurvature, 80),
            field(ItemPropertyField::OutS, 80)
        ]
        .spacing(6),
        text(draft.error.as_deref().unwrap_or("")).size(12),
        row![
            iced::widget::Space::new().width(Length::Fill),
            button("OK").on_press(Message::ApplyItemProperties(true)),
            button("Cancel").on_press(Message::CloseItemProperties),
            button("Apply").on_press_maybe(
                draft
                    .has_changes()
                    .then_some(Message::ApplyItemProperties(false))
            ),
        ]
        .spacing(6)
    ]
    .spacing(4);
    container(content)
        .padding(8)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
