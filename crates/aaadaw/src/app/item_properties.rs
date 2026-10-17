mod beats;
use super::{App, Message};
use crate::project_time::{
    format_project_time, format_project_timecode, parse_project_time, parse_project_timecode,
};
use aaadaw_core::{
    AudioFade, AudioItem, AudioItemFades, FadeCurve, FadeCurveParameters, ItemId, MidiItem,
    MidiItemPosition, Project,
};
use iced::Task;
use serde::{Deserialize, Serialize};

pub(super) fn format_marker_position(project: &Project, tick: u64) -> Result<String, String> {
    let sample = project
        .sample_at_tick_position(tick as f64)
        .map_err(|error| error.to_string())?;
    beats::format_position(project, sample)
}

pub(super) fn parse_marker_position(project: &Project, text: &str) -> Result<u64, String> {
    let sample = beats::parse_position(project, text)?;
    let tick = project
        .tick_at_sample_position(sample)
        .map_err(|error| error.to_string())?;
    if !tick.is_finite() || tick < 0.0 || tick >= u64::MAX as f64 {
        return Err("Marker position is outside the supported project range".to_owned());
    }
    Ok(tick.round() as u64)
}

pub(super) fn format_marker_length(
    project: &Project,
    start_tick: u64,
    end_tick: u64,
) -> Result<String, String> {
    let start_sample = project
        .sample_at_tick_position(start_tick as f64)
        .map_err(|error| error.to_string())?;
    let end_sample = project
        .sample_at_tick_position(end_tick as f64)
        .map_err(|error| error.to_string())?;
    beats::format_length(project, start_sample, end_sample - start_sample)
}

pub(super) fn parse_marker_length_end_tick(
    project: &Project,
    start_tick: u64,
    text: &str,
) -> Result<u64, String> {
    let start_sample = project
        .sample_at_tick_position(start_tick as f64)
        .map_err(|error| error.to_string())?;
    let length_samples = beats::parse_length(project, text, start_sample)?;
    let end_tick = project
        .tick_at_sample_position(start_sample + length_samples)
        .map_err(|error| error.to_string())?
        .round();
    if !end_tick.is_finite()
        || end_tick <= start_tick as f64
        || end_tick > aaadaw_core::MAX_PROJECT_REGION_TICKS as f64
    {
        return Err("Length must end after the region start within the supported range".to_owned());
    }
    Ok(end_tick as u64)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TimeUnit {
    #[default]
    Time,
    Beats,
    Hmsf,
    Samples,
}

impl std::fmt::Display for TimeUnit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Time => "Time",
            Self::Beats => "Beats",
            Self::Hmsf => "H:M:S:F",
            Self::Samples => "Samples",
        })
    }
}

#[derive(Serialize, Deserialize)]
struct PropertiesConfig {
    version: u32,
    time_unit: TimeUnit,
}

pub(super) fn load_unit() -> Result<TimeUnit, String> {
    let Some(path) = super::config_paths::config_file_path("item-properties.json") else {
        return Ok(TimeUnit::default());
    };
    load_unit_path(&path)
}

fn load_unit_path(path: &std::path::Path) -> Result<TimeUnit, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(TimeUnit::default());
        }
        Err(error) => return Err(error.to_string()),
    };
    let config: PropertiesConfig =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if config.version != 1 {
        return Err("Unsupported Item Properties configuration version".to_owned());
    }
    Ok(config.time_unit)
}

fn save_unit_path(path: &std::path::Path, unit: TimeUnit) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(&PropertiesConfig {
        version: 1,
        time_unit: unit,
    })
    .map_err(|error| error.to_string())?;
    super::config_paths::write_atomic(path, &bytes).map_err(|error| error.to_string())
}

impl App {
    pub(super) fn change_item_properties_unit(&mut self, unit: TimeUnit) {
        if self.item_properties_generation != self.project_generation {
            return;
        }
        let Some(item_id) = self
            .item_properties
            .as_ref()
            .map(|draft| draft.item_id)
            .or_else(|| {
                self.midi_item_properties
                    .as_ref()
                    .map(|draft| draft.item_id)
            })
        else {
            return;
        };
        if self
            .item_properties
            .as_ref()
            .is_some_and(|draft| draft.unit != unit)
        {
            let Some(item) = self
                .project
                .audio_items()
                .iter()
                .find(|item| item.id() == item_id)
            else {
                return;
            };
            self.item_properties =
                Some(ItemProperties::from_project_item(item, &self.project, unit));
        } else if self
            .midi_item_properties
            .as_ref()
            .is_some_and(|draft| draft.unit != unit)
        {
            let Some(item) = self
                .project
                .midi_items()
                .iter()
                .find(|item| item.id() == item_id)
            else {
                return;
            };
            self.midi_item_properties =
                Some(MidiItemProperties::from_item(item, &self.project, unit));
        }
        self.item_properties_unit = unit;
        let result = super::config_paths::config_file_path("item-properties.json")
            .ok_or_else(|| "No application configuration directory".to_owned())
            .and_then(|path| save_unit_path(&path, unit));
        if let Err(error) = result {
            let message = format!("Display unit could not be saved: {error}");
            if let Some(draft) = &mut self.item_properties {
                draft.error = Some(message);
            } else if let Some(draft) = &mut self.midi_item_properties {
                draft.error = Some(message);
            }
        }
    }

