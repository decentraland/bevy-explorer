// Terrain parameters shared by the terrain vertex shader and the ground and grass fragment shaders
// (`TerrainParams` in terrain.rs).

struct TerrainParams {
    // first parcel of the step texture (x, unity z)
    min: vec2<i32>,
    size: vec2<i32>,
    max_steps: f32,
    // unity's occupancy texture size, only used to reproduce its uv arithmetic
    uv_size: f32,
    // ground and grass resolution follows parcel grass lod around this parcel (x, unity z)
    player_parcel: vec2<i32>,
    // the player's position within that parcel, 0 to 1 (x, unity z)
    player_offset: vec2<f32>,
    // coast bounds in unity x/z metres (min x, min z, max x, max z); the ground ends at the cliffs
    coast: vec4<f32>,
}

@group(2) @binding(101) var<uniform> terrain: TerrainParams;
