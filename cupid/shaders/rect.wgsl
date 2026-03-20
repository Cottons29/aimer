struct Viewport {
    size: vec2<f32>,
};

@group(0) @binding(0) var<uniform> viewport: Viewport;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) local_pos: vec2<f32>,
    @location(2) rect_size: vec2<f32>,
    @location(3) border_radius: f32,
};

struct RectInstance {
    @location(0) pos: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) border_radius: f32,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: RectInstance) -> VertexOutput {
    // Generate a quad from 6 vertices (two triangles)
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

    // Convert pixel coordinates to NDC: x: [0, width] -> [-1, 1], y: [0, height] -> [1, -1]
    let ndc = vec2<f32>(
        (pixel_pos.x / viewport.size.x) * 2.0 - 1.0,
        1.0 - (pixel_pos.y / viewport.size.y) * 2.0,
    );

    var out: VertexOutput;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.color = inst.color;
    out.local_pos = corner * inst.size;
    out.rect_size = inst.size;
    out.border_radius = inst.border_radius;
    return out;
}

fn sdf_rounded_rect(p: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let r = min(radius, min(half_size.x, half_size.y));
    let q = abs(p) - half_size + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let half_size = in.rect_size * 0.5;
    let centered = in.local_pos - half_size;
    let d = sdf_rounded_rect(centered, half_size, in.border_radius);
    let alpha = 1.0 - smoothstep(-0.5, 0.5, d);
    return vec4<f32>(in.color.rgb * in.color.a, in.color.a * alpha);
}
