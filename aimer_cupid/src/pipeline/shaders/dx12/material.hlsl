// Hand-authored HLSL for Cupid's native backdrop material pipeline.
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

ConstantBuffer<MaterialUniform> material : register(b0, space0);
Texture2D<float4> backdrop : register(t1, space0);
SamplerState backdrop_sampler : register(s2, space0);

struct MaterialVaryings {
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
    float2 pixel_pos : TEXCOORD1;
};

MaterialVaryings vs_main(uint vertex_id : SV_VertexID) {
    static const float2 corners[6] = {
        float2(0, 0), float2(1, 0), float2(0, 1),
        float2(1, 0), float2(1, 1), float2(0, 1)
    };
    float2 uv = corners[vertex_id];
    float2 pixel = material.bounds.xy + uv * material.bounds.zw;
    MaterialVaryings output;
    output.position = float4(pixel.x / max(material.viewport.x, 1.0) * 2.0 - 1.0,
                             1.0 - pixel.y / max(material.viewport.y, 1.0) * 2.0,
                             0.0, 1.0);
    output.uv = uv;
    output.pixel_pos = pixel;
    return output;
}

float selected_radius(float2 position, float4 radii) {
    float radius = radii.z;
    if (position.x < 0.0) {
        radius = position.y < 0.0 ? radii.x : radii.w;
    } else if (position.y < 0.0) {
        radius = radii.y;
    }
    return radius;
}

float rounded_rect_sdf(float2 position, float2 half_size, float4 radii) {
    float radius = min(selected_radius(position, radii), min(half_size.x, half_size.y));
    float2 q = abs(position) - half_size + radius;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
}

float sdf_alpha(float distance) {
    float antialias = max(fwidth(distance), 0.65);
    return 1.0 - smoothstep(-antialias, antialias, distance);
}

float liquid_shape_distance(float2 uv) {
    float2 size = max(material.bounds.zw, float2(1.0, 1.0));
    float2 half_size = size * 0.5;
    float2 local_position = (uv - 0.5) * size;
    float tip_pull = material.liquid.w;
    if (tip_pull > 0.0) {
        float vertical_t = saturate(local_position.y / max(half_size.y, 1.0) * 0.5 + 0.5);
        float taper = lerp(1.0 - tip_pull * 0.82, 1.0, vertical_t);
        local_position.x /= max(taper, 0.12);
    }
    float distance = rounded_rect_sdf(local_position, half_size, material.radii);
    float blob_amount = material.liquid.x;
    if (blob_amount > 0.0) {
        float seed = material.liquid.y * 6.28318;
        float angle = atan2(local_position.y, local_position.x);
        float wobble = sin(angle * 3.0 + seed) * 0.5
            + sin(angle * 5.0 - seed * 1.7) * 0.3
            + sin(angle * 7.0 + seed * 2.3) * 0.2;
        float amplitude = blob_amount * min(half_size.x, half_size.y) * 0.20;
        distance -= wobble * amplitude;
    }
    return distance;
}

float surface_alpha(float2 uv) {
    float distance;
    if (material.effect.x > 0.5) {
        distance = liquid_shape_distance(uv);
    } else {
        float2 size = max(material.bounds.zw, float2(1.0, 1.0));
        float2 local_position = (uv - 0.5) * size;
        distance = rounded_rect_sdf(local_position, size * 0.5, material.radii);
    }
    return sdf_alpha(distance);
}

float clip_alpha(float2 pixel) {
    float coverage = 1.0;
    if (material.clip_rect.z >= 0.0) {
        if (material.clip_rect.z <= 0.0 || material.clip_rect.w <= 0.0) {
            coverage = 0.0;
        } else {
            float2 half_size = material.clip_rect.zw * 0.5;
            float2 center = material.clip_rect.xy + half_size;
            coverage = sdf_alpha(rounded_rect_sdf(pixel - center, half_size,
                                                   material.clip_radii));
        }
    }
    return coverage;
}

float srgb_to_linear(float value) {
    return value <= 0.04045 ? value / 12.92
        : pow(max((value + 0.055) / 1.055, 0.0), 2.4);
}

float linear_to_srgb(float value) {
    return value <= 0.0031308 ? value * 12.92
        : 1.055 * pow(max(value, 0.0), 1.0 / 2.4) - 0.055;
}

