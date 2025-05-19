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

    p.x += nishita.time * 20.0 * speed;
	
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

    let light_cloud_color_indirect = max(vec3(0.05), nishita.sun_color * min(1.0, nishita.dir_light_intensity / 5000.0));
    let light_cloud_color_direct = max(vec3(0.05), nishita.sun_color * min(nishita.dir_light_intensity / 1000.0, 4.0));
    let light_cloud_color = mix(light_cloud_color_indirect, light_cloud_color_direct, pow(smoothstep(0.8 + (0.1 * shade_sum.y), 1.0, sun_amount), 5.0));
    shade_sum.y = mix(shade_sum.y, sqrt(shade_sum.y), smoothstep(0.9, 1.0, sun_amount));

    let clouds = mix(light_cloud_color, vec3(0.05), shade_sum.x);
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

    if ray.y < 0.0 {
        textureStore(
            image,
            vec2<i32>(invocation_id.xy),
            i32(invocation_id.z),
            vec4<f32>(0.07074, 0.17261, 0.02899, 1.0),
        );

        return;
    }
    ray = normalize(ray);


    var render_base = vec3<f32>(0.0);

    let fwd = normalize(nishita.sun_position);
    let basis_u = vec3<f32>(0.0, sign(nishita.sun_position.z), 0.0);
    let right = normalize(cross(basis_u, fwd));
    let up = normalize(cross(fwd, right));
    let sun_transform = mat3x3(right, up, fwd);

    if nishita.dir_light_intensity > 0.0 {
        render_base = render_nishita(
            ray,
            nishita.ray_origin,
            nishita.sun_position,
            nishita.sun_intensity,
            nishita.planet_radius,
            nishita.atmosphere_radius,
            nishita.rayleigh_coefficient,
            nishita.mie_coefficient,
            nishita.rayleigh_scale_height,
            nishita.mie_scale_height,
            nishita.mie_direction,
        );


        // add sun
        let sun_weight = dot(ray, normalize(nishita.sun_position));
        if sun_weight >= 0.997 {
            render_base = max(render_base, mix(render_base, nishita.sun_color, smoothstep(0.997, 0.999, sun_weight)));
        }
        // sun 2 ..
        let angle = 0.3;
        let cosa = cos(angle);
        let sina = sin(angle);
        let sun_weight_2 = dot(ray, normalize(nishita.sun_position - vec3(0.3, 0.3, 0.3)));
        if sun_weight_2 >= 0.998 {
            render_base = max(render_base, mix(render_base, nishita.sun_color, smoothstep(0.999, 1.0, sun_weight_2)));
        }
    }

    // stars
    if nishita.dir_light_intensity < 1000.0 {
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
            let size = 0.99999 + 0.000005 * pow(fract(hash * 100000.0), 0.25);
            if stardirdot > size {
                let color = vec3<f32>(0.25 + 0.75 * fract(hash * 1000.0), 0.625 + 0.375 * fract(hash * 1000.0), 1.0);
                let brightness = smoothstep(1.0, 0.99999, size);
                render_base += vec3<f32>(
                    color 
                    * clamp(1.0 - nishita.dir_light_intensity / 1000.0, 0.0, 1.0)) // sun brightness
                    * brightness // star brightness
                    * pow(smoothstep(size, 1.0, stardirdot), 3.0)  // distance from middle of the star
                    * clamp(ray.y * 10.0, 0.0, 1.0); // distance above horizon
            }
        }
    }

    let render = render_cloud(render_base, nishita.ray_origin * 0.0, normalize(ray));

    textureStore(
        image,
        vec2<i32>(invocation_id.xy),
        i32(invocation_id.z),
        vec4<f32>(render, 1.0)
    );
}
