//! One-off converter: take the per-scene imposter cache (legacy
//! `imposters/scenes/<hash>/...` layout, where one folder per scene held the
//! body / floor / spec for every parcel that scene covered) and emit per-parcel
//! mip-0 imposters in the standard mip layout
//! (`imposters/realms/<realm>/0/<x>,<y>...`). Optionally writes per-parcel
//! `.zip` packages mirroring the bake-time `--zip-output` shape so the result
//! can be uploaded for distribution without re-baking.
//!
//! Run with:
//!     cargo run --bin convert_scenes -- \
//!         --realm <realm url> \
//!         [--zip-output <path>] \
//!         [--cache-path <path>]
//!
//! `--realm` is mandatory because the on-disk path encodes the realm. The
//! converter does not download or render anything; it only re-arranges the
//! cache and computes the level-0 CRC per parcel (= CRC32_CKSUM of the scene
//! hash, matching `ScenePointers::crc`'s level-0 branch).

use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use bevy::{math::IVec2, platform::collections::HashMap};
use imposters::imposter_spec::{BakedScene, ImposterSpec};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

fn main() -> anyhow::Result<()> {
    let mut args = pico_args::Arguments::from_env();
    let realm: String = args.value_from_str("--realm")?;
    let zip_output: Option<PathBuf> = args.opt_value_from_str("--zip-output")?;
    let cache_path: Option<PathBuf> = args.opt_value_from_str("--cache-path")?;
    let remaining = args.finish();
    if !remaining.is_empty() {
        anyhow::bail!("unrecognised args: {remaining:?}");
    }

    // Mirror `IpfsIoPlugin::build`'s cache-root choice:
    // `data_local_dir().join("cache")` (not `cache_dir()`).
    let cache_path = cache_path
        .or_else(|| platform::project_directories().map(|d| d.data_local_dir().join("cache")))
        .ok_or_else(|| anyhow::anyhow!("--cache-path not given and no platform cache dir"))?;

    let scenes_dir = cache_path.join("imposters").join("scenes");
    if !scenes_dir.exists() {
        anyhow::bail!("no scenes dir at {scenes_dir:?}");
    }

    let realm_enc = urlencoding::encode(&realm).into_owned();
    let loose_mip0_dir = cache_path
        .join("imposters")
        .join("realms")
        .join(&realm_enc)
        .join("0");
    fs::create_dir_all(&loose_mip0_dir)?;

    let zip_mip0_dir = zip_output.as_ref().map(|zo| {
        zo.join("imposters")
            .join("realms")
            .join(&realm_enc)
            .join("0")
    });
    if let Some(d) = &zip_mip0_dir {
        fs::create_dir_all(d)?;
    }

    let mut scenes = 0usize;
    let mut parcels = 0usize;
    let mut skipped_scenes = 0usize;
    let mut skipped_parcels = 0usize;

    for entry in fs::read_dir(&scenes_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let scene_hash = entry.file_name().to_string_lossy().into_owned();
        let scene_dir = entry.path();

        let spec_path = scene_dir.join("spec.json");
        let spec_bytes = match fs::read(&spec_path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("skip {scene_hash}: no spec ({e})");
                skipped_scenes += 1;
                continue;
            }
        };
        let baked: BakedScene = match serde_json::from_slice(&spec_bytes) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("skip {scene_hash}: bad spec ({e})");
                skipped_scenes += 1;
                continue;
            }
        };

        // Level-0 CRC = CRC32_CKSUM of the scene hash bytes. This matches
        // `ScenePointers::crc(parcel, 0)`'s `Exists{hash}` branch, so
        // runtime requests will look for the zip at the same name.
        let crc = crc::Crc::<u32>::new(&crc::CRC_32_CKSUM).checksum(scene_hash.as_bytes());

        scenes += 1;
        for (parcel, imposter_spec) in &baked.imposters {
            if let Err(e) = convert_parcel(
                &scene_dir,
                &loose_mip0_dir,
                zip_mip0_dir.as_deref(),
                *parcel,
                imposter_spec,
                crc,
            ) {
                eprintln!("skip {scene_hash} {parcel:?}: {e}");
                skipped_parcels += 1;
                continue;
            }
            parcels += 1;
        }
    }

    println!(
        "converted {scenes} scenes / {parcels} parcels; skipped {skipped_scenes} scenes / {skipped_parcels} parcels"
    );
    Ok(())
}

fn convert_parcel(
    scene_dir: &Path,
    loose_mip0_dir: &Path,
    zip_mip0_dir: Option<&Path>,
    parcel: IVec2,
    imposter_spec: &ImposterSpec,
    crc: u32,
) -> anyhow::Result<()> {
    let body_name = format!("{},{}.boimp", parcel.x, parcel.y);
    let floor_name = format!("{},{}-floor.boimp", parcel.x, parcel.y);
    let spec_name = format!("{},{}-spec.json", parcel.x, parcel.y);

    let src_body = scene_dir.join(&body_name);
    let src_floor = scene_dir.join(&floor_name);

    // Body must exist; floor is optional (older scenes may not have one).
    let body_bytes =
        fs::read(&src_body).map_err(|e| anyhow::anyhow!("missing body {body_name}: {e}"))?;
    let floor_bytes = fs::read(&src_floor).ok();

    // Build a single-parcel BakedScene with the parcel's own spec and the
    // recomputed level-0 CRC.
    let single = BakedScene {
        imposters: HashMap::from_iter([(parcel, *imposter_spec)]),
        crc,
    };
    let single_bytes = serde_json::to_vec(&single)?;

    // Loose cache files (mip-0 layout).
    fs::write(loose_mip0_dir.join(&spec_name), &single_bytes)?;
    fs::write(loose_mip0_dir.join(&body_name), &body_bytes)?;
    if let Some(floor) = &floor_bytes {
        fs::write(loose_mip0_dir.join(&floor_name), floor)?;
    }

    if let Some(zip_dir) = zip_mip0_dir {
        let zip_path = zip_dir.join(format!("{},{}.{}.zip", parcel.x, parcel.y, crc));
        let file = fs::File::create(&zip_path)?;
        let mut zip = ZipWriter::new(file);
        let opts: SimpleFileOptions =
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

        zip.start_file(&spec_name, opts)?;
        zip.write_all(&single_bytes)?;
        zip.start_file(&body_name, opts)?;
        zip.write_all(&body_bytes)?;
        if let Some(floor) = &floor_bytes {
            zip.start_file(&floor_name, opts)?;
            zip.write_all(floor)?;
        }
        zip.finish()?;
    }

    Ok(())
}
