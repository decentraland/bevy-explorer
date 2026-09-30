#import bevy_pbr::{
    prepass_bindings,
    prepass_io::{Vertex, VertexOutput, FragmentOutput},
    mesh_view_bindings::{view, previous_view_proj},
    pbr_types::STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_prepass_functions::prepass_alpha_discard,
    mesh_functions,
}
#import bevy_render::globals::Globals;

#import "embedded://shaders/simplex.wgsl"::simplex_noise_3d
#import "embedded://shaders/bound_material_effect.wgsl"::discard_dither
#import "embedded://shaders/scene_bounds.wgsl"::{scene_bounds, scene_outside_amount}

@group(0) @binding(1) var<uniform> globals: Globals;

@fragment
fn fragment(
    in: VertexOutput, 
    @builtin(front_facing) is_front: bool
) 

#ifdef PREPASS_FRAGMENT
-> FragmentOutput {
    var out: FragmentOutput;
#else
{
#endif
    // Lookup the tag for the given mesh
    let mesh_tag = mesh_functions::get_tag(in.instance_index);

#ifdef INVERTED_SCALE
    let is_front_m = !is_front;
#else
    let is_front_m = is_front;
#endif

    if (mesh_tag & (#{NO_DITHERING_MESH_TAG} | #{OUTLINE_RED_MESH_TAG})) == 0 {
        discard_dither(in.position.xy, in.world_position.xyz, view.user_value, (mesh_tag & #{CONE_ONLY_DITHER_MESH_TAG}) == 0);
    }

#ifdef NORMAL_PREPASS
    out.normal = vec4(in.world_normal * 0.5 + vec3(0.5), 1.0);
#endif

#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth = in.unclipped_depth;
#endif // UNCLIPPED_DEPTH_ORTHO_EMULATION

#ifdef MOTION_VECTOR_PREPASS
    let clip_position_t = view.unjittered_view_proj * in.world_position;
    let clip_position = clip_position_t.xy / clip_position_t.w;
    let previous_clip_position_t = prepass_bindings::previous_view_proj * in.previous_world_position;
    let previous_clip_position = previous_clip_position_t.xy / previous_clip_position_t.w;
    // These motion vectors are used as offsets to UV positions and are stored
    // in the range -1,1 to allow offsetting from the one corner to the
    // diagonally-opposite corner in UV coordinates, in either direction.
    // A difference between diagonally-opposite corners of clip space is in the
    // range -2,2, so this needs to be scaled by 0.5. And the V direction goes
    // down where clip space y goes up, so y needs to be flipped.
    out.motion_vector = (clip_position - previous_clip_position) * vec2(0.5, -0.5);
#endif // MOTION_VECTOR_PREPASS

#ifdef DEFERRED_PREPASS
    // There isn't any material info available for this default prepass shader so we are just writing 
    // emissive magenta out to the deferred gbuffer to be rendered by the first deferred lighting pass layer.
    // The is here so if the default prepass fragment is used for deferred magenta will be rendered, and also
    // as an example to show that a user could write to the deferred gbuffer if they were to start from this shader.
    out.deferred = vec4(0u, bevy_pbr::rgb9e5::vec3_to_rgb9e5_(vec3(1.0, 0.0, 1.0)), 0u, 0u);
    out.deferred_lighting_pass_id = 1u;
#endif

    let world_position = in.world_position.xyz;
    // check bounds
    let outside_amt = scene_outside_amount(world_position);

    var noise = 0.0;
    if outside_amt > 0.0 {
        if outside_amt < scene_bounds.distance {
            noise = simplex_noise_3d(world_position * 2.0 + globals.time * vec3(0.2, 0.16, 0.24)) * 0.5 + 0.55;
            if noise < (outside_amt - 0.125) / 2.0 {
                discard;
            }
        } else if outside_amt > 0.05 {
            discard;
        }
    }

    prepass_alpha_discard(in);

#ifdef PREPASS_FRAGMENT
    return out;
#endif
}
