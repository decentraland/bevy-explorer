/// Unity parcel occupancy and CPU collision heights. X/Z use Unity coordinates;
/// negate Bevy Z. Visibility, GPU height-map sampling and collision are separate.
#[derive(Clone, Debug)]
pub struct TerrainField {
    pub min_parcel: [i32; 2],
    pub max_parcel: [i32; 2],
    pub size: usize,
    pub floor: u8,
    pub max_steps: u32,
    pub pixels: Vec<u8>,
}

impl TerrainField {
    pub fn from_parcels(parcels: &[[i32; 2]], world: bool) -> Result<Self, &'static str> {
        if parcels.is_empty() {
            return Err("terrain requires occupied parcels");
        }
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
            if lo < -2047 || hi > 2047 {
                return Err("terrain occupancy exceeds the 4096-pixel limit");
            }
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
        Ok(Self {
            min_parcel: min,
            max_parcel: max,
            size,
            floor,
            max_steps,
            pixels,
        })
    }

    pub fn contains(&self, x: f32, unity_z: f32) -> bool {
        [x, unity_z].into_iter().enumerate().all(|(axis, value)| {
            value >= self.min_parcel[axis] as f32 * 16.0
                && value < (self.max_parcel[axis] + 1) as f32 * 16.0
        })
    }

    /// Matches the CPU lookup, including samples outside rendered terrain bounds.
    /// Surface callers must check `contains` before treating a height as terrain.
    pub fn height(&self, x: f32, unity_z: f32) -> f32 {
        let occupancy = self.sample_occupancy(x, unity_z);
        let floor = f32::from(self.floor) / 255.0;
        if occupancy <= floor {
            return 0.0;
        }
        let normalized = (occupancy - floor) / (1.0 - floor);
        (normalized * (self.max_steps as f32 * 1.8)
            + mountain_noise(x, unity_z) * (normalized * 20.0).clamp(0.0, 1.0))
        .max(0.0)
    }

    fn sample_occupancy(&self, x: f32, unity_z: f32) -> f32 {
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
        lerp(top, bottom, fraction[1]) * (1.0 / 255.0)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> TerrainField {
        let parcels: Vec<_> = (0..20).flat_map(|x| (0..20).map(move |y| [x, y])).collect();
        TerrainField::from_parcels(&parcels, true).unwrap()
    }

    #[test]
    fn captured_unity_world_heights_include_empty_terrain_and_negative_coordinates() {
        let field = world();
        for (x, z, expected) in [
            (-176.0, -56.0, 4.207_179),
            (16.0, -40.0, 3.038_654_8),
            (0.0, 0.0, 0.0),
            (336.0, 0.0, 0.279_771_98),
            (-48.0, 40.0, 0.989_635_9),
            (-32.0, 344.0, 1.757_285_8),
        ] {
            assert!(
                (field.height(x, z) - expected).abs() <= 0.000_001,
                "{x},{z}"
            );
        }
        assert!(!field.contains(-176.0, -56.0));
        assert!(field.contains(-64.0, -64.0));
        assert!(!field.contains(384.0, 0.0));
        assert!(!field.contains(0.0, 384.0));
    }

    #[test]
    fn captured_world_occupancy_has_exact_bounds_and_distribution() {
        let field = world();
        assert_eq!(field.min_parcel, [-4, -4]);
        assert_eq!(field.max_parcel, [23, 23]);
        assert_eq!((field.size, field.floor, field.max_steps), (64, 245, 1));
        let mut histogram = [0; 256];
        for value in field.pixels {
            histogram[value as usize] += 1;
        }
        assert_eq!(histogram[0], 2760);
        assert_eq!(histogram[245], 192);
        assert_eq!(histogram[255], 1144);
    }

    #[test]
    fn duplicates_and_order_do_not_change_the_distance_field() {
        let parcels = [[-4, -2], [-3, -1], [-1, 0]];
        let expected = TerrainField::from_parcels(&parcels, false).unwrap();
        let actual =
            TerrainField::from_parcels(&[parcels[2], parcels[0], parcels[1], parcels[0]], false)
                .unwrap();
        assert_eq!(actual.pixels, expected.pixels);
    }

    #[test]
    fn world_padding_rounds_midpoints_to_even() {
        for (height, padding) in [(9, 2), (29, 4), (49, 4)] {
            let parcels: Vec<_> = (0..height).map(|y| [0, y]).collect();
            let field = TerrainField::from_parcels(&parcels, true).unwrap();
            assert_eq!(field.min_parcel, [-padding, -padding]);
            assert_eq!(field.max_parcel, [padding, height - 1 + padding]);
        }
    }

    #[test]
    fn texture_edge_and_invalid_extents_are_bounded() {
        let field = TerrainField::from_parcels(&[[13, 13]], false).unwrap();
        assert_eq!(field.max_parcel, [15, 15]);
        assert_eq!(field.size, 32);
        assert_eq!(field.height(1e9, 1e9), 0.0);
        for parcels in [
            &[][..],
            &[[i32::MIN, 0]][..],
            &[[i32::MAX, 0]][..],
            &[[0, -3000]][..],
        ] {
            assert!(TerrainField::from_parcels(parcels, false).is_err());
        }
    }
}
