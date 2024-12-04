#import bevy_pbr::{
    mesh_functions,
    view_transformations::{position_world_to_clip, position_view_to_world},
}

#ifdef PREPASS_PIPELINE
    #import bevy_pbr::prepass_io::Vertex;
#else
    #import bevy_pbr::forward_io::Vertex;
#endif

#import boimp::shared::ImposterVertexOut;
#import boimp::bindings::{imposter_data, sample_uvs_unbounded, grid_weights, sample_positions_from_camera_dir};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) inverse_rotation_0c: vec3<f32>,
    @location(1) inverse_rotation_1c: vec3<f32>,
    @location(2) inverse_rotation_2c: vec3<f32>,
    @location(3) uv: vec2<f32>,
}

@vertex
fn vertex(vertex: Vertex) -> VertexOut {
    var out: VertexOut;

    var model = mesh_functions::get_world_from_local(vertex.instance_index);

    let imposter_world_position = mesh_functions::mesh_position_local_to_world(model, vec4<f32>(0.0, 0.0, 0.0, 1.0)).xyz;

    // extract inverse rotation
    let inv_rot = transpose(mat3x3<f32>(
        normalize(model[0].xyz),
        normalize(model[1].xyz),
        normalize(model[2].xyz)
    ));

    out.inverse_rotation_0c = inv_rot[0];
    out.inverse_rotation_1c = inv_rot[1];
    out.inverse_rotation_2c = inv_rot[2];

    let world_position = mesh_functions::mesh_position_local_to_world(model, vec4<f32>(vertex.position, 1.0)).xyz;
    out.position = position_world_to_clip(world_position);
    out.uv = vertex.uv;

    return out;
}
