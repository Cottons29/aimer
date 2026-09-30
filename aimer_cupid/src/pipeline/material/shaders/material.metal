#include <metal_stdlib>

using namespace metal;

struct MaterialUniform {
    float4 bounds;
    float4 tint;
    float4 border_color;
    float4 shadow;
    float4 effect;
    float4 light;
    float4 detail;
    float4 radii;
    float4 clip_rect;
    float4 clip_radii;
    float4 viewport;
    float4 backdrop_rect;
    float4 liquid;
    float4 liquid2;
};

constant float2 kMaterialCorners[6] = {
    float2(0.0, 0.0), float2(1.0, 0.0), float2(0.0, 1.0),
    float2(1.0, 0.0), float2(1.0, 1.0), float2(0.0, 1.0),
};

struct MaterialVaryings {
    float4 position [[position]];
    float2 uv [[user(loc0)]];
    float2 pixel_pos [[user(loc1)]];
};

vertex MaterialVaryings vs_main(uint vertex_id [[vertex_id]],
                                constant MaterialUniform& material [[buffer(0)]]) {
    float2 uv = kMaterialCorners[vertex_id];
    float2 pixel = material.bounds.xy + uv * material.bounds.zw;
    float2 ndc = float2(pixel.x / max(material.viewport.x, 1.0) * 2.0 - 1.0,
                        1.0 - pixel.y / max(material.viewport.y, 1.0) * 2.0);

    MaterialVaryings out;
    out.position = float4(ndc, 0.0, 1.0);
    out.uv = uv;
    out.pixel_pos = pixel;
    return out;
}

static float selected_radius(float2 point, float4 radii) {
    if (point.x < 0.0) {
        return point.y < 0.0 ? radii.x : radii.w;
    }
    return point.y < 0.0 ? radii.y : radii.z;
}

