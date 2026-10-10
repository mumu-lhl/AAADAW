use super::{App, Message};
use aaadaw_core::{AudioFade, AudioItem, AudioItemFades, FadeCurve, FadeCurveParameters, ItemId};
use iced::Task;

impl App {
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
        self.item_properties = Some(ItemProperties::from_item(
            item,
            self.project.settings().sample_rate(),
        ));
        self.timeline.context_item = None;
        self.timeline.context_fade = None;
        let (id, task) = iced::window::open(iced::window::Settings {
            size: iced::Size::new(526.0, 310.0),
            min_size: Some(iced::Size::new(526.0, 310.0)),
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
        let placement = match draft.placement(item, sample_rate) {
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
            self.item_properties = Some(ItemProperties::from_item(item, sample_rate));
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
    pub(crate) error: Option<String>,
}

impl ItemProperties {
    pub(crate) fn from_item(item: &AudioItem, sample_rate: u32) -> Self {
        let mut draft = Self::new(item.id(), item.fades(), sample_rate);
        draft.fields[6] = format_time(item.start_sample() as f64 / f64::from(sample_rate));
        draft.fields[7] = format_time(item.length_samples() as f64 / f64::from(sample_rate));
        draft.fields[8] = format_time(item.source_offset_samples() as f64 / f64::from(sample_rate));
        draft
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
                let parsed =
                    (parse_time(&self.fields[index + 6])? * f64::from(sample_rate)).round();
                // Keep conversions exact; do not saturate overflow to u64::MAX.
                if !parsed.is_finite() || parsed > ((1_u64 << 53) - 1) as f64 {
                    return Err("Time exceeds the supported sample range".to_owned());
                }
                *value = parsed as u64;
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
    let rounded = (seconds * 1000.0).round() / 1000.0;
    let minutes = (rounded / 60.0).floor();
    format!("{minutes:.0}:{:06.3}", rounded - minutes * 60.0)
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
