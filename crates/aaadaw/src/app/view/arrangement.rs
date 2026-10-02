use super::super::commands::{self, CommandEntry, CommandId, TrackCommand};
use super::super::{App, Message};
use crate::timeline::{
    self, ArrangementPane, TCP_SCROLL_ID, TIMELINE_ROW_HEIGHT, TIMELINE_SCROLL_ID, TimelineEvent,
};
use aaadaw_core::Track;
use iced::widget::{
    button, column, container, float, mouse_area, pane_grid, row, scrollable, stack, text,
    text_input,
};
use iced::{Alignment, Element, Length, Theme};

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let toolbar = row![
        button("+ Track")
            .style(iced::widget::button::secondary)
            .on_press_maybe(
                commands::is_enabled(app, CommandId::AddTrack)
                    .then_some(Message::ExecuteCommand(CommandId::AddTrack)),
            ),
        button("+ MIDI item")
            .style(iced::widget::button::secondary)
            .on_press_maybe(
                commands::is_enabled(app, CommandId::AddMidiItem)
                    .then_some(Message::ExecuteCommand(CommandId::AddMidiItem)),
            ),
        button(if !app.timeline.has_sixteenth_grid() {
            "Snap unavailable"
        } else if app.timeline.snap_to_sixteenth {
            "Snap 1/16"
        } else {
            "Snap Off"
        })
        .style(if app.timeline.snap_to_sixteenth {
            iced::widget::button::warning
        } else {
            iced::widget::button::secondary
        })
        .on_press_maybe(
            app.timeline
                .has_sixteenth_grid()
                .then_some(Message::Timeline(TimelineEvent::ToggleSnapToSixteenth,))
        ),
        text("Zoom").size(12),
        button("−")
            .on_press(Message::Timeline(TimelineEvent::ZoomAt {
                factor: 0.8,
                anchor_x: 480.0,
            }))
            .style(iced::widget::button::secondary),
        button("+")
            .on_press(Message::Timeline(TimelineEvent::ZoomAt {
                factor: 1.25,
                anchor_x: 480.0,
            }))
            .style(iced::widget::button::secondary),
        text(format!("{} px / quarter", {
            let ppq = app.project.settings().ppq() as f32;
            app.timeline.pixels_per_tick * ppq
        }))
        .size(12),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let panes = pane_grid(&app.timeline.panes, |_pane, role, _is_maximized| {
        let content: Element<'_, Message> = match role {
            ArrangementPane::TrackControls => track_controls(app),
            ArrangementPane::Timeline => timeline_content(app),
        };
        pane_grid::Content::new(content)
    })
    .spacing(2)
    .on_resize(8, |event| {
        Message::Timeline(TimelineEvent::ResizeSplit {
            split: event.split,
            ratio: event.ratio,
        })
    })
    .width(Length::Fill)
    .height(Length::Fill);

    column![toolbar, panes, super::item_inspector::view(app)]
        .spacing(6)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn track_controls(app: &App) -> Element<'_, Message> {
    let rows = app
        .project
        .tracks()
        .iter()
        .map(|track| track_row(app, track))
        .collect::<Vec<_>>();
    let track_rows = if rows.is_empty() {
        column![container(text("No tracks").size(12)).height(TIMELINE_ROW_HEIGHT)]
    } else {
        column(rows).spacing(0)
    };
    let scroll = scrollable(track_rows)
        .id(iced::widget::Id::new(TCP_SCROLL_ID))
        .height(Length::Fill)
        .on_scroll(|viewport| Message::TcpScrolled {
            offset: viewport.absolute_offset().y,
            height: viewport.bounds().height,
        });

    let controls = column![
        container(text("Track controls").size(12))
            .width(Length::Fill)
            .height(32)
            .padding([8, 6]),
        scroll,
    ]
    .spacing(0)
    .width(Length::Fill)
    .height(Length::Fill);

    let Some(context_track_id) = app.timeline.context_track else {
        return controls.into();
    };
    let Some((track_index, track)) = app
        .project
        .tracks()
        .iter()
        .enumerate()
        .find(|(_, track)| track.id() == context_track_id)
    else {
        return controls.into();
    };
    let row_y = 32.0 + track_index as f32 * TIMELINE_ROW_HEIGHT - app.timeline.vertical_scroll;
    let popup = float(track_context_menu(app, track)).translate(move |bounds, viewport| {
        let max_y = (viewport.y + viewport.height - bounds.height).max(viewport.y);
        let target_y = (bounds.y + row_y).clamp(viewport.y, max_y);
        iced::Vector::new(8.0, target_y - bounds.y)
    });
    stack![controls, popup]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn track_context_menu<'a>(app: &'a App, track: &'a Track) -> Element<'a, Message> {
    let track_id = track.id();
    let heading = row![
        text(track.name()).size(12).width(Length::Fill),
        button("×")
            .style(iced::widget::button::text)
            .on_press(Message::Timeline(TimelineEvent::CloseTrackContextMenu))
            .padding([super::tokens::SPACING_XS, super::tokens::SPACING_SM]),
    ]
    .align_y(Alignment::Center);
    let mut actions = column![action_button(
        "Select track",
        Message::Timeline(TimelineEvent::SelectTrack(track_id)),
    ),]
    .spacing(super::tokens::ROW_GAP);
    for entry in commands::for_track_context(app, track_id) {
        if entry.separator_before {
            actions = actions.push(iced::widget::rule::horizontal(1));
        }
        actions = actions.push(context_menu_command(entry));
    }
    let contents = column![
        heading,
        scrollable(actions)
            .width(Length::Fill)
            .height(Length::Shrink),
    ]
    .spacing(super::tokens::ROW_GAP);
    container(contents)
        .width(220)
        .padding(super::tokens::PANEL_PADDING)
        .style(|_| container::Style {
            background: Some(iced::Color::from_rgb8(38, 43, 47).into()),
            border: iced::Border::default()
                .color(iced::Color::from_rgb8(91, 100, 106))
                .width(1.0)
                .rounded(2.0),
            ..container::Style::default()
        })
        .into()
}

