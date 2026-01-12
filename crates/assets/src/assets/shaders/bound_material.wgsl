#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{SampleBias, alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_bindings::{material, emissive_texture, emissive_sampler},
    pbr_types::{STANDARD_MATERIAL_FLAGS_EMISSIVE_TEXTURE_BIT, STANDARD_MATERIAL_FLAGS_BASE_COLOR_TEXTURE_BIT, STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT}
    mesh_view_bindings::{globals, view},
    pbr_types,
}
#import "embedded://shaders/simplex.wgsl"::simplex_noise_3d
#import "embedded://shaders/bound_material_effect.wgsl"::{apply_outline, discard_dither}

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
    _pad: u32,
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
const DISABLE_DITHER: u32 = 16u;
const CONE_ONLY_DITHER: u32 = 32u;

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
) -> FragmentOutput {
    var cap_brightness: f32 = 0.0;
    if (bounds.flags & (DISABLE_DITHER + OUTLINE_RED)) == 0 {
        cap_brightness = discard_dither(in.position.xy, in.world_position.xyz, view.user_value, (bounds.flags & CONE_ONLY_DITHER) == 0);
    }

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
    var emissive: vec3<f32> = material.emissive.rgb;
#ifdef VERTEX_UVS
    if ((material.flags & STANDARD_MATERIAL_FLAGS_EMISSIVE_TEXTURE_BIT) != 0u) {
        if dot(emissive, emissive) == 0.0 {
            emissive = vec3(2.0);
        }
        var bias: SampleBias;
        bias.mip_bias = view.mip_bias;
        emissive = vec3<f32>(emissive * textureSampleBias(
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
            bias.mip_bias,
        ).rgb);
    } else {
        // emissive is set, no emissive texture, use base color texture as emissive texture (only if present)
        if ((material.flags & STANDARD_MATERIAL_FLAGS_BASE_COLOR_TEXTURE_BIT) != 0u) {
            emissive = emissive * pbr_input.material.base_color.rgb;
        }
    }
#endif
    // scale up for lumens, use 0 for auto-exposure weight (alpha channel)
    pbr_input.material.emissive = vec4(emissive * 10.0, 0.0);

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

    var noise = 0.05;
    var should_discard = false;
    if outside_amt > 0.00 {
        if outside_amt < bounds.distance {
            noise = simplex_noise_3d(world_position * 2.0 + globals.time * vec3(0.2, 0.16, 0.24)) * 0.5 + 0.55;
            if noise < (outside_amt - 0.125) / 2.0 {
                should_discard = true;
            }
        } else if outside_amt > 0.05 {
            should_discard = true;
        }
    }

    if should_discard && ((bounds.flags & SHOW_OUTSIDE) == 0) {
        discard;
    }

    // alpha discard
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    // apply lighting
    if (pbr_input.material.flags & bevy_pbr::pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }

    if should_discard {
        out.color.a = out.color.a * 0.5;
        out.color.r = 4.0;
    } else {
        if noise < outside_amt / 2.0 {
            out.color = mix(out.color, vec4(10.0, 1.0, 0.0, 1.0), (outside_amt / 2.0 - noise) / 0.125);
        }
    }

#ifdef OUTLINE
    let alpha_mode = material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS;
    if (alpha_mode == pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_OPAQUE) || ((bounds.flags & OUTLINE_FORCE) != 0u) {
        out.color = apply_outline(
            in.position,
            out.color, 
            (bounds.flags & OUTLINE_RED) != 0u,
            sample_index,
        );
    }
#endif

    // apply in-shader post processing (fog, alpha-premultiply, and also tonemapping, debanding if the camera is non-hdr)
    // note this does not include fullscreen postprocessing effects like bloom.
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

    let cap_factor = max(max(out.color.r, out.color.g), max(out.color.b, 1.0));
    out.color = mix(out.color, vec4<f32>(out.color.rgb / cap_factor, out.color.a), saturate(cap_brightness * 2.0));

    return out;
}
