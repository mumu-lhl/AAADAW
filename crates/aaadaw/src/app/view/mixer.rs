use super::super::commands::{self, CommandId, TrackCommand};
use super::super::fader;
use super::super::{StereoPeakHold, TrackDraftField};
use super::arrangement::{
    TrackMixLayout, slider_interaction, stereo_peak_meter, track_draft_input_style,
    track_fx_button, track_mix_controls, track_name_input, track_output_selector, track_peak_meter,
    track_routing_button, track_selection_background,
};
use super::tokens;
use super::{App, Message};
use aaadaw_core::Track;
use iced::widget::{
    button, column, container, mouse_area, progress_bar, row, rule, scrollable, slider, text,
    text_input, vertical_slider,
};
use iced::{Alignment, Element, Length};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    view_with_mix_layout(app, TrackMixLayout::Compact)
}

pub(super) fn mobile_view(app: &App) -> Element<'_, Message> {
    view_with_mix_layout(app, TrackMixLayout::TouchCompact)
}

fn view_with_mix_layout(app: &App, mix_layout: TrackMixLayout) -> Element<'_, Message> {
    let strips = app
        .project
        .tracks()
        .iter()
        .map(|track| track_strip(app, track, mix_layout));
    let touch_targets = matches!(mix_layout, TrackMixLayout::TouchCompact);
    let master = if touch_targets {
        master_strip(app)
    } else {
        desktop_master_strip(app)
    };
    let content = if touch_targets {
        row(strips).push(master)
    } else {
        row(std::iter::once(master).chain(strips))
    }
    .spacing(if touch_targets {
        tokens::SECTION_GAP
    } else {
        0.0
    });
    let scroll = scrollable(content)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default(),
        ))
        .height(Length::Fill);
    if !touch_targets {
        return column![
            scroll,
            button(text("× Mixer").size(10))
                .padding([1, 3])
                .style(button::text)
                .on_press(Message::ToggleMixerPanel)
        ]
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    }
    let toolbar = row![
        text("Mixer").size(14),
        button("+ Track")
            .height(if touch_targets {
                Length::Fixed(tokens::TOUCH_TARGET_MIN)
            } else {
                Length::Shrink
            })
            .padding(if touch_targets {
                [tokens::SPACING_LG as u16, tokens::SPACING_MD as u16]
            } else {
                [4, 8]
            })
            .on_press_maybe(
                commands::is_enabled(app, CommandId::AddTrack)
                    .then_some(Message::ExecuteCommand(CommandId::AddTrack)),
            )
            .style(button::secondary),
        button("+ Bus")
            .height(if touch_targets {
                Length::Fixed(tokens::TOUCH_TARGET_MIN)
            } else {
                Length::Shrink
            })
            .padding(if touch_targets {
                [tokens::SPACING_LG as u16, tokens::SPACING_MD as u16]
            } else {
                [4, 8]
            })
            .on_press(super::Message::AddBusTrack)
            .style(button::secondary),
    ]
    .spacing(tokens::SECTION_GAP)
    .align_y(Alignment::Center);
    let toolbar = if touch_targets {
        toolbar
    } else {
        toolbar
            .push(iced::widget::Space::new().width(Length::Fill))
            .push(
                button("×")
                    .on_press(Message::ToggleMixerPanel)
                    .padding([2, 6]),
            )
    };
    column![toolbar, scroll]
        .spacing(tokens::SECTION_GAP)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn track_strip<'a>(
    app: &'a App,
    track: &'a Track,
    mix_layout: TrackMixLayout,
) -> Element<'a, Message> {
    let touch = matches!(mix_layout, TrackMixLayout::TouchCompact);
    if !touch {
        return desktop_track_strip(app, track);
    }
    let touch_target = if touch {
        Length::Fixed(tokens::TOUCH_TARGET_MIN)
    } else {
        Length::Shrink
    };
    let track_id = track.id();
    let selected = app.timeline.is_track_selected(track_id);
    let primary = app.timeline.selected_track == Some(track_id);
    let selection = button(if primary {
        "●"
    } else if selected {
        "◉"
    } else {
        "○"
    })
    .on_press(Message::Timeline(
        crate::timeline::TimelineEvent::SelectTrackWithModifiers {
            track_id,
            modifiers: app.keyboard_modifiers,
        },
    ))
    .style(if primary {
        button::warning
    } else if selected {
        button::success
    } else {
        button::secondary
    })
    .padding(if touch {
        [tokens::SPACING_SM as u16; 2]
    } else {
        [2, 4]
    })
    .width(touch_target)
    .height(touch_target);
    let header = row![
        selection,
        track_name_input(app, track),
        track_fx_button(track),
        track_routing_button(track),
    ]
    .spacing(tokens::SPACING_XS)
    .align_y(Alignment::Center);
    let (volume, pan) = track_mix_controls(app, track, mix_layout);
    let content = column![
        header,
        rule::horizontal(1),
        text("Volume · dB").size(11),
        volume,
        track_peak_meter(app, track),
        text("Pan").size(11),
        pan,
        rule::horizontal(1),
        track_output_selector(app, track),
    ]
    .spacing(tokens::SPACING_SM)
    .width(Length::Fill)
    .height(Length::Fill);
    let background = track_selection_background(selected);
    container(content)
        .width(Length::Fixed(tokens::MIXER_STRIP_COMPACT))
        .height(Length::Fill)
        .padding(tokens::PANEL_PADDING)
        .style(move |_| container::Style {
            background: Some(background.into()),
            border: iced::Border::default()
                .color(iced::Color::from_rgb8(65, 73, 78))
                .width(1.0),
            ..container::Style::default()
        })
        .into()
}

