// scene bounds check shared by the scene-bound materials. bindings sit at group 2, 100/101 in
// every material that imports this (layout from scene_material::SceneBoundData / SceneBounds).

struct SceneBoundData {
    // bounding box of the scene in parcel cells (world xz / 16)
    origin: vec2<i32>,
    size: vec2<u32>,
    // height limit when the scene is a single full rectangle
    rect_height: f32,
    flags: u32,
    // out-of-bounds fade width
    distance: f32,
}

// heights: data[0..height_count] (f32 bits)
// then one u16 per bounding-box cell, row-major, two per u32: 0 = not in the scene, else 1 + height index
struct SceneCells {
    height_count: u32,
    data: array<u32>,
}

const SCENE_BOUNDED: u32 = 1u;
const SCENE_FULL_RECT: u32 = 2u;

@group(2) @binding(100)
var<uniform> scene_bounds: SceneBoundData;
@group(2) @binding(101)
var<storage, read> scene_cells: SceneCells;

fn cell_region(cell: vec2<i32>) -> u32 {
    let local = cell - scene_bounds.origin;
    if any(local < vec2(0)) || any(vec2<u32>(local) >= scene_bounds.size) {
        return 0u;
    }
    let ix = u32(local.y) * scene_bounds.size.x + u32(local.x);
    let word = scene_cells.data[scene_cells.height_count + (ix >> 1u)];
    return (word >> ((ix & 1u) * 16u)) & 0xFFFFu;
}

fn region_height(region: u32) -> f32 {
    return bitcast<f32>(scene_cells.data[region - 1u]);
}

// distance outside the scene: the larger of the xz (chebyshev) distance to the nearest scene parcel
// and the height above that parcel's limit. 0 inside, 9999 when nowhere near.
fn scene_outside_amount(world_position: vec3<f32>) -> f32 {
    if (scene_bounds.flags & SCENE_BOUNDED) == 0u {
        return 0.0;
    }

    let p = world_position.xz;
    if (scene_bounds.flags & SCENE_FULL_RECT) != 0u {
        let min_wp = vec2<f32>(scene_bounds.origin) * 16.0;
        let max_wp = min_wp + vec2<f32>(scene_bounds.size) * 16.0;
        let outside_xy = abs(clamp(p, min_wp, max_wp) - p);
        return max(max(outside_xy.x, outside_xy.y), world_position.y - scene_bounds.rect_height);
    }

    let cell = vec2<i32>(floor(p / 16.0));
    let region = cell_region(cell);
    if region != 0u {
        return max(world_position.y - region_height(region), 0.0);
    }

    // outside the scene. the fade width is far below half a parcel, so only the neighbours across
    // the nearest x edge, z edge and corner can be within reach
    let local = p - vec2<f32>(cell) * 16.0;
    let upper = local >= vec2(8.0);
    let step = select(vec2(-1), vec2(1), upper);
    let edge = select(local, vec2(16.0) - local, upper);
    let reach = max(scene_bounds.distance, 0.05);

    var amount = 9999.0;
    if edge.x < reach {
        let r = cell_region(cell + vec2(step.x, 0));
        if r != 0u {
            amount = min(amount, max(edge.x, world_position.y - region_height(r)));
        }
    }
    if edge.y < reach {
        let r = cell_region(cell + vec2(0, step.y));
        if r != 0u {
            amount = min(amount, max(edge.y, world_position.y - region_height(r)));
        }
        if edge.x < reach {
            let r = cell_region(cell + step);
            if r != 0u {
                amount = min(amount, max(max(edge.x, edge.y), world_position.y - region_height(r)));
            }
        }
    }
    return amount;
}
