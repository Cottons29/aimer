struct Viewport {
    size: vec2<f32>,
};

@group(0) @binding(0) var<uniform> viewport: Viewport;
@group(1) @binding(0) var t_diffuse: texture_2d<f32>;
@group(1) @binding(1) var s_diffuse: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct ImageInstance {
    @location(0) pos: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) uv_offset: vec2<f32>,
    @location(3) uv_scale: vec2<f32>,
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
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_diffuse, s_diffuse, in.uv);
}
