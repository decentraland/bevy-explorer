#import bevy_pbr::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput},
    view_transformations::position_world_to_clip,
    pbr_bindings::base_color_texture,
}
#import bevy_render::instance_index::get_instance_index

struct TextQuadData {
    valign: f32,
    halign: f32,
    pix_per_m: f32,
    add_y_pix: f32,
}

@group(1) @binding(200)
var<uniform> quad_data: TextQuadData;

@vertex
fn vertex(vertex_no_morph: Vertex) -> VertexOutput {
    var out: VertexOutput;
    var vertex = vertex_no_morph;

    // Use vertex_no_morph.instance_index instead of vertex.instance_index to work around a wgpu dx12 bug.
    // See https://github.com/gfx-rs/naga/issues/2416 .
    var model = mesh_functions::get_model_matrix(vertex_no_morph.instance_index);

    // we assume a quad with [-0.5, 0.5] bounds in x,y and 0 in z
    // then we take the pixels per meter and texture size to determine where the vertices should lie
    let tex_dims = textureDimensions(base_color_texture);
    let modified_vertex_position = vec3<f32>(
        (vertex.position.x + quad_data.halign) * f32(tex_dims.x) / quad_data.pix_per_m,
        ((vertex.position.y + quad_data.valign) * f32(tex_dims.y) + quad_data.add_y_pix) / quad_data.pix_per_m,
        0.0
    );

    out.world_position = mesh_functions::mesh_position_local_to_world(model, vec4<f32>(modified_vertex_position, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);

    out.uv = vertex.uv;

    // Use vertex_no_morph.instance_index instead of vertex.instance_index to work around a wgpu dx12 bug.
    // See https://github.com/gfx-rs/naga/issues/2416
    out.instance_index = get_instance_index(vertex_no_morph.instance_index);

    return out;
}
