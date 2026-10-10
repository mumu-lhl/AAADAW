use super::{
    ItemDragPreview, ItemKind, ItemTrimPreview, SelectedItemGeometry, TimeSelection, TimelineItem,
    TrackRowLayout, WaveformBinGeometry,
};
use bytemuck::{Pod, Zeroable, cast_slice};
use iced::wgpu;
use iced::widget::shader::{Pipeline, Primitive, Viewport};
use iced::{Point, Rectangle};
use std::num::NonZeroU64;
use std::sync::Arc;

const LANE: u32 = 0;
const AUDIO_ITEM: u32 = 1;
const MIDI_ITEM: u32 = 2;
const BAR_LINE: u32 = 3;
const BEAT_LINE: u32 = 4;
const EDIT_CURSOR: u32 = 5;
const PLAYHEAD: u32 = 6;
const SELECTED_ITEM: u32 = 7;
const DROP_TARGET: u32 = 8;
const TIME_SELECTION_FILL: u32 = 9;
const TIME_SELECTION_EDGE: u32 = 10;
const AUDIO_WAVEFORM: u32 = 11;
const AUTOMATION_SEGMENT: u32 = 12;
const AUTOMATION_POINT: u32 = 13;
const FX_LANE_DIVIDER: u32 = 14;
const ITEM_MARQUEE: u32 = 15;
const FADE_SEGMENT: u32 = 16;

#[derive(Debug, Clone, Copy)]
pub(super) struct FadeSegment {
    pub(super) start: Point,
    pub(super) end: Point,
    pub(super) color: [u8; 4],
}

#[derive(Debug)]
pub(super) struct AutomationLane {
    pub(super) track_index: u32,
    pub(super) is_fx: bool,
    pub(super) lane_top: f32,
    pub(super) lane_height: f32,
    pub(super) value_range: (f32, f32),
    pub(super) selected_point: Option<usize>,
    pub(super) points: Vec<(u64, f32)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct AutomationPointPreview {
    pub(super) track_index: u32,
    pub(super) tick: u64,
    pub(super) y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ItemSelectionPreview {
    pub(super) start_tick: u64,
    pub(super) end_tick: u64,
    pub(super) top: f32,
    pub(super) bottom: f32,
}

#[derive(Debug)]
pub(super) struct TimelinePrimitive {
    pub(super) fade_segments: Vec<FadeSegment>,
    pub(super) generation: u64,
    pub(super) waveform_generation: u64,
    pub(super) waveform_render_generation: u64,
    pub(super) items: Arc<[TimelineItem]>,
    pub(super) track_count: u32,
    pub(super) row_layout: Vec<TrackRowLayout>,
    pub(super) origin_tick: u64,
    pub(super) pixels_per_tick: f32,
    pub(super) edit_cursor_tick: u64,
    pub(super) playhead_tick: Option<u64>,
    pub(super) selected_items: Vec<SelectedItemGeometry>,
    pub(super) waveform_bins: Arc<[WaveformBinGeometry]>,
    pub(super) time_selection: Option<TimeSelection>,
    pub(super) drag_preview: Option<ItemDragPreview>,
    pub(super) item_trim_preview: Option<ItemTrimPreview>,
    pub(super) selected_track_index: u32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) grid_lines: Vec<(u64, bool)>,
    pub(super) automation_lanes: Vec<AutomationLane>,
    pub(super) automation_point_preview: Option<AutomationPointPreview>,
    pub(super) item_selection_preview: Option<ItemSelectionPreview>,
}

#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct GpuRect {
    start_tick: [u32; 2],
    end_tick: [u32; 2],
    y_height: [f32; 2],
    color: [u8; 4],
    item_id: [u32; 2],
    track_index: u32,
    kind: u32,
}

struct GpuRectSpec {
    start_tick: u64,
    end_tick: u64,
    y: f32,
    height: f32,
    color: [u8; 4],
    item_id: u64,
    track_index: u32,
    kind: u32,
}

