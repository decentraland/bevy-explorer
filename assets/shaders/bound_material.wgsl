#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    mesh_view_bindings::globals,
}
#import "shaders/simplex.wgsl"::simplex_noise_3d

struct SceneBounds {
    bounds: vec4<f32>,
}

@group(1) @binding(100)
var<uniform> bounds: SceneBounds;

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    // generate a PbrInput struct from the StandardMaterial bindings
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // check bounds
    let world_position = pbr_input.world_position.xyz;
    let outside_amt = max(max(max(0.0, bounds.bounds.x - world_position.x), max(world_position.x - bounds.bounds.z, bounds.bounds.y - world_position.z)), world_position.z - bounds.bounds.w);

    var noise = 0.0;
    if outside_amt > 0.0 && outside_amt < 2.0 {
        noise = simplex_noise_3d(world_position * 2.0 + globals.time * vec3(0.2, 0.16, 0.24)) * 0.5 + 0.55;
    }
    if noise < (outside_amt - 0.125) / 2.0 {
        discard;
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

    // apply in-shader post processing (fog, alpha-premultiply, and also tonemapping, debanding if the camera is non-hdr)
    // note this does not include fullscreen postprocessing effects like bloom.
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

    if noise < outside_amt / 2.0 {
        out.color = mix(out.color, vec4(10.0, 1.0, 0.0, 1.0), (outside_amt / 2.0 - noise) / 0.125);
    }

    return out;
}
