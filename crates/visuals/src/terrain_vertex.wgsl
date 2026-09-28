// Displaces ground and grass meshes by the empty-parcel terrain height. The height function
// matches `common::terrain::height`, reading eased per-parcel step values from `terrain_steps`.

#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip,
}

#ifdef PREPASS_PIPELINE
    #import bevy_pbr::prepass_io::{Vertex, VertexOutput};
#else
    #import bevy_pbr::forward_io::{Vertex, VertexOutput};
#endif

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
}

@group(2) @binding(100) var terrain_steps: texture_2d<f32>;
@group(2) @binding(101) var<uniform> terrain: TerrainParams;

const STEP_HEIGHT: f32 = 1.8;
const TEXEL_STEP: f32 = 10.0;
const NORMAL_EPSILON: f32 = 0.5;
// parcels around the player drawn at parcel grass resolution (terrain.rs GROUND_NEAR_MIN/MAX)
const GROUND_NEAR_MIN: i32 = -13;
const GROUND_NEAR_MAX: i32 = 12;
// cell size of the ground ring beyond them (terrain.rs RING_CELL)
const RING_CELL: f32 = 16.0;
// Beyond the rings at RING_CELL (terrain.rs TERRAIN_RINGS, each doubling the extent) the ground is
// flat: coarser cells slide over the terrain as the mesh follows the player, so can't follow it
// without swimming.
const FLAT_REACH: f32 = f32(-GROUND_NEAR_MIN) * 16.0 * 4.0;

fn modulo289(value: f32) -> f32 {
    return value - floor(value * (1.0 / 289.0)) * 289.0;
}

fn permute(value: f32) -> f32 {
    return modulo289((34.0 * value + 1.0) * value);
}

fn simplex_corner(i: vec2<f32>, offset: vec2<f32>, corner: vec2<f32>) -> f32 {
    let p = permute(permute(i.y + offset.y) + i.x + offset.x);
    var m = max(0.5 - dot(corner, corner), 0.0);
    m *= m;
    m *= m;
    let fractional = p * 0.024390243 - floor(p * 0.024390243);
    let x = 2.0 * fractional - 1.0;
    let h = abs(x) - 0.5;
    let a = x - floor(x + 0.5);
    m *= 1.7928429 - 0.85373473 * (a * a + h * h);
    return m * (a * corner.x + h * corner.y);
}

fn simplex(v: vec2<f32>) -> f32 {
    let c = vec4<f32>(0.21132487, 0.36602542, -0.57735026, 0.024390243);
    let i = floor(v + dot(v, c.yy));
    let x0 = v - i + dot(i, c.xx);
    var i1 = vec2<f32>(0.0, 1.0);
    if x0.x > x0.y {
        i1 = vec2<f32>(1.0, 0.0);
    }
    let x1 = x0 + c.xx - i1;
    let x2 = x0 + c.zz;
    let im = vec2<f32>(modulo289(i.x), modulo289(i.y));
    return 130.0 * (
        simplex_corner(im, vec2<f32>(0.0), x0)
        + simplex_corner(im, i1, x1)
        + simplex_corner(im, vec2<f32>(1.0), x2)
    );
}

fn mountain_noise(p: vec2<f32>) -> f32 {
    let offsets = array<vec2<f32>, 3>(
        vec2<f32>(-99974.82, -93748.33),
        vec2<f32>(-67502.3, -22190.19),
        vec2<f32>(77881.34, -61863.88),
    );
    var amplitude = 1.0;
    var frequency = 1.0;
    var height = 0.0;
    for (var octave = 0; octave < 3; octave++) {
        height += simplex((p + offsets[octave]) * 0.02 * frequency) * amplitude;
        amplitude *= 0.338;
        frequency *= 2.9;
    }
    return height * 3.0;
}

fn terrain_texel(parcel: vec2<i32>) -> f32 {
    let local = parcel - terrain.min;
    if any(local < vec2<i32>(0)) || any(local >= terrain.size) {
        return 0.0;
    }
    let steps = textureLoad(terrain_steps, local, 0).r;
    if steps < 0.0 {
        return 0.0;
    }
    return 255.0 - TEXEL_STEP * terrain.max_steps + TEXEL_STEP * steps;
}

