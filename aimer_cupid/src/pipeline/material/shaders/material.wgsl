struct MaterialUniform {
    bounds: vec4<f32>,
    tint: vec4<f32>,
    border_color: vec4<f32>,
    shadow: vec4<f32>,
    // x: material kind (0 = Glass, 1 = Liquid)
    // y: opacity, z: animation phase, w: distortion strength
    effect: vec4<f32>,
    // x: saturation, y: brightness, z: contrast, w: edge lighting
    light: vec4<f32>,
    // x: specular, y: interaction, z: blur radius, w: border width
    detail: vec4<f32>,
    radii: vec4<f32>,
    clip_rect: vec4<f32>,
    clip_radii: vec4<f32>,
    // x: viewport width, y: viewport height, z: surface is sRGB,
    // w: a captured frame texture is bound
    viewport: vec4<f32>,
    // xy: capture origin in frame pixels, zw: valid captured extent
    backdrop_rect: vec4<f32>,
    // x: blob amount, y: blob seed, z: magnification, w: tip pull
    liquid: vec4<f32>,
    // x: chromatic aberration, y: bevel radius, z/w: reserved
    liquid2: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> material: MaterialUniform;

@group(0) @binding(1)
var backdrop: texture_2d<f32>;

@group(0) @binding(2)
var backdrop_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) pixel_pos: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );

    let uv = corners[vertex_index];
    let pixel = material.bounds.xy + uv * material.bounds.zw;
    let ndc = vec2<f32>(
        pixel.x / max(material.viewport.x, 1.0) * 2.0 - 1.0,
        1.0 - pixel.y / max(material.viewport.y, 1.0) * 2.0,
    );

    var output: VertexOutput;
    output.position = vec4<f32>(ndc, 0.0, 1.0);
    output.uv = uv;
    output.pixel_pos = pixel;
    return output;
}

fn selected_radius(point: vec2<f32>, radii: vec4<f32>) -> f32 {
    if point.x < 0.0 {
        if point.y < 0.0 {
            return radii.x;
        }
        return radii.w;
    }
    if point.y < 0.0 {
        return radii.y;
    }
    return radii.z;
}

fn sdf_rounded_rect(point: vec2<f32>, half_size: vec2<f32>, radii: vec4<f32>) -> f32 {
    let radius = min(selected_radius(point, radii), min(half_size.x, half_size.y));
    let q = abs(point) - half_size + vec2<f32>(radius, radius);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

fn sdf_alpha(distance: f32) -> f32 {
    let antialias = max(fwidth(distance), 0.65);
    return 1.0 - smoothstep(-antialias, antialias, distance);
}

// Signed distance for the Liquid droplet silhouette: a rounded rect tapered
// toward a teardrop tip (`tip_pull`) and perturbed by a low-frequency angular
// wobble (`blob_amount`/`blob_seed`) so the outline reads as an organic
// droplet instead of a plain rounded rectangle. The taper only narrows the
// shape inward from the surface bounds, so it never draws outside the
// material's captured backdrop region.
fn liquid_shape_distance(uv: vec2<f32>) -> f32 {
    let size = max(material.bounds.zw, vec2<f32>(1.0, 1.0));
    let half_size = size * 0.5;
    var point = (uv - vec2<f32>(0.5, 0.5)) * size;

    let tip_pull = material.liquid.w;
    if tip_pull > 0.0 {
        let vertical_t = clamp(point.y / max(half_size.y, 1.0) * 0.5 + 0.5, 0.0, 1.0);
        let taper = mix(1.0 - tip_pull * 0.82, 1.0, vertical_t);
        point.x = point.x / max(taper, 0.12);
    }

    var distance = sdf_rounded_rect(point, half_size, material.radii);

    let blob_amount = material.liquid.x;
    if blob_amount > 0.0 {
        let seed = material.liquid.y * 6.28318;
        let angle = atan2(point.y, point.x);
        let wobble = sin(angle * 3.0 + seed) * 0.5
            + sin(angle * 5.0 - seed * 1.7) * 0.3
            + sin(angle * 7.0 + seed * 2.3) * 0.2;
        let amplitude = blob_amount * min(half_size.x, half_size.y) * 0.20;
        distance -= wobble * amplitude;
    }

    return distance;
}

fn surface_alpha(uv: vec2<f32>) -> f32 {
    var distance: f32;
    if material.effect.x > 0.5 {
        distance = liquid_shape_distance(uv);
    } else {
        let size = max(material.bounds.zw, vec2<f32>(1.0, 1.0));
        let point = (uv - vec2<f32>(0.5, 0.5)) * size;
        distance = sdf_rounded_rect(point, size * 0.5, material.radii);
    }
    return sdf_alpha(distance);
}

fn clip_alpha(pixel: vec2<f32>) -> f32 {
    if material.clip_rect.z < 0.0 {
        return 1.0;
    }
    if material.clip_rect.z <= 0.0 || material.clip_rect.w <= 0.0 {
        return 0.0;
    }
    let point = pixel - (material.clip_rect.xy + material.clip_rect.zw * 0.5);
    let distance = sdf_rounded_rect(point, material.clip_rect.zw * 0.5, material.clip_radii);
    return sdf_alpha(distance);
}

fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        return value / 12.92;
    }
    return pow((value + 0.055) / 1.055, 2.4);
}

