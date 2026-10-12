// The sea: warped ripple slopes from two samples of one texture, lit by the standard pipeline,
// with shallow water and foam at the shore.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    mesh_view_bindings::globals,
}
#import "embedded://visuals/coast_surf.wgsl"::coast_surf
#import "embedded://visuals/coast_profile.wgsl"::coast_profile

#ifdef ENVIRONMENT_MAP
#import bevy_pbr::{ambient, clustered_forward::ClusterableObjectIndexRanges, environment_map, lighting, mesh_view_bindings::lights}
#import "embedded://shaders/sky_envmap_lookup.wgsl"::{sky_envmap_input, sky_envmap_level}

// the sky envmap, levelled towards the flat ambient light, is the only ambient light
// (see sky_envmap_lookup.wgsl)
override fn environment_map::compute_radiances(input: lighting::LayerLightingInput, clusterable_object_index_ranges: ptr<function, ClusterableObjectIndexRanges>, world_position: vec3<f32>, found_diffuse_indirect: bool) -> environment_map::EnvironmentMapRadiances {
    let upper = sky_envmap_input(input);
    let radiances = environment_map::compute_radiances(upper, clusterable_object_index_ranges, world_position, found_diffuse_indirect);
    return sky_envmap_level(radiances, input, upper, lights.ambient_color);
}

override fn ambient::ambient_light(world_position: vec4<f32>, world_normal: vec3<f32>, V: vec3<f32>, NdotV: f32, diffuse_color: vec3<f32>, specular_color: vec3<f32>, perceptual_roughness: f32, occlusion: vec3<f32>) -> vec3<f32> {
    return vec3(0.0);
}
#endif

@group(2) @binding(100) var<uniform> land_bounds: vec4<f32>;
@group(2) @binding(101) var ripples: texture_2d<f32>;
@group(2) @binding(102) var ripple_sampler: sampler;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, is_front);
    let p = in.world_position.xz;
    // long crossing swells carry smaller chop; the swell bends the chop's lookup so the two
    // layers don't read as rigid copies sliding over each other
    let swell_space = vec2(dot(p, vec2(0.939693, -0.342020)),
        dot(p, vec2(0.342020, 0.939693)));
    let swell = textureSample(ripples, ripple_sampler,
        swell_space * vec2(0.0073, 0.0091) + globals.time * vec2(0.007, -0.0045)).rg * 2.0 - 1.0;
    let chop_space = vec2(dot(p, vec2(0.529919, 0.848048)),
        dot(p, vec2(-0.848048, 0.529919)));
    let chop = textureSample(ripples, ripple_sampler,
        chop_space * vec2(0.02275, 0.02925) + swell * 0.72
        + globals.time * vec2(-0.005625, 0.0040625)).rg * 2.0 - 1.0;
    // rotate the sampled slopes back into world x/z
    let swell_slope = vec2(dot(swell, vec2(0.939693, 0.342020)),
        dot(swell, vec2(-0.342020, 0.939693)));
    let chop_slope = vec2(dot(chop, vec2(0.529919, -0.848048)),
        dot(chop, vec2(0.848048, 0.529919)));
    let chop_strength = clamp(0.16 + swell.x * 0.36 + swell.y * 0.22, 0.06, 0.28);
    let dx = dpdx(p);
    let dy = dpdy(p);
    let footprint = sqrt(max(dot(dx, dx), dot(dy, dy)));
    let unity = p * vec2(1.0, -1.0);
    let outside = max(max(land_bounds.xy - unity, unity - land_bounds.zw), vec2(0.0));
    // the waterline is at most 35 m out and the shore tint ends 18 m past it, so beyond 54 m
    // there is no foam or tint to compute
    var wash = vec4(0.0, 0.0, 0.0, 18.0);
    if dot(outside, outside) < 54.0 * 54.0 {
        wash = coast_surf(unity, land_bounds, globals.time, footprint);
    }
    // the sand below the waterline, 4 m down over 6 m in coast.rs, fading with depth
    let seabed = exp(-max(wash.w, 0.0) * (4.0 / 6.0) / 1.5);
    // calm the chop over the shallows, where the sand shows through
    let slope = swell_slope * 0.32 + chop_slope * chop_strength * (1.0 - seabed);
    pbr.N = normalize(vec3(-slope.x, 1.0, -slope.y));
    // tilt to the beach's slope at the waterline, so the water meets the sand's lighting: the
    // sand drops 1.7 m from coast.rs's shelf ring (ring 4) to the waterline
    let meet = 1.0 - smoothstep(0.0, 1.5, wash.w);
    if meet > 0.0 && wash.w > -3.0 {
        let edge = clamp(unity, land_bounds.xy, land_bounds.zw);
        let profile = coast_profile(edge);
        let shelf = profile.x + 7.5 + (profile.y - profile.x - 18.0) * 0.4;
        let beach = 1.7 / (profile.y - shelf);
        let o = normalize(unity - edge);
        let sand_normal = normalize(vec3(o.x * beach, 1.0, -o.y * beach));
        pbr.N = normalize(mix(pbr.N, sand_normal, meet));
    }
    let shore = 1.0 - smoothstep(0.0, 18.0, wash.w);
    let foam = wash.x;
    let deep = vec3(0.032, 0.065, 0.15);
    let shallow = vec3(0.09, 0.15, 0.19);
    // wet sand seen through water that absorbs the reds
    let sand = vec3(0.295, 0.233, 0.126);
    let water = mix(mix(deep, shallow, shore * 0.75), sand, seabed);
    let film = mix(water, vec3(0.09, 0.15, 0.19), wash.y * 0.35);
    pbr.material.base_color = vec4(mix(film, vec3(0.72, 0.78, 0.78), foam), 1.0);
    pbr.material.perceptual_roughness = mix(0.28, 0.62, foam);
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr, apply_pbr_lighting(pbr));
    return out;
}
