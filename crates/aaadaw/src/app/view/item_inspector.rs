use super::super::{App, Message};
use super::tokens;
use aaadaw_app::{AudioAssetSourceStatus, AudioAssetSourceStatusEntry};
use aaadaw_core::ItemId;
use iced::widget::{Button, button, column, container, row, scrollable, text, text_input};
use iced::{Element, Length};

const EMPTY_INSPECTOR_HEIGHT: f32 = 52.0;
const SELECTED_INSPECTOR_HEIGHT: f32 = 156.0;

pub(super) fn view(app: &App) -> Element<'_, Message> {
    view_with_touch_targets(app, false)
}

pub(super) fn touch_view(app: &App) -> Element<'_, Message> {
    view_with_touch_targets(app, true)
}

fn view_with_touch_targets(app: &App, touch_targets: bool) -> Element<'_, Message> {
    let has_selection = app.timeline.selected_item.is_some();
    let height = if has_selection {
        SELECTED_INSPECTOR_HEIGHT
    } else {
        EMPTY_INSPECTOR_HEIGHT
    };
    let content = match app.timeline.selected_item {
        Some(item_id) => selected_item_view(app, item_id, touch_targets),
        None => column![
            text("Inspector").size(12),
            text("Select an Item to inspect it.").size(12)
        ]
        .spacing(6)
        .into(),
    };
    let content = container(content).padding(iced::Padding::default().right(12.0));
    let content: Element<'_, Message> = if has_selection {
        scrollable(content).into()
    } else {
        content.into()
    };
    container(content)
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .padding([8, 10])
        .style(|_| container::Style {
            background: Some(iced::Color::from_rgb8(30, 34, 37).into()),
            ..container::Style::default()
        })
        .into()
}

