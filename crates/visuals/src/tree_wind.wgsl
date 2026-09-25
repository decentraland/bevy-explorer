// Original procedural wind. The same deformation is used for color, depth and shadows.
#import bevy_pbr::{mesh_functions, view_transformations::position_world_to_clip}
#ifdef PREPASS_PIPELINE
#import bevy_pbr::prepass_io::{Vertex, VertexOutput}
#import bevy_render::globals::Globals
// Shadow/depth passes use the compact prepass view layout, not forward binding 11.
@group(0) @binding(1) var<uniform> globals: Globals;
#else
#import bevy_pbr::forward_io::{Vertex, VertexOutput}
#import bevy_pbr::mesh_view_bindings::globals
#endif

@group(2) @binding(100) var<uniform> wind_settings: vec4<f32>;

fn wind(root: vec3<f32>, position: vec3<f32>, weights: vec2<f32>, time: f32) -> vec3<f32> {
    // Keep the rooted vertices fixed; rocks and cliffs use the rigid material.
    if all(weights == vec2(0.0)) { return vec3(0.0); }
    let phase = dot(root.xz, vec2(0.13, 0.19));
    let bend = sin(time * 0.9 + phase) * wind_settings.x * weights.x * weights.x;
    // Bark has zero flutter weight. Preserve the leaf expression and all pass
    // deformation exactly, without evaluating a second sine for trunk vertices.
    var flutter = 0.0;
    if weights.y != 0.0 {
        flutter = sin(time * 3.7 + dot(position, vec3(1.7, 0.8, 2.1))) * wind_settings.y * weights.y;
    }
    return vec3(bend + flutter, flutter * 0.2, bend * 0.6 - flutter * 0.3);
}

@vertex
fn vertex(in: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let transform = mesh_functions::get_world_from_local(in.instance_index);
    let root = transform[3].xyz;
    let world = mesh_functions::mesh_position_local_to_world(transform, vec4(in.position, 1.0));
    out.world_position = world + vec4(wind(root, world.xyz, in.uv_b, globals.time), 0.0);
    out.position = position_world_to_clip(out.world_position.xyz);
    out.uv = in.uv;
    out.uv_b = in.uv_b;
    out.color = in.color;
#ifdef PREPASS_PIPELINE
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
    out.world_normal = mesh_functions::mesh_normal_local_to_world(in.normal, in.instance_index);
#endif
#ifdef MOTION_VECTOR_PREPASS
    let previous = mesh_functions::get_previous_world_from_local(in.instance_index);
    let previous_world = mesh_functions::mesh_position_local_to_world(previous, vec4(in.position, 1.0));
    out.previous_world_position = previous_world + vec4(wind(previous[3].xyz, previous_world.xyz, in.uv_b, globals.time - globals.delta_time), 0.0);
#endif
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.unclipped_depth = out.position.z;
    out.position.z = min(out.position.z, 1.0);
#endif
#else
    out.world_normal = mesh_functions::mesh_normal_local_to_world(in.normal, in.instance_index);
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = in.instance_index;
#endif
    return out;
}