fn srgb_rgb_to_linear(color: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        srgb_to_linear(color.r),
        srgb_to_linear(color.g),
        srgb_to_linear(color.b),
    );
}

fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.0031308 {
        return value * 12.92;
    }
    return 1.055 * pow(value, 1.0 / 2.4) - 0.055;
}

fn linear_rgb_to_srgb(color: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        linear_to_srgb(color.r),
        linear_to_srgb(color.g),
        linear_to_srgb(color.b),
    );
}

fn sample_backdrop(pixel: vec2<f32>) -> vec3<f32> {
    let valid_size = max(material.backdrop_rect.zw, vec2<f32>(1.0, 1.0));
    let local_pixel = clamp(
        pixel - material.backdrop_rect.xy,
        vec2<f32>(0.0, 0.0),
        valid_size - vec2<f32>(1.0, 1.0),
    );
    let texture_size = vec2<f32>(textureDimensions(backdrop));
    return textureSample(
        backdrop,
        backdrop_sampler,
        (local_pixel + vec2<f32>(0.5, 0.5)) / max(texture_size, vec2<f32>(1.0, 1.0)),
    ).rgb;
}

fn frosted_backdrop(pixel: vec2<f32>) -> vec3<f32> {
    // Nine texture reads replace the old 17-tap cross. A small reeded offset
    // makes silhouettes visibly refract like patterned architectural glass,
    // while the symmetric taps provide stable diffusion without a second blur
    // texture or an unbounded loop.
    let radius = material.detail.z;
    if radius <= 0.0 {
        return sample_backdrop(pixel);
    }
    let reed = sin(pixel.x * 0.115) + sin(pixel.x * 0.037 + pixel.y * 0.021);
    let ripple = cos(pixel.y * 0.083 + reed * 0.7);
    let center = pixel + vec2<f32>(reed * radius * 0.075, ripple * radius * 0.035);
    let axis = radius * 0.42;
    let diagonal = radius * 0.24;
    var total = sample_backdrop(center) * 0.20;
    total += sample_backdrop(center + vec2<f32>(axis, 0.0)) * 0.12;
    total += sample_backdrop(center - vec2<f32>(axis, 0.0)) * 0.12;
    total += sample_backdrop(center + vec2<f32>(0.0, axis)) * 0.12;
    total += sample_backdrop(center - vec2<f32>(0.0, axis)) * 0.12;
    total += sample_backdrop(center + vec2<f32>(diagonal, diagonal)) * 0.08;
    total += sample_backdrop(center + vec2<f32>(diagonal, -diagonal)) * 0.08;
    total += sample_backdrop(center + vec2<f32>(-diagonal, diagonal)) * 0.08;
    total += sample_backdrop(center - vec2<f32>(diagonal, diagonal)) * 0.08;
    return total;
}

