use super::super::{
    App, Message,
    item_properties::{BatchItemProperties, ItemPropertyField, MidiItemProperties, TimeUnit},
};
use iced::widget::{button, checkbox, column, container, pick_list, row, text, text_input};
use iced::{Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    if let Some(draft) = &app.batch_item_properties {
        return batch_item_view(draft);
    }
    if let Some(draft) = &app.midi_item_properties {
        return midi_item_view(draft);
    }
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
            text("Source:").width(120),
            checkbox(draft.loop_source)
                .label("Loop source")
                .on_toggle(Message::ItemPropertyLoopSourceChanged)
        ]
        .spacing(6),
        row![
            text("").width(120),
            checkbox(draft.muted)
                .label("Mute")
                .on_toggle(Message::ItemPropertyMuteChanged)
        ]
        .spacing(6),
        row![
            text("Take name:").width(120),
            text_input("", &draft.name)
                .on_input(Message::ItemPropertyNameChanged)
                .on_submit(Message::ApplyItemProperties(true))
                .width(Length::Fill)
        ]
        .spacing(6),
        row![
            text("Start in source:").width(120),
            field(ItemPropertyField::SourceOffset, 130)
        ]
        .spacing(6),
        row![
            text("Take rate:").width(120),
            text_input("", &draft.take_rate)
                .on_input(Message::ItemPropertyTakeRateChanged)
                .on_submit(Message::ApplyItemProperties(true))
                .width(130),
            text("×")
        ]
        .spacing(6),
        row![
            text("Volume/pan:").width(120),
            field(ItemPropertyField::TakeVolume, 90),
            field(ItemPropertyField::TakePan, 90)
        ]
        .spacing(6),
        row![
            text("Phase:").width(120),
            checkbox(draft.phase_inverted)
                .label("Invert phase")
                .on_toggle(Message::ItemPropertyPhaseChanged)
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

fn batch_item_view(draft: &BatchItemProperties) -> Element<'_, Message> {
    let mute_controls: Element<'_, Message> = match draft.muted {
        Some(muted) => checkbox(muted)
            .label("Mute")
            .on_toggle(Message::BatchItemPropertyMuteChanged)
            .into(),
        None => row![
            text("Mixed values"),
            button("Mute all").on_press(Message::BatchItemPropertyMuteChanged(true)),
            button("Unmute all").on_press(Message::BatchItemPropertyMuteChanged(false)),
        ]
        .spacing(6)
        .into(),
    };
    let content = column![
        text(format!("{} selected items", draft.item_ids.len())),
        row![text("Mute:").width(120), mute_controls].spacing(6),
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
    .spacing(8);
    container(content)
        .padding(8)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn midi_item_view<'a>(draft: &'a MidiItemProperties) -> Element<'a, Message> {
    let field = |id: ItemPropertyField, width| {
        text_input("", &draft.fields[id.index()])
            .on_input(move |value| Message::ItemPropertyFieldChanged(id, value))
            .on_submit(Message::ApplyItemProperties(true))
            .width(width)
    };
    let content = column![
        row![
            text("Name:").width(120),
            text_input("", &draft.name)
                .on_input(Message::MidiItemPropertyNameChanged)
                .on_submit(Message::ApplyItemProperties(true))
                .width(Length::Fill)
        ]
        .spacing(6),
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
            text("").width(120),
            checkbox(draft.loop_source)
                .label("Loop source")
                .on_toggle(Message::ItemPropertyLoopSourceChanged)
        ]
        .spacing(6),
        row![
            text("Source length (PPQ):").width(120),
            text_input("", &draft.source_loop_length)
                .on_input(Message::MidiItemPropertySourceLoopLengthChanged)
                .on_submit(Message::ApplyItemProperties(true))
                .width(130)
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
            field(ItemPropertyField::SourceOffset, 120),
            text(if draft.unit == TimeUnit::Samples {
                "Samples"
            } else {
                "Time"
            })
        ]
        .spacing(6),
        row![
            text("").width(120),
            checkbox(draft.muted)
                .label("Mute")
                .on_toggle(Message::ItemPropertyMuteChanged)
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
