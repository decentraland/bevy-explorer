// Shared low-frequency tint: foliage roots and the distant turf meet naturally.
// No texture or artwork is imported from the reference renderer.
#ifdef LANDSCAPE_COAST
@group(2) @binding(108) var meadow_texture: texture_2d<f32>;
@group(2) @binding(109) var meadow_sampler: sampler;
#else
@group(2) @binding(8) var meadow_texture: texture_2d<f32>;
@group(2) @binding(9) var meadow_sampler: sampler;
#endif

fn turf_sample(position: vec2<f32>, detail: vec4<f32>) -> vec4<f32> {
    return textureSample(meadow_texture, meadow_sampler, position * (detail.x / 8.0));
}

#ifdef LANDSCAPE_COAST
fn turf_sample_grad(position: vec2<f32>, detail: vec4<f32>, dx: vec2<f32>, dy: vec2<f32>) -> vec4<f32> {
    let scale = detail.x / 8.0;
    return textureSampleGrad(meadow_texture, meadow_sampler, position * scale, dx * scale, dy * scale);
}
#endif

fn turf_normal(normal: vec3<f32>, sample: vec4<f32>, detail: vec4<f32>) -> vec3<f32> {
    // Relief is local material detail, not an exposure or global grading fix.
    // Zero strength really is flat; the selected meadow stays softly matte.
    return normalize(normal + vec3(sample.r - 0.5, 0.0, sample.g - 0.5) * (4.0 * detail.w));
}
fn ground_hash(p: vec2<f32>) -> f32 {
    let q = fract(vec3(p.xyx) * vec3(0.1031, 0.1030, 0.0973));
    let r = q + dot(q, q.yzx + vec3(33.33));
    return fract((r.x + r.y) * r.z);
}

fn ground_noise(p: vec2<f32>) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(ground_hash(cell), ground_hash(cell + vec2(1.0, 0.0)), u.x),
        mix(ground_hash(cell + vec2(0.0, 1.0)), ground_hash(cell + vec2(1.0)), u.x), u.y);
}

fn turf_color(root: vec3<f32>, tip: vec3<f32>, position: vec2<f32>, detail: vec4<f32>, sample: vec4<f32>) -> vec3<f32> {
    let broad = ground_noise(position * 0.23);
    // Contrast is centered around a stable mean. Lowering it must not make
    // the entire meadow darker or replace the fine flecks with cloudy blobs.
    let blend = 0.35 + (smoothstep(0.15, 0.85, broad) - 0.5) * detail.z * 0.45;
    let color = mix(root, tip, blend);
    let moss = sample.b * detail.y;
    return mix(color, color * vec3(0.48, 1.18, 0.73), moss) * sample.a;
}
