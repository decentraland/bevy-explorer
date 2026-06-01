#import boimp::shared::{
    ImposterVertexOut, compose_over, pack_pbrinput, pack_props, passes_depth_check, unpack_pbrinput,
    unpack_props,
};
#import boimp::bindings::{sample_tile_material, UVSample};

struct BakeDims {
    width: u32,
}

@group(2) @binding(100)
var<uniform> offset: f32;

@group(3) @binding(0) var<storage, read_write> bake_buffer: array<vec2<u32>>;
@group(3) @binding(1) var<uniform> bake_dims: BakeDims;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) inverse_rotation_0c: vec3<f32>,
    @location(1) inverse_rotation_1c: vec3<f32>,
    @location(2) inverse_rotation_2c: vec3<f32>,
    @location(3) uv: vec2<f32>,
}

@fragment
fn fragment(in: VertexOut) {
    let inv_rot = mat3x3(
        in.inverse_rotation_0c,
        in.inverse_rotation_1c,
        in.inverse_rotation_2c,
    );

    // Floor is a flat plane — no parallax, so initial_depth=0 and dduddv=0.
    var sample: UVSample;
    sample.initial_uv = clamp(in.uv, vec2(0.0001), vec2(71.0/72.0 - 0.0001));
    sample.initial_depth = 0.0;
    sample.dduddv = vec2<f32>(0.0);
    var props = sample_tile_material(sample, vec2(0u,0u), vec2(offset, offset));
    if props.rgba.a <= 0.0 {
        discard;
    }

    var pbr_input = unpack_pbrinput(props, in.position);
    pbr_input.N = inv_rot * normalize(pbr_input.N);
    pbr_input.world_normal = pbr_input.N;

    let new_packed = pack_pbrinput(pbr_input);
    let new_props = unpack_props(new_packed);

    let pixel = vec2<u32>(in.position.xy);
    let idx = pixel.y * bake_dims.width + pixel.x;
    let existing = unpack_props(bake_buffer[idx]);
    if !passes_depth_check(new_props.depth, existing) {
        discard;
    }
    let composed = compose_over(existing, new_props);
    bake_buffer[idx] = pack_props(composed);
}
