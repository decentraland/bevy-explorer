#import bevy_ui::ui_vertex_output::UiVertexOutput

struct SliceData {
    bounds: vec4<f32>,
    surface: vec4<f32>,
}

@group(1) @binding(0) var image_texture: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;
@group(1) @binding(2) var<uniform> slice_data: SliceData;
@group(1) @binding(3) var<uniform> bg_color: vec4<f32>;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    var uv = in.uv;
    let image_size = vec2<f32>(textureDimensions(image_texture));
    let border_size = vec2<f32>(slice_data.bounds.x + slice_data.bounds.z, slice_data.bounds.y + slice_data.bounds.z);
    let position = in.uv * slice_data.surface.xy;

    if slice_data.surface.x > image_size.x {
        let left = slice_data.bounds.x - position.x;
        let right = position.x - (slice_data.surface.x - slice_data.bounds.z);

        if left > 0.0 && left > right {
            uv.x = position.x / image_size.x;
        } else if right > 0.0 {
            uv.x = 1.0 - (slice_data.surface.x - position.x) / image_size.x;
        } else {
            uv.x = ((position.x - slice_data.bounds.x) / (slice_data.surface.x - border_size.x) * (image_size.x - border_size.x) + slice_data.bounds.x) / image_size.x;
        }
    }

    if slice_data.surface.y > image_size.y {
        let top = slice_data.bounds.y - position.y;
        let bottom = position.y - (slice_data.surface.y - slice_data.bounds.w);
        
        if top > 0.0 && top > bottom {
            uv.y = position.y / image_size.y;
        } else if bottom > 0.0 {
            uv.y = 1.0 - (slice_data.surface.y - position.y) / image_size.y;
        } else {
            uv.y = ((position.y - slice_data.bounds.y) / (slice_data.surface.y - border_size.y) * (image_size.y - border_size.y) + slice_data.bounds.y) / image_size.y;
        }
    }

    return textureSample(image_texture, image_sampler, uv) * bg_color;
}
