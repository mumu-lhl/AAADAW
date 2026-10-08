use super::super::commands::{self, CommandId};
use super::arrangement::{
    TrackMixLayout, stereo_peak_meter, track_fx_button, track_mix_controls, track_name_input,
    track_output_selector, track_peak_meter, track_selection_background,
};
use super::tokens;
use super::{App, Message};
use aaadaw_core::Track;
use iced::widget::{button, column, container, row, rule, scrollable, text};
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
    let master = master_strip(app);
    let content = row(strips).push(master).spacing(tokens::SECTION_GAP);
    let scroll = scrollable(content)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default(),
        ))
        .height(Length::Fill);
    let toolbar = row![
        text("Mixer").size(14),
        button("+ Track")
            .padding(if matches!(mix_layout, TrackMixLayout::TouchCompact) {
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
            .padding(if matches!(mix_layout, TrackMixLayout::TouchCompact) {
                [tokens::SPACING_LG as u16, tokens::SPACING_MD as u16]
            } else {
                [4, 8]
            })
            .on_press(super::Message::AddBusTrack)
            .style(button::secondary),
    ]
    .spacing(tokens::SECTION_GAP)
    .align_y(Alignment::Center);
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
