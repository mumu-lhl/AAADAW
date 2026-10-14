mod beats;
use super::{App, Message};
use crate::project_time::{
    format_project_time, format_project_timecode, parse_project_time, parse_project_timecode,
};
use aaadaw_core::{
    AudioFade, AudioItem, AudioItemFades, FadeCurve, FadeCurveParameters, ItemId, Project,
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
        let Some(draft) = &mut self.item_properties else {
            return;
        };
        let Some(item) = self
            .project
            .audio_items()
            .iter()
            .find(|item| item.id() == draft.item_id)
        else {
            return;
        };
        if draft.unit != unit {
            *draft = ItemProperties::from_project_item(item, &self.project, unit);
        }
        self.item_properties_unit = unit;
        let result = super::config_paths::config_file_path("item-properties.json")
            .ok_or_else(|| "No application configuration directory".to_owned())
            .and_then(|path| save_unit_path(&path, unit));
        if let Err(error) = result {
            draft.error = Some(format!("Display unit could not be saved: {error}"));
        }
    }

    pub(super) fn open_item_properties(&mut self) -> Task<Message> {
        if let Some(id) = self.item_properties_window_id {
            return iced::window::gain_focus(id);
        }
        let Some(item) = self.timeline.selected_item.and_then(|id| {
            self.project
                .audio_items()
                .iter()
                .find(|item| item.id() == id)
        }) else {
            return Task::none();
        };
        self.item_properties_generation = self.project_generation;
        self.item_properties = Some(ItemProperties::from_project_item(
            item,
            &self.project,
            self.item_properties_unit,
        ));
        self.timeline.context_item = None;
        self.timeline.context_fade = None;
        let (id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(526.0, 390.0),
            min_size: Some(iced::Size::new(526.0, 390.0)),
            ..iced::window::Settings::default()
        });
        self.item_properties_window_id = Some(id);
        task.discard()
    }

    pub(super) fn close_item_properties(&mut self) -> Task<Message> {
        self.item_properties = None;
        self.item_properties_window_id
            .take()
            .map_or_else(Task::none, iced::window::close)
    }

    pub(super) fn apply_item_properties(&mut self, close: bool) -> Task<Message> {
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
        let current_take_volume = item.take_volume();
        let current_take_pan = item.take_pan();
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
        if (changes_placement || changes_loop_source) && self.playback_active() {
            let error =
                "Close audio output before editing item placement or source looping".to_owned();
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
                    && item.fades() == fades
                    && item.take_volume() == take_volume
                    && item.take_pan() == take_pan
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

impl ItemPropertyField {
    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ItemProperties {
    pub(crate) item_id: ItemId,
    pub(crate) fields: [String; 11],
    pub(crate) loop_source: bool,
    pub(crate) muted: bool,
    loop_source_changed: bool,
    mute_changed: bool,
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
        for (index, samples) in [
            item.start_sample(),
            item.length_samples(),
            item.source_offset_samples(),
        ]
        .into_iter()
        .enumerate()
        {
            draft.fields[index + 6] = format_samples(samples, sample_rate, unit);
        }
        draft.loop_source = item.is_loop_source();
        draft.muted = item.is_muted();
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
        self.loop_source_changed || self.mute_changed || self.changed.iter().any(|changed| *changed)
    }

    fn placement(&self, item: &AudioItem, sample_rate: u32) -> Result<ItemPlacement, String> {
        let values = [
            item.start_sample(),
            item.length_samples(),
            item.source_offset_samples(),
        ];
        let mut samples = values;
        for (index, value) in samples.iter_mut().enumerate() {
            if self.changed[index + 6] {
                let parsed = match self.unit {
                    TimeUnit::Time | TimeUnit::Beats | TimeUnit::Hmsf => {
                        parse_time(&self.fields[index + 6])? * f64::from(sample_rate)
                    }
                    TimeUnit::Samples => self.fields[index + 6]
                        .trim()
                        .parse::<f64>()
                        .map_err(|_| "Invalid samples".to_owned())?,
                };
                // Keep conversions exact; do not saturate overflow to u64::MAX.
                if !parsed.is_finite() || parsed < 0.0 || parsed > ((1_u64 << 53) - 1) as f64 {
                    return Err("Time exceeds the supported sample range".to_owned());
                }
                *value = parsed.round() as u64;
            }
        }
        if samples[1] == 0 {
            return Err("Item length must be at least one sample".to_owned());
        }
        if samples[0].checked_add(samples[1]).is_none() {
            return Err("Invalid item position".to_owned());
        }
        Ok(ItemPlacement {
            start: samples[0],
            length: samples[1],
            source_offset: samples[2],
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
            loop_source_changed: false,
            mute_changed: false,
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

fn format_take_volume_db(volume: f32) -> String {
    if volume == 0.0 {
        "-inf dB".to_owned()
    } else {
        format!("{:.2}dB", 20.0 * volume.log10())
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ItemPlacement {
    start: u64,
    length: u64,
    source_offset: u64,
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
