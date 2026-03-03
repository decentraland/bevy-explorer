#ifdef PREPASS_PIPELINE
    #import bevy_pbr::prepass_io::{VertexOutput, FragmentOutput};
#else
    #import bevy_pbr::forward_io::{VertexOutput, FragmentOutput};
    #import bevy_pbr::pbr_fragment::pbr_input_from_vertex_output;
    #import bevy_pbr::pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing};
#endif
#import bevy_pbr::mesh_functions::get_tag;

#import "embedded://shaders/simplex.wgsl"::simplex_noise_3d

@group(2) @binding(0) var<uniform> subdivisions: u32;
@group(2) @binding(1) var<uniform> layers: u32;
@group(2) @binding(3) var<uniform> root_color: vec4<f32>;
@group(2) @binding(4) var<uniform> tip_color: vec4<f32>;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {

    let layer = get_tag(in.instance_index);
    let layer_f32 = f32(layer);
    let layers_f32 = f32(layers);
    let factor = layer_f32 / layers_f32;

    let subdivisions_f32 = f32(subdivisions);

    let wpx = in.world_position.x * subdivisions_f32;
    let wpz = in.world_position.z * subdivisions_f32;
    let simplex_coord = vec3(
        round(wpx + 0.5) / subdivisions_f32,
        0.,
        round(wpz + 0.5) / subdivisions_f32,
    );

    let octave1 = (simplex_noise_3d(simplex_coord * 0.1512) + 1.) / 2.;
    let octave2 = (simplex_noise_3d(simplex_coord) + 1.) / 2.;
    let octave3 = (simplex_noise_3d(simplex_coord * 435.167) + 1.) / 2.;
    let simplex = (octave1 + octave2 + octave3) / 3.;
    if simplex <= factor {
        discard;
    }
    if distance(fract(vec2(wpx, wpz)), vec2(0.5)) >= mix(0.1, 0.45, 1. - factor) {
        discard;
    }

    var out: FragmentOutput;

#ifdef PREPASS_PIPELINE
#ifdef NORMAL_PREPASS
    out.normal = vec4(in.world_normal * 0.5 + vec3(0.5), 1.0);
#endif

#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth = in.unclipped_depth;
#endif // UNCLIPPED_DEPTH_ORTHO_EMULATION
#endif // PREPASS_PIPELINE

#ifndef PREPASS_PIPELINE
    var pbr_input = pbr_input_from_vertex_output(in, is_front, true);
    pbr_input.material.base_color = mix(root_color, tip_color, layer_f32 / layers_f32);

    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif // PREPASS_PIPELINE

    return out;
}
