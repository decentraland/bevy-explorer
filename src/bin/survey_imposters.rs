//! Random-sample the imposter cache and report stats comparing the current
//! full-pack palette indexing scheme against a proposed "split color from
//! geometry" scheme that indexes only the lower 32-bit color half and writes
//! the upper 32-bit normal/depth half directly per-pixel.
//!
//! Run with:
//!   cargo run --bin survey_imposters --release -- [--cache-path <path>] \
//!       [--realm <realm-url>] [--n <samples>] [--seed <seed>]
//!
//! Reports per-imposter unique counts and aggregate uncompressed bytes/pixel.

use std::{
    collections::HashSet,
    fs,
    io::{Cursor, Read as _},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use rand::{seq::SliceRandom, SeedableRng};

struct Stats {
    level: u32,
    file_size: u64,
    total_pixels: usize,
    indexed_orig: bool,
    unique_full: usize,
    unique_color: usize,
    unique_color_rgba_only: usize,
    unique_nd_24bit_normal: usize,
    unique_nd_16bit_normal: usize,
    unique_color_normal_24bit: usize,
    unique_color_normal_16bit: usize,
    quant: Option<QuantQuality>,
    quant_256: Option<QuantQuality>,
}

#[allow(dead_code)] // p999 fields kept for ad-hoc analysis runs
struct QuantQuality {
    palette_used: usize,
    rgb_rmse: f64, // pixel-weighted, 0-255 RGB euclidean
    rgb_p99: f32,
    rgb_p999: f32,
    normal_rmse_deg: f64,
    normal_p99_deg: f32,
    normal_p999_deg: f32,
    depth_rmse_8bit: f64,
    depth_p99_8bit: f32,
}

type SchemeFn = Box<dyn Fn(&Stats) -> f64>;

fn main() -> Result<()> {
    let mut args = pico_args::Arguments::from_env();
    let cache_path: Option<PathBuf> = args.opt_value_from_str("--cache-path")?;
    let realm: Option<String> = args.opt_value_from_str("--realm")?;
    let n_per_level: usize = args.opt_value_from_str("--per-level")?.unwrap_or(150);
    let seed: u64 = args.opt_value_from_str("--seed")?.unwrap_or(0);
    let remaining = args.finish();
    if !remaining.is_empty() {
        anyhow::bail!("unrecognised args: {remaining:?}");
    }

    let cache_path = cache_path
        .or_else(|| platform::project_directories().map(|d| d.data_local_dir().join("cache")))
        .context("no --cache-path and no platform cache dir")?;

    let mut root = cache_path.join("imposters").join("realms");
    if let Some(r) = realm.as_ref() {
        root = root.join(urlencoding::encode(r).into_owned());
    }

    let mut paths = Vec::new();
    walk(&root, &mut paths)?;
    println!(
        "found {} .boimp files under {}",
        paths.len(),
        root.display()
    );

    // Bucket by mip level (extracted from the path: .../realms/<id>/<level>/<x>,<y>...).
    let mut by_level: std::collections::BTreeMap<u32, Vec<PathBuf>> = Default::default();
    for p in paths {
        if let Some(level) = level_of(&p) {
            by_level.entry(level).or_default().push(p);
        }
    }
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let mut sample: Vec<(u32, PathBuf)> = Vec::new();
    for (level, mut ps) in by_level {
        let count = ps.len();
        ps.shuffle(&mut rng);
        let take = ps.len().min(n_per_level);
        println!("  level {level}: {count} files, sampling {take}");
        for p in ps.into_iter().take(take) {
            sample.push((level, p));
        }
    }
    let total = sample.len();
    println!("total sample: {total}");

    let mut all = Vec::new();
    for (i, (level, p)) in sample.iter().enumerate() {
        if i % 50 == 0 {
            eprint!("\r...{i}/{total}");
        }
        match analyse(p, *level) {
            Ok(s) => all.push(s),
            Err(e) => eprintln!("\nskip {p:?}: {e}"),
        }
    }
    eprintln!();

    report(&all);
    Ok(())
}

fn level_of(p: &Path) -> Option<u32> {
    // .../realms/<id>/<level>/<x>,<y>...boimp — level is the immediate parent dir.
    p.parent()
        .and_then(|d| d.file_name())
        .and_then(|n| n.to_str())
        .and_then(|s| s.parse().ok())
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let e = entry?;
        let p = e.path();
        if p.is_dir() {
            walk(&p, out)?;
        } else if p.extension().and_then(|s| s.to_str()) == Some("boimp") {
            out.push(p);
        }
    }
    Ok(())
}

