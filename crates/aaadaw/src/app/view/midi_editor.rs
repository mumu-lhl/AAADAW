use super::super::{
    App, MIDI_EDITOR_KEY_WIDTH, Message, MidiEditorChannelSelection, MidiEditorControllerFilter,
    MidiEditorEventKind, MidiEditorEventListCursor, MidiEditorEventTypeFilter,
    MidiEditorFilterRepeat, MidiEditorFilterState, MidiEditorLane, MidiEditorMode,
    MidiEditorNoteFilter, MidiEditorNoteFilterEvent, MidiEditorPitchBendFilter,
    MidiEditorPitchFilterValue, MidiEditorPositionFilter, MidiEditorProgramChangeFilter,
    MidiEditorTool, effective_midi_bank_value, midi_editor_filter_repeat_position_value,
    midi_editor_measure_length_ticks,
};
use super::tokens::{PANEL_PADDING, ROW_GAP, SPACING_LG, SPACING_MD, SPACING_XS, TOUCH_TARGET_MIN};
use crate::timeline::{SnapGrid, TimelineEvent, snap_tick_to_grid};
use aaadaw_core::{
    ItemId, MidiChannelPressureData, MidiControllerData, MidiEventOrderTarget, MidiItem, MidiNote,
    MidiNoteData, MidiOpaqueEventData, MidiOpaqueEventKind, MidiPitchBendData,
    MidiPolyAftertouchData, MidiProgramChangeData, NoteId, Project, TimeSignature,
};
use iced::advanced::text::{Alignment as TextAlignment, LineHeight, Shaping};
use iced::widget::canvas::{self, Text};
use iced::widget::{
    button, canvas as canvas_widget, checkbox, column, container, mouse_area, pick_list, row,
    scrollable, slider, stack, text, text_editor, text_input,
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
const MAX_MIDI_EDITOR_LOOP_OCCURRENCES_PER_EVENT: usize = 4_096;
const MIDI_EDITOR_FILTER_VALUES: [u8; 128] = {
    let mut values = [0; 128];
    let mut value = 0;
    while value < values.len() {
        values[value] = value as u8;
        value += 1;
    }
    values
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MidiEditorControllerFilterValue(u8);

impl MidiEditorControllerFilterValue {
    const OPTIONS: [Self; 128] = {
        let mut options = [Self(0); 128];
        let mut controller = 0;
        while controller < options.len() {
            options[controller] = Self(controller as u8);
            controller += 1;
        }
        options
    };
}

impl std::fmt::Display for MidiEditorControllerFilterValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&midi_event_controller_property_label(self.0))
    }
}

fn source_tick_is_visible(item: &MidiItem, tick: u64) -> bool {
    let start_tick = item.source_offset_ticks();
    let end_tick = start_tick.saturating_add(item.length_ticks());
    source_event_occurrences(item, tick, start_tick, end_tick)
        .next()
        .is_some()
}

/// Returns unwrapped source positions of event instances visible in the item and tick window.
///
/// MIDI Editor note rows use the source clock so that editing one displayed loop instance edits
/// the shared source event. The unwrapped positions retain the loop iteration for layout while
/// remaining compatible with source-relative drag deltas.
fn source_event_occurrences(
    item: &MidiItem,
    event_tick: u64,
    visible_start: u64,
    visible_end: u64,
) -> impl Iterator<Item = u64> {
    let item_start = item.source_offset_ticks();
    let item_end = item_start.saturating_add(item.length_ticks());
    let range_start = item_start.max(visible_start);
    let range_end = item_end.min(visible_end);
    let (first_tick, interval) = if item.is_loop_source() {
        let loop_length = item.source_loop_length_ticks();
        if loop_length == 0 {
            (None, None)
        } else {
            let event_phase = event_tick % loop_length;
            let offset_phase = item_start % loop_length;
            let phase = if event_phase >= offset_phase {
                event_phase - offset_phase
            } else {
                loop_length - (offset_phase - event_phase)
            };
            let first = item_start.checked_add(phase).and_then(|first| {
                if first >= range_start {
                    Some(first)
                } else {
                    let distance = range_start - first;
                    let iterations = distance.div_ceil(loop_length);
                    iterations
                        .checked_mul(loop_length)
                        .and_then(|advance| first.checked_add(advance))
                }
            });
            (first, Some(loop_length))
        }
    } else if event_tick >= range_start {
        (Some(event_tick), None)
    } else {
        (None, None)
    };
    let mut next_tick = first_tick;
    std::iter::from_fn(move || {
        let tick = next_tick.filter(|tick| *tick < range_end)?;
        next_tick = interval.and_then(|interval| tick.checked_add(interval));
        Some(tick)
    })
    .take(MAX_MIDI_EDITOR_LOOP_OCCURRENCES_PER_EVENT)
}

fn visible_note_duration(
    item: &MidiItem,
    note: &aaadaw_core::MidiNote,
    occurrence_tick: u64,
) -> u64 {
    let clip_end = item
        .source_offset_ticks()
        .saturating_add(item.length_ticks());
    source_note_duration(item, note.tick(), note.duration())
        .min(clip_end.saturating_sub(occurrence_tick))
}

fn source_note_duration(item: &MidiItem, source_tick: u64, duration: u64) -> u64 {
    let mut duration = duration;
    if item.is_loop_source() {
        let loop_length = item.source_loop_length_ticks();
        if loop_length > 0 {
            let phase = source_tick % loop_length;
            duration = duration.min(loop_length - phase);
        }
    }
    duration
}

fn midi_note_insert_data(
    item: &MidiItem,
    local_tick: u64,
    pitch: u8,
    requested_duration: u64,
    velocity: u8,
    channel: u8,
) -> Option<MidiNoteData> {
    if local_tick >= item.length_ticks() {
        return None;
    }
    let display_tick = item.source_offset_ticks().checked_add(local_tick)?;
    let tick = source_edit_tick(item, display_tick);
    let duration = source_note_duration(
        item,
        tick,
        requested_duration.min(item.length_ticks() - local_tick),
    );
    let (_, source_end) = midi_source_edit_bounds(item);
    if duration == 0
        || tick
            .checked_add(duration)
            .is_none_or(|end| end > source_end)
    {
        return None;
    }
    Some(MidiNoteData {
        channel: channel.min(15),
        pitch,
        tick,
        duration,
        velocity,
        off_velocity: 0,
    })
}

fn midi_source_edit_bounds(item: &MidiItem) -> (u64, u64) {
    if item.is_loop_source() {
        (0, item.source_loop_length_ticks().max(1))
    } else {
        let start = item.source_offset_ticks();
        (start, start.saturating_add(item.length_ticks()))
    }
}

fn source_loop_iteration(item: &MidiItem, event_tick: u64, occurrence_tick: u64) -> u64 {
    if !item.is_loop_source() {
        return 0;
    }
    let loop_length = item.source_loop_length_ticks();
    if loop_length == 0 {
        return 0;
    }
    occurrence_tick.saturating_sub(event_tick % loop_length) / loop_length
}

fn occurrence_source_tick(item: &MidiItem, event_tick: u64, iteration: u64) -> u64 {
    if !item.is_loop_source() {
        return event_tick;
    }
    let loop_length = item.source_loop_length_ticks();
    if loop_length == 0 {
        return event_tick;
    }
    (event_tick % loop_length).saturating_add(iteration.saturating_mul(loop_length))
}

fn source_edit_tick(item: &MidiItem, display_tick: u64) -> u64 {
    if item.is_loop_source() && item.source_loop_length_ticks() > 0 {
        display_tick % item.source_loop_length_ticks()
    } else {
        display_tick
    }
}

fn visible_edit_cursor_tick(app: &App) -> u64 {
    app.midi_editor_paste_target_tick(app.midi_editor_item_id)
}

fn midi_snap_for_app(app: &App) -> MidiSnap {
    let transpose_preview = app.midi_transpose_dialog.as_ref().and_then(|dialog| {
        let semitones = dialog.semitones.trim().parse::<i8>().ok()?;
        let item = app
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == dialog.item_id)?;
        let range_valid = item
            .notes()
            .iter()
            .filter(|note| {
                dialog.scope == super::super::MidiTransposeScope::AllNotes
                    || app.midi_editor_selected_notes.contains(&note.id())
            })
            .all(|note| note.pitch().checked_add_signed(semitones).is_some());
        Some(MidiTransposePreview {
            item_id: dialog.item_id,
            scope: dialog.scope,
            semitones,
            range_valid,
        })
    });
    let ppq = app.project.settings().ppq();
    let measure_ticks = app
        .midi_editor_item_id
        .and_then(|item_id| {
            app.project
                .midi_items()
                .iter()
                .find(|item| item.id() == item_id)
        })
        .map(|item| {
            midi_editor_measure_length_ticks(
                ppq,
                app.project.time_signature_at_tick(item.start_tick()),
            )
        })
        .unwrap_or_else(|| {
            midi_editor_measure_length_ticks(
                ppq,
                TimeSignature::new(4, 4).expect("4/4 is a valid time signature"),
            )
        });
    MidiSnap {
        grid: app.timeline.midi_editor_snap_grid,
        enabled: app.timeline.snap_enabled,
        swing_amount_percent: app.timeline.active_snap_swing_amount_percent(),
        cursor_tick: visible_edit_cursor_tick(app),
        context_menu_epoch: app.midi_expression_context_menu_epoch,
        channel_selection: app.midi_editor_channel_selection,
        draw_channel: app.midi_editor_draw_channel.min(15),
        channel_filter_enabled: app.midi_editor_filter_enabled,
        event_filter_enabled: app.midi_editor_event_filter_enabled,
        filter_channels: app.midi_editor_filter_channels,
        filter_inverted: app.midi_editor_filter_inverted,
        event_type_filter: app.midi_editor_event_type_filter,
        note_filter: app.midi_editor_note_filter,
        controller_filter: app.midi_editor_controller_filter,
        pitch_bend_filter: app.midi_editor_pitch_bend_filter,
        program_change_filter: app.midi_editor_program_change_filter,
        position_filter: app.midi_editor_position_filter,
        ppq,
        measure_ticks,
        transpose_preview,
    }
}

