#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_vertex_output,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#import "shaders/outline.wgsl"::apply_outline

struct MaskMaterial {
    color: vec4<f32>,
};

@group(2) @binding(0)
var<uniform> material: MaskMaterial;
@group(2) @binding(1)
var base_texture: texture_2d<f32>;
@group(2) @binding(2)
var base_sampler: sampler;
@group(2) @binding(3)
var mask_texture: texture_2d<f32>;
@group(2) @binding(4)
var mask_sampler: sampler;

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
#ifdef MULTISAMPLED
    @builtin(sample_index) sample_index: u32,
#endif
) -> FragmentOutput {
#ifndef MULTISAMPLED
    let sample_index = 0u;
#endif

    var pbr_input = pbr_input_from_vertex_output(in, is_front, false);

    let mask = textureSample(mask_texture, mask_sampler, in.uv);
    let base = textureSample(base_texture, base_sampler, in.uv);
    let color_amt = mask.r * mask.a;

    let color = mix(material.color, vec4<f32>(1.0), color_amt) * base;

    pbr_input.material.base_color = color;

    var out: FragmentOutput;
    // apply lighting
    out.color = apply_pbr_lighting(pbr_input);

    out.color = apply_outline(
        in.position,
        out.color, 
        false,
        sample_index,
    );

    // apply in-shader post processing (fog, alpha-premultiply, and also tonemapping, debanding if the camera is non-hdr)
    // note this does not include fullscreen postprocessing effects like bloom.
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

    return out;
}
