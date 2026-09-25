// Opaque vertex-colored rock/cliff surfaces. Standard vertex/depth passes;
// no foliage atlas, alpha test, wind weights, or deformation work.
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::main_pass_post_lighting_processing,
}
#import "embedded://visuals/landscape_lighting.wgsl"::landscape_lighting
#ifdef LANDSCAPE_COAST
#import bevy_pbr::mesh_view_bindings::globals
#import "embedded://visuals/grass_palette.wgsl"::{turf_color, turf_sample_grad, turf_normal}
#import "embedded://visuals/coast_surf.wgsl"::coast_surf
@group(2) @binding(100) var<uniform> land_bounds: vec4<f32>;
@group(2) @binding(102) var<uniform> root_color: vec4<f32>;
@group(2) @binding(103) var<uniform> tip_color: vec4<f32>;
@group(2) @binding(104) var<uniform> ground_detail: vec4<f32>;
#endif

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, front);
#ifdef LANDSCAPE_COAST
    // Alpha is a mesh surface tag, never transparency: the apron samples the
    // same meadow asset/palette as core terrain, without another draw call.
    let p = in.world_position.xz;
    let dx = dpdx(p);
    let dy = dpdy(p);
    let footprint = max(length(dx), length(dy));
    if in.color.a < 0.5 {
        // Explicit gradients keep sampling valid inside a surface-tag branch;
        // cliff and sand fragments pay no meadow texture fetch.
        let sample = turf_sample_grad(p, ground_detail, dx, dy);
        pbr.N = turf_normal(in.world_normal, sample, ground_detail);
        pbr.material.base_color = vec4(turf_color(root_color.rgb, tip_color.rgb, p,
            ground_detail, sample), 1.0);
    } else if in.world_position.y < -16.2 {
        let fade = 1.0 - smoothstep(0.08, 0.3, footprint);
        let sand = max(pbr.N.y, 0.0);
        var ripple = 0.0;
        // Minified and downward-facing sand have exactly zero ripple weight.
        if fade > 0.0 && sand > 0.0 {
            ripple = sin(dot(p, vec2(3.6, 7.8)) + sin(dot(p, vec2(0.31, -0.27))) * 1.8);
        }
        let wash = coast_surf(p * vec2(1.0, -1.0), land_bounds, globals.time, footprint);
        let dry = pbr.material.base_color.rgb * (1.0 + ripple * fade * sand * 0.035);
        let wet = dry * (1.0 - wash.z * 0.32);
        let film = mix(wet, vec3(0.09, 0.15, 0.19), wash.y * 0.35);
        pbr.material.base_color = vec4(mix(film, vec3(0.72, 0.78, 0.78), wash.x), 1.0);
    }
    pbr.material.base_color.a = 1.0;
#endif
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr, landscape_lighting(pbr, 0.0));
    return out;
}