fn midi_event_list_snap_for_app(app: &App) -> MidiSnap {
    let mut snap = midi_snap_for_app(app);
    snap.event_filter_enabled = app.midi_editor_event_list_filter_active;
    snap
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MidiTransposePreview {
    item_id: ItemId,
    scope: super::super::MidiTransposeScope,
    semitones: i8,
    range_valid: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MidiSnap {
    grid: SnapGrid,
    enabled: bool,
    swing_amount_percent: i16,
    cursor_tick: u64,
    context_menu_epoch: u64,
    channel_selection: MidiEditorChannelSelection,
    draw_channel: u8,
    channel_filter_enabled: bool,
    event_filter_enabled: bool,
    filter_channels: u16,
    filter_inverted: bool,
    event_type_filter: MidiEditorEventTypeFilter,
    note_filter: MidiEditorNoteFilter,
    controller_filter: MidiEditorControllerFilter,
    pitch_bend_filter: MidiEditorPitchBendFilter,
    program_change_filter: MidiEditorProgramChangeFilter,
    position_filter: MidiEditorPositionFilter,
    ppq: u32,
    measure_ticks: u64,
    transpose_preview: Option<MidiTransposePreview>,
}

impl Default for MidiSnap {
    fn default() -> Self {
        Self {
            grid: SnapGrid::ThirtySecond,
            enabled: true,
            swing_amount_percent: 0,
            cursor_tick: 0,
            context_menu_epoch: 0,
            channel_selection: MidiEditorChannelSelection::AllChannels,
            draw_channel: 0,
            channel_filter_enabled: false,
            event_filter_enabled: false,
            filter_channels: u16::MAX,
            filter_inverted: false,
            event_type_filter: MidiEditorEventTypeFilter::AllEvents,
            note_filter: MidiEditorNoteFilter::default(),
            controller_filter: MidiEditorControllerFilter::default(),
            pitch_bend_filter: MidiEditorPitchBendFilter::default(),
            program_change_filter: MidiEditorProgramChangeFilter::default(),
            position_filter: MidiEditorPositionFilter::default(),
            ppq: 960,
            measure_ticks: 3_840,
            transpose_preview: None,
        }
    }
}

impl MidiSnap {
    fn event_visible(self, channel: u8, event_kind: MidiEditorEventKind) -> bool {
        self.filter_state().event_visible(Some(channel), event_kind)
    }

    fn filter_state(self) -> MidiEditorFilterState {
        MidiEditorFilterState {
            channel_selection: self.channel_selection,
            channel_filter_enabled: self.channel_filter_enabled,
            event_filter_enabled: self.event_filter_enabled,
            channels: self.filter_channels,
            inverted: self.filter_inverted,
            event_type: self.event_type_filter,
            note: self.note_filter,
            controller: self.controller_filter,
            pitch_bend: self.pitch_bend_filter,
            program_change: self.program_change_filter,
            position: self.position_filter,
        }
    }

    fn note_visible(self, note: &MidiNote) -> bool {
        self.filter_state().note_visible(
            note.channel(),
            MidiEditorNoteFilterEvent {
                pitch: note.pitch(),
                velocity: note.velocity(),
                position_in_measure: midi_editor_filter_repeat_position_value(
                    note.tick(),
                    self.ppq,
                    self.measure_ticks,
                    self.position_filter.repeat_every,
                ),
                length_ticks: note.duration(),
            },
        )
    }

    fn controller_visible(self, event: MidiControllerData) -> bool {
        let position = midi_editor_filter_repeat_position_value(
            event.tick,
            self.ppq,
            self.measure_ticks,
            self.position_filter.repeat_every,
        );
        self.filter_state().controller_visible_at(
            event.channel,
            event.controller,
            event.value,
            position,
        )
    }

    fn pitch_bend_visible(self, event: MidiPitchBendData) -> bool {
        let position = midi_editor_filter_repeat_position_value(
            event.tick,
            self.ppq,
            self.measure_ticks,
            MidiEditorFilterRepeat::Measure,
        );
        self.filter_state()
            .pitch_bend_visible_at(event.channel, event.value, position)
    }

    fn program_change_visible(self, event: MidiProgramChangeData) -> bool {
        let position = midi_editor_filter_repeat_position_value(
            event.tick,
            self.ppq,
            self.measure_ticks,
            self.position_filter.repeat_every,
        );
        self.filter_state()
            .program_change_visible_at(event.channel, event.program, position)
    }

    fn poly_aftertouch_visible(self, channel: u8) -> bool {
        self.event_visible(channel, MidiEditorEventKind::PolyAftertouch)
    }

    fn channel_pressure_visible(self, channel: u8) -> bool {
        self.event_visible(channel, MidiEditorEventKind::ChannelPressure)
    }

    fn opaque_event_visible(self, event: &MidiOpaqueEventData) -> bool {
        let event_kind = match &event.kind {
            MidiOpaqueEventKind::SystemExclusive { .. } => MidiEditorEventKind::SysEx,
            MidiOpaqueEventKind::Meta { .. } => MidiEditorEventKind::Text,
        };
        self.filter_state().event_visible(None, event_kind)
    }

    fn end_row_visible(self, item_end_tick: u64) -> bool {
        let position = midi_editor_filter_repeat_position_value(
            item_end_tick,
            self.ppq,
            self.measure_ticks,
            self.position_filter.repeat_every,
        );
        self.filter_state()
            .controller_visible_at(0, 123, 0, position)
    }

    fn note_pitch(
        self,
        item_id: ItemId,
        selected: &HashSet<aaadaw_core::NoteId>,
        note: &aaadaw_core::MidiNote,
    ) -> u8 {
        let Some(preview) = self.transpose_preview.filter(|preview| {
            preview.item_id == item_id
                && preview.range_valid
                && (preview.scope == super::super::MidiTransposeScope::AllNotes
                    || selected.contains(&note.id()))
        }) else {
            return note.pitch();
        };
        note.pitch()
            .checked_add_signed(preview.semitones)
            .unwrap_or(note.pitch())
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
    let event_snap = midi_snap_for_app(app);
    let visible_note_count = item
        .notes()
        .iter()
        .filter(|note| event_snap.note_visible(note))
        .count();
    let edit_toolbar: Element<'_, Message> = if touch_targets {
        row![
            text(format!("Piano roll · {visible_note_count} notes")).width(Length::Fill),
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
                visible_note_count
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
            button("Quantize…")
                .style(iced::widget::button::secondary)
                .on_press_maybe(
                    app.timeline
                        .midi_editor_has_snap_grid()
                        .then_some(Message::OpenMidiQuantizeDialog(item_id),)
                )
                .padding([SPACING_XS / 2.0, SPACING_XS]),
            button("Humanize…")
                .style(iced::widget::button::secondary)
                .on_press_maybe(
                    (!item.notes().is_empty() && app.timeline.midi_editor_has_snap_grid())
                        .then_some(Message::OpenMidiHumanizeDialog(item_id)),
                )
                .padding([SPACING_XS / 2.0, SPACING_XS]),
            button("Transpose…")
                .style(iced::widget::button::secondary)
                .on_press(Message::OpenMidiTransposeDialog(item_id))
                .padding([SPACING_XS / 2.0, SPACING_XS]),
            button("Q pos")
                .style(iced::widget::button::secondary)
                .on_press_maybe(
                    (!app.midi_editor_selected_notes.is_empty()
                        && app.timeline.midi_editor_has_snap_grid())
                    .then_some(Message::QuantizeSelectedMidiNotes(item_id)),
                )
                .padding([SPACING_XS / 2.0, SPACING_XS]),
            button("Q pos+end")
                .style(iced::widget::button::secondary)
                .on_press_maybe(
                    (!app.midi_editor_selected_notes.is_empty()
                        && app.timeline.midi_editor_has_snap_grid())
                    .then_some(Message::QuantizeSelectedMidiNotesAndEnds(item_id)),
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
                button(if !app.timeline.midi_editor_has_snap_grid() {
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
                        .midi_editor_has_snap_grid()
                        .then_some(Message::Timeline(TimelineEvent::ToggleMidiEditorSnap)),
                ),
                pick_list(
                    &SnapGrid::ALL[..],
                    Some(app.timeline.midi_editor_snap_grid),
                    |grid| Message::Timeline(TimelineEvent::SetMidiEditorSnapGrid(grid)),
                )
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
            text("MIDI grid").size(12),
            button(if !app.timeline.midi_editor_has_snap_grid() {
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
                    .midi_editor_has_snap_grid()
                    .then_some(Message::Timeline(TimelineEvent::ToggleMidiEditorSnap)),
            ),
            pick_list(
                &SnapGrid::ALL[..],
                Some(app.timeline.midi_editor_snap_grid),
                |grid| Message::Timeline(TimelineEvent::SetMidiEditorSnapGrid(grid)),
            )
            .width(Length::Fixed(112.0)),
            text("Wheel: pitch · Shift+wheel: time · Ctrl/Cmd+wheel: zoom").size(11),
        ]
        .spacing(ROW_GAP)
        .align_y(iced::Alignment::Center)
        .into()
    };
    let filter_control: Element<'_, Message> =
        if app.midi_editor_channel_selection == MidiEditorChannelSelection::AllChannels {
            iced::widget::Space::new().into()
        } else {
            checkbox(app.midi_editor_filter_enabled)
                .label("Filter")
                .on_toggle(|_| Message::ToggleMidiEditorChannelFilter)
                .into()
        };
    let channel_toolbar = row![
        text("MIDI channel").size(if touch_targets { 14 } else { 12 }),
        pick_list(
            &MidiEditorChannelSelection::OPTIONS[..],
            Some(app.midi_editor_channel_selection),
            Message::SelectMidiEditorChannel,
        )
        .width(Length::Fixed(if touch_targets { 180.0 } else { 164.0 }))
        .padding(if touch_targets {
            SPACING_LG
        } else {
            SPACING_XS
        }),
        filter_control,
        button("Filter events…").on_press(Message::OpenMidiEditorFilter),
        button("Piano Roll")
            .style(iced::widget::button::primary)
            .on_press(Message::SetMidiEditorMode(MidiEditorMode::PianoRoll)),
        button("Event List")
            .style(iced::widget::button::secondary)
            .on_press(Message::SetMidiEditorMode(MidiEditorMode::EventList)),
    ]
    .spacing(ROW_GAP)
    .align_y(iced::Alignment::Center);
    let channel_filter_dialog = if app.midi_editor_filter_dialog_open {
        Some(channel_filter_dialog(app, touch_targets))
    } else {
        None
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
    let midi_snap = midi_snap_for_app(app);
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
        snap: midi_snap,
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
    let content: Element<'_, Message> = if app.midi_editor_mode == MidiEditorMode::EventList {
        midi_event_list_view(
            app,
            item,
            item_id,
            title,
            touch_targets,
            channel_filter_dialog,
        )
    } else {
        let mut content = column![edit_toolbar];
        if let Some(feedback) = app.midi_editor_feedback.as_deref() {
            content = content.push(text(feedback).size(12));
        }
        content = content.push(navigation_toolbar);
        if touch_targets {
            content = content.push(touch_tool_toolbar);
        }
        let mut content = content.push(grid_toolbar).push(channel_toolbar);
        if let Some(channel_filter_dialog) = channel_filter_dialog {
            content = content.push(channel_filter_dialog);
        }
        content
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
    };
    if app.midi_quantize_dialog.is_some()
        || app.midi_humanize_dialog.is_some()
        || app.midi_transpose_dialog.is_some()
        || app.midi_event_properties_dialog.is_some()
    {
        let dialog = if app.midi_event_properties_dialog.is_some() {
            midi_event_properties_dialog(app, touch_targets)
        } else if app.midi_transpose_dialog.is_some() {
            midi_transpose_dialog(app, touch_targets)
        } else if app.midi_quantize_dialog.is_some() {
            midi_quantize_dialog(app, touch_targets)
        } else {
            midi_humanize_dialog(app, touch_targets)
        };
        let cancel_message = if app.midi_event_properties_dialog.is_some() {
            Message::CancelMidiEventProperties
        } else if app.midi_transpose_dialog.is_some() {
            Message::CancelMidiTranspose
        } else if app.midi_quantize_dialog.is_some() {
            Message::CancelMidiQuantize
        } else {
            Message::CancelMidiHumanize
        };
        let backdrop = mouse_area(
            container(
                iced::widget::Space::new()
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| iced::widget::container::Style {
                background: Some(iced::Background::Color(iced::Color::from_rgba8(
                    0, 0, 0, 0.66,
                ))),
                ..iced::widget::container::Style::default()
            }),
        )
        .on_press(cancel_message);
        let dialog = container(dialog)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill);
        stack![content, backdrop, dialog]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        content
    }
}

#[derive(Clone, Copy)]
enum MidiEventListEntry<'a> {
    Note(&'a MidiNote),
    Controller(MidiControllerData),
    BankSelect(MidiControllerData, MidiControllerData),
    PitchBend(MidiPitchBendData),
    ProgramChange(MidiProgramChangeData),
    PolyAftertouch(MidiPolyAftertouchData),
    ChannelPressure(MidiChannelPressureData),
    Opaque(u32, &'a MidiOpaqueEventData),
    End(u64),
}

impl MidiEventListEntry<'_> {
    fn tick(self) -> u64 {
        match self {
            Self::Note(note) => note.tick(),
            Self::Controller(event) => event.tick,
            Self::BankSelect(msb, _) => msb.tick,
            Self::PitchBend(event) => event.tick,
            Self::ProgramChange(event) => event.tick,
            Self::PolyAftertouch(event) => event.tick,
            Self::ChannelPressure(event) => event.tick,
            Self::Opaque(_, event) => event.tick,
            Self::End(tick) => tick,
        }
    }

    fn order_key(
        self,
        source_order: &HashMap<MidiEventOrderTarget, u32>,
        imported_ordering: bool,
    ) -> (u8, u64, u64) {
        if !imported_ordering {
            let order = match self {
                Self::Opaque(index, event) => {
                    let category = match &event.kind {
                        MidiOpaqueEventKind::Meta { kind: 0x03, .. } => 0,
                        MidiOpaqueEventKind::Meta {
                            kind: 0x06 | 0x07, ..
                        } => 5,
                        MidiOpaqueEventKind::SystemExclusive { .. } => 6,
                        MidiOpaqueEventKind::Meta { .. } => 2,
                    };
                    category * 1_000_000 + u64::from(index)
                }
                Self::Note(_) => 1_000_000,
                Self::Controller(_) | Self::BankSelect(_, _) => 3_000_000,
                Self::PitchBend(_) => 3_500_000,
                Self::ProgramChange(_) => 4_000_000,
                Self::PolyAftertouch(_) => 4_500_000,
                Self::ChannelPressure(_) => 4_750_000,
                Self::End(_) => u64::MAX,
            };
            return (0, order, 0);
        }
        match self {
            Self::Opaque(index, event) => (0, u64::from(event.order), u64::from(index)),
            Self::ProgramChange(event) => (
                1,
                source_order
                    .get(&MidiEventOrderTarget::ProgramChange(event))
                    .copied()
                    .map_or(4_000_000, u64::from),
                0,
            ),
            Self::Controller(event) => (
                2,
                source_order
                    .get(&MidiEventOrderTarget::Controller(event))
                    .copied()
                    .map_or(3_000_000, u64::from),
                0,
            ),
            Self::BankSelect(msb, lsb) => (
                2,
                [msb, lsb]
                    .into_iter()
                    .filter_map(|event| {
                        source_order
                            .get(&MidiEventOrderTarget::Controller(event))
                            .copied()
                    })
                    .min()
                    .map_or(3_000_000, u64::from),
                0,
            ),
            Self::PitchBend(event) => (
                2,
                source_order
                    .get(&MidiEventOrderTarget::PitchBend(event))
                    .copied()
                    .map_or(3_500_000, u64::from),
                0,
            ),
            Self::PolyAftertouch(event) => (
                2,
                source_order
                    .get(&MidiEventOrderTarget::PolyAftertouch(event))
                    .copied()
                    .map_or(4_500_000, u64::from),
                0,
            ),
            Self::ChannelPressure(event) => (
                2,
                source_order
                    .get(&MidiEventOrderTarget::ChannelPressure(event))
                    .copied()
                    .map_or(4_750_000, u64::from),
                0,
            ),
            Self::Note(note) => (
                3,
                source_order
                    .get(&MidiEventOrderTarget::Note(MidiNoteData {
                        pitch: note.pitch(),
                        channel: note.channel(),
                        tick: note.tick(),
                        duration: note.duration(),
                        velocity: note.velocity(),
                        off_velocity: note.off_velocity(),
                    }))
                    .copied()
                    .map_or(0, u64::from),
                0,
            ),
            Self::End(_) => (4, 0, 0),
        }
    }

    fn cursor(self) -> MidiEditorEventListCursor {
        match self {
            Self::Note(note) => MidiEditorEventListCursor::Note(note.id()),
            Self::Controller(event) => MidiEditorEventListCursor::Controller(event),
            Self::BankSelect(msb, lsb) => MidiEditorEventListCursor::BankSelect(msb, lsb),
            Self::PitchBend(event) => MidiEditorEventListCursor::PitchBend(event),
            Self::ProgramChange(event) => MidiEditorEventListCursor::ProgramChange(event),
            Self::PolyAftertouch(event) => MidiEditorEventListCursor::PolyAftertouch(event),
            Self::ChannelPressure(event) => MidiEditorEventListCursor::ChannelPressure(event),
            Self::Opaque(index, _) => MidiEditorEventListCursor::Opaque(index),
            Self::End(_) => MidiEditorEventListCursor::End,
        }
    }
}

fn midi_event_list_entries(item: &MidiItem, snap: MidiSnap) -> Vec<MidiEventListEntry<'_>> {
    let controllers = item
        .controllers()
        .iter()
        .copied()
        .filter(|event| snap.controller_visible(*event))
        .collect::<Vec<_>>();
    let mut paired_controllers = HashSet::new();
    let mut bank_select_lsb_by_msb = HashMap::new();
    for (index, msb) in controllers.iter().copied().enumerate() {
        if msb.controller != 0 || paired_controllers.contains(&index) {
            continue;
        }
        let Some((lsb_index, lsb)) =
            controllers
                .iter()
                .copied()
                .enumerate()
                .find(|(candidate_index, candidate)| {
                    candidate.controller == 32
                        && candidate.channel == msb.channel
                        && candidate.tick == msb.tick
                        && !paired_controllers.contains(candidate_index)
                })
        else {
            continue;
        };
        paired_controllers.insert(index);
        paired_controllers.insert(lsb_index);
        bank_select_lsb_by_msb.insert(index, lsb);
    }
    let controller_entries =
        controllers
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(index, event)| {
                if let Some(lsb) = bank_select_lsb_by_msb.get(&index) {
                    Some(MidiEventListEntry::BankSelect(event, *lsb))
                } else if paired_controllers.contains(&index) {
                    None
                } else {
                    Some(MidiEventListEntry::Controller(event))
                }
            });
    let mut source_order = HashMap::with_capacity(item.event_order().len());
    for entry in item.event_order() {
        source_order
            .entry(entry.target)
            .and_modify(|order: &mut u32| *order = (*order).min(entry.order))
            .or_insert(entry.order);
    }
    let mut events = item
        .notes()
        .iter()
        .filter(|note| snap.note_visible(note))
        .map(MidiEventListEntry::Note)
        .chain(controller_entries)
        .chain(
            item.pitch_bends()
                .iter()
                .copied()
                .filter(|event| snap.pitch_bend_visible(*event))
                .map(MidiEventListEntry::PitchBend),
        )
        .chain(
            item.program_changes()
                .iter()
                .copied()
                .filter(|event| snap.program_change_visible(*event))
                .map(MidiEventListEntry::ProgramChange),
        )
        .chain(
            item.poly_aftertouches()
                .iter()
                .copied()
                .filter(|event| snap.poly_aftertouch_visible(event.channel))
                .map(MidiEventListEntry::PolyAftertouch),
        )
        .chain(
            item.channel_pressures()
                .iter()
                .copied()
                .filter(|event| snap.channel_pressure_visible(event.channel))
                .map(MidiEventListEntry::ChannelPressure),
        )
        .chain(
            item.opaque_events()
                .iter()
                .enumerate()
                .filter(|(_, event)| snap.opaque_event_visible(event))
                .map(|(index, event)| MidiEventListEntry::Opaque(index as u32, event)),
        )
        .collect::<Vec<_>>();
    let imported_ordering = !source_order.is_empty() || !item.opaque_events().is_empty();
    events.sort_by_key(|event| {
        (
            event.tick(),
            event.order_key(&source_order, imported_ordering),
        )
    });
    if snap.end_row_visible(item.length_ticks()) {
        events.push(MidiEventListEntry::End(item.length_ticks()));
    }
    events
}

fn midi_event_bank_select_value(msb: MidiControllerData, lsb: MidiControllerData) -> u16 {
    (u16::from(msb.value) << 7) | u16::from(lsb.value)
}

fn midi_event_program_change_parameter(item: &MidiItem, event: MidiProgramChangeData) -> String {
    format!(
        "{}-{}-{}",
        effective_midi_bank_value(item, event.channel, event.tick, 0),
        effective_midi_bank_value(item, event.channel, event.tick, 32),
        event.program
    )
}

fn midi_opaque_meta_text(data: &[u8]) -> String {
    if std::str::from_utf8(data).is_err() {
        return midi_opaque_meta_hex(data);
    }
    let text = String::from_utf8_lossy(data).trim_matches('\0').to_owned();
    if text
        .chars()
        .all(|character| !character.is_control() || matches!(character, '\t' | '\n' | '\r'))
    {
        text
    } else {
        data.iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn midi_opaque_meta_hex(data: &[u8]) -> String {
    data.iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn midi_meta_kind_name(kind: u8) -> &'static str {
    match kind {
        0x00 => "Sequence number",
        0x01 => "Text event",
        0x02 => "Copyright notice",
        0x03 => "Track name",
        0x04 => "Instrument name",
        0x05 => "Lyrics",
        0x06 => "Marker",
        0x07 => "Cue",
        0x08 => "Program name",
        0x09 => "Device name",
        0x20 => "MIDI channel prefix",
        0x21 => "MIDI port",
        0x2F => "End of track",
        0x51 => "Tempo",
        0x54 => "SMPTE offset",
        0x58 => "Time signature",
        0x59 => "Key signature",
        0x7F => "Sequencer-specific",
        _ => "Meta event",
    }
}

fn midi_opaque_event_label(event: &MidiOpaqueEventData) -> (&'static str, String) {
    match &event.kind {
        MidiOpaqueEventKind::SystemExclusive { status, data } => {
            let bytes = std::iter::once(status)
                .chain(data.iter())
                .map(|byte| format!("{byte:02X}"))
                .collect::<Vec<_>>()
                .join(" ");
            ("SYSEX", bytes)
        }
        MidiOpaqueEventKind::Meta { kind: 0x03, data } => {
            ("Track name", midi_opaque_meta_text(data))
        }
        MidiOpaqueEventKind::Meta {
            kind: 0x06 | 0x07,
            data,
        } => ("Marker", midi_opaque_meta_text(data)),
        MidiOpaqueEventKind::Meta { kind, data } if (0x01..=0x09).contains(kind) => {
            ("Text event", midi_opaque_meta_text(data))
        }
        MidiOpaqueEventKind::Meta { kind, data } => {
            (midi_meta_kind_name(*kind), midi_opaque_meta_hex(data))
        }
    }
}

pub(in crate::app) fn midi_event_list_cursors(
    app: &App,
    item: &MidiItem,
) -> Vec<MidiEditorEventListCursor> {
    midi_event_list_entries(item, midi_event_list_snap_for_app(app))
        .into_iter()
        .map(MidiEventListEntry::cursor)
        .collect()
}

pub(in crate::app) fn midi_event_list_home_target(
    app: &App,
    item: &MidiItem,
) -> Option<(MidiEditorEventListCursor, u64, u64)> {
    let entry = *midi_event_list_entries(item, midi_event_list_snap_for_app(app)).first()?;
    match entry {
        MidiEventListEntry::End(tick) => Some((
            MidiEditorEventListCursor::End,
            tick,
            item.start_tick().saturating_add(item.length_ticks()),
        )),
        _ => {
            let content_tick = entry.tick();
            let position = item.project_position_at_content_tick(content_tick);
            let local_tick = position.map_or_else(
                || {
                    content_tick
                        .saturating_sub(item.source_offset_ticks())
                        .min(item.length_ticks())
                },
                |position| position.tick().saturating_sub(item.start_tick()),
            );
            let project_tick = position.map_or_else(
                || item.start_tick().saturating_add(local_tick),
                |position| position.tick(),
            );
            Some((entry.cursor(), local_tick, project_tick))
        }
    }
}

const MIDI_EVENT_LIST_COLUMN_LABELS: [&str; 9] = [
    "Index",
    "Position",
    "Length",
    "Channel",
    "Type",
    "Parameter",
    "Value",
    "Off Value",
    "REAPER Data",
];
const MIDI_EVENT_LIST_COLUMN_WIDTHS: [f32; 9] =
    [48.0, 104.0, 104.0, 68.0, 100.0, 204.0, 72.0, 80.0, 180.0];
const MIDI_EVENT_LIST_CONTENT_WIDTH: f32 = 976.0;

fn midi_event_list_view<'a>(
    app: &'a App,
    item: &'a MidiItem,
    item_id: ItemId,
    track_name: &'a str,
    touch_targets: bool,
    channel_filter_dialog: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let snap = midi_event_list_snap_for_app(app);
    let events = midi_event_list_entries(item, snap);
    let ppq = u64::from(app.project.settings().ppq());
    let meter = app.project.time_signature_at_tick(0);
    let toolbar = row![
        text(format!(
            "Event List · Track {track_name} · {} events",
            events.len()
        ))
        .width(Length::Fill),
        button("Piano Roll")
            .style(iced::widget::button::secondary)
            .on_press(Message::SetMidiEditorMode(MidiEditorMode::PianoRoll)),
        button("Event List")
            .style(iced::widget::button::primary)
            .on_press(Message::SetMidiEditorMode(MidiEditorMode::EventList)),
    ]
    .spacing(ROW_GAP)
    .align_y(iced::Alignment::Center);
    let channel_toolbar = row![
        text("MIDI channel").size(if touch_targets { 14 } else { 12 }),
        pick_list(
            &MidiEditorChannelSelection::OPTIONS[..],
            Some(app.midi_editor_channel_selection),
            Message::SelectMidiEditorChannel,
        )
        .width(Length::Fixed(if touch_targets { 180.0 } else { 164.0 })),
        button("Filter events…").on_press(Message::OpenMidiEditorFilter),
        button("Filter")
            .style(if app.midi_editor_event_list_filter_active {
                iced::widget::button::primary
            } else {
                iced::widget::button::secondary
            })
            .on_press(Message::ToggleMidiEditorEventListFilter),
    ]
    .spacing(ROW_GAP)
    .align_y(iced::Alignment::Center);
    let header = midi_event_list_header();
    let rows = events
        .iter()
        .enumerate()
        .map(|(index, event)| {
            let (
                position_tick,
                length_ticks,
                channel,
                event_type,
                parameter,
                value,
                off_value,
                reaper_data,
                selected,
                message,
            ) = match *event {
                MidiEventListEntry::Note(note) => (
                    note.tick(),
                    Some(note.duration()),
                    Some(note.channel()),
                    "Note",
                    pitch_name(snap.note_pitch(item_id, &app.midi_editor_selected_notes, note)),
                    note.velocity().to_string(),
                    note.off_velocity().to_string(),
                    String::new(),
                    app.midi_editor_selected_notes.contains(&note.id()),
                    Message::SelectMidiEditorEventSelection(
                        MidiEditorEventListCursor::Note(note.id()),
                        app.keyboard_modifiers.command() || app.keyboard_modifiers.control(),
                    ),
                ),
                MidiEventListEntry::Controller(event) => (
                    event.tick,
                    None,
                    Some(event.channel),
                    "CC",
                    midi_event_controller_name(event.controller),
                    event.value.to_string(),
                    String::new(),
                    String::new(),
                    app.midi_editor_selected_controllers.contains(&event),
                    Message::SelectMidiEditorEventSelection(
                        MidiEditorEventListCursor::Controller(event),
                        app.keyboard_modifiers.command() || app.keyboard_modifiers.control(),
                    ),
                ),
                MidiEventListEntry::BankSelect(msb, lsb) => (
                    msb.tick,
                    None,
                    Some(msb.channel),
                    "CC",
                    "Bank Select 14-bit (0/32)".to_owned(),
                    midi_event_bank_select_value(msb, lsb).to_string(),
                    String::new(),
                    String::new(),
                    app.midi_editor_selected_controllers.contains(&msb)
                        || app.midi_editor_selected_controllers.contains(&lsb),
                    Message::SelectMidiEditorEventSelection(
                        MidiEditorEventListCursor::BankSelect(msb, lsb),
                        app.keyboard_modifiers.command() || app.keyboard_modifiers.control(),
                    ),
                ),
                MidiEventListEntry::PitchBend(event) => (
                    event.tick,
                    None,
                    Some(event.channel),
                    "Pitch Bend",
                    String::new(),
                    format_midi_pitch_bend_value(event.value),
                    String::new(),
                    String::new(),
                    app.midi_editor_selected_pitch_bends.contains(&event),
                    Message::SelectMidiEditorEventSelection(
                        MidiEditorEventListCursor::PitchBend(event),
                        app.keyboard_modifiers.command() || app.keyboard_modifiers.control(),
                    ),
                ),
                MidiEventListEntry::ProgramChange(event) => (
                    event.tick,
                    None,
                    Some(event.channel),
                    "PC",
                    midi_event_program_change_parameter(item, event),
                    event.program.to_string(),
                    String::new(),
                    String::new(),
                    app.midi_editor_selected_program_changes.contains(&event),
                    Message::SelectMidiEditorEventSelection(
                        MidiEditorEventListCursor::ProgramChange(event),
                        app.keyboard_modifiers.command() || app.keyboard_modifiers.control(),
                    ),
                ),
                MidiEventListEntry::PolyAftertouch(event) => (
                    event.tick,
                    None,
                    Some(event.channel),
                    "Poly Aftertouch",
                    pitch_name(event.note),
                    event.value.to_string(),
                    String::new(),
                    String::new(),
                    app.midi_editor_selected_poly_aftertouches.contains(&event),
                    Message::SelectMidiEditorEventSelection(
                        MidiEditorEventListCursor::PolyAftertouch(event),
                        app.keyboard_modifiers.command() || app.keyboard_modifiers.control(),
                    ),
                ),
                MidiEventListEntry::ChannelPressure(event) => (
                    event.tick,
                    None,
                    Some(event.channel),
                    "Channel Pressure",
                    String::new(),
                    event.value.to_string(),
                    String::new(),
                    String::new(),
                    app.midi_editor_selected_channel_pressures.contains(&event),
                    Message::SelectMidiEditorEventSelection(
                        MidiEditorEventListCursor::ChannelPressure(event),
                        app.keyboard_modifiers.command() || app.keyboard_modifiers.control(),
                    ),
                ),
                MidiEventListEntry::Opaque(index, event) => {
                    let (event_type, parameter) = midi_opaque_event_label(event);
                    (
                        event.tick,
                        None,
                        None,
                        event_type,
                        parameter,
                        String::new(),
                        String::new(),
                        String::new(),
                        app.midi_editor_selected_opaque_events.contains(&index),
                        Message::SelectMidiEditorEventSelection(
                            MidiEditorEventListCursor::Opaque(index),
                            app.keyboard_modifiers.command() || app.keyboard_modifiers.control(),
                        ),
                    )
                }
                MidiEventListEntry::End(tick) => (
                    tick,
                    None,
                    Some(0),
                    "(end) All Notes Off",
                    String::new(),
                    "0".to_owned(),
                    String::new(),
                    String::new(),
                    app.midi_editor_event_list_cursor == Some(MidiEditorEventListCursor::End)
                        || events.len() == 1,
                    Message::SelectMidiEditorEventSelection(
                        MidiEditorEventListCursor::End,
                        app.keyboard_modifiers.command() || app.keyboard_modifiers.control(),
                    ),
                ),
            };
            midi_event_list_row(MidiEventListRow {
                index: (index + 1).to_string(),
                position: midi_event_list_position(position_tick, ppq, meter),
                length: midi_event_list_optional_length(length_ticks, ppq, meter),
                channel: channel
                    .map_or_else(|| "--".to_owned(), |channel| (channel + 1).to_string()),
                event_type: event_type.to_owned(),
                parameter,
                value,
                off_value,
                reaper_data,
                selected,
                message,
            })
        })
        .collect::<Vec<_>>();
    let event_rows: Element<'_, Message> = container(column![header].extend(rows).spacing(1))
        .width(Length::Fixed(MIDI_EVENT_LIST_CONTENT_WIDTH))
        .into();
    let has_selection = !app.midi_editor_selected_notes.is_empty()
        || !app.midi_editor_selected_controllers.is_empty()
        || !app.midi_editor_selected_pitch_bends.is_empty()
        || !app.midi_editor_selected_program_changes.is_empty()
        || !app.midi_editor_selected_poly_aftertouches.is_empty()
        || !app.midi_editor_selected_channel_pressures.is_empty()
        || !app.midi_editor_selected_opaque_events.is_empty();
    let has_homogeneous_editable_selection = (!app.midi_editor_selected_notes.is_empty()
        && app.midi_editor_selected_controllers.is_empty()
        && app.midi_editor_selected_pitch_bends.is_empty()
        && app.midi_editor_selected_program_changes.is_empty()
        && app.midi_editor_selected_poly_aftertouches.is_empty()
        && app.midi_editor_selected_channel_pressures.is_empty()
        && app.midi_editor_selected_opaque_events.is_empty())
        || (!app.midi_editor_selected_controllers.is_empty()
            && app.midi_editor_selected_notes.is_empty()
            && app.midi_editor_selected_pitch_bends.is_empty()
            && app.midi_editor_selected_program_changes.is_empty()
            && app.midi_editor_selected_poly_aftertouches.is_empty()
            && app.midi_editor_selected_channel_pressures.is_empty()
            && app.midi_editor_selected_opaque_events.is_empty())
        || (!app.midi_editor_selected_pitch_bends.is_empty()
            && app.midi_editor_selected_notes.is_empty()
            && app.midi_editor_selected_controllers.is_empty()
            && app.midi_editor_selected_program_changes.is_empty()
            && app.midi_editor_selected_poly_aftertouches.is_empty()
            && app.midi_editor_selected_channel_pressures.is_empty()
            && app.midi_editor_selected_opaque_events.is_empty())
        || (!app.midi_editor_selected_program_changes.is_empty()
            && app.midi_editor_selected_notes.is_empty()
            && app.midi_editor_selected_controllers.is_empty()
            && app.midi_editor_selected_pitch_bends.is_empty()
            && app.midi_editor_selected_poly_aftertouches.is_empty()
            && app.midi_editor_selected_channel_pressures.is_empty()
            && app.midi_editor_selected_opaque_events.is_empty())
        || (!app.midi_editor_selected_poly_aftertouches.is_empty()
            && app.midi_editor_selected_notes.is_empty()
            && app.midi_editor_selected_controllers.is_empty()
            && app.midi_editor_selected_pitch_bends.is_empty()
            && app.midi_editor_selected_program_changes.is_empty()
            && app.midi_editor_selected_channel_pressures.is_empty()
            && app.midi_editor_selected_opaque_events.is_empty())
        || (!app.midi_editor_selected_channel_pressures.is_empty()
            && app.midi_editor_selected_notes.is_empty()
            && app.midi_editor_selected_controllers.is_empty()
            && app.midi_editor_selected_pitch_bends.is_empty()
            && app.midi_editor_selected_program_changes.is_empty()
            && app.midi_editor_selected_poly_aftertouches.is_empty()
            && app.midi_editor_selected_opaque_events.is_empty());
    let has_single_opaque_editable_selection = app.midi_editor_selected_opaque_events.len() == 1
        && app.midi_editor_selected_notes.is_empty()
        && app.midi_editor_selected_controllers.is_empty()
        && app.midi_editor_selected_pitch_bends.is_empty()
        && app.midi_editor_selected_program_changes.is_empty()
        && app.midi_editor_selected_poly_aftertouches.is_empty()
        && app.midi_editor_selected_channel_pressures.is_empty()
        && app
            .midi_editor_selected_opaque_events
            .iter()
            .next()
            .and_then(|index| item.opaque_events().get(*index as usize))
            .is_some_and(|event| {
                super::super::midi_opaque_event_properties_type_and_payload(&event.kind).is_some()
            });
    let has_terminal_cursor = !has_selection
        && (app.midi_editor_event_list_cursor == Some(MidiEditorEventListCursor::End)
            || (events.len() == 1 && matches!(events.first(), Some(MidiEventListEntry::End(_)))));
    let mut content = column![toolbar, channel_toolbar];
    if let Some(channel_filter_dialog) = channel_filter_dialog {
        content = content.push(channel_filter_dialog);
    }
    if let Some(feedback) = app.midi_editor_feedback.as_deref() {
        content = content.push(text(feedback).size(12));
    }
    content
        .push(
            scrollable(
                scrollable(event_rows)
                    .direction(scrollable::Direction::Horizontal(
                        scrollable::Scrollbar::default(),
                    ))
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::default(),
            ))
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .push(
            row![
                text(
                    "Click selects; Ctrl/Cmd-click toggles; Shift+Up/Down extends; Delete removes."
                )
                .size(11)
                .width(Length::Fill),
                button("Edit event…")
                    .style(iced::widget::button::secondary)
                    .on_press_maybe(
                        (has_homogeneous_editable_selection
                            || has_single_opaque_editable_selection
                            || has_terminal_cursor)
                            .then_some(Message::OpenMidiEventProperties(item_id)),
                    ),
                button("Delete selected")
                    .style(if has_selection {
                        iced::widget::button::danger
                    } else {
                        iced::widget::button::secondary
                    })
                    .on_press_maybe(
                        has_selection.then_some(Message::DeleteSelectedMidiEvents(item_id)),
                    ),
            ]
            .align_y(iced::Alignment::Center),
        )
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

struct MidiEventListRow {
    index: String,
    position: String,
    length: String,
    channel: String,
    event_type: String,
    parameter: String,
    value: String,
    off_value: String,
    reaper_data: String,
    selected: bool,
    message: Message,
}

fn midi_event_list_row(row: MidiEventListRow) -> Element<'static, Message> {
    let contents = row![
        text(row.index).width(Length::Fixed(MIDI_EVENT_LIST_COLUMN_WIDTHS[0])),
        text(row.position).width(Length::Fixed(MIDI_EVENT_LIST_COLUMN_WIDTHS[1])),
        text(row.length).width(Length::Fixed(MIDI_EVENT_LIST_COLUMN_WIDTHS[2])),
        text(row.channel).width(Length::Fixed(MIDI_EVENT_LIST_COLUMN_WIDTHS[3])),
        text(row.event_type).width(Length::Fixed(MIDI_EVENT_LIST_COLUMN_WIDTHS[4])),
        text(row.parameter).width(Length::Fixed(MIDI_EVENT_LIST_COLUMN_WIDTHS[5])),
        text(row.value).width(Length::Fixed(MIDI_EVENT_LIST_COLUMN_WIDTHS[6])),
        text(row.off_value).width(Length::Fixed(MIDI_EVENT_LIST_COLUMN_WIDTHS[7])),
        text(row.reaper_data).width(Length::Fixed(MIDI_EVENT_LIST_COLUMN_WIDTHS[8])),
    ]
    .spacing(2)
    .align_y(iced::Alignment::Center);
    let button = button(contents)
        .width(Length::Fill)
        .padding([2, 3])
        .style(if row.selected {
            iced::widget::button::primary
        } else {
            iced::widget::button::secondary
        });
    button.on_press(row.message).into()
}

fn midi_event_list_header() -> Element<'static, Message> {
    container(
        row![
            text(MIDI_EVENT_LIST_COLUMN_LABELS[0])
                .width(Length::Fixed(MIDI_EVENT_LIST_COLUMN_WIDTHS[0])),
            text(MIDI_EVENT_LIST_COLUMN_LABELS[1])
                .width(Length::Fixed(MIDI_EVENT_LIST_COLUMN_WIDTHS[1])),
            text(MIDI_EVENT_LIST_COLUMN_LABELS[2])
                .width(Length::Fixed(MIDI_EVENT_LIST_COLUMN_WIDTHS[2])),
            text(MIDI_EVENT_LIST_COLUMN_LABELS[3])
                .width(Length::Fixed(MIDI_EVENT_LIST_COLUMN_WIDTHS[3])),
            text(MIDI_EVENT_LIST_COLUMN_LABELS[4])
                .width(Length::Fixed(MIDI_EVENT_LIST_COLUMN_WIDTHS[4])),
            text(MIDI_EVENT_LIST_COLUMN_LABELS[5])
                .width(Length::Fixed(MIDI_EVENT_LIST_COLUMN_WIDTHS[5])),
            text(MIDI_EVENT_LIST_COLUMN_LABELS[6])
                .width(Length::Fixed(MIDI_EVENT_LIST_COLUMN_WIDTHS[6])),
            text(MIDI_EVENT_LIST_COLUMN_LABELS[7])
                .width(Length::Fixed(MIDI_EVENT_LIST_COLUMN_WIDTHS[7])),
            text(MIDI_EVENT_LIST_COLUMN_LABELS[8])
                .width(Length::Fixed(MIDI_EVENT_LIST_COLUMN_WIDTHS[8])),
        ]
        .spacing(2)
        .align_y(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .padding([4, 3])
    .style(container::bordered_box)
    .into()
}

fn midi_event_controller_name(controller: u8) -> String {
    match controller {
        1 => "Modulation (CC1)".to_owned(),
        7 => "Volume (CC7)".to_owned(),
        10 => "Pan (CC10)".to_owned(),
        11 => "Expression (CC11)".to_owned(),
        64 => "Sustain (CC64)".to_owned(),
        _ => format!("CC {controller}"),
    }
}

pub(in crate::app) fn midi_event_controller_property_label(controller: u8) -> String {
    let name = match controller {
        0 => "Bank Select MSB",
        1 => "Modulation MSB",
        2 => "Breath Controller MSB",
        3 => "Undefined",
        4 => "Foot Controller MSB",
        5 => "Portamento Time MSB",
        6 => "Data Entry MSB",
        7 => "Volume MSB",
        8 => "Balance MSB",
        9 => "Undefined",
        10 => "Pan MSB",
        11 => "Expression MSB",
        12 => "Effect Control 1 MSB",
        13 => "Effect Control 2 MSB",
        16 => "General Purpose 1 MSB",
        17 => "General Purpose 2 MSB",
        18 => "General Purpose 3 MSB",
        19 => "General Purpose 4 MSB",
        32 => "Bank Select LSB",
        33 => "Modulation LSB",
        34 => "Breath Controller LSB",
        36 => "Foot Controller LSB",
        37 => "Portamento Time LSB",
        38 => "Data Entry LSB",
        39 => "Volume LSB",
        40 => "Balance LSB",
        42 => "Pan LSB",
        43 => "Expression LSB",
        44 => "Effect Control 1 LSB",
        45 => "Effect Control 2 LSB",
        48 => "General Purpose 1 LSB",
        49 => "General Purpose 2 LSB",
        50 => "General Purpose 3 LSB",
        51 => "General Purpose 4 LSB",
        64 => "Hold Pedal (on/off)",
        65 => "Portamento (on/off)",
        66 => "Sostenuto (on/off)",
        67 => "Soft Pedal (on/off)",
        68 => "Legato Footswitch (on/off)",
        69 => "Hold 2 (on/off)",
        70 => "Sound Variation",
        71 => "Timbre/Resonance",
        72 => "Release Time",
        73 => "Attack Time",
        74 => "Brightness/Cutoff",
        75..=79 => "Sound Controller",
        80..=83 => "General Purpose Controller",
        84 => "Portamento Control",
        91 => "Reverb Send Level",
        92 => "Tremolo Depth",
        93 => "Chorus Send Level",
        94 => "Celeste/Detune Depth",
        95 => "Phaser Depth",
        96 => "Data Increment",
        97 => "Data Decrement",
        98 => "NRPN LSB",
        99 => "NRPN MSB",
        100 => "RPN LSB",
        101 => "RPN MSB",
        120 => "All Sound Off",
        121 => "Reset All Controllers",
        122 => "Local Control (on/off)",
        123 => "All Notes Off",
        124 => "Omni Mode Off",
        125 => "Omni Mode On",
        126 => "Mono Mode On",
        127 => "Poly Mode On",
        _ => "Undefined",
    };
    format!("{controller:02} {name}")
}

pub(in crate::app) fn midi_event_controller_property_options() -> Vec<String> {
    (0..=127)
        .map(midi_event_controller_property_label)
        .collect()
}

pub(in crate::app) fn midi_event_list_position(
    tick: u64,
    ppq: u64,
    meter: TimeSignature,
) -> String {
    let beat_ticks = ppq
        .saturating_mul(4)
        .checked_div(u64::from(meter.denominator()))
        .unwrap_or(0)
        .max(1);
    let measure_ticks = beat_ticks
        .saturating_mul(u64::from(meter.numerator()))
        .max(1);
    let bar = tick / measure_ticks + 1;
    let within_measure = tick % measure_ticks;
    let beat = within_measure / beat_ticks + 1;
    let beat_subticks = within_measure % beat_ticks;
    let hundredth = (u128::from(beat_subticks) * 100 / u128::from(beat_ticks)) as u8;
    format!("{bar}.{beat}.{hundredth:02}")
}

pub(in crate::app) fn midi_event_list_length(ticks: u64, ppq: u64, meter: TimeSignature) -> String {
    let beat_ticks = ppq
        .saturating_mul(4)
        .checked_div(u64::from(meter.denominator()))
        .unwrap_or(0)
        .max(1);
    let measure_ticks = beat_ticks
        .saturating_mul(u64::from(meter.numerator()))
        .max(1);
    let bars = ticks / measure_ticks;
    let within_measure = ticks % measure_ticks;
    let beats = within_measure / beat_ticks;
    let beat_subticks = within_measure % beat_ticks;
    let hundredth = (u128::from(beat_subticks) * 100 / u128::from(beat_ticks)) as u8;
    format!("{bars}.{beats}.{hundredth:02}")
}

fn midi_event_list_optional_length(ticks: Option<u64>, ppq: u64, meter: TimeSignature) -> String {
    ticks
        .map(|ticks| midi_event_list_length(ticks, ppq, meter))
        .unwrap_or_default()
}

fn format_midi_pitch_bend_value(value: u16) -> String {
    let centered = i32::from(value) - 8_192;
    if centered > 0 {
        format!("+{centered}")
    } else {
        centered.to_string()
    }
}

fn midi_humanize_dialog(app: &App, touch_targets: bool) -> Element<'_, Message> {
    let Some(state) = app.midi_humanize_dialog else {
        return iced::widget::Space::new().into();
    };
    let scope = pick_list(
        super::super::MidiHumanizeScope::OPTIONS,
        Some(state.scope),
        Message::SetMidiHumanizeScope,
    )
    .width(Length::Fixed(160.0));
    let timing = row![
        text("Timing").width(Length::Fixed(92.0)),
        slider(
            0.0..=100.0,
            f32::from(state.timing_amount_percent),
            |value| { Message::SetMidiHumanizeTiming(value.round() as u8) }
        )
        .step(1.0_f32)
        .width(Length::Fill),
        text(format!("{}%", state.timing_amount_percent)).width(Length::Fixed(42.0)),
    ]
    .spacing(SPACING_XS)
    .align_y(iced::Alignment::Center);
    let velocity = row![
        text("Velocity").width(Length::Fixed(92.0)),
        slider(
            0.0..=100.0,
            f32::from(state.velocity_amount_percent),
            |value| { Message::SetMidiHumanizeVelocity(value.round() as u8) }
        )
        .step(1.0_f32)
        .width(Length::Fill),
        text(format!("{}%", state.velocity_amount_percent)).width(Length::Fixed(42.0)),
    ]
    .spacing(SPACING_XS)
    .align_y(iced::Alignment::Center);
    let timing_bias = row![
        text("Timing bias").width(Length::Fixed(92.0)),
        slider(
            -100.0..=100.0,
            f32::from(state.timing_bias_percent),
            |value| Message::SetMidiHumanizeTimingBias(value.round() as i16),
        )
        .step(1.0_f32)
        .width(Length::Fill),
        text(format!("{}%", state.timing_bias_percent)).width(Length::Fixed(42.0)),
    ]
    .spacing(SPACING_XS)
    .align_y(iced::Alignment::Center);
    let controls = row![
        button("New random seed")
            .style(iced::widget::button::secondary)
            .on_press(Message::NewMidiHumanizeSeed),
        iced::widget::Space::new().width(Length::Fill),
        button("OK")
            .style(iced::widget::button::primary)
            .on_press(Message::AcceptMidiHumanize),
        button("Cancel")
            .style(iced::widget::button::secondary)
            .on_press(Message::CancelMidiHumanize),
    ]
    .spacing(SPACING_XS)
    .align_y(iced::Alignment::Center);
    container(
        column![
            text("Humanize Notes").size(16),
            row![text("Notes").width(Length::Fixed(92.0)), scope]
                .spacing(SPACING_XS)
                .align_y(iced::Alignment::Center),
            timing,
            velocity,
            timing_bias,
            controls,
        ]
        .spacing(if touch_targets {
            SPACING_MD
        } else {
            SPACING_XS
        }),
    )
    .width(Length::Fixed(if touch_targets { 480.0 } else { 448.0 }))
    .padding(SPACING_MD)
    .style(container::rounded_box)
    .into()
}

fn midi_transpose_dialog(app: &App, touch_targets: bool) -> Element<'_, Message> {
    let Some(state) = app.midi_transpose_dialog.as_ref() else {
        return iced::widget::Space::new().into();
    };
    let scope = pick_list(
        super::super::MidiTransposeScope::OPTIONS,
        Some(state.scope),
        Message::SetMidiTransposeScope,
    )
    .width(Length::Fixed(if touch_targets { 184.0 } else { 160.0 }));
    let semitones = iced::widget::text_input("0", &state.semitones)
        .on_input(Message::SetMidiTransposeSemitones)
        .width(Length::Fixed(88.0));
    let can_apply = midi_snap_for_app(app)
        .transpose_preview
        .is_some_and(|preview| preview.range_valid);
    let controls = row![
        iced::widget::Space::new().width(Length::Fill),
        button("Cancel")
            .style(iced::widget::button::secondary)
            .on_press(Message::CancelMidiTranspose),
        button("OK")
            .style(iced::widget::button::primary)
            .on_press_maybe(can_apply.then_some(Message::AcceptMidiTranspose)),
    ]
    .spacing(SPACING_XS)
    .align_y(iced::Alignment::Center);
    container(
        column![
            text("Transpose MIDI").size(16),
            row![text("Transpose:").width(Length::Fixed(92.0)), scope]
                .spacing(SPACING_XS)
                .align_y(iced::Alignment::Center),
            row![text("Semitones").width(Length::Fixed(92.0)), semitones]
                .spacing(SPACING_XS)
                .align_y(iced::Alignment::Center),
            controls,
        ]
        .spacing(if touch_targets {
            SPACING_MD
        } else {
            SPACING_XS
        }),
    )
    .width(Length::Fixed(if touch_targets { 384.0 } else { 344.0 }))
    .padding(SPACING_MD)
    .style(container::rounded_box)
    .into()
}

fn midi_event_properties_dialog(app: &App, touch_targets: bool) -> Element<'_, Message> {
    let Some(state) = app.midi_event_properties_dialog.as_ref() else {
        return iced::widget::Space::new().into();
    };
    let is_controller = matches!(
        &state.target,
        super::super::MidiEventPropertiesTarget::Controllers(_)
    );
    let is_pitch_bend = matches!(
        &state.target,
        super::super::MidiEventPropertiesTarget::PitchBends(_)
    );
    let is_program_change = matches!(
        &state.target,
        super::super::MidiEventPropertiesTarget::ProgramChanges(_)
    );
    let is_poly_aftertouch = matches!(
        &state.target,
        super::super::MidiEventPropertiesTarget::PolyAftertouches(_)
    );
    let is_channel_pressure = matches!(
        &state.target,
        super::super::MidiEventPropertiesTarget::ChannelPressures(_)
    );
    let is_opaque = matches!(
        &state.target,
        super::super::MidiEventPropertiesTarget::Opaque(..)
    );
    let is_terminal = matches!(
        &state.target,
        super::super::MidiEventPropertiesTarget::Terminal
    );
    let is_opaque_sysex =
        state.opaque_type == Some(super::super::MidiOpaqueEventPropertiesType::SysEx);
    let is_opaque_meta = is_opaque && !is_opaque_sysex;
    let is_opaque_raw_meta =
        state.opaque_type == Some(super::super::MidiOpaqueEventPropertiesType::RawMeta);
    let is_opaque_meta_hex =
        super::super::midi_opaque_event_properties_dialog_payload_is_hex(state);
    let position = iced::widget::text_input("1.1.00", &state.position)
        .on_input(Message::SetMidiEventPropertiesPosition)
        .width(Length::Fixed(if touch_targets { 148.0 } else { 116.0 }));
    let length = iced::widget::text_input("0.1.00", &state.length)
        .on_input(Message::SetMidiEventPropertiesLength)
        .width(Length::Fixed(if touch_targets { 116.0 } else { 90.0 }));
    let channel = iced::widget::text_input("1", &state.channel)
        .on_input(Message::SetMidiEventPropertiesChannel)
        .width(Length::Fixed(if touch_targets { 148.0 } else { 116.0 }));
    let pitch = iced::widget::text_input("60", &state.parameter)
        .on_input(Message::SetMidiEventPropertiesParameter)
        .width(Length::Fixed(if touch_targets { 176.0 } else { 152.0 }));
    let value = iced::widget::text_input("96", &state.value)
        .on_input(Message::SetMidiEventPropertiesValue)
        .width(Length::Fixed(if touch_targets { 104.0 } else { 88.0 }));
    let off_value = iced::widget::text_input(
        if state.off_value_mixed { "" } else { "0" },
        &state.off_value,
    )
    .on_input(Message::SetMidiEventPropertiesOffValue)
    .width(Length::Fixed(if touch_targets { 104.0 } else { 88.0 }));
    let bank_msb =
        iced::widget::text_input(if state.bank_msb_mixed { "" } else { "0" }, &state.bank_msb)
            .on_input(Message::SetMidiEventPropertiesBankMsb)
            .width(Length::Fixed(if touch_targets { 104.0 } else { 88.0 }));
    let bank_lsb =
        iced::widget::text_input(if state.bank_lsb_mixed { "" } else { "0" }, &state.bank_lsb)
            .on_input(Message::SetMidiEventPropertiesBankLsb)
            .width(Length::Fixed(if touch_targets { 104.0 } else { 88.0 }));
    let title = if is_program_change {
        "Bank/Program Select"
    } else if is_opaque_sysex {
        "SysEx Properties"
    } else {
        "Event Properties"
    };
    let opaque_meta_text: Element<'_, Message> = if state.opaque_multiline || is_opaque_meta_hex {
        text_editor(&state.opaque_editor)
            .on_action(Message::EditMidiOpaqueEventPropertiesText)
            .height(Length::Fixed(if touch_targets { 128.0 } else { 96.0 }))
            .into()
    } else {
        iced::widget::text_input("Text event", &state.opaque_payload)
            .on_input(Message::SetMidiOpaqueEventPropertiesPayload)
            .width(Length::Fill)
            .into()
    };
    let opaque_sysex_data = text_editor(&state.opaque_editor)
        .on_action(Message::EditMidiOpaqueEventPropertiesText)
        .height(Length::Fixed(if touch_targets { 220.0 } else { 188.0 }));
    let (sysex_start, sysex_end) = match &state.target {
        super::super::MidiEventPropertiesTarget::Opaque(
            _,
            MidiOpaqueEventData {
                kind: MidiOpaqueEventKind::SystemExclusive { status, .. },
                ..
            },
        ) => midi_sysex_frame_labels(*status),
        _ => ("F0", Some("F7")),
    };
    let mut content = if is_terminal {
        column![]
    } else {
        column![text(title).size(16)]
    };
    if is_terminal {
        content = content
            .push(
                row![
                    text("Type:").width(Length::Fixed(84.0)),
                    midi_event_properties_blank_field(240.0, true),
                ]
                .spacing(SPACING_XS)
                .align_y(iced::Alignment::Center),
            )
            .push(
                row![
                    text("Position:").width(Length::Fixed(84.0)),
                    midi_event_properties_blank_field(88.0, false),
                ]
                .spacing(SPACING_XS)
                .align_y(iced::Alignment::Center),
            )
            .push(
                row![
                    text("Channel:").width(Length::Fixed(84.0)),
                    midi_event_properties_blank_field(96.0, true),
                ]
                .spacing(SPACING_XS)
                .align_y(iced::Alignment::Center),
            )
            .push(text("Use +, -, *, / for relative changes in any field.").size(11))
            .push(text("Use +1o2 to add 1 octave and 2").size(11));
    } else if is_opaque_sysex {
        let mut sysex_data_row = row![text(sysex_start), opaque_sysex_data]
            .spacing(SPACING_XS)
            .align_y(iced::Alignment::Center);
        if let Some(sysex_end) = sysex_end {
            sysex_data_row = sysex_data_row.push(text(sysex_end));
        }
        content = content
            .push(sysex_data_row)
            .push(
                row![
                    text("Position").width(Length::Fixed(76.0)),
                    position,
                    iced::widget::Space::new().width(Length::Fill),
                    button("Load…")
                        .style(iced::widget::button::secondary)
                        .on_press(Message::PickMidiOpaqueSysExFile(false)),
                    button("Save…")
                        .style(iced::widget::button::secondary)
                        .on_press(Message::PickMidiOpaqueSysExFile(true)),
                ]
                .spacing(SPACING_XS)
                .align_y(iced::Alignment::Center),
            )
            .push(text("Enter hexadecimal bytes separated by spaces.").size(11));
    } else if is_opaque_meta {
        let type_row: Element<'_, Message> = if is_opaque_raw_meta {
            let label = match &state.target {
                super::super::MidiEventPropertiesTarget::Opaque(_, event) => match &event.kind {
                    MidiOpaqueEventKind::Meta { kind, .. } => {
                        format!("{} (0x{kind:02X})", midi_meta_kind_name(*kind))
                    }
                    _ => "Meta event".to_owned(),
                },
                _ => "Meta event".to_owned(),
            };
            row![text("Type").width(Length::Fixed(76.0)), text(label)]
                .spacing(SPACING_XS)
                .align_y(iced::Alignment::Center)
                .into()
        } else {
            row![
                text("Type").width(Length::Fixed(76.0)),
                pick_list(
                    &super::super::MidiOpaqueEventPropertiesType::META_OPTIONS[..],
                    state.opaque_type,
                    Message::SetMidiOpaqueEventPropertiesType,
                )
                .width(Length::Fill),
            ]
            .spacing(SPACING_XS)
            .align_y(iced::Alignment::Center)
            .into()
        };
        let mut position_row = row![
            text("Position").width(Length::Fixed(76.0)),
            position,
            iced::widget::Space::new().width(Length::Fill),
        ]
        .spacing(SPACING_XS)
        .align_y(iced::Alignment::Center);
        if !is_opaque_meta_hex {
            position_row = position_row.push(
                button("Load…")
                    .style(iced::widget::button::secondary)
                    .on_press(Message::PickMidiOpaqueMetaTextFile),
            );
        }
        content = content
            .push(
                row![
                    text(if is_opaque_meta_hex {
                        "Data (hex)"
                    } else {
                        "Text"
                    })
                    .width(Length::Fixed(76.0)),
                    opaque_meta_text
                ]
                .spacing(SPACING_XS)
                .align_y(iced::Alignment::Center),
            )
            .push(type_row);
        if !is_opaque_meta_hex {
            content = content.push(
                row![
                    iced::widget::Space::new().width(Length::Fixed(76.0)),
                    checkbox(state.opaque_multiline)
                        .label("Multi-line text")
                        .on_toggle(Message::SetMidiOpaqueEventPropertiesMultiline),
                ]
                .spacing(SPACING_XS)
                .align_y(iced::Alignment::Center),
            );
        }
        content = content.push(position_row);
    } else if is_controller {
        content =
            content
                .push(
                    row![
                        text("Type").width(Length::Fixed(76.0)),
                        text("Control Change (CC)"),
                    ]
                    .spacing(SPACING_XS)
                    .align_y(iced::Alignment::Center),
                )
                .push(
                    row![text("Position").width(Length::Fixed(76.0)), position]
                        .spacing(SPACING_XS)
                        .align_y(iced::Alignment::Center),
                )
                .push(
                    row![text("Channel").width(Length::Fixed(76.0)), channel]
                        .spacing(SPACING_XS)
                        .align_y(iced::Alignment::Center),
                )
                .push(
                    row![
                        text("Controller").width(Length::Fixed(76.0)),
                        iced::widget::combo_box(
                            &app.midi_event_properties_controller_options.0,
                            "7",
                            (!state.parameter.is_empty()).then_some(&state.parameter),
                            Message::SetMidiEventPropertiesParameter,
                        )
                        .on_input(Message::SetMidiEventPropertiesParameter)
                        .width(Length::Fixed(if touch_targets { 224.0 } else { 200.0 })),
                    ]
                    .spacing(SPACING_XS)
                    .align_y(iced::Alignment::Center),
                )
                .push(
                    row![text("Value").width(Length::Fixed(76.0)), value]
                        .spacing(SPACING_XS)
                        .align_y(iced::Alignment::Center),
                )
                .push(text("CC shape and Bezier tension · not modeled").size(11))
                .push(
                    text("Use +, -, *, / for relative changes; position uses bar.beat.hundredths.")
                        .size(11),
                );
    } else if is_pitch_bend {
        let offset = state
            .value
            .trim()
            .parse::<i32>()
            .map(|value| format!("({:+})", value - 8_192))
            .unwrap_or_default();
        content = content
            .push(
                row![text("Type").width(Length::Fixed(76.0)), text("Pitch")]
                    .spacing(SPACING_XS)
                    .align_y(iced::Alignment::Center),
            )
            .push(
                row![text("Position").width(Length::Fixed(76.0)), position]
                    .spacing(SPACING_XS)
                    .align_y(iced::Alignment::Center),
            )
            .push(
                row![text("Channel").width(Length::Fixed(76.0)), channel]
                    .spacing(SPACING_XS)
                    .align_y(iced::Alignment::Center),
            )
            .push(
                row![
                    text("Value").width(Length::Fixed(76.0)),
                    value,
                    text(offset)
                ]
                .spacing(SPACING_XS)
                .align_y(iced::Alignment::Center),
            )
            .push(text("CC shape and Bezier tension · not modeled").size(11))
            .push(
                text("Use +, -, *, / for relative changes; position uses bar.beat.hundredths.")
                    .size(11),
            );
    } else if is_program_change {
        content = content
            .push(
                row![
                    text("Type").width(Length::Fixed(76.0)),
                    text("Program Change"),
                ]
                .spacing(SPACING_XS)
                .align_y(iced::Alignment::Center),
            )
            .push(
                row![text("Position").width(Length::Fixed(76.0)), position]
                    .spacing(SPACING_XS)
                    .align_y(iced::Alignment::Center),
            )
            .push(
                row![text("MIDI channel").width(Length::Fixed(128.0)), channel]
                    .spacing(SPACING_XS)
                    .align_y(iced::Alignment::Center),
            )
            .push(
                row![text("Program number").width(Length::Fixed(160.0)), value]
                    .spacing(SPACING_XS)
                    .align_y(iced::Alignment::Center),
            )
            .push(
                row![text("Bank MSB (CC0)").width(Length::Fixed(160.0)), bank_msb]
                    .spacing(SPACING_XS)
                    .align_y(iced::Alignment::Center),
            )
            .push(
                row![
                    text("Bank LSB (CC32)").width(Length::Fixed(160.0)),
                    bank_lsb
                ]
                .spacing(SPACING_XS)
                .align_y(iced::Alignment::Center),
            )
            .push(text("Bank MSB, Bank LSB, and program ranges: 0–127.").size(11));
    } else if is_poly_aftertouch {
        content = content
            .push(
                row![
                    text("Type").width(Length::Fixed(76.0)),
                    text("Poly Aftertouch"),
                ]
                .spacing(SPACING_XS)
                .align_y(iced::Alignment::Center),
            )
            .push(
                row![text("Position").width(Length::Fixed(76.0)), position]
                    .spacing(SPACING_XS)
                    .align_y(iced::Alignment::Center),
            )
            .push(
                row![text("Channel").width(Length::Fixed(76.0)), channel]
                    .spacing(SPACING_XS)
                    .align_y(iced::Alignment::Center),
            )
            .push(
                row![
                    text("Note").width(Length::Fixed(76.0)),
                    iced::widget::text_input("60 C4", &state.parameter)
                        .on_input(Message::SetMidiEventPropertiesParameter)
                        .width(Length::Fixed(if touch_targets { 176.0 } else { 152.0 })),
                ]
                .spacing(SPACING_XS)
                .align_y(iced::Alignment::Center),
            )
            .push(
                row![text("Value").width(Length::Fixed(76.0)), value]
                    .spacing(SPACING_XS)
                    .align_y(iced::Alignment::Center),
            )
            .push(text("CC shape and Bezier tension · not modeled").size(11))
            .push(
                text("Use +, -, *, / for relative changes; position uses bar.beat.hundredths.")
                    .size(11),
            );
    } else if is_channel_pressure {
        content = content
            .push(
                row![
                    text("Type").width(Length::Fixed(76.0)),
                    text("Channel Pressure"),
                ]
                .spacing(SPACING_XS)
                .align_y(iced::Alignment::Center),
            )
            .push(
                row![text("Position").width(Length::Fixed(76.0)), position]
                    .spacing(SPACING_XS)
                    .align_y(iced::Alignment::Center),
            )
            .push(
                row![text("Channel").width(Length::Fixed(76.0)), channel]
                    .spacing(SPACING_XS)
                    .align_y(iced::Alignment::Center),
            )
            .push(
                row![text("Value").width(Length::Fixed(76.0)), value]
                    .spacing(SPACING_XS)
                    .align_y(iced::Alignment::Center),
            )
            .push(text("CC shape and Bezier tension · not modeled").size(11))
            .push(
                text("Use +, -, *, / for relative changes; position uses bar.beat.hundredths.")
                    .size(11),
            );
    } else {
        content = content
            .push(
                row![text("Type").width(Length::Fixed(76.0)), text("Note")]
                    .spacing(SPACING_XS)
                    .align_y(iced::Alignment::Center),
            )
            .push(
                row![
                    text("Position").width(Length::Fixed(64.0)),
                    position,
                    text("Length").width(Length::Fixed(52.0)),
                    length,
                ]
                .spacing(SPACING_XS)
                .align_y(iced::Alignment::Center),
            )
            .push(
                row![text("Channel").width(Length::Fixed(76.0)), channel]
                    .spacing(SPACING_XS)
                    .align_y(iced::Alignment::Center),
            )
            .push(
                row![text("Note").width(Length::Fixed(76.0)), pitch]
                    .spacing(SPACING_XS)
                    .align_y(iced::Alignment::Center),
            )
            .push(
                row![
                    text("Velocity").width(Length::Fixed(76.0)),
                    value,
                    text("Off value").width(Length::Fixed(76.0)),
                    off_value,
                ]
                .spacing(SPACING_XS)
                .align_y(iced::Alignment::Center),
            )
            .push(text("Use +, -, *, / for relative changes; position and length use bar.beat.hundredths.").size(11))
            .push(text("Pitch also accepts +1o2 to add 1 octave and 2 semitones.").size(11));
    }
    let mut content = content.spacing(if touch_targets {
        SPACING_MD
    } else {
        SPACING_XS
    });
    if let Some(feedback) = app.midi_editor_feedback.as_deref() {
        content = content.push(text(feedback).size(12));
    }
    let actions = if is_opaque {
        row![
            iced::widget::Space::new().width(Length::Fill),
            button("OK")
                .style(iced::widget::button::primary)
                .on_press(Message::AcceptMidiEventProperties),
            button("Cancel")
                .style(iced::widget::button::secondary)
                .on_press(Message::CancelMidiEventProperties),
        ]
        .spacing(SPACING_XS)
        .align_y(iced::Alignment::Center)
    } else {
        row![
            iced::widget::Space::new().width(Length::Fill),
            button("OK")
                .style(iced::widget::button::primary)
                .on_press(Message::AcceptMidiEventProperties),
            button("Cancel")
                .style(iced::widget::button::secondary)
                .on_press(Message::CancelMidiEventProperties),
            button("Apply")
                .style(iced::widget::button::secondary)
                .on_press(Message::ApplyMidiEventProperties),
        ]
        .spacing(SPACING_XS)
        .align_y(iced::Alignment::Center)
    };
    content = content.push(actions);
    container(content)
        .width(Length::Fixed(if touch_targets { 560.0 } else { 520.0 }))
        .padding(SPACING_MD)
        .style(container::rounded_box)
        .into()
}

