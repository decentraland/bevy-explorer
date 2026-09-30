// Sky environment map generation. The atmosphere cubemap (one mip) is
// box-downsampled into a small mipped cube, which is then convolved into a
// GGX-prefiltered specular cube (one perceptual roughness per mip) and a
// cosine-convolved irradiance cube for `EnvironmentMapLight`.

const PI: f32 = 3.141592653589793;

#ifdef DOWNSAMPLE

@group(0) @binding(0) var input_texture: texture_2d_array<f32>;
@group(0) @binding(1) var input_sampler: sampler;
@group(0) @binding(2) var output_texture: texture_storage_2d_array<rgba16float, write>;

// Each output texel averages a `scale x scale` block of input texels, using one
// bilinear tap per 2x2 sub-block.
@compute
@workgroup_size(8, 8, 1)
fn downsample(@builtin(global_invocation_id) id: vec3<u32>) {
    let out_size = textureDimensions(output_texture);
    if any(id.xy >= out_size) {
        return;
    }
    let in_size = textureDimensions(input_texture);
    let scale = in_size.x / out_size.x;
    let taps = max(scale / 2u, 1u);
    let texel = 1.0 / vec2<f32>(in_size);
    let origin = (vec2<f32>(id.xy * scale) + 1.0) * texel;

    var sum = vec4<f32>(0.0);
    for (var y = 0u; y < taps; y++) {
        for (var x = 0u; x < taps; x++) {
            let uv = origin + vec2<f32>(f32(x), f32(y)) * 2.0 * texel;
            sum += textureSampleLevel(input_texture, input_sampler, uv, id.z, 0.0);
        }
    }
    textureStore(output_texture, id.xy, id.z, sum / f32(taps * taps));
}

#else // DOWNSAMPLE

struct Constants {
    intensity: f32,
    // perceptual roughness of the specular mip being generated
    roughness: f32,
    sample_count: u32,
    // source mip sampled by the irradiance pass
    source_lod: f32,
}

@group(0) @binding(0) var source_cube: texture_cube<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var output_texture: texture_storage_2d_array<rgba16float, write>;
@group(0) @binding(3) var<uniform> constants: Constants;

// direction of cube texel (uv, face) as the hardware samples it
fn cube_texel_dir(uv: vec2<f32>, face: u32) -> vec3<f32> {
    let uvc = 2.0 * uv - 1.0;
    var dir: vec3<f32>;
    switch face {
        case 0u: { dir = vec3<f32>(1.0, -uvc.y, -uvc.x); }
        case 1u: { dir = vec3<f32>(-1.0, -uvc.y, uvc.x); }
        case 2u: { dir = vec3<f32>(uvc.x, 1.0, uvc.y); }
        case 3u: { dir = vec3<f32>(uvc.x, -1.0, -uvc.y); }
        case 4u: { dir = vec3<f32>(uvc.x, -uvc.y, 1.0); }
        default: { dir = vec3<f32>(-uvc.x, -uvc.y, -1.0); }
    }
    return normalize(dir);
}

// world direction the pbr shader reads from output texel (uv, face). bevy
// negates z when it samples environment cubemaps (the atmosphere skybox
// material does not), so the maps are authored with z flipped.
fn output_texel_dir(uv: vec2<f32>, face: u32) -> vec3<f32> {
    return cube_texel_dir(uv, face) * vec3<f32>(1.0, 1.0, -1.0);
}

// the pbr lookups only read the upper hemisphere (see sky_envmap_lookup.wgsl), so the outputs
// are generated for the upper half only: the +y face, the top half of the side faces plus one
// row below the horizon for bilinear filtering, and none of the -y face. dispatched with 5
// layers, skipping -y.
fn output_face(layer: u32) -> u32 {
    return select(layer, layer + 1u, layer >= 3u);
}

fn below_generated_half(row: u32, size: u32, face: u32) -> bool {
    return face != 2u && row > size / 2u;
}

fn sample_sky(dir: vec3<f32>, lod: f32) -> vec3<f32> {
    return textureSampleLevel(source_cube, source_sampler, dir, lod).rgb;
}

// first downsample, from the sky cube: box-filter each output texel's block of sky texels (one
// bilinear tap per 2x2), taking below-horizon texels from their mirror image above. the sky fades
// to black below the horizon; the mirror stands in for light coming back up off the ground, so
// the filter lobes and irradiance hemispheres that cross the horizon see sky on both sides.
@compute
@workgroup_size(8, 8, 1)
fn downsample_sky(@builtin(global_invocation_id) id: vec3<u32>) {
    let out_size = textureDimensions(output_texture);
    if any(id.xy >= out_size) {
        return;
    }
    let in_size = textureDimensions(source_cube, 0);
    let scale = in_size.x / out_size.x;
    let taps = max(scale / 2u, 1u);
    let texel = 1.0 / vec2<f32>(in_size);
    let origin = (vec2<f32>(id.xy * scale) + 1.0) * texel;

    var sum = vec3<f32>(0.0);
    for (var y = 0u; y < taps; y++) {
        for (var x = 0u; x < taps; x++) {
            let uv = origin + vec2<f32>(f32(x), f32(y)) * 2.0 * texel;
            let dir = cube_texel_dir(uv, id.z);
            let mirrored = vec3<f32>(dir.x, abs(dir.y), dir.z);
            sum += textureSampleLevel(source_cube, source_sampler, mirrored, 0.0).rgb;
        }
    }
    textureStore(output_texture, id.xy, id.z, vec4<f32>(sum / f32(taps * taps), 1.0));
}

