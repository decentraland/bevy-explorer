use std::collections::HashSet;

use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};

use super::*;

/// Unity's manifest-built occupancy texture, ported verbatim by eordano in #1342. Used as the
/// reference the incremental targets must reproduce.
struct Reference {
    min_parcel: [i32; 2],
    max_parcel: [i32; 2],
    size: usize,
    floor: u8,
    max_steps: u32,
    pixels: Vec<u8>,
}

impl Reference {
    fn from_parcels(parcels: &[[i32; 2]], world: bool) -> Self {
        let mut min = [i32::MAX; 2];
        let mut max = [i32::MIN; 2];
        for parcel in parcels {
            for axis in 0..2 {
                min[axis] = min[axis].min(parcel[axis]);
                max[axis] = max[axis].max(parcel[axis]);
            }
        }
        let dimensions = [
            i64::from(max[0]) - i64::from(min[0]) + 1,
            i64::from(max[1]) - i64::from(min[1]) + 1,
        ];
        let padding = 2 + if world {
            (0.1_f32 * (dimensions[0] + dimensions[1]) as f32 / 2.0).round_ties_even() as i64
        } else {
            0
        };
        for axis in 0..2 {
            let lo =
                i64::from(min[axis]) + dimensions[axis] / 2 - (dimensions[axis] + 2 * padding) / 2;
            let hi = lo + dimensions[axis] + 2 * padding - 1;
            min[axis] = lo as i32;
            max[axis] = hi as i32;
        }
        let extent = min
            .into_iter()
            .chain(max)
            .map(i32::unsigned_abs)
            .max()
            .unwrap();
        let size = (extent * 2 + 2).next_power_of_two() as usize;
        let half = (size / 2) as i32;
        let [min_x, min_y] = min.map(|value| (value + half) as usize);
        let [max_x, max_y] = max.map(|value| (value + half) as usize);
        let mut pixels = vec![0; size * size];
        for y in min_y..=max_y {
            pixels[y * size..(y + 1) * size].fill(255);
            pixels[y * size + min_x - 1] = 0;
            if max_x + 1 < size {
                pixels[y * size + max_x + 1] = 0;
            }
        }
        for [x, y] in parcels {
            pixels[(y + half) as usize * size + (x + half) as usize] = 0;
        }
        let mut distances = vec![0_u32; pixels.len()];
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if pixels[y * size + x] != 0 {
                    distances[y * size + x] = 1_000_000;
                }
            }
        }
        let distance = |distances: &[u32], x: usize, y: usize| {
            if x < min_x || x > max_x || y < min_y || y > max_y {
                0
            } else {
                distances[y * size + x]
            }
        };
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let index = y * size + x;
                if distances[index] != 0 {
                    distances[index] = distances[index]
                        .min(distance(&distances, x - 1, y) + 3)
                        .min(distance(&distances, x, y - 1) + 3)
                        .min(distance(&distances, x - 1, y - 1) + 4)
                        .min(distance(&distances, x + 1, y - 1) + 4);
                }
            }
        }
        let mut max_distance = 0;
        for y in (min_y..=max_y).rev() {
            for x in (min_x..=max_x).rev() {
                let index = y * size + x;
                if distances[index] != 0 {
                    distances[index] = distances[index]
                        .min(distance(&distances, x + 1, y) + 3)
                        .min(distance(&distances, x, y + 1) + 3)
                        .min(distance(&distances, x + 1, y + 1) + 4)
                        .min(distance(&distances, x - 1, y + 1) + 4);
                }
                max_distance = max_distance.max(distances[index]);
            }
        }
        let max_steps = (max_distance / 3).saturating_sub(1).min(255);
        let step = if max_steps <= 25 {
            10
        } else {
            (254 / max_steps).max(1)
        };
        let floor = (255 - step * max_steps) as u8;
        if max_steps > 0 {
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    let index = y * size + x;
                    if pixels[index] != 0 {
                        pixels[index] = (u32::from(floor)
                            + step * (distances[index] / 3).saturating_sub(1).min(max_steps))
                            as u8;
                    }
                }
            }
        }
        Self {
            min_parcel: min,
            max_parcel: max,
            size,
            floor,
            max_steps,
            pixels,
        }
    }

    fn pixel(&self, parcel: IVec2) -> u8 {
        let half = (self.size / 2) as i32;
        let [x, y] = [parcel.x + half, parcel.y + half];
        if x < 0 || y < 0 || x >= self.size as i32 || y >= self.size as i32 {
            return 0;
        }
        self.pixels[y as usize * self.size + x as usize]
    }

    fn height(&self, x: f32, unity_z: f32) -> f32 {
        let uv =
            [x, unity_z].map(|value| (value / 16.0 + self.size as f32 * 0.5) / self.size as f32);
        let pixel = uv.map(|value| value * self.size as f32 - 0.5);
        let base = pixel.map(|value| value.floor() as i32);
        let fraction = [pixel[0] - pixel[0].floor(), pixel[1] - pixel[1].floor()];
        let load = |x: i32, y: i32| {
            f32::from(
                self.pixels[y.clamp(0, self.size as i32 - 1) as usize * self.size
                    + x.clamp(0, self.size as i32 - 1) as usize],
            )
        };
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
        let floor = f32::from(self.floor) / 255.0;
        if occupancy <= floor {
            return 0.0;
        }
        let normalized = (occupancy - floor) / (1.0 - floor);
        (normalized * (self.max_steps as f32 * 1.8)
            + mountain_noise(x, unity_z) * (normalized * 20.0).clamp(0.0, 1.0))
        .max(0.0)
    }
}

