// Cliffs and sand: vertex-coloured standard lighting, with the shore wash on the sand.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    mesh_view_bindings::globals,
}
#import "embedded://visuals/coast_surf.wgsl"::coast_surf
#import "embedded://visuals/coast_profile.wgsl"::{coast_outside, coast_profile}
#import "embedded://shaders/simplex.wgsl"::simplex_noise_2d

@group(2) @binding(100) var<uniform> land_bounds: vec4<f32>;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, is_front);
    // derivatives must be taken in uniform control flow (WebGPU rejects them in the branch)
    let p = in.world_position.xz;
    let footprint = max(length(dpdx(p)), length(dpdy(p)));
    if in.world_position.y < -16.2 {
        let fade = 1.0 - smoothstep(0.27, 0.8, footprint);
        let sand = max(pbr.N.y, 0.0);
        let unity = p * vec2(1.0, -1.0);
        // crest highlights and trough shadows vary independently over a few metres, averaging
        // to the plain profile
        let glint = 1.0 + simplex_noise_2d(p * 0.23) * 0.7;
        // wind ripples about 1 m apart along the beach: a gentle seaward face rising to a
        // sharp crest, a landward face that falls steeply then eases into a broad trough,
        // varying in spacing, shape and height along the shore
        var height = 0.5;
        if fade > 0.0 && sand > 0.0 {
            // across the beach from the cliff foot (coast.rs's ring 3, 12.3 m from the waterline
            // on average) to the waterline, so the bands run parallel to both; the meander calms
            // near the foot so bands don't cut into the rock, and the swash zone smooths out
            let shore = coast_profile(clamp(unity, land_bounds.xy, land_bounds.zw));
            let foot = shore.x + 5.7 + (shore.y - shore.x - 18.0) * 0.4;
            let out = coast_outside(unity, land_bounds);
            let across = (out - foot) / (shore.y - foot);
            let calm = smoothstep(0.0, 4.0, out - foot);
            let keep = 1.0 - smoothstep(-8.0, 0.0, out - shore.y);
            let phase = -across * 12.3
                + (simplex_noise_2d(p * 0.06) * 0.9 + simplex_noise_2d(p * 0.19) * 0.2) * calm;
            let band = fract(phase);
            let crest = 0.6 + simplex_noise_2d(p * 0.09) * 0.1;
            let amplitude = 0.75 + simplex_noise_2d(p * 0.045) * 0.45;
            var rise: f32;
            if band < crest {
                height = (band / crest) * (band / crest);
                rise = 2.0 * band / (crest * crest);
            } else {
                let t = (band - crest) / (1.0 - crest);
                height = (1.0 - t) * (1.0 - t);
                rise = -2.0 * (1.0 - t) / (1.0 - crest);
            }
            height = mix(0.5, height, amplitude * fade * sand * keep);
            // phase runs landward, so the surface climbs outward at -rise / 1 m per metre,
            // 0.1 m high
            let outward = unity - clamp(unity, land_bounds.xy, land_bounds.zw);
            if dot(outward, outward) > 0.0 {
                let peak = mix(1.0, glint, smoothstep(0.5, 0.9, height));
                let slope = -rise * 0.1 * amplitude * peak * fade * sand * keep;
                let o = normalize(outward);
                pbr.N = normalize(pbr.N - vec3(o.x, 0.0, -o.y) * slope);
            }
        }
        let tint = 1.0 + simplex_noise_2d(p * 0.05) * 0.06;
        pbr.material.perceptual_roughness = mix(pbr.material.perceptual_roughness, 0.7, sand);
        let wash = coast_surf(p * vec2(1.0, -1.0), land_bounds, globals.time, footprint);
        let shadow = simplex_noise_2d(p * 0.21 + 31.7) * 0.7;
        let trough = 1.0 - smoothstep(0.0, 0.35, height);
        let dry = pbr.material.base_color.rgb * tint
            * (0.93 + 0.1 * height + 0.14 * pow(height, 8.0) * glint)
            * (1.0 - 0.08 * trough * shadow);
        let wet = dry * (1.0 - wash.z * 0.32);
        // meet the water's colour and gloss at the waterline, where coast_water.wgsl starts its
        // seabed
        let sink = smoothstep(-1.5, 0.0, wash.w);
        let submerged = mix(wet, vec3(0.295, 0.233, 0.126), sink);
        pbr.material.perceptual_roughness = mix(pbr.material.perceptual_roughness, 0.28, sink);
        let film = mix(submerged, vec3(0.09, 0.15, 0.19), wash.y * 0.35);
        pbr.material.base_color = vec4(mix(film, vec3(0.72, 0.78, 0.78), wash.x), 1.0);
    }
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr, apply_pbr_lighting(pbr));
    return out;
}
