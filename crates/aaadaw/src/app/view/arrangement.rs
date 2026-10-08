use super::super::commands::{self, CommandEntry, CommandId, TrackCommand};
use super::super::{App, Message, StereoPeakHold, TrackDraftField};
use super::tokens;
use crate::timeline::{
    self, ArrangementPane, TCP_SCROLL_ID, TIMELINE_ROW_HEIGHT, TIMELINE_SCROLL_ID, TimelineEvent,
};
use aaadaw_core::Track;
use iced::advanced::widget::Operation;
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, renderer};
use iced::widget::{
    button, column, container, float, mouse_area, pane_grid, pick_list, progress_bar, responsive,
    row, scrollable, slider, stack, text, text_input, tooltip,
};
use iced::{Alignment, Element, Length, Theme};
use std::fmt;

const MOBILE_TRACK_NAME_MAX_CHARS: usize = 9;
const MOBILE_TRACK_SELECTOR_BUTTON_WIDTH: f32 = 160.0;

fn truncate_track_name(name: &str, max_chars: usize) -> String {
    let mut chars = name.chars();
    let truncated = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let toolbar = row![
        button("+ Track")
            .style(iced::widget::button::secondary)
            .on_press_maybe(
                commands::is_enabled(app, CommandId::AddTrack)
                    .then_some(Message::ExecuteCommand(CommandId::AddTrack)),
            ),
        button("+ Bus")
            .style(iced::widget::button::secondary)
            .on_press(Message::AddBusTrack),
        button("+ MIDI item")
            .style(iced::widget::button::secondary)
            .on_press_maybe(
                commands::is_enabled(app, CommandId::AddMidiItem)
                    .then_some(Message::ExecuteCommand(CommandId::AddMidiItem)),
            ),
        button(if !app.timeline.has_snap_grid() {
            "Snap unavailable"
        } else if app.timeline.snap_enabled {
            "Snap On"
        } else {
            "Snap Off"
        })
        .style(if app.timeline.snap_enabled {
            iced::widget::button::warning
        } else {
            iced::widget::button::secondary
        })
        .on_press_maybe(
            app.timeline
                .has_snap_grid()
                .then_some(Message::Timeline(TimelineEvent::ToggleSnap))
        ),
        text("Grid").size(12),
        pick_list(
            &timeline::SnapGrid::ALL[..],
            Some(app.timeline.snap_grid),
            |grid| Message::Timeline(TimelineEvent::SetSnapGrid(grid)),
        )
        .width(Length::Fixed(120.0)),
        text("Zoom · MMB drag ↑↓").size(10),
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
    let gesture_hints = column![
        text("Items: click selects · Ctrl/Cmd-click adds/removes · Shift-click ranges · Ctrl/Cmd-drag copies · RMB-drag marquees")
            .size(10)
            .width(Length::Fill),
        text("Shift-drag bypasses Snap · Esc cancels · Tracks: click selects · Ctrl/Cmd-click toggles · Shift-click ranges")
            .size(10)
            .width(Length::Fill),
    ]
    .spacing(2);

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

    column![
        toolbar,
        gesture_hints,
        panes,
        super::item_inspector::view(app)
    ]
    .spacing(6)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

pub(super) fn mobile_view(app: &App) -> Element<'_, Message> {
    let track_choices = app
        .project
        .tracks()
        .iter()
        .map(|track| -> Element<'_, Message> {
            let selected = app.timeline.selected_track == Some(track.id());
            let label = truncate_track_name(track.name(), MOBILE_TRACK_NAME_MAX_CHARS);
            let track_button = button(
                row![
                    text(label).size(13).width(Length::Fill),
                    text(if selected { "✓" } else { "" }).size(13),
                ]
                .spacing(tokens::SPACING_XS)
                .align_y(Alignment::Center),
            )
            .width(Length::Fixed(MOBILE_TRACK_SELECTOR_BUTTON_WIDTH))
            .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
            .padding([tokens::SPACING_SM, tokens::SPACING_MD])
            .style(if selected {
                button::primary
            } else {
                button::secondary
            })
            .on_press(Message::Timeline(TimelineEvent::SelectTrack(track.id())));
            tooltip::Tooltip::new(
                track_button,
                text(track.name()).size(12),
                tooltip::Position::Bottom,
            )
            .into()
        });
    let track_selector = scrollable(row(track_choices).spacing(super::tokens::SPACING_XS))
        .direction(iced::widget::scrollable::Direction::Horizontal(
            iced::widget::scrollable::Scrollbar::default(),
        ))
        .height(Length::Shrink);

    let selected_track = app
        .timeline
        .selected_track
        .and_then(|id| app.project.tracks().iter().find(|track| track.id() == id));
    let selected_controls: Element<'_, Message> = if app.timeline.selected_item.is_some() {
        super::item_inspector::touch_view(app)
    } else if let Some(track) = selected_track {
        let (volume, pan) = track_mix_controls(app, track, TrackMixLayout::TouchCompact);
        container(
            column![
                row![
                    text(track.name()).size(15).width(Length::Fill),
                    track_peak_meter(app, track),
                ]
                .align_y(Alignment::Center),
                volume,
                pan,
            ]
            .spacing(super::tokens::SPACING_XS),
        )
        .width(Length::Fill)
        .padding(super::tokens::PANEL_PADDING)
        .into()
    } else {
        container(text("Select a track to show its controls").size(13))
            .width(Length::Fill)
            .padding(super::tokens::PANEL_PADDING)
            .into()
    };

    let toolbar = scrollable(
        row![
            button("+ Track")
                .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
                .padding([tokens::SPACING_LG, tokens::SPACING_MD])
                .on_press_maybe(
                    commands::is_enabled(app, CommandId::AddTrack)
                        .then_some(Message::ExecuteCommand(CommandId::AddTrack)),
                ),
            button("+ MIDI")
                .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
                .padding([tokens::SPACING_LG, tokens::SPACING_MD])
                .on_press_maybe(
                    commands::is_enabled(app, CommandId::AddMidiItem)
                        .then_some(Message::ExecuteCommand(CommandId::AddMidiItem)),
                ),
            button(if app.timeline.snap_enabled {
                "Snap on"
            } else {
                "Snap off"
            })
            .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
            .padding([tokens::SPACING_LG, tokens::SPACING_MD])
            .style(if app.timeline.snap_enabled {
                button::warning
            } else {
                button::secondary
            })
            .on_press_maybe(
                app.timeline
                    .has_snap_grid()
                    .then_some(Message::Timeline(TimelineEvent::ToggleSnap)),
            ),
            button("−")
                .width(Length::Fixed(tokens::TOUCH_TARGET_MIN))
                .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
                .padding([tokens::SPACING_LG; 2])
                .on_press(Message::Timeline(TimelineEvent::ZoomAt {
                    factor: 0.8,
                    anchor_x: 180.0,
                })),
            button("+")
                .width(Length::Fixed(tokens::TOUCH_TARGET_MIN))
                .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
                .padding([tokens::SPACING_LG; 2])
                .on_press(Message::Timeline(TimelineEvent::ZoomAt {
                    factor: 1.25,
                    anchor_x: 180.0,
                })),
        ]
        .spacing(super::tokens::SPACING_XS),
    )
    .direction(iced::widget::scrollable::Direction::Horizontal(
        iced::widget::scrollable::Scrollbar::default(),
    ))
    .height(Length::Shrink);

    column![
        toolbar,
        track_selector,
        selected_controls,
        timeline_content(app)
    ]
    .spacing(super::tokens::SPACING_SM)
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
    let row_y = 32.0
        + app
            .timeline
            .row_layout(track_index)
            .map_or(track_index as f32 * TIMELINE_ROW_HEIGHT, |row| row.top)
        - app.timeline.vertical_scroll;
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

