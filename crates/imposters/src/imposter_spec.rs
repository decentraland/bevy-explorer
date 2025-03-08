use std::{
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};

use bevy::{asset::AsyncReadExt, prelude::*, utils::HashMap};
use common::structs::IVec2Arg;
use ipfs::IpfsIo;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Serialize, Deserialize, Component, Clone)]
pub struct ImposterSpec {
    pub scale: f32,
    pub region_min: Vec3,
    pub region_max: Vec3,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct BakedScene {
    #[serde(serialize_with = "imposter_serialize")]
    #[serde(deserialize_with = "imposter_deserialize")]
    pub imposters: HashMap<IVec2, ImposterSpec>,
    pub crc: u32,
}

fn imposter_serialize<S>(val: &HashMap<IVec2, ImposterSpec>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let string_map = HashMap::from_iter(
        val.iter()
            .map(|(key, val)| (format!("{},{}", key.x, key.y), val)),
    );
    s.collect_map(string_map)
}

fn imposter_deserialize<'de, D>(d: D) -> Result<HashMap<IVec2, ImposterSpec>, D::Error>
where
    D: Deserializer<'de>,
{
    let string_map = HashMap::<String, ImposterSpec>::deserialize(d)?;
    Ok(HashMap::from_iter(string_map.into_iter().map(
        |(key, value)| (IVec2Arg::from_str(&key).unwrap().0, value),
    )))
}

impl BakedScene {}

fn file_root(cache_path: &Path, id: &str, level: usize) -> PathBuf {
    let mut path = cache_path.to_owned();

    if level == 0 {
        path.push("imposters");
        path.push("scenes");
        path.push(id);
    } else {
        path.push("imposters");
        path.push("realms");
        path.push(urlencoding::encode(id).into_owned());
        path.push(format!("{level}"));
    }
    path
}

pub(crate) fn spec_path(cache_path: &Path, id: &str, parcel: IVec2, level: usize) -> PathBuf {
    let mut path = file_root(cache_path, id, level);
    if level == 0 {
        path.push("spec.json");
    } else {
        path.push(format!("{},{}-spec.json", parcel.x, parcel.y));
    }
    path
}

pub(crate) fn texture_path(cache_path: &Path, id: &str, parcel: IVec2, level: usize) -> PathBuf {
    let mut path = file_root(cache_path, id, level);
    path.push(format!("{},{}.boimp", parcel.x, parcel.y));
    path
}

pub(crate) fn floor_path(cache_path: &Path, id: &str, parcel: IVec2, level: usize) -> PathBuf {
    let mut path = file_root(cache_path, id, level);
    path.push(format!("{},{}-floor.boimp", parcel.x, parcel.y));
    path
}

pub(crate) fn zip_path(cache_path: &Path, id: &str, parcel: IVec2, level: usize) -> PathBuf {
    let mut path = file_root(cache_path, id, level);
    if level == 0 {
        path.push("scene.zip");
    } else {
        path.push(format!("{},{}.zip", parcel.x, parcel.y));
    }
    path
}

pub(crate) fn write_imposter(
    cache_path: &Path,
    id: &str,
    parcel: IVec2,
    level: usize,
    baked_scene: &BakedScene,
) {
    let path = spec_path(cache_path, id, parcel, level);
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    if let Err(e) = std::fs::File::create(path)
        .map_err(|e| e.to_string())
        .and_then(|f| serde_json::to_writer(f, baked_scene).map_err(|e| e.to_string()))
    {
        warn!("failed to write imposter spec: {e}");
    }
}

pub async fn load_imposter(
    ipfs: Arc<IpfsIo>,
    id: String,
    parcel: IVec2,
    level: usize,
    required_crc: Option<u32>,
) -> Option<BakedScene> {
    // try locally
    let path = spec_path(ipfs.cache_path(), &id, parcel, level);
    if let Ok(mut file) = async_fs::File::open(&path).await {
        let mut buf = Vec::default();
        if file.read_to_end(&mut buf).await.is_ok() {
            if let Ok(baked_scene) = serde_json::from_slice::<BakedScene>(&buf) {
                if required_crc.is_none_or(|crc| crc == baked_scene.crc) {
                    return Some(baked_scene);
                } else {
                    warn!(
                        "mismatched hash for {path:?} (expected {}, found {}",
                        required_crc.unwrap(),
                        baked_scene.crc
                    );
                }
            } else {
                warn!("failed to deserialize {path:?}");
            }
        };
    } else {
        warn!("missing imposter @ {path:?}");
    }

    // TODO try remote

    None
}
