struct Viewport {
    size: vec2<f32>,
    surface_is_srgb: f32,
    _pad: f32,
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
    @location(6) outline_width: vec4<f32>,
    @location(7) outline_color: vec4<f32>,
    @location(8) pixel_pos: vec2<f32>,
    @location(9) clip_rect: vec4<f32>,
    @location(10) clip_border_radius: vec4<f32>,
};

struct RectInstance {
    @location(0) pos: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) border_radius: vec4<f32>,
    @location(4) border_width: vec4<f32>,
    @location(5) border_color: vec4<f32>,
    @location(6) outline_width: vec4<f32>,
    @location(7) outline_color: vec4<f32>,
    @location(8) clip_rect: vec4<f32>,
    @location(9) clip_border_radius: vec4<f32>,
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
    out.outline_width = inst.outline_width;
    out.outline_color = inst.outline_color;
    out.pixel_pos = pixel_pos;
    out.clip_rect = inst.clip_rect;
    out.clip_border_radius = inst.clip_border_radius;
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
fn clip_alpha(pixel_pos: vec2<f32>, clip_rect: vec4<f32>, clip_radii: vec4<f32>) -> f32 {
    if clip_rect.z < 0.0 {
        return 1.0;
    }

    if clip_rect.z <= 0.01 || clip_rect.w <= 0.01 {
        return 0.0;
    }

    if clip_radii.x > 0.0 || clip_radii.y > 0.0 || clip_radii.z > 0.0 || clip_radii.w > 0.0 {
        // Rounded clip: use SDF
        let clip_center = clip_rect.xy + clip_rect.zw * 0.5;
        let clip_half = clip_rect.zw * 0.5;
        let p = pixel_pos - clip_center;
        let d = sdf_rounded_rect(p, clip_half, clip_radii);
        return 1.0 - smoothstep(-0.5, 0.5, d);
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

// Convert a single linear channel back to sRGB.
fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        return c * 12.92;
    }
    return 1.055 * pow(c, 1.0 / 2.4) - 0.055;
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

// Convert a linear color (with alpha) back to sRGB space.
fn linear_color_to_srgb(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(
        linear_to_srgb(c.r),
        linear_to_srgb(c.g),
        linear_to_srgb(c.b),
        c.a,
    );
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // The quad may have been expanded by outline_width on each side.
    let ow_top = in.outline_width.x;
    let ow_right = in.outline_width.y;
    let ow_bottom = in.outline_width.z;
    let ow_left = in.outline_width.w;
    let has_outline = (ow_top + ow_right + ow_bottom + ow_left) > 0.0;

    // Original rect size (before outline expansion)
    let orig_size = vec2<f32>(
        in.rect_size.x - ow_left - ow_right,
        in.rect_size.y - ow_top - ow_bottom,
    );
    let orig_half = orig_size * 0.5;

    // local_pos relative to the original rect
    let orig_local = in.local_pos - vec2<f32>(ow_left, ow_top);
    let orig_centered = orig_local - orig_half;

    // SDF for the original rect outer edge
    let d = sdf_rounded_rect(orig_centered, orig_half, in.border_radius);
    // Anti-aliasing with a small margin to ensure no gap
    let outer_alpha = 1.0 - smoothstep(-0.5, 0.5, d);

    // Anti-aliased clip
    let ca = clip_alpha(in.pixel_pos, in.clip_rect, in.clip_border_radius);

    // Always convert sRGB input colors to linear space for correct blending.
    let fill_color = srgb_color_to_linear(in.color);
    let stroke_color = srgb_color_to_linear(in.border_color);
    let ol_color = srgb_color_to_linear(in.outline_color);

    // Compute outline ring if needed
    var outline_premul = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    if has_outline {
        // Outline outer edge: expanded rect with expanded radii
        let expanded_half = in.rect_size * 0.5;
        let expanded_centered = in.local_pos - expanded_half;
        let outline_radii = vec4<f32>(
            in.border_radius.x + max(ow_top, ow_left),
            in.border_radius.y + max(ow_top, ow_right),
            in.border_radius.z + max(ow_bottom, ow_right),
            in.border_radius.w + max(ow_bottom, ow_left),
        );
        let outline_d = sdf_rounded_rect(expanded_centered, expanded_half, outline_radii);
        let outline_outer_alpha = 1.0 - smoothstep(-0.5, 0.5, outline_d);
        let outline_ring_alpha = clamp(outline_outer_alpha - outer_alpha, 0.0, 1.0);
        outline_premul = vec4<f32>(ol_color.rgb * ol_color.a, ol_color.a) * outline_ring_alpha;
    }

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
            orig_half.x - (bw_left + bw_right) * 0.5,
            orig_half.y - (bw_top + bw_bottom) * 0.5,
        );

        // Inner radii: shrink each corner radius by the max of its two adjacent border widths
        let inner_radii = vec4<f32>(
            max(in.border_radius.x - max(bw_top, bw_left), 0.0),     // top-left
            max(in.border_radius.y - max(bw_top, bw_right), 0.0),    // top-right
            max(in.border_radius.z - max(bw_bottom, bw_right), 0.0), // bottom-right
            max(in.border_radius.w - max(bw_bottom, bw_left), 0.0),  // bottom-left
        );

        let inner_p = orig_centered - inner_offset;
        let inner_d = sdf_rounded_rect(inner_p, max(inner_half, vec2<f32>(0.0, 0.0)), inner_radii);
        let inner_alpha = 1.0 - smoothstep(-0.5, 0.5, inner_d);

        // Border ring: outer minus inner
        let border_alpha = clamp(outer_alpha - inner_alpha, 0.0, 1.0);
        let bc = stroke_color;
        let fc = fill_color;
        
        // Ensure fill and border combined do not exceed outer_alpha
        let final_inner_alpha = min(inner_alpha, outer_alpha);
        
        let fill_premul = vec4<f32>(fc.rgb * fc.a, fc.a) * final_inner_alpha;
        let border_premul = vec4<f32>(bc.rgb * bc.a, bc.a) * border_alpha;
        
        // Combine layers. Since they are disjoint, addition is correct.
        var result = (outline_premul + border_premul + fill_premul) * ca;
        
        // If the surface is NOT sRGB, we must manually convert back to sRGB from linear
        // because WGPU won't do it for us.
        if viewport.surface_is_srgb < 0.5 {
            // result is currently premultiplied linear.
            // sRGB conversion should ideally happen on non-premultiplied color.
            let a = result.a;
            if a > 0.00001 {
                let unpremul = result.rgb / a;
                let srgb_rgb = vec3<f32>(linear_to_srgb(unpremul.r), linear_to_srgb(unpremul.g), linear_to_srgb(unpremul.b));
                result = vec4<f32>(srgb_rgb * a, a);
            }
        }
        return result;
    } else {
        let fill_premul = vec4<f32>(fill_color.rgb * fill_color.a, fill_color.a) * outer_alpha;
        var result = (outline_premul + fill_premul) * ca;

        // Same for the non-border case.
        if viewport.surface_is_srgb < 0.5 {
            let a = result.a;
            if a > 0.00001 {
                let unpremul = result.rgb / a;
                let srgb_rgb = vec3<f32>(linear_to_srgb(unpremul.r), linear_to_srgb(unpremul.g), linear_to_srgb(unpremul.b));
                result = vec4<f32>(srgb_rgb * a, a);
            }
        }
        return result;
    }
}
