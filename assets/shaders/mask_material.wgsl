#import bevy_pbr::mesh_vertex_output MeshVertexOutput

struct MaskMaterial {
    color: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> material: MaskMaterial;
@group(1) @binding(1)
var base_texture: texture_2d<f32>;
@group(1) @binding(2)
var base_sampler: sampler;
@group(1) @binding(3)
var mask_texture: texture_2d<f32>;
@group(1) @binding(4)
var mask_sampler: sampler;

@fragment
fn fragment(
    in: MeshVertexOutput
) -> @location(0) vec4<f32> {
    let mask = textureSample(mask_texture, mask_sampler, in.uv);
    let base = textureSample(base_texture, base_sampler, in.uv);
    let color_amt = mask.r * mask.a;
    // TODO: proper lighting - easy after https://github.com/bevyengine/bevy/pull/7820 lands
    return vec4<f32>(mix(material.color, vec4<f32>(1.0), color_amt) * base);
}
