use super::super::{App, Message, TimeMapTab};
use super::tokens;
use aaadaw_core::TempoCurve;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

const PROJECT_TICK_COLUMN_WIDTH: f32 = 104.0;
const MUSICAL_POSITION_COLUMN_WIDTH: f32 = 104.0;
const BPM_COLUMN_WIDTH: f32 = 78.0;

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
                .width(Length::Fixed(PROJECT_TICK_COLUMN_WIDTH)),
            text(musical_position)
                .size(10)
                .width(Length::Fixed(MUSICAL_POSITION_COLUMN_WIDTH)),
            text_input("BPM", &point.bpm)
                .on_input(move |value| Message::TempoPointBpmChanged(index, value))
                .width(Length::Fixed(BPM_COLUMN_WIDTH)),
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
            text("Project tick").width(Length::Fixed(PROJECT_TICK_COLUMN_WIDTH)),
            text("Bar.Beat.Tick").width(Length::Fixed(MUSICAL_POSITION_COLUMN_WIDTH)),
            text("BPM").width(Length::Fixed(BPM_COLUMN_WIDTH)),
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

pub(super) fn mobile_view(app: &App) -> Element<'_, Message> {
    if app.time_map_tab == TimeMapTab::Meter {
        mobile_meter_view(app)
    } else {
        mobile_tempo_view(app)
    }
}

fn mobile_tempo_view(app: &App) -> Element<'_, Message> {
    let last_index = app.tempo_map_edits.len().saturating_sub(1);
    let mut rows = column![].spacing(tokens::ROW_GAP);
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
        let mut actions = row![]
            .spacing(tokens::SPACING_XS)
            .align_y(Alignment::Center);
        if index < last_index {
            actions = actions.push(
                button(curve_label(point.curve))
                    .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
                    .on_press(Message::CycleTempoCurve(index)),
            );
        } else {
            actions = actions.push(text("Final point").size(11));
        }
        if !is_initial {
            actions = actions.push(
                button("Remove")
                    .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
                    .on_press(Message::DeleteTempoPoint(index)),
            );
        }
        rows = rows.push(
            container(
                column![
                    text(format!("Tempo point {}", index + 1)).size(13),
                    row![
                        column![
                            text("Project tick").size(10),
                            text_input("Tick", &point.tick)
                                .on_input(move |value| Message::TempoPointTickChanged(index, value))
                                .padding([tokens::SPACING_LG, tokens::SPACING_SM])
                                .width(Length::Fill),
                        ]
                        .width(Length::Fill),
                        column![
                            text("BPM").size(10),
                            text_input("BPM", &point.bpm)
                                .on_input(move |value| Message::TempoPointBpmChanged(index, value))
                                .padding([tokens::SPACING_LG, tokens::SPACING_SM])
                                .width(Length::Fill),
                        ]
                        .width(Length::Fill),
                    ]
                    .spacing(tokens::SPACING_SM),
                    text(format!("Bar.Beat.Tick · {musical_position}")).size(10),
                    actions,
                ]
                .spacing(tokens::SPACING_XS),
            )
            .width(Length::Fill)
            .padding(tokens::PANEL_PADDING)
            .style(iced::widget::container::rounded_box),
        );
    }
    let content = column![
        mobile_tabs(),
        text("Tempo changes start at project ticks. Curve sets the ramp to the next point.")
            .size(11),
        scrollable(rows).height(Length::Fill),
        row![
            mobile_action_button("Add at cursor", Message::AddTempoPoint),
            mobile_action_button("Apply changes", Message::ApplyTempoMap),
        ]
        .spacing(tokens::SPACING_XS),
        row![
            mobile_action_button("Undo", Message::Undo),
            mobile_action_button("Redo", Message::Redo),
        ]
        .spacing(tokens::SPACING_XS),
        text(app.tempo_map_feedback.clone()).size(11),
    ]
    .spacing(tokens::SPACING_SM)
    .padding(tokens::PANEL_PADDING)
    .height(Length::Fill);
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn mobile_meter_view(app: &App) -> Element<'_, Message> {
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
        let numeric_fields = row![
            column![
                text("Beats").size(10),
                text_input("Numerator", &point.numerator)
                    .on_input(move |value| Message::MeterPointNumeratorChanged(index, value))
                    .padding([tokens::SPACING_LG, tokens::SPACING_SM])
                    .width(Length::Fill),
            ]
            .width(Length::Fill),
            column![
                text("Note value").size(10),
                text_input("Denominator", &point.denominator)
                    .on_input(move |value| Message::MeterPointDenominatorChanged(index, value))
                    .padding([tokens::SPACING_LG, tokens::SPACING_SM])
                    .width(Length::Fill),
            ]
            .width(Length::Fill),
        ]
        .spacing(tokens::SPACING_SM);
        let tick_field = column![
            text("Project tick").size(10),
            text_input("Tick", &point.tick)
                .on_input(move |value| Message::MeterPointTickChanged(index, value))
                .padding([tokens::SPACING_LG, tokens::SPACING_SM])
                .width(Length::Fill),
        ];
        let mut actions = row![
            text(if is_initial {
                "Initial signature"
            } else {
                "Time signature"
            })
            .size(11)
        ]
        .spacing(tokens::SPACING_XS)
        .align_y(Alignment::Center);
        if !is_initial {
            actions = actions.push(
                button("Remove")
                    .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
                    .on_press(Message::DeleteMeterPoint(index)),
            );
        }
        rows = rows.push(
            container(
                column![
                    text(format!("Meter point {}", index + 1)).size(13),
                    tick_field,
                    numeric_fields,
                    text(format!("Bar.Beat.Tick · {musical_position}")).size(10),
                    actions,
                ]
                .spacing(tokens::SPACING_XS),
            )
            .width(Length::Fill)
            .padding(tokens::PANEL_PADDING)
            .style(iced::widget::container::rounded_box),
        );
    }
    let content = column![
        mobile_tabs(),
        text("Meter changes must start on a bar line. Apply commits all edits together.").size(11),
        scrollable(rows).height(Length::Fill),
        row![
            mobile_action_button("Add at cursor", Message::AddMeterPoint),
            mobile_action_button("Apply changes", Message::ApplyMeterMap),
        ]
        .spacing(tokens::SPACING_XS),
        row![
            mobile_action_button("Undo", Message::Undo),
            mobile_action_button("Redo", Message::Redo),
        ]
        .spacing(tokens::SPACING_XS),
        text(app.meter_map_feedback.clone()).size(11),
    ]
    .spacing(tokens::SPACING_SM)
    .padding(tokens::PANEL_PADDING)
    .height(Length::Fill);
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn mobile_tabs() -> Element<'static, Message> {
    row![
        mobile_action_button("Tempo", Message::SelectTimeMapTab(TimeMapTab::Tempo)),
        mobile_action_button("Meter", Message::SelectTimeMapTab(TimeMapTab::Meter)),
    ]
    .spacing(tokens::SPACING_XS)
    .into()
}

fn mobile_action_button<'a>(label: &'a str, message: Message) -> iced::widget::Button<'a, Message> {
    button(label)
        .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
        .width(Length::Fill)
        .on_press(message)
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
                    .width(Length::Fixed(PROJECT_TICK_COLUMN_WIDTH)),
                text(musical_position)
                    .size(10)
                    .width(Length::Fixed(MUSICAL_POSITION_COLUMN_WIDTH)),
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
            text("Project tick").width(Length::Fixed(PROJECT_TICK_COLUMN_WIDTH)),
            text("Bar.Beat.Tick").width(Length::Fixed(MUSICAL_POSITION_COLUMN_WIDTH)),
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