impl GpuRect {
    fn new(spec: GpuRectSpec) -> Self {
        Self {
            start_tick: split_tick(spec.start_tick),
            end_tick: split_tick(spec.end_tick),
            y_height: [spec.y, spec.height],
            color: spec.color.map(linearize_srgb),
            item_id: split_tick(spec.item_id),
            track_index: spec.track_index,
            kind: spec.kind,
        }
    }
}

fn linearize_srgb(channel: u8) -> u8 {
    let encoded = f32::from(channel) / 255.0;
    let linear = if encoded <= 0.04045 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    };
    (linear * 255.0).round() as u8
}

#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C, align(16))]
struct Uniforms {
    view: [f32; 4],
    origin_cursor: [u32; 4],
    playhead_selection: [u32; 4],
    state: [u32; 4],
}

pub(super) struct TimelinePipeline {
    render_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    static_buffer: wgpu::Buffer,
    static_capacity: usize,
    static_count: u32,
    static_generation: Option<u64>,
    previewed_indices: Vec<usize>,
    waveform_buffer: wgpu::Buffer,
    waveform_capacity: usize,
    waveform_count: u32,
    waveform_generation: Option<u64>,
    waveform_render_generation: Option<u64>,
    waveform_preview: Option<ItemTrimPreview>,
    dynamic_buffer: wgpu::Buffer,
    dynamic_capacity: usize,
    dynamic_count: u32,
}

