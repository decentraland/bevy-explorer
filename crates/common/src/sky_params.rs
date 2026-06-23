//! Day-cycle curves for scene lighting and sky: the directional ("sun") light,
//! the ambient fill, the fog tint, and the atmosphere rayleigh/mie coefficients.
//! Lighting values were sampled from reference-client captures; the atmosphere
//! curves were hand-tuned. All are keyed by normalized time of day
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
    (0.333, Vec3::new(0.985, 0.864, 0.745)),
    (0.519, Vec3::new(1.0, 0.931, 0.892)),
    (0.683, Vec3::new(0.984, 0.863, 0.743)),
    (0.801, Vec3::new(1.0, 0.6, 0.631)),
    (1.0, Vec3::new(0.515, 0.387, 1.0)),
]);

/// Ambient fill color over the day. The measured colors are pulled 20% toward
/// white (in linear space) so the sky hue doesn't wash environment albedo —
/// baked into the stops rather than applied as a runtime tint.
pub const AMBIENT: Gradient = Gradient(&[
    (0.0, Vec3::new(0.568, 0.485, 1.0)),
    (0.25, Vec3::new(1.0, 0.703, 0.658)),
    (0.5, Vec3::new(0.654, 0.758, 0.8)),
    (0.7, Vec3::new(1.0, 0.643, 0.619)),
    (1.0, Vec3::new(0.567, 0.485, 1.0)),
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

/// Atmosphere rayleigh coefficient (the sky's hue lever) over the day. Blue-
/// biased so the sky reads blue; the green channel drops at dawn/dusk to warm
/// the horizon. Interpolation across sunrise/sunset tracks the sun crossing
/// elevation 0 -> 0.25 (eased over day 0.25 -> 0.35 and 0.65 -> 0.75); night
/// holds the dusk value via the midnight wrap.
pub const RAYLEIGH: Gradient = Gradient(&[
    (0.25, Vec3::new(6.0e-6, 1.0e-6, 22.0e-6)),
    (0.35, Vec3::new(6.0e-6, 13.0e-6, 22.0e-6)),
    (0.65, Vec3::new(6.0e-6, 13.0e-6, 22.0e-6)),
    (0.75, Vec3::new(6.0e-6, 1.0e-6, 22.0e-6)),
]);

/// Flat night-sky colour added per-direction in the atmosphere shader by
/// `max(-1, -sun·ray) * 0.25 + 0.75`, so the night sky isn't pure black.
pub const NIGHT_SKY: Vec3 = Vec3::new(0.1, 0.05, 0.3);

/// A scalar ramp keyed by normalized time of day, sampled the same way as
/// [`Gradient`] (wraps across midnight).
pub struct Curve(pub &'static [(f32, f32)]);

impl Curve {
    pub fn sample(&self, t: f32) -> f32 {
        let stops = self.0;
        let t = t.rem_euclid(1.0);
        let first = stops.first().unwrap();
        let last = stops.last().unwrap();
        if t <= first.0 {
            let span = first.0 + (1.0 - last.0);
            let f = if span > 0.0 {
                (t + (1.0 - last.0)) / span
            } else {
                0.0
            };
            return last.1 + (first.1 - last.1) * f;
        }
        if t >= last.0 {
            let span = first.0 + (1.0 - last.0);
            let f = if span > 0.0 { (t - last.0) / span } else { 0.0 };
            return last.1 + (first.1 - last.1) * f;
        }
        for pair in stops.windows(2) {
            let (t0, c0) = pair[0];
            let (t1, c1) = pair[1];
            if t >= t0 && t <= t1 {
                return c0 + (c1 - c0) * (t - t0) / (t1 - t0);
            }
        }
        last.1
    }
}

/// Atmosphere mie (haze) coefficient over the day. Mie is a scalar, so it only
/// controls horizon glow intensity, not hue. Strong by day (~42e-6) for a hazy
/// horizon; floored low at dawn/dusk and through the night (the floor keeps the
/// night term alive — exactly-zero mie kills it). Tracks the same sunrise/sunset
/// elevation crossing as [`RAYLEIGH`].
pub const MIE: Curve = Curve(&[
    (0.25, 0.21e-6),
    (0.35, 42.0e-6),
    (0.65, 42.0e-6),
    (0.75, 0.21e-6),
]);
