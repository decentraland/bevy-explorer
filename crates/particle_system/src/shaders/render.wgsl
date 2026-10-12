// Draws the live ring of the particle buffer as quads, oldest first. The mesh holds at
// least `alive` quads whose position.xy is the corner (-0.5..0.5) and position.z the quad
// index; the mesh tag carries `first_slot | alive << 16`. Quads past the ring and dead
// slots collapse to a clipped point.
#import bevy_pbr::mesh_functions::{get_world_from_local, get_tag}
#import bevy_pbr::mesh_view_bindings::view
#import "embedded://particle_system/shaders/common.wgsl"::{Particle, RenderParams, is_alive}
#import "embedded://particle_system/shaders/size.wgsl"::size_at
#import "embedded://particle_system/shaders/color.wgsl"::color_at
#import "embedded://particle_system/shaders/sprite_sheet.wgsl"::sprite_uv
#import "embedded://particle_system/shaders/billboard.wgsl"::quad_axes
#ifdef TRANSPARENT_FOCUS_OUTPUT
#import bevy_pbr::view_transformations::depth_ndc_to_view_z
#endif

@group(2) @binding(0) var<uniform> params: RenderParams;
@group(2) @binding(1) var<storage, read> particles: array<Particle>;
#ifdef TEXTURE
@group(2) @binding(2) var color_texture: texture_2d<f32>;
@group(2) @binding(3) var color_sampler: sampler;
#endif

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

// matches bevy_pbr::forward_io::FragmentOutput: the transparent pass carries a second
// target (alpha-weighted view depth + coverage for depth of field) that WebGPU requires
// every fragment shader to write
struct FragmentOutput {
    @location(0) color: vec4<f32>,
#ifdef TRANSPARENT_FOCUS_OUTPUT
    @location(1) focus: vec4<f32>,
#endif
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let corner = vertex.position.xy;
    let tag = get_tag(vertex.instance_index);
    let quad = u32(vertex.position.z + 0.5);
    let alive = tag >> 16u;
    let slot = ((tag & 0xffffu) + quad) % max(params.capacity, 1u);
    let p = particles[slot];
    if quad >= alive || !is_alive(p) {
        out.position = vec4(0.0, 0.0, 2.0, 1.0);
        return out;
    }

    let world_from_local = get_world_from_local(vertex.instance_index);
#ifdef WORLD_SPACE
    let center = p.position;
    let velocity = p.velocity;
#else
    let center = (world_from_local * vec4(p.position, 1.0)).xyz;
    let velocity = (world_from_local * vec4(p.velocity, 0.0)).xyz;
#endif
    let axes = quad_axes(p, velocity, world_from_local, view.world_from_view);
    let size = size_at(p, params);
    let world_position = center + (axes.right * corner.x + axes.up * corner.y) * size;
    out.position = view.clip_from_world * vec4(world_position, 1.0);

    var uv = vec2(corner.x + 0.5, 0.5 - corner.y);
#ifdef SPRITE_SHEET
    uv = sprite_uv(uv, p, params);
#endif
    out.uv = uv;
    out.color = color_at(p, params);
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    var color = in.color;
#ifdef TEXTURE
    color *= textureSample(color_texture, color_sampler, in.uv);
#endif
    var out: FragmentOutput;
#ifdef BLEND_PREMULTIPLIED_ALPHA
    // additive: premultiplied colour, no coverage
    out.color = vec4(color.rgb * color.a, 0.0);
#else ifdef BLEND_MULTIPLY
    // multiply blend state is dst * src + dst * (1 - src.a): premultiply so alpha weights it
    out.color = vec4(color.rgb * color.a, color.a);
#else
    out.color = color;
#endif
#ifdef TRANSPARENT_FOCUS_OUTPUT
    out.focus = vec4(-depth_ndc_to_view_z(in.position.z), 1.0, 0.0, color.a);
#endif
    return out;
}
