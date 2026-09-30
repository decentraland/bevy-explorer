//! Day-cycle colour gradient for the ambient fill. Values were sampled from
//! reference-client captures, keyed by normalized time of day (0.0 = midnight,
//! 0.5 = noon). Colors are linear HDR. (Fog tint is derived from the ambient
//! colour in `visuals` so it follows scene light overrides.)

use bevy::math::Vec3;
use common::util::Gradient;

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