// x and unity z (= -bevy z)
fn terrain_height_unity(p: vec2<f32>) -> f32 {
    if terrain.max_steps <= 0.0 {
        return 0.0;
    }
    let uv_size = terrain.uv_size;
    let half = i32(uv_size * 0.5);
    let last = vec2<i32>(i32(uv_size) - 1);
    let pixel = ((p / 16.0 + uv_size * 0.5) / uv_size) * uv_size - 0.5;
    // unity's texture clamps to its edge
    let base0 = clamp(vec2<i32>(floor(pixel)), vec2<i32>(0), last) - half;
    let base1 = clamp(vec2<i32>(floor(pixel)) + 1, vec2<i32>(0), last) - half;
    let fraction = pixel - floor(pixel);
    let top = mix(
        terrain_texel(base0),
        terrain_texel(vec2<i32>(base1.x, base0.y)),
        fraction.x,
    );
    let bottom = mix(
        terrain_texel(vec2<i32>(base0.x, base1.y)),
        terrain_texel(base1),
        fraction.x,
    );
    let occupancy = mix(top, bottom, fraction.y) * (1.0 / 255.0);
    let floor_value = (255.0 - TEXEL_STEP * terrain.max_steps) / 255.0;
    if occupancy <= floor_value {
        return 0.0;
    }
    let normalized = (occupancy - floor_value) / (1.0 - floor_value);
    return max(
        normalized * (terrain.max_steps * STEP_HEIGHT)
            + mountain_noise(p) * clamp(normalized * 20.0, 0.0, 1.0),
        0.0,
    );
}

// bevy world x/z
fn terrain_height(xz: vec2<f32>) -> f32 {
    return terrain_height_unity(vec2<f32>(xz.x, -xz.y));
}

// Detail morph: nearer than this to a border (in parcels) whose crossing gives a parcel a coarser
// grid, the parcel's vertices blend onto that grid, reaching it at the crossing.
const MORPH_BAND: f32 = 0.5;

// cell size of a parcel's ground and grass grid: ParcelGrassLod::from_distance for 16 / lod cells
fn lod_cell(parcel: vec2<i32>) -> f32 {
    return lod_cell_from(parcel, terrain.player_parcel);
}

// lod_cell for the player at `player`
fn lod_cell_from(parcel: vec2<i32>, player: vec2<i32>) -> f32 {
    let offset = parcel - player;
    if any(offset < vec2<i32>(GROUND_NEAR_MIN)) || any(offset > vec2<i32>(GROUND_NEAR_MAX)) {
        return RING_CELL;
    }
    let distance = dot(offset, offset);
    if distance < 8 {
        return 1.0;
    }
    if distance < 16 {
        return 2.0;
    }
    return 4.0;
}

// height on the edge of a `cell`-sized grid along one axis (unity x/z), anchored at `anchor`
fn edge_height(p: vec2<f32>, along_x: bool, cell: f32, anchor: vec2<f32>) -> f32 {
    let axis = select(p.y - anchor.y, p.x - anchor.x, along_x);
    let low = floor(axis / cell) * cell;
    let t = (axis - low) / cell;
    var a = p;
    var b = p;
    if along_x {
        a.x = anchor.x + low;
        b.x = anchor.x + low + cell;
    } else {
        a.y = anchor.y + low;
        b.y = anchor.y + low + cell;
    }
    return mix(terrain_height_unity(a), terrain_height_unity(b), t);
}

// Height of a ground or grass vertex (unity x/z). Vertices on an edge shared with a coarser grid
// sit on that grid's edge so neither leaves a crack: `stitch` gives the coarser cell along an axis
// for ring edges (anchored at the mesh origin); parcel edges use the parcel grass lod.
fn vertex_height(p: vec2<f32>, stitch: vec2<f32>, anchor: vec2<f32>) -> f32 {
    if stitch.x > 0.0 {
        return edge_height(p, true, stitch.x, anchor);
    }
    if stitch.y > 0.0 {
        return edge_height(p, false, stitch.y, anchor);
    }
    return morphed_height(p);
}

// Height of a parcel grid vertex (unity x/z) with the grids around the player at `player`
fn parcel_grid_height(p: vec2<f32>, player: vec2<i32>) -> f32 {
    let on_x_edge = fract(p.x / 16.0) == 0.0;
    let on_z_edge = fract(p.y / 16.0) == 0.0;
    if on_x_edge && !on_z_edge {
        let column = i32(p.x / 16.0);
        let row = i32(floor(p.y / 16.0));
        let cell = max(
            lod_cell_from(vec2<i32>(column - 1, row), player),
            lod_cell_from(vec2<i32>(column, row), player),
        );
        return edge_height(p, false, cell, vec2<f32>(0.0));
    }
    if on_z_edge && !on_x_edge {
        let column = i32(floor(p.x / 16.0));
        let row = i32(p.y / 16.0);
        let cell = max(
            lod_cell_from(vec2<i32>(column, row - 1), player),
            lod_cell_from(vec2<i32>(column, row), player),
        );
        return edge_height(p, true, cell, vec2<f32>(0.0));
    }
    return terrain_height_unity(p);
}