fn master_strip(app: &App) -> Element<'_, Message> {
    let ceiling = app.audio_settings.master_output_ceiling;
    let guard_indicator = if app.master_guard_ticks_remaining > 0 {
        text("GUARD ACTIVE").style(|_| text::Style {
            color: Some(iced::Color::from_rgb8(237, 77, 68)),
        })
    } else {
        text("Guard idle").style(|_| text::Style {
            color: Some(iced::Color::from_rgb8(145, 158, 164)),
        })
    };
    container(
        column![
            text("MASTER").size(12),
            rule::horizontal(1),
            text("INPUT · capture/monitor").size(10),
            stereo_peak_meter(
                app.input_peak_level,
                app.input_peak_hold,
                Message::ClearInputMeter,
            ),
            text("Output · L/R").size(11),
            stereo_peak_meter(
                app.master_peak_level,
                app.master_peak_hold,
                Message::ClearMasterMeter,
            ),
            guard_indicator.size(10),
            text("Output ceiling").size(11),
            text(format!("{ceiling}")).size(14),
            text("Stereo output · sample peak guard").size(10),
        ]
        .spacing(tokens::SPACING_SM)
        .width(Length::Fill),
    )
    .width(Length::Fixed(tokens::MIXER_STRIP_COMPACT))
    .height(Length::Fill)
    .padding(tokens::PANEL_PADDING)
    .style(|_| container::Style {
        background: Some(iced::Color::from_rgb8(28, 33, 37).into()),
        border: iced::Border::default()
            .color(iced::Color::from_rgb8(91, 100, 106))
            .width(1.0),
        ..container::Style::default()
    })
    .into()
}

