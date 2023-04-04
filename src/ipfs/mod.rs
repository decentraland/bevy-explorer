use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{Arc, RwLock}, time::Duration,
};

use bevy::{
    asset::{Asset, AssetIo, AssetIoError, AssetLoader, FileAssetIo, LoadedAsset},
    prelude::*,
    reflect::TypeUuid,
    utils::HashMap,
};
use bevy_common_assets::json::JsonAssetPlugin;
use bimap::BiMap;
use isahc::{http::StatusCode, AsyncReadResponseExt, prelude::Configurable, RequestExt};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct TypedIpfsRef {
    file: String,
    hash: String,
}

#[derive(Deserialize)]
pub struct SceneDefinitionJson {
    id: String,
    pointers: Vec<String>,
    content: Vec<TypedIpfsRef>,
}

#[derive(Deserialize, TypeUuid, Debug)]
#[uuid = "5b587f78-4650-4132-8788-6fe683bec3aa"]
pub struct SceneMeta {
    pub main: String,
}

#[derive(TypeUuid, Debug)]
#[uuid = "d373738a-208e-4560-9e2e-020e5c64a852"]
pub struct SceneDefinition {
    pub id: String,
    pub pointers: Vec<String>,
    pub content: SceneContent,
}

#[derive(TypeUuid, Debug, Clone)]
#[uuid = "f9f54e97-439f-4768-9ea0-f3e894049492"]
pub struct SceneJsFile(pub Arc<String>);

#[derive(Default)]
pub struct SceneDefinitionLoader;

impl AssetLoader for SceneDefinitionLoader {
    fn load<'a>(
        &'a self,
        bytes: &'a [u8],
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> bevy::utils::BoxedFuture<'a, Result<(), bevy::asset::Error>> {
        Box::pin(async move {
            let mut definition_json: Vec<SceneDefinitionJson> = serde_json::from_reader(bytes)?;
            let definition_json = definition_json
                .pop()
                .ok_or(bevy::asset::Error::msg("scene pointer is empty"))?;
            let content = SceneContent(BiMap::from_iter(
                definition_json
                    .content
                    .into_iter()
                    .map(|ipfs| (normalize_path(&ipfs.file), ipfs.hash)),
            ));
            let definition = SceneDefinition {
                id: definition_json.id,
                pointers: definition_json.pointers,
                content,
            };
            load_context.set_default_asset(LoadedAsset::new(definition));
            Ok(())
        })
    }

    fn extensions(&self) -> &[&str] {
        &["scene_pointer", "scene_entity"]
    }
}

#[derive(Default)]
pub struct SceneJsLoader;

impl AssetLoader for SceneJsLoader {
    fn load<'a>(
        &'a self,
        bytes: &'a [u8],
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> bevy::utils::BoxedFuture<'a, Result<(), bevy::asset::Error>> {
        Box::pin(async move {
            load_context.set_default_asset(LoadedAsset::new(SceneJsFile(Arc::new(
                String::from_utf8(bytes.to_vec())?,
            ))));
            Ok(())
        })
    }

    fn extensions(&self) -> &[&str] {
        &["js"]
    }
}

#[derive(Debug, Clone)]
pub struct SceneContent(BiMap<String, String>);

impl SceneContent {
    pub fn file(&self, hash: &str) -> Option<&str> {
        self.0.get_by_right(hash).map(String::as_str)
    }