/// Targets with every parcel known, as the incremental path converges to.
fn resolved(parcels: &[[i32; 2]], world: bool) -> TerrainTargets {
    let occupied: HashSet<IVec2> = parcels.iter().map(|p| IVec2::from_array(*p)).collect();
    let mut targets = TerrainTargets::default();
    targets.reset(world);
    targets.resolve(
        |parcel| {
            if occupied.contains(&parcel) {
                Occupancy::Occupied
            } else {
                Occupancy::Empty
            }
        },
        occupied.iter().copied(),
    );
    targets
}

fn texel(targets: &TerrainTargets) -> impl Fn(IVec2) -> f32 + '_ {
    let max_steps = f32::from(targets.max_steps());
    move |parcel| texel_value(f32::from(targets.steps(parcel)), max_steps)
}

fn target_height(targets: &TerrainTargets, x: f32, unity_z: f32) -> f32 {
    height(
        x,
        unity_z,
        f32::from(targets.max_steps()),
        targets.bounds().map_or(1.0, uv_size),
        texel(targets),
    )
}

fn square_world() -> Vec<[i32; 2]> {
    (0..20).flat_map(|x| (0..20).map(move |y| [x, y])).collect()
}

#[test]
fn captured_unity_world_heights_include_empty_terrain_and_negative_coordinates() {
    let targets = resolved(&square_world(), true);
    for (x, z, expected) in [
        (16.0, -40.0, 3.038_654_8),
        (0.0, 0.0, 0.0),
        (336.0, 0.0, 0.279_771_98),
        (-48.0, 40.0, 0.989_635_9),
        (-32.0, 344.0, 1.757_285_8),
    ] {
        let actual = target_height(&targets, x, z);
        assert!((actual - expected).abs() <= 0.000_001, "{x},{z}: {actual}");
    }
    // Unity captured 4.207179 here: its texture keeps 255 beyond the x bounds of in-bounds rows,
    // behind the border walls. Out of bounds is flat until the coast slice.
    assert_eq!(target_height(&targets, -176.0, -56.0), 0.0);
    assert_eq!(targets.bounds(), Some((IVec2::splat(-4), IVec2::splat(23))));
    assert_eq!(targets.max_steps(), 1);
}

#[test]
fn world_padding_rounds_midpoints_to_even() {
    for (height, padding) in [(9, 2), (29, 4), (49, 4)] {
        let parcels: Vec<_> = (0..height).map(|y| [0, y]).collect();
        let targets = resolved(&parcels, true);
        assert_eq!(
            targets.bounds(),
            Some((
                IVec2::splat(-padding),
                IVec2::new(padding, height - 1 + padding)
            ))
        );
    }
}

fn random_parcels(rng: &mut StdRng, extent: i32, count: usize) -> Vec<[i32; 2]> {
    (0..count)
        .map(|_| {
            [
                rng.gen_range(-extent..=extent),
                rng.gen_range(-extent..=extent),
            ]
        })
        .collect()
}

#[test]
fn resolved_targets_match_unity_pixels_and_heights() {
    let mut rng = StdRng::seed_from_u64(1342);
    let mut compared = 0;
    for case in 0..40 {
        let world = case % 2 == 0;
        let extent = rng.gen_range(3..40);
        let count = rng.gen_range(1..(extent * extent) as usize);
        let parcels = random_parcels(&mut rng, extent, count);
        let reference = Reference::from_parcels(&parcels, world);
        let targets = resolved(&parcels, world);
        assert_eq!(
            targets.bounds(),
            Some((
                IVec2::from_array(reference.min_parcel),
                IVec2::from_array(reference.max_parcel)
            ))
        );
        if reference.max_steps > u32::from(MAX_STEPS) {
            // capped: step values beyond the cap differ by design
            assert_eq!(targets.max_steps(), MAX_STEPS);
            continue;
        }
        compared += 1;
        assert_eq!(u32::from(targets.max_steps()), reference.max_steps);
        // Unity only uses the texture inside the bounds; beyond its zero frame, in-bounds rows
        // keep 255 (see the captured test)
        let (min, max) = targets.bounds().unwrap();
        for y in min.y - 1..=max.y + 1 {
            for x in min.x - 1..=max.x + 1 {
                let parcel = IVec2::new(x, y);
                let expected = if reference.max_steps == 0 {
                    // Unity leaves empty texels at 255 == floor; both are flat
                    continue;
                } else {
                    f32::from(reference.pixel(parcel))
                };
                assert_eq!(texel(&targets)(parcel), expected, "{parcel}");
            }
        }
        for _ in 0..200 {
            let x = rng.gen_range(min.x as f32 * 16.0..(max.x + 1) as f32 * 16.0);
            let z = rng.gen_range(min.y as f32 * 16.0..(max.y + 1) as f32 * 16.0);
            let expected = reference.height(x, z);
            let actual = target_height(&targets, x, z);
            assert!(
                (actual - expected).abs() <= 0.000_01,
                "{x},{z}: {actual} vs {expected}, steps {} vs {}",
                targets.max_steps(),
                reference.max_steps
            );
        }
    }
    assert!(compared > 10, "only {compared} uncapped cases");
}

