use std::{io::ErrorKind, path::PathBuf, sync::Arc};

use bevy::{
    asset::{Asset, AssetIo, AssetIoError, AssetLoader, FileAssetIo, LoadedAsset},
    prelude::*,
    reflect::TypeUuid,
};
use bevy_common_assets::json::JsonAssetPlugin;
use bimap::BiMap;
use isahc::{http::StatusCode, AsyncReadResponseExt};
use serde::Deserialize;

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
                    .map(|ipfs| (ipfs.hash, ipfs.file)),
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

#[derive(Debug)]
pub struct SceneContent(BiMap<String, String>);

impl SceneContent {
    pub fn file(&self, hash: &str) -> Option<&str> {
        self.0.get_by_left(hash).map(String::as_str)
    }
    pub fn ext(&self, hash: &str) -> Option<&str> {
        self.file(hash).map(|file| {
            let ext = &file[file.find('.').unwrap_or_default()..];
            match ext {
                ".json" => file,
                _ => &ext[1..],
            }
        })
    }
    pub fn hash(&self, hash: &str) -> Option<&str> {
        self.0.get_by_right(hash).map(String::as_str)
    }
}

pub enum SceneIpfsLocation {
    Pointer(i32, i32),
    Hash(String),
    Js(String),
}

pub trait IpfsLoaderExt {
    fn load_scene_pointer(&self, x: i32, y: i32) -> Handle<SceneDefinition>;

    fn load_scene_file<T: Asset>(&self, file: &str, content: &SceneContent) -> Option<Handle<T>>;
}

impl IpfsLoaderExt for AssetServer {
    fn load_scene_pointer(&self, x: i32, y: i32) -> Handle<SceneDefinition> {
        self.load(format!("{x},{y}.scene_pointer"))
    }

    fn load_scene_file<T: Asset>(&self, file: &str, content: &SceneContent) -> Option<Handle<T>> {
        let hash = content.hash(file)?;
        let ext = content.ext(hash)?;
        Some(self.load(format!("{hash}.{ext}")))
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
        let ipfs_io = IpfsIo::new(self.server_prefix.clone(), default_io, default_fs_path);

        // the asset server is constructed and added the resource manager
        app.insert_resource(AssetServer::new(ipfs_io))
            .add_asset::<SceneDefinition>()
            .add_asset::<SceneJsFile>()
            .init_asset_loader::<SceneDefinitionLoader>()
            .init_asset_loader::<SceneJsLoader>()
            .add_plugin(JsonAssetPlugin::<SceneMeta>::new(&["scene.json"]));
    }
}

struct IpfsIo {
    default_io: Box<dyn AssetIo>,
    default_fs_path: Option<PathBuf>,
    server_prefix: String,
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
        }
    }

    pub fn scene_path(&self, pointer: &str) -> String {
        format!("{}/entities/scene?pointer={}", self.server_prefix, pointer)
    }

    pub fn content_path(&self, path: &str, ext: &str) -> String {
        match ext {
            "scene_pointer" => self.scene_path(path),
            _ => format!("{}/contents/{}", self.server_prefix, path),
        }
    }

    pub fn path_should_cache(&self, path: &str) -> bool {
        self.default_fs_path.is_some() && !path.starts_with("b64") && !path.ends_with("pointer")
    }
}

impl AssetIo for IpfsIo {
    fn load_path<'a>(
        &'a self,
        path: &'a std::path::Path,
    ) -> bevy::utils::BoxedFuture<'a, Result<Vec<u8>, bevy::asset::AssetIoError>> {
        Box::pin(async move {
            let path_str = path.to_string_lossy();
            debug!("request: {}", path_str);
            let file = match self.path_should_cache(&path_str) {
                true => self.default_io.load_path(path).await.ok(),
                false => None,
            };
            if let Some(existing) = file {
                debug!("existing: {}", path_str);
                Ok(existing)
            } else {
                let file_name = path.file_name().unwrap().to_string_lossy();
                debug!("remote: {}", file_name);
                let ext_ix = file_name.find('.').unwrap();
                let base = &file_name[0..ext_ix];
                let ext = &file_name[ext_ix + 1..];

                let remote = self.content_path(base, ext);
                debug!("requesting: `{remote}`");
                let mut response = isahc::get_async(remote).await.map_err(|e| {
                    warn!("asset io error: {e:?}");
                    AssetIoError::Io(std::io::Error::new(ErrorKind::Other, e.to_string()))
                })?;

                if !matches!(response.status(), StatusCode::OK) {
                    return Err(AssetIoError::Io(std::io::Error::new(
                        ErrorKind::Other,
                        format!(
                            "server responded with status {} requesting `{}`",
                            response.status(),
                            self.content_path(base, ext)
                        ),
                    )));
                };

                let data = response.bytes().await?;

                if self.path_should_cache(&path_str) {
                    let mut cache_path = self.default_fs_path.clone().unwrap();
                    cache_path.push(path);
                    let cache_path_str = cache_path.to_string_lossy().into_owned();
                    // ignore errors trying to cache
                    if let Err(e) = std::fs::write(cache_path, &data) {
                        warn!("failed to cache `{cache_path_str}`: {e}");
                    } else {
                        warn!("cached ok `{cache_path_str}`");
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
