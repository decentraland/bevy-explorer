#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_vertex_output,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    mesh_view_bindings::globals,
}
#import "shaders/simplex.wgsl"::simplex_noise_3d
#import "shaders/outline.wgsl"::apply_outline

struct Bounds {
    min: u32,
    max: u32,
    height: f32,
    _padding0: u32,
}

fn unpack_bounds(packed: u32) -> vec2<f32> {
    let x = i32((packed >> 16) & 0xFFFF);
    let x_signed = select(x, x - 0x10000, (x & 0x8000) != 0);
    let y = i32(packed & 0xFFFF);
    let y_signed = select(y, y - 0x10000, (y & 0x8000) != 0);
    return vec2<f32>(f32((x_signed) * 16), f32((y_signed) * 16));
}

struct MaskMaterial {
    bounds: array<Bounds,8>,
    color: vec4<f32>,
    distance: f32,
    num_bounds: u32,
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

    let world_position = pbr_input.world_position.xyz;
    // check bounds
    var outside_amt: f32 = 9999.0;
    var nearest_region_distance: f32 = 9999.0;
    var nearest_region_height: f32 = 9999.0;
    if material.num_bounds > 0 {
        for (var ix = 0u; ix < material.num_bounds; ix += 1u) {
            let min_wp = unpack_bounds(material.bounds[ix].min);
            let max_wp = unpack_bounds(material.bounds[ix].max);

            let outside_xy = abs(clamp(world_position.xz, min_wp, max_wp) - world_position.xz);
            let distance = max(outside_xy.x, outside_xy.y);
            if distance < nearest_region_distance {
                nearest_region_distance = distance;
                nearest_region_height = material.bounds[ix].height;
            }
            outside_amt = min(outside_amt, distance);
        }
        let outside_height = max(world_position.y - nearest_region_height, 0.0);
        outside_amt = max(outside_amt, outside_height);
    } else {
        outside_amt = 0.0;
    }

    var noise = 0.05;
    if outside_amt > 0.00 {
        if outside_amt < material.distance {
            noise = simplex_noise_3d(world_position * 2.0 + globals.time * vec3(0.2, 0.16, 0.24)) * 0.5 + 0.55;
            if noise < (outside_amt - 0.125) / 2.0 {
                discard;
            }
        } else if outside_amt > 0.05 {
            discard;
        }
    }

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
