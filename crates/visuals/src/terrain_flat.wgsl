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

#ifdef ENVIRONMENT_MAP
#import bevy_pbr::{ambient, clustered_forward::ClusterableObjectIndexRanges, environment_map, lighting, mesh_view_bindings::lights}
#import "embedded://shaders/sky_envmap_lookup.wgsl"::{sky_envmap_input, sky_envmap_level}

// the sky envmap, levelled towards the flat ambient light, is the only ambient light
// (see sky_envmap_lookup.wgsl)
override fn environment_map::compute_radiances(input: lighting::LayerLightingInput, clusterable_object_index_ranges: ptr<function, ClusterableObjectIndexRanges>, world_position: vec3<f32>, found_diffuse_indirect: bool) -> environment_map::EnvironmentMapRadiances {
    let upper = sky_envmap_input(input);
    let radiances = environment_map::compute_radiances(upper, clusterable_object_index_ranges, world_position, found_diffuse_indirect);
    return sky_envmap_level(radiances, upper, lights.ambient_color);
}

override fn ambient::ambient_light(world_position: vec4<f32>, world_normal: vec3<f32>, V: vec3<f32>, NdotV: f32, diffuse_color: vec3<f32>, specular_color: vec3<f32>, perceptual_roughness: f32, occlusion: vec3<f32>) -> vec3<f32> {
    return vec3(0.0);
}
#endif

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
