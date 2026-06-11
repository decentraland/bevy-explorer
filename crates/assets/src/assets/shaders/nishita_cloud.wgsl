// cloud sampling:

// find cloud_floor intersection
// step through
// at each point
//   sample once based on xy only -> take as density at lo + 0.25 * (hi-lo)
//   sample again based on xy only -> take as density at lo + 0.75 * (hi-lo)
//   smooth step to get actual density at point

struct Nishita {
    ray_origin: vec3<f32>,
    sun_position: vec3<f32>,
    sun_intensity: f32,
    planet_radius: f32,
    atmosphere_radius: f32,
    rayleigh_coefficient: vec3<f32>,
    rayleigh_scale_height: f32,
    mie_coefficient: f32,
    mie_scale_height: f32,
    mie_direction: f32,
    time: f32,
    cloudy: f32,
    tick: u32,
    sun_color: vec3<f32>,
    dir_light_intensity: f32,
    day: f32,
}

const PI: f32 = 3.141592653589793;
const ISTEPS: u32 = 16u;
const JSTEPS: u32 = 8u;

fn rsi(rd: vec3<f32>, r0: vec3<f32>, sr: f32) -> vec2<f32> {
    // ray-sphere intersection that assumes
    // the sphere is centered at the origin.
    // No intersection when result.x > result.y
    let a = dot(rd, rd);
    let b = 2.0 * dot(rd, r0);
    let c = dot(r0, r0) - (sr * sr);
    let d = (b * b) - (4.0 * a * c);

    if d < 0.0 {
        return vec2<f32>(1e5, -1e5);
    } else {
        return vec2<f32>(
            (-b - sqrt(d)) / (2.0 * a),
            (-b + sqrt(d)) / (2.0 * a)
        );
    }
}

fn render_nishita(r_full: vec3<f32>, r0: vec3<f32>, p_sun_full: vec3<f32>, i_sun: f32, r_planet: f32, r_atmos: f32, k_rlh: vec3<f32>, k_mie: f32, sh_rlh: f32, sh_mie: f32, g: f32) -> vec3<f32> {
    // Normalize the ray direction and sun position.
    let r = normalize(r_full);
    let p_sun = normalize(p_sun_full);

    // Calculate the step size of the primary ray.
    var p = rsi(r, r0, r_atmos);
    if p.x > p.y { return vec3<f32>(0f); }
    p.y = min(p.y, rsi(r, r0, r_planet).x);
    let i_step_size = (p.y - p.x) / f32(ISTEPS);

    // Initialize the primary ray depth.
    var i_depth = 0.0;

    // Initialize accumulators for Rayleigh and Mie scattering.
    var total_rlh = vec3<f32>(0f);
    var total_mie = vec3<f32>(0f);

    // Initialize optical depth accumulators for the primary ray.
    var i_od_rlh = 0f;
    var i_od_mie = 0f;

    // Calculate the Rayleigh and Mie phases.
    let mu = dot(r, p_sun);
    let mumu = mu * mu;
    let gg = g * g;
    let p_rlh = 3.0 / (16.0 * PI) * (1.0 + mumu);
    let p_mie = 3.0 / (8.0 * PI) * ((1.0 - gg) * (mumu + 1.0)) / (pow(1.0 + gg - 2.0 * mu * g, 1.5) * (2.0 + gg));

    // Sample the primary ray.
    for (var i = 0u; i < ISTEPS; i++) {
        // Calculate the primary ray sample position.
        let i_pos = r0 + r * (i_depth + i_step_size * 0.5);

        // Calculate the height of the sample.
        let i_height = length(i_pos) - r_planet;

        // Calculate the optical depth of the Rayleigh and Mie scattering for this step.
        let od_step_rlh = exp(-i_height / sh_rlh) * i_step_size;
        let od_step_mie = exp(-i_height / sh_mie) * i_step_size;

        // Accumulate optical depth.
        i_od_rlh += od_step_rlh;
        i_od_mie += od_step_mie;

        // Calculate the step size of the secondary ray.
        let j_step_size = rsi(p_sun, i_pos, r_atmos).y / f32(JSTEPS);

        // Initialize the secondary ray depth.
        var j_depth = 0f;

        // Initialize optical depth accumulators for the secondary ray.
        var j_od_rlh = 0f;
        var j_od_mie = 0f;

        // Sample the secondary ray.
        for (var j = 0u; j < JSTEPS; j++) {

            // Calculate the secondary ray sample position.
            let j_pos = i_pos + p_sun * (j_depth + j_step_size * 0.5);

            // Calculate the height of the sample.
            let j_height = length(j_pos) - r_planet;

            // Accumulate the optical depth.
            j_od_rlh += exp(-j_height / sh_rlh) * j_step_size;
            j_od_mie += exp(-j_height / sh_mie) * j_step_size;

            // Increment the secondary ray depth.
            j_depth += j_step_size;
        }

        // Calculate attenuation.
        let attn = exp(-(k_mie * (i_od_mie + j_od_mie) + k_rlh * (i_od_rlh + j_od_rlh)));

        // Accumulate scattering.
        total_rlh += od_step_rlh * attn;
        total_mie += od_step_mie * attn;

        // Increment the primary ray depth.
        i_depth += i_step_size;
    }

    // Calculate and return the final color.
    return i_sun * (p_rlh * k_rlh * total_rlh + p_mie * k_mie * total_mie);
}