fn analyse(path: &Path, level: u32) -> Result<Stats> {
    let file_size = fs::metadata(path)?.len();
    let bytes = fs::read(path)?;
    let mut zip = zip::ZipArchive::new(Cursor::new(&bytes[..]))?;

    let mut settings = String::new();
    zip.by_name("settings.txt")?.read_to_string(&mut settings)?;
    let parts: Vec<&str> = settings.split(' ').collect();
    if parts.len() < 8 {
        anyhow::bail!("bad settings: {settings}");
    }
    let grid_size: u32 = parts[0].parse()?;
    let packed_size_x: u32 = parts[6].parse()?;
    let packed_size_y: u32 = parts[7].parse()?;
    let total_w = (packed_size_x * grid_size) as usize;
    let total_h = (packed_size_y * grid_size) as usize;

    let names: Vec<String> = zip.file_names().map(String::from).collect();
    let indexed = names.iter().any(|n| n == "pixels.png");

    let packs: Vec<[u8; 8]> = if indexed {
        let mut palette_png = Vec::new();
        zip.by_name("pixels.png")?.read_to_end(&mut palette_png)?;
        let palette = image::load_from_memory_with_format(&palette_png, image::ImageFormat::Png)?
            .into_rgba8();
        let palette_w_rgba8 = palette.width() as usize;
        let palette_h = palette.height() as usize;
        let palette_bytes = palette.into_raw();
        let palette_entries: Vec<[u8; 8]> = palette_bytes
            .chunks_exact(8)
            .map(|c| c.try_into().unwrap())
            .collect();

        let pixels_x = palette_w_rgba8 / 2;
        let pixels_y = palette_h;
        let use_u16 = pixels_x * pixels_y < 65536;

        let mut indices_png = Vec::new();
        zip.by_name("indices.png")?.read_to_end(&mut indices_png)?;
        let indices_img =
            image::load_from_memory_with_format(&indices_png, image::ImageFormat::Png)?
                .into_rgba8();
        let indices_w_rgba8 = indices_img.width() as usize;
        let indices_bytes = indices_img.into_raw();

        let mut packs = Vec::with_capacity(total_w * total_h);
        for row in 0..total_h {
            for col in 0..total_w {
                let idx = if use_u16 {
                    let off = (row * indices_w_rgba8 + col / 2) * 4 + (col % 2) * 2;
                    u16::from_le_bytes(indices_bytes[off..off + 2].try_into().unwrap()) as usize
                } else {
                    let off = (row * indices_w_rgba8 + col) * 4;
                    u32::from_le_bytes(indices_bytes[off..off + 4].try_into().unwrap()) as usize
                };
                packs.push(palette_entries[idx]);
            }
        }
        packs
    } else {
        let mut tex_png = Vec::new();
        zip.by_name("texture.png")?.read_to_end(&mut tex_png)?;
        let img =
            image::load_from_memory_with_format(&tex_png, image::ImageFormat::Png)?.into_rgba8();
        let raw = img.into_raw();
        raw.chunks_exact(8).map(|c| c.try_into().unwrap()).collect()
    };

    let mut full: HashSet<[u8; 8]> = HashSet::new();
    let mut color: HashSet<u32> = HashSet::new();
    let mut color_rgba: HashSet<u32> = HashSet::new();
    let mut nd24: HashSet<u32> = HashSet::new();
    let mut nd16: HashSet<u32> = HashSet::new();
    let mut cn24: HashSet<u64> = HashSet::new();
    let mut cn16: HashSet<u64> = HashSet::new();
    for p in &packs {
        full.insert(*p);
        let c = u32::from_le_bytes(p[0..4].try_into().unwrap());
        let nd = u32::from_le_bytes(p[4..8].try_into().unwrap());
        color.insert(c);
        // Bits 0-19 hold RGBA5555. Drop roughness/metallic/flags to see how
        // much of color uniqueness comes from material vs RGBA alone.
        color_rgba.insert(c & 0xF_FFFF);
        nd24.insert(nd);
        // Reduce normal x/y from 12 bits each to 8 bits each.
        let nx = (nd & 0xFFF) >> 4;
        let ny = ((nd >> 12) & 0xFFF) >> 4;
        let d = (nd >> 24) & 0xFF;
        nd16.insert(nx | (ny << 8) | (d << 16));
        // (color, normal) key — depth pulled out. 24-bit and 16-bit normal
        // variants. nd & 0x00FF_FFFF strips depth (bits 24-31 of nd).
        cn24.insert((c as u64) | (((nd & 0x00FF_FFFF) as u64) << 32));
        let n16 = nx | (ny << 8);
        cn16.insert((c as u64) | ((n16 as u64) << 32));
    }

    // Quantization-quality simulation at the two palette sizes we care about:
    //  - 4096 entries (12-bit idx, the always-applicable scheme).
    //  - 256 entries (8-bit idx, only feasible when its own RMSE is below a
    //    threshold — measured per-imposter so the threshold gate operates on
    //    actual quantized quality, not a unique-count proxy).
    let quant = compute_quantization(&packs, 4096, /* include_depth */ true);
    let quant_256 = compute_quantization(&packs, 256, true);

    Ok(Stats {
        level,
        file_size,
        total_pixels: packs.len(),
        indexed_orig: indexed,
        unique_full: full.len(),
        unique_color: color.len(),
        unique_color_rgba_only: color_rgba.len(),
        unique_nd_24bit_normal: nd24.len(),
        unique_nd_16bit_normal: nd16.len(),
        unique_color_normal_24bit: cn24.len(),
        unique_color_normal_16bit: cn16.len(),
        quant,
        quant_256,
    })
}

#[derive(Clone)]
struct Point {
    coords: Vec<f32>, // 8 (no depth) or 9 (with depth) dims
    count: u32,
    rgb8: [f32; 3],
    normal: [f32; 3],
    depth8: f32, // original depth bits scaled to [0, 255]
}

fn unpack_point(key: [u8; 8], count: u32, include_depth: bool) -> Point {
    let lower = u32::from_le_bytes(key[0..4].try_into().unwrap());
    let upper = u32::from_le_bytes(key[4..8].try_into().unwrap());
    let r = (lower & 0x1F) as f32 / 31.0;
    let g = ((lower >> 5) & 0x1F) as f32 / 31.0;
    let b = ((lower >> 10) & 0x1F) as f32 / 31.0;
    let a = ((lower >> 15) & 0x1F) as f32 / 31.0;
    let rough = ((lower >> 20) & 0xF) as f32 / 15.0;
    let metal = ((lower >> 24) & 0xF) as f32 / 15.0;
    let nx = (upper & 0xFFF) as f32 / 4095.0;
    let ny = ((upper >> 12) & 0xFFF) as f32 / 4095.0;
    let depth = ((upper >> 24) & 0xFF) as f32 / 255.0;
    let normal = uv_to_normal(nx, ny);
    let mut coords = vec![r, g, b, a, rough, metal, nx, ny];
    if include_depth {
        coords.push(depth);
    }
    Point {
        coords,
        count,
        rgb8: [r * 255.0, g * 255.0, b * 255.0],
        normal,
        depth8: depth * 255.0,
    }
}

