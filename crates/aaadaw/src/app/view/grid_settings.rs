use super::super::{App, Message};
use iced::widget::{button, checkbox, column, container, row, slider, text};
use iced::{Alignment, Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let swing = checkbox(app.snap_swing_enabled)
        .label("Swing grid")
        .size(14)
        .spacing(7)
        .text_size(13)
        .on_toggle(Message::SnapSwingGridChanged);
    let amount = app.snap_swing_amount_percent.0;
    let swing_amount = row![
        text("Swing amount").size(13).width(Length::Fixed(110.0)),
        slider(-100.0..=100.0, f32::from(amount), |value| {
            Message::SnapSwingAmountChanged(value.round() as i16)
        })
        .step(1.0_f32)
        .width(Length::Fill),
        text(format!("{amount}%"))
            .size(13)
            .width(Length::Fixed(48.0)),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    let controls = column![
        row![
            text("Grid line spacing").size(13),
            text(app.timeline.snap_grid.to_string()).size(13)
        ]
        .spacing(12)
        .align_y(Alignment::Center),
        swing,
        swing_amount,
        row![
            iced::widget::Space::new().width(Length::Fill),
            button("Close")
                .padding([4, 14])
                .on_press(Message::CloseSnapGridSettings)
        ]
    ]
    .spacing(13)
    .padding(16)
    .width(Length::Fill);

    container(controls)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
