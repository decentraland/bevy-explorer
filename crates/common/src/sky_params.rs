//! Day-cycle color gradients for scene lighting: the directional ("sun")
//! light, the ambient fill, and the fog tint. Values were sampled from
//! reference-client captures and are keyed by normalized time of day
//! (0.0 = midnight, 0.5 = noon). Colors are linear HDR.

use bevy::math::Vec3;

/// A color ramp keyed by normalized time of day in `[0, 1)`. Stops are sorted
/// ascending; sampling wraps across midnight (last stop -> first stop).
pub struct Gradient(pub &'static [(f32, Vec3)]);

impl Gradient {
    pub fn sample(&self, t: f32) -> Vec3 {
        let stops = self.0;
        let t = t.rem_euclid(1.0);
        let first = stops.first().unwrap();
        let last = stops.last().unwrap();
        if t <= first.0 {
            // wrap midnight: blend from last stop to first
            let span = first.0 + (1.0 - last.0);
            let f = if span > 0.0 {
                (t + (1.0 - last.0)) / span
            } else {
                0.0
            };
            return last.1.lerp(first.1, f);
        }
        if t >= last.0 {
            let span = first.0 + (1.0 - last.0);
            let f = if span > 0.0 { (t - last.0) / span } else { 0.0 };
            return last.1.lerp(first.1, f);
        }
        for pair in stops.windows(2) {
            let (t0, c0) = pair[0];
            let (t1, c1) = pair[1];
            if t >= t0 && t <= t1 {
                return c0.lerp(c1, (t - t0) / (t1 - t0));
            }
        }
        last.1
    }
}

/// Directional ("sun"/"moon") light color over the day. The night stops are a
/// violet "moon" tint so the scene stays directional after dark.
pub const DIR_LIGHT: Gradient = Gradient(&[
    (0.05, Vec3::new(0.514, 0.388, 1.0)),
    (0.185, Vec3::new(1.0, 0.602, 0.632)),
    (0.333, Vec3::new(0.985, 0.864, 0.645)),
    (0.519, Vec3::new(1.0, 0.931, 0.692)),
    (0.683, Vec3::new(0.984, 0.863, 0.643)),
    (0.801, Vec3::new(1.0, 0.6, 0.631)),
    (1.0, Vec3::new(0.515, 0.387, 1.0)),
]);

/// Ambient fill color over the day.
pub const AMBIENT: Gradient = Gradient(&[
    (0.0, Vec3::new(0.354, 0.0, 1.0)),
    (0.25, Vec3::new(1.0, 0.597, 0.526)),
    (0.5, Vec3::new(0.519, 0.679, 0.738)),
    (0.7, Vec3::new(1.0, 0.5, 0.458)),
    (1.0, Vec3::new(0.353, 0.0, 1.0)),
]);

/// Fog tint over the day.
pub const FOG: Gradient = Gradient(&[
    (0.05, Vec3::new(0.239, 0.086, 0.471)),
    (0.133, Vec3::new(0.287, 0.278, 0.514)),
    (0.25, Vec3::new(0.764, 0.551, 0.575)),
    (0.503, Vec3::new(0.31, 0.556, 0.708)),
    (0.7, Vec3::new(0.66, 0.539, 0.514)),
    (0.873, Vec3::new(0.509, 0.156, 0.478)),
    (1.0, Vec3::new(0.24, 0.086, 0.472)),
]);
