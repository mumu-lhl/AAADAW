use super::super::{App, Message, TimeMapTab};
use super::tokens;
use aaadaw_core::TempoCurve;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    if app.time_map_tab == TimeMapTab::Meter {
        return meter_view(app);
    }
    let mut rows = column![].spacing(tokens::ROW_GAP);
    let last_index = app.tempo_map_edits.len().saturating_sub(1);
    for (index, point) in app.tempo_map_edits.iter().enumerate() {
        let is_initial = point.original_tick == Some(0);
        let musical_position = point
            .tick
            .parse::<u64>()
            .ok()
            .and_then(|tick| app.project.musical_position_at_tick(tick).ok())
            .map_or_else(
                || "—".to_owned(),
                |position| {
                    format!(
                        "{}.{}.{}",
                        position.measure(),
                        position.beat(),
                        position.tick_in_beat()
                    )
                },
            );
        let mut entry = row![
            text_input("Start tick", &point.tick)
                .on_input(move |value| Message::TempoPointTickChanged(index, value))
                .width(Length::Fixed(112.0)),
            text(musical_position).size(10).width(Length::Fixed(68.0)),
            text_input("BPM", &point.bpm)
                .on_input(move |value| Message::TempoPointBpmChanged(index, value))
                .width(Length::Fixed(90.0)),
        ]
        .spacing(tokens::SPACING_SM)
        .align_y(Alignment::Center);
        if index < last_index {
            entry = entry
                .push(
                    button(text(curve_label(point.curve)).size(12))
                        .on_press(Message::CycleTempoCurve(index)),
                )
                .push(text("to next point").size(10));
        } else {
            entry = entry.push(text("final point").size(10));
        }
        entry = entry.push(
            button("Remove")
                .on_press_maybe((!is_initial).then_some(Message::DeleteTempoPoint(index))),
        );
        rows = rows.push(entry);
    }
    let content = column![
        text("Tempo map").size(17),
        row![
            button("Tempo").on_press(Message::SelectTimeMapTab(TimeMapTab::Tempo)),
            button("Meter").on_press(Message::SelectTimeMapTab(TimeMapTab::Meter)),
        ]
        .spacing(tokens::SPACING_SM),
        text("Enter project tick positions and BPM. Each curve controls the ramp to the next point; edits are undoable and saved with the project.").size(11),
        row![
            text("Project tick").width(Length::Fixed(112.0)),
            text("Bar.Beat.Tick").width(Length::Fixed(68.0)),
            text("BPM").width(Length::Fixed(90.0)),
            text("Curve"),
        ]
        .spacing(tokens::SPACING_SM),
        scrollable(rows).height(Length::Fill),
        row![
            button("Add at edit cursor").on_press(Message::AddTempoPoint),
            button("Apply changes").on_press(Message::ApplyTempoMap),
            iced::widget::Space::new().width(Length::Fill),
            button("Undo").on_press(Message::Undo),
            button("Redo").on_press(Message::Redo),
        ]
        .spacing(tokens::SPACING_SM)
        .align_y(Alignment::Center),
        text(app.tempo_map_feedback.clone()).size(11),
    ]
    .spacing(tokens::SECTION_GAP)
    .padding(tokens::PANEL_PADDING)
    .height(Length::Fill);
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn meter_view(app: &App) -> Element<'_, Message> {
    let mut rows = column![].spacing(tokens::ROW_GAP);
    for (index, point) in app.meter_map_edits.iter().enumerate() {
        let is_initial = point.original_tick == Some(0);
        let musical_position = point
            .tick
            .parse::<u64>()
            .ok()
            .and_then(|tick| app.project.musical_position_at_tick(tick).ok())
            .map_or_else(
                || "—".to_owned(),
                |position| {
                    format!(
                        "{}.{}.{}",
                        position.measure(),
                        position.beat(),
                        position.tick_in_beat()
                    )
                },
            );
        rows = rows.push(
            row![
                text_input("Project tick", &point.tick)
                    .on_input(move |value| Message::MeterPointTickChanged(index, value))
                    .width(Length::Fixed(112.0)),
                text(musical_position).size(10).width(Length::Fixed(68.0)),
                text_input("Beats", &point.numerator)
                    .on_input(move |value| Message::MeterPointNumeratorChanged(index, value))
                    .width(Length::Fixed(70.0)),
                text("/").size(12),
                text_input("Note value", &point.denominator)
                    .on_input(move |value| Message::MeterPointDenominatorChanged(index, value))
                    .width(Length::Fixed(82.0)),
                button("Remove")
                    .on_press_maybe((!is_initial).then_some(Message::DeleteMeterPoint(index))),
            ]
            .spacing(tokens::SPACING_SM)
            .align_y(Alignment::Center),
        );
    }
    let content = column![
        text("Time-signature map").size(17),
        row![
            button("Tempo").on_press(Message::SelectTimeMapTab(TimeMapTab::Tempo)),
            button("Meter").on_press(Message::SelectTimeMapTab(TimeMapTab::Meter)),
        ]
        .spacing(tokens::SPACING_SM),
        text("Meter changes must start on a bar line. Edits apply atomically and share project Undo/Redo.").size(11),
        row![
            text("Project tick").width(Length::Fixed(112.0)),
            text("Bar.Beat.Tick").width(Length::Fixed(68.0)),
            text("Numerator").width(Length::Fixed(70.0)),
            text("/"),
            text("Denominator").width(Length::Fixed(82.0)),
        ]
        .spacing(tokens::SPACING_SM),
        scrollable(rows).height(Length::Fill),
        row![
            button("Add at edit cursor").on_press(Message::AddMeterPoint),
            button("Apply changes").on_press(Message::ApplyMeterMap),
            iced::widget::Space::new().width(Length::Fill),
            button("Undo").on_press(Message::Undo),
            button("Redo").on_press(Message::Redo),
        ]
        .spacing(tokens::SPACING_SM)
        .align_y(Alignment::Center),
        text(app.meter_map_feedback.clone()).size(11),
    ]
    .spacing(tokens::SECTION_GAP)
    .padding(tokens::PANEL_PADDING)
    .height(Length::Fill);
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn curve_label(curve: TempoCurve) -> &'static str {
    match curve {
        TempoCurve::Step => "Step",
        TempoCurve::Linear => "Linear BPM",
        TempoCurve::Logarithmic => "Log BPM",
        TempoCurve::Bézier => "Bézier",
    }
}
