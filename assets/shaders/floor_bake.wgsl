#import boimp::shared::{ImposterVertexOut, unpack_pbrinput, pack_pbrinput};
#import boimp::bindings::sample_tile_material;

@group(2) @binding(100)
var<uniform> offset: f32;

@fragment
fn fragment(in: ImposterVertexOut) -> @location(0) vec2<u32> {
    let inv_rot = mat3x3(
        in.inverse_rotation_0c,
        in.inverse_rotation_1c,
        in.inverse_rotation_2c,
    );

    var props = sample_tile_material(clamp(in.uv_c, vec2(0.0001), vec2(17.0/18.0 - 0.0001)), vec2(0u,0u), vec2(offset, offset));
    var pbr_input = unpack_pbrinput(props, in.position);
    pbr_input.N = inv_rot * normalize(pbr_input.N);
    pbr_input.world_normal = pbr_input.N;

    // write the imposter gbuffer
    return pack_pbrinput(pbr_input);
}
