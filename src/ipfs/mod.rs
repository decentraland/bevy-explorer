pub mod ipfs_path;

use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{
        atomic::{self, AtomicU16},
        Arc,
    },
    time::Duration,
};

use anyhow::anyhow;
use bevy::{
    asset::{Asset, AssetIo, AssetIoError, AssetLoader, FileAssetIo, LoadedAsset},
    prelude::*,
    reflect::TypeUuid,
    tasks::{IoTaskPool, Task},
    utils::HashMap,
};
use bevy_console::{ConsoleCommand, PrintConsoleLine};
use bimap::BiMap;
use isahc::{http::StatusCode, prelude::Configurable, AsyncReadResponseExt, RequestExt};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::console::DoAddConsoleCommand;

use self::ipfs_path::{normalize_path, EntityType, IpfsPath, IpfsType};

const MAX_CONCURRENT_REQUESTS: usize = 8;

#[derive(Serialize, Deserialize, Debug)]
pub struct TypedIpfsRef {
    file: String,
    hash: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EntityDefinitionJson {
    id: Option<String>,
    pointers: Vec<String>,
    content: Vec<TypedIpfsRef>,
    metadata: Option<serde_json::Value>,
}

#[derive(TypeUuid, Debug, Default)]
#[uuid = "d373738a-208e-4560-9e2e-020e5c64a852"]
pub struct EntityDefinition {
    pub id: String,
    pub pointers: Vec<String>,
    pub content: ContentMap,
    pub metadata: Option<serde_json::Value>,
}

#[derive(TypeUuid, Debug, Clone)]
#[uuid = "f9f54e97-439f-4768-9ea0-f3e894049492"]
pub struct SceneJsFile(pub Arc<String>);

#[derive(Default)]
pub struct EntityDefinitionLoader;

impl AssetLoader for EntityDefinitionLoader {
    fn load<'a>(
        &'a self,
        bytes: &'a [u8],
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> bevy::utils::BoxedFuture<'a, Result<(), bevy::asset::Error>> {
        Box::pin(async move {
            let maybe_definition_json = {
                // try to parse as a vec
                let definition_json_vec: Result<Vec<EntityDefinitionJson>, _> =
                    serde_json::from_reader(bytes);
                match definition_json_vec {
                    Ok(mut vec) => vec.pop(),
                    Err(_) => {
                        // else try to parse as a single item
                        Some(serde_json::from_reader(bytes)?)
                    }
                }
            };
            let Some(definition_json) = maybe_definition_json else {
                // if the source was an empty vec, we have loaded a pointer with no content, just set default
                load_context.set_default_asset(LoadedAsset::new(EntityDefinition::default()));
                return Ok(());
            };
            let content = ContentMap(BiMap::from_iter(
                definition_json
                    .content
                    .into_iter()
                    .map(|ipfs| (normalize_path(&ipfs.file), ipfs.hash)),
            ));
            let id = definition_json.id.unwrap_or_else(|| {
                // we must have been loaded as an entity with the format "$ipfs/$entity/{hash}.entity_type" - use the ipfs path to resolve the id
                load_context
                    .path()
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .split_once('.')
                    .unwrap()
                    .0
                    .to_owned()
            });

            let definition = EntityDefinition {
                id,
                pointers: definition_json.pointers,
                content,
                metadata: definition_json.metadata,
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
    Hash(String),
    Urn(String),
}

pub trait IpfsLoaderExt {
    fn load_content_file<T: Asset>(
        &self,
        file_path: &str,
        content_hash: &str,
    ) -> Result<Handle<T>, anyhow::Error>;

    fn load_urn<T: Asset>(&self, urn: &str, ty: EntityType) -> Result<Handle<T>, anyhow::Error>;

    fn load_hash<T: Asset>(&self, hash: &str, ty: EntityType) -> Handle<T>;

    fn active_endpoint(&self) -> Option<String>;

    fn ipfs(&self) -> &IpfsIo;

    fn ipfs_cache_path(&self) -> &Path;
}

impl IpfsLoaderExt for AssetServer {
    fn load_content_file<T: Asset>(
        &self,
        file_path: &str,
        content_hash: &str,
    ) -> Result<Handle<T>, anyhow::Error> {
        // note - we can't resolve paths to hashes here because some loaders use the path to locate dependent assets (e.g. gltf embedded textures)
        // TODO we could use this immediate resolution for file types that don't rely on context
        // TODO or we could add a `canonicalize` method to bevy's AssetIo trait
        // let ipfs_io = self.asset_io().downcast_ref::<IpfsIo>().unwrap();
        // let context = ipfs_io.context.blocking_read();
        // let collection = context
        //     .collections
        //     .get(content_hash)
        //     .ok_or(anyhow::anyhow!("collection not found: {content_hash}"))?;
        // let hash = collection
        //     .hash(&normalize_path(file_path))
        //     .ok_or(anyhow::anyhow!(
        //         "file_path not found in collection: {file_path}"
        //     ))?;
        // // TODO use registered loaders to extract extension
        // let file_path = Path::new(file_path);
        // let file_name = file_path.file_name().unwrap().to_str().unwrap();
        // let path = format!("$ipfs//$entity//{hash}.{file_name}");
        // Ok(self.load(path))
        let ipfs_path = IpfsPath::new(IpfsType::new_content_file(
            content_hash.to_owned(),
            file_path.to_owned(),
        ));
        Ok(self.load(PathBuf::from(&ipfs_path)))
    }

    fn load_urn<T: Asset>(&self, urn: &str, ty: EntityType) -> Result<Handle<T>, anyhow::Error> {
        let ipfs_path = IpfsPath::new_from_urn(urn, ty)?;
        let hash = ipfs_path
            .context_free_hash()?
            .ok_or(anyhow::anyhow!("urn did not resolve to a hash"))?;
        let ext = ty.ext();
        let path = format!("$ipfs//$entity//{hash}.{ext}");

        if let Some(base_url) = ipfs_path.base_url() {
            // update the context
            let ipfs_io = self.asset_io().downcast_ref::<IpfsIo>().unwrap();
            let mut context = ipfs_io.context.blocking_write();
            context.modifiers.insert(
                hash,
                IpfsModifier {
                    base_url: Some(base_url.to_owned()),
                },
            );
        }
        Ok(self.load(path))
    }

    fn load_hash<T: Asset>(&self, hash: &str, ty: EntityType) -> Handle<T> {
        let ext = ty.ext();
        let path = format!("$ipfs//$entity//{hash}.{ext}");
        self.load(path)
    }

    fn active_endpoint(&self) -> Option<String> {
        let ipfs_io = self.asset_io().downcast_ref::<IpfsIo>().unwrap();
        ipfs_io
            .realm_config_receiver
            .borrow()
            .as_ref()
            .and_then(|(_, about)| about.content.as_ref())
            .map(|content| format!("{}/entities/active", &content.public_url))
    }

    fn ipfs(&self) -> &IpfsIo {
        self.asset_io().downcast_ref().unwrap()
    }

    fn ipfs_cache_path(&self) -> &Path {
        self.ipfs().cache_path()
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct EndpointConfig {
    pub healthy: bool,
    #[serde(rename = "publicUrl")]
    pub public_url: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CommsConfig {
    pub healthy: bool,
    pub protocol: String,
    #[serde(rename = "fixedAdapter")]
    pub fixed_adapter: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct ServerConfiguration {
    #[serde(rename = "scenesUrn")]
    pub scenes_urn: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ServerAbout {
    pub content: Option<EndpointConfig>,
    pub comms: Option<CommsConfig>,
    pub configurations: Option<ServerConfiguration>,
}

impl Default for ServerAbout {
    fn default() -> Self {
        Self {
            content: None,
            comms: Some(CommsConfig {
                healthy: true,
                protocol: "v3".to_owned(),
                fixed_adapter: Some("offline:offline".to_owned()),
            }),
            configurations: Default::default(),
        }
    }
}

pub struct IpfsIoPlugin {
    pub cache_root: Option<String>,
    pub starting_realm: Option<String>,
}

impl Plugin for IpfsIoPlugin {
    fn build(&self, app: &mut App) {
        let default_io = AssetPlugin {
            asset_folder: self.cache_root.clone().unwrap_or("assets".to_owned()),
            ..Default::default()
        }
        .create_platform_default_asset_io();

        // TODO this will fail on android and wasm, investigate a caching solution there
        let default_fs_path = default_io
            .downcast_ref::<FileAssetIo>()
            .unwrap()
            .root_path()
            .clone();

        // create the custom asset io instance
        info!("remote server: {:?}", self.starting_realm);

        let ipfs_io = IpfsIo::new(default_io, default_fs_path);

        // the asset server is constructed and added the resource manager
        app.insert_resource(AssetServer::new(ipfs_io))
            .add_asset::<EntityDefinition>()
            .add_asset::<SceneJsFile>()
            .init_asset_loader::<EntityDefinitionLoader>()
            .init_asset_loader::<SceneJsLoader>();

        app.add_event::<ChangeRealmEvent>();
        app.init_resource::<CurrentRealm>();
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

        app.add_console_command::<ChangeRealmCommand, _>(change_realm_command);
    }
}

/// Switch to a new realm
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/changerealm")]
struct ChangeRealmCommand {
    new_realm: String,
}

fn change_realm_command(
    mut input: ConsoleCommand<ChangeRealmCommand>,
    mut writer: EventWriter<ChangeRealmEvent>,
) {
    if let Some(Ok(command)) = input.take() {
        writer.send(ChangeRealmEvent {
            new_realm: command.new_realm,
        });
        input.ok();
    }
}

pub struct ChangeRealmEvent {
    pub new_realm: String,
}

#[derive(Resource, Default)]
pub struct CurrentRealm {
    pub address: String,
    pub config: ServerConfiguration,
    pub comms: Option<CommsConfig>,
}

#[allow(clippy::type_complexity)]
fn change_realm(
    mut change_realm_requests: EventReader<ChangeRealmEvent>,
    asset_server: Res<AssetServer>,
    mut realm_change: Local<Option<tokio::sync::watch::Receiver<Option<(String, ServerAbout)>>>>,
    mut current_realm: ResMut<CurrentRealm>,
    mut print: EventWriter<PrintConsoleLine>,
) {
    let ipfsio = asset_server.asset_io().downcast_ref::<IpfsIo>().unwrap();
    match *realm_change {
        None => *realm_change = Some(ipfsio.realm_config_receiver.clone()),
        Some(ref mut realm_change) => {
            if realm_change.has_changed().unwrap_or_default() {
                if let Some((realm, about)) = &*realm_change.borrow_and_update() {
                    *current_realm = CurrentRealm {
                        address: realm.clone(),
                        config: about.configurations.clone().unwrap_or_default(),
                        comms: about.comms.clone(),
                    };

                    match about.configurations {
                        Some(_) => print.send(PrintConsoleLine::new(
                            format!("Realm set to `{realm}`").into(),
                        )),
                        None => print.send(PrintConsoleLine::new(
                            format!("Failed to set realm `{realm}`").into(),
                        )),
                    }
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

pub struct IpfsModifier {
    pub base_url: Option<String>,
}

#[derive(Default)]
pub struct IpfsContext {
    collections: HashMap<String, ContentMap>,
    base_url: Option<String>,
    modifiers: HashMap<String, IpfsModifier>,
}

pub struct IpfsIo {
    default_io: Box<dyn AssetIo>,
    default_fs_path: PathBuf,
    pub realm_config_receiver: tokio::sync::watch::Receiver<Option<(String, ServerAbout)>>,
    realm_config_sender: tokio::sync::watch::Sender<Option<(String, ServerAbout)>>,
    context: RwLock<IpfsContext>,
    request_slots: tokio::sync::Semaphore,
    reqno: AtomicU16,
}

impl IpfsIo {
    pub fn new(default_io: Box<dyn AssetIo>, default_fs_path: PathBuf) -> Self {
        let (sender, receiver) = tokio::sync::watch::channel(None);

        Self {
            default_io,
            default_fs_path,
            realm_config_receiver: receiver,
            realm_config_sender: sender,
            context: Default::default(),
            request_slots: tokio::sync::Semaphore::new(MAX_CONCURRENT_REQUESTS),
            reqno: default(),
        }
    }

    pub async fn set_realm(&self, new_realm: String) {
        let res = self.set_realm_inner(new_realm.clone()).await;
        if let Err(e) = res {
            error!("failed to set realm: {e}");
            self.realm_config_sender
                .send(Some((new_realm, Default::default())))
                .expect("channel closed");
        }
    }

    pub fn set_realm_about(&self, about: ServerAbout) {
        self.context.blocking_write().base_url = about
            .content
            .as_ref()
            .map(|endpoint| endpoint.public_url.clone());
        self.realm_config_sender
            .send(Some(("manual value".to_owned(), about)))
            .expect("channel closed");
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
            .send(Some((new_realm, about)))
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

    pub fn add_collection(
        &self,
        hash: String,
        collection: ContentMap,
        modifier: Option<IpfsModifier>,
    ) {
        let mut write = self.context.blocking_write();

        if let Some(modifier) = modifier {
            write.modifiers.insert(hash.clone(), modifier);
        }
        write.collections.insert(hash, collection);
    }

    pub fn cache_path(&self) -> &Path {
        self.default_fs_path.as_path()
    }

    // load entities from pointers and cache urls
    pub fn active_entities(
        &self,
        pointers: &Vec<String>,
        endpoint: Option<&str>,
    ) -> ActiveEntityTask {
        let active_url = match endpoint {
            Some(url) => Some(url.to_owned()),
            None => self
                .realm_config_receiver
                .borrow()
                .as_ref()
                .and_then(|(_, about)| about.content.as_ref())
                .map(|content| content.public_url.to_owned()),
        }
        .map(|url| format!("{url}/entities/active"));

        let body = serde_json::to_string(&ActiveEntitiesRequest { pointers });
        let cache_path = self.cache_path().to_owned();

        IoTaskPool::get().spawn(async move {
            let active_url = active_url.ok_or(anyhow!("not connected"))?;

            let body = body?;
            let mut response = isahc::Request::post(active_url)
                .header("content-type", "application/json")
                .body(body)?
                .send_async()
                .await?;

            if response.status() != StatusCode::OK {
                return Err(anyhow::anyhow!("status: {}", response.status()));
            }

            let active_entities = response
                .json::<ActiveEntitiesResponse>()
                .await
                .map_err(|e| anyhow::anyhow!(e))?;
            let mut res = Vec::default();
            for entity in active_entities.0 {
                let id = entity.id.as_ref().unwrap();
                // cache to file system
                let cache_path = cache_path.join(id);

                if id.starts_with("b64-") || !cache_path.exists() {
                    let file = std::fs::File::create(&cache_path)?;
                    serde_json::to_writer(file, &entity)?;
                }

                // return active entity struct
                res.push(EntityDefinition {
                    id: entity.id.unwrap(),
                    pointers: entity.pointers,
                    metadata: entity.metadata,
                    content: ContentMap(BiMap::from_iter(
                        entity
                            .content
                            .into_iter()
                            .map(|ipfs| (normalize_path(&ipfs.file), ipfs.hash)),
                    )),
                });
            }

            Ok(res)
        })
    }
}

pub type ActiveEntityTask = Task<Result<Vec<EntityDefinition>, anyhow::Error>>;

#[derive(Debug)]
pub struct ActiveEntity {
    pub id: String,
    pub pointers: Vec<String>,
    pub content: ContentMap,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct ActiveEntitiesRequest<'a> {
    pointers: &'a Vec<String>,
}

#[derive(Deserialize, Debug)]
pub struct ActiveEntitiesResponse(Vec<EntityDefinitionJson>);

impl AssetIo for IpfsIo {
    fn load_path<'a>(
        &'a self,
        path: &'a std::path::Path,
    ) -> bevy::utils::BoxedFuture<'a, Result<Vec<u8>, bevy::asset::AssetIoError>> {
        Box::pin(async move {
            let wrap_err = |e| {
                bevy::asset::AssetIoError::Io(std::io::Error::new(
                    ErrorKind::Other,
                    format!("w: {e}"),
                ))
            };

            debug!("request: {:?}", path);

            let maybe_ipfs_path = IpfsPath::new_from_path(path).map_err(wrap_err)?;
            debug!("ipfs: {maybe_ipfs_path:?}");
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

                // get semaphore to limit concurrent requests
                let _permit = self
                    .request_slots
                    .acquire()
                    .await
                    .map_err(|e| AssetIoError::Io(std::io::Error::new(ErrorKind::Other, e)))?;
                let token = self.reqno.fetch_add(1, atomic::Ordering::SeqCst);

                // wait till connected
                self.connected().await.map_err(wrap_err)?;

                let remote = ipfs_path
                    .to_url(&*self.context.read().await)
                    .map_err(wrap_err)?;

                debug!("[{token:?}]: remote url: `{remote}`");

                let mut attempt = 0;
                let data = loop {
                    attempt += 1;

                    let request = isahc::Request::get(&remote)
                        .connect_timeout(Duration::from_secs(5 * attempt))
                        .timeout(Duration::from_secs(30 * attempt))
                        .body(())
                        .map_err(|e| {
                            AssetIoError::Io(std::io::Error::new(
                                ErrorKind::Other,
                                format!("[{token:?}]: {e}"),
                            ))
                        })?;

                    let response = request.send_async().await;

                    debug!("[{token:?}]: attempt {attempt}: response: {response:?}");

                    let mut response = match response {
                        Err(e) if e.is_timeout() && attempt <= 3 => continue,
                        Err(e) => {
                            return Err(AssetIoError::Io(std::io::Error::new(
                                ErrorKind::Other,
                                format!("[{token:?}]: {e}"),
                            )))
                        }
                        Ok(response) if !matches!(response.status(), StatusCode::OK) => {
                            return Err(AssetIoError::Io(std::io::Error::new(
                                ErrorKind::Other,
                                format!(
                                    "[{token:?}]: server responded with status {} requesting `{}`",
                                    response.status(),
                                    remote,
                                ),
                            )))
                        }
                        Ok(response) => response,
                    };

                    let data = response.bytes().await;

                    match data {
                        Ok(data) => break data,
                        Err(e) => {
                            if matches!(e.kind(), std::io::ErrorKind::TimedOut) && attempt <= 3 {
                                continue;
                            }
                            return Err(AssetIoError::Io(std::io::Error::new(
                                ErrorKind::Other,
                                format!("[{token:?}] {e}"),
                            )));
                        }
                    }
                };

                if ipfs_path.should_cache() {
                    if let Some(hash) = hash {
                        let mut cache_path = PathBuf::from(self.cache_path());
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

                debug!("[{token:?}]: completed remote url: `{remote}`");
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