float3 srgb_rgb_to_linear(float3 color) {
    return float3(srgb_to_linear(color.r), srgb_to_linear(color.g),
                  srgb_to_linear(color.b));
}

float3 linear_rgb_to_srgb(float3 color) {
    return float3(linear_to_srgb(color.r), linear_to_srgb(color.g),
                  linear_to_srgb(color.b));
}

float3 sample_backdrop(float2 pixel) {
    float2 valid_size = max(material.backdrop_rect.zw, float2(1.0, 1.0));
    float2 local_pixel = clamp(pixel - material.backdrop_rect.xy,
                               float2(0.0, 0.0), valid_size - 1.0);
    uint width;
    uint height;
    backdrop.GetDimensions(width, height);
    float2 dimensions = float2(width, height);
    return backdrop.Sample(backdrop_sampler,
                          (local_pixel + 0.5) / max(dimensions, float2(1.0, 1.0))).rgb;
}

float3 frosted_backdrop(float2 pixel) {
    float radius = material.detail.z;
    float3 total = sample_backdrop(pixel);
    if (radius > 0.0) {
        float reed = sin(pixel.x * 0.115) + sin(pixel.x * 0.037 + pixel.y * 0.021);
        float ripple = cos(pixel.y * 0.083 + reed * 0.7);
        float2 center = pixel + float2(reed * radius * 0.075, ripple * radius * 0.035);
        float axis = radius * 0.42;
        float diagonal = radius * 0.24;
        total = sample_backdrop(center) * 0.20;
        total += sample_backdrop(center + float2(axis, 0.0)) * 0.12;
        total += sample_backdrop(center - float2(axis, 0.0)) * 0.12;
        total += sample_backdrop(center + float2(0.0, axis)) * 0.12;
        total += sample_backdrop(center - float2(0.0, axis)) * 0.12;
        total += sample_backdrop(center + float2(diagonal, diagonal)) * 0.08;
        total += sample_backdrop(center + float2(diagonal, -diagonal)) * 0.08;
        total += sample_backdrop(center + float2(-diagonal, diagonal)) * 0.08;
        total += sample_backdrop(center - float2(diagonal, diagonal)) * 0.08;
    }
    return total;
}

float3 backdrop_to_material_rgb(float3 color) {
    float3 result = color;
    if (material.viewport.w > 0.5 && material.viewport.z > 0.5)
        result = linear_rgb_to_srgb(color);
    return result;
}

float3 adjust_backdrop(float3 color) {
    float luminance = dot(color, float3(0.2126, 0.7152, 0.0722));
    float3 saturated = lerp(luminance.xxx, color, material.light.x);
    float3 contrasted = (saturated - 0.5) * material.light.z + 0.5;
    return saturate(contrasted * material.light.y);
}

float3 glass_frosted_base(float3 sampled_backdrop) {
    float3 milky_tint = lerp(float3(0.86, 0.89, 0.92), material.tint.rgb, 0.72);
    float tint_strength = clamp(material.effect.y * material.tint.a * 0.76, 0.0, 0.82);
    return lerp(sampled_backdrop, milky_tint, tint_strength);
}

float3 glass_glow(float2 uv, float3 sampled_backdrop) {
    float2 blue_point = (uv - float2(0.60, 0.34)) * float2(1.35, 1.05);
    float blue = exp(-dot(blue_point, blue_point) * 3.2);
    float2 cyan_point = (uv - float2(0.18, 0.84)) * float2(1.5, 1.1);
    float cyan = exp(-dot(cyan_point, cyan_point) * 4.4);
    float diagonal = smoothstep(0.18, 0.92, uv.x + (1.0 - uv.y) * 0.32);
    float3 base = glass_frosted_base(sampled_backdrop);
    float3 highlight_color = lerp(float3(0.82, 0.91, 1.0), material.tint.rgb, 0.38);
    float highlight = (blue * 0.22 + cyan * 0.28 + diagonal * 0.08) * material.detail.x;
    return base + highlight_color * highlight;
}

float glass_rim(float2 uv) {
    float edge = min(min(uv.x, uv.y), min(1.0 - uv.x, 1.0 - uv.y));
    float rim = 1.0 - smoothstep(0.0, 0.075, edge);
    float top_sheen = 1.0 - smoothstep(0.0, 0.18, uv.y + uv.x * 0.18);
    return clamp(rim * 0.82 + top_sheen * 0.14, 0.0, 1.0);
}

