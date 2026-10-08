Warning: truncated output (original token count: 60968)
Total output lines: 6297

mod renderer;

use aaadaw_core::{ItemId, Project, TempoCurve, TrackId, VolumeAutomationPoint};
use aaadaw_media::{AudioWaveform, WaveformPeak};
use aaadaw_storage::{
    ArrangementViewState, FxAutomationLaneViewState, VolumeAutomationLaneViewState,
};
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
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// Compact TCP layouts shorten horizontal content but keep controls at a shared vertical minimum.
pub(crate) const TIMELINE_ROW_HEIGHT: f32 = 128.0;
pub(crate) const FX_AUTOMATION_LANE_HEIGHT: f32 = 24.0;
const MIN_FX_AUTOMATION_LANE_HEIGHT: f32 = 20.0;
const MAX_FX_AUTOMATION_LANE_HEIGHT: f32 = 192.0;
const MAX_MIDI_ITEM_PREVIEW_NOTES: usize = 512;
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

    pub(crate) fn tick_interval(self, ppq: u32) -> Option<u64> {
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
    FitProjectToView {
        viewport_width: f32,
    },
    FitSelectionToView {
        viewport_width: f32,
    },
    FitSelectedItemsToView {
        viewport_width: f32,
    },
    #[cfg(feature = "audio-device")]
    SetFollowPlayhead {
        enabled: bool,
        viewport_width: f32,
    },
    SelectItem {
        item_id: Option<ItemId>,
        additive: bool,
        range: bool,
    },
    SelectItemsInMarquee {
        start_tick: u64,
        end_tick: u64,
        top: f32,
        bottom: f32,
        additive: bool,
    },
    OpenItemContextMenu {
        item_id: ItemId,
        x: f32,
        y: f32,
    },
    CloseItemContextMenu,
    OpenVolumeAutomationPointMenu {
        track_id: TrackId,
        index: usize,
        x: f32,
        y: f32,
    },
    OpenFxAutomationPointMenu {
        track_id: TrackId,
        chain_index: usize,
        parameter_id: u32,
        index: usize,
        x: f32,
        y: f32,
    },
    CloseAutomationPointMenu,
    ToggleVolumeAutomation(TrackId),
    ToggleFxAutomation {
        track_id: TrackId,
        chain_index: usize,
        parameter_id: u32,
        name: String,
        value_range: (f64, f64),
        stepped: bool,
    },
    ResizeFxAutomationLane {
        track_id: TrackId,
        chain_index: usize,
        parameter_id: u32,
        height: f32,
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
    SetEditCursor(u64),
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
        copy: bool,
    },
    UpdateItemDrag {
        pointer_delta_ticks: i128,
        target_track_index: Option<usize>,
        ignore_snap: bool,
        copy: bool,
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
    SelectTrackWithModifiers {
        track_id: TrackId,
        modifiers: keyboard::Modifiers,
    },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutomationPointContext {
    Volume {
        track_id: TrackId,
        index: usize,
    },
    Fx {
        track_id: TrackId,
        chain_index: usize,
        parameter_id: u32,
        index: usize,
    },
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
    midi_notes: Vec<MidiNotePreview>,
    start_sample: u64,
    length_samples: u64,
    source_offset_samples: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MidiNotePreview {
    tick: u64,
    duration: u64,
    pitch: u8,
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

fn slowest_tempo_in_viewport(project: &Project, start_tick: u64, end_tick: u64) -> f64 {
    let mut slowest_tempo = project.tempo_at_tick(start_tick);
    let mut previous_curve = None;
    for (tempo_tick, bpm, curve) in project.tempo_points() {
        if tempo_tick > start_tick
            && tempo_tick <= end_tick
            && (tempo_tick < end_tick || previous_curve != Some(TempoCurve::Step))
        {
            slowest_tempo = slowest_tempo.min(bpm);
        }
        previous_curve = Some(curve);
    }
    slowest_tempo
}

#[derive(Debug, Default, Clone)]
struct TimelineCache {
    generation: u64,
    items: Arc<[TimelineItem]>,
    item_indices: HashMap<ItemId, usize>,
    track_ids: Arc<[TrackId]>,
    ppq: u32,
    snap_grid_ticks: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WaveformGeometryKey {
    project_generation: u64,
    origin_tick: u64,
    pixels_per_tick: u32,
    width: u32,
}

#[derive(Debug, Default)]
struct CachedWaveformGeometry {
    key: Option<WaveformGeometryKey>,
    generation: u64,
    bins: Arc<[WaveformBinGeometry]>,
}

impl TimelineCache {
    fn rebuild(&mut self, project: &Project, snap_grid: SnapGrid) {
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
                midi_notes: Vec::new(),
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
                label: item.name().to_owned(),
                media_ref: None,
                midi_notes: item
                    .notes()
                    .iter()
                    .filter_map(|note| {
                        let start_tick = note.tick();
                        let end_tick = start_tick.checked_add(note.duration())?;
                        let source_start_tick = item.source_offset_ticks();
                        let source_end_tick = source_start_tick.saturating_add(item.length_ticks());
                        (source_start_tick..source_end_tick)
                            .contains(&start_tick)
                            .then(|| MidiNotePreview {
                                tick: start_tick - source_start_tick,
                                duration: end_tick.min(source_end_tick) - start_tick,
                                pitch: note.pitch(),
                            })
                    })
                    .step_by(
                        item.notes()
                            .len()
                            .div_ceil(MAX_MIDI_ITEM_PREVIEW_NOTES)
                            .max(1),
                    )
                    .take(MAX_MIDI_ITEM_PREVIEW_NOTES)
                    .collect(),
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
    }

    fn waveform_geometry_for_viewport(
        &self,
        project: &Project,
        waveforms: &HashMap<String, Arc<AudioWaveform>>,
        origin_tick: u64,
        pixels_per_tick: f32,
        width: f32,
    ) -> Vec<WaveformBinGeometry> {
        if !pixels_per_tick.is_finite() || pixels_per_tick <= 0.0 || !width.is_finite() {
            return Vec::new();
        }
        let visible_end_tick = origin_tick.saturating_add(
            (f64::from(width.max(0.0)) / f64::from(pixels_per_tick))
                .ceil()
                .clamp(0.0, u64::MAX as f64) as u64,
        );
        if visible_end_tick <= origin_tick {
            return Vec::new();
        }
        let project_rate = u128::from(project.settings().sample_rate());
        let mut bins: Vec<WaveformBinGeometry> = Vec::new();
        for item in self.items.iter().filter(|item| {
            item.kind == ItemKind::Audio
                && item.start_tick < visible_end_tick
                && item.end_tick > origin_tick
        }) {
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
                (u128::from(item.length_samples) * source_rate).div_ceil(project_rate);
            let Some(item_source_end) =
                u64::try_from(u128::from(item.source_offset_samples) + source_length).ok()
            else {
                continue;
            };
            let visible_start_tick = item.start_tick.max(origin_tick);
            let visible_end_tick = item.end_tick.min(visible_end_tick);
            let Ok(visible_start_sample) = project.sample_at_tick(visible_start_tick) else {
                continue;
            };
            let item_end_sample = item.start_sample.saturating_add(item.length_samples);
            let visible_end_sample = if visible_end_tick == item.end_tick {
                item_end_sample
            } else if let Ok(sample) = project.sample_at_tick(visible_end_tick) {
                sample
            } else {
                continue;
            };
            let visible_start_sample = visible_start_sample.max(item.start_sample);
            let visible_end_sample = visible_end_sample.min(item_end_sample);
            if visible_start_sample >= visible_end_sample {
                continue;
            }
            let source_start = u64::try_from(
                u128::from(item.source_offset_samples)
                    + (u128::from(visible_start_sample - item.start_sample) * source_rate)
                        / project_rate,
            )
            .unwrap_or(item_source_end)
            .min(item_source_end)
            .min(waveform.frame_count());
            let source_end = u64::try_from(
                u128::from(item.source_offset_samples)
                    + (u128::from(visible_end_sample - item.start_sample) * source_rate)
                        .div_ceil(project_rate),
            )
            .unwrap_or(item_source_end)
            .min(item_source_end)
            .min(waveform.frame_count());
            if source_start >= source_end {
                continue;
            }
            let slowest_tempo =
                slowest_tempo_in_viewport(project, visible_start_tick, visible_end_tick);
            let frames_per_pixel = (f64::from(waveform.sample_rate()) * 60.0
                / (slowest_tempo
                    * f64::from(project.settings().ppq())
                    * f64::from(pixels_per_tick)))
            .ceil()
            .clamp(1.0, f64::from(u32::MAX)) as u32;
            let (level, peak_range) =
                waveform.peak_range_for_source_frames(source_start, source_end, frames_per_pixel);
            let frames_per_peak = u64::from(level.frames_per_peak());
            let mut append_peak = |peak: WaveformPeak, overlap_start: u64, overlap_end: u64| {
                if overlap_start >= overlap_end {
                    return;
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
                    return;
                };
                let Some(end_sample) =
                    u64::try_from(u128::from(item.start_sample) + end_delta).ok()
                else {
                    return;
                };
                let start_sample = start_sample.min(item_end_sample);
                let end_sample = end_sample.min(item_end_sample);
                if start_sample >= end_sample {
                    return;
                }
                let (Ok(start_tick), Ok(end_tick)) = (
                    project.tick_at_sample(start_sample),
                    project.tick_at_sample(end_sample),
                ) else {
                    return;
                };
                let start_tick = start_tick.max(origin_tick);
                let mut end_tick = end_tick.min(visible_end_tick);
                if start_tick >= end_tick {
                    if let Some(previous) = bins.last_mut().filter(|previous| {
                        previous.item_id == item.id
                            && previous.track_index == item.track_index
                            && previous.start_tick <= start_tick
                            && previous.end_tick >= start_tick
                    }) {
                        previous.min = previous.min.min(peak.min);
                        previous.max = previous.max.max(peak.max);
                        return;
                    }
                    end_tick = start_tick.saturating_add(1).min(visible_end_tick);
                    if start_tick >= end_tick {
                        return;
                    }
                }
                bins.push(WaveformBinGeometry {
                    item_id: item.id,
                    start_tick,
                    end_tick,
                    track_index: item.track_index,
                    min: peak.min,
                    max: peak.max,
                });
            };
            for peak_index in peak_range {
                let peak = level.peaks()[peak_index];
                let peak_start = (peak_index as u64).saturating_mul(frames_per_peak);
                let peak_end = peak_start
                    .saturating_add(frames_per_peak)
                    .min(waveform.frame_count());
                let overlap_start = peak_start.max(source_start).max(item.source_offset_samples);
                let overlap_end = peak_end.min(source_end).min(item_source_end);
                if overlap_start >= overlap_end {
                    continue;
                }
                if frames_per_peak > u64::from(waveform.frames_per_peak())
                    && (overlap_start > peak_start || overlap_end < peak_end)
                {
                    let refinement_frames = ((overlap_end - overlap_start).div_ceil(8))
                        .max(u64::from(waveform.frames_per_peak()))
                        .min(u64::from(u32::MAX))
                        as u32;
                    let (base_level, base_range) = waveform.peak_range_for_source_frames(
                        overlap_start,
                        overlap_end,
                        refinement_frames,
                    );
                    let base_frames_per_peak = u64::from(base_level.frames_per_peak());
                    const MAX_BOUNDARY_REFINEMENT_PEAKS: usize = 32;
                    if base_range.len() > MAX_BOUNDARY_REFINEMENT_PEAKS {
                        // Never reuse a coarse summary here: it may contain audio beyond
                        // the clip trim. The adaptive target keeps this bounded in normal
                        // pyramids; skip an edge whose summaries cannot be refined safely.
                        continue;
                    }
                    let mut refined_min = f32::INFINITY;
                    let mut refined_max = f32::NEG_INFINITY;
                    let mut refined_start = None;
                    let mut refined_end = None;
                    for base_index in base_range {
                        let base_start = (base_index as u64).saturating_mul(base_frames_per_peak);
                        let base_end = base_start
                            .saturating_add(base_frames_per_peak)
                            .min(waveform.frame_count());
                        // Peak summaries cannot be clipped to individual samples. Omit a
                        // partial base bin at a trim edge rather than leaking audio from
                        // outside the clip into its waveform.
                        if base_start < item.source_offset_samples || base_end > item_source_end {
                            continue;
                        }
                        let clipped_start = base_start.max(overlap_start);
                        let clipped_end = base_end.min(overlap_end);
                        if clipped_start >= clipped_end {
                            continue;
                        }
                        let peak = base_level.peaks()[base_index];
                        refined_min = refined_min.min(peak.min);
                        refined_max = refined_max.max(peak.max);
                        refined_start.get_or_insert(clipped_start);
                        refined_end = Some(clipped_end);
                    }
                    if let (Some(refined_start), Some(refined_end)) = (refined_start, refined_end) {
                        append_peak(
                            WaveformPeak {
                                min: refined_min,
                                max: refined_max,
                            },
                            refined_start,
                            refined_end,
                        );
                    }
                } else if frames_per_peak == u64::from(waveform.frames_per_peak())
                    && (peak_start < item.source_offset_samples || peak_end > item_source_end)
                {
                    // A base-level summary still contains samples outside an unaligned trim.
                    continue;
                } else {
                    append_peak(peak, overlap_start, overlap_end);
                }
            }
        }
        bins
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
    pub(crate) selected_tracks: HashSet<TrackId>,
    pub(crate) selected_item: Option<ItemId>,
    pub(crate) selected_items: HashSet<ItemId>,
    pub(crate) follow_playhead: bool,
    viewport_width: f32,
    pub(crate) time_selection: Option<TimeSelection>,
    pub(crate) context_track: Option<TrackId>,
    pub(crate) context_item: Option<ItemId>,
    pub(crate) context_item_position: Option<(f32, f32)>,
    pub(crate) context_automation_point: Option<AutomationPointContext>,
    pub(crate) context_automation_position: Option<(f32, f32)>,
    audio_waveforms: HashMap<String, Arc<AudioWaveform>>,
    pub(crate) snap_enabled: bool,
    pub(crate) snap_grid: SnapGrid,
    pub(crate) edit_cursor_tick: u64,
    pub(crate) origin_tick: u64,
    pub(crate) pixels_per_tick: f32,
    cache: TimelineCache,
    waveform_source_generation: u64,
    waveform_geometry_cache: Arc<Mutex<CachedWaveformGeometry>>,
    pan_fractional_tick: f64,
    selection_anchor: Option<ItemId>,
    track_selection_anchor: Option<TrackId>,
    drag_preview: Option<ItemDragPreview>,
    item_trim_preview: Option<ItemTrimPreview>,
    pub(crate) volume_automation_tracks: HashSet<TrackId>,
    pub(crate) hidden_volume_automation_tracks: HashSet<TrackId>,
    selected_volume_automation_point: Option<(TrackId, usize)>,
    pub(crate) fx_automation_lanes: HashSet<(TrackId, usize, u32)>,
    fx_automation_lane_info: HashMap<(TrackId, usize, u32), FxAutomationLaneInfo>,
    fx_automation_lane_heights: HashMap<(TrackId, usize, u32), f32>,
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

#[derive(Debug, Clone, Copy, PartialEq)]
struct FxAutomationBand {
    target: (TrackId, usize, u32),
    lane_index: usize,
    top: f32,
    height: f32,
}

fn fx_automation_bands_for_track(
    track_id: TrackId,
    lanes: &HashSet<(TrackId, usize, u32)>,
    heights: &HashMap<(TrackId, usize, u32), f32>,
) -> Vec<FxAutomationBand> {
    let mut lanes = lanes
        .iter()
        .copied()
        .filter(|(id, _, _)| *id == track_id)
        .collect::<Vec<_>>();
    lanes.sort_unstable_by_key(|(_, chain_index, parameter_id)| (*chain_index, *parameter_id));
    let mut top = TIMELINE_ROW_HEIGHT;
    lanes
        .into_iter()
        .enumerate()
        .map(|(lane_index, target)| {
            let height = heights
                .get(&target)
                .copied()
                .filter(|height| height.is_finite())
                .unwrap_or(FX_AUTOMATION_LANE_HEIGHT)
                .clamp(MIN_FX_AUTOMATION_LANE_HEIGHT, MAX_FX_AUTOMATION_LANE_HEIGHT);
            let band = FxAutomationBand {
                target,
                lane_index,
                top,
                height,
            };
            top += height;
            band
        })
        .collect()
}

fn fx_automation_band_at_y(
    track_id: TrackId,
    row_y: f32,
    lanes: &HashSet<(TrackId, usize, u32)>,
    heights: &HashMap<(TrackId, usize, u32), f32>,
) -> Option<FxAutomationBand> {
    fx_automation_bands_for_track(track_id, lanes, heights)
        .into_iter()
        .find(|band| row_y >= band.top && row_y < band.top + band.height)
}

fn fx_automation_lane_resize_target(
    track_id: TrackId,
    row_y: f32,
    lanes: &HashSet<(TrackId, usize, u32)>,
    heights: &HashMap<(TrackId, usize, u32), f32>,
) -> Option<FxAutomationBand> {
    fx_automation_bands_for_track(track_id, lanes, heights)
        .into_iter()
        .find(|band| (row_y - (band.top + band.height)).abs() <= 3.0)
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
    pub(crate) copy: bool,
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
        cache.rebuild(&project, SnapGrid::Sixteenth);
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
            selected_tracks: HashSet::new(),
            selected_item: None,
            selected_items: HashSet::new(),
            follow_playhead: false,
            viewport_width: 480.0,
            time_selection: None,
            context_track: None,
            context_item: None,
            context_item_position: None,
            context_automation_point: None,
            context_automation_position: None,
            audio_waveforms: HashMap::new(),
            snap_enabled: true,
            snap_grid: SnapGrid::Sixteenth,
            edit_cursor_tick: 0,
            origin_tick: 0,
            pixels_per_tick: 0.065,
            cache,
            waveform_source_generation: 0,
            waveform_geometry_cache: Arc::default(),
            pan_fractional_tick: 0.0,
            selection_anchor: None,
            track_selection_anchor: None,
            drag_preview: None,
            item_trim_preview: None,
            volume_automation_tracks: HashSet::new(),
            hidden_volume_automation_tracks: HashSet::new(),
            selected_volume_automation_point: None,
            fx_automation_lanes: HashSet::new(),
            fx_automation_lane_info: HashMap::new(),
            fx_automation_lane_heights: HashMap::new(),
            selected_fx_automation_point: None,
            fx_chain_snapshot: HashMap::new(),
            fx_lane_snapshot: HashMap::new(),
            row_layout: Vec::new(),
        }
    }
}

impl TimelineState {
    pub(crate) fn replace_project(
        &mut self,
        project: &Project,
        view_state: Option<&ArrangementViewState>,
    ) {
        self.volume_automation_tracks.clear();
        self.hidden_volume_automation_tracks.clear();
        self.selected_volume_automation_point = None;
        self.fx_automation_lanes.clear();
        self.fx_automation_lane_info.clear();
        self.fx_automation_lane_heights.clear();
        self.selected_fx_automation_point = None;
        self.context_automation_point = None;
        self.context_automation_position = None;
        self.fx_chain_snapshot.clear();
        self.fx_lane_snapshot.clear();
        self.selected_track = None;
        self.selected_tracks.clear();
        self.track_selection_anchor = None;
        self.rebuild(project);

        if let Some(view_state) = view_state {
            let saved_volume_visibility = view_state
                .volume_lanes
                .iter()
                .map(|lane| (lane.track_id, lane.visible))
                .collect::<HashMap<_, _>>();
            self.volume_automation_tracks.clear();
            self.hidden_volume_automation_tracks.clear();
            for track in project.tracks() {
                match saved_volume_visibility.get(&track.id().value()) {
                    Some(true) => {
                        self.volume_automation_tracks.insert(track.id());
                    }
                    Some(false) => {
                        self.hidden_volume_automation_tracks.insert(track.id());
                    }
                    None if !track.volume_automation().is_empty() => {
                        self.volume_automation_tracks.insert(track.id());
                    }
                    None => {}
                }
            }

            for lane in &view_state.fx_lanes {
                let Some(track) = project
                    .tracks()
                    .iter()
                    .find(|track| track.id().value() == lane.track_id)
                else {
                    continue;
                };
                let Some(plugin) = track.fx_chain().get(lane.chain_index) else {
                    continue;
                };
                if plugin.plugin_id() != lane.plugin_id || plugin.bundle_path() != lane.bundle_path
                {
                    continue;
                }
                let target = (track.id(), lane.chain_index, lane.parameter_id);
                self.fx_automation_lanes.insert(target);
                self.fx_automation_lane_info.insert(
                    target,
                    FxAutomationLaneInfo {
                        name: lane.name.clone(),
                        value_range: lane.value_range,
                        stepped: lane.stepped,
                    },
                );
                if lane.height.is_finite() {
                    self.fx_automation_lane_heights.insert(
                        target,
                        lane.height
                            .clamp(MIN_FX_AUTOMATION_LANE_HEIGHT, MAX_FX_AUTOMATION_LANE_HEIGHT),
                    );
                }
            }
        }
        self.rebuild_row_layout();
        self.cache.generation = self.cache.generation.wrapping_add(1);
    }

    pub(crate) fn arrangement_view_state(&self, project: &Project) -> ArrangementViewState {
        let volume_lanes = project
            .tracks()
            .iter()
            .map(|track| VolumeAutomationLaneViewState {
                track_id: track.id().value(),
                visible: self.volume_automation_tracks.contains(&track.id())
                    || (!self.hidden_volume_automation_tracks.contains(&track.id())
                        && !track.volume_automation().is_empty()),
            })
            .collect();

        let mut fx_lanes = self
            .fx_automation_lanes
            .iter()
            .filter_map(|(track_id, chain_index, parameter_id)| {
                let track = project
                    .tracks()
                    .iter()
                    .find(|track| track.id() == *track_id)?;
                let plugin = track.fx_chain().get(*chain_index)?;
                let info =
                    self.fx_automation_lane_info
                        .get(&(*track_id, *chain_index, *parameter_id));
                Some(FxAutomationLaneViewState {
                    track_id: track_id.value(),
                    chain_index: *chain_index,
                    plugin_id: plugin.plugin_id().to_owned(),
                    bundle_path: plugin.bundle_path().to_owned(),
                    parameter_id: *parameter_id,
                    name: info.map_or_else(
                        || format!("Parameter {parameter_id}"),
                        |info| info.name.clone(),
                    ),
                    value_range: info.map_or((0.0, 1.0), |info| info.value_range),
                    stepped: info.is_some_and(|info| info.stepped),
                    height: self.fx_automation_lane_height((
                        *track_id,
                        *chain_index,
                        *parameter_id,
                    )),
                })
            })
            .collect::<Vec<_>>();
        fx_lanes.sort_by_key(|lane| {
            (
                project
                    .tracks()
                    .iter()
                    .position(|track| track.id().value() == lane.track_id)
                    .unwrap_or(usize::MAX),
                lane.chain_index,
                lane.parameter_id,
            )
        });
        ArrangementViewState {
            volume_lanes,
            fx_lanes,
        }
    }

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
        self.cache.rebuild(project, self.snap_grid);
        self.waveform_source_generation = self.waveform_source_generation.wrapping_add(1);
        self.rebuild_row_layout();
        if self.cache.snap_grid_ticks.is_none() {
            self.snap_enabled = false;
        }
        self.selected_tracks
            .retain(|track_id| self.cache.track_ids.contains(track_id));
        if self
            .selected_track
            .is_some_and(|track_id| !self.cache.track_ids.contains(&track_id))
        {
            self.selected_track = self
                .cache
                .track_ids
                .iter()
                .copied()
                .find(|track_id| self.selected_tracks.contains(track_id));
        }
        if self
            .track_selection_anchor
            .is_some_and(|track_id| !self.cache.track_ids.contains(&track_id))
        {
            self.track_selection_anchor = None;
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
        let mut height_remap = HashMap::new();
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
                if let Some(height) =
                    self.fx_automation_lane_heights
                        .get(&(track_id, old_index, parameter_id))
                {
                    height_remap.insert((track_id, index, parameter_id), *height);
                }
            }
        }
        self.fx_automation_lanes = remapped;
        self.fx_automation_lane_info = info_remap;
        self.fx_automation_lane_heights = height_remap;
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
                let fx_lane_height = self
                    .fx_automation_lanes
                    .iter()
                    .filter(|(id, _, _)| id == track_id)
                    .map(|key| self.fx_automation_lane_height(*key))
                    .sum::<f32>();
                let height = TIMELINE_ROW_HEIGHT + fx_lane_height;
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

    fn fx_automation_lane_height(&self, key: (TrackId, usize, u32)) -> f32 {
        self.fx_automation_lane_heights
            .get(&key)
            .copied()
            .filter(|height| height.is_finite())
            .unwrap_or(FX_AUTOMATION_LANE_HEIGHT)
            .clamp(MIN_FX_AUTOMATION_LANE_HEIGHT, MAX_FX_AUTOMATION_LANE_HEIGHT)
    }

    fn fx_automation_bands(&self, track_id: TrackId) -> Vec<FxAutomationBand> {
        fx_automation_bands_for_track(
            track_id,
            &self.fx_automation_lanes,
            &self.fx_automation_lane_heights,
        )
    }

    pub(crate) fn set_audio_waveforms(
        &mut self,
        _project: &Project,
        waveforms: HashMap<String, Arc<AudioWaveform>>,
    ) {
        self.audio_waveforms = waveforms;
        self.waveform_source_generation = self.waveform_source_generation.wrapping_add(1);
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
            TimelineEvent::FitProjectToView { viewport_width } => {
                let range: Option<(u64, u64)> =
                    self.cache.items.iter().fold(None, |range, item| {
                        Some(match range {
                            Some((start, end)) => {
                                (start.min(item.start_tick), end.max(item.end_tick))
                            }
                            None => (item.start_tick, item.end_tick),
                        })
                    });
                self.fit_tick_range(range, viewport_width);
            }
            TimelineEvent::FitSelectionToView { viewport_width } => {
                let range = self
                    .time_selection
                    .map(|selection| (selection.start_tick, selection.end_tick));
                self.fit_tick_range(range, viewport_width);
            }
            TimelineEvent::FitSelectedItemsToView { viewport_width } => {
                let range: Option<(u64, u64)> = self
                    .cache
                    .items
                    .iter()
                    .filter(|item| self.selected_items.contains(&item.id))
                    .fold(None, |range, item| {
                        Some(match range {
                            Some((start, end)) => {
                                (start.min(item.start_tick), end.max(item.end_tick))
                            }
                            None => (item.start_tick, item.end_tick),
                        })
                    });
                self.fit_tick_range(range, viewport_width);
            }
            #[cfg(feature = "audio-device")]
            TimelineEvent::SetFollowPlayhead {
                enabled,
                viewport_width,
            } => {
                self.follow_playhead = enabled;
                if viewport_width.is_finite() && viewport_width > 1.0 {
                    self.viewport_width = viewport_width;
                }
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
            TimelineEvent::SelectItemsInMarquee {
                start_tick,
                end_tick,
                top,
                bottom,
                additive,
            } => self.select_items_in_marquee(start_tick, end_tick, top, bottom, additive),
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
            TimelineEvent::OpenVolumeAutomationPointMenu {
                track_id,
                index,
                x,
                y,
            } => {
                self.context_automation_point =
                    Some(AutomationPointContext::Volume { track_id, index });
                self.context_automation_position = Some((x, y));
            }
            TimelineEvent::OpenFxAutomationPointMenu {
                track_id,
                chain_index,
                parameter_id,
                index,
                x,
                y,
            } => {
                self.context_automation_point = Some(AutomationPointContext::Fx {
                    track_id,
                    chain_index,
                    parameter_id,
                    index,
                });
                self.context_automation_position = Some((x, y));
            }
            TimelineEvent::CloseAutomationPointMenu => {
                self.context_automation_point = None;
                self.context_automation_position = None;
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
            TimelineEvent::ResizeFxAutomationLane {
                track_id,
                chain_index,
                parameter_id,
                height,
            } => {
                let key = (track_id, chain_index, parameter_id);
                if self.fx_automation_lanes.contains(&key) && height.is_finite() {
                    self.fx_automation_lane_heights.insert(
                        key,
                        height.clamp(MIN_FX_AUTOMATION_LANE_HEIGHT, MAX_FX_AUTOMATION_LANE_HEIGHT),
                    );
                    self.rebuild_row_layout();
                    self.cache.generation = self.cache.generation.wrapping_add(1);
                }
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
            TimelineEvent::SetEditCursor(tick) => self.edit_cursor_tick = tick,
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
            Timeli…30968 tokens truncated…ample_between(8, 4, 9), Some(8));
        assert_eq!(automation_sample_between(2, 4, 9), Some(4));
        assert_eq!(automation_sample_between(8, 9, 4), None);
    }

    #[test]
    fn drag_below_populated_track_rows_creates_a_time_selection() {
        let (project, _, _) = project_with_items();
        let mut timeline = TimelineState {
            origin_tick: 240,
            pixels_per_tick: 0.5,
            ..TimelineState::default()
        };
        timeline.rebuild(&project);
        let program = timeline.program(&project, None);
        let mut interaction = super::TimelineInteractionState::default();
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(800.0, 480.0));
        let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let _ = iced::widget::shader::Program::update(
            &program,
            &mut interaction,
            &press,
            bounds,
            mouse::Cursor::Available(Point::new(100.0, 400.0)),
        )
        .expect("space below populated track rows should start a selection gesture");

        let moved = Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(200.0, 400.0),
        });
        let action = iced::widget::shader::Program::update(
            &program,
            &mut interaction,
            &moved,
            bounds,
            mouse::Cursor::Available(Point::new(200.0, 400.0)),
        )
        .expect("space below populated track rows should support time selection");
        let (message, _, _) = action.into_inner();
        assert!(
            matches!(
                message,
                Some(crate::app::Message::Timeline(
                    TimelineEvent::SetTimeSelection {
                        start_tick: 480,
                        end_tick: 720,
                    }
                ))
            ),
            "unexpected populated-area drag event: {message:?}"
        );
    }

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
    fn arrangement_view_state_restores_lane_layout_and_isolated_between_projects() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Persisted".to_owned(),
            })
            .unwrap();
        let track_id = project.tracks()[0].id();
        project
            .apply(DawAction::SetTrackVolumeAutomation {
                track_id,
                points: vec![aaadaw_core::VolumeAutomationPoint::new(0, -6.0).unwrap()],
            })
            .unwrap();
        project
            .apply(DawAction::SetTrackFxChain {
                track_id,
                plugins: vec![
                    aaadaw_core::TrackFxPlugin::new("vendor.persisted", "/persisted.clap").unwrap(),
                ],
            })
            .unwrap();

        let mut source = TimelineState::default();
        source.rebuild(&project);
        source.handle(TimelineEvent::ToggleVolumeAutomation(track_id));
        source.handle(TimelineEvent::ToggleFxAutomation {
            track_id,
            chain_index: 0,
            parameter_id: 7,
            name: "Mix".to_owned(),
            value_range: (0.0, 1.0),
            stepped: true,
        });
        source.handle(TimelineEvent::ResizeFxAutomationLane {
            track_id,
            chain_index: 0,
            parameter_id: 7,
            height: 48.0,
        });
        let saved_view = source.arrangement_view_state(&project);

        let mut reopened = TimelineState::default();
        reopened.replace_project(&project, Some(&saved_view));
        assert!(reopened.hidden_volume_automation_tracks.contains(&track_id));
        assert!(!reopened.volume_automation_tracks.contains(&track_id));
        assert!(reopened.fx_automation_lanes.contains(&(track_id, 0, 7)));
        assert_eq!(reopened.fx_automation_lane_height((track_id, 0, 7)), 48.0);
        assert!(reopened.fx_automation_lane_info[&(track_id, 0, 7)].stepped);
        assert_eq!(
            reopened.row_layout(0).unwrap().height,
            TIMELINE_ROW_HEIGHT + 48.0
        );

        let mut other_project = Project::new();
        other_project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Other".to_owned(),
            })
            .unwrap();
        let other_track_id = other_project.tracks()[0].id();
        assert_eq!(other_track_id, track_id);
        other_project
            .apply(DawAction::SetTrackFxChain {
                track_id: other_track_id,
                plugins: vec![
                    aaadaw_core::TrackFxPlugin::new("vendor.other", "/other.clap").unwrap(),
                ],
            })
            .unwrap();
        reopened.replace_project(&other_project, None);
        assert!(reopened.fx_automation_lanes.is_empty());
        assert!(reopened.hidden_volume_automation_tracks.is_empty());
        assert_eq!(reopened.row_layout(0).unwrap().height, TIMELINE_ROW_HEIGHT);
    }

    #[test]
    fn fx_automation_lane_bands_drive_hit_testing_and_resizing() {
        let (project, tracks, _) = project_with_items();
        let track_id = tracks[0];
        let lanes = HashSet::from([(track_id, 0, 5), (track_id, 1, 90)]);
        let mut heights = HashMap::new();
        heights.insert((track_id, 0, 5), 40.0);

        let first = fx_automation_band_at_y(track_id, TIMELINE_ROW_HEIGHT + 20.0, &lanes, &heights)
            .unwrap();
        assert_eq!(first.target, (track_id, 0, 5));
        assert_eq!(first.top, TIMELINE_ROW_HEIGHT);
        assert_eq!(first.height, 40.0);
        assert!(
            fx_automation_band_at_y(track_id, TIMELINE_ROW_HEIGHT + 39.0, &lanes, &heights)
                .is_some()
        );
        assert!(
            fx_automation_band_at_y(track_id, TIMELINE_ROW_HEIGHT + 40.0, &lanes, &heights)
                .is_some()
        );
        assert!(
            fx_automation_band_at_y(track_id, TIMELINE_ROW_HEIGHT + 63.9, &lanes, &heights)
                .is_some()
        );

        let resize = fx_automation_lane_resize_target(
            track_id,
            TIMELINE_ROW_HEIGHT + 39.0,
            &lanes,
            &heights,
        )
        .unwrap();
        assert_eq!(resize.target, (track_id, 0, 5));
        assert!(
            fx_automation_lane_resize_target(
                track_id,
                TIMELINE_ROW_HEIGHT + 45.0,
                &lanes,
                &heights
            )
            .is_none()
        );

        let mut timeline = TimelineState::default();
        timeline.rebuild(&project);
        timeline.fx_automation_lanes = lanes;
        timeline.fx_automation_lane_heights = heights;
        timeline.rebuild_row_layout();
        timeline.handle(TimelineEvent::ResizeFxAutomationLane {
            track_id,
            chain_index: 0,
            parameter_id: 5,
            height: 210.0,
        });
        assert_eq!(
            timeline.fx_automation_lane_height((track_id, 0, 5)),
            MAX_FX_AUTOMATION_LANE_HEIGHT
        );
        assert_eq!(
            timeline.row_layout(0).unwrap().height,
            TIMELINE_ROW_HEIGHT + 192.0 + FX_AUTOMATION_LANE_HEIGHT
        );
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
        timeline.handle(TimelineEvent::ResizeFxAutomationLane {
            track_id,
            chain_index: 0,
            parameter_id: 17,
            height: 48.0,
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
        assert_eq!(timeline.fx_automation_lane_height((track_id, 1, 17)), 48.0);
        assert_eq!(
            timeline.selected_fx_automation_point,
            Some((track_id, 1, 17, 0))
        );
        assert_eq!(
            timeline.fx_automation_lane_info[&(track_id, 1, 17)].name,
            "Mix"
        );

        let remaining_plugin = project.tracks()[0].fx_chain()[0].clone();
        project
            .apply(DawAction::SetTrackFxChain {
                track_id,
                plugins: vec![remaining_plugin],
            })
            .unwrap();
        timeline.rebuild(&project);
        assert!(timeline.fx_automation_lanes.is_empty());
        assert_eq!(timeline.row_layout(0).unwrap().height, TIMELINE_ROW_HEIGHT);
        assert_eq!(timeline.selected_fx_automation_point, None);
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
        cache.rebuild(&project, SnapGrid::Sixteenth);
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
    fn track_rows_reserve_the_tcp_control_panel_height() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Audio".to_owned(),
            })
            .unwrap();
        let mut timeline = TimelineState::default();
        timeline.rebuild(&project);

        let row = timeline.row_layout(0).unwrap();
        assert_eq!(row.base_height, TIMELINE_ROW_HEIGHT);
        assert_eq!(row.height, row.base_height);
        assert!(row.base_height >= 128.0);
    }

    #[test]
    fn track_selection_supports_additive_and_range_selection_with_a_primary_track() {
        let mut project = Project::new();
        for index in 0..4 {
            project
                .apply(aaadaw_core::DawAction::CreateTrack {
                    index,
                    name: format!("Track {index}"),
                })
                .unwrap();
        }
        let tracks = project
            .tracks()
            .iter()
            .map(|track| track.id())
            .collect::<Vec<_>>();
        let mut timeline = TimelineState::default();
        timeline.rebuild(&project);

        timeline.handle(TimelineEvent::SelectTrackWithModifiers {
            track_id: tracks[0],
            modifiers: keyboard::Modifiers::NONE,
        });
        timeline.handle(TimelineEvent::SelectTrackWithModifiers {
            track_id: tracks[2],
            modifiers: keyboard::Modifiers::SHIFT,
        });
        assert_eq!(timeline.selected_track, Some(tracks[2]));
        assert_eq!(
            timeline.selected_tracks,
            HashSet::from([tracks[0], tracks[1], tracks[2]])
        );

        timeline.handle(TimelineEvent::SelectTrackWithModifiers {
            track_id: tracks[3],
            modifiers: keyboard::Modifiers::CTRL,
        });
        assert_eq!(timeline.selected_track, Some(tracks[3]));
        assert_eq!(timeline.selected_tracks.len(), 4);
        assert!(timeline.is_track_selected(tracks[1]));

        timeline.handle(TimelineEvent::SelectTrackWithModifiers {
            track_id: tracks[3],
            modifiers: keyboard::Modifiers::COMMAND,
        });
        assert_eq!(timeline.selected_track, Some(tracks[2]));
        assert!(!timeline.is_track_selected(tracks[3]));

        timeline.handle(TimelineEvent::SelectTrackWithModifiers {
            track_id: tracks[2],
            modifiers: keyboard::Modifiers::CTRL,
        });
        timeline.handle(TimelineEvent::SelectTrackWithModifiers {
            track_id: tracks[0],
            modifiers: keyboard::Modifiers::CTRL,
        });
        timeline.handle(TimelineEvent::SelectTrackWithModifiers {
            track_id: tracks[1],
            modifiers: keyboard::Modifiers::SHIFT,
        });
        assert_eq!(timeline.selected_track, Some(tracks[1]));
        assert_eq!(
            timeline.selected_tracks,
            HashSet::from([tracks[0], tracks[1]])
        );

        timeline.handle(TimelineEvent::SelectTrackWithModifiers {
            track_id: tracks[0],
            modifiers: keyboard::Modifiers::CTRL,
        });
        timeline.handle(TimelineEvent::SelectTrackWithModifiers {
            track_id: tracks[1],
            modifiers: keyboard::Modifiers::CTRL,
        });
        assert!(timeline.selected_tracks.is_empty());
        assert_eq!(timeline.selected_track, None);
    }

    #[test]
    fn timeline_cache_keeps_midi_item_names_and_note_preview_content() {
        let mut project = Project::new();
        project
            .apply(aaadaw_core::DawAction::CreateTrack {
                index: 0,
                name: "Keys".to_owned(),
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
            .apply(aaadaw_core::DawAction::SetMidiItemName {
                item_id,
                name: "Opening theme".to_owned(),
            })
            .unwrap();
        project
            .apply(aaadaw_core::DawAction::AddMidiNotes {
                item_id,
                notes: vec![
                    aaadaw_core::MidiNoteData {
                        pitch: 60,
                        tick: 0,
                        duration: 240,
                        velocity: 100,
                    },
                    aaadaw_core::MidiNoteData {
                        pitch: 67,
                        tick: 480,
                        duration: 360,
                        velocity: 100,
                    },
                ],
            })
            .unwrap();

        let mut timeline = TimelineState::default();
        timeline.rebuild(&project);
        let cached = timeline
            .cache
            .items
            .iter()
            .find(|item| item.id == item_id)
            .unwrap();
        assert_eq!(cached.label, "Opening theme");
        assert_eq!(
            cached.midi_notes,
            vec![
                MidiNotePreview {
                    tick: 0,
                    duration: 240,
                    pitch: 60,
                },
                MidiNotePreview {
                    tick: 480,
                    duration: 360,
                    pitch: 67,
                },
            ]
        );
    }

    #[test]
    fn midi_note_preview_geometry_maps_project_time_and_pitch_into_the_clip() {
        let notes = [
            MidiNotePreview {
                tick: 0,
                duration: 240,
                pitch: 60,
            },
            MidiNotePreview {
                tick: 480,
                duration: 240,
                pitch: 72,
            },
        ];
        let rects =
            midi_note_preview_geometry(&notes, 960, 100.0, 200.0, 20.0, 48.0).collect::<Vec<_>>();
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].x, 100.0);
        assert_eq!(rects[0].width, 50.0);
        assert_eq!(rects[1].x, 200.0);
        assert!(rects[0].y > rects[1].y);
        assert!(rects.iter().all(|rect| (20.0..68.0).contains(&rect.y)));
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
                source_offset_samples: 1_024,
                length_samples: 2_048,
            })
            .unwrap();
        let samples = [
            vec![-16_384; 1_024],
            vec![8_192; 1_024],
            vec![16_384; 1_024],
            vec![0; 1_024],
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
        cache.rebuild(&project, SnapGrid::Sixteenth);

        let waveform = cache.waveform_geometry_for_viewport(&project, &waveforms, 0, 1.0, 200.0);
        assert_eq!(waveform.len(), 8);
        assert_eq!(waveform[0].track_index, 0);
        assert!((waveform[0].min - 0.25).abs() < 1.0e-6);
        assert!((waveform[0].max - 0.25).abs() < 1.0e-6);
        assert!((waveform[3].min - 0.25).abs() < 1.0e-6);
        assert!((waveform[4].min - 0.5).abs() < 1.0e-6);
        assert!((waveform[7].max - 0.5).abs() < 1.0e-6);
        assert_eq!(
            waveform[0].start_tick,
            project.tick_at_sample(1_000).unwrap()
        );
        assert_eq!(waveform[7].end_tick, project.tick_at_sample(3_048).unwrap());

        assert!(
            cache
                .waveform_geometry_for_viewport(&project, &waveforms, 1_000, 1.0, 100.0)
                .is_empty()
        );
        project
            .apply(DawAction::SetTempo {
                start_tick: 90,
                bpm: 5.0,
            })
            .unwrap();
        cache.rebuild(&project, SnapGrid::Sixteenth);
        let overview = cache.waveform_geometry_for_viewport(&project, &waveforms, 0, 0.1, 1_000.0);
        let closeup = cache.waveform_geometry_for_viewport(&project, &waveforms, 40, 1.0, 40.0);
        assert!(overview.len() <= 8);
        assert!(overview.iter().all(|bin| bin.min >= 0.25));
        assert_eq!(closeup.len(), 4);
        assert_eq!(
            overview[0].start_tick,
            project.tick_at_sample(1_000).unwrap()
        );
        assert_eq!(
            overview.last().unwrap().end_tick,
            project.tick_at_sample(3_048).unwrap()
        );
    }

    #[test]
    fn waveform_geometry_bounds_large_items_to_visible_peak_bins() {
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
                media_ref: "asset://long-waveform".to_owned(),
                start_sample: 0,
                source_offset_samples: 0,
                length_samples: 48_000,
            })
            .unwrap();
        let samples = vec![0; 48_000];
        let bytes = waveform_test_wav(&samples, 48_000);
        let mut decoder = AudioStreamDecoder::from_reader(
            Cursor::new(bytes.clone()),
            Some(bytes.len() as u64),
            Some("wav"),
        )
        .unwrap();
        let waveform = Arc::new(AudioWaveform::decode(&mut decoder, 256).unwrap());
        let waveforms = HashMap::from([("asset://long-waveform".to_owned(), waveform)]);
        let mut cache = TimelineCache::default();
        cache.rebuild(&project, SnapGrid::Sixteenth);

        let bins = cache.waveform_geometry_for_viewport(&project, &waveforms, 0, 20.0, 100.0);
        assert!(!bins.is_empty());
        assert!(bins.len() <= 10);
        assert!(
            bins.iter()
                .all(|bin| bin.start_tick < 5 && bin.end_tick <= 5)
        );
    }

    #[test]
    fn waveform_geometry_keeps_trimmed_clip_visible_when_coarse_edge_is_large() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Audio".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertAudioItem {
                track_id: project.tracks()[0].id(),
                media_ref: "asset://coarse-trimmed-waveform".to_owned(),
                start_sample: 0,
                source_offset_samples: 12_000,
                length_samples: 40_000,
            })
            .unwrap();
        let samples = vec![8_192; 131_072];
        let bytes = waveform_test_wav(&samples, 48_000);
        let mut decoder = AudioStreamDecoder::from_reader(
            Cursor::new(bytes.clone()),
            Some(bytes.len() as u64),
            Some("wav"),
        )
        .unwrap();
        let waveform = Arc::new(AudioWaveform::decode(&mut decoder, 256).unwrap());
        let waveforms = HashMap::from([("asset://coarse-trimmed-waveform".to_owned(), waveform)]);
        let mut cache = TimelineCache::default();
        cache.rebuild(&project, SnapGrid::Sixteenth);

        let bins = cache.waveform_geometry_for_viewport(&project, &waveforms, 0, 0.01, 1_000.0);
        assert!(!bins.is_empty());
        assert!(bins.len() <= 32);
        assert!(bins.iter().all(|bin| (bin.max - 0.25).abs() < 1.0e-6));
    }

    #[test]
    fn waveform_geometry_selects_detail_from_zoom_level() {
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
                media_ref: "asset://zoom-waveform".to_owned(),
                start_sample: 0,
                source_offset_samples: 0,
                length_samples: 4_096,
            })
            .unwrap();
        project
            .apply(DawAction::SetTempo {
                start_tick: 90,
                bpm: 5.0,
            })
            .unwrap();
        let samples = vec![0; 4_096];
        let bytes = waveform_test_wav(&samples, 48_000);
        let mut decoder = AudioStreamDecoder::from_reader(
            Cursor::new(bytes.clone()),
            Some(bytes.len() as u64),
            Some("wav"),
        )
        .unwrap();
        let waveform = Arc::new(AudioWaveform::decode(&mut decoder, 256).unwrap());
        let waveforms = HashMap::from([("asset://zoom-waveform".to_owned(), waveform)]);
        let mut cache = TimelineCache::default();
        cache.rebuild(&project, SnapGrid::Sixteenth);

        let overview = cache.waveform_geometry_for_viewport(&project, &waveforms, 0, 0.1, 1_000.0);
        let tempo_boundary =
            cache.waveform_geometry_for_viewport(&project, &waveforms, 0, 1.0, 90.0);
        let closeup = cache.waveform_geometry_for_viewport(&project, &waveforms, 0, 1.0, 40.0);
        assert_eq!(overview.len(), 1);
        assert!(!tempo_boundary.is_empty());
        assert_eq!(closeup.len(), 4);
    }

    #[test]
    fn viewport_tempo_lod_uses_ramp_endpoint_but_excludes_step_endpoint() {
        let mut project = Project::new();
        project
            .apply(DawAction::SetTempo {
                start_tick: 90,
                bpm: 5.0,
            })
            .unwrap();
        assert_eq!(slowest_tempo_in_viewport(&project, 0, 90), 120.0);

        project
            .apply(DawAction::SetTempoCurve {
                start_tick: 0,
                curve: TempoCurve::Linear,
            })
            .unwrap();
        assert_eq!(slowest_tempo_in_viewport(&project, 0, 90), 5.0);
    }

    #[test]
    fn waveform_geometry_omits_unaligned_trim_edge_peaks() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Audio".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertAudioItem {
                track_id: project.tracks()[0].id(),
                media_ref: "asset://unaligned-waveform".to_owned(),
                start_sample: 0,
                source_offset_samples: 1_028,
                length_samples: 2_045,
            })
            .unwrap();
        let samples = [
            vec![-16_384; 1_024],
            vec![8_192; 2_048],
            vec![-16_384; 1_024],
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
        let waveforms = HashMap::from([("asset://unaligned-waveform".to_owned(), waveform)]);
        let mut cache = TimelineCache::default();
        cache.rebuild(&project, SnapGrid::Sixteenth);

        let bins = cache.waveform_geometry_for_viewport(&project, &waveforms, 0, 0.1, 1_000.0);
        assert!(!bins.is_empty());
        assert!(bins.iter().all(|bin| bin.min >= 0.25));
        assert!(bins.len() <= 8);
    }

    #[test]
    fn waveform_geometry_refines_over_cap_unaligned_trim_edges_without_leaking() {
        let mut project = Project::new();
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Audio".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertAudioItem {
                track_id: project.tracks()[0].id(),
                media_ref: "asset://wide-trim-edge".to_owned(),
                start_sample: 0,
                source_offset_samples: 255,
                length_samples: 4_080,
            })
            .unwrap();
        let samples = [vec![-16_384; 255], vec![8_192; 4_080], vec![-16_384; 1_000]].concat();
        let bytes = waveform_test_wav(&samples, 48_000);
        let mut decoder = AudioStreamDecoder::from_reader(
            Cursor::new(bytes.clone()),
            Some(bytes.len() as u64),
            Some("wav"),
        )
        .unwrap();
        let waveform = Arc::new(AudioWaveform::decode(&mut decoder, 256).unwrap());
        let waveforms = HashMap::from([("asset://wide-trim-edge".to_owned(), waveform)]);
        let mut cache = TimelineCache::default();
        cache.rebuild(&project, SnapGrid::Sixteenth);

        let bins = cache.waveform_geometry_for_viewport(&project, &waveforms, 0, 0.001, 1_000.0);
        assert!(!bins.is_empty());
        assert!(bins.len() <= 2);
        assert!(bins.iter().all(|bin| bin.min >= 0.25));
    }

    #[test]
    fn waveform_geometry_covers_fractional_source_end_frame() {
        let settings = ProjectSettings::new(48_000, 960, 120.0).unwrap();
        let mut project = Project::with_settings(settings);
        project
            .apply(DawAction::CreateTrack {
                index: 0,
                name: "Audio".to_owned(),
            })
            .unwrap();
        project
            .apply(DawAction::InsertAudioItem {
                track_id: project.tracks()[0].id(),
                media_ref: "asset://fractional-source-end".to_owned(),
                start_sample: 0,
                source_offset_samples: 0,
                length_samples: 1_610,
            })
            .unwrap();
        let mut samples = vec![0; 1_480];
        *samples.last_mut().unwrap() = 16_384;
        let bytes = waveform_test_wav(&samples, 44_100);
        let mut decoder = AudioStreamDecoder::from_reader(
            Cursor::new(bytes.clone()),
            Some(bytes.len() as u64),
            Some("wav"),
        )
        .unwrap();
        let waveform = Arc::new(AudioWaveform::decode(&mut decoder, 1).unwrap());
        let waveforms = HashMap::from([("asset://fractional-source-end".to_owned(), waveform)]);
        let mut cache = TimelineCache::default();
        cache.rebuild(&project, SnapGrid::Sixteenth);

        let bins = cache.waveform_geometry_for_viewport(&project, &waveforms, 0, 4.0, 1_000.0);
        assert!(bins.iter().any(|bin| bin.max >= 0.49));
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
    fn middle_drag_zooms_vertically_and_pans_horizontally() {
        let mut zoom = MiddleDragState::new(Point::new(120.0, 80.0));
        let Some(TimelineEvent::ZoomAt { factor, anchor_x }) = zoom.update(Point::new(121.0, 48.0))
        else {
            panic!("vertical middle drag should zoom");
        };
        assert!(factor > 1.0);
        assert_eq!(anchor_x, 120.0);

        let mut pan = MiddleDragState::new(Point::new(120.0, 80.0));
        let Some(TimelineEvent::PanByPixels(delta_x)) = pan.update(Point::new(160.0, 81.0)) else {
            panic!("horizontal middle drag should pan");
        };
        assert_eq!(delta_x, 40.0);
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
    fn shift_click_selects_a_range_while_shift_drag_only_bypasses_snap() {
        let (project, _, items) = project_with_items();
        let mut timeline = TimelineState {
            pixels_per_tick: 0.1,
            ..TimelineState::default()
        };
        timeline.rebuild(&project);
        let program = timeline.program(&project, None);
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(500.0, 300.0));
        let modifiers = Event::Keyboard(keyboard::Event::ModifiersChanged(
            keyboard::Modifiers::SHIFT,
        ));
        let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let click = mouse::Cursor::Available(Point::new(112.0, 20.0));

        let mut drag_state = TimelineInteractionState::default();
        let _ = iced::widget::shader::Program::update(
            &program,
            &mut drag_state,
            &modifiers,
            bounds,
            click,
        );
        let _ =
            iced::widget::shader::Program::update(&program, &mut drag_state, &press, bounds, click);
        let moved = Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(122.0, 20.0),
        });
        let action = iced::widget::shader::Program::update(
            &program,
            &mut drag_state,
            &moved,
            bounds,
            mouse::Cursor::Available(Point::new(122.0, 20.0)),
        )
        .expect("Shift-drag should start an item drag");
        let (message, _, _) = action.into_inner();
        assert!(
            matches!(
                message,
                Some(crate::app::Message::Timeline(TimelineEvent::BeginItemDrag {
                    item_id,
                    range: false,
                    ignore_snap: true,
                    ..
                })) if item_id == items[1]
            ),
            "unexpected Shift-drag event: {message:?}"
        );

        let copy_modifiers = Event::Keyboard(keyboard::Event::ModifiersChanged(
            keyboard::Modifiers::COMMAND | keyboard::Modifiers::SHIFT,
        ));
        let mut copy_state = TimelineInteractionState::default();
        let _ = iced::widget::shader::Program::update(
            &program,
            &mut copy_state,
            &copy_modifiers,
            bounds,
            click,
        );
        let _ =
            iced::widget::shader::Program::update(&program, &mut copy_state, &press, bounds, click);
        let action = iced::widget::shader::Program::update(
            &program,
            &mut copy_state,
            &moved,
            bounds,
            mouse::Cursor::Available(Point::new(122.0, 20.0)),
        )
        .expect("Command-drag should start an item copy drag");
        let (message, _, _) = action.into_inner();
        assert!(
            matches!(
                message,
                Some(crate::app::Message::Timeline(
                    TimelineEvent::BeginItemDrag {
                        copy: true,
                        ignore_snap: true,
                        ..
                    }
                ))
            ),
            "unexpected Command-drag event: {message:?}"
        );

        let mut click_state = TimelineInteractionState::default();
        let _ = iced::widget::shader::Program::update(
            &program,
            &mut click_state,
            &modifiers,
            bounds,
            click,
        );
        let _ = iced::widget::shader::Program::update(
            &program,
            &mut click_state,
            &press,
            bounds,
            click,
        );
        let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        let action = iced::widget::shader::Program::update(
            &program,
            &mut click_state,
            &release,
            bounds,
            click,
        )
        .expect("Shift-click should publish range selection");
        let (message, _, _) = action.into_inner();
        assert!(matches!(
            message,
            Some(crate::app::Message::Timeline(TimelineEvent::SelectItem {
                item_id: Some(item_id),
                range: true,
                ..
            })) if item_id == items[1]
        ));
    }

    #[test]
    fn right_drag_in_blank_space_previews_and_selects_items_across_tracks() {
        let (project, _, items) = project_with_items();
        let mut timeline = TimelineState {
            pixels_per_tick: 0.1,
            ..TimelineState::default()
        };
        timeline.rebuild(&project);
        let project_before = project.snapshot();
        let program = timeline.program(&project, None);
        let mut state = TimelineInteractionState::default();
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(500.0, 300.0));
        let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right));
        let _ = iced::widget::shader::Program::update(
            &program,
            &mut state,
            &press,
            bounds,
            mouse::Cursor::Available(Point::new(250.0, 30.0)),
        )
        .expect("right-drag on blank track space should begin marquee selection");
        let moved = Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(25.0, 130.0),
        });
        let _ = iced::widget::shader::Program::update(
            &program,
            &mut state,
            &moved,
            bounds,
            mouse::Cursor::Available(Point::new(25.0, 130.0)),
        )
        .expect("moving the pointer should show the marquee preview");
        assert!(
            state
                .pending_item_selection_drag
                .is_some_and(|drag| drag.is_dragging)
        );
        assert!(timeline.selected_items.is_empty());

        let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right));
        let action = iced::widget::shader::Program::update(
            &program,
            &mut state,
            &release,
            bounds,
            mouse::Cursor::Available(Point::new(25.0, 130.0)),
        )
        .expect("releasing the marquee should publish the selected items");
        let (message, _, _) = action.into_inner();
        let Some(crate::app::Message::Timeline(event)) = message else {
            panic!("marquee release should publish a timeline selection event");
        };
        assert_eq!(project.snapshot(), project_before);
        timeline.handle(event);
        assert_eq!(timeline.selected_items, HashSet::from(items));
        assert_eq!(timeline.selected_item, Some(items[2]));
    }

    #[test]
    fn horizontal_marquee_selects_items_in_the_track_under_the_drag() {
        let (project, _, items) = project_with_items();
        let mut timeline = TimelineState::default();
        timeline.rebuild(&project);

        timeline.handle(TimelineEvent::SelectItemsInMarquee {
            start_tick: 200,
            end_tick: 1_300,
            top: 30.0,
            bottom: 30.0,
            additive: false,
        });

        assert_eq!(timeline.selected_items, HashSet::from([items[0], items[1]]));
    }

    #[test]
    fn escape_cancels_an_active_item_marquee_without_changing_selection() {
        let (project, _, _) = project_with_items();
        let mut timeline = TimelineState {
            pixels_per_tick: 0.1,
            ..TimelineState::default()
        };
        timeline.rebuild(&project);
        let program = timeline.program(&project, None);
        let mut state = TimelineInteractionState::default();
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(500.0, 300.0));
        let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right));
        let _ = iced::widget::shader::Program::update(
            &program,
            &mut state,
            &press,
            bounds,
            mouse::Cursor::Available(Point::new(250.0, 30.0)),
        );
        let moved = Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(25.0, 130.0),
        });
        let _ = iced::widget::shader::Program::update(
            &program,
            &mut state,
            &moved,
            bounds,
            mouse::Cursor::Available(Point::new(25.0, 130.0)),
        );

        let escape = Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            modified_key: keyboard::Key::Named(keyboard::key::Named::Escape),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Escape),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::NONE,
            text: None,
            repeat: false,
        });
        let action = iced::widget::shader::Program::update(
            &program,
            &mut state,
            &escape,
            bounds,
            mouse::Cursor::Available(Point::new(25.0, 130.0)),
        )
        .expect("Escape should cancel an active item marquee");
        let (message, _, _) = action.into_inner();

        assert!(message.is_none());
        assert!(state.pending_item_selection_drag.is_none());
        assert!(timeline.selected_items.is_empty());
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
    fn setting_edit_cursor_preserves_item_and_time_selection() {
        let (project, _, items) = project_with_items();
        let mut timeline = TimelineState::default();
        timeline.rebuild(&project);
        timeline.handle(TimelineEvent::SelectItem {
            item_id: Some(items[1]),
            additive: false,
            range: false,
        });
        timeline.handle(TimelineEvent::SetTimeSelection {
            start_tick: 240,
            end_tick: 960,
        });
        let selected_items = timeline.selected_items.clone();
        let selected_track = timeline.selected_track;
        let selection = timeline.time_selection;

        timeline.handle(TimelineEvent::SetEditCursor(1_200));

        assert_eq!(timeline.edit_cursor_tick, 1_200);
        assert_eq!(timeline.selected_items, selected_items);
        assert_eq!(timeline.selected_item, Some(items[1]));
        assert_eq!(timeline.selected_track, selected_track);
        assert_eq!(timeline.time_selection, selection);
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
            deselect_items_on_click: false,
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
            copy: false,
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
            copy: false,
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
            copy: false,
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
            copy: false,
        });
        assert_eq!(timeline.drag_preview().unwrap().delta_ticks, 101);
    }
}