fn backdrop_to_material_rgb(color: vec3<f32>) -> vec3<f32> {
    // Sampling an sRGB texture returns linear values. The procedural material
    // fields are authored in display RGB, so move captured samples into that
    // same domain before mixing and convert the final result back at output.
    if material.viewport.w > 0.5 && material.viewport.z > 0.5 {
        return linear_rgb_to_srgb(color);
    }
    return color;
}

fn adjust_backdrop(color: vec3<f32>) -> vec3<f32> {
    let luminance = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    let saturated = mix(vec3<f32>(luminance, luminance, luminance), color, material.light.x);
    let contrasted = (saturated - vec3<f32>(0.5, 0.5, 0.5)) * material.light.z
        + vec3<f32>(0.5, 0.5, 0.5);
    return clamp(contrasted * material.light.y, vec3<f32>(0.0, 0.0, 0.0), vec3<f32>(1.0, 1.0, 1.0));
}

fn glass_frosted_base(sampled_backdrop: vec3<f32>) -> vec3<f32> {
    // Keep the milky diffusion neutral enough for the user tint to remain the
    // dominant hue all the way to the surface boundary.
    let milky_tint = mix(
        vec3<f32>(0.86, 0.89, 0.92),
        material.tint.rgb,
        0.72,
    );
    let tint_strength = clamp(material.effect.y * material.tint.a * 0.76, 0.0, 0.82);
    return mix(sampled_backdrop, milky_tint, tint_strength);
}

fn glass_glow(uv: vec2<f32>, sampled_backdrop: vec3<f32>) -> vec3<f32> {
    // The highlight follows the configured tint instead of injecting a fixed
    // cyan palette. `detail.x` is the public specular-highlight strength.
    let blue_point = (uv - vec2<f32>(0.60, 0.34)) * vec2<f32>(1.35, 1.05);
    let blue = exp(-dot(blue_point, blue_point) * 3.2);
    let cyan_point = (uv - vec2<f32>(0.18, 0.84)) * vec2<f32>(1.5, 1.1);
    let cyan = exp(-dot(cyan_point, cyan_point) * 4.4);
    let diagonal = smoothstep(0.18, 0.92, uv.x + (1.0 - uv.y) * 0.32);
    let glass_base = glass_frosted_base(sampled_backdrop);
    let highlight_color = mix(vec3<f32>(0.82, 0.91, 1.0), material.tint.rgb, 0.38);
    let highlight = (blue * 0.22 + cyan * 0.28 + diagonal * 0.08) * material.detail.x;
    return glass_base + highlight_color * highlight;
}

fn glass_rim(uv: vec2<f32>) -> f32 {
    let edge = min(min(uv.x, uv.y), min(1.0 - uv.x, 1.0 - uv.y));
    let rim = 1.0 - smoothstep(0.0, 0.075, edge);
    let top_sheen = 1.0 - smoothstep(0.0, 0.18, uv.y + uv.x * 0.18);
    return clamp(rim * 0.82 + top_sheen * 0.14, 0.0, 1.0);
}

// Apple's Liquid Glass is a stable lens, not a trembling water surface: this
// stays a faint shimmer detail layered under the edge refraction below, not
// the dominant effect, so even `distortion_strength = 1.0` reads as a subtle
// liveliness rather than smearing the backdrop.
fn liquid_warp(uv: vec2<f32>) -> vec2<f32> {
    let phase = material.effect.z;
    let strength = material.effect.w;
    let interaction = material.detail.y;
    return vec2<f32>(
        sin(uv.y * 25.0 + phase * 1.2 + interaction * 4.0) * 0.006
            + sin(uv.y * 8.0 - phase * 0.55) * 0.004,
        cos(uv.x * 22.0 - phase * 0.9 + interaction * 3.0) * 0.0055
            + cos(uv.x * 7.0 + phase * 0.45) * 0.0035,
    ) * strength;
}

