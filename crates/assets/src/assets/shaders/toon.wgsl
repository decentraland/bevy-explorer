// Toon (cel) shading for avatars, modelled on the Unity client's DCL_Toon
// shader ("double shade with feather", a trimmed-down UTS2).
//
// Model:
// - half-lambert (0.5*NdotL+0.5) drives a 3-band ramp: base / shade1 / shade2
// - band edges are "feathered" (soft) by a controllable width
// - system (cascade) shadows push fragments into the shade bands
// - a stylized specular "high color" highlight
// - a fresnel rim light on the lit side
//
// Shade colors are derived from the base color (Unity's Use_BaseAs1st /
// Use_1stAs2nd path) multiplied by per-band tints.

#define_import_path dcl::toon

#import bevy_pbr::{
    pbr_types::PbrInput,
    mesh_view_bindings::{lights, view},
    shadows::fetch_directional_shadow,
    mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT,
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
}

struct ToonParams {
    // rgb: tint for 1st shade band, w: ramp step position
    shade1: vec4<f32>,
    // rgb: tint for 2nd (darkest) shade band, w: ramp step position
    shade2: vec4<f32>,
    // x: 1st feather, y: 2nd feather, z: rim power, w: rim strength
    misc: vec4<f32>,
    // x: highlight strength, y: highlight power (0-1), z/w: unused
    high: vec4<f32>,
}

// feathered step: 1.0 below (edge - feather), 0.0 above edge
fn feathered_band(value: f32, edge: f32, feather: f32) -> f32 {
    return 1.0 - smoothstep(edge - max(feather, 0.0001), edge, value);
}

fn toon_lighting(pbr_input: PbrInput, toon: ToonParams) -> vec4<f32> {
    let base_color = pbr_input.material.base_color;
    let n = pbr_input.N;
    let v = pbr_input.V;
    let world_position = pbr_input.world_position;

    var direct: vec3<f32> = vec3(0.0);

    let view_z = dot(vec4<f32>(
        view.view_from_world[0].z,
        view.view_from_world[1].z,
        view.view_from_world[2].z,
        view.view_from_world[3].z
    ), world_position);

    let n_lights = min(lights.n_directional_lights, 4u);
    for (var i = 0u; i < n_lights; i += 1u) {
        let light = lights.directional_lights[i];
        let l = light.direction_to_light;
        // color is premultiplied with illuminance (physical units); the ramp
        // itself works on the half-lambert term, brightness scales linearly
        let light_color = light.color.rgb / 3.14159265;

        var shadow: f32 = 1.0;
        if (light.flags & DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u
            && (pbr_input.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u {
            shadow = fetch_directional_shadow(i, world_position, n, view_z);
        }

        let half_lambert = 0.5 * dot(n, l) + 0.5;
        // system shadows pull the surface towards the shade bands
        let ramp_in = half_lambert * mix(0.5, 1.0, shadow);

        let t1 = feathered_band(ramp_in, toon.shade1.w, toon.misc.x);
        let t2 = feathered_band(ramp_in, toon.shade2.w, toon.misc.y);

        var band_color = mix(base_color.rgb, base_color.rgb * toon.shade1.rgb, t1);
        band_color = mix(band_color, base_color.rgb * toon.shade2.rgb, t2);

        // stylized specular highlight ("high color")
        let h = normalize(v + l);
        let spec_in = 0.5 * dot(n, h) + 0.5;
        let spec = pow(saturate(spec_in), exp2(mix(11.0, 1.0, saturate(toon.high.y)))) * toon.high.x
            * (1.0 - t1); // keep highlight out of the shade bands

        // rim light on the lit side
        let fresnel = pow(1.0 - saturate(dot(n, v)), max(toon.misc.z, 0.0001));
        let rim = fresnel * toon.misc.w * saturate(dot(n, l)) * mix(0.3, 1.0, shadow);

        direct += (band_color + (spec + rim) * base_color.rgb) * light_color;
    }

    // ambient fill, flat (no normal term) to preserve the toon look
    let ambient = lights.ambient_color.rgb * base_color.rgb;

    var color = (direct + ambient) * view.exposure;
    color += pbr_input.material.emissive.rgb * view.exposure;

    return vec4<f32>(color, base_color.a);
}
