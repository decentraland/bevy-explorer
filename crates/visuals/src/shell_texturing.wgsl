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

// Pre-calculated constant: (0.85 * 0.5) = 0.425
const SCALED_DIST: f32 = 0.425;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {

    let tag = get_tag(in.instance_index);
    let layer = tag & 0xFFFF;
    let lod = tag >> 16;

    let layer_f32 = f32(layer);
    let layers_f32 = f32(layers);
    let factor = layer_f32 / layers_f32;
    let color_factor = factor * color_attenuation(layers_f32);

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
    let octave3 = (simplex_noise_3d(simplex_coord * 13.167) + 1.) / 2.;
    let simplex = (octave1 + octave2 + octave3) / 3.;
    if simplex <= factor && layer > 0 {
        discard;
    }

    let octave4 = (simplex_noise_3d(simplex_coord * 212.167) + 1.) / 2.;
    // 1. Shift to [-0.5, 0.5] range (1 Subtraction each)
    let dx = fract(octave3) - 0.5;
    let dz = fract(octave4) - 0.5;

    // 2. Find the square-space "radius" (2 Abs, 1 Max)
    // Small epsilon 1e-5 prevents division by zero
    let edge_dist = max(abs(dx), abs(dz)) + 0.00001;

    // 3. Combined Scale (1 Division, 1 Multiplication)
    // This maps the square point back to a circle of radius 0.85
    let scale = SCALED_DIST / edge_dist;

    // 4. Final Position (2 Multiplications, 2 Additions)
    let root_x = wpx + dx * scale;
    let root_z = wpz + dz * scale;

    if distance(fract(vec2(root_x, root_z)), vec2(0.5)) >= mix(0.25, 0.45, 1. - (factor / simplex)) && layer > 0 {
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
    pbr_input.material.base_color = mix(root_color, tip_color, color_factor);

    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif // PREPASS_PIPELINE

    return out;
}

fn color_attenuation(layers: f32) -> f32 {
    return 1 - (log(2 * layers) / (2 * log(10.))) + 0.6;
}
