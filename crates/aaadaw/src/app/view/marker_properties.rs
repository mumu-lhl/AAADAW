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
    let content = column![
        row![text("Name:").width(64), name]
            .spacing(8)
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
    .spacing(8);
    container(content)
        .padding(10)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
