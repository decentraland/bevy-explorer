// Particle size: random start size, optionally lerped start->end over the lifetime.
#import "embedded://particle_system/shaders/common.wgsl"::{Particle, SimParams, RenderParams, Rng, rng_next, life_fraction}

fn initial_size(rng: ptr<function, Rng>, params: SimParams) -> f32 {
    return mix(params.size_min, params.size_max, rng_next(rng));
}

fn size_at(p: Particle, params: RenderParams) -> f32 {
#ifdef SIZE_OVER_TIME
    return p.size * mix(params.size_over_time.x, params.size_over_time.y, life_fraction(p));
#else
    return p.size;
#endif
}
