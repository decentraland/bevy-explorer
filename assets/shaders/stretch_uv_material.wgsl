#import bevy_ui::ui_vertex_output::UiVertexOutput

@group(1) @binding(0) var image_texture: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;
@group(1) @binding(2) var<uniform> uvs: array<vec4<f32>,2>;
@group(1) @binding(3) var<uniform> color: vec4<f32>;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    // this is a bit lazy; really we should find a way to push the uvs directly into the vertex buffer
    let uv = mix(mix(uvs[0].xy, uvs[1].zw, in.uv.x), mix(uvs[0].zw, uvs[1].xy, in.uv.x), 1.0 - in.uv.y) * vec2<f32>(1.0, -1.0) + vec2<f32>(0.0, 1.0);
    return textureSample(image_texture, image_sampler, uv) * color;
}