fn tangent_frame(n: vec3<f32>) -> mat3x3<f32> {
    let up = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 1.0), abs(n.z) < 0.999);
    let t = normalize(cross(up, n));
    let b = cross(n, t);
    return mat3x3<f32>(t, b, n);
}

fn hammersley_2d(i: u32, n: u32) -> vec2<f32> {
    return vec2<f32>(f32(i) / f32(n), f32(reverseBits(i)) * 2.3283064365386963e-10);
}

// per-texel offset applied to the hammersley sequence (cranley-patterson rotation)
// so neighbouring texels don't share sample directions. deterministic, so the
// result is stable frame to frame.
fn texel_noise(coords: vec2<u32>, face: u32) -> vec2<f32> {
    var state = (coords.x * 3966231743u) ^ (coords.y * 3928936651u) ^ (face * 1199786941u);
    state = state * 747796405u + 2891336453u;
    let a = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    state = state * 747796405u + 2891336453u;
    let b = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return vec2<f32>(f32((a >> 22u) ^ a), f32((b >> 22u) ^ b)) * 2.3283064365386963e-10;
}

fn d_ggx(alpha: f32, ndoth: f32) -> f32 {
    let a2 = alpha * alpha;
    let d = ndoth * ndoth * (a2 - 1.0) + 1.0;
    return a2 / (PI * d * d);
}

// bounded vndf sampling (Eto & Tokuyoshi 2023) specialised to view == normal,
// in tangent space
fn sample_ggx_half_vector(xi: vec2<f32>, alpha: f32) -> vec3<f32> {
    let a2 = alpha * alpha;
    let b = (1.0 - a2) / (1.0 + a2);
    let z = 1.0 - xi.y * (1.0 + b);
    let sin_theta = sqrt(max(0.0, 1.0 - z * z));
    let phi = 2.0 * PI * xi.x;
    let h_std = vec3<f32>(sin_theta * cos(phi), sin_theta * sin(phi), z + 1.0);
    return normalize(vec3<f32>(alpha * h_std.xy, h_std.z));
}

fn ggx_pdf(alpha: f32, ndoth: f32) -> f32 {
    let a2 = alpha * alpha;
    let k = (1.0 - a2) / (1.0 + a2);
    return d_ggx(alpha, ndoth) / (2.0 * (k + 1.0));
}

// filtered importance sampling: pick the source mip whose texel solid angle
// matches the solid angle covered by one sample
fn source_lod_for(pdf: f32, samples: f32) -> f32 {
    let width = f32(textureDimensions(source_cube, 0).x);
    let omega_s = 1.0 / (samples * pdf);
    let omega_p = 4.0 * PI / (6.0 * width * width);
    return max(0.5 * log2(omega_s / omega_p), 0.0);
}

@compute
@workgroup_size(8, 8, 1)
fn specular(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(output_texture);
    let face = output_face(id.z);
    if any(id.xy >= size) || below_generated_half(id.y, size.y, face) {
        return;
    }
    let uv = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(size);
    let normal = output_texel_dir(uv, face);

    // mirror level: straight copy
    if constants.roughness < 0.01 {
        let color = sample_sky(normal, 0.0);
        textureStore(output_texture, id.xy, face, vec4<f32>(color * constants.intensity, 1.0));
        return;
    }

    let alpha = constants.roughness * constants.roughness;
    let frame = tangent_frame(normal);
    let noise = texel_noise(id.xy, face);
    let samples = constants.sample_count;

    var radiance = vec3<f32>(0.0);
    var total_weight = 0.0;
    for (var i = 0u; i < samples; i++) {
        let xi = fract(hammersley_2d(i, samples) + noise);
        let h = sample_ggx_half_vector(xi, alpha);
        // reflect the view (== normal, +z) about h
        let l = 2.0 * h.z * h - vec3<f32>(0.0, 0.0, 1.0);
        let ndotl = l.z;
        if ndotl <= 0.0 {
            continue;
        }
        // smith g with ndotv == 1
        let k = alpha / 2.0;
        let g = ndotl / (ndotl * (1.0 - k) + k);
        let lod = source_lod_for(ggx_pdf(alpha, h.z), f32(samples));
        let color = sample_sky(frame * l, lod);
        radiance += color * ndotl * g;
        total_weight += ndotl * g;
    }
    if total_weight > 0.0 {
        radiance /= total_weight;
    }
    textureStore(output_texture, id.xy, face, vec4<f32>(radiance * constants.intensity, 1.0));
}

@compute
@workgroup_size(8, 8, 1)
fn irradiance(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(output_texture);
    let face = output_face(id.z);
    if any(id.xy >= size) || below_generated_half(id.y, size.y, face) {
        return;
    }
    let uv = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(size);
    let normal = output_texel_dir(uv, face);
    let frame = tangent_frame(normal);
    let noise = texel_noise(id.xy, face);
    let samples = constants.sample_count;

    // cosine-weighted hemisphere: the mean of the samples is irradiance / pi,
    // which is what the pbr shader multiplies the diffuse colour by
    var irradiance = vec3<f32>(0.0);
    for (var i = 0u; i < samples; i++) {
        let xi = fract(hammersley_2d(i, samples) + noise);
        let cos_theta = sqrt(1.0 - xi.y);
        let sin_theta = sqrt(xi.y);
        let phi = 2.0 * PI * xi.x;
        let l = vec3<f32>(sin_theta * cos(phi), sin_theta * sin(phi), cos_theta);
        irradiance += sample_sky(frame * l, constants.source_lod);
    }
    irradiance /= f32(samples);
    textureStore(output_texture, id.xy, face, vec4<f32>(irradiance * constants.intensity, 1.0));
}

#endif // DOWNSAMPLE
