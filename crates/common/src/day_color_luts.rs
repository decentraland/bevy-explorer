// Measured Unity-client sky colors, one entry per hour (index = hour).
// Source: dcl-regenesislabs/color-lighting-test-scene — hourly screenshots of
// the Unity explorer skybox in all cardinal directions, averaged by region.
// Values are sRGB 0-1.

use bevy::math::Vec3;

/// sky color straight up (from upward screenshots, central region)
pub const SKY_ZENITH: [Vec3; 24] = [
    Vec3::new(0.16, 0.118, 0.463),
    Vec3::new(0.161, 0.118, 0.463),
    Vec3::new(0.188, 0.165, 0.525),
    Vec3::new(0.235, 0.227, 0.604),
    Vec3::new(0.278, 0.286, 0.663),
    Vec3::new(0.322, 0.345, 0.702),
    Vec3::new(0.388, 0.404, 0.706),
    Vec3::new(0.447, 0.455, 0.737),
    Vec3::new(0.423, 0.478, 0.745),
    Vec3::new(0.425, 0.516, 0.746),
    Vec3::new(0.493, 0.599, 0.755),
    Vec3::new(0.531, 0.672, 0.774),
    Vec3::new(0.569, 0.73, 0.777),
    Vec3::new(0.615, 0.74, 0.783),
    Vec3::new(0.514, 0.673, 0.774),
    Vec3::new(0.464, 0.588, 0.754),
    Vec3::new(0.404, 0.471, 0.744),
    Vec3::new(0.385, 0.381, 0.741),
    Vec3::new(0.427, 0.353, 0.741),
    Vec3::new(0.384, 0.314, 0.722),
    Vec3::new(0.349, 0.278, 0.686),
    Vec3::new(0.302, 0.239, 0.639),
    Vec3::new(0.255, 0.2, 0.596),
    Vec3::new(0.212, 0.161, 0.525),
];

/// sky color at the horizon (cardinal screenshots, just above the waterline)
pub const SKY_HORIZON: [Vec3; 24] = [
    Vec3::new(0.29, 0.035, 0.637),
    Vec3::new(0.273, 0.032, 0.616),
    Vec3::new(0.31, 0.108, 0.639),
    Vec3::new(0.351, 0.235, 0.646),
    Vec3::new(0.404, 0.334, 0.618),
    Vec3::new(0.501, 0.401, 0.569),
    Vec3::new(0.638, 0.473, 0.496),
    Vec3::new(0.776, 0.606, 0.547),
    Vec3::new(0.748, 0.687, 0.649),
    Vec3::new(0.703, 0.745, 0.729),
    Vec3::new(0.715, 0.76, 0.766),
    Vec3::new(0.75, 0.786, 0.8),
    Vec3::new(0.764, 0.796, 0.816),
    Vec3::new(0.767, 0.78, 0.801),
    Vec3::new(0.765, 0.756, 0.779),
    Vec3::new(0.77, 0.729, 0.75),
    Vec3::new(0.773, 0.705, 0.72),
    Vec3::new(0.776, 0.663, 0.666),
    Vec3::new(0.783, 0.636, 0.66),
    Vec3::new(0.66, 0.484, 0.629),
    Vec3::new(0.488, 0.393, 0.671),
    Vec3::new(0.4, 0.304, 0.685),
    Vec3::new(0.366, 0.213, 0.669),
    Vec3::new(0.346, 0.136, 0.662),
];

/// upper-sky average across all directions — used as ambient light color
pub const SKY_AMBIENT: [Vec3; 24] = [
    Vec3::new(0.188, 0.045, 0.444),
    Vec3::new(0.169, 0.0363, 0.43),
    Vec3::new(0.198, 0.094, 0.487),
    Vec3::new(0.247, 0.174, 0.544),
    Vec3::new(0.297, 0.261, 0.6),
    Vec3::new(0.37, 0.341, 0.618),
    Vec3::new(0.493, 0.396, 0.607),
    Vec3::new(0.609, 0.469, 0.619),
    Vec3::new(0.583, 0.536, 0.658),
    Vec3::new(0.516, 0.599, 0.717),
    Vec3::new(0.501, 0.625, 0.739),
    Vec3::new(0.486, 0.641, 0.752),
    Vec3::new(0.482, 0.65, 0.773),
    Vec3::new(0.483, 0.609, 0.75),
    Vec3::new(0.512, 0.582, 0.727),
    Vec3::new(0.531, 0.549, 0.714),
    Vec3::new(0.558, 0.516, 0.689),
    Vec3::new(0.596, 0.47, 0.649),
    Vec3::new(0.59, 0.391, 0.629),
    Vec3::new(0.46, 0.302, 0.619),
    Vec3::new(0.3, 0.208, 0.585),
    Vec3::new(0.228, 0.145, 0.543),
    Vec3::new(0.223, 0.121, 0.524),
    Vec3::new(0.206, 0.0882, 0.497),
];

/// fog tint per hour — the horizon reading, which is what distant
/// geometry fades into in the unity client
pub const FOG_COLOR: [Vec3; 24] = SKY_HORIZON;

/// sample an hourly lut with linear interpolation and midnight wrap.
/// `day` is normalized time of day: 0.0 = midnight, 0.5 = noon.
pub fn sample_day_lut(lut: &[Vec3; 24], day: f32) -> Vec3 {
    let h = day.rem_euclid(1.0) * 24.0;
    let i0 = (h.floor() as usize) % 24;
    let i1 = (i0 + 1) % 24;
    lut[i0].lerp(lut[i1], h.fract())
}