fn is_volume_automation_visible(app: &App, track: &Track) -> bool {
    let track_id = track.id();
    app.timeline.volume_automation_tracks.contains(&track_id)
        || (!app
            .timeline
            .hidden_volume_automation_tracks
            .contains(&track_id)
            && !track.volume_automation().is_empty())
}

pub(super) fn track_output_selector<'a>(app: &'a App, track: &'a Track) -> Element<'a, Message> {
    let track_id = track.id();
    let output_choices: Vec<_> = std::iter::once(TrackOutputChoice {
        track_id: None,
        label: "Master".to_owned(),
    })
    .chain(app.project.tracks().iter().filter_map(|candidate| {
        if !candidate.is_bus() || candidate.id() == track_id {
            return None;
        }
        let mut ancestor = candidate.output_track();
        while let Some(ancestor_id) = ancestor {
            if ancestor_id == track_id {
                return None;
            }
            ancestor = app
                .project
                .tracks()
                .iter()
                .find(|track| track.id() == ancestor_id)
                .and_then(Track::output_track);
        }
        Some(TrackOutputChoice {
            track_id: Some(candidate.id()),
            label: format!("{} (Bus)", candidate.name()),
        })
    }))
    .collect();
    let selected_output = output_choices
        .iter()
        .find(|choice| choice.track_id == track.output_track())
        .cloned();
    column![
        text("Output").size(11),
        pick_list(output_choices, selected_output, move |choice| {
            Message::SetTrackOutput(track_id, choice.track_id)
        })
        .width(Length::Fill),
    ]
    .spacing(super::tokens::ROW_GAP)
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
    let instrument_label = track.instrument().map_or("None", |instrument| {
        app.clap_plugin_scan
            .plugins
            .iter()
            .find(|plugin| plugin.plugin_id == instrument.plugin_id())
            .map_or(instrument.plugin_id(), |plugin| plugin.name.as_str())
    });
    let has_midi_notes = app
        .project
        .midi_items()
        .iter()
        .any(|item| item.track_id() == track_id && !item.notes().is_empty());
    let has_audio_items = app
        .project
        .audio_items()
        .iter()
        .any(|item| item.track_id() == track_id);
    if track.is_frozen() {
        actions = actions
            .push(text("Frozen audio is playing; source edits require unfreeze.").size(10))
            .push(action_button(
                "Unfreeze track",
                Message::UnfreezeTrack(track_id),
            ));
    } else {
        actions = actions
            .push(text(format!("Instrument: {instrument_label}")).size(11))
            .push(action_button(
                "Set instrument…",
                Message::OpenTrackInstrumentPicker(track_id),
            ));
        if track.instrument().is_some() {
            actions = actions
                .push(text(super::CLAP_PLUGIN_RISK).size(10))
                .push(action_button(
                    if app.track_instrument_gui_open(track_id) {
                        "Close instrument editor"
                    } else {
                        "Open instrument editor"
                    },
                    Message::SetTrackInstrumentGui(
                        track_id,
                        !app.track_instrument_gui_open(track_id),
                    ),
                ))
                .push(action_button(
                    "Clear instrument",
                    Message::ClearTrackInstrument(track_id),
                ));
            if !track.is_bus() && has_midi_notes && !has_audio_items {
                actions = actions.push(action_button(
                    "Freeze track",
                    Message::FreezeTrack(track_id),
                ));
            } else {
                actions = actions.push(
                    text("Freeze requires MIDI notes and no audio items on this track.").size(10),
                );
            }
        }
        actions = actions.push(action_button(
            "Open FX chain…",
            Message::OpenTrackFxChain(track_id),
        ));
    }
    actions = actions.push(iced::widget::rule::horizontal(1));
    actions = actions.push(track_output_selector(app, track));
    let automation_visible = is_volume_automation_visible(app, track);
    actions = actions.push(action_button(
        if automation_visible {
            "Hide volume automation"
        } else {
            "Show volume automation"
        },
        Message::Timeline(TimelineEvent::ToggleVolumeAutomation(track_id)),
    ));
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct TrackOutputChoice {
    track_id: Option<aaadaw_core::TrackId>,
    label: String,
}

impl fmt::Display for TrackOutputChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}

