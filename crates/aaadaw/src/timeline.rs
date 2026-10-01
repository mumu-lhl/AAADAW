mod renderer;

use aaadaw_core::{ItemId, Project, TrackId};
use iced::advanced::text::{Alignment as TextAlignment, LineHeight, Shaping};
use iced::widget::canvas;
use iced::widget::canvas::Text;
use iced::widget::pane_grid::{self, Axis, Split};
use iced::widget::shader;
use iced::{Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Theme, keyboard, mouse};
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) const TIMELINE_ROW_HEIGHT: f32 = 64.0;
pub(crate) const TIMELINE_RULER_HEIGHT: f32 = 32.0;
pub(crate) const TCP_SCROLL_ID: &str = "aaadaw-tcp-scroll";
pub(crate) const TIMELINE_SCROLL_ID: &str = "aaadaw-timeline-scroll";

const MIN_PIXELS_PER_TICK: f32 = 0.002;
const MAX_PIXELS_PER_TICK: f32 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArrangementPane {
    TrackControls,
    Timeline,
}

#[derive(Debug, Clone)]
pub(crate) enum TimelineEvent {
    PanByPixels(f32),
    ZoomAt { factor: f32, anchor_x: f32 },
    SetEditCursor(u64),
    SelectItem(Option<ItemId>),
    SelectTrack(TrackId),
    OpenTrackContextMenu(TrackId),
    ToggleTrackContextMenu(TrackId),
    CloseTrackContextMenu,
    ResizeSplit { split: Split, ratio: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemKind {
    Audio,
    Midi,
}

#[derive(Debug, Clone)]
struct TimelineItem {
    id: ItemId,
    track_id: TrackId,
    track_index: usize,
    start_tick: u64,
    end_tick: u64,
    kind: ItemKind,
    label: String,
}

#[derive(Debug, Default, Clone)]
struct TimelineCache {
    generation: u64,
    items: Arc<[TimelineItem]>,
    track_ids: Arc<[TrackId]>,
}

impl TimelineCache {
    fn rebuild(&mut self, project: &Project) {
        let track_ids = project
            .tracks()
            .iter()
            .map(|track| track.id())
            .collect::<Vec<_>>();
        let track_indices = track_ids
            .iter()
            .enumerate()
            .map(|(index, track_id)| (*track_id, index))
            .collect::<HashMap<_, _>>();
        let mut items =
            Vec::with_capacity(project.audio_items().len() + project.midi_items().len());

        for item in project.audio_items() {
            let Some(&track_index) = track_indices.get(&item.track_id()) else {
                continue;
            };
            let (Ok(start_tick), Ok(end_tick)) = (
                project.tick_at_sample(item.start_sample()),
                project.tick_at_sample(item.end_sample()),
            ) else {
                continue;
            };
            items.push(TimelineItem {
                id: item.id(),
                track_id: item.track_id(),
                track_index,
                start_tick,
                end_tick: end_tick.max(start_tick),
                kind: ItemKind::Audio,
                label: media_label(item.media_ref()),
            });
        }

        for item in project.midi_items() {
            let Some(&track_index) = track_indices.get(&item.track_id()) else {
                continue;
            };
            items.push(TimelineItem {
                id: item.id(),
                track_id: item.track_id(),
                track_index,
                start_tick: item.start_tick(),
                end_tick: item.start_tick().saturating_add(item.length_ticks()),
                kind: ItemKind::Midi,
                label: "MIDI".to_owned(),
            });
        }

        items.sort_by_key(|item| (item.track_index, item.start_tick, item.id.value()));
        self.generation = self.generation.wrapping_add(1);
        self.items = items.into();
        self.track_ids = track_ids.into();
    }

    fn item_at(&self, track_index: usize, tick: u64) -> Option<&TimelineItem> {
        self.items.iter().rev().find(|item| {
            item.track_index == track_index && item.start_tick <= tick && tick < item.end_tick
        })
    }
}

/// UI-only Arrangement state and the project-derived data uploaded to its GPU viewport.
pub(crate) struct TimelineState {
    pub(crate) panes: pane_grid::State<ArrangementPane>,
    pub(crate) split: Split,
    pub(crate) vertical_scroll: f32,
    pub(crate) viewport_height: f32,
    pub(crate) selected_track: Option<TrackId>,
    pub(crate) selected_item: Option<ItemId>,
    pub(crate) context_track: Option<TrackId>,
    pub(crate) edit_cursor_tick: u64,
    pub(crate) origin_tick: u64,
    pub(crate) pixels_per_tick: f32,
    cache: TimelineCache,
    pan_fractional_tick: f64,
}

impl Default for TimelineState {
    fn default() -> Self {
        let (mut panes, track_pane) = pane_grid::State::new(ArrangementPane::TrackControls);
        let (_timeline_pane, split) = panes
            .split(Axis::Vertical, track_pane, ArrangementPane::Timeline)
            .expect("a fresh pane grid can be split");
        panes.resize(split, 0.28);
        Self {
            panes,
            split,
            vertical_scroll: 0.0,
            viewport_height: 480.0,
            selected_track: None,
            selected_item: None,
            context_track: None,
            edit_cursor_tick: 0,
            origin_tick: 0,
            pixels_per_tick: 0.065,
            cache: TimelineCache::default(),
            pan_fractional_tick: 0.0,
        }
    }
}

impl TimelineState {
    pub(crate) fn rebuild(&mut self, project: &Project) {
        self.cache.rebuild(project);
        if self
            .selected_track
            .is_some_and(|track_id| !self.cache.track_ids.contains(&track_id))
        {
            self.selected_track = None;
        }
        if self
            .selected_item
            .is_some_and(|item_id| !self.cache.items.iter().any(|item| item.id == item_id))
        {
            self.selected_item = None;
        }
        if self
            .context_track
            .is_some_and(|track_id| !self.cache.track_ids.contains(&track_id))
        {
            self.context_track = None;
        }
    }

    pub(crate) fn handle(&mut self, event: TimelineEvent) {
        match event {
            TimelineEvent::PanByPixels(delta_x) => {
                let delta = -(f64::from(delta_x) / f64::from(self.pixels_per_tick))
                    + self.pan_fractional_tick;
                let whole_ticks = delta.trunc();
                self.pan_fractional_tick = delta - whole_ticks;
                self.origin_tick = (i128::from(self.origin_tick) + whole_ticks as i128)
                    .clamp(0, i128::from(u64::MAX)) as u64;
            }
            TimelineEvent::ZoomAt { factor, anchor_x } => {
                if !factor.is_finite() || factor <= 0.0 {
                    return;
                }
                let anchor_tick = self.origin_tick as f64
                    + f64::from(anchor_x.max(0.0)) / f64::from(self.pixels_per_tick);
                self.pixels_per_tick =
                    (self.pixels_per_tick * factor).clamp(MIN_PIXELS_PER_TICK, MAX_PIXELS_PER_TICK);
                let new_origin =
                    anchor_tick - f64::from(anchor_x.max(0.0)) / f64::from(self.pixels_per_tick);
                self.origin_tick = new_origin.clamp(0.0, u64::MAX as f64).round() as u64;
                self.pan_fractional_tick = 0.0;
            }
            TimelineEvent::SetEditCursor(tick) => self.edit_cursor_tick = tick,
            TimelineEvent::SelectItem(item_id) => {
                self.selected_item = item_id;
                if let Some(item_id) = item_id {
                    self.selected_track = self
                        .cache
                        .items
                        .iter()
                        .find(|item| item.id == item_id)
                        .map(|item| item.track_id);
                }
            }
            TimelineEvent::SelectTrack(track_id) => self.selected_track = Some(track_id),
            TimelineEvent::OpenTrackContextMenu(track_id) => {
                if self.cache.track_ids.contains(&track_id) {
                    self.selected_track = Some(track_id);
                    self.context_track = Some(track_id);
                }
            }
            TimelineEvent::ToggleTrackContextMenu(track_id) => {
                if self.cache.track_ids.contains(&track_id) {
                    if self.context_track == Some(track_id) {
                        self.context_track = None;
                    } else {
                        self.selected_track = Some(track_id);
                        self.context_track = Some(track_id);
                    }
                }
            }
            TimelineEvent::CloseTrackContextMenu => self.context_track = None,
            TimelineEvent::ResizeSplit { split, ratio } if split == self.split => {
                self.panes.resize(split, ratio.clamp(0.22, 0.58));
            }
            TimelineEvent::ResizeSplit { .. } => {}
        }
    }

    fn program<'a>(
        &'a self,
        project: &'a Project,
        playhead_sample: Option<u64>,
    ) -> TimelineProgram<'a> {
        let playhead_tick = playhead_sample.and_then(|sample| project.tick_at_sample(sample).ok());
        TimelineProgram {
            project,
            cache: &self.cache,
            origin_tick: self.origin_tick,
            pixels_per_tick: self.pixels_per_tick,
            edit_cursor_tick: self.edit_cursor_tick,
            playhead_tick,
            selected_item: self.selected_item,
            selected_track: self.selected_track,
        }
    }

