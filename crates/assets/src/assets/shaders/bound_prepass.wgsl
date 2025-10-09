#import bevy_pbr::{
    prepass_bindings,
    prepass_io::{Vertex, VertexOutput, FragmentOutput},
    mesh_view_bindings::{view, previous_view_proj},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_prepass_functions::prepass_alpha_discard,
}
#import bevy_render::globals::Globals;

#import "embedded://shaders/simplex.wgsl"::simplex_noise_3d

@group(0) @binding(1) var<uniform> globals: Globals;

struct Bounds {
    min: u32,
    max: u32,
    height: f32,
    _padding0: u32,
}

struct SceneBounds {
    bounds: array<Bounds,8>,
    distance: f32,
    flags: u32,
    num_bounds: u32,
    _pad: u32,
}

fn unpack_bounds(packed: u32) -> vec2<f32> {
    let x = i32((packed >> 16) & 0xFFFF);
    let x_signed = select(x, x - 0x10000, (x & 0x8000) != 0);
    let y = i32(packed & 0xFFFF);
    let y_signed = select(y, y - 0x10000, (y & 0x8000) != 0);
    return vec2<f32>(f32((x_signed) * 16), f32((y_signed) * 16));
}

@group(2) @binding(100)
var<uniform> bounds: SceneBounds;


#ifdef PREPASS_FRAGMENT
@fragment
fn fragment(
    in: VertexOutput, 
    @builtin(front_facing) is_front: bool
) -> FragmentOutput {
    var out: FragmentOutput;

#ifdef NORMAL_PREPASS
    out.normal = vec4(in.world_normal * 0.5 + vec3(0.5), 1.0);
#endif

#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth = in.unclipped_depth;
#endif // UNCLIPPED_DEPTH_ORTHO_EMULATION

#ifdef MOTION_VECTOR_PREPASS
    let clip_position_t = view.unjittered_view_proj * in.world_position;
    let clip_position = clip_position_t.xy / clip_position_t.w;
    let previous_clip_position_t = prepass_bindings::previous_view_proj * in.previous_world_position;
    let previous_clip_position = previous_clip_position_t.xy / previous_clip_position_t.w;
    // These motion vectors are used as offsets to UV positions and are stored
    // in the range -1,1 to allow offsetting from the one corner to the
    // diagonally-opposite corner in UV coordinates, in either direction.
    // A difference between diagonally-opposite corners of clip space is in the
    // range -2,2, so this needs to be scaled by 0.5. And the V direction goes
    // down where clip space y goes up, so y needs to be flipped.
    out.motion_vector = (clip_position - previous_clip_position) * vec2(0.5, -0.5);
#endif // MOTION_VECTOR_PREPASS

#ifdef DEFERRED_PREPASS
    // There isn't any material info available for this default prepass shader so we are just writing 
    // emissive magenta out to the deferred gbuffer to be rendered by the first deferred lighting pass layer.
    // The is here so if the default prepass fragment is used for deferred magenta will be rendered, and also
    // as an example to show that a user could write to the deferred gbuffer if they were to start from this shader.
    out.deferred = vec4(0u, bevy_pbr::rgb9e5::vec3_to_rgb9e5_(vec3(1.0, 0.0, 1.0)), 0u, 0u);
    out.deferred_lighting_pass_id = 1u;
#endif

    let world_position = in.world_position.xyz;
    // check bounds
    var outside_amt: f32 = 9999.0;
    var nearest_region_distance: f32 = 9999.0;
    var nearest_region_height: f32 = 9999.0;
    if bounds.num_bounds > 0 {
        for (var ix = 0u; ix < bounds.num_bounds; ix += 1u) {
            let min_wp = unpack_bounds(bounds.bounds[ix].min);
            let max_wp = unpack_bounds(bounds.bounds[ix].max);

            let outside_xy = abs(clamp(world_position.xz, min_wp, max_wp) - world_position.xz);
            let distance = max(outside_xy.x, outside_xy.y);
            if distance < nearest_region_distance {
                nearest_region_distance = distance;
                nearest_region_height = bounds.bounds[ix].height;
            }
            outside_amt = min(outside_amt, distance);
        }
        let outside_height = max(world_position.y - nearest_region_height, 0.0);
        outside_amt = max(outside_amt, outside_height);
    } else {
        outside_amt = 0.0;
    }

    var noise = 0.0;
    if outside_amt > 0.0 {
        if outside_amt < bounds.distance {
            noise = simplex_noise_3d(world_position * 2.0 + globals.time * vec3(0.2, 0.16, 0.24)) * 0.5 + 0.55;
            if noise < (outside_amt - 0.125) / 2.0 {
                discard;
            }
        } else if outside_amt > 0.05 {
            discard;
        }
    }

    prepass_alpha_discard(in);

    return out;
}
#else // !PREPASS_FRAGMENT (?)
@fragment
fn fragment(in: VertexOutput) {
    let world_position = in.world_position.xyz;
    var outside_amt: f32 = 9999.0;
    var nearest_region_distance: f32 = 9999.0;
    var nearest_region_height: f32 = 9999.0;
    if bounds.num_bounds > 0 {
        for (var ix = 0u; ix < bounds.num_bounds; ix += 1u) {
            let min_wp = unpack_bounds(bounds.bounds[ix].min);
            let max_wp = unpack_bounds(bounds.bounds[ix].max);

            let outside_xy = abs(clamp(world_position.xz, min_wp, max_wp) - world_position.xz);
            let distance = max(outside_xy.x, outside_xy.y);
            if distance < nearest_region_distance {
                nearest_region_distance = distance;
                nearest_region_height = bounds.bounds[ix].height;
            }
            outside_amt = min(outside_amt, distance);
        }
        let outside_height = max(world_position.y - nearest_region_height, 0.0);
        outside_amt = max(outside_amt, outside_height);
    } else {
        outside_amt = 0.0;
    }

    var noise = 0.0;
    if outside_amt > 0.0 {
        if outside_amt < bounds.distance {
            noise = simplex_noise_3d(world_position * 2.0 + globals.time * vec3(0.2, 0.16, 0.24)) * 0.5 + 0.55;
            if noise < (outside_amt - 0.125) / 2.0 {
                discard;
            }
        } else if outside_amt > 0.05 {
            discard;
        }
    }
    
    prepass_alpha_discard(in);
}
#endif // PREPASS_FRAGMENT
