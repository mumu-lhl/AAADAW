use super::super::{App, MIDI_EDITOR_KEY_WIDTH, Message, MidiEditorLane, MidiEditorTool};
use super::tokens::{PANEL_PADDING, ROW_GAP, SPACING_LG, SPACING_XS, TOUCH_TARGET_MIN};
use crate::timeline::{SnapGrid, TimelineEvent};
use aaadaw_core::{
    ItemId, MidiControllerData, MidiItem, MidiNoteData, MidiPitchBendData, NoteId, Project,
};
use iced::advanced::text::{Alignment as TextAlignment, LineHeight, Shaping};
use iced::widget::canvas::{self, Text};
use iced::widget::{
    button, canvas as canvas_widget, column, container, pick_list, row, scrollable, text,
};
use iced::{
    Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, Theme, keyboard, mouse,
};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

const KEY_WIDTH: f32 = MIDI_EDITOR_KEY_WIDTH;
const HEADER_HEIGHT: f32 = 28.0;
const NOTE_ROW_HEIGHT: f32 = 18.0;
const PITCH_COUNT: u8 = 36;
const MOUSE_WHEEL_LINE_PIXELS: f32 = 40.0;
const PITCH_STEPS_PER_WHEEL_LINE: i8 = 3;
const VELOCITY_LANE_HEIGHT: f32 = 104.0;
const SUSTAIN_LANE_HEIGHT: f32 = 96.0;
const VOLUME_LANE_HEIGHT: f32 = 72.0;
const PAN_LANE_HEIGHT: f32 = 72.0;
const PITCH_BEND_LANE_HEIGHT: f32 = 72.0;
const MODULATION_LANE_HEIGHT: f32 = 72.0;
const EXPRESSION_LANE_HEIGHT: f32 = 72.0;
const CONTROLLER_CONTEXT_WIDTH: f32 = 112.0;
const CONTROLLER_CONTEXT_HEIGHT: f32 = 24.0;

fn source_tick_is_visible(item: &MidiItem, tick: u64) -> bool {
    let start_tick = item.source_offset_ticks();
    (start_tick..start_tick.saturating_add(item.length_ticks())).contains(&tick)
}

fn visible_note_duration(item: &MidiItem, note: &aaadaw_core::MidiNote) -> u64 {
    note.duration().min(
        item.source_offset_ticks()
            .saturating_add(item.length_ticks())
            .saturating_sub(note.tick()),
    )
}

fn visible_edit_cursor_tick(app: &App) -> u64 {
    app.midi_editor_paste_target_tick(app.midi_editor_item_id)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MidiSnap {
    grid: SnapGrid,
    enabled: bool,
    cursor_tick: u64,
    context_menu_epoch: u64,
}

impl Default for MidiSnap {
    fn default() -> Self {
        Self {
            grid: SnapGrid::Sixteenth,
            enabled: true,
            cursor_tick: 0,
            context_menu_epoch: 0,
        }
    }
}

pub(super) fn view(app: &App) -> Element<'_, Message> {
    view_with_profile(app, false)
}

pub(super) fn mobile_view(app: &App) -> Element<'_, Message> {
    view_with_profile(app, true)
}

fn view_with_profile(app: &App, touch_targets: bool) -> Element<'_, Message> {
    let Some(item_id) = app.midi_editor_item_id else {
        return container(text("No MIDI item selected"))
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    };
    let Some(item) = app
        .project
        .midi_items()
        .iter()
        .find(|item| item.id() == item_id)
    else {
        return container(text("The MIDI item was deleted"))
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    };
    let title = app
        .project
        .tracks()
        .iter()
        .find(|track| track.id() == item.track_id())
        .map_or("Track", |track| track.name());
    let playhead_tick = app.midi_editor_playhead_tick(item);
    let edit_toolbar: Element<'_, Message> = if touch_targets {
        row![
            text(format!("Piano roll · {} notes", item.notes().len())).width(Length::Fill),
            button("Done")
                .height(Length::Fixed(TOUCH_TARGET_MIN))
                .on_press(Message::CloseMidiEditor),
        ]
        .align_y(iced::Alignment::Center)
        .into()
    } else {
        row![
            text(format!(
                "Piano roll · Track {title} · {} notes",
                item.notes().len()
            ))
            .width(Length::Fill),
            button("Copy")
                .style(iced::widget::button::secondary)
                .on_press_maybe((!app.midi_editor_selected_notes.is_empty()).then_some(
                    Message::CopyMidiNotes(
                        item_id,
                        app.midi_editor_selected_notes.iter().copied().collect(),
                    ),
                ))
                .padding([SPACING_XS / 2.0, SPACING_XS]),
            button("Paste")
                .style(iced::widget::button::secondary)
                .on_press_maybe(
                    (!app.midi_note_clipboard.notes.is_empty())
                        .then_some(Message::PasteMidiNotes(item_id)),
                )
                .padding([SPACING_XS / 2.0, SPACING_XS]),
            button("Duplicate")
                .style(iced::widget::button::secondary)
                .on_press_maybe(
                    (!app.midi_editor_selected_notes.is_empty())
                        .then_some(Message::DuplicateMidiNotes(item_id)),
                )
                .padding([SPACING_XS / 2.0, SPACING_XS]),
            button("Delete notes")
                .style(iced::widget::button::danger)
                .on_press_maybe((!app.midi_editor_selected_notes.is_empty()).then_some(
                    Message::DeleteMidiNotes(
                        item_id,
                        app.midi_editor_selected_notes.iter().copied().collect(),
                    ),
                ))
                .padding([SPACING_XS / 2.0, SPACING_XS]),
            roll_button("×", Message::CloseMidiEditor),
        ]
        .spacing(ROW_GAP)
        .align_y(iced::Alignment::Center)
        .into()
    };
    let navigation_toolbar: Element<'_, Message> = if touch_targets {
        scrollable(
            row![
                touch_roll_button("Fit", Message::FitPianoRollToNotes(item_id)),
                touch_roll_button("− Beat", Message::PianoRollPan(-1)),
                touch_roll_button("+ Beat", Message::PianoRollPan(1)),
                touch_roll_button("Zoom −", Message::PianoRollZoom(0.8)),
                touch_roll_button("Zoom +", Message::PianoRollZoom(1.25)),
                touch_roll_button("− Oct", Message::PianoRollPitchScroll(-12)),
                touch_roll_button("+ Oct", Message::PianoRollPitchScroll(12)),
            ]
            .spacing(SPACING_XS),
        )
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default(),
        ))
        .height(Length::Fixed(TOUCH_TARGET_MIN))
        .into()
    } else {
        row![
            roll_button("Fit notes", Message::FitPianoRollToNotes(item_id)),
            roll_button(
                if app.midi_editor_follow_playhead {
                    "Follow on"
                } else {
                    "Follow off"
                },
                Message::TogglePianoRollFollowPlayhead,
            ),
            roll_button("− Beat", Message::PianoRollPan(-1)),
            roll_button("+ Beat", Message::PianoRollPan(1)),
            roll_button("Zoom −", Message::PianoRollZoom(0.8)),
            roll_button("Zoom +", Message::PianoRollZoom(1.25)),
            roll_button("− Oct", Message::PianoRollPitchScroll(-12)),
            roll_button("+ Oct", Message::PianoRollPitchScroll(12)),
        ]
        .spacing(ROW_GAP)
        .align_y(iced::Alignment::Center)
        .into()
    };
    let touch_tool_toolbar: Element<'_, Message> = row![
        tool_button("Select", MidiEditorTool::Select, app.midi_editor_tool),
        tool_button("Draw", MidiEditorTool::Draw, app.midi_editor_tool),
        tool_button("Erase", MidiEditorTool::Erase, app.midi_editor_tool),
    ]
    .spacing(SPACING_XS)
    .into();
    let grid_toolbar: Element<'_, Message> = if touch_targets {
        scrollable(
            row![
                button(if !app.timeline.has_snap_grid() {
                    "Snap unavailable"
                } else if app.timeline.snap_enabled {
                    "Snap On"
                } else {
                    "Snap Off"
                })
                .height(Length::Fixed(TOUCH_TARGET_MIN))
                .style(if app.timeline.snap_enabled {
                    iced::widget::button::warning
                } else {
                    iced::widget::button::secondary
                })
                .on_press_maybe(
                    app.timeline
                        .has_snap_grid()
                        .then_some(Message::Timeline(TimelineEvent::ToggleSnap)),
                ),
                pick_list(&SnapGrid::ALL[..], Some(app.timeline.snap_grid), |grid| {
                    Message::Timeline(TimelineEvent::SetSnapGrid(grid))
                })
                .padding([SPACING_LG, SPACING_XS])
                .width(Length::Fixed(120.0)),
            ]
            .spacing(SPACING_XS),
        )
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default(),
        ))
        .height(Length::Fixed(TOUCH_TARGET_MIN + SPACING_LG))
        .into()
    } else {
        row![
            text("Shared grid").size(12),
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
                    .then_some(Message::Timeline(TimelineEvent::ToggleSnap)),
            ),
            pick_list(&SnapGrid::ALL[..], Some(app.timeline.snap_grid), |grid| {
                Message::Timeline(TimelineEvent::SetSnapGrid(grid))
            })
            .width(Length::Fixed(112.0)),
            text("Wheel: pitch · Shift+wheel: time · Ctrl/Cmd+wheel: zoom").size(11),
        ]
        .spacing(ROW_GAP)
        .align_y(iced::Alignment::Center)
        .into()
    };
    let gesture_hints: Element<'_, Message> = if touch_targets {
        iced::widget::Space::new().into()
    } else {
        column![
            text("Notes: click positions cursor · double-click inserts · empty-space drag selects · Ctrl/Cmd-click toggles · Shift-click ranges")
                .size(10)
                .width(Length::Fill),
            text("Ctrl/Cmd-drag copies · Shift-drag bypasses Snap · Esc cancels · click ruler sets paste target")
                .size(10)
                .width(Length::Fill),
        ]
        .spacing(2)
        .into()
    };
    let midi_snap = MidiSnap {
        grid: app.timeline.snap_grid,
        enabled: app.timeline.snap_enabled,
        cursor_tick: visible_edit_cursor_tick(app),
        context_menu_epoch: app.midi_expression_context_menu_epoch,
    };
    let ticks_per_beat = u64::from(app.project.settings().ppq());
    let pitch_canvas = canvas_widget::Canvas::new(PianoRoll {
        project: &app.project,
        item,
        item_id,
        selected: &app.midi_editor_selected_notes,
        origin_tick: app.midi_editor_origin_tick,
        high_pitch: app.midi_editor_high_pitch,
        pitch_rows: app.midi_editor_pitch_rows,
        pitch_row_height: app.midi_editor_pitch_row_height,
        pixels_per_beat: app.midi_editor_pixels_per_beat,
        ticks_per_beat,
        snap: MidiSnap {
            grid: app.timeline.snap_grid,
            enabled: app.timeline.snap_enabled,
            cursor_tick: visible_edit_cursor_tick(app),
            context_menu_epoch: app.midi_expression_context_menu_epoch,
        },
        playhead_tick,
        region: RollRegion::Pitch,
        tool: app.midi_editor_tool,
    })
    .width(Length::Fill)
    .height(Length::Fill);
    let lane_toolbar = row![
        lane_button(
            "Velocity",
            MidiEditorLane::Velocity,
            app.midi_editor_lane,
            touch_targets,
        ),
        lane_button(
            "Sustain",
            MidiEditorLane::Sustain,
            app.midi_editor_lane,
            touch_targets,
        ),
        lane_button(
            "Volume CC7",
            MidiEditorLane::Volume,
            app.midi_editor_lane,
            touch_targets,
        ),
        lane_button(
            "Pan CC10",
            MidiEditorLane::Pan,
            app.midi_editor_lane,
            touch_targets,
        ),
        lane_button(
            "Pitch Bend",
            MidiEditorLane::PitchBend,
            app.midi_editor_lane,
            touch_targets,
        ),
        lane_button(
            "Mod CC1",
            MidiEditorLane::Modulation,
            app.midi_editor_lane,
            touch_targets,
        ),
        lane_button(
            "Expression",
            MidiEditorLane::Expression,
            app.midi_editor_lane,
            touch_targets,
        ),
    ]
    .spacing(ROW_GAP)
    .align_y(iced::Alignment::Center);
    let lane_toolbar: Element<'_, Message> = if touch_targets {
        scrollable(lane_toolbar)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default(),
            ))
            .height(Length::Fixed(TOUCH_TARGET_MIN))
            .into()
    } else {
        lane_toolbar.into()
    };
    let active_lane: Element<'_, Message> = match app.midi_editor_lane {
        MidiEditorLane::Velocity => {
            let canvas = canvas_widget::Canvas::new(PianoRoll {
                project: &app.project,
                item,
                item_id,
                selected: &app.midi_editor_selected_notes,
                origin_tick: app.midi_editor_origin_tick,
                high_pitch: app.midi_editor_high_pitch,
                pitch_rows: app.midi_editor_pitch_rows,
                pitch_row_height: app.midi_editor_pitch_row_height,
                pixels_per_beat: app.midi_editor_pixels_per_beat,
                ticks_per_beat,
                snap: midi_snap,
                playhead_tick,
                region: RollRegion::Velocity,
                tool: app.midi_editor_tool,
            })
            .width(Length::Fill)
            .height(Length::Fixed(VELOCITY_LANE_HEIGHT));
            lane_row("Velocity", VELOCITY_LANE_HEIGHT, canvas)
        }
        MidiEditorLane::Sustain => controller_lane(
            item,
            item_id,
            64,
            "Sustain",
            SUSTAIN_LANE_HEIGHT,
            app,
            ticks_per_beat,
        ),
        MidiEditorLane::Volume => controller_lane(
            item,
            item_id,
            7,
            "Volume CC7",
            VOLUME_LANE_HEIGHT,
            app,
            ticks_per_beat,
        ),
        MidiEditorLane::Pan => controller_lane(
            item,
            item_id,
            10,
            "Pan CC10",
            PAN_LANE_HEIGHT,
            app,
            ticks_per_beat,
        ),
        MidiEditorLane::PitchBend => {
            pitch_bend_lane(item, item_id, PITCH_BEND_LANE_HEIGHT, app, ticks_per_beat)
        }
        MidiEditorLane::Modulation => controller_lane(
            item,
            item_id,
            1,
            "Mod CC1",
            MODULATION_LANE_HEIGHT,
            app,
            ticks_per_beat,
        ),
        MidiEditorLane::Expression => controller_lane(
            item,
            item_id,
            11,
            "Expression",
            EXPRESSION_LANE_HEIGHT,
            app,
            ticks_per_beat,
        ),
    };
    let mut content = column![edit_toolbar];
    if let Some(feedback) = app.midi_editor_feedback.as_deref() {
        content = content.push(text(feedback).size(12));
    }
    content = content.push(navigation_toolbar);
    if touch_targets {
        content = content.push(touch_tool_toolbar);
    }
    content
        .push(grid_toolbar)
        .push(gesture_hints)
        .push(lane_toolbar)
        .push(pitch_canvas)
        .push(active_lane)
        .spacing(ROW_GAP)
        .padding(if touch_targets {
            SPACING_XS
        } else {
            PANEL_PADDING
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn lane_button(
    label: &'static str,
    lane: MidiEditorLane,
    selected: MidiEditorLane,
    touch_targets: bool,
) -> iced::widget::Button<'static, Message> {
    button(label)
        .style(if lane == selected {
            iced::widget::button::primary
        } else {
            iced::widget::button::secondary
        })
        .on_press(Message::SelectMidiEditorLane(lane))
        .height(Length::Fixed(if touch_targets {
            TOUCH_TARGET_MIN
        } else {
            32.0
        }))
        .padding([SPACING_XS / 2.0, SPACING_XS])
}

fn tool_button(
    label: &'static str,
    tool: MidiEditorTool,
    selected: MidiEditorTool,
) -> iced::widget::Button<'static, Message> {
    button(label)
        .height(Length::Fixed(TOUCH_TARGET_MIN))
        .style(if tool == selected {
            iced::widget::button::primary
        } else {
            iced::widget::button::secondary
        })
        .on_press(Message::SelectMidiEditorTool(tool))
}

fn touch_roll_button(
    label: &'static str,
    message: Message,
) -> iced::widget::Button<'static, Message> {
    button(label)
        .height(Length::Fixed(TOUCH_TARGET_MIN))
        .on_press(message)
}

fn lane_row<'a>(
    label: &'a str,
    height: f32,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    row![
        container(text(label))
            .width(Length::Fixed(KEY_WIDTH))
            .height(Length::Fixed(height))
            .center_y(Length::Fill)
            .padding(SPACING_XS),
        content.into()
    ]
    .height(Length::Fixed(height))
    .into()
}

fn controller_lane<'a>(
    item: &'a MidiItem,
    item_id: ItemId,
    controller: u8,
    label: &'a str,
    lane_height: f32,
    app: &App,
    ticks_per_beat: u64,
) -> Element<'a, Message> {
    let canvas = canvas_widget::Canvas::new(ControllerLane {
        item,
        item_id,
        controller,
        lane_height,
        origin_tick: app.midi_editor_origin_tick,
        pixels_per_beat: app.midi_editor_pixels_per_beat,
        ticks_per_beat,
        playhead_tick: app.midi_editor_playhead_tick(item),
        snap: MidiSnap {
            grid: app.timeline.snap_grid,
            enabled: app.timeline.snap_enabled,
            cursor_tick: visible_edit_cursor_tick(app),
            context_menu_epoch: app.midi_expression_context_menu_epoch,
        },
    })
    .width(Length::Fill)
    .height(Length::Fixed(lane_height));
    lane_row(label, lane_height, canvas)
}

fn pitch_bend_lane<'a>(
    item: &'a MidiItem,
    item_id: ItemId,
    lane_height: f32,
    app: &App,
    ticks_per_beat: u64,
) -> Element<'a, Message> {
    let canvas = canvas_widget::Canvas::new(PitchBendLane {
        item,
        item_id,
        lane_height,
        origin_tick: app.midi_editor_origin_tick,
        pixels_per_beat: app.midi_editor_pixels_per_beat,
        ticks_per_beat,
        playhead_tick: app.midi_editor_playhead_tick(item),
        snap: MidiSnap {
            grid: app.timeline.snap_grid,
            enabled: app.timeline.snap_enabled,
            cursor_tick: visible_edit_cursor_tick(app),
            context_menu_epoch: app.midi_expression_context_menu_epoch,
        },
    })
    .width(Length::Fill)
    .height(Length::Fixed(lane_height));
    lane_row("Pitch Bend", lane_height, canvas)
}

struct ControllerLane<'a> {
    item: &'a MidiItem,
    item_id: ItemId,
    controller: u8,
    lane_height: f32,
    origin_tick: u64,
    pixels_per_beat: f32,
    ticks_per_beat: u64,
    playhead_tick: Option<u64>,
    snap: MidiSnap,
}

#[derive(Default)]
struct ControllerLaneInteraction {
    drag: Option<ControllerDrag>,
    context_menu: Option<(usize, Point)>,
    context_menu_epoch: u64,
    modifiers: keyboard::Modifiers,
}

fn sync_context_menu_epoch(
    context_menu: &mut Option<(usize, Point)>,
    current_epoch: &mut u64,
    next_epoch: u64,
) {
    if *current_epoch != next_epoch {
        *context_menu = None;
        *current_epoch = next_epoch;
    }
}

struct ControllerDrag {
    controllers: Vec<MidiControllerData>,
    index: Option<usize>,
    original: Option<MidiControllerData>,
    current: MidiControllerData,
}