// Mirrors spherical_normal_from_uv in boimp/src/shaders/shared.wgsl.
fn uv_to_normal(nx_q: f32, ny_q: f32) -> [f32; 3] {
    let x = nx_q * 2.0 - 1.0;
    let z = ny_q * 2.0 - 1.0;
    let y = 1.0 - x.abs() - z.abs();
    let (nx, ny, nz) = if y < 0.0 {
        (
            x.signum() * (1.0 - z.abs()),
            y,
            z.signum() * (1.0 - x.abs()),
        )
    } else {
        (x, y, z)
    };
    let len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-9);
    [nx / len, ny / len, nz / len]
}

// Recursive weighted-median-cut. Each leaf bucket is assigned a unique id in
// bucket_of (indexed by point position in `points`). Splits along the
// widest-range dimension, by weighted median of pixel counts so that each
// bucket holds roughly the same number of rendered pixels.
fn median_cut(
    points: &[Point],
    indices: &mut [usize],
    bucket_of: &mut [usize],
    next_id: &mut usize,
    k: usize,
) {
    if k <= 1 || indices.len() <= 1 {
        let id = *next_id;
        *next_id += 1;
        for &i in indices.iter() {
            bucket_of[i] = id;
        }
        return;
    }

    let dims = points[indices[0]].coords.len();
    let mut best_dim = 0usize;
    let mut best_spread = 0.0f32;
    for d in 0..dims {
        let mut lo = f32::INFINITY;
        let mut hi = f32::NEG_INFINITY;
        for &i in indices.iter() {
            let v = points[i].coords[d];
            if v < lo {
                lo = v;
            }
            if v > hi {
                hi = v;
            }
        }
        let spread = hi - lo;
        if spread > best_spread {
            best_spread = spread;
            best_dim = d;
        }
    }
    if best_spread == 0.0 {
        let id = *next_id;
        *next_id += 1;
        for &i in indices.iter() {
            bucket_of[i] = id;
        }
        return;
    }

    indices.sort_by(|&a, &b| {
        points[a].coords[best_dim]
            .partial_cmp(&points[b].coords[best_dim])
            .unwrap()
    });

    // Weighted-median split by pixel count.
    let total_w: u64 = indices.iter().map(|&i| points[i].count as u64).sum();
    let mut running: u64 = 0;
    let mut split_at = indices.len() / 2;
    for (pos, &i) in indices.iter().enumerate() {
        running += points[i].count as u64;
        if running * 2 >= total_w {
            split_at = (pos + 1).clamp(1, indices.len() - 1);
            break;
        }
    }
    let (left, right) = indices.split_at_mut(split_at);
    let k_left = k / 2;
    let k_right = k - k_left;
    median_cut(points, left, bucket_of, next_id, k_left);
    median_cut(points, right, bucket_of, next_id, k_right);
}

