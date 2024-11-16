#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{SampleBias, sample_texture, alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_bindings::{material, emissive_texture, emissive_sampler},
    pbr_types::STANDARD_MATERIAL_FLAGS_EMISSIVE_TEXTURE_BIT,
    mesh_view_bindings::{globals, view},
    pbr_types,
}
#import boimp::shared::pack_pbrinput;

struct Bounds {
    min: u32,
    max: u32,
    height: f32,
    _padding0: u32,
}

struct SceneBounds {
    bounds: array<Bounds,8>,
    distance: f32,
    flags: u32,
    num_bounds: u32,
}

fn unpack_bounds(packed: u32) -> vec2<f32> {
    let x = i32((packed >> 16) & 0xFFFF);
    let x_signed = select(x, x - 0x10000, (x & 0x8000) != 0);
    let y = i32(packed & 0xFFFF);
    let y_signed = select(y, y - 0x10000, (y & 0x8000) != 0);
    return vec2<f32>(f32((x_signed) * 16), f32((y_signed) * 16));
}

const SHOW_OUTSIDE: u32 = 1u;
//const OUTLINE: u32 = 2u; // replaced by OUTLINE shader def
const OUTLINE_RED: u32 = 4u;
const OUTLINE_FORCE: u32 = 8u;

@group(2) @binding(100)
var<uniform> bounds: SceneBounds;

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
#ifdef OUTLINE
#ifdef MULTISAMPLED
    @builtin(sample_index) sample_index: u32,
#endif
#endif
) -> @location(0) vec2<u32> {
    // generate a PbrInput struct from the StandardMaterial bindings
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    var out: FragmentOutput;

#ifdef OUTLINE
#ifndef MULTISAMPLED
    let sample_index = 0u;
#endif
#endif

    // apply emmissive multiplier
    // dcl uses default 2.0 intensity. we also override bevy_pbr base emissive rules so that 
    // - if emissive texture is supplied but color is not, we use the texture (bevy by default multiplies emissive color and emissive texture, so color must be white to pass the texture through)
    // - if emissive color (== gltf emissive_intensity == dcl pbr emissive_color * emissive_intensity) is supplied but emissive texture is not, we use emissive color * base color
    // emissive color | emissive texture  | result
    // 0                no                  0
    // x                no                  x * base color
    // 0                t                   2 * t
    // x != 0           t                   x * t
    var emissive: vec4<f32> = material.emissive;
#ifdef VERTEX_UVS
    if ((material.flags & STANDARD_MATERIAL_FLAGS_EMISSIVE_TEXTURE_BIT) != 0u) {
        if dot(emissive, emissive) == 0.0 {
            emissive = vec4(2.0);
        }
        var bias: SampleBias;
        bias.mip_bias = view.mip_bias;
        emissive = vec4<f32>(emissive.rgb * sample_texture(
            emissive_texture,
            emissive_sampler,
#ifdef STANDARD_MATERIAL_EMISSIVE_UV_B
#ifdef VERTEX_UVS_B
            (material.uv_transform * vec3(in.uv_b, 1.0)).xy,
#else
            (material.uv_transform * vec3(in.uv, 1.0)).xy,
#endif
#else
            (material.uv_transform * vec3(in.uv, 1.0)).xy,
#endif
            bias,
        ).rgb, emissive.a);
    } else {
        if dot(emissive, emissive) != 0.0 {
            // emissive is set, no emissive texture, use base color texture as emissive texture
            emissive = emissive * pbr_input.material.base_color;
        }
    }
#endif
    // scale up for lumens
    pbr_input.material.emissive = emissive * 10.0;
    pbr_input.material.emissive.a = min(pbr_input.material.emissive.a, 1.0);

    let world_position = pbr_input.world_position.xyz;

    // check bounds
    var outside_amt: f32 = 9999.0;
    var nearest_region_distance: f32 = 9999.0;
    var nearest_region_height: f32 = 9999.0;
    if bounds.num_bounds > 0 {
        for (var ix = 0u; ix < bounds.num_bounds; ix += 1u) {
            let min_wp = unpack_bounds(bounds.bounds[ix].min);
            let max_wp = unpack_bounds(bounds.bounds[ix].max);

            let outside_xy = abs(clamp(world_position.xz, min_wp, max_wp) - world_position.xz);
            let distance = max(outside_xy.x, outside_xy.y);
            if distance < nearest_region_distance {
                nearest_region_distance = distance;
                nearest_region_height = bounds.bounds[ix].height;
            }
            outside_amt = min(outside_amt, distance);
        }
        let outside_height = max(world_position.y - nearest_region_height, 0.0);
        outside_amt = max(outside_amt, outside_height);
    } else {
        outside_amt = 0.0;
    }

    if outside_amt > 0.00 {
        discard;
    }

    // alpha discard
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    // use max of emissive and color (imposters only take albedo)
    pbr_input.material.base_color = max(pbr_input.material.base_color, pbr_input.material.emissive);

    return pack_pbrinput(pbr_input);
}