fn selected_item_view(app: &App, item_id: ItemId, touch_targets: bool) -> Element<'_, Message> {
    let buttons = ItemInspectorButtons { touch_targets };
    if let Some(item) = app
        .project
        .audio_items()
        .iter()
        .find(|item| item.id() == item_id)
    {
        let status = app.audio_asset_source_statuses.get(item.media_ref());
        let mut actions = row![
            text(format!("Audio · {}", item.media_ref())).width(Length::Fill),
            buttons.action("Duplicate", Message::DuplicateAudioItem(item_id)),
            buttons.danger("Delete", Message::DeleteAudioItem(item_id)),
        ]
        .spacing(6);
        #[cfg(feature = "audio-device")]
        {
            actions =
                actions.push(buttons.action("Seek", Message::SeekToItem(item.start_sample())));
        }
        if status.is_some_and(can_relink_source) {
            actions = actions.push(buttons.action("Relink", Message::RelinkAudioItem(item_id)));
        }
        let position_controls = if let Some(query) = app.audio_item_start_edits.get(&item_id) {
            row![
                text("Start sample"),
                text_input("sample", query)
                    .on_input(move |query| Message::AudioItemStartSampleChanged(item_id, query))
                    .on_submit(Message::CommitAudioItemStartSample(item_id))
                    .width(160),
                buttons.action("Set", Message::CommitAudioItemStartSample(item_id)),
                buttons.action("Cancel", Message::CancelAudioItemStartSampleEdit(item_id)),
            ]
            .spacing(6)
        } else {
            row![
                text(format!("Start sample: {}", item.start_sample())).width(Length::Fill),
                buttons.action("Edit", Message::BeginAudioItemStartSampleEdit(item_id)),
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
            source_status =
                source_status.push(buttons.action("Reimport", Message::ReimportAudioItem(item_id)));
        }
        column![
            action_row(actions, touch_targets),
            row![text(format!(
                "Position {start_seconds:.3} s · Length {duration_seconds:.3} s"
            )),]
            .spacing(12),
            action_row(source_status, touch_targets),
            action_row(position_controls, touch_targets),
            action_row(
                row![
                    text("Nudge"),
                    buttons.action("−1 s", Message::NudgeAudioItem(item_id, -1, 1_000)),
                    buttons.action("−100 ms", Message::NudgeAudioItem(item_id, -1, 100)),
                    buttons.action("−10 ms", Message::NudgeAudioItem(item_id, -1, 10)),
                    buttons.action("+10 ms", Message::NudgeAudioItem(item_id, 1, 10)),
                    buttons.action("+100 ms", Message::NudgeAudioItem(item_id, 1, 100)),
                    buttons.action("+1 s", Message::NudgeAudioItem(item_id, 1, 1_000)),
                ]
                .spacing(4),
                touch_targets
            ),
        ]
        .spacing(6)
        .into()
    } else if let Some(item) = app
        .project
        .midi_items()
        .iter()
        .find(|item| item.id() == item_id)
    {
        let edited_name = app
            .midi_item_name_edits
            .get(&item_id)
            .map_or(item.name(), String::as_str);
        let has_name_error = app.midi_item_name_errors.contains_key(&item_id);
        let name_input = text_input("MIDI item name", edited_name)
            .on_input(move |name| Message::MidiItemNameChanged(item_id, name))
            .on_submit(Message::CommitMidiItemName(item_id))
            .style(move |theme, status| {
                let mut style = text_input::default(theme, status);
                if has_name_error {
                    style.border.color = iced::Color::from_rgb8(237, 77, 68);
                }
                style
            })
            .width(Length::Fill);
        let mut item_actions = row![
            text(format!(
                "Tick {} · length {}",
                item.start_tick(),
                item.length_ticks()
            ))
            .width(Length::Fill),
            buttons.action("Piano roll…", Message::OpenMidiEditor(item_id)),
            buttons.action("− beat", Message::NudgeMidiItem(item_id, -1)),
            buttons.action("+ beat", Message::NudgeMidiItem(item_id, 1)),
            buttons.action("Add C4", Message::AddMidiNote(item_id)),
        ]
        .spacing(6);
        if !item.notes().is_empty() {
            item_actions = item_actions
                .push(buttons.action("Quantize 1/16", Message::QuantizeMidiItem(item_id)));
        }
        item_actions =
            item_actions.push(buttons.danger("Delete", Message::DeleteMidiItem(item_id)));
        let notes = item
            .notes()
            .iter()
            .map(|note| {
                action_row(
                    row![
                        text(format!(
                            "{} · tick {} · {} ticks · velocity {}",
                            midi_pitch_name(note.pitch()),
                            note.tick(),
                            note.duration(),
                            note.velocity()
                        ))
                        .width(Length::Fill),
                        buttons.action("−1/16", Message::NudgeMidiNote(item_id, note.id(), -1)),
                        buttons.action("+1/16", Message::NudgeMidiNote(item_id, note.id(), 1)),
                        buttons.action(
                            "Pitch−",
                            Message::AdjustMidiNotePitch(item_id, note.id(), -1)
                        ),
                        buttons.action(
                            "Pitch+",
                            Message::AdjustMidiNotePitch(item_id, note.id(), 1)
                        ),
                        buttons.action(
                            "Vel−",
                            Message::AdjustMidiNoteVelocity(item_id, note.id(), -1)
                        ),
                        buttons.action(
                            "Vel+",
                            Message::AdjustMidiNoteVelocity(item_id, note.id(), 1)
                        ),
                        buttons.danger("Delete", Message::DeleteMidiNote(item_id, note.id())),
                    ]
                    .spacing(5),
                    touch_targets,
                )
            })
            .collect::<Vec<Element<'_, Message>>>();
        column![
            row![text("Name"), name_input].spacing(8),
            action_row(item_actions, touch_targets),
            column(notes).spacing(4)
        ]
        .spacing(6)
        .into()
    } else {
        column![text("Selected Item no longer exists.")].into()
    }
}

fn action_row<'a>(
    content: impl Into<Element<'a, Message>>,
    touch_targets: bool,
) -> Element<'a, Message> {
    let content = content.into();
    if touch_targets {
        scrollable(content)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default(),
            ))
            .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
            .into()
    } else {
        content
    }
}

#[derive(Clone, Copy)]
struct ItemInspectorButtons {
    touch_targets: bool,
}

impl ItemInspectorButtons {
    fn action<'a>(&self, label: &'a str, message: Message) -> Button<'a, Message> {
        action_button(label, message, self.touch_targets)
    }

    fn danger<'a>(&self, label: &'a str, message: Message) -> Button<'a, Message> {
        danger_button(label, message, self.touch_targets)
    }
}

fn action_button<'a>(label: &'a str, message: Message, touch_targets: bool) -> Button<'a, Message> {
    let button = button(label)
        .style(iced::widget::button::secondary)
        .on_press(message);
    if touch_targets {
        button
            .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
            .padding([tokens::SPACING_SM, tokens::SPACING_MD])
    } else {
        button.padding([2, 5])
    }
}

fn danger_button<'a>(label: &'a str, message: Message, touch_targets: bool) -> Button<'a, Message> {
    let button = button(label)
        .style(iced::widget::button::danger)
        .on_press(message);
    if touch_targets {
        button
            .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
            .padding([tokens::SPACING_SM, tokens::SPACING_MD])
    } else {
        button.padding([2, 5])
    }
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
