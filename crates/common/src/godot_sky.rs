//! The godot-explorer skybox color cycles, extracted from
//! godot/assets/sky.tres (decentraland/godot-explorer). Each gradient is
//! keyed by normalized time of day (0.0 = midnight, 0.5 = noon) and values
//! are linear HDR (the sun cycle peaks far above 1.0 by design).

use bevy::math::Vec3;

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

/// sky straight up ("Gradient_zenit")
pub const ZENITH: Gradient = Gradient(&[
    (0.05, Vec3::new(0.259, 0.197, 0.507)),
    (0.2, Vec3::new(0.369, 0.399, 0.792)),
    (0.3, Vec3::new(0.52, 0.538, 0.896)),
    (0.5, Vec3::new(0.187, 0.601, 0.933)),
    (0.75, Vec3::new(0.49, 0.414, 0.887)),
    (1.0, Vec3::new(0.261, 0.199, 0.51)),
]);

/// sky at the horizon ("Gradient_horizon")
pub const HORIZON: Gradient = Gradient(&[
    (0.05, Vec3::new(0.293, 0.0, 0.44)),
    (0.194, Vec3::new(0.414, 0.372, 0.589)),
    (0.3, Vec3::new(1.0, 0.561, 0.524)),
    (0.38, Vec3::new(0.573, 0.792, 0.772)),
    (0.503, Vec3::new(0.676, 0.828, 0.962)),
    (0.75, Vec3::new(0.953, 0.499, 0.563)),
    (0.844, Vec3::new(0.256, 0.165, 0.457)),
    (1.0, Vec3::new(0.291, 0.0, 0.44)),
]);

/// below the horizon ("Gradient_nadir")
pub const NADIR: Gradient = Gradient(&[
    (0.047, Vec3::new(0.0, 0.0, 0.0)),
    (0.253, Vec3::new(0.858, 0.442, 0.433)),
    (0.503, Vec3::new(0.267, 0.795, 0.851)),
    (0.7, Vec3::new(0.887, 0.345, 0.953)),
    (1.0, Vec3::new(0.0, 0.0, 0.0)),
]);

/// sun halo color, HDR ("Gradient_sun")
pub const SUN: Gradient = Gradient(&[
    (0.072, Vec3::new(2.142, 1.365, 2.996)),
    (0.18, Vec3::new(0.345, 0.395, 0.749)),
    (0.3, Vec3::new(23.969, 2.772, 0.0)),
    (0.519, Vec3::new(12.437, 23.969, 13.217)),
    (0.75, Vec3::new(4.978, 1.981, 0.667)),
    (0.86, Vec3::new(1.125, 1.145, 2.996)),
    (1.0, Vec3::new(2.142, 1.365, 2.996)),
]);

/// sun radiance halo tint, HDR ("Gradient_rim")
pub const RIM: Gradient = Gradient(&[
    (0.05, Vec3::new(0.012, 0.042, 0.151)),
    (0.14, Vec3::new(0.041, 0.113, 0.29)),
    (0.318, Vec3::new(3.66, 0.922, 0.0)),
    (0.5, Vec3::new(0.457, 0.758, 0.61)),
    (0.701, Vec3::new(1.75, 0.619, 0.182)),
    (0.807, Vec3::new(0.727, 0.245, 0.447)),
    (0.915, Vec3::new(0.057, 0.12, 0.381)),
    (1.0, Vec3::new(0.012, 0.042, 0.151)),
]);

/// cloud body color, HDR ("Gradient_clouds_cycle")
pub const CLOUDS: Gradient = Gradient(&[
    (0.05, Vec3::new(0.339, 0.194, 1.059)),
    (0.14, Vec3::new(0.298, 0.416, 1.153)),
    (0.26, Vec3::new(0.72, 0.33, 0.024)),
    (0.5, Vec3::new(1.423, 1.798, 2.0)),
    (0.71, Vec3::new(1.498, 0.847, 0.664)),
    (0.85, Vec3::new(0.759, 0.769, 1.553)),
    (1.0, Vec3::new(0.336, 0.197, 1.061)),
]);

/// cloud highlight lerp factor (r channel), HDR ("Gradient_cloud_highlights")
pub const CLOUD_HIGHLIGHTS: Gradient = Gradient(&[
    (0.0, Vec3::new(0.6, 0.6, 0.6)),
    (0.1, Vec3::new(0.501, 0.501, 0.501)),
    (0.3, Vec3::new(4.0, 4.0, 4.0)),
    (0.5, Vec3::new(0.8, 0.8, 0.8)),
    (0.738, Vec3::new(1.245, 1.245, 1.245)),
    (0.804, Vec3::new(0.669, 0.669, 0.669)),
    (1.0, Vec3::new(0.6, 0.6, 0.6)),
]);

/// directional light color (gradients/directional_light_color.tres)
pub const DIR_LIGHT: Gradient = Gradient(&[
    (0.05, Vec3::new(0.514, 0.388, 1.0)),
    (0.185, Vec3::new(1.0, 0.602, 0.632)),
    (0.333, Vec3::new(0.985, 0.864, 0.645)),
    (0.519, Vec3::new(1.0, 0.931, 0.692)),
    (0.683, Vec3::new(0.984, 0.863, 0.643)),
    (0.801, Vec3::new(1.0, 0.6, 0.631)),
    (1.0, Vec3::new(0.515, 0.387, 1.0)),
]);

/// ambient light color (gradients/ambient_light_color.tres)
pub const AMBIENT: Gradient = Gradient(&[
    (0.0, Vec3::new(0.354, 0.0, 1.0)),
    (0.25, Vec3::new(1.0, 0.597, 0.526)),
    (0.5, Vec3::new(0.519, 0.679, 0.738)),
    (0.7, Vec3::new(1.0, 0.5, 0.458)),
    (1.0, Vec3::new(0.353, 0.0, 1.0)),
]);

/// fog color (gradients/fog_color.tres)
pub const FOG: Gradient = Gradient(&[
    (0.05, Vec3::new(0.239, 0.086, 0.471)),
    (0.133, Vec3::new(0.287, 0.278, 0.514)),
    (0.25, Vec3::new(0.764, 0.551, 0.575)),
    (0.503, Vec3::new(0.31, 0.556, 0.708)),
    (0.7, Vec3::new(0.66, 0.539, 0.514)),
    (0.873, Vec3::new(0.509, 0.156, 0.478)),
    (1.0, Vec3::new(0.24, 0.086, 0.472)),
]);
