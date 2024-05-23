#import bevy_pbr::{
    pbr_types::STANDARD_MATERIAL_FLAGS_EMISSIVE_TEXTURE_BIT,
    mesh_view_bindings::view,
    prepass_utils::{prepass_depth, prepass_normal},
    pbr_types,
}

#ifdef DEPTH_PREPASS
fn get_depth(pos: vec4<f32>, si: u32) -> f32 {
    return view.projection[3][2] / prepass_depth(pos, si); 
}
#endif

fn apply_outline(position: vec4<f32>, color_in: vec4<f32>, hilight: bool, sample_index: u32) -> vec4<f32> {
    var out = color_in;
#ifdef DEPTH_PREPASS

    var edge1 = false;
    var edge2 = false;

    let mid = get_depth(position, sample_index);
    let width = (1.0 / mid) * view.viewport.z / 640.0;
    let d = clamp(width, 1.0, 5.0);

    let pxpy = get_depth(position + vec4<f32>(d, d, 0.0, 0.0), sample_index);
    let pxmy = get_depth(position + vec4<f32>(d, -d, 0.0, 0.0), sample_index);
    let mxpy = get_depth(position + vec4<f32>(-d, d, 0.0, 0.0), sample_index);
    let mxmy = get_depth(position + vec4<f32>(-d, -d, 0.0, 0.0), sample_index);

    let expected = (pxpy + pxmy + mxpy + mxmy) / 4.0;

    if /*(expected / depth - 1.0) > 0.005 ||*/ (expected / mid - 1) > 0.03 {
        edge1 = true;
    }

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