fn timeline_content(app: &App) -> Element<'_, Message> {
    let playhead_sample = {
        #[cfg(feature = "audio-device")]
        {
            app.playback.as_ref().map(|_| app.playhead_sample)
        }
        #[cfg(not(feature = "audio-device"))]
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
    if let Some(context) = app.timeline.context_automation_point {
        let (x, y) = app
            .timeline
            .context_automation_position
            .unwrap_or((0.0, 0.0));
        let popup =
            float(automation_point_context_menu(context)).translate(move |bounds, viewport| {
                let max_x = (viewport.x + viewport.width - bounds.width).max(viewport.x);
                let max_y = (viewport.y + viewport.height - bounds.height).max(viewport.y);
                let target_x = (bounds.x + x).clamp(viewport.x, max_x);
                let target_y =
                    (bounds.y + 32.0 + y - app.timeline.vertical_scroll).clamp(viewport.y, max_y);
                iced::Vector::new(target_x - bounds.x, target_y - bounds.y)
            });
        return stack![contents, popup]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }
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

fn automation_point_context_menu(
    context: timeline::AutomationPointContext,
) -> Element<'static, Message> {
    let delete_message = match context {
        timeline::AutomationPointContext::Volume { track_id, index } => {
            Message::Timeline(TimelineEvent::DeleteVolumeAutomationPoint { track_id, index })
        }
        timeline::AutomationPointContext::Fx {
            track_id,
            chain_index,
            parameter_id,
            index,
        } => Message::Timeline(TimelineEvent::DeleteFxAutomationPoint {
            track_id,
            chain_index,
            parameter_id,
            index,
        }),
    };
    container(
        column![
            row![
                text("Automation point").size(12).width(Length::Fill),
                button("×")
                    .style(iced::widget::button::text)
                    .on_press(Message::Timeline(TimelineEvent::CloseAutomationPointMenu))
                    .padding([super::tokens::SPACING_XS, super::tokens::SPACING_SM]),
            ]
            .align_y(Alignment::Center),
            action_button("Delete point", delete_message),
        ]
        .spacing(super::tokens::ROW_GAP),
    )
    .width(190)
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

fn item_context_menu<'a>(app: &'a App, item_id: aaadaw_core::ItemId) -> Element<'a, Message> {
    let is_audio = app
        .project
        .audio_items()
        .iter()
        .any(|item| item.id() == item_id);
    let is_midi = app
        .project
        .midi_items()
        .iter()
        .any(|item| item.id() == item_id);
    let frozen_track_id = app
        .project
        .tracks()
        .iter()
        .find(|track| track.frozen_audio_item_id() == Some(item_id))
        .map(|track| track.id());
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
    if let Some(track_id) = frozen_track_id {
        actions = actions
            .push(text("Frozen render; unfreeze before editing this item.").size(10))
            .push(action_button(
                "Unfreeze track",
                Message::UnfreezeTrack(track_id),
            ));
    } else {
        for entry in commands::for_menu(app, super::super::MainMenu::Item) {
            if entry.id == CommandId::DuplicateSelectedAudioItem && !is_audio {
                continue;
            }
            if entry.id == CommandId::DuplicateSelectedMidiItem && !is_midi {
                continue;
            }
            if entry.separator_before {
                actions = actions.push(iced::widget::rule::horizontal(1));
            }
            actions = actions.push(context_item_command(entry));
        }
    }
    if is_midi {
        actions = actions.push(action_button(
            "Open piano roll",
            Message::OpenMidiEditor(item_id),
        ));
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

const COMPACT_TCP_WIDTH: f32 = 340.0;

fn track_row<'a>(app: &'a App, track: &'a Track) -> Element<'a, Message> {
    let height = app
        .project
        .tracks()
        .iter()
        .position(|candidate| candidate.id() == track.id())
        .and_then(|index| app.timeline.row_layout(index))
        .map_or(TIMELINE_ROW_HEIGHT, |row| row.height);
    responsive(move |size| track_row_layout(app, track, size.width < COMPACT_TCP_WIDTH, height))
        .into()
}

fn track_row_layout<'a>(
    app: &'a App,
    track: &'a Track,
    compact: bool,
    height: f32,
) -> Element<'a, Message> {
    let track_id = track.id();
    let has_edit = app.track_name_edits.contains_key(&track_id);
    let output_label = track
        .output_track()
        .and_then(|output_id| {
            app.project
                .tracks()
                .iter()
                .find(|candidate| candidate.id() == output_id)
        })
        .map_or_else(|| "Master".to_owned(), |output| output.name().to_owned());
    let is_selected = app.timeline.is_track_selected(track_id);
    let is_primary = app.timeline.selected_track == Some(track_id);
    let automation_visible = is_volume_automation_visible(app, track);
    let fx_button = track_fx_button(track);
    let selection_button = text(if is_primary {
        "●"
    } else if is_selected {
        "◉"
    } else {
        "○"
    })
    .size(13)
    .color(if is_primary {
        iced::Color::from_rgb8(240, 190, 105)
    } else if is_selected {
        iced::Color::from_rgb8(112, 190, 150)
    } else {
        iced::Color::from_rgb8(142, 153, 159)
    });
    let name_input = track_name_input(app, track);
    let context_button = button(if has_edit { "✓" } else { "⋯" })
        .on_press(if has_edit {
            Message::CommitTrackName(track_id)
        } else {
            Message::Timeline(TimelineEvent::ToggleTrackContextMenu(track_id))
        })
        .style(iced::widget::button::secondary)
        .padding([3, 7]);
    let heading = if compact {
        row![selection_button, name_input, context_button]
            .spacing(3)
            .align_y(Alignment::Center)
    } else {
        row![
            selection_button,
            if track.is_bus() {
                text("BUS")
                    .size(10)
                    .color(iced::Color::from_rgb8(139, 196, 210))
            } else {
                text("").size(10)
            },
            name_input,
            fx_button,
            text(format!("→ {output_label}")).size(10),
            button(if automation_visible { "AUTO" } else { "auto" })
                .style(if automation_visible {
                    iced::widget::button::success
                } else {
                    iced::widget::button::secondary
                })
                .on_press(Message::Timeline(TimelineEvent::ToggleVolumeAutomation(
                    track_id
                )))
                .padding([2, 4]),
            context_button,
        ]
        .spacing(3)
        .align_y(Alignment::Center)
    };
    let mix_layout = if compact {
        TrackMixLayout::Compact
    } else {
        TrackMixLayout::Normal
    };
    let (volume_controls, pan_controls) = track_mix_controls(app, track, mix_layout);
    let selected = app.timeline.is_track_selected(track_id);
    let meter = track_peak_meter(app, track);
    let row = container(column![heading, volume_controls, meter, pan_controls].spacing(2))
        .padding([5, 4])
        .height(height)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(track_selection_background(selected).into()),
            ..container::Style::default()
        });
    mouse_area(row)
        .on_press(Message::Timeline(TimelineEvent::SelectTrackWithModifiers {
            track_id,
            modifiers: app.keyboard_modifiers,
        }))
        .on_right_press(Message::Timeline(TimelineEvent::OpenTrackContextMenu(
            track_id,
        )))
        .into()
}

