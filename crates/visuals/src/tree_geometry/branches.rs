//! Continuous tapered tubes. Shared rings avoid overlapping segment caps and
//! horizontal lighting seams while substantially reducing branch vertices.
use super::*;

impl Geometry {
    pub(crate) fn branch(&mut self, from: Vec3, to: Vec3, radius: f32, tip: f32, sides: usize) {
        self.branch_path(&[from, to], &[radius, tip], sides);
    }

    pub(super) fn branch_path(&mut self, points: &[Vec3], radii: &[f32], sides: usize) {
        assert!(points.len() >= 2 && points.len() == radii.len() && sides >= 3);
        let base = self.positions.len() as u32;
        let stride = (sides + 1) as u32;
        let mut last_axis = (points[1] - points[0]).normalize();
        let mut across = last_axis.any_orthonormal_vector();
        let mut distance = 0.0;
        let mut axes = Vec::with_capacity(points.len());
        for (ring, &point) in points.iter().enumerate() {
            let previous = ring.saturating_sub(1);
            let next = (ring + 1).min(points.len() - 1);
            let axis = (points[next] - points[previous]).normalize();
            // Transport the frame along the curve instead of choosing a new
            // arbitrary radial basis for each segment.
            across = Quat::from_rotation_arc(last_axis, axis) * across;
            across = (across - axis * across.dot(axis)).normalize();
            let around = axis.cross(across);
            let taper = (radii[previous] - radii[next]) / points[previous].distance(points[next]);
            if ring > 0 {
                distance += point.distance(points[ring - 1]);
            }
            for side in 0..=sides {
                let angle = side as f32 / sides as f32 * std::f32::consts::TAU;
                let radial = across * angle.cos() + around * angle.sin();
                let position = point + radial * radii[ring];
                let ridge = 0.96 + 0.04 * (angle * 3.0).sin();
                let color = Color::srgb(0.64 * ridge, 0.37 * ridge, 0.19 * ridge).to_linear();
                self.positions.push(position.to_array());
                self.normals
                    .push((radial + axis * taper).normalize().to_array());
                self.colors.push(color.to_f32_array());
                self.uv.push([
                    0.925 + 0.06 * side as f32 / sides as f32,
                    0.05 + distance * 0.08,
                ]);
                self.weights
                    .push([(position.y / 10.0).clamp(0.0, 1.0), 0.0]);
            }
            if ring > 0 {
                for side in 0..sides as u32 {
                    let a = base + (ring as u32 - 1) * stride + side;
                    let b = a + 1;
                    let c = a + stride;
                    let d = c + 1;
                    self.indices.extend_from_slice(&[a, b, c, b, d, c]);
                }
            }
            axes.push(axis);
            last_axis = axis;
        }
        // Only the two exposed ends have caps; internal joints share rings.
        for ring in [0, points.len() - 1] {
            let normal = axes[ring] * if ring == 0 { -1.0 } else { 1.0 };
            let center = self.positions.len() as u32;
            self.positions.push(points[ring].to_array());
            self.normals.push(normal.to_array());
            self.colors
                .push(Color::srgb(0.51, 0.29, 0.14).to_linear().to_f32_array());
            self.uv.push([0.997, 0.5]);
            self.weights
                .push([(points[ring].y / 10.0).clamp(0.0, 1.0), 0.0]);
            for side in 0..=sides {
                let index = base as usize + ring * stride as usize + side;
                self.positions.push(self.positions[index]);
                self.normals.push(normal.to_array());
                self.colors.push(self.colors[index]);
                self.uv.push([0.997, 0.5]);
                self.weights.push(self.weights[index]);
            }
            for side in 0..sides as u32 {
                let (a, b) = (center + 1 + side, center + 2 + side);
                self.indices.extend_from_slice(&if ring == 0 {
                    [center, b, a]
                } else {
                    [center, a, b]
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trunk_has_shared_smooth_rings_without_internal_caps_and_fewer_vertices() {
        let points = trunk_points();
        let mut geometry = Geometry::default();
        geometry.branch_path(&points, &[0.48, 0.36, 0.29, 0.24], 8);
        assert_eq!(geometry.positions.len(), 4 * 9 + 2 * 10);
        assert_eq!(geometry.indices.len() / 3, 3 * 8 * 2 + 2 * 8);
        assert!(geometry.positions.len() * 4 < 3 * 8 * 12);
        for ring in 0..4 {
            let first = ring * 9;
            let last = first + 8;
            assert!(
                Vec3::from_array(geometry.positions[first])
                    .distance(Vec3::from_array(geometry.positions[last]))
                    < 0.00001
            );
            assert!(
                Vec3::from_array(geometry.normals[first])
                    .distance(Vec3::from_array(geometry.normals[last]))
                    < 0.00001
            );
        }
        // A shared ring is referenced by triangles on both adjoining spans.
        for joint in 1..3 {
            let index = joint * 9 + 3;
            assert_eq!(geometry.indices.iter().filter(|&&i| i == index).count(), 6);
        }
        for indices in geometry.indices.chunks_exact(3) {
            let [a, b, c] =
                [0, 1, 2].map(|i| Vec3::from_array(geometry.positions[indices[i] as usize]));
            let normal = Vec3::from_array(geometry.normals[indices[0] as usize]);
            assert!((b - a).cross(c - a).dot(normal) > 0.0, "inverted tube face");
        }
    }
}
