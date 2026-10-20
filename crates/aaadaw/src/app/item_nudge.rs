use super::{App, Message, commands};
use crate::project_time;
use aaadaw_core::{
    AudioItem, DawAction, ItemId, MIDI_ITEM_SUBTICKS_PER_TICK, MidiItem, MidiItemPosition,
};
use aaadaw_storage::ProjectStore;
use iced::Task;
use iced::widget::combo_box;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Mode {
    #[default]
    Nudge,
    Set,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Nudge => "Nudge",
            Self::Set => "Set",
        })
    }
}

impl Mode {
    pub(super) const ALL: [Self; 2] = [Self::Nudge, Self::Set];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Unit {
    #[default]
    Milliseconds,
    Samples,
    Time,
    Grid,
    MeasuresBeats,
    Frames,
    Pixels,
    ItemLengths,
    ItemSelections,
    Notes,
}

impl std::fmt::Display for Unit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Milliseconds => "milliseconds",
            Self::Samples => "samples",
            Self::Time => "minutes:seconds",
            Self::Grid => "grid units",
            Self::MeasuresBeats => "measures.beats",
            Self::Frames => "frames",
            Self::Pixels => "pixels",
            Self::ItemLengths => "item lengths",
            Self::ItemSelections => "item selections",
            Self::Notes => "notes",
        })
    }
}

impl Unit {
    pub(super) const NUDGE_OPTIONS: [Self; 10] = [
        Self::Milliseconds,
        Self::Time,
        Self::Grid,
        Self::MeasuresBeats,
        Self::Samples,
        Self::Frames,
        Self::Pixels,
        Self::ItemLengths,
        Self::ItemSelections,
        Self::Notes,
    ];

    pub(super) const SET_OPTIONS: [Self; 5] = [
        Self::Time,
        Self::MeasuresBeats,
        Self::Samples,
        Self::Frames,
        Self::Pixels,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum NoteValue {
    TwoFiftySixth,
    OneTwentyEighth,
    SixtyFourth,
    ThirtySecondTriplet,
    ThirtySecond,
    SixteenthTriplet,
    Sixteenth,
    EighthTriplet,
    Eighth,
    #[default]
    Quarter,
    QuarterTriplet,
    Half,
    Whole,
}

impl NoteValue {
    pub(crate) const ALL: [Self; 13] = [
        Self::TwoFiftySixth,
        Self::OneTwentyEighth,
        Self::SixtyFourth,
        Self::ThirtySecondTriplet,
        Self::ThirtySecond,
        Self::SixteenthTriplet,
        Self::Sixteenth,
        Self::EighthTriplet,
        Self::Eighth,
        Self::QuarterTriplet,
        Self::Quarter,
        Self::Half,
        Self::Whole,
    ];

    fn quarter_notes(self) -> f64 {
        match self {
            Self::TwoFiftySixth => 1.0 / 64.0,
            Self::OneTwentyEighth => 1.0 / 32.0,
            Self::SixtyFourth => 1.0 / 16.0,
            Self::ThirtySecondTriplet => 1.0 / 12.0,
            Self::ThirtySecond => 1.0 / 8.0,
            Self::SixteenthTriplet => 1.0 / 6.0,
            Self::Sixteenth => 1.0 / 4.0,
            Self::EighthTriplet => 1.0 / 3.0,
            Self::Eighth => 1.0 / 2.0,
            Self::QuarterTriplet => 2.0 / 3.0,
            Self::Quarter => 1.0,
            Self::Half => 2.0,
            Self::Whole => 4.0,
        }
    }

    fn ticks(self, ppq: u32) -> f64 {
        f64::from(ppq) * self.quarter_notes()
    }
}

impl std::fmt::Display for NoteValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::TwoFiftySixth => "1/256",
            Self::OneTwentyEighth => "1/128",
            Self::SixtyFourth => "1/64",
            Self::ThirtySecondTriplet => "1/32T",
            Self::ThirtySecond => "1/32",
            Self::SixteenthTriplet => "1/16T",
            Self::Sixteenth => "1/16",
            Self::EighthTriplet => "1/8T",
            Self::Eighth => "1/8",
            Self::QuarterTriplet => "1/4T",
            Self::Quarter => "1/4",
            Self::Half => "1/2",
            Self::Whole => "whole",
        })
    }
}

impl NoteValue {
    pub(super) fn options() -> combo_box::State<Self> {
        combo_box::State::new(Self::ALL.to_vec())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Target {
    #[default]
    Position,
    LeftTrim,
    LeftEdge,
    EndPosition,
    RightTrim,
    Contents,
    Duplicate,
    EditCursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MidiEdgeEditMode {
    Nudge,
    Set,
}

impl std::fmt::Display for Target {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Position => "position",
            Self::LeftTrim => "left trim",
            Self::LeftEdge => "left edge",
            Self::EndPosition => "end position",
            Self::RightTrim => "right trim",
            Self::Contents => "contents",
            Self::Duplicate => "duplicate",
            Self::EditCursor => "edit cursor",
        })
    }
}

impl Target {
    pub(super) const ALL: [Self; 8] = [
        Self::Position,
        Self::LeftTrim,
        Self::LeftEdge,
        Self::EndPosition,
        Self::RightTrim,
        Self::Contents,
        Self::Duplicate,
        Self::EditCursor,
    ];
}

#[derive(Debug, Clone)]
pub(super) struct Dialog {
    pub(super) mode: Mode,
    pub(super) target: Target,
    pub(super) unit: Unit,
    pub(super) note_value: NoteValue,
    pub(super) value: String,
    pub(super) snap_to_unit: bool,
    pub(super) relative_set: bool,
    pub(super) error: Option<String>,
    pub(super) mode_options: combo_box::State<Mode>,
    pub(super) target_options: combo_box::State<Target>,
    pub(super) nudge_unit_options: combo_box::State<Unit>,
    pub(super) set_unit_options: combo_box::State<Unit>,
    pub(super) note_value_options: combo_box::State<NoteValue>,
}

impl Default for Dialog {
    fn default() -> Self {
        Self {
            mode: Mode::Nudge,
            target: Target::Position,
            unit: Unit::Milliseconds,
            note_value: NoteValue::Quarter,
            value: "1".to_owned(),
            snap_to_unit: false,
            relative_set: false,
            error: None,
            mode_options: combo_box::State::new(Mode::ALL.to_vec()),
            target_options: combo_box::State::new(Target::ALL.to_vec()),
            nudge_unit_options: combo_box::State::new(Unit::NUDGE_OPTIONS.to_vec()),
            set_unit_options: combo_box::State::new(Unit::SET_OPTIONS.to_vec()),
            note_value_options: NoteValue::options(),
        }
    }
}

impl Dialog {
    fn set_mode(&mut self, mode: Mode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        self.error = None;
        self.snap_to_unit = false;
        self.relative_set = false;
        self.unit = match mode {
            Mode::Nudge if unit_is_valid_for_nudge(self.unit) => self.unit,
            Mode::Nudge => Unit::Milliseconds,
            Mode::Set if unit_is_valid_for_set(self.unit) => self.unit,
            Mode::Set => Unit::Time,
        };
        self.value = default_value(mode, self.unit).to_owned();
    }

    fn set_unit(&mut self, unit: Unit) {
        self.unit = unit;
        self.error = None;
        self.value = default_value(self.mode, unit).to_owned();
    }
}

fn unit_is_valid_for_nudge(unit: Unit) -> bool {
    matches!(
        unit,
        Unit::Milliseconds
            | Unit::Time
            | Unit::Grid
            | Unit::MeasuresBeats
            | Unit::Samples
            | Unit::Frames
            | Unit::Pixels
            | Unit::ItemLengths
            | Unit::ItemSelections
            | Unit::Notes
    )
}

fn unit_is_valid_for_set(unit: Unit) -> bool {
    matches!(
        unit,
        Unit::Time | Unit::MeasuresBeats | Unit::Samples | Unit::Frames | Unit::Pixels
    )
}

fn default_value(mode: Mode, unit: Unit) -> &'static str {
    match (mode, unit) {
        (Mode::Nudge, Unit::MeasuresBeats) => "1.0",
        (Mode::Nudge, Unit::Pixels) => "0.5",
        (Mode::Nudge, _) => "1",
        (Mode::Set, Unit::Time) => "0:00.000",
        (Mode::Set, Unit::MeasuresBeats) => "1.1.00",
        (Mode::Set, Unit::Pixels) => "0.0",
        (Mode::Set, _) => "0",
    }
}