impl Pipeline for TimelinePipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("AAADAW timeline shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("timeline.wgsl").into()),
        });
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("AAADAW timeline uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("AAADAW timeline bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(std::mem::size_of::<Uniforms>() as u64),
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("AAADAW timeline bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("AAADAW timeline pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("AAADAW timeline pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GpuRect>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Uint32x2,
                        1 => Uint32x2,
                        2 => Float32x2,
                        3 => Unorm8x4,
                        4 => Uint32x2,
                        5 => Uint32,
                        6 => Uint32
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..wgpu::PrimitiveState::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let empty_buffer = || {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("AAADAW empty timeline instances"),
                size: std::mem::size_of::<GpuRect>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };

        Self {
            render_pipeline,
            bind_group,
            uniform_buffer,
            static_buffer: empty_buffer(),
            static_capacity: 1,
            static_count: 0,
            static_generation: None,
            previewed_indices: Vec::new(),
            waveform_buffer: empty_buffer(),
            waveform_capacity: 1,
            waveform_count: 0,
            waveform_generation: None,
            waveform_render_generation: None,
            waveform_preview: None,
            dynamic_buffer: empty_buffer(),
            dynamic_capacity: 1,
            dynamic_count: 0,
        }
    }
}

impl TimelinePrimitive {
    fn has_volume_automation(&self, track_index: u32) -> bool {
        self.automation_lanes
            .iter()
            .any(|lane| lane.track_index == track_index && !lane.is_fx)
    }
}

impl Primitive for TimelinePrimitive {
    type Pipeline = TimelinePipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        if pipeline.static_generation != Some(self.generation) {
            pipeline.previewed_indices.clear();
            let mut instances = Vec::with_capacity(
                self.track_count as usize
                    + self.items.len()
                    + self
                        .automation_lanes
                        .iter()
                        .map(|lane| lane.points.len() * 2 + 1 + usize::from(lane.is_fx))
                        .sum::<usize>(),
            );
            for track_index in 0..self.track_count {
                let Some(row) = self.row_layout.get(track_index as usize) else {
                    continue;
                };
                let color = if track_index % 2 == 0 {
                    [34, 39, 43, 255]
                } else {
                    [38, 43, 47, 255]
                };
                instances.push(GpuRect::new(GpuRectSpec {
                    start_tick: 0,
                    end_tick: 0,
                    y: row.top,
                    height: row.height,
                    color,
                    item_id: 0,
                    track_index,
                    kind: LANE,
                }));
            }
            for item in self.items.iter() {
                let Some(row) = self.row_layout.get(item.track_index) else {
                    continue;
                };
                let has_automation = self
                    .automation_lanes
                    .iter()
                    .any(|lane| lane.track_index == item.track_index as u32 && !lane.is_fx);
                let (color, kind) = match item.kind {
                    ItemKind::Audio => ([74, 99, 122, 255], AUDIO_ITEM),
                    ItemKind::Midi => ([89, 112, 74, 255], MIDI_ITEM),
                };
                instances.push(GpuRect::new(GpuRectSpec {
                    start_tick: item.start_tick,
                    end_tick: item.end_tick,
                    y: row.item_geometry(has_automation).0,
                    height: row.item_geometry(has_automation).1,
                    color,
                    item_id: item.id.value(),
                    track_index: item.track_index as u32,
                    kind,
                }));
            }
            for lane in &self.automation_lanes {
                let Some(row) = self.row_layout.get(lane.track_index as usize) else {
                    continue;
                };
                let y = |value: f32| {
                    let (min, max) = if lane.is_fx {
                        lane.value_range
                    } else {
                        (-60.0, 6.0)
                    };
                    let top = if lane.is_fx {
                        lane.lane_top + 2.0
                    } else {
                        66.0
                    };
                    let bottom = if lane.is_fx {
                        top + lane.lane_height - 4.0
                    } else {
                        82.0
                    };
                    row.top
                        + top
                        + (max - value.clamp(min, max))
                            * ((bottom - top) / (max - min).max(f32::EPSILON))
                };
                for pair in lane.points.windows(2) {
                    instances.push(GpuRect::new(GpuRectSpec {
                        start_tick: pair[0].0,
                        end_tick: pair[1].0,
                        y: y(pair[0].1),
                        height: y(pair[1].1),
                        color: [92, 205, 162, 255],
                        item_id: 0,
                        track_index: lane.track_index,
                        kind: AUTOMATION_SEGMENT,
                    }));
                }
                if let Some((first_tick, first_db)) = lane.points.first().copied() {
                    instances.push(GpuRect::new(GpuRectSpec {
                        start_tick: 0,
                        end_tick: first_tick,
                        y: y(first_db),
                        height: y(first_db),
                        color: [92, 205, 162, 255],
                        item_id: 0,
                        track_index: lane.track_index,
                        kind: AUTOMATION_SEGMENT,
                    }));
                }
                if let Some((last_tick, last_db)) = lane.points.last().copied() {
                    let view_end_tick = self.origin_tick.saturating_add(
                        (f64::from(self.width) / f64::from(self.pixels_per_tick)).ceil() as u64,
                    );
                    instances.push(GpuRect::new(GpuRectSpec {
                        start_tick: last_tick,
                        end_tick: view_end_tick.max(last_tick),
                        y: y(last_db),
                        height: y(last_db),
                        color: [92, 205, 162, 255],
                        item_id: 0,
                        track_index: lane.track_index,
                        kind: AUTOMATION_SEGMENT,
                    }));
                }
                for (index, point) in lane.points.iter().enumerate() {
                    let py = y(point.1);
                    instances.push(GpuRect::new(GpuRectSpec {
                        start_tick: point.0,
                        end_tick: point.0,
                        y: py - 3.5,
                        height: 7.0,
                        color: if lane.selected_point == Some(index) {
                            [255, 221, 127, 255]
                        } else {
                            [170, 244, 205, 255]
                        },
                        item_id: 0,
                        track_index: lane.track_index,
                        kind: AUTOMATION_POINT,
                    }));
                }
                if lane.is_fx {
                    instances.push(GpuRect::new(GpuRectSpec {
                        start_tick: 0,
                        end_tick: 0,
                        y: row.top + lane.lane_top + lane.lane_height - 1.0,
                        height: 1.0,
                        color: [68, 75, 80, 255],
                        item_id: 0,
                        track_index: lane.track_index,
                        kind: FX_LANE_DIVIDER,
                    }));
                }
            }
            ensure_capacity(
                device,
                &mut pipeline.static_buffer,
                &mut pipeline.static_capacity,
                instances.len(),
                "AAADAW arrangement instances",
            );
            if !instances.is_empty() {
                queue.write_buffer(&pipeline.static_buffer, 0, cast_slice(&instances));
            }
            pipeline.static_count = instances.len() as u32;
            pipeline.static_generation = Some(self.generation);
        }

        let previewed_indices = if self.drag_preview.is_some() || self.item_trim_preview.is_some() {
            let mut indices = self
                .selected_items
                .iter()
                .map(|item| item.cache_index)
                .collect::<Vec<_>>();
            if let Some(preview) = self.item_trim_preview
                && let Some(index) = self
                    .items
                    .iter()
                    .position(|item| item.id == preview.item_id)
            {
                indices.push(index);
            }
            indices.sort_unstable();
            indices.dedup();
            indices
        } else {
            Vec::new()
        };
        for cache_index in pipeline.previewed_indices.iter().copied() {
            if previewed_indices.binary_search(&cache_index).is_ok() {
                continue;
            }
            if let Some(item) = self.items.get(cache_index) {
                let rect = item_rect(
                    item,
                    &self.row_layout,
                    self.has_volume_automation(item.track_index as u32),
                );
                write_item_rect(
                    queue,
                    &pipeline.static_buffer,
                    self.track_count,
                    cache_index,
                    &rect,
                );
            }
        }
        if let Some(preview) = self.drag_preview {
            for selected in &self.selected_items {
                let start_tick = shift_tick(selected.start_tick, preview.delta_ticks)
                    .unwrap_or(selected.start_tick);
                let end_tick =
                    shift_tick(selected.end_tick, preview.delta_ticks).unwrap_or(selected.end_tick);
                let row = i128::try_from(selected.track_index).unwrap_or(i128::MAX)
                    + i128::from(preview.track_delta);
                let track_index = u32::try_from(row).unwrap_or(u32::MAX);
                let rect = selected_item_rect(
                    start_tick,
                    end_tick,
                    track_index,
                    selected.item_id.value(),
                    selected.kind,
                    &self.row_layout,
                    self.has_volume_automation(track_index),
                );
                write_item_rect(
                    queue,
                    &pipeline.static_buffer,
                    self.track_count,
                    selected.cache_index,
                    &rect,
                );
            }
        }
        if let Some(preview) = self.item_trim_preview
            && let Some((cache_index, item)) = self
                .items
                .iter()
                .enumerate()
                .find(|(_, item)| item.id == preview.item_id)
        {
            let rect = selected_item_rect(
                preview.start_tick,
                preview.end_tick,
                item.track_index as u32,
                item.id.value(),
                item.kind,
                &self.row_layout,
                self.has_volume_automation(item.track_index as u32),
            );
            write_item_rect(
                queue,
                &pipeline.static_buffer,
                self.track_count,
                cache_index,
                &rect,
            );
        }
        pipeline.previewed_indices = previewed_indices;

        if pipeline.waveform_generation != Some(self.waveform_generation)
            || pipeline.waveform_render_generation != Some(self.waveform_render_generation)
            || pipeline.waveform_preview != self.item_trim_preview
        {
            let mut waveforms = Vec::with_capacity(self.waveform_bins.len());
            for bin in self.waveform_bins.iter().copied() {
                let Some(row) = self.row_layout.get(bin.track_index).copied() else {
                    continue;
                };
                let clipped = match self.item_trim_preview.filter(|p| p.item_id == bin.item_id) {
                    Some(preview) if preview.valid => WaveformBinGeometry {
                        start_tick: bin.start_tick.max(preview.start_tick),
                        end_tick: bin.end_tick.min(preview.end_tick),
                        ..bin
                    },
                    Some(preview) => WaveformBinGeometry {
                        start_tick: preview.start_tick,
                        end_tick: preview.start_tick,
                        ..bin
                    },
                    None => bin,
                };
                waveforms.push(waveform_rect(
                    clipped,
                    row,
                    self.automation_lanes
                        .iter()
                        .any(|lane| lane.track_index == bin.track_index as u32 && !lane.is_fx),
                ));
            }
            ensure_capacity(
                device,
                &mut pipeline.waveform_buffer,
                &mut pipeline.waveform_capacity,
                waveforms.len(),
                "AAADAW viewport waveform instances",
            );
            if !waveforms.is_empty() {
                queue.write_buffer(&pipeline.waveform_buffer, 0, cast_slice(&waveforms));
            }
            pipeline.waveform_count = waveforms.len() as u32;
            pipeline.waveform_generation = Some(self.waveform_generation);
            pipeline.waveform_render_generation = Some(self.waveform_render_generation);
            pipeline.waveform_preview = self.item_trim_preview;
        }

        let mut dynamic = Vec::with_capacity(
            self.grid_lines.len()
                + 3
                + self.selected_items.len()
                + usize::from(self.item_trim_preview.is_some())
                + usize::from(self.item_selection_preview.is_some())
                + if self.time_selection.is_some() { 3 } else { 0 },
        );
        if let Some(selection) = self.time_selection {
            dynamic.push(GpuRect::new(GpuRectSpec {
                start_tick: selection.start_tick,
                end_tick: selection.end_tick,
                y: 0.0,
                height: self.height,
                color: [63, 126, 147, 48],
                item_id: 0,
                track_index: u32::MAX,
                kind: TIME_SELECTION_FILL,
            }));
            for tick in [selection.start_tick, selection.end_tick] {
                dynamic.push(GpuRect::new(GpuRectSpec {
                    start_tick: tick,
                    end_tick: tick,
                    y: 0.0,
                    height: self.height,
                    color: [110, 181, 195, 235],
                    item_id: 0,
                    track_index: u32::MAX,
                    kind: TIME_SELECTION_EDGE,
                }));
            }
        }
        if let Some(selection) = self.item_selection_preview {
            dynamic.push(GpuRect::new(GpuRectSpec {
                start_tick: selection.start_tick,
                end_tick: selection.end_tick,
                y: selection.top,
                height: (selection.bottom - selection.top).max(3.0),
                color: [103, 173, 218, 180],
                item_id: 0,
                track_index: u32::MAX,
                kind: ITEM_MARQUEE,
            }));
        }
        if let Some(preview) = self.drag_preview
            && let Some(target_track_index) = preview.target_track_index
        {
            dynamic.push(GpuRect::new(GpuRectSpec {
                start_tick: 0,
                end_tick: 0,
                y: self
                    .row_layout
                    .get(target_track_index)
                    .map_or(0.0, |row| row.top),
                height: self
                    .row_layout
                    .get(target_track_index)
                    .map_or(0.0, |row| row.height),
                color: if preview.valid {
                    [79, 111, 87, 110]
                } else {
                    [139, 67, 57, 110]
                },
                item_id: 0,
                track_index: target_track_index as u32,
                kind: DROP_TARGET,
            }));
        }
        for selected in &self.selected_items {
            let preview = self.drag_preview;
            let trim = self
                .item_trim_preview
                .filter(|trim| trim.item_id == selected.item_id);
            let start_tick = trim.map_or_else(
                || {
                    preview
                        .and_then(|preview| shift_tick(selected.start_tick, preview.delta_ticks))
                        .unwrap_or(selected.start_tick)
                },
                |trim| trim.start_tick,
            );
            let end_tick = trim.map_or_else(
                || {
                    preview
                        .and_then(|preview| shift_tick(selected.end_tick, preview.delta_ticks))
                        .unwrap_or(selected.end_tick)
                },
                |trim| trim.end_tick,
            );
            let row = i128::try_from(selected.track_index).unwrap_or(i128::MAX)
                + preview.map_or(0, |preview| i128::from(preview.track_delta));
            let track_index = u32::try_from(row).unwrap_or(u32::MAX);
            dynamic.push(GpuRect::new(GpuRectSpec {
                start_tick,
                end_tick,
                y: self
                    .row_layout
                    .get(track_index as usize)
                    .map_or(0.0, |row| {
                        row.item_geometry(self.has_volume_automation(track_index)).0
                    }),
                height: self
                    .row_layout
                    .get(track_index as usize)
                    .map_or(0.0, |row| {
                        row.item_geometry(self.has_volume_automation(track_index)).1
                    }),
                color: if trim.is_some_and(|trim| !trim.valid) {
                    [218, 80, 71, 255]
                } else if preview.is_some_and(|preview| preview.copy) {
                    [117, 196, 143, 190]
                } else {
                    [245, 185, 92, 255]
                },
                item_id: selected.item_id.value(),
                track_index,
                kind: SELECTED_ITEM,
            }));
        }
        for (tick, is_measure) in &self.grid_lines {
            dynamic.push(GpuRect::new(GpuRectSpec {
                start_tick: *tick,
                end_tick: *tick,
                y: 0.0,
                height: self.height,
                color: if *is_measure {
                    [83, 91, 98, 170]
                } else {
                    [57, 64, 70, 135]
                },
                item_id: 0,
                track_index: u32::MAX,
                kind: if *is_measure { BAR_LINE } else { BEAT_LINE },
            }));
        }
        if let Some(preview) = self.automation_point_preview
            && let Some(row) = self.row_layout.get(preview.track_index as usize)
        {
            dynamic.push(GpuRect::new(GpuRectSpec {
                start_tick: preview.tick,
                end_tick: preview.tick,
                y: row.top + preview.y - 4.0,
                height: 8.0,
                color: [255, 238, 174, 255],
                item_id: 0,
                track_index: preview.track_index,
                kind: AUTOMATION_POINT,
            }));
        }
        dynamic.push(GpuRect::new(GpuRectSpec {
            start_tick: self.edit_cursor_tick,
            end_tick: self.edit_cursor_tick,
            y: 0.0,
            height: self.height,
            color: [231, 184, 101, 255],
            item_id: 0,
            track_index: u32::MAX,
            kind: EDIT_CURSOR,
        }));
        if let Some(playhead_tick) = self.playhead_tick {
            dynamic.push(GpuRect::new(GpuRectSpec {
                start_tick: playhead_tick,
                end_tick: playhead_tick,
                y: 0.0,
                height: self.height,
                color: [76, 178, 209, 235],
                item_id: 0,
                track_index: u32::MAX,
                kind: PLAYHEAD,
            }));
        }
        dynamic.extend(self.fade_segments.iter().map(|segment| GpuRect {
            start_tick: [segment.start.x.to_bits(), 0],
            end_tick: [segment.end.x.to_bits(), 0],
            y_height: [segment.start.y, segment.end.y],
            color: segment.color.map(linearize_srgb),
            item_id: [0, 0],
            track_index: u32::MAX,
            kind: FADE_SEGMENT,
        }));
        ensure_capacity(
            device,
            &mut pipeline.dynamic_buffer,
            &mut pipeline.dynamic_capacity,
            dynamic.len(),
            "AAADAW dynamic timeline instances",
        );
        if !dynamic.is_empty() {
            queue.write_buffer(&pipeline.dynamic_buffer, 0, cast_slice(&dynamic));
        }
        pipeline.dynamic_count = dynamic.len() as u32;

        let playhead = self.playhead_tick.map_or([u32::MAX, u32::MAX], split_tick);
        let uniforms = Uniforms {
            view: [
                self.width * viewport.scale_factor(),
                self.height * viewport.scale_factor(),
                self.pixels_per_tick,
                viewport.scale_factor(),
            ],
            origin_cursor: [
                (self.origin_tick >> 32) as u32,
                self.origin_tick as u32,
                (self.edit_cursor_tick >> 32) as u32,
                self.edit_cursor_tick as u32,
            ],
            playhead_selection: [playhead[0], playhead[1], u32::MAX, u32::MAX],
            state: [self.selected_track_index, 0, 0, 0],
        };
        queue.write_buffer(&pipeline.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    fn draw(&self, pipeline: &Self::Pipeline, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        render_pass.set_pipeline(&pipeline.render_pipeline);
        render_pass.set_bind_group(0, &pipeline.bind_group, &[]);
        if pipeline.static_count > 0 {
            render_pass.set_vertex_buffer(0, pipeline.static_buffer.slice(..));
            render_pass.draw(0..6, 0..pipeline.static_count);
        }
        if pipeline.waveform_count > 0 {
            render_pass.set_vertex_buffer(0, pipeline.waveform_buffer.slice(..));
            render_pass.draw(0..6, 0..pipeline.waveform_count);
        }
        if pipeline.dynamic_count > 0 {
            render_pass.set_vertex_buffer(0, pipeline.dynamic_buffer.slice(..));
            render_pass.draw(0..6, 0..pipeline.dynamic_count);
        }
        true
    }
}

fn split_tick(value: u64) -> [u32; 2] {
    [(value >> 32) as u32, value as u32]
}

fn shift_tick(tick: u64, delta: i128) -> Option<u64> {
    u64::try_from(i128::from(tick) + delta).ok()
}

fn item_appearance(kind: ItemKind) -> ([u8; 4], u32) {
    match kind {
        ItemKind::Audio => ([74, 99, 122, 255], AUDIO_ITEM),
        ItemKind::Midi => ([89, 112, 74, 255], MIDI_ITEM),
    }
}

fn item_rect(
    item: &TimelineItem,
    row_layout: &[TrackRowLayout],
    has_volume_automation: bool,
) -> GpuRect {
    let (color, kind) = item_appearance(item.kind);
    let Some(row) = row_layout.get(item.track_index) else {
        return GpuRect::zeroed();
    };
    GpuRect::new(GpuRectSpec {
        start_tick: item.start_tick,
        end_tick: item.end_tick,
        y: row.item_geometry(has_volume_automation).0,
        height: row.item_geometry(has_volume_automation).1,
        color,
        item_id: item.id.value(),
        track_index: item.track_index as u32,
        kind,
    })
}

fn selected_item_rect(
    start_tick: u64,
    end_tick: u64,
    track_index: u32,
    item_id: u64,
    kind: ItemKind,
    row_layout: &[TrackRowLayout],
    has_volume_automation: bool,
) -> GpuRect {
    let (color, item_kind) = item_appearance(kind);
    let row = row_layout
        .get(track_index as usize)
        .copied()
        .unwrap_or(TrackRowLayout {
            top: 0.0,
            height: 0.0,
            base_height: 0.0,
            fx_lane_count: 0,
        });
    GpuRect::new(GpuRectSpec {
        start_tick,
        end_tick,
        y: row.item_geometry(has_volume_automation).0,
        height: row.item_geometry(has_volume_automation).1,
        color,
        item_id,
        track_index,
        kind: item_kind,
    })
}

fn waveform_rect(bin: WaveformBinGeometry, row: TrackRowLayout, has_automation: bool) -> GpuRect {
    let height = (row.base_height * if has_automation { 0.35 } else { 0.56 }).max(1.0);
    let center = row.top + row.base_height * if has_automation { 0.38 } else { 0.62 };
    let top = center - bin.max.clamp(-1.0, 1.0) * height / 2.0;
    let bottom = center - bin.min.clamp(-1.0, 1.0) * height / 2.0;
    GpuRect::new(GpuRectSpec {
        start_tick: bin.start_tick,
        end_tick: bin.end_tick,
        y: top.min(bottom),
        height: (bottom - top).abs().max(1.0),
        color: [150, 177, 190, 255],
        item_id: 0,
        track_index: bin.track_index as u32,
        kind: AUDIO_WAVEFORM,
    })
}

fn write_item_rect(
    queue: &wgpu::Queue,
    buffer: &wgpu::Buffer,
    track_count: u32,
    cache_index: usize,
    rect: &GpuRect,
) {
    let instance_index = track_count as usize + cache_index;
    write_static_instance(queue, buffer, instance_index, rect);
}

fn write_static_instance(
    queue: &wgpu::Queue,
    buffer: &wgpu::Buffer,
    instance_index: usize,
    rect: &GpuRect,
) {
    let byte_offset = (instance_index * std::mem::size_of::<GpuRect>()) as u64;
    queue.write_buffer(buffer, byte_offset, bytemuck::bytes_of(rect));
}

fn ensure_capacity(
    device: &wgpu::Device,
    buffer: &mut wgpu::Buffer,
    capacity: &mut usize,
    required: usize,
    label: &'static str,
) {
    if required <= *capacity {
        return;
    }
    let next_capacity = required.next_power_of_two().max(1);
    *buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (next_capacity * std::mem::size_of::<GpuRect>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    *capacity = next_capacity;
}

#[cfg(test)]
mod tests {
    use super::linearize_srgb;

    #[test]
    fn gpu_vertex_colors_are_converted_from_srgb_to_linear() {
        assert_eq!(linearize_srgb(0), 0);
        assert_eq!(linearize_srgb(128), 55);
        assert_eq!(linearize_srgb(255), 255);
    }
}
