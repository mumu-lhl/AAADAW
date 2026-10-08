use super::super::{App, Message, PathPickerTarget};
use super::tokens;
use aaadaw_app::{AudioAssetManagementOperation, AudioAssetSourceStatus};
use iced::widget::{button, column, container, row, rule, scrollable, text, text_input};
use iced::{Alignment, Border, Color, Element, Length};

pub(super) fn dock_view(app: &App) -> Element<'_, Message> {
    panel_view(app, false)
}

pub(super) fn mobile_view(app: &App) -> Element<'_, Message> {
    panel_view(app, true)
}

fn panel_view(app: &App, touch_targets: bool) -> Element<'_, Message> {
    let import_status = if !app.import_busy {
        "Import at the edit cursor on the selected track (or first audio track)".to_owned()
    } else if app.import_finalizing {
        "Placing timeline item…".to_owned()
    } else if let Some(total_bytes) = app.import_total_bytes {
        format!("Importing {} / {total_bytes} bytes", app.import_bytes)
    } else {
        "Starting import…".to_owned()
    };
    let import_buttons = if app.import_busy {
        row![action_button(
            button("Cancel").on_press(Message::CancelAudioImport),
            touch_targets
        )]
    } else {
        row![
            action_button(
                button("Choose…").on_press(Message::PickPath(PathPickerTarget::ImportAudio)),
                touch_targets
            ),
            action_button(
                button("Import").on_press(Message::ImportAudio),
                touch_targets
            ),
        ]
    }
    .spacing(if touch_targets {
        tokens::SPACING_XS
    } else {
        6.0
    });

    let asset_buttons = if app.audio_asset_management_busy {
        row![action_button(
            button("Cancel").on_press(Message::CancelAudioAssetManagement),
            touch_targets
        )]
    } else {
        row![
            action_button(
                button("Scan").on_press(Message::RunAudioAssetManagement(
                    AudioAssetManagementOperation::ScanSources,
                )),
                touch_targets
            ),
            action_button(
                button("Pack external").on_press(Message::RunAudioAssetManagement(
                    AudioAssetManagementOperation::PackExternalAssets,
                )),
                touch_targets
            ),
        ]
    }
    .spacing(if touch_targets {
        tokens::SPACING_XS
    } else {
        6.0
    });

    let mut source_rows = column![].spacing(4);
    let mut entries = app.audio_asset_source_statuses.values().collect::<Vec<_>>();
    entries.sort_unstable_by(|left, right| left.media_ref.cmp(&right.media_ref));
    if entries.is_empty() {
        source_rows = source_rows.push(text("No sources scanned").size(11));
    }
    for entry in entries {
        let status = format!("{} · {:?}", entry.media_ref, entry.status);
        let mut source_row = row![text(status).size(11).width(Length::Fill)];
        if entry.is_external_link
            && entry.status == AudioAssetSourceStatus::Missing
            && let Some(item) = app
                .project
                .audio_items()
                .iter()
                .find(|item| item.media_ref() == entry.media_ref)
        {
            source_row = source_row.push(action_button(
                button("Relink").on_press(Message::RelinkAudioItem(item.id())),
                touch_targets,
            ));
        }
        source_rows = source_rows.push(source_row.spacing(4).align_y(Alignment::Center));
    }

    let panel_heading: Element<'_, Message> = if touch_targets {
        iced::widget::Space::new().height(0).into()
    } else {
        row![
            text("Media Browser").size(13).width(Length::Fill),
            button("Hide").on_press(Message::ToggleMediaBrowserPanel),
        ]
        .align_y(Alignment::Center)
        .into()
    };
    let panel_divider: Element<'_, Message> = if touch_targets {
        iced::widget::Space::new().height(0).into()
    } else {
        rule::horizontal(1).into()
    };
    let source_list: Element<'_, Message> = if touch_targets {
        source_rows.into()
    } else {
        scrollable(source_rows).height(Length::Fill).into()
    };
    let content = column![
        panel_heading,
        panel_divider,
        text("Import audio").size(12),
        text_input("Audio file path", &app.audio_file_path_query)
            .on_input(Message::AudioFilePathChanged)
            .padding(if touch_targets {
                [tokens::SPACING_LG, tokens::SPACING_SM]
            } else {
                [6, 8]
            })
            .width(Length::Fill),
        import_buttons,
        text(import_status).size(11),
        rule::horizontal(1),
        text("Project sources").size(12),
        asset_buttons,
        text_input("Replacement source path", &app.relink_source_path_query)
            .on_input(Message::RelinkSourcePathChanged)
            .padding(if touch_targets {
                [tokens::SPACING_LG, tokens::SPACING_SM]
            } else {
                [6, 8]
            })
            .width(Length::Fill),
        action_button(
            button("Choose replacement…")
                .on_press(Message::PickPath(PathPickerTarget::RelinkAudio)),
            touch_targets,
        ),
        source_list,
    ]
    .spacing(if touch_targets {
        tokens::SPACING_SM
    } else {
        6.0
    })
    .width(Length::Fill);
    let panel_content: Element<'_, Message> = if touch_targets {
        scrollable(content).height(Length::Fill).into()
    } else {
        content.height(Length::Fill).into()
    };

    container(panel_content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(if touch_targets {
            tokens::PANEL_PADDING
        } else {
            8.0
        })
        .style(|_| iced::widget::container::Style {
            background: Some(Color::from_rgb8(34, 37, 40).into()),
            border: Border::default()
                .color(Color::from_rgb8(67, 73, 77))
                .width(1.0)
                .rounded(1.0),
            ..iced::widget::container::Style::default()
        })
        .into()
}

fn action_button<'a>(
    button: iced::widget::Button<'a, Message>,
    touch_targets: bool,
) -> iced::widget::Button<'a, Message> {
    if touch_targets {
        button
            .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
            .padding([tokens::SPACING_SM, tokens::SPACING_MD])
    } else {
        button
    }
}
