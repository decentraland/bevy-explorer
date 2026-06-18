// Toon (cel) shading for avatars.
//
// Port of the DCL avatar toon look, cross-referenced from both clients:
// - unity-shared-dependencies/Runtime/Shaders/Avatar/DCL_Toon (full UTS2)
// - godot-explorer/godot/assets/avatar/dcl_toon.gdshaderinc (distilled, with
//   the actual deployed parameter values — used as the primary reference)
//
// Model (matching the Godot port):
// - half-lambert * cast-shadow drives a 3-zone ramp: lit / 1st shade / 2nd shade
// - the ramp is a scalar brightness multiplier on albedo; light COLOR is
//   ignored so avatars look consistent day & night (Unity behaviour)
// - brightness floor: the avatar is never darker than the 2nd shade zone
// - hard-edged blinn-phong specular dot, soft fresnel rim on the lit side

#import bevy_pbr::{
    pbr_types::PbrInput,
    mesh_view_bindings::{lights, view},
    shadows::fetch_directional_shadow,
    mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT,
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
}

// --- live-tunable knobs (edit + save = ~2s hot reload, no rebuild) ---
// how much cast shadows darken the avatar body. 1.0 = full shadows (old
// behaviour), 0.0 = none (unity-like: avatars are flat-lit, no shadows on
// their material). try 0.0–0.3.
const SELF_SHADOW_STRENGTH: f32 = 0.0;
// extra multiplier on emissive parts so glowing wearables actually glow like
// unity. 1.0 = current, raise for more punch. try 1.0–4.0.
const EMISSIVE_BOOST: f32 = 2.0;

// params are passed as plain vec4s (structs don't cross naga_oil module
// boundaries cleanly):
// shade1: rgb = brightness multiplier in the 1st shade zone, w = ramp step
// shade2: rgb = brightness multiplier in the 2nd (darkest) zone, w = ramp step
// misc:   x = 1st zone feather, y = 2nd zone feather, z = rim power, w = rim strength
// high:   x = specular strength, y/z/w = unused
fn toon_lighting(
    pbr_input: PbrInput,
    shade1: vec4<f32>,
    shade2: vec4<f32>,
    misc: vec4<f32>,
    high: vec4<f32>,
) -> vec4<f32> {
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

    // brightness floor: never darker than the 2nd shade zone (godot bakes
    // this into EMISSION with ambient disabled)
    var shade_rgb = shade2.rgb;
    var highlight: f32 = 0.0;

    var scene_lum: f32 = 0.0;

    let n_lights = min(lights.n_directional_lights, 4u);
    for (var i = 0u; i < n_lights; i += 1u) {
        // note: pointer access, copying the struct out of the uniform does not validate
        let light = &lights.directional_lights[i];
        let l = (*light).direction_to_light;
        let light_color = (*light).color.rgb;
        scene_lum = max(scene_lum, dot(light_color, vec3(0.2126, 0.7152, 0.0722)));

        var shadow: f32 = 1.0;
        if ((*light).flags & DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u
            && (pbr_input.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u {
            shadow = fetch_directional_shadow(i, world_position, n, view_z);
        }

        let n_dot_l = dot(n, l);
        let half_lambert = 0.5 * n_dot_l + 0.5;
        // fade the cast shadow toward 1 (no darkening) by SELF_SHADOW_STRENGTH
        let self_shadow = mix(1.0, shadow, SELF_SHADOW_STRENGTH);
        let ramp_in = half_lambert * self_shadow;

        // 3-zone ramp, edges rise from shade to lit as ramp_in increases
        let shadow_mask = smoothstep(shade1.w - misc.x, shade1.w, ramp_in);
        let zone2 = smoothstep(shade2.w - misc.y, shade2.w, ramp_in);

        let shade_zone = mix(shade2.rgb, shade1.rgb, zone2);
        let final_shade = mix(shade_zone, vec3(1.0), shadow_mask);
        shade_rgb = max(shade_rgb, final_shade);

        // hard-edged toon specular
        let h = normalize(v + l);
        let n_dot_h = max(dot(n, h), 0.0);
        let spec = pow(n_dot_h, 128.0) * high.x;
        let spec_mask = smoothstep(0.45, 0.55, spec) * shadow_mask;

        // soft fresnel rim, only on the lit side
        let rim_dot = 1.0 - max(dot(n, v), 0.0);
        let rim = pow(rim_dot, max(misc.z, 0.0001)) * misc.w;
        let rim_dir_mask = smoothstep(0.0, 0.3, n_dot_l * self_shadow);

        highlight = max(highlight, spec_mask + rim * rim_dir_mask);
    }

    // light color is deliberately ignored (Unity avatars are light-color
    // independent); track only overall scene brightness so avatars don't
    // glow at night. luminance of the brightest directional + ambient.
    let ambient_lum = dot(lights.ambient_color.rgb, vec3(0.2126, 0.7152, 0.0722));
    let brightness = (scene_lum / 3.14159265 + ambient_lum) * view.exposure;

    var color = (base_color.rgb * shade_rgb + vec3(highlight)) * brightness;
    color += pbr_input.material.emissive.rgb * view.exposure * EMISSIVE_BOOST;

    return vec4<f32>(color, base_color.a);
}
