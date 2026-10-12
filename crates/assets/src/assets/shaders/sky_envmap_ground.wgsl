// The ground under the sky environment map: directions below the horizon read the sky's mirror
// image (see sky_envmap_lookup.wgsl), and this is how that image is adjusted towards the ground.
// Shared by the lookups and the map generation. Pure maths: the generation shader can't import
// the lookup module, which imports bevy_pbr.
//
// Everything blends on one curve of the direction's elevation: no adjustment from
// FULL_SKY_ELEVATION up, growing linearly with the angle to the full adjustment straight down.

const FULL_SKY_ELEVATION: f32 = 0.34906585; // 20 degrees
const HALF_PI: f32 = 1.5707963;

// reflections lose shininess towards the ground: this much of it is left straight down
const GROUND_SHININESS: f32 = 0.33;
// the mirrored sky is tinted towards the ground: this is the tint straight down
const GROUND_TINT: vec3<f32> = vec3<f32>(0.7, 0.4, 0.25);

// 0 at and above FULL_SKY_ELEVATION, 1 straight down
fn sky_envmap_toward_ground(direction: vec3<f32>) -> f32 {
    let elevation = asin(clamp(direction.y * inverseSqrt(dot(direction, direction)), -1.0, 1.0));
    return saturate((FULL_SKY_ELEVATION - elevation) / (FULL_SKY_ELEVATION + HALF_PI));
}

fn sky_envmap_ground_tint(direction: vec3<f32>) -> vec3<f32> {
    return mix(vec3<f32>(1.0), GROUND_TINT, sky_envmap_toward_ground(direction));
}

fn sky_envmap_ground_shininess(direction: vec3<f32>) -> f32 {
    return mix(1.0, GROUND_SHININESS, sky_envmap_toward_ground(direction));
}