pub(super) fn track_peak_meter<'a>(app: &'a App, track: &Track) -> Element<'a, Message> {
    let peaks = app
        .track_peak_levels
        .get(&track.id())
        .copied()
        .unwrap_or([0.0; 2]);
    let hold = app
        .track_peak_holds
        .get(&track.id())
        .copied()
        .unwrap_or_default();
    stereo_peak_meter(peaks, hold, Message::ClearTrackMeter(track.id()))
}

pub(super) fn stereo_peak_meter<'a>(
    peaks: [f32; 2],
    hold: StereoPeakHold,
    clear_message: Message,
) -> Element<'a, Message> {
    let channel_meter = |channel: usize| {
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
        progress_bar(0.0..=1.0, value).girth(8.0).style(move |_| {
            iced::widget::progress_bar::Style {
                background: iced::Background::Color(iced::Color::from_rgb8(23, 27, 29)),
                bar: iced::Background::Color(color),
                border: iced::Border::default(),
            }
        })
    };
    let channel_row = |channel: usize, label: &'static str| {
        let peak_db = if hold.levels[channel] > 0.0 {
            format!("{:+.1}", 20.0 * hold.levels[channel].log10())
        } else {
            "−∞".to_owned()
        };
        let clip = if hold.clipped[channel] {
            text("CLIP")
                .size(10)
                .color(iced::Color::from_rgb8(237, 77, 68))
        } else {
            text("").size(10)
        };
        row![
            text(label).size(10).width(Length::Fixed(12.0)),
            channel_meter(channel),
            text(peak_db).size(10).width(Length::Fixed(36.0)),
            container(clip).width(Length::Fixed(26.0)),
        ]
        .spacing(3)
        .align_y(Alignment::Center)
    };
    column![
        channel_row(0, "L"),
        channel_row(1, "R"),
        row![
            text("−60   −30   −12   0 dBFS").size(10),
            button(text("Clear").size(10))
                .on_press(clear_message)
                .style(button::secondary)
                .padding([0, 3]),
        ]
        .spacing(3)
        .align_y(Alignment::Center),
    ]
    .spacing(1)
    .width(Length::Fill)
    .into()
}

