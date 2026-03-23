struct Viewport {
    size: vec2<f32>,
};

@group(0) @binding(0) var<uniform> viewport: Viewport;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) local_pos: vec2<f32>,
    @location(2) rect_size: vec2<f32>,
    @location(3) border_radius: vec4<f32>,
    @location(4) border_width: vec4<f32>,
    @location(5) border_color: vec4<f32>,
    @location(6) pixel_pos: vec2<f32>,
    @location(7) clip_rect: vec4<f32>,
};

struct RectInstance {
    @location(0) pos: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) border_radius: vec4<f32>,
    @location(4) border_width: vec4<f32>,
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

/// SDF for a rounded rectangle with per-corner radii.
/// radii = (top-left, top-right, bottom-right, bottom-left)
fn sdf_rounded_rect(p: vec2<f32>, half_size: vec2<f32>, radii: vec4<f32>) -> f32 {
    // Select the radius for the quadrant the point is in
    var r: f32;
    if p.x < 0.0 {
        if p.y < 0.0 {
            r = radii.x; // top-left
        } else {
            r = radii.w; // bottom-left
        }
    } else {
        if p.y < 0.0 {
            r = radii.y; // top-right
        } else {
            r = radii.z; // bottom-right
        }
    }
    r = min(r, min(half_size.x, half_size.y));
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

// Convert a single sRGB channel to linear space.
fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

// Convert an sRGB color (with alpha) to linear space. Alpha is kept as-is.
fn srgb_color_to_linear(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(
        srgb_to_linear(c.r),
        srgb_to_linear(c.g),
        srgb_to_linear(c.b),
        c.a,
    );
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let half_size = in.rect_size * 0.5;
    let centered = in.local_pos - half_size;
    let d = sdf_rounded_rect(centered, half_size, in.border_radius);
    let outer_alpha = 1.0 - smoothstep(-0.5, 0.5, d);

    // Anti-aliased clip
    let ca = clip_alpha(in.pixel_pos, in.clip_rect);

    // Convert sRGB input colors to linear space for correct blending on sRGB surface
    let fill_color = srgb_color_to_linear(in.color);
    let stroke_color = srgb_color_to_linear(in.border_color);

    // border_width: (top, right, bottom, left)
    let has_border = (in.border_width.x + in.border_width.y + in.border_width.z + in.border_width.w) > 0.0;

    if has_border {
        let bw_top = in.border_width.x;
        let bw_right = in.border_width.y;
        let bw_bottom = in.border_width.z;
        let bw_left = in.border_width.w;

        // Inner rect center offset and half-size
        let inner_offset = vec2<f32>((bw_left - bw_right) * 0.5, (bw_top - bw_bottom) * 0.5);
        let inner_half = vec2<f32>(
            half_size.x - (bw_left + bw_right) * 0.5,
            half_size.y - (bw_top + bw_bottom) * 0.5,
        );

        // Inner radii: shrink each corner radius by the max of its two adjacent border widths
        let inner_radii = vec4<f32>(
            max(in.border_radius.x - max(bw_top, bw_left), 0.0),     // top-left
            max(in.border_radius.y - max(bw_top, bw_right), 0.0),    // top-right
            max(in.border_radius.z - max(bw_bottom, bw_right), 0.0), // bottom-right
            max(in.border_radius.w - max(bw_bottom, bw_left), 0.0),  // bottom-left
        );

        let inner_p = centered - inner_offset;
        let inner_d = sdf_rounded_rect(inner_p, max(inner_half, vec2<f32>(0.0, 0.0)), inner_radii);
        let inner_alpha = 1.0 - smoothstep(-0.5, 0.5, inner_d);

        // Border ring: outer minus inner
        let border_alpha = outer_alpha - inner_alpha;
        let bc = stroke_color;
        let fc = fill_color;
        let border_premul = vec4<f32>(bc.rgb * bc.a, bc.a) * border_alpha;
        let fill_premul = vec4<f32>(fc.rgb * fc.a, fc.a) * inner_alpha;
        return (border_premul + fill_premul) * ca;
    } else {
        return vec4<f32>(fill_color.rgb * fill_color.a, fill_color.a * outer_alpha) * ca;
    }
}