    pub(super) fn open_item_properties(&mut self) -> Task<Message> {
        if let Some(id) = self.item_properties_window_id {
            return iced::window::gain_focus(id);
        }
        if self.timeline.selected_items.len() > 1 {
            let mut item_ids = self
                .timeline
                .selected_items
                .iter()
                .copied()
                .collect::<Vec<_>>();
            item_ids.sort_unstable_by_key(|item_id| item_id.value());
            let Some(draft) = BatchItemProperties::from_project(&item_ids, &self.project) else {
                return Task::none();
            };
            self.item_properties = None;
            self.midi_item_properties = None;
            self.batch_item_properties = Some(draft);
            self.item_properties_generation = self.project_generation;
            self.timeline.context_item = None;
            self.timeline.context_fade = None;
            let (id, task) = iced::window::open(iced::window::Settings {
                size: iced::Size::new(526.0, 220.0),
                min_size: Some(iced::Size::new(526.0, 220.0)),
                ..iced::window::Settings::default()
            });
            self.item_properties_window_id = Some(id);
            return task.discard();
        }
        let Some(item_id) = self.timeline.selected_item else {
            return Task::none();
        };
        if let Some(item) = self
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == item_id)
        {
            self.item_properties = Some(ItemProperties::from_project_item(
                item,
                &self.project,
                self.item_properties_unit,
            ));
            self.midi_item_properties = None;
            self.batch_item_properties = None;
        } else if let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == item_id)
        {
            self.item_properties = None;
            self.midi_item_properties = Some(MidiItemProperties::from_item(
                item,
                &self.project,
                self.item_properties_unit,
            ));
            self.batch_item_properties = None;
        } else {
            return Task::none();
        }
        self.item_properties_generation = self.project_generation;
        self.timeline.context_item = None;
        self.timeline.context_fade = None;
        let window_height = if self.item_properties.is_some() {
            526.0
        } else {
            390.0
        };
        let (id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(526.0, window_height),
            min_size: Some(iced::Size::new(526.0, window_height)),
            ..iced::window::Settings::default()
        });
        self.item_properties_window_id = Some(id);
        task.discard()
    }

    pub(super) fn close_item_properties(&mut self) -> Task<Message> {
        self.item_properties = None;
        self.midi_item_properties = None;
        self.batch_item_properties = None;
        self.item_properties_window_id
            .take()
            .map_or_else(Task::none, iced::window::close)
    }

    pub(super) fn apply_item_properties(&mut self, close: bool) -> Task<Message> {
        let playback_active = self.playback_active();
        self.apply_item_properties_with_playback(close, playback_active)
    }

    pub(super) fn apply_item_properties_with_playback(
        &mut self,
        close: bool,
        playback_active: bool,
    ) -> Task<Message> {
        if self.batch_item_properties.is_some() {
            return self.apply_batch_item_properties(close);
        }
        if self.midi_item_properties.is_some() {
            return self.apply_midi_item_properties(close);
        }
        if self.item_properties_generation != self.project_generation {
            return self.close_item_properties();
        }
        let Some(draft) = &self.item_properties else {
            return Task::none();
        };
        let Some(item) = self
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == draft.item_id)
        else {
            return self.close_item_properties();
        };
        let id = item.id();
        let current = item.fades();
        let target_name = if draft.name_changed {
            if draft.name.chars().count() > 128 {
                self.item_properties.as_mut().expect("open draft").error =
                    Some("Take name must be 128 characters or fewer".to_owned());
                return Task::none();
            }
            draft.name.clone()
        } else {
            item.name().to_owned()
        };
        let current_take_volume = item.take_volume();
        let current_take_pan = item.take_pan();
        let current_take_rate = item.take_playback_rate();
        let target_take_rate = if draft.take_rate_changed {
            match parse_take_rate(&draft.take_rate) {
                Ok(rate) => rate,
                Err(error) => {
                    self.item_properties.as_mut().expect("open draft").error = Some(error);
                    return Task::none();
                }
            }
        } else {
            current_take_rate
        };
        let sample_rate = self.project.settings().sample_rate();
        let placement = match draft.placement_in_project(item, &self.project) {
            Ok(placement) => placement,
            Err(error) => {
                self.item_properties.as_mut().expect("open draft").error = Some(error);
                return Task::none();
            }
        };
        let changes_placement = placement != ItemPlacement::from_item(item);
        let current_loop_source = item.is_loop_source();
        let target_loop_source = draft.loop_source;
        let changes_loop_source =
            draft.loop_source_changed && target_loop_source != current_loop_source;
        let current_muted = item.is_muted();
        let target_muted = draft.muted;
        let changes_mute = draft.mute_changed && target_muted != current_muted;
        let current_phase = item.is_phase_inverted();
        let target_phase = draft.phase_inverted;
        let changes_phase = draft.phase_changed && target_phase != current_phase;
        let changes_take_rate = target_take_rate != current_take_rate;
        if (changes_placement || changes_loop_source || changes_take_rate) && playback_active {
            let error =
                "Close audio output before editing item placement, source looping, or Take rate"
                    .to_owned();
            self.status = error.clone();
            self.item_properties.as_mut().expect("open draft").error = Some(error);
            return Task::none();
        }
        let media_ref = item.media_ref().to_owned();
        let fades = match draft.fades(current, sample_rate, placement.length) {
            Ok(fades) => fades,
            Err(error) => {
                self.item_properties.as_mut().expect("open draft").error = Some(error);
                return Task::none();
            }
        };
        let take_volume = if draft.changed[9] {
            match parse_take_volume_db(&draft.fields[9]) {
                Ok(volume) => volume,
                Err(error) => {
                    self.item_properties.as_mut().expect("open draft").error = Some(error);
                    return Task::none();
                }
            }
        } else {
            current_take_volume
        };
        let take_pan = if draft.changed[10] {
            match parse_take_pan(&draft.fields[10]) {
                Ok(pan) => pan,
                Err(error) => {
                    self.item_properties.as_mut().expect("open draft").error = Some(error);
                    return Task::none();
                }
            }
        } else {
            current_take_pan
        };
        let mut actions = Vec::new();
        if target_name != item.name() {
            actions.push(aaadaw_core::DawAction::SetAudioItemName {
                item_id: id,
                name: target_name.clone(),
            });
        }
        if changes_placement {
            actions.push(aaadaw_core::DawAction::EditAudioItem {
                item_id: id,
                media_ref,
                start_sample: placement.start,
                source_offset_samples: placement.source_offset,
                length_samples: placement.length,
            });
        }
        if changes_loop_source {
            actions.push(aaadaw_core::DawAction::SetAudioItemLoopSource {
                item_id: id,
                enabled: target_loop_source,
            });
        }
        if changes_mute {
            actions.push(aaadaw_core::DawAction::SetAudioItemMute {
                item_id: id,
                muted: target_muted,
            });
        }
        if changes_phase {
            actions.push(aaadaw_core::DawAction::SetAudioItemPhase {
                item_id: id,
                inverted: target_phase,
            });
        }
        if fades != current {
            actions.push(aaadaw_core::DawAction::SetAudioItemFades { item_id: id, fades });
        }
        if take_volume != current_take_volume || take_pan != current_take_pan {
            actions.push(aaadaw_core::DawAction::SetAudioItemTakeMix {
                item_id: id,
                volume: take_volume,
                pan: take_pan,
            });
        }
        if changes_take_rate {
            actions.push(aaadaw_core::DawAction::SetAudioItemTakePlaybackRate {
                item_id: id,
                rate: target_take_rate,
            });
        }
        if !actions.is_empty() {
            let action = if actions.len() == 1 {
                actions.pop().expect("one action")
            } else {
                aaadaw_core::DawAction::BatchTransaction {
                    tx_id: self.revision,
                    actions,
                }
            };
            self.apply_action(action, "Item properties applied");
            if !self.project.audio_items().iter().any(|item| {
                item.id() == id
                    && item.name() == target_name
                    && item.fades() == fades
                    && item.take_volume() == take_volume
                    && item.take_pan() == take_pan
                    && item.take_playback_rate() == target_take_rate
                    && ItemPlacement::from_item(item) == placement
                    && item.is_loop_source()
                        == if changes_loop_source {
                            target_loop_source
                        } else {
                            current_loop_source
                        }
                    && item.is_muted()
                        == if changes_mute {
                            target_muted
                        } else {
                            current_muted
                        }
                    && item.is_phase_inverted()
                        == if changes_phase {
                            target_phase
                        } else {
                            current_phase
                        }
            }) {
                self.item_properties.as_mut().expect("open draft").error =
                    Some(self.status.clone());
                return Task::none();
            }
        }
        if close {
            self.close_item_properties()
        } else {
            let item = self
                .project
                .audio_items()
                .iter()
                .find(|item| item.id() == id)
                .expect("applied item");
            self.item_properties = Some(ItemProperties::from_project_item(
                item,
                &self.project,
                self.item_properties_unit,
            ));
            Task::none()
        }
    }

    fn apply_batch_item_properties(&mut self, close: bool) -> Task<Message> {
        let playback_active = self.playback_active();
        self.apply_batch_item_properties_with_playback(close, playback_active)
    }

    pub(super) fn apply_batch_item_properties_with_playback(
        &mut self,
        close: bool,
        playback_active: bool,
    ) -> Task<Message> {
        if self.item_properties_generation != self.project_generation {
            return self.close_item_properties();
        }
        let Some(draft) = &self.batch_item_properties else {
            return Task::none();
        };
        let Some(target_muted) = draft.muted else {
            return if close {
                self.close_item_properties()
            } else {
                Task::none()
            };
        };
        if !draft.mute_changed {
            return if close {
                self.close_item_properties()
            } else {
                Task::none()
            };
        }
        let item_ids = draft.item_ids.clone();
        let mut actions = Vec::new();
        for item_id in &item_ids {
            if let Some(item) = self
                .project
                .audio_items()
                .iter()
                .find(|item| item.id() == *item_id)
            {
                if item.is_muted() != target_muted {
                    actions.push(aaadaw_core::DawAction::SetAudioItemMute {
                        item_id: *item_id,
                        muted: target_muted,
                    });
                }
            } else if let Some(item) = self
                .project
                .midi_items()
                .iter()
                .find(|item| item.id() == *item_id)
            {
                if item.is_muted() != target_muted {
                    actions.push(aaadaw_core::DawAction::SetMidiItemMute {
                        item_id: *item_id,
                        muted: target_muted,
                    });
                }
            } else {
                return self.close_item_properties();
            }
        }
        if playback_active
            && actions
                .iter()
                .any(|action| matches!(action, aaadaw_core::DawAction::SetMidiItemMute { .. }))
        {
            let error = "Stop playback before changing MIDI item mute".to_owned();
            self.status = error.clone();
            self.batch_item_properties
                .as_mut()
                .expect("open batch item draft")
                .error = Some(error);
            return Task::none();
        }
        if !actions.is_empty() {
            let action = if actions.len() == 1 {
                actions.pop().expect("one changed selected item")
            } else {
                aaadaw_core::DawAction::BatchTransaction {
                    tx_id: self.revision,
                    actions,
                }
            };
            self.apply_action(action, "Item properties applied");
            let all_match = item_ids.iter().all(|item_id| {
                self.project
                    .audio_items()
                    .iter()
                    .find(|item| item.id() == *item_id)
                    .is_some_and(|item| item.is_muted() == target_muted)
                    || self
                        .project
                        .midi_items()
                        .iter()
                        .find(|item| item.id() == *item_id)
                        .is_some_and(|item| item.is_muted() == target_muted)
            });
            if !all_match {
                self.batch_item_properties
                    .as_mut()
                    .expect("open batch item draft")
                    .error = Some(self.status.clone());
                return Task::none();
            }
        }
        if close {
            self.close_item_properties()
        } else {
            self.batch_item_properties =
                BatchItemProperties::from_project(&item_ids, &self.project);
            Task::none()
        }
    }

    fn apply_midi_item_properties(&mut self, close: bool) -> Task<Message> {
        if self.item_properties_generation != self.project_generation {
            return self.close_item_properties();
        }
        let Some(draft) = &self.midi_item_properties else {
            return Task::none();
        };
        let Some(item) = self
            .project
            .midi_items()
            .iter()
            .find(|item| item.id() == draft.item_id)
        else {
            return self.close_item_properties();
        };
        let item_id = item.id();
        let current_muted = item.is_muted();
        let current_loop_source = item.is_loop_source();
        let current_source_loop_length = item.source_loop_length_ticks();
        let current_start = item.start_position();
        let current_length = item.length_ticks();
        let current_source_offset = item.source_offset_ticks();
        let current_name = item.name().to_owned();
        let (start_position, length_ticks, source_offset_ticks) =
            match draft.placement_in_project(item, &self.project) {
                Ok(placement) => placement,
                Err(error) => {
                    self.midi_item_properties
                        .as_mut()
                        .expect("open MIDI item draft")
                        .error = Some(error);
                    return Task::none();
                }
            };
        let changes_placement = (draft.changed[ItemPropertyField::Position.index()]
            || draft.changed[ItemPropertyField::Length.index()]
            || draft.changed[ItemPropertyField::SourceOffset.index()])
            && (start_position != current_start
                || length_ticks != current_length
                || source_offset_ticks != current_source_offset);
        let target_muted = draft.muted;
        let changes_mute = draft.mute_changed && target_muted != current_muted;
        let target_loop_source = draft.loop_source;
        let changes_loop_source =
            draft.loop_source_changed && target_loop_source != current_loop_source;
        let target_source_loop_length = match draft.source_loop_length_ticks_for_item(
            item,
            target_loop_source,
            current_source_loop_length,
        ) {
            Ok(length) => length,
            Err(error) => {
                self.midi_item_properties
                    .as_mut()
                    .expect("open MIDI item draft")
                    .error = Some(error);
                return Task::none();
            }
        };
        let changes_source_loop_length = target_source_loop_length != current_source_loop_length;
        let target_name = draft.name.clone();
        let changes_name = draft.name_changed && target_name != current_name;
        if (changes_placement || changes_mute || changes_loop_source || changes_source_loop_length)
            && self.playback_active()
        {
            let error = "Stop playback before changing MIDI item properties".to_owned();
            self.status = error.clone();
            self.midi_item_properties
                .as_mut()
                .expect("open MIDI item draft")
                .error = Some(error);
            return Task::none();
        }
        let mut actions = Vec::with_capacity(4);
        if changes_placement {
            actions.push(aaadaw_core::DawAction::EditMidiItemPlacement {
                item_id,
                start_tick: start_position.tick(),
                length_ticks,
                source_offset_ticks,
            });
            if start_position != current_start {
                actions.push(aaadaw_core::DawAction::SetMidiItemPosition {
                    item_id,
                    position: start_position,
                });
            }
        }
        if changes_mute {
            actions.push(aaadaw_core::DawAction::SetMidiItemMute {
                item_id,
                muted: target_muted,
            });
        }
        if changes_loop_source {
            actions.push(aaadaw_core::DawAction::SetMidiItemLoopSource {
                item_id,
                enabled: target_loop_source,
            });
        }
        if changes_source_loop_length {
            actions.push(aaadaw_core::DawAction::SetMidiItemContents {
                item_id,
                source_offset_ticks,
                source_loop_length_ticks: target_source_loop_length,
                content_shift_ticks: 0,
            });
        }
        if changes_name {
            actions.push(aaadaw_core::DawAction::SetMidiItemName {
                item_id,
                name: target_name.clone(),
            });
        }
        if !actions.is_empty() {
            let action = if actions.len() == 1 {
                actions.pop().expect("one MIDI property action")
            } else {
                aaadaw_core::DawAction::BatchTransaction {
                    tx_id: self.revision,
                    actions,
                }
            };
            self.apply_action(action, "MIDI item properties applied");
            if !self.project.midi_items().iter().any(|item| {
                item.id() == item_id
                    && item.start_position() == start_position
                    && item.length_ticks() == length_ticks
                    && item.source_offset_ticks() == source_offset_ticks
                    && item.source_loop_length_ticks() == target_source_loop_length
                    && item.is_muted() == target_muted
                    && item.is_loop_source() == target_loop_source
                    && item.name() == target_name
            }) {
                self.midi_item_properties
                    .as_mut()
                    .expect("open MIDI item draft")
                    .error = Some(self.status.clone());
                return Task::none();
            }
        }
        if close {
            self.close_item_properties()
        } else {
            let item = self
                .project
                .midi_items()
                .iter()
                .find(|item| item.id() == item_id)
                .expect("MIDI item is still present");
            self.midi_item_properties = Some(MidiItemProperties::from_item(
                item,
                &self.project,
                self.item_properties_unit,
            ));
            Task::none()
        }
    }

    pub(super) fn toggle_selected_items_mute(&mut self) {
        let selected = &self.timeline.selected_items;
        if selected.is_empty()
            || selected.iter().any(|item_id| {
                !self
                    .project
                    .audio_items()
                    .iter()
                    .any(|item| item.id() == *item_id)
                    && !self
                        .project
                        .midi_items()
                        .iter()
                        .any(|item| item.id() == *item_id)
            })
            || (self.playback_active()
                && selected.iter().any(|item_id| {
                    self.project
                        .midi_items()
                        .iter()
                        .any(|item| item.id() == *item_id)
                }))
        {
            self.status = "Select idle media items to toggle item mute".to_owned();
            return;
        }
        let mut actions = self
            .project
            .audio_items()
            .iter()
            .filter(|item| selected.contains(&item.id()))
            .map(|item| {
                (
                    item.id(),
                    aaadaw_core::DawAction::SetAudioItemMute {
                        item_id: item.id(),
                        muted: !item.is_muted(),
                    },
                )
            })
            .chain(
                self.project
                    .midi_items()
                    .iter()
                    .filter(|item| selected.contains(&item.id()))
                    .map(|item| {
                        (
                            item.id(),
                            aaadaw_core::DawAction::SetMidiItemMute {
                                item_id: item.id(),
                                muted: !item.is_muted(),
                            },
                        )
                    }),
            )
            .collect::<Vec<_>>();
        actions.sort_unstable_by_key(|(item_id, _)| item_id.value());
        let actions = actions
            .into_iter()
            .map(|(_, action)| action)
            .collect::<Vec<_>>();
        let action = if actions.len() == 1 {
            actions.into_iter().next().expect("one selected item")
        } else {
            aaadaw_core::DawAction::BatchTransaction {
                tx_id: self.revision,
                actions,
            }
        };
        self.apply_action(action, "Audio item mute toggled");
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ItemPropertyField {
    InLength,
    InCurvature,
    InS,
    OutLength,
    OutCurvature,
    OutS,
    Position,
    Length,
    SourceOffset,
    TakeVolume,
    TakePan,
}

#[derive(Debug, Clone)]
pub(crate) struct BatchItemProperties {
    pub(crate) item_ids: Vec<ItemId>,
    pub(crate) muted: Option<bool>,
    pub(crate) error: Option<String>,
    initial_muted: Option<bool>,
    mute_changed: bool,
}

impl BatchItemProperties {
    fn from_project(item_ids: &[ItemId], project: &Project) -> Option<Self> {
        if item_ids.len() < 2 {
            return None;
        }
        let states = item_ids
            .iter()
            .map(|item_id| {
                project
                    .audio_items()
                    .iter()
                    .find(|item| item.id() == *item_id)
                    .map(|item| item.is_muted())
                    .or_else(|| {
                        project
                            .midi_items()
                            .iter()
                            .find(|item| item.id() == *item_id)
                            .map(|item| item.is_muted())
                    })
            })
            .collect::<Option<Vec<_>>>()?;
        let first_muted = *states.first()?;
        let muted = states
            .iter()
            .all(|muted| *muted == first_muted)
            .then_some(first_muted);
        Some(Self {
            item_ids: item_ids.to_vec(),
            muted,
            error: None,
            initial_muted: muted,
            mute_changed: false,
        })
    }

    pub(crate) fn set_muted(&mut self, muted: bool) {
        self.mute_changed = self.initial_muted != Some(muted);
        self.muted = Some(muted);
        self.error = None;
    }

    pub(crate) fn has_changes(&self) -> bool {
        self.mute_changed
    }
}

impl ItemPropertyField {
    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ItemProperties {
    pub(crate) item_id: ItemId,
    pub(crate) name: String,
    name_changed: bool,
    pub(crate) take_rate: String,
    take_rate_changed: bool,
    pub(crate) fields: [String; 11],
    pub(crate) loop_source: bool,
    pub(crate) muted: bool,
    pub(crate) phase_inverted: bool,
    loop_source_changed: bool,
    mute_changed: bool,
    phase_changed: bool,
    changed: [bool; 11],
    pub(crate) unit: TimeUnit,
    frame_rate: aaadaw_core::FrameRate,
    pub(crate) error: Option<String>,
}

impl ItemProperties {
    fn from_project_item(item: &AudioItem, project: &aaadaw_core::Project, unit: TimeUnit) -> Self {
        let mut draft = Self::from_item(item, project.settings().sample_rate(), unit);
        draft.frame_rate = project.settings().frame_rate();
        if unit == TimeUnit::Time {
            match project.project_time_at_sample(item.start_sample()) {
                Ok(seconds) => draft.fields[6] = format_project_time(seconds),
                Err(error) => {
                    draft.fields[6] = "Unavailable".into();
                    draft.error = Some(error.to_string());
                }
            }
        }
        if unit == TimeUnit::Beats {
            let result =
                beats::format_position(project, item.start_sample() as f64).and_then(|position| {
                    beats::format_length(
                        project,
                        item.start_sample() as f64,
                        item.length_samples() as f64,
                    )
                    .map(|length| (position, length))
                });
            match result {
                Ok((position, length)) => {
                    draft.fields[6] = position;
                    draft.fields[7] = length;
                }
                Err(error) => {
                    draft.fields[6] = "Unavailable".into();
                    draft.fields[7] = "Unavailable".into();
                    draft.error = Some(error);
                }
            }
        }
        if unit == TimeUnit::Hmsf {
            let rate = project.settings().frame_rate();
            let sr = f64::from(project.settings().sample_rate());
            match project
                .project_time_at_sample(item.start_sample())
                .and_then(|seconds| format_project_timecode(rate, seconds))
                .and_then(|position| {
                    rate.format_timecode(item.length_samples() as f64 / sr)
                        .map(|length| (position, length))
                }) {
                Ok((position, length)) => {
                    draft.fields[6] = position;
                    draft.fields[7] = length;
                }
                Err(error) => {
                    draft.fields[6] = "Unavailable".into();
                    draft.fields[7] = "Unavailable".into();
                    draft.error = Some(error.to_string());
                }
            }
        }
        draft
    }

    fn placement_in_project(
        &self,
        item: &AudioItem,
        project: &aaadaw_core::Project,
    ) -> Result<ItemPlacement, String> {
        if self.unit == TimeUnit::Time {
            let sample_rate = project.settings().sample_rate();
            let mut converted = self.clone();
            converted.unit = TimeUnit::Samples;
            if self.changed[6] {
                let displayed_seconds =
                    parse_project_time(&self.fields[6]).map_err(str::to_owned)?;
                let sample = project
                    .sample_at_project_time(displayed_seconds)
                    .map_err(|error| error.to_string())?;
                converted.fields[6] = sample.to_string();
            }
            for index in [7, 8] {
                if self.changed[index] {
                    let seconds = parse_time(&self.fields[index])?;
                    converted.fields[index] = (seconds * f64::from(sample_rate)).to_string();
                }
            }
            return converted.placement(item, sample_rate);
        }
        if !matches!(self.unit, TimeUnit::Beats | TimeUnit::Hmsf) {
            return self.placement(item, project.settings().sample_rate());
        }
        let mut converted = self.clone();
        converted.unit = TimeUnit::Samples;
        if self.unit == TimeUnit::Hmsf
            && self.has_changes()
            && self.frame_rate != project.settings().frame_rate()
        {
            // Native retains old text while Project Settings changes, then
            // reparses frame fields on a subsequent effective Apply.
            converted.changed[6] = true;
            converted.changed[7] = true;
        }
        let offset = project.settings().project_start_time_offset_seconds();
        let parse_frame_position = |text: &str| {
            parse_project_timecode(project.settings().frame_rate(), text)
                .map(|seconds| (seconds - offset) * f64::from(project.settings().sample_rate()))
                .map_err(|error| error.to_string())
        };
        let parse_frame_length = |text: &str| {
            project
                .settings()
                .frame_rate()
                .parse_timecode(text)
                .map(|seconds| seconds * f64::from(project.settings().sample_rate()))
                .map_err(|error| error.to_string())
        };
        let start = if converted.changed[6] {
            if self.unit == TimeUnit::Hmsf {
                parse_frame_position(&self.fields[6])?
            } else {
                beats::parse_position(project, &self.fields[6])?
            }
        } else {
            item.start_sample() as f64
        };
        if converted.changed[6] {
            converted.fields[6] = start.to_string();
        }
        if converted.changed[7] {
            converted.fields[7] = if self.unit == TimeUnit::Hmsf {
                parse_frame_length(&self.fields[7])?
            } else {
                beats::parse_length(project, &self.fields[7], start)?
            }
            .to_string();
        }
        if self.changed[8] {
            converted.fields[8] = (parse_time(&self.fields[8])?
                * f64::from(project.settings().sample_rate()))
            .to_string();
        }
        converted.placement(item, project.settings().sample_rate())
    }

    pub(crate) fn from_item(item: &AudioItem, sample_rate: u32, unit: TimeUnit) -> Self {
        let mut draft = Self::new(
            item.id(),
            item.fades(),
            item.take_volume(),
            item.take_pan(),
            sample_rate,
        );
        draft.unit = unit;
        draft.name = item.name().to_owned();
        draft.take_rate = format_take_rate(item.take_playback_rate());
        for (index, samples) in [item.start_sample(), item.length_samples()]
            .into_iter()
            .enumerate()
        {
            draft.fields[index + 6] = format_samples(samples, sample_rate, unit);
        }
        draft.fields[8] = format_signed_samples(item.source_offset_samples(), sample_rate, unit);
        draft.loop_source = item.is_loop_source();
        draft.muted = item.is_muted();
        draft.phase_inverted = item.is_phase_inverted();
        draft
    }

    #[cfg(test)]
    fn set_unit(&mut self, unit: TimeUnit, item: &AudioItem, sample_rate: u32) {
        if self.unit != unit {
            // REAPER refreshes every property from the model on a unit change,
            // discarding all unapplied text, including unrelated fade fields.
            *self = Self::from_item(item, sample_rate, unit);
        }
    }

    pub(crate) fn has_changes(&self) -> bool {
        self.name_changed
            || self.take_rate_changed
            || self.loop_source_changed
            || self.mute_changed
            || self.phase_changed
            || self.changed.iter().any(|changed| *changed)
    }

    fn placement(&self, item: &AudioItem, sample_rate: u32) -> Result<ItemPlacement, String> {
        let mut start = item.start_sample();
        let mut length = item.length_samples();
        let mut source_offset = item.source_offset_samples();
        for (index, field) in self.fields[6..9].iter().enumerate() {
            if !self.changed[index + 6] {
                continue;
            }
            let parsed = match self.unit {
                TimeUnit::Time | TimeUnit::Beats | TimeUnit::Hmsf if index == 2 => {
                    parse_signed_time(field)? * f64::from(sample_rate)
                }
                TimeUnit::Time | TimeUnit::Beats | TimeUnit::Hmsf => {
                    parse_time(field)? * f64::from(sample_rate)
                }
                TimeUnit::Samples => field
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| "Invalid samples".to_owned())?,
            };
            let max_exact_sample = ((1_u64 << 53) - 1) as f64;
            if !parsed.is_finite()
                || parsed > max_exact_sample
                || (index != 2 && parsed < 0.0)
                || (index == 2 && parsed < -max_exact_sample)
            {
                return Err("Time exceeds the supported sample range".to_owned());
            }
            match index {
                0 => start = parsed.round() as u64,
                1 => length = parsed.round() as u64,
                2 => source_offset = parsed.round() as i64,
                _ => unreachable!("placement only contains start, length, and source offset"),
            }
        }
        if length == 0 {
            return Err("Item length must be at least one sample".to_owned());
        }
        if start.checked_add(length).is_none() {
            return Err("Invalid item position".to_owned());
        }
        Ok(ItemPlacement {
            start,
            length,
            source_offset,
        })
    }

    pub(crate) fn new(
        item_id: ItemId,
        fades: AudioItemFades,
        take_volume: f32,
        take_pan: f32,
        sample_rate: u32,
    ) -> Self {
        let input = fades.fade_in.curve().parameters();
        let output = fades.fade_out.curve().parameters();
        Self {
            item_id,
            name: String::new(),
            name_changed: false,
            take_rate: format_take_rate(1.0),
            take_rate_changed: false,
            fields: [
                format_time(fades.fade_in.length_samples() / f64::from(sample_rate)),
                format!("{:.2}", input.curvature()),
                format!("{:.2}", input.s_parameter()),
                format_time(fades.fade_out.length_samples() / f64::from(sample_rate)),
                format!("{:.2}", output.curvature()),
                format!("{:.2}", output.s_parameter()),
                "0:00.000".to_owned(),
                "0:00.000".to_owned(),
                "0:00.000".to_owned(),
                format_take_volume_db(take_volume),
                format_take_pan(take_pan),
            ],
            loop_source: false,
            muted: false,
            phase_inverted: false,
            loop_source_changed: false,
            mute_changed: false,
            phase_changed: false,
            changed: [false; 11],
            unit: TimeUnit::Time,
            frame_rate: aaadaw_core::FrameRate::default(),
            error: None,
        }
    }

    pub(crate) fn edit(&mut self, field: ItemPropertyField, text: String) {
        self.fields[field.index()] = text;
        self.changed[field.index()] = true;
        self.error = None;
    }

    pub(crate) fn set_name(&mut self, name: String) {
        self.name = name;
        self.name_changed = true;
        self.error = None;
    }

    pub(crate) fn set_take_rate(&mut self, rate: String) {
        self.take_rate = rate;
        self.take_rate_changed = true;
        self.error = None;
    }

    pub(crate) fn set_loop_source(&mut self, enabled: bool) {
        self.loop_source = enabled;
        self.loop_source_changed = true;
        self.error = None;
    }

    pub(crate) fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.mute_changed = true;
        self.error = None;
    }

    pub(crate) fn set_phase_inverted(&mut self, inverted: bool) {
        self.phase_inverted = inverted;
        self.phase_changed = true;
        self.error = None;
    }

    /// Start with current model values: untouched fields preserve full
    /// precision, mode, and changes made elsewhere while the window is open.
    pub(crate) fn fades(
        &self,
        current: AudioItemFades,
        sample_rate: u32,
        item_length: u64,
    ) -> Result<AudioItemFades, String> {
        let mut fades = current;
        for (offset, fade) in [(0, &mut fades.fade_in), (3, &mut fades.fade_out)] {
            let length = if self.changed[offset] {
                parse_time(&self.fields[offset])? * f64::from(sample_rate)
            } else {
                fade.length_samples()
            };
            let curve = if self.changed[offset + 1] || self.changed[offset + 2] {
                let parameters = fade.curve().parameters();
                let curvature = if self.changed[offset + 1] {
                    parse_curve(&self.fields[offset + 1])?
                } else {
                    parameters.curvature()
                };
                let s = if self.changed[offset + 2] {
                    parse_curve(&self.fields[offset + 2])?
                } else {
                    parameters.s_parameter()
                };
                FadeCurve::Native(
                    FadeCurveParameters::new(curvature, s)
                        .map_err(|_| "Invalid curve".to_owned())?,
                )
            } else {
                fade.curve()
            };
            *fade = AudioFade::with_curve(length, curve)
                .map_err(|_| "Invalid fade length".to_owned())?;
        }
        let item_length = item_length as f64;
        // The Properties Apply path handles edited fade-in first, then fade-out.
        // A later edited end wins overlap; the API model still retains requests.
        if self.changed[0] {
            let length = fades.fade_in.length_samples().min(item_length);
            fades.fade_in = AudioFade::with_curve(length, fades.fade_in.curve())
                .map_err(|_| "Invalid fade length".to_owned())?;
            if length + fades.fade_out.length_samples() > item_length {
                fades.fade_out =
                    AudioFade::with_curve(item_length - length, fades.fade_out.curve())
                        .map_err(|_| "Invalid fade length".to_owned())?;
            }
        }
        if self.changed[3] {
            // Recover the entered value: the preceding in-length adjustment
            // must not erase the explicitly edited out-length.
            let length = (parse_time(&self.fields[3])? * f64::from(sample_rate)).min(item_length);
            fades.fade_out = AudioFade::with_curve(length, fades.fade_out.curve())
                .map_err(|_| "Invalid fade length".to_owned())?;
            if length + fades.fade_in.length_samples() > item_length {
                fades.fade_in = AudioFade::with_curve(item_length - length, fades.fade_in.curve())
                    .map_err(|_| "Invalid fade length".to_owned())?;
            }
        }
        Ok(fades)
    }
}

