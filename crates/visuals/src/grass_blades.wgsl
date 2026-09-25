// Original multi-leaf tuft cutouts: twelve leaves on two inexpensive crossed cards.
#import bevy_pbr::{mesh_functions, mesh_view_bindings::view, view_transformations::position_world_to_clip}
#ifdef PREPASS_PIPELINE
#import bevy_pbr::prepass_io::{Vertex, VertexOutput}
#ifdef PREPASS_FRAGMENT
#import bevy_pbr::prepass_io::FragmentOutput
#ifdef MOTION_VECTOR_PREPASS
#import bevy_pbr::pbr_prepass_functions::calculate_motion_vector
#endif
#endif
#import bevy_render::globals::Globals
@group(0) @binding(1) var<uniform> globals: Globals;
#else
#import bevy_pbr::{
    mesh_view_bindings::globals,
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_vertex_output,
    pbr_functions::main_pass_post_lighting_processing,
    pbr_types::STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT,
}
#import "embedded://visuals/landscape_lighting.wgsl"::landscape_lighting
#endif

@group(2) @binding(0) var<uniform> root_color: vec4<f32>;
#import "embedded://visuals/grass_palette.wgsl"::{turf_color, turf_sample}
@group(2) @binding(1) var<uniform> tip_color: vec4<f32>;
@group(2) @binding(2) var ground_mask: texture_2d<u32>;
@group(2) @binding(3) var leaf_mask: texture_2d_array<f32>;
@group(2) @binding(4) var leaf_sampler: sampler;
@group(2) @binding(5) var<uniform> form: vec4<f32>;
@group(2) @binding(6) var<uniform> shading: vec4<f32>;
@group(2) @binding(7) var<uniform> ground_detail: vec4<f32>;

fn wind(root: vec2<f32>, seed: f32, tip: f32, time: f32) -> vec3<f32> {
    let phase = dot(root, vec2(0.31, 0.23)) + time * (1.4 + seed * 0.3);
    let gust = 0.5 + 0.5 * sin(dot(root, vec2(0.065, -0.043)) - time * 0.8);
    let sway = (0.018 + gust * 0.025) * sin(phase) + 0.012 * gust;
    let flutter = 0.004 * sin(time * 4.3 + seed * 31.0 + phase);
    return vec3(sway * 0.8 + flutter, -abs(sway) * 0.22, sway * 0.6 - flutter) * tip * tip * form.w;
}

@vertex
fn vertex(in: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let transform = mesh_functions::get_world_from_local(in.instance_index);
    let root_world = transform * vec4(in.uv_b.x, in.color.x, in.uv_b.y, 1.0);
    let root = root_world.xz;
    var local = in.position;
    let clump = 0.5 + 0.5 * sin(root.x * 1.1 + sin(root.y * 0.8)) * sin(root.y * 1.3);
    local.y = in.color.x + (local.y - in.color.x) * form.x * (1.0 - form.z * clump);
    let sweep = form.y * in.uv.y * in.uv.y * (0.65 + in.color.y * 0.35);
    local.x += sweep * 0.8;
    local.z += sweep * 0.6;
    let world = mesh_functions::mesh_position_local_to_world(transform, vec4(local, 1.0));
    let size = vec2<i32>(textureDimensions(ground_mask));
    let pixel = vec2<i32>(floor(root * vec2(1.0, -1.0) / 16.0)) + size / 2;
    var visible = 1.0;
    if all(pixel >= vec2(0)) && all(pixel < size) {
        visible = select(1.0, 0.0, textureLoad(ground_mask, pixel, 0).r != 0u);
    }
    let fade = 1.0 - smoothstep(40.0, 64.0, distance(world.xyz, view.world_position));
    let visibility = visible * fade;
    // Collapsing all vertices of a masked blade also removes it from depth/normal prepasses.
    let position = mix(root_world, world, visibility);
    var current_wind = vec3(0.0);
    // Hidden/faded blades already collapse to their root. Do not animate zero displacement.
    if visibility > 0.0 {
        current_wind = wind(root, in.color.y, in.uv.y, globals.time);
    }
    out.world_position = position + vec4(current_wind * visible * fade, 0.0);
    out.position = position_world_to_clip(out.world_position.xyz);
    out.uv = in.uv;
    out.uv_b = in.uv_b;
    out.color = vec4(in.color.y, in.color.z, 0.0, 1.0);
#ifdef PREPASS_PIPELINE
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
    out.world_normal = mesh_functions::mesh_normal_local_to_world(in.normal, in.instance_index);
#endif
#ifdef MOTION_VECTOR_PREPASS
    var previous_wind = vec3(0.0);
    if visibility > 0.0 {
        previous_wind = wind(root, in.color.y, in.uv.y, globals.time - globals.delta_time);
    }
    out.previous_world_position = out.world_position + vec4(
        (previous_wind - current_wind) * visible * fade, 0.0);
#endif
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.unclipped_depth = out.position.z;
    out.position.z = min(out.position.z, 1.0);
#endif
#else
    out.world_normal = mesh_functions::mesh_normal_local_to_world(in.normal, in.instance_index);
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = in.instance_index;
#endif
    return out;
}

// Original silhouettes are baked once into a 171 KiB, coverage-preserving mip
// chain. Each fragment uses one filtered sample instead of a procedural loop.
fn cut_tuft(uv: vec2<f32>, seed: f32) {
    let variant = i32(floor(fract(seed) * 8.0));
    if textureSample(leaf_mask, leaf_sampler, uv, variant).r < 0.5 { discard; }
}

#ifdef PREPASS_PIPELINE
#ifdef PREPASS_FRAGMENT
@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    cut_tuft(in.uv, in.color.x + in.color.y * 0.17);
    var out: FragmentOutput;
#ifdef NORMAL_PREPASS
    out.normal = vec4(in.world_normal * 0.5 + vec3(0.5), 1.0);
#endif
#ifdef MOTION_VECTOR_PREPASS
    out.motion_vector = calculate_motion_vector(in.world_position, in.previous_world_position);
#endif
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth = in.unclipped_depth;
#endif
    return out;
}
#else
@fragment
fn fragment(in: VertexOutput) {
    cut_tuft(in.uv, in.color.x + in.color.y * 0.17);
}
#endif
#else
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) front: bool) -> FragmentOutput {
    cut_tuft(in.uv, in.color.x + in.color.y * 0.17);
    var pbr = pbr_input_from_vertex_output(in, front, false);
    let gradient = smoothstep(0.05, 0.95, in.uv.y);
    let ground = turf_color(root_color.rgb, tip_color.rgb, in.world_position.xz, ground_detail,
        turf_sample(in.world_position.xz, ground_detail));
    let tip = tip_color.rgb * (1.0 - shading.y * 0.5 + shading.y * in.color.x);
    let color = mix(ground, tip, gradient * shading.z);
    pbr.material.base_color = vec4(color, 1.0);
    pbr.material.perceptual_roughness = 0.95;
    pbr.material.reflectance = vec3(0.04);
    // Match the underlying turf's existing camera fog, not a second haze pass.
    pbr.material.flags |= STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT;
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr, landscape_lighting(pbr, gradient * shading.x));
    return out;
}
#endif