// The outward 2D surface normal of the droplet's silhouette, approximated by
// the gradient of its signed distance field. Distance increases moving away
// from the shape, so this always points from the surface toward its
// exterior.
fn liquid_shape_normal(uv: vec2<f32>) -> vec2<f32> {
    let eps = 0.0015;
    let dx = liquid_shape_distance(uv + vec2<f32>(eps, 0.0))
        - liquid_shape_distance(uv - vec2<f32>(eps, 0.0));
    let dy = liquid_shape_distance(uv + vec2<f32>(0.0, eps))
        - liquid_shape_distance(uv - vec2<f32>(0.0, eps));
    let gradient = vec2<f32>(dx, dy);
    let gradient_length = length(gradient);
    if gradient_length < 1e-5 {
        return vec2<f32>(0.0, 0.0);
    }
    return gradient / gradient_length;
}

// The slope (d height / d inset) of a quarter-circle bevel profile at a
// given inset from the boundary: `height = bevel_radius * sqrt(1 - u^2)`
// where `u = 1 - inset / bevel_radius`. Vertical (near-infinite) right at
// the boundary — matching a real rounded edge meeting its vertical side
// wall — and `0` past `bevel_radius`, where the fake glass slab has fully
// flattened out into its undistorted "top".
fn liquid_bevel_slope(inset: f32, bevel_radius: f32) -> f32 {
    let t = clamp(inset / bevel_radius, 0.0, 1.0);
    let u = 1.0 - t;
    return u / sqrt(max(1.0 - u * u, 1e-4));
}

// The fake 3D surface normal of the glass's rounded-bevel profile at this
// point: `(0, 0, 1)` — straight toward the viewer, no bending — on the flat
// interior "top" of the slab, tilting toward the local outward 2D direction
// as the point approaches the boundary. This is what makes a doubly-curved
// rounded corner bend light into a full radial lens (the normal tilts in
// every direction around the corner) while a straight edge only bends it in
// one direction, like a fluted glass rod — the same distinction visible in
// the reference material's dramatic corner lensing versus its calmer flat
// runs, and it falls out of this model for free rather than needing an
// explicit corner/edge case.
fn liquid_bevel_normal(uv: vec2<f32>, distance: f32, bevel_radius: f32) -> vec3<f32> {
    let inset = max(-distance, 0.0);
    let slope = liquid_bevel_slope(inset, bevel_radius);
    let outward = liquid_shape_normal(uv);
    return normalize(vec3<f32>(outward * slope, 1.0));
}

// Bends a straight-through view ray using an approximate Snell's-law
// refraction through the bevel normal, returning the resulting backdrop
// sample displacement in logical pixels. `eta` is the (inverse) index of
// refraction: `1.0` is flat window glass (no bend); lower values bend more
// strongly, like denser glass. Total internal reflection (past the
// critical angle, at the steepest part of the bevel) falls back to the
// most extreme valid refraction instead of producing garbage.
fn liquid_bevel_refract_offset(normal: vec3<f32>, eta: f32, depth: f32) -> vec2<f32> {
    let incident = vec3<f32>(0.0, 0.0, -1.0);
    let cos_i = -dot(normal, incident);
    let sin2_t = eta * eta * max(1.0 - cos_i * cos_i, 0.0);
    if sin2_t >= 1.0 {
        return -normal.xy * depth;
    }
    let cos_t = sqrt(1.0 - sin2_t);
    let refracted = eta * incident + (eta * cos_i - cos_t) * normal;
    return refracted.xy / max(abs(refracted.z), 0.05) * depth;
}