fn vertical_meter<'a>(
    peaks: [f32; 2],
    hold: StereoPeakHold,
    clear: Message,
    girth: f32,
) -> Element<'a, Message> {
    let bar = |channel: usize| {
        let peak = peaks[channel];
        let db = if peak > 0.0 {
            20.0 * peak.log10()
        } else {
            -60.0
        };
        let value = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
        let color = if peak >= 1.0 {
            iced::Color::from_rgb8(237, 77, 68)
        } else if peak >= 0.708 {
            iced::Color::from_rgb8(226, 177, 72)
        } else {
            iced::Color::from_rgb8(93, 190, 127)
        };
        progress_bar(0.0..=1.0, value)
            .vertical()
            .length(Length::Fill)
            .girth(girth)
            .style(move |_| iced::widget::progress_bar::Style {
                background: iced::Color::from_rgb8(23, 27, 29).into(),
                bar: color.into(),
                border: iced::Border::default(),
            })
    };
    let peak = hold.levels[0].max(hold.levels[1]);
    let label = if hold.clipped.iter().any(|clipped| *clipped) {
        "CLIP".into()
    } else if peak > 0.0 {
        format!("{:+.1}", 20.0 * peak.log10())
    } else {
        "−∞".into()
    };
    column![
        button(text(label).size(8))
            .style(if hold.clipped.iter().any(|clipped| *clipped) {
                button::danger
            } else {
                button::text
            })
            .padding(0)
            .on_press(clear),
        row![bar(0), bar(1)].spacing(2).height(Length::Fill),
    ]
    .spacing(2)
    .height(Length::Fill)
    .into()
}

fn desktop_track_strip<'a>(app: &'a App, track: &'a Track) -> Element<'a, Message> {
    let id = track.id();
    let selected = app.timeline.is_track_selected(id);
    let gesture = app
        .track_mix_gesture
        .filter(|gesture| gesture.track_id == id);
    let volume = gesture.map_or(track.volume_db(), |gesture| gesture.after_volume_db);
    let pan = gesture.map_or(track.pan(), |gesture| gesture.after_pan);
    let fader = vertical_slider(0.0..=1000.0, fader::to_position(volume), move |value| {
        Message::PreviewTrackVolume(id, fader::from_position(value))
    })
    .step(1.0_f32)
    .shift_step(0.1_f32)
    .width(16.0)
    .height(Length::Fill)
    .on_release(Message::CommitTrackVolume(id));
    let fader = slider_interaction(
        fader.into(),
        Some(Message::ResetTrackVolumeByDoubleClick(id)),
        Message::CancelTrackMixGesture,
    );
    let pan = slider(-1.0..=1.0, pan, move |value| {
        Message::PreviewTrackPan(id, value)
    })
    .step(0.01_f32)
    .shift_step(0.001_f32)
    .height(12.0)
    .on_release(Message::CommitTrackPan(id));
    let pan = slider_interaction(
        pan.into(),
        Some(Message::ResetTrackPanByDoubleClick(id)),
        Message::CancelTrackMixGesture,
    );
    let toggle = |label: &'static str, command: TrackCommand, on: bool| {
        let command = CommandId::Track {
            track_id: id,
            command,
        };
        button(text(label).size(10))
            .padding([1, 3])
            .style(if on {
                if matches!(
                    command,
                    CommandId::Track {
                        command: TrackCommand::ToggleRecordArm,
                        ..
                    }
                ) {
                    button::danger
                } else {
                    button::warning
                }
            } else {
                button::secondary
            })
            .on_press_maybe(
                commands::is_enabled(app, command).then_some(Message::ExecuteCommand(command)),
            )
    };
    let switches = column![
        toggle("M", TrackCommand::ToggleMute, track.is_muted()),
        toggle("S", TrackCommand::ToggleSolo, track.is_solo()),
        toggle("Ø", TrackCommand::TogglePhase, track.is_phase_inverted()),
        toggle("R", TrackCommand::ToggleRecordArm, track.is_record_armed()),
    ]
    .spacing(2);
    #[cfg(feature = "audio-device")]
    let switches = {
        let enabled = app
            .playback
            .as_ref()
            .is_some_and(|playback| playback.input_monitor_enabled(id));
        switches.push(
            button(text("I").size(10))
                .padding([1, 3])
                .style(if enabled {
                    button::success
                } else {
                    button::secondary
                })
                .on_press_maybe(
                    (track.is_record_armed() && !app.recording_starting && !app.recording_stopping)
                        .then_some(Message::ToggleInputMonitor(id)),
                ),
        )
    };
    let peaks = app.track_peak_levels.get(&id).copied().unwrap_or_default();
    let hold = app.track_peak_holds.get(&id).copied().unwrap_or_default();
    let value = app
        .track_volume_edits
        .get(&id)
        .cloned()
        .unwrap_or_else(|| fader::format_db(volume));
    let volume_value = text_input("dB", &value)
        .style(track_draft_input_style(
            app.track_draft_errors
                .contains_key(&(id, TrackDraftField::Volume)),
        ))
        .size(10)
        .padding([1, 3])
        .on_input(move |value| Message::TrackVolumeTextChanged(id, value))
        .on_submit(Message::CommitTrackVolumeText(id));
    let index = app
        .project
        .tracks()
        .iter()
        .position(|candidate| candidate.id() == id)
        .unwrap_or(0)
        + 1;
    let content = column![
        row![track_fx_button(track), track_routing_button(track)].spacing(2),
        pan,
        volume_value,
        row![
            vertical_meter(peaks, hold, Message::ClearTrackMeter(id), 8.0),
            fader,
            switches
        ]
        .spacing(4)
        .height(Length::Fill),
        track_name_input(app, track),
        button(text(index.to_string()).size(10))
            .width(Length::Fill)
            .padding(0)
            .style(if selected {
                button::primary
            } else {
                button::text
            })
            .on_press(Message::Timeline(
                crate::timeline::TimelineEvent::SelectTrackWithModifiers {
                    track_id: id,
                    modifiers: app.keyboard_modifiers
                }
            )),
    ]
    .spacing(3)
    .width(Length::Fill)
    .height(Length::Fill);
    mouse_area(
        container(content)
            .width(tokens::MCP_TRACK_WIDTH)
            .height(Length::Fill)
            .padding([3, 4])
            .style(move |_| container::Style {
                background: Some(track_selection_background(selected).into()),
                border: iced::Border::default()
                    .color(iced::Color::from_rgb8(65, 73, 78))
                    .width(1.0),
                ..Default::default()
            }),
    )
    .on_right_press(Message::Timeline(
        crate::timeline::TimelineEvent::OpenTrackContextMenu(id),
    ))
    .into()
}

