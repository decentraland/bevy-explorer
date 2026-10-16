//! Signed distance fields from anti-aliased glyph masks: the coverage of the edge pixels gives
//! the distance sub-pixel accuracy, so a glyph rastered once at a modest size scales cleanly.
//!
//! A port of Stefan Gustavson's edtaa3 ("Anti-aliased Euclidean distance transform", Pattern
//! Recognition Letters 32, 2011; reference implementation `edtaa3func.c`, copyright (C)
//! 2009-2012 Stefan Gustavson, MIT license).

const SQRT2: f32 = std::f32::consts::SQRT_2;
const FAR: f32 = 1e6;
const EPSILON: f32 = 1e-3;

/// Convert an 8-bit coverage mask into an 8-bit signed distance field with `pad` pixels of
/// margin on every side: 128 at the edge, 255 at `pad` or more pixels inside the ink, 0 at
/// `pad` or more pixels outside it.
pub fn sdf_from_mask(mask: &[u8], width: u32, height: u32, pad: u32) -> Vec<u8> {
    let (width, height, pad) = (width as usize, height as usize, pad as usize);
    let (w, h) = (width + 2 * pad, height + 2 * pad);
    let mut coverage = vec![0.0f32; w * h];
    for y in 0..height {
        for x in 0..width {
            coverage[(y + pad) * w + x + pad] = mask[y * width + x] as f32 / 255.0;
        }
    }
    let outside = distance_transform(&coverage, w, h);
    let inverted = coverage.iter().map(|a| 1.0 - a).collect::<Vec<_>>();
    let inside = distance_transform(&inverted, w, h);

    let scale = 1.0 / (2.0 * pad as f32);
    outside
        .iter()
        .zip(&inside)
        .map(|(outside, inside)| {
            let distance = inside.max(0.0) - outside.max(0.0);
            ((0.5 + distance * scale).clamp(0.0, 1.0) * 255.0).round() as u8
        })
        .collect()
}

/// Distance from each pixel to the nearest covered pixel edge (0 inside full coverage).
fn distance_transform(img: &[f32], w: usize, h: usize) -> Vec<f32> {
    debug_assert!(w >= 2 && h >= 2);
    let (gx, gy) = gradient(img, w, h);
    let mut distx = vec![0i32; w * h];
    let mut disty = vec![0i32; w * h];
    let mut dist = img
        .iter()
        .enumerate()
        .map(|(i, &a)| {
            if a <= 0.0 {
                FAR
            } else if a < 1.0 {
                edge_distance(gx[i], gy[i], a)
            } else {
                0.0
            }
        })
        .collect::<Vec<_>>();

    // distance via neighbour `c`'s closest edge pixel, from `(xi, yi)` away
    let distance_via = |c: usize, xc: i32, yc: i32, xi: i32, yi: i32| -> f32 {
        let closest = (c as i32 - xc - yc * w as i32) as usize;
        let a = img[closest].clamp(0.0, 1.0);
        if a == 0.0 {
            return FAR;
        }
        let (dx, dy) = (xi as f32, yi as f32);
        let di = (dx * dx + dy * dy).sqrt();
        let df = if di == 0.0 {
            edge_distance(gx[closest], gy[closest], a)
        } else {
            edge_distance(dx, dy, a)
        };
        di + df
    };

    // (index delta, x step, y step) of each neighbour
    const L: (isize, i32, i32) = (-1, 1, 0);
    const R: (isize, i32, i32) = (1, -1, 0);
    let (u, ur, lu) = (
        (-(w as isize), 0, 1),
        (1 - w as isize, -1, 1),
        (-1 - w as isize, 1, 1),
    );
    let (d, dl, rd) = (
        (w as isize, 0, -1),
        (w as isize - 1, 1, -1),
        (w as isize + 1, -1, -1),
    );

    let mut propagate = |i: usize, neighbours: &[(isize, i32, i32)]| -> bool {
        if dist[i] <= 0.0 {
            return false;
        }
        let mut changed = false;
        for &(delta, ox, oy) in neighbours {
            let c = (i as isize + delta) as usize;
            let (nx, ny) = (distx[c] + ox, disty[c] + oy);
            let nd = distance_via(c, distx[c], disty[c], nx, ny);
            if nd < dist[i] - EPSILON {
                distx[i] = nx;
                disty[i] = ny;
                dist[i] = nd;
                changed = true;
            }
        }
        changed
    };

    // until no pixel finds a closer edge: in a concave region the way to the closest edge can
    // turn against both scans, and takes another pass
    loop {
        let mut changed = false;
        // forward scan: from above and the left, then the right
        for y in 1..h {
            changed |= propagate(y * w, &[u, ur]);
            for x in 1..w - 1 {
                changed |= propagate(y * w + x, &[L, lu, u, ur]);
            }
            changed |= propagate(y * w + w - 1, &[L, lu, u]);
            for x in (0..w - 1).rev() {
                changed |= propagate(y * w + x, &[R]);
            }
        }
        // backward scan: from below and the right, then the left
        for y in (0..h - 1).rev() {
            changed |= propagate(y * w + w - 1, &[d, dl]);
            for x in (1..w - 1).rev() {
                changed |= propagate(y * w + x, &[R, rd, d, dl]);
            }
            changed |= propagate(y * w, &[R, rd, d]);
            for x in 1..w {
                changed |= propagate(y * w + x, &[L]);
            }
        }
        if !changed {
            break;
        }
    }

    dist
}