// Surface of a `cell` grid (unity x/z) with the grids around the player at `player`: each cell is
// split along unity's (x0, z0)-(x1, z1) diagonal, as the meshes are.
fn grid_surface(p: vec2<f32>, cell: f32, player: vec2<i32>) -> f32 {
    let base = floor(p / cell) * cell;
    let uv = (p - base) / cell;
    let h00 = parcel_grid_height(base, player);
    let h11 = parcel_grid_height(base + cell, player);
    if uv.x >= uv.y {
        let h10 = parcel_grid_height(base + vec2<f32>(cell, 0.0), player);
        return h00 + uv.x * (h10 - h00) + uv.y * (h11 - h10);
    }
    let h01 = parcel_grid_height(base + vec2<f32>(0.0, cell), player);
    return h00 + uv.y * (h01 - h00) + uv.x * (h11 - h01);
}

fn nearness(toward: f32) -> f32 {
    return smoothstep(1.0 - MORPH_BAND, 1.0, toward);
}

// How far a parcel has morphed toward the grid it gets when the player crosses into a neighbouring
// parcel, and that parcel: the crossing nearest to happening among those that coarsen its grid.
struct Morph {
    weight: f32,
    player: vec2<i32>,
}

fn parcel_morph(parcel: vec2<i32>) -> Morph {
    let cell = lod_cell(parcel);
    let offset = terrain.player_offset;
    var morph = Morph(0.0, terrain.player_parcel);
    var morph_cell = cell;
    for (var dz = -1; dz <= 1; dz++) {
        for (var dx = -1; dx <= 1; dx++) {
            let player = terrain.player_parcel + vec2<i32>(dx, dz);
            let next = lod_cell_from(parcel, player);
            if next <= cell {
                continue;
            }
            var weight = 1.0;
            if dx != 0 {
                weight = min(weight, nearness(select(1.0 - offset.x, offset.x, dx > 0)));
            }
            if dz != 0 {
                weight = min(weight, nearness(select(1.0 - offset.y, offset.y, dz > 0)));
            }
            if weight > morph.weight || (weight > 0.0 && weight == morph.weight && next > morph_cell) {
                morph = Morph(weight, player);
                morph_cell = next;
            }
        }
    }
    return morph;
}

// Parcel grid height (unity x/z), morphed toward the grid the next crossing brings. A vertex on a
// parcel edge takes the stronger morph of the parcels either side, so both agree; parcel corners
// are on every grid.
fn morphed_height(p: vec2<f32>) -> f32 {
    let height = parcel_grid_height(p, terrain.player_parcel);
    let on_x_edge = fract(p.x / 16.0) == 0.0;
    let on_z_edge = fract(p.y / 16.0) == 0.0;
    if on_x_edge && on_z_edge {
        return height;
    }
    let parcel = vec2<i32>(floor(p / 16.0));
    var other = parcel;
    if on_x_edge {
        other.x -= 1;
    } else if on_z_edge {
        other.y -= 1;
    }
    var morph = parcel_morph(parcel);
    if any(other != parcel) {
        let other_morph = parcel_morph(other);
        if other_morph.weight > morph.weight {
            morph = other_morph;
        }
    }
    if morph.weight <= 0.0 {
        return height;
    }
    let cell = max(lod_cell_from(parcel, morph.player), lod_cell_from(other, morph.player));
    return mix(height, grid_surface(p, cell, morph.player), morph.weight);
}

fn terrain_normal(xz: vec2<f32>) -> vec3<f32> {
    let dx = vec2<f32>(NORMAL_EPSILON, 0.0);
    let dz = vec2<f32>(0.0, NORMAL_EPSILON);
    return normalize(vec3<f32>(
        terrain_height(xz - dx) - terrain_height(xz + dx),
        2.0 * NORMAL_EPSILON,
        terrain_height(xz - dz) - terrain_height(xz + dz),
    ));
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    var world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(vertex.position, 1.0),
    );

    // uv holds the coarser cell size met by vertices on a ground ring's edge
    var stitch = vec2<f32>(0.0);
#ifdef VERTEX_UVS_A
    stitch = vertex.uv;
    out.uv = vec2<f32>(0.0);
#endif
    let xz = world_position.xz;
    let anchor = world_from_local[3].xz;
    let outer = max(abs(xz.x - anchor.x), abs(xz.y - anchor.y)) >= FLAT_REACH;
    if !outer {
        world_position.y += vertex_height(
            vec2<f32>(xz.x, -xz.y),
            stitch,
            vec2<f32>(anchor.x, -anchor.y),
        );
    }

    out.world_position = world_position;
    out.position = position_world_to_clip(world_position.xyz);

#ifdef PREPASS_PIPELINE
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.unclipped_depth = out.position.z;
    out.position.z = min(out.position.z, 1.0);
#endif
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
    out.world_normal = select(terrain_normal(xz), vec3<f32>(0.0, 1.0, 0.0), outer);
#endif
#ifdef MOTION_VECTOR_PREPASS
    out.previous_world_position = world_position;
#endif
#else
    out.world_normal = select(terrain_normal(xz), vec3<f32>(0.0, 1.0, 0.0), outer);
#endif

#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif

#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex.instance_index,
        world_from_local[3],
    );
#endif

    return out;
}
