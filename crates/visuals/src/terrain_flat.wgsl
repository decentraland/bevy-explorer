// Flat-colour ground (parcel grass off): standard lighting, ending at the coast like the grass.

#ifdef PREPASS_PIPELINE
    #import bevy_pbr::prepass_io::{VertexOutput, FragmentOutput};
#else
    #import bevy_pbr::forward_io::{VertexOutput, FragmentOutput};
    #import bevy_pbr::pbr_fragment::pbr_input_from_standard_material;
    #import bevy_pbr::pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing};
#endif

#import "embedded://visuals/terrain_params.wgsl"::terrain
#import "embedded://visuals/coast_profile.wgsl"::beyond_coast

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    if beyond_coast(in.world_position.xz, terrain.coast) {
        discard;
    }

    var out: FragmentOutput;

#ifdef PREPASS_PIPELINE
#ifdef NORMAL_PREPASS
    out.normal = vec4(in.world_normal * 0.5 + vec3(0.5), 1.0);
#endif
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth = in.unclipped_depth;
#endif
#else
    let pbr_input = pbr_input_from_standard_material(in, is_front);
    out.color = main_pass_post_lighting_processing(pbr_input, apply_pbr_lighting(pbr_input));
#endif

    return out;
}
