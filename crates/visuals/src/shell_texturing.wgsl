#ifdef PREPASS_PIPELINE
    #import bevy_pbr::prepass_io::{VertexOutput, FragmentOutput};
#else
    #import bevy_pbr::forward_io::{VertexOutput, FragmentOutput};
    #import bevy_pbr::pbr_fragment::pbr_input_from_vertex_output;
    #import bevy_pbr::pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing};
    #import bevy_pbr::{mesh_view_bindings::lights, shadows};
    #import "embedded://shaders/shadow_softening.wgsl"::soften_directional_shadow
#endif
#import bevy_pbr::mesh_functions::get_tag;
#import bevy_pbr::mesh_view_bindings::view;

#import "embedded://shaders/simplex.wgsl"::simplex_noise_2d
#import "embedded://shaders/bound_material_effect.wgsl"::discard_dither
#import "embedded://visuals/terrain_params.wgsl"::terrain
#import "embedded://visuals/coast_profile.wgsl"::beyond_coast

#ifndef PREPASS_PIPELINE
// same softened sun shadows as scene materials (see bound_material.wgsl)
override fn shadows::fetch_directional_shadow(light_id: u32, frag_position: vec4<f32>, surface_normal: vec3<f32>, view_z: f32) -> f32 {
    let base = shadows::fetch_directional_shadow(light_id, frag_position, surface_normal, view_z);
    // far bound of the last cascade = the shadow distance for this light
    let light = &lights.directional_lights[light_id];
    let far = (*light).cascades[max((*light).num_cascades, 1u) - 1u].far_bound;
    return soften_directional_shadow(base, far, view_z);
}

#ifdef ENVIRONMENT_MAP
#import bevy_pbr::{ambient, clustered_forward::ClusterableObjectIndexRanges, environment_map, lighting}
#import "embedded://shaders/sky_envmap_lookup.wgsl"::{sky_envmap_input, sky_envmap_level}

// the sky envmap, levelled towards the flat ambient light, is the only ambient light
// (see sky_envmap_lookup.wgsl)
override fn environment_map::compute_radiances(input: lighting::LayerLightingInput, clusterable_object_index_ranges: ptr<function, ClusterableObjectIndexRanges>, world_position: vec3<f32>, found_diffuse_indirect: bool) -> environment_map::EnvironmentMapRadiances {
    let upper = sky_envmap_input(input);
    let radiances = environment_map::compute_radiances(upper, clusterable_object_index_ranges, world_position, found_diffuse_indirect);
    return sky_envmap_level(radiances, upper, lights.ambient_color);
}

override fn ambient::ambient_light(world_position: vec4<f32>, world_normal: vec3<f32>, V: vec3<f32>, NdotV: f32, diffuse_color: vec3<f32>, specular_color: vec3<f32>, perceptual_roughness: f32, occlusion: vec3<f32>) -> vec3<f32> {
    return vec3(0.0);
}
#endif
#endif

struct ShellTexture {
    subdivisions: u32,
    layers: u32,
    // xz the snap grid is anchored to
    snap_anchor: vec2<f32>,
    root_color: vec4<f32>,
    tip_color: vec4<f32>,
}

@group(2) @binding(0) var<uniform> shell: ShellTexture;

// blade offsets of up to ±0.425 of a cell
const OFFSET_SCALE: f32 = 0.85;

// PCG2D hash → 2 independent floats in [0, 1].
// From "Hash Functions for GPU Rendering" (Jarzynski & Olano, 2020).
// Good avalanche for small sequential integer inputs (typical cell coords).
fn cell_hash2(cell: vec2<i32>) -> vec2<f32> {
    var v = bitcast<vec2<u32>>(cell);                                                                                      
    v = v * 1664525u + 1013904223u;                                                                                        
    v.x += v.y * 1664525u;                                                                                                 
    v.y += v.x * 1664525u;                                                                                                 
    v ^= v >> vec2(16u);                                                                                                   
    v.x += v.y * 1664525u;                                                                                                 
    v.y += v.x * 1664525u;                                                                                                 
    v ^= v >> vec2(16u);                                                                                                   
    return vec2<f32>(f32(v.x), f32(v.y)) * (1.0 / 4294967296.0);    
}