impl App {
    pub(super) fn open_item_nudge(&mut self) -> Task<Message> {
        if let Some(window_id) = self.item_nudge_window_id {
            return iced::window::gain_focus(window_id);
        }
        let (window_id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(528.0, 121.0),
            min_size: Some(iced::Size::new(528.0, 121.0)),
            ..iced::window::Settings::default()
        });
        self.item_nudge_window_id = Some(window_id);
        task.discard()
    }

    pub(super) fn close_item_nudge(&mut self) -> Task<Message> {
        self.item_nudge_window_id
            .take()
            .map_or_else(Task::none, iced::window::close)
    }

    pub(super) fn change_item_nudge_mode(&mut self, mode: Mode) {
        self.item_nudge_dialog.set_mode(mode);
        if mode == Mode::Set {
            self.refresh_item_nudge_set_value();
        }
    }

    fn refresh_item_nudge_set_value(&mut self) {
        let sample = if self.item_nudge_dialog.target == Target::EditCursor {
            self.timeline.edit_cursor_sample(&self.project).ok()
        } else {
            self.selected_item_start_samples()
                .ok()
                .and_then(|selected| selected.first().map(|(_, sample)| *sample))
        };
        if let Some(sample) = sample
            && let Some(value) = self.set_value_for_sample(self.item_nudge_dialog.unit, sample)
        {
            self.item_nudge_dialog.value = value;
        }
    }

    fn set_value_for_sample(&self, unit: Unit, sample: u64) -> Option<String> {
        let sample_rate = f64::from(self.project.settings().sample_rate());
        match unit {
            Unit::Time => Some(project_time::format_project_time(
                sample as f64 / sample_rate,
            )),
            Unit::Samples => Some(sample.to_string()),
            Unit::Frames => Some(format!(
                "{:.3}",
                sample as f64 * self.project.settings().frame_rate().frames_per_second()
                    / sample_rate
            )),
            Unit::Pixels => {
                let tick = self.project.tick_at_sample(sample).ok()?;
                let pixel = (tick as f64 - self.timeline.origin_tick as f64)
                    * f64::from(self.timeline.pixels_per_tick);
                Some(format!("{pixel:.1}"))
            }
            Unit::MeasuresBeats => {
                let tick = self.project.tick_at_sample(sample).ok()?;
                let position = self.project.musical_position_at_tick(tick).ok()?;
                let meter = self.project.time_signature_at_tick(tick);
                let ticks_per_beat =
                    u64::from(self.project.settings().ppq()) * 4 / u64::from(meter.denominator());
                if ticks_per_beat == 0 {
                    return None;
                }
                let fraction = (position.tick_in_beat() as f64 * 100.0 / ticks_per_beat as f64)
                    .round()
                    .clamp(0.0, 99.0) as u32;
                Some(format!(
                    "{}.{}.{fraction:02}",
                    position.measure(),
                    position.beat()
                ))
            }
            Unit::Milliseconds
            | Unit::Grid
            | Unit::ItemLengths
            | Unit::ItemSelections
            | Unit::Notes => None,
        }
    }

    pub(super) fn change_item_nudge_unit(&mut self, unit: Unit) {
        let valid = match self.item_nudge_dialog.mode {
            Mode::Nudge => unit_is_valid_for_nudge(unit),
            Mode::Set => unit_is_valid_for_set(unit),
        };
        if valid {
            self.item_nudge_dialog.set_unit(unit);
            if self.item_nudge_dialog.mode == Mode::Set {
                self.refresh_item_nudge_set_value();
            }
        }
    }

    pub(super) fn change_item_nudge_note_value(&mut self, note_value: NoteValue) {
        self.item_nudge_dialog.note_value = note_value;
        self.item_nudge_dialog.error = None;
    }

    pub(super) fn change_item_nudge_target(&mut self, target: Target) {
        self.item_nudge_dialog.target = target;
        if !target_supports_relative_set(target) {
            self.item_nudge_dialog.relative_set = false;
        }
        self.item_nudge_dialog.error = None;
        if self.item_nudge_dialog.mode == Mode::Set {
            self.refresh_item_nudge_set_value();
        }
    }

    pub(super) fn change_item_nudge_value(&mut self, value: String) {
        self.item_nudge_dialog.value = value;
        self.item_nudge_dialog.error = None;
    }

    pub(super) fn change_item_nudge_snap(&mut self, enabled: bool) {
        self.item_nudge_dialog.snap_to_unit = enabled;
    }

    pub(super) fn change_item_nudge_relative_set(&mut self, enabled: bool) {
        self.item_nudge_dialog.relative_set =
            enabled && target_supports_relative_set(self.item_nudge_dialog.target);
    }

    pub(super) fn item_nudge_get_cursor(&mut self) {
        if self.item_nudge_dialog.mode != Mode::Set {
            return;
        }
        let result = self
            .timeline
            .edit_cursor_sample(&self.project)
            .map_err(|error| error.to_string());
        match result {
            Ok(sample) => {
                if let Some(value) = self.set_value_for_sample(self.item_nudge_dialog.unit, sample)
                {
                    self.item_nudge_dialog.value = value;
                    self.item_nudge_dialog.error = None;
                } else {
                    self.item_nudge_dialog.error = Some(
                        "The selected unit cannot represent the edit cursor position".to_owned(),
                    );
                }
            }
            Err(error) => self.item_nudge_dialog.error = Some(error),
        }
    }

    pub(super) fn nudge_selected_items(&mut self, direction: i8) {
        if commands::project_edit_busy(self) {
            self.item_nudge_dialog.error = Some(
                "Stop playback or finish the current operation before moving items".to_owned(),
            );
            return;
        }
        if !matches!(direction, -1 | 1) {
            return;
        }
        if self.item_nudge_dialog.target == Target::EditCursor {
            if self.item_nudge_dialog.mode == Mode::Nudge {
                let delta = match self.nudge_delta_samples(direction) {
                    Ok(delta) if delta != 0 => delta,
                    Ok(_) => {
                        self.item_nudge_dialog.error =
                            Some("Enter a nudge greater than zero".to_owned());
                        return;
                    }
                    Err(error) => {
                        self.item_nudge_dialog.error = Some(error);
                        return;
                    }
                };
                self.move_edit_cursor_by_samples(delta);
            } else {
                self.item_nudge_dialog.error = Some(
                    "Setting the edit cursor is not supported by this dialog mode yet".to_owned(),
                );
            }
            return;
        }
        match self.item_nudge_dialog.mode {
            Mode::Nudge => {
                let delta = match self.nudge_delta_samples(direction) {
                    Ok(delta) if delta != 0 => delta,
                    Ok(_) => {
                        self.item_nudge_dialog.error =
                            Some("Enter a nudge greater than zero".to_owned());
                        return;
                    }
                    Err(error) => {
                        self.item_nudge_dialog.error = Some(error);
                        return;
                    }
                };
                let mut signed_delta = delta;
                if self.item_nudge_dialog.target == Target::Position
                    && self.item_nudge_dialog.snap_to_unit
                {
                    signed_delta = match self.snap_delta_to_selected_anchor(signed_delta) {
                        Ok(delta) => delta,
                        Err(error) => {
                            self.item_nudge_dialog.error = Some(error);
                            return;
                        }
                    };
                }
                match self.item_nudge_dialog.target {
                    Target::Position => self.shift_selected_items(signed_delta),
                    Target::Duplicate => self.nudge_duplicate_selected_items(signed_delta),
                    Target::LeftTrim
                    | Target::LeftEdge
                    | Target::EndPosition
                    | Target::RightTrim => {
                        if self.item_nudge_dialog.snap_to_unit {
                            signed_delta = match self.snap_delta_to_selected_anchor(signed_delta) {
                                Ok(delta) => delta,
                                Err(error) => {
                                    self.item_nudge_dialog.error = Some(error);
                                    return;
                                }
                            };
                        }
                        self.nudge_selected_edge_targets(signed_delta);
                    }
                    Target::Contents => {
                        self.nudge_selected_contents(signed_delta);
                    }
                    Target::EditCursor => unreachable!("edit cursor target handled above"),
                }
            }
            Mode::Set => self.apply_item_nudge_set(),
        }
    }

    pub(super) fn apply_item_nudge_set(&mut self) {
        if self.item_nudge_dialog.mode != Mode::Set {
            return;
        }
        match self.item_nudge_dialog.target {
            Target::Position => self.apply_set_position(),
            Target::Duplicate => self.apply_set_duplicate(),
            Target::LeftTrim | Target::LeftEdge | Target::EndPosition | Target::RightTrim => {
                self.apply_set_edge_target()
            }
            Target::Contents => self.apply_set_contents(),
            Target::EditCursor => self.apply_edit_cursor_set(),
        }
    }

    fn apply_edit_cursor_set(&mut self) {
        let mut sample = if self.item_nudge_dialog.relative_set {
            let delta = match self.dialog_delta_samples(true) {
                Ok(delta) => delta,
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error);
                    return;
                }
            };
            let current = match self.timeline.edit_cursor_sample(&self.project) {
                Ok(sample) => sample,
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error.to_string());
                    return;
                }
            };
            let Some(sample) = shifted_sample(current, delta) else {
                self.item_nudge_dialog.error =
                    Some("The edit cursor cannot move beyond the sample timeline".to_owned());
                return;
            };
            sample
        } else {
            match self.dialog_target_sample() {
                Ok(sample) => sample,
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error);
                    return;
                }
            }
        };
        if self.item_nudge_dialog.snap_to_unit {
            match self.snap_set_target_sample(sample) {
                Ok(snapped) => sample = snapped,
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error);
                    return;
                }
            }
        }
        match self.project.tick_at_sample(sample) {
            Ok(_) => {
                self.set_edit_cursor_position(crate::timeline::EditCursorPosition::Sample(sample));
                self.item_nudge_dialog.error = None;
            }
            Err(error) => self.item_nudge_dialog.error = Some(error.to_string()),
        }
    }

    fn apply_set_position(&mut self) {
        if commands::project_edit_busy(self) {
            self.item_nudge_dialog.error = Some(
                "Stop playback or finish the current operation before moving items".to_owned(),
            );
            return;
        }
        let selected = match self.selected_item_start_samples() {
            Ok(selected) => selected,
            Err(error) => {
                self.item_nudge_dialog.error = Some(error);
                return;
            }
        };
        let Some((_, first_sample)) = selected.first() else {
            self.item_nudge_dialog.error = Some("[no selected items]".to_owned());
            return;
        };
        let first_sample = *first_sample;
        let mut delta = if self.item_nudge_dialog.relative_set {
            match self.dialog_delta_samples(true) {
                Ok(delta) => delta,
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error);
                    return;
                }
            }
        } else {
            match self.dialog_target_sample() {
                Ok(sample) => i128::from(sample) - i128::from(first_sample),
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error);
                    return;
                }
            }
        };
        if self.item_nudge_dialog.snap_to_unit {
            let target = match i128::from(first_sample).checked_add(delta) {
                Some(target) => target,
                None => {
                    self.item_nudge_dialog.error =
                        Some("Item position is outside the supported range".to_owned());
                    return;
                }
            };
            let target = match u64::try_from(target) {
                Ok(target) => target,
                Err(_) => {
                    self.item_nudge_dialog.error = Some(
                        "Item position cannot be before sample zero or beyond the timeline"
                            .to_owned(),
                    );
                    return;
                }
            };
            match self.snap_set_target_sample(target) {
                Ok(target) => delta = i128::from(target) - i128::from(first_sample),
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error);
                    return;
                }
            }
        }
        self.shift_selected_items(delta);
    }

    fn apply_set_duplicate(&mut self) {
        if commands::project_edit_busy(self) {
            self.item_nudge_dialog.error = Some(
                "Stop playback or finish the current operation before duplicating items".to_owned(),
            );
            return;
        }
        let selected = match self.selected_item_start_samples() {
            Ok(selected) => selected,
            Err(error) => {
                self.item_nudge_dialog.error = Some(error);
                return;
            }
        };
        let Some((_, first_sample)) = selected.first() else {
            self.item_nudge_dialog.error = Some("No selected items to duplicate".to_owned());
            return;
        };
        let mut target_sample = match self.dialog_target_sample() {
            Ok(sample) => sample,
            Err(error) => {
                self.item_nudge_dialog.error = Some(error);
                return;
            }
        };
        if self.item_nudge_dialog.snap_to_unit {
            match self.snap_set_target_sample(target_sample) {
                Ok(target) => target_sample = target,
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error);
                    return;
                }
            }
        }

        // REAPER exposes Relative Set for Duplicate but still treats the value
        // as the absolute destination for the first selected item copy.
        let delta = i128::from(target_sample) - i128::from(*first_sample);
        self.duplicate_selected_items_at_delta(delta);
    }

    fn apply_set_edge_target(&mut self) {
        if commands::project_edit_busy(self) {
            self.item_nudge_dialog.error = Some(
                "Stop playback or finish the current operation before changing item edges"
                    .to_owned(),
            );
            return;
        }
        let target = self.item_nudge_dialog.target;
        let selected = match self.selected_item_start_samples() {
            Ok(selected) => selected,
            Err(error) => {
                self.item_nudge_dialog.error = Some(error);
                return;
            }
        };
        if selected.is_empty() {
            self.item_nudge_dialog.error = Some("No selected items to set".to_owned());
            return;
        }
        let relative_end_position =
            target == Target::EndPosition && self.item_nudge_dialog.relative_set;
        let relative_anchor_sample = if relative_end_position {
            let Some((last_id, _)) = selected.last().copied() else {
                self.item_nudge_dialog.error = Some("No selected items to set".to_owned());
                return;
            };
            match self.selected_item_end_sample(last_id) {
                Ok(sample) => sample,
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error);
                    return;
                }
            }
        } else {
            0
        };
        let mut target_sample = match self.dialog_target_sample() {
            Ok(sample) => sample,
            Err(error) => {
                self.item_nudge_dialog.error = Some(error);
                return;
            }
        };
        if self.item_nudge_dialog.snap_to_unit {
            match self.snap_set_target_sample(target_sample) {
                Ok(target) => target_sample = target,
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error);
                    return;
                }
            }
        }
        let relative_delta = relative_end_position
            .then(|| i128::from(target_sample) - i128::from(relative_anchor_sample));
        let selected_ids = &self.timeline.selected_items;
        let audio_items = self
            .project
            .audio_items()
            .iter()
            .filter(|item| selected_ids.contains(&item.id()))
            .collect::<Vec<_>>();
        let midi_items = self
            .project
            .midi_items()
            .iter()
            .filter(|item| selected_ids.contains(&item.id()))
            .collect::<Vec<_>>();
        let source_rates = if target == Target::LeftTrim && !audio_items.is_empty() {
            let media_refs = audio_items
                .iter()
                .map(|item| item.media_ref().to_owned())
                .collect::<Vec<_>>();
            match self.audio_source_sample_rates(media_refs) {
                Ok(rates) => rates,
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error);
                    return;
                }
            }
        } else {
            Default::default()
        };
        let actions = self
            .audio_set_edge_actions(
                &audio_items,
                target,
                target_sample,
                relative_delta,
                &source_rates,
            )
            .and_then(|mut actions| {
                actions.extend(self.midi_edge_actions(
                    &midi_items,
                    target,
                    target_sample,
                    relative_delta,
                    MidiEdgeEditMode::Set,
                )?);
                Ok(actions)
            });
        match actions {
            Ok(actions) => {
                if !actions.is_empty() {
                    self.apply_action(
                        DawAction::BatchTransaction {
                            tx_id: self.revision,
                            actions,
                        },
                        "Nudge items",
                    );
                }
                self.item_nudge_dialog.error = None;
            }
            Err(error) => self.item_nudge_dialog.error = Some(error),
        }
    }

    fn audio_set_edge_actions(
        &self,
        items: &[&AudioItem],
        target: Target,
        target_sample: u64,
        relative_delta: Option<i128>,
        source_rates: &HashMap<String, u32>,
    ) -> Result<Vec<DawAction>, String> {
        let project_rate = self.project.settings().sample_rate();
        let mut actions = Vec::with_capacity(items.len());
        for item in items {
            let edge_delta = match target {
                Target::LeftTrim | Target::LeftEdge => {
                    i128::from(target_sample) - i128::from(item.start_sample())
                }
                Target::EndPosition => match relative_delta {
                    Some(delta) => delta,
                    None => {
                        let end_sample = item
                            .start_sample()
                            .checked_add(item.length_samples())
                            .ok_or_else(|| {
                                "Selected item edge is outside the supported timeline".to_owned()
                            })?;
                        i128::from(target_sample) - i128::from(end_sample)
                    }
                },
                Target::RightTrim => {
                    let end_sample = item
                        .start_sample()
                        .checked_add(item.length_samples())
                        .ok_or_else(|| {
                            "Selected item edge is outside the supported timeline".to_owned()
                        })?;
                    i128::from(target_sample) - i128::from(end_sample)
                }
                _ => return Err("Selected target is not an item edge".to_owned()),
            };
            let (start_sample, source_offset_samples, length_samples) = match target {
                Target::LeftTrim | Target::LeftEdge => {
                    let length_samples = shifted_unsigned(item.length_samples(), -edge_delta)
                        .ok_or_else(|| {
                            "Left edge would remove the entire item or exceed its supported length"
                                .to_owned()
                        })?;
                    let source_offset_samples = if target == Target::LeftTrim {
                        let source_rate =
                            source_rates.get(item.media_ref()).copied().ok_or_else(|| {
                                format!(
                                    "Source sample rate is unavailable for {}",
                                    item.media_ref()
                                )
                            })?;
                        // REAPER's absolute Set → Left Trim advances the source offset
                        // by the distance to the requested edge, even when that edge
                        // lies before an item's current start (verified with a
                        // staggered, loop-source selection).
                        let source_delta = scale_sample_delta(
                            edge_delta.unsigned_abs() as i128,
                            source_rate,
                            project_rate,
                        )
                        .ok_or_else(|| "Source trim is outside the supported range".to_owned())?;
                        shifted_source_offset(item.source_offset_samples(), source_delta)
                            .ok_or_else(|| {
                                "Left trim is outside the supported source range".to_owned()
                            })?
                    } else {
                        item.source_offset_samples()
                    };
                    (target_sample, source_offset_samples, length_samples)
                }
                Target::EndPosition => {
                    let start_sample = if relative_delta.is_some() {
                        shifted_sample(item.start_sample(), edge_delta).ok_or_else(|| {
                            "Selected items cannot move beyond the sample timeline".to_owned()
                        })?
                    } else {
                        target_sample
                            .checked_sub(item.length_samples())
                            .ok_or_else(|| {
                                "End position would move an item before sample zero".to_owned()
                            })?
                    };
                    (
                        start_sample,
                        item.source_offset_samples(),
                        item.length_samples(),
                    )
                }
                Target::RightTrim => (
                    item.start_sample(),
                    item.source_offset_samples(),
                    target_sample
                        .checked_sub(item.start_sample())
                        .filter(|length| *length > 0)
                        .ok_or_else(|| {
                            "Right trim would remove the entire item or exceed its supported length"
                                .to_owned()
                        })?,
                ),
                _ => unreachable!("edge target validated above"),
            };
            if start_sample != item.start_sample()
                || source_offset_samples != item.source_offset_samples()
                || length_samples != item.length_samples()
            {
                actions.push(DawAction::EditAudioItem {
                    item_id: item.id(),
                    media_ref: item.media_ref().to_owned(),
                    start_sample,
                    source_offset_samples,
                    length_samples,
                });
            }
        }
        Ok(actions)
    }

    fn midi_edge_actions(
        &self,
        items: &[&MidiItem],
        target: Target,
        target_sample: u64,
        relative_delta: Option<i128>,
        mode: MidiEdgeEditMode,
    ) -> Result<Vec<DawAction>, String> {
        const MAX_EXACT: i128 = 1 << 53;
        let mut actions = Vec::with_capacity(items.len());
        for item in items {
            let end_tick = item
                .start_tick()
                .checked_add(item.length_ticks())
                .ok_or_else(|| "MIDI item edge is outside the supported range".to_owned())?;
            let start_sample = self
                .project
                .sample_at_tick(item.start_tick())
                .map_err(|error| error.to_string())?;
            let end_sample = self
                .project
                .sample_at_tick(end_tick)
                .map_err(|error| error.to_string())?;
            let edge_delta = match target {
                Target::LeftTrim | Target::LeftEdge => {
                    i128::from(target_sample) - i128::from(start_sample)
                }
                Target::EndPosition => match relative_delta {
                    Some(delta) => delta,
                    None => i128::from(target_sample) - i128::from(end_sample),
                },
                Target::RightTrim => i128::from(target_sample) - i128::from(end_sample),
                _ => return Err("Selected target is not a MIDI item edge".to_owned()),
            };
            if edge_delta.unsigned_abs() > MAX_EXACT as u128 {
                return Err("MIDI item edge is outside the supported range".to_owned());
            }
            let mut midi_contents_edit: Option<(i64, u64, i64)> = None;
            let (next_start_tick, next_length_ticks, next_source_offset_ticks) = match target {
                Target::LeftTrim | Target::LeftEdge => {
                    let next_start_tick = self
                        .project
                        .tick_at_sample(target_sample)
                        .map_err(|error| error.to_string())?;
                    let next_length_ticks = end_tick
                        .checked_sub(next_start_tick)
                        .ok_or_else(|| "Left edge would remove the entire MIDI item".to_owned())?;
                    if next_length_ticks == 0 {
                        return Err("Left edge would remove the entire MIDI item".to_owned());
                    }
                    let source_offset_ticks = if target == Target::LeftTrim {
                        if mode == MidiEdgeEditMode::Nudge && !item.is_loop_source() {
                            let target_tick_position = self
                                .project
                                .tick_at_sample_position(target_sample as f64)
                                .map_err(|error| error.to_string())?;
                            let edge_delta_ticks =
                                (target_tick_position - item.start_tick() as f64).trunc();
                            if !edge_delta_ticks.is_finite()
                                || edge_delta_ticks.abs() > MAX_EXACT as f64
                            {
                                return Err("MIDI left trim is outside the supported source range"
                                    .to_owned());
                            }
                            let edge_delta_ticks = edge_delta_ticks as i128;
                            let old_source_offset = i128::from(item.signed_source_offset_ticks());
                            let desired_source_offset = old_source_offset
                                .checked_add(edge_delta_ticks)
                                .ok_or_else(|| {
                                    "MIDI left trim is outside the supported source range"
                                        .to_owned()
                                })?;
                            let (next_source_offset, content_shift_ticks) = if desired_source_offset
                                < 0
                            {
                                if old_source_offset == 0 {
                                    (0, -desired_source_offset)
                                } else {
                                    // REAPER retains one MIDI tick when Nudge crosses
                                    // a non-loop source's beginning from a positive offset.
                                    (1, 1 - desired_source_offset)
                                }
                            } else if old_source_offset == 0 && edge_delta_ticks > 0 {
                                // At source offset zero, REAPER removes only the prefix
                                // before the earliest event. Any remaining trim advances
                                // the take offset so events at source zero stay nonnegative.
                                let earliest_event_tick = item
                                    .notes()
                                    .iter()
                                    .map(|note| note.tick())
                                    .chain(
                                        item.controllers().iter().map(|controller| controller.tick),
                                    )
                                    .chain(item.pitch_bends().iter().map(|bend| bend.tick))
                                    .min()
                                    .unwrap_or(0);
                                let physical_trim =
                                    edge_delta_ticks.min(i128::from(earliest_event_tick));
                                (desired_source_offset - physical_trim, -physical_trim)
                            } else {
                                (desired_source_offset, 0)
                            };
                            let next_source_length_ticks =
                                i128::from(item.source_loop_length_ticks())
                                    .checked_add(content_shift_ticks)
                                    .filter(|length| (1..=MAX_EXACT).contains(length))
                                    .and_then(|length| u64::try_from(length).ok())
                                    .ok_or_else(|| {
                                        "MIDI left trim is outside the supported source range"
                                            .to_owned()
                                    })?;
                            let next_source_offset = u64::try_from(next_source_offset)
                                .ok()
                                .filter(|offset| *offset <= MAX_EXACT as u64)
                                .ok_or_else(|| {
                                    "MIDI left trim is outside the supported source range"
                                        .to_owned()
                                })?;
                            let content_shift_ticks =
                                i64::try_from(content_shift_ticks).map_err(|_| {
                                    "MIDI left trim is outside the supported source range"
                                        .to_owned()
                                })?;
                            if content_shift_ticks != 0 {
                                midi_contents_edit = Some((
                                    i64::try_from(next_source_offset).map_err(|_| {
                                        "MIDI left trim is outside the supported source range"
                                            .to_owned()
                                    })?,
                                    next_source_length_ticks,
                                    content_shift_ticks,
                                ));
                            }
                            i64::try_from(next_source_offset).map_err(|_| {
                                "MIDI left trim is outside the supported source range".to_owned()
                            })?
                        } else {
                            let old_offset_samples = self
                                .project
                                .midi_signed_source_offset_samples(
                                    item.start_tick(),
                                    item.signed_source_offset_ticks(),
                                )
                                .map_err(|error| error.to_string())?;
                            let mut next_offset_samples = old_offset_samples + edge_delta as f64;
                            if !next_offset_samples.is_finite() {
                                return Err("MIDI left trim is outside the supported source range"
                                    .to_owned());
                            }
                            if item.is_loop_source() {
                                if next_offset_samples < 0.0 {
                                    // REAPER wraps a MIDI loop's source phase when Set →
                                    // Left Trim extends the left edge before its old start.
                                    let loop_length_samples = self
                                        .project
                                        .midi_source_offset_samples(
                                            next_start_tick,
                                            item.source_loop_length_ticks(),
                                        )
                                        .map_err(|error| error.to_string())?;
                                    if !loop_length_samples.is_finite()
                                        || loop_length_samples <= 0.0
                                    {
                                        return Err(
                                            "MIDI source loop is outside the supported range"
                                                .to_owned(),
                                        );
                                    }
                                    next_offset_samples =
                                        next_offset_samples.rem_euclid(loop_length_samples);
                                }
                                let source_phase = self
                                    .project
                                    .midi_source_phase_at_offset_samples(
                                        next_start_tick,
                                        item.source_loop_length_ticks(),
                                        next_offset_samples,
                                    )
                                    .map_err(|error| error.to_string())?;
                                i64::try_from(source_phase).map_err(|_| {
                                    "MIDI left trim is outside the supported source range"
                                        .to_owned()
                                })?
                            } else {
                                let edge_delta_ticks =
                                    i128::from(next_start_tick) - i128::from(item.start_tick());
                                let desired_source_offset =
                                    i128::from(item.signed_source_offset_ticks())
                                        .checked_add(edge_delta_ticks)
                                        .ok_or_else(|| {
                                            "MIDI left trim is outside the supported source range"
                                                .to_owned()
                                        })?;
                                let earliest_event_tick = item
                                    .notes()
                                    .iter()
                                    .map(|note| note.tick())
                                    .chain(
                                        item.controllers().iter().map(|controller| controller.tick),
                                    )
                                    .chain(item.pitch_bends().iter().map(|bend| bend.tick))
                                    .min()
                                    .unwrap_or(0);
                                // Keep the take phase aligned with the item's edge while keeping
                                // the non-loop source PPQ nonnegative. REAPER rebases content at
                                // source zero when the requested phase crosses either boundary.
                                let content_shift_ticks = if desired_source_offset < 0 {
                                    -desired_source_offset
                                } else if desired_source_offset > i128::from(earliest_event_tick) {
                                    -i128::from(earliest_event_tick)
                                } else {
                                    0
                                };
                                let next_source_offset_ticks = desired_source_offset
                                    .checked_add(content_shift_ticks)
                                    .filter(|offset| (0..=MAX_EXACT).contains(offset))
                                    .and_then(|offset| u64::try_from(offset).ok())
                                    .ok_or_else(|| {
                                        "MIDI left trim is outside the supported source range"
                                            .to_owned()
                                    })?;
                                let next_source_length_ticks =
                                    i128::from(item.source_loop_length_ticks())
                                        .checked_add(content_shift_ticks)
                                        .filter(|length| (1..=MAX_EXACT).contains(length))
                                        .and_then(|length| u64::try_from(length).ok())
                                        .ok_or_else(|| {
                                            "MIDI left trim is outside the supported source range"
                                                .to_owned()
                                        })?;
                                let content_shift_ticks = i64::try_from(content_shift_ticks)
                                    .map_err(|_| {
                                        "MIDI left trim is outside the supported source range"
                                            .to_owned()
                                    })?;
                                if content_shift_ticks != 0 {
                                    midi_contents_edit = Some((
                                        i64::try_from(next_source_offset_ticks).map_err(|_| {
                                            "MIDI left trim is outside the supported source range"
                                                .to_owned()
                                        })?,
                                        next_source_length_ticks,
                                        content_shift_ticks,
                                    ));
                                }
                                i64::try_from(next_source_offset_ticks).map_err(|_| {
                                    "MIDI left trim is outside the supported source range"
                                        .to_owned()
                                })?
                            }
                        }
                    } else {
                        item.signed_source_offset_ticks()
                    };
                    (next_start_tick, next_length_ticks, source_offset_ticks)
                }
                Target::EndPosition => {
                    let next_start_sample =
                        shifted_sample(start_sample, edge_delta).ok_or_else(|| {
                            "Selected items cannot move beyond the sample timeline".to_owned()
                        })?;
                    let next_start_tick = self
                        .project
                        .tick_at_sample(next_start_sample)
                        .map_err(|error| error.to_string())?;
                    (
                        next_start_tick,
                        item.length_ticks(),
                        item.signed_source_offset_ticks(),
                    )
                }
                Target::RightTrim => {
                    let next_end_tick = self
                        .project
                        .tick_at_sample(target_sample)
                        .map_err(|error| error.to_string())?;
                    let next_length_ticks = next_end_tick
                        .checked_sub(item.start_tick())
                        .filter(|length| *length > 0)
                        .ok_or_else(|| "Right trim would remove the entire MIDI item".to_owned())?;
                    (
                        item.start_tick(),
                        next_length_ticks,
                        item.signed_source_offset_ticks(),
                    )
                }
                _ => return Err("Selected target is not a MIDI item edge".to_owned()),
            };
            if target == Target::RightTrim && !item.is_loop_source() {
                let next_source_length_ticks = u64::try_from(
                    i128::from(next_source_offset_ticks) + i128::from(next_length_ticks),
                )
                .ok()
                .filter(|length| *length > 0 && *length <= MAX_EXACT as u64)
                .ok_or_else(|| {
                    "MIDI right trim is outside the supported source range".to_owned()
                })?;
                if next_source_length_ticks != item.source_loop_length_ticks() {
                    // REAPER resizes a non-loop MIDI source to the visible right edge
                    // when Set → Right Trim is applied. A trimmed source offset stays
                    // intact, so the new source length is offset + visible item length.
                    midi_contents_edit =
                        Some((next_source_offset_ticks, next_source_length_ticks, 0));
                }
            }
            if next_start_tick != item.start_tick()
                || next_length_ticks != item.length_ticks()
                || next_source_offset_ticks != item.signed_source_offset_ticks()
            {
                actions.push(DawAction::EditMidiItemPlacement {
                    item_id: item.id(),
                    start_tick: next_start_tick,
                    length_ticks: next_length_ticks,
                    source_offset_ticks: next_source_offset_ticks,
                });
            }
            if let Some((source_offset_ticks, source_loop_length_ticks, content_shift_ticks)) =
                midi_contents_edit
            {
                actions.push(DawAction::SetMidiItemContents {
                    item_id: item.id(),
                    source_offset_ticks,
                    source_loop_length_ticks,
                    content_shift_ticks,
                });
            }
        }
        Ok(actions)
    }

    fn selected_item_end_sample(&self, item_id: ItemId) -> Result<u64, String> {
        if let Some(item) = self
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == item_id)
        {
            return item
                .start_sample()
                .checked_add(item.length_samples())
                .ok_or_else(|| "Selected item edge is outside the supported timeline".to_owned());
        }
        let item = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
            .ok_or_else(|| "Selected item is unavailable".to_owned())?;
        let end_position = item
            .start_position()
            .checked_add_ticks(item.length_ticks())
            .ok_or_else(|| {
                "Selected MIDI item edge is outside the supported timeline".to_owned()
            })?;
        let sample = self
            .project
            .sample_at_tick_position(end_position.as_ticks_f64())
            .map_err(|error| error.to_string())?;
        checked_sample_position(sample, "Selected MIDI item edge")
    }

    fn apply_set_contents(&mut self) {
        if commands::project_edit_busy(self) {
            self.item_nudge_dialog.error = Some(
                "Stop playback or finish the current operation before moving item contents"
                    .to_owned(),
            );
            return;
        }
        if self.item_nudge_dialog.relative_set {
            self.item_nudge_dialog.error =
                Some("Relative Set is unavailable for Contents".to_owned());
            return;
        }
        let target_sample = match self.dialog_target_sample() {
            Ok(sample) => sample,
            Err(error) => {
                self.item_nudge_dialog.error = Some(error);
                return;
            }
        };
        let target_sample = if self.item_nudge_dialog.snap_to_unit {
            match self.snap_set_target_sample(target_sample) {
                Ok(target) => target,
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error);
                    return;
                }
            }
        } else {
            target_sample
        };
        let selected_ids = &self.timeline.selected_items;
        let midi_items = self
            .project
            .midi_items()
            .iter()
            .filter(|item| selected_ids.contains(&item.id()))
            .collect::<Vec<_>>();
        let audio_items = self
            .project
            .audio_items()
            .iter()
            .filter(|item| selected_ids.contains(&item.id()))
            .collect::<Vec<_>>();
        if audio_items.is_empty() && midi_items.is_empty() {
            self.item_nudge_dialog.error = Some("No selected items to set".to_owned());
            return;
        }
        let actions = match self
            .audio_contents_set_actions(&audio_items, target_sample)
            .and_then(|mut actions| {
                actions.extend(self.midi_contents_set_actions(&midi_items, target_sample)?);
                Ok(actions)
            }) {
            Ok(actions) => actions,
            Err(error) => {
                self.item_nudge_dialog.error = Some(error);
                return;
            }
        };
        if !actions.is_empty() {
            self.apply_action(
                DawAction::BatchTransaction {
                    tx_id: self.revision,
                    actions,
                },
                "Selected item contents set",
            );
        }
        self.item_nudge_dialog.error = None;
    }

    fn audio_contents_set_actions(
        &self,
        items: &[&AudioItem],
        target_sample: u64,
    ) -> Result<Vec<DawAction>, String> {
        if items.is_empty() {
            return Ok(Vec::new());
        }
        let media_refs = items
            .iter()
            .map(|item| item.media_ref().to_owned())
            .collect::<Vec<_>>();
        let source_limits = self.audio_source_limits(media_refs)?;
        let project_rate = self.project.settings().sample_rate();
        let mut actions = Vec::with_capacity(items.len());
        for item in items {
            let Some((source_rate, frame_count)) = source_limits.get(item.media_ref()).copied()
            else {
                return Err(format!(
                    "Source length is unavailable for {}",
                    item.media_ref()
                ));
            };
            let project_delta = i128::from(item.start_sample()) - i128::from(target_sample);
            let Some(source_delta) = scale_sample_delta(project_delta, source_rate, project_rate)
            else {
                return Err("Contents target is outside the supported source range".to_owned());
            };
            let source_offset_samples = if item.is_loop_source() {
                let (section_start, section_length) = match (
                    item.source_section_start_samples(),
                    item.source_section_length_samples(),
                ) {
                    (Some(start), Some(length)) => (start, length),
                    (None, None) => (0, frame_count),
                    _ => {
                        return Err(
                            "Selected audio item has an invalid loop source section".to_owned()
                        );
                    }
                };
                let Some(section_end) = section_start.checked_add(section_length) else {
                    return Err("Selected audio item has an invalid loop source section".to_owned());
                };
                if section_length == 0 || section_end > frame_count {
                    return Err("Selected audio item has an invalid loop source section".to_owned());
                }
                let Some(offset) = wrapped_source_sample(0, 0, section_length, source_delta) else {
                    return Err("Contents target is outside the supported source range".to_owned());
                };
                i64::try_from(offset).map_err(|_| {
                    "Contents target is outside the supported source range".to_owned()
                })?
            } else {
                i64::try_from(source_delta)
                    .map_err(|_| "Contents target is outside the signed source range".to_owned())?
            };
            if source_offset_samples != item.source_offset_samples() {
                actions.push(DawAction::EditAudioItem {
                    item_id: item.id(),
                    media_ref: item.media_ref().to_owned(),
                    start_sample: item.start_sample(),
                    source_offset_samples,
                    length_samples: item.length_samples(),
                });
            }
        }
        Ok(actions)
    }

    fn midi_contents_set_actions(
        &self,
        items: &[&MidiItem],
        target_sample: u64,
    ) -> Result<Vec<DawAction>, String> {
        const MAX_EXACT: u64 = 1 << 53;
        if target_sample > MAX_EXACT {
            return Err("Contents target is outside the supported range".to_owned());
        }
        let mut actions = Vec::with_capacity(items.len());
        for item in items {
            let source_loop_length = item.source_loop_length_ticks();
            if source_loop_length == 0 {
                return Err("Selected MIDI item has an invalid source length".to_owned());
            }
            if !item.is_loop_source() {
                let target_tick = self
                    .project
                    .tick_at_sample(target_sample)
                    .map_err(|error| error.to_string())?;
                let (source_offset_ticks, content_shift_ticks, new_source_length) = if target_tick
                    <= item.start_tick()
                {
                    (item.start_tick() - target_tick, 0, source_loop_length)
                } else {
                    let content_shift_ticks = target_tick - item.start_tick();
                    let source_length = source_loop_length
                        .checked_add(content_shift_ticks)
                        .ok_or_else(|| {
                            "Contents target is outside the supported MIDI source range".to_owned()
                        })?;
                    (0, content_shift_ticks, source_length)
                };
                if i64::try_from(source_offset_ticks).ok()
                    != Some(item.signed_source_offset_ticks())
                    || content_shift_ticks != 0
                    || new_source_length != source_loop_length
                {
                    let content_shift_ticks = i64::try_from(content_shift_ticks).map_err(|_| {
                        "Contents target is outside the supported MIDI source range".to_owned()
                    })?;
                    actions.push(DawAction::SetMidiItemContents {
                        item_id: item.id(),
                        source_offset_ticks: i64::try_from(source_offset_ticks).map_err(|_| {
                            "Contents target is outside the supported MIDI source range".to_owned()
                        })?,
                        source_loop_length_ticks: new_source_length,
                        content_shift_ticks,
                    });
                }
                continue;
            }
            let item_start_sample = self
                .project
                .sample_at_tick(item.start_tick())
                .map_err(|error| error.to_string())?;
            if item_start_sample > MAX_EXACT {
                return Err("Contents target is outside the supported range".to_owned());
            }
            let source_loop_samples = self
                .project
                .midi_source_offset_samples(item.start_tick(), source_loop_length)
                .map_err(|error| error.to_string())?;
            if !source_loop_samples.is_finite()
                || source_loop_samples <= 0.0
                || source_loop_samples > MAX_EXACT as f64
            {
                return Err("MIDI source loop is outside the supported range".to_owned());
            }
            let source_offset_samples =
                (item_start_sample as f64 - target_sample as f64).rem_euclid(source_loop_samples);
            let source_offset_ticks = self
                .project
                .midi_source_phase_at_offset_samples(
                    item.start_tick(),
                    source_loop_length,
                    source_offset_samples,
                )
                .map_err(|error| error.to_string())?;
            let current_source_phase = i128::from(item.signed_source_offset_ticks())
                .rem_euclid(i128::from(source_loop_length))
                as u64;
            if source_offset_ticks != current_source_phase {
                actions.push(DawAction::EditMidiItemPlacement {
                    item_id: item.id(),
                    start_tick: item.start_tick(),
                    length_ticks: item.length_ticks(),
                    source_offset_ticks: i64::try_from(source_offset_ticks).map_err(|_| {
                        "Contents nudge is outside the supported source range".to_owned()
                    })?,
                });
            }
        }
        Ok(actions)
    }

    fn shift_selected_items(&mut self, delta_samples: i128) {
        let selected_ids = &self.timeline.selected_items;
        let mut actions = Vec::new();
        for item in self
            .project
            .audio_items()
            .iter()
            .filter(|item| selected_ids.contains(&item.id()))
        {
            let Some(start_sample) = shifted_sample(item.start_sample(), delta_samples) else {
                self.item_nudge_dialog.error =
                    Some("Selected items cannot move beyond the sample timeline".to_owned());
                return;
            };
            if start_sample != item.start_sample() {
                actions.push(DawAction::EditAudioItem {
                    item_id: item.id(),
                    media_ref: item.media_ref().to_owned(),
                    start_sample,
                    source_offset_samples: item.source_offset_samples(),
                    length_samples: item.length_samples(),
                });
            }
        }
        for item in self
            .project
            .midi_items()
            .iter()
            .filter(|item| selected_ids.contains(&item.id()))
        {
            const MAX_EXACT: f64 = (1_u64 << 53) as f64;
            let start_sample = match self
                .project
                .sample_at_tick_position(item.start_position().as_ticks_f64())
            {
                Ok(sample) => sample,
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error.to_string());
                    return;
                }
            };
            if delta_samples.unsigned_abs() > (1_u64 << 53) as u128 {
                self.item_nudge_dialog.error =
                    Some("Selected items cannot move beyond the sample timeline".to_owned());
                return;
            }
            let new_sample = start_sample + delta_samples as f64;
            if !new_sample.is_finite() || !(0.0..=MAX_EXACT).contains(&new_sample) {
                self.item_nudge_dialog.error =
                    Some("Selected items cannot move beyond the sample timeline".to_owned());
                return;
            }
            let new_position = match midi_item_position_at_sample(&self.project, new_sample) {
                Ok(position) => position,
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error);
                    return;
                }
            };
            if new_position != item.start_position() {
                if new_position
                    .checked_add_ticks(item.length_ticks())
                    .is_none()
                {
                    self.item_nudge_dialog.error =
                        Some("Selected items cannot move beyond the MIDI tick timeline".to_owned());
                    return;
                }
                actions.push(DawAction::SetMidiItemPosition {
                    item_id: item.id(),
                    position: new_position,
                });
            }
        }
        if actions.is_empty() {
            let has_selected_items = self
                .project
                .audio_items()
                .iter()
                .any(|item| selected_ids.contains(&item.id()))
                || self
                    .project
                    .midi_items()
                    .iter()
                    .any(|item| selected_ids.contains(&item.id()));
            self.item_nudge_dialog.error =
                (!has_selected_items).then(|| "No selected items to move".to_owned());
            return;
        }
        self.apply_action(
            DawAction::BatchTransaction {
                tx_id: self.revision,
                actions,
            },
            "Selected items moved",
        );
        self.item_nudge_dialog.error = None;
    }

    fn nudge_selected_edge_targets(&mut self, delta_samples: i128) {
        let selected_ids = &self.timeline.selected_items;
        let items = self
            .project
            .audio_items()
            .iter()
            .filter(|item| selected_ids.contains(&item.id()))
            .collect::<Vec<_>>();
        let midi_items = self
            .project
            .midi_items()
            .iter()
            .filter(|item| selected_ids.contains(&item.id()))
            .collect::<Vec<_>>();
        if items.is_empty() && midi_items.is_empty() {
            self.item_nudge_dialog.error = Some("No selected items to nudge".to_owned());
            return;
        }

        let source_rates = if self.item_nudge_dialog.target == Target::LeftTrim {
            let media_refs = items
                .iter()
                .map(|item| item.media_ref().to_owned())
                .collect::<Vec<_>>();
            match self.audio_source_sample_rates(media_refs) {
                Ok(rates) => rates,
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error);
                    return;
                }
            }
        } else {
            Default::default()
        };

        let project_rate = self.project.settings().sample_rate();
        let mut actions = Vec::with_capacity(items.len());
        for item in items {
            let (start_sample, source_offset_samples, length_samples) =
                match self.item_nudge_dialog.target {
                    Target::LeftTrim => {
                        let Some(start_sample) = shifted_sample(item.start_sample(), delta_samples)
                        else {
                            self.item_nudge_dialog.error = Some(
                                "Selected items cannot move beyond the sample timeline".to_owned(),
                            );
                            return;
                        };
                        let Some(length_samples) =
                            shifted_unsigned(item.length_samples(), -delta_samples)
                        else {
                            self.item_nudge_dialog.error = Some(
                            "Left trim would remove the entire item or exceed its supported length"
                                .to_owned(),
                        );
                            return;
                        };
                        let Some(source_rate) = source_rates.get(item.media_ref()).copied() else {
                            self.item_nudge_dialog.error = Some(format!(
                                "Source sample rate is unavailable for {}",
                                item.media_ref()
                            ));
                            return;
                        };
                        let Some(source_delta) =
                            scale_sample_delta(delta_samples, source_rate, project_rate)
                        else {
                            self.item_nudge_dialog.error =
                                Some("Source trim is outside the supported range".to_owned());
                            return;
                        };
                        let Some(source_offset_samples) =
                            shifted_source_offset(item.source_offset_samples(), source_delta)
                        else {
                            self.item_nudge_dialog.error =
                                Some("Left trim is outside the supported source range".to_owned());
                            return;
                        };
                        (start_sample, source_offset_samples, length_samples)
                    }
                    Target::LeftEdge => {
                        let Some(start_sample) = shifted_sample(item.start_sample(), delta_samples)
                        else {
                            self.item_nudge_dialog.error = Some(
                                "Selected items cannot move beyond the sample timeline".to_owned(),
                            );
                            return;
                        };
                        let Some(length_samples) =
                            shifted_unsigned(item.length_samples(), -delta_samples)
                        else {
                            self.item_nudge_dialog.error = Some(
                            "Left edge would remove the entire item or exceed its supported length"
                                .to_owned(),
                        );
                            return;
                        };
                        (start_sample, item.source_offset_samples(), length_samples)
                    }
                    Target::EndPosition => {
                        let Some(start_sample) = shifted_sample(item.start_sample(), delta_samples)
                        else {
                            self.item_nudge_dialog.error = Some(
                                "Selected items cannot move beyond the sample timeline".to_owned(),
                            );
                            return;
                        };
                        (
                            start_sample,
                            item.source_offset_samples(),
                            item.length_samples(),
                        )
                    }
                    Target::RightTrim => {
                        let Some(length_samples) =
                            shifted_unsigned(item.length_samples(), delta_samples)
                        else {
                            self.item_nudge_dialog.error = Some(
                            "Right trim would remove the entire item or exceed its supported length"
                                .to_owned(),
                        );
                            return;
                        };
                        (
                            item.start_sample(),
                            item.source_offset_samples(),
                            length_samples,
                        )
                    }
                    _ => unreachable!("only implemented audio targets reach this method"),
                };
            if start_sample != item.start_sample()
                || source_offset_samples != item.source_offset_samples()
                || length_samples != item.length_samples()
            {
                actions.push(DawAction::EditAudioItem {
                    item_id: item.id(),
                    media_ref: item.media_ref().to_owned(),
                    start_sample,
                    source_offset_samples,
                    length_samples,
                });
            }
        }
        let mut action_batch = actions;
        for item in &midi_items {
            let (target_sample, relative_delta) = match self.item_nudge_dialog.target {
                Target::LeftTrim | Target::LeftEdge => {
                    let start_sample = match self.project.sample_at_tick(item.start_tick()) {
                        Ok(sample) => sample,
                        Err(error) => {
                            self.item_nudge_dialog.error = Some(error.to_string());
                            return;
                        }
                    };
                    let Some(target_sample) = shifted_sample(start_sample, delta_samples) else {
                        self.item_nudge_dialog.error = Some(
                            "Selected items cannot move beyond the sample timeline".to_owned(),
                        );
                        return;
                    };
                    (target_sample, None)
                }
                Target::EndPosition => (0, Some(delta_samples)),
                Target::RightTrim => {
                    let end_tick = match item.start_tick().checked_add(item.length_ticks()) {
                        Some(tick) => tick,
                        None => {
                            self.item_nudge_dialog.error =
                                Some("MIDI item edge is outside the supported range".to_owned());
                            return;
                        }
                    };
                    let end_sample = match self.project.sample_at_tick(end_tick) {
                        Ok(sample) => sample,
                        Err(error) => {
                            self.item_nudge_dialog.error = Some(error.to_string());
                            return;
                        }
                    };
                    let Some(target_sample) = shifted_sample(end_sample, delta_samples) else {
                        self.item_nudge_dialog.error =
                            Some("Selected item edge is outside the supported timeline".to_owned());
                        return;
                    };
                    (target_sample, None)
                }
                _ => unreachable!("only edge targets reach this method"),
            };
            let item_actions = match self.midi_edge_actions(
                &[item],
                self.item_nudge_dialog.target,
                target_sample,
                relative_delta,
                MidiEdgeEditMode::Nudge,
            ) {
                Ok(actions) => actions,
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error);
                    return;
                }
            };
            action_batch.extend(item_actions);
        }
        if action_batch.is_empty() {
            self.item_nudge_dialog.error =
                Some("Nudge did not change the selected items".to_owned());
            return;
        }
        self.apply_action(
            DawAction::BatchTransaction {
                tx_id: self.revision,
                actions: action_batch,
            },
            "Nudge items",
        );
        self.item_nudge_dialog.error = None;
    }

    fn nudge_duplicate_selected_items(&mut self, delta_samples: i128) {
        if self.item_nudge_dialog.snap_to_unit {
            self.item_nudge_dialog.error =
                Some("Snap to unit is not supported for the duplicate target yet".to_owned());
            return;
        }
        self.duplicate_selected_items_at_delta(delta_samples);
    }

    fn duplicate_selected_items_at_delta(&mut self, delta_samples: i128) {
        let selected_ids = &self.timeline.selected_items;
        let mut actions = Vec::new();
        for item in self
            .project
            .audio_items()
            .iter()
            .filter(|item| selected_ids.contains(&item.id()))
        {
            let Some(start_sample) = shifted_sample(item.start_sample(), delta_samples) else {
                self.item_nudge_dialog.error = Some(
                    "Duplicate position cannot be before sample zero or beyond the timeline"
                        .to_owned(),
                );
                return;
            };
            actions.push(DawAction::DuplicateAudioItemAt {
                item_id: item.id(),
                track_id: item.track_id(),
                start_sample,
            });
        }
        for item in self
            .project
            .midi_items()
            .iter()
            .filter(|item| selected_ids.contains(&item.id()))
        {
            let start_sample = match self.project.sample_at_tick(item.start_tick()) {
                Ok(sample) => sample,
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error.to_string());
                    return;
                }
            };
            let Some(start_sample) = shifted_sample(start_sample, delta_samples) else {
                self.item_nudge_dialog.error = Some(
                    "Duplicate position cannot be before sample zero or beyond the timeline"
                        .to_owned(),
                );
                return;
            };
            let start_tick = match self.project.tick_at_sample(start_sample) {
                Ok(tick) => tick,
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error.to_string());
                    return;
                }
            };
            actions.push(DawAction::DuplicateMidiItemAt {
                item_id: item.id(),
                start_tick,
            });
        }
        if actions.is_empty() {
            self.item_nudge_dialog.error = Some("No selected items to duplicate".to_owned());
            return;
        }

        let previous_ids = self
            .project
            .audio_items()
            .iter()
            .map(|item| item.id())
            .chain(self.project.midi_items().iter().map(|item| item.id()))
            .collect::<std::collections::HashSet<_>>();
        let revision = self.revision;
        self.apply_action(
            DawAction::BatchTransaction {
                tx_id: self.revision,
                actions,
            },
            "Selected items duplicated",
        );
        if self.revision == revision {
            return;
        }
        let selected = self
            .project
            .audio_items()
            .iter()
            .filter(|item| !previous_ids.contains(&item.id()))
            .map(|item| (item.id(), item.track_id()))
            .chain(self.project.midi_items().iter().filter_map(|item| {
                (!previous_ids.contains(&item.id())).then_some((item.id(), item.track_id()))
            }))
            .collect::<Vec<_>>();
        self.timeline.select_items(selected);
        self.item_nudge_dialog.error = None;
    }

    fn nudge_selected_contents(&mut self, delta_samples: i128) {
        let selected_ids = &self.timeline.selected_items;
        let audio_items = self
            .project
            .audio_items()
            .iter()
            .filter(|item| selected_ids.contains(&item.id()))
            .collect::<Vec<_>>();
        let midi_items = self
            .project
            .midi_items()
            .iter()
            .filter(|item| selected_ids.contains(&item.id()))
            .collect::<Vec<_>>();
        let selected_audio = !audio_items.is_empty();
        let selected_midi = !midi_items.is_empty();
        if !selected_audio && !selected_midi {
            self.item_nudge_dialog.error = Some("No selected items to nudge".to_owned());
            return;
        }

        let mut actions = Vec::with_capacity(audio_items.len() + midi_items.len());
        if selected_audio {
            match self.audio_contents_nudge_actions(&audio_items, delta_samples) {
                Ok(audio_actions) => actions.extend(audio_actions),
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error);
                    return;
                }
            }
        }
        if selected_midi {
            match self.midi_contents_nudge_actions(&midi_items, delta_samples) {
                Ok(midi_actions) => actions.extend(midi_actions),
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error);
                    return;
                }
            }
        }
        if actions.is_empty() {
            self.item_nudge_dialog.error =
                Some("Nudge did not change the selected item contents".to_owned());
            return;
        }

        let label = match (selected_audio, selected_midi) {
            (true, true) => "Selected audio and MIDI item contents moved",
            (true, false) => "Selected audio item contents moved",
            (false, true) => "Selected MIDI item contents moved",
            (false, false) => unreachable!("selected item kinds were checked above"),
        };
        self.apply_action(
            DawAction::BatchTransaction {
                tx_id: self.revision,
                actions,
            },
            label,
        );
        self.item_nudge_dialog.error = None;
    }

    fn midi_contents_nudge_actions(
        &self,
        items: &[&MidiItem],
        delta_samples: i128,
    ) -> Result<Vec<DawAction>, String> {
        if items.is_empty() {
            return Ok(Vec::new());
        }
        let Some(sample_delta) = delta_samples
            .checked_abs()
            .and_then(|samples| u64::try_from(samples).ok())
        else {
            return Err("Contents nudge is outside the supported range".to_owned());
        };
        if sample_delta > (1_u64 << 53) {
            return Err("Contents nudge is outside the supported range".to_owned());
        }
        let mut actions = Vec::with_capacity(items.len());
        for item in items {
            let source_loop_length = item.source_loop_length_ticks();
            if source_loop_length == 0 {
                return Err("Selected MIDI item has an invalid source length".to_owned());
            }
            if !item.is_loop_source() {
                if delta_samples == 0 {
                    continue;
                }
                if source_loop_length > (1_u64 << 53) {
                    return Err("Contents nudge is outside the supported source range".to_owned());
                }
                let current_offset_samples = self
                    .project
                    .midi_signed_source_offset_samples(
                        item.start_tick(),
                        item.signed_source_offset_ticks(),
                    )
                    .map_err(|error| error.to_string())?;
                let nudge_samples = sample_delta as f64;
                let desired_offset_samples = if delta_samples > 0 {
                    current_offset_samples - nudge_samples
                } else {
                    current_offset_samples + nudge_samples
                };
                let (source_offset_samples, content_shift_samples) =
                    if delta_samples > 0 && desired_offset_samples < 0.0 {
                        if item.signed_source_offset_ticks() >= 0 {
                            // A right nudge that crosses source PPQ zero is represented by rebasing
                            // the events forward and keeping the take offset at zero.
                            (0.0, -desired_offset_samples)
                        } else {
                            // Negative offsets can put the source start before MIDI PPQ zero. REAPER
                            // keeps one tick of positive offset when a right nudge crosses this
                            // boundary, shifting events by the remaining displacement.
                            let one_tick_offset = self
                                .project
                                .midi_signed_source_offset_samples(item.start_tick(), 1)
                                .map_err(|error| error.to_string())?;
                            (one_tick_offset, one_tick_offset - desired_offset_samples)
                        }
                    } else if delta_samples < 0 && item.signed_source_offset_ticks() == 0 {
                        // At the source origin, left Contents Nudge moves events toward PPQ zero
                        // instead of creating a positive offset. Away from zero, the offset itself
                        // carries that displacement, including across signed zero.
                        (0.0, -nudge_samples)
                    } else {
                        (desired_offset_samples, 0.0)
                    };
                let mut source_offset_ticks = self
                    .project
                    .midi_signed_source_offset_ticks_at_samples(
                        item.start_tick(),
                        source_offset_samples,
                    )
                    .map_err(|error| error.to_string())?;
                let mut content_shift_ticks = if content_shift_samples == 0.0 {
                    0
                } else {
                    self.midi_source_content_shift_ticks(item.start_tick(), content_shift_samples)?
                };
                if content_shift_ticks < 0 {
                    let earliest_event_tick = item
                        .notes()
                        .iter()
                        .map(|note| note.tick())
                        .chain(item.controllers().iter().map(|controller| controller.tick))
                        .chain(item.pitch_bends().iter().map(|bend| bend.tick))
                        .min()
                        .unwrap_or(0);
                    let requested_shift_ticks = content_shift_ticks.unsigned_abs();
                    let applied_shift_ticks = requested_shift_ticks.min(earliest_event_tick);
                    let remaining_shift_ticks = requested_shift_ticks - applied_shift_ticks;
                    content_shift_ticks = -i64::try_from(applied_shift_ticks).map_err(|_| {
                        "Contents nudge is outside the supported source range".to_owned()
                    })?;
                    // REAPER keeps MIDI events at or after source PPQ zero. Any remaining
                    // leftward displacement advances the take source offset instead.
                    source_offset_ticks = i64::try_from(remaining_shift_ticks).map_err(|_| {
                        "Contents nudge is outside the supported source range".to_owned()
                    })?;
                }
                let source_loop_length_ticks = if content_shift_ticks > 0 {
                    let content_shift_ticks = u64::try_from(content_shift_ticks).map_err(|_| {
                        "Contents nudge is outside the supported source range".to_owned()
                    })?;
                    source_loop_length
                        .checked_add(content_shift_ticks)
                        .filter(|length| *length <= (1_u64 << 53))
                        .ok_or_else(|| {
                            "Contents nudge is outside the supported source range".to_owned()
                        })?
                } else if content_shift_ticks < 0 {
                    source_loop_length
                        .checked_sub(content_shift_ticks.unsigned_abs())
                        .filter(|length| *length > 0 && *length <= (1_u64 << 53))
                        .ok_or_else(|| {
                            "Contents nudge is outside the supported source range".to_owned()
                        })?
                } else {
                    source_loop_length
                };
                if source_offset_ticks != item.signed_source_offset_ticks()
                    || content_shift_ticks != 0
                    || source_loop_length_ticks != source_loop_length
                {
                    actions.push(DawAction::SetMidiItemContents {
                        item_id: item.id(),
                        source_offset_ticks,
                        source_loop_length_ticks,
                        content_shift_ticks,
                    });
                }
                continue;
            }
            let current_phase = i128::from(item.signed_source_offset_ticks())
                .rem_euclid(i128::from(source_loop_length)) as u64;
            let signed_delta_samples = if delta_samples > 0 {
                -(sample_delta as f64)
            } else if delta_samples < 0 {
                sample_delta as f64
            } else {
                0.0
            };
            let current_offset_samples = self
                .project
                .midi_source_offset_samples(item.start_tick(), current_phase)
                .map_err(|error| error.to_string())?;
            let source_loop_samples = self
                .project
                .midi_source_offset_samples(item.start_tick(), source_loop_length)
                .map_err(|error| error.to_string())?;
            if !source_loop_samples.is_finite()
                || source_loop_samples <= 0.0
                || source_loop_samples > (1_u64 << 53) as f64
            {
                return Err("Contents nudge is outside the supported range".to_owned());
            }
            let source_offset_samples =
                (current_offset_samples + signed_delta_samples).rem_euclid(source_loop_samples);
            let source_offset_ticks = self
                .project
                .midi_source_phase_at_offset_samples(
                    item.start_tick(),
                    source_loop_length,
                    source_offset_samples,
                )
                .map_err(|error| error.to_string())?;
            if source_offset_ticks != current_phase {
                actions.push(DawAction::EditMidiItemPlacement {
                    item_id: item.id(),
                    start_tick: item.start_tick(),
                    length_ticks: item.length_ticks(),
                    source_offset_ticks: i64::try_from(source_offset_ticks).map_err(|_| {
                        "Contents nudge is outside the supported source range".to_owned()
                    })?,
                });
            }
        }
        Ok(actions)
    }

    fn midi_source_content_shift_ticks(
        &self,
        item_start_tick: u64,
        shift_samples: f64,
    ) -> Result<i64, String> {
        const MAX_EXACT: f64 = (1_u64 << 53) as f64;
        if !shift_samples.is_finite() || shift_samples.abs() > MAX_EXACT {
            return Err("Contents nudge is outside the supported source range".to_owned());
        }
        let start_sample = self
            .project
            .sample_at_tick_position(item_start_tick as f64)
            .map_err(|error| error.to_string())?;
        let target_sample = start_sample + shift_samples;
        if !target_sample.is_finite() || target_sample.abs() > MAX_EXACT {
            return Err("Contents nudge is outside the supported source range".to_owned());
        }
        let target_tick = self
            .project
            .tick_at_sample_position(target_sample)
            .map_err(|error| error.to_string())?;
        let shift_ticks = (target_tick - item_start_tick as f64).round();
        if !shift_ticks.is_finite() || shift_ticks.abs() > MAX_EXACT {
            return Err("Contents nudge is outside the supported source range".to_owned());
        }
        Ok(shift_ticks as i64)
    }

    fn audio_contents_nudge_actions(
        &self,
        items: &[&AudioItem],
        delta_samples: i128,
    ) -> Result<Vec<DawAction>, String> {
        if items.is_empty() {
            return Ok(Vec::new());
        }
        let media_refs = items
            .iter()
            .map(|item| item.media_ref().to_owned())
            .collect::<Vec<_>>();
        let source_limits = self.audio_source_limits(media_refs)?;

        let project_rate = self.project.settings().sample_rate();
        let mut actions = Vec::with_capacity(items.len());
        for item in items {
            let Some((source_rate, frame_count)) = source_limits.get(item.media_ref()).copied()
            else {
                return Err(format!(
                    "Source length is unavailable for {}",
                    item.media_ref()
                ));
            };
            let Some(source_delta) = delta_samples
                .checked_neg()
                .and_then(|delta| scale_sample_delta(delta, source_rate, project_rate))
            else {
                return Err("Contents nudge is outside the supported range".to_owned());
            };
            let source_offset_samples = if item.is_loop_source() {
                let (section_start, section_length) = match (
                    item.source_section_start_samples(),
                    item.source_section_length_samples(),
                ) {
                    (Some(start), Some(length)) => (start, length),
                    (None, None) => (0, frame_count),
                    _ => {
                        return Err(
                            "Selected audio item has an invalid loop source section".to_owned()
                        );
                    }
                };
                let Some(section_end) = section_start.checked_add(section_length) else {
                    return Err("Selected audio item has an invalid loop source section".to_owned());
                };
                let Some(current_offset) = u64::try_from(item.source_offset_samples()).ok() else {
                    return Err("Selected audio item has an invalid loop source section".to_owned());
                };
                if section_length == 0
                    || section_end > frame_count
                    || current_offset >= section_length
                {
                    return Err("Selected audio item has an invalid loop source section".to_owned());
                }
                let Some(offset) =
                    wrapped_source_sample(current_offset, 0, section_length, source_delta)
                else {
                    return Err("Contents nudge is outside the supported source range".to_owned());
                };
                let Ok(offset) = i64::try_from(offset) else {
                    return Err("Contents nudge is outside the supported source range".to_owned());
                };
                offset
            } else {
                let Some(offset) =
                    shifted_source_offset(item.source_offset_samples(), source_delta)
                else {
                    return Err(
                        "Contents nudge is outside the supported signed source range".to_owned(),
                    );
                };
                offset
            };
            if source_offset_samples != item.source_offset_samples() {
                actions.push(DawAction::EditAudioItem {
                    item_id: item.id(),
                    media_ref: item.media_ref().to_owned(),
                    start_sample: item.start_sample(),
                    source_offset_samples,
                    length_samples: item.length_samples(),
                });
            }
        }
        Ok(actions)
    }

    pub(super) fn audio_source_limits(
        &self,
        media_refs: impl IntoIterator<Item = String>,
    ) -> Result<HashMap<String, (u32, u64)>, String> {
        let mut media_refs = media_refs.into_iter().collect::<Vec<_>>();
        media_refs.sort_unstable();
        media_refs.dedup();
        if media_refs.is_empty() {
            return Ok(HashMap::new());
        }
        let path = self
            .media_store_path()
            .filter(|path| path.is_file())
            .ok_or_else(|| "audio media store is unavailable".to_owned())?;
        let store = ProjectStore::open(&path).map_err(|error| error.to_string())?;
        let mut limits = HashMap::with_capacity(media_refs.len());
        let mut metadata_result = Ok(());
        for media_ref in media_refs {
            match store.audio_asset_metadata(&media_ref) {
                Ok(Some(metadata)) => {
                    let Some(sample_rate) = metadata.sample_rate.filter(|rate| *rate > 0) else {
                        metadata_result =
                            Err(format!("source sample rate is unavailable for {media_ref}"));
                        break;
                    };
                    let Some(frame_count) = metadata.frame_count.filter(|frames| *frames > 0)
                    else {
                        metadata_result =
                            Err(format!("source frame count is unavailable for {media_ref}"));
                        break;
                    };
                    limits.insert(media_ref, (sample_rate, frame_count));
                }
                Ok(None) => {
                    metadata_result =
                        Err(format!("source metadata is unavailable for {media_ref}"));
                    break;
                }
                Err(error) => {
                    metadata_result = Err(error.to_string());
                    break;
                }
            }
        }
        let close_result = store.close().map_err(|error| error.to_string());
        metadata_result?;
        close_result?;
        Ok(limits)
    }

    fn move_edit_cursor_by_samples(&mut self, delta_samples: i128) {
        let sample = match self.timeline.edit_cursor_sample(&self.project) {
            Ok(sample) => sample,
            Err(error) => {
                self.item_nudge_dialog.error = Some(error.to_string());
                return;
            }
        };
        let Some(mut sample) = shifted_sample(sample, delta_samples) else {
            self.item_nudge_dialog.error =
                Some("The edit cursor cannot move beyond the sample timeline".to_owned());
            return;
        };
        if self.item_nudge_dialog.snap_to_unit {
            match self.snap_nudge_target_sample(sample) {
                Ok(snapped) => sample = snapped,
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error);
                    return;
                }
            }
        }
        match self.project.tick_at_sample(sample) {
            Ok(_) => {
                self.set_edit_cursor_position(crate::timeline::EditCursorPosition::Sample(sample));
                self.item_nudge_dialog.error = None;
            }
            Err(error) => self.item_nudge_dialog.error = Some(error.to_string()),
        }
    }

    fn dialog_delta_samples(&self, signed_time: bool) -> Result<i128, String> {
        let value = self.item_nudge_dialog.value.trim();
        let sample_rate = f64::from(self.project.settings().sample_rate());
        let samples = match self.item_nudge_dialog.unit {
            Unit::Milliseconds => {
                value
                    .parse::<f64>()
                    .map_err(|_| "Enter a valid millisecond amount".to_owned())?
                    * sample_rate
                    / 1_000.0
            }
            Unit::Samples => {
                if signed_time {
                    return value
                        .parse::<i64>()
                        .map(i128::from)
                        .map_err(|_| "Enter a signed whole sample amount".to_owned());
                }
                return value
                    .parse::<u64>()
                    .map(i128::from)
                    .map_err(|_| "Enter a whole sample amount".to_owned());
            }
            Unit::Time => {
                project_time::parse_project_time(value).map_err(str::to_owned)? * sample_rate
            }
            Unit::Frames => {
                let frames = value
                    .parse::<f64>()
                    .map_err(|_| "Enter a valid frame amount".to_owned())?;
                frames * sample_rate / self.project.settings().frame_rate().frames_per_second()
            }
            Unit::Pixels if signed_time => {
                let pixels = value
                    .parse::<f64>()
                    .map_err(|_| "Enter a valid pixel amount".to_owned())?;
                let anchor = self.pixel_relative_set_anchor_sample()?;
                return self.pixel_delta_samples(anchor, pixels);
            }
            Unit::Pixels => {
                return Err("Pixel nudge requires an arrangement position".to_owned());
            }
            Unit::Grid | Unit::MeasuresBeats => {
                return Err("Musical units are only available for Nudge".to_owned());
            }
            Unit::ItemLengths | Unit::ItemSelections | Unit::Notes => {
                return Err("This unit is only available for Nudge".to_owned());
            }
        };
        if !samples.is_finite() || samples.abs() >= i128::MAX as f64 {
            return Err("Nudge amount is outside the supported range".to_owned());
        }
        if !signed_time && samples < 0.0 {
            return Err("Nudge amount must not be negative".to_owned());
        }
        let samples = samples.round();
        if samples < i128::MIN as f64 || samples > i128::MAX as f64 {
            return Err("Nudge amount is outside the supported range".to_owned());
        }
        Ok(samples as i128)
    }

    fn dialog_target_sample(&self) -> Result<u64, String> {
        let sample_rate = f64::from(self.project.settings().sample_rate());
        let samples = match self.item_nudge_dialog.unit {
            Unit::Samples => self
                .item_nudge_dialog
                .value
                .trim()
                .parse::<u64>()
                .map_err(|_| "Enter a whole sample position".to_owned())?,
            Unit::Time => {
                let seconds = project_time::parse_project_time(&self.item_nudge_dialog.value)
                    .map_err(str::to_owned)?;
                if seconds < 0.0 {
                    return Err("Item position cannot be before sample zero".to_owned());
                }
                let samples = seconds * sample_rate;
                if !samples.is_finite() || samples >= u64::MAX as f64 {
                    return Err("Item position is outside the supported range".to_owned());
                }
                samples.round() as u64
            }
            Unit::Frames => {
                let frames = self
                    .item_nudge_dialog
                    .value
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| "Enter a valid frame position".to_owned())?;
                let samples =
                    frames * sample_rate / self.project.settings().frame_rate().frames_per_second();
                checked_sample_position(samples, "Frame position")?
            }
            Unit::MeasuresBeats => self.measures_beats_target_sample()?,
            Unit::Pixels => {
                let pixels = self
                    .item_nudge_dialog
                    .value
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| "Enter a valid pixel position".to_owned())?;
                let pixels = if self.item_nudge_dialog.snap_to_unit {
                    pixels.round()
                } else {
                    pixels
                };
                self.sample_at_timeline_pixel(pixels)?
            }
            _ => return Err("Select a set-position unit".to_owned()),
        };
        Ok(samples)
    }

    fn pixel_delta_samples(&self, anchor_sample: u64, pixel_delta: f64) -> Result<i128, String> {
        if !pixel_delta.is_finite() {
            return Err("Pixel nudge is outside the supported range".to_owned());
        }
        let anchor_pixel = self.timeline_pixel_for_sample(anchor_sample)?;
        let target_sample = self.sample_at_timeline_pixel(anchor_pixel + pixel_delta)?;
        Ok(i128::from(target_sample) - i128::from(anchor_sample))
    }

    fn timeline_pixel_for_sample(&self, sample: u64) -> Result<f64, String> {
        let tick = self
            .project
            .tick_at_sample(sample)
            .map_err(|error| error.to_string())?;
        let pixels_per_tick = f64::from(self.timeline.pixels_per_tick);
        if !pixels_per_tick.is_finite() || pixels_per_tick <= 0.0 {
            return Err("Arrangement pixel scale is unavailable".to_owned());
        }
        Ok((tick as f64 - self.timeline.origin_tick as f64) * pixels_per_tick)
    }

    fn sample_at_timeline_pixel(&self, pixel: f64) -> Result<u64, String> {
        let pixels_per_tick = f64::from(self.timeline.pixels_per_tick);
        if !pixel.is_finite() || pixel < 0.0 {
            return Err("Pixel position cannot be before the visible timeline origin".to_owned());
        }
        if !pixels_per_tick.is_finite() || pixels_per_tick <= 0.0 {
            return Err("Arrangement pixel scale is unavailable".to_owned());
        }
        let tick = self.timeline.origin_tick as f64 + pixel / pixels_per_tick;
        if !tick.is_finite() || tick < 0.0 || tick >= u64::MAX as f64 {
            return Err("Pixel position is outside the supported timeline".to_owned());
        }
        self.project
            .sample_at_tick(tick.round() as u64)
            .map_err(|error| error.to_string())
    }

    fn snap_nudge_target_sample(&self, sample: u64) -> Result<u64, String> {
        match self.item_nudge_dialog.unit {
            Unit::Pixels => {
                self.sample_at_timeline_pixel(self.timeline_pixel_for_sample(sample)?.floor())
            }
            Unit::Frames => self.nearest_frame_sample(sample),
            Unit::Grid
            | Unit::MeasuresBeats
            | Unit::ItemLengths
            | Unit::ItemSelections
            | Unit::Notes => {
                let delta = self.snap_musical_delta_to_sample(0, i128::from(sample))?;
                u64::try_from(delta)
                    .map_err(|_| "Snapped position is outside the supported range".to_owned())
            }
            _ => {
                let unit = self.snap_resolution_samples()?;
                Ok(round_sample(sample, unit))
            }
        }
    }

    fn snap_set_target_sample(&self, sample: u64) -> Result<u64, String> {
        match self.item_nudge_dialog.unit {
            Unit::Pixels => {
                self.sample_at_timeline_pixel(self.timeline_pixel_for_sample(sample)?.round())
            }
            Unit::Frames => self.nearest_frame_sample(sample),
            Unit::MeasuresBeats => {
                let target_tick = self
                    .project
                    .tick_at_sample_position(sample as f64)
                    .map_err(|error| error.to_string())?;
                let grid = self
                    .timeline
                    .snap_grid
                    .tick_interval(self.project.settings().ppq())
                    .ok_or_else(|| {
                        "Current Snap Grid cannot be represented in project ticks".to_owned()
                    })?;
                let snapped_tick = (target_tick / grid as f64).round() * grid as f64;
                let snapped_sample = self
                    .project
                    .sample_at_tick_position(snapped_tick)
                    .map_err(|error| error.to_string())?;
                checked_sample_position(snapped_sample, "Snapped musical position")
            }
            Unit::Grid => Err("Grid units are not available for Set".to_owned()),
            _ => {
                let unit = self.snap_resolution_samples()?;
                Ok(round_sample(sample, unit))
            }
        }
    }

    fn measures_beats_target_sample(&self) -> Result<u64, String> {
        let value = self.item_nudge_dialog.value.trim();
        let fields = value.split('.').collect::<Vec<_>>();
        if !(2..=3).contains(&fields.len()) {
            return Err("Enter a measure.beat.hundredths position".to_owned());
        }
        let measure = fields[0]
            .parse::<i64>()
            .map_err(|_| "Enter a valid measure.beat.hundredths position".to_owned())?;
        let beat = fields[1]
            .parse::<u32>()
            .map_err(|_| "Enter a valid measure.beat.hundredths position".to_owned())?;
        let fraction = if fields.len() == 3 {
            fields[2]
                .parse::<u32>()
                .map_err(|_| "Enter a valid measure.beat.hundredths position".to_owned())?
        } else {
            0
        };
        if !(0..=99).contains(&fraction) {
            return Err("Beat fraction must be between .00 and .99".to_owned());
        }
        let beat_tick = self
            .project
            .tick_at_musical_position(measure, beat, 0.0)
            .map_err(|error| error.to_string())?;
        let meter = self
            .project
            .time_signature_at_tick(beat_tick.max(0.0).floor() as u64);
        let ticks_per_beat =
            u64::from(self.project.settings().ppq()) * 4 / u64::from(meter.denominator());
        let tick_in_beat = fraction as f64 * ticks_per_beat as f64 / 100.0;
        let target_tick = self
            .project
            .tick_at_musical_position(measure, beat, tick_in_beat)
            .map_err(|error| error.to_string())?;
        let target_sample = self
            .project
            .sample_at_tick_position(target_tick)
            .map_err(|error| error.to_string())?;
        checked_sample_position(target_sample, "Musical position")
    }

    fn pixel_relative_set_anchor_sample(&self) -> Result<u64, String> {
        if self.item_nudge_dialog.target == Target::EditCursor {
            return self
                .timeline
                .edit_cursor_sample(&self.project)
                .map_err(|error| error.to_string());
        }
        let selected = self.selected_item_start_samples()?;
        selected
            .first()
            .map(|(_, sample)| *sample)
            .ok_or_else(|| "No selected items to set".to_owned())
    }

    fn snap_resolution_samples(&self) -> Result<u64, String> {
        let sample_rate = u64::from(self.project.settings().sample_rate());
        let unit = match self.item_nudge_dialog.unit {
            Unit::Milliseconds => (sample_rate + 500) / 1_000,
            Unit::Samples => 1,
            Unit::Time => sample_rate,
            Unit::Frames => {
                return Err("Frame snapping requires a frame-grid position".to_owned());
            }
            Unit::Pixels => {
                return Err("Pixel snapping requires an arrangement position".to_owned());
            }
            Unit::Grid | Unit::MeasuresBeats => {
                return Err("Musical snapping requires a selected item".to_owned());
            }
            Unit::ItemLengths | Unit::ItemSelections | Unit::Notes => {
                return Err("This unit requires an item or musical grid anchor".to_owned());
            }
        };
        if unit == 0 {
            Err("Project sample rate is unavailable".to_owned())
        } else {
            Ok(unit)
        }
    }

    fn snap_delta_to_selected_anchor(&self, delta_samples: i128) -> Result<i128, String> {
        let selected = self.selected_item_start_samples()?;
        let Some((_, first_sample)) = selected.first() else {
            return Err("No selected items to move".to_owned());
        };
        if matches!(
            self.item_nudge_dialog.unit,
            Unit::Grid
                | Unit::MeasuresBeats
                | Unit::ItemLengths
                | Unit::ItemSelections
                | Unit::Notes
        ) {
            return self.snap_musical_delta_to_sample(*first_sample, delta_samples);
        }
        let target = shifted_sample(*first_sample, delta_samples)
            .ok_or_else(|| "Selected items cannot move beyond the sample timeline".to_owned())?;
        let snapped = self.snap_nudge_target_sample(target)?;
        Ok(i128::from(snapped) - i128::from(*first_sample))
    }

    fn nudge_delta_samples(&self, direction: i8) -> Result<i128, String> {
        match self.item_nudge_dialog.unit {
            Unit::Grid => return self.grid_delta_samples(direction),
            Unit::MeasuresBeats => return self.measures_beats_delta_samples(direction),
            Unit::ItemLengths | Unit::ItemSelections => {
                let selected = self.selected_item_start_samples()?;
                let Some((first_id, first_start)) = selected.first().copied() else {
                    return Err("No selected items to move".to_owned());
                };
                let unit_samples = match self.item_nudge_dialog.unit {
                    Unit::ItemLengths => self
                        .selected_item_end_sample(first_id)?
                        .checked_sub(first_start)
                        .ok_or_else(|| "Selected item has an invalid length".to_owned())?,
                    Unit::ItemSelections => {
                        let selection_end =
                            selected.iter().try_fold(first_start, |end, (id, _)| {
                                self.selected_item_end_sample(*id)
                                    .map(|item_end| end.max(item_end))
                            })?;
                        selection_end
                            .checked_sub(first_start)
                            .ok_or_else(|| "Selected item range is invalid".to_owned())?
                    }
                    _ => unreachable!("item-unit branch is exhaustive"),
                };
                if unit_samples == 0 {
                    return Err("Selected item has no duration".to_owned());
                }
                let samples = unit_samples as f64 * self.positive_nudge_count()?;
                return Ok(checked_signed_delta(samples, "Nudge amount")? * i128::from(direction));
            }
            Unit::Notes => return self.note_value_delta_samples(direction),
            Unit::Pixels => {
                let anchor = if self.item_nudge_dialog.target == Target::EditCursor {
                    self.timeline
                        .edit_cursor_sample(&self.project)
                        .map_err(|error| error.to_string())?
                } else {
                    self.selected_item_start_samples()?
                        .first()
                        .map(|(_, sample)| *sample)
                        .ok_or_else(|| "No selected items to move".to_owned())?
                };
                let amount = self
                    .item_nudge_dialog
                    .value
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| "Enter a valid pixel amount".to_owned())?;
                if amount <= 0.0 || !amount.is_finite() {
                    return Err("Pixel nudge must be greater than zero".to_owned());
                }
                return self.pixel_delta_samples(anchor, amount * f64::from(direction));
            }
            _ => {}
        }
        self.dialog_delta_samples(false)
            .map(|delta| delta * i128::from(direction))
    }

    fn nearest_frame_sample(&self, sample: u64) -> Result<u64, String> {
        let samples_per_frame = f64::from(self.project.settings().sample_rate())
            / self.project.settings().frame_rate().frames_per_second();
        let frame_index = (sample as f64 / samples_per_frame).round();
        let snapped_sample = frame_index * samples_per_frame;
        if !snapped_sample.is_finite() || snapped_sample < 0.0 || snapped_sample >= u64::MAX as f64
        {
            return Err("Snapped frame position is outside the supported range".to_owned());
        }
        Ok(snapped_sample.round() as u64)
    }

    fn grid_delta_samples(&self, direction: i8) -> Result<i128, String> {
        let grid_count = self
            .item_nudge_dialog
            .value
            .trim()
            .parse::<f64>()
            .map_err(|_| "Enter a valid grid-unit amount".to_owned())?;
        if !grid_count.is_finite() || grid_count <= 0.0 {
            return Err("Grid-unit amount must be greater than zero".to_owned());
        }
        let anchor_sample = self.musical_nudge_anchor_sample()?;
        let start_tick = self
            .project
            .tick_at_sample_position(anchor_sample as f64)
            .map_err(|error| error.to_string())?;
        let interval = self
            .timeline
            .snap_grid
            .tick_interval(self.project.settings().ppq())
            .ok_or_else(|| "Current Snap Grid cannot be represented in project ticks".to_owned())?;
        let target_tick = start_tick + f64::from(direction) * grid_count * interval as f64;
        if !target_tick.is_finite() || target_tick < 0.0 {
            return Err("Nudge would move the selected item before sample zero".to_owned());
        }
        let target_sample = self
            .project
            .sample_at_tick_position(target_tick)
            .map_err(|error| error.to_string())?;
        let delta = target_sample.round() - anchor_sample as f64;
        if !delta.is_finite() || delta.abs() >= i128::MAX as f64 {
            return Err("Grid-unit nudge is outside the supported range".to_owned());
        }
        Ok(delta as i128)
    }

    fn measures_beats_delta_samples(&self, direction: i8) -> Result<i128, String> {
        let value = self.item_nudge_dialog.value.trim();
        let (measures, beats) = value.split_once('.').unwrap_or((value, "0"));
        let measures = measures
            .parse::<u64>()
            .map_err(|_| "Enter a valid measure.beat amount".to_owned())?;
        let beats = beats
            .parse::<u64>()
            .map_err(|_| "Enter a valid measure.beat amount".to_owned())?;
        if measures == 0 && beats == 0 {
            return Err("Measure.beat amount must be greater than zero".to_owned());
        }
        let anchor_sample = self.musical_nudge_anchor_sample()?;
        let start_tick = self
            .project
            .tick_at_sample_position(anchor_sample as f64)
            .map_err(|error| error.to_string())?;
        let meter = self
            .project
            .time_signature_at_tick(start_tick.floor() as u64);
        let ppq = u64::from(self.project.settings().ppq());
        let ticks_per_beat = ppq
            .checked_mul(4)
            .and_then(|ticks| ticks.checked_div(u64::from(meter.denominator())))
            .ok_or_else(|| "Current meter cannot be represented in project ticks".to_owned())?;
        let ticks_per_measure = ticks_per_beat
            .checked_mul(u64::from(meter.numerator()))
            .ok_or_else(|| "Measure.beat amount is outside the supported range".to_owned())?;
        let amount_ticks = measures
            .checked_mul(ticks_per_measure)
            .and_then(|ticks| ticks.checked_add(beats.checked_mul(ticks_per_beat)?))
            .ok_or_else(|| "Measure.beat amount is outside the supported range".to_owned())?;
        let target_tick = start_tick + f64::from(direction) * amount_ticks as f64;
        if !target_tick.is_finite() || target_tick < 0.0 {
            return Err("Nudge would move the selected item before sample zero".to_owned());
        }
        let target_sample = self
            .project
            .sample_at_tick_position(target_tick)
            .map_err(|error| error.to_string())?;
        let delta = target_sample.round() - anchor_sample as f64;
        if !delta.is_finite() || delta.abs() >= i128::MAX as f64 {
            return Err("Measure.beat nudge is outside the supported range".to_owned());
        }
        Ok(delta as i128)
    }

    fn musical_nudge_anchor_sample(&self) -> Result<u64, String> {
        if self.item_nudge_dialog.target == Target::EditCursor {
            return self
                .timeline
                .edit_cursor_sample(&self.project)
                .map_err(|error| error.to_string());
        }
        self.selected_item_start_samples()?
            .first()
            .map(|(_, sample)| *sample)
            .ok_or_else(|| "No selected items to move".to_owned())
    }

    fn positive_nudge_count(&self) -> Result<f64, String> {
        let count = self
            .item_nudge_dialog
            .value
            .trim()
            .parse::<f64>()
            .map_err(|_| "Enter a valid positive unit count".to_owned())?;
        if !count.is_finite() || count <= 0.0 {
            return Err("Nudge amount must be greater than zero".to_owned());
        }
        Ok(count)
    }

    fn note_value_delta_samples(&self, direction: i8) -> Result<i128, String> {
        let count = self.positive_nudge_count()?;
        let anchor = self.musical_nudge_anchor_sample()?;
        let anchor_tick = self
            .project
            .tick_at_sample_position(anchor as f64)
            .map_err(|error| error.to_string())?;
        let delta_ticks = self
            .item_nudge_dialog
            .note_value
            .ticks(self.project.settings().ppq())
            * count
            * f64::from(direction);
        let target_tick = anchor_tick + delta_ticks;
        if !target_tick.is_finite() || target_tick < 0.0 || target_tick >= u64::MAX as f64 {
            return Err("Nudge amount is outside the supported range".to_owned());
        }
        let target_sample = self
            .project
            .sample_at_tick_position(target_tick)
            .map_err(|error| error.to_string())?;
        if !target_sample.is_finite() || target_sample < 0.0 || target_sample >= u64::MAX as f64 {
            return Err("Nudge amount is outside the supported range".to_owned());
        }
        checked_signed_delta(target_sample - anchor as f64, "Nudge amount")
    }

    fn snap_musical_delta_to_sample(
        &self,
        anchor_sample: u64,
        delta_samples: i128,
    ) -> Result<i128, String> {
        let target_sample = shifted_sample(anchor_sample, delta_samples)
            .ok_or_else(|| "Selected items cannot move beyond the sample timeline".to_owned())?;
        let target_tick = self
            .project
            .tick_at_sample_position(target_sample as f64)
            .map_err(|error| error.to_string())?;
        let snapped_tick = match self.item_nudge_dialog.unit {
            Unit::Grid | Unit::ItemLengths | Unit::ItemSelections => {
                let interval = self
                    .timeline
                    .snap_grid
                    .tick_interval(self.project.settings().ppq())
                    .ok_or_else(|| {
                        "Current Snap Grid cannot be represented in project ticks".to_owned()
                    })?;
                (target_tick / interval as f64).round() * interval as f64
            }
            Unit::Notes => {
                let interval = self
                    .item_nudge_dialog
                    .note_value
                    .ticks(self.project.settings().ppq());
                (target_tick / interval).round() * interval
            }
            Unit::MeasuresBeats => {
                let target_tick_floor = target_tick.floor();
                if target_tick_floor < 0.0 || target_tick_floor >= u64::MAX as f64 {
                    return Err("Snapped item position is outside the supported range".to_owned());
                }
                let measure = self
                    .project
                    .musical_position_at_tick(target_tick_floor as u64)
                    .map_err(|error| error.to_string())?
                    .measure();
                let measure_start = self
                    .project
                    .tick_at_musical_position(measure, 1, 0.0)
                    .map_err(|error| error.to_string())?;
                let next_measure_start = self
                    .project
                    .tick_at_musical_position(
                        measure.checked_add(1).ok_or_else(|| {
                            "Snapped measure is outside the supported range".to_owned()
                        })?,
                        1,
                        0.0,
                    )
                    .map_err(|error| error.to_string())?;
                if target_tick - measure_start <= next_measure_start - target_tick {
                    measure_start
                } else {
                    next_measure_start
                }
            }
            _ => return Err("Select a musical Nudge unit before snapping".to_owned()),
        };
        let snapped_sample = self
            .project
            .sample_at_tick_position(snapped_tick)
            .map_err(|error| error.to_string())?;
        if !snapped_sample.is_finite() || snapped_sample < 0.0 || snapped_sample >= u64::MAX as f64
        {
            return Err("Snapped item position is outside the supported range".to_owned());
        }
        Ok(snapped_sample.round() as i128 - i128::from(anchor_sample))
    }

    pub(super) fn selected_item_start_samples(&self) -> Result<Vec<(ItemId, u64)>, String> {
        let selected_ids = &self.timeline.selected_items;
        let mut selected = self
            .project
            .audio_items()
            .iter()
            .filter(|item| selected_ids.contains(&item.id()))
            .map(|item| (item.id(), item.start_sample()))
            .collect::<Vec<_>>();
        for item in self
            .project
            .midi_items()
            .iter()
            .filter(|item| selected_ids.contains(&item.id()))
        {
            selected.push((
                item.id(),
                checked_sample_position(
                    self.project
                        .sample_at_tick_position(item.start_position().as_ticks_f64())
                        .map_err(|error| error.to_string())?,
                    "MIDI item position",
                )?,
            ));
        }
        selected.sort_by_key(|(_, sample)| *sample);
        Ok(selected)
    }
}

