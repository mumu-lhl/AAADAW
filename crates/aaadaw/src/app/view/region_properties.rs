use super::super::{App, Message, region_properties};
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Alignment, Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let Some(draft) = &app.region_properties else {
        return text("No region selected").into();
    };
    let name = text_input("", &draft.name)
        .id(iced::widget::Id::new(region_properties::NAME_INPUT_ID))
        .on_input(Message::RegionNameChanged)
        .on_submit(Message::ApplyRegionProperties)
        .width(Length::Fill);
    let number = text_input("", &draft.number)
        .id(iced::widget::Id::new(region_properties::NUMBER_INPUT_ID))
        .on_input(Message::RegionNumberChanged)
        .on_submit(Message::ApplyRegionProperties)
        .width(Length::Fixed(96.0));
    let start = text_input("", &draft.start)
        .id(iced::widget::Id::new(region_properties::START_INPUT_ID))
        .on_input(Message::RegionStartChanged)
        .on_submit(Message::ApplyRegionProperties)
        .width(Length::Fixed(136.0));
    let end = text_input("", &draft.end)
        .id(iced::widget::Id::new(region_properties::END_INPUT_ID))
        .on_input(Message::RegionEndChanged)
        .on_submit(Message::ApplyRegionProperties)
        .width(Length::Fixed(136.0));
    let content = column![
        row![text("Name:").width(52), name, text("ID:"), number]
            .spacing(8)
            .align_y(Alignment::Center),
        row![text("Start:").width(52), start, text("End:").width(42), end,]
            .spacing(8)
            .align_y(Alignment::Center),
        text(draft.error.as_deref().unwrap_or(""))
            .size(12)
            .width(Length::Fill),
        row![
            iced::widget::Space::new().width(Length::Fill),
            button("OK").on_press(Message::ApplyRegionProperties),
            button("Cancel").on_press(Message::CloseRegionProperties),
        ]
        .spacing(8)
    ]
    .spacing(6);
    container(content)
        .padding(8)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
