//! Original shoreline shape shared with `coast_profile.wgsl`. Coordinates are
//! Unity X/Z metres, anchored to the nearest point of the playable rectangle.
use bevy::prelude::*;

/// Exterior crest and waterline radii. All variation stays outside core land.
pub(super) fn coast_profile(edge: Vec2) -> Vec2 {
    let a = (edge.dot(Vec2::new(0.013, 0.019)) + 0.7).sin();
    let b = (edge.dot(Vec2::new(0.041, -0.027)) + 2.2).sin();
    let c = (edge.dot(Vec2::new(0.007, -0.011)) - 0.4).sin();
    let crest = 8.0 + 4.0 * a + 2.0 * b;
    Vec2::new(crest, crest + 18.0 + 2.0 * c + b)
}

/// Signed distance from the varying waterline, using circular outer corners.
/// Geometry samples this field every four metres and at 9-degree arc intervals.
#[cfg(test)]
pub(super) fn coast_distance(unity: Vec2, bounds: Vec4) -> f32 {
    let edge = unity.clamp(bounds.xy(), bounds.zw());
    let outside = (bounds.xy() - unity)
        .max(unity - bounds.zw())
        .max(Vec2::ZERO);
    outside.length() - coast_profile(edge).y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_is_deterministic_bounded_and_has_broad_variation() {
        let mut low = Vec2::splat(f32::INFINITY);
        let mut high = Vec2::splat(f32::NEG_INFINITY);
        for x in -256..=256 {
            let edge = Vec2::new(x as f32 * 8.0, -496.0);
            let profile = coast_profile(edge);
            assert_eq!(profile, coast_profile(edge));
            assert!((2.0..=14.0).contains(&profile.x));
            assert!((15.0..=21.0).contains(&(profile.y - profile.x)));
            assert!((17.0..=35.0).contains(&profile.y));
            low = low.min(profile);
            high = high.max(profile);
            // Fine mesh sampling must not turn broad headlands into sawteeth.
            assert!(
                (coast_profile(edge + Vec2::X * 4.0) - profile)
                    .abs()
                    .max_element()
                    < 1.0
            );
        }
        assert!(high.x - low.x > 10.0);
        assert!(high.y - low.y > 10.0);
    }

    #[test]
    fn shader_and_cpu_profile_keep_the_same_three_wave_contract() {
        let shader = include_str!("coast_profile.wgsl");
        for expression in [
            "sin(dot(edge, vec2(0.013, 0.019)) + 0.7)",
            "sin(dot(edge, vec2(0.041, -0.027)) + 2.2)",
            "sin(dot(edge, vec2(0.007, -0.011)) - 0.4)",
            "8.0 + 4.0 * a + 2.0 * b",
            "crest + 18.0 + 2.0 * c + b",
            "clamp(unity, bounds.xy, bounds.zw)",
            "length(outside) - coast_profile(edge).y",
        ] {
            assert!(
                shader.contains(expression),
                "shoreline contract drift: {expression}"
            );
        }
        assert_eq!(shader.matches("sin(").count(), 3);
    }
}