fn timeline_content(app: &App) -> Element<'_, Message> {
    let playhead_sample = {
        #[cfg(feature = "jack-backend")]
        {
            app.playback.as_ref().map(|_| app.playhead_sample)
        }
        #[cfg(not(feature = "jack-backend"))]
        {
            None
        }
    };
    let timeline = stack![
        timeline::timeline_widget(&app.timeline, &app.project, playhead_sample),
        timeline::item_labels_widget(&app.timeline),
    ];
    let scroll = scrollable(timeline)
        .id(iced::widget::Id::new(TIMELINE_SCROLL_ID))
        .height(Length::Fill)
        .on_scroll(|viewport| Message::TimelineScrolled {
            offset: viewport.absolute_offset().y,
            height: viewport.bounds().height,
        });
    let contents = column![timeline::ruler_widget(&app.timeline, &app.project), scroll]
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fill);
    let Some(item_id) = app.timeline.context_item else {
        return contents.into();
    };
    let (x, y) = app.timeline.context_item_position.unwrap_or((0.0, 0.0));
    let popup = float(item_context_menu(app, item_id)).translate(move |bounds, viewport| {
        let max_x = (viewport.x + viewport.width - bounds.width).max(viewport.x);
        let max_y = (viewport.y + viewport.height - bounds.height).max(viewport.y);
        let target_x = (bounds.x + x).clamp(viewport.x, max_x);
        let target_y =
            (bounds.y + 32.0 + y - app.timeline.vertical_scroll).clamp(viewport.y, max_y);
        iced::Vector::new(target_x - bounds.x, target_y - bounds.y)
    });
    stack![contents, popup]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn item_context_menu<'a>(app: &'a App, item_id: aaadaw_core::ItemId) -> Element<'a, Message> {
    let is_audio = app
        .project
        .audio_items()
        .iter()
        .any(|item| item.id() == item_id);
    let title = if is_audio { "Audio item" } else { "MIDI item" };
    let heading = row![
        text(title).size(12).width(Length::Fill),
        button("×")
            .style(iced::widget::button::text)
            .on_press(Message::Timeline(TimelineEvent::CloseItemContextMenu))
            .padding([super::tokens::SPACING_XS, super::tokens::SPACING_SM]),
    ]
    .align_y(Alignment::Center);
    let mut actions = column![];
    for entry in commands::for_menu(app, super::super::MainMenu::Item) {
        if entry.id == CommandId::DuplicateSelectedAudioItem && !is_audio {
            continue;
        }
        if entry.separator_before {
            actions = actions.push(iced::widget::rule::horizontal(1));
        }
        actions = actions.push(context_item_command(entry));
    }
    let contents =
        column![heading, actions.spacing(super::tokens::ROW_GAP)].spacing(super::tokens::ROW_GAP);
    container(contents)
        .width(230)
        .padding(super::tokens::PANEL_PADDING)
        .style(|_| container::Style {
            background: Some(iced::Color::from_rgb8(38, 43, 47).into()),
            border: iced::Border::default()
                .color(iced::Color::from_rgb8(91, 100, 106))
                .width(1.0)
                .rounded(2.0),
            ..container::Style::default()
        })
        .into()
}

