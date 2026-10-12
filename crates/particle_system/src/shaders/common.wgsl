// Shared types for the particle simulation (compute) and render shaders.
// Layouts must match `SimParams`, `RenderParams` and `Particle` in config.rs.

struct Particle {
    position: vec3<f32>,
    age: f32,
    velocity: vec3<f32>,
    lifetime: f32,
    rotation: vec4<f32>,
    color: vec4<f32>,
    size: f32,
    seed: u32,
    _pad: vec2<f32>,
}

// Per-emitter simulation parameters. Every effect has a slot; unused ones are zero.
struct SimParams {
    emitter_transform: mat4x4<f32>,
    emitter_rotation: vec4<f32>,
    emitter_inv_rotation: vec4<f32>,
    initial_rotation: vec4<f32>,
    angular_velocity: vec4<f32>,
    color_min: vec4<f32>,
    color_max: vec4<f32>,
    acceleration: vec3<f32>,
    lifetime: f32,
    box_size: vec3<f32>,
    shape_radius: f32,
    cone_angle: f32,
    speed_min: f32,
    speed_max: f32,
    size_min: f32,
    size_max: f32,
    limit_speed: f32,
    limit_dampen: f32,
    dt: f32,
    seed: u32,
    // the live ring: slots [first_slot, first_slot + slot_count) mod capacity, the last
    // spawn_count of which are spawned this step
    first_slot: u32,
    slot_count: u32,
    spawn_count: u32,
    capacity: u32,
    _pad: vec2<u32>,
}

// Per-emitter render parameters.
struct RenderParams {
    color_over_time_start: vec4<f32>,
    color_over_time_end: vec4<f32>,
    size_over_time: vec2<f32>,
    sprite_tiles: vec2<u32>,
    frames_per_second: f32,
    capacity: u32,
    _pad: vec2<f32>,
}

fn is_alive(p: Particle) -> bool {
    return p.lifetime > 0.0 && p.age < p.lifetime;
}

fn life_fraction(p: Particle) -> f32 {
    return clamp(p.age / p.lifetime, 0.0, 1.0);
}

struct Rng {
    state: u32,
}

fn pcg_hash(input: u32) -> u32 {
    let state = input * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn rng_new(index: u32, seed: u32) -> Rng {
    return Rng(pcg_hash(index ^ pcg_hash(seed)));
}

fn rng_next(rng: ptr<function, Rng>) -> f32 {
    (*rng).state = pcg_hash((*rng).state);
    return f32((*rng).state) / 4294967296.0;
}

fn quat_mul(a: vec4<f32>, b: vec4<f32>) -> vec4<f32> {
    return vec4(a.w * b.xyz + b.w * a.xyz + cross(a.xyz, b.xyz), a.w * b.w - dot(a.xyz, b.xyz));
}

fn quat_rotate(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    let t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

fn quat_from_axis_angle(axis: vec3<f32>, angle: f32) -> vec4<f32> {
    let half = angle * 0.5;
    return vec4(axis * sin(half), cos(half));
}
