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

// --- measured Unity sky colors -------------------------------------------
// Sampled DIRECTLY from the Unity reference screenshots in
// color-lighting-test-scene/colortool/Screenshots/ (N/E/S/W/U x 24h), NOT from
// skybox_colors_godot.json — that pre-cooked JSON had flattened the colors to a
// dull blue (it mis-sampled / averaged in the dark water), which is what made
// our sky go monochrome. The screenshots are the ground truth.
//
// We also confirmed N/E/S/W are near-identical at every hour (within ~0.03), so
// the sky is NOT direction-dependent — it's a single dome that only varies by
// height (zenith->horizon->below) and time of day. So:
//   ZENITH  <- U (straight-up) shot
//   HORIZON <- mean(N/E/S/W), the warm band just above the waterline
//   NADIR   <- mean(N/E/S/W), the water / below-horizon fill
// One stop per hour at t = hour/24. Values are 0..1 sRGB screen colors fed to
// the LUT; tune live with /skyzenith /skyhorizon /skynadir /skygain.

/// sky straight up — Unity U shots
pub const ZENITH: Gradient = Gradient(&[
    (0.00000, Vec3::new(0.161, 0.118, 0.463)),
    (0.04167, Vec3::new(0.161, 0.118, 0.463)),
    (0.08333, Vec3::new(0.189, 0.165, 0.526)),
    (0.12500, Vec3::new(0.236, 0.228, 0.604)),
    (0.16667, Vec3::new(0.279, 0.287, 0.663)),
    (0.20833, Vec3::new(0.322, 0.345, 0.702)),
    (0.25000, Vec3::new(0.388, 0.404, 0.706)),
    (0.29167, Vec3::new(0.447, 0.455, 0.737)),
    (0.33333, Vec3::new(0.443, 0.486, 0.745)),
    (0.37500, Vec3::new(0.484, 0.542, 0.748)),
    (0.41667, Vec3::new(0.540, 0.622, 0.763)),
    (0.45833, Vec3::new(0.589, 0.700, 0.781)),
    (0.50000, Vec3::new(0.549, 0.722, 0.780)),
    (0.54167, Vec3::new(0.492, 0.685, 0.773)),
    (0.58333, Vec3::new(0.454, 0.624, 0.768)),
    (0.62500, Vec3::new(0.405, 0.532, 0.747)),
    (0.66667, Vec3::new(0.369, 0.433, 0.742)),
    (0.70833, Vec3::new(0.385, 0.381, 0.741)),
    (0.75000, Vec3::new(0.428, 0.353, 0.741)),
    (0.79167, Vec3::new(0.385, 0.314, 0.722)),
    (0.83333, Vec3::new(0.349, 0.279, 0.686)),
    (0.87500, Vec3::new(0.302, 0.240, 0.639)),
    (0.91667, Vec3::new(0.255, 0.200, 0.596)),
    (0.95833, Vec3::new(0.212, 0.161, 0.526)),
]);

/// warm band just above the horizon — Unity mean(N/E/S/W)
pub const HORIZON: Gradient = Gradient(&[
    (0.00000, Vec3::new(0.241, 0.034, 0.570)),
    (0.04167, Vec3::new(0.235, 0.032, 0.567)),
    (0.08333, Vec3::new(0.274, 0.111, 0.594)),
    (0.12500, Vec3::new(0.317, 0.232, 0.610)),
    (0.16667, Vec3::new(0.425, 0.341, 0.591)),
    (0.20833, Vec3::new(0.557, 0.432, 0.552)),
    (0.25000, Vec3::new(0.679, 0.520, 0.514)),
    (0.29167, Vec3::new(0.735, 0.600, 0.540)),
    (0.33333, Vec3::new(0.717, 0.657, 0.625)),
    (0.37500, Vec3::new(0.683, 0.693, 0.692)),
    (0.41667, Vec3::new(0.667, 0.700, 0.720)),
    (0.45833, Vec3::new(0.661, 0.711, 0.742)),
    (0.50000, Vec3::new(0.655, 0.719, 0.761)),
    (0.54167, Vec3::new(0.666, 0.705, 0.741)),
    (0.58333, Vec3::new(0.679, 0.686, 0.715)),
    (0.62500, Vec3::new(0.696, 0.669, 0.688)),
    (0.66667, Vec3::new(0.711, 0.651, 0.655)),
    (0.70833, Vec3::new(0.721, 0.620, 0.611)),
    (0.75000, Vec3::new(0.716, 0.572, 0.598)),
    (0.79167, Vec3::new(0.630, 0.445, 0.584)),
    (0.83333, Vec3::new(0.487, 0.333, 0.602)),
    (0.87500, Vec3::new(0.403, 0.247, 0.612)),
    (0.91667, Vec3::new(0.343, 0.174, 0.607)),
    (0.95833, Vec3::new(0.293, 0.099, 0.594)),
]);

