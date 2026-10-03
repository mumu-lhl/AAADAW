use super::super::{App, Message};
use super::tokens::{PANEL_PADDING, ROW_GAP, SPACING_XS};
use aaadaw_core::{ItemId, MidiControllerData, MidiItem, MidiNoteData, NoteId, Project};
use iced::advanced::text::{Alignment as TextAlignment, LineHeight, Shaping};
use iced::widget::canvas::{self, Text};
use iced::widget::{button, canvas as canvas_widget, column, container, row, scrollable, text};
use iced::{
    Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, Theme, keyboard, mouse,
};
use std::collections::{HashMap, HashSet};

const KEY_WIDTH: f32 = 56.0;
const HEADER_HEIGHT: f32 = 28.0;
const NOTE_ROW_HEIGHT: f32 = 18.0;
const PITCH_COUNT: u8 = 36;
const VELOCITY_LANE_HEIGHT: f32 = 104.0;
const SUSTAIN_LANE_HEIGHT: f32 = 96.0;
const MODULATION_LANE_HEIGHT: f32 = 72.0;
const CONTROLLER_CONTEXT_WIDTH: f32 = 112.0;
const CONTROLLER_CONTEXT_HEIGHT: f32 = 24.0;

pub(super) fn view(app: &App) -> Element<'_, Message> {
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
    let edit_toolbar = row![
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
                    app.midi_editor_selected_notes.iter().copied().collect()
                ),
            ))
            .padding([SPACING_XS / 2.0, SPACING_XS]),
        button("Paste")
            .style(iced::widget::button::secondary)
            .on_press_maybe(
                (!app.midi_note_clipboard.notes.is_empty())
                    .then_some(Message::PasteMidiNotes(item_id))
            )
            .padding([SPACING_XS / 2.0, SPACING_XS]),
        button("Delete notes")
            .style(iced::widget::button::danger)
            .on_press_maybe((!app.midi_editor_selected_notes.is_empty()).then_some(
                Message::DeleteMidiNotes(
                    item_id,
                    app.midi_editor_selected_notes.iter().copied().collect(),
                )
            ))
            .padding([SPACING_XS / 2.0, SPACING_XS]),
        roll_button("×", Message::CloseMidiEditor),
    ]
    .spacing(ROW_GAP)
    .align_y(iced::Alignment::Center);
    let navigation_toolbar = row![
        roll_button("− Beat", Message::PianoRollPan(-1)),
        roll_button("+ Beat", Message::PianoRollPan(1)),
        roll_button("Zoom −", Message::PianoRollZoom(0.8)),
        roll_button("Zoom +", Message::PianoRollZoom(1.25)),
        roll_button("− Oct", Message::PianoRollPitchScroll(-12)),
        roll_button("+ Oct", Message::PianoRollPitchScroll(12)),
    ]
    .spacing(ROW_GAP)
    .align_y(iced::Alignment::Center);
    let ticks_per_beat = u64::from(app.project.settings().ppq());
    let pitch_canvas = canvas_widget::Canvas::new(PianoRoll {
        project: &app.project,
        item,
        item_id,
        selected: &app.midi_editor_selected_notes,
        origin_tick: app.midi_editor_origin_tick,
        high_pitch: app.midi_editor_high_pitch,
        pixels_per_beat: app.midi_editor_pixels_per_beat,
        ticks_per_beat,
        region: RollRegion::Pitch,
    })
    .width(Length::Fill)
    .height(Length::Fixed(
        HEADER_HEIGHT + f32::from(PITCH_COUNT) * NOTE_ROW_HEIGHT,
    ));
    let velocity_canvas = canvas_widget::Canvas::new(PianoRoll {
        project: &app.project,
        item,
        item_id,
        selected: &app.midi_editor_selected_notes,
        origin_tick: app.midi_editor_origin_tick,
        high_pitch: app.midi_editor_high_pitch,
        pixels_per_beat: app.midi_editor_pixels_per_beat,
        ticks_per_beat,
        region: RollRegion::Velocity,
    })
    .width(Length::Fill)
    .height(Length::Fixed(VELOCITY_LANE_HEIGHT));
    let velocity_lane = row![
        container(text("Velocity"))
            .width(Length::Fixed(KEY_WIDTH))
            .height(Length::Fixed(VELOCITY_LANE_HEIGHT))
            .center_y(Length::Fill)
            .padding(SPACING_XS),
        velocity_canvas
    ]
    .height(Length::Fixed(VELOCITY_LANE_HEIGHT));
    let sustain_canvas = canvas_widget::Canvas::new(ControllerLane {
        item,
        item_id,
        controller: 64,
        lane_height: SUSTAIN_LANE_HEIGHT,
        origin_tick: app.midi_editor_origin_tick,
        pixels_per_beat: app.midi_editor_pixels_per_beat,
        ticks_per_beat,
    })
    .width(Length::Fill)
    .height(Length::Fixed(SUSTAIN_LANE_HEIGHT));
    let sustain_lane = row![
        container(text("Sustain"))
            .width(Length::Fixed(KEY_WIDTH))
            .height(Length::Fixed(SUSTAIN_LANE_HEIGHT))
            .center_y(Length::Fill)
            .padding(SPACING_XS),
        sustain_canvas
    ]
    .height(Length::Fixed(SUSTAIN_LANE_HEIGHT));
    let modulation_canvas = canvas_widget::Canvas::new(ControllerLane {
        item,
        item_id,
        controller: 1,
        lane_height: MODULATION_LANE_HEIGHT,
        origin_tick: app.midi_editor_origin_tick,
        pixels_per_beat: app.midi_editor_pixels_per_beat,
        ticks_per_beat,
    })
    .width(Length::Fill)
    .height(Length::Fixed(MODULATION_LANE_HEIGHT));
    let modulation_lane = row![
        container(text("Mod CC1"))
            .width(Length::Fixed(KEY_WIDTH))
            .height(Length::Fixed(MODULATION_LANE_HEIGHT))
            .center_y(Length::Fill)
            .padding(SPACING_XS),
        modulation_canvas
    ]
    .height(Length::Fixed(MODULATION_LANE_HEIGHT));
    column![
        edit_toolbar,
        navigation_toolbar,
        scrollable(pitch_canvas).height(Length::Fill),
        velocity_lane,
        sustain_lane,
        modulation_lane
    ]
    .spacing(ROW_GAP)
    .padding(PANEL_PADDING)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