/// Reveal a realm in random batches, as pointer responses arrive. Every intermediate state must
/// equal a from-scratch resolve of what is known so far, and heights must never fall.
#[test]
fn incremental_resolve_matches_rebuild_and_heights_only_rise() {
    let mut rng = StdRng::seed_from_u64(7);
    for case in 0..12 {
        let world = case % 3 == 0;
        let extent = rng.gen_range(8..30);
        let occupied: HashSet<IVec2> = random_parcels(&mut rng, extent, (extent * extent) as usize)
            .into_iter()
            .map(IVec2::from_array)
            .collect();
        let mut order: Vec<IVec2> = (-extent - 4..=extent + 4)
            .flat_map(|x| (-extent - 4..=extent + 4).map(move |y| IVec2::new(x, y)))
            .collect();
        order.shuffle(&mut rng);

        let mut known = HashSet::new();
        let mut targets = TerrainTargets::default();
        targets.reset(world);
        let samples: Vec<(f32, f32)> = (0..300)
            .map(|_| {
                let range = (extent + 6) as f32 * 16.0;
                (rng.gen_range(-range..range), rng.gen_range(-range..range))
            })
            .collect();
        let mut previous: Vec<f32> = vec![0.0; samples.len()];
        let lookup = |known: &HashSet<IVec2>, parcel: IVec2| {
            if !known.contains(&parcel) {
                Occupancy::Unknown
            } else if occupied.contains(&parcel) {
                Occupancy::Occupied
            } else {
                Occupancy::Empty
            }
        };
        for batch in order.chunks(rng.gen_range(1..60)) {
            for parcel in batch {
                known.insert(*parcel);
                targets.mark(*parcel, occupied.contains(parcel));
            }
            targets.resolve(
                |parcel| lookup(&known, parcel),
                known.iter().copied().filter(|p| occupied.contains(p)),
            );

            let mut fresh = TerrainTargets::default();
            fresh.reset(world);
            fresh.resolve(
                |parcel| lookup(&known, parcel),
                known.iter().copied().filter(|p| occupied.contains(p)),
            );
            assert_eq!(targets.bounds(), fresh.bounds());
            assert_eq!(targets.max_steps(), fresh.max_steps());
            if let Some((min, max)) = fresh.bounds() {
                for y in min.y..=max.y {
                    for x in min.x..=max.x {
                        let parcel = IVec2::new(x, y);
                        assert_eq!(targets.steps(parcel), fresh.steps(parcel), "{parcel}");
                    }
                }
            }

            // the step count only drops once, when the last parcel resolves
            if targets.max_steps() == MAX_STEPS {
                for (sample, previous) in samples.iter().zip(previous.iter_mut()) {
                    let height = target_height(&targets, sample.0, sample.1);
                    assert!(height >= *previous - 0.000_1, "{sample:?} fell");
                    *previous = height;
                }
            }
        }
    }
}

/// Genesis converged from pointers equals Unity's manifest texture.
/// `GENESIS_MANIFEST=/tmp/genesis-manifest.json cargo test -p common genesis -- --ignored`
#[test]
#[ignore]
fn genesis_matches_manifest() {
    let path = std::env::var("GENESIS_MANIFEST").unwrap();
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let parcels: Vec<[i32; 2]> = manifest["occupied"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let (x, y) = p.as_str().unwrap().split_once(',').unwrap();
            [x.trim().parse().unwrap(), y.trim().parse().unwrap()]
        })
        .collect();
    let reference = Reference::from_parcels(&parcels, false);
    let targets = resolved(&parcels, false);
    assert_eq!(reference.max_steps, 8);
    assert_eq!(targets.max_steps(), 8);
    let (min, max) = targets.bounds().unwrap();
    for y in min.y..=max.y {
        for x in min.x..=max.x {
            let parcel = IVec2::new(x, y);
            assert_eq!(
                texel(&targets)(parcel),
                f32::from(reference.pixel(parcel)),
                "{parcel}"
            );
        }
    }
}