// Real lenses show color fringing at their most-refractive edges because
// different wavelengths bend by slightly different amounts (dispersion): the
// index of refraction itself depends on wavelength. This runs the same
// bevel refraction three times with a slightly different `eta` per channel
// instead of an ad-hoc directional offset, so the fringe is a direct
// consequence of the same physical model rather than a separate effect
// layered on top.
fn liquid_chromatic_sample(pixel: vec2<f32>, normal: vec3<f32>, eta: f32, depth: f32, aberration: f32) -> vec3<f32> {
    if aberration <= 0.0 {
        let offset = liquid_bevel_refract_offset(normal, eta, depth);
        return backdrop_to_material_rgb(sample_backdrop(pixel + offset));
    }
    let spread = aberration * 0.05;
    let red_offset = liquid_bevel_refract_offset(normal, eta * (1.0 - spread), depth);
    let green_offset = liquid_bevel_refract_offset(normal, eta, depth);
    let blue_offset = liquid_bevel_refract_offset(normal, eta * (1.0 + spread), depth);
    let red = backdrop_to_material_rgb(sample_backdrop(pixel + red_offset)).r;
    let green = backdrop_to_material_rgb(sample_backdrop(pixel + green_offset)).g;
    let blue = backdrop_to_material_rgb(sample_backdrop(pixel + blue_offset)).b;
    return vec3<f32>(red, green, blue);
}

// Dark fresnel-style ring near the droplet's outline, matching how a real
// droplet's edge bends light away from the viewer.
fn liquid_rim(distance: f32) -> f32 {
    let size = max(material.bounds.zw, vec2<f32>(1.0, 1.0));
    let width = max(min(size.x, size.y) * 0.10, 3.0);
    let inner = clamp(-distance / width, 0.0, 1.0);
    return pow(1.0 - inner, 2.2);
}

// A crisp specular sheen traced along the droplet's curved rim toward a
// fixed key light — Apple's Liquid Glass reads as a thin bright arc that
// follows the silhouette, not a soft floating glow. Deliberately no
// gaussian "bokeh" blob: the sheen is masked by `rim` itself, so it only
// ever lights the part of the edge actually facing the light, and its
// falloff is a sharp power curve rather than a blurred disc.
fn liquid_sheen(uv: vec2<f32>, rim: f32) -> f32 {
    let phase = material.effect.z;
    let interaction = material.detail.y;
    let seed = material.liquid.y;
    // A fixed upper-left key light with a small deterministic drift, so the
    // sheen feels alive without turning into a moving light blob.
    let light_angle = 3.9 + sin(seed * 6.28318) * 0.25 + sin(phase * 0.2) * 0.10 + interaction * 0.15;
    let light_dir = vec2<f32>(cos(light_angle), sin(light_angle));
    let outward = normalize(uv - vec2<f32>(0.5, 0.5) + vec2<f32>(0.0001, -0.0001));
    let facing = clamp(dot(outward, light_dir), 0.0, 1.0);
    return rim * pow(facing, 2.4);
}

// Shifts the user's tint toward white over a dark backdrop and toward black
// over a light one, the way Apple's Liquid Glass material stays legible
// against anything sitting behind it instead of keeping one fixed shade.
fn adaptive_tint(sampled_backdrop: vec3<f32>) -> vec3<f32> {
    let luminance = dot(sampled_backdrop, vec3<f32>(0.2126, 0.7152, 0.0722));
    let lightened = mix(material.tint.rgb, vec3<f32>(1.0, 1.0, 1.0), 0.55);
    let darkened = mix(material.tint.rgb, vec3<f32>(0.0, 0.0, 0.0), 0.45);
    return mix(lightened, darkened, luminance);
}

