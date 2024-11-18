use std::{path::PathBuf, str::FromStr, sync::Arc};

use bevy::{asset::AsyncReadExt, prelude::*, utils::HashMap};
use common::structs::IVec2Arg;
use ipfs::{IpfsAssetServer, IpfsIo};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Serialize, Deserialize, Component, Clone)]
pub struct ImposterSpec {
    pub scale: f32,
    pub region_min: Vec3,
    pub region_max: Vec3,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct BakedScene {
    #[serde(serialize_with = "imposter_serialize")]
    #[serde(deserialize_with = "imposter_deserialize")]
    pub imposters: HashMap<IVec2, ImposterSpec>,
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

fn scene_path(ipfs: &IpfsIo, hash: &str) -> PathBuf {
    let mut path = ipfs.cache_path().to_owned();
    path.push("imposters");
    path.push("scenes");
    path.push(hash);
    path
}

pub(crate) fn scene_spec_path(ipfs: &IpfsIo, hash: &str) -> PathBuf {
    let mut path = scene_path(ipfs, hash);
    path.push("spec.json");
    path
}

pub(crate) fn scene_texture_path(ipfs: &IpfsIo, hash: &str, parcel: IVec2) -> PathBuf {
    let mut path = scene_path(ipfs, hash);
    path.push(format!("{},{}.boimp", parcel.x, parcel.y));
    path
}

pub(crate) fn scene_floor_path(ipfs: &IpfsIo, hash: &str, parcel: IVec2) -> PathBuf {
    let mut path = scene_path(ipfs, hash);
    path.push(format!("{},{}-floor.boimp", parcel.x, parcel.y));
    path
}

pub(crate) fn write_scene_imposter(ipfas: &IpfsAssetServer, hash: &str, imposter: &BakedScene) {
    let path = scene_spec_path(ipfas.ipfs(), hash);
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    if let Err(e) = std::fs::File::create(path)
        .map_err(|e| e.to_string())
        .and_then(|f| serde_json::to_writer(f, &imposter).map_err(|e| e.to_string()))
    {
        warn!("failed to write imposter spec: {e}");
    }
}

pub async fn load_scene_imposter(ipfs: Arc<IpfsIo>, scene_hash: String) -> Option<BakedScene> {
    // try locally
    if let Ok(mut file) = async_fs::File::open(scene_spec_path(&ipfs, &scene_hash)).await {
        let mut buf = Vec::default();
        if file.read_to_end(&mut buf).await.is_ok() {
            if let Ok(imposter) = serde_json::from_slice(&buf) {
                return Some(imposter);
            }
        };
    }

    // TODO try remote

    None
}
