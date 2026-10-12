// Quad orientation: camera-facing (BILLBOARD), along the velocity (FACE_TRAVEL), or the
// particle's own rotation. `velocity` is in world space.
#import "embedded://particle_system/shaders/common.wgsl"::{Particle, quat_rotate}

struct QuadAxes {
    right: vec3<f32>,
    up: vec3<f32>,
}

fn quad_axes(p: Particle, velocity: vec3<f32>, world_from_local: mat4x4<f32>, world_from_view: mat4x4<f32>) -> QuadAxes {
    let view_right = world_from_view[0].xyz;
    let view_up = world_from_view[1].xyz;
    let view_back = world_from_view[2].xyz;
    var axes = QuadAxes(view_right, view_up);
#ifdef FACE_TRAVEL
    let speed = length(velocity);
    if speed > 1e-5 {
        axes.up = velocity / speed;
        let right = cross(axes.up, view_back);
        let right_len = length(right);
        if right_len > 1e-5 {
            axes.right = right / right_len;
        }
    }
#else ifndef BILLBOARD
    axes.right = quat_rotate(p.rotation, vec3(1.0, 0.0, 0.0));
    axes.up = quat_rotate(p.rotation, vec3(0.0, 1.0, 0.0));
#ifndef WORLD_SPACE
    axes.right = normalize((world_from_local * vec4(axes.right, 0.0)).xyz);
    axes.up = normalize((world_from_local * vec4(axes.up, 0.0)).xyz);
#endif
#endif
    return axes;
}
