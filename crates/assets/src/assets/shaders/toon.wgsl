// Toon (cel) shading for avatars.
//
// A 3-band cel ramp driven by two independent darkening sources, each of which
// steps the band down one level:
//   - angle:       the surface faces away from the dominant light (low half-lambert)
//   - cast shadow: the surface is in the dominant light's shadow
// lit + unshadowed = HIGH band, exactly one source = MID, both = LOW. Light
// COLOUR is ignored so avatars read consistently day and night; only overall
// scene brightness is tracked (and capped, so bright daylight doesn't wash them
// out). A fresnel backlight rim glows on the silhouette when the light is behind
// the avatar.
//
// Avatars still CAST shadows (that is the shadow pass's job, not this shading).

#import bevy_pbr::{
    pbr_types::PbrInput,
    mesh_view_bindings::{lights, view},
    shadows::fetch_directional_shadow,
    mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT,
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
}

// --- baked params (edit + rebuild to tune) ---
// the 3 cel bands (albedo brightness multiplier).
const BAND_HIGH: f32 = 1.0; // lit and unshadowed
const BAND_MID: f32 = 0.7;  // one darkening source (angle OR cast shadow)
const BAND_LOW: f32 = 0.45; // both angle-shaded AND cast-shadowed
// half-lambert split between the lit and angle-shaded bands (0.5 = terminator).
const ANGLE_STEP: f32 = 0.5;
const ANGLE_FEATHER: f32 = 0.05;
// cast-shadow split (sun_shadow: 1 = lit, lower = shadowed).
const SHADOW_STEP: f32 = 0.95;
const SHADOW_FEATHER: f32 = 0.04;
// fresnel backlight rim on the silhouette.
const RIM_POWER: f32 = 5.0;
const RIM_STRENGTH: f32 = 0.2;
// multiplier on emissive wearables so they glow.
const EMISSIVE_BOOST: f32 = 2.0;
// ceiling on the (light-colour-derived, illuminance-premultiplied ~thousands of
// lux) brightness, so bright midday doesn't wash avatars out while night, which
// sits below the cap, is left untouched.
const BRIGHTNESS_CAP: f32 = 1500.0;

const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

fn toon_lighting(pbr_input: PbrInput) -> vec4<f32> {
    let base_color = pbr_input.material.base_color;
    let n = pbr_input.N;
    let v = pbr_input.V;
    let world_position = pbr_input.world_position;

    let view_z = dot(vec4<f32>(
        view.view_from_world[0].z,
        view.view_from_world[1].z,
        view.view_from_world[2].z,
        view.view_from_world[3].z
    ), world_position);

    // dominant (brightest) directional light drives the band; the rim takes the
    // strongest backlight contribution across lights.
    var scene_lum: f32 = 0.0;
    var sun_shadow: f32 = 1.0;
    var sun_half_lambert: f32 = 1.0;
    var rim: f32 = 0.0;

    let n_lights = min(lights.n_directional_lights, 4u);
    for (var i = 0u; i < n_lights; i += 1u) {
        let light = &lights.directional_lights[i];
        let l = (*light).direction_to_light;
        let lum_i = dot((*light).color.rgb, LUMA);

        var shadow: f32 = 1.0;
        if ((*light).flags & DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u
            && (pbr_input.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u {
            shadow = fetch_directional_shadow(i, world_position, n, view_z);
        }

        if lum_i > scene_lum {
            scene_lum = lum_i;
            sun_shadow = shadow;
            sun_half_lambert = 0.5 * dot(n, l) + 0.5;
        }

        // fresnel silhouette edge (1 - n·v), strongest when the light is behind
        // the avatar (backlit: light direction opposite the view).
        let edge = pow(1.0 - max(dot(n, v), 0.0), max(RIM_POWER, 0.0001));
        let backlit = smoothstep(0.0, 0.5, -dot(l, v));
        rim = max(rim, edge * backlit * RIM_STRENGTH);
    }

    // 1 where lit, 0 where darkened, per source
    let angle_lit = smoothstep(ANGLE_STEP - ANGLE_FEATHER, ANGLE_STEP + ANGLE_FEATHER, sun_half_lambert);
    let shadow_lit = smoothstep(SHADOW_STEP - SHADOW_FEATHER, SHADOW_STEP + SHADOW_FEATHER, sun_shadow);

    // each lit source raises the band one level: 0 -> LOW, 1 -> MID, 2 -> HIGH
    let lit_steps = angle_lit + shadow_lit;
    var band: f32;
    if lit_steps > 1.0 {
        band = mix(BAND_MID, BAND_HIGH, lit_steps - 1.0);
    } else {
        band = mix(BAND_LOW, BAND_MID, lit_steps);
    }
    rim *= shadow_lit; // rim drops out in cast shadow

    // light colour ignored; track only overall scene brightness (capped) so
    // avatars dim at night without going dark and don't wash out at midday.
    let ambient_lum = dot(lights.ambient_color.rgb, LUMA);
    let brightness = min(scene_lum / 3.14159265 + ambient_lum, BRIGHTNESS_CAP) * view.exposure;

    var color = (base_color.rgb * band + vec3(rim)) * brightness;
    color += pbr_input.material.emissive.rgb * view.exposure * EMISSIVE_BOOST;

    return vec4<f32>(color, base_color.a);
}