impl ControllerLane<'_> {
    fn mapping(&self) -> RollMapping {
        RollMapping {
            origin_tick: self
                .origin_tick
                .saturating_add(self.item.source_offset_ticks()),
            pixels_per_beat: self.pixels_per_beat,
            ticks_per_beat: self.ticks_per_beat,
            snap: self.snap,
            high_pitch: 0,
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
        }
    }

    fn point_data(&self, point: Point, ignore_snap: bool) -> MidiControllerData {
        let mapping = self.mapping();
        MidiControllerData {
            controller: self.controller,
            tick: mapping
                .snap_tick(mapping.tick_at_x(point.x), ignore_snap)
                .max(self.item.source_offset_ticks())
                .min(
                    self.item
                        .source_offset_ticks()
                        .saturating_add(self.item.length_ticks())
                        .saturating_sub(1),
                ),
            value: self.value_at_y(point.y),
        }
    }

    fn value_at_y(&self, y: f32) -> u8 {
        if self.controller == 64 {
            if y < self.lane_height / 2.0 { 127 } else { 0 }
        } else {
            value_at_y(y, self.lane_height)
        }
    }

    fn closest_point(&self, point: Point) -> Option<usize> {
        let mapping = self.mapping();
        self.item
            .controllers()
            .iter()
            .enumerate()
            .filter(|(_, controller)| {
                controller.controller == self.controller
                    && source_tick_is_visible(self.item, controller.tick)
            })
            .filter_map(|(index, controller)| {
                let x_distance = mapping.x_at_tick(controller.tick) - point.x;
                let y_distance = controller_y(controller.value, self.lane_height) - point.y;
                let distance = x_distance.hypot(y_distance);
                (distance <= 12.0).then_some((index, distance))
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(index, _)| index)
    }
}

fn controller_target_is_duplicate(
    controllers: &[MidiControllerData],
    moving_index: Option<usize>,
    target: MidiControllerData,
) -> bool {
    controllers.iter().enumerate().any(|(index, point)| {
        Some(index) != moving_index
            && point.controller == target.controller
            && point.tick == target.tick
    })
}

fn pitch_bend_target_is_duplicate(
    bends: &[MidiPitchBendData],
    moving_index: Option<usize>,
    target: MidiPitchBendData,
) -> bool {
    bends
        .iter()
        .enumerate()
        .any(|(index, bend)| Some(index) != moving_index && bend.tick == target.tick)
}

fn is_escape_key(event: &Event) -> bool {
    matches!(
        event,
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            repeat: false,
            ..
        })
    )
}

fn cancel_canvas_drag_on_escape<T, C>(
    event: &Event,
    drag: &mut Option<T>,
    context_menu: Option<&mut Option<C>>,
) -> Option<canvas::Action<Message>> {
    if !is_escape_key(event) {
        return None;
    }
    let mut cancelled = drag.take().is_some();
    if let Some(context_menu) = context_menu {
        cancelled |= context_menu.take().is_some();
    }
    cancelled.then(|| canvas::Action::request_redraw().and_capture())
}

impl canvas::Program<Message> for ControllerLane<'_> {
    type State = ControllerLaneInteraction;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        sync_context_menu_epoch(
            &mut state.context_menu,
            &mut state.context_menu_epoch,
            self.snap.context_menu_epoch,
        );
        if let Some(action) =
            cancel_canvas_drag_on_escape(event, &mut state.drag, Some(&mut state.context_menu))
        {
            return Some(action);
        }
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.modifiers = *modifiers;
            if let (Some(point), Some(drag)) = (cursor.position_in(bounds), state.drag.as_mut()) {
                drag.current = self.point_data(point, state.modifiers.shift());
                return Some(canvas::Action::request_redraw());
            }
            return None;
        }
        if let Event::Mouse(mouse::Event::WheelScrolled { delta }) = event {
            let position = cursor.position_in(bounds)?;
            return wheel_navigation_message(delta, state.modifiers, position, false)
                .map(canvas::Action::publish);
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let point = cursor.position_in(bounds)?;
                if let Some((index, origin)) = state.context_menu.take() {
                    let menu_bounds = Rectangle::new(
                        origin,
                        Size::new(CONTROLLER_CONTEXT_WIDTH, CONTROLLER_CONTEXT_HEIGHT),
                    );
                    if menu_bounds.contains(point) {
                        let mut controllers = self.item.controllers().to_vec();
                        if index < controllers.len() {
                            controllers.remove(index);
                            return Some(canvas::Action::publish(Message::SetMidiControllers(
                                self.item_id,
                                controllers,
                            )));
                        }
                    }
                    return Some(canvas::Action::capture());
                }
                if point.x < 0.0 || point.y < 0.0 || self.item.length_ticks() == 0 {
                    return Some(canvas::Action::capture());
                }
                let controllers = self.item.controllers().to_vec();
                if let Some(index) = self.closest_point(point) {
                    let original = controllers[index];
                    state.drag = Some(ControllerDrag {
                        controllers,
                        index: Some(index),
                        original: Some(original),
                        current: original,
                    });
                } else {
                    state.drag = Some(ControllerDrag {
                        controllers,
                        index: None,
                        original: None,
                        current: self.point_data(point, state.modifiers.shift()),
                    });
                }
                Some(canvas::Action::capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                let point = cursor.position_in(bounds)?;
                let Some(index) = self.closest_point(point) else {
                    state.context_menu = None;
                    return Some(canvas::Action::capture());
                };
                let origin = Point::new(
                    point
                        .x
                        .min((bounds.width - CONTROLLER_CONTEXT_WIDTH).max(0.0)),
                    point
                        .y
                        .min((bounds.height - CONTROLLER_CONTEXT_HEIGHT).max(0.0)),
                );
                state.context_menu = Some((index, origin));
                Some(canvas::Action::request_redraw())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let point = cursor.position_in(bounds)?;
                let Some(drag) = &mut state.drag else {
                    return None;
                };
                drag.current = self.point_data(point, state.modifiers.shift());
                Some(canvas::Action::request_redraw())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let mut drag = state.drag.take()?;
                if let Some(point) = cursor.position_in(bounds) {
                    drag.current = self.point_data(point, state.modifiers.shift());
                }
                let mut controllers = drag.controllers;
                if let Some(index) = drag.index {
                    controllers[index] = drag.current;
                } else {
                    controllers.push(drag.current);
                }
                let changed_index = drag.index.unwrap_or(controllers.len() - 1);
                let duplicate_position =
                    controller_target_is_duplicate(&controllers, Some(changed_index), drag.current);
                if duplicate_position {
                    return Some(canvas::Action::publish(Message::MidiEditorFeedback(
                        "CC points cannot share the same tick".to_owned(),
                    )));
                }
                if drag.original == Some(drag.current) {
                    return Some(canvas::Action::capture());
                }
                Some(canvas::Action::publish(Message::SetMidiControllers(
                    self.item_id,
                    controllers,
                )))
            }
            Event::Window(iced::window::Event::RedrawRequested(_)) if state.drag.is_some() => {
                Some(canvas::Action::request_redraw())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), Color::from_rgb8(27, 31, 34));
        let mapping = self.mapping();
        let grid_ticks = mapping.grid_ticks();
        let end_tick = mapping.tick_at_x(bounds.width);
        let mut tick = self.origin_tick / grid_ticks * grid_ticks;
        while tick <= end_tick.saturating_add(grid_ticks) {
            let x = mapping.x_at_tick(tick);
            if (0.0..=bounds.width).contains(&x) {
                let path = canvas::Path::line(Point::new(x, 0.0), Point::new(x, bounds.height));
                frame.stroke(
                    &path,
                    canvas::Stroke::default().with_color(Color::from_rgb8(43, 48, 51)),
                );
            }
            if tick > u64::MAX - grid_ticks {
                break;
            }
            tick += grid_ticks;
        }
        draw_playhead_line(
            &mut frame,
            mapping,
            self.playhead_tick,
            0.0,
            0.0,
            bounds.width,
            bounds.height,
        );

        let controllers = self.item.controllers();
        let mut value = controllers
            .iter()
            .filter(|controller| {
                controller.controller == self.controller
                    && source_tick_is_visible(self.item, controller.tick)
                    && controller.tick < mapping.origin_tick
            })
            .max_by_key(|controller| controller.tick)
            .map_or(0, |controller| controller.value);
        let mut segment_start = 0.0;
        for controller in controllers.iter().filter(|controller| {
            controller.controller == self.controller
                && source_tick_is_visible(self.item, controller.tick)
                && mapping.origin_tick <= controller.tick
                && controller.tick <= end_tick
        }) {
            let x = mapping.x_at_tick(controller.tick).clamp(0.0, bounds.width);
            let old_y = controller_y(value, bounds.height);
            let new_y = controller_y(controller.value, bounds.height);
            let path = canvas::Path::line(Point::new(segment_start, old_y), Point::new(x, old_y));
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_width(2.0)
                    .with_color(Color::from_rgb8(111, 190, 150)),
            );
            if old_y != new_y {
                let transition = canvas::Path::line(Point::new(x, old_y), Point::new(x, new_y));
                frame.stroke(
                    &transition,
                    canvas::Stroke::default()
                        .with_width(2.0)
                        .with_color(Color::from_rgb8(111, 190, 150)),
                );
            }
            frame.fill_rectangle(
                Point::new(x - 4.0, new_y - 4.0),
                Size::new(8.0, 8.0),
                Color::from_rgb8(175, 225, 196),
            );
            segment_start = x;
            value = controller.value;
        }
        if let Some(drag) = &state.drag {
            let invalid_target =
                controller_target_is_duplicate(&drag.controllers, drag.index, drag.current);
            let current_x = mapping
                .x_at_tick(drag.current.tick)
                .clamp(0.0, bounds.width);
            let current_y = controller_y(drag.current.value, bounds.height);
            let path = canvas::Path::line(
                Point::new(segment_start, current_y),
                Point::new(bounds.width, current_y),
            );
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_width(2.0)
                    .with_color(if invalid_target {
                        Color::from_rgb8(226, 92, 84)
                    } else {
                        Color::from_rgb8(111, 190, 150)
                    }),
            );
            frame.fill_rectangle(
                Point::new(current_x - 5.0, current_y - 5.0),
                Size::new(10.0, 10.0),
                if invalid_target {
                    Color::from_rgb8(255, 107, 92)
                } else {
                    Color::WHITE
                },
            );
        } else {
            let y = controller_y(value, bounds.height);
            let path =
                canvas::Path::line(Point::new(segment_start, y), Point::new(bounds.width, y));
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_width(2.0)
                    .with_color(Color::from_rgb8(111, 190, 150)),
            );
        }
        if state.context_menu_epoch == self.snap.context_menu_epoch
            && let Some((_, origin)) = state.context_menu
        {
            frame.fill_rectangle(
                origin,
                Size::new(CONTROLLER_CONTEXT_WIDTH, CONTROLLER_CONTEXT_HEIGHT),
                Color::from_rgb8(49, 54, 59),
            );
            frame.stroke_rectangle(
                origin,
                Size::new(CONTROLLER_CONTEXT_WIDTH, CONTROLLER_CONTEXT_HEIGHT),
                canvas::Stroke::default().with_color(Color::from_rgb8(104, 112, 118)),
            );
            frame.fill_text(Text {
                content: format!("Delete CC{} point", self.controller),
                position: Point::new(origin.x + 7.0, origin.y + 2.0),
                max_width: CONTROLLER_CONTEXT_WIDTH - 12.0,
                color: Color::WHITE,
                size: Pixels(11.0),
                line_height: LineHeight::Relative(1.0),
                font: Font::default(),
                align_x: TextAlignment::Left,
                align_y: iced::alignment::Vertical::Center,
                shaping: Shaping::Basic,
            });
        }
        let cursor_x = mapping.x_at_tick(self.snap.cursor_tick);
        if (0.0..=bounds.width).contains(&cursor_x) {
            let cursor_line = canvas::Path::line(
                Point::new(cursor_x, 0.0),
                Point::new(cursor_x, bounds.height),
            );
            frame.stroke(
                &cursor_line,
                canvas::Stroke::default()
                    .with_width(1.5)
                    .with_color(Color::from_rgb8(255, 184, 92)),
            );
        }
        draw_playhead_line(
            &mut frame,
            mapping,
            self.playhead_tick,
            0.0,
            0.0,
            bounds.width,
            bounds.height,
        );
        vec![frame.into_geometry()]
    }
}

struct PitchBendLane<'a> {
    item: &'a MidiItem,
    item_id: ItemId,
    lane_height: f32,
    origin_tick: u64,
    pixels_per_beat: f32,
    ticks_per_beat: u64,
    playhead_tick: Option<u64>,
    snap: MidiSnap,
}

#[derive(Default)]
struct PitchBendLaneInteraction {
    drag: Option<PitchBendDrag>,
    context_menu: Option<(usize, Point)>,
    context_menu_epoch: u64,
    modifiers: keyboard::Modifiers,
}

struct PitchBendDrag {
    bends: Vec<MidiPitchBendData>,
    index: Option<usize>,
    original: Option<MidiPitchBendData>,
    current: MidiPitchBendData,
}

impl PitchBendLane<'_> {
    fn mapping(&self) -> RollMapping {
        RollMapping {
            origin_tick: self
                .origin_tick
                .saturating_add(self.item.source_offset_ticks()),
            pixels_per_beat: self.pixels_per_beat,
            ticks_per_beat: self.ticks_per_beat,
            snap: self.snap,
            high_pitch: 0,
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
        }
    }

    fn point_data(&self, point: Point, ignore_snap: bool) -> MidiPitchBendData {
        let mapping = self.mapping();
        MidiPitchBendData {
            tick: mapping
                .snap_tick(mapping.tick_at_x(point.x), ignore_snap)
                .max(self.item.source_offset_ticks())
                .min(
                    self.item
                        .source_offset_ticks()
                        .saturating_add(self.item.length_ticks())
                        .saturating_sub(1),
                ),
            value: pitch_bend_value_at_y(point.y, self.lane_height),
        }
    }

    fn closest_point(&self, point: Point) -> Option<usize> {
        let mapping = self.mapping();
        self.item
            .pitch_bends()
            .iter()
            .enumerate()
            .filter(|(_, bend)| source_tick_is_visible(self.item, bend.tick))
            .filter_map(|(index, bend)| {
                let x_distance = mapping.x_at_tick(bend.tick) - point.x;
                let y_distance = pitch_bend_y(bend.value, self.lane_height) - point.y;
                let distance = x_distance.hypot(y_distance);
                (distance <= 12.0).then_some((index, distance))
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(index, _)| index)
    }
}

impl canvas::Program<Message> for PitchBendLane<'_> {
    type State = PitchBendLaneInteraction;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        sync_context_menu_epoch(
            &mut state.context_menu,
            &mut state.context_menu_epoch,
            self.snap.context_menu_epoch,
        );
        if let Some(action) =
            cancel_canvas_drag_on_escape(event, &mut state.drag, Some(&mut state.context_menu))
        {
            return Some(action);
        }
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.modifiers = *modifiers;
            if let (Some(point), Some(drag)) = (cursor.position_in(bounds), state.drag.as_mut()) {
                drag.current = self.point_data(point, state.modifiers.shift());
                return Some(canvas::Action::request_redraw());
            }
            return None;
        }
        if let Event::Mouse(mouse::Event::WheelScrolled { delta }) = event {
            let position = cursor.position_in(bounds)?;
            return wheel_navigation_message(delta, state.modifiers, position, false)
                .map(canvas::Action::publish);
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let point = cursor.position_in(bounds)?;
                if let Some((index, origin)) = state.context_menu.take() {
                    let menu_bounds = Rectangle::new(
                        origin,
                        Size::new(CONTROLLER_CONTEXT_WIDTH, CONTROLLER_CONTEXT_HEIGHT),
                    );
                    if menu_bounds.contains(point) {
                        let mut bends = self.item.pitch_bends().to_vec();
                        if index < bends.len() {
                            bends.remove(index);
                            return Some(canvas::Action::publish(Message::SetMidiPitchBends(
                                self.item_id,
                                bends,
                            )));
                        }
                    }
                    return Some(canvas::Action::capture());
                }
                if point.x < 0.0 || point.y < 0.0 || self.item.length_ticks() == 0 {
                    return Some(canvas::Action::capture());
                }
                let bends = self.item.pitch_bends().to_vec();
                if let Some(index) = self.closest_point(point) {
                    let original = bends[index];
                    state.drag = Some(PitchBendDrag {
                        bends,
                        index: Some(index),
                        original: Some(original),
                        current: original,
                    });
                } else {
                    state.drag = Some(PitchBendDrag {
                        bends,
                        index: None,
                        original: None,
                        current: self.point_data(point, state.modifiers.shift()),
                    });
                }
                Some(canvas::Action::capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                let point = cursor.position_in(bounds)?;
                let Some(index) = self.closest_point(point) else {
                    state.context_menu = None;
                    return Some(canvas::Action::capture());
                };
                let origin = Point::new(
                    point
                        .x
                        .min((bounds.width - CONTROLLER_CONTEXT_WIDTH).max(0.0)),
                    point
                        .y
                        .min((bounds.height - CONTROLLER_CONTEXT_HEIGHT).max(0.0)),
                );
                state.context_menu = Some((index, origin));
                Some(canvas::Action::request_redraw())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let point = cursor.position_in(bounds)?;
                let Some(drag) = &mut state.drag else {
                    return None;
                };
                drag.current = self.point_data(point, state.modifiers.shift());
                Some(canvas::Action::request_redraw())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let mut drag = state.drag.take()?;
                if let Some(point) = cursor.position_in(bounds) {
                    drag.current = self.point_data(point, state.modifiers.shift());
                }
                let mut bends = drag.bends;
                if let Some(index) = drag.index {
                    bends[index] = drag.current;
                } else {
                    bends.push(drag.current);
                }
                let changed_index = drag.index.unwrap_or(bends.len() - 1);
                let duplicate_position =
                    pitch_bend_target_is_duplicate(&bends, Some(changed_index), drag.current);
                if duplicate_position || drag.original == Some(drag.current) {
                    if duplicate_position {
                        return Some(canvas::Action::publish(Message::MidiEditorFeedback(
                            "Pitch-bend points cannot share the same tick".to_owned(),
                        )));
                    }
                    return Some(canvas::Action::capture());
                }
                Some(canvas::Action::publish(Message::SetMidiPitchBends(
                    self.item_id,
                    bends,
                )))
            }
            Event::Window(iced::window::Event::RedrawRequested(_)) if state.drag.is_some() => {
                Some(canvas::Action::request_redraw())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), Color::from_rgb8(27, 31, 34));
        let mapping = self.mapping();
        let grid_ticks = mapping.grid_ticks();
        let end_tick = mapping.tick_at_x(bounds.width);
        let mut tick = self.origin_tick / grid_ticks * grid_ticks;
        while tick <= end_tick.saturating_add(grid_ticks) {
            let x = mapping.x_at_tick(tick);
            if (0.0..=bounds.width).contains(&x) {
                let path = canvas::Path::line(Point::new(x, 0.0), Point::new(x, bounds.height));
                frame.stroke(
                    &path,
                    canvas::Stroke::default().with_color(Color::from_rgb8(43, 48, 51)),
                );
            }
            if tick > u64::MAX - grid_ticks {
                break;
            }
            tick += grid_ticks;
        }
        draw_playhead_line(
            &mut frame,
            mapping,
            self.playhead_tick,
            0.0,
            0.0,
            bounds.width,
            bounds.height,
        );
        let center_y = pitch_bend_y(8192, bounds.height);
        let center_line = canvas::Path::line(
            Point::new(0.0, center_y),
            Point::new(bounds.width, center_y),
        );
        frame.stroke(
            &center_line,
            canvas::Stroke::default()
                .with_width(1.5)
                .with_color(Color::from_rgb8(106, 112, 118)),
        );

        let mut value = self
            .item
            .pitch_bends()
            .iter()
            .filter(|bend| {
                source_tick_is_visible(self.item, bend.tick) && bend.tick < mapping.origin_tick
            })
            .max_by_key(|bend| bend.tick)
            .map_or(8192, |bend| bend.value);
        let mut segment_start = 0.0;
        for bend in self.item.pitch_bends().iter().filter(|bend| {
            source_tick_is_visible(self.item, bend.tick)
                && mapping.origin_tick <= bend.tick
                && bend.tick <= end_tick
        }) {
            let x = mapping.x_at_tick(bend.tick).clamp(0.0, bounds.width);
            let old_y = pitch_bend_y(value, bounds.height);
            let new_y = pitch_bend_y(bend.value, bounds.height);
            let path = canvas::Path::line(Point::new(segment_start, old_y), Point::new(x, old_y));
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_width(2.0)
                    .with_color(Color::from_rgb8(128, 165, 226)),
            );
            let transition = canvas::Path::line(Point::new(x, old_y), Point::new(x, new_y));
            frame.stroke(
                &transition,
                canvas::Stroke::default()
                    .with_width(2.0)
                    .with_color(Color::from_rgb8(128, 165, 226)),
            );
            frame.fill_rectangle(
                Point::new(x - 4.0, new_y - 4.0),
                Size::new(8.0, 8.0),
                Color::from_rgb8(177, 201, 245),
            );
            segment_start = x;
            value = bend.value;
        }
        if let Some(drag) = &state.drag {
            let invalid_target =
                pitch_bend_target_is_duplicate(&drag.bends, drag.index, drag.current);
            let x = mapping
                .x_at_tick(drag.current.tick)
                .clamp(0.0, bounds.width);
            let y = pitch_bend_y(drag.current.value, bounds.height);
            let path =
                canvas::Path::line(Point::new(segment_start, y), Point::new(bounds.width, y));
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_width(2.0)
                    .with_color(if invalid_target {
                        Color::from_rgb8(226, 92, 84)
                    } else {
                        Color::from_rgb8(128, 165, 226)
                    }),
            );
            frame.fill_rectangle(
                Point::new(x - 5.0, y - 5.0),
                Size::new(10.0, 10.0),
                if invalid_target {
                    Color::from_rgb8(255, 107, 92)
                } else {
                    Color::WHITE
                },
            );
        } else {
            let y = pitch_bend_y(value, bounds.height);
            let path =
                canvas::Path::line(Point::new(segment_start, y), Point::new(bounds.width, y));
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_width(2.0)
                    .with_color(Color::from_rgb8(128, 165, 226)),
            );
        }
        if state.context_menu_epoch == self.snap.context_menu_epoch
            && let Some((_, origin)) = state.context_menu
        {
            frame.fill_rectangle(
                origin,
                Size::new(CONTROLLER_CONTEXT_WIDTH, CONTROLLER_CONTEXT_HEIGHT),
                Color::from_rgb8(49, 54, 59),
            );
            frame.stroke_rectangle(
                origin,
                Size::new(CONTROLLER_CONTEXT_WIDTH, CONTROLLER_CONTEXT_HEIGHT),
                canvas::Stroke::default().with_color(Color::from_rgb8(104, 112, 118)),
            );
            frame.fill_text(Text {
                content: "Delete pitch bend".to_owned(),
                position: Point::new(origin.x + 7.0, origin.y + 2.0),
                max_width: CONTROLLER_CONTEXT_WIDTH - 12.0,
                color: Color::WHITE,
                size: Pixels(11.0),
                line_height: LineHeight::Relative(1.0),
                font: Font::default(),
                align_x: TextAlignment::Left,
                align_y: iced::alignment::Vertical::Center,
                shaping: Shaping::Basic,
            });
        }
        let cursor_x = mapping.x_at_tick(self.snap.cursor_tick);
        if (0.0..=bounds.width).contains(&cursor_x) {
            let cursor_line = canvas::Path::line(
                Point::new(cursor_x, 0.0),
                Point::new(cursor_x, bounds.height),
            );
            frame.stroke(
                &cursor_line,
                canvas::Stroke::default()
                    .with_width(1.5)
                    .with_color(Color::from_rgb8(255, 184, 92)),
            );
        }
        vec![frame.into_geometry()]
    }
}

