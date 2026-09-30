//! Atmosphere coefficients for the Nishita sky: the rayleigh coefficient (hue),
//! the mie coefficient (haze), and a flat night-sky colour.

use bevy::math::Vec3;

/// Atmosphere rayleigh coefficient (the sky's hue lever) under the noon sun.
/// Blue-biased so the sky reads blue. The sky follows the light colour from
/// here: `apply_global_light` scales it per channel by the light colour
/// against noon's, cubed, which warms the horizon at dawn and dusk and lets a
/// scene's light colour change the sky.
pub const RAYLEIGH_NOON: Vec3 = Vec3::new(6.0e-6, 13.0e-6, 22.0e-6);

/// Flat night-sky colour added per-direction in the atmosphere shader by
/// `max(-1, -sun·ray) * 0.25 + 0.75`, so the night sky isn't pure black.
pub const NIGHT_SKY: Vec3 = Vec3::new(0.1, 0.05, 0.3);

/// Atmosphere mie (haze) coefficient by the sine of the sun's elevation. Mie is
/// a scalar, so it only controls horizon glow intensity, not hue. Strong by day
/// (~42e-6) for a hazy horizon; floored low at dawn/dusk and through the night
/// (the floor keeps the night term alive — exactly-zero mie kills it).
pub fn mie(elevation: f32) -> f32 {
    let t = ((elevation + 0.1) / 0.58).clamp(0.0, 1.0);
    0.21e-6 + (42.0e-6 - 0.21e-6) * t * t * (3.0 - 2.0 * t)
}