fn compute_quantization(packs: &[[u8; 8]], k: usize, include_depth: bool) -> Option<QuantQuality> {
    if packs.is_empty() {
        return None;
    }
    // Aggregate unique keys with counts.
    let mut counts: std::collections::HashMap<[u8; 8], u32> = std::collections::HashMap::new();
    for p in packs {
        *counts.entry(*p).or_insert(0) += 1;
    }
    let key_list: Vec<[u8; 8]> = counts.keys().copied().collect();
    let points: Vec<Point> = key_list
        .iter()
        .map(|k| unpack_point(*k, *counts.get(k).unwrap(), include_depth))
        .collect();

    // Median-cut.
    let mut indices: Vec<usize> = (0..points.len()).collect();
    let mut bucket_of = vec![0usize; points.len()];
    let mut next_id = 0usize;
    median_cut(&points, &mut indices, &mut bucket_of, &mut next_id, k);
    let palette_used = next_id;

    // Compute weighted centroids per bucket. Centroid in N-D space (or its
    // averaged unit normal, re-normalised, for angular error) plus depth.
    let mut nsums: Vec<[f64; 3]> = vec![[0.0; 3]; palette_used];
    let mut rgb8_sums: Vec<[f64; 3]> = vec![[0.0; 3]; palette_used];
    let mut depth_sums: Vec<f64> = vec![0.0; palette_used];
    let mut wsums: Vec<u64> = vec![0u64; palette_used];
    for (pi, p) in points.iter().enumerate() {
        let b = bucket_of[pi];
        for d in 0..3 {
            nsums[b][d] += p.normal[d] as f64 * p.count as f64;
            rgb8_sums[b][d] += p.rgb8[d] as f64 * p.count as f64;
        }
        depth_sums[b] += p.depth8 as f64 * p.count as f64;
        wsums[b] += p.count as u64;
    }
    let centroids_rgb8: Vec<[f32; 3]> = (0..palette_used)
        .map(|b| {
            let w = wsums[b].max(1) as f64;
            [
                (rgb8_sums[b][0] / w) as f32,
                (rgb8_sums[b][1] / w) as f32,
                (rgb8_sums[b][2] / w) as f32,
            ]
        })
        .collect();
    let centroids_normal: Vec<[f32; 3]> = (0..palette_used)
        .map(|b| {
            let w = wsums[b].max(1) as f64;
            let n = [
                (nsums[b][0] / w) as f32,
                (nsums[b][1] / w) as f32,
                (nsums[b][2] / w) as f32,
            ];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
            [n[0] / len, n[1] / len, n[2] / len]
        })
        .collect();
    let centroids_depth: Vec<f32> = (0..palette_used)
        .map(|b| (depth_sums[b] / wsums[b].max(1) as f64) as f32)
        .collect();

    // For each unique key, look up its (rgb8 error, angular error). Then
    // iterate packs to weight by pixel count (avoids materialising a per-pixel
    // error vector at 1M entries).
    let key_to_idx: std::collections::HashMap<[u8; 8], usize> =
        key_list.iter().enumerate().map(|(i, k)| (*k, i)).collect();
    let rgb_err: Vec<f32> = points
        .iter()
        .enumerate()
        .map(|(pi, p)| {
            let c = centroids_rgb8[bucket_of[pi]];
            let dr = p.rgb8[0] - c[0];
            let dg = p.rgb8[1] - c[1];
            let db = p.rgb8[2] - c[2];
            (dr * dr + dg * dg + db * db).sqrt()
        })
        .collect();
    let normal_err_deg: Vec<f32> = points
        .iter()
        .enumerate()
        .map(|(pi, p)| {
            let c = centroids_normal[bucket_of[pi]];
            let dot =
                (p.normal[0] * c[0] + p.normal[1] * c[1] + p.normal[2] * c[2]).clamp(-1.0, 1.0);
            dot.acos().to_degrees()
        })
        .collect();
    let depth_err: Vec<f32> = points
        .iter()
        .enumerate()
        .map(|(pi, p)| (p.depth8 - centroids_depth[bucket_of[pi]]).abs())
        .collect();

    // Pixel-weighted RMSE and percentiles.
    let mut rgb_sq_sum: f64 = 0.0;
    let mut nrm_sq_sum: f64 = 0.0;
    let mut dpt_sq_sum: f64 = 0.0;
    let total: u64 = packs.len() as u64;
    let mut rgb_pairs: Vec<(f32, u64)> = (0..points.len())
        .map(|i| (rgb_err[i], points[i].count as u64))
        .collect();
    let mut nrm_pairs: Vec<(f32, u64)> = (0..points.len())
        .map(|i| (normal_err_deg[i], points[i].count as u64))
        .collect();
    let mut dpt_pairs: Vec<(f32, u64)> = (0..points.len())
        .map(|i| (depth_err[i], points[i].count as u64))
        .collect();
    for p in packs {
        let ki = key_to_idx[p];
        let r = rgb_err[ki] as f64;
        let n = normal_err_deg[ki] as f64;
        let d = depth_err[ki] as f64;
        rgb_sq_sum += r * r;
        nrm_sq_sum += n * n;
        dpt_sq_sum += d * d;
    }
    let rgb_rmse = (rgb_sq_sum / total as f64).sqrt();
    let normal_rmse_deg = (nrm_sq_sum / total as f64).sqrt();
    let depth_rmse_8bit = (dpt_sq_sum / total as f64).sqrt();
    rgb_pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    nrm_pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    dpt_pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let p99 = |pairs: &[(f32, u64)], p: f64| -> f32 {
        let target = (total as f64 * p) as u64;
        let mut running = 0u64;
        for (v, w) in pairs {
            running += *w;
            if running >= target {
                return *v;
            }
        }
        pairs.last().map(|x| x.0).unwrap_or(0.0)
    };
    Some(QuantQuality {
        palette_used,
        rgb_rmse,
        rgb_p99: p99(&rgb_pairs, 0.99),
        rgb_p999: p99(&rgb_pairs, 0.999),
        normal_rmse_deg,
        normal_p99_deg: p99(&nrm_pairs, 0.99),
        normal_p999_deg: p99(&nrm_pairs, 0.999),
        depth_rmse_8bit,
        depth_p99_8bit: p99(&dpt_pairs, 0.99),
    })
}

