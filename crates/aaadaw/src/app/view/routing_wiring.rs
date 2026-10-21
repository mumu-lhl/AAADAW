use super::super::{App, Message};
use aaadaw_core::{Project, TrackId};
use iced::advanced::text::{Alignment as TextAlignment, LineHeight, Shaping};
use iced::widget::{canvas, scrollable};
use iced::{Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, Theme, mouse};

const NODE_WIDTH: f32 = 208.0;
const NODE_HEADER: f32 = 30.0;
const NODE_ROW: f32 = 34.0;
const NODE_HEIGHT: f32 = NODE_HEADER + NODE_ROW * 4.0 + 12.0;
const MASTER_X: f32 = 24.0;
const TRACK_START_X: f32 = 290.0;
const NODE_STEP_X: f32 = 254.0;
const NODE_Y: f32 = 72.0;
const DEVICE_GAP_X: f32 = 254.0;
const HIT_RADIUS: f32 = 10.0;

#[derive(Clone, Copy, Debug)]
struct Wire {
    source: Option<TrackId>,
    destination: Option<TrackId>,
    start: Point,
    end: Point,
    bend_offset: f32,
    color: Color,
}

#[derive(Default)]
struct WiringState {
    drag_source: Option<TrackId>,
    drag_cursor: Option<Point>,
}

pub(super) fn view(app: &App, graph_busy: bool) -> Element<'_, Message> {
    let width = graph_width(app.project.tracks().len());
    let diagram = canvas::Canvas::new(WiringProgram {
        project: &app.project,
        graph_busy,
    })
    .width(Length::Fixed(width))
    .height(Length::Fixed(430.0));

    scrollable(diagram)
        .direction(scrollable::Direction::Both {
            vertical: scrollable::Scrollbar::default(),
            horizontal: scrollable::Scrollbar::default(),
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

struct WiringProgram<'a> {
    project: &'a Project,
    graph_busy: bool,
}

impl WiringProgram<'_> {
    fn tracks(&self) -> &[aaadaw_core::Track] {
        self.project.tracks()
    }

    fn track_rect(&self, index: usize) -> Rectangle {
        Rectangle::new(
            Point::new(TRACK_START_X + index as f32 * NODE_STEP_X, NODE_Y),
            Size::new(NODE_WIDTH, NODE_HEIGHT),
        )
    }

    fn master_rect(&self) -> Rectangle {
        Rectangle::new(
            Point::new(MASTER_X, NODE_Y),
            Size::new(NODE_WIDTH, NODE_HEIGHT),
        )
    }

    fn device_rect(&self) -> Rectangle {
        let x = TRACK_START_X
            + self.tracks().len().saturating_sub(1) as f32 * NODE_STEP_X
            + DEVICE_GAP_X;
        Rectangle::new(Point::new(x, NODE_Y), Size::new(NODE_WIDTH, NODE_HEIGHT))
    }

    fn input_port(rect: Rectangle, row: usize, offset: f32) -> Point {
        Point::new(rect.x + 3.0, row_center(rect, row) + offset)
    }

    fn output_port(rect: Rectangle, row: usize, offset: f32) -> Point {
        Point::new(rect.x + rect.width - 3.0, row_center(rect, row) + offset)
    }

    fn wires(&self) -> Vec<Wire> {
        let mut wires = Vec::new();
        let master = self.master_rect();
        let device = self.device_rect();
        let device_input = Self::input_port(device, 0, 0.0);

        // The project Master always feeds the selected device output. Track hardware
        // outputs remain separate paths and can bypass this Master node.
        wires.push(Wire {
            source: None,
            destination: None,
            start: Self::output_port(master, 0, 0.0),
            end: device_input,
            bend_offset: -52.0,
            color: Color::from_rgb8(152, 164, 171),
        });

        for (source_index, source) in self.tracks().iter().enumerate() {
            let source_rect = self.track_rect(source_index);
            if source.main_send_enabled() {
                let destination_rect = source
                    .effective_output_track()
                    .and_then(|destination| {
                        self.tracks()
                            .iter()
                            .position(|track| track.id() == destination)
                            .map(|index| self.track_rect(index))
                    })
                    .unwrap_or(master);
                wires.push(Wire {
                    source: Some(source.id()),
                    destination: None,
                    start: Self::output_port(source_rect, 2, 0.0),
                    end: Self::input_port(destination_rect, 0, 0.0),
                    bend_offset: 0.0,
                    color: Color::from_rgb8(193, 198, 200),
                });
            }

            for (send_index, send) in source.sends().iter().enumerate() {
                let Some(destination_index) = self
                    .tracks()
                    .iter()
                    .position(|track| track.id() == send.destination())
                else {
                    continue;
                };
                let destination_rect = self.track_rect(destination_index);
                let offset = fan_offset(send_index, source.sends().len());
                wires.push(Wire {
                    source: Some(source.id()),
                    destination: Some(send.destination()),
                    start: Self::output_port(source_rect, 0, offset),
                    end: Self::input_port(destination_rect, 0, offset),
                    bend_offset: 0.0,
                    color: if send.parameters().muted {
                        Color::from_rgb8(112, 119, 122)
                    } else {
                        Color::from_rgb8(134, 191, 202)
                    },
                });
            }

            for (output_index, _) in source.hardware_outputs().iter().enumerate() {
                let offset = fan_offset(output_index, source.hardware_outputs().len());
                wires.push(Wire {
                    source: Some(source.id()),
                    destination: None,
                    start: Self::output_port(source_rect, 1, offset),
                    end: device_input,
                    bend_offset: 0.0,
                    color: Color::from_rgb8(72, 183, 170),
                });
            }
        }

        wires
    }

    fn track_output_at(&self, position: Point) -> Option<TrackId> {
        self.tracks().iter().enumerate().find_map(|(index, track)| {
            (distance(position, Self::output_port(self.track_rect(index), 0, 0.0)) <= HIT_RADIUS)
                .then_some(track.id())
        })
    }

    fn track_input_at(&self, position: Point) -> Option<TrackId> {
        self.tracks().iter().enumerate().find_map(|(index, track)| {
            (distance(position, Self::input_port(self.track_rect(index), 0, 0.0)) <= HIT_RADIUS)
                .then_some(track.id())
        })
    }

    fn wire_at(&self, position: Point) -> Option<Wire> {
        self.wires().into_iter().find(|wire| {
            bezier_segments(wire.start, wire.end, wire.bend_offset)
                .windows(2)
                .any(|segment| {
                    point_segment_distance(position, segment[0], segment[1]) <= HIT_RADIUS
                })
        })
    }
}