float2 liquid_warp(float2 uv) {
    float phase = material.effect.z;
    float strength = material.effect.w;
    float interaction = material.detail.y;
    return float2(
        sin(uv.y * 25.0 + phase * 1.2 + interaction * 4.0) * 0.006
            + sin(uv.y * 8.0 - phase * 0.55) * 0.004,
        cos(uv.x * 22.0 - phase * 0.9 + interaction * 3.0) * 0.0055
            + cos(uv.x * 7.0 + phase * 0.45) * 0.0035) * strength;
}

float2 liquid_shape_normal(float2 uv) {
    const float epsilon = 0.0015;
    float dx = liquid_shape_distance(uv + float2(epsilon, 0.0))
        - liquid_shape_distance(uv - float2(epsilon, 0.0));
    float dy = liquid_shape_distance(uv + float2(0.0, epsilon))
        - liquid_shape_distance(uv - float2(0.0, epsilon));
    float2 gradient = float2(dx, dy);
    float gradient_length = length(gradient);
    float2 normal = float2(0.0, 0.0);
    if (gradient_length >= 1e-5) normal = gradient / gradient_length;
    return normal;
}

float liquid_bevel_slope(float inset, float bevel_radius) {
    float t = clamp(inset / bevel_radius, 0.0, 1.0);
    float u = 1.0 - t;
    return u / sqrt(max(1.0 - u * u, 1e-4));
}

float3 liquid_bevel_normal(float2 uv, float distance, float bevel_radius) {
    float inset = max(-distance, 0.0);
    float slope = liquid_bevel_slope(inset, bevel_radius);
    float2 outward = liquid_shape_normal(uv);
    return normalize(float3(outward * slope, 1.0));
}

float2 liquid_bevel_refract_offset(float3 normal, float eta, float depth) {
    float3 incident = float3(0.0, 0.0, -1.0);
    float cos_i = -dot(normal, incident);
    float sin2_t = eta * eta * max(1.0 - cos_i * cos_i, 0.0);
    float2 offset = -normal.xy * depth;
    if (sin2_t < 1.0) {
        float cos_t = sqrt(1.0 - sin2_t);
        float3 refracted = eta * incident + (eta * cos_i - cos_t) * normal;
        offset = refracted.xy / max(abs(refracted.z), 0.05) * depth;
    }
    return offset;
}

float3 liquid_chromatic_sample(float2 pixel, float3 normal, float eta,
                               float depth, float aberration) {
    float3 sample = float3(0.0, 0.0, 0.0);
    if (aberration <= 0.0) {
        float2 offset = liquid_bevel_refract_offset(normal, eta, depth);
        sample = backdrop_to_material_rgb(sample_backdrop(pixel + offset));
    } else {
        float spread = aberration * 0.05;
        float2 red_offset = liquid_bevel_refract_offset(normal, eta * (1.0 - spread), depth);
        float2 green_offset = liquid_bevel_refract_offset(normal, eta, depth);
        float2 blue_offset = liquid_bevel_refract_offset(normal, eta * (1.0 + spread), depth);
        float red = backdrop_to_material_rgb(sample_backdrop(pixel + red_offset)).r;
        float green = backdrop_to_material_rgb(sample_backdrop(pixel + green_offset)).g;
        float blue = backdrop_to_material_rgb(sample_backdrop(pixel + blue_offset)).b;
        sample = float3(red, green, blue);
    }
    return sample;
}

float liquid_rim(float distance) {
    float2 size = max(material.bounds.zw, float2(1.0, 1.0));
    float width = max(min(size.x, size.y) * 0.10, 3.0);
    float inner = saturate(-distance / width);
    return pow(1.0 - inner, 2.2);
}

float liquid_sheen(float2 uv, float rim) {
    float phase = material.effect.z;
    float interaction = material.detail.y;
    float seed = material.liquid.y;
    float light_angle = 3.9 + sin(seed * 6.28318) * 0.25
        + sin(phase * 0.2) * 0.10 + interaction * 0.15;
    float2 light_dir = float2(cos(light_angle), sin(light_angle));
    float2 outward = normalize(uv - float2(0.5, 0.5) + float2(0.0001, -0.0001));
    float facing = clamp(dot(outward, light_dir), 0.0, 1.0);
    return rim * pow(facing, 2.4);
}

