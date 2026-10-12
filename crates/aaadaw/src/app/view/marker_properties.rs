use super::super::{App, Message, marker_properties};
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Alignment, Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let Some(draft) = &app.marker_properties else {
        return text("No marker selected").into();
    };
    let name = text_input("", &draft.name)
        .id(iced::widget::Id::new(marker_properties::NAME_INPUT_ID))
        .on_input(Message::MarkerNameChanged)
        .on_submit(Message::ApplyMarkerProperties)
        .width(Length::Fill);
    let position = text_input("", &draft.position)
        .id(iced::widget::Id::new(marker_properties::POSITION_INPUT_ID))
        .on_input(Message::MarkerPositionChanged)
        .on_submit(Message::ApplyMarkerProperties)
        .width(Length::Fixed(116.0));
    let marker_number = text_input("", &draft.number)
        .id(iced::widget::Id::new(marker_properties::NUMBER_INPUT_ID))
        .on_input(Message::MarkerNumberChanged)
        .on_submit(Message::ApplyMarkerProperties)
        .width(Length::Fixed(96.0));
    let content = column![
        row![text("Name:").width(52), name, text("ID:"), marker_number,]
            .spacing(8)
            .align_y(Alignment::Center),
        row![
            text("Position:").width(72),
            position,
            iced::widget::Space::new().width(Length::Fill),
            button("Manager...").on_press(Message::OpenMarkerManager),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        row![
            text("Lane:").width(52),
            text("Lane editing is not supported yet").size(12),
        ]
        .align_y(Alignment::Center),
        text(draft.error.as_deref().unwrap_or(""))
            .size(12)
            .width(Length::Fill),
        row![
            iced::widget::Space::new().width(Length::Fill),
            button("OK").on_press(Message::ApplyMarkerProperties),
            button("Cancel").on_press(Message::CloseMarkerProperties),
        ]
        .spacing(8)
    ]
    .spacing(4);
    container(content)
        .padding(8)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
