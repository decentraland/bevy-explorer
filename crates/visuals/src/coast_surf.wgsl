// Shore wash shared by the sand and the sea: foam advances up the sand, retreats, and leaves a
// darker wet margin.

#import "embedded://visuals/coast_profile.wgsl"::coast_distance
#import "embedded://shaders/simplex.wgsl"::simplex_noise_3d

// simplex with time as the third axis: evolves in place as well as drifting, with no grid
fn surf_noise(p: vec2<f32>, time: f32) -> f32 {
    return simplex_noise_3d(vec3(p.x, time, p.y)) * 0.5 + 0.5;
}

// x = foam, y = thin wash, z = damp sand, w = distance beyond the waterline.
// `footprint` is the pixel size in metres, to fade detail the filter would remove.
fn coast_surf(unity: vec2<f32>, bounds: vec4<f32>, time: f32, footprint: f32) -> vec4<f32> {
    let distance = coast_distance(unity, bounds);
    if distance < -7.0 || distance > 5.0 {
        return vec4(0.0, 0.0, 0.0, distance);
    }
    let edge = clamp(unity, bounds.xy, bounds.zw);
    let phase = time * 0.64 + sin(dot(edge, vec2(0.033, 0.027))) * 0.65;
    let pulse = max(sin(phase), 0.0);
    // how far the front has surged, so the wash strengthens as the water moves rather than before
    let surge = pulse * pulse;
    // independent scales, so the foam doesn't break into evenly spaced dashes
    let broad = surf_noise(unity * 0.32 + time * vec2(0.035, -0.028), time * 0.15);
    var detail = 0.5;
    if footprint < 0.65 {
        detail = mix(surf_noise(unity * 1.7 + time * vec2(-0.11, 0.08), time * 0.5),
            0.5, smoothstep(0.15, 0.65, footprint));
    }
    let front = 0.8 - 4.4 * surge
        + (broad - 0.5) * 1.3 + (detail - 0.5) * 0.35;
    let breakup = mix(smoothstep(0.3, 0.68, broad * 0.45 + detail * 0.55),
        0.4, smoothstep(0.4, 2.0, footprint));
    let width = 0.35 + broad * 0.45 + min(footprint, 0.65);
    let ribbon = 1.0 - smoothstep(width * 0.15, width, abs(distance - front));
    let trail = smoothstep(front, front + 0.5, distance)
        * (1.0 - smoothstep(front + 0.5, front + 2.0, distance));
    // time as the third axis, so the foam boils in place rather than sliding; averages to 1
    let churn = mix(1.0 + simplex_noise_3d(vec3(unity.x * 0.9, time * 0.7, unity.y * 0.9)) * 0.85,
        1.0, smoothstep(0.25, 1.0, footprint));
    let foam = clamp(clamp(ribbon + trail * 0.35, 0.0, 1.0)
        * (0.06 + 0.84 * surge) * breakup * churn, 0.0, 1.0);
    let film = smoothstep(front - 0.3, front + 0.4, distance) * surge * churn;
    let damp = smoothstep(-6.0, -0.5, distance);
    return vec4(foam, film, damp, distance);
}
