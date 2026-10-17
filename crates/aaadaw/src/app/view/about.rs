use super::super::Message;
use super::tokens;
use iced::widget::{button, column, container, text};
use iced::{Alignment, Element, Length};

pub(super) fn view() -> Element<'static, Message> {
    container(
        column![
            text("AAADAW").size(28),
            text(format!("Version {}", env!("CARGO_PKG_VERSION"))).size(14),
            text("Digital audio workstation"),
            text("https://github.com/mumu-lhl/AAADAW").size(12),
            button("Close").on_press(Message::CloseAbout),
        ]
        .spacing(tokens::SECTION_GAP)
        .align_x(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(tokens::PANEL_PADDING)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}
