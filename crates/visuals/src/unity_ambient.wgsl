#import bevy_pbr::mesh_view_bindings as bindings

struct UnityAmbient {
    constant: vec4<f32>,
    linear: vec4<f32>,
    quadratic: vec4<f32>,
}

@group(2) @binding(101) var<storage, read> unity_ambient: UnityAmbient;

fn unity_ambient_irradiance(normal: vec3<f32>) -> vec3<f32> {
    if unity_ambient.constant.w == 0.0 {
        return bindings::lights.ambient_color.rgb;
    }
    let color = unity_ambient.constant.rgb + unity_ambient.linear.rgb * normal.y
        + unity_ambient.quadratic.rgb * (normal.y * normal.y + (1.0 - dot(normal, normal)) * 0.5);
    return max(color, vec3(0.0)) / max(bindings::view.exposure, 0.00000001);
}