/// Normalised coverage gradient at the edge pixels (zero elsewhere and on the border).
fn gradient(img: &[f32], w: usize, h: usize) -> (Vec<f32>, Vec<f32>) {
    let mut gx = vec![0.0f32; w * h];
    let mut gy = vec![0.0f32; w * h];
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let k = y * w + x;
            if img[k] <= 0.0 || img[k] >= 1.0 {
                continue;
            }
            let mut dx = -img[k - w - 1] - SQRT2 * img[k - 1] - img[k + w - 1]
                + img[k - w + 1]
                + SQRT2 * img[k + 1]
                + img[k + w + 1];
            let mut dy = -img[k - w - 1] - SQRT2 * img[k - w] - img[k - w + 1]
                + img[k + w - 1]
                + SQRT2 * img[k + w]
                + img[k + w + 1];
            let len = (dx * dx + dy * dy).sqrt();
            if len > 0.0 {
                dx /= len;
                dy /= len;
            }
            gx[k] = dx;
            gy[k] = dy;
        }
    }
    (gx, gy)
}

/// Distance from a pixel's centre to the edge crossing it, from its coverage `a` and the edge
/// direction `(gx, gy)`; positive on the uncovered side.
fn edge_distance(gx: f32, gy: f32, a: f32) -> f32 {
    if gx == 0.0 || gy == 0.0 {
        return 0.5 - a;
    }
    let len = (gx * gx + gy * gy).sqrt();
    let (mut gx, mut gy) = ((gx / len).abs(), (gy / len).abs());
    if gx < gy {
        std::mem::swap(&mut gx, &mut gy);
    }
    let a1 = 0.5 * gy / gx;
    if a < a1 {
        0.5 * (gx + gy) - (2.0 * gx * gy * a).sqrt()
    } else if a < 1.0 - a1 {
        (0.5 - a) * gx
    } else {
        -0.5 * (gx + gy) + (2.0 * gx * gy * (1.0 - a)).sqrt()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn filled_rect() {
        let (w, h, pad) = (40u32, 30u32, 8u32);
        let sdf = sdf_from_mask(&vec![255u8; (w * h) as usize], w, h, pad);
        let stride = (w + 2 * pad) as usize;
        let at = |x: u32, y: u32| sdf[y as usize * stride + x as usize] as f32 / 255.0;
        let mid_y = pad + h / 2;

        assert_eq!(at(0, 0), 0.0);
        assert_eq!(at(pad + w / 2, mid_y), 1.0);
        // half a pixel inside / outside the left edge
        assert!((at(pad, mid_y) - (0.5 + 0.5 / 16.0)).abs() < 0.01);
        assert!((at(pad - 1, mid_y) - (0.5 - 0.5 / 16.0)).abs() < 0.01);
        // monotonic across the edge
        let row = (0..stride as u32).map(|x| at(x, mid_y)).collect::<Vec<_>>();
        assert!(row.windows(2).take(stride / 2).all(|p| p[0] <= p[1]));
        // outside the rect the distance follows the euclidean distance to the corner
        let corner = at(pad - 4, pad - 3);
        let expected = 0.5 - (4.0f32 - 0.5).hypot(3.0 - 0.5) / 16.0;
        assert!((corner - expected).abs() < 0.02, "{corner} vs {expected}");
    }

    #[test]
    fn antialiased_edge_sits_at_half_coverage() {
        // a column of half coverage puts the edge through the pixel centres
        let (w, h, pad) = (6u32, 4u32, 4u32);
        let mut mask = vec![255u8; (w * h) as usize];
        for y in 0..h {
            mask[(y * w) as usize] = 128;
        }
        let sdf = sdf_from_mask(&mask, w, h, pad);
        let stride = (w + 2 * pad) as usize;
        let edge = sdf[(pad + h / 2) as usize * stride + pad as usize] as f32 / 255.0;
        assert!((edge - 0.5).abs() < 0.02, "{edge}");
    }
}