pub(super) fn track_fx_button(track: &Track) -> Element<'static, Message> {
    let fx_chain = track.fx_chain();
    let has_bypassed_fx = fx_chain.iter().any(|plugin| !plugin.is_enabled());
    button(text(if fx_chain.is_empty() {
        "FX".to_owned()
    } else if has_bypassed_fx {
        format!("FX {} B", fx_chain.len())
    } else {
        format!("FX {}", fx_chain.len())
    }))
    .style(if has_bypassed_fx {
        iced::widget::button::warning
    } else if fx_chain.is_empty() {
        iced::widget::button::secondary
    } else {
        iced::widget::button::primary
    })
    .on_press(Message::OpenTrackFxChain(track.id()))
    .padding([2, 5])
    .into()
}

pub(super) fn track_name_input<'a>(app: &'a App, track: &'a Track) -> Element<'a, Message> {
    let track_id = track.id();
    let has_error = app
        .track_draft_errors
        .contains_key(&(track_id, TrackDraftField::Name));
    let edited_name = app
        .track_name_edits
        .get(&track_id)
        .map_or(track.name(), String::as_str);
    text_input("Track name", edited_name)
        .id(super::super::messages::track_name_input_id(track_id))
        .on_input(move |name| Message::TrackNameChanged(track_id, name))
        .on_submit(Message::CommitTrackName(track_id))
        .style(track_draft_input_style(has_error))
        .padding([2, 4])
        .width(Length::Fill)
        .into()
}

fn track_draft_input_style(
    has_error: bool,
) -> impl Fn(&Theme, text_input::Status) -> text_input::Style {
    move |theme, status| {
        let mut style = text_input::default(theme, status);
        if has_error {
            style.border.color = iced::Color::from_rgb8(237, 77, 68);
            style.border.width = 1.5;
        }
        style
    }
}

pub(super) fn track_selection_background(selected: bool) -> iced::Color {
    if selected {
        iced::Color::from_rgb8(48, 57, 62)
    } else {
        iced::Color::from_rgb8(34, 39, 43)
    }
}

#[derive(Clone, Copy)]
pub(super) enum TrackMixLayout {
    Compact,
    Normal,
    TouchCompact,
}

