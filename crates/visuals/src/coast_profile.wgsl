// Original smooth exterior outline. Keep the expressions in coast_profile.rs
// synchronized: sand wash, ocean foam and mesh vertices share this shoreline.
fn coast_profile(edge: vec2<f32>) -> vec2<f32> {
    let a = sin(dot(edge, vec2(0.013, 0.019)) + 0.7);
    let b = sin(dot(edge, vec2(0.041, -0.027)) + 2.2);
    let c = sin(dot(edge, vec2(0.007, -0.011)) - 0.4);
    let crest = 8.0 + 4.0 * a + 2.0 * b;
    return vec2(crest, crest + 18.0 + 2.0 * c + b);
}

fn coast_distance(unity: vec2<f32>, bounds: vec4<f32>) -> f32 {
    let edge = clamp(unity, bounds.xy, bounds.zw);
    let outside = max(max(bounds.xy - unity, unity - bounds.zw), vec2(0.0));
    return length(outside) - coast_profile(edge).y;
}
