// Sprite sheet flipbook: the frame is picked from the particle age, wrapping.
#import "embedded://particle_system/shaders/common.wgsl"::{Particle, RenderParams}

fn sprite_uv(uv: vec2<f32>, p: Particle, params: RenderParams) -> vec2<f32> {
    let tiles = max(params.sprite_tiles, vec2(1u, 1u));
    let frame = u32(max(p.age * params.frames_per_second, 0.0)) % (tiles.x * tiles.y);
    let tile = vec2<f32>(vec2(frame % tiles.x, frame / tiles.x));
    return (uv + tile) / vec2<f32>(tiles);
}