pub(super) fn track_mix_controls<'a>(
    app: &'a App,
    track: &'a Track,
    layout: TrackMixLayout,
) -> (Element<'a, Message>, Element<'a, Message>) {
    let compact = !matches!(layout, TrackMixLayout::Normal);
    let touch = matches!(layout, TrackMixLayout::TouchCompact);
    let touch_target_width = if touch {
        Length::Fixed(tokens::TOUCH_TARGET_MIN)
    } else {
        Length::Shrink
    };
    let touch_target_height = touch_target_width;
    let track_id = track.id();
    let toggle_padding: [u16; 2] = if touch {
        [tokens::SPACING_MD as u16, tokens::SPACING_SM as u16]
    } else {
        [2, 8]
    };
    let mute_command = CommandId::Track {
        track_id,
        command: TrackCommand::ToggleMute,
    };
    let mute = button(if touch && track.is_muted() {
        "M ✓"
    } else {
        "M"
    })
    .on_press_maybe(
        commands::is_enabled(app, mute_command).then_some(Message::ExecuteCommand(mute_command)),
    )
    .style(move |theme: &Theme, status| {
        if track.is_muted() {
            iced::widget::button::danger(theme, status)
        } else {
            iced::widget::button::secondary(theme, status)
        }
    })
    .padding(toggle_padding)
    .width(touch_target_width)
    .height(touch_target_height);
    let solo_command = CommandId::Track {
        track_id,
        command: TrackCommand::ToggleSolo,
    };
    let solo = button(if touch && track.is_solo() {
        "S ✓"
    } else {
        "S"
    })
    .on_press_maybe(
        commands::is_enabled(app, solo_command).then_some(Message::ExecuteCommand(solo_command)),
    )
    .style(move |theme: &Theme, status| {
        if track.is_solo() {
            iced::widget::button::warning(theme, status)
        } else {
            iced::widget::button::secondary(theme, status)
        }
    })
    .padding(toggle_padding)
    .width(touch_target_width)
    .height(touch_target_height);
    let record_arm_command = CommandId::Track {
        track_id,
        command: TrackCommand::ToggleRecordArm,
    };
    #[cfg(feature = "audio-device")]
    let is_recording = app.recording.is_some() && app.recording_tracks.contains(&track_id);
    #[cfg(not(feature = "audio-device"))]
    let is_recording = false;
    let record_arm = button(if is_recording {
        "REC"
    } else if touch && track.is_record_armed() {
        "R ✓"
    } else {
        "R"
    })
    .on_press_maybe(
        commands::is_enabled(app, record_arm_command)
            .then_some(Message::ExecuteCommand(record_arm_command)),
    )
    .style(move |theme: &Theme, status| {
        if is_recording {
            iced::widget::button::danger(theme, status)
        } else if track.is_record_armed() {
            iced::widget::button::warning(theme, status)
        } else {
            iced::widget::button::secondary(theme, status)
        }
    })
    .padding(toggle_padding)
    .width(touch_target_width)
    .height(touch_target_height);
    #[cfg(feature = "audio-device")]
    let monitor_enabled = app
        .playback
        .as_ref()
        .is_some_and(|playback| playback.input_monitor_enabled(track_id));
    #[cfg(feature = "audio-device")]
    let monitor_pending = app.standby_monitor_track == Some(track_id);
    #[cfg(feature = "audio-device")]
    let show_input_monitor = monitor_enabled || monitor_pending;
    #[cfg(not(feature = "audio-device"))]
    let show_input_monitor = false;
    #[cfg(feature = "audio-device")]
    let input_monitor = button(if monitor_enabled {
        "MON"
    } else if monitor_pending {
        "MON…"
    } else {
        "mon"
    })
    .on_press_maybe(
        (track.is_record_armed() && !app.recording_starting && !app.recording_stopping)
            .then_some(Message::ToggleInputMonitor(track_id)),
    )
    .style(move |theme: &Theme, status| {
        if monitor_enabled {
            iced::widget::button::success(theme, status)
        } else {
            iced::widget::button::secondary(theme, status)
        }
    })
    .padding(if touch {
        [tokens::SPACING_MD as u16, tokens::SPACING_SM as u16]
    } else {
        [2_u16, 6_u16]
    })
    .width(touch_target_width)
    .height(touch_target_height);
    #[cfg(not(feature = "audio-device"))]
    let input_monitor = text("").size(10);
    let mix_gesture = app
        .track_mix_gesture
        .filter(|gesture| gesture.track_id == track_id);
    let volume_db = mix_gesture
        .map(|gesture| gesture.after_volume_db)
        .unwrap_or_else(|| track.volume_db());
    let pan = mix_gesture
        .map(|gesture| gesture.after_pan)
        .unwrap_or_else(|| track.pan());
    let volume_text = app
        .track_volume_edits
        .get(&track_id)
        .cloned()
        .unwrap_or_else(|| format!("{volume_db:.1}"));
    let pan_text = app
        .track_pan_edits
        .get(&track_id)
        .cloned()
        .unwrap_or_else(|| format!("{pan:.2}"));
    let volume_slider = slider(-60.0..=6.0, volume_db.clamp(-60.0, 6.0), move |value| {
        Message::PreviewTrackVolume(track_id, value)
    })
    .step(0.1_f32)
    .shift_step(0.01_f32)
    .on_release(Message::CommitTrackVolume(track_id))
    .width(Length::Fill);
    let volume = slider_interaction(
        volume_slider.into(),
        Some(Message::ResetTrackVolumeByDoubleClick(track_id)),
        Message::CancelTrackMixGesture,
    );
    let volume: Element<'_, Message> = if touch {
        container(volume)
            .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
            .align_y(Alignment::Center)
            .into()
    } else {
        volume
    };
    let pan_slider = slider(-1.0..=1.0, pan, move |value| {
        Message::PreviewTrackPan(track_id, value)
    })
    .step(0.01_f32)
    .shift_step(0.001_f32)
    .on_release(Message::CommitTrackPan(track_id))
    .width(Length::Fill);
    let pan_slider = slider_interaction(
        pan_slider.into(),
        Some(Message::ResetTrackPanByDoubleClick(track_id)),
        Message::CancelTrackMixGesture,
    );
    let pan_slider: Element<'_, Message> = if touch {
        container(pan_slider)
            .height(Length::Fixed(tokens::TOUCH_TARGET_MIN))
            .align_y(Alignment::Center)
            .into()
    } else {
        pan_slider
    };
    let volume_has_error = app
        .track_draft_errors
        .contains_key(&(track_id, TrackDraftField::Volume));
    let volume_value = text_input("dB", &volume_text)
        .on_input(move |value| Message::TrackVolumeTextChanged(track_id, value))
        .on_submit(Message::CommitTrackVolumeText(track_id))
        .style(track_draft_input_style(volume_has_error))
        .padding(if touch { 10_u16 } else { 4_u16 })
        .width(Length::Fixed(48.0));
    let volume_controls: Element<'_, Message> = if touch {
        let controls = row![mute, solo, record_arm];
        let controls = if track.is_record_armed() || show_input_monitor {
            controls.push(input_monitor)
        } else {
            controls
        };
        column![
            controls,
            row![volume, volume_value].spacing(tokens::SPACING_XS)
        ]
        .spacing(tokens::SPACING_XS)
        .into()
    } else if compact {
        let controls = row![mute, solo, record_arm];
        let controls = if track.is_record_armed() || show_input_monitor {
            controls.push(input_monitor)
        } else {
            controls
        };
        controls
            .push(volume)
            .push(volume_value)
            .spacing(2)
            .align_y(Alignment::Center)
            .into()
    } else {
        row![
            mute,
            solo,
            record_arm,
            input_monitor,
            text("Vol").size(11),
            volume,
            volume_value,
            text("dB").size(10),
            button("0")
                .style(iced::widget::button::secondary)
                .on_press(Message::ResetTrackVolume(track_id))
                .padding([2, 5]),
        ]
        .spacing(2)
        .align_y(Alignment::Center)
        .into()
    };
    let pan_has_error = app
        .track_draft_errors
        .contains_key(&(track_id, TrackDraftField::Pan));
    let pan_value = text_input("-1 to 1", &pan_text)
        .on_input(move |value| Message::TrackPanTextChanged(track_id, value))
        .on_submit(Message::CommitTrackPanText(track_id))
        .style(track_draft_input_style(pan_has_error))
        .padding(if touch { 10_u16 } else { 4_u16 })
        .width(Length::Fixed(48.0));
    let pan_controls = row![
        text(format!("Pan {}", pan_label(pan))).size(11),
        pan_slider,
        pan_value,
        button("C")
            .style(iced::widget::button::secondary)
            .on_press(Message::ResetTrackPan(track_id))
            .padding(if touch {
                [tokens::SPACING_MD as u16, tokens::SPACING_SM as u16]
            } else {
                [2_u16, 5_u16]
            })
            .width(touch_target_width)
            .height(touch_target_height),
    ]
    .spacing(if touch {
        tokens::SPACING_XS
    } else {
        tokens::SPACING_TIGHT
    })
    .align_y(Alignment::Center);
    (volume_controls.into(), pan_controls.into())
}

