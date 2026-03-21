struct Viewport {
    size: vec2<f32>,
};

@group(0) @binding(0) var<uniform> viewport: Viewport;
@group(1) @binding(0) var t_diffuse: texture_2d<f32>;
@group(1) @binding(1) var s_diffuse: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) pixel_pos: vec2<f32>,
    @location(2) clip_rect: vec4<f32>,
};

struct ImageInstance {
    @location(0) pos: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) uv_offset: vec2<f32>,
    @location(3) uv_scale: vec2<f32>,
    @location(4) clip_rect: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: ImageInstance) -> VertexOutput {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );

    let corner = corners[vi];
    let pixel_pos = inst.pos + corner * inst.size;

    let ndc = vec2<f32>(
        (pixel_pos.x / viewport.size.x) * 2.0 - 1.0,
        1.0 - (pixel_pos.y / viewport.size.y) * 2.0,
    );

    var out: VertexOutput;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = inst.uv_offset + corner * inst.uv_scale;
    out.pixel_pos = pixel_pos;
    out.clip_rect = inst.clip_rect;
    return out;
}

/// Compute anti-aliased clip alpha from a clip rect in pixel coordinates.
/// clip_rect = (x, y, width, height). If width <= 0, clipping is disabled (returns 1.0).
fn clip_alpha(pixel_pos: vec2<f32>, clip_rect: vec4<f32>) -> f32 {
    if clip_rect.z <= 0.0 {
        return 1.0;
    }
    let clip_min = clip_rect.xy;
    let clip_max = clip_rect.xy + clip_rect.zw;

    let d_left   = pixel_pos.x - clip_min.x;
    let d_right  = clip_max.x - pixel_pos.x;
    let d_top    = pixel_pos.y - clip_min.y;
    let d_bottom = clip_max.y - pixel_pos.y;

    let a_left   = smoothstep(0.0, 1.0, d_left);
    let a_right  = smoothstep(0.0, 1.0, d_right);
    let a_top    = smoothstep(0.0, 1.0, d_top);
    let a_bottom = smoothstep(0.0, 1.0, d_bottom);

    return a_left * a_right * a_top * a_bottom;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(t_diffuse, s_diffuse, in.uv);
    let ca = clip_alpha(in.pixel_pos, in.clip_rect);
    return color * ca;
}