fn midi_event_properties_blank_field(width: f32, dropdown: bool) -> Element<'static, Message> {
    let content: Element<'static, Message> = if dropdown {
        row![
            iced::widget::Space::new().width(Length::Fill),
            text("▾").size(12),
        ]
        .align_y(iced::Alignment::Center)
        .into()
    } else {
        iced::widget::Space::new().width(Length::Fill).into()
    };
    container(content)
        .width(Length::Fixed(width))
        .height(Length::Fixed(24.0))
        .padding([2, 4])
        .style(container::bordered_box)
        .into()
}

fn midi_sysex_frame_labels(status: u8) -> (&'static str, Option<&'static str>) {
    if status == 0xF7 {
        ("F7", None)
    } else {
        ("F0", Some("F7"))
    }
}

fn midi_quantize_dialog(app: &App, touch_targets: bool) -> Element<'_, Message> {
    let Some(state) = app.midi_quantize_dialog else {
        return iced::widget::Space::new().into();
    };
    let grid = app.timeline.midi_editor_snap_grid;
    let scope = pick_list(
        super::super::MidiQuantizeScope::OPTIONS,
        Some(state.scope),
        Message::SetMidiQuantizeScope,
    )
    .width(Length::Fixed(136.0));
    let operation = pick_list(
        super::super::MidiQuantizeOperation::OPTIONS,
        Some(state.operation),
        Message::SetMidiQuantizeOperation,
    )
    .width(Length::Fill);
    let strength = row![
        text("Strength").width(Length::Fixed(72.0)),
        slider(0.0..=100.0, f32::from(state.strength_percent), |value| {
            Message::SetMidiQuantizeStrength(value.round() as u8)
        })
        .step(1.0_f32)
        .width(Length::Fill),
        text(format!("{}%", state.strength_percent)).width(Length::Fixed(42.0)),
    ]
    .spacing(SPACING_XS)
    .align_y(iced::Alignment::Center);
    let bypass = checkbox(state.bypass)
        .label("Bypass")
        .on_toggle(Message::SetMidiQuantizeBypass);
    let fix_overlaps = checkbox(state.fix_overlaps)
        .label("Fix overlaps")
        .on_toggle(Message::SetMidiQuantizeFixOverlaps);
    let action_buttons = row![
        fix_overlaps,
        iced::widget::Space::new().width(Length::Fill),
        button("Commit")
            .style(iced::widget::button::secondary)
            .on_press(Message::CommitMidiQuantize),
        button("OK")
            .style(iced::widget::button::primary)
            .on_press(Message::AcceptMidiQuantize),
        button("Cancel")
            .style(iced::widget::button::secondary)
            .on_press(Message::CancelMidiQuantize),
    ]
    .spacing(SPACING_XS)
    .align_y(iced::Alignment::Center);
    container(
        column![
            text("Quantize Events").size(16),
            row![
                text("Settings").width(Length::Fixed(72.0)),
                text(format!("Use grid · {grid}")),
                iced::widget::Space::new().width(Length::Fill),
                bypass,
            ]
            .spacing(SPACING_XS)
            .align_y(iced::Alignment::Center),
            row![
                text("Quantize").width(Length::Fixed(72.0)),
                scope,
                operation,
            ]
            .spacing(SPACING_XS)
            .align_y(iced::Alignment::Center),
            strength,
            action_buttons,
        ]
        .spacing(if touch_targets {
            SPACING_MD
        } else {
            SPACING_XS
        }),
    )
    .width(Length::Fixed(if touch_targets { 480.0 } else { 448.0 }))
    .padding(SPACING_MD)
    .style(container::rounded_box)
    .into()
}