@group(0) @binding(0) var<uniform> nishita: Nishita;
@group(0) @binding(1) var noise_texture: texture_2d<f32>;
@group(0) @binding(2) var noise_sampler: sampler;
// sky color cycles from godot-explorer sky.tres: x = time of day,
// rows: 0 zenith, 1 horizon, 2 nadir, 3 sun, 4 rim, 5 cloud, 6 cloud highlights
@group(0) @binding(3) var sky_lut: texture_2d<f32>;
@group(0) @binding(4) var sky_lut_sampler: sampler;
// godot painted cloud cubemap as a 6x1 face strip (+X,-X,+Y,-Y,+Z,-Z).
// R = cloud body, G = silhouette mask, B = sun-side highlight
@group(0) @binding(5) var clouds_strip: texture_2d<f32>;
@group(0) @binding(6) var clouds_sampler: sampler;

// sample the 6x1 cubemap strip with a direction vector
fn sample_cloud_cubemap(dir_in: vec3<f32>) -> vec3<f32> {
    let d = normalize(dir_in);
    let ax = abs(d.x);
    let ay = abs(d.y);
    let az = abs(d.z);
    var face: f32;
    var uv: vec2<f32>;
    if ax >= ay && ax >= az {
        if d.x > 0.0 { face = 0.0; uv = vec2(-d.z, -d.y) / ax; }
        else        { face = 1.0; uv = vec2(d.z, -d.y) / ax; }
    } else if ay >= az {
        if d.y > 0.0 { face = 2.0; uv = vec2(d.x, d.z) / ay; }
        else        { face = 3.0; uv = vec2(d.x, -d.z) / ay; }
    } else {
        if d.z > 0.0 { face = 4.0; uv = vec2(d.x, -d.y) / az; }
        else        { face = 5.0; uv = vec2(-d.x, -d.y) / az; }
    }
    let fuv = (uv * 0.5 + 0.5);
    // inset half a texel to avoid bleeding across faces in the strip
    let inset = clamp(fuv, vec2(0.002), vec2(0.998));
    let strip_uv = vec2((face + inset.x) / 6.0, inset.y);
    return textureSampleLevel(clouds_strip, clouds_sampler, strip_uv, 0.0).rgb;
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let safe = max(c, vec3(0.0));
    let lo = safe / 12.92;
    let hi = pow(max((safe + 0.055) / 1.055, vec3(1.192092896e-07)), vec3(2.4));
    return select(hi, lo, safe <= vec3(0.04045));
}

fn cycle(row: i32) -> vec3<f32> {
    let w = f32(textureDimensions(sky_lut).x);
    let x = fract(nishita.day) * w;
    let x0 = i32(floor(x)) % i32(w);
    let x1 = (x0 + 1) % i32(w);
    let c0 = textureLoad(sky_lut, vec2<i32>(x0, row), 0).rgb;
    let c1 = textureLoad(sky_lut, vec2<i32>(x1, row), 0).rgb;
    return mix(c0, c1, fract(x));
}

const PI_CONST: f32 = 3.141592653589793;



const CLOUD_LOWER: f32 = 3300.0;
const CLOUD_UPPER: f32 = 4800.0;

fn noise(x: vec2<f32>) -> f32 {
    return textureSampleLevel(noise_texture, noise_sampler, x / 65536.0, 0.0).r;
}

const m: mat3x3<f32> = mat3x3<f32>( 0.00,  0.80,  0.60,
                                   -0.80,  0.36, -0.48,
                                   -0.60, -0.48,  0.64 ) * 2.345;