fn pitch_bend_y(value: u16, height: f32) -> f32 {
    height - 10.0 - (f32::from(value) / 16_383.0) * (height - 20.0)
}

fn pitch_bend_value_at_y(y: f32, height: f32) -> u16 {
    (((height - 10.0 - y) / (height - 20.0)) * 16_383.0)
        .round()
        .clamp(0.0, 16_383.0) as u16
}

fn controller_y(value: u8, height: f32) -> f32 {
    height - 10.0 - (f32::from(value) / 127.0) * (height - 20.0)
}

fn value_at_y(y: f32, height: f32) -> u8 {
    (((height - 10.0 - y) / (height - 20.0)) * 127.0)
        .round()
        .clamp(0.0, 127.0) as u8
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RollMapping {
    origin_tick: u64,
    pixels_per_beat: f32,
    ticks_per_beat: u64,
    high_pitch: u8,
    pitch_rows: u8,
    pitch_row_height: f32,
    snap: MidiSnap,
}

impl RollMapping {
    fn grid_ticks(self) -> u64 {
        self.snap
            .grid
            .tick_interval(self.ticks_per_beat.min(u64::from(u32::MAX)) as u32)
            .unwrap_or_else(|| (self.ticks_per_beat / 4).max(1))
    }

    fn snap_tick(self, tick: u64, ignore_snap: bool) -> u64 {
        let grid_ticks = self
            .snap
            .grid
            .tick_interval(self.ticks_per_beat.min(u64::from(u32::MAX)) as u32);
        snap_tick(tick, grid_ticks, self.snap.enabled, ignore_snap)
    }

    fn tick_at_x(self, x: f32) -> u64 {
        let ticks = (f64::from(x.max(0.0)) / f64::from(self.pixels_per_beat)
            * self.ticks_per_beat as f64)
            .round() as u64;
        self.origin_tick.saturating_add(ticks)
    }

    fn pitch_at_y(self, y: f32) -> u8 {
        let row = (y.max(0.0) / self.pitch_row_height)
            .floor()
            .min(f32::from(self.pitch_rows.saturating_sub(1))) as u8;
        self.high_pitch.saturating_sub(row).min(127)
    }

    fn x_at_tick(self, tick: u64) -> f32 {
        (i128::from(tick) - i128::from(self.origin_tick)) as f32 / self.ticks_per_beat as f32
            * self.pixels_per_beat
    }

    fn y_at_pitch(self, pitch: u8) -> f32 {
        f32::from(self.high_pitch.saturating_sub(pitch)) * self.pitch_row_height
    }
}

fn draw_playhead_line(
    frame: &mut canvas::Frame,
    mapping: RollMapping,
    playhead_tick: Option<u64>,
    left: f32,
    top: f32,
    width: f32,
    height: f32,
) {
    let Some(tick) = playhead_tick else {
        return;
    };
    let x = left + mapping.x_at_tick(tick);
    if (left..=width).contains(&x) {
        let line = canvas::Path::line(Point::new(x, top), Point::new(x, height));
        frame.stroke(
            &line,
            canvas::Stroke::default()
                .with_width(1.5)
                .with_color(Color::from_rgb8(244, 98, 89)),
        );
    }
}

struct PianoRoll<'a> {
    project: &'a Project,
    item: &'a MidiItem,
    item_id: ItemId,
    selected: &'a HashSet<NoteId>,
    origin_tick: u64,
    high_pitch: u8,
    pitch_rows: u8,
    pitch_row_height: f32,
    pixels_per_beat: f32,
    ticks_per_beat: u64,
    snap: MidiSnap,
    playhead_tick: Option<u64>,
    region: RollRegion,
    tool: MidiEditorTool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RollRegion {
    Pitch,
    Velocity,
}

#[derive(Default)]
struct Interaction {
    modifiers: keyboard::Modifiers,
    drag: Option<NoteDrag>,
    empty_drag: Option<EmptySpaceGesture>,
    last_empty_click: Option<(Instant, Point)>,
    hovered_velocity_note: Option<NoteId>,
    hovered_pitch: Option<u8>,
    auditioning_pitch: Option<u8>,
}

#[derive(Clone, Copy)]
struct EmptySpaceGesture {
    start: Point,
    current: Point,
    additive: bool,
    marquee: bool,
}

#[derive(Clone)]
struct NoteDrag {
    start: Point,
    anchor_note_id: NoteId,
    notes: Vec<(NoteId, MidiNoteData)>,
    resize: bool,
    delta_tick: i64,
    delta_pitch: i16,
    velocity: bool,
    delta_velocity: i16,
    copy: bool,
    moved: bool,
    click_selection: Option<HashSet<NoteId>>,
    original_selection: HashSet<NoteId>,
}

impl canvas::Program<Message> for PianoRoll<'_> {
    type State = Interaction;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        if let Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) = event
            && matches!(
                key.as_ref(),
                keyboard::Key::Named(keyboard::key::Named::Escape)
            )
            && (state.drag.is_some()
                || state.empty_drag.is_some()
                || state.auditioning_pitch.is_some())
        {
            let original_selection = state.drag.take().map(|drag| drag.original_selection);
            state.empty_drag = None;
            state.last_empty_click = None;
            let release_preview = state.auditioning_pitch.take().is_some();
            if release_preview {
                return Some(canvas::Action::publish(Message::ReleaseMidiPreview).and_capture());
            }
            return Some(
                original_selection.map_or_else(canvas::Action::capture, |selected| {
                    canvas::Action::publish(Message::SelectMidiNotes(selected))
                }),
            );
        }
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.modifiers = *modifiers;
            if let (Some(position), Some(drag)) = (cursor.position_in(bounds), state.drag.as_mut())
            {
                self.update_drag(drag, self.roll_point(position), state.modifiers.shift());
                return Some(canvas::Action::request_redraw());
            }
            return None;
        }
        if let Event::Mouse(mouse::Event::WheelScrolled { delta }) = event {
            let position = cursor.position_in(bounds)?;
            return wheel_navigation_message(
                delta,
                state.modifiers,
                position,
                self.region == RollRegion::Pitch,
            )
            .map(canvas::Action::publish);
        }
        if self.region == RollRegion::Pitch
            && let Event::Mouse(mouse::Event::CursorLeft) = event
            && state.auditioning_pitch.take().is_some()
        {
            return Some(canvas::Action::publish(Message::ReleaseMidiPreview));
        }
        if self.region == RollRegion::Pitch
            && let Event::Mouse(mouse::Event::CursorMoved { .. }) = event
        {
            let position = cursor.position_in(bounds);
            let hovered_pitch = position.and_then(|position| self.piano_key_pitch_at(position));
            let hover_changed = state.hovered_pitch != hovered_pitch;
            if hover_changed {
                state.hovered_pitch = hovered_pitch;
            }
            if state.auditioning_pitch.is_some() {
                match hovered_pitch {
                    Some(pitch) if state.auditioning_pitch != Some(pitch) => {
                        state.auditioning_pitch = Some(pitch);
                        return Some(canvas::Action::publish(Message::PreviewMidiNote(
                            self.item.track_id(),
                            pitch,
                        )));
                    }
                    None => {
                        state.auditioning_pitch = None;
                        return Some(canvas::Action::publish(Message::ReleaseMidiPreview));
                    }
                    _ => return Some(canvas::Action::request_redraw()),
                }
            }
            if hover_changed {
                return Some(canvas::Action::request_redraw());
            }
        }
        if self.region == RollRegion::Pitch
            && let Event::Keyboard(keyboard::Event::KeyPressed {
                key, repeat: false, ..
            }) = event
            && (state.modifiers.command() || state.modifiers.control())
            && let keyboard::Key::Character(character) = key.as_ref()
            && character.eq_ignore_ascii_case("c")
            && !self.selected.is_empty()
        {
            return Some(canvas::Action::publish(Message::CopyMidiNotes(
                self.item_id,
                self.selected.iter().copied().collect(),
            )));
        }
        if self.region == RollRegion::Pitch
            && let Event::Keyboard(keyboard::Event::KeyPressed {
                key, repeat: false, ..
            }) = event
            && (state.modifiers.command() || state.modifiers.control())
            && let keyboard::Key::Character(character) = key.as_ref()
            && character.eq_ignore_ascii_case("v")
        {
            return Some(canvas::Action::publish(Message::PasteMidiNotes(
                self.item_id,
            )));
        }
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key, repeat: false, ..
        }) = event
            && matches!(
                key.as_ref(),
                keyboard::Key::Named(
                    keyboard::key::Named::Delete | keyboard::key::Named::Backspace
                )
            )
        {
            if let Some(message) = delete_key_message(self.region, self.item_id, self.selected) {
                return Some(canvas::Action::publish(message));
            }
            return Some(canvas::Action::capture());
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let position = cursor.position_in(bounds)?;
                if self.region == RollRegion::Pitch
                    && let Some(pitch) = self.piano_key_pitch_at(position)
                {
                    state.auditioning_pitch = Some(pitch);
                    return Some(
                        canvas::Action::publish(Message::PreviewMidiNote(
                            self.item.track_id(),
                            pitch,
                        ))
                        .and_capture(),
                    );
                }
                if self.region == RollRegion::Pitch && position.y < HEADER_HEIGHT {
                    state.last_empty_click = None;
                    if position.x < KEY_WIDTH {
                        return Some(canvas::Action::capture());
                    }
                    let mapping = self.mapping();
                    let tick = mapping
                        .snap_tick(
                            mapping.tick_at_x(position.x - KEY_WIDTH),
                            state.modifiers.shift(),
                        )
                        .min(self.item.length_ticks());
                    return Some(canvas::Action::publish(Message::SetPianoRollCursor(
                        self.item_id,
                        tick,
                    )));
                }
                let point = Point::new(
                    position.x
                        - if self.region == RollRegion::Pitch {
                            KEY_WIDTH
                        } else {
                            0.0
                        },
                    position.y
                        - if self.region == RollRegion::Pitch {
                            HEADER_HEIGHT
                        } else {
                            0.0
                        },
                );
                if point.x < 0.0 || point.y < 0.0 {
                    return Some(canvas::Action::capture());
                }
                if self.region == RollRegion::Velocity {
                    state.last_empty_click = None;
                    let lane_y = point.y;
                    let Some(note_id) = velocity_note_at_x(
                        self.item.notes(),
                        self.content_mapping(),
                        self.item.source_offset_ticks(),
                        self.item
                            .source_offset_ticks()
                            .saturating_add(self.item.length_ticks()),
                        point.x,
                    ) else {
                        return Some(canvas::Action::capture());
                    };
                    let note = self.item.notes().iter().find(|note| note.id() == note_id)?;
                    let selected = selection_after_click(
                        self.selected,
                        note.id(),
                        state.modifiers.command() || state.modifiers.control(),
                    );
                    if (state.modifiers.command() || state.modifiers.control())
                        && !selected.contains(&note.id())
                    {
                        return Some(canvas::Action::publish(Message::SelectMidiNotes(selected)));
                    }
                    let note_ids = selected;
                    let notes = self
                        .item
                        .notes()
                        .iter()
                        .filter(|candidate| note_ids.contains(&candidate.id()))
                        .map(|candidate| (candidate.id(), note_data(candidate)))
                        .collect();
                    state.drag = Some(NoteDrag {
                        start: Point::new(point.x, lane_y),
                        anchor_note_id: note.id(),
                        notes,
                        resize: false,
                        delta_tick: 0,
                        delta_pitch: 0,
                        velocity: true,
                        delta_velocity: 0,
                        copy: false,
                        moved: false,
                        click_selection: None,
                        original_selection: self.selected.clone(),
                    });
                    return Some(
                        canvas::Action::publish(Message::SelectMidiNotes(note_ids)).and_capture(),
                    );
                }
                let touch_targets = cfg!(target_os = "android") || bounds.width < 720.0;
                let note_hit = self.note_at_point(point, touch_targets);
                if self.region == RollRegion::Pitch && self.tool == MidiEditorTool::Erase {
                    state.last_empty_click = None;
                    return Some(note_hit.map_or_else(canvas::Action::capture, |(note, _)| {
                        canvas::Action::publish(Message::DeleteMidiNotes(
                            self.item_id,
                            vec![note.id()],
                        ))
                    }));
                }
                if self.region == RollRegion::Pitch
                    && self.tool == MidiEditorTool::Draw
                    && note_hit.is_none()
                {
                    state.last_empty_click = None;
                    let mapping = self.mapping();
                    let data = MidiNoteData {
                        pitch: mapping.pitch_at_y(point.y),
                        tick: self
                            .item
                            .source_offset_ticks()
                            .saturating_add(mapping.snap_tick(mapping.tick_at_x(point.x), false)),
                        duration: mapping.grid_ticks(),
                        velocity: 96,
                    };
                    if data.tick.saturating_add(data.duration)
                        <= self
                            .item
                            .source_offset_ticks()
                            .saturating_add(self.item.length_ticks())
                    {
                        return Some(canvas::Action::publish(Message::AddMidiNoteAt(
                            self.item_id,
                            data,
                        )));
                    }
                    return Some(canvas::Action::capture());
                }
                let mapping = self.mapping();
                let Some((note, resize)) = note_hit else {
                    let now = Instant::now();
                    let double_click = state.last_empty_click.take().is_some_and(
                        |(previous_time, previous_point)| {
                            now.duration_since(previous_time) <= Duration::from_millis(450)
                                && (point.x - previous_point.x).hypot(point.y - previous_point.y)
                                    <= 5.0
                        },
                    );
                    if double_click {
                        let data = MidiNoteData {
                            pitch: mapping.pitch_at_y(point.y),
                            tick: self.item.source_offset_ticks().saturating_add(
                                mapping
                                    .snap_tick(mapping.tick_at_x(point.x), state.modifiers.shift()),
                            ),
                            duration: mapping.grid_ticks(),
                            velocity: 96,
                        };
                        if data.tick.saturating_add(data.duration)
                            <= self
                                .item
                                .source_offset_ticks()
                                .saturating_add(self.item.length_ticks())
                        {
                            return Some(canvas::Action::publish(Message::AddMidiNoteAt(
                                self.item_id,
                                data,
                            )));
                        }
                        return Some(canvas::Action::capture());
                    }
                    state.last_empty_click = Some((now, point));
                    state.empty_drag = Some(EmptySpaceGesture {
                        start: point,
                        current: point,
                        additive: state.modifiers.command() || state.modifiers.control(),
                        marquee: false,
                    });
                    return Some(canvas::Action::capture());
                };
                state.last_empty_click = None;
                let copy = state.modifiers.command() || state.modifiers.control();
                let selected = if copy {
                    selection_after_click(self.selected, note.id(), true)
                } else {
                    selection_after_click(self.selected, note.id(), false)
                };
                let note_ids = if resize || (copy && !self.selected.contains(&note.id())) {
                    HashSet::from([note.id()])
                } else if copy {
                    self.selected.clone()
                } else {
                    selected.clone()
                };
                let notes = self
                    .item
                    .notes()
                    .iter()
                    .filter(|candidate| note_ids.contains(&candidate.id()))
                    .map(|candidate| (candidate.id(), note_data(candidate)))
                    .collect();
                state.drag = Some(NoteDrag {
                    start: point,
                    anchor_note_id: note.id(),
                    notes,
                    resize,
                    delta_tick: 0,
                    delta_pitch: 0,
                    velocity: false,
                    delta_velocity: 0,
                    copy,
                    moved: false,
                    click_selection: copy.then_some(selected.clone()),
                    original_selection: self.selected.clone(),
                });
                Some(if copy {
                    canvas::Action::capture()
                } else {
                    canvas::Action::publish(Message::SelectMidiNotes(selected)).and_capture()
                })
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let position = cursor.position_in(bounds)?;
                let point = Point::new(
                    position.x
                        - if self.region == RollRegion::Pitch {
                            KEY_WIDTH
                        } else {
                            0.0
                        },
                    position.y
                        - if self.region == RollRegion::Pitch {
                            HEADER_HEIGHT
                        } else {
                            0.0
                        },
                );
                if let Some(gesture) = &mut state.empty_drag {
                    gesture.current = point;
                    if !gesture.marquee
                        && (point.x - gesture.start.x).hypot(point.y - gesture.start.y) >= 3.0
                    {
                        gesture.marquee = true;
                        state.last_empty_click = None;
                    }
                    return Some(canvas::Action::request_redraw());
                }
                if state.drag.is_none() {
                    let hovered = if self.region == RollRegion::Velocity {
                        velocity_note_at_x(
                            self.item.notes(),
                            self.content_mapping(),
                            self.item.source_offset_ticks(),
                            self.item
                                .source_offset_ticks()
                                .saturating_add(self.item.length_ticks()),
                            point.x,
                        )
                    } else {
                        None
                    };
                    if hovered != state.hovered_velocity_note {
                        state.hovered_velocity_note = hovered;
                        return Some(canvas::Action::request_redraw());
                    }
                }
                let Some(drag) = &mut state.drag else {
                    return None;
                };
                drag.moved |= (point.x - drag.start.x).hypot(point.y - drag.start.y) >= 3.0;
                self.update_drag(drag, point, state.modifiers.shift());
                Some(canvas::Action::request_redraw())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.auditioning_pitch.take().is_some() {
                    return Some(
                        canvas::Action::publish(Message::ReleaseMidiPreview).and_capture(),
                    );
                }
                if let Some(mut gesture) = state.empty_drag.take() {
                    let Some(position) = cursor.position_in(bounds) else {
                        state.last_empty_click = None;
                        return Some(canvas::Action::capture());
                    };
                    gesture.current = self.roll_point(position);
                    gesture.marquee |= (gesture.current.x - gesture.start.x)
                        .hypot(gesture.current.y - gesture.start.y)
                        >= 3.0;
                    if gesture.marquee {
                        let left = gesture.start.x.min(gesture.current.x);
                        let right = gesture.start.x.max(gesture.current.x);
                        let top = gesture.start.y.min(gesture.current.y);
                        let bottom = gesture.start.y.max(gesture.current.y);
                        let mapping = self.content_mapping();
                        let mut selected = if gesture.additive {
                            self.selected.clone()
                        } else {
                            HashSet::new()
                        };
                        for note in self.item.notes() {
                            if !source_tick_is_visible(self.item, note.tick()) {
                                continue;
                            }
                            let x = mapping.x_at_tick(note.tick());
                            let note_right = x + note_width_pixels(
                                note.duration(),
                                self.ticks_per_beat,
                                self.pixels_per_beat,
                            );
                            let y = mapping.y_at_pitch(note.pitch());
                            let note_bottom = y + self.pitch_row_height;
                            if x <= right && note_right >= left && y <= bottom && note_bottom >= top
                            {
                                selected.insert(note.id());
                            }
                        }
                        return Some(canvas::Action::publish(Message::SelectMidiNotes(selected)));
                    }
                    let mapping = self.mapping();
                    let tick = mapping
                        .snap_tick(
                            mapping.tick_at_x(gesture.current.x),
                            state.modifiers.shift(),
                        )
                        .min(self.item.length_ticks());
                    return Some(canvas::Action::publish(Message::SetPianoRollCursor(
                        self.item_id,
                        tick,
                    )));
                }
                let mut drag = state.drag.take()?;
                if let Some(position) = cursor.position_in(bounds) {
                    self.update_drag(
                        &mut drag,
                        self.roll_point(position),
                        state.modifiers.shift(),
                    );
                }
                state.hovered_velocity_note = None;
                let edits = drag
                    .notes
                    .iter()
                    .map(|(note_id, data)| {
                        (
                            *note_id,
                            note_drag_preview_data(
                                &drag,
                                *data,
                                self.item.source_offset_ticks(),
                                self.item
                                    .source_offset_ticks()
                                    .saturating_add(self.item.length_ticks()),
                            ),
                        )
                    })
                    .collect::<Vec<_>>();
                let invalid_target = edits.iter().any(|(id, data)| {
                    self.item
                        .notes()
                        .iter()
                        .find(|note| note.id() == *id)
                        .is_some_and(|_| {
                            data.tick < self.item.source_offset_ticks()
                                || data.tick.saturating_add(data.duration)
                                    > self
                                        .item
                                        .source_offset_ticks()
                                        .saturating_add(self.item.length_ticks())
                        })
                });
                if invalid_target {
                    Some(canvas::Action::publish(Message::MidiEditorFeedback(
                        "Resize rejected: note would extend beyond the MIDI item".to_owned(),
                    )))
                } else if drag.copy && !drag.moved {
                    Some(canvas::Action::publish(Message::SelectMidiNotes(
                        drag.click_selection.unwrap_or_else(|| {
                            selection_after_click(self.selected, drag.anchor_note_id, true)
                        }),
                    )))
                } else if drag.copy {
                    Some(canvas::Action::publish(Message::CopyDragMidiNotes(
                        self.item_id,
                        edits.into_iter().map(|(_, data)| data).collect(),
                    )))
                } else if edits.iter().any(|(id, data)| {
                    self.item
                        .notes()
                        .iter()
                        .find(|note| note.id() == *id)
                        .is_some_and(|note| {
                            note.tick() != data.tick
                                || note.duration() != data.duration
                                || note.pitch() != data.pitch
                                || note.velocity() != data.velocity
                        })
                }) {
                    Some(canvas::Action::publish(Message::EditMidiNotes(
                        self.item_id,
                        edits,
                    )))
                } else {
                    Some(canvas::Action::capture())
                }
            }
            Event::Window(iced::window::Event::RedrawRequested(_))
                if state.drag.is_some() || state.empty_drag.is_some() =>
            {
                Some(canvas::Action::request_redraw())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        if self.region == RollRegion::Velocity {
            self.draw_velocity(state, &mut frame, bounds);
            return vec![frame.into_geometry()];
        }
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), Color::from_rgb8(27, 31, 34));
        let grid_left = KEY_WIDTH;
        let grid_top = HEADER_HEIGHT;
        let mapping = self.mapping();
        frame.fill_rectangle(
            Point::new(0.0, 0.0),
            Size::new(bounds.width, HEADER_HEIGHT),
            Color::from_rgb8(34, 39, 42),
        );
        frame.fill_rectangle(
            Point::new(0.0, HEADER_HEIGHT),
            Size::new(KEY_WIDTH, bounds.height - HEADER_HEIGHT),
            Color::from_rgb8(37, 42, 45),
        );
        let end_tick = mapping.tick_at_x((bounds.width - KEY_WIDTH).max(0.0));
        let grid_ticks = mapping.grid_ticks();
        let mut tick = self.origin_tick / grid_ticks * grid_ticks;
        while tick <= end_tick.saturating_add(grid_ticks) {
            let x = grid_left + mapping.x_at_tick(tick);
            if x >= grid_left && x <= bounds.width {
                let project_tick = self.item.start_tick().saturating_add(tick);
                let position = self.project.musical_position_at_tick(project_tick).ok();
                let measure = position
                    .is_some_and(|position| position.beat() == 1 && position.tick_in_beat() == 0);
                let beat = position.is_some_and(|position| position.tick_in_beat() == 0);
                let color = if measure {
                    Color::from_rgb8(92, 101, 107)
                } else if beat {
                    Color::from_rgb8(62, 70, 75)
                } else {
                    Color::from_rgb8(43, 49, 53)
                };
                let path =
                    canvas::Path::line(Point::new(x, grid_top), Point::new(x, bounds.height));
                frame.stroke(
                    &path,
                    canvas::Stroke::default()
                        .with_color(color)
                        .with_width(if measure { 1.4 } else { 0.8 }),
                );
                if measure || beat {
                    let label = if measure {
                        position.map_or_else(
                            || "1".to_owned(),
                            |position| position.measure().to_string(),
                        )
                    } else {
                        position
                            .map_or_else(|| "1".to_owned(), |position| position.beat().to_string())
                    };
                    frame.fill_text(Text {
                        content: label,
                        position: Point::new(x + 4.0, 2.0),
                        max_width: 32.0,
                        color: Color::from_rgb8(195, 201, 205),
                        size: Pixels(11.0),
                        line_height: LineHeight::Relative(1.0),
                        font: Font::default(),
                        align_x: TextAlignment::Left,
                        align_y: iced::alignment::Vertical::Center,
                        shaping: Shaping::Basic,
                    });
                }
            }
            let next = tick.saturating_add(grid_ticks);
            if next <= tick {
                break;
            }
            tick = next;
        }
        for row in 0..self.pitch_rows {
            let pitch = self.high_pitch.saturating_sub(row);
            let y = grid_top + f32::from(row) * self.pitch_row_height;
            let black = is_black_key(pitch);
            let row_color = if black {
                Color::from_rgb8(31, 35, 38)
            } else {
                Color::from_rgb8(39, 44, 47)
            };
            frame.fill_rectangle(
                Point::new(grid_left, y),
                Size::new(bounds.width - grid_left, self.pitch_row_height),
                row_color,
            );
            let line = canvas::Path::line(Point::new(grid_left, y), Point::new(bounds.width, y));
            frame.stroke(
                &line,
                canvas::Stroke::default()
                    .with_color(Color::from_rgb8(52, 59, 63))
                    .with_width(0.7),
            );
            if !black {
                let key = piano_key_rect(pitch, y, self.pitch_row_height);
                let hovered = state.hovered_pitch == Some(pitch);
                frame.fill_rectangle(
                    Point::new(key.x, key.y),
                    Size::new(key.width, key.height),
                    if hovered {
                        Color::from_rgb8(220, 229, 232)
                    } else {
                        Color::from_rgb8(205, 211, 213)
                    },
                );
                let outline = canvas::Path::rectangle(
                    Point::new(key.x, key.y),
                    Size::new(key.width, key.height),
                );
                frame.stroke(
                    &outline,
                    canvas::Stroke::default()
                        .with_color(Color::from_rgb8(81, 89, 93))
                        .with_width(0.8),
                );
            }
        }
        for row in 0..self.pitch_rows {
            let pitch = self.high_pitch.saturating_sub(row);
            if pitch % 12 != 0 {
                continue;
            }
            let y = grid_top + f32::from(row) * self.pitch_row_height;
            frame.fill_text(Text {
                content: pitch_name(pitch),
                position: Point::new(KEY_WIDTH - 5.0, y + self.pitch_row_height / 2.0),
                max_width: KEY_WIDTH - 8.0,
                color: Color::from_rgb8(31, 37, 40),
                size: Pixels(11.0),
                line_height: LineHeight::Relative(1.0),
                font: Font::default(),
                align_x: TextAlignment::Right,
                align_y: iced::alignment::Vertical::Center,
                shaping: Shaping::Basic,
            });
        }
        for row in 0..self.pitch_rows {
            let pitch = self.high_pitch.saturating_sub(row);
            if !is_black_key(pitch) {
                continue;
            }
            let y = grid_top + f32::from(row) * self.pitch_row_height;
            let key = piano_key_rect(pitch, y, self.pitch_row_height);
            let hovered = state.hovered_pitch == Some(pitch);
            frame.fill_rectangle(
                Point::new(key.x, key.y),
                Size::new(key.width, key.height),
                if hovered {
                    Color::from_rgb8(63, 72, 77)
                } else {
                    Color::from_rgb8(27, 32, 35)
                },
            );
            let outline =
                canvas::Path::rectangle(Point::new(key.x, key.y), Size::new(key.width, key.height));
            frame.stroke(
                &outline,
                canvas::Stroke::default()
                    .with_color(Color::from_rgb8(14, 17, 19))
                    .with_width(0.9),
            );
        }
        if let Some(pitch) = state.hovered_pitch {
            let badge =
                canvas::Path::rectangle(Point::new(0.0, 0.0), Size::new(KEY_WIDTH, HEADER_HEIGHT));
            frame.fill(&badge, Color::from_rgb8(34, 39, 42));
            frame.fill_text(Text {
                content: pitch_name(pitch),
                position: Point::new(KEY_WIDTH / 2.0, HEADER_HEIGHT / 2.0),
                max_width: KEY_WIDTH - 8.0,
                color: Color::from_rgb8(236, 240, 242),
                size: Pixels(12.0),
                line_height: LineHeight::Relative(1.0),
                font: Font::default(),
                align_x: TextAlignment::Center,
                align_y: iced::alignment::Vertical::Center,
                shaping: Shaping::Basic,
            });
        }
        let item_end_x = grid_left + mapping.x_at_tick(self.item.length_ticks());
        if item_end_x < bounds.width {
            let shade_left = item_end_x.max(grid_left);
            frame.fill_rectangle(
                Point::new(shade_left, grid_top),
                Size::new(
                    (bounds.width - shade_left).max(0.0),
                    bounds.height - grid_top,
                ),
                Color::from_rgba8(9, 12, 14, 0.4),
            );
        }
        for note in self.item.notes() {
            if note.pitch() > self.high_pitch
                || note.pitch() < self.high_pitch.saturating_sub(self.pitch_rows - 1)
            {
                continue;
            }
            if !source_tick_is_visible(self.item, note.tick()) {
                continue;
            }
            let x = grid_left + self.content_mapping().x_at_tick(note.tick());
            let y = grid_top + mapping.y_at_pitch(note.pitch());
            let width = note_width_pixels(
                visible_note_duration(self.item, note),
                self.ticks_per_beat,
                self.pixels_per_beat,
            );
            let rect = canvas::Path::rectangle(
                Point::new(x, y + 2.0),
                Size::new(width, (self.pitch_row_height - 4.0).max(1.0)),
            );
            let color = if self.selected.contains(&note.id()) {
                Color::from_rgb8(104, 179, 204)
            } else {
                Color::from_rgb8(69, 139, 160)
            };
            frame.fill(&rect, color);
            if let Some(drag) = state
                .drag
                .as_ref()
                .filter(|drag| drag.notes.iter().any(|(id, _)| *id == note.id()))
            {
                let preview = note_drag_preview_data(
                    drag,
                    note_data(note),
                    self.item.source_offset_ticks(),
                    self.item
                        .source_offset_ticks()
                        .saturating_add(self.item.length_ticks()),
                );
                let preview_x = grid_left + self.content_mapping().x_at_tick(preview.tick);
                let preview_y = grid_top + mapping.y_at_pitch(preview.pitch);
                let preview_width =
                    note_width_pixels(preview.duration, self.ticks_per_beat, self.pixels_per_beat);
                let invalid_target = preview.tick < self.item.source_offset_ticks()
                    || preview.tick.saturating_add(preview.duration)
                        > self
                            .item
                            .source_offset_ticks()
                            .saturating_add(self.item.length_ticks());
                frame.fill_rectangle(
                    Point::new(preview_x, preview_y + 2.0),
                    Size::new(preview_width, (self.pitch_row_height - 4.0).max(1.0)),
                    if invalid_target {
                        Color::from_rgba8(230, 70, 65, 0.72)
                    } else if drag.copy {
                        Color::from_rgba8(117, 196, 143, 0.72)
                    } else {
                        Color::from_rgba8(214, 205, 111, 0.45)
                    },
                );
            }
        }
        if let Some(gesture) = state.empty_drag.filter(|gesture| gesture.marquee) {
            frame.fill_rectangle(
                Point::new(
                    KEY_WIDTH + gesture.start.x.min(gesture.current.x),
                    HEADER_HEIGHT + gesture.start.y.min(gesture.current.y),
                ),
                Size::new(
                    (gesture.start.x - gesture.current.x).abs(),
                    (gesture.start.y - gesture.current.y).abs(),
                ),
                Color::from_rgba8(100, 170, 190, 0.24),
            );
        }
        let cursor_x = grid_left + mapping.x_at_tick(self.snap.cursor_tick);
        if (grid_left..=bounds.width).contains(&cursor_x) {
            let cursor_line = canvas::Path::line(
                Point::new(cursor_x, grid_top),
                Point::new(cursor_x, bounds.height),
            );
            frame.stroke(
                &cursor_line,
                canvas::Stroke::default()
                    .with_width(1.5)
                    .with_color(Color::from_rgb8(255, 184, 92)),
            );
        }
        draw_playhead_line(
            &mut frame,
            mapping,
            self.playhead_tick,
            grid_left,
            grid_top,
            bounds.width,
            bounds.height,
        );
        vec![frame.into_geometry()]
    }
    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            if self.region == RollRegion::Pitch
                && let Some(position) = cursor.position_in(bounds)
            {
                let point = Point::new(position.x - KEY_WIDTH, position.y - HEADER_HEIGHT);
                let touch_targets = cfg!(target_os = "android") || bounds.width < 720.0;
                if let Some((_, resize)) = self.note_at_point(point, touch_targets) {
                    return if resize {
                        mouse::Interaction::ResizingHorizontally
                    } else {
                        mouse::Interaction::Grab
                    };
                }
            }
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

