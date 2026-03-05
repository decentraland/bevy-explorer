#import bevy_pbr::{
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::{STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ADD, STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT},
}

#ifdef PREPASS_PIPELINE
 #import bevy_pbr::prepass_io::FragmentOutput;
#else
 #import bevy_pbr::forward_io::FragmentOutput;
#endif

#import boimp::shared::unpack_pbrinput;
#import boimp::bindings::sample_tile_material;

@group(2) @binding(100)
var<uniform> offset: f32;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) inverse_rotation_0c: vec3<f32>,
    @location(1) inverse_rotation_1c: vec3<f32>,
    @location(2) inverse_rotation_2c: vec3<f32>,
    @location(3) uv: vec2<f32>,
}

@fragment
fn fragment(in: VertexOut) -> FragmentOutput {
    var out: FragmentOutput;

#ifdef PREPASS_PIPELINE

    #ifdef NORMAL_PREPASS
        out.normal = vec4<f32>(0.0, 1.0, 0.0, 0.0);
    #endif
    // we don't support MOTION_VECTOR or DEFERRED
    #ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
        out.frag_depth = in.position.z;
    #endif

#else 

    let inv_rot = mat3x3(
        in.inverse_rotation_0c,
        in.inverse_rotation_1c,
        in.inverse_rotation_2c,
    );

    var props = sample_tile_material(vec4<f32>(clamp(in.uv, vec2(0.0001), vec2(17.0/18.0 - 0.0001)), vec2<f32>(0.0)), vec2(0u,0u), vec2(offset, offset));

    if props.rgba.a == 0.0 {
        // hacky - we are using opaque to ensure imposters render above the floor
        // so we have to put the ground color here
        // todo use some nice 
        props.rgba = vec4<f32>(0.07323897, 0.17064494, 0.033104762, 1.0);
        props.roughness = 1.0;
        props.metallic = 0.0;
        props.normal = vec3<f32>(0.0, 1.0, 0.0);
    }

    props.rgba.a = 1.0;

    var pbr_input = unpack_pbrinput(props, in.position);
    pbr_input.N = inv_rot * normalize(pbr_input.N);
    pbr_input.world_normal = pbr_input.N;
    pbr_input.material.flags |= STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ADD;
    out.color = apply_pbr_lighting(pbr_input);

    pbr_input.material.flags |= STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT;
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

#endif

    return out;
}
