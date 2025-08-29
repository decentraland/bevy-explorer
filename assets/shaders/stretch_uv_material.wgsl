#import bevy_ui::ui_vertex_output::UiVertexOutput

@group(1) @binding(0) var image_texture: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;
@group(1) @binding(2) var<uniform> uvs: array<vec4<f32>,2>;
@group(1) @binding(3) var<uniform> color: vec4<f32>;

fn sd_rounded_box(point: vec2<f32>, size: vec2<f32>, corner_radii: vec4<f32>) -> f32 {
    // If 0.0 < y then select bottom left (w) and bottom right corner radius (z).
    // Else select top left (x) and top right corner radius (y).
    let rs = select(corner_radii.xy, corner_radii.wz, 0.0 < point.y);
    // w and z are swapped above so that both pairs are in left to right order, otherwise this second 
    // select statement would return the incorrect value for the bottom pair.
    let radius = select(rs.x, rs.y, 0.0 < point.x);
    // Vector from the corner closest to the point, to the point.
    let corner_to_point = abs(point) - 0.5 * size;
    // Vector from the center of the radius circle to the point.
    let q = corner_to_point + radius;
    // Length from center of the radius circle to the point, zeros a component if the point is not 
    // within the quadrant of the radius circle that is part of the curved corner.
    let l = length(max(q, vec2(0.0)));
    let m = min(max(q.x, q.y), 0.0);
    return l + m - radius;
}

fn sd_inset_rounded_box(point: vec2<f32>, size: vec2<f32>, radius: vec4<f32>, inset: vec4<f32>) -> f32 {
    let inner_size = size - inset.xy - inset.zw;
    let inner_center = inset.xy + 0.5 * inner_size - 0.5 * size;
    let inner_point = point - inner_center;

    var r = radius;

    // Top left corner.
    r.x = r.x - max(inset.x, inset.y);

    // Top right corner.
    r.y = r.y - max(inset.z, inset.y);

    // Bottom right corner.
    r.z = r.z - max(inset.z, inset.w); 

    // Bottom left corner.
    r.w = r.w - max(inset.x, inset.w);

    let half_size = inner_size * 0.5;
    let min_size = min(half_size.x, half_size.y);

    r = min(max(r, vec4(0.0)), vec4<f32>(min_size));

    return sd_rounded_box(inner_point, inner_size, r);
}

fn draw(in: UiVertexOutput, color: vec4<f32>) -> vec4<f32> {
    // Signed distances. The magnitude is the distance of the point from the edge of the shape.
    // * Negative values indicate that the point is inside the shape.
    // * Zero values indicate the point is on the edge of the shape.
    // * Positive values indicate the point is outside the shape.

    let point = (in.uv - vec2<f32>(0.5)) * 2.0;

    // Signed distance from the border's internal edge (the signed distance is negative if the point 
    // is inside the rect but not on the border).
    // If the border size is set to zero, this is the same as the external distance.
    let internal_distance = clamp(abs(sd_inset_rounded_box(point, in.size, in.border_radius, in.border_widths)) / 100.0, 0.0, 1.0);
    // let internal_distance = 1.0;

    // let t = step(0.0, -internal_distance);

    if all(in.border_radius == vec4<f32>(0.0)) {
        return vec4(color.rgb, saturate(color.a));
    } else {
        return vec4(internal_distance, internal_distance, internal_distance, 1.0); //saturate(color.a * t));
    }
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    // this is a bit lazy; really we should find a way to push the uvs directly into the vertex buffer
    let uv = mix(mix(uvs[0].xy, uvs[1].zw, in.uv.x), mix(uvs[0].zw, uvs[1].xy, in.uv.x), 1.0 - in.uv.y) * vec2<f32>(1.0, -1.0) + vec2<f32>(0.0, 1.0);

    let texture_color = textureSample(image_texture, image_sampler, uv) * color;
    return draw(in, texture_color);
    // return texture_color;
}