    pub(crate) fn ruler_lines(&self, project: &Project, width: f32) -> Vec<RulerLine> {
        ruler_lines(project, self.origin_tick, self.pixels_per_tick, width)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RulerLine {
    pub(crate) tick: u64,
    pub(crate) x: f32,
    pub(crate) measure: u64,
    pub(crate) is_measure: bool,
    pub(crate) show_label: bool,
}

struct TimelineProgram<'a> {
    project: &'a Project,
    cache: &'a TimelineCache,
    origin_tick: u64,
    pixels_per_tick: f32,
    edit_cursor_tick: u64,
    playhead_tick: Option<u64>,
    selected_item: Option<ItemId>,
    selected_track: Option<TrackId>,
}

#[derive(Default)]
struct TimelineInteractionState {
    modifiers: keyboard::Modifiers,
    pan_last_x: Option<f32>,
}

impl shader::Program<crate::app::Message> for TimelineProgram<'_> {
    type State = TimelineInteractionState;
    type Primitive = renderer::TimelinePrimitive;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<shader::Action<crate::app::Message>> {
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.modifiers = *modifiers;
            return None;
        }

        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let Some(position) = cursor.position_in(bounds) else {
                    return None;
                };
                let (x, y) = match delta {
                    mouse::ScrollDelta::Lines { x, y } => (*x * 42.0, *y),
                    mouse::ScrollDelta::Pixels { x, y } => (*x, *y / 80.0),
                };
                if state.modifiers.alt() && y != 0.0 {
                    Some(
                        shader::Action::publish(crate::app::Message::Timeline(
                            TimelineEvent::ZoomAt {
                                factor: 1.12_f32.powf(y),
                                anchor_x: position.x,
                            },
                        ))
                        .and_capture(),
                    )
                } else if x != 0.0 || (state.modifiers.shift() && y != 0.0) {
                    let delta_x = if x != 0.0 { x } else { y * 42.0 };
                    Some(
                        shader::Action::publish(crate::app::Message::Timeline(
                            TimelineEvent::PanByPixels(delta_x),
                        ))
                        .and_capture(),
                    )
                } else {
                    None
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => {
                if let Some(position) = cursor.position_in(bounds) {
                    state.pan_last_x = Some(position.x);
                    Some(shader::Action::capture())
                } else {
                    None
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle)) => {
                if state.pan_last_x.take().is_some() {
                    Some(shader::Action::capture())
                } else {
                    None
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(last_x) = state.pan_last_x {
                    let local_x = position.x - bounds.x;
                    state.pan_last_x = Some(local_x);
                    let delta_x = local_x - last_x;
                    Some(
                        shader::Action::publish(crate::app::Message::Timeline(
                            TimelineEvent::PanByPixels(delta_x),
                        ))
                        .and_capture(),
                    )
                } else {
                    None
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(position) = cursor.position_in(bounds) else {
                    return None;
                };
                let tick = tick_at_x(self.origin_tick, self.pixels_per_tick, position.x);
                let track_index = (position.y / TIMELINE_ROW_HEIGHT).floor() as usize;
                let hit = self.cache.item_at(track_index, tick);
                let event = if let Some(item) = hit {
                    TimelineEvent::SelectItem(Some(item.id))
                } else {
                    TimelineEvent::SetEditCursor(tick)
                };
                Some(shader::Action::publish(crate::app::Message::Timeline(event)).and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> Self::Primitive {
        let track_index = self
            .selected_track
            .and_then(|track_id| self.cache.track_ids.iter().position(|id| *id == track_id))
            .map_or(u32::MAX, |index| index as u32);
        renderer::TimelinePrimitive {
            generation: self.cache.generation,
            items: Arc::clone(&self.cache.items),
            track_count: self.cache.track_ids.len() as u32,
            row_height: TIMELINE_ROW_HEIGHT,
            origin_tick: self.origin_tick,
            pixels_per_tick: self.pixels_per_tick,
            edit_cursor_tick: self.edit_cursor_tick,
            playhead_tick: self.playhead_tick,
            selected_item: self.selected_item,
            selected_track_index: track_index,
            width: bounds.width,
            height: bounds.height,
            grid_lines: ruler_lines(
                self.project,
                self.origin_tick,
                self.pixels_per_tick,
                bounds.width,
            )
            .into_iter()
            .map(|line| (line.tick, line.is_measure))
            .collect(),
        }
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if let Some(position) = cursor.position_in(bounds) {
            let tick = tick_at_x(self.origin_tick, self.pixels_per_tick, position.x);
            let row_index = (position.y / TIMELINE_ROW_HEIGHT).floor() as usize;
            if self.cache.item_at(row_index, tick).is_some() {
                return mouse::Interaction::Pointer;
            }
            return mouse::Interaction::Crosshair;
        }
        mouse::Interaction::default()
    }
}

fn ruler_lines(
    project: &Project,
    origin_tick: u64,
    pixels_per_tick: f32,
    width: f32,
) -> Vec<RulerLine> {
    let visible_ticks = (f64::from(width.max(0.0)) / f64::from(pixels_per_tick))
        .ceil()
        .min(u64::MAX as f64) as u64;
    let end_tick = origin_tick.saturating_add(visible_ticks);
    let position = project
        .musical_position_at_tick(origin_tick)
        .unwrap_or_else(|_| {
            project
                .musical_position_at_tick(0)
                .expect("tick zero is valid")
        });
    let mut tick = origin_tick.saturating_sub(position.tick_in_beat());
    let pixels_per_quarter = pixels_per_tick * project.settings().ppq() as f32;
    let labels_per_measure = if pixels_per_quarter < 24.0 {
        (24.0 / pixels_per_quarter.max(0.001)).ceil() as u64
    } else {
        1
    };
    let mut lines = Vec::new();
    let mut steps = 0usize;
    while tick <= end_tick && steps < 2_000 {
        steps += 1;
        let Ok(position) = project.musical_position_at_tick(tick) else {
            break;
        };
        let is_measure = position.beat() == 1 && position.tick_in_beat() == 0;
        let signature = project.time_signature_at_tick(tick);
        let ticks_per_beat = u64::from(project.settings().ppq()).saturating_mul(4)
            / u64::from(signature.denominator());
        if ticks_per_beat == 0 {
            break;
        }
        let beat_width = pixels_per_tick * ticks_per_beat as f32;
        let show_beat = is_measure || beat_width >= 18.0;
        let show_label =
            is_measure && (position.measure().saturating_sub(1) % labels_per_measure == 0);
        if tick >= origin_tick && (show_beat || show_label) {
            let x = (tick - origin_tick) as f32 * pixels_per_tick;
            lines.push(RulerLine {
                tick,
                x,
                measure: position.measure(),
                is_measure,
                show_label,
            });
        }
        let until_next_beat = ticks_per_beat.saturating_sub(position.tick_in_beat());
        tick = tick.saturating_add(until_next_beat.max(1));
        if tick == u64::MAX {
            break;
        }
    }
    lines
}

pub(crate) fn timeline_widget<'a>(
    state: &'a TimelineState,
    project: &'a Project,
    playhead_sample: Option<u64>,
) -> Element<'a, crate::app::Message> {
    shader::Shader::new(state.program(project, playhead_sample))
        .width(Length::Fill)
        .height(Length::Fixed(
            state.cache.track_ids.len() as f32 * TIMELINE_ROW_HEIGHT,
        ))
        .into()
}

pub(crate) fn ruler_widget<'a>(
    state: &'a TimelineState,
    project: &'a Project,
) -> Element<'a, crate::app::Message> {
    canvas::Canvas::new(RulerProgram { state, project })
        .width(Length::Fill)
        .height(Length::Fixed(TIMELINE_RULER_HEIGHT))
        .into()
}

pub(crate) fn item_labels_widget<'a>(state: &'a TimelineState) -> Element<'a, crate::app::Message> {
    canvas::Canvas::new(ItemLabelsProgram { state })
        .width(Length::Fill)
        .height(Length::Fixed(
            state.cache.track_ids.len() as f32 * TIMELINE_ROW_HEIGHT,
        ))
        .into()
}

