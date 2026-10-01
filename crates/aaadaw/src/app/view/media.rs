use super::super::{App, Message, PathPickerTarget};
use aaadaw_app::{AudioAssetManagementOperation, AudioAssetSourceStatus};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

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
