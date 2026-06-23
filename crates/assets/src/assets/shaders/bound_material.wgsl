#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{SampleBias, alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_bindings::{material, emissive_texture, emissive_sampler},
    pbr_types::{STANDARD_MATERIAL_FLAGS_EMISSIVE_TEXTURE_BIT, STANDARD_MATERIAL_FLAGS_BASE_COLOR_TEXTURE_BIT, STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT},
    mesh_functions,
    mesh_view_bindings::{globals, view, lights},
    pbr_types,
    shadows,
}
#import bevy_core_pipeline::tonemapping::approximate_inverse_tone_mapping

#import "embedded://shaders/simplex.wgsl"::simplex_noise_3d
#import "embedded://shaders/bound_material_effect.wgsl"::{apply_outline, discard_dither}
#import "embedded://shaders/toon.wgsl"::toon_lighting

// SHADOW_OPACITY: how dark the sun's cast shadows are on the world.
// 1.0 = full black shadows (bevy default), 0.0 = no shadow at all.
// overriding the shadow fetch lifts the shadow value toward "lit" inside the
// single pbr lighting pass, so shadows become partial instead of fully
// occluding the sun — no second lighting evaluation needed.
const SHADOW_OPACITY: f32 = 0.5;

// Beyond the furthest shadow cascade bevy returns "fully lit" (shadow = 1.0),
// which pops hard at the shadow-distance edge. Instead, fade toward a partial-
// shadow floor. CASCADE_FAR_SHADOW is how far that floor sits from lit toward
// the in-cascade shadowed value: 0.5 = halfway between a fully-shadowed face
// and fully lit (scales with SHADOW_OPACITY so it tracks the near-field
// shadows). CASCADE_FAR_FADE is the fraction of the shadow distance to blend
// over.
const CASCADE_FAR_SHADOW: f32 = 0.5;
const CASCADE_FAR_FADE: f32 = 0.3;

override fn shadows::fetch_directional_shadow(light_id: u32, frag_position: vec4<f32>, surface_normal: vec3<f32>, view_z: f32) -> f32 {
    let base = shadows::fetch_directional_shadow(light_id, frag_position, surface_normal, view_z);
    let softened = mix(1.0, base, SHADOW_OPACITY);

    // far bound of the last cascade = the shadow distance for this light
    let light = &lights.directional_lights[light_id];
    let far = (*light).cascades[max((*light).num_cascades, 1u) - 1u].far_bound;
    if far <= 0.0 {
        return softened;
    }
    // floor halfway between the in-cascade shadowed value (1 - SHADOW_OPACITY)
    // and fully lit (1.0): 1 - CASCADE_FAR_SHADOW * SHADOW_OPACITY
    let far_shadow = 1.0 - CASCADE_FAR_SHADOW * SHADOW_OPACITY;
    // blend the (softened) shadow toward that floor over the last
    // CASCADE_FAR_FADE of the distance, holding the floor past the edge
    let fade = smoothstep(far * (1.0 - CASCADE_FAR_FADE), far, -view_z);
    return mix(softened, far_shadow, fade);
}

struct Bounds {
    min: u32,
    max: u32,
    height: f32,
    _padding0: u32,
}

struct SceneBounds {
    bounds: array<Bounds,8>,
    distance: f32,
    num_bounds: u32,
}

fn unpack_bounds(packed: u32) -> vec2<f32> {
    let x = i32((packed >> 16) & 0xFFFF);
    let x_signed = select(x, x - 0x10000, (x & 0x8000) != 0);
    let y = i32(packed & 0xFFFF);
    let y_signed = select(y, y - 0x10000, (y & 0x8000) != 0);
    return vec2<f32>(f32((x_signed) * 16), f32((y_signed) * 16));
}

@group(2) @binding(100)
var<uniform> bounds: SceneBounds;

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
#ifdef MULTISAMPLED
    @builtin(sample_index) sample_index: u32,
#endif
) -> FragmentOutput {

    // Lookup the tag for the given mesh
    let mesh_tag = mesh_functions::get_tag(in.instance_index);

#ifdef INVERTED_SCALE
    let is_front_m = !is_front;
#else
    let is_front_m = is_front;
#endif

    var cap_brightness: f32 = 0.0;
    if (mesh_tag & (#{NO_DITHERING_MESH_TAG} | #{OUTLINE_RED_MESH_TAG})) == 0 {
        cap_brightness = discard_dither(in.position.xy, in.world_position.xyz, view.user_value, (mesh_tag & #{CONE_ONLY_DITHER_MESH_TAG}) == 0);
    }

    // generate a PbrInput struct from the StandardMaterial bindings
    var pbr_input = pbr_input_from_standard_material(in, is_front_m);
    var out: FragmentOutput;

#ifndef MULTISAMPLED
    let sample_index = 0u;
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

    if should_discard && ((mesh_tag & #{SHOW_OUTSIDE_BOUNDS_MESH_TAG}) == 0) {
        discard;
    }

    // alpha discard
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    // apply lighting
    if (pbr_input.material.flags & bevy_pbr::pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        if (mesh_tag & #{TOON_MESH_TAG}) != 0u {
            // avatars: cel shading (own light model; uses the cast shadow only
            // to pick the lit-vs-shadowed band pair)
            out.color = toon_lighting(pbr_input);
        } else {
            out.color = apply_pbr_lighting(pbr_input);
        }
    } else {
        // invert tonemapping for unlit materials
        out.color = approximate_inverse_tone_mapping(pbr_input.material.base_color, view.color_grading); 
    }

    if should_discard {
        out.color.a = out.color.a * 0.5;
        out.color.r = 4.0;
    } else {
        if noise < outside_amt / 2.0 {
            out.color = mix(out.color, vec4(10.0, 1.0, 0.0, 1.0), (outside_amt / 2.0 - noise) / 0.125);
        }
    }

    if (mesh_tag & #{OUTLINE_MESH_TAGS}) != 0 {
        let outline_color = vec3(
            f32((mesh_tag & #{OUTLINE_RED_MESH_TAG}) != 0),
            f32((mesh_tag & #{OUTLINE_GREEN_MESH_TAG}) != 0),
            f32((mesh_tag & #{OUTLINE_BLUE_MESH_TAG}) != 0),
        );
        let black = (mesh_tag & (#{OUTLINE_MESH_TAGS} & ~#{OUTLINE_BLACK_MESH_TAG})) == 0;
        out.color = apply_outline(
            in.position,
            out.color, 
            outline_color,
            !black,
            sample_index,
        );
    }

    // apply in-shader post processing (fog, alpha-premultiply, and also tonemapping, debanding if the camera is non-hdr)
    // note this does not include fullscreen postprocessing effects like bloom.
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

    let cap_factor = max(max(out.color.r, out.color.g), max(out.color.b, 1.0));
    out.color = mix(out.color, vec4<f32>(out.color.rgb / cap_factor, out.color.a), saturate(cap_brightness * 2.0));

    if out.color.a < 0.001 {
        // avoid writing to the depth buffer for alpha-blend materials with low alpha
        discard;
    }

    return out;
}
