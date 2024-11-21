#import bevy_pbr::{
    forward_io::FragmentOutput,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::{STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ADD, STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT},
}

#import boimp::shared::{ImposterVertexOut, unpack_pbrinput};
#import boimp::bindings::sample_tile_material;

@group(2) @binding(100)
var<uniform> offset: f32;

@fragment
fn fragment(in: ImposterVertexOut) -> FragmentOutput {
    var out: FragmentOutput;

    let inv_rot = mat3x3(
        in.inverse_rotation_0c,
        in.inverse_rotation_1c,
        in.inverse_rotation_2c,
    );

    var props = sample_tile_material(clamp(in.uv_c, vec2(0.0001), vec2(17.0/18.0 - 0.0001)), vec2(0u,0u), vec2(offset, offset));

    if props.rgba.a == 0.0 {
        discard;
    }

    props.rgba.a = 1.0;

    var pbr_input = unpack_pbrinput(props, in.position);
    pbr_input.N = inv_rot * normalize(pbr_input.N);
    pbr_input.world_normal = pbr_input.N;
    pbr_input.material.flags |= STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ADD;
    out.color = apply_pbr_lighting(pbr_input);

    pbr_input.material.flags |= STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT;
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

    // out.color = clamp(out.color, vec4(0.2, 0.0, 0.0, 0.2), vec4(1.0));

    return out;
}
