//! Empty-parcel terrain, matching unity-explorer's `TerrainGenerator` / `Landscape`.
//!
//! Heights come from a per-parcel chamfer distance to the nearest occupied parcel. X/Z are Unity
//! coordinates here (negate Bevy Z). Parcels whose occupancy is not yet known count as occupied,
//! so heights only ever rise as the realm resolves.

use bevy::{ecs::resource::Resource, math::IVec2};

pub const STEP_HEIGHT: f32 = 1.8;
/// Unity's step count is the largest distance in the realm. It is capped here so that a parcel's
/// value only depends on its local neighbourhood (see `RESOLVE_RADIUS`).
pub const MAX_STEPS: u8 = 8;
/// A capped step value depends only on occupancy within this many parcels.
pub const RESOLVE_RADIUS: i32 = MAX_STEPS as i32 + 1;
/// Step value of occupied, unknown and out-of-bounds parcels.
pub const ZERO: i8 = -1;
/// Unity's per-step increment of the occupancy texture when there are at most 25 steps.
pub const TEXEL_STEP: f32 = 10.0;

const BORDER_PADDING: i32 = 2;
const MAX_TEXELS: i64 = 4096 * 4096;
const FAR: u32 = 1_000_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Occupancy {
    Empty,
    Occupied,
    Unknown,
}

/// Occupancy texture value (Unity's 0-255 scale) of a texel with the given step value.
pub fn texel_value(steps: f32, max_steps: f32) -> f32 {
    if steps < 0.0 {
        0.0
    } else {
        255.0 - TEXEL_STEP * max_steps + TEXEL_STEP * steps
    }
}

/// Unity's texture size for the given terrain bounds. Only used to reproduce the uv arithmetic.
pub fn uv_size(bounds: (IVec2, IVec2)) -> f32 {
    let extent = bounds
        .0
        .abs()
        .max(bounds.1.abs())
        .max_element()
        .unsigned_abs();
    (extent * 2 + 2).next_power_of_two() as f32
}

/// Unity's `GetHeight`: bilinear occupancy over parcel-centred texels plus the mountain noise.
/// `texel(parcel)` returns the occupancy texture value (see `texel_value`).
pub fn height(
    x: f32,
    unity_z: f32,
    max_steps: f32,
    uv_size: f32,
    texel: impl Fn(IVec2) -> f32,
) -> f32 {
    if max_steps <= 0.0 {
        return 0.0;
    }
    let half = (uv_size * 0.5) as i32;
    let last = uv_size as i32 - 1;
    let uv = [x, unity_z].map(|value| (value / 16.0 + uv_size * 0.5) / uv_size);
    let pixel = uv.map(|value| value * uv_size - 0.5);
    let base = pixel.map(|value| value.floor() as i32);
    let fraction = [pixel[0] - pixel[0].floor(), pixel[1] - pixel[1].floor()];
    // Unity's texture clamps to its edge
    let load = |x: i32, y: i32| texel(IVec2::new(x.clamp(0, last) - half, y.clamp(0, last) - half));
    let lerp = |a: f32, b: f32, t: f32| a + t * (b - a);
    let top = lerp(
        load(base[0], base[1]),
        load(base[0] + 1, base[1]),
        fraction[0],
    );
    let bottom = lerp(
        load(base[0], base[1] + 1),
        load(base[0] + 1, base[1] + 1),
        fraction[0],
    );
    let occupancy = lerp(top, bottom, fraction[1]) * (1.0 / 255.0);
    let floor = (255.0 - TEXEL_STEP * max_steps) / 255.0;
    if occupancy <= floor {
        return 0.0;
    }
    let normalized = (occupancy - floor) / (1.0 - floor);
    (normalized * (max_steps * STEP_HEIGHT)
        + mountain_noise(x, unity_z) * (normalized * 20.0).clamp(0.0, 1.0))
    .max(0.0)
}

