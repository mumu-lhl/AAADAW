use super::super::audio_export::{
    RenderBounds, RenderSampleRate, RenderSource, parse_render_peak_target_centi_dbfs,
    parse_render_tail_milliseconds,
};
use super::super::{App, Message, PathPickerTarget};
use super::tokens;
use aaadaw_app::{WavChannelCount, WavSampleFormat};
use iced::widget::{
    button, checkbox, column, container, pick_list, row, rule, scrollable, text, text_input,
};
use iced::{Alignment, Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let options = app.wav_export_options;
    let bounds = app.render_dialog.bounds;
    let multiple_files = app.render_requires_multiple_files();
    let output_description = if multiple_files {
        "Choose the render range and audio format, then select a filename prefix for the output set."
    } else {
        "Choose the render range and audio format, then select an output filename."
    };
    let form_hint = if app.render_dialog.tail_enabled
        && parse_render_tail_milliseconds(&app.render_dialog.tail_duration_text).is_err()
    {
        Some("Tail must be a non-negative whole number of milliseconds.")
    } else if bounds == RenderBounds::CustomTimeRange
        && crate::project_time::parse_project_time(&app.render_dialog.custom_start_text).is_err()
    {
        Some("Enter a valid custom start time (for example, 0:00.000).")
    } else if bounds == RenderBounds::CustomTimeRange
        && crate::project_time::parse_project_time(&app.render_dialog.custom_end_text).is_err()
    {
        Some("Enter a valid custom end time (for example, 0:05.000).")
    } else if bounds == RenderBounds::TimeSelection && app.timeline.time_selection.is_none() {
        Some("Create a time selection in Arrange to render that range.")
    } else if bounds == RenderBounds::AllProjectRegions && app.project.regions().is_empty() {
        Some("Add at least one project region in Arrange to render all regions.")
    } else if bounds == RenderBounds::AllProjectMarkers && app.project.markers().is_empty() {
        Some("Add at least one project marker in Arrange to render all markers.")
    } else if !app.has_valid_render_source() {
        Some("Select one or more tracks in Arrange to render them via the master.")
    } else if cfg!(target_os = "android") && app.render_requires_multiple_files() {
        Some("The Android document picker supports one output file at a time.")
    } else if app.render_dialog.normalize_peak
        && parse_render_peak_target_centi_dbfs(&app.render_dialog.normalize_peak_target_text)
            .is_err()
    {
        Some("Peak target must be between -60 and 0 dBFS with up to 2 decimals.")
    } else if bounds == RenderBounds::CustomTimeRange && !app.can_submit_render() {
        Some("Custom range end must follow start and both times must map to valid project samples.")
    } else if bounds == RenderBounds::AllProjectMarkers && !app.can_submit_render() {
        Some(
            "Each project marker must have a non-empty range up to the next marker or project end.",
        )
    } else if !app.can_submit_render() {
        Some("Render range plus tail must contain at least one frame.")
    } else {
        None
    };
    let form_hint: Option<Element<'_, Message>> =
        form_hint.map(|message| text(message).size(11).into());
    let mut contents = column![
        text("Render project to WAV").size(20),
        text(output_description).size(12),
        rule::horizontal(1),
        row![
            text("Source").size(13),
            iced::widget::Space::new().width(Length::Fill),
            pick_list(
                &RenderSource::ALL[..],
                Some(app.render_dialog.source),
                Message::SetRenderSource,
            )
            .text_size(13)
            .padding([4, 8])
            .width(Length::Fixed(180.0)),
        ]
        .spacing(tokens::SPACING_SM)
        .align_y(Alignment::Center),
        text(app.render_source_description()).size(11),
        row![
            column![
                checkbox(app.render_dialog.stems_pre_fader)
                    .label("Render stems pre-fader")
                    .on_toggle_maybe(
                        app.can_render_stems_pre_fader()
                            .then_some(|enabled| { Message::SetRenderStemsPreFader(enabled) })
                    ),
                text("Bypasses selected-track volume fader and automation for stems.").size(11),
            ]
            .spacing(2),
        ]
        .spacing(tokens::SPACING_SM)
        .align_y(Alignment::Center),
        row![
            text("Bounds").size(13),
            iced::widget::Space::new().width(Length::Fill),
            pick_list(
                &RenderBounds::ALL[..],
                Some(bounds),
                Message::SetRenderBounds,
            )
            .text_size(13)
            .padding([4, 8])
            .width(Length::Fixed(180.0)),
        ]
        .spacing(tokens::SPACING_SM)
        .align_y(Alignment::Center),
        {
            let fields: Element<'_, Message> = if bounds == RenderBounds::CustomTimeRange {
                row![
                    text("Start:").size(13),
                    text_input("0:00.000", &app.render_dialog.custom_start_text)
                        .on_input(Message::SetRenderCustomStart)
                        .size(13)
                        .padding([4, 8])
                        .width(Length::FillPortion(1)),
                    text("End:").size(13),
                    text_input("0:05.000", &app.render_dialog.custom_end_text)
                        .on_input(Message::SetRenderCustomEnd)
                        .size(13)
                        .padding([4, 8])
                        .width(Length::FillPortion(1)),
                    text("Length:").size(13),
                    text(
                        app.custom_render_length_text()
                            .unwrap_or_else(|| "—".to_owned())
                    )
                    .size(13),
                ]
                .spacing(tokens::SPACING_SM)
                .align_y(Alignment::Center)
                .into()
            } else {
                iced::widget::Space::new().height(Length::Fixed(0.0)).into()
            };
            fields
        },
        row![
            checkbox(app.render_dialog.tail_enabled)
                .label("Tail")
                .on_toggle(Message::SetRenderTailEnabled),
            text_input("1000", &app.render_dialog.tail_duration_text)
                .on_input_maybe(
                    app.render_dialog
                        .tail_enabled
                        .then_some(Message::SetRenderTailDuration)
                )
                .size(13)
                .padding([4, 8])
                .width(Length::Fixed(88.0)),
            text("ms").size(13),
            iced::widget::Space::new().width(Length::Fill),
        ]
        .spacing(tokens::SPACING_SM)
        .align_y(Alignment::Center),
        row![
            text("Sample rate").size(13),
            iced::widget::Space::new().width(Length::Fill),
            pick_list(
                &RenderSampleRate::ALL[..],
                Some(app.render_dialog.sample_rate),
                Message::SetRenderSampleRate,
            )
            .text_size(13)
            .padding([4, 8])
            .width(Length::Fixed(180.0)),
        ]
        .spacing(tokens::SPACING_SM)
        .align_y(Alignment::Center),
        row![
            text("Channels").size(13),
            iced::widget::Space::new().width(Length::Fill),
            pick_list(
                &WavChannelCount::ALL[..],
                Some(app.render_dialog.channels),
                Message::SetRenderChannels,
            )
            .text_size(13)
            .padding([4, 8])
            .width(Length::Fixed(180.0)),
        ]
        .spacing(tokens::SPACING_SM)
        .align_y(Alignment::Center),
        row![
            column![
                text("Peak normalize").size(13),
                text("Scales the loudest output sample to the selected target.").size(11),
            ]
            .spacing(2),
            iced::widget::Space::new().width(Length::Fill),
            button(if app.render_dialog.normalize_peak {
                "On"
            } else {
                "Off"
            })
            .style(if app.render_dialog.normalize_peak {
                button::primary
            } else {
                button::secondary
            })
            .on_press(Message::SetRenderPeakNormalization(
                !app.render_dialog.normalize_peak,
            )),
        ]
        .spacing(tokens::SPACING_SM)
        .align_y(Alignment::Center),
        row![
            column![
                text("Peak target (dBFS)").size(13),
                text("Range: -60 to 0 dBFS.").size(11),
            ]
            .spacing(2),
            iced::widget::Space::new().width(Length::Fill),
            text_input("0", &app.render_dialog.normalize_peak_target_text)
                .on_input(Message::SetRenderPeakTarget)
                .size(13)
                .padding([4, 8])
                .width(Length::Fixed(88.0)),
        ]
        .spacing(tokens::SPACING_SM)
        .align_y(Alignment::Center),
        text(format!(
            "Output rate: {} Hz",
            app.render_output_sample_rate()
        ))
        .size(11),
        rule::horizontal(1),
        row![
            text("Format").size(13),
            iced::widget::Space::new().width(Length::Fill),
            pick_list(
                &WavSampleFormat::ALL[..],
                Some(options.sample_format),
                Message::SetWavSampleFormat,
            )
            .text_size(13)
            .padding([4, 8])
            .width(Length::Fixed(180.0)),
        ]
        .spacing(tokens::SPACING_SM)
        .align_y(Alignment::Center),
        row![
            column![
                text("TPDF dither").size(13),
                text("Applies to integer PCM output.").size(11),
            ]
            .spacing(2),
            iced::widget::Space::new().width(Length::Fill),
            button(if options.dither { "On" } else { "Off" })
                .style(if options.dither {
                    button::primary
                } else {
                    button::secondary
                })
                .on_press_maybe(
                    options
                        .sample_format
                        .is_integer()
                        .then_some(Message::SetWavDither(!options.dither)),
                ),
        ]
        .spacing(tokens::SPACING_SM)
        .align_y(Alignment::Center),
        rule::horizontal(1),
    ]
    .spacing(tokens::SPACING_MD)
    .padding(tokens::SPACING_LG)
    .width(Length::Fill);

    if let Some(form_hint) = form_hint {
        contents = contents.push(form_hint);
    }

    if let Some(active) = &app.active_offline_job {
        contents = contents.push(text(format!("Active job: {}", active.job.label())).size(12));
    } else {
        contents = contents.push(text("No active render job").size(12));
    }
    contents = contents.push(
        text(format!(
            "Waiting jobs: {} / {}",
            app.offline_job_queue.len(),
            super::super::offline_job_queue::MAX_PENDING_OFFLINE_JOBS,
        ))
        .size(12),
    );

    if app.offline_render_busy {
        contents =
            contents.push(button("Cancel active job").on_press(Message::CancelOfflineRender));
    }
    if let Some(progress) = app
        .offline_render_progress
        .as_ref()
        .and_then(|progress| progress.try_lock().ok().map(|value| *value))
        && progress.1 > 0
    {
        contents = contents.push(
            text(format!(
                "Progress: {:.0}%",
                (progress.0 as f64 / progress.1 as f64 * 100.0).clamp(0.0, 100.0),
            ))
            .size(12),
        );
    }

    contents = contents.push(
        button(if app.path_picker_busy {
            if multiple_files {
                "Choose output prefix…"
            } else {
                "Choose output file…"
            }
        } else if app.offline_render_busy
            && app.offline_job_queue.len()
                >= super::super::offline_job_queue::MAX_PENDING_OFFLINE_JOBS
        {
            "Render queue full"
        } else if app.offline_render_busy {
            "Queue another render…"
        } else {
            "Render…"
        })
        .style(button::primary)
        .on_press_maybe(
            (!app.path_picker_busy
                && app.can_submit_render()
                && (!app.offline_render_busy
                    || app.offline_job_queue.len()
                        < super::super::offline_job_queue::MAX_PENDING_OFFLINE_JOBS))
                .then_some(Message::PickPath(PathPickerTarget::ExportWav)),
        ),
    );

    container(scrollable(contents))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
