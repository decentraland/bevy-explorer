// signed-distance text (scene_runner text shapes): the base colour texture is an atlas of
// glyph distance fields (0.5 at the ink edge, 0/1 at `spread` pixels outside/inside), and the
// base colour is derived from it here, in place of the standard material's texture sample.

#import bevy_pbr::{
    pbr_bindings::{material, base_color_texture, base_color_sampler},
    mesh_view_bindings::view,
}

struct TextSdfParams {
    outline_color: vec4<f32>,
    shadow_color: vec4<f32>,
    // atlas pixels, centred on the ink edge
    outline_width: f32,
    // atlas pixels added to the shadow's edge transition
    shadow_softness: f32,
    // atlas pixels of distance range either side of the edge
    spread: f32,
    flags: u32,
}

// which layers to draw (each effect is normally a mesh of its own, drawn before the face; the
// shadow's quads are offset by the shape, so it samples the same glyph as the face)
const TEXT_SDF_FACE: u32 = 1u;
const TEXT_SDF_OUTLINE: u32 = 2u;
const TEXT_SDF_SHADOW: u32 = 4u;
// the page holds bitmaps (colour glyphs), not distance fields
const TEXT_SDF_BITMAP: u32 = 8u;

@group(2) @binding(200)
var<uniform> text_sdf: TextSdfParams;

// signed distance in atlas pixels, positive inside the ink. explicit gradients: samples are
// taken inside non-uniform branches
fn sdf_distance(uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>) -> f32 {
    let sample = textureSampleGrad(base_color_texture, base_color_sampler, uv, dx, dy).r;
    return (sample - 0.5) * 2.0 * text_sdf.spread;
}

// coverage of a distance, anti-aliased with a linear ramp `width` atlas pixels wide
fn coverage(distance: f32, width: f32) -> f32 {
    return saturate(distance / width + 0.5);
}

// anti-aliased coverage of the ink dilated by `dilate` atlas pixels, with the edge softened
// by a further `soft`. `px` is atlas pixels per screen pixel: the ramp is one screen pixel,
// and minified glyphs take four rotated-grid taps so thin strokes don't sparkle or vanish.
fn layer_alpha(uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, px: f32, dilate: f32, soft: f32) -> f32 {
    let width = max(px, 1e-4) + soft;
    if px > 1.0 {
        let a = coverage(sdf_distance(uv + 0.125 * dx + 0.375 * dy, dx, dy) + dilate, width)
            + coverage(sdf_distance(uv - 0.375 * dx + 0.125 * dy, dx, dy) + dilate, width)
            + coverage(sdf_distance(uv + 0.375 * dx - 0.125 * dy, dx, dy) + dilate, width)
            + coverage(sdf_distance(uv - 0.125 * dx - 0.375 * dy, dx, dy) + dilate, width);
        return a * 0.25;
    }
    return coverage(sdf_distance(uv, dx, dy) + dilate, width);
}

// `over` composite of straight-alpha colours
fn over(top: vec4<f32>, bottom: vec4<f32>) -> vec4<f32> {
    let alpha = top.a + bottom.a * (1.0 - top.a);
    let rgb = (top.rgb * top.a + bottom.rgb * bottom.a * (1.0 - top.a)) / max(alpha, 1e-4);
    return vec4(rgb, alpha);
}

fn text_sdf_base_color(raw_uv: vec2<f32>, vertex_color: vec4<f32>) -> vec4<f32> {
    let uv = (material.uv_transform * vec3(raw_uv, 1.0)).xy;
    let dims = vec2<f32>(textureDimensions(base_color_texture));
    // derivatives first, in uniform control flow
    let dx = dpdx(uv);
    let dy = dpdy(uv);
    // atlas pixels per screen pixel, along the denser axis
    let px = max(length(dx * dims), length(dy * dims));

    if (text_sdf.flags & TEXT_SDF_BITMAP) != 0u {
        let bitmap = textureSampleGrad(base_color_texture, base_color_sampler, uv, dx, dy);
        return material.base_color * vertex_color * bitmap;
    }

    let half_outline = text_sdf.outline_width * 0.5;
    var color = vec4(0.0);

    if (text_sdf.flags & TEXT_SDF_FACE) != 0u {
        let face_alpha = layer_alpha(uv, dx, dy, px, -half_outline, 0.0);
        let face = material.base_color * vertex_color;
        color = vec4(face.rgb, face.a * face_alpha);
    }

    if (text_sdf.flags & TEXT_SDF_OUTLINE) != 0u {
        let outline_alpha = layer_alpha(uv, dx, dy, px, half_outline, 0.0);
        let outline = text_sdf.outline_color * vec4(1.0, 1.0, 1.0, outline_alpha);
        color = over(color, outline);
    }

    if (text_sdf.flags & TEXT_SDF_SHADOW) != 0u {
        let shadow_alpha = layer_alpha(uv, dx, dy, px, 0.0, text_sdf.shadow_softness);
        let shadow = text_sdf.shadow_color * vec4(1.0, 1.0, 1.0, shadow_alpha);
        color = over(color, shadow);
    }

    return color;
}
