use super::super::audio_export::{RenderBounds, RenderSampleRate};
use super::super::{App, Message, PathPickerTarget};
use super::tokens;
use aaadaw_app::WavSampleFormat;
use iced::widget::{button, column, container, pick_list, row, rule, text, text_input};
use iced::{Alignment, Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let options = app.wav_export_options;
    let bounds = app.render_dialog.bounds;
    let form_hint = if app
        .render_dialog
        .tail_seconds_text
        .trim()
        .parse::<u32>()
        .is_err()
    {
        Some("Tail must be a whole number of seconds.")
    } else if bounds == RenderBounds::TimeSelection && app.timeline.time_selection.is_none() {
        Some("Create a time selection in Arrange to render that range.")
    } else if !app.can_submit_render() {
        Some("Render range plus tail must contain at least one frame.")
    } else {
        None
    };
    let form_hint: Option<Element<'_, Message>> =
        form_hint.map(|message| text(message).size(11).into());
    let mut contents = column![
        text("Render project to WAV").size(20),
        text("Choose the render range and audio format, then select an output file.").size(12),
        rule::horizontal(1),
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
        row![
            text("Tail (seconds)").size(13),
            iced::widget::Space::new().width(Length::Fill),
            text_input("2", &app.render_dialog.tail_seconds_text)
                .on_input(Message::SetRenderTailSeconds)
                .size(13)
                .padding([4, 8])
                .width(Length::Fixed(88.0)),
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
            text("Stereo").size(12),
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
            "Choose output file…"
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

    container(contents)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
