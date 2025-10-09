#import bevy_ui::ui_vertex_output::UiVertexOutput
#import bevy_render::view::View

@group(0) @binding(0) var<uniform> view: View;

struct Bounds {
    bounds: vec4<f32>,
    border_color: vec4<f32>,
    edge_scale: vec4<f32>,
    corner_size: f32,
    corner_blend_size: f32,
    border_size: f32,
}

@group(1) @binding(0) var image_texture: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;
@group(1) @binding(2) var<uniform> bounds_data: Bounds;
@group(1) @binding(3) var<uniform> bg_color: vec4<f32>;

fn edge_color(uv: vec2<f32>, position: vec4<f32>, in_color: vec4<f32>) -> vec4<f32> {
    let corner_size = bounds_data.corner_size;
    let bounds = bounds_data.bounds;
    let edges = bounds_data.edge_scale;

    let left = max(0.0, (bounds.x + corner_size) - position.x) * edges.x;
    let right = max(0.0, position.x - (bounds.z - corner_size)) * edges.y;
    let top = max(0.0, (bounds.y + corner_size) - position.y) * edges.z;
    let bottom = max(0.0, position.y - (bounds.w - corner_size)) * edges.w;
    let corner_dist_sq = max(left * left, right * right) + max(top * top, bottom * bottom);
    let corner_end_size_sq = corner_size * corner_size;

    if corner_dist_sq > corner_end_size_sq {
        discard;
    }

    let edge_sq = max(corner_dist_sq, max(max(left * left, right * right), max(top * top, bottom * bottom)));
    var out_color = in_color;
    if bounds_data.border_size > 0.0 {
        let border_start_size_sq = (corner_size - bounds_data.border_size) * (corner_size - bounds_data.border_size);
        let border = 1.0 - clamp((edge_sq - border_start_size_sq) / (corner_end_size_sq - border_start_size_sq), 0.0, 1.0);
        out_color = mix(bounds_data.border_color, out_color, border);
    }

    if bounds_data.corner_blend_size > 0.0 {
        let corner_start_size_sq = (corner_size - bounds_data.corner_blend_size) * (corner_size - bounds_data.corner_blend_size);
        let alpha = 1.0 - clamp((edge_sq - corner_start_size_sq) / (corner_end_size_sq - corner_start_size_sq), 0.0, 1.0);
        out_color.a *= alpha;
    }

    return out_color;
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    var color = textureSample(image_texture, image_sampler, in.uv) * bg_color;
    return edge_color(in.uv, in.position, color);
}
