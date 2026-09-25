// A single matte turf surface replaces the old five-layer, noisy shell carpet.
#ifdef PREPASS_PIPELINE
#import bevy_pbr::prepass_io::{VertexOutput, FragmentOutput}
#ifdef MOTION_VECTOR_PREPASS
#import bevy_pbr::pbr_prepass_functions::calculate_motion_vector
#endif
#else
#import bevy_pbr::forward_io::{VertexOutput, FragmentOutput}
#import bevy_pbr::pbr_fragment::pbr_input_from_vertex_output
#import bevy_pbr::pbr_functions::main_pass_post_lighting_processing
#import bevy_pbr::pbr_types::STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT
#import "embedded://visuals/landscape_lighting.wgsl"::landscape_lighting
#endif
#import "embedded://visuals/grass_palette.wgsl"::{turf_color, turf_sample, turf_normal}

@group(2) @binding(3) var<uniform> root_color: vec4<f32>;
@group(2) @binding(4) var<uniform> tip_color: vec4<f32>;
@group(2) @binding(6) var<uniform> ground_detail: vec4<f32>;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) front: bool) -> FragmentOutput {
    var out: FragmentOutput;
#ifdef PREPASS_PIPELINE
#ifdef NORMAL_PREPASS
    let sample = turf_sample(in.world_position.xz, ground_detail);
    let normal = turf_normal(in.world_normal, sample, ground_detail);
    out.normal = vec4(normal * 0.5 + vec3(0.5), 1.0);
#endif
#ifdef MOTION_VECTOR_PREPASS
    out.motion_vector = calculate_motion_vector(in.world_position, in.previous_world_position);
#endif
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth = in.unclipped_depth;
#endif
#else
    let sample = turf_sample(in.world_position.xz, ground_detail);
    let normal = turf_normal(in.world_normal, sample, ground_detail);
    var pbr = pbr_input_from_vertex_output(in, front, true);
    pbr.N = normal;
    pbr.material.base_color = vec4(turf_color(root_color.rgb, tip_color.rgb, in.world_position.xz, ground_detail, sample), 1.0);
    pbr.material.perceptual_roughness = 1.0;
    pbr.material.reflectance = vec3(0.04);
    // Custom PbrInput starts without StandardMaterial's default fog flag.
    // Consume the camera's existing fog, matching rocks and trees.
    pbr.material.flags |= STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT;
    out.color = main_pass_post_lighting_processing(pbr, landscape_lighting(pbr, 0.0));
#endif
    return out;
}
