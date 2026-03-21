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
    @location(4) border_width: f32,
    @location(5) border_color: vec4<f32>,
    @location(6) pixel_pos: vec2<f32>,
    @location(7) clip_rect: vec4<f32>,
};

struct RectInstance {
    @location(0) pos: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) border_radius: f32,
    @location(4) border_width: f32,
    @location(5) border_color: vec4<f32>,
    @location(6) clip_rect: vec4<f32>,
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
    out.border_width = inst.border_width;
    out.border_color = inst.border_color;
    out.pixel_pos = pixel_pos;
    out.clip_rect = inst.clip_rect;
    return out;
}

fn sdf_rounded_rect(p: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let r = min(radius, min(half_size.x, half_size.y));
    let q = abs(p) - half_size + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

/// Compute anti-aliased clip alpha from a clip rect in pixel coordinates.
/// clip_rect = (x, y, width, height). If width <= 0, clipping is disabled (returns 1.0).
fn clip_alpha(pixel_pos: vec2<f32>, clip_rect: vec4<f32>) -> f32 {
    if clip_rect.z <= 0.0 {
        return 1.0;
    }
    let clip_min = clip_rect.xy;
    let clip_max = clip_rect.xy + clip_rect.zw;

    // Signed distance from each edge (positive = inside)
    let d_left   = pixel_pos.x - clip_min.x;
    let d_right  = clip_max.x - pixel_pos.x;
    let d_top    = pixel_pos.y - clip_min.y;
    let d_bottom = clip_max.y - pixel_pos.y;

    // Smoothstep each edge for anti-aliasing (1px transition)
    let a_left   = smoothstep(0.0, 1.0, d_left);
    let a_right  = smoothstep(0.0, 1.0, d_right);
    let a_top    = smoothstep(0.0, 1.0, d_top);
    let a_bottom = smoothstep(0.0, 1.0, d_bottom);

    return a_left * a_right * a_top * a_bottom;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let half_size = in.rect_size * 0.5;
    let centered = in.local_pos - half_size;
    let d = sdf_rounded_rect(centered, half_size, in.border_radius);
    let outer_alpha = 1.0 - smoothstep(-0.5, 0.5, d);

    // Anti-aliased clip
    let ca = clip_alpha(in.pixel_pos, in.clip_rect);

    if in.border_width > 0.0 {
        // Inner SDF: shrink by border_width
        let inner_radius = max(in.border_radius - in.border_width, 0.0);
        let inner_half = half_size - vec2<f32>(in.border_width, in.border_width);
        let inner_d = sdf_rounded_rect(centered, inner_half, inner_radius);
        let inner_alpha = 1.0 - smoothstep(-0.5, 0.5, inner_d);

        // Border ring: outer minus inner
        let border_alpha = outer_alpha - inner_alpha;
        let bc = in.border_color;
        let fc = in.color;
        let border_premul = vec4<f32>(bc.rgb * bc.a, bc.a) * border_alpha;
        let fill_premul = vec4<f32>(fc.rgb * fc.a, fc.a) * inner_alpha;
        return (border_premul + fill_premul) * ca;
    } else {
        return vec4<f32>(in.color.rgb * in.color.a, in.color.a * outer_alpha) * ca;
    }
}
