// Cliffs and sand: vertex-coloured standard lighting, with the shore wash on the sand.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    mesh_view_bindings::globals,
}
#import "embedded://visuals/coast_surf.wgsl"::coast_surf

@group(2) @binding(100) var<uniform> land_bounds: vec4<f32>;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, is_front);
    if in.world_position.y < -16.2 {
        let p = in.world_position.xz;
        let footprint = max(length(dpdx(p)), length(dpdy(p)));
        let fade = 1.0 - smoothstep(0.08, 0.3, footprint);
        let sand = max(pbr.N.y, 0.0);
        var ripple = 0.0;
        if fade > 0.0 && sand > 0.0 {
            ripple = sin(dot(p, vec2(3.6, 7.8)) + sin(dot(p, vec2(0.31, -0.27))) * 1.8);
        }
        let wash = coast_surf(p * vec2(1.0, -1.0), land_bounds, globals.time, footprint);
        let dry = pbr.material.base_color.rgb * (1.0 + ripple * fade * sand * 0.035);
        let wet = dry * (1.0 - wash.z * 0.32);
        let film = mix(wet, vec3(0.09, 0.15, 0.19), wash.y * 0.35);
        pbr.material.base_color = vec4(mix(film, vec3(0.72, 0.78, 0.78), wash.x), 1.0);
    }
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr, apply_pbr_lighting(pbr));
    return out;
}