static float sdf_rounded_rect(float2 point, float2 half_size, float4 radii) {
    float radius = min(selected_radius(point, radii),
                       min(half_size.x, half_size.y));
    float2 q = abs(point) - half_size + float2(radius);
    return length(max(q, float2(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

static float sdf_alpha(float distance) {
    float antialias = max(fwidth(distance), 0.65);
    return 1.0 - smoothstep(-antialias, antialias, distance);
}

static float liquid_shape_distance(constant MaterialUniform& material, float2 uv) {
    float2 size = max(material.bounds.zw, float2(1.0));
    float2 half_size = size * 0.5;
    float2 point = (uv - float2(0.5)) * size;

    float tip_pull = material.liquid.w;
    if (tip_pull > 0.0) {
        float vertical_t = clamp(point.y / max(half_size.y, 1.0) * 0.5 + 0.5,
                                 0.0, 1.0);
        float taper = mix(1.0 - tip_pull * 0.82, 1.0, vertical_t);
        point.x /= max(taper, 0.12);
    }

    float distance = sdf_rounded_rect(point, half_size, material.radii);
    float blob_amount = material.liquid.x;
    if (blob_amount > 0.0) {
        float seed = material.liquid.y * 6.28318;
        float angle = atan2(point.y, point.x);
        float wobble = sin(angle * 3.0 + seed) * 0.5
            + sin(angle * 5.0 - seed * 1.7) * 0.3
            + sin(angle * 7.0 + seed * 2.3) * 0.2;
        float amplitude = blob_amount * min(half_size.x, half_size.y) * 0.20;
        distance -= wobble * amplitude;
    }
    return distance;
}

static float surface_alpha(constant MaterialUniform& material, float2 uv) {
    float distance;
    if (material.effect.x > 0.5) {
        distance = liquid_shape_distance(material, uv);
    } else {
        float2 size = max(material.bounds.zw, float2(1.0));
        float2 point = (uv - float2(0.5)) * size;
        distance = sdf_rounded_rect(point, size * 0.5, material.radii);
    }
    return sdf_alpha(distance);
}

static float clip_alpha(constant MaterialUniform& material, float2 pixel) {
    if (material.clip_rect.z < 0.0) {
        return 1.0;
    }
    if (material.clip_rect.z <= 0.0 || material.clip_rect.w <= 0.0) {
        return 0.0;
    }
    float2 point = pixel - (material.clip_rect.xy + material.clip_rect.zw * 0.5);
    float distance = sdf_rounded_rect(point, material.clip_rect.zw * 0.5,
                                      material.clip_radii);
    return sdf_alpha(distance);
}

static float srgb_to_linear(float value) {
    if (value <= 0.04045) {
        return value / 12.92;
    }
    return pow((value + 0.055) / 1.055, 2.4);
}

static float3 srgb_rgb_to_linear(float3 color) {
    return float3(srgb_to_linear(color.r), srgb_to_linear(color.g),
                  srgb_to_linear(color.b));
}

static float linear_to_srgb(float value) {
    if (value <= 0.0031308) {
        return value * 12.92;
    }
    return 1.055 * pow(value, 1.0 / 2.4) - 0.055;
}

static float3 linear_rgb_to_srgb(float3 color) {
    return float3(linear_to_srgb(color.r), linear_to_srgb(color.g),
                  linear_to_srgb(color.b));
}

static float3 sample_backdrop(constant MaterialUniform& material,
                              texture2d<float> backdrop,
                              sampler backdrop_sampler,
                              float2 pixel) {
    float2 valid_size = max(material.backdrop_rect.zw, float2(1.0));
    float2 local_pixel = clamp(pixel - material.backdrop_rect.xy,
                               float2(0.0), valid_size - float2(1.0));
    float2 texture_size = float2(float(backdrop.get_width()),
                                 float(backdrop.get_height()));
    return backdrop.sample(backdrop_sampler,
                           (local_pixel + float2(0.5)) / max(texture_size, float2(1.0))).rgb;
}

static float3 frosted_backdrop(constant MaterialUniform& material,
                               texture2d<float> backdrop,
                               sampler backdrop_sampler,
                               float2 pixel) {
    float radius = material.detail.z;
    if (radius <= 0.0) {
        return sample_backdrop(material, backdrop, backdrop_sampler, pixel);
    }
    float reed = sin(pixel.x * 0.115) + sin(pixel.x * 0.037 + pixel.y * 0.021);
    float ripple = cos(pixel.y * 0.083 + reed * 0.7);
    float2 center = pixel + float2(reed * radius * 0.075,
                                   ripple * radius * 0.035);
    float axis = radius * 0.42;
    float diagonal = radius * 0.24;
    float3 total = sample_backdrop(material, backdrop, backdrop_sampler, center) * 0.20;
    total += sample_backdrop(material, backdrop, backdrop_sampler,
                             center + float2(axis, 0.0)) * 0.12;
    total += sample_backdrop(material, backdrop, backdrop_sampler,
                             center - float2(axis, 0.0)) * 0.12;
    total += sample_backdrop(material, backdrop, backdrop_sampler,
                             center + float2(0.0, axis)) * 0.12;
    total += sample_backdrop(material, backdrop, backdrop_sampler,
                             center - float2(0.0, axis)) * 0.12;
    total += sample_backdrop(material, backdrop, backdrop_sampler,
                             center + float2(diagonal, diagonal)) * 0.08;
    total += sample_backdrop(material, backdrop, backdrop_sampler,
                             center + float2(diagonal, -diagonal)) * 0.08;
    total += sample_backdrop(material, backdrop, backdrop_sampler,
                             center + float2(-diagonal, diagonal)) * 0.08;
    total += sample_backdrop(material, backdrop, backdrop_sampler,
                             center - float2(diagonal, diagonal)) * 0.08;
    return total;
}

static float3 backdrop_to_material_rgb(constant MaterialUniform& material,
                                       float3 color) {
    if (material.viewport.w > 0.5 && material.viewport.z > 0.5) {
        return linear_rgb_to_srgb(color);
    }
    return color;
}

static float3 adjust_backdrop(constant MaterialUniform& material, float3 color) {
    float luminance = dot(color, float3(0.2126, 0.7152, 0.0722));
    float3 saturated = mix(float3(luminance), color, material.light.x);
    float3 contrasted = (saturated - float3(0.5)) * material.light.z + float3(0.5);
    return clamp(contrasted * material.light.y, float3(0.0), float3(1.0));
}

static float3 glass_frosted_base(constant MaterialUniform& material,
                                 float3 sampled_backdrop) {
    float3 milky_tint = mix(float3(0.86, 0.89, 0.92), material.tint.rgb, 0.72);
    float tint_strength = clamp(material.effect.y * material.tint.a * 0.76,
                                0.0, 0.82);
    return mix(sampled_backdrop, milky_tint, tint_strength);
}

static float3 glass_glow(constant MaterialUniform& material,
                         float2 uv,
                         float3 sampled_backdrop) {
    float2 blue_point = (uv - float2(0.60, 0.34)) * float2(1.35, 1.05);
    float blue = exp(-dot(blue_point, blue_point) * 3.2);
    float2 cyan_point = (uv - float2(0.18, 0.84)) * float2(1.5, 1.1);
    float cyan = exp(-dot(cyan_point, cyan_point) * 4.4);
    float diagonal = smoothstep(0.18, 0.92, uv.x + (1.0 - uv.y) * 0.32);
    float3 glass_base = glass_frosted_base(material, sampled_backdrop);
    float3 highlight_color = mix(float3(0.82, 0.91, 1.0), material.tint.rgb, 0.38);
    float highlight = (blue * 0.22 + cyan * 0.28 + diagonal * 0.08)
        * material.detail.x;
    return glass_base + highlight_color * highlight;
}

static float glass_rim(float2 uv) {
    float edge = min(min(uv.x, uv.y), min(1.0 - uv.x, 1.0 - uv.y));
    float rim = 1.0 - smoothstep(0.0, 0.075, edge);
    float top_sheen = 1.0 - smoothstep(0.0, 0.18, uv.y + uv.x * 0.18);
    return clamp(rim * 0.82 + top_sheen * 0.14, 0.0, 1.0);
}

static float2 liquid_warp(constant MaterialUniform& material, float2 uv) {
    float phase = material.effect.z;
    float strength = material.effect.w;
    float interaction = material.detail.y;
    return float2(
        sin(uv.y * 25.0 + phase * 1.2 + interaction * 4.0) * 0.006
            + sin(uv.y * 8.0 - phase * 0.55) * 0.004,
        cos(uv.x * 22.0 - phase * 0.9 + interaction * 3.0) * 0.0055
            + cos(uv.x * 7.0 + phase * 0.45) * 0.0035) * strength;
}

static float2 liquid_shape_normal(constant MaterialUniform& material, float2 uv) {
    float eps = 0.0015;
    float dx = liquid_shape_distance(material, uv + float2(eps, 0.0))
        - liquid_shape_distance(material, uv - float2(eps, 0.0));
    float dy = liquid_shape_distance(material, uv + float2(0.0, eps))
        - liquid_shape_distance(material, uv - float2(0.0, eps));
    float2 gradient = float2(dx, dy);
    float gradient_length = length(gradient);
    if (gradient_length < 1e-5) {
        return float2(0.0);
    }
    return gradient / gradient_length;
}

static float liquid_bevel_slope(float inset, float bevel_radius) {
    float t = clamp(inset / bevel_radius, 0.0, 1.0);
    float u = 1.0 - t;
    return u / sqrt(max(1.0 - u * u, 1e-4));
}

static float3 liquid_bevel_normal(constant MaterialUniform& material,
                                  float2 uv,
                                  float distance,
                                  float bevel_radius) {
    float inset = max(-distance, 0.0);
    float slope = liquid_bevel_slope(inset, bevel_radius);
    float2 outward = liquid_shape_normal(material, uv);
    return normalize(float3(outward * slope, 1.0));
}

static float2 liquid_bevel_refract_offset(float3 normal, float eta, float depth) {
    float3 incident = float3(0.0, 0.0, -1.0);
    float cos_i = -dot(normal, incident);
    float sin2_t = eta * eta * max(1.0 - cos_i * cos_i, 0.0);
    if (sin2_t >= 1.0) {
        return -normal.xy * depth;
    }
    float cos_t = sqrt(1.0 - sin2_t);
    float3 refracted = eta * incident + (eta * cos_i - cos_t) * normal;
    return refracted.xy / max(abs(refracted.z), 0.05) * depth;
}

static float3 liquid_chromatic_sample(constant MaterialUniform& material,
                                      texture2d<float> backdrop,
                                      sampler backdrop_sampler,
                                      float2 pixel,
                                      float3 normal,
                                      float eta,
                                      float depth,
                                      float aberration) {
    if (aberration <= 0.0) {
        float2 offset = liquid_bevel_refract_offset(normal, eta, depth);
        return backdrop_to_material_rgb(material,
            sample_backdrop(material, backdrop, backdrop_sampler, pixel + offset));
    }
    float spread = aberration * 0.05;
    float2 red_offset = liquid_bevel_refract_offset(normal, eta * (1.0 - spread), depth);
    float2 green_offset = liquid_bevel_refract_offset(normal, eta, depth);
    float2 blue_offset = liquid_bevel_refract_offset(normal, eta * (1.0 + spread), depth);
    float red = backdrop_to_material_rgb(material,
        sample_backdrop(material, backdrop, backdrop_sampler, pixel + red_offset)).r;
    float green = backdrop_to_material_rgb(material,
        sample_backdrop(material, backdrop, backdrop_sampler, pixel + green_offset)).g;
    float blue = backdrop_to_material_rgb(material,
        sample_backdrop(material, backdrop, backdrop_sampler, pixel + blue_offset)).b;
    return float3(red, green, blue);
}

static float liquid_rim(constant MaterialUniform& material, float distance) {
    float2 size = max(material.bounds.zw, float2(1.0));
    float width = max(min(size.x, size.y) * 0.10, 3.0);
    float inner = clamp(-distance / width, 0.0, 1.0);
    return pow(1.0 - inner, 2.2);
}

static float liquid_sheen(constant MaterialUniform& material, float2 uv, float rim) {
    float phase = material.effect.z;
    float interaction = material.detail.y;
    float seed = material.liquid.y;
    float light_angle = 3.9 + sin(seed * 6.28318) * 0.25
        + sin(phase * 0.2) * 0.10 + interaction * 0.15;
    float2 light_dir = float2(cos(light_angle), sin(light_angle));
    float2 outward = normalize(uv - float2(0.5) + float2(0.0001, -0.0001));
    float facing = clamp(dot(outward, light_dir), 0.0, 1.0);
    return rim * pow(facing, 2.4);
}

static float3 adaptive_tint(constant MaterialUniform& material,
                            float3 sampled_backdrop) {
    float luminance = dot(sampled_backdrop, float3(0.2126, 0.7152, 0.0722));
    float3 lightened = mix(material.tint.rgb, float3(1.0), 0.55);
    float3 darkened = mix(material.tint.rgb, float3(0.0), 0.45);
    return mix(lightened, darkened, luminance);
}

static float3 material_rgb(constant MaterialUniform& material,
                           float2 uv,
                           float3 sampled_backdrop) {
    if (material.effect.x < 0.5) {
        float3 glow = glass_glow(material, uv, sampled_backdrop);
        float rim = glass_rim(uv);
        float border_mix = material.border_color.a * clamp(material.detail.w, 0.0, 1.0);
        float3 edge_color = mix(material.tint.rgb, material.border_color.rgb, border_mix);
        float edge_strength = rim * material.light.w * 0.35;
        return mix(glow, edge_color, edge_strength);
    }

    float distance = liquid_shape_distance(material, uv);
    float rim = liquid_rim(material, distance);
    float sheen = liquid_sheen(material, uv, rim);
    float luminance = dot(sampled_backdrop, float3(0.2126, 0.7152, 0.0722));
    float tint_strength = clamp(material.tint.a * material.effect.y * 0.35, 0.0, 0.5);
    float3 tinted = mix(sampled_backdrop, adaptive_tint(material, sampled_backdrop),
                        tint_strength);
    float3 rim_tone = mix(float3(1.0), float3(0.0), luminance);
    float3 rim_shaded = mix(tinted, rim_tone, rim * material.light.w * 0.55);
    return mix(rim_shaded, float3(1.0), sheen * material.detail.x);
}

static float material_alpha(constant MaterialUniform& material, float2 uv) {
    float opacity = material.effect.y * material.tint.a;
    if (material.effect.x < 0.5) {
        float rim = glass_rim(uv);
        if (material.viewport.w > 0.5 && material.detail.z > 0.0) {
            return 1.0;
        }
        return opacity * (0.62 + rim * material.light.w * 0.30);
    }
    float distance = liquid_shape_distance(material, uv);
    float rim = liquid_rim(material, distance);
    float sheen = liquid_sheen(material, uv, rim);
    float base_alpha = mix(0.8, 1.0, clamp(opacity, 0.0, 1.0));
    return clamp(base_alpha + rim * material.light.w * 0.15
                 + sheen * material.detail.x * 0.15, 0.0, 1.0);
}

fragment float4 fs_main(MaterialVaryings varyings [[stage_in]],
                        constant MaterialUniform& material [[buffer(0)]],
                        texture2d<float> backdrop [[texture(1)]],
                        sampler backdrop_sampler [[sampler(2)]]) {
    float3 softened = backdrop_to_material_rgb(material,
        frosted_backdrop(material, backdrop, backdrop_sampler, varyings.pixel_pos));
    float3 material_backdrop = softened;
    if (material.effect.x > 0.5) {
        float2 size = max(material.bounds.zw, float2(1.0));
        float edge_distance = liquid_shape_distance(material, varyings.uv);
        float bevel_radius = max(material.liquid2.y, 1.0);
        float3 normal = liquid_bevel_normal(material, varyings.uv,
                                            edge_distance, bevel_radius);
        float magnification = material.liquid.z;
        float eta = 1.0 / (1.0 + magnification * 0.6);
        float depth = bevel_radius * (1.0 + magnification);
        float2 ripple_pixel = varyings.pixel_pos
            + liquid_warp(material, varyings.uv) * size;
        float3 refracted = liquid_chromatic_sample(
            material, backdrop, backdrop_sampler, ripple_pixel, normal, eta,
            depth, material.liquid2.x);
        material_backdrop = mix(softened, refracted, 0.78);
    }
    float3 sampled_backdrop = adjust_backdrop(material, material_backdrop);
    float3 rgb = clamp(material_rgb(material, varyings.uv, sampled_backdrop),
                       float3(0.0), float3(1.0));
    float mask = surface_alpha(material, varyings.uv)
        * clip_alpha(material, varyings.pixel_pos);
    float alpha = clamp(material_alpha(material, varyings.uv) * mask, 0.0, 1.0);
    float3 converted = material.viewport.z > 0.5
        ? srgb_rgb_to_linear(rgb)
        : rgb;
    return float4(converted * alpha, alpha);
}