float3 adaptive_tint(float3 sampled_backdrop) {
    float luminance = dot(sampled_backdrop, float3(0.2126, 0.7152, 0.0722));
    float3 lightened = lerp(material.tint.rgb, float3(1.0, 1.0, 1.0), 0.55);
    float3 darkened = lerp(material.tint.rgb, float3(0.0, 0.0, 0.0), 0.45);
    return lerp(lightened, darkened, luminance);
}

float3 material_rgb(float2 uv, float3 sampled_backdrop) {
    float3 color = float3(0.0, 0.0, 0.0);
    if (material.effect.x < 0.5) {
        float3 glow = glass_glow(uv, sampled_backdrop);
        float rim = glass_rim(uv);
        float border_mix = material.border_color.a * clamp(material.detail.w, 0.0, 1.0);
        float3 edge_color = lerp(material.tint.rgb, material.border_color.rgb, border_mix);
        float edge_strength = rim * material.light.w * 0.35;
        color = lerp(glow, edge_color, edge_strength);
    } else {
        float distance = liquid_shape_distance(uv);
        float rim = liquid_rim(distance);
        float sheen = liquid_sheen(uv, rim);
        float luminance = dot(sampled_backdrop, float3(0.2126, 0.7152, 0.0722));
        float tint_strength = clamp(material.tint.a * material.effect.y * 0.35, 0.0, 0.5);
        float3 tinted = lerp(sampled_backdrop, adaptive_tint(sampled_backdrop), tint_strength);
        float3 rim_tone = lerp(float3(1.0, 1.0, 1.0), float3(0.0, 0.0, 0.0), luminance);
        float3 rim_shaded = lerp(tinted, rim_tone, rim * material.light.w * 0.55);
        color = lerp(rim_shaded, float3(1.0, 1.0, 1.0), sheen * material.detail.x);
    }
    return color;
}

float material_alpha(float2 uv) {
    float opacity = material.effect.y * material.tint.a;
    float alpha = 0.0;
    if (material.effect.x < 0.5) {
        float rim = glass_rim(uv);
        if (material.viewport.w > 0.5 && material.detail.z > 0.0) alpha = 1.0;
        else alpha = opacity * (0.62 + rim * material.light.w * 0.30);
    } else {
        float distance = liquid_shape_distance(uv);
        float rim = liquid_rim(distance);
        float sheen = liquid_sheen(uv, rim);
        float base_alpha = lerp(0.8, 1.0, clamp(opacity, 0.0, 1.0));
        alpha = clamp(base_alpha + rim * material.light.w * 0.15
            + sheen * material.detail.x * 0.15, 0.0, 1.0);
    }
    return alpha;
}

float4 fs_main(MaterialVaryings input) : SV_Target0 {
    float3 softened = backdrop_to_material_rgb(frosted_backdrop(input.pixel_pos));
    float3 material_backdrop = softened;
    if (material.effect.x > 0.5) {
        float2 size = max(material.bounds.zw, float2(1.0, 1.0));
        float edge_distance = liquid_shape_distance(input.uv);
        float bevel_radius = max(material.liquid2.y, 1.0);
        float3 normal = liquid_bevel_normal(input.uv, edge_distance, bevel_radius);
        float magnification = material.liquid.z;
        float eta = 1.0 / (1.0 + magnification * 0.6);
        float depth = bevel_radius * (1.0 + magnification);
        float2 ripple_pixel = input.pixel_pos + liquid_warp(input.uv) * size;
        float3 refracted = liquid_chromatic_sample(ripple_pixel, normal, eta, depth,
                                                    material.liquid2.x);
        material_backdrop = lerp(softened, refracted, 0.78);
    }
    float3 sampled_backdrop = adjust_backdrop(material_backdrop);
    float3 rgb = saturate(material_rgb(input.uv, sampled_backdrop));
    float mask = surface_alpha(input.uv) * clip_alpha(input.pixel_pos);
    float alpha = saturate(material_alpha(input.uv) * mask);
    float3 converted = material.viewport.z > 0.5 ? srgb_rgb_to_linear(rgb) : rgb;
    return float4(converted * alpha, alpha);
}