struct ControllerLane<'a> {
    item: &'a MidiItem,
    item_id: ItemId,
    controller: u8,
    lane_height: f32,
    origin_tick: u64,
    pixels_per_beat: f32,
    ticks_per_beat: u64,
}

#[derive(Default)]
struct ControllerLaneInteraction {
    drag: Option<ControllerDrag>,
    context_menu: Option<(usize, Point)>,
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
            origin_tick: self.origin_tick,
            pixels_per_beat: self.pixels_per_beat,
            ticks_per_beat: self.ticks_per_beat,
            high_pitch: 0,
        }
    }

    fn point_data(&self, point: Point) -> MidiControllerData {
        let mapping = self.mapping();
        MidiControllerData {
            controller: self.controller,
            tick: snap_tick(mapping.tick_at_x(point.x), mapping.grid_ticks())
                .min(self.item.length_ticks().saturating_sub(1)),
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
            .filter(|(_, controller)| controller.controller == self.controller)
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

impl canvas::Program<Message> for ControllerLane<'_> {
    type State = ControllerLaneInteraction;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
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
                        current: self.point_data(point),
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
                let mapping = self.mapping();
                drag.current = MidiControllerData {
                    controller: self.controller,
                    tick: snap_tick(mapping.tick_at_x(point.x), mapping.grid_ticks())
                        .min(self.item.length_ticks().saturating_sub(1)),
                    value: self.value_at_y(point.y),
                };
                Some(canvas::Action::request_redraw())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let drag = state.drag.take()?;
                let mut controllers = drag.controllers;
                if let Some(index) = drag.index {
                    controllers[index] = drag.current;
                } else {
                    controllers.push(drag.current);
                }
                let changed_index = drag.index.unwrap_or(controllers.len() - 1);
                let duplicate_position = controllers.iter().enumerate().any(|(index, current)| {
                    index != changed_index
                        && current.controller == drag.current.controller
                        && current.tick == drag.current.tick
                });
                if duplicate_position {
                    return Some(canvas::Action::capture());
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

        let controllers = self.item.controllers();
        let mut value = controllers
            .iter()
            .filter(|controller| {
                controller.controller == self.controller && controller.tick < self.origin_tick
            })
            .max_by_key(|controller| controller.tick)
            .map_or(0, |controller| controller.value);
        let mut segment_start = 0.0;
        for controller in controllers.iter().filter(|controller| {
            controller.controller == self.controller
                && self.origin_tick <= controller.tick
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
                    .with_color(Color::from_rgb8(111, 190, 150)),
            );
            frame.fill_rectangle(
                Point::new(current_x - 5.0, current_y - 5.0),
                Size::new(10.0, 10.0),
                Color::WHITE,
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
        if let Some((_, origin)) = state.context_menu {
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
        vec![frame.into_geometry()]
    }
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
}

impl RollMapping {
    fn grid_ticks(self) -> u64 {
        (self.ticks_per_beat / 4).max(1)
    }

    fn tick_at_x(self, x: f32) -> u64 {
        let ticks = (f64::from(x.max(0.0)) / f64::from(self.pixels_per_beat)
            * self.ticks_per_beat as f64)
            .round() as u64;
        self.origin_tick.saturating_add(ticks)
    }

    fn pitch_at_y(self, y: f32) -> u8 {
        let row = (y.max(0.0) / NOTE_ROW_HEIGHT).floor() as u8;
        self.high_pitch.saturating_sub(row).min(127)
    }

    fn x_at_tick(self, tick: u64) -> f32 {
        (i128::from(tick) - i128::from(self.origin_tick)) as f32 / self.ticks_per_beat as f32
            * self.pixels_per_beat
    }

    fn y_at_pitch(self, pitch: u8) -> f32 {
        f32::from(self.high_pitch.saturating_sub(pitch)) * NOTE_ROW_HEIGHT
    }
}

struct PianoRoll<'a> {
    project: &'a Project,
    item: &'a MidiItem,
    item_id: ItemId,
    selected: &'a HashSet<NoteId>,
    origin_tick: u64,
    high_pitch: u8,
    pixels_per_beat: f32,
    ticks_per_beat: u64,
    region: RollRegion,
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
    hovered_velocity_note: Option<NoteId>,
}

#[derive(Clone)]
struct NoteDrag {
    start: Point,
    notes: Vec<(NoteId, MidiNoteData)>,
    resize: bool,
    delta_tick: i64,
    delta_pitch: i16,
    velocity: bool,
    delta_velocity: i16,
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
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.modifiers = *modifiers;
            return None;
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
        if self.region == RollRegion::Pitch
            && let Event::Keyboard(keyboard::Event::KeyPressed {
                key, repeat: false, ..
            }) = event
            && matches!(
                key.as_ref(),
                keyboard::Key::Named(
                    keyboard::key::Named::Delete | keyboard::key::Named::Backspace
                )
            )
            && !self.selected.is_empty()
        {
            return Some(canvas::Action::publish(Message::DeleteMidiNotes(
                self.item_id,
                self.selected.iter().copied().collect(),
            )));
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
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
                if point.x < 0.0 || point.y < 0.0 {
                    return Some(canvas::Action::capture());
                }
                if self.region == RollRegion::Velocity {
                    let lane_y = point.y;
                    let Some(note_id) =
                        velocity_note_at_x(self.item.notes(), self.mapping(), point.x)
                    else {
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
                        notes,
                        resize: false,
                        delta_tick: 0,
                        delta_pitch: 0,
                        velocity: true,
                        delta_velocity: 0,
                    });
                    return Some(
                        canvas::Action::publish(Message::SelectMidiNotes(note_ids)).and_capture(),
                    );
                }
                let mapping = self.mapping();
                let hit = self.item.notes().iter().rev().find(|note| {
                    let left = mapping.x_at_tick(note.tick());
                    let right = mapping.x_at_tick(note.tick().saturating_add(note.duration()));
                    let top = mapping.y_at_pitch(note.pitch());
                    point.x >= left
                        && point.x <= right
                        && point.y >= top
                        && point.y < top + NOTE_ROW_HEIGHT
                });
                let Some(note) = hit else {
                    let pitch = mapping.pitch_at_y(point.y);
                    let tick = snap_tick(mapping.tick_at_x(point.x), mapping.grid_ticks());
                    let data = MidiNoteData {
                        pitch,
                        tick,
                        duration: mapping.grid_ticks(),
                        velocity: 96,
                    };
                    if data.tick.saturating_add(data.duration) > self.item.length_ticks() {
                        return Some(canvas::Action::capture());
                    }
                    return Some(canvas::Action::publish(Message::AddMidiNoteAt(
                        self.item_id,
                        data,
                    )));
                };
                let selected = selection_after_click(
                    self.selected,
                    note.id(),
                    state.modifiers.command() || state.modifiers.control(),
                );
                if state.modifiers.command() || state.modifiers.control() {
                    return Some(
                        canvas::Action::publish(Message::SelectMidiNotes(selected)).and_capture(),
                    );
                }
                let resize =
                    mapping.x_at_tick(note.tick().saturating_add(note.duration())) - point.x < 9.0;
                let note_ids = if resize {
                    HashSet::from([note.id()])
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
                    notes,
                    resize,
                    delta_tick: 0,
                    delta_pitch: 0,
                    velocity: false,
                    delta_velocity: 0,
                });
                Some(canvas::Action::publish(Message::SelectMidiNotes(selected)).and_capture())
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
                if state.drag.is_none() {
                    let hovered = if self.region == RollRegion::Velocity {
                        velocity_note_at_x(self.item.notes(), self.mapping(), point.x)
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
                let ticks = (f64::from(point.x - drag.start.x) / f64::from(self.pixels_per_beat)
                    * self.ticks_per_beat as f64)
                    .round() as i64;
                let grid_ticks = (self.ticks_per_beat / 4).max(1) as i64;
                drag.delta_tick = (ticks as f64 / grid_ticks as f64).round() as i64 * grid_ticks;
                if drag.velocity {
                    drag.delta_velocity = velocity_delta(drag.start.y, point.y);
                } else {
                    drag.delta_pitch = ((drag.start.y - point.y) / NOTE_ROW_HEIGHT).round() as i16;
                }
                Some(canvas::Action::request_redraw())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let drag = state.drag.take()?;
                state.hovered_velocity_note = None;
                let edits = drag
                    .notes
                    .into_iter()
                    .map(|(note_id, mut data)| {
                        if drag.velocity {
                            data.velocity =
                                apply_velocity_delta(data.velocity, drag.delta_velocity);
                        } else if drag.resize {
                            data.duration = (i128::from(data.duration)
                                + i128::from(drag.delta_tick))
                            .max(1) as u64;
                        } else {
                            data.tick =
                                (i128::from(data.tick) + i128::from(drag.delta_tick)).max(0) as u64;
                            data.pitch =
                                (i16::from(data.pitch) + drag.delta_pitch).clamp(0, 127) as u8;
                        }
                        (note_id, data)
                    })
                    .collect::<Vec<_>>();
                if edits.iter().all(|(id, data)| {
                    self.item
                        .notes()
                        .iter()
                        .find(|note| note.id() == *id)
                        .is_some_and(|_| {
                            data.tick.saturating_add(data.duration) <= self.item.length_ticks()
                        })
                }) && edits.iter().any(|(id, data)| {
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
        for row in 0..PITCH_COUNT {
            let pitch = self.high_pitch.saturating_sub(row);
            let y = grid_top + f32::from(row) * NOTE_ROW_HEIGHT;
            let black = matches!(pitch % 12, 1 | 3 | 6 | 8 | 10);
            let row_color = if black {
                Color::from_rgb8(31, 35, 38)
            } else {
                Color::from_rgb8(39, 44, 47)
            };
            frame.fill_rectangle(
                Point::new(grid_left, y),
                Size::new(bounds.width - grid_left, NOTE_ROW_HEIGHT),
                row_color,
            );
            let line = canvas::Path::line(Point::new(grid_left, y), Point::new(bounds.width, y));
            frame.stroke(
                &line,
                canvas::Stroke::default()
                    .with_color(Color::from_rgb8(52, 59, 63))
                    .with_width(0.7),
            );
            let key =
                canvas::Path::rectangle(Point::new(0.0, y), Size::new(KEY_WIDTH, NOTE_ROW_HEIGHT));
            frame.fill(
                &key,
                if black {
                    Color::from_rgb8(46, 51, 54)
                } else {
                    Color::from_rgb8(66, 72, 76)
                },
            );
            if pitch % 12 == 0 || row == 0 {
                frame.fill_text(Text {
                    content: pitch_name(pitch),
                    position: Point::new(KEY_WIDTH - 5.0, y + NOTE_ROW_HEIGHT / 2.0),
                    max_width: KEY_WIDTH - 8.0,
                    color: Color::from_rgb8(218, 222, 224),
                    size: Pixels(10.0),
                    line_height: LineHeight::Relative(1.0),
                    font: Font::default(),
                    align_x: TextAlignment::Right,
                    align_y: iced::alignment::Vertical::Center,
                    shaping: Shaping::Basic,
                });
            }
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
                || note.pitch() < self.high_pitch.saturating_sub(PITCH_COUNT - 1)
            {
                continue;
            }
            let x = grid_left + mapping.x_at_tick(note.tick());
            let y = grid_top + mapping.y_at_pitch(note.pitch());
            let width = (note.duration() as f32 / self.ticks_per_beat as f32
                * self.pixels_per_beat)
                .max(3.0);
            let rect = canvas::Path::rectangle(
                Point::new(x, y + 2.0),
                Size::new(width, NOTE_ROW_HEIGHT - 4.0),
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
                frame.fill_rectangle(
                    Point::new(
                        x + drag.delta_tick as f32 / self.ticks_per_beat as f32
                            * self.pixels_per_beat,
                        y + 2.0 - f32::from(drag.delta_pitch) * NOTE_ROW_HEIGHT,
                    ),
                    Size::new(width, NOTE_ROW_HEIGHT - 4.0),
                    Color::from_rgba8(214, 205, 111, 0.45),
                );
            }
        }
        vec![frame.into_geometry()]
    }
    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

impl PianoRoll<'_> {
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
        let velocity_positions = velocity_handle_positions(self.item.notes(), mapping);
        for note in self.item.notes() {
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
    }

    fn mapping(&self) -> RollMapping {
        RollMapping {
            origin_tick: self.origin_tick,
            pixels_per_beat: self.pixels_per_beat.max(16.0),
            ticks_per_beat: self.ticks_per_beat,
            high_pitch: self.high_pitch,
        }
    }
}

fn snap_tick(tick: u64, grid_ticks: u64) -> u64 {
    (tick.saturating_add(grid_ticks / 2) / grid_ticks) * grid_ticks
}

fn note_data(note: &aaadaw_core::MidiNote) -> MidiNoteData {
    MidiNoteData {
        pitch: note.pitch(),
        tick: note.tick(),
        duration: note.duration(),
        velocity: note.velocity(),
    }
}

fn velocity_note_at_x(
    notes: &[aaadaw_core::MidiNote],
    mapping: RollMapping,
    x: f32,
) -> Option<NoteId> {
    let positions = velocity_handle_positions(notes, mapping);
    notes
        .iter()
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
) -> HashMap<NoteId, f32> {
    let mut ordered = notes.iter().collect::<Vec<_>>();
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

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::canvas::Program;

    #[test]
    fn time_and_pitch_mapping_respect_origin_and_visible_pitch_range() {
        let mapping = RollMapping {
            origin_tick: 1_920,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
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
        assert_eq!(snap_tick(100, 240), 0);
        assert_eq!(snap_tick(140, 240), 240);
        assert_eq!(snap_tick(500, 240), 480);
        assert_eq!(snap_tick(u64::MAX, 240), u64::MAX / 240 * 240);
    }

    #[test]
    fn mapping_and_sixteenth_grid_follow_project_ppq() {
        let mapping = RollMapping {
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 480,
            high_pitch: 60,
        };
        assert_eq!(mapping.tick_at_x(24.0), 120);
        assert_eq!(mapping.x_at_tick(120), 24.0);
        assert_eq!(mapping.grid_ticks(), 120);
        assert_eq!(snap_tick(70, mapping.grid_ticks()), 120);
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
        };
        let point = lane.point_data(Point::new(96.0, 36.0));
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
    fn same_onset_velocity_handles_are_spread_around_the_note_tick() {
        let mapping = RollMapping {
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
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
        let positions = velocity_handle_positions(notes, mapping);
        let first = positions[&notes[0].id()];
        let second = positions[&notes[1].id()];
        assert_eq!(second - first, 5.0);
        assert_eq!(
            velocity_note_at_x(notes, mapping, first + 4.0),
            Some(notes[0].id())
        );
        assert_eq!(
            velocity_note_at_x(notes, mapping, second + 4.0),
            Some(notes[1].id())
        );
    }
}
