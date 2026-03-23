// Minimal text shader — GLES/ANGLE safe.
// No @builtin(position) in fragment input to avoid `invariant` validation errors.

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct FragmentInput {
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct Viewport {
    resolution: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> viewport: Viewport;
@group(0) @binding(1) var atlas_texture: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;

// Per-instance data passed as vertex attributes:
//   position: vec2<f32>  (screen-space top-left)
//   size:     vec2<f32>  (quad width/height in pixels)
//   uv_rect:  vec4<f32>  (u_min, v_min, u_max, v_max)
//   color:    vec4<f32>  (text color with alpha)

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_idx: u32,
    @location(0) inst_pos: vec2<f32>,
    @location(1) inst_size: vec2<f32>,
    @location(2) inst_uv: vec4<f32>,
    @location(3) inst_color: vec4<f32>,
) -> VertexOutput {
    // Triangle list for a quad: vertices 0-5 map to corners.
    // 0(0,0) 1(1,0) 2(0,1) | 3(0,1) 4(1,0) 5(1,1)
    let tri_idx = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let c = tri_idx[vertex_idx];

    let pixel_pos = inst_pos + c * inst_size;
    let ndc = vec2<f32>(
        pixel_pos.x / viewport.resolution.x * 2.0 - 1.0,
        -(pixel_pos.y / viewport.resolution.y * 2.0 - 1.0),
    );

    var out: VertexOutput;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = mix(inst_uv.xy, inst_uv.zw, c);
    out.color = inst_color;
    return out;
}

// Convert a single sRGB channel to linear space.
fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

@fragment
fn fs_main(in: FragmentInput) -> @location(0) vec4<f32> {
    let alpha = textureSampleLevel(atlas_texture, atlas_sampler, in.uv, 0.0).r;
    let a = in.color.a * alpha;
    let r = srgb_to_linear(in.color.r);
    let g = srgb_to_linear(in.color.g);
    let b = srgb_to_linear(in.color.b);
    return vec4<f32>(r * a, g * a, b * a, a);
}