struct SliderInteractionState {
    previous_click: Option<mouse::Click>,
    press_position: Option<iced::Point>,
    left_pressed: bool,
    cancelled: bool,
}

struct SliderInteraction<'a> {
    content: Element<'a, Message>,
    reset_message: Option<Message>,
    cancel_message: Message,
}

pub(super) fn slider_interaction<'a>(
    content: Element<'a, Message>,
    reset_message: Option<Message>,
    cancel_message: Message,
) -> Element<'a, Message> {
    Element::new(SliderInteraction {
        content,
        reset_message,
        cancel_message,
    })
}

fn record_left_click(previous_click: &mut Option<mouse::Click>, position: iced::Point) -> bool {
    let click = mouse::Click::new(position, mouse::Button::Left, *previous_click);
    *previous_click = Some(click);
    click.kind() == mouse::click::Kind::Double
}

fn invalidate_click_after_drag(state: &mut SliderInteractionState, position: iced::Point) {
    if state
        .press_position
        .is_some_and(|start| start.distance(position) >= 6.0)
    {
        state.previous_click = None;
    }
}

fn forward_left_release_after_cancel(
    state: &mut SliderInteractionState,
    event: &iced::Event,
) -> bool {
    if !state.cancelled
        || !matches!(
            event,
            iced::Event::Mouse(iced::mouse::Event::ButtonReleased(mouse::Button::Left))
        )
    {
        return false;
    }

    state.left_pressed = false;
    state.cancelled = false;
    state.press_position = None;
    true
}