fn report(stats: &[Stats]) {
    if stats.is_empty() {
        println!("no imposters decoded");
        return;
    }
    let n = stats.len();
    println!("\n== Survey: {n} imposters ==");

    // Per-level summary first. Each row: mean unique counts, mean total pixels,
    // pixel-weighted current bpp, pixel-weighted idx12-variable bpp, RGB and
    // normal quantization RMSE (when applicable).
    println!("\n-- by mip level (the shape of the corpus per-level) --");
    let mut by_level: std::collections::BTreeMap<u32, Vec<&Stats>> = Default::default();
    for s in stats {
        by_level.entry(s.level).or_default().push(s);
    }
    println!(
        "  {:>5}  {:>5}  {:>12}  {:>12}  {:>9}  {:>8}  {:>8}  {:>8}  {:>8}",
        "level",
        "n",
        "px (med)",
        "px (max)",
        "uniq_p50",
        "uniq_p90",
        "cur bpp",
        "new bpp",
        "RGB RMSE"
    );
    for (level, group) in by_level.iter() {
        let mut px: Vec<usize> = group.iter().map(|s| s.total_pixels).collect();
        px.sort_unstable();
        let px_med = px[(px.len() - 1) / 2];
        let px_max = *px.last().unwrap();

        let mut uniq: Vec<usize> = group.iter().map(|s| s.unique_full).collect();
        uniq.sort_unstable();
        let uniq_p50 = uniq[(uniq.len() - 1) / 2];
        let uniq_p90 = uniq[((uniq.len() - 1) * 9 / 10).min(uniq.len() - 1)];

        let total_px: f64 = group.iter().map(|s| s.total_pixels as f64).sum();
        let cur_bytes: f64 = group
            .iter()
            .map(|s| bpp_current_full_pack(s) * s.total_pixels as f64)
            .sum();
        let new_bytes: f64 = group
            .iter()
            .map(|s| bpp_idx12_variable(s) * s.total_pixels as f64)
            .sum();
        let cur_bpp = cur_bytes / total_px;
        let new_bpp = new_bytes / total_px;

        let rgb_rmse_corpus = (group
            .iter()
            .filter_map(|s| s.quant.as_ref().map(|q| (q, s.total_pixels)))
            .map(|(q, p)| q.rgb_rmse * q.rgb_rmse * p as f64)
            .sum::<f64>()
            / total_px)
            .sqrt();

        println!(
            "  {:>5}  {:>5}  {:>12}  {:>12}  {:>9}  {:>8}  {:>8.2}  {:>8.2}  {:>8.2}",
            level,
            group.len(),
            px_med,
            px_max,
            uniq_p50,
            uniq_p90,
            cur_bpp,
            new_bpp,
            rgb_rmse_corpus
        );
    }

    let total_pixels: u64 = stats.iter().map(|s| s.total_pixels as u64).sum();
    let total_file: u64 = stats.iter().map(|s| s.file_size).sum();
    println!("total post-shrink pixels: {total_pixels}");
    println!(
        "total on-disk zip bytes:  {total_file} ({:.2} bpp on disk, mean)",
        total_file as f64 * 8.0 / total_pixels as f64
    );
    let indexed_count = stats.iter().filter(|s| s.indexed_orig).count();
    println!("current scheme uses indexing: {indexed_count}/{n}");

    println!("\n-- per-imposter unique counts --");
    for (label, get) in [
        (
            "unique_full (current key, 64-bit)",
            (|s: &Stats| s.unique_full) as fn(&Stats) -> usize,
        ),
        ("unique_color (32-bit lower)", |s| s.unique_color),
        ("unique_color (RGBA only, 20-bit)", |s| {
            s.unique_color_rgba_only
        }),
        ("unique_normal+depth (32-bit, 24-bit normal)", |s| {
            s.unique_nd_24bit_normal
        }),
        ("unique_normal+depth (24-bit, 16-bit normal)", |s| {
            s.unique_nd_16bit_normal
        }),
        ("unique (color, normal) [depth-pulled, 56-bit]", |s| {
            s.unique_color_normal_24bit
        }),
        ("unique (color, normal16) [depth-pulled, 48-bit]", |s| {
            s.unique_color_normal_16bit
        }),
    ] {
        let mut vals: Vec<usize> = stats.iter().map(get).collect();
        vals.sort_unstable();
        let p50 = vals[(n - 1) / 2];
        let p90 = vals[(n * 9) / 10];
        let p99 = vals[(n * 99) / 100];
        let max = *vals.last().unwrap();
        println!("  {label:46}  p50={p50:>7}  p90={p90:>7}  p99={p99:>7}  max={max:>7}");
    }

    println!("\n-- palette feasibility (≤65536 entries → fits in u16 idx) --");
    for (label, get) in [
        (
            "unique_full (current key)",
            (|s: &Stats| s.unique_full) as fn(&Stats) -> usize,
        ),
        ("unique_color", |s| s.unique_color),
        ("unique (color, normal24) — depth-pulled", |s| {
            s.unique_color_normal_24bit
        }),
        (
            "unique (color, normal16) — depth-pulled, 16-bit normal",
            |s| s.unique_color_normal_16bit,
        ),
    ] {
        let fit_256 = stats.iter().filter(|s| get(s) <= 256).count();
        let fit_64k = stats.iter().filter(|s| get(s) <= 65536).count();
        println!("  {label:54}  ≤256: {fit_256:>4}/{n}  ≤65536: {fit_64k:>4}/{n}");
    }

    println!("\n-- imposter size distribution (post-shrink pixels) --");
    {
        let mut sizes: Vec<usize> = stats.iter().map(|s| s.total_pixels).collect();
        sizes.sort_unstable();
        let p50 = sizes[(n - 1) / 2];
        let p90 = sizes[(n * 9) / 10];
        let p99 = sizes[(n * 99) / 100];
        let max = *sizes.last().unwrap();
        println!("  total_pixels    p50={p50:>7}  p90={p90:>7}  p99={p99:>7}  max={max:>7}");
    }

    // Schemes to compare. Each yields per-pixel BYTES; we compute both the
    // pixel-weighted "fleet" mean (sum_bytes / sum_pixels) and the unweighted
    // per-imposter mean. Pixel-weighted is the one that matters for "how
    // much do total VRAM and downloads shrink across the corpus", because a
    // 4 KB palette amortises differently across a 1 K-pixel vs 1 M-pixel
    // imposter.
    let schemes: [(&str, SchemeFn); 14] = [
        (
            "current (full-pack palette indexed)",
            Box::new(bpp_current_full_pack),
        ),
        (
            "depth-pulled, lossless, 24-bit normal",
            Box::new(|s| bpp_depth_pulled(s, false)),
        ),
        (
            "depth-pulled, lossless, 16-bit normal",
            Box::new(|s| bpp_depth_pulled(s, true)),
        ),
        (
            "forced quantized: idx10+d6 (16-bit slot, 1024 pal × 4 B)",
            Box::new(|s| bpp_forced_index(s, 1024, 10, 6, 4)),
        ),
        (
            "forced quantized: idx8 +d8 (16-bit slot, 256 pal × 4 B)",
            Box::new(|s| bpp_forced_index(s, 256, 8, 8, 4)),
        ),
        (
            "forced quantized: idx12+d4 (16-bit slot, 4096 pal × 4 B)",
            Box::new(|s| bpp_forced_index(s, 4096, 12, 4, 4)),
        ),
        (
            "idx12 packed, depth-in-palette (1.5 B/px, 4096 pal × 8 B)",
            Box::new(|s| bpp_packed_index(s, 4096, 12, 8)),
        ),
        (
            "idx12 packed, depth-in-palette (1.5 B/px, 4096 pal × 4 B)",
            Box::new(|s| bpp_packed_index(s, 4096, 12, 4)),
        ),
        (
            "idx12 R16Uint slot, depth-in-pal (2 B/px, 4096 pal × 8 B)",
            Box::new(|s| bpp_packed_index(s, 4096, 16, 8)),
        ),
        (
            "idx12 packed, variable palette (lossless ≤4096 entries × 8 B)",
            Box::new(bpp_idx12_variable),
        ),
        (
            "hybrid: idx8 (≤256 unique) / idx12 (else), variable palette",
            Box::new(bpp_hybrid_idx8_idx12),
        ),
        (
            "hybrid: idx8 if 256-entry RGB RMSE < 1.0, else idx12",
            Box::new(|s| bpp_hybrid_threshold(s, 1.0)),
        ),
        (
            "hybrid: idx8 if 256-entry RGB RMSE < 2.0, else idx12",
            Box::new(|s| bpp_hybrid_threshold(s, 2.0)),
        ),
        (
            "hybrid: idx8 if 256-entry RGB RMSE < 5.0, else idx12",
            Box::new(|s| bpp_hybrid_threshold(s, 5.0)),
        ),
    ];

    let total_pixels_f = total_pixels as f64;
    println!("\n-- current scheme breakdown (which regime each imposter is in) --");
    let mut cat_pixels = [0u64; 3]; // [u16, u32, raw]
    let mut cat_bytes = [0u64; 3];
    let mut cat_count = [0usize; 3];
    let labels = ["u16-indexed (≤65536 unique)", "u32-indexed", "raw fallback"];
    for s in stats {
        let palette = s.unique_full * 8;
        let use_u16 = s.unique_full < 65536;
        let idx = s.total_pixels * if use_u16 { 2 } else { 4 };
        let indexed = (palette + idx) as u64;
        let raw = (s.total_pixels * 8) as u64;
        let (cat, bytes) = if indexed >= raw {
            (2, raw) // raw fallback
        } else if use_u16 {
            (0, indexed)
        } else {
            (1, indexed)
        };
        cat_pixels[cat] += s.total_pixels as u64;
        cat_bytes[cat] += bytes;
        cat_count[cat] += 1;
    }
    for c in 0..3 {
        if cat_count[c] == 0 {
            continue;
        }
        let bpp = if cat_pixels[c] > 0 {
            cat_bytes[c] as f64 / cat_pixels[c] as f64
        } else {
            0.0
        };
        let px_pct = 100.0 * cat_pixels[c] as f64 / total_pixels_f;
        let cnt_pct = 100.0 * cat_count[c] as f64 / n as f64;
        println!(
            "  {:30}  imposters={:>4}/{} ({:.1}%)  pixels={:.1}% of corpus  weighted={:>5.2} bpp",
            labels[c], cat_count[c], n, cnt_pct, px_pct, bpp,
        );
    }

    println!("\n-- VRAM-equivalent bytes/pixel --");
    println!("  raw Rg32Uint                                               weighted= 8.00 bpp");
    let raw_total = total_pixels_f * 8.0;
    let mut current_total = 0.0_f64;
    for (label, scheme) in &schemes {
        let weighted_sum: f64 = stats
            .iter()
            .map(|s| scheme(s) * s.total_pixels as f64)
            .sum();
        let weighted = weighted_sum / total_pixels_f;
        let unweighted = mean(stats.iter().map(scheme.as_ref()));
        println!("  {label:60}  weighted={weighted:>5.2}  unweighted={unweighted:>6.2}");
        if label.starts_with("current") {
            current_total = weighted_sum;
        }
    }

    // Hybrid floor: per-imposter min over a chosen subset (current vs each
    // proposed). Bytes saved is real corpus-wide reduction.
    println!("\n-- per-imposter hybrid (pick the smallest scheme per imposter) --");
    let schemes_for_hybrid: Vec<(&str, SchemeFn)> = vec![
        ("current", Box::new(bpp_current_full_pack)),
        ("depth-pulled n16", Box::new(|s| bpp_depth_pulled(s, true))),
        (
            "forced idx10+d6 (1024 pal)",
            Box::new(|s| bpp_forced_index(s, 1024, 10, 6, 4)),
        ),
    ];
    let mut total_hybrid = 0.0_f64;
    let mut counts: std::collections::HashMap<&str, usize> = Default::default();
    for s in stats {
        let mut best = f64::INFINITY;
        let mut winner = "";
        for (name, scheme) in &schemes_for_hybrid {
            let bpp = scheme(s);
            if bpp < best {
                best = bpp;
                winner = name;
            }
        }
        total_hybrid += best * s.total_pixels as f64;
        *counts.entry(winner).or_insert(0) += 1;
    }
    println!(
        "  weighted bpp: {:.2}  ({:.1}% reduction vs current, {:.1}% vs raw)",
        total_hybrid / total_pixels_f,
        100.0 * (current_total - total_hybrid) / current_total,
        100.0 * (raw_total - total_hybrid) / raw_total,
    );
    for (name, count) in counts {
        println!("    won by {name}: {count}/{n}");
    }

    // Threshold-gated hybrid: per imposter, choose idx8 (256 entries, 1 B/px)
    // or idx12 (variable ≤4096 entries, 1.5 B/px) based on whether the
    // 256-entry RGB RMSE is below threshold. Report bpp + effective corpus
    // RMSE so threshold can be picked from the front of the quality curve.
    println!("\n-- threshold-gated hybrid sweep --");
    println!(
        "  {:>7}  {:>9}  {:>9}  {:>11}  {:>11}  {:>11}",
        "thresh", "idx8 ct", "idx12 ct", "bpp", "RGB RMSE", "normal RMSE"
    );
    for threshold in [0.001, 1.0, 2.0, 3.0, 5.0, 10.0, 20.0] {
        let mut idx8_count = 0usize;
        let mut idx12_count = 0usize;
        let mut total_bytes = 0.0_f64;
        let mut total_px = 0.0_f64;
        let mut rgb_sq_sum = 0.0_f64;
        let mut nrm_sq_sum = 0.0_f64;
        for s in stats {
            let q256_rmse = s
                .quant_256
                .as_ref()
                .map(|q| q.rgb_rmse)
                .unwrap_or(f64::INFINITY);
            let px = s.total_pixels as f64;
            total_px += px;
            if q256_rmse < threshold {
                idx8_count += 1;
                total_bytes += bpp_idx8_lossless(s) * px;
                if let Some(q) = &s.quant_256 {
                    rgb_sq_sum += q.rgb_rmse * q.rgb_rmse * px;
                    nrm_sq_sum += q.normal_rmse_deg * q.normal_rmse_deg * px;
                }
            } else {
                idx12_count += 1;
                total_bytes += bpp_idx12_variable(s) * px;
                if let Some(q) = &s.quant {
                    rgb_sq_sum += q.rgb_rmse * q.rgb_rmse * px;
                    nrm_sq_sum += q.normal_rmse_deg * q.normal_rmse_deg * px;
                }
            }
        }
        let bpp = total_bytes / total_px;
        let rgb_rmse = (rgb_sq_sum / total_px).sqrt();
        let normal_rmse = (nrm_sq_sum / total_px).sqrt();
        println!(
            "  {:>7.3}  {:>9}  {:>9}  {:>11.3}  {:>11.2}  {:>11.2}",
            threshold, idx8_count, idx12_count, bpp, rgb_rmse, normal_rmse
        );
    }

    println!("\n-- 4096-entry palette quantization quality (depth in palette) --");
    // Two views: corpus-weighted (treats each pixel equally) and per-imposter
    // (each imposter equally, useful for spotting bad cases).
    let with_quant: Vec<&Stats> = stats.iter().filter(|s| s.quant.is_some()).collect();
    let m = with_quant.len();
    if m == 0 {
        println!("  no quantization data");
    } else {
        // Pixel-weighted (corpus) — sum (rmse² × pixels) then sqrt / total.
        let total_px: f64 = with_quant.iter().map(|s| s.total_pixels as f64).sum();
        let rgb_rmse_corpus = (with_quant
            .iter()
            .map(|s| {
                let q = s.quant.as_ref().unwrap();
                q.rgb_rmse * q.rgb_rmse * s.total_pixels as f64
            })
            .sum::<f64>()
            / total_px)
            .sqrt();
        let normal_rmse_corpus = (with_quant
            .iter()
            .map(|s| {
                let q = s.quant.as_ref().unwrap();
                q.normal_rmse_deg * q.normal_rmse_deg * s.total_pixels as f64
            })
            .sum::<f64>()
            / total_px)
            .sqrt();
        println!(
            "  corpus pixel-weighted RMSE — RGB (0-255 euclidean): {:.2}, normal: {:.2}°",
            rgb_rmse_corpus, normal_rmse_corpus
        );

        // Distribution of per-imposter RMSE.
        let mut rgb_rmses: Vec<f64> = with_quant
            .iter()
            .map(|s| s.quant.as_ref().unwrap().rgb_rmse)
            .collect();
        rgb_rmses.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut normal_rmses: Vec<f64> = with_quant
            .iter()
            .map(|s| s.quant.as_ref().unwrap().normal_rmse_deg)
            .collect();
        normal_rmses.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let pct = |v: &[f64], p: f64| v[((v.len() - 1) as f64 * p) as usize];
        println!(
            "  per-imposter RGB RMSE        p50={:.2}  p90={:.2}  p99={:.2}  max={:.2}",
            pct(&rgb_rmses, 0.5),
            pct(&rgb_rmses, 0.9),
            pct(&rgb_rmses, 0.99),
            rgb_rmses.last().copied().unwrap_or(0.0)
        );
        println!(
            "  per-imposter normal RMSE (°) p50={:.2}  p90={:.2}  p99={:.2}  max={:.2}",
            pct(&normal_rmses, 0.5),
            pct(&normal_rmses, 0.9),
            pct(&normal_rmses, 0.99),
            normal_rmses.last().copied().unwrap_or(0.0)
        );

        // p99/p999 per-pixel error across the corpus (worst-case per-pixel
        // observation, weighted by pixel count over the sample).
        let mut all_rgb_p99: Vec<f32> = with_quant
            .iter()
            .map(|s| s.quant.as_ref().unwrap().rgb_p99)
            .collect();
        all_rgb_p99.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut all_normal_p99: Vec<f32> = with_quant
            .iter()
            .map(|s| s.quant.as_ref().unwrap().normal_p99_deg)
            .collect();
        all_normal_p99.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let pctf = |v: &[f32], p: f64| v[((v.len() - 1) as f64 * p) as usize];
        println!(
            "  per-imposter p99 pixel-error RGB     median={:.2}  p99 imposter={:.2}  max={:.2}",
            pctf(&all_rgb_p99, 0.5),
            pctf(&all_rgb_p99, 0.99),
            all_rgb_p99.last().copied().unwrap_or(0.0)
        );
        println!(
            "  per-imposter p99 pixel-error normal  median={:.2}°  p99 imposter={:.2}°  max={:.2}°",
            pctf(&all_normal_p99, 0.5),
            pctf(&all_normal_p99, 0.99),
            all_normal_p99.last().copied().unwrap_or(0.0)
        );

        // Depth RMSE (8-bit scale).
        let depth_rmse_corpus = (with_quant
            .iter()
            .map(|s| {
                let q = s.quant.as_ref().unwrap();
                q.depth_rmse_8bit * q.depth_rmse_8bit * s.total_pixels as f64
            })
            .sum::<f64>()
            / total_px)
            .sqrt();
        println!(
            "  corpus pixel-weighted depth RMSE (0-255 scale): {:.2}",
            depth_rmse_corpus
        );

        // How many imposters need quantization at all (unique_full > 4096)?
        let need_quant: Vec<&Stats> = with_quant
            .iter()
            .filter(|s| s.unique_full > 4096)
            .copied()
            .collect();
        println!(
            "  imposters needing quantization (unique_full > 4096): {}/{}",
            need_quant.len(),
            m
        );
        if !need_quant.is_empty() {
            let mut rgb_q: Vec<f64> = need_quant
                .iter()
                .map(|s| s.quant.as_ref().unwrap().rgb_rmse)
                .collect();
            rgb_q.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mut nrm_q: Vec<f64> = need_quant
                .iter()
                .map(|s| s.quant.as_ref().unwrap().normal_rmse_deg)
                .collect();
            nrm_q.sort_by(|a, b| a.partial_cmp(b).unwrap());
            println!(
                "    (those only) RGB RMSE    p50={:.2}  p90={:.2}  p99={:.2}  max={:.2}",
                pct(&rgb_q, 0.5),
                pct(&rgb_q, 0.9),
                pct(&rgb_q, 0.99),
                rgb_q.last().copied().unwrap_or(0.0)
            );
            println!(
                "    (those only) normal (°) p50={:.2}  p90={:.2}  p99={:.2}  max={:.2}",
                pct(&nrm_q, 0.5),
                pct(&nrm_q, 0.9),
                pct(&nrm_q, 0.99),
                nrm_q.last().copied().unwrap_or(0.0)
            );
        }
    }
}

