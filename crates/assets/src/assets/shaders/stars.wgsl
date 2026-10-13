// Stars, added onto the finished frame after depth of field so they stay pin sharp at screen
// resolution. Each pixel of clear night sky looks up the few star cells around its view
// direction on a cube grid and adds any star that lands within a pixel or two of it. The
// output is blended additively, so pixels with no star are discarded.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import bevy_render::view::View

struct Stars {
    // direction to the sun: the field turns with it through the day
    sun_position: vec3<f32>,
    // 0 by day, 1 at night
    night: f32,
    // linear brightness of the brightest star
    brightness: f32,
    // radius of the largest star in screen pixels
    size: f32,
    // chance a grid cell holds a star
    density: f32,
    // seconds, for the twinkle
    time: f32,
}

@group(0) @binding(0) var<uniform> view: View;
#ifdef MULTISAMPLED
@group(0) @binding(1) var depth_texture: texture_depth_multisampled_2d;
#else
@group(0) @binding(1) var depth_texture: texture_depth_2d;
#endif
// the sky cubemap: alpha is cloud cover
@group(0) @binding(2) var sky_texture: texture_cube<f32>;
@group(0) @binding(3) var sky_sampler: sampler;
@group(0) @binding(4) var<uniform> stars: Stars;
#ifdef FOCUS
// depth of field's transparent coverage (g): imposters and other transparents
#ifdef MULTISAMPLED
@group(0) @binding(5) var focus_texture: texture_multisampled_2d<f32>;
#else
@group(0) @binding(5) var focus_texture: texture_2d<f32>;
#endif
#endif

// cells along each edge of a cube face: 6 * 16 * 16 cells, about a thousand stars at
// density 0.65
const CELLS: f32 = 16.0;

// twinkle: each star pulses at its own fixed rate (radians per second) somewhere in this
// range, growing and brightening for the top of each cycle
const TWINKLE_RATE_MIN: f32 = 0.5;
const TWINKLE_RATE_MAX: f32 = 2.5;

fn hash13(p: vec3<f32>) -> f32 {
    var p3 = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    p3 = p3 + dot(p3, p3.yzx + 19.19);
    return fract((p3.x + p3.y) * p3.z);
}

fn hash33(p: vec3<f32>) -> vec3<f32> {
    var p3 = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    p3 = p3 + dot(p3, p3.yxz + 19.19);
    return fract((p3.xxy + p3.yzz) * p3.zyx);
}

// cube face (0..5) and face coordinates (0..1) of a direction
fn face_of(d: vec3<f32>) -> vec3<f32> {
    let a = abs(d);
    if a.x >= a.y && a.x >= a.z {
        return vec3<f32>(select(1.0, 0.0, d.x > 0.0), d.y / a.x * 0.5 + 0.5, d.z / a.x * 0.5 + 0.5);
    }
    if a.y >= a.z {
        return vec3<f32>(select(3.0, 2.0, d.y > 0.0), d.x / a.y * 0.5 + 0.5, d.z / a.y * 0.5 + 0.5);
    }
    return vec3<f32>(select(5.0, 4.0, d.z > 0.0), d.x / a.z * 0.5 + 0.5, d.y / a.z * 0.5 + 0.5);
}

// direction of face coordinates on a cube face (inverse of face_of)
fn direction_of(face: f32, uv: vec2<f32>) -> vec3<f32> {
    let s = uv * 2.0 - 1.0;
    switch u32(face) {
        case 0u: { return normalize(vec3<f32>(1.0, s.x, s.y)); }
        case 1u: { return normalize(vec3<f32>(-1.0, s.x, s.y)); }
        case 2u: { return normalize(vec3<f32>(s.x, 1.0, s.y)); }
        case 3u: { return normalize(vec3<f32>(s.x, -1.0, s.y)); }
        case 4u: { return normalize(vec3<f32>(s.x, s.y, 1.0)); }
        default: { return normalize(vec3<f32>(s.x, s.y, -1.0)); }
    }
}

// screen position of a world direction, or none if it's behind the camera
fn project(direction: vec3<f32>) -> vec3<f32> {
    let clip = view.clip_from_world * vec4<f32>(view.world_position + direction, 1.0);
    if clip.w <= 0.0 {
        return vec3<f32>(0.0, 0.0, -1.0);
    }
    let ndc = clip.xy / clip.w;
    let pixel = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5) * view.viewport.zw + view.viewport.xy;
    return vec3<f32>(pixel, 1.0);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let coord = vec2<i32>(in.position.xy);

    // only where nothing was drawn
    if stars.night <= 0.0 || textureLoad(depth_texture, coord, 0) != 0.0 {
        discard;
    }

    var visible = stars.night;
#ifdef FOCUS
    visible *= 1.0 - clamp(textureLoad(focus_texture, coord, 0).g, 0.0, 1.0);
#endif

    // the view direction of this pixel
    let ndc = vec2<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0);
    let world = view.world_from_clip * vec4<f32>(ndc, 1.0, 1.0);
    let direction = normalize(world.xyz / world.w - view.world_position);

    // fade at the horizon and behind clouds
    visible *= clamp(direction.y * 10.0, 0.0, 1.0);
    visible *= 1.0 - clamp(textureSampleLevel(sky_texture, sky_sampler, direction, 0.0).a, 0.0, 1.0);
    if visible <= 0.0 {
        discard;
    }

    // the star field is fixed to a frame that turns with the sun, so it wheels through the
    // night like the sky
    let fwd = normalize(stars.sun_position);
    let basis_u = vec3<f32>(0.0, sign(stars.sun_position.z), 0.0);
    let right = normalize(cross(basis_u, fwd));
    let up = normalize(cross(fwd, right));
    let sun_transform = mat3x3<f32>(right, up, fwd);
    let local_direction = transpose(sun_transform) * direction;

    let face = face_of(local_direction);
    let cell = floor(face.yz * CELLS);

    var light = vec3<f32>(0.0);
    for (var dy = -1.0; dy <= 1.0; dy += 1.0) {
        for (var dx = -1.0; dx <= 1.0; dx += 1.0) {
            let c = cell + vec2<f32>(dx, dy);
            if c.x < 0.0 || c.y < 0.0 || c.x >= CELLS || c.y >= CELLS {
                continue;
            }
            let seed = vec3<f32>(face.x * 131.0, c.x, c.y);
            let h = hash33(seed);
            if h.x > stars.density {
                continue;
            }
            // where in the cell the star sits, and how big and bright it is
            let star_uv = (c + vec2<f32>(h.y, h.z)) / CELLS;
            let star = project(sun_transform * direction_of(face.x, star_uv));
            if star.z < 0.0 {
                continue;
            }
            let t = hash13(seed + 7.0);
            // biased toward small stars; the largest are the brightest
            let rate = mix(TWINKLE_RATE_MIN, TWINKLE_RATE_MAX, hash13(seed + 11.0));
            let phase = hash13(seed + 13.0) * 6.2832;
            let twinkle = max(0.0, sin(stars.time * rate + phase) - 0.7);
            let size = stars.size * mix(0.5, 1.0, t * t) + twinkle;
            let brightness = stars.brightness * (mix(0.25, 1.0, t * t) + twinkle);
            let tint = vec3<f32>(0.25 + 0.75 * hash13(seed + 3.0), 0.625 + 0.375 * hash13(seed + 5.0), 1.0);

            let distance = length(in.position.xy - star.xy);
            // a flat core with a one-pixel edge
            let core = 1.0 - smoothstep(size - 0.5, size + 0.5, distance);
            light += tint * brightness * core;
        }
    }

    return vec4<f32>(light * visible, 0.0);
}
