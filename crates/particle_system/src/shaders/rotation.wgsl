// Particle orientation: the start rotation and the rotation over time (an angular
// velocity, axis * radians per second).
#import "embedded://particle_system/shaders/common.wgsl"::{Particle, SimParams, quat_mul, quat_from_axis_angle}

fn initial_rotation(params: SimParams) -> vec4<f32> {
    return params.initial_rotation;
}

fn rotate_over_time(p: ptr<function, Particle>, params: SimParams) {
    let w = params.angular_velocity.xyz;
    let rate = length(w);
    if rate > 0.0 {
        let dq = quat_from_axis_angle(w / rate, rate * params.dt);
        (*p).rotation = normalize(quat_mul(dq, (*p).rotation));
    }
}
