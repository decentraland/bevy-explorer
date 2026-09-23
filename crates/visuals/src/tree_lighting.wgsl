// Original leaf lighting. StandardMaterial supplies the same alpha-tested atlas
// to color and Bevy's default prepass, so holes also appear in the shadows.
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, main_pass_post_lighting_processing},
}
#import "embedded://visuals/landscape_lighting.wgsl"::landscape_lighting

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, front);
    pbr.material.base_color = alpha_discard(pbr.material, pbr.material.base_color);
    // Canopy normals describe the crown volume, not the side of a flat card.
    // landscape_lighting normalizes this interpolated vector exactly once;
    // post-lighting processing uses color/position, not the lighting normal.
    pbr.N = in.world_normal;
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr, landscape_lighting(pbr, in.uv_b.y));
    return out;
}
