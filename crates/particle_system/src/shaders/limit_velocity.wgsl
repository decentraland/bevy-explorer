// Limit velocity: remove `limit_dampen` of the speed above `limit_speed` each frame.
#import "embedded://particle_system/shaders/common.wgsl"::{Particle, SimParams}

fn apply_limit_velocity(p: ptr<function, Particle>, params: SimParams) {
    let speed = length((*p).velocity);
    if speed > params.limit_speed && speed > 0.0 {
        let excess = speed - params.limit_speed;
        let dampened = mix(speed, speed - excess, params.limit_dampen);
        (*p).velocity *= dampened / speed;
    }
}
