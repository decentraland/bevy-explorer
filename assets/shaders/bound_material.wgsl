#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_bindings::{material, emissive_texture, emissive_sampler},
    pbr_types::STANDARD_MATERIAL_FLAGS_EMISSIVE_TEXTURE_BIT,
    mesh_view_bindings::{globals, view},
}
#import "shaders/simplex.wgsl"::simplex_noise_3d

struct SceneBounds {
    bounds: vec4<f32>,
    distance: f32,
}

@group(2) @binding(100)
var<uniform> bounds: SceneBounds;

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    // generate a PbrInput struct from the StandardMaterial bindings
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // apply emmissive multiplier
    // dcl uses default 2.0 intensity. we also override bevy_pbr base emissive rules so that 
    // - if emissive texture is supplied but color is not, we use the texture (bevy by default multiplies emissive color and emissive texture, so color must be white to pass the texture through)
    // - if emissive color (== gltf emissive_intensity == dcl pbr emissive_color * emissive_intensity) is supplied but emissive texture is not, we use emissive color * base color
    // emissive color | emissive texture  | result
    // 0                no                  0
    // x                no                  x * base color
    // 0                t                   2 * t
    // x != 0           t                   x * t
    var emissive: vec4<f32> = material.emissive;
#ifdef VERTEX_UVS
    if ((material.flags & STANDARD_MATERIAL_FLAGS_EMISSIVE_TEXTURE_BIT) != 0u) {
        if dot(emissive, emissive) == 0.0 {
            emissive = vec4(2.0);
        }
        emissive = vec4<f32>(emissive.rgb * textureSampleBias(emissive_texture, emissive_sampler, in.uv, view.mip_bias).rgb, 1.0);
    } else {
        if dot(emissive, emissive) != 0.0 {
            // emissive is set, no emissive texture, use base color texture as emissive texture
            emissive = emissive * pbr_input.material.base_color;
        }
    }
#endif
    // scale up for lumens
    pbr_input.material.emissive = emissive * 10000.0;

    // check bounds
    let world_position = pbr_input.world_position.xyz;
    let outside_amt = max(max(max(0.0, bounds.bounds.x - world_position.x), max(world_position.x - bounds.bounds.z, bounds.bounds.y - world_position.z)), world_position.z - bounds.bounds.w);

    var noise = 0.05;
    if outside_amt > 0.00 {
        if outside_amt < bounds.distance {
            noise = simplex_noise_3d(world_position * 2.0 + globals.time * vec3(0.2, 0.16, 0.24)) * 0.5 + 0.55;
            if noise < (outside_amt - 0.125) / 2.0 {
                discard;
            }
        } else if outside_amt > 0.05 {
            discard;
        }
    }

    // alpha discard
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    // apply lighting
    if (pbr_input.material.flags & bevy_pbr::pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }

    if noise < outside_amt / 2.0 {
        out.color = mix(out.color, vec4(10.0, 1.0, 0.0, 1.0), (outside_amt / 2.0 - noise) / 0.125);
    }

    // apply in-shader post processing (fog, alpha-premultiply, and also tonemapping, debanding if the camera is non-hdr)
    // note this does not include fullscreen postprocessing effects like bloom.
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

    return out;
}
