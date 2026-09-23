struct Settings { source_mip: f32, radius: f32, flip_z: u32, padding: u32 }
@group(0) @binding(0) var source: texture_cube<f32>;
@group(0) @binding(1) var linear_sampler: sampler;
@group(0) @binding(2) var output: texture_storage_2d_array<rgba16float, write>;
@group(0) @binding(3) var<uniform> settings: Settings;

fn direction(face: u32, uv: vec2<f32>) -> vec3<f32> {
    let t = uv * 2.0 - 1.0;
    switch face {
        case 0u: { return normalize(vec3(1.0, -t.y, -t.x)); }
        case 1u: { return normalize(vec3(-1.0, -t.y, t.x)); }
        case 2u: { return normalize(vec3(t.x, 1.0, t.y)); }
        case 3u: { return normalize(vec3(t.x, -1.0, -t.y)); }
        case 4u: { return normalize(vec3(t.x, -t.y, 1.0)); }
        default: { return normalize(vec3(-t.x, -t.y, -1.0)); }
    }
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(output);
    if any(id.xy >= size) { return; }
    var n = direction(id.z, (vec2<f32>(id.xy) + 0.5) / vec2<f32>(size));
    if settings.flip_z != 0u { n.z = -n.z; }
    var color = vec3(0.0);
    if settings.radius == 0.0 {
        color = textureSampleLevel(source, linear_sampler, n, settings.source_mip).rgb;
    } else {
        let up = select(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), abs(n.y) < 0.99);
        let tangent = normalize(cross(up, n));
        let bitangent = cross(n, tangent);
        var weight_sum = 0.0;
        for (var y = -2; y <= 2; y += 1) {
            for (var x = -2; x <= 2; x += 1) {
                let offset = vec2<f32>(f32(x), f32(y)) * settings.radius;
                let weight = exp(-dot(offset, offset) / max(2.0 * settings.radius * settings.radius, 0.000001));
                let ray = normalize(n + tangent * offset.x + bitangent * offset.y);
                color += textureSampleLevel(source, linear_sampler, ray, settings.source_mip).rgb * weight;
                weight_sum += weight;
            }
        }
        color /= weight_sum;
    }
    textureStore(output, vec2<i32>(id.xy), i32(id.z), vec4(color, 1.0));
}