fn FBM(p0: vec3<f32>) -> f32 {
	var p = p0;

    let speed = 2.0;

    p.x += nishita.time * 3.0 * speed;
	
	var f = 0.3750 * noise(p.xz); p = m*p; p.y -= nishita.time * 20.0 * speed;
	f += 0.3750   * noise(p.xz); p = m*p; p.y += nishita.time * 10.0 * speed;
	f += 0.1250   * noise(p.xz); p = m*p; p.x -= nishita.time * 10.0 * speed;
	f += 0.0625   * noise(p.xz); p = m*p;
	f += 0.03125  * noise(p.xz); p = m*p;
	f += 0.015625 * noise(p.xz);
    return f;
}

fn cloudy() -> f32 {
    return (nishita.cloudy - 0.5) * 0.666;
}

fn density(p: vec3<f32>) -> f32 {
	let density = clamp(FBM(p) * 0.5 + cloudy(), 0.0, 1.0);
    let cloud_range = CLOUD_UPPER - CLOUD_LOWER;
    let outside = clamp(abs(clamp(p.y, CLOUD_LOWER + 0.1 * cloud_range, CLOUD_UPPER - 0.1 * cloud_range) - p.y) / (0.1 * cloud_range), 0.0, 1.0);
    return mix(density, 0.0, outside);
}

fn lighting(p: vec3<f32>, dir: vec3<f32>, sun: vec3<f32>) -> f32 {
    let l = density(p + sun * 400.0);
    return clamp(pow(l, 0.25), 0.0, 1.0);
}

const STEPS: u32 = 20u;

fn render_cloud(sky: vec3<f32>, pos: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    let p_sun = normalize(nishita.sun_position);
    let sun_amount: f32 = max(dot(dir, p_sun), 0.0);

	var low = ((CLOUD_LOWER-pos.y) / dir.y);
	let high = ((CLOUD_UPPER-pos.y) / dir.y);
	    
    var p = pos + dir * low;

    var d = 0.0;
    let add = dir * ((high - low) / f32(STEPS));
    var shade_sum: vec2<f32> = vec2<f32>(0.0);

    let step_length = length(add);

    let density_cap = 0.8;
    for (var i = 0u; i < STEPS; i += 1u) {
        if shade_sum.y >= density_cap {
            break;
        }

        let base_density = clamp(cloudy(), 0.0, 1.0);
        let base_light = pow(base_density, 0.25) * base_density;

        let sample_density = density(p);
        let sample_light = lighting(p, dir, p_sun) * sample_density;

        let density = mix(sample_density, base_density, clamp(length(p.xz) / 100000.0, 0.0, 1.0));
        let light = mix(sample_light, base_light, clamp(length(p.xz) / 100000.0, 0.0, 1.0));

        shade_sum += vec2<f32>(light, density) * (vec2<f32>(1.0) - shade_sum.y);
        p += add;
    }

    shade_sum /= max(shade_sum.y, density_cap);

    let light_cloud_color_indirect = max(vec3(0.05), saturate(nishita.sun_color * 1.5) * min(1.0, nishita.dir_light_intensity / 5000.0));
    let light_cloud_color_direct = max(vec3(0.05), saturate(nishita.sun_color * 1.5) * min(nishita.dir_light_intensity / 1000.0, 4.0));
    let light_cloud_color = mix(light_cloud_color_indirect, light_cloud_color_direct, pow(smoothstep(0.8 + (0.1 * shade_sum.y), 1.0, sun_amount), 5.0));
    shade_sum.y = mix(shade_sum.y, sqrt(shade_sum.y), smoothstep(0.9, 1.0, sun_amount));

    let clouds = mix(light_cloud_color, vec3(0.05), pow(shade_sum.x, 3.0));
    let result = mix(sky, min(clouds, vec3<f32>(1.0)), shade_sum.y);
    return clamp(result, vec3<f32>(0.0), vec3<f32>(1.0));
}

@group(1) @binding(0)
var image: texture_storage_2d_array<rgba16float, write>;

