struct Uniforms {
    view: vec4<f32>,
    origin_cursor: vec4<u32>,
    playhead_selection: vec4<u32>,
    state: vec4<u32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) item_id: vec2<u32>,
    @location(3) track_index: u32,
    @location(4) kind: u32,
};

fn linearize_srgb_color(color: vec3<f32>) -> vec3<f32> {
    return select(
        color / 12.92,
        pow((color + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)),
        color > vec3<f32>(0.04045),
    );
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

fn positive_tick_delta(a: vec2<u32>, b: vec2<u32>) -> f32 {
    let low = a.y - b.y;
    let borrow = select(0u, 1u, a.y < b.y);
    let high = a.x - b.x - borrow;
    return f32(high) * 4294967296.0 + f32(low);
}

fn signed_tick_delta(a: vec2<u32>, b: vec2<u32>) -> f32 {
    let a_after_b = a.x > b.x || (a.x == b.x && a.y >= b.y);
    if a_after_b {
        return positive_tick_delta(a, b);
    }
    return -positive_tick_delta(b, a);
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) start_tick: vec2<u32>,
    @location(1) end_tick: vec2<u32>,
    @location(2) y_height: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) item_id: vec2<u32>,
    @location(5) track_index: u32,
    @location(6) kind: u32,
) -> VertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let corner = corners[vertex_index];
    let origin = uniforms.origin_cursor.xy;
    let start_x = signed_tick_delta(start_tick, origin) * uniforms.view.z;
    let end_x = signed_tick_delta(end_tick, origin) * uniforms.view.z;
    var x = mix(start_x, end_x, corner.x) * uniforms.view.w;
    var width = (end_x - start_x) * uniforms.view.w;
    if kind == 0u {
        x = corner.x * uniforms.view.x;
        width = uniforms.view.x;
    } else if kind >= 3u {
        width = uniforms.view.w;
        if corner.x > 0.5 {
            x += select(1.0, 2.0, kind == 5u || kind == 6u);
        }
    } else {
        width = max(width, 3.0);
    }
    let logical_y = y_height.x + corner.y * y_height.y;
    let y = logical_y * uniforms.view.w;
    let screen_x = x;
    let screen_y = y;
    let ndc = vec2<f32>(
        2.0 * screen_x / max(uniforms.view.x, 1.0) - 1.0,
        1.0 - 2.0 * screen_y / max(uniforms.view.y, 1.0),
    );

    var output: VertexOutput;
    output.position = vec4<f32>(ndc, 0.0, 1.0);
    output.color = color;
    output.uv = vec2<f32>(corner.x * width / max(width, 1.0), corner.y);
    output.item_id = item_id;
    output.track_index = track_index;
    output.kind = kind;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var color = input.color;
    if input.kind == 0u && input.track_index == uniforms.state.x {
        color = vec4<f32>(linearize_srgb_color(vec3<f32>(0.19, 0.23, 0.26)), 1.0);
    }
    var selected = false;
    if input.kind == 1u || input.kind == 2u {
        selected = all(input.item_id == uniforms.playhead_selection.zw);
    }
    if selected && (input.uv.x < 0.06 || input.uv.x > 0.94 || input.uv.y < 0.08 || input.uv.y > 0.92) {
        color = vec4<f32>(linearize_srgb_color(vec3<f32>(0.96, 0.72, 0.36)), 1.0);
    }
    return color;
}