fn simplex(v: [f32; 2]) -> f32 {
    let c = [0.211_324_87_f32, 0.366_025_42, -0.577_350_26, 0.024_390_243];
    // Wider accumulation is required by the recorded Unity editor CPU samples.
    let dot = |a: [f32; 2], b: [f32; 2]| {
        (f64::from(a[0]) * f64::from(b[0]) + f64::from(a[1]) * f64::from(b[1])) as f32
    };
    let skew = dot(v, [c[1]; 2]);
    let i = v.map(|value| (value + skew).floor());
    let unskew = dot(i, [c[0]; 2]);
    let x0 = [v[0] - i[0] + unskew, v[1] - i[1] + unskew];
    let i1 = if x0[0] > x0[1] {
        [1.0, 0.0]
    } else {
        [0.0, 1.0]
    };
    let corners = [
        x0,
        [x0[0] + c[0] - i1[0], x0[1] + c[0] - i1[1]],
        [x0[0] + c[2], x0[1] + c[2]],
    ];
    let modulo = |value: f32| value - (value * (1.0 / 289.0)).floor() * 289.0;
    let permute = |value: f32| modulo((34.0 * value + 1.0) * value);
    let i = i.map(modulo);
    let offsets = [[0.0, 0.0], i1, [1.0, 1.0]];
    let mut terms = [0.0; 3];
    for index in 0..3 {
        let p = permute(permute(i[1] + offsets[index][1]) + i[0] + offsets[index][0]);
        let mut m = (0.5 - dot(corners[index], corners[index])).max(0.0);
        m *= m;
        m *= m;
        let fractional = p * c[3] - (p * c[3]).floor();
        let x = 2.0 * fractional - 1.0;
        let h = x.abs() - 0.5;
        let a = x - (x + 0.5).floor();
        m *= 1.792_842_9 - 0.853_734_73 * (a * a + h * h);
        terms[index] = m * (a * corners[index][0] + h * corners[index][1]);
    }
    130.0 * (terms[0] + terms[1] + terms[2])
}

pub fn mountain_noise(x: f32, unity_z: f32) -> f32 {
    let mut amplitude = 1.0;
    let mut frequency = 1.0;
    let mut height = 0.0;
    for offset in [
        [-99_974.82, -93_748.33],
        [-67_502.3, -22_190.19],
        [77_881.34, -61_863.88],
    ] {
        height += simplex([
            (x + offset[0]) * 0.02 * frequency,
            (unity_z + offset[1]) * 0.02 * frequency,
        ]) * amplitude;
        amplitude *= 0.338;
        frequency *= 2.9;
    }
    height * 3.0
}

/// Current terrain height under the primary user (0 on scene parcels).
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct PlayerTerrainHeight(pub f32);

/// A resolved region of the step grid.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TerrainChange {
    /// The bounds changed; every texel may differ.
    Full,
    /// Only texels in this inclusive parcel rect changed.
    Rect(IVec2, IVec2),
}

/// Per-parcel step targets. Occupancy changes are marked as they arrive and resolved in batches:
/// a changed parcel only affects step values within `RESOLVE_RADIUS`, so each batch re-runs the
/// chamfer over the changed rect plus a margin instead of the whole realm.
#[derive(Debug)]
pub struct TerrainTargets {
    world: bool,
    epoch: u64,
    occupied_bounds: (IVec2, IVec2),
    bounds: Option<(IVec2, IVec2)>,
    occupancy: Vec<Occupancy>,
    steps: Vec<i8>,
    unknown: usize,
    max_steps: u8,
    dirty: Option<(IVec2, IVec2)>,
    rebuild: bool,
}

impl Default for TerrainTargets {
    fn default() -> Self {
        Self {
            world: false,
            epoch: 0,
            occupied_bounds: (IVec2::MAX, IVec2::MIN),
            bounds: None,
            occupancy: Vec::new(),
            steps: Vec::new(),
            unknown: 0,
            max_steps: MAX_STEPS,
            dirty: None,
            rebuild: true,
        }
    }
}

fn union(rect: Option<(IVec2, IVec2)>, min: IVec2, max: IVec2) -> Option<(IVec2, IVec2)> {
    Some(match rect {
        Some((a, b)) => (a.min(min), b.max(max)),
        None => (min, max),
    })
}

impl TerrainTargets {
    /// Drop all state, e.g. on realm change. Consumers should snap rather than ease.
    pub fn reset(&mut self, world: bool) {
        let epoch = self.epoch.wrapping_add(1);
        *self = Self {
            world,
            epoch,
            ..Default::default()
        };
    }