impl canvas::Program<Message> for WiringProgram<'_> {
    type State = WiringState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let position = cursor.position_in(bounds)?;
                if !self.graph_busy
                    && let Some(source) = self.track_output_at(position)
                {
                    state.drag_source = Some(source);
                    state.drag_cursor = Some(position);
                    return Some(canvas::Action::capture());
                }
                let wire = self.wire_at(position)?;
                let source = wire.source?;
                let message = wire.destination.map_or_else(
                    || Message::OpenTrackRouting(source),
                    |destination| Message::OpenTrackRoutingTo(source, destination),
                );
                Some(canvas::Action::publish(message))
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.drag_source.is_some() => {
                state.drag_cursor = cursor.position_in(bounds);
                Some(canvas::Action::request_redraw().and_capture())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let source = state.drag_source.take()?;
                let destination = cursor
                    .position_in(bounds)
                    .and_then(|position| self.track_input_at(position));
                state.drag_cursor = None;
                if let Some(destination) = destination
                    && source != destination
                    && self.project.can_route_to(source, destination)
                {
                    return Some(
                        canvas::Action::publish(Message::SetRoutingMatrixSend(source, destination))
                            .and_capture(),
                    );
                }
                Some(canvas::Action::request_redraw().and_capture())
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
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), Color::from_rgb8(31, 35, 38));
        let mut x = 0.0;
        while x < bounds.width {
            let grid = canvas::Path::line(Point::new(x, 0.0), Point::new(x, bounds.height));
            frame.stroke(
                &grid,
                canvas::Stroke::default()
                    .with_color(Color::from_rgba8(119, 130, 136, 34.0 / 255.0))
                    .with_width(1.0),
            );
            x += 24.0;
        }
        let mut y = 0.0;
        while y < bounds.height {
            let grid = canvas::Path::line(Point::new(0.0, y), Point::new(bounds.width, y));
            frame.stroke(
                &grid,
                canvas::Stroke::default()
                    .with_color(Color::from_rgba8(119, 130, 136, 34.0 / 255.0))
                    .with_width(1.0),
            );
            y += 24.0;
        }

        for wire in self.wires() {
            frame.stroke(
                &wire_path(wire.start, wire.end, wire.bend_offset),
                canvas::Stroke::default()
                    .with_color(wire.color)
                    .with_width(2.0),
            );
        }
        if let (Some(source), Some(cursor)) = (state.drag_source, state.drag_cursor)
            && let Some(index) = self.tracks().iter().position(|track| track.id() == source)
        {
            frame.stroke(
                &wire_path(
                    Self::output_port(self.track_rect(index), 0, 0.0),
                    cursor,
                    0.0,
                ),
                canvas::Stroke::default()
                    .with_color(Color::from_rgb8(238, 191, 92))
                    .with_width(2.0),
            );
        }

        draw_node(
            &mut frame,
            self.master_rect(),
            "MASTER",
            ["Fader", "Hardware Output", "Render/save output", ""],
            1,
        );
        for (index, track) in self.tracks().iter().enumerate() {
            let sends = track.sends().len();
            let receives = self
                .tracks()
                .iter()
                .flat_map(|source| source.sends())
                .filter(|send| send.destination() == track.id())
                .count();
            let parent_state = if track.main_send_enabled() {
                if track.effective_output_track().is_some() {
                    "Parent Send  On"
                } else {
                    "Master Send  On"
                }
            } else {
                "Master / Parent  Off"
            };
            let rows = [
                "Fader".to_owned(),
                format!("Hardware Output  ×{}", track.hardware_outputs().len()),
                parent_state.to_owned(),
                format!("Sends ×{sends}  Receives ×{receives}"),
            ];
            draw_node(
                &mut frame,
                self.track_rect(index),
                &format!("{}  {}", index + 1, track.name()),
                [
                    rows[0].as_str(),
                    rows[1].as_str(),
                    rows[2].as_str(),
                    rows[3].as_str(),
                ],
                0,
            );
        }
        draw_node(
            &mut frame,
            self.device_rect(),
            "AUDIO DEVICE OUTPUT",
            ["Output 1/2", "Stereo", "", ""],
            2,
        );

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        let Some(position) = cursor.position_in(bounds) else {
            return mouse::Interaction::default();
        };
        if state.drag_source.is_some()
            || self.track_output_at(position).is_some()
            || self.wire_at(position).is_some()
        {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

fn graph_width(track_count: usize) -> f32 {
    let last_track = track_count.saturating_sub(1) as f32;
    (TRACK_START_X + last_track * NODE_STEP_X + DEVICE_GAP_X + NODE_WIDTH + 36.0).max(1000.0)
}

fn row_center(rect: Rectangle, row: usize) -> f32 {
    rect.y + NODE_HEADER + NODE_ROW * (row as f32 + 0.5)
}

fn fan_offset(index: usize, count: usize) -> f32 {
    (index as f32 - count.saturating_sub(1) as f32 * 0.5) * 6.0
}

fn draw_node(
    frame: &mut canvas::Frame<iced::Renderer>,
    rect: Rectangle,
    title: &str,
    rows: [&str; 4],
    kind: u8,
) {
    let node = canvas::Path::rectangle(Point::new(rect.x, rect.y), rect.size());
    frame.fill(&node, Color::from_rgb8(55, 60, 63));
    frame.stroke(
        &node,
        canvas::Stroke::default()
            .with_color(Color::from_rgb8(121, 132, 137))
            .with_width(1.0),
    );
    frame.fill_text(canvas::Text {
        content: title.to_owned(),
        position: Point::new(rect.x + 14.0, rect.y + 15.0),
        max_width: rect.width - 24.0,
        color: Color::from_rgb8(223, 228, 230),
        size: Pixels(12.0),
        line_height: LineHeight::Relative(1.0),
        font: Font::default(),
        align_x: TextAlignment::Left,
        align_y: iced::alignment::Vertical::Center,
        shaping: Shaping::Basic,
    });

    let row_colors = [
        Color::from_rgb8(118, 121, 181),
        Color::from_rgb8(56, 133, 128),
        Color::from_rgb8(83, 117, 143),
        Color::from_rgb8(74, 117, 139),
    ];
    for (index, label) in rows.into_iter().enumerate() {
        let y = rect.y + NODE_HEADER + NODE_ROW * index as f32 + 5.0;
        let row_rect = Rectangle::new(
            Point::new(rect.x + 10.0, y),
            Size::new(rect.width - 20.0, NODE_ROW - 9.0),
        );
        frame.fill_rectangle(
            Point::new(row_rect.x, row_rect.y),
            row_rect.size(),
            row_colors[index],
        );
        frame.fill_text(canvas::Text {
            content: label.to_owned(),
            position: Point::new(
                row_rect.x + row_rect.width * 0.5,
                row_rect.y + row_rect.height * 0.5,
            ),
            max_width: row_rect.width - 16.0,
            color: Color::from_rgb8(232, 235, 236),
            size: Pixels(10.5),
            line_height: LineHeight::Relative(1.0),
            font: Font::default(),
            align_x: TextAlignment::Center,
            align_y: iced::alignment::Vertical::Center,
            shaping: Shaping::Basic,
        });
        if kind != 1 || index == 0 || index == 1 || index == 2 {
            draw_stereo_ports(frame, Point::new(rect.x + 3.0, row_center(rect, index)));
            draw_stereo_ports(
                frame,
                Point::new(rect.x + rect.width - 3.0, row_center(rect, index)),
            );
        }
    }
}

fn draw_stereo_ports(frame: &mut canvas::Frame<iced::Renderer>, center: Point) {
    for offset in [-3.0, 3.0] {
        let port = canvas::Path::circle(Point::new(center.x, center.y + offset), 2.5);
        frame.fill(&port, Color::from_rgb8(224, 228, 229));
    }
}

fn wire_path(start: Point, end: Point, bend_offset: f32) -> canvas::Path {
    let middle_x = (start.x + end.x) * 0.5;
    canvas::Path::new(|path| {
        path.move_to(start);
        path.bezier_curve_to(
            Point::new(middle_x, start.y + bend_offset),
            Point::new(middle_x, end.y + bend_offset),
            end,
        );
    })
}

fn bezier_segments(start: Point, end: Point, bend_offset: f32) -> Vec<Point> {
    let middle_x = (start.x + end.x) * 0.5;
    let control_a = Point::new(middle_x, start.y + bend_offset);
    let control_b = Point::new(middle_x, end.y + bend_offset);
    (0..=16)
        .map(|step| {
            let t = step as f32 / 16.0;
            let inverse = 1.0 - t;
            Point::new(
                inverse.powi(3) * start.x
                    + 3.0 * inverse.powi(2) * t * control_a.x
                    + 3.0 * inverse * t.powi(2) * control_b.x
                    + t.powi(3) * end.x,
                inverse.powi(3) * start.y
                    + 3.0 * inverse.powi(2) * t * control_a.y
                    + 3.0 * inverse * t.powi(2) * control_b.y
                    + t.powi(3) * end.y,
            )
        })
        .collect()
}

fn distance(a: Point, b: Point) -> f32 {
    (a.x - b.x).hypot(a.y - b.y)
}

fn point_segment_distance(point: Point, start: Point, end: Point) -> f32 {
    let delta = Point::new(end.x - start.x, end.y - start.y);
    let length_squared = delta.x * delta.x + delta.y * delta.y;
    if length_squared <= f32::EPSILON {
        return distance(point, start);
    }
    let projection = (((point.x - start.x) * delta.x + (point.y - start.y) * delta.y)
        / length_squared)
        .clamp(0.0, 1.0);
    distance(
        point,
        Point::new(
            start.x + projection * delta.x,
            start.y + projection * delta.y,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use aaadaw_core::{DawAction, Project};

    #[test]
    fn wiring_curve_is_hit_testable_along_its_visible_path() {
        let start = Point::new(30.0, 20.0);
        let end = Point::new(170.0, 120.0);
        let points = bezier_segments(start, end, 0.0);
        assert_eq!(points.first(), Some(&start));
        assert_eq!(points.last(), Some(&end));
        assert!(points.windows(2).any(|segment| point_segment_distance(
            Point::new(100.0, 70.0),
            segment[0],
            segment[1]
        ) < 10.0));
        assert!(points.windows(2).all(|segment| point_segment_distance(
            Point::new(100.0, 200.0),
            segment[0],
            segment[1]
        ) > 10.0));
    }

    #[test]
    fn wiring_graph_has_room_for_nodes_and_device_outputs_without_tracks() {
        assert!(graph_width(0) >= 1000.0);
        assert!(graph_width(4) > graph_width(1));
    }

    #[test]
    fn dragging_a_track_output_to_another_input_publishes_the_existing_route_action() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Source".into(),
            })
            .unwrap();
        project
            .apply(DawAction::CreateTrack {
                index: 1,
                name: "Destination".into(),
            })
            .unwrap();
        let source = project.tracks()[0].id();
        let destination = project.tracks()[1].id();
        let program = WiringProgram {
            project: &project,
            graph_busy: false,
        };
        let mut state = WiringState::default();
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(1400.0, 430.0));
        let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let press_action = canvas::Program::update(
            &program,
            &mut state,
            &press,
            bounds,
            mouse::Cursor::Available(WiringProgram::output_port(program.track_rect(0), 0, 0.0)),
        )
        .expect("track output port should start a wire drag");
        assert_eq!(press_action.into_inner().2, iced::event::Status::Captured);

        let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        let release_action = canvas::Program::update(
            &program,
            &mut state,
            &release,
            bounds,
            mouse::Cursor::Available(WiringProgram::input_port(program.track_rect(1), 0, 0.0)),
        )
        .expect("valid destination port should finish the wire drag");
        let (message, _, status) = release_action.into_inner();
        assert_eq!(status, iced::event::Status::Captured);
        assert!(matches!(
            message,
            Some(Message::SetRoutingMatrixSend(actual_source, actual_destination))
                if actual_source == source && actual_destination == destination
        ));
    }

    #[test]
    fn clicking_an_audio_send_wire_opens_its_source_destination_controls() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Source".into(),
            })
            .unwrap();
        project
            .apply(DawAction::CreateTrack {
                index: 1,
                name: "Destination".into(),
            })
            .unwrap();
        let source = project.tracks()[0].id();
        let destination = project.tracks()[1].id();
        project
            .apply(DawAction::CreateAudioSend {
                track_id: source,
                destination,
                parameters: Default::default(),
            })
            .unwrap();
        let program = WiringProgram {
            project: &project,
            graph_busy: false,
        };
        let wire = program
            .wires()
            .into_iter()
            .find(|wire| wire.destination == Some(destination))
            .unwrap();
        let position = bezier_segments(wire.start, wire.end, wire.bend_offset)[8];
        let mut state = WiringState::default();
        let event = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let action = canvas::Program::update(
            &program,
            &mut state,
            &event,
            Rectangle::new(Point::ORIGIN, Size::new(1400.0, 430.0)),
            mouse::Cursor::Available(position),
        )
        .expect("visible send wire should be interactive");
        let (message, _, status) = action.into_inner();
        assert_eq!(status, iced::event::Status::Ignored);
        assert!(matches!(
            message,
            Some(Message::OpenTrackRoutingTo(actual_source, actual_destination))
                if actual_source == source && actual_destination == destination
        ));
    }

    #[test]
    fn clicking_main_or_hardware_output_wire_opens_track_routing_controls() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Source".into(),
            })
            .unwrap();
        let source = project.tracks()[0].id();
        project
            .apply(DawAction::CreateAudioHardwareOutput {
                track_id: source,
                parameters: Default::default(),
            })
            .unwrap();
        let program = WiringProgram {
            project: &project,
            graph_busy: false,
        };
        for row in [1, 2] {
            let wire = program
                .wires()
                .into_iter()
                .find(|wire| {
                    wire.source == Some(source)
                        && wire.destination.is_none()
                        && (wire.start.y - row_center(program.track_rect(0), row)).abs() < 0.1
                })
                .unwrap_or_else(|| {
                    panic!(
                        "missing row {row} route: sends={}, hardware={}, main_send={}, wires={:?}",
                        project.tracks()[0].sends().len(),
                        project.tracks()[0].hardware_outputs().len(),
                        project.tracks()[0].main_send_enabled(),
                        program.wires()
                    )
                });
            let position = bezier_segments(wire.start, wire.end, wire.bend_offset)[8];
            let mut state = WiringState::default();
            let action = canvas::Program::update(
                &program,
                &mut state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Rectangle::new(Point::ORIGIN, Size::new(1600.0, 430.0)),
                mouse::Cursor::Available(position),
            )
            .expect("visible main and Hardware Output paths should be interactive");
            let (message, _, status) = action.into_inner();
            assert_eq!(status, iced::event::Status::Ignored);
            assert!(matches!(message, Some(Message::OpenTrackRouting(id)) if id == source));
        }
    }
}
