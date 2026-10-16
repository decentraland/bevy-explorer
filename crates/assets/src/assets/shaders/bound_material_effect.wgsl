#import bevy_pbr::{
    pbr_types::STANDARD_MATERIAL_FLAGS_EMISSIVE_TEXTURE_BIT,
    mesh_view_bindings::{view, globals},
    prepass_utils::{prepass_depth, prepass_normal},
    pbr_types,
}

#ifdef DEPTH_PREPASS
fn get_depth(pos: vec4<f32>, si: u32) -> f32 {
    return view.clip_from_view[3][2] / prepass_depth(pos, si); 
}

// a neighbouring prepass pixel the dither discarded holds whatever lies behind the mesh, so it
// would read as an edge. the prepass ran the same dither on it: the same noise at its pixel, and
// a coverage that barely changes over a few pixels. predict the discard from this fragment's
// opacity and the neighbour's threshold, and use this fragment's value in its place
fn neighbour_dithered(position: vec4<f32>, offset: vec2<f32>, dither_opacity: f32) -> bool {
    // the prepass hashed the neighbour pixel's centre
    return dither_opacity < 1.0 && dither_opacity < dither_threshold(floor(position.xy + offset) + 0.5);
}

fn neighbour_depth(position: vec4<f32>, offset: vec2<f32>, fallback: f32, dither_opacity: f32, si: u32) -> f32 {
    if neighbour_dithered(position, offset, dither_opacity) {
        return fallback;
    }
    return get_depth(position + vec4(offset, 0.0, 0.0), si);
}

#ifdef NORMAL_PREPASS
// a neighbour on another surface (seen through the dither holes, or in front of this fragment)
// can't form a crease with it: the silhouette between them is the nearer surface's depth edge
fn neighbour_normal(position: vec4<f32>, offset: vec2<f32>, fallback: vec3<f32>, center_depth: f32, dither_opacity: f32, si: u32) -> vec3<f32> {
    if neighbour_dithered(position, offset, dither_opacity) {
        return fallback;
    }
    let p = position + vec4(offset, 0.0, 0.0);
    if abs(get_depth(p, si) / center_depth - 1.0) > 0.03 {
        return fallback;
    }
    return prepass_normal(p, si);
}
#endif
#endif

// dither_opacity: this fragment's discard_dither opacity; 1 when it isn't dithered
fn apply_outline(position: vec4<f32>, color_in: vec4<f32>, outline_color: vec3<f32>, hilight: bool, sample_index: u32, dither_opacity: f32) -> vec4<f32> {
    var out = color_in;
#ifdef DEPTH_PREPASS

    var edge1 = false;
    var edge2 = false;

    let is_orthographic = view.clip_from_view[3].w == 1.0;
    let center = get_depth(position, sample_index);
    var mid = center;
    if is_orthographic {
        mid = 2.0 / view.clip_from_view[1].y;
    }
    let width = (1.0 / mid) * view.viewport.z / 640.0;
    let d = clamp(width, 1.0, 5.0);

    let pxpy = neighbour_depth(position, vec2(d, d), center, dither_opacity, sample_index);
    let pxmy = neighbour_depth(position, vec2(d, -d), center, dither_opacity, sample_index);
    let mxpy = neighbour_depth(position, vec2(-d, d), center, dither_opacity, sample_index);
    let mxmy = neighbour_depth(position, vec2(-d, -d), center, dither_opacity, sample_index);

    let expected = (pxpy + pxmy + mxpy + mxmy) / 4.0;

    if (expected / mid - 1) > 0.03 {
        edge1 = true;
    }

#ifdef NORMAL_PREPASS
    let nd = 1.0;
    let nmid = prepass_normal(position, sample_index);
    let npxpy = neighbour_normal(position, vec2(nd, nd), nmid, center, dither_opacity, sample_index);
    let npxmy = neighbour_normal(position, vec2(nd, -nd), nmid, center, dither_opacity, sample_index);
    let nmxpy = neighbour_normal(position, vec2(-nd, nd), nmid, center, dither_opacity, sample_index);
    let nmxmy = neighbour_normal(position, vec2(-nd, -nd), nmid, center, dither_opacity, sample_index);

    let nexpected = (npxpy + nmxmy) / 2.0;
    let nexpected2 = (npxmy + nmxpy) / 2.0;
    if length(nmid - nexpected) + length(nmid - nexpected2) > 0.4 {
        edge2 = true;
    }
#endif

    var hi = 0.0;
    if hilight {
        hi = 1.0;
    }
    if edge1 {
        out = vec4<f32>(outline_color * 10. * hi, out.a);
    } else if edge2 {
        out = vec4<f32>(out.rgb * (outline_color * 4.5 * hi + 0.5), out.a);
    }
#endif

return out;
}