    pub fn hash(&self, file: &str) -> Option<&str> {
        self.0
            .get_by_left(&normalize_path(file))
            .map(String::as_str)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum SceneIpfsLocation {
    Pointer(i32, i32),
    Hash(String),
    Js(String),
}

pub trait IpfsLoaderExt {
    fn load_scene_pointer(&self, x: i32, y: i32) -> Handle<SceneDefinition>;

    fn load_scene_file<T: Asset>(&self, file: &str, scene_entity_hash: &str) -> Handle<T>;
}

impl IpfsLoaderExt for AssetServer {
    fn load_scene_pointer(&self, x: i32, y: i32) -> Handle<SceneDefinition> {
        self.load(format!("{x},{y}.scene_pointer"))
    }

    fn load_scene_file<T: Asset>(&self, file: &str, scene_entity_hash: &str) -> Handle<T> {
        debug!(
            "load: {file} from {scene_entity_hash} -> `{}`",
            format!("{scene_entity_hash}.{file}")
        );
        self.load(format!("{scene_entity_hash}.{file}"))
    }
}

pub struct IpfsIoPlugin {
    pub server_prefix: String,
}

impl Plugin for IpfsIoPlugin {
    fn build(&self, app: &mut App) {
        let default_io = AssetPlugin::default().create_platform_default_asset_io();

        // TODO this will fail on android and wasm, investigate a caching solution there
        let default_fs_path = default_io
            .downcast_ref::<FileAssetIo>()
            .map(|fio| fio.root_path().clone());

        // create the custom asset io instance
        info!("remote server: {}", self.server_prefix);
        let ipfs_io = IpfsIo::new(
            format!("{}/content", self.server_prefix),
            default_io,
            default_fs_path,
        );

        // the asset server is constructed and added the resource manager
        app.insert_resource(AssetServer::new(ipfs_io))
            .add_asset::<SceneDefinition>()
            .add_asset::<SceneJsFile>()
            .init_asset_loader::<SceneDefinitionLoader>()
            .init_asset_loader::<SceneJsLoader>()
            .add_plugin(JsonAssetPlugin::<SceneMeta>::new(&["scene.json"]));
    }
}

pub struct IpfsIo {
    default_io: Box<dyn AssetIo>,
    default_fs_path: Option<PathBuf>,
    server_prefix: String,
    collections: RwLock<HashMap<String, SceneContent>>,
}

impl IpfsIo {
    pub fn new(
        server_prefix: String,
        default_io: Box<dyn AssetIo>,
        default_fs_path: Option<PathBuf>,
    ) -> Self {
        Self {
            default_io,
            default_fs_path,
            server_prefix,
            collections: Default::default(),
        }
    }

    pub fn scene_path(&self, pointer: &str) -> String {
        format!("{}/entities/scene?pointer={}", self.server_prefix, pointer)
    }

    pub fn remote_path(&self, target: &str, path: &str) -> String {
        let (base, ext) = path.rsplit_once('.').unwrap_or_default();
        match ext {
            "scene_pointer" => self.scene_path(base),
            _ => format!("{}/contents/{}", self.server_prefix, target),
        }
    }

    pub fn path_should_cache(&self, target: &str, path: &Path) -> bool {
        self.default_fs_path.is_some() && !target.starts_with("b64") && !path.ends_with("pointer")
    }

    pub fn add_collection(&self, hash: String, collection: SceneContent) {
        self.collections.write().unwrap().insert(hash, collection);
    }
}

impl AssetIo for IpfsIo {
    fn load_path<'a>(
        &'a self,
        path: &'a std::path::Path,
    ) -> bevy::utils::BoxedFuture<'a, Result<Vec<u8>, bevy::asset::AssetIoError>> {
        Box::pin(async move {
            debug!("request: {:?}", path);

            let path_string = path.to_string_lossy();
            let target = {
                let collections = self.collections.read().unwrap();

                path_string
                    .split_once('.')
                    .and_then(|(collection_hash, filename)| {
                        debug!("got collection hash {collection_hash}");

                        debug!("filename {:?}", filename);

                        collections
                            .get(collection_hash)
                            .map(|hash| (hash, filename))
                    })
                    .and_then(|(collection, filename)| {
                        debug!("looking up `{}`", normalize_path(filename));
                        let res = collection.hash(filename);
                        debug!("found: `{:?}`", res);
                        if res.is_none() {
                            debug!("contents were: {:#?}", collection);
                        }
                        res
                    })
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| path_string.into_owned())
            };

            debug!("target: {}", target);
            let file = match self.path_should_cache(&target, path) {
                true => self.default_io.load_path(Path::new(&target)).await.ok(),
                false => None,
            };

            if let Some(existing) = file {
                debug!("existing: {}", path.to_string_lossy());
                Ok(existing)
            } else {
                debug!("remote: {}", target);

                let remote = self.remote_path(
                    target.as_str(),
                    &path.file_name().unwrap().to_string_lossy(),
                );
                debug!("requesting: `{remote}`");
                let request = isahc::Request::get(&remote).timeout(Duration::from_secs(5)).body(()).map_err(|e| {
                    warn!("request failed: {e:?}");
                    AssetIoError::Io(std::io::Error::new(ErrorKind::Other, e.to_string()))
                })?;
                let mut response = request.send_async().await.map_err(|e| {
                    warn!("asset io error: {e:?}");
                    AssetIoError::Io(std::io::Error::new(ErrorKind::Other, e.to_string()))
                })?;

                if !matches!(response.status(), StatusCode::OK) {
                    return Err(AssetIoError::Io(std::io::Error::new(
                        ErrorKind::Other,
                        format!(
                            "server responded with status {} requesting `{}`",
                            response.status(),
                            target,
                        ),
                    )));
                };

                let data = response.bytes().await?;

                if self.path_should_cache(&target, path) {
                    let mut cache_path = self.default_fs_path.clone().unwrap();
                    cache_path.push(target);
                    let cache_path_str = cache_path.to_string_lossy().into_owned();
                    // ignore errors trying to cache
                    if let Err(e) = std::fs::write(cache_path, &data) {
                        warn!("failed to cache `{cache_path_str}`: {e}");
                    } else {
                        debug!("cached ok `{cache_path_str}`");
                    }
                }

                Ok(data)
            }
        })
    }

    fn read_directory(
        &self,
        _: &std::path::Path,
    ) -> Result<Box<dyn Iterator<Item = std::path::PathBuf>>, bevy::asset::AssetIoError> {
        panic!("unsupported")
    }

    fn get_metadata(
        &self,
        _: &std::path::Path,
    ) -> Result<bevy::asset::Metadata, bevy::asset::AssetIoError> {
        panic!("unsupported")
    }

    fn watch_path_for_changes(
        &self,
        _: &std::path::Path,
        _: Option<std::path::PathBuf>,
    ) -> Result<(), bevy::asset::AssetIoError> {
        // do nothing
        Ok(())
    }

    fn watch_for_changes(&self) -> Result<(), bevy::asset::AssetIoError> {
        // do nothing
        Ok(())
    }
}

// must be a better way to do this
fn normalize_path(path: &str) -> String {
    path.to_lowercase().replace('\\', "/")
}