fn delete_key_message(
    region: RollRegion,
    item_id: ItemId,
    selected: &HashSet<NoteId>,
) -> Option<Message> {
    (region == RollRegion::Pitch && !selected.is_empty())
        .then(|| Message::DeleteMidiNotes(item_id, selected.iter().copied().collect()))
}

fn wheel_navigation_message(
    delta: &mouse::ScrollDelta,
    modifiers: keyboard::Modifiers,
    position: Point,
    has_key_gutter: bool,
) -> Option<Message> {
    let (wheel_x, wheel_y) = match delta {
        mouse::ScrollDelta::Lines { x, y } => {
            (*x * MOUSE_WHEEL_LINE_PIXELS, *y * MOUSE_WHEEL_LINE_PIXELS)
        }
        mouse::ScrollDelta::Pixels { x, y } => (*x, *y),
    };
    if modifiers.control() || modifiers.command() {
        let factor = (f64::from(wheel_y) * 0.002).exp() as f32;
        let anchor_x = if has_key_gutter {
            (position.x - KEY_WIDTH).max(0.0)
        } else {
            position.x
        };
        return Some(Message::PianoRollZoomAt(factor, anchor_x));
    }
    if modifiers.shift() || wheel_x != 0.0 {
        let delta = if wheel_x != 0.0 { wheel_x } else { wheel_y };
        return Some(Message::PianoRollPanPixels(-delta));
    }
    let pitch_delta = (wheel_y / MOUSE_WHEEL_LINE_PIXELS)
        .round()
        .clamp(-42.0, 42.0) as i8
        * PITCH_STEPS_PER_WHEEL_LINE;
    (pitch_delta != 0).then_some(Message::PianoRollPitchScroll(pitch_delta))
}

