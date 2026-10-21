use super::super::{App, Message, item_nudge};
use iced::widget::{button, checkbox, column, combo_box, container, row, text, text_input};
use iced::{Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let draft = &app.item_nudge_dialog;
    let value_label = if draft.mode == item_nudge::Mode::Nudge {
        "by:"
    } else {
        "to:"
    };
    let input = text_input("", &draft.value)
        .on_input(Message::ItemNudgeValueChanged)
        .on_submit(if draft.mode == item_nudge::Mode::Nudge {
            Message::NudgeSelectedItems(1)
        } else {
            Message::ApplyItemNudgeSet
        })
        .width(Length::Fixed(100.0))
        .padding([2, 6])
        .size(13);
    let snap_label = match draft.unit {
        item_nudge::Unit::Grid => "Snap to grid",
        item_nudge::Unit::MeasuresBeats => "Snap to bar",
        item_nudge::Unit::ItemLengths | item_nudge::Unit::ItemSelections => "Snap to grid",
        _ => "Snap to unit",
    };
    let snap_checkbox = container(
        checkbox(draft.snap_to_unit)
            .label(snap_label)
            .size(14)
            .spacing(6)
            .text_size(13)
            .on_toggle(Message::ItemNudgeSnapToUnitChanged),
    )
    .padding(iced::Padding::ZERO.top(6.0));
    let mut footer = row![snap_checkbox].spacing(9);
    if draft.mode == item_nudge::Mode::Set
        && matches!(
            draft.target,
            item_nudge::Target::Position
                | item_nudge::Target::EndPosition
                | item_nudge::Target::Duplicate
                | item_nudge::Target::EditCursor
        )
    {
        footer = footer.push(
            container(
                checkbox(draft.relative_set)
                    .label("Relative set")
                    .size(14)
                    .spacing(6)
                    .text_size(13)
                    .on_toggle(Message::ItemNudgeRelativeSetChanged),
            )
            .padding(iced::Padding::ZERO.top(6.0)),
        );
    }
    footer = footer.push(iced::widget::Space::new().width(Length::Fill));
    if draft.mode == item_nudge::Mode::Nudge {
        footer = footer
            .push(
                button(text("Nudge left").size(13))
                    .width(Length::Fixed(90.0))
                    .padding([5, 6])
                    .on_press(Message::NudgeSelectedItems(-1)),
            )
            .push(
                button(text("Nudge right").size(13))
                    .width(Length::Fixed(91.0))
                    .padding([5, 6])
                    .on_press(Message::NudgeSelectedItems(1)),
            );
    } else {
        footer = footer
            .push(
                button(text("Get cursor").size(13))
                    .width(Length::Fixed(90.0))
                    .padding([5, 6])
                    .on_press(Message::ItemNudgeGetCursor),
            )
            .push(
                button(text("Apply move").size(13))
                    .width(Length::Fixed(91.0))
                    .padding([5, 6])
                    .on_press(Message::ApplyItemNudgeSet),
            );
    }
    let summary = draft
        .error
        .clone()
        .unwrap_or_else(|| item_position_summary(app));
    let mut controls = row![
        combo_box(
            &draft.mode_options,
            "",
            Some(&draft.mode),
            Message::ItemNudgeModeChanged
        )
        .width(Length::Fixed(75.0))
        .padding([2, 6])
        .icon(dropdown_icon())
        .size(13)
        .menu_height(Length::Fixed(200.0)),
        combo_box(
            &draft.target_options,
            "",
            Some(&draft.target),
            Message::ItemNudgeTargetChanged,
        )
        .width(Length::Fixed(106.0))
        .padding([2, 6])
        .icon(dropdown_icon())
        .size(13)
        .menu_height(Length::Fixed(240.0)),
        text(value_label).size(13).width(Length::Fixed(22.0)),
        input,
    ]
    .spacing(7);
    let has_note_value_picker =
        draft.mode == item_nudge::Mode::Nudge && draft.unit == item_nudge::Unit::Notes;
    let note_value_picker: Option<Element<'_, Message>> = has_note_value_picker.then(|| {
        combo_box(
            &draft.note_value_options,
            "",
            Some(&draft.note_value),
            Message::ItemNudgeNoteValueChanged,
        )
        .width(Length::Fixed(70.0))
        .padding([2, 6])
        .icon(dropdown_icon())
        .size(13)
        .menu_height(Length::Fixed(300.0))
        .into()
    });
    if let Some(note_value_picker) = note_value_picker {
        controls = controls.push(note_value_picker);
    }
    controls = controls.push(
        combo_box(
            match draft.mode {
                item_nudge::Mode::Nudge => &draft.nudge_unit_options,
                item_nudge::Mode::Set => &draft.set_unit_options,
            },
            "",
            Some(&draft.unit),
            Message::ItemNudgeUnitChanged,
        )
        .width(Length::Fixed(if has_note_value_picker {
            95.0
        } else {
            172.0
        }))
        .padding([2, 6])
        .icon(dropdown_icon())
        .size(13)
        .menu_height(Length::Fixed(300.0)),
    );
    let content = column![
        controls,
        iced::widget::Space::new().height(Length::Fixed(12.0)),
        text(summary).size(13),
        iced::widget::Space::new().height(Length::Fixed(20.0)),
        footer.spacing(9),
    ]
    .spacing(0);
    container(content)
        .padding(
            iced::Padding::new(0.0)
                .top(14.0)
                .right(13.0)
                .bottom(10.0)
                .left(12.0),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn dropdown_icon() -> iced::widget::text_input::Icon<iced::Font> {
    iced::widget::text_input::Icon {
        font: iced::Font::DEFAULT,
        code_point: '▾',
        size: Some(iced::Pixels(12.0)),
        spacing: 2.0,
        side: iced::widget::text_input::Side::Right,
    }
}

fn item_position_summary(app: &App) -> String {
    let selected = match app.selected_item_start_samples() {
        Ok(selected) => selected,
        Err(_) => return "Position unavailable".to_owned(),
    };
    let Some((_, sample)) = selected.first() else {
        return "[no selected items]".to_owned();
    };
    let seconds = *sample as f64 / f64::from(app.project.settings().sample_rate());
    let musical = app
        .project
        .tick_at_sample(*sample)
        .map_err(|error| error.to_string())
        .and_then(|tick| super::super::item_properties::format_marker_position(&app.project, tick))
        .unwrap_or_else(|_| "?".to_owned());
    format!(
        "Position of first selected item: {musical}   {}",
        crate::project_time::format_project_time(seconds),
    )
}