fn format_time(seconds: f64) -> String {
    // Current native formatting truncates milliseconds with a 10 ns boundary
    // allowance; preserve the untouched model value rather than this display.
    let adjusted = seconds + 1e-8;
    let milliseconds = (adjusted * 1000.0).floor();
    if milliseconds < u64::MAX as f64 {
        let milliseconds = milliseconds as u64;
        let whole = milliseconds / 1000;
        if whole < 3600 {
            format!(
                "{}:{:02}.{:03}",
                whole / 60,
                whole % 60,
                milliseconds % 1000
            )
        } else {
            format!(
                "{}:{:02}:{:02}.{:03}",
                whole / 3600,
                whole / 60 % 60,
                whole % 60,
                milliseconds % 1000
            )
        }
    } else {
        // The domain permits finite oversized fade requests. Display them
        // without overflowing a millisecond integer or changing the request.
        format!(
            "{:.0}:{:02.0}:{:02.0}.{:03.0}",
            (adjusted / 3600.0).floor(),
            (adjusted % 3600.0 / 60.0).floor(),
            (adjusted % 60.0).floor(),
            (adjusted.fract() * 1000.0).floor()
        )
    }
}

fn format_samples(samples: u64, sample_rate: u32, unit: TimeUnit) -> String {
    match unit {
        TimeUnit::Time | TimeUnit::Beats | TimeUnit::Hmsf => {
            format_time(samples as f64 / f64::from(sample_rate))
        }
        TimeUnit::Samples => samples.to_string(),
    }
}

