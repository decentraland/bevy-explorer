// Sun shadow softening shared by the fetch_directional_shadow overrides in bound_material.wgsl
// and the grass shell shader. The overrides stay in each top-level shader, and this module must
// not import view bindings: importing such a module breaks the override ("can't find function
// shadows::fetch_directional_shadow for redirection"), so callers pass in the cascade far bound.

// SHADOW_OPACITY: how dark the sun's cast shadows are on the world.
// 1.0 = full black shadows (bevy default), 0.0 = no shadow at all.
// overriding the shadow fetch lifts the shadow value toward "lit" inside the
// single pbr lighting pass, so shadows become partial instead of fully
// occluding the sun — no second lighting evaluation needed.
const SHADOW_OPACITY: f32 = 0.65;

// Beyond the furthest shadow cascade bevy returns "fully lit" (shadow = 1.0),
// which pops hard at the shadow-distance edge. Instead, fade toward a partial-
// shadow floor. CASCADE_FAR_SHADOW is how dark that floor is, like
// SHADOW_OPACITY: 0.25 = a quarter of the way from fully lit to black.
// CASCADE_FAR_FADE is the fraction of the shadow distance to blend over.
const CASCADE_FAR_SHADOW: f32 = 0.25;
const CASCADE_FAR_FADE: f32 = 0.3;

// Soften a directional shadow value from `shadows::fetch_directional_shadow`. `far` is the far
// bound of the light's last cascade, i.e. its shadow distance.
fn soften_directional_shadow(base: f32, far: f32, view_z: f32) -> f32 {
    let softened = mix(1.0, base, SHADOW_OPACITY);
    if far <= 0.0 {
        return softened;
    }
    let far_shadow = 1.0 - CASCADE_FAR_SHADOW;
    // blend the (softened) shadow toward that floor over the last
    // CASCADE_FAR_FADE of the distance, holding the floor past the edge
    let fade = smoothstep(far * (1.0 - CASCADE_FAR_FADE), far, -view_z);
    return mix(softened, far_shadow, fade);
}
