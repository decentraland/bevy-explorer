// Particle colour: random start colour between two colours, optionally multiplied by a
// start->end lerp over the lifetime. All colours are linear.
#import "embedded://particle_system/shaders/common.wgsl"::{Particle, SimParams, RenderParams, Rng, rng_next, life_fraction}

fn initial_color(rng: ptr<function, Rng>, params: SimParams) -> vec4<f32> {
    return mix(params.color_min, params.color_max, rng_next(rng));
}

fn color_at(p: Particle, params: RenderParams) -> vec4<f32> {
#ifdef COLOR_OVER_TIME
    return p.color * mix(params.color_over_time_start, params.color_over_time_end, life_fraction(p));
#else
    return p.color;
#endif
}
