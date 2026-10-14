use super::{App, Message, commands};
use crate::project_time;
use aaadaw_core::{DawAction, ItemId};
use iced::Task;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Unit {
    #[default]
    Milliseconds,
    Seconds,
    Samples,
    Time,
}

impl std::fmt::Display for Unit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Milliseconds => "milliseconds",
            Self::Seconds => "seconds",
            Self::Samples => "samples",
            Self::Time => "minutes:seconds",
        })
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

#[derive(Debug, Clone)]
pub(super) struct Dialog {
    pub(super) mode: Mode,
    pub(super) target: Target,
    pub(super) unit: Unit,
    pub(super) value: String,
    pub(super) snap_to_unit: bool,
    pub(super) relative_set: bool,
    pub(super) error: Option<String>,
}

impl Default for Dialog {
    fn default() -> Self {
        Self {
            mode: Mode::Nudge,
            target: Target::Position,
            unit: Unit::Milliseconds,
            value: "1".to_owned(),
            snap_to_unit: false,
            relative_set: false,
            error: None,
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
        match mode {
            Mode::Nudge => {
                self.unit = Unit::Milliseconds;
                self.value = "1".to_owned();
            }
            Mode::Set => {
                self.unit = Unit::Time;
                self.value = "0:00.000".to_owned();
            }
        }
    }

    fn set_unit(&mut self, unit: Unit) {
        self.unit = unit;
        self.error = None;
        self.value = match (self.mode, unit) {
            (Mode::Nudge, Unit::Milliseconds | Unit::Seconds) => "1".to_owned(),
            (Mode::Nudge, Unit::Samples) => "1".to_owned(),
            (Mode::Set, Unit::Time) => "0:00.000".to_owned(),
            (Mode::Set, Unit::Samples) => "0".to_owned(),
            _ => self.value.clone(),
        };
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
    }

    pub(super) fn change_item_nudge_unit(&mut self, unit: Unit) {
        let valid = match self.item_nudge_dialog.mode {
            Mode::Nudge => matches!(unit, Unit::Milliseconds | Unit::Seconds | Unit::Samples),
            Mode::Set => matches!(unit, Unit::Time | Unit::Samples),
        };
        if valid {
            self.item_nudge_dialog.set_unit(unit);
        }
    }

    pub(super) fn change_item_nudge_target(&mut self, target: Target) {
        self.item_nudge_dialog.target = target;
        self.item_nudge_dialog.error = None;
    }

    pub(super) fn change_item_nudge_value(&mut self, value: String) {
        self.item_nudge_dialog.value = value;
        self.item_nudge_dialog.error = None;
    }

    pub(super) fn change_item_nudge_snap(&mut self, enabled: bool) {
        self.item_nudge_dialog.snap_to_unit = enabled;
    }

    pub(super) fn change_item_nudge_relative_set(&mut self, enabled: bool) {
        self.item_nudge_dialog.relative_set = enabled;
    }

    pub(super) fn item_nudge_get_cursor(&mut self) {
        if self.item_nudge_dialog.mode != Mode::Set {
            return;
        }
        let result = self
            .project
            .sample_at_tick(self.timeline.edit_cursor_tick)
            .map_err(|error| error.to_string());
        match result {
            Ok(sample) => {
                self.item_nudge_dialog.value = match self.item_nudge_dialog.unit {
                    Unit::Samples => sample.to_string(),
                    _ => project_time::format_project_time(
                        sample as f64 / f64::from(self.project.settings().sample_rate()),
                    ),
                };
                self.item_nudge_dialog.error = None;
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
                let delta = match self.dialog_delta_samples(false) {
                    Ok(delta) if delta > 0 => delta * i128::from(direction),
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
        if self.item_nudge_dialog.target != Target::Position {
            self.item_nudge_dialog.error = Some(format!(
                "The '{}' nudge target is not implemented yet",
                self.item_nudge_dialog.target
            ));
            return;
        }
        match self.item_nudge_dialog.mode {
            Mode::Nudge => {
                let delta = match self.dialog_delta_samples(false) {
                    Ok(delta) if delta > 0 => delta,
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
                let mut signed_delta = delta * i128::from(direction);
                if self.item_nudge_dialog.snap_to_unit {
                    signed_delta = match self.snap_delta_to_selected_anchor(signed_delta) {
                        Ok(delta) => delta,
                        Err(error) => {
                            self.item_nudge_dialog.error = Some(error);
                            return;
                        }
                    };
                }
                self.shift_selected_items(signed_delta);
            }
            Mode::Set => self.apply_set_position(),
        }
    }

    pub(super) fn apply_item_nudge_set(&mut self) {
        if self.item_nudge_dialog.mode != Mode::Set {
            return;
        }
        match self.item_nudge_dialog.target {
            Target::Position => self.apply_set_position(),
            Target::EditCursor => self.apply_edit_cursor_set(),
            target => {
                self.item_nudge_dialog.error = Some(format!(
                    "The '{}' set target is not implemented yet",
                    target
                ));
            }
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
            let current = match self.project.sample_at_tick(self.timeline.edit_cursor_tick) {
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
            match self.snap_resolution_samples() {
                Ok(unit) => sample = round_sample(sample, unit),
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error);
                    return;
                }
            }
        }
        match self.project.tick_at_sample(sample) {
            Ok(tick) => {
                self.timeline.edit_cursor_tick = tick;
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
            match self.snap_resolution_samples() {
                Ok(unit) => {
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
                    delta = i128::from(round_sample(target, unit)) - i128::from(first_sample);
                }
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error);
                    return;
                }
            }
        }
        self.shift_selected_items(delta);
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
            let start_sample = match self.project.sample_at_tick(item.start_tick()) {
                Ok(sample) => sample,
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error.to_string());
                    return;
                }
            };
            let Some(new_sample) = shifted_sample(start_sample, delta_samples) else {
                self.item_nudge_dialog.error =
                    Some("Selected items cannot move beyond the sample timeline".to_owned());
                return;
            };
            let start_tick = match self.project.tick_at_sample(new_sample) {
                Ok(tick) => tick,
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error.to_string());
                    return;
                }
            };
            if start_tick != item.start_tick() {
                actions.push(DawAction::EditMidiItem {
                    item_id: item.id(),
                    start_tick,
                    length_ticks: item.length_ticks(),
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

    fn move_edit_cursor_by_samples(&mut self, delta_samples: i128) {
        let sample = match self.project.sample_at_tick(self.timeline.edit_cursor_tick) {
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
            match self.snap_resolution_samples() {
                Ok(unit) => sample = round_sample(sample, unit),
                Err(error) => {
                    self.item_nudge_dialog.error = Some(error);
                    return;
                }
            }
        }
        match self.project.tick_at_sample(sample) {
            Ok(tick) => {
                self.timeline.edit_cursor_tick = tick;
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
            Unit::Seconds => {
                value
                    .parse::<f64>()
                    .map_err(|_| "Enter a valid seconds amount".to_owned())?
                    * sample_rate
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
            Unit::Time if signed_time => {
                project_time::parse_project_time(value).map_err(str::to_owned)? * sample_rate
            }
            Unit::Time => return Err("Select a time unit for Nudge mode".to_owned()),
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
            _ => return Err("Select a set-position unit".to_owned()),
        };
        Ok(samples)
    }

    fn snap_resolution_samples(&self) -> Result<u64, String> {
        let sample_rate = u64::from(self.project.settings().sample_rate());
        let unit = match self.item_nudge_dialog.unit {
            Unit::Milliseconds => (sample_rate + 500) / 1_000,
            Unit::Seconds => sample_rate,
            Unit::Samples => 1,
            Unit::Time => (sample_rate + 500) / 1_000,
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
        let target = shifted_sample(*first_sample, delta_samples)
            .ok_or_else(|| "Selected items cannot move beyond the sample timeline".to_owned())?;
        let unit = self.snap_resolution_samples()?;
        Ok(i128::from(round_sample(target, unit)) - i128::from(*first_sample))
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
                self.project
                    .sample_at_tick(item.start_tick())
                    .map_err(|error| error.to_string())?,
            ));
        }
        selected.sort_by_key(|(_, sample)| *sample);
        Ok(selected)
    }
}

fn shifted_sample(sample: u64, delta: i128) -> Option<u64> {
    let shifted = i128::from(sample).checked_add(delta)?;
    u64::try_from(shifted).ok()
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