fn channel_filter_dialog(app: &App, touch_targets: bool) -> Element<'_, Message> {
    let note_filter = app.midi_editor_note_filter;
    let controller_filter = app.midi_editor_controller_filter;
    let note_filter_visible = matches!(
        app.midi_editor_event_type_filter,
        MidiEditorEventTypeFilter::AllEvents | MidiEditorEventTypeFilter::Notes
    );
    let controller_filter_visible =
        app.midi_editor_event_type_filter == MidiEditorEventTypeFilter::ControlChange;
    let pitch_bend_filter_visible =
        app.midi_editor_event_type_filter == MidiEditorEventTypeFilter::Pitch;
    let program_change_filter_visible =
        app.midi_editor_event_type_filter == MidiEditorEventTypeFilter::ProgramChange;
    let position_filter_visible = note_filter_visible
        || controller_filter_visible
        || pitch_bend_filter_visible
        || program_change_filter_visible;
    let repeat_filter_visible =
        note_filter_visible || controller_filter_visible || program_change_filter_visible;
    let pitch_options = if note_filter.pitch_enabled {
        &MidiEditorPitchFilterValue::OPTIONS[..]
    } else {
        &[]
    };
    let velocity_options = if note_filter.velocity_enabled {
        &MIDI_EDITOR_FILTER_VALUES[..]
    } else {
        &[]
    };
    let mut first_row = row![].spacing(SPACING_XS);
    let mut second_row = row![].spacing(SPACING_XS);
    for channel in 0..16_u8 {
        let option = checkbox(app.midi_editor_filter_channels & (1_u16 << channel) != 0)
            .label(format!("Channel {}", channel + 1))
            .on_toggle(move |_| Message::ToggleMidiEditorFilterChannel(channel));
        if channel < 8 {
            first_row = first_row.push(option);
        } else {
            second_row = second_row.push(option);
        }
    }
    let first_row: Element<'_, Message> = if touch_targets {
        scrollable(first_row)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default(),
            ))
            .height(Length::Fixed(TOUCH_TARGET_MIN))
            .into()
    } else {
        first_row.into()
    };
    let second_row: Element<'_, Message> = if touch_targets {
        scrollable(second_row)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default(),
            ))
            .height(Length::Fixed(TOUCH_TARGET_MIN))
            .into()
    } else {
        second_row.into()
    };
    let all_channels = app.midi_editor_filter_channels == u16::MAX;
    let header = row![
        text("Filter Events").width(Length::Fill),
        checkbox(all_channels)
            .label("All")
            .on_toggle(Message::SetMidiEditorFilterChannelsAll),
        button("Done")
            .height(Length::Fixed(if touch_targets {
                TOUCH_TARGET_MIN
            } else {
                28.0
            }))
            .on_press(Message::CloseMidiEditorFilter),
    ]
    .spacing(ROW_GAP)
    .align_y(iced::Alignment::Center);
    let filter_options = row![
        checkbox(app.midi_editor_event_filter_enabled)
            .label("Show only events that pass filter")
            .on_toggle(|_| Message::ToggleMidiEditorEventFilter),
        checkbox(app.midi_editor_filter_inverted)
            .label("Invert")
            .on_toggle(|_| Message::ToggleMidiEditorFilterInvert),
    ]
    .spacing(ROW_GAP)
    .align_y(iced::Alignment::Center);
    let event_type_filter = row![
        text("Event type").width(Length::Fixed(92.0)),
        pick_list(
            &MidiEditorEventTypeFilter::OPTIONS[..],
            Some(app.midi_editor_event_type_filter),
            Message::SetMidiEditorEventTypeFilter,
        )
        .width(Length::Fixed(208.0)),
    ]
    .spacing(ROW_GAP)
    .align_y(iced::Alignment::Center);
    let pitch_filter = row![
        checkbox(note_filter.pitch_enabled)
            .label("Note")
            .on_toggle(Message::SetMidiEditorFilterNotePitchEnabled),
        text("Low").size(11),
        pick_list(
            pitch_options,
            note_filter
                .pitch_range_initialized
                .then(|| MidiEditorPitchFilterValue::new(note_filter.pitch_low)),
            |value| Message::SetMidiEditorFilterNotePitchLow(value.pitch()),
        )
        .placeholder("")
        .width(Length::Fixed(92.0)),
        text("High").size(11),
        pick_list(
            pitch_options,
            note_filter
                .pitch_range_initialized
                .then(|| MidiEditorPitchFilterValue::new(note_filter.pitch_high)),
            |value| Message::SetMidiEditorFilterNotePitchHigh(value.pitch()),
        )
        .placeholder("")
        .width(Length::Fixed(92.0)),
    ]
    .spacing(ROW_GAP)
    .align_y(iced::Alignment::Center);
    let velocity_filter = row![
        checkbox(note_filter.velocity_enabled)
            .label("Velocity")
            .on_toggle(Message::SetMidiEditorFilterNoteVelocityEnabled),
        text("Low").size(11),
        pick_list(
            velocity_options,
            note_filter
                .velocity_range_initialized
                .then_some(note_filter.velocity_low),
            Message::SetMidiEditorFilterNoteVelocityLow,
        )
        .placeholder("")
        .width(Length::Fixed(80.0)),
        text("High").size(11),
        pick_list(
            velocity_options,
            note_filter
                .velocity_range_initialized
                .then_some(note_filter.velocity_high),
            Message::SetMidiEditorFilterNoteVelocityHigh,
        )
        .placeholder("")
        .width(Length::Fixed(80.0)),
    ]
    .spacing(ROW_GAP)
    .align_y(iced::Alignment::Center);
    let position_filter = row![
        checkbox(app.midi_editor_position_filter.enabled)
            .label("Position in measure")
            .on_toggle(Message::SetMidiEditorFilterPositionEnabled),
        text_input("", &app.midi_editor_filter_position_low)
            .on_input_maybe(
                app.midi_editor_position_filter
                    .enabled
                    .then_some(Message::SetMidiEditorFilterPositionLow,)
            )
            .width(Length::Fixed(96.0)),
        text("to").size(11),
        text_input("", &app.midi_editor_filter_position_high)
            .on_input_maybe(
                app.midi_editor_position_filter
                    .enabled
                    .then_some(Message::SetMidiEditorFilterPositionHigh,)
            )
            .width(Length::Fixed(96.0)),
    ]
    .spacing(ROW_GAP)
    .align_y(iced::Alignment::Center);
    let length_filter = row![
        checkbox(note_filter.length_enabled)
            .label("Length")
            .on_toggle(Message::SetMidiEditorFilterNoteLengthEnabled),
        text_input("", &app.midi_editor_filter_note_length_low)
            .on_input_maybe(
                note_filter
                    .length_enabled
                    .then_some(Message::SetMidiEditorFilterNoteLengthLow,)
            )
            .width(Length::Fixed(88.0)),
        text("to").size(11),
        text_input("", &app.midi_editor_filter_note_length_high)
            .on_input_maybe(
                note_filter
                    .length_enabled
                    .then_some(Message::SetMidiEditorFilterNoteLengthHigh,)
            )
            .width(Length::Fixed(88.0)),
    ]
    .spacing(ROW_GAP)
    .align_y(iced::Alignment::Center);
    let controller_filter_row = row![
        checkbox(controller_filter.controller_enabled)
            .label("Control")
            .on_toggle(Message::SetMidiEditorFilterControllerEnabled),
        pick_list(
            if controller_filter.controller_enabled {
                &MidiEditorControllerFilterValue::OPTIONS[..]
            } else {
                &[]
            },
            controller_filter
                .controller_enabled
                .then_some(MidiEditorControllerFilterValue(
                    controller_filter.controller
                )),
            |value| Message::SetMidiEditorFilterController(value.0),
        )
        .placeholder(if controller_filter.controller_enabled {
            ""
        } else {
            "<all>"
        })
        .width(Length::Fixed(220.0)),
    ]
    .spacing(ROW_GAP)
    .align_y(iced::Alignment::Center);
    let value_filter_row = row![
        checkbox(controller_filter.value_enabled)
            .label("Value")
            .on_toggle(Message::SetMidiEditorFilterValueEnabled),
        text_input("", &app.midi_editor_filter_value_low)
            .on_input_maybe(
                controller_filter
                    .value_enabled
                    .then_some(Message::SetMidiEditorFilterValueLow)
            )
            .width(Length::Fixed(80.0)),
        text("to").size(11),
        text_input("", &app.midi_editor_filter_value_high)
            .on_input_maybe(
                controller_filter
                    .value_enabled
                    .then_some(Message::SetMidiEditorFilterValueHigh)
            )
            .width(Length::Fixed(80.0)),
    ]
    .spacing(ROW_GAP)
    .align_y(iced::Alignment::Center);
    let pitch_bend_filter_row = row![
        checkbox(app.midi_editor_pitch_bend_filter.enabled)
            .label("Pitch")
            .on_toggle(Message::SetMidiEditorFilterPitchBendEnabled),
        text("Low").size(11),
        text_input("", &app.midi_editor_filter_value_low)
            .on_input_maybe(
                app.midi_editor_pitch_bend_filter
                    .enabled
                    .then_some(Message::SetMidiEditorFilterValueLow)
            )
            .width(Length::Fixed(80.0)),
        text("High").size(11),
        text_input("", &app.midi_editor_filter_value_high)
            .on_input_maybe(
                app.midi_editor_pitch_bend_filter
                    .enabled
                    .then_some(Message::SetMidiEditorFilterValueHigh)
            )
            .width(Length::Fixed(80.0)),
    ]
    .spacing(ROW_GAP)
    .align_y(iced::Alignment::Center);
    let program_options = if app.midi_editor_program_change_filter.enabled {
        &MIDI_EDITOR_FILTER_VALUES[..]
    } else {
        &[]
    };
    let program_change_filter_row = row![
        checkbox(app.midi_editor_program_change_filter.enabled)
            .label("Program")
            .on_toggle(Message::SetMidiEditorFilterProgramEnabled),
        text("Low").size(11),
        pick_list(
            program_options,
            app.midi_editor_program_change_filter
                .range_initialized
                .then_some(app.midi_editor_program_change_filter.low),
            Message::SetMidiEditorFilterProgramLow,
        )
        .placeholder("")
        .width(Length::Fixed(80.0)),
        text("High").size(11),
        pick_list(
            program_options,
            app.midi_editor_program_change_filter
                .range_initialized
                .then_some(app.midi_editor_program_change_filter.high),
            Message::SetMidiEditorFilterProgramHigh,
        )
        .placeholder("")
        .width(Length::Fixed(80.0)),
    ]
    .spacing(ROW_GAP)
    .align_y(iced::Alignment::Center);
    let repeat_control: Element<'_, Message> = if repeat_filter_visible {
        pick_list(
            &MidiEditorFilterRepeat::OPTIONS[..],
            Some(app.midi_editor_position_filter.repeat_every),
            Message::SetMidiEditorFilterRepeatEvery,
        )
        .width(Length::Fixed(150.0))
        .into()
    } else {
        text_input("", "measure")
            .on_input_maybe(None::<fn(String) -> Message>)
            .width(Length::Fixed(150.0))
            .into()
    };
    let repeat_filter_row = row![
        text("Repeat every").width(Length::Fixed(118.0)),
        repeat_control,
    ]
    .spacing(ROW_GAP)
    .align_y(iced::Alignment::Center);
    let mut criteria_rows: Vec<Element<'_, Message>> = Vec::new();
    if note_filter_visible {
        criteria_rows.extend([pitch_filter.into(), velocity_filter.into()]);
    }
    if controller_filter_visible {
        criteria_rows.extend([controller_filter_row.into(), value_filter_row.into()]);
    }
    if pitch_bend_filter_visible {
        criteria_rows.push(pitch_bend_filter_row.into());
    }
    if program_change_filter_visible {
        criteria_rows.push(program_change_filter_row.into());
    }
    if position_filter_visible {
        criteria_rows.extend([position_filter.into(), repeat_filter_row.into()]);
    }
    if note_filter_visible {
        criteria_rows.push(length_filter.into());
    }
    container(
        column![
            header,
            filter_options,
            event_type_filter,
            first_row,
            second_row,
            column(criteria_rows).spacing(ROW_GAP),
            row![
                text("Draw events on channel").size(11),
                pick_list(
                    &MidiEditorChannelSelection::OPTIONS[1..17],
                    Some(MidiEditorChannelSelection::Channel(
                        app.midi_editor_draw_channel.min(15)
                    )),
                    |selection| match selection {
                        MidiEditorChannelSelection::Channel(channel) => {
                            Message::SetMidiEditorDrawChannel(channel)
                        }
                        _ => Message::SetMidiEditorDrawChannel(0),
                    },
                )
                .width(Length::Fixed(150.0)),
            ]
            .spacing(ROW_GAP)
            .align_y(iced::Alignment::Center),
            row![
                button("Set selection")
                    .width(Length::Fill)
                    .height(Length::Fixed(if touch_targets {
                        TOUCH_TARGET_MIN
                    } else {
                        28.0
                    }))
                    .on_press(Message::ApplyMidiEditorFilterSelection(
                        super::super::MidiEditorFilterSelectionOperation::Set,
                    )),
                button("Clear filter")
                    .width(Length::Fill)
                    .height(Length::Fixed(if touch_targets {
                        TOUCH_TARGET_MIN
                    } else {
                        28.0
                    }))
                    .on_press(Message::ClearMidiEditorFilter),
            ]
            .spacing(SPACING_XS),
            button("Add to selection")
                .width(Length::Fill)
                .height(Length::Fixed(if touch_targets {
                    TOUCH_TARGET_MIN
                } else {
                    28.0
                }))
                .on_press(Message::ApplyMidiEditorFilterSelection(
                    super::super::MidiEditorFilterSelectionOperation::Add,
                )),
            button("Remove from selection")
                .width(Length::Fill)
                .height(Length::Fixed(if touch_targets {
                    TOUCH_TARGET_MIN
                } else {
                    28.0
                }))
                .on_press(Message::ApplyMidiEditorFilterSelection(
                    super::super::MidiEditorFilterSelectionOperation::Remove,
                )),
            button("Set filter from selection")
                .width(Length::Fill)
                .height(Length::Fixed(if touch_targets {
                    TOUCH_TARGET_MIN
                } else {
                    28.0
                }))
                .on_press(Message::SetMidiEditorFilterFromSelection),
        ]
        .spacing(SPACING_XS),
    )
    .padding(SPACING_XS)
    .width(Length::Fill)
    .style(container::bordered_box)
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
    app: &'a App,
    ticks_per_beat: u64,
) -> Element<'a, Message> {
    let canvas = canvas_widget::Canvas::new(ControllerLane {
        item,
        item_id,
        selected: &app.midi_editor_selected_controllers,
        controller,
        lane_height,
        origin_tick: app.midi_editor_origin_tick,
        pixels_per_beat: app.midi_editor_pixels_per_beat,
        ticks_per_beat,
        playhead_tick: app.midi_editor_playhead_tick(item),
        snap: midi_snap_for_app(app),
    })
    .width(Length::Fill)
    .height(Length::Fixed(lane_height));
    lane_row(label, lane_height, canvas)
}

