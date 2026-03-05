#import boimp::shared::{ImposterVertexOut, unpack_pbrinput, pack_pbrinput};
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
fn fragment(in: VertexOut) -> @location(0) vec2<u32> {
    let inv_rot = mat3x3(
        in.inverse_rotation_0c,
        in.inverse_rotation_1c,
        in.inverse_rotation_2c,
    );

    var props = sample_tile_material(vec4<f32>(clamp(in.uv, vec2(0.0001), vec2(17.0/18.0 - 0.0001)), vec2<f32>(0.0)), vec2(0u,0u), vec2(offset, offset));
    var pbr_input = unpack_pbrinput(props, in.position);
    pbr_input.N = inv_rot * normalize(pbr_input.N);
    pbr_input.world_normal = pbr_input.N;

    // write the imposter gbuffer
    return pack_pbrinput(pbr_input);
}
