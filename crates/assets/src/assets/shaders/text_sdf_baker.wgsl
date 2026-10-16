// the scene-bound imposter bake (bound_material_baker.wgsl) with the base colour taken from a
// glyph distance field (text_sdf_common.wgsl)

#import bevy_pbr::{
    prepass_io::VertexOutput,
    pbr_fragment,
    pbr_types::PbrInput,
}
#import "embedded://shaders/bound_material_baker.wgsl"::fragment as bound_bake_fragment
#import "embedded://shaders/text_sdf_common.wgsl"::text_sdf_base_color

override fn pbr_fragment::pbr_input_from_standard_material(in: VertexOutput, is_front: bool) -> PbrInput {
    var pbr_input = pbr_fragment::pbr_input_from_standard_material(in, is_front);
#ifdef VERTEX_COLORS
    let vertex_color = in.color;
#else
    let vertex_color = vec4(1.0);
#endif
    pbr_input.material.base_color = text_sdf_base_color(in.uv, vertex_color);
    return pbr_input;
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) {
    bound_bake_fragment(in, is_front);
}