fn hash13(p: vec3<f32>) -> f32 {
    // A common simple hash function
    var p3 = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    p3 = p3 + dot(p3, p3.yzx + 19.19);
    return fract((p3.x + p3.y) * p3.z);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) original_invocation_id: vec3<u32>, @builtin(num_workgroups) num_workgroups: vec3<u32>) {
    let size = textureDimensions(image).x;
    let scale = f32(size) / 2f;

    // dither pattern for updates
    let tick = nishita.tick & 63;

    var UPDATE_OFFSETS_X: array<u32,64> = array<u32,64>(
        0u, 4u, 4u, 0u, 2u, 6u, 6u, 2u, 2u, 6u, 6u, 2u, 0u, 4u, 4u, 0u,
        1u, 5u, 5u, 1u, 3u, 7u, 7u, 3u, 3u, 7u, 7u, 3u, 1u, 5u, 5u, 1u,
        1u, 5u, 5u, 1u, 3u, 7u, 7u, 3u, 3u, 7u, 7u, 3u, 1u, 5u, 5u, 1u,
        0u, 4u, 4u, 0u, 2u, 6u, 6u, 2u, 2u, 6u, 6u, 2u, 0u, 4u, 4u, 0u
    );

    var UPDATE_OFFSETS_Y: array<u32,64> = array<u32,64>(
        0u, 4u, 0u, 4u, 2u, 6u, 2u, 6u, 0u, 4u, 0u, 4u, 2u, 6u, 2u, 6u,
        1u, 5u, 1u, 5u, 3u, 7u, 3u, 7u, 1u, 5u, 1u, 5u, 3u, 7u, 3u, 7u,
        4u, 0u, 4u, 0u, 6u, 2u, 6u, 2u, 4u, 0u, 4u, 0u, 6u, 2u, 6u, 2u,
        5u, 1u, 5u, 1u, 7u, 3u, 7u, 3u, 5u, 1u, 5u, 1u, 7u, 3u, 7u, 3u
    );

    let invocation_id = original_invocation_id * vec3(8, 8, 1) + vec3(UPDATE_OFFSETS_X[tick], UPDATE_OFFSETS_Y[tick], 0);

    let dir = vec2<f32>((f32(invocation_id.x) / scale) - 1f, (f32(invocation_id.y) / scale) - 1f);

    var ray: vec3<f32>;

    switch invocation_id.z {
        case 0u {
            ray = vec3<f32>(1f, -dir.y, -dir.x); // +X
        }
        case 1u {
            ray = vec3<f32>(-1f, -dir.y, dir.x);// -X
        }
        case 2u {
            ray = vec3<f32>(dir.x, 1f, dir.y); // +Y
        }
        case 3u {
            ray = vec3<f32>(dir.x, -1f, -dir.y);// -Y
        }
        case 4u {
            ray = vec3<f32>(dir.x, -dir.y, 1f); // +Z
        }
        default: {
            ray = vec3<f32>(-dir.x, -dir.y, -1f);// -Z
        }
    }

    var initial_y = normalize(ray).y;
    if ray.y < 0.0 {
        ray.y = 0.0;
    }
    ray = normalize(ray);


    var render_base = vec3<f32>(0.0);

    let fwd = normalize(nishita.sun_position);
    let basis_u = vec3<f32>(0.0, sign(nishita.sun_position.z), 0.0);
    let right = normalize(cross(basis_u, fwd));
    let up = normalize(cross(fwd, right));
    let sun_transform = mat3x3(right, up, fwd);

    let multiplier = 1.0 + saturate(nishita.dir_light_intensity / 11000.0);

    // === godot-explorer sky model (sky.gdshader port) ===
    let day_factor = max(0.0, sin(nishita.day * 3.14159265));
    let eye_y = clamp(initial_y, -1.0, 1.0);
    let sun_dir = normalize(nishita.sun_position);
    let sun_dot = dot(ray, sun_dir);

    // tri-zone gradient: zenith / horizon / nadir blended on view height
    let zenith_w = smoothstep(0.25, 0.75, eye_y);
    let nadir_w = smoothstep(-0.25, -0.75, eye_y);
    let horizon_w = max(0.0, 1.0 - zenith_w - nadir_w);
    let zenith_tint = cycle(0);
    let horizon_tint = cycle(1);
    let nadir_tint = cycle(2);
    render_base = zenith_tint * zenith_w + horizon_tint * horizon_w + nadir_tint * nadir_w;

    // painted clouds (godot compositing: screen blends + per-TOD tint)
    let clouds_sample = sample_cloud_cubemap(ray);
    let clouds_mask = smoothstep(0.5, 0.85, clouds_sample.g);
    let cloud_tint = cycle(5);
    let sun_tint = cycle(3);
    let sun_chroma = normalize(max(sun_tint, vec3(1e-4)));
    let cloud_body = clouds_sample.r * cloud_tint;
    let cloud_highlight = clouds_sample.b * sun_chroma;
    let cloud_highlights = cycle(6).r;
    let inner_screen = vec3(1.0) - (vec3(1.0) - cloud_highlight) * (vec3(1.0) - cloud_body);
    let cloud_color = mix(cloud_body, inner_screen, cloud_highlights);
    let cloud_linear = srgb_to_linear(cloud_color);
    let outer_screen = vec3(1.0) - (vec3(1.0) - cloud_linear) * (vec3(1.0) - render_base);
    render_base = mix(render_base, outer_screen, clouds_mask);

    // celestial params: r = sun opacity, g = sun size, b = moon bite size
    let celestial_params = cycle(7);
    let sun_opacity = celestial_params.r;
    let remapped_sun_size = celestial_params.g * 0.35;
    let moon_mask_size = celestial_params.b;

    // sun disc with the moon's crescent bite carved out
    let sun_disc = step(cos(remapped_sun_size), sun_dot);
    let moon_mask_dir = normalize(sun_dir + vec3(0.01, -0.01, 0.0) * remapped_sun_size * 40.0);
    let moon_mask_dot = dot(ray, moon_mask_dir);
    let moon_mask_threshold = moon_mask_size * remapped_sun_size * 4.5;
    let moon_active = step(0.15, moon_mask_size);
    let moon_mask_step = mix(1.0, step(moon_mask_dot, cos(moon_mask_threshold)), moon_active);
    let celestial = sun_disc * moon_mask_step * sun_opacity * sun_opacity * day_factor;

    // disc is white-hot sun by day, tinted moon when the bite is active
    let moon_tint = cycle(8);
    let moon_influence = smoothstep(0.0, 0.01, moon_mask_size);
    let celestial_col = mix(vec3(2.0), moon_tint * 7.0, moon_influence);
    render_base += celestial_col * celestial * (1.0 - clouds_mask * 0.9);

    // real moon: tinted disc opposite the sun, visible at night
    let moon_dir = -sun_dir;
    let moon_night = 1.0 - day_factor;
    let moon_dot = dot(ray, moon_dir);
    let moon_disc = step(cos(0.035), moon_dot) * moon_night;
    render_base += moon_tint * 7.0 * moon_disc * (1.0 - clouds_mask * 0.9);
    // soft halo around the moon
    render_base += moon_tint * pow(max(moon_dot, 0.0), 64.0) * moon_night * 0.6;

    // atmospheric glow + radiance halo around the sun
    render_base += vec3(2.0) * smoothstep(0.9, 1.8, sun_dot) * sun_opacity * day_factor * 0.35;
    let radiance = pow(max(sun_dot, 0.0), 12.0) * (1.0 - abs(eye_y) * 0.5);
    render_base += cycle(4) * radiance * 0.05 * sun_opacity * day_factor;

    // asymmetric glowing horizon bands (hard limit at eye_y = 0)
    let above_gate = step(0.0, eye_y);
    let above_band = above_gate * exp(-max(eye_y, 0.0) * 65.0);
    let below_band = (1.0 - above_gate) * exp(-max(-eye_y, 0.0) * 55.0);
    render_base += horizon_tint * above_band;
    render_base = mix(render_base, nadir_tint, below_band);

    // overall energy/gamma from sky.tres (energy 1.0, gamma 0.9, x0.6)
    render_base = pow(max(render_base, vec3(0.0)), vec3(0.9)) * 0.6;

    // stars — gated by time of day, not light intensity (the night moon
    // light floor would otherwise keep them permanently off)
    let night_factor = 1.0 - max(0.0, sin(nishita.day * 3.14159265));
    if night_factor > 0.05 {
        for (var i=0u; i<1000u; i++) {
            let star_world_dir = normalize(
                vec3<f32>(
                    hash13(vec3::<f32>(17.5, 19.2, -888.2) * f32(i)), 
                    hash13(vec3::<f32>(117.5, 19.2, -888.2) * f32(i)), 
                    abs(hash13(vec3::<f32>(217.5, 19.2, -888.2) * f32(i)))
                ) * 2.0 - 1.0
            );
            let star_dir = sun_transform * star_world_dir;
            let stardirdot = dot(normalize(star_dir), ray);
            let hash = hash13(star_world_dir);
            // size range should vary based on resolution of the cubemap
            let size = 0.99995 + 0.000025 * pow(fract(hash * 100000.0), 0.25);
            if stardirdot > size {
                let color = vec3<f32>(0.25 + 0.75 * fract(hash * 1000.0), 0.625 + 0.375 * fract(hash * 1000.0), 1.0);
                let brightness = smoothstep(0.99995 + 0.000025, 0.99995, size);
                render_base += vec3<f32>(
                    color 
                    * night_factor) // night visibility
                    * brightness // star brightness
                    * pow(smoothstep(size, 1.0, stardirdot), 3.0)  // distance from middle of the star
                    * clamp(ray.y * 10.0, 0.0, 1.0); // distance above horizon
            }
        }
    }

    let render = render_cloud(render_base, nishita.ray_origin * 0.0, normalize(ray));

    let store_value = render;

    textureStore(
        image,
        vec2<i32>(invocation_id.xy),
        i32(invocation_id.z),
        vec4<f32>(store_value, 1.0)
    );
}
