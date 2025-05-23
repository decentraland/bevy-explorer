@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;

@fragment
fn downsample(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    // original res 1024
    // new res 64

    // we want 8x8 samples at midpoints of the 16x16 pixels, which should be 
    // uv - 1.5 /1024 and uv + 0.5 / 1024
    var sample = vec4<f32>(0.0);

    sample += textureSample(input_texture, s, uv, vec2<i32>(-7,-7));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-7,-5));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-7,-3));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-7,-1));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-7,1));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-7,3));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-7,5));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-7,7));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-5,-7));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-5,-5));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-5,-3));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-5,-1));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-5,1));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-5,3));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-5,5));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-5,7));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-3,-7));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-3,-5));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-3,-3));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-3,-1));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-3,1));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-3,3));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-3,5));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-3,7));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-1,-7));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-1,-5));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-1,-3));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-1,-1));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-1,1));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-1,3));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-1,5));
    sample += textureSample(input_texture, s, uv, vec2<i32>(-1,7));
    sample += textureSample(input_texture, s, uv, vec2<i32>(7,-7));
    sample += textureSample(input_texture, s, uv, vec2<i32>(7,-5));
    sample += textureSample(input_texture, s, uv, vec2<i32>(7,-3));
    sample += textureSample(input_texture, s, uv, vec2<i32>(7,-1));
    sample += textureSample(input_texture, s, uv, vec2<i32>(7,1));
    sample += textureSample(input_texture, s, uv, vec2<i32>(7,3));
    sample += textureSample(input_texture, s, uv, vec2<i32>(7,5));
    sample += textureSample(input_texture, s, uv, vec2<i32>(7,7));
    sample += textureSample(input_texture, s, uv, vec2<i32>(5,-7));
    sample += textureSample(input_texture, s, uv, vec2<i32>(5,-5));
    sample += textureSample(input_texture, s, uv, vec2<i32>(5,-3));
    sample += textureSample(input_texture, s, uv, vec2<i32>(5,-1));
    sample += textureSample(input_texture, s, uv, vec2<i32>(5,1));
    sample += textureSample(input_texture, s, uv, vec2<i32>(5,3));
    sample += textureSample(input_texture, s, uv, vec2<i32>(5,5));
    sample += textureSample(input_texture, s, uv, vec2<i32>(5,7));
    sample += textureSample(input_texture, s, uv, vec2<i32>(3,-7));
    sample += textureSample(input_texture, s, uv, vec2<i32>(3,-5));
    sample += textureSample(input_texture, s, uv, vec2<i32>(3,-3));
    sample += textureSample(input_texture, s, uv, vec2<i32>(3,-1));
    sample += textureSample(input_texture, s, uv, vec2<i32>(3,1));
    sample += textureSample(input_texture, s, uv, vec2<i32>(3,3));
    sample += textureSample(input_texture, s, uv, vec2<i32>(3,5));
    sample += textureSample(input_texture, s, uv, vec2<i32>(3,7));
    sample += textureSample(input_texture, s, uv, vec2<i32>(1,-7));
    sample += textureSample(input_texture, s, uv, vec2<i32>(1,-5));
    sample += textureSample(input_texture, s, uv, vec2<i32>(1,-3));
    sample += textureSample(input_texture, s, uv, vec2<i32>(1,-1));
    sample += textureSample(input_texture, s, uv, vec2<i32>(1,1));
    sample += textureSample(input_texture, s, uv, vec2<i32>(1,3));
    sample += textureSample(input_texture, s, uv, vec2<i32>(1,5));
    sample += textureSample(input_texture, s, uv, vec2<i32>(1,7));

    return sample / 64.0;
}
