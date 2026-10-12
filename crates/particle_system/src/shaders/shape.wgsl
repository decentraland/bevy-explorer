// Emitter shape: where a particle starts and which way it is thrown.
// Exactly one of SHAPE_SPHERE / SHAPE_CONE / SHAPE_BOX is defined, else point.
#import "embedded://particle_system/shaders/common.wgsl"::{SimParams, Rng, rng_next}

const POINT_RADIUS: f32 = 1.0 / 128.0;
const TAU: f32 = 6.28318530718;

fn random_unit_vector(rng: ptr<function, Rng>) -> vec3<f32> {
    let z = rng_next(rng) * 2.0 - 1.0;
    let a = rng_next(rng) * TAU;
    let r = sqrt(max(1.0 - z * z, 0.0));
    return vec3(r * cos(a), r * sin(a), z);
}

fn random_in_sphere(rng: ptr<function, Rng>) -> vec3<f32> {
    return random_unit_vector(rng) * pow(rng_next(rng), 1.0 / 3.0);
}

fn emit_position(rng: ptr<function, Rng>, params: SimParams) -> vec3<f32> {
#ifdef SHAPE_SPHERE
    return random_in_sphere(rng) * params.shape_radius;
#else ifdef SHAPE_BOX
    return (vec3(rng_next(rng), rng_next(rng), rng_next(rng)) - 0.5) * params.box_size;
#else ifdef SHAPE_CONE
    let r = params.shape_radius * sqrt(rng_next(rng));
    let a = rng_next(rng) * TAU;
    return vec3(r * cos(a), r * sin(a), 0.0);
#else
    return random_in_sphere(rng) * POINT_RADIUS;
#endif
}

fn emit_direction(position: vec3<f32>, rng: ptr<function, Rng>, params: SimParams) -> vec3<f32> {
#ifdef SHAPE_BOX
    return vec3(0.0, 0.0, -1.0);
#else ifdef SHAPE_CONE
    let axis = vec3(0.0, 0.0, -1.0);
    let distance_to_axis = length(position.xy);
    if distance_to_axis > 0.0 {
        let rotation_axis = normalize(cross(axis, position));
        let theta = params.cone_angle * clamp(distance_to_axis / max(params.shape_radius, 1e-6), 0.0, 1.0);
        // Rodrigues' rotation of the axis away from the centre line
        return axis * cos(theta)
            + cross(rotation_axis, axis) * sin(theta)
            + rotation_axis * dot(rotation_axis, axis) * (1.0 - cos(theta));
    }
    return axis;
#else
    let len = length(position);
    if len > 0.0 {
        return position / len;
    }
    return random_unit_vector(rng);
#endif
}