struct Dither {
    // what the pixel's threshold is tested against; 1 or more never discards
    opacity: f32,
    // how faded the fragment is; used to cap brightness so bloom doesn't fill the discarded regions
    transparency: f32,
}

// "interleaved gradient noise" magic numbers
fn dither_threshold(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(pixel * (1.0 * 5.588238), vec2(0.06711056, 0.00583715))));
}

// avatars fade as a whole rather than per fragment: the dither is taken at the point of their
// (fixed-size) capsule nearest the ray, so every fragment lands on the same opacity
const AVATAR_CAPSULE_RADIUS: f32 = 0.35;
const AVATAR_CAPSULE_HEIGHT: f32 = 2.0;

// the point of the segment a-b nearest the line through o along (unit) d
fn closest_point_on_segment_to_line(a: vec3<f32>, b: vec3<f32>, o: vec3<f32>, d: vec3<f32>) -> vec3<f32> {
    let ab = b - a;
    let ao = a - o;
    let abab = dot(ab, ab);
    let abd = dot(ab, d);
    let denom = abab - abd * abd;
    var t = 0.0;
    if denom > 1e-6 {
        t = clamp((abd * dot(d, ao) - dot(ab, ao)) / denom, 0.0, 1.0);
    }
    return a + ab * t;
}

// discards fragments between the camera and the player
// base_opacity: the opacity the dither fades towards; 0 fades out fully
// capsule: dither by the avatar capsule standing at capsule_root instead of by the fragment
fn discard_dither(ndc_position: vec2<f32>, world_position: vec3<f32>, depth: f32, distance_dither: bool, base_opacity: f32, capsule_root: vec3<f32>, capsule: bool) -> Dither {
    // player is left of the view forward by 0.25 * clamp(camera distance, 0, 3). we use half of that as our target
    let view_fwd = -view.world_from_view[2].xyz;
    let view_right = view.world_from_view[0].xyz;
    let view_up = view.world_from_view[1].xyz;
    let target_offset = depth * view_fwd - 0.125 * clamp(depth, 0.0, 3.0) * view_right; 

    let view_direction = normalize(target_offset);

    var position = world_position;
    var radius = 0.0;
    if capsule {
        position = closest_point_on_segment_to_line(
            capsule_root + vec3(0.0, AVATAR_CAPSULE_RADIUS, 0.0),
            capsule_root + vec3(0.0, AVATAR_CAPSULE_HEIGHT - AVATAR_CAPSULE_RADIUS, 0.0),
            view.world_position,
            view_direction,
        );
        radius = AVATAR_CAPSULE_RADIUS;
    }

    let view_to_frag = position - view.world_position;
    let projection_length = dot(view_to_frag, view_direction);

    if projection_length < depth + 0.35 { // 0.35 = collider radius
        let cone_distance = 
            // distance from ray to target
            max(0.0, length(position - (view.world_position.xyz + (view_direction * projection_length))) - radius)
            // shrink to cone, 0 radius at view position, 1 at target position
            / (projection_length / depth) 
            // expand cone at further camera distances
            / (1.0 + max(0.0, (projection_length - depth)) / 10.0);

        var use_distance = cone_distance;
        // add transparency to fullscreen for near fragments 
        if distance_dither {
            let full_transparent_start_distance = depth * 0.0;
            let full_transparent_end_distance = depth * 0.75;

            let full_transparent_factor = saturate((projection_length - full_transparent_start_distance) / (full_transparent_end_distance - full_transparent_start_distance));

            if cone_distance < 1.0 {
                use_distance = mix(
                    min(cone_distance, full_transparent_factor * 0.75), 
                    cone_distance,
                    full_transparent_factor,
                );
            } else {
                use_distance = mix(
                    full_transparent_factor * 0.75,
                    min(pow(cone_distance, 0.5), 2.0),
                    full_transparent_factor,
                );
            }
        }

        let opacity = use_distance 
            // add opacity
            + 0.5 
            // remove further from target
            - saturate((depth - projection_length) * 0.5) * 1.0 
            // add behind target
            + saturate(projection_length - depth) * 5.0;

        let result = Dither(max(0.1, base_opacity + opacity * (1.0 - base_opacity)), 1.0 - saturate(use_distance));
        if result.opacity < dither_threshold(ndc_position) {
            discard;
        }
        return result;
    }

    return Dither(1.0, 0.0);
}