fn format_signed_samples(samples: i64, sample_rate: u32, unit: TimeUnit) -> String {
    if samples >= 0 {
        return format_samples(samples as u64, sample_rate, unit);
    }
    let magnitude = samples.unsigned_abs();
    match unit {
        TimeUnit::Time | TimeUnit::Beats | TimeUnit::Hmsf => format!(
            "-{}",
            format_time(magnitude as f64 / f64::from(sample_rate))
        ),
        TimeUnit::Samples => samples.to_string(),
    }
}

fn format_take_volume_db(volume: f32) -> String {
    if volume == 0.0 {
        "-inf dB".to_owned()
    } else {
        format!("{:.2}dB", 20.0 * volume.log10())
    }
}

fn format_take_rate(rate: f64) -> String {
    rate.to_string()
}

fn parse_take_rate(text: &str) -> Result<f64, String> {
    let rate = text
        .trim()
        .trim_end_matches('×')
        .trim()
        .parse::<f64>()
        .map_err(|_| "Enter a positive finite Take rate".to_owned())?;
    if !rate.is_finite() || rate <= 0.0 || rate > 64.0 {
        return Err("Take rate must be greater than 0 and no more than 64".to_owned());
    }
    Ok(rate)
}

fn parse_take_volume_db(text: &str) -> Result<f32, String> {
    let text = text.trim();
    let text = text
        .strip_suffix("dB")
        .or_else(|| text.strip_suffix("db"))
        .unwrap_or(text)
        .trim();
    if matches!(text.to_ascii_lowercase().as_str(), "-inf" | "-infinity") || text == "-∞" {
        return Ok(0.0);
    }
    let decibels = text
        .parse::<f32>()
        .map_err(|_| "Enter take volume in dB or -inf for silence".to_owned())?;
    if !decibels.is_finite() || !(-150.0..=24.0).contains(&decibels) {
        return Err("Take volume must be -inf or between -150 and +24 dB".to_owned());
    }
    let volume = 10.0_f32.powf(decibels / 20.0);
    if !volume.is_finite() || volume > aaadaw_core::MAX_AUDIO_ITEM_TAKE_VOLUME {
        return Err("Take volume is outside the supported range".to_owned());
    }
    Ok(volume)
}

