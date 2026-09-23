// One original wash model for the beach and ocean: foam advances up the sand,
// retreats, and leaves a darker wet margin. No depth copy or transparency pass.
#import "embedded://visuals/coast_profile.wgsl"::coast_distance

fn surf_hash(cell: vec2<i32>) -> f32 {
    var h = bitcast<u32>(cell.x) * 1597334677u ^ bitcast<u32>(cell.y) * 3812015801u;
    h = (h ^ (h >> 16u)) * 2246822519u;
    h = h ^ (h >> 13u);
    return f32(h & 16777215u) / 16777216.0;
}

fn surf_noise(p: vec2<f32>) -> f32 {
    let cell = vec2<i32>(floor(p));
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(surf_hash(cell), surf_hash(cell + vec2(1, 0)), u.x),
        mix(surf_hash(cell + vec2(0, 1)), surf_hash(cell + vec2(1)), u.x), u.y);
}

// x = foam, y = thin wash, z = damp sand, w = shared shoreline distance.
// Returning distance avoids evaluating the three outline waves twice in water.
fn coast_surf(unity: vec2<f32>, bounds: vec4<f32>, time: f32, footprint: f32) -> vec4<f32> {
    let distance = coast_distance(unity, bounds);
    if distance < -7.0 || distance > 5.0 { return vec4(0.0, 0.0, 0.0, distance); }
    let edge = clamp(unity, bounds.xy, bounds.zw);
    let phase = time * 0.64 + sin(dot(edge, vec2(0.033, 0.027))) * 0.65;
    let pulse = max(sin(phase), 0.0);
    // Independent spatial scales avoid the evenly spaced white dashes made by
    // a sine mask. Only the narrow shore strip evaluates these two noise fields.
    let broad = surf_noise(unity * 0.32 + time * vec2(0.035, -0.028));
    // The existing minification filter is exactly 0.5 at/above this footprint.
    // Avoid four integer hashes for detail that the filter completely removes.
    var detail = 0.5;
    if footprint < 0.65 {
        detail = mix(surf_noise(unity * 1.7 + time * vec2(-0.11, 0.08)),
            0.5, smoothstep(0.15, 0.65, footprint));
    }
    let front = 0.8 - 4.4 * pulse * pulse
        + (broad - 0.5) * 1.3 + (detail - 0.5) * 0.35;
    let breakup = mix(smoothstep(0.3, 0.68, broad * 0.45 + detail * 0.55),
        0.4, smoothstep(0.4, 2.0, footprint));
    let width = 0.35 + broad * 0.45 + min(footprint, 0.65);
    let ribbon = 1.0 - smoothstep(width * 0.15, width, abs(distance - front));
    let trail = smoothstep(front, front + 0.5, distance)
        * (1.0 - smoothstep(front + 0.5, front + 2.0, distance));
    let foam = clamp(ribbon + trail * 0.35, 0.0, 1.0)
        * (0.12 + 0.78 * pulse) * breakup;
    let film = smoothstep(front - 0.3, front + 0.4, distance) * pulse;
    let damp = smoothstep(-6.0, -0.5, distance);
    return vec4(foam, film, damp, distance);
}
