use super::super::{App, Message};
use super::tokens;
use aaadaw_app::{AudioAssetSourceStatus, AudioAssetSourceStatusEntry};
use aaadaw_core::{ItemId, MidiItemPosition};
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
        .spacing(if touch_targets {
            tokens::SPACING_SM
        } else {
            tokens::SPACING_COMPACT
        })
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
        .padding([tokens::PANEL_PADDING, tokens::PANEL_PADDING])
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
        let take_count = item.take_count();
        let take_name = if item.name().is_empty() {
            item.media_ref()
        } else {
            item.name()
        };
        let selected_audio_ids = super::super::commands::selected_audio_item_ids(app);
        let multi_take_selection = app.timeline.selected_items.len() > 1
            && selected_audio_ids.iter().any(|selected_id| {
                app.project
                    .audio_items()
                    .iter()
                    .find(|selected_item| selected_item.id() == *selected_id)
                    .is_some_and(|selected_item| selected_item.take_count() > 1)
            });
        let multi_audio_selection = selected_audio_ids.len() > 1;
        let mut take_controls = row![
            text(format!(
                "Take {}/{} · {}",
                item.active_take_index() + 1,
                take_count,
                take_name
            ))
            .width(Length::Fill),
        ]
        .spacing(if touch_targets {
            tokens::SPACING_SM
        } else {
            tokens::SPACING_COMPACT
        });
        if !multi_audio_selection
            && super::super::commands::is_enabled(
                app,
                super::super::commands::CommandId::DuplicateActiveTake,
            )
        {
            take_controls = take_controls
                .push(buttons.action("Duplicate take", Message::DuplicateAudioItemTake(item_id)));
        }
        if take_count > 1 || multi_take_selection {
            let (previous_label, previous_message, next_label, next_message) =
                if multi_take_selection {
                    (
                        "Previous takes",
                        Message::SwitchSelectedAudioItemTakes(false),
                        "Next takes",
                        Message::SwitchSelectedAudioItemTakes(true),
                    )
                } else {
                    (
                        "Previous take",
                        Message::SwitchAudioItemTake(item_id, false),
                        "Next take",
                        Message::SwitchAudioItemTake(item_id, true),
                    )
                };
            take_controls = take_controls
                .push(buttons.action(previous_label, previous_message))
                .push(buttons.action(next_label, next_message));
            if take_count > 1 && !multi_take_selection {
                take_controls = take_controls.push(buttons.action(
                    "Move active to top",
                    Message::MoveSelectedAudioItemActiveTakesToTop,
                ));
                take_controls = take_controls.push(buttons.danger(
                    "Crop to active take",
                    Message::CropSelectedAudioItemsToActiveTake,
                ));
            }
        }
        if multi_take_selection {
            take_controls = take_controls.push(buttons.action(
                "Move selected to top",
                Message::MoveSelectedAudioItemActiveTakesToTop,
            ));
            take_controls = take_controls.push(buttons.danger(
                "Crop selected to active",
                Message::CropSelectedAudioItemsToActiveTake,
            ));
        }
        let mut actions = row![
            text(format!("Audio · {}", item.media_ref())).width(Length::Fill),
            buttons.action("Duplicate", Message::DuplicateAudioItem(item_id)),
        ]
        .spacing(if touch_targets {
            tokens::SPACING_SM
        } else {
            tokens::SPACING_COMPACT
        });
        #[cfg(feature = "audio-device")]
        {
            actions =
                actions.push(buttons.action("Seek", Message::SeekToItem(item.start_sample())));
        }
        if status.is_some_and(can_relink_source) {
            actions = actions.push(buttons.action("Relink", Message::RelinkAudioItem(item_id)));
        }
        if multi_audio_selection
            && super::super::commands::is_enabled(
                app,
                super::super::commands::CommandId::DuplicateActiveTake,
            )
        {
            actions = actions.push(buttons.action(
                "Duplicate selected takes",
                Message::DuplicateSelectedItemActiveTakes,
            ));
        }
        if super::super::commands::is_enabled(
            app,
            super::super::commands::CommandId::PasteAudioItemAsTake,
        ) {
            actions = actions.push(buttons.action("Paste as takes", Message::PasteAudioItemAsTake));
        }
        if super::super::commands::is_enabled(
            app,
            super::super::commands::CommandId::DeleteActiveTake,
        ) {
            let label = if app.timeline.selected_items.len() > 1 {
                "Delete selected active takes"
            } else {
                "Delete active take"
            };
            actions = actions.push(buttons.danger(label, Message::DeleteSelectedItemActiveTakes));
        }
        actions = actions.push(buttons.danger("Delete", Message::DeleteAudioItem(item_id)));
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
            .spacing(if touch_targets {
                tokens::SPACING_SM
            } else {
                tokens::SPACING_COMPACT
            })
        } else {
            row![
                text(format!("Start sample: {}", item.start_sample())).width(Length::Fill),
                buttons.action("Edit", Message::BeginAudioItemStartSampleEdit(item_id)),
            ]
            .spacing(if touch_targets {
                tokens::SPACING_SM
            } else {
                tokens::SPACING_COMPACT
            })
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
            action_row(take_controls, touch_targets),
            row![text(format!(
                "Position {start_seconds:.3} s · Length {duration_seconds:.3} s"
            )),]
            .spacing(tokens::SECTION_GAP),
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
                .spacing(tokens::SPACING_XS),
                touch_targets
            ),
        ]
        .spacing(if touch_targets {
            tokens::SPACING_SM
        } else {
            tokens::SPACING_COMPACT
        })
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
        let mut take_controls = row![
            text(format!(
                "Take {}/{} · {}",
                item.active_take_index() + 1,
                item.take_count(),
                item.name()
            ))
            .width(Length::Fill),
        ]
        .spacing(if touch_targets {
            tokens::SPACING_SM
        } else {
            tokens::SPACING_COMPACT
        });
        if item.take_count() > 1 {
            take_controls = take_controls
                .push(buttons.action("Previous take", Message::SwitchMidiItemTake(item_id, false)))
                .push(buttons.action("Next take", Message::SwitchMidiItemTake(item_id, true)));
        }
        if super::super::commands::is_enabled(
            app,
            super::super::commands::CommandId::DeleteActiveTake,
        ) {
            take_controls = take_controls
                .push(buttons.danger("Delete active take", Message::DeleteSelectedItemActiveTakes));
        }
        let mut item_actions = row![
            text(format!(
                "Tick {} · length {}",
                format_midi_item_position(item.start_position()),
                item.length_ticks()
            ))
            .width(Length::Fill),
            buttons.action("Piano roll…", Message::OpenMidiEditor(item_id)),
            buttons.action("− beat", Message::NudgeMidiItem(item_id, -1)),
            buttons.action("+ beat", Message::NudgeMidiItem(item_id, 1)),
            buttons.action("Add C4", Message::AddMidiNote(item_id)),
        ]
        .spacing(if touch_targets {
            tokens::SPACING_SM
        } else {
            tokens::SPACING_COMPACT
        });
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
                    .spacing(if touch_targets {
                        tokens::SPACING_XS
                    } else {
                        tokens::SPACING_DENSE
                    }),
                    touch_targets,
                )
            })
            .collect::<Vec<Element<'_, Message>>>();
        column![
            take_controls,
            row![text("Name"), name_input].spacing(if touch_targets {
                tokens::SPACING_SM
            } else {
                tokens::SPACING_COMPACT
            }),
            action_row(item_actions, touch_targets),
            column(notes).spacing(tokens::SPACING_XS)
        ]
        .spacing(if touch_targets {
            tokens::SPACING_SM
        } else {
            tokens::SPACING_COMPACT
        })
        .into()
    } else {
        column![text("Selected Item no longer exists.")].into()
    }
}

fn format_midi_item_position(position: MidiItemPosition) -> String {
    if position.subtick() == 0 {
        return position.tick().to_string();
    }
    let thousandths = u16::from(position.subtick()) * 5;
    let mut formatted = format!("{}.{thousandths:03}", position.tick());
    while formatted.ends_with('0') {
        formatted.pop();
    }
    formatted
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

#[cfg(test)]
mod tests {
    use super::format_midi_item_position;
    use aaadaw_core::MidiItemPosition;

    #[test]
    fn midi_item_inspector_formats_fractional_ppq_without_rounding() {
        for (position, expected) in [
            (MidiItemPosition::new(0, 0).unwrap(), "0"),
            (MidiItemPosition::new(0, 1).unwrap(), "0.005"),
            (MidiItemPosition::new(964, 160).unwrap(), "964.8"),
            (MidiItemPosition::new(964, 199).unwrap(), "964.995"),
        ] {
            assert_eq!(format_midi_item_position(position), expected);
        }
    }
}
