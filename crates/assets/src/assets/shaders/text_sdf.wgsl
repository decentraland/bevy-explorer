// the scene-bound material (bound_material.wgsl) with the base colour taken from a glyph
// distance field (text_sdf_common.wgsl)

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment,
    pbr_types::PbrInput,
}
#import "embedded://shaders/bound_material.wgsl"::fragment as bound_fragment
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
#ifdef MULTISAMPLED
    @builtin(sample_index) sample_index: u32,
#endif
) -> FragmentOutput {
#ifdef MULTISAMPLED
    var out = bound_fragment(in, is_front, sample_index);
#else
    var out = bound_fragment(in, is_front);
#endif
#ifdef TRANSPARENT_FOCUS_OUTPUT
    // the glyph's anti-aliased fringe focuses with the glyph, not with what's behind it
    out.focus.a = 1.0;
#endif
    return out;
}
