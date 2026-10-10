mod beats;
use super::{App, Message};
use aaadaw_core::{AudioFade, AudioItem, AudioItemFades, FadeCurve, FadeCurveParameters, ItemId};
use iced::Task;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TimeUnit {
    #[default]
    Time,
    Beats,
    Samples,
}

impl std::fmt::Display for TimeUnit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Time => "Time",
            Self::Beats => "Beats",
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
            size: iced::Size::new(526.0, 350.0),
            min_size: Some(iced::Size::new(526.0, 350.0)),
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
        let sample_rate = self.project.settings().sample_rate();
        let placement = match draft.placement_in_project(item, &self.project) {
            Ok(placement) => placement,
            Err(error) => {
                self.item_properties.as_mut().expect("open draft").error = Some(error);
                return Task::none();
            }
        };
        let changes_placement = placement != ItemPlacement::from_item(item);
        if changes_placement && self.playback_active() {
            let error = "Close audio output before editing item placement".to_owned();
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
        if fades != current {
            actions.push(aaadaw_core::DawAction::SetAudioItemFades { item_id: id, fades });
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
                    && ItemPlacement::from_item(item) == placement
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
}

impl ItemPropertyField {
    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ItemProperties {
    pub(crate) item_id: ItemId,
    pub(crate) fields: [String; 9],
    changed: [bool; 9],
    pub(crate) unit: TimeUnit,
    pub(crate) error: Option<String>,
}

impl ItemProperties {
    fn from_project_item(item: &AudioItem, project: &aaadaw_core::Project, unit: TimeUnit) -> Self {
        let mut draft = Self::from_item(item, project.settings().sample_rate(), unit);
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
        draft
    }

    fn placement_in_project(
        &self,
        item: &AudioItem,
        project: &aaadaw_core::Project,
    ) -> Result<ItemPlacement, String> {
        if self.unit != TimeUnit::Beats {
            return self.placement(item, project.settings().sample_rate());
        }
        let mut converted = self.clone();
        converted.unit = TimeUnit::Samples;
        let start = if self.changed[6] {
            beats::parse_position(project, &self.fields[6])?
        } else {
            item.start_sample() as f64
        };
        if self.changed[6] {
            converted.fields[6] = start.to_string();
        }
        if self.changed[7] {
            converted.fields[7] = beats::parse_length(project, &self.fields[7], start)?.to_string();
        }
        if self.changed[8] {
            converted.fields[8] = (parse_time(&self.fields[8])?
                * f64::from(project.settings().sample_rate()))
            .to_string();
        }
        converted.placement(item, project.settings().sample_rate())
    }

    pub(crate) fn from_item(item: &AudioItem, sample_rate: u32, unit: TimeUnit) -> Self {
        let mut draft = Self::new(item.id(), item.fades(), sample_rate);
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
        self.changed.iter().any(|changed| *changed)
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
                    TimeUnit::Time | TimeUnit::Beats => {
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

    pub(crate) fn new(item_id: ItemId, fades: AudioItemFades, sample_rate: u32) -> Self {
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
            ],
            changed: [false; 9],
            unit: TimeUnit::Time,
            error: None,
        }
    }

    pub(crate) fn edit(&mut self, field: ItemPropertyField, text: String) {
        self.fields[field.index()] = text;
        self.changed[field.index()] = true;
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
        TimeUnit::Time | TimeUnit::Beats => format_time(samples as f64 / f64::from(sample_rate)),
        TimeUnit::Samples => samples.to_string(),
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
        assert_eq!(&draft.fields[6..], &["16001", "48000", "23"]);
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
        assert_eq!(&draft.fields[6..], &["16001", "48000", "23"]);
        assert!(!draft.has_changes());
        draft.edit(ItemPropertyField::Position, "-0.4".into());
        assert!(draft.placement(item, 48000).is_err());
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
        assert_eq!(&draft.fields[6..], &["1.1.67", "0.0.50", "0:00.125"]);
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
