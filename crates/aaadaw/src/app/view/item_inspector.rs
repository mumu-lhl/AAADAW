use super::super::{App, Message};
use aaadaw_app::{AudioAssetSourceStatus, AudioAssetSourceStatusEntry};
use aaadaw_core::ItemId;
use iced::widget::{Button, button, column, container, row, scrollable, text, text_input};
use iced::{Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let content = match app.timeline.selected_item {
        Some(item_id) => selected_item_view(app, item_id),
        None => column![
            text("Inspector").size(12),
            text("Select an Item to inspect it.").size(12)
        ]
        .spacing(6)
        .into(),
    };
    container(scrollable(
        container(content).padding(iced::Padding::default().right(12.0)),
    ))
    .width(Length::Fill)
    .height(Length::Fixed(156.0))
    .padding([8, 10])
    .style(|_| container::Style {
        background: Some(iced::Color::from_rgb8(30, 34, 37).into()),
        ..container::Style::default()
    })
    .into()
}

fn selected_item_view(app: &App, item_id: ItemId) -> Element<'_, Message> {
    if let Some(item) = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == item_id)
    {
        let status = app.audio_asset_source_statuses.get(item.media_ref());
        let mut actions = row![
            text(format!("Audio · {}", item.media_ref())).width(Length::Fill),
            action_button("Duplicate", Message::DuplicateAudioItem(item_id)),
            danger_button("Delete", Message::DeleteAudioItem(item_id)),
        ]
        .spacing(6);
        #[cfg(any(feature = "jack-backend", feature = "pipewire-backend"))]
        {
            actions = actions.push(action_button(
                "Seek",
                Message::SeekToItem(item.start_sample()),
            ));
        }
        if status.is_some_and(can_relink_source) {
            actions = actions.push(action_button("Relink", Message::RelinkAudioItem(item_id)));
        }
        let position_controls = if let Some(query) = app.audio_item_start_edits.get(&item_id) {
            row![
                text("Start sample"),
                text_input("sample", query)
                    .on_input(move |query| Message::AudioItemStartSampleChanged(item_id, query))
                    .on_submit(Message::CommitAudioItemStartSample(item_id))
                    .width(160),
                action_button("Set", Message::CommitAudioItemStartSample(item_id)),
                action_button("Cancel", Message::CancelAudioItemStartSampleEdit(item_id)),
            ]
            .spacing(6)
        } else {
            row![
                text(format!("Start sample: {}", item.start_sample())).width(Length::Fill),
                action_button("Edit", Message::BeginAudioItemStartSampleEdit(item_id)),
            ]
            .spacing(6)
        };
        let start_seconds =
            item.start_sample() as f64 / app.project.settings().sample_rate() as f64;
        let duration_seconds =
            item.length_samples() as f64 / app.project.settings().sample_rate() as f64;
        let source = status.map_or_else(String::new, |entry| {
            format!("Source: {}", source_status_label(entry.status))
        });
        let mut source_status = row![text(source).width(Length::Fill)];
        if status.is_some_and(can_reimport_source) {
            source_status = source_status.push(action_button(
                "Reimport",
                Message::ReimportAudioItem(item_id),
            ));
        }
        column![
            actions,
            row![text(format!(
                "Position {start_seconds:.3} s · Length {duration_seconds:.3} s"
            )),]
            .spacing(12),
            source_status,
            position_controls,
            row![
                text("Nudge"),
                action_button("−1 s", Message::NudgeAudioItem(item_id, -1, 1_000)),
                action_button("−100 ms", Message::NudgeAudioItem(item_id, -1, 100)),
                action_button("−10 ms", Message::NudgeAudioItem(item_id, -1, 10)),
                action_button("+10 ms", Message::NudgeAudioItem(item_id, 1, 10)),
                action_button("+100 ms", Message::NudgeAudioItem(item_id, 1, 100)),
                action_button("+1 s", Message::NudgeAudioItem(item_id, 1, 1_000)),
            ]
            .spacing(4),
        ]
        .spacing(6)
        .into()
    } else if let Some(item) = app
        .project
        .midi_items()
        .iter()
        .find(|item| item.id() == item_id)
    {
        let mut item_actions = row![
            text(format!(
                "MIDI Item · tick {} · length {}",
                item.start_tick(),
                item.length_ticks()
            ))
            .width(Length::Fill),
            action_button("− beat", Message::NudgeMidiItem(item_id, -1)),
            action_button("+ beat", Message::NudgeMidiItem(item_id, 1)),
            action_button("Add C4", Message::AddMidiNote(item_id)),
        ]
        .spacing(6);
        if !item.notes().is_empty() {
            item_actions = item_actions.push(action_button(
                "Quantize 1/16",
                Message::QuantizeMidiItem(item_id),
            ));
        }
        item_actions = item_actions.push(danger_button("Delete", Message::DeleteMidiItem(item_id)));
        let notes = item
            .notes()
            .iter()
            .map(|note| {
                row![
                    text(format!(
                        "{} · tick {} · {} ticks · velocity {}",
                        midi_pitch_name(note.pitch()),
                        note.tick(),
                        note.duration(),
                        note.velocity()
                    ))
                    .width(Length::Fill),
                    action_button("−1/16", Message::NudgeMidiNote(item_id, note.id(), -1)),
                    action_button("+1/16", Message::NudgeMidiNote(item_id, note.id(), 1)),
                    action_button(
                        "Pitch−",
                        Message::AdjustMidiNotePitch(item_id, note.id(), -1)
                    ),
                    action_button(
                        "Pitch+",
                        Message::AdjustMidiNotePitch(item_id, note.id(), 1)
                    ),
                    action_button(
                        "Vel−",
                        Message::AdjustMidiNoteVelocity(item_id, note.id(), -1)
                    ),
                    action_button(
                        "Vel+",
                        Message::AdjustMidiNoteVelocity(item_id, note.id(), 1)
                    ),
                    danger_button("Delete", Message::DeleteMidiNote(item_id, note.id())),
                ]
                .spacing(5)
                .into()
            })
            .collect::<Vec<Element<'_, Message>>>();
        column![item_actions, column(notes).spacing(4)]
            .spacing(6)
            .into()
    } else {
        column![text("Selected Item no longer exists.")].into()
    }
}