impl PianoRoll<'_> {
    fn piano_key_pitch_at(&self, position: Point) -> Option<u8> {
        if position.x >= KEY_WIDTH || position.y < HEADER_HEIGHT {
            return None;
        }
        let row = ((position.y - HEADER_HEIGHT) / self.pitch_row_height).floor() as u8;
        (row < self.pitch_rows).then(|| self.high_pitch.saturating_sub(row))
    }

    fn roll_point(&self, position: Point) -> Point {
        Point::new(
            position.x
                - if self.region == RollRegion::Pitch {
                    KEY_WIDTH
                } else {
                    0.0
                },
            position.y
                - if self.region == RollRegion::Pitch {
                    HEADER_HEIGHT
                } else {
                    0.0
                },
        )
    }

    fn update_drag(&self, drag: &mut NoteDrag, point: Point, ignore_snap: bool) {
        let ticks = (f64::from(point.x - drag.start.x) / f64::from(self.pixels_per_beat)
            * self.ticks_per_beat as f64)
            .round() as i64;
        let delta_tick = snap_delta(ticks, self.mapping(), ignore_snap);
        if drag.velocity {
            drag.delta_velocity = velocity_delta(drag.start.y, point.y);
        } else if drag.resize {
            drag.delta_tick = delta_tick;
        } else {
            let delta_pitch = ((drag.start.y - point.y) / self.pitch_row_height).round() as i16;
            (drag.delta_tick, drag.delta_pitch) = bounded_note_move_delta(
                &drag.notes,
                self.item.source_offset_ticks(),
                self.item
                    .source_offset_ticks()
                    .saturating_add(self.item.length_ticks()),
                delta_tick,
                delta_pitch,
            );
        }
    }

    fn note_at_point(
        &self,
        point: Point,
        touch_targets: bool,
    ) -> Option<(&aaadaw_core::MidiNote, bool)> {
        let mapping = self.content_mapping();
        let note_bounds = |note: &aaadaw_core::MidiNote| {
            let left = mapping.x_at_tick(note.tick());
            let width = note_width_pixels(
                visible_note_duration(self.item, note),
                self.ticks_per_beat,
                self.pixels_per_beat,
            );
            let top = mapping.y_at_pitch(note.pitch());
            (left, width, top)
        };
        if let Some(note) = self.item.notes().iter().rev().find(|note| {
            if !source_tick_is_visible(self.item, note.tick()) {
                return false;
            }
            let (left, width, top) = note_bounds(note);
            point.x >= left
                && point.x <= left + width
                && point.y >= top
                && point.y < top + self.pitch_row_height
        }) {
            let (left, width, _) = note_bounds(note);
            return Some((note, point.x >= left + width - resize_handle_width(width)));
        }
        if !touch_targets {
            return None;
        }
        self.item
            .notes()
            .iter()
            .rev()
            .filter(|note| source_tick_is_visible(self.item, note.tick()))
            .filter_map(|note| {
                let (left, width, top) = note_bounds(note);
                let hit_width = width.max(48.0);
                let hit_height = self.pitch_row_height.max(48.0);
                let hit_left = left - (hit_width - width) / 2.0;
                let hit_top = top - (hit_height - self.pitch_row_height) / 2.0;
                let in_touch_target = point.x >= hit_left
                    && point.x <= hit_left + hit_width
                    && point.y >= hit_top
                    && point.y <= hit_top + hit_height;
                in_touch_target.then_some((
                    note,
                    (point.x - (left + width / 2.0))
                        .hypot(point.y - (top + self.pitch_row_height / 2.0)),
                ))
            })
            .min_by(|(_, left), (_, right)| left.total_cmp(right))
            .map(|(note, _)| (note, false))
    }

    fn draw_velocity(
        &self,
        state: &Interaction,
        frame: &mut canvas::Frame<iced::Renderer>,
        bounds: Rectangle,
    ) {
        let mapping = self.mapping();
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), Color::from_rgb8(30, 35, 38));
        for fraction in [0.25, 0.5, 0.75] {
            let y = VELOCITY_LANE_HEIGHT * (1.0 - fraction);
            let line = canvas::Path::line(Point::new(0.0, y), Point::new(bounds.width, y));
            frame.stroke(
                &line,
                canvas::Stroke::default()
                    .with_color(Color::from_rgb8(57, 64, 68))
                    .with_width(0.7),
            );
        }
        let velocity_positions = velocity_handle_positions(
            self.item.notes(),
            self.content_mapping(),
            self.item.source_offset_ticks(),
            self.item
                .source_offset_ticks()
                .saturating_add(self.item.length_ticks()),
        );
        for note in self.item.notes() {
            if !source_tick_is_visible(self.item, note.tick()) {
                continue;
            }
            let velocity_x = velocity_positions
                .get(&note.id())
                .copied()
                .unwrap_or_default();
            let velocity = state
                .drag
                .as_ref()
                .filter(|drag| drag.velocity)
                .and_then(|drag| {
                    drag.notes
                        .iter()
                        .any(|(id, _)| *id == note.id())
                        .then(|| apply_velocity_delta(note.velocity(), drag.delta_velocity))
                })
                .unwrap_or_else(|| note.velocity());
            let velocity_height = f32::from(velocity) / 127.0 * (VELOCITY_LANE_HEIGHT - 20.0);
            let velocity_y = VELOCITY_LANE_HEIGHT - velocity_height - 2.0;
            let velocity_bar = canvas::Path::rectangle(
                Point::new(velocity_x, velocity_y),
                Size::new(4.0, velocity_height.max(1.0)),
            );
            frame.fill(
                &velocity_bar,
                if self.selected.contains(&note.id()) {
                    Color::from_rgb8(244, 184, 93)
                } else if state.hovered_velocity_note == Some(note.id()) {
                    Color::from_rgb8(221, 171, 91)
                } else {
                    Color::from_rgb8(174, 125, 66)
                },
            );
            if state.hovered_velocity_note == Some(note.id()) {
                frame.fill_text(Text {
                    content: velocity.to_string(),
                    position: Point::new(
                        velocity_x + 7.0,
                        (velocity_y - 6.0).clamp(10.0, VELOCITY_LANE_HEIGHT - 10.0),
                    ),
                    max_width: 28.0,
                    color: Color::from_rgb8(213, 218, 221),
                    size: Pixels(9.0),
                    line_height: LineHeight::Relative(1.0),
                    font: Font::default(),
                    align_x: TextAlignment::Left,
                    align_y: iced::alignment::Vertical::Center,
                    shaping: Shaping::Basic,
                });
            }
        }
        let cursor_x = mapping.x_at_tick(self.snap.cursor_tick);
        if (0.0..=bounds.width).contains(&cursor_x) {
            let cursor_line = canvas::Path::line(
                Point::new(cursor_x, 0.0),
                Point::new(cursor_x, bounds.height),
            );
            frame.stroke(
                &cursor_line,
                canvas::Stroke::default()
                    .with_width(1.5)
                    .with_color(Color::from_rgb8(255, 184, 92)),
            );
        }
        draw_playhead_line(
            frame,
            mapping,
            self.playhead_tick,
            0.0,
            0.0,
            bounds.width,
            bounds.height,
        );
    }

    fn mapping(&self) -> RollMapping {
        RollMapping {
            origin_tick: self.origin_tick,
            pixels_per_beat: self.pixels_per_beat.max(1.0),
            ticks_per_beat: self.ticks_per_beat,
            high_pitch: self.high_pitch,
            pitch_rows: self.pitch_rows,
            pitch_row_height: self.pitch_row_height,
            snap: self.snap,
        }
    }

    fn content_mapping(&self) -> RollMapping {
        let mut mapping = self.mapping();
        mapping.origin_tick = mapping
            .origin_tick
            .saturating_add(self.item.source_offset_ticks());
        mapping
    }
}

fn snap_tick(tick: u64, grid_ticks: Option<u64>, enabled: bool, ignore_snap: bool) -> u64 {
    let Some(grid_ticks) = grid_ticks.filter(|grid| enabled && !ignore_snap && *grid > 0) else {
        return tick;
    };
    (tick.saturating_add(grid_ticks / 2) / grid_ticks) * grid_ticks
}

fn snap_delta(delta_ticks: i64, mapping: RollMapping, ignore_snap: bool) -> i64 {
    let Some(grid_ticks) = mapping
        .snap
        .grid
        .tick_interval(mapping.ticks_per_beat.min(u64::from(u32::MAX)) as u32)
        .filter(|_| mapping.snap.enabled && !ignore_snap)
    else {
        return delta_ticks;
    };
    (delta_ticks as f64 / grid_ticks as f64).round() as i64 * grid_ticks as i64
}

fn note_data(note: &aaadaw_core::MidiNote) -> MidiNoteData {
    MidiNoteData {
        pitch: note.pitch(),
        tick: note.tick(),
        duration: note.duration(),
        velocity: note.velocity(),
    }
}

fn note_width_pixels(duration_ticks: u64, ticks_per_beat: u64, pixels_per_beat: f32) -> f32 {
    (duration_ticks as f32 / ticks_per_beat as f32 * pixels_per_beat).max(3.0)
}

fn resize_handle_width(note_width: f32) -> f32 {
    (note_width / 2.0).clamp(1.0, 9.0)
}

fn bounded_note_move_delta(
    notes: &[(NoteId, MidiNoteData)],
    source_start_tick: u64,
    source_end_tick: u64,
    delta_tick: i64,
    delta_pitch: i16,
) -> (i64, i16) {
    let Some(min_tick) = notes.iter().map(|(_, note)| note.tick).min() else {
        return (delta_tick, delta_pitch);
    };
    let max_end_tick = notes
        .iter()
        .map(|(_, note)| note.tick.saturating_add(note.duration))
        .max()
        .unwrap_or_default();
    let min_delta_tick = i128::from(source_start_tick) - i128::from(min_tick);
    let max_delta_tick = i128::from(source_end_tick) - i128::from(max_end_tick);
    let delta_tick = i128::from(delta_tick)
        .clamp(min_delta_tick, max_delta_tick)
        .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64;

    let min_pitch = notes
        .iter()
        .map(|(_, note)| note.pitch)
        .min()
        .unwrap_or_default();
    let max_pitch = notes
        .iter()
        .map(|(_, note)| note.pitch)
        .max()
        .unwrap_or_default();
    let min_delta_pitch = -i16::from(min_pitch);
    let max_delta_pitch = 127 - i16::from(max_pitch);
    let delta_pitch = delta_pitch.clamp(min_delta_pitch, max_delta_pitch);

    (delta_tick, delta_pitch)
}

fn note_drag_preview_data(
    drag: &NoteDrag,
    mut note: MidiNoteData,
    source_start_tick: u64,
    source_end_tick: u64,
) -> MidiNoteData {
    if drag.velocity {
        note.velocity = apply_velocity_delta(note.velocity, drag.delta_velocity);
    } else if drag.resize {
        note.duration = (i128::from(note.duration) + i128::from(drag.delta_tick)).max(1) as u64;
    } else {
        let (delta_tick, delta_pitch) = bounded_note_move_delta(
            &drag.notes,
            source_start_tick,
            source_end_tick,
            drag.delta_tick,
            drag.delta_pitch,
        );
        note.tick = (i128::from(note.tick) + i128::from(delta_tick)) as u64;
        note.pitch = (i16::from(note.pitch) + delta_pitch) as u8;
    }
    note
}

fn velocity_note_at_x(
    notes: &[aaadaw_core::MidiNote],
    mapping: RollMapping,
    source_start_tick: u64,
    source_end_tick: u64,
    x: f32,
) -> Option<NoteId> {
    let positions = velocity_handle_positions(notes, mapping, source_start_tick, source_end_tick);
    notes
        .iter()
        .filter(|note| (source_start_tick..source_end_tick).contains(&note.tick()))
        .min_by(|left, right| {
            let left_x = positions.get(&left.id()).copied().unwrap_or_default() + 2.0;
            let right_x = positions.get(&right.id()).copied().unwrap_or_default() + 2.0;
            (left_x - x).abs().total_cmp(&(right_x - x).abs())
        })
        .filter(|note| {
            (positions.get(&note.id()).copied().unwrap_or_default() + 2.0 - x).abs() <= 4.0
        })
        .map(|note| note.id())
}

fn velocity_handle_positions(
    notes: &[aaadaw_core::MidiNote],
    mapping: RollMapping,
    source_start_tick: u64,
    source_end_tick: u64,
) -> HashMap<NoteId, f32> {
    let mut ordered = notes
        .iter()
        .filter(|note| (source_start_tick..source_end_tick).contains(&note.tick()))
        .collect::<Vec<_>>();
    ordered.sort_by_key(|note| (note.tick(), note.pitch()));
    let mut positions = HashMap::with_capacity(notes.len());
    let mut group_start = 0;
    while group_start < ordered.len() {
        let tick = ordered[group_start].tick();
        let group_end = ordered[group_start..]
            .iter()
            .position(|note| note.tick() != tick)
            .map_or(ordered.len(), |offset| group_start + offset);
        let group_len = group_end - group_start;
        let onset_x = mapping.x_at_tick(tick) + 1.0;
        let leftmost_offset = -((group_len.saturating_sub(1)) as f32 * 5.0 / 2.0);
        let shift_from_item_edge = (1.0 - onset_x - leftmost_offset).max(0.0);
        for (index, note) in ordered[group_start..group_end].iter().enumerate() {
            let offset = leftmost_offset + index as f32 * 5.0 + shift_from_item_edge;
            positions.insert(note.id(), onset_x + offset);
        }
        group_start = group_end;
    }
    positions
}

fn velocity_delta(start_y: f32, current_y: f32) -> i16 {
    ((start_y - current_y) * 127.0 / VELOCITY_LANE_HEIGHT).round() as i16
}

fn apply_velocity_delta(velocity: u8, delta: i16) -> u8 {
    (i16::from(velocity) + delta).clamp(1, 127) as u8
}

fn selection_after_click<T: Copy + Eq + std::hash::Hash>(
    selected: &HashSet<T>,
    note_id: T,
    toggle: bool,
) -> HashSet<T> {
    if toggle {
        let mut next = selected.clone();
        if !next.remove(&note_id) {
            next.insert(note_id);
        }
        next
    } else if selected.contains(&note_id) {
        selected.clone()
    } else {
        HashSet::from([note_id])
    }
}

fn roll_button<'a>(label: &'a str, message: Message) -> iced::widget::Button<'a, Message> {
    button(label)
        .style(iced::widget::button::secondary)
        .on_press(message)
        .padding([SPACING_XS / 2.0, SPACING_XS])
}

fn pitch_name(pitch: u8) -> String {
    const PITCH_CLASSES: [&str; 12] = [
        "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
    ];
    format!(
        "{}{}",
        PITCH_CLASSES[usize::from(pitch % 12)],
        i16::from(pitch) / 12 - 1
    )
}

fn is_black_key(pitch: u8) -> bool {
    matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
}

