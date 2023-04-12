pub mod ipfs_path;

use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::anyhow;
use bevy::{
    asset::{Asset, AssetIo, AssetIoError, AssetLoader, FileAssetIo, LoadedAsset},
    prelude::*,
    reflect::TypeUuid,
    tasks::IoTaskPool,
    utils::HashMap,
};
use bevy_common_assets::json::JsonAssetPlugin;
use bimap::BiMap;
use isahc::{http::StatusCode, prelude::Configurable, AsyncReadResponseExt, RequestExt};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use self::ipfs_path::{normalize_path, IpfsPath, IpfsType, PointerType};

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

#[derive(Deserialize, Debug)]
pub struct SceneMetaScene {
    pub base: String,
}

#[derive(Deserialize, TypeUuid, Debug)]
#[uuid = "5b587f78-4650-4132-8788-6fe683bec3aa"]
pub struct SceneMeta {
    pub main: String,
    pub scene: SceneMetaScene,
}

#[derive(TypeUuid, Debug, Default)]
#[uuid = "d373738a-208e-4560-9e2e-020e5c64a852"]
pub struct SceneDefinition {
    pub id: String,
    pub pointers: Vec<IVec2>,
    pub content: ContentMap,
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
            let Some(definition_json) = definition_json.pop() else {
                load_context.set_default_asset(LoadedAsset::new(SceneDefinition::default()));
                return Ok(());
            };
            let content = ContentMap(BiMap::from_iter(
                definition_json
                    .content
                    .into_iter()
                    .map(|ipfs| (normalize_path(&ipfs.file), ipfs.hash)),
            ));
            let pointers = definition_json
                .pointers
                .iter()
                .map(|pointer_str| {
                    let (pointer_x, pointer_y) = pointer_str.split_once(',').unwrap();
                    let pointer_x = pointer_x.parse::<i32>().unwrap();
                    let pointer_y = pointer_y.parse::<i32>().unwrap();
                    IVec2::new(pointer_x, pointer_y)
                })
                .collect();
            let definition = SceneDefinition {
                id: definition_json.id,
                pointers,
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

#[derive(Debug, Clone, Default)]
pub struct ContentMap(BiMap<String, String>);

impl ContentMap {
    pub fn file(&self, hash: &str) -> Option<&str> {
        self.0.get_by_right(hash).map(String::as_str)
    }

    pub fn hash(&self, file: &str) -> Option<&str> {
        self.0.get_by_left(file).map(String::as_str)
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

    fn load_content_file<T: Asset>(&self, file_path: String, content_hash: String) -> Handle<T>;

    fn load_urn<T: Asset>(&self, urn: &str) -> Handle<T>;
}

impl IpfsLoaderExt for AssetServer {
    fn load_scene_pointer(&self, x: i32, y: i32) -> Handle<SceneDefinition> {
        let ipfs_path = IpfsPath::new(IpfsType::Pointer {
            pointer_type: PointerType::Scene,
            address: format!("{x},{y}"),
        });
        self.load(PathBuf::from(&ipfs_path))
    }

    fn load_content_file<T: Asset>(&self, file_path: String, content_hash: String) -> Handle<T> {
        let ipfs_path = IpfsPath::new(IpfsType::new_content_file(content_hash, file_path));
        self.load(PathBuf::from(&ipfs_path))
    }

    fn load_urn<T: Asset>(&self, _urn: &str) -> Handle<T> {
        unimplemented!()
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct EndpointConfig {
    pub healthy: bool,
    #[serde(rename = "publicUrl")]
    pub public_url: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct ServerConfiguration {
    #[serde(rename = "scenesUrn")]
    pub scenes_urn: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ServerAbout {
    content: Option<EndpointConfig>,
    configurations: Option<ServerConfiguration>,
}

pub struct IpfsIoPlugin {
    pub starting_realm: Option<String>,
}

impl Plugin for IpfsIoPlugin {
    fn build(&self, app: &mut App) {
        let default_io = AssetPlugin::default().create_platform_default_asset_io();

        // TODO this will fail on android and wasm, investigate a caching solution there
        let default_fs_path = default_io
            .downcast_ref::<FileAssetIo>()
            .map(|fio| fio.root_path().clone());

        // create the custom asset io instance
        info!("remote server: {:?}", self.starting_realm);

        let ipfs_io = IpfsIo::new(default_io, default_fs_path);

        // the asset server is constructed and added the resource manager
        app.insert_resource(AssetServer::new(ipfs_io))
            .add_asset::<SceneDefinition>()
            .add_asset::<SceneJsFile>()
            .init_asset_loader::<SceneDefinitionLoader>()
            .init_asset_loader::<SceneJsLoader>()
            .add_plugin(JsonAssetPlugin::<SceneMeta>::new(&["scene.json"]));

        app.add_event::<ChangeRealmEvent>();
        app.add_event::<RealmChangedEvent>();
        app.add_system(change_realm.in_base_set(CoreSet::PostUpdate));

        if let Some(realm) = &self.starting_realm {
            let asset_server = app.world.resource::<AssetServer>().clone();
            let realm = realm.clone();
            IoTaskPool::get()
                .spawn(async move {
                    let ipfsio = asset_server.asset_io().downcast_ref::<IpfsIo>().unwrap();
                    ipfsio.set_realm(realm).await;
                })
                .detach();
        }
    }
}

pub struct ChangeRealmEvent {
    new_realm: String,
}

pub struct RealmChangedEvent {
    pub config: ServerConfiguration,
}

fn change_realm(
    mut change_realm_requests: EventReader<ChangeRealmEvent>,
    mut change_realm_results: EventWriter<RealmChangedEvent>,
    asset_server: Res<AssetServer>,
    mut realm_change: Local<Option<tokio::sync::watch::Receiver<Option<ServerAbout>>>>,
) {
    let ipfsio = asset_server.asset_io().downcast_ref::<IpfsIo>().unwrap();
    match *realm_change {
        None => *realm_change = Some(ipfsio.realm_config_receiver.clone()),
        Some(ref mut realm_change) => {
            if realm_change.has_changed().unwrap_or_default() {
                if let Some(new_realm) = &*realm_change.borrow_and_update() {
                    change_realm_results.send(RealmChangedEvent {
                        config: new_realm.configurations.clone().unwrap_or_default(),
                    });
                }
            }
        }
    }

    if !change_realm_requests.is_empty() {
        let asset_server = asset_server.clone();
        let new_realm = change_realm_requests
            .iter()
            .last()
            .unwrap()
            .new_realm
            .to_owned();
        IoTaskPool::get()
            .spawn(async move {
                let ipfsio = asset_server.asset_io().downcast_ref::<IpfsIo>().unwrap();
                ipfsio.set_realm(new_realm).await;
            })
            .detach();
    }
}

#[derive(Default)]
pub struct IpfsContext {
    collections: HashMap<String, ContentMap>,
    base_url: Option<String>,
}

pub struct IpfsIo {
    default_io: Box<dyn AssetIo>,
    default_fs_path: Option<PathBuf>,
    pub realm_config_receiver: tokio::sync::watch::Receiver<Option<ServerAbout>>,
    realm_config_sender: tokio::sync::watch::Sender<Option<ServerAbout>>,
    context: RwLock<IpfsContext>,
}

impl IpfsIo {
    pub fn new(default_io: Box<dyn AssetIo>, default_fs_path: Option<PathBuf>) -> Self {
        let (sender, receiver) = tokio::sync::watch::channel(None);

        Self {
            default_io,
            default_fs_path,
            realm_config_receiver: receiver,
            realm_config_sender: sender,
            context: Default::default(),
        }
    }

    pub async fn set_realm(&self, new_realm: String) {
        let res = self.set_realm_inner(new_realm).await;
        if let Err(e) = res {
            error!("failed to set realm: {e}");
        }
    }

    async fn set_realm_inner(&self, new_realm: String) -> Result<(), anyhow::Error> {
        info!("disconnecting");
        self.realm_config_sender.send(None).expect("channel closed");
        self.context.write().await.base_url = None;

        let mut about = isahc::get_async(format!("{new_realm}/about"))
            .await
            .map_err(|e| anyhow!(e))?;
        if about.status() != StatusCode::OK {
            return Err(anyhow!("status: {}", about.status()));
        }

        let about = about.json::<ServerAbout>().await.map_err(|e| anyhow!(e))?;

        self.context.write().await.base_url = about
            .content
            .as_ref()
            .map(|endpoint| endpoint.public_url.clone());
        self.realm_config_sender
            .send(Some(about))
            .expect("channel closed");
        Ok(())
    }

    async fn connected(&self) -> Result<(), anyhow::Error> {
        if self.realm_config_receiver.borrow().is_some() {
            return Ok(());
        }

        let mut watcher = self.realm_config_receiver.clone();

        loop {
            if self.realm_config_receiver.borrow().is_some() {
                return Ok(());
            }

            watcher.changed().await?;
        }
    }

    pub fn add_collection(&self, hash: String, collection: ContentMap) {
        self.context
            .blocking_write()
            .collections
            .insert(hash, collection);
    }
}

impl AssetIo for IpfsIo {
    fn load_path<'a>(
        &'a self,
        path: &'a std::path::Path,
    ) -> bevy::utils::BoxedFuture<'a, Result<Vec<u8>, bevy::asset::AssetIoError>> {
        Box::pin(async move {
            let wrap_err =
                |e| bevy::asset::AssetIoError::Io(std::io::Error::new(ErrorKind::Other, e));

            debug!("request: {:?}", path);

            let maybe_ipfs_path = IpfsPath::from_path(path).map_err(wrap_err)?;
            let ipfs_path = match maybe_ipfs_path {
                Some(ipfs_path) => ipfs_path,
                // non-ipfs files are loaded as normal
                None => return self.default_io.load_path(path).await,
            };

            let hash = ipfs_path.hash(&*self.context.read().await);

            let file = match &hash {
                None => None,
                Some(hash) => {
                    debug!("hash: {}", hash);
                    match ipfs_path.should_cache() {
                        true => self.default_io.load_path(Path::new(hash)).await.ok(),
                        false => None,
                    }
                }
            };

            if let Some(existing) = file {
                debug!("existing");
                Ok(existing)
            } else {
                debug!("remote");

                // wait till connected
                self.connected().await.map_err(wrap_err)?;

                let remote = ipfs_path
                    .to_url(&*self.context.read().await)
                    .map_err(wrap_err)?;

                debug!("remote url: `{remote}`");
                let request = isahc::Request::get(&remote)
                    .timeout(Duration::from_secs(120))
                    .body(())
                    .map_err(|e| AssetIoError::Io(std::io::Error::new(ErrorKind::Other, e)))?;
                let mut response = request
                    .send_async()
                    .await
                    .map_err(|e| AssetIoError::Io(std::io::Error::new(ErrorKind::Other, e)))?;

                if !matches!(response.status(), StatusCode::OK) {
                    return Err(AssetIoError::Io(std::io::Error::new(
                        ErrorKind::Other,
                        format!(
                            "server responded with status {} requesting `{}`",
                            response.status(),
                            remote,
                        ),
                    )));
                }

                let data = response.bytes().await?;

                if ipfs_path.should_cache() {
                    if let Some(hash) = hash {
                        let mut cache_path = self.default_fs_path.clone().unwrap();
                        cache_path.push(hash);
                        let cache_path_str = cache_path.to_string_lossy().into_owned();
                        // ignore errors trying to cache
                        if let Err(e) = std::fs::write(cache_path, &data) {
                            warn!("failed to cache `{cache_path_str}`: {e}");
                        } else {
                            debug!("cached ok `{cache_path_str}`");
                        }
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