impl Widget<Message, Theme, iced::Renderer> for SliderInteraction<'_> {
    fn size(&self) -> iced::Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> iced::Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &iced::Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<SliderInteractionState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(SliderInteractionState {
            previous_click: None,
            press_position: None,
            left_pressed: false,
            cancelled: false,
        })
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(self.content.as_widget())]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[self.content.as_widget()]);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &iced::Rectangle,
    ) {
        let state = tree.state.downcast_mut::<SliderInteractionState>();
        if state.cancelled && !forward_left_release_after_cancel(state, event) {
            match event {
                iced::Event::Mouse(
                    iced::mouse::Event::ButtonPressed(mouse::Button::Right)
                    | iced::mouse::Event::ButtonReleased(mouse::Button::Right),
                ) if !cursor.is_over(layout.bounds()) => {}
                iced::Event::Mouse(_) => {
                    shell.capture_event();
                    return;
                }
                _ => {}
            }
        }
        match event {
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(mouse::Button::Left))
                if let Some(position) = cursor.position_over(layout.bounds()) =>
            {
                state.left_pressed = true;
                state.press_position = Some(position);
                if let Some(reset_message) = &self.reset_message
                    && record_left_click(&mut state.previous_click, position)
                {
                    shell.publish(reset_message.clone());
                    shell.capture_event();
                    return;
                }
            }
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(mouse::Button::Right))
                if state.left_pressed && cursor.is_over(layout.bounds()) =>
            {
                state.cancelled = true;
                state.previous_click = None;
                shell.publish(self.cancel_message.clone());
                shell.capture_event();
                return;
            }
            iced::Event::Mouse(iced::mouse::Event::CursorMoved { .. }) => {
                if let Some(position) = cursor.position() {
                    invalidate_click_after_drag(state, position);
                }
            }
            iced::Event::Mouse(iced::mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.left_pressed = false;
                state.press_position = None;
            }
            _ => {}
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &iced::Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &iced::Renderer,
        viewport: &iced::Rectangle,
        translation: iced::Vector,
    ) -> Option<iced::advanced::overlay::Element<'a, Message, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
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
        CommandId::DuplicateSelectedItem => "Duplicate".to_owned(),
        CommandId::DuplicateSelectedAudioItem => "Duplicate".to_owned(),
        CommandId::DuplicateSelectedMidiItem => "Duplicate".to_owned(),
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

#[cfg(test)]
mod tests {
    use super::{
        SliderInteractionState, forward_left_release_after_cancel, invalidate_click_after_drag,
        record_left_click,
    };
    use iced::{Event, Point, advanced::mouse};

    #[test]
    fn consecutive_left_clicks_are_detected_as_double_click() {
        let mut previous_click = None;
        let position = Point::new(12.0, 8.0);

        assert!(!record_left_click(&mut previous_click, position));
        assert!(record_left_click(&mut previous_click, position));
        assert_eq!(
            previous_click.expect("click should be recorded").kind(),
            mouse::click::Kind::Double
        );
    }

    #[test]
    fn dragging_invalidates_the_previous_click_for_double_click_detection() {
        let start = Point::new(12.0, 8.0);
        let mut state = SliderInteractionState {
            previous_click: Some(mouse::Click::new(start, mouse::Button::Left, None)),
            press_position: Some(start),
            left_pressed: false,
            cancelled: false,
        };

        invalidate_click_after_drag(&mut state, Point::new(18.0, 8.0));

        assert!(state.previous_click.is_none());
    }

    #[test]
    fn cancelled_slider_forwards_left_release_to_clear_the_child_widget_state() {
        let mut state = SliderInteractionState {
            previous_click: None,
            press_position: Some(Point::new(12.0, 8.0)),
            left_pressed: true,
            cancelled: true,
        };

        let should_forward = forward_left_release_after_cancel(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        );

        assert!(should_forward);
        assert!(!state.left_pressed);
        assert!(!state.cancelled);
        assert!(state.press_position.is_none());
    }
}
