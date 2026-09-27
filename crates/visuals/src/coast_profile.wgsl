// Shoreline shape around the terrain bounds, shared with `coast.rs` (keep `coast_profile` in sync).
// Coordinates are unity x/z metres; `bounds` is (min x, min z, max x, max z) of the terrain.

// Cliff crest and waterline distances outward from the nearest point of the bounds.
fn coast_profile(edge: vec2<f32>) -> vec2<f32> {
    let a = sin(dot(edge, vec2(0.013, 0.019)) + 0.7);
    let b = sin(dot(edge, vec2(0.041, -0.027)) + 2.2);
    let c = sin(dot(edge, vec2(0.007, -0.011)) - 0.4);
    let crest = 8.0 + 4.0 * a + 2.0 * b;
    return vec2(crest, crest + 18.0 + 2.0 * c + b);
}

fn coast_outside(unity: vec2<f32>, bounds: vec4<f32>) -> f32 {
    return length(max(max(bounds.xy - unity, unity - bounds.zw), vec2(0.0)));
}

// Signed distance beyond the waterline, with circular outer corners.
fn coast_distance(unity: vec2<f32>, bounds: vec4<f32>) -> f32 {
    return coast_outside(unity, bounds) - coast_profile(clamp(unity, bounds.xy, bounds.zw)).y;
}

// Signed distance beyond the cliff crest, where the ground ends.
fn coast_crest_distance(unity: vec2<f32>, bounds: vec4<f32>) -> f32 {
    return coast_outside(unity, bounds) - coast_profile(clamp(unity, bounds.xy, bounds.zw)).x;
}

// The ground and grass end just past the cliff crest (bevy world x/z), which the cliff top meets
// below them, so the 4 m chords of the cliff mesh never open a gap.
fn beyond_coast(xz: vec2<f32>, bounds: vec4<f32>) -> bool {
    return coast_crest_distance(vec2(xz.x, -xz.y), bounds) > 0.3;
}