fn target_supports_relative_set(target: Target) -> bool {
    matches!(
        target,
        Target::Position | Target::EndPosition | Target::Duplicate | Target::EditCursor
    )
}

fn wrapped_source_sample(current: u64, start: u64, length: u64, delta: i128) -> Option<u64> {
    let end = start.checked_add(length)?;
    if length == 0 || current < start || current >= end {
        return None;
    }
    let modulus = u128::from(length);
    let phase = u128::from(current - start);
    let delta_phase = u128::try_from(delta.rem_euclid(i128::from(length))).ok()?;
    let wrapped_phase = (phase + delta_phase) % modulus;
    start.checked_add(u64::try_from(wrapped_phase).ok()?)
}

fn shifted_sample(sample: u64, delta: i128) -> Option<u64> {
    let shifted = i128::from(sample).checked_add(delta)?;
    u64::try_from(shifted).ok()
}

fn midi_item_position_at_sample(
    project: &aaadaw_core::Project,
    sample: f64,
) -> Result<MidiItemPosition, String> {
    let tick = project
        .tick_at_sample_position(sample)
        .map_err(|error| error.to_string())?;
    let scaled = tick * f64::from(MIDI_ITEM_SUBTICKS_PER_TICK);
    if !scaled.is_finite() || scaled < 0.0 || scaled > ((1_u64 << 53) - 1) as f64 {
        return Err("MIDI item position is outside the supported range".to_owned());
    }
    MidiItemPosition::from_total_subticks(scaled.round() as u128)
        .ok_or_else(|| "MIDI item position is outside the supported range".to_owned())
}