fn material_rgb(uv: vec2<f32>, sampled_backdrop: vec3<f32>) -> vec3<f32> {
    if material.effect.x < 0.5 {
        let glow = glass_glow(uv, sampled_backdrop);
        let rim = glass_rim(uv);
        let border_mix = material.border_color.a * clamp(material.detail.w, 0.0, 1.0);
        let edge_color = mix(material.tint.rgb, material.border_color.rgb, border_mix);
        let edge_strength = rim * material.light.w * 0.35;
        return mix(glow, edge_color, edge_strength);
    }

    // A droplet stays close to the true refracted backdrop: it is shaped by
    // refraction/magnification, an adaptive fresnel rim, and a crisp
    // specular sheen, not by a painted-on water texture that would hide
    // what is behind it.
    let distance = liquid_shape_distance(uv);
    let rim = liquid_rim(distance);
    let sheen = liquid_sheen(uv, rim);
    let luminance = dot(sampled_backdrop, vec3<f32>(0.2126, 0.7152, 0.0722));
    let tint_strength = clamp(material.tint.a * material.effect.y * 0.35, 0.0, 0.5);
    let tinted = mix(sampled_backdrop, adaptive_tint(sampled_backdrop), tint_strength);
    // The rim glows toward white over dark content and shadows toward black
    // over light content, instead of always darkening — matching how a real
    // Liquid Glass edge catches or grounds against whatever sits behind it.
    let rim_tone = mix(vec3<f32>(1.0, 1.0, 1.0), vec3<f32>(0.0, 0.0, 0.0), luminance);
    let rim_shaded = mix(tinted, rim_tone, rim * material.light.w * 0.55);
    return mix(rim_shaded, vec3<f32>(1.0, 1.0, 1.0), sheen * material.detail.x);
}

fn material_alpha(uv: vec2<f32>) -> f32 {
    let opacity = material.effect.y * material.tint.a;
    if material.effect.x < 0.5 {
        let rim = glass_rim(uv);
        if material.viewport.w > 0.5 && material.detail.z > 0.0 {
            // The captured frosted backdrop is already in the source color.
            // Opaque compositing prevents the sharp destination from leaking
            // through the frosted surface.
            return 1.0;
        }
        return opacity * (0.62 + rim * material.light.w * 0.30);
    }
    // A droplet must show its OWN computed color (the true backdrop, warped
    // by refraction/magnification) rather than being alpha-blended against
    // whatever is already in the framebuffer underneath it — including this
    // same widget's own flat CPU fallback rect, painted moments earlier.
    // Blending the two would average the warp back out toward whatever was
    // there before, which is what made refraction invisible in practice.
    // Staying close to opaque here means the warp painted into `rgb` is what
    // the viewer actually sees; `opacity` still lets the user fade the
    // surface (toward a near-invisible, pure-refraction "clear glass" look
    // at low values) without reintroducing that wash-out.
    let distance = liquid_shape_distance(uv);
    let rim = liquid_rim(distance);
    let sheen = liquid_sheen(uv, rim);
    let base_alpha = mix(0.8, 1.0, clamp(opacity, 0.0, 1.0));
    return clamp(
        base_alpha + rim * material.light.w * 0.15 + sheen * material.detail.x * 0.15,
        0.0,
        1.0,
    );
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let softened = backdrop_to_material_rgb(frosted_backdrop(input.pixel_pos));
    var material_backdrop = softened;
    if material.effect.x > 0.5 {
        let size = max(material.bounds.zw, vec2<f32>(1.0, 1.0));
        let edge_distance = liquid_shape_distance(input.uv);
        let bevel_radius = max(material.liquid2.y, 1.0);
        let normal = liquid_bevel_normal(input.uv, edge_distance, bevel_radius);
        let magnification = material.liquid.z;
        let eta = 1.0 / (1.0 + magnification * 0.6);
        let depth = bevel_radius * (1.0 + magnification);
        let ripple_pixel = input.pixel_pos + liquid_warp(input.uv) * size;
        let refracted = liquid_chromatic_sample(ripple_pixel, normal, eta, depth, material.liquid2.x);
        material_backdrop = mix(softened, refracted, 0.78);
    }
    let sampled_backdrop = adjust_backdrop(material_backdrop);
    let rgb = clamp(material_rgb(input.uv, sampled_backdrop), vec3<f32>(0.0, 0.0, 0.0), vec3<f32>(1.0, 1.0, 1.0));
    let mask = surface_alpha(input.uv) * clip_alpha(input.pixel_pos);
    let alpha = clamp(material_alpha(input.uv) * mask, 0.0, 1.0);
    let converted = select(rgb, srgb_rgb_to_linear(rgb), material.viewport.z > 0.5);
    return vec4<f32>(converted * alpha, alpha);
}
