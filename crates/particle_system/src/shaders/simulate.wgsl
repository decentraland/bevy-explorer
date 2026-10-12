// One thread per particle slot: advance live particles, then let dead slots claim one of
// this frame's `spawn_count` spawns through the atomic counter at the head of the buffer.
#import "embedded://particle_system/shaders/common.wgsl"::{Particle, SimParams, is_alive, Rng, rng_new, rng_next, quat_mul, quat_rotate}
#import "embedded://particle_system/shaders/shape.wgsl"::{emit_position, emit_direction}
#import "embedded://particle_system/shaders/forces.wgsl"::apply_forces
#import "embedded://particle_system/shaders/limit_velocity.wgsl"::apply_limit_velocity
#import "embedded://particle_system/shaders/rotation.wgsl"::{initial_rotation, rotate_over_time}
#import "embedded://particle_system/shaders/size.wgsl"::initial_size
#import "embedded://particle_system/shaders/color.wgsl"::initial_color

struct ParticleBuffer {
    spawn_claimed: atomic<u32>,
    _pad: vec3<u32>,
    particles: array<Particle>,
}

@group(0) @binding(0) var<uniform> params: SimParams;
@group(0) @binding(1) var<storage, read_write> particles: ParticleBuffer;

@compute @workgroup_size(64)
fn simulate(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if index >= params.capacity {
        return;
    }
    var p = particles.particles[index];
    if params.reset != 0u {
        p.lifetime = 0.0;
    }

    if is_alive(p) {
        p.age += params.dt;
        apply_forces(&p, params);
#ifdef LIMIT_VELOCITY
        apply_limit_velocity(&p, params);
#endif
        p.position += p.velocity * params.dt;
#ifdef ROTATE_OVER_TIME
        rotate_over_time(&p, params);
#endif
    }

    if !is_alive(p) {
        p.lifetime = 0.0;
        if params.spawn_count > 0u && atomicAdd(&particles.spawn_claimed, 1u) < params.spawn_count {
            var rng = rng_new(index, params.seed);
            spawn(&p, &rng);
        }
    }

    particles.particles[index] = p;
}

fn spawn(p: ptr<function, Particle>, rng: ptr<function, Rng>) {
    let position = emit_position(rng, params);
    let direction = emit_direction(position, rng, params);
    let speed = mix(params.speed_min, params.speed_max, rng_next(rng));
    (*p).age = 0.0;
    (*p).lifetime = params.lifetime;
#ifdef WORLD_SPACE
    (*p).position = (params.emitter_transform * vec4(position, 1.0)).xyz;
    (*p).velocity = quat_rotate(params.emitter_rotation, direction) * speed;
    (*p).rotation = quat_mul(params.emitter_rotation, initial_rotation(params));
#else
    (*p).position = position;
    (*p).velocity = direction * speed;
    (*p).rotation = initial_rotation(params);
#endif
    (*p).size = initial_size(rng, params);
    (*p).color = initial_color(rng, params);
    (*p).seed = (*rng).state;
}
