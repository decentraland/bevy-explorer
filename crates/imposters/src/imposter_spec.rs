use std::{
    io::Cursor,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};

use bevy::{
    asset::AsyncReadExt,
    platform::{collections::HashMap, hash::FixedHasher},
    prelude::*,
};
use common::structs::IVec2Arg;
use ipfs::{ipfs_path::IpfsPath, IpfsIo};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tokio_util::sync::CancellationToken;
use zip::ZipArchive;

#[cfg(not(target_arch = "wasm32"))]
use async_fs as platform_fs;
#[cfg(target_arch = "wasm32")]
use web_fs as platform_fs;

#[derive(Debug, Serialize, Deserialize, Component, Clone, Copy, PartialEq)]
pub struct ImposterSpec {
    pub scale: f32,
    pub region_min: Vec3,
    pub region_max: Vec3,
    /// World-space distance the baked texture holds content past the
    /// parcel-clamped region (the level-0 `bound_tolerance`, carried up the mip
    /// chain as `max(children)`). Used to expose that overhang when this
    /// imposter is baked as an ingredient — see `ImposterMesh::from_spec`.
    /// Defaults to 0 for specs written before this field existed.
    #[serde(default)]
    pub overhang: f32,
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
    let string_map: HashMap<_, _, FixedHasher> = HashMap::from_iter(
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

fn file_root(cache_path: Option<&Path>, as_ipfs_path: bool, id: &str, level: usize) -> PathBuf {
    let mut path = cache_path.map(ToOwned::to_owned).unwrap_or_default();
    path.push("imposters");
    path.push("realms");
    path.push(urlencoding::encode(id).into_owned());
    path.push(format!("{level}"));

    if cache_path.is_none() && as_ipfs_path {
        PathBuf::from(&IpfsPath::new_indexdb(path))
    } else {
        path
    }
}

pub(crate) fn spec_path(
    cache_path: Option<&Path>,
    id: &str,
    parcel: IVec2,
    level: usize,
) -> PathBuf {
    let mut path = file_root(cache_path, false, id, level);
    path.push(format!("{},{}-spec.json", parcel.x, parcel.y));
    path
}

pub(crate) fn texture_path(
    cache_path: Option<&Path>,
    id: &str,
    parcel: IVec2,
    level: usize,
) -> PathBuf {
    let mut path = file_root(cache_path, true, id, level);
    path.push(format!("{},{}.boimp", parcel.x, parcel.y));
    path
}

pub(crate) fn floor_path(
    cache_path: Option<&Path>,
    id: &str,
    parcel: IVec2,
    level: usize,
) -> PathBuf {
    let mut path = file_root(cache_path, true, id, level);
    path.push(format!("{},{}-floor.boimp", parcel.x, parcel.y));
    path
}

pub(crate) fn zip_path(
    cache_path: Option<&Path>,
    id: &str,
    parcel: IVec2,
    level: usize,
    crc: Option<u32>,
) -> PathBuf {
    let mut path = file_root(cache_path, false, id, level);
    path.push(format!("{},{}.{}.zip", parcel.x, parcel.y, crc.unwrap()));
    path
}

pub(crate) fn write_imposter(
    cache_path: Option<&Path>,
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
    download: bool,
    cancel: CancellationToken,
) -> Option<BakedScene> {
    if required_crc.is_some_and(|crc| crc == 0) {
        // crc==0 means the area has no scenes, so resolve directly to an empty
        // spec instead of falling through to remote fetch (which would 404) or
        // PendingRemote (which would re-trigger the bake every frame).
        return Some(BakedScene::default());
    }

    // try locally
    if let Some(imposter) = load_imposter_local(&ipfs, &id, parcel, level, required_crc).await {
        return Some(imposter);
    }

    if download {
        if let Err(e) = load_imposter_remote(&ipfs, &id, parcel, level, required_crc, cancel).await
        {
            warn!("{e}");
            return None;
        }
        return load_imposter_local(&ipfs, &id, parcel, level, required_crc).await;
    }

    None
}

pub async fn load_imposter_remote(
    ipfs: &IpfsIo,
    id: &str,
    parcel: IVec2,
    level: usize,
    crc: Option<u32>,
    cancel: CancellationToken,
) -> Result<(), anyhow::Error> {
    let client = ipfs.client();
    let zip_file = zip_path(None, id, parcel, level, crc)
        .to_string_lossy()
        .into_owned()
        .replace("\\", "/");
    let zip_url = format!("https://bevy-imposters.dclregenesislabs.xyz/{zip_file}")
        // double url encode
        .replace("%", "%25");
    debug!("zip_url {zip_url}");

    // Bulk imposter-zip download; generous total-timeout floor until the
    // content-inactivity timeout (Tier 2) replaces it.
    let request = client
        .get(&zip_url)
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    // Race the network fetch + body read against the cancel token. If the
    // owning `ImposterLoadTask` Component is dropped (entity despawn / out of
    // range), this future is dropped together with the `select!`, releasing
    // the IPFS semaphore permit and (on wasm) firing reqwest's `AbortGuard`
    // to abort the in-flight browser fetch.
    let bytes = tokio::select! {
        fetched = async {
            let response = ipfs.async_request(request, client).await?;
            if response.status() != reqwest::StatusCode::OK {
                return Ok(None);
            }
            Ok::<_, anyhow::Error>(Some(response.bytes().await?))
        } => match fetched? {
            Some(bytes) => bytes,
            None => return Ok(()),
        },
        _ = cancel.cancelled() => {
            debug!("imposter load cancelled: id={id} parcel={parcel} level={level} url={zip_url}");
            return Ok(());
        }
    };
    let mut zip = ZipArchive::new(Cursor::new(bytes))?;
    let root = file_root(ipfs.cache_path(), false, id, level);
    platform_fs::create_dir_all(&root).await?;

    #[cfg(not(target_arch = "wasm32"))]
    zip.extract(root)?;

    #[cfg(target_arch = "wasm32")]
    {
        for i in 0..zip.len() {
            use futures_lite::io::AsyncWriteExt;
            use std::io::Read;

            let mut file = zip.by_index(i)?;
            let outpath = root.clone().join(
                file.enclosed_name()
                    .ok_or(anyhow::anyhow!("bad filename in zip?"))?,
            );
            let mut outfile = platform_fs::File::create(&outpath).await?;
            let mut buf = Vec::default();
            file.read_to_end(&mut buf)?;
            outfile.write_all(&buf).await?;
        }
    }

    Ok(())
}

pub async fn load_imposter_local(
    ipfs: &IpfsIo,
    id: &str,
    parcel: IVec2,
    level: usize,
    required_crc: Option<u32>,
) -> Option<BakedScene> {
    let path = spec_path(ipfs.cache_path(), id, parcel, level);
    if let Ok(mut file) = platform_fs::File::open(&path).await {
        let mut buf = Vec::default();
        if file.read_to_end(&mut buf).await.is_ok() {
            if let Ok(baked_scene) = serde_json::from_slice::<BakedScene>(&buf) {
                if required_crc.is_none_or(|crc| crc == baked_scene.crc) {
                    return Some(baked_scene);
                } else {
                    debug!(
                        "mismatched hash for {path:?} (expected {}, found {})",
                        required_crc.unwrap(),
                        baked_scene.crc
                    );
                }
            } else {
                warn!("failed to deserialize {path:?}");
            }
        };
    }

    None
}