fn bpp_current_full_pack(s: &Stats) -> f64 {
    let palette = s.unique_full * 8;
    let use_u16 = s.unique_full < 65536;
    let idx = s.total_pixels * if use_u16 { 2 } else { 4 };
    let indexed = (palette + idx) as f64;
    let raw = (s.total_pixels * 8) as f64;
    f64::min(indexed, raw) / s.total_pixels as f64
}

fn bpp_forced_index(
    s: &Stats,
    palette_size: usize,
    idx_bits: usize,
    depth_bits: usize,
    palette_entry_bytes: usize,
) -> f64 {
    let bits_per_pixel = idx_bits + depth_bits;
    let bytes_per_pixel = bits_per_pixel.div_ceil(8).next_power_of_two();
    let palette_bytes = palette_size * palette_entry_bytes;
    let pixels = s.total_pixels * bytes_per_pixel;
    (palette_bytes + pixels) as f64 / s.total_pixels as f64
}

// Tightly packed bits-per-pixel variant — doesn't round up to a power-of-2
// byte slot, so 12-bit idx = 1.5 B/pixel.
fn bpp_packed_index(
    s: &Stats,
    palette_size: usize,
    bits_per_pixel: usize,
    palette_entry_bytes: usize,
) -> f64 {
    let palette_bytes = (palette_size * palette_entry_bytes) as f64;
    let pixels = s.total_pixels as f64 * (bits_per_pixel as f64 / 8.0);
    (palette_bytes + pixels) / s.total_pixels as f64
}

