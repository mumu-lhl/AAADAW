use super::super::{App, Message, item_properties::FadeField};
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let Some(draft) = &app.item_properties else {
        return text("No item selected").into();
    };
    let field = |id: FadeField, width| {
        text_input("", &draft.fields[id.index()])
            .on_input(move |value| Message::ItemFadeFieldChanged(id, value))
            .on_submit(Message::ApplyItemProperties(true))
            .width(width)
    };
    let content = column![
        row![text("Fade in:").width(80), field(FadeField::InLength, 130)].spacing(6),
        row![
            text("Curve:").width(80),
            field(FadeField::InCurvature, 80),
            field(FadeField::InS, 80)
        ]
        .spacing(6),
        row![
            text("Fade out:").width(80),
            field(FadeField::OutLength, 130)
        ]
        .spacing(6),
        row![
            text("Curve:").width(80),
            field(FadeField::OutCurvature, 80),
            field(FadeField::OutS, 80)
        ]
        .spacing(6),
        text(draft.error.as_deref().unwrap_or("")).size(12),
        row![
            iced::widget::Space::new().width(Length::Fill),
            button("OK").on_press(Message::ApplyItemProperties(true)),
            button("Cancel").on_press(Message::CloseItemProperties),
            button("Apply").on_press(Message::ApplyItemProperties(false)),
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