fn pitch_bend_lane<'a>(
    item: &'a MidiItem,
    item_id: ItemId,
    lane_height: f32,
    app: &'a App,
    ticks_per_beat: u64,
) -> Element<'a, Message> {
    let canvas = canvas_widget::Canvas::new(PitchBendLane {
        item,
        item_id,
        selected: &app.midi_editor_selected_pitch_bends,
        lane_height,
        origin_tick: app.midi_editor_origin_tick,
        pixels_per_beat: app.midi_editor_pixels_per_beat,
        ticks_per_beat,
        playhead_tick: app.midi_editor_playhead_tick(item),
        snap: midi_snap_for_app(app),
    })
    .width(Length::Fill)
    .height(Length::Fixed(lane_height));
    lane_row("Pitch Bend", lane_height, canvas)
}

struct ControllerLane<'a> {
    item: &'a MidiItem,
    item_id: ItemId,
    selected: &'a HashSet<MidiControllerData>,
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
    display_tick: u64,
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

    fn point_data(
        &self,
        point: Point,
        ignore_snap: bool,
        channel: u8,
    ) -> (MidiControllerData, u64) {
        let mapping = self.mapping();
        let display_tick = mapping
            .snap_tick(mapping.tick_at_x(point.x), ignore_snap)
            .max(self.item.source_offset_ticks())
            .min(
                self.item
                    .source_offset_ticks()
                    .saturating_add(self.item.length_ticks())
                    .saturating_sub(1),
            );
        (
            MidiControllerData {
                channel,
                controller: self.controller,
                tick: source_edit_tick(self.item, display_tick),
                value: self.value_at_y(point.y),
            },
            display_tick,
        )
    }

    fn value_at_y(&self, y: f32) -> u8 {
        if self.controller == 64 {
            if y < self.lane_height / 2.0 { 127 } else { 0 }
        } else {
            value_at_y(y, self.lane_height)
        }
    }

    fn closest_point(&self, point: Point) -> Option<(usize, u64)> {
        let mapping = self.mapping();
        let item_start = self.item.source_offset_ticks();
        let item_end = item_start.saturating_add(self.item.length_ticks());
        self.item
            .controllers()
            .iter()
            .enumerate()
            .filter(|(_, controller)| {
                controller.controller == self.controller
                    && self.snap.controller_visible(**controller)
            })
            .flat_map(|(index, controller)| {
                source_event_occurrences(self.item, controller.tick, item_start, item_end)
                    .map(move |tick| (index, tick, controller.value))
            })
            .filter_map(|(index, tick, value)| {
                let x_distance = mapping.x_at_tick(tick) - point.x;
                let y_distance = controller_y(value, self.lane_height) - point.y;
                let distance = x_distance.hypot(y_distance);
                (distance <= 12.0).then_some((index, tick, distance))
            })
            .min_by(|left, right| left.2.total_cmp(&right.2))
            .map(|(index, tick, _)| (index, tick))
    }
}

fn controller_target_is_duplicate(
    controllers: &[MidiControllerData],
    moving_index: Option<usize>,
    target: MidiControllerData,
) -> bool {
    controllers.iter().enumerate().any(|(index, point)| {
        Some(index) != moving_index
            && point.channel == target.channel
            && point.controller == target.controller
            && point.tick == target.tick
    })
}

