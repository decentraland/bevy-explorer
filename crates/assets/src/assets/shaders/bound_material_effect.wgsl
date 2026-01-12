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
#endif

fn apply_outline(position: vec4<f32>, color_in: vec4<f32>, hilight: bool, sample_index: u32) -> vec4<f32> {
    var out = color_in;
#ifdef DEPTH_PREPASS

    var edge1 = false;
    var edge2 = false;

    let is_orthographic = view.clip_from_view[3].w == 1.0;
    var mid = 0.0;
    if is_orthographic {
        mid = 2.0 / view.clip_from_view[1].y;
    } else {
        mid = get_depth(position, sample_index);
    }
    let width = (1.0 / mid) * view.viewport.z / 640.0;
    let d = clamp(width, 1.0, 5.0);

    let pxpy = get_depth(position + vec4<f32>(d, d, 0.0, 0.0), sample_index);
    let pxmy = get_depth(position + vec4<f32>(d, -d, 0.0, 0.0), sample_index);
    let mxpy = get_depth(position + vec4<f32>(-d, d, 0.0, 0.0), sample_index);
    let mxmy = get_depth(position + vec4<f32>(-d, -d, 0.0, 0.0), sample_index);

    let expected = (pxpy + pxmy + mxpy + mxmy) / 4.0;

    if (expected / mid - 1) > 0.03 {
        edge1 = true;
    }

#ifdef NORMAL_PREPASS
    let nd = 1.0;
    let nmid = prepass_normal(position, sample_index);
    let npxpy = prepass_normal(position + vec4<f32>(nd, nd, 0.0, 0.0), sample_index);
    let npxmy = prepass_normal(position + vec4<f32>(nd, -nd, 0.0, 0.0), sample_index);
    let nmxpy = prepass_normal(position + vec4<f32>(-nd, nd, 0.0, 0.0), sample_index);
    let nmxmy = prepass_normal(position + vec4<f32>(-nd, -nd, 0.0, 0.0), sample_index);        

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
        out = vec4<f32>(10.0 * hi, 0.0, 0.0, out.a);
    } else if edge2 {
        out = vec4<f32>(out.rgb * vec3<f32>(4.5 * hi + 0.5, 0.5, 0.5), out.a);
    }
#endif

return out;
}

fn discard_dither(ndc_position: vec2<f32>, world_position: vec3<f32>, depth: f32, distance_dither: bool) -> f32 {
    let view_to_frag = world_position - view.world_position;
    
    // player is left of the view forward by 0.25 * clamp(camera distance, 0, 3). we use half of that as our target
    // we also lower by 0.5m world, as target is at head height
    let view_fwd = -view.world_from_view[2].xyz;
    let view_right = view.world_from_view[0].xyz;
    let view_up = view.world_from_view[1].xyz;
    let target_offset = depth * view_fwd - 0.125 * clamp(depth, 0.0, 3.0) * view_right; 

    let view_direction = normalize(target_offset);
    let projection_length = dot(view_to_frag, view_direction);

    if projection_length < depth + 0.35 { // 0.35 = collider radius
        let cone_distance = length(world_position - (view.world_position.xyz + (view_direction * projection_length))) / (projection_length / depth) / (1.0 + max(0.0, (projection_length - depth)) / 10.0);
        let threshold = fract(52.9829189 * fract(dot(ndc_position * (1.0 * 5.588238), vec2(0.06711056, 0.00583715))));

        var use_distance = cone_distance;
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

        if  max(
               0.1,
               use_distance 
                 + 0.5 
                 - saturate((depth - projection_length) * 0.5) * 1.0 
                 + saturate(projection_length - depth) * 5.0
            ) < threshold
        {
            discard;
        }
        return 1.0 - saturate(use_distance);
    }

    return 0.0;
}