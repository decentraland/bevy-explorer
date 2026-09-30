// Lookups into the sky environment map (visuals/src/sky_envmap.rs), shared by the pbr material
// shaders. Each overrides environment_map::compute_radiances to sample with sky_envmap_input and
// level the result with sky_envmap_level, and overrides ambient::ambient_light to return nothing.
//
// Only the upper hemisphere of the maps is generated. Directions below the horizon read their
// mirror image above it, standing in for the sky reflected back up off the ground, tinted towards
// the ground and, for reflections, blurred: see sky_envmap_ground.wgsl for the curve.
//
// The envmap is the only ambient light, levelled towards a target: the flat ambient light, which
// the overrides zero out and pass in here instead.
// - above the target, the brightest channel is compressed towards the target's in log space,
//   keeping the hue: brightest * (target / brightest)^compression. Full for irradiance, scaled by
//   roughness for reflections so mirrors still match the drawn sky.
// - below it, the target is a floor: an even mix of the ambient light as a plain per-channel
//   minimum, and a lift of the brightest channel up to the target's, keeping the envmap's hue.
// The compression rides in the ambient light's alpha, negated (the uniform holds alpha *
// brightness; cameras with their own ambient light have a positive alpha, and no compression).
//
// The overrides themselves stay in each top-level shader: naga_oil only applies an override
// declared there, and only the override can call bevy's original compute_radiances.

#import bevy_pbr::{environment_map, lighting}
#import "embedded://shaders/sky_envmap_ground.wgsl"::{sky_envmap_ground_shininess, sky_envmap_ground_tint}

// the input to sample the envmap with in place of `input`. bevy samples along R exactly at zero
// roughness, so R is set to where it would have sampled, mirrored up
fn sky_envmap_input(input: lighting::LayerLightingInput) -> lighting::LayerLightingInput {
    let direction = environment_map::radiance_sample_direction(input.N, input.R, input.roughness);
    return lighting::LayerLightingInput(
        sky_envmap_upper(input.N),
        sky_envmap_upper(direction),
        input.NdotV,
        sky_envmap_perceptual_roughness(direction, input.perceptual_roughness),
        0.0,
    );
}

// `radiances` as sampled with `upper` (from sky_envmap_input) in place of `input`; `ambient` is
// the lights uniform's ambient_color
fn sky_envmap_level(radiances: environment_map::EnvironmentMapRadiances, input: lighting::LayerLightingInput, upper: lighting::LayerLightingInput, ambient: vec4<f32>) -> environment_map::EnvironmentMapRadiances {
    let direction = environment_map::radiance_sample_direction(input.N, input.R, input.roughness);
    return environment_map::EnvironmentMapRadiances(
        sky_envmap_ambient(radiances.irradiance * sky_envmap_ground_tint(input.N), ambient, 1.0),
        sky_envmap_ambient(radiances.radiance * sky_envmap_ground_tint(direction), ambient, upper.perceptual_roughness),
    );
}

fn sky_envmap_upper(direction: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(direction.x, abs(direction.y), direction.z);
}

fn sky_envmap_perceptual_roughness(direction: vec3<f32>, perceptual_roughness: f32) -> f32 {
    return 1.0 - (1.0 - perceptual_roughness) * sky_envmap_ground_shininess(direction);
}

const FLOOR_HUE_WEIGHT: f32 = 0.5;

// `ambient` is the lights uniform's ambient_color; `compression_weight` scales its compression
// (1 for irradiance, the perceptual roughness for reflections)
fn sky_envmap_ambient(color: vec3<f32>, ambient: vec4<f32>, compression_weight: f32) -> vec3<f32> {
    let goal = ambient.rgb;
    let brightest = max(color.r, max(color.g, color.b));
    let goal_brightest = max(goal.r, max(goal.g, goal.b));

    var levelled = color;
    if brightest > goal_brightest && goal_brightest > 0.0 {
        let compression = saturate(-ambient.a) * compression_weight;
        levelled = color * pow(goal_brightest / brightest, compression);
    }

    let plain = max(levelled, goal);
    var hue = levelled;
    if brightest <= 0.0 {
        // no hue to keep
        hue = goal;
    } else if brightest < goal_brightest {
        hue = color * (goal_brightest / brightest);
    }
    return mix(plain, hue, FLOOR_HUE_WEIGHT);
}
