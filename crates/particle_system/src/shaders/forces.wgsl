// Gravity and the constant additional force, both given in world space.
#import "embedded://particle_system/shaders/common.wgsl"::{Particle, SimParams, quat_rotate}

fn apply_forces(p: ptr<function, Particle>, params: SimParams) {
#ifdef WORLD_SPACE
    let acceleration = params.acceleration;
#else
    let acceleration = quat_rotate(params.emitter_inv_rotation, params.acceleration);
#endif
    (*p).velocity += acceleration * params.dt;
}