fn pitch_bend_target_is_duplicate(
    bends: &[MidiPitchBendData],
    moving_index: Option<usize>,
    target: MidiPitchBendData,
) -> bool {
    bends.iter().enumerate().any(|(index, bend)| {
        Some(index) != moving_index && bend.channel == target.channel && bend.tick == target.tick
    })
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
                let channel = drag
                    .original
                    .map_or(self.snap.draw_channel, |original| original.channel);
                (drag.current, drag.display_tick) =
                    self.point_data(point, state.modifiers.shift(), channel);
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
                if let Some((index, display_tick)) = self.closest_point(point) {
                    let original = controllers[index];
                    let additive = state.modifiers.command() || state.modifiers.control();
                    if additive {
                        return Some(
                            canvas::Action::publish(Message::SelectMidiEditorEventSelection(
                                MidiEditorEventListCursor::Controller(original),
                                true,
                            ))
                            .and_capture(),
                        );
                    }
                    state.drag = Some(ControllerDrag {
                        controllers,
                        index: Some(index),
                        original: Some(original),
                        current: original,
                        display_tick,
                    });
                    return Some(
                        canvas::Action::publish(Message::SelectMidiEditorEventSelection(
                            MidiEditorEventListCursor::Controller(original),
                            false,
                        ))
                        .and_capture(),
                    );
                } else {
                    let (current, display_tick) =
                        self.point_data(point, state.modifiers.shift(), self.snap.draw_channel);
                    state.drag = Some(ControllerDrag {
                        controllers,
                        index: None,
                        original: None,
                        current,
                        display_tick,
                    });
                }
                Some(canvas::Action::capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                let point = cursor.position_in(bounds)?;
                let Some((index, _)) = self.closest_point(point) else {
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
                let channel = drag
                    .original
                    .map_or(self.snap.draw_channel, |original| original.channel);
                (drag.current, drag.display_tick) =
                    self.point_data(point, state.modifiers.shift(), channel);
                Some(canvas::Action::request_redraw())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let mut drag = state.drag.take()?;
                if let Some(point) = cursor.position_in(bounds) {
                    let channel = drag
                        .original
                        .map_or(self.snap.draw_channel, |original| original.channel);
                    (drag.current, drag.display_tick) =
                        self.point_data(point, state.modifiers.shift(), channel);
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
                        "CC points on the same channel cannot share the same tick".to_owned(),
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
        let end_tick = mapping.tick_at_x(bounds.width);
        for tick in mapping.grid_tick_positions(self.origin_tick, end_tick) {
            let x = mapping.x_at_tick(tick);
            if (0.0..=bounds.width).contains(&x) {
                let path = canvas::Path::line(Point::new(x, 0.0), Point::new(x, bounds.height));
                frame.stroke(
                    &path,
                    canvas::Stroke::default().with_color(Color::from_rgb8(43, 48, 51)),
                );
            }
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
        let item_start = self.item.source_offset_ticks();
        let item_end = item_start.saturating_add(self.item.length_ticks());
        let prior_window_start = if self.item.is_loop_source() {
            item_start.max(
                mapping
                    .origin_tick
                    .saturating_sub(self.item.source_loop_length_ticks()),
            )
        } else {
            item_start
        };
        let mut prior_events = controllers
            .iter()
            .filter(|controller| {
                controller.controller == self.controller
                    && self.snap.controller_visible(**controller)
            })
            .flat_map(|controller| {
                source_event_occurrences(
                    self.item,
                    controller.tick,
                    prior_window_start,
                    mapping.origin_tick,
                )
                .map(move |tick| (tick, controller.value))
            })
            .collect::<Vec<_>>();
        prior_events.sort_unstable_by_key(|(tick, _)| *tick);
        let mut value = prior_events.last().map_or(0, |(_, value)| *value);
        let mut segment_start = 0.0;
        let mut visible_events = controllers
            .iter()
            .filter(|controller| {
                controller.controller == self.controller
                    && self.snap.controller_visible(**controller)
            })
            .flat_map(|controller| {
                source_event_occurrences(
                    self.item,
                    controller.tick,
                    mapping.origin_tick,
                    end_tick.saturating_add(1).min(item_end),
                )
                .map(move |tick| (tick, controller.value, *controller))
            })
            .collect::<Vec<_>>();
        visible_events.sort_unstable_by_key(|(tick, _, _)| *tick);
        for (event_tick, event_value, event) in visible_events {
            let x = mapping.x_at_tick(event_tick).clamp(0.0, bounds.width);
            let old_y = controller_y(value, bounds.height);
            let new_y = controller_y(event_value, bounds.height);
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
            let selected = self.selected.contains(&event);
            let point_size = if selected { 11.0 } else { 8.0 };
            frame.fill_rectangle(
                Point::new(x - point_size / 2.0, new_y - point_size / 2.0),
                Size::new(point_size, point_size),
                if selected {
                    Color::from_rgb8(255, 199, 105)
                } else {
                    Color::from_rgb8(175, 225, 196)
                },
            );
            segment_start = x;
            value = event_value;
        }
        if let Some(drag) = &state.drag {
            let invalid_target =
                controller_target_is_duplicate(&drag.controllers, drag.index, drag.current);
            let current_x = mapping
                .x_at_tick(drag.display_tick)
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
    selected: &'a HashSet<MidiPitchBendData>,
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
    display_tick: u64,
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

    fn point_data(&self, point: Point, ignore_snap: bool, channel: u8) -> (MidiPitchBendData, u64) {
        let mapping = self.mapping();
        let display_tick = mapping
            .snap_tick(mapping.tick_at_x(point.x), ignore_snap)
            .max(self.item.source_offset_ticks())
            .min(
                self.item
                    .source_offset_ticks()
                    .saturating_add(self.item.length_ticks())
                    .saturating_sub(1),
            );
        (
            MidiPitchBendData {
                channel,
                tick: source_edit_tick(self.item, display_tick),
                value: pitch_bend_value_at_y(point.y, self.lane_height),
            },
            display_tick,
        )
    }

    fn closest_point(&self, point: Point) -> Option<(usize, u64)> {
        let mapping = self.mapping();
        let item_start = self.item.source_offset_ticks();
        let item_end = item_start.saturating_add(self.item.length_ticks());
        self.item
            .pitch_bends()
            .iter()
            .enumerate()
            .filter(|(_, bend)| self.snap.pitch_bend_visible(**bend))
            .flat_map(|(index, bend)| {
                source_event_occurrences(self.item, bend.tick, item_start, item_end)
                    .map(move |tick| (index, tick, bend.value))
            })
            .filter_map(|(index, tick, value)| {
                let x_distance = mapping.x_at_tick(tick) - point.x;
                let y_distance = pitch_bend_y(value, self.lane_height) - point.y;
                let distance = x_distance.hypot(y_distance);
                (distance <= 12.0).then_some((index, tick, distance))
            })
            .min_by(|left, right| left.2.total_cmp(&right.2))
            .map(|(index, tick, _)| (index, tick))
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
                let channel = drag
                    .original
                    .map_or(self.snap.draw_channel, |original| original.channel);
                (drag.current, drag.display_tick) =
                    self.point_data(point, state.modifiers.shift(), channel);
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
                if let Some((index, display_tick)) = self.closest_point(point) {
                    let original = bends[index];
                    let additive = state.modifiers.command() || state.modifiers.control();
                    if additive {
                        return Some(
                            canvas::Action::publish(Message::SelectMidiEditorEventSelection(
                                MidiEditorEventListCursor::PitchBend(original),
                                true,
                            ))
                            .and_capture(),
                        );
                    }
                    state.drag = Some(PitchBendDrag {
                        bends,
                        index: Some(index),
                        original: Some(original),
                        current: original,
                        display_tick,
                    });
                    return Some(
                        canvas::Action::publish(Message::SelectMidiEditorEventSelection(
                            MidiEditorEventListCursor::PitchBend(original),
                            false,
                        ))
                        .and_capture(),
                    );
                } else {
                    let (current, display_tick) =
                        self.point_data(point, state.modifiers.shift(), self.snap.draw_channel);
                    state.drag = Some(PitchBendDrag {
                        bends,
                        index: None,
                        original: None,
                        current,
                        display_tick,
                    });
                }
                Some(canvas::Action::capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                let point = cursor.position_in(bounds)?;
                let Some((index, _)) = self.closest_point(point) else {
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
                let channel = drag
                    .original
                    .map_or(self.snap.draw_channel, |original| original.channel);
                (drag.current, drag.display_tick) =
                    self.point_data(point, state.modifiers.shift(), channel);
                Some(canvas::Action::request_redraw())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let mut drag = state.drag.take()?;
                if let Some(point) = cursor.position_in(bounds) {
                    let channel = drag
                        .original
                        .map_or(self.snap.draw_channel, |original| original.channel);
                    (drag.current, drag.display_tick) =
                        self.point_data(point, state.modifiers.shift(), channel);
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
                            "Pitch-bend points on the same channel cannot share the same tick"
                                .to_owned(),
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
        let end_tick = mapping.tick_at_x(bounds.width);
        for tick in mapping.grid_tick_positions(self.origin_tick, end_tick) {
            let x = mapping.x_at_tick(tick);
            if (0.0..=bounds.width).contains(&x) {
                let path = canvas::Path::line(Point::new(x, 0.0), Point::new(x, bounds.height));
                frame.stroke(
                    &path,
                    canvas::Stroke::default().with_color(Color::from_rgb8(43, 48, 51)),
                );
            }
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

        let item_start = self.item.source_offset_ticks();
        let item_end = item_start.saturating_add(self.item.length_ticks());
        let prior_window_start = if self.item.is_loop_source() {
            item_start.max(
                mapping
                    .origin_tick
                    .saturating_sub(self.item.source_loop_length_ticks()),
            )
        } else {
            item_start
        };
        let mut prior_events = self
            .item
            .pitch_bends()
            .iter()
            .filter(|bend| self.snap.pitch_bend_visible(**bend))
            .flat_map(|bend| {
                source_event_occurrences(
                    self.item,
                    bend.tick,
                    prior_window_start,
                    mapping.origin_tick,
                )
                .map(move |tick| (tick, bend.value))
            })
            .collect::<Vec<_>>();
        prior_events.sort_unstable_by_key(|(tick, _)| *tick);
        let mut value = prior_events.last().map_or(8192, |(_, value)| *value);
        let mut segment_start = 0.0;
        let mut visible_events = self
            .item
            .pitch_bends()
            .iter()
            .filter(|bend| self.snap.pitch_bend_visible(**bend))
            .flat_map(|bend| {
                source_event_occurrences(
                    self.item,
                    bend.tick,
                    mapping.origin_tick,
                    end_tick.saturating_add(1).min(item_end),
                )
                .map(move |tick| (tick, bend.value, *bend))
            })
            .collect::<Vec<_>>();
        visible_events.sort_unstable_by_key(|(tick, _, _)| *tick);
        for (event_tick, event_value, event) in visible_events {
            let x = mapping.x_at_tick(event_tick).clamp(0.0, bounds.width);
            let old_y = pitch_bend_y(value, bounds.height);
            let new_y = pitch_bend_y(event_value, bounds.height);
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
            let selected = self.selected.contains(&event);
            let point_size = if selected { 11.0 } else { 8.0 };
            frame.fill_rectangle(
                Point::new(x - point_size / 2.0, new_y - point_size / 2.0),
                Size::new(point_size, point_size),
                if selected {
                    Color::from_rgb8(255, 199, 105)
                } else {
                    Color::from_rgb8(177, 201, 245)
                },
            );
            segment_start = x;
            value = event_value;
        }
        if let Some(drag) = &state.drag {
            let invalid_target =
                pitch_bend_target_is_duplicate(&drag.bends, drag.index, drag.current);
            let x = mapping
                .x_at_tick(drag.display_tick)
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

    fn grid_tick_positions(self, start_tick: u64, end_tick: u64) -> Vec<u64> {
        let grid_ticks = self.grid_ticks();
        let first_index = (start_tick / grid_ticks).saturating_sub(2);
        let last_index = (end_tick / grid_ticks).saturating_add(3);
        let swing_amount_percent = self.snap.swing_amount_percent.clamp(-100, 100);
        let mut positions = Vec::new();
        for index in first_index..=last_index {
            let numerator = i128::from(index) * i128::from(grid_ticks) * 200
                + if index % 2 == 1 {
                    i128::from(grid_ticks) * i128::from(swing_amount_percent)
                } else {
                    0
                };
            if numerator < 0 {
                continue;
            }
            let tick = ((numerator + 100) / 200).min(i128::from(u64::MAX)) as u64;
            if tick <= end_tick.saturating_add(grid_ticks)
                && tick.saturating_add(grid_ticks) >= start_tick
            {
                positions.push(tick);
            }
        }
        positions.sort_unstable();
        positions.dedup();
        positions
    }

    fn snap_tick(self, tick: u64, ignore_snap: bool) -> u64 {
        let grid_ticks = self
            .snap
            .grid
            .tick_interval(self.ticks_per_beat.min(u64::from(u32::MAX)) as u32);
        snap_tick_to_grid(
            tick,
            grid_ticks,
            self.snap.enabled,
            ignore_snap,
            self.snap.swing_amount_percent,
        )
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
                    let visible_notes = self.visible_notes();
                    let Some(note_id) = velocity_note_at_x(
                        &visible_notes,
                        self.content_mapping(),
                        self.item.source_offset_ticks(),
                        self.item
                            .source_offset_ticks()
                            .saturating_add(self.item.length_ticks()),
                        point.x,
                    ) else {
                        return Some(canvas::Action::capture());
                    };
                    let note = visible_notes.iter().find(|note| note.id() == note_id)?;
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
                let note_hit = self.note_at_point(point, touch_targets, bounds.width);
                if self.region == RollRegion::Pitch && self.tool == MidiEditorTool::Erase {
                    state.last_empty_click = None;
                    return Some(
                        note_hit.map_or_else(canvas::Action::capture, |(note, _, _)| {
                            canvas::Action::publish(Message::DeleteMidiNotes(
                                self.item_id,
                                vec![note.id()],
                            ))
                        }),
                    );
                }
                if self.region == RollRegion::Pitch
                    && self.tool == MidiEditorTool::Draw
                    && note_hit.is_none()
                {
                    state.last_empty_click = None;
                    let mapping = self.mapping();
                    let local_tick = mapping.snap_tick(mapping.tick_at_x(point.x), false);
                    if let Some(data) = midi_note_insert_data(
                        self.item,
                        local_tick,
                        mapping.pitch_at_y(point.y),
                        mapping.grid_ticks(),
                        96,
                        self.snap.draw_channel,
                    ) {
                        return Some(canvas::Action::publish(Message::AddMidiNoteAt(
                            self.item_id,
                            data,
                        )));
                    }
                    return Some(canvas::Action::capture());
                }
                let mapping = self.mapping();
                let Some((note, resize, _occurrence_tick)) = note_hit else {
                    let now = Instant::now();
                    let double_click = state.last_empty_click.take().is_some_and(
                        |(previous_time, previous_point)| {
                            now.duration_since(previous_time) <= Duration::from_millis(450)
                                && (point.x - previous_point.x).hypot(point.y - previous_point.y)
                                    <= 5.0
                        },
                    );
                    if double_click {
                        let local_tick =
                            mapping.snap_tick(mapping.tick_at_x(point.x), state.modifiers.shift());
                        if let Some(data) = midi_note_insert_data(
                            self.item,
                            local_tick,
                            mapping.pitch_at_y(point.y),
                            mapping.grid_ticks(),
                            96,
                            self.snap.draw_channel,
                        ) {
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
                        let visible_notes = self.visible_notes();
                        velocity_note_at_x(
                            &visible_notes,
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
                        for note in self
                            .item
                            .notes()
                            .iter()
                            .filter(|note| self.snap.note_visible(note))
                        {
                            let y = mapping.y_at_pitch(self.snap.note_pitch(
                                self.item_id,
                                self.selected,
                                note,
                            ));
                            let note_bottom = y + self.pitch_row_height;
                            let note_duration = visible_note_duration(self.item, note, note.tick());
                            let (view_start, view_end) = self.content_tick_range(bounds.width);
                            for occurrence in source_event_occurrences(
                                self.item,
                                note.tick(),
                                view_start.saturating_sub(note_duration),
                                view_end,
                            ) {
                                let x = mapping.x_at_tick(occurrence);
                                let note_right = x + note_width_pixels(
                                    visible_note_duration(self.item, note, occurrence),
                                    self.ticks_per_beat,
                                    self.pixels_per_beat,
                                );
                                if x <= right
                                    && note_right >= left
                                    && y <= bottom
                                    && note_bottom >= top
                                {
                                    selected.insert(note.id());
                                }
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
                let source_bounds = midi_source_edit_bounds(self.item);
                let edits = drag
                    .notes
                    .iter()
                    .map(|(note_id, data)| {
                        (
                            *note_id,
                            note_drag_preview_data(&drag, *data, source_bounds.0, source_bounds.1),
                        )
                    })
                    .collect::<Vec<_>>();
                let invalid_target = edits.iter().any(|(id, data)| {
                    self.item
                        .notes()
                        .iter()
                        .find(|note| note.id() == *id)
                        .is_some_and(|_| {
                            data.tick < source_bounds.0
                                || data.tick.saturating_add(data.duration) > source_bounds.1
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
        let item_start_position = self.item.start_position();
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
        for tick in mapping.grid_tick_positions(self.origin_tick, end_tick) {
            let x = grid_left + mapping.x_at_tick(tick);
            if x >= grid_left && x <= bounds.width {
                let project_position = item_start_position.checked_add_ticks(tick);
                let position = project_position.and_then(|position| {
                    self.project.musical_position_at_tick(position.tick()).ok()
                });
                let on_project_tick =
                    project_position.is_some_and(|position| position.subtick() == 0);
                let measure = on_project_tick
                    && position.is_some_and(|position| {
                        position.beat() == 1 && position.tick_in_beat() == 0
                    });
                let beat = on_project_tick
                    && position.is_some_and(|position| position.tick_in_beat() == 0);
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
        let content_mapping = self.content_mapping();
        let (view_start, view_end) = self.content_tick_range(bounds.width);
        for note in self
            .item
            .notes()
            .iter()
            .filter(|note| self.snap.note_visible(note))
        {
            let pitch = self.snap.note_pitch(self.item_id, self.selected, note);
            if pitch > self.high_pitch
                || pitch < self.high_pitch.saturating_sub(self.pitch_rows - 1)
            {
                continue;
            }
            let y = grid_top + mapping.y_at_pitch(pitch);
            let source_duration = visible_note_duration(self.item, note, note.tick());
            for occurrence_tick in source_event_occurrences(
                self.item,
                note.tick(),
                view_start.saturating_sub(source_duration),
                view_end,
            ) {
                let x = grid_left + content_mapping.x_at_tick(occurrence_tick);
                let width = note_width_pixels(
                    visible_note_duration(self.item, note, occurrence_tick),
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
                    let source_bounds = midi_source_edit_bounds(self.item);
                    let preview = note_drag_preview_data(
                        drag,
                        note_data(note),
                        source_bounds.0,
                        source_bounds.1,
                    );
                    let iteration = source_loop_iteration(self.item, note.tick(), occurrence_tick);
                    let preview_tick = occurrence_source_tick(self.item, preview.tick, iteration);
                    let preview_x = grid_left + content_mapping.x_at_tick(preview_tick);
                    let preview_y = grid_top + mapping.y_at_pitch(preview.pitch);
                    let preview_duration =
                        source_note_duration(self.item, preview.tick, preview.duration).min(
                            self.item
                                .source_offset_ticks()
                                .saturating_add(self.item.length_ticks())
                                .saturating_sub(preview_tick),
                        );
                    let preview_width = note_width_pixels(
                        preview_duration,
                        self.ticks_per_beat,
                        self.pixels_per_beat,
                    );
                    let invalid_target = preview.tick < source_bounds.0
                        || preview.tick.saturating_add(preview.duration) > source_bounds.1;
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
                if let Some((_, resize, _)) = self.note_at_point(point, touch_targets, bounds.width)
                {
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
    fn visible_notes(&self) -> Vec<aaadaw_core::MidiNote> {
        self.item
            .notes()
            .iter()
            .filter(|note| self.snap.note_visible(note))
            .cloned()
            .collect()
    }

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
        let anchor_tick = drag
            .notes
            .iter()
            .find(|(note_id, _)| *note_id == drag.anchor_note_id)
            .map(|(_, note)| {
                if drag.resize {
                    note.tick.saturating_add(note.duration)
                } else {
                    note.tick
                }
            })
            .unwrap_or_default();
        let delta_tick = snap_delta(ticks, anchor_tick, self.mapping(), ignore_snap);
        if drag.velocity {
            drag.delta_velocity = velocity_delta(drag.start.y, point.y);
        } else if drag.resize {
            drag.delta_tick = delta_tick;
        } else {
            let delta_pitch = ((drag.start.y - point.y) / self.pitch_row_height).round() as i16;
            let (source_start, source_end) = midi_source_edit_bounds(self.item);
            (drag.delta_tick, drag.delta_pitch) = bounded_note_move_delta(
                &drag.notes,
                source_start,
                source_end,
                delta_tick,
                delta_pitch,
            );
        }
    }

    fn note_at_point(
        &self,
        point: Point,
        touch_targets: bool,
        width: f32,
    ) -> Option<(&aaadaw_core::MidiNote, bool, u64)> {
        let mapping = self.content_mapping();
        let (view_start, view_end) = self.content_tick_range(width);
        for note in self
            .item
            .notes()
            .iter()
            .rev()
            .filter(|note| self.snap.note_visible(note))
        {
            let duration = visible_note_duration(self.item, note, note.tick());
            for occurrence_tick in source_event_occurrences(
                self.item,
                note.tick(),
                view_start.saturating_sub(duration),
                view_end,
            ) {
                let left = mapping.x_at_tick(occurrence_tick);
                let note_width = note_width_pixels(
                    visible_note_duration(self.item, note, occurrence_tick),
                    self.ticks_per_beat,
                    self.pixels_per_beat,
                );
                let top =
                    mapping.y_at_pitch(self.snap.note_pitch(self.item_id, self.selected, note));
                if point.x >= left
                    && point.x <= left + note_width
                    && point.y >= top
                    && point.y < top + self.pitch_row_height
                {
                    return Some((
                        note,
                        point.x >= left + note_width - resize_handle_width(note_width),
                        occurrence_tick,
                    ));
                }
            }
        }
        if !touch_targets {
            return None;
        }
        self.item
            .notes()
            .iter()
            .rev()
            .filter(|note| self.snap.note_visible(note))
            .flat_map(|note| {
                let duration = visible_note_duration(self.item, note, note.tick());
                source_event_occurrences(
                    self.item,
                    note.tick(),
                    view_start.saturating_sub(duration),
                    view_end,
                )
                .map(move |occurrence_tick| (note, occurrence_tick))
            })
            .filter_map(|(note, occurrence_tick)| {
                let left = mapping.x_at_tick(occurrence_tick);
                let note_width = note_width_pixels(
                    visible_note_duration(self.item, note, occurrence_tick),
                    self.ticks_per_beat,
                    self.pixels_per_beat,
                );
                let top =
                    mapping.y_at_pitch(self.snap.note_pitch(self.item_id, self.selected, note));
                let hit_width = note_width.max(48.0);
                let hit_height = self.pitch_row_height.max(48.0);
                let hit_left = left - (hit_width - note_width) / 2.0;
                let hit_top = top - (hit_height - self.pitch_row_height) / 2.0;
                let in_touch_target = point.x >= hit_left
                    && point.x <= hit_left + hit_width
                    && point.y >= hit_top
                    && point.y <= hit_top + hit_height;
                in_touch_target.then_some((
                    note,
                    occurrence_tick,
                    (point.x - (left + note_width / 2.0))
                        .hypot(point.y - (top + self.pitch_row_height / 2.0)),
                ))
            })
            .min_by(|(_, _, left), (_, _, right)| left.total_cmp(right))
            .map(|(note, occurrence_tick, _)| (note, false, occurrence_tick))
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
        let visible_notes = self.visible_notes();
        let velocity_positions = velocity_handle_positions(
            &visible_notes,
            self.content_mapping(),
            self.item.source_offset_ticks(),
            self.item
                .source_offset_ticks()
                .saturating_add(self.item.length_ticks()),
        );
        for note in &visible_notes {
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

    fn content_tick_range(&self, width: f32) -> (u64, u64) {
        let mapping = self.content_mapping();
        (
            mapping.origin_tick,
            mapping.tick_at_x((width - KEY_WIDTH).max(0.0)),
        )
    }
}

fn snap_delta(delta_ticks: i64, anchor_tick: u64, mapping: RollMapping, ignore_snap: bool) -> i64 {
    if !mapping.snap.enabled || ignore_snap {
        return delta_ticks;
    }
    let target_tick = i128::from(anchor_tick) + i128::from(delta_ticks);
    if !(0..=i128::from(u64::MAX)).contains(&target_tick) {
        return delta_ticks;
    }
    let snapped_tick = mapping.snap_tick(target_tick as u64, false);
    (i128::from(snapped_tick) - i128::from(anchor_tick))
        .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

#[cfg(test)]
fn snap_tick(tick: u64, grid_ticks: Option<u64>, enabled: bool, ignore_snap: bool) -> u64 {
    snap_tick_to_grid(tick, grid_ticks, enabled, ignore_snap, 0)
}

fn note_data(note: &aaadaw_core::MidiNote) -> MidiNoteData {
    MidiNoteData {
        channel: note.channel(),
        pitch: note.pitch(),
        tick: note.tick(),
        duration: note.duration(),
        velocity: note.velocity(),
        off_velocity: note.off_velocity(),
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
    use crate::app::MidiTransposeScope;

    #[test]
    fn event_list_formats_pitch_bend_values_around_the_midi_center() {
        assert_eq!(format_midi_pitch_bend_value(0), "-8192");
        assert_eq!(format_midi_pitch_bend_value(4_096), "-4096");
        assert_eq!(format_midi_pitch_bend_value(8_192), "0");
        assert_eq!(format_midi_pitch_bend_value(10_000), "+1808");
        assert_eq!(format_midi_pitch_bend_value(16_383), "+8191");
    }

    #[test]
    fn event_list_columns_match_the_native_order_and_include_reaper_data() {
        assert_eq!(
            MIDI_EVENT_LIST_COLUMN_LABELS,
            [
                "Index",
                "Position",
                "Length",
                "Channel",
                "Type",
                "Parameter",
                "Value",
                "Off Value",
                "REAPER Data",
            ]
        );
        assert_eq!(
            MIDI_EVENT_LIST_COLUMN_LABELS.len(),
            MIDI_EVENT_LIST_COLUMN_WIDTHS.len()
        );
        let content_width = MIDI_EVENT_LIST_COLUMN_WIDTHS.iter().sum::<f32>()
            + 2.0 * (MIDI_EVENT_LIST_COLUMN_WIDTHS.len() - 1) as f32;
        assert_eq!(MIDI_EVENT_LIST_CONTENT_WIDTH, content_width);
    }

    #[test]
    fn opaque_meta_text_keeps_readable_newlines() {
        assert_eq!(
            midi_opaque_meta_text(b"Imported line one\nImported line two"),
            "Imported line one\nImported line two"
        );
        assert_eq!(
            midi_opaque_meta_text(b"bad\x01text"),
            "62 61 64 01 74 65 78 74"
        );
        assert_eq!(midi_opaque_meta_text(&[b'A', 0xFF, b'B']), "41 FF 42");
    }

    #[test]
    fn sysex_properties_frame_f0_messages_and_f7_continuations_distinctly() {
        assert_eq!(midi_sysex_frame_labels(0xF0), ("F0", Some("F7")));
        assert_eq!(midi_sysex_frame_labels(0xF7), ("F7", None));
    }

    #[test]
    fn structural_meta_events_use_named_rows_and_preserve_their_bytes() {
        let event = MidiOpaqueEventData {
            tick: 0,
            order: 0,
            kind: MidiOpaqueEventKind::Meta {
                kind: 0x59,
                data: vec![0x02, 0x00],
            },
        };

        assert_eq!(
            midi_opaque_event_label(&event),
            ("Key signature", "02 00".into())
        );
        assert_eq!(midi_meta_kind_name(0x51), "Tempo");
        assert_eq!(midi_meta_kind_name(0x58), "Time signature");
    }

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
    fn piano_roll_channel_filter_hides_notes_from_hit_testing() {
        let (project, item_id, _) = project_with_note(
            3_840,
            MidiNoteData {
                channel: 2,
                pitch: 60,
                tick: 0,
                duration: 240,
                velocity: 96,
                off_velocity: 0,
            },
        );
        let selected = HashSet::new();
        let mut roll = piano_roll_for_test(&project, item_id, &selected, MidiEditorTool::Select);
        roll.snap = MidiSnap {
            channel_selection: MidiEditorChannelSelection::Channel(1),
            channel_filter_enabled: true,
            ..MidiSnap::default()
        };
        assert!(
            roll.note_at_point(Point::new(2.0, 9.0), false, 500.0)
                .is_none()
        );

        roll.snap.channel_selection = MidiEditorChannelSelection::AllChannels;
        roll.snap.event_filter_enabled = true;
        roll.snap.event_type_filter = MidiEditorEventTypeFilter::ControlChange;
        assert!(
            roll.note_at_point(Point::new(2.0, 9.0), false, 500.0)
                .is_none()
        );
        roll.snap.event_filter_enabled = false;
        assert!(
            roll.note_at_point(Point::new(2.0, 9.0), false, 500.0)
                .is_some()
        );
        roll.snap.channel_filter_enabled = true;
        roll.snap.event_type_filter = MidiEditorEventTypeFilter::AllEvents;

        roll.snap.channel_selection = MidiEditorChannelSelection::Multichannel;
        roll.snap.channel_filter_enabled = false;
        roll.snap.event_filter_enabled = true;
        roll.snap.filter_channels = 0b0100;
        roll.snap.filter_inverted = true;
        assert!(
            roll.note_at_point(Point::new(2.0, 9.0), false, 500.0)
                .is_none()
        );
        roll.snap.filter_channels = 0b0010;
        assert!(
            roll.note_at_point(Point::new(2.0, 9.0), false, 500.0)
                .is_some()
        );

        roll.snap.event_filter_enabled = false;
        assert!(
            roll.note_at_point(Point::new(2.0, 9.0), false, 500.0)
                .is_some()
        );
    }

    #[test]
    fn expression_lanes_filter_controller_and_pitch_bend_hit_targets_by_channel() {
        let (mut project, item_id, _) = project_with_note(
            3_840,
            MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 0,
                duration: 240,
                velocity: 96,
                off_velocity: 0,
            },
        );
        project
            .apply(aaadaw_core::DawAction::SetMidiControllers {
                item_id,
                controllers: vec![
                    MidiControllerData {
                        channel: 0,
                        controller: 1,
                        tick: 0,
                        value: 64,
                    },
                    MidiControllerData {
                        channel: 1,
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
                        channel: 0,
                        tick: 0,
                        value: 8_192,
                    },
                    MidiPitchBendData {
                        channel: 1,
                        tick: 960,
                        value: 8_192,
                    },
                ],
            })
            .unwrap();
        let item = &project.midi_items()[0];
        let snap = MidiSnap {
            channel_selection: MidiEditorChannelSelection::Multichannel,
            channel_filter_enabled: true,
            filter_channels: 0b10,
            ..MidiSnap::default()
        };
        let mut controller_lane = ControllerLane {
            item,
            item_id,
            selected: &HashSet::new(),
            controller: 1,
            lane_height: 72.0,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            playhead_tick: None,
            snap,
        };
        assert!(
            controller_lane
                .closest_point(Point::new(0.0, controller_y(64, 72.0)))
                .is_none()
        );
        assert_eq!(
            controller_lane
                .closest_point(Point::new(96.0, controller_y(64, 72.0)))
                .map(|(index, _)| index),
            Some(1)
        );
        controller_lane.snap.event_filter_enabled = true;
        controller_lane.snap.event_type_filter = MidiEditorEventTypeFilter::Notes;
        assert!(
            controller_lane
                .closest_point(Point::new(96.0, controller_y(64, 72.0)))
                .is_none()
        );
        controller_lane.snap.event_type_filter = MidiEditorEventTypeFilter::ControlChange;
        assert_eq!(
            controller_lane
                .closest_point(Point::new(96.0, controller_y(64, 72.0)))
                .map(|(index, _)| index),
            Some(1)
        );
        controller_lane.snap = snap;

        let inverted_snap = MidiSnap {
            channel_filter_enabled: false,
            event_filter_enabled: true,
            filter_channels: 0b10,
            filter_inverted: true,
            ..snap
        };
        let inverted_controller_lane = ControllerLane {
            snap: inverted_snap,
            ..controller_lane
        };
        assert_eq!(
            inverted_controller_lane
                .closest_point(Point::new(0.0, controller_y(64, 72.0)))
                .map(|(index, _)| index),
            Some(0)
        );
        assert!(
            inverted_controller_lane
                .closest_point(Point::new(96.0, controller_y(64, 72.0)))
                .is_none()
        );

        let mut pitch_bend_lane = PitchBendLane {
            item,
            item_id,
            selected: &HashSet::new(),
            lane_height: 72.0,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            playhead_tick: None,
            snap,
        };
        assert!(
            pitch_bend_lane
                .closest_point(Point::new(0.0, pitch_bend_y(8_192, 72.0)))
                .is_none()
        );
        assert_eq!(
            pitch_bend_lane
                .closest_point(Point::new(96.0, pitch_bend_y(8_192, 72.0)))
                .map(|(index, _)| index),
            Some(1)
        );
        pitch_bend_lane.snap.event_filter_enabled = true;
        pitch_bend_lane.snap.event_type_filter = MidiEditorEventTypeFilter::Notes;
        assert!(
            pitch_bend_lane
                .closest_point(Point::new(96.0, pitch_bend_y(8_192, 72.0)))
                .is_none()
        );
        pitch_bend_lane.snap.event_type_filter = MidiEditorEventTypeFilter::Pitch;
        assert_eq!(
            pitch_bend_lane
                .closest_point(Point::new(96.0, pitch_bend_y(8_192, 72.0)))
                .map(|(index, _)| index),
            Some(1)
        );
        pitch_bend_lane.snap = snap;

        let inverted_pitch_bend_lane = PitchBendLane {
            snap: inverted_snap,
            ..pitch_bend_lane
        };
        assert_eq!(
            inverted_pitch_bend_lane
                .closest_point(Point::new(0.0, pitch_bend_y(8_192, 72.0)))
                .map(|(index, _)| index),
            Some(0)
        );
        assert!(
            inverted_pitch_bend_lane
                .closest_point(Point::new(96.0, pitch_bend_y(8_192, 72.0)))
                .is_none()
        );
    }

    #[test]
    fn looped_source_occurrences_render_and_hit_test_after_wrapping_the_offset() {
        let (mut project, item_id, _) = project_with_note(
            9_600,
            MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 960,
                duration: 120,
                velocity: 96,
                off_velocity: 0,
            },
        );
        project
            .apply(aaadaw_core::DawAction::EditMidiItemPlacement {
                item_id,
                start_tick: 0,
                length_ticks: 20_000,
                source_offset_ticks: 9_120,
            })
            .unwrap();
        project
            .apply(aaadaw_core::DawAction::SetMidiControllers {
                item_id,
                controllers: vec![MidiControllerData {
                    channel: 0,
                    controller: 1,
                    tick: 960,
                    value: 64,
                }],
            })
            .unwrap();
        project
            .apply(aaadaw_core::DawAction::SetMidiPitchBends {
                item_id,
                pitch_bends: vec![MidiPitchBendData {
                    channel: 0,
                    tick: 1_920,
                    value: 12_000,
                }],
            })
            .unwrap();
        let item = &project.midi_items()[0];

        assert_eq!(
            source_event_occurrences(item, 960, 0, u64::MAX).collect::<Vec<_>>(),
            [10_560, 20_160]
        );
        assert!(source_tick_is_visible(item, 960));
        assert_eq!(
            midi_note_insert_data(item, 1_440, 64, 240, 96, 0),
            Some(MidiNoteData {
                channel: 0,
                pitch: 64,
                tick: 960,
                duration: 240,
                velocity: 96,
                off_velocity: 0,
            })
        );
        assert_eq!(
            midi_note_insert_data(item, 1_440, 64, 240, 96, 7)
                .expect("inserted note preserves the selected MIDI channel")
                .channel,
            7
        );
        assert_eq!(
            midi_note_insert_data(item, 9_480, 64, 960, 96, 0),
            Some(MidiNoteData {
                channel: 0,
                pitch: 64,
                tick: 9_000,
                duration: 600,
                velocity: 96,
                off_velocity: 0,
            })
        );

        let selected = HashSet::new();
        let roll = piano_roll_for_test(&project, item_id, &selected, MidiEditorTool::Select);
        let hit = roll
            .note_at_point(Point::new(146.0, 9.0), false, 500.0)
            .expect("the wrapped source note should be editable in the piano roll");
        assert_eq!(hit.0.tick(), 960);
        assert_eq!(hit.2, 10_560);

        let controller_lane = ControllerLane {
            item,
            item_id,
            selected: &HashSet::new(),
            controller: 1,
            lane_height: 72.0,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            playhead_tick: None,
            snap: MidiSnap::default(),
        };
        assert_eq!(
            controller_lane.closest_point(Point::new(144.0, controller_y(64, 72.0))),
            Some((0, 10_560))
        );
        let (controller_edit, controller_display_tick) =
            controller_lane.point_data(Point::new(168.0, controller_y(96, 72.0)), true, 0);
        assert_eq!(controller_display_tick, 10_800);
        assert_eq!(controller_edit.tick, 1_200);
        let pitch_bend_lane = PitchBendLane {
            item,
            item_id,
            selected: &HashSet::new(),
            lane_height: 72.0,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            playhead_tick: None,
            snap: MidiSnap::default(),
        };
        assert_eq!(
            pitch_bend_lane.closest_point(Point::new(240.0, pitch_bend_y(12_000, 72.0))),
            Some((0, 11_520))
        );
        let (pitch_bend_edit, pitch_bend_display_tick) =
            pitch_bend_lane.point_data(Point::new(264.0, pitch_bend_y(10_000, 72.0)), true, 0);
        assert_eq!(pitch_bend_display_tick, 11_760);
        assert_eq!(pitch_bend_edit.tick, 2_160);
    }

    #[test]
    fn touch_hitboxes_reach_48dp_without_changing_desktop_hits() {
        let (project, item_id, _) = project_with_note(
            3_840,
            MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 480,
                duration: 120,
                velocity: 96,
                off_velocity: 0,
            },
        );
        let selected = HashSet::new();
        let roll = piano_roll_for_test(&project, item_id, &selected, MidiEditorTool::Select);
        let point = Point::new(70.0, 24.0);

        assert!(roll.note_at_point(point, false, 400.0).is_none());
        assert!(roll.note_at_point(point, true, 400.0).is_some());
    }

    #[test]
    fn touch_draw_and_erase_tools_edit_notes_with_single_taps() {
        let (project, item_id, _) = project_with_note(
            3_840,
            MidiNoteData {
                channel: 0,
                pitch: 60,
                tick: 2_880,
                duration: 240,
                velocity: 96,
                off_velocity: 0,
            },
        );
        let selected = HashSet::new();
        let mut draw_roll = piano_roll_for_test(&project, item_id, &selected, MidiEditorTool::Draw);
        draw_roll.snap.grid = SnapGrid::Sixteenth;
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
                channel: 0,
                pitch: 60,
                tick: 480,
                duration: 240,
                velocity: 96,
                off_velocity: 0,
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
                channel: 0,
                pitch: 60,
                tick: 0,
                duration: 120,
                velocity: 96,
                off_velocity: 0,
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
        app.timeline.midi_editor_snap_grid = SnapGrid::EighthTriplet;
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
        let _ = app.update(Message::Timeline(TimelineEvent::ToggleMidiEditorSnap));
        app.midi_editor_edit_cursor_tick = Some(190);
        app.midi_note_clipboard.notes = vec![MidiNoteData {
            channel: 0,
            pitch: 60,
            tick: 0,
            duration: 120,
            velocity: 96,
            off_velocity: 0,
        }];
        app.midi_note_clipboard.span_ticks = 120;

        let _ = app.update(Message::PasteMidiNotes(item_id));
        assert_eq!(app.project.midi_items()[0].notes()[0].tick(), 190);

        let _ = app.update(Message::Timeline(TimelineEvent::SetMidiEditorSnapGrid(
            SnapGrid::Sixteenth,
        )));
        let _ = app.update(Message::Timeline(TimelineEvent::ToggleMidiEditorSnap));
        assert_eq!(visible_edit_cursor_tick(&app), 480);

        let _ = app.update(Message::PasteMidiNotes(item_id));
        assert_eq!(app.project.midi_items()[0].notes()[1].tick(), 480);
    }

    #[test]
    fn piano_roll_wheel_maps_to_pitch_and_modifier_navigation() {
        let note = MidiNoteData {
            channel: 0,
            pitch: 60,
            tick: 480,
            duration: 240,
            velocity: 96,
            off_velocity: 0,
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
            channel: 0,
            pitch: 60,
            tick: 720,
            duration: 240,
            velocity: 96,
            off_velocity: 0,
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
            channel: 0,
            pitch: 60,
            tick: 0,
            duration: 24,
            velocity: 96,
            off_velocity: 0,
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
            channel: 0,
            pitch: 60,
            tick: 720,
            duration: 240,
            velocity: 96,
            off_velocity: 0,
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
            channel: 0,
            pitch: 60,
            tick: 0,
            duration: 240,
            velocity: 96,
            off_velocity: 0,
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
            selected: &HashSet::new(),
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
                    channel: 0,
                    controller: 1,
                    tick: 0,
                    value: 64,
                },
                display_tick: 0,
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
            selected: &HashSet::new(),
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
                    channel: 0,
                    tick: 0,
                    value: 8192,
                },
                display_tick: 0,
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
        let channel_one_controller = MidiControllerData {
            channel: 1,
            controller: 1,
            tick: 960,
            value: 64,
        };
        assert!(!controller_target_is_duplicate(
            &[channel_one_controller],
            None,
            MidiControllerData {
                channel: 0,
                ..channel_one_controller
            }
        ));
        assert!(controller_target_is_duplicate(
            &[channel_one_controller],
            None,
            channel_one_controller
        ));
        let channel_one_bend = MidiPitchBendData {
            channel: 1,
            tick: 960,
            value: 8192,
        };
        assert!(!pitch_bend_target_is_duplicate(
            &[channel_one_bend],
            None,
            MidiPitchBendData {
                channel: 0,
                ..channel_one_bend
            }
        ));
        assert!(pitch_bend_target_is_duplicate(
            &[channel_one_bend],
            None,
            channel_one_bend
        ));
        let note = MidiNoteData {
            channel: 0,
            pitch: 60,
            tick: 0,
            duration: 240,
            velocity: 96,
            off_velocity: 0,
        };
        let (mut project, item_id, _) = project_with_note(3_840, note);
        project
            .apply(aaadaw_core::DawAction::SetMidiControllers {
                item_id,
                controllers: vec![
                    MidiControllerData {
                        channel: 0,
                        controller: 1,
                        tick: 0,
                        value: 64,
                    },
                    MidiControllerData {
                        channel: 0,
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
                        channel: 0,
                        tick: 0,
                        value: 8192,
                    },
                    MidiPitchBendData {
                        channel: 0,
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
            selected: &HashSet::new(),
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
            selected: &HashSet::new(),
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
                    channel: 0,
                    pitch: 60,
                    tick: 0,
                    duration: 240,
                    velocity: 96,
                    off_velocity: 0,
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
            selected: &HashSet::new(),
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
                channel: 0,
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
            selected: &HashSet::new(),
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
                channel: 0,
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
            selected: &HashSet::new(),
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
            snap: MidiSnap {
                grid: SnapGrid::Sixteenth,
                ..MidiSnap::default()
            },
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
        assert_eq!(snap_delta(190, 0, mapping, false), 320);
        assert_eq!(snap_delta(190, 0, mapping, true), 190);

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
        assert_eq!(snap_delta(190, 0, disabled, false), 190);
    }

    #[test]
    fn midi_note_move_snaps_the_anchor_onto_swing_grid_points() {
        let mapping = RollMapping {
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            high_pitch: 60,
            snap: MidiSnap {
                grid: SnapGrid::Sixteenth,
                enabled: true,
                swing_amount_percent: 50,
                ..MidiSnap::default()
            },
        };

        assert_eq!(snap_delta(299, 0, mapping, false), 300);
        assert_eq!(snap_delta(179, 300, mapping, false), 180);
        assert_eq!(mapping.snap_tick(150, false), 0);
    }

    #[test]
    fn midi_editor_draw_grid_uses_the_same_swing_points_as_note_snapping() {
        let mapping = RollMapping {
            pitch_rows: PITCH_COUNT,
            pitch_row_height: NOTE_ROW_HEIGHT,
            origin_tick: 0,
            pixels_per_beat: 96.0,
            ticks_per_beat: 960,
            high_pitch: 60,
            snap: MidiSnap {
                grid: SnapGrid::Sixteenth,
                enabled: true,
                swing_amount_percent: 50,
                ..MidiSnap::default()
            },
        };
        let positions = mapping.grid_tick_positions(0, 600);
        assert!(positions.contains(&300));
        assert!(positions.contains(&480));
        assert!(!positions.contains(&240));
    }

    #[test]
    fn clicking_the_piano_roll_ruler_sets_the_snapped_edit_cursor() {
        let note = MidiNoteData {
            channel: 0,
            pitch: 60,
            tick: 0,
            duration: 240,
            velocity: 96,
            off_velocity: 0,
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
            channel: 0,
            pitch: 60,
            tick: 0,
            duration: 240,
            velocity: 96,
            off_velocity: 0,
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
            channel: 0,
            pitch: 60,
            tick: 480,
            duration: 240,
            velocity: 96,
            off_velocity: 0,
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
            channel: 0,
            pitch: 60,
            tick: 480,
            duration: 240,
            velocity: 96,
            off_velocity: 0,
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
            channel: 0,
            pitch: 60,
            tick: 0,
            duration: 480,
            velocity: 96,
            off_velocity: 0,
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
            selected: &HashSet::new(),
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
            selected: &HashSet::new(),
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
            selected: &HashSet::new(),
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
                channel: 0,
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
            selected: &HashSet::new(),
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
                channel: 0,
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
            selected: &HashSet::new(),
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
            selected: &HashSet::new(),
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
        let (point, display_tick) = lane.point_data(Point::new(96.0, 36.0), false, 0);
        assert_eq!(display_tick, 960);
        assert_eq!(
            point,
            MidiControllerData {
                channel: 0,
                controller: 1,
                tick: 960,
                value: 64,
            }
        );
        let (channel_five_point, _) = lane.point_data(Point::new(96.0, 36.0), false, 5);
        assert_eq!(channel_five_point.channel, 5);
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
            selected: &HashSet::new(),
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
                channel: 0,
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
            selected: &HashSet::new(),
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
                    channel: 0,
                    pitch: 60,
                    tick: 120,
                    duration: 240,
                    velocity: 96,
                    off_velocity: 0,
                }],
            })
            .unwrap();
        let existing_controllers = vec![
            MidiControllerData {
                channel: 0,
                controller: 1,
                tick: 0,
                value: 50,
            },
            MidiControllerData {
                channel: 0,
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
            selected: &HashSet::new(),
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
                    channel: 0,
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
                    channel: 0,
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
            channel: 0,
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
            selected: &HashSet::new(),
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
                channel: 0,
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
                    channel: 0,
                    pitch: 60,
                    tick: 120,
                    duration: 240,
                    velocity: 96,
                    off_velocity: 0,
                }],
            })
            .unwrap();
        let other_controllers = vec![MidiControllerData {
            channel: 0,
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
            selected: &HashSet::new(),
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
                channel: 0,
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
            selected: &HashSet::new(),
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
                channel: 0,
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
            selected: &HashSet::new(),
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
                    channel: 0,
                    pitch: 5,
                    tick: 20,
                    duration: 10,
                    velocity: 90,
                    off_velocity: 0,
                },
            ),
            (
                NoteId::from_value(2).unwrap(),
                MidiNoteData {
                    channel: 0,
                    pitch: 100,
                    tick: 50,
                    duration: 20,
                    velocity: 80,
                    off_velocity: 0,
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
                        channel: 0,
                        pitch: 0,
                        tick: 500,
                        duration: 100,
                        velocity: 90,
                        off_velocity: 0,
                    },
                    MidiNoteData {
                        channel: 0,
                        pitch: 5,
                        tick: 1_000,
                        duration: 100,
                        velocity: 80,
                        off_velocity: 0,
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
                        channel: 0,
                        pitch: 60,
                        tick: 0,
                        duration: 240,
                        velocity: 90,
                        off_velocity: 0,
                    },
                    MidiNoteData {
                        channel: 0,
                        pitch: 64,
                        tick: 0,
                        duration: 240,
                        velocity: 90,
                        off_velocity: 0,
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

    #[test]
    fn event_list_positions_and_lengths_use_project_bbt_ticks() {
        let common_time = TimeSignature::new(4, 4).expect("valid common time");
        assert_eq!(midi_event_list_position(0, 960, common_time), "1.1.00");
        assert_eq!(midi_event_list_position(480, 960, common_time), "1.1.50");
        assert_eq!(midi_event_list_position(960, 960, common_time), "1.2.00");
        assert_eq!(midi_event_list_position(3_840, 960, common_time), "2.1.00");
        assert_eq!(midi_event_list_length(240, 960, common_time), "0.0.25");
        assert_eq!(midi_event_list_length(960, 960, common_time), "0.1.00");
        assert_eq!(midi_event_list_length(3_840, 960, common_time), "1.0.00");
        assert_eq!(midi_event_list_optional_length(None, 960, common_time), "");
        assert_eq!(
            midi_event_list_optional_length(Some(240), 960, common_time),
            "0.0.25"
        );

        let six_eight = TimeSignature::new(6, 8).expect("valid compound meter");
        assert_eq!(midi_event_list_position(480, 960, six_eight), "1.2.00");
        assert_eq!(midi_event_list_length(2_880, 960, six_eight), "1.0.00");
    }

    #[test]
    fn imported_same_tick_event_list_matches_native_order_in_both_source_directions() {
        let fixtures: [(&[u8], &[&str]); 2] = [
            (
                include_bytes!(
                    "../../../../../docs/verification/reaper-parity/fixtures/midi-file-import-same-tick-order.mid"
                ),
                &[
                    "sysex",
                    "meta",
                    "program",
                    "pitch",
                    "controller",
                    "poly-aftertouch",
                    "channel-pressure",
                    "controller",
                    "note",
                ],
            ),
            (
                include_bytes!(
                    "../../../../../docs/verification/reaper-parity/fixtures/midi-file-import-same-tick-order-reversed.mid"
                ),
                &[
                    "meta",
                    "sysex",
                    "program",
                    "controller",
                    "channel-pressure",
                    "poly-aftertouch",
                    "controller",
                    "pitch",
                    "note",
                ],
            ),
        ];

        for (fixture, expected_order) in fixtures {
            let mut imported = aaadaw_core::decode_standard_midi(fixture)
                .unwrap()
                .to_project_import(960)
                .unwrap();
            let track = imported.tracks.remove(0);
            let mut project = Project::new();
            project
                .apply(aaadaw_core::DawAction::ImportMidiTrack {
                    start_tick: 0,
                    track,
                })
                .unwrap();
            let entries = midi_event_list_entries(&project.midi_items()[0], MidiSnap::default());
            let actual_order = entries
                .into_iter()
                .filter(|entry| entry.tick() == 240)
                .map(|entry| match entry {
                    MidiEventListEntry::Note(_) => "note",
                    MidiEventListEntry::Controller(_) | MidiEventListEntry::BankSelect(_, _) => {
                        "controller"
                    }
                    MidiEventListEntry::PitchBend(_) => "pitch",
                    MidiEventListEntry::ProgramChange(_) => "program",
                    MidiEventListEntry::PolyAftertouch(_) => "poly-aftertouch",
                    MidiEventListEntry::ChannelPressure(_) => "channel-pressure",
                    MidiEventListEntry::Opaque(_, event) => match &event.kind {
                        MidiOpaqueEventKind::SystemExclusive { .. } => "sysex",
                        MidiOpaqueEventKind::Meta { .. } => "meta",
                    },
                    MidiEventListEntry::End(_) => "end",
                })
                .collect::<Vec<_>>();
            assert_eq!(actual_order, expected_order);
        }
    }

    #[test]
    fn imported_opaque_only_events_keep_same_tick_source_order() {
        let mut project = Project::new();
        project
            .apply(aaadaw_core::DawAction::ImportMidiTrack {
                start_tick: 0,
                track: aaadaw_core::MidiImportedTrack {
                    source_index: 0,
                    has_channel_events: false,
                    name: "Opaque order".to_owned(),
                    length_ticks: 960,
                    notes: Vec::new(),
                    controllers: Vec::new(),
                    pitch_bends: Vec::new(),
                    program_changes: Vec::new(),
                    poly_aftertouches: Vec::new(),
                    channel_pressures: Vec::new(),
                    opaque_events: vec![
                        MidiOpaqueEventData {
                            tick: 240,
                            order: 0,
                            kind: MidiOpaqueEventKind::SystemExclusive {
                                status: 0xF0,
                                data: vec![0x43, 0x12, 0xF7],
                            },
                        },
                        MidiOpaqueEventData {
                            tick: 240,
                            order: 1,
                            kind: MidiOpaqueEventKind::Meta {
                                kind: 0x01,
                                data: b"text".to_vec(),
                            },
                        },
                    ],
                    event_order: Vec::new(),
                },
            })
            .unwrap();

        let actual_order = midi_event_list_entries(&project.midi_items()[0], MidiSnap::default())
            .into_iter()
            .filter(|entry| entry.tick() == 240)
            .map(|entry| match entry {
                MidiEventListEntry::Opaque(_, event) => match &event.kind {
                    MidiOpaqueEventKind::SystemExclusive { .. } => "sysex",
                    MidiOpaqueEventKind::Meta { .. } => "meta",
                },
                _ => panic!("opaque-only track should not contain channel events"),
            })
            .collect::<Vec<_>>();
        assert_eq!(actual_order, ["sysex", "meta"]);
    }

    #[test]
    fn event_list_orders_and_filters_the_modeled_event_types() {
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
                    channel: 1,
                    pitch: 64,
                    tick: 240,
                    duration: 120,
                    velocity: 90,
                    off_velocity: 37,
                }],
            })
            .unwrap();
        project
            .apply(aaadaw_core::DawAction::SetMidiControllers {
                item_id,
                controllers: vec![MidiControllerData {
                    controller: 1,
                    channel: 0,
                    tick: 120,
                    value: 70,
                }],
            })
            .unwrap();
        project
            .apply(aaadaw_core::DawAction::SetMidiPitchBends {
                item_id,
                pitch_bends: vec![MidiPitchBendData {
                    channel: 1,
                    tick: 120,
                    value: 12_000,
                }],
            })
            .unwrap();
        project
            .apply(aaadaw_core::DawAction::SetMidiProgramChanges {
                item_id,
                program_changes: vec![MidiProgramChangeData {
                    channel: 1,
                    tick: 120,
                    program: 41,
                }],
            })
            .unwrap();
        project
            .apply(aaadaw_core::DawAction::SetMidiPolyAftertouches {
                item_id,
                poly_aftertouches: vec![MidiPolyAftertouchData {
                    channel: 1,
                    tick: 240,
                    note: 64,
                    value: 37,
                }],
            })
            .unwrap();
        project
            .apply(aaadaw_core::DawAction::SetMidiChannelPressures {
                item_id,
                channel_pressures: vec![MidiChannelPressureData {
                    channel: 2,
                    tick: 360,
                    value: 74,
                }],
            })
            .unwrap();
        let item = &project.midi_items()[0];
        let events = midi_event_list_entries(item, MidiSnap::default());
        assert!(matches!(events[0], MidiEventListEntry::Controller(_)));
        assert!(matches!(events[1], MidiEventListEntry::PitchBend(_)));
        assert!(matches!(events[2], MidiEventListEntry::ProgramChange(_)));
        assert!(matches!(events[3], MidiEventListEntry::Note(_)));
        let MidiEventListEntry::Note(note) = events[3] else {
            unreachable!()
        };
        assert_eq!(note.off_velocity(), 37);
        assert!(matches!(events[4], MidiEventListEntry::PolyAftertouch(_)));
        assert!(matches!(events[5], MidiEventListEntry::ChannelPressure(_)));
        assert!(matches!(events[6], MidiEventListEntry::End(3_840)));

        let channel_one = MidiSnap {
            channel_selection: MidiEditorChannelSelection::Channel(1),
            channel_filter_enabled: true,
            ..MidiSnap::default()
        };
        let events = midi_event_list_entries(item, channel_one);
        assert_eq!(events.len(), 4);
        assert!(matches!(events[0], MidiEventListEntry::PitchBend(_)));
        assert!(matches!(events[1], MidiEventListEntry::ProgramChange(_)));
        assert!(matches!(events[2], MidiEventListEntry::Note(_)));
        assert!(matches!(events[3], MidiEventListEntry::PolyAftertouch(_)));

        let programs_only = MidiSnap {
            event_filter_enabled: true,
            event_type_filter: MidiEditorEventTypeFilter::ProgramChange,
            ..MidiSnap::default()
        };
        let events = midi_event_list_entries(item, programs_only);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], MidiEventListEntry::ProgramChange(_)));

        let poly_only = MidiSnap {
            event_filter_enabled: true,
            event_type_filter: MidiEditorEventTypeFilter::PolyAftertouch,
            ..MidiSnap::default()
        };
        let events = midi_event_list_entries(item, poly_only);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], MidiEventListEntry::PolyAftertouch(_)));

        let pressure_only = MidiSnap {
            event_filter_enabled: true,
            event_type_filter: MidiEditorEventTypeFilter::ChannelPressure,
            ..MidiSnap::default()
        };
        let events = midi_event_list_entries(item, pressure_only);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], MidiEventListEntry::ChannelPressure(_)));
    }

    #[test]
    fn event_list_filters_controller_parameter_and_inclusive_value_range() {
        let mut project = Project::new();
        project
            .apply(aaadaw_core::DawAction::CreateTrack {
                index: 0,
                name: "CC Filter".to_owned(),
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
            .apply(aaadaw_core::DawAction::SetMidiControllers {
                item_id,
                controllers: vec![
                    MidiControllerData {
                        controller: 7,
                        channel: 0,
                        tick: 0,
                        value: 64,
                    },
                    MidiControllerData {
                        controller: 7,
                        channel: 0,
                        tick: 240,
                        value: 100,
                    },
                    MidiControllerData {
                        controller: 10,
                        channel: 0,
                        tick: 480,
                        value: 64,
                    },
                    MidiControllerData {
                        controller: 7,
                        channel: 0,
                        tick: 720,
                        value: 127,
                    },
                ],
            })
            .unwrap();

        let snap = MidiSnap {
            event_filter_enabled: true,
            event_type_filter: MidiEditorEventTypeFilter::ControlChange,
            controller_filter: MidiEditorControllerFilter {
                controller_enabled: true,
                controller: 7,
                value_enabled: true,
                value_range_initialized: true,
                value_low: 64,
                value_high: 100,
            },
            ..MidiSnap::default()
        };
        let entries = midi_event_list_entries(&project.midi_items()[0], snap);
        assert_eq!(entries.len(), 2);
        assert!(
            matches!(entries[0], MidiEventListEntry::Controller(event) if event.controller == 7 && event.value == 64)
        );
        assert!(
            matches!(entries[1], MidiEventListEntry::Controller(event) if event.controller == 7 && event.value == 100)
        );
    }

    #[test]
    fn event_list_repeats_note_and_controller_position_filter_each_selected_period() {
        let mut project = Project::new();
        project
            .apply(aaadaw_core::DawAction::CreateTrack {
                index: 0,
                name: "CC Position Filter".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        project
            .apply(aaadaw_core::DawAction::InsertMidiItem {
                track_id,
                start_tick: 0,
                length_ticks: 15_360,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        project
            .apply(aaadaw_core::DawAction::SetMidiControllers {
                item_id,
                controllers: (0..=15_240)
                    .step_by(120)
                    .map(|tick| MidiControllerData {
                        controller: 7,
                        channel: 0,
                        tick,
                        value: 64,
                    })
                    .collect(),
            })
            .unwrap();
        project
            .apply(aaadaw_core::DawAction::AddMidiNotes {
                item_id,
                notes: (0..=15_240)
                    .step_by(120)
                    .map(|tick| aaadaw_core::MidiNoteData {
                        channel: 0,
                        pitch: 60,
                        tick,
                        duration: 120,
                        velocity: 64,
                        off_velocity: 0,
                    })
                    .collect(),
            })
            .unwrap();

        for (repeat_every, expected_count, expected_last_tick) in [
            (MidiEditorFilterRepeat::Measure, 4, 12_000),
            (MidiEditorFilterRepeat::Half, 8, 13_920),
            (MidiEditorFilterRepeat::Quarter, 16, 14_880),
        ] {
            let snap = MidiSnap {
                event_filter_enabled: true,
                event_type_filter: MidiEditorEventTypeFilter::AllEvents,
                position_filter: MidiEditorPositionFilter {
                    enabled: true,
                    range_initialized: true,
                    low: 125_000_000,
                    high: 125_000_000,
                    repeat_every,
                },
                ..MidiSnap::default()
            };
            let entries = midi_event_list_entries(&project.midi_items()[0], snap);
            let matched_controller_ticks = entries
                .into_iter()
                .filter_map(|entry| match entry {
                    MidiEventListEntry::Controller(event) => Some(event.tick),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let matched_note_ticks = midi_event_list_entries(&project.midi_items()[0], snap)
                .into_iter()
                .filter_map(|entry| match entry {
                    MidiEventListEntry::Note(note) => Some(note.tick()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            for matched_ticks in [&matched_controller_ticks, &matched_note_ticks] {
                assert_eq!(matched_ticks.len(), expected_count);
                assert_eq!(matched_ticks.first(), Some(&480));
                assert_eq!(matched_ticks.last(), Some(&expected_last_tick));
                assert_eq!(matched_controller_ticks, matched_note_ticks);
            }
        }

        for (repeat_every, expected_ticks) in [
            (
                MidiEditorFilterRepeat::Measure,
                vec![2_880, 6_720, 10_560, 14_400],
            ),
            (MidiEditorFilterRepeat::Quarter, Vec::new()),
        ] {
            let snap = MidiSnap {
                event_filter_enabled: true,
                event_type_filter: MidiEditorEventTypeFilter::AllEvents,
                position_filter: MidiEditorPositionFilter {
                    enabled: true,
                    range_initialized: true,
                    low: 750_000_000,
                    high: 750_000_000,
                    repeat_every,
                },
                ..MidiSnap::default()
            };
            let entries = midi_event_list_entries(&project.midi_items()[0], snap);
            let matched_controller_ticks = entries
                .iter()
                .filter_map(|entry| match entry {
                    MidiEventListEntry::Controller(event) => Some(event.tick),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let matched_note_ticks = entries
                .iter()
                .filter_map(|entry| match entry {
                    MidiEventListEntry::Note(note) => Some(note.tick()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(matched_controller_ticks, expected_ticks);
            assert_eq!(matched_note_ticks, expected_ticks);
        }
    }

    #[test]
    fn event_list_shows_text_marker_and_sysex_item_events() {
        let mut project = Project::new();
        project
            .apply(aaadaw_core::DawAction::CreateTrack {
                index: 0,
                name: "Lead".to_owned(),
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
            .apply(aaadaw_core::DawAction::SetMidiOpaqueEvents {
                item_id,
                events: vec![
                    MidiOpaqueEventData {
                        tick: 0,
                        order: 0,
                        kind: MidiOpaqueEventKind::Meta {
                            kind: 0x03,
                            data: b"Lead".to_vec(),
                        },
                    },
                    MidiOpaqueEventData {
                        tick: 120,
                        order: 1,
                        kind: MidiOpaqueEventKind::Meta {
                            kind: 0x01,
                            data: b"Verse text".to_vec(),
                        },
                    },
                    MidiOpaqueEventData {
                        tick: 240,
                        order: 2,
                        kind: MidiOpaqueEventKind::Meta {
                            kind: 0x06,
                            data: b"Cue A".to_vec(),
                        },
                    },
                    MidiOpaqueEventData {
                        tick: 360,
                        order: 3,
                        kind: MidiOpaqueEventKind::SystemExclusive {
                            status: 0xF0,
                            data: vec![0x43, 0x12, 0xF7],
                        },
                    },
                ],
            })
            .unwrap();

        let item = &project.midi_items()[0];
        let events = midi_event_list_entries(item, MidiSnap::default());
        assert_eq!(events.len(), 5);
        assert!(matches!(events[0], MidiEventListEntry::Opaque(0, _)));
        assert_eq!(
            midi_opaque_event_label(item.opaque_events().first().unwrap()),
            ("Track name", "Lead".to_owned())
        );
        assert_eq!(
            midi_opaque_event_label(item.opaque_events().get(1).unwrap()),
            ("Text event", "Verse text".to_owned())
        );
        assert_eq!(
            midi_opaque_event_label(item.opaque_events().get(2).unwrap()),
            ("Marker", "Cue A".to_owned())
        );
        assert_eq!(
            midi_opaque_event_label(item.opaque_events().get(3).unwrap()),
            ("SYSEX", "F0 43 12 F7".to_owned())
        );
        assert!(matches!(events[4], MidiEventListEntry::End(960)));
        assert_eq!(events[0].cursor(), MidiEditorEventListCursor::Opaque(0));

        let metadata_only = MidiSnap {
            event_filter_enabled: true,
            event_type_filter: MidiEditorEventTypeFilter::SysExMeta,
            ..MidiSnap::default()
        };
        let events = midi_event_list_entries(item, metadata_only);
        assert_eq!(events.len(), 4);
        assert!(matches!(events[0], MidiEventListEntry::Opaque(0, _)));
        assert!(matches!(events[1], MidiEventListEntry::Opaque(1, _)));
        assert!(matches!(events[2], MidiEventListEntry::Opaque(2, _)));
        assert!(matches!(events[3], MidiEventListEntry::Opaque(3, _)));

        let metadata_ignores_channel_selection = MidiSnap {
            channel_filter_enabled: true,
            event_filter_enabled: true,
            event_type_filter: MidiEditorEventTypeFilter::SysExMeta,
            channel_selection: MidiEditorChannelSelection::Channel(7),
            ..MidiSnap::default()
        };
        let events = midi_event_list_entries(item, metadata_ignores_channel_selection);
        assert_eq!(events.len(), 4);
        assert!(matches!(events[0], MidiEventListEntry::Opaque(0, _)));
        assert!(matches!(events[1], MidiEventListEntry::Opaque(1, _)));
        assert!(matches!(events[2], MidiEventListEntry::Opaque(2, _)));
        assert!(matches!(events[3], MidiEventListEntry::Opaque(3, _)));

        let inverted_metadata = MidiSnap {
            event_filter_enabled: true,
            filter_inverted: true,
            event_type_filter: MidiEditorEventTypeFilter::SysExMeta,
            ..MidiSnap::default()
        };
        let events = midi_event_list_entries(item, inverted_metadata);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], MidiEventListEntry::End(960)));
    }

    #[test]
    fn event_list_combines_same_tick_bank_select_and_labels_program_bank() {
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
        let bank_msb = MidiControllerData {
            controller: 0,
            channel: 2,
            tick: 480,
            value: 12,
        };
        let bank_lsb = MidiControllerData {
            controller: 32,
            value: 23,
            ..bank_msb
        };
        let unrelated_cc = MidiControllerData {
            controller: 7,
            value: 90,
            ..bank_msb
        };
        let unpaired_msb = MidiControllerData {
            tick: 600,
            value: 5,
            ..bank_msb
        };
        project
            .apply(aaadaw_core::DawAction::SetMidiControllers {
                item_id,
                controllers: vec![unrelated_cc, bank_msb, bank_lsb, unpaired_msb],
            })
            .unwrap();
        let program = MidiProgramChangeData {
            channel: 2,
            tick: 480,
            program: 42,
        };
        project
            .apply(aaadaw_core::DawAction::SetMidiProgramChanges {
                item_id,
                program_changes: vec![program],
            })
            .unwrap();

        let item = &project.midi_items()[0];
        let events = midi_event_list_entries(item, MidiSnap::default());
        assert_eq!(events.len(), 5);
        assert!(events.iter().any(|entry| matches!(
            entry,
            MidiEventListEntry::BankSelect(msb, lsb) if *msb == bank_msb && *lsb == bank_lsb
        )));
        assert!(events.iter().any(|entry| matches!(
            entry,
            MidiEventListEntry::Controller(event) if *event == unrelated_cc
        )));
        assert!(events.iter().any(|entry| matches!(
            entry,
            MidiEventListEntry::Controller(event) if *event == unpaired_msb
        )));
        assert_eq!(midi_event_bank_select_value(bank_msb, bank_lsb), 1_559);
        assert_eq!(
            midi_event_program_change_parameter(item, program),
            "12-23-42"
        );
        assert_eq!(
            midi_event_program_change_parameter(
                item,
                MidiProgramChangeData {
                    channel: 1,
                    ..program
                }
            ),
            "0-0-42"
        );
    }

    #[test]
    fn transpose_preview_follows_scope_and_rejects_the_whole_invalid_range() {
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
                        channel: 0,
                        pitch: 60,
                        tick: 0,
                        duration: 120,
                        velocity: 90,
                        off_velocity: 0,
                    },
                    MidiNoteData {
                        channel: 1,
                        pitch: 72,
                        tick: 240,
                        duration: 120,
                        velocity: 91,
                        off_velocity: 0,
                    },
                ],
            })
            .unwrap();
        let notes = project.midi_items()[0].notes();
        let selected = HashSet::from([notes[0].id()]);
        let selected_preview = MidiSnap {
            transpose_preview: Some(MidiTransposePreview {
                item_id,
                scope: MidiTransposeScope::SelectedNotes,
                semitones: 2,
                range_valid: true,
            }),
            ..MidiSnap::default()
        };
        assert_eq!(
            selected_preview.note_pitch(item_id, &selected, &notes[0]),
            62
        );
        assert_eq!(
            selected_preview.note_pitch(item_id, &selected, &notes[1]),
            72
        );

        let all_preview = MidiSnap {
            transpose_preview: Some(MidiTransposePreview {
                item_id,
                scope: MidiTransposeScope::AllNotes,
                semitones: 2,
                range_valid: true,
            }),
            ..MidiSnap::default()
        };
        assert_eq!(all_preview.note_pitch(item_id, &selected, &notes[0]), 62);
        assert_eq!(all_preview.note_pitch(item_id, &selected, &notes[1]), 74);

        let invalid_all_preview = MidiSnap {
            transpose_preview: Some(MidiTransposePreview {
                item_id,
                scope: MidiTransposeScope::AllNotes,
                semitones: 1,
                range_valid: false,
            }),
            ..MidiSnap::default()
        };
        assert_eq!(
            invalid_all_preview.note_pitch(item_id, &selected, &notes[0]),
            60
        );
        assert_eq!(
            invalid_all_preview.note_pitch(item_id, &selected, &notes[1]),
            72
        );
    }
}