struct RulerProgram<'a> {
    state: &'a TimelineState,
    project: &'a Project,
}

impl canvas::Program<crate::app::Message> for RulerProgram<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), Color::from_rgb8(29, 33, 36));
        for line in self.state.ruler_lines(self.project, bounds.width) {
            let line_color = if line.is_measure {
                Color::from_rgb8(101, 110, 116)
            } else {
                Color::from_rgb8(63, 70, 75)
            };
            let height = if line.is_measure { bounds.height } else { 10.0 };
            let path = canvas::Path::line(
                Point::new(line.x, bounds.height - height),
                Point::new(line.x, bounds.height),
            );
            frame.stroke(&path, canvas::Stroke::default().with_color(line_color));
            if line.show_label {
                frame.fill_text(Text {
                    content: line.measure.to_string(),
                    position: Point::new(line.x + 4.0, bounds.height / 2.0),
                    max_width: 36.0,
                    color: Color::from_rgb8(190, 197, 201),
                    size: Pixels(11.0),
                    line_height: LineHeight::Relative(1.0),
                    font: Font::default(),
                    align_x: TextAlignment::Left,
                    align_y: iced::alignment::Vertical::Center,
                    shaping: Shaping::Basic,
                });
            }
        }
        vec![frame.into_geometry()]
    }
}

