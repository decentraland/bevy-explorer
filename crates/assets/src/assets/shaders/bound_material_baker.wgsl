#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{SampleBias, alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_bindings::{material, emissive_texture, emissive_sampler},
    pbr_types::STANDARD_MATERIAL_FLAGS_EMISSIVE_TEXTURE_BIT,
    mesh_view_bindings::{globals, view},
    pbr_types,
}
#import boimp::shared::{compose_over, pack_pbrinput, pack_props, passes_depth_check, unpack_props};
#import "embedded://shaders/scene_bounds.wgsl"::{scene_bounds, scene_outside_amount}

struct BakeDims {
    width: u32,
}

@group(3) @binding(0) var<storage, read_write> bake_buffer: array<vec2<u32>>;
@group(3) @binding(1) var<uniform> bake_dims: BakeDims;

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) {
#ifdef INVERTED_SCALE
    let is_front_m = !is_front;
#else
    let is_front_m = is_front;
#endif

    // generate a PbrInput struct from the StandardMaterial bindings
    var pbr_input = pbr_input_from_standard_material(in, is_front_m);
    var out: FragmentOutput;

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
        emissive = vec4<f32>(emissive.rgb * textureSampleBias(
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
    let outside_amt = scene_outside_amount(world_position);

    if outside_amt > scene_bounds.distance {
        discard;
    }

    // alpha discard (material-specific: mask cutoff / opaque snap / blend preserve)
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    // skip fully-transparent fragments (no contribution to the composite)
    if pbr_input.material.base_color.a <= 0.0 {
        discard;
    }

    // use max of emissive and color (imposters only take albedo)
    // pbr_input.material.base_color = max(pbr_input.material.base_color, pbr_input.material.emissive);

    // composite the new fragment over whatever's already at this pixel in the bake buffer.
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
