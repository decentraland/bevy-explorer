//! Day-cycle atmosphere coefficients for the Nishita sky: the rayleigh
//! coefficient (hue), the mie coefficient (haze), and a flat night-sky colour.
//! Hand-tuned, keyed by normalized time of day (0.0 = midnight, 0.5 = noon).

use bevy::math::Vec3;
use common::util::{Curve, Gradient};

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
