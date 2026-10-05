mod renderer;

use aaadaw_core::{ItemId, Project, TrackId, VolumeAutomationPoint};
use aaadaw_media::AudioWaveform;
use iced::advanced::text::{Alignment as TextAlignment, LineHeight, Shaping};
use iced::widget::canvas;
use iced::widget::canvas::Text;
use iced::widget::pane_grid::{self, Axis, Split};
use iced::widget::shader;
use iced::{
    Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, Theme, keyboard, mouse,
};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(crate) const TIMELINE_ROW_HEIGHT: f32 = 100.0;
pub(crate) const FX_AUTOMATION_LANE_HEIGHT: f32 = 24.0;
pub(crate) const TIMELINE_RULER_HEIGHT: f32 = 32.0;
pub(crate) const TCP_SCROLL_ID: &str = "aaadaw-tcp-scroll";
pub(crate) const TIMELINE_SCROLL_ID: &str = "aaadaw-timeline-scroll";

const MIN_PIXELS_PER_TICK: f32 = 0.002;
const MAX_PIXELS_PER_TICK: f32 = 1.5;
const TIME_SELECTION_EDGE_HIT_RADIUS_PX: f64 = 7.0;
const ITEM_TRIM_EDGE_HIT_RADIUS_PX: f64 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArrangementPane {
    TrackControls,
    Timeline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SnapGrid {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    DottedHalf,
    DottedQuarter,
    DottedEighth,
    DottedSixteenth,
    HalfTriplet,
    QuarterTriplet,
    EighthTriplet,
    SixteenthTriplet,
    ThirtySecondTriplet,
}

impl SnapGrid {
    pub(crate) const ALL: [Self; 15] = [
        Self::Whole,
        Self::Half,
        Self::Quarter,
        Self::Eighth,
        Self::Sixteenth,
        Self::ThirtySecond,
        Self::DottedHalf,
        Self::DottedQuarter,
        Self::DottedEighth,
        Self::DottedSixteenth,
        Self::HalfTriplet,
        Self::QuarterTriplet,
        Self::EighthTriplet,
        Self::SixteenthTriplet,
        Self::ThirtySecondTriplet,
    ];

    fn ratio(self) -> (u64, u64) {
        match self {
            Self::Whole => (4, 1),
            Self::Half => (2, 1),
            Self::Quarter => (1, 1),
            Self::Eighth => (1, 2),
            Self::Sixteenth => (1, 4),
            Self::ThirtySecond => (1, 8),
            Self::DottedHalf => (3, 1),
            Self::DottedQuarter => (3, 2),
            Self::DottedEighth => (3, 4),
            Self::DottedSixteenth => (3, 8),
            Self::HalfTriplet => (4, 3),
            Self::QuarterTriplet => (2, 3),
            Self::EighthTriplet => (1, 3),
            Self::SixteenthTriplet => (1, 6),
            Self::ThirtySecondTriplet => (1, 12),
        }
    }

    fn tick_interval(self, ppq: u32) -> Option<u64> {
        let (numerator, denominator) = self.ratio();
        let ticks = u64::from(ppq).checked_mul(numerator)?;
        (ticks % denominator == 0)
            .then_some(ticks / denominator)
            .filter(|ticks| *ticks > 0)
    }
}

impl fmt::Display for SnapGrid {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Whole => "1/1",
            Self::Half => "1/2",
            Self::Quarter => "1/4",
            Self::Eighth => "1/8",
            Self::Sixteenth => "1/16",
            Self::ThirtySecond => "1/32",
            Self::DottedHalf => "1/2 dotted",
            Self::DottedQuarter => "1/4 dotted",
            Self::DottedEighth => "1/8 dotted",
            Self::DottedSixteenth => "1/16 dotted",
            Self::HalfTriplet => "1/2 triplet",
            Self::QuarterTriplet => "1/4 triplet",
            Self::EighthTriplet => "1/8 triplet",
            Self::SixteenthTriplet => "1/16 triplet",
            Self::ThirtySecondTriplet => "1/32 triplet",
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) enum TimelineEvent {
    PanByPixels(f32),
    ZoomAt {
        factor: f32,
        anchor_x: f32,
    },
    SelectItem {
        item_id: Option<ItemId>,
        additive: bool,
        range: bool,
    },
    OpenItemContextMenu {
        item_id: ItemId,
        x: f32,
        y: f32,
    },
    CloseItemContextMenu,
    ToggleVolumeAutomation(TrackId),
    ToggleFxAutomation {
        track_id: TrackId,
        chain_index: usize,
        parameter_id: u32,
        name: String,
        value_range: (f64, f64),
        stepped: bool,
    },
    SetVolumeAutomation(TrackId, Vec<VolumeAutomationPoint>),
    InsertVolumeAutomationAt {
        track_index: usize,
        tick: u64,
        gain_db: f32,
    },
    DeleteVolumeAutomationPoint {
        track_id: TrackId,
        index: usize,
    },
    SelectVolumeAutomationPoint {
        track_id: TrackId,
        index: usize,
    },
    ClearSelectedVolumeAutomationPoint,
    SetFxAutomation {
        track_id: TrackId,
        chain_index: usize,
        parameter_id: u32,
        points: Vec<aaadaw_core::FxParameterAutomationPoint>,
        selected_point: Option<usize>,
    },
    DeleteFxAutomationPoint {
        track_id: TrackId,
        chain_index: usize,
        parameter_id: u32,
        index: usize,
    },
    SelectFxAutomationPoint {
        track_id: TrackId,
        chain_index: usize,
        parameter_id: u32,
        index: usize,
    },
    ClearSelectedFxAutomationPoint,
    SelectEmpty(u64),
    SetTimeSelection {
        start_tick: u64,
        end_tick: u64,
    },
    ClearTimeSelection,
    ToggleSnap,
    SetSnapGrid(SnapGrid),
    BeginItemDrag {
        item_id: ItemId,
        pointer_delta_ticks: i128,
        target_track_index: Option<usize>,
        range: bool,
        ignore_snap: bool,
    },
    UpdateItemDrag {
        pointer_delta_ticks: i128,
        target_track_index: Option<usize>,
        ignore_snap: bool,
    },
    EndItemDrag,
    CancelItemDrag,
    BeginItemTrim {
        item_id: ItemId,
        edge: ItemTrimEdge,
        target_tick: u64,
        ignore_snap: bool,
    },
    UpdateItemTrim {
        target_tick: u64,
        ignore_snap: bool,
    },
    EndItemTrim,
    CancelItemTrim,
    SelectTrack(TrackId),
    OpenTrackContextMenu(TrackId),
    ToggleTrackContextMenu(TrackId),
    CloseTrackContextMenu,
    ResizeSplit {
        split: Split,
        ratio: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItemKind {
    Audio,
    Midi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TimeSelection {
    pub(crate) start_tick: u64,
    pub(crate) end_tick: u64,
}

impl TimeSelection {
    fn normalized(start_tick: u64, end_tick: u64) -> Option<Self> {
        (start_tick != end_tick).then(|| Self {
            start_tick: start_tick.min(end_tick),
            end_tick: start_tick.max(end_tick),
        })
    }
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
    media_ref: Option<String>,
    start_sample: u64,
    length_samples: u64,
    source_offset_samples: u64,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct WaveformBinGeometry {
    pub(super) item_id: ItemId,
    pub(super) start_tick: u64,
    pub(super) end_tick: u64,
    pub(super) track_index: usize,
    pub(super) min: f32,
    pub(super) max: f32,
}

#[derive(Debug, Default, Clone)]
struct TimelineCache {
    generation: u64,
    items: Arc<[TimelineItem]>,
    item_indices: HashMap<ItemId, usize>,
    track_ids: Arc<[TrackId]>,
    ppq: u32,
    snap_grid_ticks: Option<u64>,
    waveform_bins: Arc<[WaveformBinGeometry]>,
}

impl TimelineCache {
    fn rebuild(
        &mut self,
        project: &Project,
        waveforms: &HashMap<String, Arc<AudioWaveform>>,
        snap_grid: SnapGrid,
    ) {
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
                media_ref: Some(item.media_ref().to_owned()),
                start_sample: item.start_sample(),
                length_samples: item.length_samples(),
                source_offset_samples: item.source_offset_samples(),
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
                media_ref: None,
                start_sample: 0,
                length_samples: 0,
                source_offset_samples: 0,
            });
        }

        items.sort_by_key(|item| (item.track_index, item.start_tick, item.id.value()));
        self.generation = self.generation.wrapping_add(1);
        self.item_indices = items
            .iter()
            .enumerate()
            .map(|(index, item)| (item.id, index))
            .collect();
        self.items = items.into();
        self.track_ids = track_ids.into();
        self.ppq = project.settings().ppq();
        self.snap_grid_ticks = snap_grid.tick_interval(self.ppq);
        self.rebuild_waveform_geometry(project, waveforms);
    }

    fn rebuild_waveform_geometry(
        &mut self,
        project: &Project,
        waveforms: &HashMap<String, Arc<AudioWaveform>>,
    ) {
        let project_rate = u128::from(project.settings().sample_rate());
        let mut bins = Vec::new();
        for item in self
            .items
            .iter()
            .filter(|item| item.kind == ItemKind::Audio)
        {
            let Some(waveform) = item
                .media_ref
                .as_ref()
                .and_then(|media_ref| waveforms.get(media_ref))
            else {
                continue;
            };
            let source_rate = u128::from(waveform.sample_rate());
            let frames_per_peak = u64::from(waveform.frames_per_peak());
            if source_rate == 0 || frames_per_peak == 0 || project_rate == 0 {
                continue;
            }
            let source_length =
                (u128::from(item.length_samples) * source_rate + project_rate / 2) / project_rate;
            let Some(source_end) =
                u64::try_from(u128::from(item.source_offset_samples) + source_length).ok()
            else {
                continue;
            };
            let first_peak = item.source_offset_samples / frames_per_peak;
            let last_peak = source_end.div_ceil(frames_per_peak);
            for peak_index in first_peak..last_peak.min(waveform.peaks().len() as u64) {
                let peak = waveform.peaks()[peak_index as usize];
                let peak_start = peak_index.saturating_mul(frames_per_peak);
                let peak_end = peak_start
                    .saturating_add(frames_per_peak)
                    .min(waveform.frame_count());
                let overlap_start = peak_start.max(item.source_offset_samples);
                let overlap_end = peak_end.min(source_end);
                if overlap_start >= overlap_end {
                    continue;
                }
                let start_delta = (u128::from(overlap_start - item.source_offset_samples)
                    * project_rate)
                    / source_rate;
                let end_delta = (u128::from(overlap_end - item.source_offset_samples)
                    * project_rate)
                    .div_ceil(source_rate);
                let Some(start_sample) =
                    u64::try_from(u128::from(item.start_sample) + start_delta).ok()
                else {
                    continue;
                };
                let Some(end_sample) =
                    u64::try_from(u128::from(item.start_sample) + end_delta).ok()
                else {
                    continue;
                };
                let start_sample = start_sample.min(item.start_sample + item.length_samples);
                let end_sample = end_sample.min(item.start_sample + item.length_samples);
                if start_sample >= end_sample {
                    continue;
                }
                let (Ok(start_tick), Ok(end_tick)) = (
                    project.tick_at_sample(start_sample),
                    project.tick_at_sample(end_sample),
                ) else {
                    continue;
                };
                bins.push(WaveformBinGeometry {
                    item_id: item.id,
                    start_tick,
                    end_tick,
                    track_index: item.track_index,
                    min: peak.min,
                    max: peak.max,
                });
            }
        }
        self.waveform_bins = bins.into();
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
    pub(crate) selected_items: HashSet<ItemId>,
    pub(crate) time_selection: Option<TimeSelection>,
    pub(crate) context_track: Option<TrackId>,
    pub(crate) context_item: Option<ItemId>,
    pub(crate) context_item_position: Option<(f32, f32)>,
    audio_waveforms: HashMap<String, Arc<AudioWaveform>>,
    pub(crate) snap_enabled: bool,
    pub(crate) snap_grid: SnapGrid,
    pub(crate) edit_cursor_tick: u64,
    pub(crate) origin_tick: u64,
    pub(crate) pixels_per_tick: f32,
    cache: TimelineCache,
    pan_fractional_tick: f64,
    selection_anchor: Option<ItemId>,
    drag_preview: Option<ItemDragPreview>,
    item_trim_preview: Option<ItemTrimPreview>,
    pub(crate) volume_automation_tracks: HashSet<TrackId>,
    pub(crate) hidden_volume_automation_tracks: HashSet<TrackId>,
    selected_volume_automation_point: Option<(TrackId, usize)>,
    pub(crate) fx_automation_lanes: HashSet<(TrackId, usize, u32)>,
    fx_automation_lane_info: HashMap<(TrackId, usize, u32), FxAutomationLaneInfo>,
    selected_fx_automation_point: Option<(TrackId, usize, u32, usize)>,
    fx_chain_snapshot: HashMap<TrackId, Vec<FxAutomationChainEntry>>,
    fx_lane_snapshot:
        HashMap<(TrackId, usize, u32), Option<Vec<aaadaw_core::FxParameterAutomationPoint>>>,
    row_layout: Vec<TrackRowLayout>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TrackRowLayout {
    pub(super) top: f32,
    pub(super) height: f32,
    pub(super) base_height: f32,
    pub(super) fx_lane_count: usize,
}

#[derive(Clone)]
struct FxAutomationChainEntry {
    plugin_id: String,
    bundle_path: String,
}

#[derive(Clone)]
struct FxAutomationLaneInfo {
    name: String,
    value_range: (f64, f64),
    stepped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ItemDragPreview {
    pub(crate) anchor_item_id: ItemId,
    pub(crate) delta_ticks: i128,
    pub(crate) track_delta: i32,
    pub(crate) target_track_index: Option<usize>,
    pub(crate) valid: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItemTrimEdge {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ItemTrimPreview {
    pub(crate) item_id: ItemId,
    pub(crate) edge: ItemTrimEdge,
    pub(crate) start_tick: u64,
    pub(crate) end_tick: u64,
    pub(crate) valid: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SelectedItemGeometry {
    pub(crate) cache_index: usize,
    pub(crate) item_id: ItemId,
    pub(crate) start_tick: u64,
    pub(crate) end_tick: u64,
    pub(crate) track_index: usize,
    pub(crate) kind: ItemKind,
}

impl Default for TimelineState {
    fn default() -> Self {
        let project = Project::new();
        let mut cache = TimelineCache::default();
        cache.rebuild(&project, &HashMap::new(), SnapGrid::Sixteenth);
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
            selected_items: HashSet::new(),
            time_selection: None,
            context_track: None,
            context_item: None,
            context_item_position: None,
            audio_waveforms: HashMap::new(),
            snap_enabled: true,
            snap_grid: SnapGrid::Sixteenth,
            edit_cursor_tick: 0,
            origin_tick: 0,
            pixels_per_tick: 0.065,
            cache,
            pan_fractional_tick: 0.0,
            selection_anchor: None,
            drag_preview: None,
            item_trim_preview: None,
            volume_automation_tracks: HashSet::new(),
            hidden_volume_automation_tracks: HashSet::new(),
            selected_volume_automation_point: None,
            fx_automation_lanes: HashSet::new(),
            fx_automation_lane_info: HashMap::new(),
            selected_fx_automation_point: None,
            fx_chain_snapshot: HashMap::new(),
            fx_lane_snapshot: HashMap::new(),
            row_layout: Vec::new(),
        }
    }
}

impl TimelineState {
    pub(crate) fn rebuild(&mut self, project: &Project) {
        self.reconcile_fx_automation_lanes(project);
        self.volume_automation_tracks.extend(
            project
                .tracks()
                .iter()
                .filter(|track| {
                    !track.volume_automation().is_empty()
                        && !self.hidden_volume_automation_tracks.contains(&track.id())
                })
                .map(|track| track.id()),
        );
        self.volume_automation_tracks
            .retain(|track_id| project.tracks().iter().any(|track| track.id() == *track_id));
        self.hidden_volume_automation_tracks
            .retain(|track_id| project.tracks().iter().any(|track| track.id() == *track_id));
        self.cache
            .rebuild(project, &self.audio_waveforms, self.snap_grid);
        self.rebuild_row_layout();
        if self.cache.snap_grid_ticks.is_none() {
            self.snap_enabled = false;
        }
        if self
            .selected_track
            .is_some_and(|track_id| !self.cache.track_ids.contains(&track_id))
        {
            self.selected_track = None;
        }
        self.selected_items
            .retain(|item_id| self.cache.item_indices.contains_key(item_id));
        if !self
            .selected_item
            .is_some_and(|item_id| self.selected_items.contains(&item_id))
        {
            self.selected_item = self.selected_items.iter().copied().next();
        }
        if self
            .selection_anchor
            .is_some_and(|item_id| !self.cache.item_indices.contains_key(&item_id))
        {
            self.selection_anchor = self.selected_item;
        }
        if self
            .context_track
            .is_some_and(|track_id| !self.cache.track_ids.contains(&track_id))
        {
            self.context_track = None;
        }
        if self
            .context_item
            .is_some_and(|item_id| !self.cache.item_indices.contains_key(&item_id))
        {
            self.context_item = None;
            self.context_item_position = None;
        }
        if self
            .selected_volume_automation_point
            .is_some_and(|(track_id, index)| {
                project
                    .tracks()
                    .iter()
                    .find(|track| track.id() == track_id)
                    .is_none_or(|track| index >= track.volume_automation().len())
            })
        {
            self.selected_volume_automation_point = None;
        }
    }

    fn reconcile_fx_automation_lanes(&mut self, project: &Project) {
        let mut remapped = HashSet::new();
        let mut target_remap = HashMap::new();
        let mut info_remap = HashMap::new();
        for (track_id, old_index, parameter_id) in self.fx_automation_lanes.iter().copied() {
            let Some(track) = project.tracks().iter().find(|track| track.id() == track_id) else {
                continue;
            };
            let Some(old_chain) = self.fx_chain_snapshot.get(&track_id) else {
                continue;
            };
            let Some(old_plugin) = old_chain.get(old_index) else {
                continue;
            };
            let occurrence = old_chain[..old_index]
                .iter()
                .filter(|plugin| {
                    plugin.plugin_id == old_plugin.plugin_id
                        && plugin.bundle_path == old_plugin.bundle_path
                })
                .count();
            let matches = track
                .fx_chain()
                .iter()
                .enumerate()
                .filter(|(_, plugin)| {
                    plugin.plugin_id() == old_plugin.plugin_id
                        && plugin.bundle_path() == old_plugin.bundle_path
                })
                .collect::<Vec<_>>();
            let old_lane = self
                .fx_lane_snapshot
                .get(&(track_id, old_index, parameter_id))
                .and_then(Option::as_ref);
            let new_index = old_lane
                .and_then(|old_lane| {
                    matches
                        .iter()
                        .find(|(_, plugin)| {
                            plugin
                                .parameter_automation_for(parameter_id)
                                .map(|lane| lane.points())
                                == Some(old_lane.as_slice())
                        })
                        .map(|(index, _)| *index)
                })
                .or_else(|| matches.get(occurrence).map(|(index, _)| *index));
            if let Some(index) = new_index {
                remapped.insert((track_id, index, parameter_id));
                target_remap.insert((track_id, old_index, parameter_id), index);
                if let Some(info) =
                    self.fx_automation_lane_info
                        .get(&(track_id, old_index, parameter_id))
                {
                    info_remap.insert((track_id, index, parameter_id), info.clone());
                }
            }
        }
        self.fx_automation_lanes = remapped;
        self.fx_automation_lane_info = info_remap;
        if let Some((track_id, old_index, parameter_id, point_index)) =
            self.selected_fx_automation_point
        {
            self.selected_fx_automation_point = target_remap
                .get(&(track_id, old_index, parameter_id))
                .copied()
                .map(|index| (track_id, index, parameter_id, point_index));
        }
        if self.selected_fx_automation_point.is_some_and(
            |(track_id, chain_index, parameter_id, point_index)| {
                project
                    .tracks()
                    .iter()
                    .find(|track| track.id() == track_id)
                    .and_then(|track| track.fx_chain().get(chain_index))
                    .and_then(|plugin| plugin.parameter_automation_for(parameter_id))
                    .is_some_and(|lane| point_index >= lane.points().len())
            },
        ) {
            self.selected_fx_automation_point = None;
        }
        self.fx_chain_snapshot = project
            .tracks()
            .iter()
            .map(|track| {
                (
                    track.id(),
                    track
                        .fx_chain()
                        .iter()
                        .map(|plugin| FxAutomationChainEntry {
                            plugin_id: plugin.plugin_id().to_owned(),
                            bundle_path: plugin.bundle_path().to_owned(),
                        })
                        .collect(),
                )
            })
            .collect();
        self.fx_lane_snapshot = self
            .fx_automation_lanes
            .iter()
            .map(|(track_id, chain_index, parameter_id)| {
                let points = project
                    .tracks()
                    .iter()
                    .find(|track| track.id() == *track_id)
                    .and_then(|track| track.fx_chain().get(*chain_index))
                    .and_then(|plugin| plugin.parameter_automation_for(*parameter_id))
                    .map(|lane| lane.points().to_vec());
                ((*track_id, *chain_index, *parameter_id), points)
            })
            .collect();
    }

    fn rebuild_row_layout(&mut self) {
        let mut top = 0.0;
        self.row_layout = self
            .cache
            .track_ids
            .iter()
            .map(|track_id| {
                let fx_lane_count = self
                    .fx_automation_lanes
                    .iter()
                    .filter(|(id, _, _)| id == track_id)
                    .count();
                let height = TIMELINE_ROW_HEIGHT + fx_lane_count as f32 * FX_AUTOMATION_LANE_HEIGHT;
                let row = TrackRowLayout {
                    top,
                    height,
                    base_height: TIMELINE_ROW_HEIGHT,
                    fx_lane_count,
                };
                top += height;
                row
            })
            .collect();
    }

    pub(crate) fn row_layout(&self, track_index: usize) -> Option<TrackRowLayout> {
        self.row_layout.get(track_index).copied()
    }

    pub(crate) fn content_height(&self) -> f32 {
        self.row_layout
            .last()
            .map_or(0.0, |row| row.top + row.height)
    }

    pub(crate) fn set_audio_waveforms(
        &mut self,
        project: &Project,
        waveforms: HashMap<String, Arc<AudioWaveform>>,
    ) {
        self.audio_waveforms = waveforms;
        self.cache
            .rebuild_waveform_geometry(project, &self.audio_waveforms);
        self.cache.generation = self.cache.generation.wrapping_add(1);
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
            TimelineEvent::SelectItem {
                item_id,
                additive,
                range,
            } => {
                self.selected_volume_automation_point = None;
                self.selected_fx_automation_point = None;
                self.select_item(item_id, additive, range, false);
            }
            TimelineEvent::OpenItemContextMenu { item_id, x, y } => {
                if self.cache.item_indices.contains_key(&item_id) {
                    if !self.selected_items.contains(&item_id) {
                        self.select_item(Some(item_id), false, false, false);
                    }
                    self.context_item = Some(item_id);
                    self.context_item_position = Some((x, y));
                }
            }
            TimelineEvent::CloseItemContextMenu => {
                self.context_item = None;
                self.context_item_position = None;
            }
            TimelineEvent::ToggleVolumeAutomation(track_id) => {
                if self.volume_automation_tracks.remove(&track_id) {
                    self.hidden_volume_automation_tracks.insert(track_id);
                } else {
                    self.hidden_volume_automation_tracks.remove(&track_id);
                    self.volume_automation_tracks.insert(track_id);
                }
                self.cache.generation = self.cache.generation.wrapping_add(1);
            }
            TimelineEvent::ToggleFxAutomation {
                track_id,
                chain_index,
                parameter_id,
                name,
                value_range,
                stepped,
            } => {
                let key = (track_id, chain_index, parameter_id);
                if self.fx_automation_lanes.remove(&key) {
                    self.fx_automation_lane_info.remove(&key);
                    if self
                        .selected_fx_automation_point
                        .is_some_and(|(id, chain, parameter, _)| (id, chain, parameter) == key)
                    {
                        self.selected_fx_automation_point = None;
                    }
                } else {
                    self.fx_automation_lanes.insert(key);
                    if name.len() < 128
                        && value_range.0.is_finite()
                        && value_range.1.is_finite()
                        && value_range.1 > value_range.0
                    {
                        self.fx_automation_lane_info.insert(
                            key,
                            FxAutomationLaneInfo {
                                name,
                                value_range,
                                stepped,
                            },
                        );
                    }
                }
                self.rebuild_row_layout();
                self.cache.generation = self.cache.generation.wrapping_add(1);
            }
            TimelineEvent::SelectFxAutomationPoint {
                track_id,
                chain_index,
                parameter_id,
                index,
            } => {
                self.selected_volume_automation_point = None;
                self.selected_fx_automation_point =
                    Some((track_id, chain_index, parameter_id, index));
                self.cache.generation = self.cache.generation.wrapping_add(1);
            }
            TimelineEvent::ClearSelectedFxAutomationPoint => {
                self.selected_volume_automation_point = None;
                self.selected_fx_automation_point = None;
                self.cache.generation = self.cache.generation.wrapping_add(1);
            }
            TimelineEvent::SelectVolumeAutomationPoint { track_id, index } => {
                self.selected_fx_automation_point = None;
                self.selected_volume_automation_point = Some((track_id, index));
                self.cache.generation = self.cache.generation.wrapping_add(1);
            }
            TimelineEvent::ClearSelectedVolumeAutomationPoint => {
                self.selected_fx_automation_point = None;
                self.selected_volume_automation_point = None;
                self.cache.generation = self.cache.generation.wrapping_add(1);
            }
            TimelineEvent::SetVolumeAutomation(_, _)
            | TimelineEvent::SetFxAutomation { .. }
            | TimelineEvent::DeleteFxAutomationPoint { .. }
            | TimelineEvent::InsertVolumeAutomationAt { .. }
            | TimelineEvent::DeleteVolumeAutomationPoint { .. } => {}
            TimelineEvent::SelectEmpty(tick) => {
                self.selected_volume_automation_point = None;
                self.selected_fx_automation_point = None;
                self.edit_cursor_tick = tick;
                self.select_item(None, false, false, false);
            }
            TimelineEvent::SetTimeSelection {
                start_tick,
                end_tick,
            } => self.time_selection = TimeSelection::normalized(start_tick, end_tick),
            TimelineEvent::ClearTimeSelection => self.time_selection = None,
            TimelineEvent::ToggleSnap => {
                if self.cache.snap_grid_ticks.is_some() {
                    self.snap_enabled = !self.snap_enabled;
                }
            }
            TimelineEvent::SetSnapGrid(grid) => {
                self.snap_grid = grid;
                self.cache.snap_grid_ticks = grid.tick_interval(self.cache.ppq);
                if self.cache.snap_grid_ticks.is_none() {
                    self.snap_enabled = false;
                }
            }
            TimelineEvent::BeginItemDrag {
                item_id,
                pointer_delta_ticks,
                target_track_index,
                range,
                ignore_snap,
            } => {
                self.select_item(Some(item_id), false, range, true);
                self.update_item_drag(
                    item_id,
                    pointer_delta_ticks,
                    target_track_index,
                    ignore_snap,
                );
            }
            TimelineEvent::UpdateItemDrag {
                pointer_delta_ticks,
                target_track_index,
                ignore_snap,
            } => {
                if let Some(preview) = self.drag_preview {
                    self.update_item_drag(
                        preview.anchor_item_id,
                        pointer_delta_ticks,
                        target_track_index,
                        ignore_snap,
                    );
                }
            }
            TimelineEvent::EndItemDrag => self.drag_preview = None,
            TimelineEvent::CancelItemDrag => self.drag_preview = None,
            TimelineEvent::BeginItemTrim {
                item_id,
                edge,
                target_tick,
                ignore_snap,
            } => {
                self.select_item(Some(item_id), false, false, false);
                self.drag_preview = None;
                self.update_item_trim(item_id, edge, target_tick, ignore_snap);
            }
            TimelineEvent::UpdateItemTrim {
                target_tick,
                ignore_snap,
            } => {
                if let Some(preview) = self.item_trim_preview {
                    self.update_item_trim(preview.item_id, preview.edge, target_tick, ignore_snap);
                }
            }
            TimelineEvent::EndItemTrim | TimelineEvent::CancelItemTrim => {
                self.item_trim_preview = None;
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

    fn select_item(
        &mut self,
        item_id: Option<ItemId>,
        additive: bool,
        range: bool,
        preserve_existing_for_drag: bool,
    ) {
        let Some(item_id) = item_id else {
            self.selected_items.clear();
            self.selected_item = None;
            self.selection_anchor = None;
            return;
        };
        let Some(&item_index) = self.cache.item_indices.get(&item_id) else {
            return;
        };
        let clicked = &self.cache.items[item_index];

        if range {
            let anchor_index = self
                .selection_anchor
                .and_then(|anchor| self.cache.item_indices.get(&anchor).copied())
                .or_else(|| {
                    self.selected_item
                        .and_then(|anchor| self.cache.item_indices.get(&anchor).copied())
                })
                .unwrap_or(item_index);
            if !additive {
                self.selected_items.clear();
            }
            for item in
                &self.cache.items[anchor_index.min(item_index)..=anchor_index.max(item_index)]
            {
                self.selected_items.insert(item.id);
            }
            self.selected_item = Some(item_id);
        } else if additive && !preserve_existing_for_drag {
            if !self.selected_items.remove(&item_id) {
                self.selected_items.insert(item_id);
                self.selected_item = Some(item_id);
            } else if self.selected_item == Some(item_id) {
                self.selected_item = self.selected_items.iter().copied().next();
            }
            self.selection_anchor = Some(item_id);
        } else if additive {
            self.selected_items.insert(item_id);
            self.selected_item = Some(item_id);
            self.selection_anchor = Some(item_id);
        } else {
            let already_selected = self.selected_items.contains(&item_id);
            if !already_selected || !preserve_existing_for_drag {
                self.selected_items.clear();
                self.selected_items.insert(item_id);
            }
            self.selected_item = Some(item_id);
            self.selection_anchor = Some(item_id);
        }
        self.selected_track = Some(clicked.track_id);
    }

    fn update_item_drag(
        &mut self,
        anchor_item_id: ItemId,
        pointer_delta_ticks: i128,
        target_track_index: Option<usize>,
        ignore_snap: bool,
    ) {
        let Some(&anchor_index) = self.cache.item_indices.get(&anchor_item_id) else {
            self.drag_preview = None;
            return;
        };
        let anchor = &self.cache.items[anchor_index];
        let target_start = i128::from(anchor.start_tick) + pointer_delta_ticks;
        let snapped_delta = if self.snap_enabled && !ignore_snap {
            self.cache.snap_grid_ticks.map(|grid| {
                let grid = i128::from(grid);
                if target_start >= 0 {
                    let snapped_start = ((target_start + grid / 2) / grid) * grid;
                    snapped_start - i128::from(anchor.start_tick)
                } else {
                    pointer_delta_ticks
                }
            })
        } else {
            Some(pointer_delta_ticks)
        };
        let track_delta =
            target_track_index.map_or(0, |target| target as i32 - anchor.track_index as i32);
        let delta_ticks = snapped_delta.unwrap_or(pointer_delta_ticks);
        let mut valid = snapped_delta.is_some() && target_track_index.is_some();
        for item_id in &self.selected_items {
            let Some(&index) = self.cache.item_indices.get(item_id) else {
                valid = false;
                continue;
            };
            let item = &self.cache.items[index];
            let new_start = i128::from(item.start_tick) + delta_ticks;
            let new_end = i128::from(item.end_tick) + delta_ticks;
            let new_track = item.track_index as i32 + track_delta;
            valid &= new_start >= 0
                && new_end <= i128::from(u64::MAX)
                && (0..self.cache.track_ids.len() as i32).contains(&new_track);
        }
        self.drag_preview = Some(ItemDragPreview {
            anchor_item_id,
            delta_ticks,
            track_delta,
            target_track_index,
            valid,
        });
    }

    fn update_item_trim(
        &mut self,
        item_id: ItemId,
        edge: ItemTrimEdge,
        target_tick: u64,
        ignore_snap: bool,
    ) {
        let Some(&item_index) = self.cache.item_indices.get(&item_id) else {
            self.item_trim_preview = None;
            return;
        };
        let item = &self.cache.items[item_index];
        if item.kind != ItemKind::Audio {
            self.item_trim_preview = None;
            return;
        }
        let target_tick = snap_tick_to_grid(
            target_tick,
            self.cache.snap_grid_ticks,
            self.snap_enabled,
            ignore_snap,
        );
        let (start_tick, end_tick, valid) = match edge {
            ItemTrimEdge::Start => {
                let start_tick = target_tick.max(item.start_tick);
                (
                    start_tick,
                    item.end_tick,
                    start_tick > item.start_tick && start_tick < item.end_tick,
                )
            }
            ItemTrimEdge::End => {
                let end_tick = target_tick.min(item.end_tick);
                (
                    item.start_tick,
                    end_tick,
                    end_tick < item.end_tick && end_tick > item.start_tick,
                )
            }
        };
        self.item_trim_preview = Some(ItemTrimPreview {
            item_id,
            edge,
            start_tick,
            end_tick,
            valid,
        });
    }

    pub(crate) fn drag_preview(&self) -> Option<ItemDragPreview> {
        self.drag_preview
    }

    pub(crate) fn item_trim_preview(&self) -> Option<ItemTrimPreview> {
        self.item_trim_preview
    }

    pub(crate) fn has_snap_grid(&self) -> bool {
        self.cache.snap_grid_ticks.is_some()
    }

    pub(crate) fn selected_item_geometries(&self) -> Vec<SelectedItemGeometry> {
        let mut selected = self
            .selected_items
            .iter()
            .filter_map(|item_id| {
                let index = *self.cache.item_indices.get(item_id)?;
                let item = &self.cache.items[index];
                Some(SelectedItemGeometry {
                    cache_index: index,
                    item_id: item.id,
                    start_tick: item.start_tick,
                    end_tick: item.end_tick,
                    track_index: item.track_index,
                    kind: item.kind,
                })
            })
            .collect::<Vec<_>>();
        selected.sort_unstable_by_key(|item| item.cache_index);
        selected
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
            selected_items: self.selected_item_geometries(),
            time_selection: self.time_selection,
            snap_enabled: self.snap_enabled,
            selected_track: self.selected_track,
            drag_preview: self.drag_preview,
            item_trim_preview: self.item_trim_preview,
            volume_automation_tracks: &self.volume_automation_tracks,
            hidden_volume_automation_tracks: &self.hidden_volume_automation_tracks,
            selected_volume_automation_point: self.selected_volume_automation_point,
            fx_automation_lanes: &self.fx_automation_lanes,
            fx_automation_lane_info: &self.fx_automation_lane_info,
            selected_fx_automation_point: self.selected_fx_automation_point,
            row_layout: &self.row_layout,
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
    selected_items: Vec<SelectedItemGeometry>,
    time_selection: Option<TimeSelection>,
    snap_enabled: bool,
    selected_track: Option<TrackId>,
    drag_preview: Option<ItemDragPreview>,
    item_trim_preview: Option<ItemTrimPreview>,
    volume_automation_tracks: &'a HashSet<TrackId>,
    hidden_volume_automation_tracks: &'a HashSet<TrackId>,
    selected_volume_automation_point: Option<(TrackId, usize)>,
    fx_automation_lanes: &'a HashSet<(TrackId, usize, u32)>,
    fx_automation_lane_info: &'a HashMap<(TrackId, usize, u32), FxAutomationLaneInfo>,
    selected_fx_automation_point: Option<(TrackId, usize, u32, usize)>,
    row_layout: &'a [TrackRowLayout],
}

#[derive(Default)]
struct TimelineInteractionState {
    modifiers: keyboard::Modifiers,
    pan_last_x: Option<f32>,
    pending_item_drag: Option<PendingItemDrag>,
    pending_item_trim: Option<PendingItemTrim>,
    pending_time_selection_drag: Option<PendingTimeSelectionDrag>,
    last_item_click: Option<(ItemId, Instant)>,
    pending_automation_point: Option<PendingAutomationPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PendingAutomationPoint {
    track_index: usize,
    point_index: usize,
    fx_target: Option<(TrackId, usize, u32)>,
    start_x: f32,
    start_y: f32,
}

#[derive(Debug, Clone, Copy)]
struct FxAutomationTarget {
    track_index: usize,
    chain_index: usize,
    parameter_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PendingItemDrag {
    item_id: ItemId,
    pointer_start_tick: u64,
    pointer_start_x: f32,
    pointer_start_y: f32,
    modifiers: keyboard::Modifiers,
    is_dragging: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PendingItemTrim {
    item_id: ItemId,
    edge: ItemTrimEdge,
    pointer_start_x: f32,
    pointer_start_y: f32,
    is_dragging: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimeSelectionEdge {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimeSelectionDragMode {
    Create,
    ResizeStart,
    ResizeEnd,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PendingTimeSelectionDrag {
    mode: TimeSelectionDragMode,
    anchor_tick: u64,
    fixed_tick: u64,
    pointer_start_x: f32,
    pointer_start_y: f32,
    is_dragging: bool,
}

impl PendingTimeSelectionDrag {
    fn range_at(self, active_tick: u64) -> (u64, u64) {
        match self.mode {
            TimeSelectionDragMode::Create => (self.fixed_tick, active_tick),
            TimeSelectionDragMode::ResizeStart => (active_tick, self.fixed_tick),
            TimeSelectionDragMode::ResizeEnd => (self.fixed_tick, active_tick),
        }
    }
}

impl TimelineProgram<'_> {
    fn fx_lanes_for_track(&self, track_id: TrackId) -> Vec<(usize, u32)> {
        let mut lanes = self
            .fx_automation_lanes
            .iter()
            .filter_map(|(id, index, parameter)| (*id == track_id).then_some((*index, *parameter)))
            .collect::<Vec<_>>();
        lanes.sort_unstable();
        lanes
    }

    fn fx_lane_at_y(&self, track_id: TrackId, row_y: f32) -> Option<(usize, u32, usize, f32, f32)> {
        self.fx_lanes_for_track(track_id)
            .into_iter()
            .enumerate()
            .find_map(|(lane_index, (chain, parameter))| {
                let top = TIMELINE_ROW_HEIGHT + lane_index as f32 * FX_AUTOMATION_LANE_HEIGHT;
                let bottom = top + FX_AUTOMATION_LANE_HEIGHT;
                (row_y >= top && row_y < bottom)
                    .then_some((chain, parameter, lane_index, top, bottom))
            })
    }
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
        if let Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) = event
            && matches!(
                key,
                keyboard::Key::Named(
                    keyboard::key::Named::Delete | keyboard::key::Named::Backspace
                )
            )
            && (self.selected_volume_automation_point.is_some()
                || self.selected_fx_automation_point.is_some())
        {
            if let Some((track_id, chain_index, parameter_id, index)) =
                self.selected_fx_automation_point
            {
                return Some(
                    shader::Action::publish(crate::app::Message::Timeline(
                        TimelineEvent::DeleteFxAutomationPoint {
                            track_id,
                            chain_index,
                            parameter_id,
                            index,
                        },
                    ))
                    .and_capture(),
                );
            }
            let (track_id, index) = self.selected_volume_automation_point?;
            return Some(
                shader::Action::publish(crate::app::Message::Timeline(
                    TimelineEvent::DeleteVolumeAutomationPoint { track_id, index },
                ))
                .and_capture(),
            );
        }
        if let Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) = event
            && *key == keyboard::Key::Named(keyboard::key::Named::Escape)
        {
            let cancel_drag = state
                .pending_item_drag
                .take()
                .is_some_and(|drag| drag.is_dragging);
            let cancel_trim = state
                .pending_item_trim
                .take()
                .is_some_and(|trim| trim.is_dragging);
            let cancel_time_selection_drag = state
                .pending_time_selection_drag
                .take()
                .is_some_and(|drag| drag.is_dragging);
            return if cancel_drag {
                Some(
                    shader::Action::publish(crate::app::Message::Timeline(
                        TimelineEvent::CancelItemDrag,
                    ))
                    .and_capture(),
                )
            } else if cancel_trim {
                Some(
                    shader::Action::publish(crate::app::Message::Timeline(
                        TimelineEvent::CancelItemTrim,
                    ))
                    .and_capture(),
                )
            } else if cancel_time_selection_drag {
                Some(shader::Action::capture())
            } else {
                None
            };
        }

        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let position = cursor.position_in(bounds)?;
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
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                let position = cursor.position_in(bounds)?;
                let tick = tick_at_x(self.origin_tick, self.pixels_per_tick, position.x);
                let (track_index, row_y) = row_at_y(self.row_layout, position.y)?;
                if let Some(track) = self.project.tracks().get(track_index)
                    && let Some((chain_index, parameter_id, _lane_index, lane_top, lane_bottom)) =
                        self.fx_lane_at_y(track.id(), row_y)
                    && let Some(index) = fx_automation_point_at(
                        self.project,
                        self.pixels_per_tick,
                        FxAutomationTarget {
                            track_index,
                            chain_index,
                            parameter_id,
                        },
                        tick,
                        row_y,
                        self.fx_automation_lane_info
                            .get(&(track.id(), chain_index, parameter_id))
                            .map(|info| info.value_range),
                        (lane_top + 2.0, lane_bottom - 2.0),
                    )
                {
                    return Some(
                        shader::Action::publish(crate::app::Message::Timeline(
                            TimelineEvent::DeleteFxAutomationPoint {
                                track_id: track.id(),
                                chain_index,
                                parameter_id,
                                index,
                            },
                        ))
                        .and_capture(),
                    );
                }
                if (62.0..TIMELINE_ROW_HEIGHT).contains(&row_y)
                    && let Some(track) = self.project.tracks().get(track_index)
                    && (self.volume_automation_tracks.contains(&track.id())
                        || (!self.hidden_volume_automation_tracks.contains(&track.id())
                            && !track.volume_automation().is_empty()))
                    && let Some(index) = automation_point_at(
                        self.project,
                        self.pixels_per_tick,
                        track_index,
                        tick,
                        row_y,
                    )
                {
                    return Some(
                        shader::Action::publish(crate::app::Message::Timeline(
                            TimelineEvent::DeleteVolumeAutomationPoint {
                                track_id: track.id(),
                                index,
                            },
                        ))
                        .and_capture(),
                    );
                }
                let event = if row_y < TIMELINE_ROW_HEIGHT {
                    self.cache.item_at(track_index, tick).map_or(
                        TimelineEvent::CloseItemContextMenu,
                        |item| TimelineEvent::OpenItemContextMenu {
                            item_id: item.id,
                            x: position.x,
                            y: position.y,
                        },
                    )
                } else {
                    TimelineEvent::CloseItemContextMenu
                };
                Some(shader::Action::publish(crate::app::Message::Timeline(event)).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle)) => {
                if state.pan_last_x.take().is_some() {
                    Some(shader::Action::capture())
                } else {
                    None
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if state.pending_automation_point.is_some() {
                    Some(shader::Action::capture())
                } else if let Some(last_x) = state.pan_last_x {
                    let local_x = position.x - bounds.x;
                    state.pan_last_x = Some(local_x);
                    let delta_x = local_x - last_x;
                    Some(
                        shader::Action::publish(crate::app::Message::Timeline(
                            TimelineEvent::PanByPixels(delta_x),
                        ))
                        .and_capture(),
                    )
                } else if let Some(mut trim) = state.pending_item_trim {
                    let local_x = position.x - bounds.x;
                    let local_y = position.y - bounds.y;
                    let distance =
                        (local_x - trim.pointer_start_x).hypot(local_y - trim.pointer_start_y);
                    let was_dragging = trim.is_dragging;
                    if !was_dragging && distance < 3.0 {
                        None
                    } else {
                        trim.is_dragging = true;
                        state.pending_item_trim = Some(trim);
                        Some(
                            shader::Action::publish(crate::app::Message::Timeline(
                                if was_dragging {
                                    TimelineEvent::UpdateItemTrim {
                                        target_tick: tick_at_x(
                                            self.origin_tick,
                                            self.pixels_per_tick,
                                            local_x,
                                        ),
                                        ignore_snap: state.modifiers.shift(),
                                    }
                                } else {
                                    TimelineEvent::BeginItemTrim {
                                        item_id: trim.item_id,
                                        edge: trim.edge,
                                        target_tick: tick_at_x(
                                            self.origin_tick,
                                            self.pixels_per_tick,
                                            local_x,
                                        ),
                                        ignore_snap: state.modifiers.shift(),
                                    }
                                },
                            ))
                            .and_capture(),
                        )
                    }
                } else if let Some(mut drag) = state.pending_item_drag {
                    let local_x = position.x - bounds.x;
                    let local_y = position.y - bounds.y;
                    let distance =
                        (local_x - drag.pointer_start_x).hypot(local_y - drag.pointer_start_y);
                    let was_dragging = drag.is_dragging;
                    if !was_dragging && distance < 3.0 {
                        None
                    } else {
                        drag.is_dragging = true;
                        state.pending_item_drag = Some(drag);
                        let current_tick =
                            tick_at_x(self.origin_tick, self.pixels_per_tick, local_x);
                        let event = if was_dragging {
                            TimelineEvent::UpdateItemDrag {
                                pointer_delta_ticks: i128::from(current_tick)
                                    - i128::from(drag.pointer_start_tick),
                                target_track_index: track_index_at_y(local_y, self.row_layout),
                                ignore_snap: state.modifiers.shift(),
                            }
                        } else {
                            TimelineEvent::BeginItemDrag {
                                item_id: drag.item_id,
                                pointer_delta_ticks: i128::from(current_tick)
                                    - i128::from(drag.pointer_start_tick),
                                target_track_index: track_index_at_y(local_y, self.row_layout),
                                range: drag.modifiers.shift(),
                                ignore_snap: state.modifiers.shift(),
                            }
                        };
                        Some(
                            shader::Action::publish(crate::app::Message::Timeline(event))
                                .and_capture(),
                        )
                    }
                } else if let Some(mut drag) = state.pending_time_selection_drag {
                    let local_x = position.x - bounds.x;
                    let local_y = position.y - bounds.y;
                    let distance =
                        (local_x - drag.pointer_start_x).hypot(local_y - drag.pointer_start_y);
                    if !drag.is_dragging && distance < 3.0 {
                        None
                    } else {
                        drag.is_dragging = true;
                        state.pending_time_selection_drag = Some(drag);
                        let active_tick = snap_tick_to_grid(
                            tick_at_x(self.origin_tick, self.pixels_per_tick, local_x),
                            self.cache.snap_grid_ticks,
                            self.snap_enabled,
                            state.modifiers.shift(),
                        );
                        let (start_tick, end_tick) = drag.range_at(active_tick);
                        Some(
                            shader::Action::publish(crate::app::Message::Timeline(
                                TimelineEvent::SetTimeSelection {
                                    start_tick,
                                    end_tick,
                                },
                            ))
                            .and_capture(),
                        )
                    }
                } else {
                    None
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let position = cursor.position_in(bounds)?;
                let (track_index, row_y) = row_at_y(self.row_layout, position.y)?;
                let raw_tick = tick_at_x(self.origin_tick, self.pixels_per_tick, position.x);
                let tick = snap_tick_to_grid(
                    raw_tick,
                    self.cache.snap_grid_ticks,
                    self.snap_enabled,
                    state.modifiers.shift(),
                );
                if let Some(track) = self.project.tracks().get(track_index)
                    && let Some((chain_index, parameter_id, _lane_index, lane_top, lane_bottom)) =
                        self.fx_lane_at_y(track.id(), row_y)
                {
                    let target = (track.id(), chain_index, parameter_id);
                    if let Some(point_index) = fx_automation_point_at(
                        self.project,
                        self.pixels_per_tick,
                        FxAutomationTarget {
                            track_index,
                            chain_index,
                            parameter_id,
                        },
                        raw_tick,
                        row_y,
                        self.fx_automation_lane_info
                            .get(&target)
                            .map(|info| info.value_range),
                        (lane_top + 2.0, lane_bottom - 2.0),
                    ) {
                        state.pending_automation_point = Some(PendingAutomationPoint {
                            track_index,
                            point_index,
                            fx_target: Some(target),
                            start_x: position.x,
                            start_y: position.y,
                        });
                        return Some(
                            shader::Action::publish(crate::app::Message::Timeline(
                                TimelineEvent::SelectFxAutomationPoint {
                                    track_id: track.id(),
                                    chain_index,
                                    parameter_id,
                                    index: point_index,
                                },
                            ))
                            .and_capture(),
                        );
                    }
                    let snapped_tick = fx_automation_tick_at(
                        raw_tick,
                        self.cache.snap_grid_ticks,
                        self.snap_enabled,
                        state.modifiers.shift(),
                    );
                    let Ok(sample) = self.project.sample_at_tick(snapped_tick) else {
                        return None;
                    };
                    let plugin = track.fx_chain().get(chain_index)?;
                    let lane = plugin.parameter_automation_for(parameter_id);
                    let range = self
                        .fx_automation_lane_info
                        .get(&target)
                        .map(|info| info.value_range)
                        .unwrap_or_else(|| {
                            fx_value_range(
                                lane.into_iter()
                                    .flat_map(|lane| lane.points())
                                    .map(|point| point.value())
                                    .chain(plugin.parameter_value(parameter_id)),
                            )
                        });
                    let stepped = self
                        .fx_automation_lane_info
                        .get(&target)
                        .is_some_and(|info| info.stepped);
                    let value =
                        fx_value_at_y(range, row_y, stepped, lane_top + 2.0, lane_bottom - 2.0);
                    let mut points = lane.map_or_else(Vec::new, |lane| lane.points().to_vec());
                    let point = aaadaw_core::FxParameterAutomationPoint::new(sample, value)?;
                    let index = match points.binary_search_by_key(&sample, |point| point.sample()) {
                        Ok(index) => {
                            points[index] = point;
                            index
                        }
                        Err(index) => {
                            points.insert(index, point);
                            index
                        }
                    };
                    return Some(
                        shader::Action::publish(crate::app::Message::Timeline(
                            TimelineEvent::SetFxAutomation {
                                track_id: track.id(),
                                chain_index,
                                parameter_id,
                                points,
                                selected_point: Some(index),
                            },
                        ))
                        .and_capture(),
                    );
                }
                if row_y >= 62.0
                    && let Some(track) = self.project.tracks().get(track_index)
                    && (self.volume_automation_tracks.contains(&track.id())
                        || (!self.hidden_volume_automation_tracks.contains(&track.id())
                            && !track.volume_automation().is_empty()))
                {
                    if let Some(point_index) = automation_point_at(
                        self.project,
                        self.pixels_per_tick,
                        track_index,
                        raw_tick,
                        row_y,
                    ) {
                        state.pending_automation_point = Some(PendingAutomationPoint {
                            track_index,
                            point_index,
                            fx_target: None,
                            start_x: position.x,
                            start_y: position.y,
                        });
                        return Some(
                            shader::Action::publish(crate::app::Message::Timeline(
                                TimelineEvent::SelectVolumeAutomationPoint {
                                    track_id: track.id(),
                                    index: point_index,
                                },
                            ))
                            .and_capture(),
                        );
                    }
                    let gain_db = (6.0 - ((row_y - 66.0) / 16.0) * 66.0).clamp(-60.0, 6.0);
                    return Some(
                        shader::Action::publish(crate::app::Message::Timeline(
                            TimelineEvent::InsertVolumeAutomationAt {
                                track_index,
                                tick,
                                gain_db,
                            },
                        ))
                        .and_capture(),
                    );
                }
                let hit = self.cache.item_at(track_index, tick);
                let trim_edge = item_trim_edge_at_x(
                    self.cache,
                    track_index,
                    position.x,
                    self.origin_tick,
                    self.pixels_per_tick,
                );
                let selection_edge = self.time_selection.and_then(|selection| {
                    time_selection_edge_at_tick(selection, tick, self.pixels_per_tick)
                });
                if let Some(edge) = selection_edge {
                    let selection = self.time_selection.expect("edge requires a selection");
                    let (mode, fixed_tick) = match edge {
                        TimeSelectionEdge::Start => {
                            (TimeSelectionDragMode::ResizeStart, selection.end_tick)
                        }
                        TimeSelectionEdge::End => {
                            (TimeSelectionDragMode::ResizeEnd, selection.start_tick)
                        }
                    };
                    state.pending_item_drag = None;
                    state.pending_time_selection_drag = Some(PendingTimeSelectionDrag {
                        mode,
                        anchor_tick: tick,
                        fixed_tick,
                        pointer_start_x: position.x,
                        pointer_start_y: position.y,
                        is_dragging: false,
                    });
                    Some(shader::Action::capture())
                } else if let Some((item_id, edge)) = trim_edge {
                    state.pending_item_drag = None;
                    state.pending_time_selection_drag = None;
                    state.pending_item_trim = Some(PendingItemTrim {
                        item_id,
                        edge,
                        pointer_start_x: position.x,
                        pointer_start_y: position.y,
                        is_dragging: false,
                    });
                    Some(shader::Action::capture())
                } else if let Some(item) = hit {
                    state.pending_time_selection_drag = None;
                    state.pending_item_drag = Some(PendingItemDrag {
                        item_id: item.id,
                        pointer_start_tick: tick,
                        pointer_start_x: position.x,
                        pointer_start_y: position.y,
                        modifiers: state.modifiers,
                        is_dragging: false,
                    });
                    Some(shader::Action::capture())
                } else {
                    state.pending_item_drag = None;
                    state.pending_time_selection_drag = Some(PendingTimeSelectionDrag {
                        mode: TimeSelectionDragMode::Create,
                        anchor_tick: tick,
                        fixed_tick: snap_tick_to_grid(
                            tick,
                            self.cache.snap_grid_ticks,
                            self.snap_enabled,
                            state.modifiers.shift(),
                        ),
                        pointer_start_x: position.x,
                        pointer_start_y: position.y,
                        is_dragging: false,
                    });
                    Some(shader::Action::capture())
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some(drag) = state.pending_automation_point.take() {
                    let Some(track) = self.project.tracks().get(drag.track_index) else {
                        return Some(shader::Action::capture());
                    };
                    let Some(position) = cursor.position_in(bounds) else {
                        return Some(shader::Action::capture());
                    };
                    if (position.x - drag.start_x).hypot(position.y - drag.start_y) < 2.0 {
                        return Some(shader::Action::capture());
                    }
                    let tick = snap_tick_to_grid(
                        tick_at_x(self.origin_tick, self.pixels_per_tick, position.x),
                        self.cache.snap_grid_ticks,
                        self.snap_enabled,
                        state.modifiers.shift(),
                    );
                    let sample = self.project.sample_at_tick(tick).ok()?;
                    if let Some((track_id, chain_index, parameter_id)) = drag.fx_target {
                        let Some(plugin) = track.fx_chain().get(chain_index) else {
                            return Some(shader::Action::capture());
                        };
                        let Some(lane) = plugin.parameter_automation_for(parameter_id) else {
                            return Some(shader::Action::capture());
                        };
                        let mut points = lane.points().to_vec();
                        if drag.point_index < points.len() {
                            let min_sample = drag
                                .point_index
                                .checked_sub(1)
                                .map_or(0, |index| points[index].sample().saturating_add(1));
                            let max_sample = points
                                .get(drag.point_index + 1)
                                .map_or(u64::MAX, |point| point.sample().saturating_sub(1));
                            let range = self
                                .fx_automation_lane_info
                                .get(&(track_id, chain_index, parameter_id))
                                .map(|info| info.value_range)
                                .unwrap_or_else(|| {
                                    fx_value_range(points.iter().map(|point| point.value()))
                                });
                            let Some(row) = self.row_layout.get(drag.track_index) else {
                                return Some(shader::Action::capture());
                            };
                            let lane_index = self
                                .fx_lanes_for_track(track_id)
                                .iter()
                                .position(|lane| *lane == (chain_index, parameter_id))
                                .unwrap_or(0);
                            let lane_top = row.base_height
                                + lane_index as f32 * FX_AUTOMATION_LANE_HEIGHT
                                + 2.0;
                            let lane_bottom = lane_top + FX_AUTOMATION_LANE_HEIGHT - 4.0;
                            let y = (position.y - row.top).clamp(lane_top, lane_bottom);
                            if let Some(point) = aaadaw_core::FxParameterAutomationPoint::new(
                                sample.clamp(min_sample, max_sample),
                                fx_value_at_y(
                                    range,
                                    y,
                                    self.fx_automation_lane_info
                                        .get(&(track_id, chain_index, parameter_id))
                                        .is_some_and(|info| info.stepped),
                                    lane_top,
                                    lane_bottom,
                                ),
                            ) {
                                points[drag.point_index] = point;
                            }
                        }
                        return Some(
                            shader::Action::publish(crate::app::Message::Timeline(
                                TimelineEvent::SetFxAutomation {
                                    track_id,
                                    chain_index,
                                    parameter_id,
                                    points,
                                    selected_point: Some(drag.point_index),
                                },
                            ))
                            .and_capture(),
                        );
                    }
                    let Some(row) = self.row_layout.get(drag.track_index) else {
                        return Some(shader::Action::capture());
                    };
                    let y = (position.y - row.top).clamp(62.0, row.base_height - 1.0);
                    let gain_db = (6.0 - ((y - 66.0) / 16.0) * 66.0).clamp(-60.0, 6.0);
                    let mut points = track.volume_automation().to_vec();
                    if drag.point_index < points.len() {
                        let min_sample = drag
                            .point_index
                            .checked_sub(1)
                            .map_or(0, |index| points[index].sample().saturating_add(1));
                        let max_sample = points
                            .get(drag.point_index + 1)
                            .map_or(u64::MAX, |point| point.sample().saturating_sub(1));
                        if let Some(point) = aaadaw_core::VolumeAutomationPoint::new(
                            sample.clamp(min_sample, max_sample),
                            gain_db,
                        ) {
                            points[drag.point_index] = point;
                        }
                    }
                    Some(
                        shader::Action::publish(crate::app::Message::Timeline(
                            TimelineEvent::SetVolumeAutomation(track.id(), points),
                        ))
                        .and_capture(),
                    )
                } else if let Some(trim) = state.pending_item_trim.take() {
                    if trim.is_dragging {
                        state.last_item_click = None;
                        Some(
                            shader::Action::publish(crate::app::Message::Timeline(
                                TimelineEvent::EndItemTrim,
                            ))
                            .and_capture(),
                        )
                    } else {
                        Some(
                            shader::Action::publish(crate::app::Message::Timeline(
                                TimelineEvent::SelectItem {
                                    item_id: Some(trim.item_id),
                                    additive: state.modifiers.command(),
                                    range: state.modifiers.shift(),
                                },
                            ))
                            .and_capture(),
                        )
                    }
                } else if let Some(drag) = state.pending_item_drag.take() {
                    if drag.is_dragging {
                        state.last_item_click = None;
                        Some(
                            shader::Action::publish(crate::app::Message::Timeline(
                                TimelineEvent::EndItemDrag,
                            ))
                            .and_capture(),
                        )
                    } else {
                        let now = Instant::now();
                        let is_double_click =
                            state.last_item_click.is_some_and(|(item_id, when)| {
                                item_id == drag.item_id
                                    && now.duration_since(when) <= Duration::from_millis(400)
                            });
                        state.last_item_click = Some((drag.item_id, now));
                        if is_double_click
                            && self
                                .cache
                                .item_indices
                                .get(&drag.item_id)
                                .is_some_and(|index| {
                                    self.cache.items[*index].kind == ItemKind::Midi
                                })
                        {
                            Some(
                                shader::Action::publish(crate::app::Message::OpenMidiEditor(
                                    drag.item_id,
                                ))
                                .and_capture(),
                            )
                        } else {
                            Some(
                                shader::Action::publish(crate::app::Message::Timeline(
                                    TimelineEvent::SelectItem {
                                        item_id: Some(drag.item_id),
                                        additive: drag.modifiers.command(),
                                        range: drag.modifiers.shift(),
                                    },
                                ))
                                .and_capture(),
                            )
                        }
                    }
                } else if let Some(drag) = state.pending_time_selection_drag.take() {
                    if drag.mode == TimeSelectionDragMode::Create && !drag.is_dragging {
                        Some(
                            shader::Action::publish(crate::app::Message::Timeline(
                                TimelineEvent::SelectEmpty(drag.anchor_tick),
                            ))
                            .and_capture(),
                        )
                    } else {
                        Some(shader::Action::capture())
                    }
                } else {
                    None
                }
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
        let mut fx_keys = self.fx_automation_lanes.iter().copied().collect::<Vec<_>>();
        fx_keys.sort_by_key(|(track_id, chain_index, parameter_id)| {
            (
                self.cache
                    .track_ids
                    .iter()
                    .position(|id| id == track_id)
                    .unwrap_or(usize::MAX),
                *chain_index,
                *parameter_id,
            )
        });
        let mut lane_slots = HashMap::<TrackId, usize>::new();
        let mut automation_lanes = self
            .project
            .tracks()
            .iter()
            .enumerate()
            .filter(|(_, track)| {
                self.volume_automation_tracks.contains(&track.id())
                    || (!self.hidden_volume_automation_tracks.contains(&track.id())
                        && !track.volume_automation().is_empty())
            })
            .map(|(track_index, track)| renderer::AutomationLane {
                track_index: track_index as u32,
                is_fx: false,
                lane_index: 0,
                value_range: (-60.0, 6.0),
                selected_point: self
                    .selected_volume_automation_point
                    .filter(|(id, _)| *id == track.id())
                    .map(|(_, index)| index),
                points: track
                    .volume_automation()
                    .iter()
                    .filter_map(|point| {
                        self.project
                            .tick_at_sample(point.sample())
                            .ok()
                            .map(|tick| (tick, point.gain_db()))
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();
        for (track_id, chain_index, parameter_id) in fx_keys {
            let Some(track_index) = self
                .project
                .tracks()
                .iter()
                .position(|track| track.id() == track_id)
            else {
                continue;
            };
            let Some(plugin) = self.project.tracks()[track_index]
                .fx_chain()
                .get(chain_index)
            else {
                continue;
            };
            let lane_index = lane_slots.entry(track_id).or_default();
            let current_lane_index = *lane_index;
            *lane_index += 1;
            let lane = plugin.parameter_automation_for(parameter_id);
            let points = lane
                .into_iter()
                .flat_map(|lane| lane.points())
                .filter_map(|point| {
                    self.project
                        .tick_at_sample(point.sample())
                        .ok()
                        .map(|tick| (tick, point.value() as f32))
                })
                .collect::<Vec<_>>();
            let value_range = self
                .fx_automation_lane_info
                .get(&(track_id, chain_index, parameter_id))
                .map(|info| info.value_range)
                .unwrap_or_else(|| {
                    fx_value_range(
                        lane.into_iter()
                            .flat_map(|lane| lane.points())
                            .map(|point| point.value())
                            .chain(plugin.parameter_value(parameter_id)),
                    )
                });
            automation_lanes.push(renderer::AutomationLane {
                track_index: track_index as u32,
                is_fx: true,
                lane_index: current_lane_index,
                value_range: (value_range.0 as f32, value_range.1 as f32),
                selected_point: self
                    .selected_fx_automation_point
                    .filter(|(id, index, parameter, _)| {
                        *id == track_id && *index == chain_index && *parameter == parameter_id
                    })
                    .map(|(_, _, _, point)| point),
                points,
            });
        }
        renderer::TimelinePrimitive {
            generation: self.cache.generation,
            items: Arc::clone(&self.cache.items),
            track_count: self.cache.track_ids.len() as u32,
            row_layout: self.row_layout.to_vec(),
            origin_tick: self.origin_tick,
            pixels_per_tick: self.pixels_per_tick,
            edit_cursor_tick: self.edit_cursor_tick,
            playhead_tick: self.playhead_tick,
            selected_items: self.selected_items.clone(),
            waveform_bins: Arc::clone(&self.cache.waveform_bins),
            time_selection: self.time_selection,
            drag_preview: self.drag_preview,
            item_trim_preview: self.item_trim_preview,
            volume_automation: automation_lanes,
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
            if self.time_selection.is_some_and(|selection| {
                time_selection_edge_at_tick(selection, tick, self.pixels_per_tick).is_some()
            }) {
                return mouse::Interaction::ResizingHorizontally;
            }
            let Some(row_index) = track_index_at_y(position.y, self.row_layout) else {
                return mouse::Interaction::default();
            };
            if item_trim_edge_at_x(
                self.cache,
                row_index,
                position.x,
                self.origin_tick,
                self.pixels_per_tick,
            )
            .is_some()
            {
                return mouse::Interaction::ResizingHorizontally;
            }
            if self.cache.item_at(row_index, tick).is_some() {
                return mouse::Interaction::Pointer;
            }
            return mouse::Interaction::Crosshair;
        }
        mouse::Interaction::default()
    }
}

fn item_trim_edge_at_x(
    cache: &TimelineCache,
    track_index: usize,
    x: f32,
    origin_tick: u64,
    pixels_per_tick: f32,
) -> Option<(ItemId, ItemTrimEdge)> {
    cache
        .items
        .iter()
        .filter(|item| item.track_index == track_index && item.kind == ItemKind::Audio)
        .flat_map(|item| {
            [
                (item.start_tick, ItemTrimEdge::Start),
                (item.end_tick, ItemTrimEdge::End),
            ]
            .map(move |(tick, edge)| (item, tick, edge))
        })
        .filter_map(|(item, tick, edge)| {
            let edge_x = (tick as f64 - origin_tick as f64) * f64::from(pixels_per_tick);
            let distance = (f64::from(x) - edge_x).abs();
            (distance <= ITEM_TRIM_EDGE_HIT_RADIUS_PX).then_some((item.id, edge, distance))
        })
        .min_by(|left, right| left.2.total_cmp(&right.2))
        .map(|(item_id, edge, _)| (item_id, edge))
}

fn row_at_y(rows: &[TrackRowLayout], y: f32) -> Option<(usize, f32)> {
    if !y.is_finite() || y < 0.0 {
        return None;
    }
    rows.iter().enumerate().find_map(|(index, row)| {
        (y >= row.top && y < row.top + row.height).then_some((index, y - row.top))
    })
}

fn track_index_at_y(y: f32, rows: &[TrackRowLayout]) -> Option<usize> {
    row_at_y(rows, y).map(|(index, _)| index)
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
        .height(Length::Fixed(state.content_height()))
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
        .height(Length::Fixed(state.content_height()))
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
        if let Some(selection) = self.state.time_selection {
            let tick_x = |tick: u64| {
                (i128::from(tick) - i128::from(self.state.origin_tick)) as f64
                    * f64::from(self.state.pixels_per_tick)
            };
            let start_x = tick_x(selection.start_tick);
            let end_x = tick_x(selection.end_tick);
            frame.fill_rectangle(
                Point::new(start_x as f32, 0.0),
                Size::new((end_x - start_x) as f32, bounds.height),
                Color::from_rgba8(63, 126, 147, 0.18),
            );
            for x in [start_x, end_x] {
                let path = canvas::Path::line(
                    Point::new(x as f32, 0.0),
                    Point::new(x as f32, bounds.height),
                );
                frame.stroke(
                    &path,
                    canvas::Stroke::default()
                        .with_color(Color::from_rgb8(110, 181, 195))
                        .with_width(1.5),
                );
            }
        }
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
        for (track_index, track_id) in self.state.cache.track_ids.iter().enumerate() {
            let Some(row) = self.state.row_layout(track_index) else {
                continue;
            };
            let mut fx_lanes = self
                .state
                .fx_automation_lanes
                .iter()
                .filter(|(id, _, _)| id == track_id)
                .copied()
                .collect::<Vec<_>>();
            fx_lanes.sort_by_key(|(_, chain_index, parameter_id)| (*chain_index, *parameter_id));
            for (lane_index, (_, chain_index, parameter_id)) in fx_lanes.into_iter().enumerate() {
                let key = (*track_id, chain_index, parameter_id);
                let parameter_name = self
                    .state
                    .fx_automation_lane_info
                    .get(&key)
                    .map_or_else(|| format!("P{parameter_id}"), |info| info.name.clone());
                frame.fill_text(Text {
                    content: format!("FX {} · {}", chain_index + 1, parameter_name),
                    position: Point::new(
                        4.0,
                        row.top
                            + row.base_height
                            + lane_index as f32 * FX_AUTOMATION_LANE_HEIGHT
                            + FX_AUTOMATION_LANE_HEIGHT / 2.0,
                    ),
                    max_width: 90.0,
                    color: Color::from_rgb8(177, 205, 236),
                    size: Pixels(8.0),
                    line_height: LineHeight::Relative(1.0),
                    font: Font::default(),
                    align_x: TextAlignment::Left,
                    align_y: iced::alignment::Vertical::Center,
                    shaping: Shaping::Basic,
                });
            }
            if !self.state.volume_automation_tracks.contains(track_id)
                || self
                    .state
                    .hidden_volume_automation_tracks
                    .contains(track_id)
            {
                continue;
            }
            for (label, y) in [("+6 dB", 67.0), ("-60", 82.0)] {
                frame.fill_text(Text {
                    content: label.to_owned(),
                    position: Point::new(4.0, row.top + y),
                    max_width: 34.0,
                    color: Color::from_rgb8(137, 202, 172),
                    size: Pixels(8.0),
                    line_height: LineHeight::Relative(1.0),
                    font: Font::default(),
                    align_x: TextAlignment::Left,
                    align_y: iced::alignment::Vertical::Center,
                    shaping: Shaping::Basic,
                });
            }
        }
        for item in self.state.cache.items.iter() {
            let preview = self
                .state
                .drag_preview
                .filter(|_| self.state.selected_items.contains(&item.id));
            let trim = self
                .state
                .item_trim_preview
                .filter(|trim| trim.item_id == item.id);
            let (start_tick, end_tick, track_index) = if let Some(trim) = trim {
                (trim.start_tick, trim.end_tick, item.track_index as i128)
            } else if let Some(preview) = preview {
                let start_tick = u64::try_from(i128::from(item.start_tick) + preview.delta_ticks)
                    .unwrap_or(item.start_tick);
                let end_tick = u64::try_from(i128::from(item.end_tick) + preview.delta_ticks)
                    .unwrap_or(item.end_tick);
                let track_index = i128::try_from(item.track_index).unwrap_or(i128::MAX)
                    + i128::from(preview.track_delta);
                (start_tick, end_tick, track_index)
            } else {
                (
                    item.start_tick,
                    item.end_tick,
                    i128::try_from(item.track_index).unwrap_or(i128::MAX),
                )
            };
            let Some(row) = usize::try_from(track_index)
                .ok()
                .and_then(|index| self.state.row_layout(index))
            else {
                continue;
            };
            if row.top + row.height < self.state.vertical_scroll
                || row.top > self.state.vertical_scroll + self.state.viewport_height
            {
                continue;
            }
            let left = (i128::from(start_tick) - i128::from(self.state.origin_tick)) as f64
                * f64::from(self.state.pixels_per_tick);
            let right = (i128::from(end_tick) - i128::from(self.state.origin_tick)) as f64
                * f64::from(self.state.pixels_per_tick);
            let width = right - left;
            if right < 0.0 || left > f64::from(bounds.width) || width < 54.0 {
                continue;
            }
            frame.fill_text(Text {
                content: item.label.clone(),
                position: Point::new(
                    (left.max(0.0) + 5.0) as f32,
                    row.top
                        + if item.media_ref.as_ref().is_some_and(|media_ref| {
                            self.state.audio_waveforms.contains_key(media_ref)
                        }) {
                            16.0
                        } else {
                            TIMELINE_ROW_HEIGHT / 2.0
                        },
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

fn automation_point_at(
    project: &Project,
    pixels_per_tick: f32,
    track_index: usize,
    tick: u64,
    y: f32,
) -> Option<usize> {
    let track = project.tracks().get(track_index)?;
    track
        .volume_automation()
        .iter()
        .enumerate()
        .filter_map(|(index, point)| {
            let point_tick = project.tick_at_sample(point.sample()).ok()?;
            let point_y = 66.0 + (6.0 - point.gain_db()) * (16.0 / 66.0);
            let x_distance = (i128::from(point_tick) - i128::from(tick)).unsigned_abs() as f64
                * f64::from(pixels_per_tick);
            ((x_distance <= 7.0) && (point_y - y).abs() <= 7.0)
                .then_some((index, x_distance + f64::from((point_y - y).abs())))
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(index, _)| index)
}

fn fx_value_range(values: impl Iterator<Item = f64>) -> (f64, f64) {
    let (min, max) = values.fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), value| {
        (min.min(value), max.max(value))
    });
    if !min.is_finite() || !max.is_finite() {
        return (-1.0, 1.0);
    }
    if max > min {
        let margin = (max - min) * 0.1;
        (min - margin, max + margin)
    } else {
        (min - 1.0, max + 1.0)
    }
}

fn fx_value_at_y(range: (f64, f64), y: f32, stepped: bool, top: f32, bottom: f32) -> f64 {
    let top = f64::from(top);
    let bottom = f64::from(bottom);
    let value =
        range.1 - (f64::from(y).clamp(top, bottom) - top) / (bottom - top) * (range.1 - range.0);
    if stepped { value.round() } else { value }
}

fn fx_automation_point_at(
    project: &Project,
    pixels_per_tick: f32,
    target: FxAutomationTarget,
    tick: u64,
    y: f32,
    configured_range: Option<(f64, f64)>,
    band: (f32, f32),
) -> Option<usize> {
    let plugin = project
        .tracks()
        .get(target.track_index)?
        .fx_chain()
        .get(target.chain_index)?;
    let lane = plugin.parameter_automation_for(target.parameter_id)?;
    let range = configured_range.unwrap_or_else(|| {
        fx_value_range(
            lane.points()
                .iter()
                .map(|point| point.value())
                .chain(plugin.parameter_value(target.parameter_id)),
        )
    });
    lane.points()
        .iter()
        .enumerate()
        .filter_map(|(index, point)| {
            let point_tick = project.tick_at_sample(point.sample()).ok()?;
            let x_distance = i128::from(point_tick).abs_diff(i128::from(tick)) as f64
                * f64::from(pixels_per_tick);
            let point_y = band.0
                + ((range.1 - point.value()) / (range.1 - range.0) * f64::from(band.1 - band.0))
                    as f32;
            ((x_distance <= 7.0) && (point_y - y).abs() <= 7.0)
                .then_some((index, x_distance + f64::from((point_y - y).abs())))
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(index, _)| index)
}

fn snap_tick_to_grid(tick: u64, grid: Option<u64>, enabled: bool, ignore_snap: bool) -> u64 {
    let Some(grid) = grid.filter(|grid| enabled && !ignore_snap && *grid > 0) else {
        return tick;
    };
    let grid = u128::from(grid);
    (((u128::from(tick) + grid / 2) / grid) * grid).min(u128::from(u64::MAX)) as u64
}

fn fx_automation_tick_at(tick: u64, grid: Option<u64>, enabled: bool, ignore_snap: bool) -> u64 {
    snap_tick_to_grid(tick, grid, enabled, ignore_snap)
}

fn time_selection_edge_at_tick(
    selection: TimeSelection,
    tick: u64,
    pixels_per_tick: f32,
) -> Option<TimeSelectionEdge> {
    let pixels_per_tick = f64::from(pixels_per_tick);
    let start_distance =
        i128::from(tick).abs_diff(i128::from(selection.start_tick)) as f64 * pixels_per_tick;
    let end_distance =
        i128::from(tick).abs_diff(i128::from(selection.end_tick)) as f64 * pixels_per_tick;
    let start_hit = start_distance <= TIME_SELECTION_EDGE_HIT_RADIUS_PX;
    let end_hit = end_distance <= TIME_SELECTION_EDGE_HIT_RADIUS_PX;
    match (start_hit, end_hit) {
        (true, true) if start_distance <= end_distance => Some(TimeSelectionEdge::Start),
        (true, true) => Some(TimeSelectionEdge::End),
        (true, false) => Some(TimeSelectionEdge::Start),
        (false, true) => Some(TimeSelectionEdge::End),
        (false, false) => None,
    }
}

fn media_label(media_ref: &str) -> String {
    let file_name = std::path::Path::new(media_ref)
        .file_name()
        .map_or(media_ref, |name| name.to_str().unwrap_or(media_ref));
    file_name.to_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        FX_AUTOMATION_LANE_HEIGHT, ItemKind, PendingTimeSelectionDrag, SnapGrid,
        TIMELINE_ROW_HEIGHT, TimeSelection, TimeSelectionDragMode, TimelineCache, TimelineEvent,
        TimelineState, fx_automation_tick_at, row_at_y, snap_tick_to_grid, tick_at_x,
        time_selection_edge_at_tick,
    };
    use aaadaw_core::{DawAction, Project, ProjectSettings, TimeSignature};
    use aaadaw_media::{AudioStreamDecoder, AudioWaveform};
    use std::collections::HashMap;
    use std::io::Cursor;
    use std::sync::Arc;

    fn project_with_items() -> (Project, [aaadaw_core::TrackId; 3], [aaadaw_core::ItemId; 3]) {
        let mut project = Project::new();
        for (index, name) in ["One", "Two", "Three"].into_iter().enumerate() {
            project
                .apply(DawAction::CreateTrack {
                    index,
                    name: name.to_owned(),
                })
                .unwrap();
        }
        let tracks = [
            project.tracks()[0].id(),
            project.tracks()[1].id(),
            project.tracks()[2].id(),
        ];
        for (track_id, start_tick) in [(tracks[0], 250), (tracks[0], 1_000), (tracks[1], 250)] {
            project
                .apply(DawAction::InsertMidiItem {
                    track_id,
                    start_tick,
                    length_ticks: 240,
                })
                .unwrap();
        }
        let items = [
            project.midi_items()[0].id(),
            project.midi_items()[1].id(),
            project.midi_items()[2].id(),
        ];
        (project, tracks, items)
    }

    #[test]
    fn automation_lane_defaults_visible_for_existing_points_and_can_be_toggled() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Automated".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        project
            .apply(DawAction::SetTrackVolumeAutomation {
                track_id,
                points: vec![aaadaw_core::VolumeAutomationPoint::new(0, -6.0).unwrap()],
            })
            .unwrap();

        let mut timeline = TimelineState::default();
        timeline.rebuild(&project);
        assert!(timeline.volume_automation_tracks.contains(&track_id));
        assert!(!timeline.hidden_volume_automation_tracks.contains(&track_id));

        timeline.handle(TimelineEvent::ToggleVolumeAutomation(track_id));
        assert!(!timeline.volume_automation_tracks.contains(&track_id));
        assert!(timeline.hidden_volume_automation_tracks.contains(&track_id));

        timeline.handle(TimelineEvent::ToggleVolumeAutomation(track_id));
        assert!(timeline.volume_automation_tracks.contains(&track_id));
        assert!(!timeline.hidden_volume_automation_tracks.contains(&track_id));
    }

    #[test]
    fn fx_automation_placement_uses_arrangement_snap_and_shift_bypass() {
        assert_eq!(fx_automation_tick_at(361, Some(240), true, false), 480);
        assert_eq!(fx_automation_tick_at(361, Some(240), true, true), 361);
        assert_eq!(fx_automation_tick_at(361, Some(240), false, false), 361);
    }

    #[test]
    fn multiple_fx_lanes_expand_only_their_track_and_keep_other_lanes_visible() {
        let (project, tracks, _) = project_with_items();
        let mut timeline = TimelineState::default();
        timeline.rebuild(&project);
        for (chain_index, parameter_id) in [(1, 90), (0, 17), (0, 5)] {
            timeline.handle(TimelineEvent::ToggleFxAutomation {
                track_id: tracks[0],
                chain_index,
                parameter_id,
                name: format!("P{parameter_id}"),
                value_range: (0.0, 1.0),
                stepped: false,
            });
        }

        assert_eq!(
            timeline.row_layout(0).unwrap().height,
            TIMELINE_ROW_HEIGHT + 3.0 * FX_AUTOMATION_LANE_HEIGHT
        );
        assert_eq!(
            timeline.row_layout(1).unwrap().top,
            TIMELINE_ROW_HEIGHT + 3.0 * FX_AUTOMATION_LANE_HEIGHT
        );
        assert_eq!(
            row_at_y(
                &timeline.row_layout,
                TIMELINE_ROW_HEIGHT + 2.0 * FX_AUTOMATION_LANE_HEIGHT + 1.0
            ),
            Some((
                0,
                TIMELINE_ROW_HEIGHT + 2.0 * FX_AUTOMATION_LANE_HEIGHT + 1.0
            ))
        );
        assert_eq!(
            row_at_y(
                &timeline.row_layout,
                timeline.row_layout(1).unwrap().top + 0.5
            ),
            Some((1, 0.5))
        );
        assert_eq!(
            timeline
                .fx_automation_lanes
                .iter()
                .filter(|(id, _, _)| *id == tracks[0])
                .count(),
            3
        );

        timeline.handle(TimelineEvent::ToggleFxAutomation {
            track_id: tracks[0],
            chain_index: 0,
            parameter_id: 17,
            name: String::new(),
            value_range: (0.0, 1.0),
            stepped: false,
        });
        assert_eq!(
            timeline.row_layout(0).unwrap().height,
            TIMELINE_ROW_HEIGHT + 2.0 * FX_AUTOMATION_LANE_HEIGHT
        );
        assert!(timeline.fx_automation_lanes.contains(&(tracks[0], 0, 5)));
        assert!(timeline.fx_automation_lanes.contains(&(tracks[0], 1, 90)));
        assert!(!timeline.fx_automation_lanes.contains(&(tracks[0], 0, 17)));
    }

    #[test]
    fn visible_fx_automation_lane_tracks_its_plugin_when_the_chain_is_reordered() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "FX".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        let first = aaadaw_core::TrackFxPlugin::new("vendor.first", "/first.clap").unwrap();
        let second = aaadaw_core::TrackFxPlugin::new("vendor.second", "/second.clap").unwrap();
        project
            .apply(DawAction::SetTrackFxChain {
                track_id,
                plugins: vec![first, second],
            })
            .unwrap();
        project
            .apply(DawAction::SetTrackFxParameterAutomation {
                track_id,
                chain_index: 0,
                parameter_id: 17,
                points: vec![aaadaw_core::FxParameterAutomationPoint::new(0, 0.25).unwrap()],
            })
            .unwrap();

        let mut timeline = TimelineState::default();
        timeline.rebuild(&project);
        timeline.handle(TimelineEvent::ToggleFxAutomation {
            track_id,
            chain_index: 0,
            parameter_id: 17,
            name: "Mix".to_owned(),
            value_range: (0.0, 1.0),
            stepped: false,
        });
        timeline.handle(TimelineEvent::ToggleFxAutomation {
            track_id,
            chain_index: 0,
            parameter_id: 18,
            name: "Resonance".to_owned(),
            value_range: (0.0, 1.0),
            stepped: false,
        });
        timeline.handle(TimelineEvent::SelectFxAutomationPoint {
            track_id,
            chain_index: 0,
            parameter_id: 17,
            index: 0,
        });

        let chain = project.tracks()[0].fx_chain();
        project
            .apply(DawAction::SetTrackFxChain {
                track_id,
                plugins: vec![chain[1].clone(), chain[0].clone()],
            })
            .unwrap();
        timeline.rebuild(&project);

        assert!(timeline.fx_automation_lanes.contains(&(track_id, 1, 17)));
        assert!(timeline.fx_automation_lanes.contains(&(track_id, 1, 18)));
        assert_eq!(timeline.row_layout(0).unwrap().fx_lane_count, 2);
        assert_eq!(
            timeline.selected_fx_automation_point,
            Some((track_id, 1, 17, 0))
        );
        assert_eq!(
            timeline.fx_automation_lane_info[&(track_id, 1, 17)].name,
            "Mix"
        );
    }

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
        cache.rebuild(
            &project,
            &std::collections::HashMap::new(),
            SnapGrid::Sixteenth,
        );
        assert_eq!(cache.items.len(), 2);
        assert_eq!(cache.items[0].track_index, 0);
        assert_eq!(cache.items[0].start_tick, 960);
        assert_eq!(cache.items[0].end_tick, 1920);
        assert!(matches!(cache.items[0].kind, ItemKind::Audio));
        assert_eq!(cache.items[0].label, "kick.wav");
        assert_eq!(cache.items[1].track_index, 1);
        assert_eq!(cache.items[1].start_tick, 1920);
        assert_eq!(cache.items[1].end_tick, 5760);
        assert!(matches!(cache.items[1].kind, ItemKind::Midi));
    }

    #[test]
    fn waveform_geometry_clips_to_audio_source_offset_and_item_duration() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Audio".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        project
            .apply(DawAction::InsertAudioItem {
                track_id,
                media_ref: "asset://waveform".to_owned(),
                start_sample: 1_000,
                source_offset_samples: 256,
                length_samples: 512,
            })
            .unwrap();
        let samples = [
            vec![-16_384; 256],
            vec![8_192; 256],
            vec![16_384; 256],
            vec![0; 256],
        ]
        .concat();
        let bytes = waveform_test_wav(&samples, 48_000);
        let mut decoder = AudioStreamDecoder::from_reader(
            Cursor::new(bytes.clone()),
            Some(bytes.len() as u64),
            Some("wav"),
        )
        .unwrap();
        let waveform = Arc::new(AudioWaveform::decode(&mut decoder, 256).unwrap());
        let waveforms = HashMap::from([("asset://waveform".to_owned(), waveform)]);

        let mut cache = TimelineCache::default();
        cache.rebuild(&project, &waveforms, SnapGrid::Sixteenth);

        assert_eq!(cache.waveform_bins.len(), 2);
        let waveform = &cache.waveform_bins;
        assert_eq!(waveform[0].track_index, 0);
        assert!((waveform[0].min - 0.25).abs() < 1.0e-6);
        assert!((waveform[0].max - 0.25).abs() < 1.0e-6);
        assert!((waveform[1].min - 0.5).abs() < 1.0e-6);
        assert!((waveform[1].max - 0.5).abs() < 1.0e-6);
        assert_eq!(
            waveform[0].start_tick,
            project.tick_at_sample(1_000).unwrap()
        );
        assert_eq!(waveform[1].end_tick, project.tick_at_sample(1_512).unwrap());
    }

    fn waveform_test_wav(samples: &[i16], sample_rate: u32) -> Vec<u8> {
        let data_len = u32::try_from(samples.len() * 2).unwrap();
        let mut bytes = Vec::with_capacity(44 + data_len as usize);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
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
        let timeline = TimelineState {
            origin_tick: 3840,
            pixels_per_tick: 0.1,
            ..TimelineState::default()
        };
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

    #[test]
    fn snap_grids_use_note_dotted_and_triplet_tick_lengths() {
        let intervals = [
            (SnapGrid::Whole, 3_840),
            (SnapGrid::Half, 1_920),
            (SnapGrid::Quarter, 960),
            (SnapGrid::Eighth, 480),
            (SnapGrid::Sixteenth, 240),
            (SnapGrid::ThirtySecond, 120),
            (SnapGrid::DottedHalf, 2_880),
            (SnapGrid::DottedQuarter, 1_440),
            (SnapGrid::DottedEighth, 720),
            (SnapGrid::DottedSixteenth, 360),
            (SnapGrid::HalfTriplet, 1_280),
            (SnapGrid::QuarterTriplet, 640),
            (SnapGrid::EighthTriplet, 320),
            (SnapGrid::SixteenthTriplet, 160),
            (SnapGrid::ThirtySecondTriplet, 80),
        ];
        for (grid, expected_ticks) in intervals {
            assert_eq!(grid.tick_interval(960), Some(expected_ticks), "{grid}");
        }
        assert_eq!(SnapGrid::Sixteenth.tick_interval(961), None);
        assert_eq!(SnapGrid::Quarter.tick_interval(961), Some(961));
    }

    #[test]
    fn default_snap_grid_is_available_in_a_new_empty_project() {
        let timeline = TimelineState::default();
        assert_eq!(timeline.snap_grid, SnapGrid::Sixteenth);
        assert!(timeline.has_snap_grid());
        assert!(timeline.snap_enabled);
    }

    #[test]
    fn selected_grid_survives_meter_changes_and_does_not_toggle_snap() {
        let (mut project, _, _) = project_with_items();
        let mut timeline = TimelineState::default();
        timeline.rebuild(&project);
        timeline.handle(TimelineEvent::SetSnapGrid(SnapGrid::QuarterTriplet));
        assert!(timeline.snap_enabled);
        assert_eq!(timeline.cache.snap_grid_ticks, Some(640));

        project
            .apply(DawAction::SetTimeSignature {
                start_tick: 3_840,
                signature: TimeSignature::new(7, 8).unwrap(),
            })
            .unwrap();
        timeline.rebuild(&project);
        assert_eq!(timeline.snap_grid, SnapGrid::QuarterTriplet);
        assert_eq!(timeline.cache.snap_grid_ticks, Some(640));
        assert!(timeline.snap_enabled);
    }

    #[test]
    fn item_selection_supports_additive_and_shift_range_selection() {
        let (project, _, items) = project_with_items();
        let mut timeline = TimelineState::default();
        timeline.rebuild(&project);

        timeline.handle(TimelineEvent::SelectItem {
            item_id: Some(items[0]),
            additive: false,
            range: false,
        });
        timeline.handle(TimelineEvent::SelectItem {
            item_id: Some(items[1]),
            additive: true,
            range: false,
        });
        assert_eq!(timeline.selected_items.len(), 2);
        assert_eq!(timeline.selected_item, Some(items[1]));

        timeline.handle(TimelineEvent::SelectItem {
            item_id: Some(items[2]),
            additive: false,
            range: true,
        });
        assert_eq!(timeline.selected_items.len(), 2);
        assert!(timeline.selected_items.contains(&items[1]));
        assert!(timeline.selected_items.contains(&items[2]));
        assert!(!timeline.selected_items.contains(&items[0]));
    }

    #[test]
    fn time_selection_is_normalized_and_independent_from_item_selection_and_cursor() {
        let (project, tracks, items) = project_with_items();
        let mut timeline = TimelineState::default();
        timeline.rebuild(&project);
        timeline.handle(TimelineEvent::SelectItem {
            item_id: Some(items[1]),
            additive: false,
            range: false,
        });
        timeline.edit_cursor_tick = 720;
        let selected_items = timeline.selected_items.clone();

        timeline.handle(TimelineEvent::SetTimeSelection {
            start_tick: 960,
            end_tick: 240,
        });
        assert_eq!(
            timeline.time_selection,
            Some(TimeSelection {
                start_tick: 240,
                end_tick: 960,
            })
        );
        assert_eq!(timeline.selected_items, selected_items);
        assert_eq!(timeline.selected_item, Some(items[1]));
        assert_eq!(timeline.selected_track, Some(tracks[0]));
        assert_eq!(timeline.edit_cursor_tick, 720);

        timeline.handle(TimelineEvent::SetTimeSelection {
            start_tick: 480,
            end_tick: 480,
        });
        assert_eq!(timeline.time_selection, None);
        timeline.handle(TimelineEvent::SetTimeSelection {
            start_tick: 100,
            end_tick: 300,
        });
        timeline.handle(TimelineEvent::ClearTimeSelection);
        assert_eq!(timeline.time_selection, None);
        assert_eq!(timeline.selected_items, selected_items);
    }

    #[test]
    fn time_selection_snaps_to_selected_grid_and_shift_bypasses_snap() {
        let (project, _, _) = project_with_items();
        let mut timeline = TimelineState::default();
        timeline.rebuild(&project);

        assert_eq!(
            snap_tick_to_grid(361, timeline.cache.snap_grid_ticks, true, false),
            480
        );
        assert_eq!(
            snap_tick_to_grid(359, timeline.cache.snap_grid_ticks, true, false),
            240
        );
        assert_eq!(
            snap_tick_to_grid(361, timeline.cache.snap_grid_ticks, true, true),
            361
        );

        timeline.handle(TimelineEvent::ToggleSnap);
        assert_eq!(
            snap_tick_to_grid(
                361,
                timeline.cache.snap_grid_ticks,
                timeline.snap_enabled,
                false
            ),
            361
        );
    }

    #[test]
    fn time_selection_edges_are_hit_testable_and_drags_keep_the_opposite_edge_fixed() {
        let selection = TimeSelection {
            start_tick: 240,
            end_tick: 960,
        };
        assert_eq!(
            time_selection_edge_at_tick(selection, 250, 0.5),
            Some(super::TimeSelectionEdge::Start)
        );
        assert_eq!(
            time_selection_edge_at_tick(selection, 950, 0.5),
            Some(super::TimeSelectionEdge::End)
        );
        assert_eq!(time_selection_edge_at_tick(selection, 600, 0.5), None);

        let resize_start = PendingTimeSelectionDrag {
            mode: TimeSelectionDragMode::ResizeStart,
            anchor_tick: 240,
            fixed_tick: selection.end_tick,
            pointer_start_x: 0.0,
            pointer_start_y: 0.0,
            is_dragging: true,
        };
        assert_eq!(resize_start.range_at(120), (120, 960));
        assert_eq!(resize_start.range_at(1_200), (1_200, 960));
    }

    #[test]
    fn item_drag_preview_uses_the_selected_grid_and_translates_selected_tracks() {
        let (project, _, items) = project_with_items();
        let mut timeline = TimelineState::default();
        timeline.rebuild(&project);
        timeline.handle(TimelineEvent::SelectItem {
            item_id: Some(items[1]),
            additive: false,
            range: false,
        });
        timeline.handle(TimelineEvent::SelectItem {
            item_id: Some(items[2]),
            additive: true,
            range: false,
        });
        timeline.handle(TimelineEvent::SetSnapGrid(SnapGrid::Eighth));

        timeline.handle(TimelineEvent::BeginItemDrag {
            item_id: items[1],
            pointer_delta_ticks: 100,
            target_track_index: Some(1),
            range: false,
            ignore_snap: false,
        });
        let preview = timeline.drag_preview().unwrap();
        assert_eq!(preview.delta_ticks, -40);
        assert_eq!(preview.track_delta, 1);
        assert_eq!(preview.target_track_index, Some(1));
        assert!(preview.valid);

        timeline.handle(TimelineEvent::UpdateItemDrag {
            pointer_delta_ticks: -2_000,
            target_track_index: Some(0),
            ignore_snap: false,
        });
        assert!(!timeline.drag_preview().unwrap().valid);
        timeline.handle(TimelineEvent::EndItemDrag);
        assert!(timeline.drag_preview().is_none());
    }

    #[test]
    fn item_drag_without_snap_keeps_pointer_delta_and_unavailable_grid_is_disabled() {
        let (project, _, items) = project_with_items();
        let mut timeline = TimelineState::default();
        timeline.rebuild(&project);
        timeline.handle(TimelineEvent::ToggleSnap);
        timeline.handle(TimelineEvent::BeginItemDrag {
            item_id: items[0],
            pointer_delta_ticks: 101,
            target_track_index: Some(0),
            range: false,
            ignore_snap: false,
        });
        assert_eq!(timeline.drag_preview().unwrap().delta_ticks, 101);

        let settings = ProjectSettings::new(48_000, 961, 120.0).unwrap();
        let project = Project::with_settings(settings);
        let mut unavailable = TimelineState::default();
        unavailable.rebuild(&project);
        assert!(!unavailable.snap_enabled);
        unavailable.handle(TimelineEvent::ToggleSnap);
        assert!(!unavailable.snap_enabled);
        unavailable.handle(TimelineEvent::SetSnapGrid(SnapGrid::Quarter));
        assert!(unavailable.has_snap_grid());
        assert!(!unavailable.snap_enabled);
        unavailable.handle(TimelineEvent::ToggleSnap);
        assert!(unavailable.snap_enabled);
    }

    #[test]
    fn shift_drag_temporarily_ignores_the_snap_toggle() {
        let (project, _, items) = project_with_items();
        let mut timeline = TimelineState::default();
        timeline.rebuild(&project);
        timeline.handle(TimelineEvent::BeginItemDrag {
            item_id: items[0],
            pointer_delta_ticks: 101,
            target_track_index: Some(0),
            range: false,
            ignore_snap: true,
        });
        assert_eq!(timeline.drag_preview().unwrap().delta_ticks, 101);
    }
}