@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    // metres of ground per pixel, taken before any discard so the derivatives stay uniform
    let footprint = max(fwidth(in.world_position.x), fwidth(in.world_position.z));

    // shells are double sided so the inside of a hill shows; dither them out between the camera
    // and the player like scene materials
    discard_dither(in.position.xy, in.world_position.xyz, view.user_value, true);
    if beyond_coast(in.world_position.xz, terrain.coast) {
        discard;
    }

    let tag = get_tag(in.instance_index);
    let layer = tag & 0xFFFF;
    let lod = tag >> 16;

    let layer_f32 = f32(layer);
    let layers_f32 = f32(shell.layers);
    let factor = layer_f32 / layers_f32;
    let color_factor = factor * color_attenuation(layers_f32);

    let subdivisions_f32 = f32(shell.subdivisions);

    // snap the position feeding the cells to a pixel-sized grid so the cell lattice can't beat
    // against the pixel grid into moire. the step varies per pixel, so the grid is only phase
    // coherent near its anchor; the anchor is kept near the player (see update_grass_anchor)
    var wp = in.world_position.xz;
    if footprint > 0.0 {
        wp = shell.snap_anchor + floor((wp - shell.snap_anchor) / footprint) * footprint;
    }
    let wpx = wp.x * subdivisions_f32;
    let wpz = wp.y * subdivisions_f32;

    // Snap to cell grid; integer coords feed the cheap hash.
    let cell = vec2<i32>(i32(round(wpx + 0.5)), i32(round(wpz + 0.5)));
    let simplex_coord = vec2(
        f32(cell.x) / subdivisions_f32,
        f32(cell.y) / subdivisions_f32,
    );

    // low-frequency octaves use 2D simplex (spatial continuity matters here).
    let octave1 = (simplex_noise_2d(simplex_coord * 0.112) + 1.) / 2.;
    let octave2 = (simplex_noise_2d(simplex_coord) + 1.0) / 2.;
    // independent hash calls for high-frequency octaves
    let octave3 = cell_hash2(cell * 7).x;
    let simplex = (octave1 + octave2 + octave3) / 3.;
    if simplex <= factor && layer > 0 {
        discard;
    }

    let disp_hash = cell_hash2(cell * 13);
    let dx = disp_hash.x - 0.5;
    let dz = disp_hash.y - 0.5;

    let root_x = wpx + dx * OFFSET_SCALE;
    let root_z = wpz + dz * OFFSET_SCALE;

    let blade_uv = fract(vec2(root_x, root_z)) - vec2(0.5);
    let threshold = mix(0.25, 0.45, 1. - (factor / simplex));
    if dot(blade_uv, blade_uv) >= threshold * threshold && layer > 0 {
        discard;
    }

    var out: FragmentOutput;

#ifdef PREPASS_PIPELINE
#ifdef NORMAL_PREPASS
    let normal = select(-in.world_normal, in.world_normal, is_front);
    out.normal = vec4(normal * 0.5 + vec3(0.5), 1.0);
#endif

#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth = in.unclipped_depth;
#endif // UNCLIPPED_DEPTH_ORTHO_EMULATION
#endif // PREPASS_PIPELINE

#ifndef PREPASS_PIPELINE
    var pbr_input = pbr_input_from_vertex_output(in, is_front, true);
    pbr_input.material.reflectance = vec3(0.125);
    pbr_input.material.base_color = mix(shell.root_color, shell.tip_color, color_factor);

    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif // PREPASS_PIPELINE

    return out;
}

fn color_attenuation(layers: f32) -> f32 {
    return 1 - (log(2 * layers) / (2 * log(10.))) + 0.6;
}