/// below the horizon / water — Unity mean(N/E/S/W)
pub const NADIR: Gradient = Gradient(&[
    (0.00000, Vec3::new(0.244, 0.187, 0.504)),
    (0.04167, Vec3::new(0.242, 0.186, 0.503)),
    (0.08333, Vec3::new(0.255, 0.207, 0.516)),
    (0.12500, Vec3::new(0.270, 0.240, 0.518)),
    (0.16667, Vec3::new(0.316, 0.272, 0.514)),
    (0.20833, Vec3::new(0.403, 0.316, 0.501)),
    (0.25000, Vec3::new(0.488, 0.359, 0.479)),
    (0.29167, Vec3::new(0.520, 0.417, 0.511)),
    (0.33333, Vec3::new(0.488, 0.438, 0.528)),
    (0.37500, Vec3::new(0.457, 0.466, 0.558)),
    (0.41667, Vec3::new(0.451, 0.484, 0.582)),
    (0.45833, Vec3::new(0.450, 0.495, 0.601)),
    (0.50000, Vec3::new(0.457, 0.508, 0.614)),
    (0.54167, Vec3::new(0.467, 0.502, 0.603)),
    (0.58333, Vec3::new(0.477, 0.487, 0.586)),
    (0.62500, Vec3::new(0.484, 0.467, 0.563)),
    (0.66667, Vec3::new(0.493, 0.446, 0.535)),
    (0.70833, Vec3::new(0.502, 0.420, 0.513)),
    (0.75000, Vec3::new(0.502, 0.390, 0.518)),
    (0.79167, Vec3::new(0.435, 0.311, 0.508)),
    (0.83333, Vec3::new(0.368, 0.263, 0.522)),
    (0.87500, Vec3::new(0.331, 0.226, 0.520)),
    (0.91667, Vec3::new(0.303, 0.221, 0.522)),
    (0.95833, Vec3::new(0.269, 0.200, 0.514)),
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

/// scalar curve (piecewise linear; godot Curve resources have flat tangents here)
pub struct Curve(pub &'static [(f32, f32)]);

impl Curve {
    pub fn sample(&self, t: f32) -> f32 {
        let pts = self.0;
        let t = t.rem_euclid(1.0);
        if t <= pts[0].0 {
            return pts[0].1;
        }
        for pair in pts.windows(2) {
            let (t0, v0) = pair[0];
            let (t1, v1) = pair[1];
            if t >= t0 && t <= t1 {
                return v0 + (v1 - v0) * ((t - t0) / (t1 - t0));
            }
        }
        pts[pts.len() - 1].1
    }
}

/// moon tint cycle ("Gradient_fb67a")
pub const MOON: Gradient = Gradient(&[
    (0.0, Vec3::new(1.0, 1.0, 1.0)),
    (0.159, Vec3::new(1.0, 0.458, 0.206)),
    (0.484, Vec3::new(0.883, 0.0, 0.073)),
    (0.87, Vec3::new(1.0, 0.459, 0.204)),
    (1.0, Vec3::new(1.0, 1.0, 1.0)),
]);

/// sun disc size over the day (sun_size_curve.tres)
pub const SUN_SIZE: Curve = Curve(&[
    (0.0, 0.12),
    (0.1458, 0.12),
    (0.1667, 0.09),
    (0.1875, 0.0),
    (0.25, 0.3),
    (0.5, 0.1),
    (0.8125, 0.3),
    (0.8542, 0.0),
    (0.875, 0.09),
    (1.0, 0.12),
]);

/// sun disc opacity over the day (sun_opacity_curve.tres)
pub const SUN_OPACITY: Curve = Curve(&[
    (0.0, 1.0),
    (0.1458, 1.0),
    (0.1667, 0.0),
    (0.1875, 0.0),
    (0.25, 1.0),
    (0.8125, 1.0),
    (0.8542, 0.0),
    (0.875, 0.0),
    (0.9167, 1.0),
    (1.0, 1.0),
]);

/// crescent-bite mask size over the day (moon_mask_size_curve.tres)
pub const MOON_MASK_SIZE: Curve = Curve(&[
    (0.0, 0.16),
    (0.25, 0.17),
    (0.26, 0.0),
    (0.83, 0.0),
    (0.84, 0.16),
    (1.0, 0.16),
]);
