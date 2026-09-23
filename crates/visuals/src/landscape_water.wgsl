// Original warped ripple slopes, two filtered flow samples. No FFT,
// tessellation, refraction pass or depth copy. Existing sky supplies reflections.
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    mesh_view_bindings::globals,
}
#import "embedded://visuals/coast_surf.wgsl"::coast_surf
@group(2) @binding(100) var<uniform> land_bounds: vec4<f32>;
@group(2) @binding(102) var ripples: texture_2d<f32>;
@group(2) @binding(103) var ripple_sampler: sampler;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, front);
    let p = in.world_position.xz;
    // Long crossing swells carry smaller chop in uneven patches. Neither UV
    // layer has a short shared tile period; the first bends the second rather
    // than sliding two rigid copies of the same pattern over one another.
    let swell_space = vec2(dot(p, vec2(0.939693, -0.342020)),
        dot(p, vec2(0.342020, 0.939693)));
    let swell = textureSample(ripples, ripple_sampler,
        swell_space * vec2(0.0073, 0.0091) + globals.time * vec2(0.0014, -0.0009)).rg * 2.0 - 1.0;
    let chop_space = vec2(dot(p, vec2(0.529919, 0.848048)),
        dot(p, vec2(-0.848048, 0.529919)));
    let chop = textureSample(ripples, ripple_sampler,
        chop_space * vec2(0.091, 0.117) + swell * 0.72
        + globals.time * vec2(-0.009, 0.0065)).rg * 2.0 - 1.0;
    // Rotate each sampled slope back into world X/Z. Implicit derivatives
    // include the UV distortion, so distant chop still filters smoothly.
    let swell_slope = vec2(dot(swell, vec2(0.939693, 0.342020)),
        dot(swell, vec2(-0.342020, 0.939693)));
    let chop_slope = vec2(dot(chop, vec2(0.529919, -0.848048)),
        dot(chop, vec2(0.848048, 0.529919)));
    let chop_strength = clamp(0.16 + swell.x * 0.36 + swell.y * 0.22, 0.06, 0.28);
    let slope = swell_slope * 0.32 + chop_slope * chop_strength;
    let dx = dpdx(p);
    let dy = dpdy(p);
    let footprint = sqrt(max(dot(dx, dx), dot(dy, dy)));
    pbr.N = normalize(vec3(-slope.x, 1.0, -slope.y));
    let unity = p * vec2(1.0, -1.0);
    let outside = max(max(land_bounds.xy - unity, unity - land_bounds.zw), vec2(0.0));
    // coast_profile's outer edge is at most 35m; shore tint saturates at 18m.
    // Beyond 54m (including 1m numerical margin), foam and shore tint are both
    // exactly zero. Most ocean pixels avoid three outline sines and a sqrt.
    var wash = vec4(0.0, 0.0, 0.0, 18.0);
    if dot(outside, outside) < 54.0 * 54.0 {
        wash = coast_surf(unity, land_bounds, globals.time, footprint);
    }
    let shore_distance = wash.w;
    let shore = 1.0 - smoothstep(0.0, 18.0, shore_distance);
    let foam = wash.x;
    let deep = vec3(0.032, 0.065, 0.15);
    let shallow = vec3(0.09, 0.15, 0.19);
    pbr.material.base_color = vec4(mix(mix(deep, shallow, shore * 0.75), vec3(0.72, 0.78, 0.78), foam), 1.0);
    pbr.material.perceptual_roughness = mix(0.28, 0.62, foam);
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr, apply_pbr_lighting(pbr));
    return out;
}
