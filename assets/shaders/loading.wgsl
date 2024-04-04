#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::globals,
}
#import "shaders/simplex.wgsl"::simplex_noise_3d

struct LoadingData {
    player_pos_and_render: vec4<f32>,
}

@group(2) @binding(0)
var<uniform> material: LoadingData;

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    if material.player_pos_and_render.a == 0.0 {
        discard;
    }

    let world_position = in.world_position.xyz;
    let offset = world_position - material.player_pos_and_render.xyz;

    let noise = simplex_noise_3d(world_position * 2.0 + globals.time * vec3(0.2, 0.16, 0.24));

    var out: FragmentOutput;
    out.color.g = noise * 3.0;
    out.color.a = pow((1.0 - clamp(dot(offset, offset) * 0.01, 0.0, 1.0)) * 0.5, 2.0);
    return out;
}