    /// Recompute everything from scratch on the next resolve.
    pub fn invalidate(&mut self) {
        self.rebuild = true;
    }

    pub fn mark(&mut self, parcel: IVec2, occupied: bool) {
        if occupied {
            self.occupied_bounds = (
                self.occupied_bounds.0.min(parcel),
                self.occupied_bounds.1.max(parcel),
            );
        }
        self.dirty = union(self.dirty, parcel, parcel);
    }

    pub fn needs_resolve(&self) -> bool {
        self.rebuild || self.dirty.is_some()
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Inclusive terrain bounds: occupied bounds plus Unity's padding.
    pub fn bounds(&self) -> Option<(IVec2, IVec2)> {
        self.bounds
    }

    /// `MAX_STEPS` while any parcel in bounds is unresolved, then Unity's value capped at
    /// `MAX_STEPS`. Unresolved parcels read as occupied, so a value computed from partial data
    /// would start low and shrink the noise ramp as the realm fills in.
    pub fn max_steps(&self) -> u8 {
        self.max_steps
    }

    /// Step value of a parcel, `ZERO` for occupied/unknown/out-of-bounds.
    pub fn steps(&self, parcel: IVec2) -> i8 {
        self.index(parcel).map_or(ZERO, |index| self.steps[index])
    }

    fn index(&self, parcel: IVec2) -> Option<usize> {
        let (min, max) = self.bounds?;
        if parcel.cmplt(min).any() || parcel.cmpgt(max).any() {
            return None;
        }
        let width = max.x - min.x + 1;
        let offset = parcel - min;
        Some((offset.y * width + offset.x) as usize)
    }

    fn padded(&self) -> Option<(IVec2, IVec2)> {
        let (min, max) = self.occupied_bounds;
        if min.cmpgt(max).any() {
            return None;
        }
        let dimensions = max - min + 1;
        let padding = BORDER_PADDING
            + if self.world {
                (0.1_f32 * (dimensions.x + dimensions.y) as f32 / 2.0).round_ties_even() as i32
            } else {
                0
            };
        let bounds = (min - padding, max + padding);
        let size = (bounds.1 - bounds.0 + 1).as_i64vec2();
        (size.x * size.y <= MAX_TEXELS).then_some(bounds)
    }

    /// Resolve pending changes. `occupancy` looks up a parcel; `occupied` lists every parcel
    /// known to be occupied (only used to rebuild the occupied bounds).
    pub fn resolve(
        &mut self,
        occupancy: impl Fn(IVec2) -> Occupancy,
        occupied: impl Iterator<Item = IVec2>,
    ) -> Option<TerrainChange> {
        if self.rebuild {
            self.occupied_bounds = occupied.fold((IVec2::MAX, IVec2::MIN), |(min, max), parcel| {
                (min.min(parcel), max.max(parcel))
            });
        }
        let bounds = self.padded();
        let change = if self.rebuild || bounds != self.bounds {
            self.bounds = bounds;
            self.rebuild_all(&occupancy);
            Some(TerrainChange::Full)
        } else {
            self.dirty
                .and_then(|dirty| self.resolve_rect(dirty, &occupancy))
        };
        self.rebuild = false;
        self.dirty = None;
        if change.is_some() {
            self.max_steps = if self.unknown > 0 {
                MAX_STEPS
            } else {
                self.steps.iter().copied().max().unwrap_or(0).max(0) as u8
            };
        }
        change
    }

    fn rebuild_all(&mut self, occupancy: &impl Fn(IVec2) -> Occupancy) {
        self.occupancy.clear();
        self.steps.clear();
        self.unknown = 0;
        let Some((min, max)) = self.bounds else {
            return;
        };
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                let value = occupancy(IVec2::new(x, y));
                self.unknown += usize::from(value == Occupancy::Unknown);
                self.occupancy.push(value);
            }
        }
        self.steps = vec![ZERO; self.occupancy.len()];
        self.chamfer(min, max, (min, max));
    }

    fn resolve_rect(
        &mut self,
        dirty: (IVec2, IVec2),
        occupancy: &impl Fn(IVec2) -> Occupancy,
    ) -> Option<TerrainChange> {
        let (min, max) = self.bounds?;
        let changed = (dirty.0.max(min), dirty.1.min(max));
        if changed.0.cmpgt(changed.1).any() {
            return None;
        }
        let mut any = false;
        for y in changed.0.y..=changed.1.y {
            for x in changed.0.x..=changed.1.x {
                let parcel = IVec2::new(x, y);
                let index = self.index(parcel).unwrap();
                let value = occupancy(parcel);
                let previous = std::mem::replace(&mut self.occupancy[index], value);
                if previous != value {
                    any = true;
                    self.unknown -= usize::from(previous == Occupancy::Unknown);
                    self.unknown += usize::from(value == Occupancy::Unknown);
                }
            }
        }
        if !any {
            return None;
        }
        let affected = (
            (changed.0 - RESOLVE_RADIUS).max(min),
            (changed.1 + RESOLVE_RADIUS).min(max),
        );
        let region = (
            (affected.0 - RESOLVE_RADIUS).max(min),
            (affected.1 + RESOLVE_RADIUS).min(max),
        );
        self.chamfer(region.0, region.1, affected);
        Some(TerrainChange::Rect(affected.0, affected.1))
    }

    /// Unity's two-pass 3/4 chamfer over `region`, writing steps for `write`. Parcels outside the
    /// bounds are occupied; parcels outside `region` but inside the bounds are far, which is exact
    /// for `write` when it lies `RESOLVE_RADIUS` inside `region` (a chamfer path stays within the
    /// box spanned by its ends, and capped values only need distances up to that radius).
    fn chamfer(&mut self, region_min: IVec2, region_max: IVec2, write: (IVec2, IVec2)) {
        let (bounds_min, bounds_max) = self.bounds.unwrap();
        let width = (region_max.x - region_min.x + 1) as usize;
        let height = (region_max.y - region_min.y + 1) as usize;
        let mut distances = vec![0u32; width * height];
        for y in 0..height {
            for x in 0..width {
                let parcel = region_min + IVec2::new(x as i32, y as i32);
                if self.occupancy[self.index(parcel).unwrap()] == Occupancy::Empty {
                    distances[y * width + x] = FAR;
                }
            }
        }
        let outside = |x: isize, y: isize| {
            let parcel = region_min + IVec2::new(x as i32, y as i32);
            if parcel.cmplt(bounds_min).any() || parcel.cmpgt(bounds_max).any() {
                0
            } else {
                FAR
            }
        };
        let distance = |distances: &[u32], x: isize, y: isize| {
            if x < 0 || y < 0 || x >= width as isize || y >= height as isize {
                outside(x, y)
            } else {
                distances[y as usize * width + x as usize]
            }
        };
        for y in 0..height as isize {
            for x in 0..width as isize {
                let index = y as usize * width + x as usize;
                if distances[index] != 0 {
                    distances[index] = distances[index]
                        .min(distance(&distances, x - 1, y) + 3)
                        .min(distance(&distances, x, y - 1) + 3)
                        .min(distance(&distances, x - 1, y - 1) + 4)
                        .min(distance(&distances, x + 1, y - 1) + 4);
                }
            }
        }
        for y in (0..height as isize).rev() {
            for x in (0..width as isize).rev() {
                let index = y as usize * width + x as usize;
                if distances[index] != 0 {
                    distances[index] = distances[index]
                        .min(distance(&distances, x + 1, y) + 3)
                        .min(distance(&distances, x, y + 1) + 3)
                        .min(distance(&distances, x + 1, y + 1) + 4)
                        .min(distance(&distances, x - 1, y + 1) + 4);
                }
            }
        }
        for y in write.0.y..=write.1.y {
            for x in write.0.x..=write.1.x {
                let parcel = IVec2::new(x, y);
                let local = parcel - region_min;
                let distance = distances[local.y as usize * width + local.x as usize];
                let index = self.index(parcel).unwrap();
                self.steps[index] = if distance == 0 {
                    ZERO
                } else {
                    (distance / 3).saturating_sub(1).min(u32::from(MAX_STEPS)) as i8
                };
            }
        }
    }
}

#[cfg(test)]
mod tests;