fn action_button<'a>(label: &'a str, message: Message) -> Button<'a, Message> {
    button(label)
        .style(iced::widget::button::secondary)
        .on_press(message)
        .padding([2, 5])
}

fn danger_button<'a>(label: &'a str, message: Message) -> Button<'a, Message> {
    button(label)
        .style(iced::widget::button::danger)
        .on_press(message)
        .padding([2, 5])
}

fn can_relink_source(entry: &AudioAssetSourceStatusEntry) -> bool {
    entry.is_external_link && entry.status == AudioAssetSourceStatus::Missing
}

fn can_reimport_source(entry: &AudioAssetSourceStatusEntry) -> bool {
    !entry.is_external_link && entry.status == AudioAssetSourceStatus::Changed
}

fn source_status_label(status: AudioAssetSourceStatus) -> &'static str {
    match status {
        AudioAssetSourceStatus::Linked => "linked",
        AudioAssetSourceStatus::Untracked => "untracked",
        AudioAssetSourceStatus::Unchanged => "unchanged",
        AudioAssetSourceStatus::Changed => "changed",
        AudioAssetSourceStatus::Missing => "missing",
        AudioAssetSourceStatus::Unverified => "unverified",
    }
}

fn midi_pitch_name(pitch: u8) -> String {
    const PITCH_CLASSES: [&str; 12] = [
        "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
    ];
    let pitch_class = PITCH_CLASSES[usize::from(pitch % 12)];
    let octave = i16::from(pitch) / 12 - 1;
    format!("{pitch_class}{octave}")
}