fn format_take_pan(pan: f32) -> String {
    if pan == 0.0 {
        return "center".to_owned();
    }
    let percentage = pan.abs() * 100.0;
    let number = format!("{percentage:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned();
    format!("{number}%{}", if pan < 0.0 { "L" } else { "R" })
}

fn parse_take_pan(text: &str) -> Result<f32, String> {
    let text = text.trim();
    if text.eq_ignore_ascii_case("center") {
        return Ok(0.0);
    }
    if let Some(percent) = text.strip_suffix("%L").or_else(|| text.strip_suffix("%l")) {
        let percent = percent
            .trim()
            .parse::<f32>()
            .map_err(|_| "Enter take pan as center, a signed value, or a percentage".to_owned())?;
        if percent.is_finite() && (0.0..=100.0).contains(&percent) {
            return Ok(-percent / 100.0);
        }
        return Err("Take pan percentage must be between 0%L and 100%L".to_owned());
    }
    if let Some(percent) = text.strip_suffix("%R").or_else(|| text.strip_suffix("%r")) {
        let percent = percent
            .trim()
            .parse::<f32>()
            .map_err(|_| "Enter take pan as center, a signed value, or a percentage".to_owned())?;
        if percent.is_finite() && (0.0..=100.0).contains(&percent) {
            return Ok(percent / 100.0);
        }
        return Err("Take pan percentage must be between 0%R and 100%R".to_owned());
    }
    let pan = text
        .parse::<f32>()
        .map_err(|_| "Enter take pan as center, a signed value, or a percentage".to_owned())?;
    if pan.is_finite() && (-1.0..=1.0).contains(&pan) {
        Ok(pan)
    } else {
        Err("Take pan must be between -1 and +1".to_owned())
    }
}

fn parse_time(text: &str) -> Result<f64, String> {
    let fields = text.trim().split(':').collect::<Vec<_>>();
    if fields.is_empty() || fields.len() > 3 {
        return Err("Invalid time".to_owned());
    }
    let mut seconds = 0.0;
    for field in fields {
        let value = field
            .parse::<f64>()
            .map_err(|_| "Invalid time".to_owned())?;
        if !value.is_finite() || value < 0.0 {
            return Err("Invalid time".to_owned());
        }
        seconds = seconds * 60.0 + value;
    }
    if seconds.is_finite() {
        Ok(seconds)
    } else {
        Err("Invalid time".to_owned())
    }
}

fn parse_signed_time(text: &str) -> Result<f64, String> {
    let text = text.trim();
    if let Some(magnitude) = text.strip_prefix('-') {
        Ok(-parse_time(magnitude)?)
    } else {
        parse_time(text)
    }
}

fn parse_curve(text: &str) -> Result<f64, String> {
    let value = text
        .trim()
        .parse::<f64>()
        .map_err(|_| "Invalid curve".to_owned())?;
    if value.is_finite() {
        Ok(value.clamp(-1.0, 1.0))
    } else {
        Err("Invalid curve".to_owned())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MidiItemProperties {
    pub(crate) item_id: ItemId,
    pub(crate) name: String,
    pub(crate) fields: [String; 11],
    changed: [bool; 11],
    pub(crate) unit: TimeUnit,
    frame_rate: aaadaw_core::FrameRate,
    pub(crate) muted: bool,
    pub(crate) loop_source: bool,
    pub(crate) source_loop_length: String,
    pub(crate) error: Option<String>,
    mute_changed: bool,
    loop_source_changed: bool,
    source_loop_length_changed: bool,
    name_changed: bool,
}

impl MidiItemProperties {
    fn from_item(item: &MidiItem, project: &Project, unit: TimeUnit) -> Self {
        let mut draft = Self {
            item_id: item.id(),
            name: item.name().to_owned(),
            fields: std::array::from_fn(|_| String::new()),
            changed: [false; 11],
            unit,
            frame_rate: project.settings().frame_rate(),
            muted: item.is_muted(),
            loop_source: item.is_loop_source(),
            source_loop_length: item.source_loop_length_ticks().to_string(),
            error: None,
            mute_changed: false,
            loop_source_changed: false,
            source_loop_length_changed: false,
            name_changed: false,
        };
        let start_position = item.start_position();
        let end_position = start_position
            .checked_add_ticks(item.length_ticks())
            .expect("validated MIDI item end");
        let start_tick = start_position.as_ticks_f64();
        let end_tick = end_position.as_ticks_f64();
        let formatted = project
            .sample_at_tick_position(start_tick)
            .and_then(|start_sample| {
                project
                    .sample_at_tick_position(end_tick)
                    .map(|end_sample| (start_sample, end_sample - start_sample))
            })
            .map_err(|error| error.to_string())
            .and_then(|(start_sample, length_samples)| {
                project
                    .sample_at_tick_position(item.source_offset_ticks() as f64)
                    .map(|source_sample| (start_sample, length_samples, source_sample))
                    .map_err(|error| error.to_string())
            })
            .and_then(|(start_sample, length_samples, source_sample)| {
                let sample_rate = f64::from(project.settings().sample_rate());
                match unit {
                    TimeUnit::Samples => Ok((
                        format!("{:.0}", start_sample),
                        format!("{:.0}", length_samples),
                        format!("{:.0}", source_sample),
                    )),
                    TimeUnit::Time => {
                        let offset = project.settings().project_start_time_offset_seconds();
                        Ok((
                            format_project_time(offset + start_sample / sample_rate),
                            format_time(length_samples / sample_rate),
                            format_time(source_sample / sample_rate),
                        ))
                    }
                    TimeUnit::Beats => {
                        beats::format_position(project, start_sample).and_then(|position| {
                            beats::format_length(project, start_sample, length_samples).map(
                                |length| {
                                    (position, length, format_time(source_sample / sample_rate))
                                },
                            )
                        })
                    }
                    TimeUnit::Hmsf => {
                        let offset = project.settings().project_start_time_offset_seconds();
                        let position = format_project_timecode(
                            draft.frame_rate,
                            offset + start_sample / sample_rate,
                        )
                        .map_err(|error| error.to_string())?;
                        let length = draft
                            .frame_rate
                            .format_timecode(length_samples / sample_rate)
                            .map_err(|error| error.to_string())?;
                        Ok((position, length, format_time(source_sample / sample_rate)))
                    }
                }
            });
        match formatted {
            Ok((position, length, source_offset)) => {
                draft.fields[ItemPropertyField::Position.index()] = position;
                draft.fields[ItemPropertyField::Length.index()] = length;
                draft.fields[ItemPropertyField::SourceOffset.index()] = source_offset;
            }
            Err(error) => {
                draft.fields[ItemPropertyField::Position.index()] = "Unavailable".to_owned();
                draft.fields[ItemPropertyField::Length.index()] = "Unavailable".to_owned();
                draft.fields[ItemPropertyField::SourceOffset.index()] = "Unavailable".to_owned();
                draft.error = Some(error);
            }
        }
        draft
    }

    pub(crate) fn edit(&mut self, field: ItemPropertyField, text: String) {
        if matches!(
            field,
            ItemPropertyField::Position
                | ItemPropertyField::Length
                | ItemPropertyField::SourceOffset
        ) {
            self.fields[field.index()] = text;
            self.changed[field.index()] = true;
            self.error = None;
        }
    }

    pub(crate) fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.mute_changed = true;
        self.error = None;
    }

    pub(crate) fn set_loop_source(&mut self, enabled: bool) {
        self.loop_source = enabled;
        self.loop_source_changed = true;
        self.error = None;
    }

    pub(crate) fn set_source_loop_length(&mut self, value: String) {
        self.source_loop_length = value;
        self.source_loop_length_changed = true;
        self.error = None;
    }

    pub(crate) fn set_name(&mut self, name: String) {
        self.name = name;
        self.name_changed = true;
        self.error = None;
    }

    pub(crate) fn has_changes(&self) -> bool {
        self.mute_changed
            || self.loop_source_changed
            || self.source_loop_length_changed
            || self.name_changed
            || self.changed.iter().any(|changed| *changed)
    }

    fn source_loop_length_ticks_for_item(
        &self,
        item: &MidiItem,
        loop_source: bool,
        current_length: u64,
    ) -> Result<u64, String> {
        if !self.source_loop_length_changed {
            return Ok(current_length);
        }
        let length = self
            .source_loop_length
            .trim()
            .parse::<u64>()
            .map_err(|_| "Source length must be a positive PPQ tick count".to_owned())?;
        if length == 0 || length > (1_u64 << 53) - 1 {
            return Err("Source length must be within the supported PPQ range".to_owned());
        }
        if loop_source {
            let event_end = item
                .notes()
                .iter()
                .map(|note| note.tick().saturating_add(note.duration()))
                .chain(
                    item.controllers()
                        .iter()
                        .map(|event| event.tick.saturating_add(1)),
                )
                .chain(
                    item.pitch_bends()
                        .iter()
                        .map(|event| event.tick.saturating_add(1)),
                )
                .max()
                .unwrap_or(0);
            if length < event_end {
                return Err("Source length cannot end before stored MIDI events".to_owned());
            }
        }
        Ok(length)
    }

    fn placement_in_project(
        &self,
        item: &MidiItem,
        project: &Project,
    ) -> Result<(MidiItemPosition, u64, u64), String> {
        let position_changed = self.changed[ItemPropertyField::Position.index()];
        let length_changed = self.changed[ItemPropertyField::Length.index()];
        let source_offset_changed = self.changed[ItemPropertyField::SourceOffset.index()];
        if !position_changed && !length_changed && !source_offset_changed {
            return Ok((
                item.start_position(),
                item.length_ticks(),
                item.source_offset_ticks(),
            ));
        }

        let current_start_sample = project
            .sample_at_tick_position(item.start_position().as_ticks_f64())
            .map_err(|error| error.to_string())?;
        let mut start_sample = current_start_sample;
        let mut length_samples = project
            .sample_at_tick_position(
                item.start_position()
                    .checked_add_ticks(item.length_ticks())
                    .expect("validated MIDI item end")
                    .as_ticks_f64(),
            )
            .map_err(|error| error.to_string())?
            - current_start_sample;
        let mut source_offset_ticks = item.source_offset_ticks();
        let frame_rate = project.settings().frame_rate();

        if position_changed {
            start_sample = match self.unit {
                TimeUnit::Time => {
                    let seconds =
                        parse_project_time(&self.fields[ItemPropertyField::Position.index()])
                            .map_err(str::to_owned)?;
                    project
                        .sample_at_project_time(seconds)
                        .map_err(|error| error.to_string())? as f64
                }
                TimeUnit::Beats => beats::parse_position(
                    project,
                    &self.fields[ItemPropertyField::Position.index()],
                )?,
                TimeUnit::Hmsf => {
                    let seconds = parse_project_timecode(
                        frame_rate,
                        &self.fields[ItemPropertyField::Position.index()],
                    )
                    .map_err(|error| error.to_string())?;
                    project
                        .sample_at_project_time(seconds)
                        .map_err(|error| error.to_string())? as f64
                }
                TimeUnit::Samples => parse_nonnegative_position(
                    &self.fields[ItemPropertyField::Position.index()],
                    "Invalid samples",
                )?,
            };
        }
        if length_changed {
            length_samples = match self.unit {
                TimeUnit::Time => {
                    parse_project_time(&self.fields[ItemPropertyField::Length.index()])
                        .map_err(str::to_owned)?
                        * f64::from(project.settings().sample_rate())
                }
                TimeUnit::Beats => beats::parse_length(
                    project,
                    &self.fields[ItemPropertyField::Length.index()],
                    start_sample,
                )?,
                TimeUnit::Hmsf => {
                    frame_rate
                        .parse_timecode(&self.fields[ItemPropertyField::Length.index()])
                        .map_err(|error| error.to_string())?
                        * f64::from(project.settings().sample_rate())
                }
                TimeUnit::Samples => parse_nonnegative_position(
                    &self.fields[ItemPropertyField::Length.index()],
                    "Invalid samples",
                )?,
            };
        }
        if source_offset_changed {
            let source_sample = if self.unit == TimeUnit::Samples {
                parse_nonnegative_position(
                    &self.fields[ItemPropertyField::SourceOffset.index()],
                    "Invalid samples",
                )?
            } else {
                parse_time(&self.fields[ItemPropertyField::SourceOffset.index()])?
                    * f64::from(project.settings().sample_rate())
            };
            source_offset_ticks = sample_to_tick(project, source_sample)?;
        }
        if !start_sample.is_finite()
            || start_sample < 0.0
            || !length_samples.is_finite()
            || length_samples <= 0.0
        {
            return Err("MIDI item position and length must be nonnegative and nonzero".to_owned());
        }
        let start_position = if position_changed {
            sample_to_midi_item_position(project, start_sample)?
        } else {
            item.start_position()
        };
        let length_ticks = if length_changed {
            let end_position =
                sample_to_midi_item_position(project, start_sample + length_samples)?;
            let length = end_position.as_ticks_f64() - start_position.as_ticks_f64();
            if !length.is_finite() || length < 0.5 || length > ((1_u64 << 53) - 1) as f64 {
                return Err("MIDI item length must be at least one project tick".to_owned());
            }
            length.round() as u64
        } else {
            item.length_ticks()
        };
        if length_ticks == 0
            || start_position.checked_add_ticks(length_ticks).is_none()
            || source_offset_ticks.checked_add(length_ticks).is_none()
        {
            return Err("Invalid MIDI item position or length".to_owned());
        }
        Ok((start_position, length_ticks, source_offset_ticks))
    }
}

fn parse_nonnegative_position(text: &str, message: &str) -> Result<f64, String> {
    let value = text.trim().parse::<f64>().map_err(|_| message.to_owned())?;
    if value.is_finite() && value >= 0.0 && value <= ((1_u64 << 53) - 1) as f64 {
        Ok(value)
    } else {
        Err(message.to_owned())
    }
}

fn sample_to_tick(project: &Project, sample: f64) -> Result<u64, String> {
    let tick = project
        .tick_at_sample_position(sample)
        .map_err(|error| error.to_string())?;
    if !tick.is_finite() || tick < 0.0 || tick > ((1_u64 << 53) - 1) as f64 {
        return Err("MIDI item position or length is outside the supported range".to_owned());
    }
    Ok(tick.round() as u64)
}

fn sample_to_midi_item_position(
    project: &Project,
    sample: f64,
) -> Result<MidiItemPosition, String> {
    let tick = project
        .tick_at_sample_position(sample)
        .map_err(|error| error.to_string())?;
    let scaled = tick * f64::from(aaadaw_core::MIDI_ITEM_SUBTICKS_PER_TICK);
    if !scaled.is_finite() || scaled < 0.0 || scaled > ((1_u64 << 53) - 1) as f64 {
        return Err("MIDI item position or length is outside the supported range".to_owned());
    }
    MidiItemPosition::from_total_subticks(scaled.round() as u128)
        .ok_or_else(|| "MIDI item position or length is outside the supported range".to_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ItemPlacement {
    start: u64,
    length: u64,
    source_offset: i64,
}
impl ItemPlacement {
    fn from_item(item: &AudioItem) -> Self {
        Self {
            start: item.start_sample(),
            length: item.length_samples(),
            source_offset: item.source_offset_samples(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aaadaw_core::{DawAction, Project};

    #[test]
    fn take_volume_db_round_trips_unity_attenuation_and_silence() {
        assert_eq!(parse_take_volume_db("0.00dB").unwrap(), 1.0);
        assert!((parse_take_volume_db("-6.02 dB").unwrap() - 0.5).abs() < 0.001);
        assert_eq!(parse_take_volume_db("-inf").unwrap(), 0.0);
        assert_eq!(parse_take_volume_db("-∞").unwrap(), 0.0);
        assert_eq!(format_take_volume_db(1.0), "0.00dB");
        assert_eq!(format_take_volume_db(0.0), "-inf dB");
        assert!((parse_take_volume_db(&format_take_volume_db(0.5)).unwrap() - 0.5).abs() < 0.001);
    }

    #[test]
    fn take_volume_db_rejects_invalid_and_out_of_range_values() {
        for text in ["", "quiet", "NaN", "inf", "-150.01", "24.01"] {
            assert!(parse_take_volume_db(text).is_err(), "accepted {text:?}");
        }
    }

    #[test]
    fn take_rate_parses_a_positive_finite_multiplier_and_rejects_invalid_values() {
        assert_eq!(parse_take_rate("1.25"), Ok(1.25));
        assert_eq!(parse_take_rate("1.25×"), Ok(1.25));
        for text in ["", "fast", "0", "-1", "NaN", "inf", "64.01"] {
            assert!(parse_take_rate(text).is_err(), "accepted {text:?}");
        }
    }

    #[test]
    fn take_pan_formats_and_parses_native_center_and_left_right_percentages() {
        for (text, expected) in [("center", 0.0), ("50%L", -0.5), ("25%R", 0.25)] {
            assert!((parse_take_pan(text).unwrap() - expected).abs() < f32::EPSILON);
            assert_eq!(format_take_pan(expected), text);
        }
        assert!(parse_take_pan("101%R").is_err());
        assert!(parse_take_pan("1.1").is_err());
    }

    #[test]
    fn signed_project_start_offset_affects_item_position_not_duration_or_source_offset() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Owned".into(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertAudioItem {
                track_id: project.tracks()[0].id(),
                media_ref: "asset://owned".into(),
                start_sample: 48_000,
                source_offset_samples: 96_000,
                length_samples: 48_000,
            })
            .unwrap();
        project
            .apply(DawAction::SetProjectStartTimeOffsetSeconds { seconds: -5.0 })
            .unwrap();

        let item = &project.audio_items()[0];
        let mut time = ItemProperties::from_project_item(item, &project, TimeUnit::Time);
        assert_eq!(
            &time.fields[6..],
            &["-0:04.000", "0:01.000", "0:02.000", "0.00dB", "center"]
        );
        time.edit(ItemPropertyField::Position, "0:00.000".into());
        assert_eq!(
            time.placement_in_project(item, &project).unwrap(),
            ItemPlacement {
                start: 240_000,
                length: 48_000,
                source_offset: 96_000,
            }
        );

        let mut frames = ItemProperties::from_project_item(item, &project, TimeUnit::Hmsf);
        assert_eq!(frames.fields[6], "-00:00:04:00");
        assert_eq!(frames.fields[7], "00:00:01:00");
        frames.edit(ItemPropertyField::Position, "-00:00:03:00".into());
        assert_eq!(
            frames.placement_in_project(item, &project).unwrap().start,
            96_000
        );
    }

    #[test]
    fn midi_position_and_length_round_trip_in_each_item_properties_unit() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "MIDI".into(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertMidiItem {
                track_id: project.tracks()[0].id(),
                start_tick: 3_840,
                length_ticks: 1_920,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        project
            .apply(DawAction::EditMidiItemPlacement {
                item_id,
                start_tick: 3_840,
                length_ticks: 1_920,
                source_offset_ticks: 480,
            })
            .unwrap();
        project
            .apply(DawAction::SetProjectStartTimeOffsetSeconds { seconds: -5.0 })
            .unwrap();
        let item = &project.midi_items()[0];

        for unit in [
            TimeUnit::Time,
            TimeUnit::Beats,
            TimeUnit::Hmsf,
            TimeUnit::Samples,
        ] {
            let mut draft = MidiItemProperties::from_item(item, &project, unit);
            let position = draft.fields[ItemPropertyField::Position.index()].clone();
            let length = draft.fields[ItemPropertyField::Length.index()].clone();
            let source_offset = draft.fields[ItemPropertyField::SourceOffset.index()].clone();
            draft.edit(ItemPropertyField::Position, position);
            draft.edit(ItemPropertyField::Length, length);
            draft.edit(ItemPropertyField::SourceOffset, source_offset);
            assert_eq!(
                draft.placement_in_project(item, &project).unwrap(),
                (
                    item.start_position(),
                    item.length_ticks(),
                    item.source_offset_ticks()
                ),
                "round trip failed for {unit}"
            );
        }

        let mut zero_length = MidiItemProperties::from_item(item, &project, TimeUnit::Samples);
        zero_length.edit(ItemPropertyField::Length, "0".to_owned());
        assert!(zero_length.placement_in_project(item, &project).is_err());

        let mut overflowing = MidiItemProperties::from_item(item, &project, TimeUnit::Samples);
        overflowing.edit(ItemPropertyField::Position, "9007199254740992".to_owned());
        assert!(overflowing.placement_in_project(item, &project).is_err());
    }

    #[test]
    fn midi_sample_position_property_round_trips_fractional_ppq_position() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "MIDI".into(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertMidiItem {
                track_id: project.tracks()[0].id(),
                start_tick: 3_840,
                length_ticks: 1_920,
            })
            .unwrap();
        let item_id = project.midi_items()[0].id();
        let position = MidiItemPosition::new(3_840, 160).unwrap();
        project
            .apply(DawAction::SetMidiItemPosition { item_id, position })
            .unwrap();
        let item = &project.midi_items()[0];
        let mut draft = MidiItemProperties::from_item(item, &project, TimeUnit::Samples);
        let displayed_position = draft.fields[ItemPropertyField::Position.index()].clone();
        draft.edit(ItemPropertyField::Position, displayed_position);

        assert_eq!(
            draft.placement_in_project(item, &project).unwrap(),
            (position, item.length_ticks(), item.source_offset_ticks())
        );
    }

    #[test]
    fn switching_units_reloads_model_and_discards_all_drafts_without_losing_applied_precision() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Owned".into(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertAudioItem {
                track_id: project.tracks()[0].id(),
                media_ref: "asset://owned".into(),
                start_sample: 16001,
                source_offset_samples: 23,
                length_samples: 48000,
            })
            .unwrap();
        let item = &project.audio_items()[0];
        let mut draft = ItemProperties::from_item(item, 48000, TimeUnit::Samples);
        assert_eq!(
            &draft.fields[6..],
            &["16001", "48000", "23", "0.00dB", "center"]
        );
        assert!(!draft.has_changes());
        draft.edit(ItemPropertyField::Position, "24000".into());
        draft.edit(ItemPropertyField::InCurvature, "0.25".into());
        assert!(draft.has_changes());
        draft.set_unit(TimeUnit::Time, item, 48000);
        assert_eq!(draft.fields[6], "0:00.333");
        assert_eq!(draft.fields[1], "0.50");
        assert!(!draft.has_changes());
        assert_eq!(
            draft.placement(item, 48000).unwrap(),
            ItemPlacement::from_item(item)
        );
        assert_eq!(
            draft.fades(item.fades(), 48000, 48000).unwrap(),
            item.fades()
        );
        draft.edit(ItemPropertyField::Length, "NaN".into());
        draft.set_unit(TimeUnit::Samples, item, 48000);
        assert_eq!(draft.unit, TimeUnit::Samples);
        assert_eq!(
            &draft.fields[6..],
            &["16001", "48000", "23", "0.00dB", "center"]
        );
        assert!(!draft.has_changes());
        draft.edit(ItemPropertyField::Position, "-0.4".into());
        assert!(draft.placement(item, 48000).is_err());
    }

    #[test]
    fn audio_source_offset_properties_format_and_edit_negative_samples() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Pre-roll".into(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertAudioItem {
                track_id: project.tracks()[0].id(),
                media_ref: "asset://negative-offset".into(),
                start_sample: 48_000,
                source_offset_samples: -12_000,
                length_samples: 48_000,
            })
            .unwrap();
        let item = &project.audio_items()[0];

        let mut samples = ItemProperties::from_item(item, 48_000, TimeUnit::Samples);
        assert_eq!(samples.fields[8], "-12000");
        samples.edit(ItemPropertyField::SourceOffset, "-24000".into());
        assert_eq!(
            samples.placement(item, 48_000).unwrap().source_offset,
            -24_000
        );

        let mut time = ItemProperties::from_item(item, 48_000, TimeUnit::Time);
        assert_eq!(time.fields[8], "-0:00.250");
        time.edit(ItemPropertyField::SourceOffset, "-0:00.500".into());
        assert_eq!(time.placement(item, 48_000).unwrap().source_offset, -24_000);
    }

    #[test]
    fn changed_project_rate_keeps_draft_but_reinterprets_frame_fields_on_effective_apply() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Owned".into(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertAudioItem {
                track_id: project.tracks()[0].id(),
                media_ref: "asset://owned".into(),
                start_sample: 16001,
                source_offset_samples: 6001,
                length_samples: 12001,
            })
            .unwrap();
        let mut draft =
            ItemProperties::from_project_item(&project.audio_items()[0], &project, TimeUnit::Hmsf);
        assert_eq!(
            &draft.fields[6..],
            &["00:00:00:10", "00:00:00:07", "0:00.125", "0.00dB", "center"]
        );
        draft.edit(ItemPropertyField::InCurvature, "0.25".into());
        assert_eq!(
            draft
                .placement_in_project(&project.audio_items()[0], &project)
                .unwrap(),
            ItemPlacement {
                start: 16001,
                length: 12001,
                source_offset: 6001
            }
        );
        draft =
            ItemProperties::from_project_item(&project.audio_items()[0], &project, TimeUnit::Hmsf);
        project
            .apply(DawAction::SetFrameRate {
                rate: aaadaw_core::FrameRate::Fps25,
            })
            .unwrap();
        assert!(!draft.has_changes());
        assert_eq!(
            draft
                .placement_in_project(&project.audio_items()[0], &project)
                .unwrap(),
            ItemPlacement {
                start: 16001,
                length: 12001,
                source_offset: 6001
            }
        );
        draft.edit(ItemPropertyField::InCurvature, "0.75".into());
        assert_eq!(
            draft
                .placement_in_project(&project.audio_items()[0], &project)
                .unwrap(),
            ItemPlacement {
                start: 19200,
                length: 13440,
                source_offset: 6001
            }
        );
        assert_eq!(
            &draft.fields[6..],
            &["00:00:00:10", "00:00:00:07", "0:00.125", "0.00dB", "center"]
        );
    }

    #[test]
    fn frame_properties_use_project_rate_and_preserve_untouched_sample_precision() {
        let mut project = Project::new();
        project
            .apply(DawAction::SetFrameRate {
                rate: aaadaw_core::FrameRate::Fps25,
            })
            .unwrap();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Owned".into(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertAudioItem {
                track_id: project.tracks()[0].id(),
                media_ref: "asset://owned".into(),
                start_sample: 16001,
                source_offset_samples: 6000,
                length_samples: 12000,
            })
            .unwrap();
        let item = &project.audio_items()[0];
        let mut draft = ItemProperties::from_project_item(item, &project, TimeUnit::Hmsf);
        assert_eq!(
            &draft.fields[6..],
            &["00:00:00:08", "00:00:00:06", "0:00.125", "0.00dB", "center"]
        );
        assert_eq!(
            draft.placement_in_project(item, &project).unwrap(),
            ItemPlacement::from_item(item)
        );
        draft.edit(ItemPropertyField::Position, "00:00:01:12".into());
        draft.edit(ItemPropertyField::Length, "00:00:00:02".into());
        assert_eq!(
            draft.placement_in_project(item, &project).unwrap(),
            ItemPlacement {
                start: 71040,
                length: 3840,
                source_offset: 6000
            }
        );
        draft.edit(ItemPropertyField::Length, "00:00:00:25".into());
        assert!(draft.placement_in_project(item, &project).is_err());
        assert_eq!(item.start_sample(), 16001);
    }

    #[test]
    fn beats_properties_keep_untouched_samples_and_anchor_length_to_edited_position() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Owned".into(),
            })
            .unwrap();
        project
            .apply(DawAction::SetTempo {
                start_tick: 3840,
                bpm: 60.0,
            })
            .unwrap();
        project
            .apply(DawAction::InsertAudioItem {
                track_id: project.tracks()[0].id(),
                media_ref: "asset://owned".into(),
                start_sample: 16001,
                source_offset_samples: 6000,
                length_samples: 12000,
            })
            .unwrap();
        let item = &project.audio_items()[0];
        let mut draft = ItemProperties::from_project_item(item, &project, TimeUnit::Beats);
        assert_eq!(
            &draft.fields[6..],
            &["1.1.67", "0.0.50", "0:00.125", "0.00dB", "center"]
        );
        assert_eq!(
            draft.placement_in_project(item, &project).unwrap(),
            ItemPlacement::from_item(item)
        );
        draft.edit(ItemPropertyField::Position, "2.1.00".into());
        draft.edit(ItemPropertyField::Length, "0.0.50".into());
        draft.edit(ItemPropertyField::SourceOffset, "0:00.250".into());
        assert_eq!(
            draft.placement_in_project(item, &project).unwrap(),
            ItemPlacement {
                start: 96000,
                length: 24000,
                source_offset: 12000
            }
        );
        for value in [
            "NaN",
            "invalid",
            "-1.1.00",
            "1.2.50 extra",
            "18446744073709551615.1.00",
        ] {
            draft.edit(ItemPropertyField::Position, value.into());
            assert!(
                draft.placement_in_project(item, &project).is_err(),
                "{value}"
            );
        }
        assert_eq!(item.start_sample(), 16001);
    }

    #[test]
    fn time_strings_match_native_api_boundaries_and_hour_formatting() {
        let reference =
            include_str!("../../../../docs/verification/reaper-parity/time-displays-reference.tsv");
        let mut checked = 0;
        for row in reference.lines().skip(2) {
            let fields = row.split('\t').collect::<Vec<_>>();
            if fields[6] != "0" {
                continue;
            }
            assert_eq!(
                format_time(fields[4].parse().unwrap()),
                fields[7],
                "{} position",
                fields[0]
            );
            assert_eq!(
                format_time(fields[5].parse().unwrap()),
                fields[8],
                "{} length",
                fields[0]
            );
            checked += 1;
        }
        assert_eq!(checked, 27);
    }

    #[test]
    fn property_unit_config_round_trips_and_rejects_future_or_invalid_values_without_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("item-properties.json");
        assert_eq!(load_unit_path(&path).unwrap(), TimeUnit::Time);
        save_unit_path(&path, TimeUnit::Samples).unwrap();
        assert_eq!(load_unit_path(&path).unwrap(), TimeUnit::Samples);
        for text in [
            r#"{"version":2,"time_unit":"samples"}"#,
            r#"{"version":1,"time_unit":"bogus"}"#,
        ] {
            std::fs::write(&path, text).unwrap();
            assert!(load_unit_path(&path).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        }
        save_unit_path(&path, TimeUnit::Time).unwrap();
        assert_eq!(load_unit_path(&path).unwrap(), TimeUnit::Time);
    }
}
