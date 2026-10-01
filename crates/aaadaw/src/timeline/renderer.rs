use super::{ItemKind, TimelineItem};
use aaadaw_core::ItemId;
use bytemuck::{Pod, Zeroable, cast_slice};
use iced::Rectangle;
use iced::wgpu;
use iced::widget::shader::{Pipeline, Primitive, Viewport};
use std::num::NonZeroU64;
use std::sync::Arc;

const LANE: u32 = 0;
const AUDIO_ITEM: u32 = 1;
const MIDI_ITEM: u32 = 2;
const BAR_LINE: u32 = 3;
const BEAT_LINE: u32 = 4;
const EDIT_CURSOR: u32 = 5;
const PLAYHEAD: u32 = 6;

#[derive(Debug)]
pub(super) struct TimelinePrimitive {
    pub(super) generation: u64,
    pub(super) items: Arc<[TimelineItem]>,
    pub(super) track_count: u32,
    pub(super) row_height: f32,
    pub(super) origin_tick: u64,
    pub(super) pixels_per_tick: f32,
    pub(super) edit_cursor_tick: u64,
    pub(super) playhead_tick: Option<u64>,
    pub(super) selected_item: Option<ItemId>,
    pub(super) selected_track_index: u32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) grid_lines: Vec<(u64, bool)>,
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

impl GpuRect {
    fn new(
        start_tick: u64,
        end_tick: u64,
        y: f32,
        height: f32,
        color: [u8; 4],
        item_id: u64,
        track_index: u32,
        kind: u32,
    ) -> Self {
        Self {
            start_tick: split_tick(start_tick),
            end_tick: split_tick(end_tick),
            y_height: [y, height],
            color: color.map(linearize_srgb),
            item_id: split_tick(item_id),
            track_index,
            kind,
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
            dynamic_buffer: empty_buffer(),
            dynamic_capacity: 1,
            dynamic_count: 0,
        }
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
            let mut instances = Vec::with_capacity(self.track_count as usize + self.items.len());
            for track_index in 0..self.track_count {
                let color = if track_index % 2 == 0 {
                    [34, 39, 43, 255]
                } else {
                    [38, 43, 47, 255]
                };
                instances.push(GpuRect::new(
                    0,
                    0,
                    track_index as f32 * self.row_height,
                    self.row_height,
                    color,
                    0,
                    track_index,
                    LANE,
                ));
            }
            for item in self.items.iter() {
                let (color, kind) = match item.kind {
                    ItemKind::Audio => ([74, 99, 122, 255], AUDIO_ITEM),
                    ItemKind::Midi => ([89, 112, 74, 255], MIDI_ITEM),
                };
                instances.push(GpuRect::new(
                    item.start_tick,
                    item.end_tick,
                    item.track_index as f32 * self.row_height + 7.0,
                    self.row_height - 14.0,
                    color,
                    item.id.value(),
                    item.track_index as u32,
                    kind,
                ));
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

        let mut dynamic = Vec::with_capacity(self.grid_lines.len() + 2);
        for (tick, is_measure) in &self.grid_lines {
            dynamic.push(GpuRect::new(
                *tick,
                *tick,
                0.0,
                self.height,
                if *is_measure {
                    [83, 91, 98, 170]
                } else {
                    [57, 64, 70, 135]
                },
                0,
                u32::MAX,
                if *is_measure { BAR_LINE } else { BEAT_LINE },
            ));
        }
        dynamic.push(GpuRect::new(
            self.edit_cursor_tick,
            self.edit_cursor_tick,
            0.0,
            self.height,
            [231, 184, 101, 255],
            0,
            u32::MAX,
            EDIT_CURSOR,
        ));
        if let Some(playhead_tick) = self.playhead_tick {
            dynamic.push(GpuRect::new(
                playhead_tick,
                playhead_tick,
                0.0,
                self.height,
                [76, 178, 209, 235],
                0,
                u32::MAX,
                PLAYHEAD,
            ));
        }
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

        let selected_item = self
            .selected_item
            .map_or([u32::MAX, u32::MAX], |item_id| split_tick(item_id.value()));
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
            playhead_selection: [playhead[0], playhead[1], selected_item[0], selected_item[1]],
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
