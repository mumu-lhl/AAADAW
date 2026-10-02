use super::super::{App, Message, PathPickerTarget};
use aaadaw_app::{AudioAssetManagementOperation, AudioAssetSourceStatus};
use iced::widget::{button, column, container, row, rule, scrollable, text, text_input};
use iced::{Alignment, Border, Color, Element, Length};

pub(super) fn dock_view(app: &App) -> Element<'_, Message> {
    let import_status = if !app.import_busy {
        "Import audio into the current project".to_owned()
    } else if app.import_finalizing {
        "Placing timeline item…".to_owned()
    } else if let Some(total_bytes) = app.import_total_bytes {
        format!("Importing {} / {total_bytes} bytes", app.import_bytes)
    } else {
        "Starting import…".to_owned()
    };
    let import_buttons = if app.import_busy {
        row![button("Cancel").on_press(Message::CancelAudioImport)]
    } else {
        row![
            button("Choose…").on_press(Message::PickPath(PathPickerTarget::ImportAudio)),
            button("Import").on_press(Message::ImportAudio),
        ]
    }
    .spacing(6);

    let asset_buttons = if app.audio_asset_management_busy {
        row![button("Cancel").on_press(Message::CancelAudioAssetManagement)]
    } else {
        row![
            button("Scan").on_press(Message::RunAudioAssetManagement(
                AudioAssetManagementOperation::ScanSources,
            )),
            button("Pack external").on_press(Message::RunAudioAssetManagement(
                AudioAssetManagementOperation::PackExternalAssets,
            )),
        ]
    }
    .spacing(6);

    let mut source_rows = column![].spacing(4);
    let mut entries = app.audio_asset_source_statuses.values().collect::<Vec<_>>();
    entries.sort_unstable_by(|left, right| left.media_ref.cmp(&right.media_ref));
    if entries.is_empty() {
        source_rows = source_rows.push(text("No sources scanned").size(11));
    }
    for entry in entries {
        let status = format!("{} · {:?}", entry.media_ref, entry.status);
        let mut source_row = row![text(status).size(11).width(Length::Fill)];
        if entry.is_external_link && entry.status == AudioAssetSourceStatus::Missing {
            if let Some(item) = app
                .project
                .audio_items()
                .iter()
                .find(|item| item.media_ref() == entry.media_ref)
            {
                source_row =
                    source_row.push(button("Relink").on_press(Message::RelinkAudioItem(item.id())));
            }
        }
        source_rows = source_rows.push(source_row.spacing(4).align_y(Alignment::Center));
    }

    let content = column![
        row![
            text("Media Browser").size(13).width(Length::Fill),
            button("Hide").on_press(Message::ToggleMediaBrowserPanel),
        ]
        .align_y(Alignment::Center),
        rule::horizontal(1),
        text("Import audio").size(12),
        text_input("Audio file path", &app.audio_file_path_query)
            .on_input(Message::AudioFilePathChanged)
            .width(Length::Fill),
        import_buttons,
        text(import_status).size(11),
        rule::horizontal(1),
        text("Project sources").size(12),
        asset_buttons,
        text_input("Replacement source path", &app.relink_source_path_query)
            .on_input(Message::RelinkSourcePathChanged)
            .width(Length::Fill),
        button("Choose replacement…").on_press(Message::PickPath(PathPickerTarget::RelinkAudio)),
        scrollable(source_rows).height(Length::Fill),
    ]
    .spacing(6)
    .width(Length::Fill)
    .height(Length::Fill);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(8)
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

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let import_progress = if !app.import_busy {
        "The source is embedded in the project after import.".to_owned()
    } else if app.import_finalizing {
        "Audio embedded; placing the timeline item…".to_owned()
    } else if let Some(total_bytes) = app.import_total_bytes {
        format!("Importing: {} / {total_bytes} bytes", app.import_bytes)
    } else {
        "Starting audio import…".to_owned()
    };
    let mut import_row = row![
        text_input("Audio file path", &app.audio_file_path_query)
            .on_input(Message::AudioFilePathChanged)
            .width(Length::Fill),
        button("Choose…").on_press(Message::PickPath(PathPickerTarget::ImportAudio)),
        button(if app.import_busy {
            "Importing…"
        } else {
            "Import audio"
        })
        .on_press(Message::ImportAudio),
    ]
    .spacing(8);
    if app.import_busy {
        import_row = import_row.push(button("Cancel").on_press(Message::CancelAudioImport));
    }
    let import_controls = column![import_row, text(import_progress)].spacing(8);

    let mut asset_row = row![
        button(if app.audio_asset_management_busy {
            "Working…"
        } else {
            "Scan sources"
        })
        .on_press(Message::RunAudioAssetManagement(
            AudioAssetManagementOperation::ScanSources,
        )),
        button("Pack external audio").on_press(Message::RunAudioAssetManagement(
            AudioAssetManagementOperation::PackExternalAssets,
        )),
    ]
    .spacing(8);
    if app.audio_asset_management_busy {
        asset_row = asset_row.push(button("Cancel").on_press(Message::CancelAudioAssetManagement));
    }
    let asset_status = if app.audio_asset_management_status.is_empty() {
        "Scan source state or pack external links into the project".to_owned()
    } else {
        app.audio_asset_management_status.clone()
    };
    let asset_controls = column![asset_row, text(asset_status)].spacing(8);

    let relink_controls = row![
        text_input(
            "Replacement path for missing external audio",
            &app.relink_source_path_query
        )
        .on_input(Message::RelinkSourcePathChanged)
        .width(Length::Fill),
        button("Choose replacement…").on_press(Message::PickPath(PathPickerTarget::RelinkAudio)),
        text("Choose a file, then relink a missing item below"),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let mut source_status_list = column![text("Scanned sources").size(16)].spacing(6);
    if app.audio_asset_source_statuses.is_empty() {
        source_status_list = source_status_list.push(text("No scan results yet."));
    } else {
        let mut entries: Vec<_> = app.audio_asset_source_statuses.values().collect();
        entries.sort_by(|left, right| left.media_ref.cmp(&right.media_ref));
        for entry in entries {
            let mut status_row = row![
                text(format!("{} · {:?}", entry.media_ref, entry.status)).width(Length::Fill),
            ];
            if entry.is_external_link && entry.status == AudioAssetSourceStatus::Missing {
                if let Some(item) = app
                    .project
                    .audio_items()
                    .iter()
                    .find(|item| item.media_ref() == entry.media_ref)
                {
                    status_row = status_row
                        .push(button("Relink").on_press(Message::RelinkAudioItem(item.id())));
                }
            }
            source_status_list = source_status_list.push(status_row.spacing(8));
        }
    }

    let media_workspace = column![
        text("Media library").size(24),
        text("Import and repair audio sources, or package external files into this project."),
        text("Import audio").size(18),
        import_controls,
        text("Source management").size(18),
        asset_controls,
        text("Repair a missing external link").size(18),
        relink_controls,
        source_status_list,
    ]
    .spacing(14);

    container(scrollable(media_workspace))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(14)
        .into()
}