fn piano_key_rect(pitch: u8, row_y: f32, row_height: f32) -> Rectangle {
    if is_black_key(pitch) {
        Rectangle {
            x: 0.0,
            y: row_y + row_height * 0.08,
            width: KEY_WIDTH * 0.62,
            height: row_height * 0.84,
        }
    } else {
        Rectangle {
            x: 0.0,
            y: row_y,
            width: KEY_WIDTH,
            height: row_height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piano_roll_for_test<'a>(
        project: &'a Project,
        item_id: ItemId,
        selected: &'a HashSet<NoteId>,
        tool: MidiEditorTool,
    ) -> PianoRoll<'a> {
        PianoRoll {
            project,
            item: project
                .midi_items()
                .iter()
                .find(|item| item.id() == item_id)
                .unwrap(),
            item_id,
            selected,
            origin_tick: 0,
            high_pitch: 60,
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),
            playhead_tick: None,
            region: RollRegion::Pitch,
            tool,
        }
    }

    #[test]
    fn context_menu_epoch_change_clears_canvas_local_menu() {
        let mut menu = Some((2, Point::new(16.0, 24.0)));
        let mut epoch = 4;

        sync_context_menu_epoch(&mut menu, &mut epoch, 5);

        assert!(menu.is_none());
        assert_eq!(epoch, 5);
    }
    use iced::widget::canvas::Program;

    #[test]
    fn piano_keyboard_geometry_places_narrow_black_keys_over_full_white_keys() {
        let white = piano_key_rect(60, 20.0, 18.0);
        let black = piano_key_rect(61, 38.0, 18.0);

        assert!(!is_black_key(60));
        assert!(is_black_key(61));
        assert_eq!(white.width, KEY_WIDTH);
        assert!(black.width < white.width);
        assert!(black.height < 18.0);
        assert!(black.y > 38.0);
        assert!(black.y + black.height < 56.0);
    }

    #[test]
    fn touch_hitboxes_reach_48dp_without_changing_desktop_hits() {
        let (project, item_id, _) = project_with_note(
            3_840,
            MidiNoteData {
                pitch: 60,
                tick: 480,
                duration: 120,
                velocity: 96,
            },
        );
        let selected = HashSet::new();
        let roll = piano_roll_for_test(&project, item_id, &selected, MidiEditorTool::Select);
        let point = Point::new(70.0, 24.0);

        assert!(roll.note_at_point(point, false).is_none());
        assert!(roll.note_at_point(point, true).is_some());
    }

    #[test]
    fn touch_draw_and_erase_tools_edit_notes_with_single_taps() {
        let (project, item_id, _) = project_with_note(
            3_840,
            MidiNoteData {
                pitch: 60,
                tick: 2_880,
                duration: 240,
                velocity: 96,
            },
        );
        let selected = HashSet::new();
        let draw_roll = piano_roll_for_test(&project, item_id, &selected, MidiEditorTool::Draw);
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(640.0, 600.0));
        let blank_note = Point::new(KEY_WIDTH + 48.0, HEADER_HEIGHT + NOTE_ROW_HEIGHT / 2.0);
        let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let draw = draw_roll
            .update(
                &mut Interaction::default(),
                &press,
                bounds,
                mouse::Cursor::Available(blank_note),
            )
            .expect("draw tap should add a note");
        assert!(matches!(
            draw.into_inner().0,
            Some(Message::AddMidiNoteAt(changed_item, data))
                if changed_item == item_id
                    && data.tick == 480
                    && data.pitch == 60
                    && data.duration == 240
        ));

        let (project, item_id, note_id) = project_with_note(
            3_840,
            MidiNoteData {
                pitch: 60,
                tick: 480,
                duration: 240,
                velocity: 96,
            },
        );
        let selected = HashSet::new();
        let erase_roll = piano_roll_for_test(&project, item_id, &selected, MidiEditorTool::Erase);
        let note = &project.midi_items()[0].notes()[0];
        let erase_note = Point::new(
            KEY_WIDTH + 48.0 + 12.0,
            HEADER_HEIGHT + NOTE_ROW_HEIGHT / 2.0,
        );
        let erase = erase_roll
            .update(
                &mut Interaction::default(),
                &press,
                bounds,
                mouse::Cursor::Available(erase_note),
            )
            .expect("erase tap should delete a note");
        assert!(matches!(
            erase.into_inner().0,
            Some(Message::DeleteMidiNotes(changed_item, note_ids))
                if changed_item == item_id && note_ids == vec![note_id]
        ));
        assert_eq!(note.id(), note_id);
    }

    #[test]
    fn piano_keys_audition_on_drag_across_pitches_and_release_outside_the_strip() {
        let (project, item_id, _) = project_with_note(
            960,
            MidiNoteData {
                pitch: 60,
                tick: 0,
                duration: 120,
                velocity: 96,
            },
        );
        let item = &project.midi_items()[0];
        let roll = PianoRoll {
            tool: MidiEditorTool::Select,
            project: &project,
            item,
            item_id,
            selected: &HashSet::new(),
            origin_tick: 0,
            high_pitch: 84,
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),
            playhead_tick: None,
            region: RollRegion::Pitch,
        };
        let bounds = Rectangle::new(
            Point::ORIGIN,
            Size::new(KEY_WIDTH + 300.0, HEADER_HEIGHT + 36.0 * NOTE_ROW_HEIGHT),
        );
        let first = Point::new(12.0, HEADER_HEIGHT + 9.0);
        let second = Point::new(12.0, HEADER_HEIGHT + NOTE_ROW_HEIGHT + 9.0);
        let outside = Point::new(KEY_WIDTH + 4.0, second.y);
        let mut state = Interaction::default();
        let action = roll
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(first),
            )
            .expect("key press starts an audition");
        let (message, _, _) = action.into_inner();
        assert!(matches!(message, Some(Message::PreviewMidiNote(_, 84))));

        let action = roll
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::CursorMoved { position: second }),
                bounds,
                mouse::Cursor::Available(second),
            )
            .expect("dragging to a new key changes the audition pitch");
        let (message, _, _) = action.into_inner();
        assert!(matches!(message, Some(Message::PreviewMidiNote(_, 83))));

        let action = roll
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::CursorMoved { position: outside }),
                bounds,
                mouse::Cursor::Available(outside),
            )
            .expect("leaving the key strip releases the held note");
        let (message, _, _) = action.into_inner();
        assert!(matches!(message, Some(Message::ReleaseMidiPreview)));
        assert_eq!(state.auditioning_pitch, None);
    }

    #[test]
    fn visible_caret_falls_back_to_the_clipboards_default_paste_target() {
        let mut app = App::default();
        app.midi_note_clipboard.default_paste_tick = Some(1_920);
        assert_eq!(visible_edit_cursor_tick(&app), 1_920);

        app.midi_editor_edit_cursor_tick = Some(960);
        assert_eq!(visible_edit_cursor_tick(&app), 960);

        app.midi_editor_edit_cursor_tick = Some(190);
        app.timeline.snap_grid = SnapGrid::EighthTriplet;
        assert_eq!(visible_edit_cursor_tick(&app), 320);

        app.timeline.snap_enabled = false;
        assert_eq!(visible_edit_cursor_tick(&app), 190);
    }

    #[test]
    fn visible_caret_tracks_continuous_paste_after_the_snap_grid_changes() {
        let mut app = App::default();
        let _ = app.update(Message::AddTrack);
        let _ = app.update(Message::AddMidiItem);
        let item_id = app.project.midi_items()[0].id();
        let _ = app.update(Message::OpenMidiEditor(item_id));
        let _ = app.update(Message::Timeline(TimelineEvent::ToggleSnap));
        app.midi_editor_edit_cursor_tick = Some(190);
        app.midi_note_clipboard.notes = vec![MidiNoteData {
            pitch: 60,
            tick: 0,
            duration: 120,
            velocity: 96,
        }];
        app.midi_note_clipboard.span_ticks = 120;

        let _ = app.update(Message::PasteMidiNotes(item_id));
        assert_eq!(app.project.midi_items()[0].notes()[0].tick(), 190);

        let _ = app.update(Message::Timeline(TimelineEvent::SetSnapGrid(
            SnapGrid::Sixteenth,
        )));
        let _ = app.update(Message::Timeline(TimelineEvent::ToggleSnap));
        assert_eq!(visible_edit_cursor_tick(&app), 480);

        let _ = app.update(Message::PasteMidiNotes(item_id));
        assert_eq!(app.project.midi_items()[0].notes()[1].tick(), 480);
    }

    #[test]
    fn piano_roll_wheel_maps_to_pitch_and_modifier_navigation() {
        let note = MidiNoteData {
            pitch: 60,
            tick: 480,
            duration: 240,
            velocity: 96,
        };
        let (project, item_id, _) = project_with_note(3_840, note);
        let item = &project.midi_items()[0];
        let selected = HashSet::new();
        let roll = PianoRoll {
            tool: MidiEditorTool::Select,
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
            project: &project,
            item,
            item_id,
            selected: &selected,
            origin_tick: 0,
            high_pitch: 84,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),
            region: RollRegion::Pitch,

            playhead_tick: None,
        };
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(640.0, 320.0));
        let cursor = mouse::Cursor::Available(Point::new(300.0, 120.0));
        let wheel = Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 },
        });
        let mut interaction = Interaction::default();

        let action = roll
            .update(&mut interaction, &wheel, bounds, cursor)
            .expect("vertical wheel should navigate pitches");
        assert!(matches!(
            action.into_inner().0,
            Some(Message::PianoRollPitchScroll(3))
        ));

        let _ = roll.update(
            &mut interaction,
            &Event::Keyboard(keyboard::Event::ModifiersChanged(
                keyboard::Modifiers::SHIFT,
            )),
            bounds,
            cursor,
        );
        let action = roll
            .update(&mut interaction, &wheel, bounds, cursor)
            .expect("Shift+wheel should pan in time");
        assert!(matches!(
            action.into_inner().0,
            Some(Message::PianoRollPanPixels(-40.0))
        ));
    }

    fn project_with_note(item_length_ticks: u64, note: MidiNoteData) -> (Project, ItemId, NoteId) {
        let mut project = Project::new();
        project
            .apply(aaadaw_core::DawAction::CreateTrack {
                index: 0,
                name: "MIDI".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        project
            .apply(aaadaw_core::DawAction::InsertMidiItem {
                track_id,
                start_tick: 0,
                length_ticks: item_length_ticks,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        project
            .apply(aaadaw_core::DawAction::AddMidiNotes {
                item_id,
                notes: vec![note],
            })
            .unwrap();
        let note_id = project.midi_items()[0].notes()[0].id();
        (project, item_id, note_id)
    }

    #[test]
    fn resizing_preview_keeps_the_note_start_and_marks_item_overflow() {
        let note = MidiNoteData {
            pitch: 60,
            tick: 720,
            duration: 240,
            velocity: 96,
        };
        let (_project, _, note_id) = project_with_note(960, note);
        let drag = NoteDrag {
            start: Point::ORIGIN,
            anchor_note_id: note_id,
            notes: vec![(note_id, note)],
            resize: true,
            delta_tick: 240,
            delta_pitch: 0,
            velocity: false,
            delta_velocity: 0,
            copy: false,
            moved: true,
            click_selection: None,
            original_selection: HashSet::from([note_id]),
        };
        let preview = note_drag_preview_data(&drag, note, 0, 960);
        assert_eq!(preview.tick, note.tick);
        assert_eq!(preview.duration, 480);
        assert!(preview.tick + preview.duration > 960);
    }

    #[test]
    fn short_notes_keep_a_move_area_beside_the_resize_handle() {
        let note = MidiNoteData {
            pitch: 60,
            tick: 0,
            duration: 24,
            velocity: 96,
        };
        let (project, item_id, _) = project_with_note(960, note);
        let roll = PianoRoll {
            tool: MidiEditorTool::Select,
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
            project: &project,
            item: &project.midi_items()[0],
            item_id,
            selected: &HashSet::new(),
            origin_tick: 0,
            high_pitch: 80,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),
            region: RollRegion::Pitch,

            playhead_tick: None,
        };
        let bounds = Rectangle::new(
            Point::ORIGIN,
            Size::new(
                300.0,
                HEADER_HEIGHT + f32::from(PITCH_COUNT) * NOTE_ROW_HEIGHT,
            ),
        );
        let y = HEADER_HEIGHT + f32::from(80 - note.pitch) * NOTE_ROW_HEIGHT + 9.0;
        assert_eq!(note_width_pixels(note.duration, 960, 96.0), 3.0);
        assert_eq!(
            roll.mouse_interaction(
                &Interaction::default(),
                bounds,
                mouse::Cursor::Available(Point::new(KEY_WIDTH + 0.75, y)),
            ),
            mouse::Interaction::Grab
        );
        assert_eq!(
            roll.mouse_interaction(
                &Interaction::default(),
                bounds,
                mouse::Cursor::Available(Point::new(KEY_WIDTH + 2.5, y)),
            ),
            mouse::Interaction::ResizingHorizontally
        );
    }

    #[test]
    fn note_resize_reports_an_invalid_release_and_edge_cursor_is_distinct() {
        let note = MidiNoteData {
            pitch: 60,
            tick: 720,
            duration: 240,
            velocity: 96,
        };
        let (project, item_id, _) = project_with_note(960, note);
        let item = &project.midi_items()[0];
        let selected = HashSet::new();
        let roll = PianoRoll {
            tool: MidiEditorTool::Select,
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
            project: &project,
            item,
            item_id,
            selected: &selected,
            origin_tick: 0,
            high_pitch: 80,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),
            region: RollRegion::Pitch,

            playhead_tick: None,
        };
        let bounds = Rectangle::new(
            Point::ORIGIN,
            Size::new(
                300.0,
                HEADER_HEIGHT + f32::from(PITCH_COUNT) * NOTE_ROW_HEIGHT,
            ),
        );
        let y = HEADER_HEIGHT + f32::from(80 - note.pitch) * NOTE_ROW_HEIGHT + 9.0;
        let body = Point::new(KEY_WIDTH + 80.0, y);
        let edge = Point::new(KEY_WIDTH + 95.0, y);
        assert_eq!(
            roll.mouse_interaction(
                &Interaction::default(),
                bounds,
                mouse::Cursor::Available(body)
            ),
            mouse::Interaction::Grab
        );
        assert_eq!(
            roll.mouse_interaction(
                &Interaction::default(),
                bounds,
                mouse::Cursor::Available(edge)
            ),
            mouse::Interaction::ResizingHorizontally
        );

        let mut interaction = Interaction::default();
        roll.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            mouse::Cursor::Available(edge),
        )
        .expect("note edge should start resize");
        let moved = Point::new(KEY_WIDTH + 120.0, y);
        roll.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::CursorMoved { position: moved }),
            bounds,
            mouse::Cursor::Available(moved),
        )
        .expect("resize should update its preview");
        let preview = note_drag_preview_data(
            interaction.drag.as_ref().unwrap(),
            note,
            0,
            item.length_ticks(),
        );
        assert!(preview.tick + preview.duration > item.length_ticks());
        let action = roll
            .update(
                &mut interaction,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(moved),
            )
            .expect("invalid resize should explain the rejection");
        assert!(matches!(
            action.into_inner().0,
            Some(Message::MidiEditorFeedback(message))
                if message.contains("beyond the MIDI item")
        ));
    }

    #[test]
    fn escape_cancels_active_note_controller_and_pitch_bend_gestures() {
        let note = MidiNoteData {
            pitch: 60,
            tick: 0,
            duration: 240,
            velocity: 96,
        };
        let (project, item_id, note_id) = project_with_note(960, note);
        let item = &project.midi_items()[0];
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(400.0, 100.0));
        let escape = Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            modified_key: keyboard::Key::Named(keyboard::key::Named::Escape),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Escape),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::NONE,
            text: None,
            repeat: false,
        });

        let mut roll_state = Interaction {
            drag: Some(NoteDrag {
                start: Point::ORIGIN,
                anchor_note_id: note_id,
                notes: vec![(note_id, note)],
                resize: false,
                delta_tick: 120,
                delta_pitch: 0,
                velocity: false,
                delta_velocity: 0,
                copy: false,
                moved: true,
                click_selection: None,
                original_selection: HashSet::from([note_id]),
            }),
            ..Interaction::default()
        };
        let roll = PianoRoll {
            tool: MidiEditorTool::Select,
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
            project: &project,
            item,
            item_id,
            selected: &HashSet::from([note_id]),
            origin_tick: 0,
            high_pitch: 80,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),
            region: RollRegion::Pitch,

            playhead_tick: None,
        };
        assert!(
            roll.update(
                &mut roll_state,
                &escape,
                bounds,
                mouse::Cursor::Available(Point::ORIGIN),
            )
            .is_some()
        );
        assert!(roll_state.drag.is_none());

        let lane = ControllerLane {
            item,
            item_id,
            controller: 1,
            lane_height: MODULATION_LANE_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),

            playhead_tick: None,
        };
        let mut controller_state = ControllerLaneInteraction {
            drag: Some(ControllerDrag {
                controllers: Vec::new(),
                index: None,
                original: None,
                current: MidiControllerData {
                    controller: 1,
                    tick: 0,
                    value: 64,
                },
            }),
            ..ControllerLaneInteraction::default()
        };
        assert!(
            lane.update(
                &mut controller_state,
                &escape,
                bounds,
                mouse::Cursor::Available(Point::ORIGIN),
            )
            .is_some()
        );
        assert!(controller_state.drag.is_none());

        let bend_lane = PitchBendLane {
            item,
            item_id,
            lane_height: PITCH_BEND_LANE_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),

            playhead_tick: None,
        };
        let mut bend_state = PitchBendLaneInteraction {
            drag: Some(PitchBendDrag {
                bends: Vec::new(),
                index: None,
                original: None,
                current: MidiPitchBendData {
                    tick: 0,
                    value: 8192,
                },
            }),
            ..PitchBendLaneInteraction::default()
        };
        assert!(
            bend_lane
                .update(
                    &mut bend_state,
                    &escape,
                    bounds,
                    mouse::Cursor::Available(Point::ORIGIN),
                )
                .is_some()
        );
        assert!(bend_state.drag.is_none());
    }

    #[test]
    fn controller_and_pitch_bend_duplicate_targets_explain_rejection() {
        let note = MidiNoteData {
            pitch: 60,
            tick: 0,
            duration: 240,
            velocity: 96,
        };
        let (mut project, item_id, _) = project_with_note(3_840, note);
        project
            .apply(aaadaw_core::DawAction::SetMidiControllers {
                item_id,
                controllers: vec![
                    MidiControllerData {
                        controller: 1,
                        tick: 0,
                        value: 64,
                    },
                    MidiControllerData {
                        controller: 1,
                        tick: 960,
                        value: 64,
                    },
                ],
            })
            .unwrap();
        project
            .apply(aaadaw_core::DawAction::SetMidiPitchBends {
                item_id,
                pitch_bends: vec![
                    MidiPitchBendData {
                        tick: 0,
                        value: 8192,
                    },
                    MidiPitchBendData {
                        tick: 960,
                        value: 8192,
                    },
                ],
            })
            .unwrap();
        let item = &project.midi_items()[0];
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(400.0, 72.0));
        let controller_lane = ControllerLane {
            item,
            item_id,
            controller: 1,
            lane_height: 72.0,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),

            playhead_tick: None,
        };
        let start = Point::new(0.0, controller_y(64, 72.0));
        let target = Point::new(96.0, start.y);
        let mut controller_state = ControllerLaneInteraction::default();
        controller_lane.update(
            &mut controller_state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            mouse::Cursor::Available(start),
        );
        controller_lane.update(
            &mut controller_state,
            &Event::Mouse(mouse::Event::CursorMoved { position: target }),
            bounds,
            mouse::Cursor::Available(target),
        );
        let action = controller_lane
            .update(
                &mut controller_state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(target),
            )
            .expect("duplicate controller target should report why it is rejected");
        assert!(matches!(
            action.into_inner().0,
            Some(Message::MidiEditorFeedback(message)) if message.contains("same tick")
        ));

        let bend_lane = PitchBendLane {
            item,
            item_id,
            lane_height: 72.0,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),

            playhead_tick: None,
        };
        let start = Point::new(0.0, pitch_bend_y(8192, 72.0));
        let target = Point::new(96.0, start.y);
        let mut bend_state = PitchBendLaneInteraction::default();
        bend_lane.update(
            &mut bend_state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            mouse::Cursor::Available(start),
        );
        bend_lane.update(
            &mut bend_state,
            &Event::Mouse(mouse::Event::CursorMoved { position: target }),
            bounds,
            mouse::Cursor::Available(target),
        );
        let action = bend_lane
            .update(
                &mut bend_state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(target),
            )
            .expect("duplicate pitch-bend target should report why it is rejected");
        assert!(matches!(
            action.into_inner().0,
            Some(Message::MidiEditorFeedback(message)) if message.contains("same tick")
        ));
    }

    #[test]
    fn delete_key_targets_selected_pitch_notes_and_has_no_arrangement_fallback() {
        let mut project = Project::new();
        project
            .apply(aaadaw_core::DawAction::CreateTrack {
                index: 0,
                name: "MIDI".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        project
            .apply(aaadaw_core::DawAction::InsertMidiItem {
                track_id,
                start_tick: 0,
                length_ticks: 960,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        project
            .apply(aaadaw_core::DawAction::AddMidiNotes {
                item_id,
                notes: vec![MidiNoteData {
                    pitch: 60,
                    tick: 0,
                    duration: 240,
                    velocity: 96,
                }],
            })
            .unwrap();
        let selected = [project.midi_items()[0].notes()[0].id()]
            .into_iter()
            .collect();

        assert!(matches!(
            delete_key_message(RollRegion::Pitch, item_id, &selected),
            Some(Message::DeleteMidiNotes(target, note_ids))
                if target == item_id && note_ids == selected.iter().copied().collect::<Vec<_>>()
        ));
        assert!(delete_key_message(RollRegion::Velocity, item_id, &selected).is_none());
        assert!(delete_key_message(RollRegion::Pitch, item_id, &HashSet::new()).is_none());
    }

    #[test]
    fn controller_lane_selection_switches_the_visible_edit_lane() {
        let mut app = App::default();
        assert_eq!(app.midi_editor_lane, MidiEditorLane::Velocity);
        let _ = app.update(Message::SelectMidiEditorLane(MidiEditorLane::Volume));
        assert_eq!(app.midi_editor_lane, MidiEditorLane::Volume);
        let _ = app.update(Message::SelectMidiEditorLane(MidiEditorLane::Pan));
        assert_eq!(app.midi_editor_lane, MidiEditorLane::Pan);
        let _ = app.update(Message::SelectMidiEditorLane(MidiEditorLane::Expression));
        assert_eq!(app.midi_editor_lane, MidiEditorLane::Expression);
        let _ = app.update(Message::SelectMidiEditorLane(MidiEditorLane::PitchBend));
        assert_eq!(app.midi_editor_lane, MidiEditorLane::PitchBend);
    }

    #[test]
    fn pitch_bend_lane_adds_moves_and_deletes_points_across_the_signed_range() {
        assert_eq!(pitch_bend_value_at_y(10.0, 72.0), 16_383);
        assert_eq!(pitch_bend_value_at_y(36.0, 72.0), 8192);
        assert_eq!(pitch_bend_value_at_y(62.0, 72.0), 0);
        let mut project = Project::new();
        project
            .apply(aaadaw_core::DawAction::CreateTrack {
                index: 0,
                name: "Track".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        project
            .apply(aaadaw_core::DawAction::InsertMidiItem {
                track_id,
                start_tick: 0,
                length_ticks: 3_840,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(400.0, PITCH_BEND_LANE_HEIGHT));
        let mut interaction = PitchBendLaneInteraction::default();
        let lane = PitchBendLane {
            item: &project.midi_items()[0],
            item_id,
            lane_height: PITCH_BEND_LANE_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),

            playhead_tick: None,
        };
        let center = mouse::Cursor::Available(Point::new(96.0, 36.0));
        lane.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            center,
        )
        .expect("click should begin inserting a pitch bend");
        let action = lane
            .update(
                &mut interaction,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                center,
            )
            .expect("release should submit a neutral pitch bend");
        let (message, _, _) = action.into_inner();
        let Message::SetMidiPitchBends(changed_item, bends) = message.unwrap() else {
            panic!("pitch-bend lane should submit pitch-bend edits");
        };
        assert_eq!(changed_item, item_id);
        assert_eq!(
            bends,
            [MidiPitchBendData {
                tick: 960,
                value: 8192
            }]
        );
        project
            .apply(aaadaw_core::DawAction::SetMidiPitchBends {
                item_id,
                pitch_bends: bends,
            })
            .unwrap();

        let lane = PitchBendLane {
            item: &project.midi_items()[0],
            item_id,
            lane_height: PITCH_BEND_LANE_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),

            playhead_tick: None,
        };
        lane.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            center,
        )
        .expect("existing pitch bend should start dragging");
        lane.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(144.0, 10.0),
            }),
            bounds,
            mouse::Cursor::Available(Point::new(144.0, 10.0)),
        )
        .expect("drag should preview the full positive bend");
        let action = lane
            .update(
                &mut interaction,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(Point::new(144.0, 10.0)),
            )
            .expect("release should submit the moved bend");
        let (message, _, _) = action.into_inner();
        let Message::SetMidiPitchBends(_, bends) = message.unwrap() else {
            panic!("pitch-bend drag should submit a pitch-bend edit");
        };
        assert_eq!(
            bends,
            [MidiPitchBendData {
                tick: 1_440,
                value: 16_383,
            }]
        );
        project
            .apply(aaadaw_core::DawAction::SetMidiPitchBends {
                item_id,
                pitch_bends: bends,
            })
            .unwrap();
        let lane = PitchBendLane {
            item: &project.midi_items()[0],
            item_id,
            lane_height: PITCH_BEND_LANE_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),

            playhead_tick: None,
        };
        lane.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
            bounds,
            mouse::Cursor::Available(Point::new(144.0, 10.0)),
        )
        .expect("right-click should open the bend context menu");
        let action = lane
            .update(
                &mut interaction,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(Point::new(160.0, 20.0)),
            )
            .expect("Delete pitch bend should remove the point");
        let (message, _, _) = action.into_inner();
        let Message::SetMidiPitchBends(_, bends) = message.unwrap() else {
            panic!("pitch-bend deletion should submit an empty lane");
        };
        assert!(bends.is_empty());
    }

    #[test]
    fn time_and_pitch_mapping_respect_origin_and_visible_pitch_range() {
        let mapping = RollMapping {
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
            origin_tick: 1_920,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),
            high_pitch: 84,
        };
        assert_eq!(mapping.tick_at_x(48.0), 2_400);
        assert_eq!(mapping.x_at_tick(2_400), 48.0);
        assert_eq!(mapping.pitch_at_y(0.0), 84);
        assert_eq!(mapping.pitch_at_y(NOTE_ROW_HEIGHT * 12.0), 72);
        assert_eq!(mapping.y_at_pitch(72), NOTE_ROW_HEIGHT * 12.0);
        assert!(mapping.x_at_tick(1_680) < 0.0);
    }

    #[test]
    fn insertion_grid_rounds_to_nearest_sixteenth() {
        assert_eq!(snap_tick(100, Some(240), true, false), 0);
        assert_eq!(snap_tick(140, Some(240), true, false), 240);
        assert_eq!(snap_tick(500, Some(240), true, false), 480);
        assert_eq!(
            snap_tick(u64::MAX, Some(240), true, false),
            u64::MAX / 240 * 240
        );
        assert_eq!(snap_tick(140, Some(240), false, false), 140);
        assert_eq!(snap_tick(140, Some(240), true, true), 140);
    }

    #[test]
    fn mapping_and_sixteenth_grid_follow_project_ppq() {
        let mapping = RollMapping {
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 480,
            snap: MidiSnap::default(),
            high_pitch: 60,
        };
        assert_eq!(mapping.tick_at_x(24.0), 120);
        assert_eq!(mapping.x_at_tick(120), 24.0);
        assert_eq!(mapping.grid_ticks(), 120);
        assert_eq!(mapping.snap_tick(70, false), 120);
    }

    #[test]
    fn selected_grid_and_shift_bypass_are_shared_by_midi_editors() {
        let mapping = RollMapping {
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            high_pitch: 60,
            snap: MidiSnap {
                grid: SnapGrid::EighthTriplet,
                enabled: true,
                cursor_tick: 0,
                ..MidiSnap::default()
            },
        };
        assert_eq!(mapping.grid_ticks(), 320);
        assert_eq!(mapping.snap_tick(190, false), 320);
        assert_eq!(mapping.snap_tick(190, true), 190);
        assert_eq!(snap_delta(190, mapping, false), 320);
        assert_eq!(snap_delta(190, mapping, true), 190);

        let disabled = RollMapping {
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
            snap: MidiSnap {
                enabled: false,
                ..mapping.snap
            },
            ..mapping
        };
        assert_eq!(disabled.snap_tick(190, false), 190);
        assert_eq!(snap_delta(190, disabled, false), 190);
    }

    #[test]
    fn clicking_the_piano_roll_ruler_sets_the_snapped_edit_cursor() {
        let note = MidiNoteData {
            pitch: 60,
            tick: 0,
            duration: 240,
            velocity: 96,
        };
        let (project, item_id, _) = project_with_note(3_840, note);
        let roll = PianoRoll {
            tool: MidiEditorTool::Select,
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
            project: &project,
            item: &project.midi_items()[0],
            item_id,
            selected: &HashSet::new(),
            origin_tick: 0,
            high_pitch: 60,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap {
                grid: SnapGrid::EighthTriplet,
                enabled: true,
                cursor_tick: 0,
                ..MidiSnap::default()
            },
            region: RollRegion::Pitch,

            playhead_tick: None,
        };
        let bounds = Rectangle::new(
            Point::ORIGIN,
            Size::new(
                400.0,
                HEADER_HEIGHT + f32::from(PITCH_COUNT) * NOTE_ROW_HEIGHT,
            ),
        );
        let action = roll
            .update(
                &mut Interaction::default(),
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(Point::new(KEY_WIDTH + 19.0, 8.0)),
            )
            .expect("ruler click should set the edit cursor");
        assert!(matches!(
            action.into_inner().0,
            Some(Message::SetPianoRollCursor(changed_item, 320)) if changed_item == item_id
        ));
    }

    #[test]
    fn piano_roll_blank_click_positions_cursor_and_double_click_inserts_note() {
        let note = MidiNoteData {
            pitch: 60,
            tick: 0,
            duration: 240,
            velocity: 96,
        };
        let (project, item_id, _) = project_with_note(3_840, note);
        let roll = PianoRoll {
            tool: MidiEditorTool::Select,
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
            project: &project,
            item: &project.midi_items()[0],
            item_id,
            selected: &HashSet::new(),
            origin_tick: 0,
            high_pitch: 60,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap {
                enabled: false,
                ..MidiSnap::default()
            },
            region: RollRegion::Pitch,
            playhead_tick: None,
        };
        let bounds = Rectangle::new(
            Point::ORIGIN,
            Size::new(
                400.0,
                HEADER_HEIGHT + f32::from(PITCH_COUNT) * NOTE_ROW_HEIGHT,
            ),
        );
        let point = Point::new(KEY_WIDTH + 48.0, HEADER_HEIGHT + NOTE_ROW_HEIGHT / 2.0);
        let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        let mut interaction = Interaction::default();
        assert_eq!(
            roll.update(
                &mut interaction,
                &press,
                bounds,
                mouse::Cursor::Available(point),
            )
            .unwrap()
            .into_inner()
            .2,
            iced::event::Status::Captured
        );
        let action = roll
            .update(
                &mut interaction,
                &release,
                bounds,
                mouse::Cursor::Available(point),
            )
            .expect("blank click release should position the edit cursor");
        assert!(matches!(
            action.into_inner().0,
            Some(Message::SetPianoRollCursor(changed_item, 480)) if changed_item == item_id
        ));

        interaction.last_empty_click = Some((
            Instant::now() - Duration::from_millis(100),
            roll.roll_point(point),
        ));
        let action = roll
            .update(
                &mut interaction,
                &press,
                bounds,
                mouse::Cursor::Available(point),
            )
            .expect("second blank click should insert a note");
        assert!(matches!(
            action.into_inner().0,
            Some(Message::AddMidiNoteAt(changed_item, data))
                if changed_item == item_id && data.tick == 480 && data.pitch == 60
        ));
    }

    #[test]
    fn piano_roll_blank_drag_marquees_notes_and_escape_cancels() {
        let note = MidiNoteData {
            pitch: 60,
            tick: 480,
            duration: 240,
            velocity: 96,
        };
        let (project, item_id, note_id) = project_with_note(3_840, note);
        let selected = HashSet::new();
        let roll = PianoRoll {
            tool: MidiEditorTool::Select,
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
            project: &project,
            item: &project.midi_items()[0],
            item_id,
            selected: &selected,
            origin_tick: 0,
            high_pitch: 60,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),
            region: RollRegion::Pitch,
            playhead_tick: None,
        };
        let bounds = Rectangle::new(
            Point::ORIGIN,
            Size::new(
                400.0,
                HEADER_HEIGHT + f32::from(PITCH_COUNT) * NOTE_ROW_HEIGHT,
            ),
        );
        let start = Point::new(KEY_WIDTH + 10.0, HEADER_HEIGHT + NOTE_ROW_HEIGHT / 2.0);
        let end = Point::new(KEY_WIDTH + 90.0, HEADER_HEIGHT + NOTE_ROW_HEIGHT / 2.0);
        let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        let moved = Event::Mouse(mouse::Event::CursorMoved { position: end });
        let mut interaction = Interaction::default();
        roll.update(
            &mut interaction,
            &press,
            bounds,
            mouse::Cursor::Available(start),
        )
        .expect("empty-space press should begin a potential marquee");
        roll.update(
            &mut interaction,
            &moved,
            bounds,
            mouse::Cursor::Available(end),
        )
        .expect("marquee drag should show its preview");
        assert!(interaction.empty_drag.unwrap().marquee);
        let action = roll
            .update(
                &mut interaction,
                &release,
                bounds,
                mouse::Cursor::Available(end),
            )
            .expect("marquee release should select notes");
        assert!(matches!(
            action.into_inner().0,
            Some(Message::SelectMidiNotes(notes)) if notes == HashSet::from([note_id])
        ));

        roll.update(
            &mut interaction,
            &press,
            bounds,
            mouse::Cursor::Available(start),
        )
        .expect("a new blank press should begin another gesture");
        let escape = Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            modified_key: keyboard::Key::Named(keyboard::key::Named::Escape),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Escape),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::NONE,
            text: None,
            repeat: false,
        });
        assert!(
            roll.update(
                &mut interaction,
                &escape,
                bounds,
                mouse::Cursor::Available(start),
            )
            .is_some()
        );
        assert!(interaction.empty_drag.is_none());
    }

    #[test]
    fn command_drag_on_selected_note_previews_and_emits_a_copy() {
        let note = MidiNoteData {
            pitch: 60,
            tick: 480,
            duration: 240,
            velocity: 96,
        };
        let (project, item_id, note_id) = project_with_note(3_840, note);
        let selected = HashSet::from([note_id]);
        let roll = PianoRoll {
            tool: MidiEditorTool::Select,
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
            project: &project,
            item: &project.midi_items()[0],
            item_id,
            selected: &selected,
            origin_tick: 0,
            high_pitch: 60,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),
            region: RollRegion::Pitch,
            playhead_tick: None,
        };
        let bounds = Rectangle::new(
            Point::ORIGIN,
            Size::new(
                400.0,
                HEADER_HEIGHT + f32::from(PITCH_COUNT) * NOTE_ROW_HEIGHT,
            ),
        );
        let start = Point::new(KEY_WIDTH + 50.0, HEADER_HEIGHT + NOTE_ROW_HEIGHT / 2.0);
        let end = Point::new(start.x + 24.0, start.y);
        let mut click_interaction = Interaction::default();
        roll.update(
            &mut click_interaction,
            &Event::Keyboard(keyboard::Event::ModifiersChanged(
                keyboard::Modifiers::COMMAND,
            )),
            bounds,
            mouse::Cursor::Available(start),
        );
        roll.update(
            &mut click_interaction,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            mouse::Cursor::Available(start),
        )
        .expect("Command-click should begin a toggle gesture");
        let click = roll
            .update(
                &mut click_interaction,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(start),
            )
            .expect("Command-click release should toggle selection");
        assert!(matches!(
            click.into_inner().0,
            Some(Message::SelectMidiNotes(notes)) if notes.is_empty()
        ));

        let mut interaction = Interaction::default();
        roll.update(
            &mut interaction,
            &Event::Keyboard(keyboard::Event::ModifiersChanged(
                keyboard::Modifiers::COMMAND,
            )),
            bounds,
            mouse::Cursor::Available(start),
        );
        roll.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            mouse::Cursor::Available(start),
        )
        .expect("Command-drag should begin a note copy gesture");
        assert!(interaction.drag.as_ref().unwrap().copy);
        roll.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::CursorMoved { position: end }),
            bounds,
            mouse::Cursor::Available(end),
        )
        .expect("copy drag should update its preview");
        let action = roll
            .update(
                &mut interaction,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(end),
            )
            .expect("copy drag release should submit a copy");
        assert!(matches!(
            action.into_inner().0,
            Some(Message::CopyDragMidiNotes(changed_item, notes))
                if changed_item == item_id && notes.len() == 1 && notes[0].tick == 720
        ));
    }

    #[test]
    fn shift_changes_recompute_active_note_and_controller_drag_previews() {
        let note = MidiNoteData {
            pitch: 60,
            tick: 0,
            duration: 480,
            velocity: 96,
        };
        let (project, item_id, note_id) = project_with_note(3_840, note);
        let item = &project.midi_items()[0];
        let selected = HashSet::from([note_id]);
        let snap = MidiSnap {
            grid: SnapGrid::EighthTriplet,
            enabled: true,
            cursor_tick: 0,
            ..MidiSnap::default()
        };
        let modifiers_changed = Event::Keyboard(keyboard::Event::ModifiersChanged(
            keyboard::Modifiers::SHIFT,
        ));

        let roll = PianoRoll {
            tool: MidiEditorTool::Select,
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
            project: &project,
            item,
            item_id,
            selected: &selected,
            origin_tick: 0,
            high_pitch: 60,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap,
            region: RollRegion::Pitch,

            playhead_tick: None,
        };
        let roll_bounds = Rectangle::new(
            Point::ORIGIN,
            Size::new(
                400.0,
                HEADER_HEIGHT + f32::from(PITCH_COUNT) * NOTE_ROW_HEIGHT,
            ),
        );
        let note_start = Point::new(KEY_WIDTH + 5.0, HEADER_HEIGHT + NOTE_ROW_HEIGHT / 2.0);
        let note_target = Point::new(KEY_WIDTH + 24.0, note_start.y);
        let mut roll_state = Interaction::default();
        roll.update(
            &mut roll_state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            roll_bounds,
            mouse::Cursor::Available(note_start),
        )
        .expect("note press should start a drag");
        roll.update(
            &mut roll_state,
            &modifiers_changed,
            roll_bounds,
            mouse::Cursor::Available(note_target),
        )
        .expect("changing Shift should refresh the note preview");
        assert_eq!(roll_state.drag.as_ref().unwrap().delta_tick, 190);
        let action = roll
            .update(
                &mut roll_state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                roll_bounds,
                mouse::Cursor::Available(note_target),
            )
            .expect("release should submit the Shift-bypassed note move");
        let Message::EditMidiNotes(_, edits) = action.into_inner().0.unwrap() else {
            panic!("note drag should submit note edits");
        };
        assert_eq!(edits[0].1.tick, 190);

        let lane_bounds = Rectangle::new(Point::ORIGIN, Size::new(400.0, VOLUME_LANE_HEIGHT));
        let controller = ControllerLane {
            item,
            item_id,
            controller: 7,
            lane_height: VOLUME_LANE_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap,

            playhead_tick: None,
        };
        let controller_start = Point::new(0.0, 36.0);
        let controller_target = Point::new(19.0, 36.0);
        let mut controller_state = ControllerLaneInteraction::default();
        controller
            .update(
                &mut controller_state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                lane_bounds,
                mouse::Cursor::Available(controller_start),
            )
            .expect("CC press should start a drag");
        controller
            .update(
                &mut controller_state,
                &modifiers_changed,
                lane_bounds,
                mouse::Cursor::Available(controller_target),
            )
            .expect("changing Shift should refresh the CC preview");
        assert_eq!(controller_state.drag.as_ref().unwrap().current.tick, 190);
        let action = controller
            .update(
                &mut controller_state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                lane_bounds,
                mouse::Cursor::Available(controller_target),
            )
            .expect("release should submit the Shift-bypassed CC edit");
        let Message::SetMidiControllers(_, points) = action.into_inner().0.unwrap() else {
            panic!("CC drag should submit controller edits");
        };
        assert_eq!(points[0].tick, 190);

        let pitch_bend = PitchBendLane {
            item,
            item_id,
            lane_height: PITCH_BEND_LANE_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap,

            playhead_tick: None,
        };
        let bend_bounds = Rectangle::new(Point::ORIGIN, Size::new(400.0, PITCH_BEND_LANE_HEIGHT));
        let mut bend_state = PitchBendLaneInteraction::default();
        pitch_bend
            .update(
                &mut bend_state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bend_bounds,
                mouse::Cursor::Available(controller_start),
            )
            .expect("pitch-bend press should start a drag");
        pitch_bend
            .update(
                &mut bend_state,
                &modifiers_changed,
                bend_bounds,
                mouse::Cursor::Available(controller_target),
            )
            .expect("changing Shift should refresh the pitch-bend preview");
        assert_eq!(bend_state.drag.as_ref().unwrap().current.tick, 190);
        let action = pitch_bend
            .update(
                &mut bend_state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bend_bounds,
                mouse::Cursor::Available(controller_target),
            )
            .expect("release should submit the Shift-bypassed pitch-bend edit");
        let Message::SetMidiPitchBends(_, points) = action.into_inner().0.unwrap() else {
            panic!("pitch-bend drag should submit pitch-bend edits");
        };
        assert_eq!(points[0].tick, 190);
    }

    #[test]
    fn sustain_lane_click_drag_and_right_click_emit_single_controller_edits() {
        let mut project = Project::new();
        project
            .apply(aaadaw_core::DawAction::CreateTrack {
                index: 0,
                name: "Track".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        project
            .apply(aaadaw_core::DawAction::InsertMidiItem {
                track_id,
                start_tick: 0,
                length_ticks: 3_840,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(400.0, SUSTAIN_LANE_HEIGHT));
        let lane = ControllerLane {
            item: &project.midi_items()[0],
            item_id,
            controller: 64,
            lane_height: SUSTAIN_LANE_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),

            playhead_tick: None,
        };
        let mut interaction = ControllerLaneInteraction::default();
        let pressed = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let released = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        let cursor = mouse::Cursor::Available(Point::new(96.0, 10.0));
        let action = lane
            .update(&mut interaction, &pressed, bounds, cursor)
            .expect("click should begin an edit gesture");
        assert_eq!(action.into_inner().2, iced::event::Status::Captured);
        let action = lane
            .update(&mut interaction, &released, bounds, cursor)
            .expect("click release should commit an edit");
        let (message, _, _) = action.into_inner();
        let Message::SetMidiControllers(changed_item, controllers) = message.unwrap() else {
            panic!("sustain lane should publish one controller replacement");
        };
        assert_eq!(changed_item, item_id);
        assert_eq!(
            controllers,
            vec![MidiControllerData {
                controller: 64,
                tick: 960,
                value: 127,
            }]
        );
        project
            .apply(aaadaw_core::DawAction::SetMidiControllers {
                item_id,
                controllers,
            })
            .unwrap();

        let lane = ControllerLane {
            item: &project.midi_items()[0],
            item_id,
            controller: 64,
            lane_height: SUSTAIN_LANE_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),

            playhead_tick: None,
        };
        let mut interaction = ControllerLaneInteraction::default();
        lane.update(
            &mut interaction,
            &pressed,
            bounds,
            mouse::Cursor::Available(Point::new(96.0, 10.0)),
        )
        .expect("existing point should start a drag");
        lane.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(192.0, 80.0),
            }),
            bounds,
            mouse::Cursor::Available(Point::new(192.0, 80.0)),
        )
        .expect("drag should update the preview");
        let action = lane
            .update(
                &mut interaction,
                &released,
                bounds,
                mouse::Cursor::Available(Point::new(192.0, 80.0)),
            )
            .expect("drag release should commit one edit");
        let (message, _, _) = action.into_inner();
        let Message::SetMidiControllers(_, controllers) = message.unwrap() else {
            panic!("drag should publish a controller replacement");
        };
        assert_eq!(
            controllers,
            vec![MidiControllerData {
                controller: 64,
                tick: 1_920,
                value: 0,
            }]
        );
        project
            .apply(aaadaw_core::DawAction::SetMidiControllers {
                item_id,
                controllers,
            })
            .unwrap();

        let lane = ControllerLane {
            item: &project.midi_items()[0],
            item_id,
            controller: 64,
            lane_height: SUSTAIN_LANE_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),

            playhead_tick: None,
        };
        let mut interaction = ControllerLaneInteraction::default();
        let action = lane
            .update(
                &mut interaction,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
                bounds,
                mouse::Cursor::Available(Point::new(192.0, 80.0)),
            )
            .expect("right-clicking a point should open its context menu");
        assert!(action.into_inner().0.is_none());
        let action = lane
            .update(
                &mut interaction,
                &pressed,
                bounds,
                mouse::Cursor::Available(Point::new(200.0, 80.0)),
            )
            .expect("choosing Delete CC64 point should remove the point");
        let (message, _, _) = action.into_inner();
        let Message::SetMidiControllers(deleted_item, controllers) = message.unwrap() else {
            panic!("context menu should publish a controller replacement");
        };
        assert_eq!(deleted_item, item_id);
        assert!(controllers.is_empty());
    }

    #[test]
    fn modulation_lane_uses_continuous_values_for_cc1() {
        let mut project = Project::new();
        project
            .apply(aaadaw_core::DawAction::CreateTrack {
                index: 0,
                name: "Track".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        project
            .apply(aaadaw_core::DawAction::InsertMidiItem {
                track_id,
                start_tick: 0,
                length_ticks: 3_840,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        let lane = ControllerLane {
            item: &project.midi_items()[0],
            item_id,
            controller: 1,
            lane_height: MODULATION_LANE_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),

            playhead_tick: None,
        };
        let point = lane.point_data(Point::new(96.0, 36.0), false);
        assert_eq!(
            point,
            MidiControllerData {
                controller: 1,
                tick: 960,
                value: 64,
            }
        );
        assert_eq!(value_at_y(10.0, MODULATION_LANE_HEIGHT), 127);
        assert_eq!(value_at_y(62.0, MODULATION_LANE_HEIGHT), 0);

        let bounds = Rectangle::new(Point::ORIGIN, Size::new(400.0, MODULATION_LANE_HEIGHT));
        let mut interaction = ControllerLaneInteraction::default();
        let cursor = mouse::Cursor::Available(Point::new(96.0, 36.0));
        lane.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        )
        .expect("click should begin a CC1 edit");
        let action = lane
            .update(
                &mut interaction,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .expect("click release should submit the CC1 point");
        let (message, _, _) = action.into_inner();
        let Message::SetMidiControllers(changed_item, controllers) = message.unwrap() else {
            panic!("modulation lane should submit a controller replacement");
        };
        assert_eq!(changed_item, item_id);
        assert_eq!(controllers, vec![point]);
        project
            .apply(aaadaw_core::DawAction::SetMidiControllers {
                item_id,
                controllers,
            })
            .unwrap();

        let lane = ControllerLane {
            item: &project.midi_items()[0],
            item_id,
            controller: 1,
            lane_height: MODULATION_LANE_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),

            playhead_tick: None,
        };
        let mut interaction = ControllerLaneInteraction::default();
        lane.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        )
        .expect("existing CC1 point should start a drag");
        lane.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(192.0, 20.0),
            }),
            bounds,
            mouse::Cursor::Available(Point::new(192.0, 20.0)),
        )
        .expect("drag should update the CC1 preview");
        let action = lane
            .update(
                &mut interaction,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(Point::new(192.0, 20.0)),
            )
            .expect("drag release should submit the CC1 point");
        let (message, _, _) = action.into_inner();
        let Message::SetMidiControllers(_, controllers) = message.unwrap() else {
            panic!("modulation drag should submit a controller replacement");
        };
        assert_eq!(
            controllers,
            vec![MidiControllerData {
                controller: 1,
                tick: 1_920,
                value: 103,
            }]
        );
        project
            .apply(aaadaw_core::DawAction::SetMidiControllers {
                item_id,
                controllers,
            })
            .unwrap();

        let lane = ControllerLane {
            item: &project.midi_items()[0],
            item_id,
            controller: 1,
            lane_height: MODULATION_LANE_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),

            playhead_tick: None,
        };
        let mut interaction = ControllerLaneInteraction::default();
        lane.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
            bounds,
            mouse::Cursor::Available(Point::new(192.0, 20.0)),
        )
        .expect("right-click should open CC1 context menu");
        let action = lane
            .update(
                &mut interaction,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(Point::new(200.0, 28.0)),
            )
            .expect("Delete CC1 point should remove the point");
        let (message, _, _) = action.into_inner();
        let Message::SetMidiControllers(_, controllers) = message.unwrap() else {
            panic!("modulation deletion should submit a controller replacement");
        };
        assert!(controllers.is_empty());
    }

    #[test]
    fn expression_lane_edits_cc11_without_changing_other_controllers() {
        let mut project = Project::new();
        project
            .apply(aaadaw_core::DawAction::CreateTrack {
                index: 0,
                name: "Track".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        project
            .apply(aaadaw_core::DawAction::InsertMidiItem {
                track_id,
                start_tick: 0,
                length_ticks: 3_840,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        project
            .apply(aaadaw_core::DawAction::AddMidiNotes {
                item_id,
                notes: vec![MidiNoteData {
                    pitch: 60,
                    tick: 120,
                    duration: 240,
                    velocity: 96,
                }],
            })
            .unwrap();
        let existing_controllers = vec![
            MidiControllerData {
                controller: 1,
                tick: 0,
                value: 50,
            },
            MidiControllerData {
                controller: 64,
                tick: 0,
                value: 127,
            },
        ];
        project
            .apply(aaadaw_core::DawAction::SetMidiControllers {
                item_id,
                controllers: existing_controllers.clone(),
            })
            .unwrap();
        let lane = ControllerLane {
            item: &project.midi_items()[0],
            item_id,
            controller: 11,
            lane_height: EXPRESSION_LANE_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),

            playhead_tick: None,
        };
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(400.0, EXPRESSION_LANE_HEIGHT));
        let mut interaction = ControllerLaneInteraction::default();
        let cursor = mouse::Cursor::Available(Point::new(96.0, 36.0));
        lane.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        )
        .expect("click should begin a CC11 edit");
        let action = lane
            .update(
                &mut interaction,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .expect("click release should submit the CC11 point");
        let (message, _, _) = action.into_inner();
        let Message::SetMidiControllers(changed_item, controllers) = message.unwrap() else {
            panic!("Expression lane should submit a controller replacement");
        };
        assert_eq!(changed_item, item_id);
        assert_eq!(
            controllers,
            [
                existing_controllers.clone(),
                vec![MidiControllerData {
                    controller: 11,
                    tick: 960,
                    value: 64,
                }],
            ]
            .concat()
        );
        project
            .apply(aaadaw_core::DawAction::SetMidiControllers {
                item_id,
                controllers,
            })
            .unwrap();
        assert_eq!(project.midi_items()[0].notes().len(), 1);
        assert_eq!(
            project.midi_items()[0].controllers(),
            [
                existing_controllers,
                vec![MidiControllerData {
                    controller: 11,
                    tick: 960,
                    value: 64,
                }],
            ]
            .concat()
        );
    }

    #[test]
    fn volume_lane_edits_cc7_and_preserves_other_controllers() {
        let mut project = Project::new();
        project
            .apply(aaadaw_core::DawAction::CreateTrack {
                index: 0,
                name: "Track".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        project
            .apply(aaadaw_core::DawAction::InsertMidiItem {
                track_id,
                start_tick: 0,
                length_ticks: 3_840,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        let existing = vec![MidiControllerData {
            controller: 11,
            tick: 0,
            value: 100,
        }];
        project
            .apply(aaadaw_core::DawAction::SetMidiControllers {
                item_id,
                controllers: existing.clone(),
            })
            .unwrap();
        let lane = ControllerLane {
            item: &project.midi_items()[0],
            item_id,
            controller: 7,
            lane_height: VOLUME_LANE_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),

            playhead_tick: None,
        };
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(400.0, VOLUME_LANE_HEIGHT));
        let cursor = mouse::Cursor::Available(Point::new(96.0, 36.0));
        let mut interaction = ControllerLaneInteraction::default();
        lane.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        )
        .expect("click should begin a CC7 edit");
        let action = lane
            .update(
                &mut interaction,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .expect("click release should submit the CC7 point");
        let (message, _, _) = action.into_inner();
        let Message::SetMidiControllers(changed_item, controllers) = message.unwrap() else {
            panic!("Volume lane should submit a controller replacement");
        };
        assert_eq!(changed_item, item_id);
        let expected = [
            existing,
            vec![MidiControllerData {
                controller: 7,
                tick: 960,
                value: 64,
            }],
        ]
        .concat();
        assert_eq!(controllers, expected);
        project
            .apply(aaadaw_core::DawAction::SetMidiControllers {
                item_id,
                controllers: controllers.clone(),
            })
            .unwrap();
        assert_eq!(project.midi_items()[0].controllers(), expected);
        project.undo().unwrap();
        assert_eq!(project.midi_items()[0].controllers().len(), 1);
        project.redo().unwrap();
        assert_eq!(project.midi_items()[0].controllers(), expected);
    }

    #[test]
    fn pan_lane_edits_cc10_and_preserves_notes_and_other_controllers() {
        let mut project = Project::new();
        project
            .apply(aaadaw_core::DawAction::CreateTrack {
                index: 0,
                name: "Track".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        project
            .apply(aaadaw_core::DawAction::InsertMidiItem {
                track_id,
                start_tick: 0,
                length_ticks: 3_840,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        project
            .apply(aaadaw_core::DawAction::AddMidiNotes {
                item_id,
                notes: vec![MidiNoteData {
                    pitch: 60,
                    tick: 120,
                    duration: 240,
                    velocity: 96,
                }],
            })
            .unwrap();
        let other_controllers = vec![MidiControllerData {
            controller: 1,
            tick: 0,
            value: 50,
        }];
        project
            .apply(aaadaw_core::DawAction::SetMidiControllers {
                item_id,
                controllers: other_controllers.clone(),
            })
            .unwrap();

        let bounds = Rectangle::new(Point::ORIGIN, Size::new(400.0, PAN_LANE_HEIGHT));
        let lane = ControllerLane {
            item: &project.midi_items()[0],
            item_id,
            controller: 10,
            lane_height: PAN_LANE_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),

            playhead_tick: None,
        };
        let mut interaction = ControllerLaneInteraction::default();
        let cursor = mouse::Cursor::Available(Point::new(96.0, 36.0));
        lane.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        )
        .expect("click should start inserting a pan point");
        let action = lane
            .update(
                &mut interaction,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .expect("release should commit the pan point");
        let (message, _, _) = action.into_inner();
        let Message::SetMidiControllers(changed_item, controllers) = message.unwrap() else {
            panic!("Pan lane should submit CC10 through the shared controller action");
        };
        assert_eq!(changed_item, item_id);
        let expected = [
            other_controllers.clone(),
            vec![MidiControllerData {
                controller: 10,
                tick: 960,
                value: 64,
            }],
        ]
        .concat();
        assert_eq!(controllers, expected);
        project
            .apply(aaadaw_core::DawAction::SetMidiControllers {
                item_id,
                controllers,
            })
            .unwrap();

        let lane = ControllerLane {
            item: &project.midi_items()[0],
            item_id,
            controller: 10,
            lane_height: PAN_LANE_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),

            playhead_tick: None,
        };
        let mut interaction = ControllerLaneInteraction::default();
        lane.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        )
        .expect("existing pan point should start a drag");
        lane.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(192.0, 15.0),
            }),
            bounds,
            mouse::Cursor::Available(Point::new(192.0, 15.0)),
        )
        .expect("drag should update the pan point");
        let action = lane
            .update(
                &mut interaction,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(Point::new(192.0, 15.0)),
            )
            .expect("release should commit the moved pan point");
        let (message, _, _) = action.into_inner();
        let Message::SetMidiControllers(_, controllers) = message.unwrap() else {
            panic!("moving a pan point should replace the controller lane");
        };
        let moved = [
            other_controllers.clone(),
            vec![MidiControllerData {
                controller: 10,
                tick: 1_920,
                value: 115,
            }],
        ]
        .concat();
        assert_eq!(controllers, moved);
        project
            .apply(aaadaw_core::DawAction::SetMidiControllers {
                item_id,
                controllers: controllers.clone(),
            })
            .unwrap();
        assert_eq!(project.midi_items()[0].notes().len(), 1);
        assert_eq!(project.midi_items()[0].controllers(), moved);
        project.undo().unwrap();
        assert_eq!(project.midi_items()[0].controllers(), expected);
        project.redo().unwrap();
        assert_eq!(project.midi_items()[0].controllers(), moved);

        let lane = ControllerLane {
            item: &project.midi_items()[0],
            item_id,
            controller: 10,
            lane_height: PAN_LANE_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),

            playhead_tick: None,
        };
        let mut interaction = ControllerLaneInteraction::default();
        lane.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
            bounds,
            mouse::Cursor::Available(Point::new(192.0, 15.0)),
        )
        .expect("right-click should open the pan point menu");
        let action = lane
            .update(
                &mut interaction,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(Point::new(200.0, 23.0)),
            )
            .expect("choosing Delete CC10 point should remove that point");
        let (message, _, _) = action.into_inner();
        let Message::SetMidiControllers(_, controllers) = message.unwrap() else {
            panic!("deleting a pan point should preserve the shared controller list");
        };
        assert_eq!(controllers, other_controllers);
        project
            .apply(aaadaw_core::DawAction::SetMidiControllers {
                item_id,
                controllers,
            })
            .unwrap();
        assert_eq!(project.midi_items()[0].controllers(), other_controllers);
        project.undo().unwrap();
        assert_eq!(project.midi_items()[0].controllers(), moved);
    }

    #[test]
    fn modifier_click_adds_or_removes_one_note_from_selection() {
        let first = 1;
        let second = 2;
        let selected = HashSet::from([first]);
        assert_eq!(
            selection_after_click(&selected, second, true),
            HashSet::from([first, second])
        );
        assert_eq!(
            selection_after_click(&selected, first, true),
            HashSet::new()
        );
    }

    #[test]
    fn velocity_drag_uses_one_delta_and_clamps_each_note_to_midi_range() {
        let delta = velocity_delta(80.0, 64.0);
        assert_eq!(delta, 20);
        assert_eq!(apply_velocity_delta(70, delta), 90);
        assert_eq!(apply_velocity_delta(120, delta), 127);
        assert_eq!(apply_velocity_delta(10, -20), 1);
    }

    #[test]
    fn group_note_move_clamps_one_shared_delta_at_item_and_pitch_bounds() {
        let notes = vec![
            (
                NoteId::from_value(1).unwrap(),
                MidiNoteData {
                    pitch: 5,
                    tick: 20,
                    duration: 10,
                    velocity: 90,
                },
            ),
            (
                NoteId::from_value(2).unwrap(),
                MidiNoteData {
                    pitch: 100,
                    tick: 50,
                    duration: 20,
                    velocity: 80,
                },
            ),
        ];

        assert_eq!(bounded_note_move_delta(&notes, 0, 100, -40, -20), (-20, -5));
        assert_eq!(bounded_note_move_delta(&notes, 0, 100, 50, 50), (30, 27));
    }

    #[test]
    fn group_note_drag_preview_matches_the_boundary_clamped_edit() {
        let mut project = Project::new();
        project
            .apply(aaadaw_core::DawAction::CreateTrack {
                index: 0,
                name: "Track".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        project
            .apply(aaadaw_core::DawAction::InsertMidiItem {
                track_id,
                start_tick: 0,
                length_ticks: 2_000,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        project
            .apply(aaadaw_core::DawAction::AddMidiNotes {
                item_id,
                notes: vec![
                    MidiNoteData {
                        pitch: 0,
                        tick: 500,
                        duration: 100,
                        velocity: 90,
                    },
                    MidiNoteData {
                        pitch: 5,
                        tick: 1_000,
                        duration: 100,
                        velocity: 80,
                    },
                ],
            })
            .unwrap();
        let item = &project.midi_items()[0];
        let selected = item
            .notes()
            .iter()
            .map(|note| note.id())
            .collect::<HashSet<_>>();
        let roll = PianoRoll {
            tool: MidiEditorTool::Select,
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
            project: &project,
            item,
            item_id,
            selected: &selected,
            origin_tick: 0,
            high_pitch: 35,
            pixels_per_beat: 960.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),
            region: RollRegion::Pitch,

            playhead_tick: None,
        };
        let bounds = Rectangle::new(
            Point::ORIGIN,
            Size::new(
                2_000.0,
                HEADER_HEIGHT + f32::from(PITCH_COUNT) * NOTE_ROW_HEIGHT,
            ),
        );
        let start = Point::new(
            KEY_WIDTH + 1_000.0,
            HEADER_HEIGHT + 30.0 * NOTE_ROW_HEIGHT + NOTE_ROW_HEIGHT / 2.0,
        );
        let mut interaction = Interaction::default();
        roll.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            mouse::Cursor::Available(start),
        )
        .expect("selected note should start a group drag");

        let moved = Point::new(start.x - 720.0, start.y + NOTE_ROW_HEIGHT);
        roll.update(
            &mut interaction,
            &Event::Mouse(mouse::Event::CursorMoved { position: moved }),
            bounds,
            mouse::Cursor::Available(moved),
        )
        .expect("group drag should update its preview");
        let preview = interaction.drag.as_ref().expect("drag remains active");
        assert_eq!((preview.delta_tick, preview.delta_pitch), (-500, 0));

        let action = roll
            .update(
                &mut interaction,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(moved),
            )
            .expect("group drag should submit an edit");
        let (message, _, _) = action.into_inner();
        let Message::EditMidiNotes(changed_item, edits) = message.unwrap() else {
            panic!("group drag should submit MIDI note edits");
        };
        assert_eq!(changed_item, item_id);
        let edits = edits.into_iter().collect::<HashMap<_, _>>();
        assert_eq!(edits.keys().copied().collect::<HashSet<_>>(), selected);
        let original_notes = item
            .notes()
            .iter()
            .map(|note| (note.id(), note))
            .collect::<HashMap<_, _>>();
        for (note_id, edit) in edits {
            let original = original_notes[&note_id];
            assert_eq!(edit.tick, original.tick().saturating_sub(500));
            assert_eq!(edit.pitch, original.pitch());
        }
    }

    #[test]
    fn same_onset_velocity_handles_are_spread_around_the_note_tick() {
        let mapping = RollMapping {
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            snap: MidiSnap::default(),
            high_pitch: 84,
        };
        let mut project = Project::new();
        project
            .apply(aaadaw_core::DawAction::CreateTrack {
                index: 0,
                name: "Track".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        project
            .apply(aaadaw_core::DawAction::InsertMidiItem {
                track_id,
                start_tick: 0,
                length_ticks: 3_840,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        project
            .apply(aaadaw_core::DawAction::AddMidiNotes {
                item_id,
                notes: vec![
                    MidiNoteData {
                        pitch: 60,
                        tick: 0,
                        duration: 240,
                        velocity: 90,
                    },
                    MidiNoteData {
                        pitch: 64,
                        tick: 0,
                        duration: 240,
                        velocity: 90,
                    },
                ],
            })
            .unwrap();
        let notes = project.midi_items()[0].notes();
        let positions = velocity_handle_positions(notes, mapping, 0, 3_840);
        let first = positions[&notes[0].id()];
        let second = positions[&notes[1].id()];
        assert_eq!(second - first, 5.0);
        assert_eq!(
            velocity_note_at_x(notes, mapping, 0, 3_840, first + 4.0),
            Some(notes[0].id())
        );
        assert_eq!(
            velocity_note_at_x(notes, mapping, 0, 3_840, second + 4.0),
            Some(notes[1].id())
        );
    }
}