// idx12 with variable palette size — store only the unique entries the
// imposter actually has, capped at 4096 (quantize beyond that). Per-pixel
// stays 12 bits = 1.5 bytes. Palette = entries × 8 B (lossless content).
fn bpp_idx12_variable(s: &Stats) -> f64 {
    let palette_entries = s.unique_full.min(4096);
    let palette_bytes = palette_entries * 8;
    let pixels = s.total_pixels.div_ceil(2) * 3;
    (palette_bytes + pixels) as f64 / s.total_pixels as f64
}

// Two-format hybrid: when an imposter has ≤256 unique full packs we can
// emit an idx8 variant (1 B/px + small palette) losslessly. Otherwise we
// fall to idx12 with variable palette. Palette content is always 8 B/entry.
fn bpp_hybrid_idx8_idx12(s: &Stats) -> f64 {
    if s.unique_full <= 256 {
        bpp_idx8_lossless(s)
    } else {
        bpp_idx12_variable(s)
    }
}

fn bpp_idx8_lossless(s: &Stats) -> f64 {
    let palette_bytes = s.unique_full.min(256) * 8;
    let pixels = s.total_pixels;
    (palette_bytes + pixels) as f64 / s.total_pixels as f64
}

// Quality-gated hybrid: idx8 chosen when its 256-entry RMSE is below
// `rgb_threshold`. This generalises the "≤256 unique" lossless test —
// quantization can be acceptable if the error stays small even when there
// are more than 256 unique entries. Else fall through to idx12 with variable
// palette (always lossless within its 4096-entry cap).
fn bpp_hybrid_threshold(s: &Stats, rgb_threshold: f64) -> f64 {
    let q256_rmse = s
        .quant_256
        .as_ref()
        .map(|q| q.rgb_rmse)
        .unwrap_or(f64::INFINITY);
    if q256_rmse < rgb_threshold {
        bpp_idx8_lossless(s)
    } else {
        bpp_idx12_variable(s)
    }
}

fn bpp_depth_pulled(s: &Stats, normal_16bit: bool) -> f64 {
    let unique = if normal_16bit {
        s.unique_color_normal_16bit
    } else {
        s.unique_color_normal_24bit
    };
    let idx_bytes = if unique <= 256 {
        1
    } else if unique <= 65536 {
        2
    } else {
        4
    };
    let palette = unique * 8; // Rg32Uint slot, depth bits unused
    let pixels = s.total_pixels * (1 + idx_bytes); // 1 byte depth + idx
    let scheme = (palette + pixels) as f64;
    let raw = (s.total_pixels * 8) as f64;
    f64::min(scheme, raw) / s.total_pixels as f64
}

fn mean(iter: impl Iterator<Item = f64>) -> f64 {
    let (sum, count) = iter.fold((0.0, 0usize), |(s, c), v| (s + v, c + 1));
    if count == 0 {
        0.0
    } else {
        sum / count as f64
    }
}