fn checked_sample_position(sample: f64, label: &str) -> Result<u64, String> {
    if !sample.is_finite() || sample < 0.0 || sample >= u64::MAX as f64 {
        return Err(format!("{label} is outside the supported sample timeline"));
    }
    Ok(sample.round() as u64)
}

fn checked_signed_delta(samples: f64, label: &str) -> Result<i128, String> {
    if !samples.is_finite() || samples.abs() >= i128::MAX as f64 {
        return Err(format!("{label} is outside the supported range"));
    }
    let samples = samples.round();
    if samples < i128::MIN as f64 || samples > i128::MAX as f64 {
        return Err(format!("{label} is outside the supported range"));
    }
    Ok(samples as i128)
}

fn shifted_source_offset(sample: i64, delta: i128) -> Option<i64> {
    let shifted = i128::from(sample).checked_add(delta)?;
    i64::try_from(shifted).ok()
}

fn shifted_unsigned(value: u64, delta: i128) -> Option<u64> {
    let shifted = i128::from(value).checked_add(delta)?;
    if shifted <= 0 {
        return None;
    }
    u64::try_from(shifted).ok()
}

fn scale_sample_delta(delta: i128, source_rate: u32, project_rate: u32) -> Option<i128> {
    if source_rate == 0 || project_rate == 0 {
        return None;
    }
    let numerator = delta.checked_mul(i128::from(source_rate))?;
    let denominator = i128::from(project_rate);
    let magnitude = numerator.checked_abs()?;
    let rounded = magnitude
        .checked_add(denominator / 2)?
        .checked_div(denominator)?;
    if numerator < 0 {
        rounded.checked_neg()
    } else {
        Some(rounded)
    }
}

fn round_to_unit(value: i128, unit: i128) -> i128 {
    if unit <= 1 {
        value
    } else {
        let quotient = value.div_euclid(unit);
        let remainder = value.rem_euclid(unit);
        let halfway = unit / 2 + unit % 2;
        quotient
            .saturating_add(i128::from(remainder >= halfway))
            .saturating_mul(unit)
    }
}

fn round_sample(sample: u64, unit: u64) -> u64 {
    round_to_unit(i128::from(sample), i128::from(unit)).clamp(0, i128::from(u64::MAX)) as u64
}