fn track_row<'a>(app: &'a App, track: &'a Track) -> Element<'a, Message> {
    let track_id = track.id();
    let edited_name = app
        .track_name_edits
        .get(&track_id)
        .map_or(track.name(), String::as_str);
    let has_edit = app.track_name_edits.contains_key(&track_id);
    let is_selected = app.timeline.selected_track == Some(track_id);
    let heading = row![
        button(if is_selected { "●" } else { "○" })
            .on_press(Message::Timeline(TimelineEvent::SelectTrack(track_id)))
            .style(if is_selected {
                iced::widget::button::warning
            } else {
                iced::widget::button::secondary
            })
            .padding([2, 4]),
        text_input("Track name", edited_name)
            .id(super::super::messages::track_name_input_id(track_id))
            .on_input(move |name| Message::TrackNameChanged(track_id, name))
            .on_submit(Message::CommitTrackName(track_id))
            .padding([2, 4])
            .width(Length::Fill),
        button(if has_edit { "✓" } else { "⋯" })
            .on_press(if has_edit {
                Message::CommitTrackName(track_id)
            } else {
                Message::Timeline(TimelineEvent::ToggleTrackContextMenu(track_id))
            })
            .style(iced::widget::button::secondary)
            .padding([3, 7]),
    ]
    .spacing(3)
    .align_y(Alignment::Center);
    let mute_command = CommandId::Track {
        track_id,
        command: TrackCommand::ToggleMute,
    };
    let mute = button("M")
        .on_press_maybe(
            commands::is_enabled(app, mute_command)
                .then_some(Message::ExecuteCommand(mute_command)),
        )
        .style(move |theme: &Theme, status| {
            if track.is_muted() {
                iced::widget::button::danger(theme, status)
            } else {
                iced::widget::button::secondary(theme, status)
            }
        })
        .padding([2, 8]);
    let solo_command = CommandId::Track {
        track_id,
        command: TrackCommand::ToggleSolo,
    };
    let solo = button("S")
        .on_press_maybe(
            commands::is_enabled(app, solo_command)
                .then_some(Message::ExecuteCommand(solo_command)),
        )
        .style(move |theme: &Theme, status| {
            if track.is_solo() {
                iced::widget::button::warning(theme, status)
            } else {
                iced::widget::button::secondary(theme, status)
            }
        })
        .padding([2, 8]);
    let controls = row![
        mute,
        solo,
        text(format!("{:.0} dB", track.volume_db())).size(11),
        small_control("−", Message::AdjustVolume(track_id, -1.0)),
        small_control("+", Message::AdjustVolume(track_id, 1.0)),
        text(pan_label(track.pan())).size(11),
        small_control("◀", Message::AdjustPan(track_id, -0.1)),
        small_control("▶", Message::AdjustPan(track_id, 0.1)),
    ]
    .spacing(3)
    .align_y(Alignment::Center);
    let selected = app.timeline.selected_track == Some(track_id);
    let row = container(column![heading, controls].spacing(4))
        .padding([5, 4])
        .height(TIMELINE_ROW_HEIGHT)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(
                if selected {
                    iced::Color::from_rgb8(48, 57, 62)
                } else {
                    iced::Color::from_rgb8(34, 39, 43)
                }
                .into(),
            ),
            ..container::Style::default()
        });
    mouse_area(row)
        .on_right_press(Message::Timeline(TimelineEvent::OpenTrackContextMenu(
            track_id,
        )))
        .into()
}

fn small_control<'a>(label: &'static str, message: Message) -> iced::widget::Button<'a, Message> {
    button(label)
        .style(iced::widget::button::secondary)
        .on_press(message)
        .padding([2, 5])
}

fn action_button<'a>(label: &'a str, message: Message) -> iced::widget::Button<'a, Message> {
    button(label)
        .style(iced::widget::button::secondary)
        .on_press(message)
        .width(Length::Fill)
        .padding([3, 7])
}

fn context_menu_command<'a>(entry: CommandEntry) -> iced::widget::Button<'a, Message> {
    let message = Message::ExecuteCommand(entry.id);
    let enabled = entry.enabled;
    let destructive = entry.destructive;
    let button = button(iced::widget::text(entry.label))
        .width(Length::Fill)
        .padding([super::tokens::SPACING_XS, super::tokens::SPACING_SM]);
    if destructive {
        button
            .style(iced::widget::button::danger)
            .on_press_maybe(enabled.then_some(message))
    } else {
        button
            .style(iced::widget::button::secondary)
            .on_press_maybe(enabled.then_some(message))
    }
}

fn context_item_command<'a>(mut entry: CommandEntry) -> iced::widget::Button<'a, Message> {
    entry.label = match entry.id {
        CommandId::DuplicateSelectedAudioItem => "Duplicate".to_owned(),
        CommandId::DeleteSelectedItems => "Delete selected items".to_owned(),
        CommandId::SplitSelectedItemsAtCursor => "Split at edit cursor".to_owned(),
        CommandId::SplitSelectedItemsAtTimeSelection => "Split at time selection".to_owned(),
        _ => entry.label,
    };
    context_menu_command(entry)
}

fn pan_label(pan: f32) -> String {
    if pan.abs() < 0.005 {
        "C".to_owned()
    } else if pan < 0.0 {
        format!("L {:.0}%", pan.abs() * 100.0)
    } else {
        format!("R {:.0}%", pan * 100.0)
    }
}