fn desktop_master_strip(app: &App) -> Element<'_, Message> {
    let mix = app
        .master_mix_gesture
        .map_or(app.project.master_mix(), |(_, mix)| mix);
    let fader = vertical_slider(0.0..=1000.0, fader::to_position(mix.volume_db()), |value| {
        Message::PreviewMasterVolume(fader::from_position(value))
    })
    .step(1.0_f32)
    .shift_step(0.1_f32)
    .width(16.0)
    .height(Length::Fill)
    .on_release(Message::CommitMasterMix);
    let fader = slider_interaction(
        fader.into(),
        Some(Message::ResetMasterVolume),
        Message::CancelMasterMix,
    );
    let pan = slider(-1.0..=1.0, mix.pan(), Message::PreviewMasterPan)
        .step(0.01_f32)
        .shift_step(0.001_f32)
        .height(12.0)
        .on_release(Message::CommitMasterMix);
    let pan = slider_interaction(
        pan.into(),
        Some(Message::ResetMasterPan),
        Message::CancelMasterMix,
    );
    container(
        column![
            text("MASTER").size(11),
            pan,
            text(format!("{} dB", fader::format_db(mix.volume_db()))).size(10),
            row![
                fader,
                vertical_meter(
                    app.master_peak_level,
                    app.master_peak_hold,
                    Message::ClearMasterMeter,
                    32.0
                )
            ]
            .spacing(8)
            .height(Length::Fill),
            text(format!(
                "Limit {}",
                app.audio_settings.master_output_ceiling
            ))
            .size(10),
        ]
        .spacing(3)
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(tokens::MCP_MASTER_WIDTH)
    .height(Length::Fill)
    .padding([3, 4])
    .style(|_| container::Style {
        background: Some(iced::Color::from_rgb8(28, 33, 37).into()),
        border: iced::Border::default()
            .color(iced::Color::from_rgb8(91, 100, 106))
            .width(1.0),
        ..Default::default()
    })
    .into()
}