struct ItemLabelsProgram<'a> {
    state: &'a TimelineState,
}

impl canvas::Program<crate::app::Message> for ItemLabelsProgram<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let first_visible_track =
            (self.state.vertical_scroll / TIMELINE_ROW_HEIGHT).floor() as usize;
        let end_visible_track = ((self.state.vertical_scroll + self.state.viewport_height)
            / TIMELINE_ROW_HEIGHT)
            .ceil() as usize
            + 1;
        for item in self.state.cache.items.iter() {
            if item.track_index < first_visible_track || item.track_index >= end_visible_track {
                continue;
            }
            let left = (i128::from(item.start_tick) - i128::from(self.state.origin_tick)) as f64
                * f64::from(self.state.pixels_per_tick);
            let right = (i128::from(item.end_tick) - i128::from(self.state.origin_tick)) as f64
                * f64::from(self.state.pixels_per_tick);
            let width = right - left;
            if right < 0.0 || left > f64::from(bounds.width) || width < 54.0 {
                continue;
            }
            frame.fill_text(Text {
                content: item.label.clone(),
                position: Point::new(
                    (left.max(0.0) + 5.0) as f32,
                    item.track_index as f32 * TIMELINE_ROW_HEIGHT + TIMELINE_ROW_HEIGHT / 2.0,
                ),
                max_width: (width - 10.0).min(220.0) as f32,
                color: Color::from_rgb8(229, 233, 235),
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

fn tick_at_x(origin_tick: u64, pixels_per_tick: f32, x: f32) -> u64 {
    (origin_tick as f64 + f64::from(x.max(0.0)) / f64::from(pixels_per_tick))
        .clamp(0.0, u64::MAX as f64)
        .round() as u64
}

fn media_label(media_ref: &str) -> String {
    let file_name = std::path::Path::new(media_ref)
        .file_name()
        .map_or(media_ref, |name| name.to_str().unwrap_or(media_ref));
    format!("Audio · {file_name}")
}

#[cfg(test)]
mod tests {
    use super::{ItemKind, TimelineCache, TimelineEvent, TimelineState, tick_at_x};
    use aaadaw_core::{DawAction, Project, TimeSignature};

    #[test]
    fn cache_maps_audio_and_midi_to_shared_track_rows_and_musical_ticks() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Audio".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::CreateTrack {
                index: 1,
                name: "MIDI".to_owned(),
            })
            .unwrap();
        let tracks = project.tracks();
        let audio_track = tracks[0].id();
        let midi_track = tracks[1].id();
        let audio_start = project.sample_at_tick(960).unwrap();
        let audio_end = project.sample_at_tick(1920).unwrap();
        project
            .apply(DawAction::InsertAudioItem {
                track_id: audio_track,
                media_ref: "asset://Drums/kick.wav".to_owned(),
                start_sample: audio_start,
                source_offset_samples: 0,
                length_samples: audio_end - audio_start,
            })
            .unwrap();
        project
            .apply(DawAction::InsertMidiItem {
                track_id: midi_track,
                start_tick: 1920,
                length_ticks: 3840,
            })
            .unwrap();

        let mut cache = TimelineCache::default();
        cache.rebuild(&project);
        assert_eq!(cache.items.len(), 2);
        assert_eq!(cache.items[0].track_index, 0);
        assert_eq!(cache.items[0].start_tick, 960);
        assert_eq!(cache.items[0].end_tick, 1920);
        assert!(matches!(cache.items[0].kind, ItemKind::Audio));
        assert_eq!(cache.items[0].label, "Audio · kick.wav");
        assert_eq!(cache.items[1].track_index, 1);
        assert_eq!(cache.items[1].start_tick, 1920);
        assert_eq!(cache.items[1].end_tick, 5760);
        assert!(matches!(cache.items[1].kind, ItemKind::Midi));
    }

    #[test]
    fn meter_aware_ruler_uses_new_beat_lengths_at_meter_changes() {
        let mut project = Project::new();
        project
            .apply(DawAction::SetTimeSignature {
                start_tick: 3840,
                signature: TimeSignature::new(7, 8).unwrap(),
            })
            .unwrap();
        let mut timeline = TimelineState::default();
        timeline.origin_tick = 3840;
        timeline.pixels_per_tick = 0.1;
        let lines = timeline.ruler_lines(&project, 400.0);
        assert_eq!(lines[0].measure, 2);
        assert!(lines[0].is_measure);
        assert_eq!(lines.iter().filter(|line| line.is_measure).count(), 2);
        let seven_eighth_beat = lines
            .iter()
            .filter(|line| !line.is_measure)
            .map(|line| line.tick)
            .collect::<Vec<_>>();
        assert!(seven_eighth_beat.contains(&4320));
        assert!(seven_eighth_beat.contains(&4800));
    }

    #[test]
    fn zoom_keeps_the_tick_under_the_pointer_stationary() {
        let mut timeline = TimelineState::default();
        let anchor_x = 300.0;
        let before = tick_at_x(timeline.origin_tick, timeline.pixels_per_tick, anchor_x);
        timeline.handle(TimelineEvent::ZoomAt {
            factor: 1.5,
            anchor_x,
        });
        let after = tick_at_x(timeline.origin_tick, timeline.pixels_per_tick, anchor_x);
        assert!(before.abs_diff(after) <= 1);
    }
}
