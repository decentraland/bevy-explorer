pub mod ipfs_path;

use std::{
    borrow::Cow,
    io::ErrorKind,
    marker::PhantomData,
    path::{Path, PathBuf},
    sync::{
        atomic::{self, AtomicU16},
        Arc,
    },
    time::{Duration, Instant, SystemTime},
};

use anyhow::anyhow;
use async_std::io::{Cursor, ReadExt, WriteExt};
use bevy::{
    asset::{
        io::{
            AssetReader, AssetReaderError, AssetSource, AssetSourceId, ErasedAssetReader, Reader
        },
        meta::Settings,
        Asset, AssetLoader, LoadState, UntypedAssetId,
    },
    ecs::system::SystemParam,
    prelude::*,
    reflect::TypePath,
    tasks::{IoTaskPool, Task},
    utils::{ConditionalSendFuture, HashMap},
};

#[cfg(feature = "native")]
use bevy::asset::io::file::FileAssetReader;

#[cfg(feature = "wasm")]
use bevy::asset::io::wasm::HttpWasmAssetReader;

use bevy_console::{ConsoleCommand, PrintConsoleLine};
use common::{
    structs::AppConfig,
    util::{project_directories, TaskCompat},
};
use ipfs_path::IpfsAsset;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use console::DoAddConsoleCommand;

#[allow(unused_imports)]
use platform::ReqwestBuilderExt;

use self::ipfs_path::{normalize_path, IpfsPath, IpfsType};

#[derive(Serialize, Deserialize, Debug)]
pub struct TypedIpfsRef {
    pub file: String,
    pub hash: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EntityDefinitionJson {
    pub id: Option<String>,
    pub pointers: Vec<String>,
    pub content: Vec<TypedIpfsRef>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Asset, Debug, Default, TypePath)]
pub struct EntityDefinition {
    pub id: String,
    pub pointers: Vec<String>,
    pub content: ContentMap,
    pub metadata: Option<serde_json::Value>,
}

impl IpfsAsset for EntityDefinition {
    fn ext() -> &'static str {
        "entity_definition"
    }
}

#[derive(Asset, Debug, Clone, TypePath)]
pub struct SceneJsFile(pub Arc<String>);

impl IpfsAsset for SceneJsFile {
    fn ext() -> &'static str {
        "js"
    }
}

#[derive(Default)]
pub struct EntityDefinitionLoader;

impl EntityDefinitionLoader {
    pub fn load_internal<'a>(
        &'a self,
        reader: &'a mut Reader,
        _settings: &'a (),
        id_fn: impl FnOnce() -> String + Send + Sync + 'a,
    ) -> bevy::utils::BoxedFuture<'a, Result<EntityDefinition, anyhow::Error>> {
        Box::pin(async move {
            let mut bytes = Vec::default();
            reader.read_to_end(&mut bytes).await?;

            let maybe_definition_json = {
                // try to parse as a vec
                let definition_json_vec: Result<Vec<EntityDefinitionJson>, _> =
                    serde_json::from_reader(bytes.as_slice());
                match definition_json_vec {
                    Ok(mut vec) => vec.pop(),
                    Err(_) => {
                        // else try to parse as a single item
                        Some(serde_json::from_reader(bytes.as_slice())?)
                    }
                }
            };
            let Some(definition_json) = maybe_definition_json else {
                // if the source was an empty vec, we have loaded a pointer with no content, just set default
                return Ok(EntityDefinition::default());
            };
            let content =
                ContentMap(HashMap::from_iter(definition_json.content.into_iter().map(
                    |ipfs| (normalize_path(&ipfs.file).to_lowercase(), ipfs.hash),
                )));
            let id = definition_json.id.unwrap_or_else(id_fn);

            let definition = EntityDefinition {
                id,
                pointers: definition_json.pointers,
                content,
                metadata: definition_json.metadata,
            };
            Ok(definition)
        })
    }
}

impl AssetLoader for EntityDefinitionLoader {
    type Asset = EntityDefinition;
    type Settings = ();
    type Error = anyhow::Error;

    fn load<'a>(
        &'a self,
        reader: &'a mut Reader,
        settings: &'a Self::Settings,
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> impl ConditionalSendFuture<Output = Result<Self::Asset, Self::Error>> {
        let id_fn = || {
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
        };

        self.load_internal(reader, settings, id_fn)
    }

    fn extensions(&self) -> &[&str] {
        &["entity_definition"]
    }
}

#[derive(Default)]
pub struct SceneJsLoader;

impl AssetLoader for SceneJsLoader {
    type Asset = SceneJsFile;
    type Settings = ();
    type Error = std::io::Error;

    fn load<'a>(
        &'a self,
        reader: &'a mut Reader,
        _settings: &'a Self::Settings,
        _load_context: &'a mut bevy::asset::LoadContext,
    ) -> impl ConditionalSendFuture<Output = Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            let mut bytes = Vec::default();
            reader.read_to_end(&mut bytes).await?;
            Ok(SceneJsFile(Arc::new(String::from_utf8(bytes).map_err(
                |e| std::io::Error::new(ErrorKind::InvalidData, e),
            )?)))
        })
    }

    fn extensions(&self) -> &[&str] {
        &["js"]
    }
}

#[derive(Debug, Clone, Default)]
pub struct ContentMap(HashMap<String, String>);

impl ContentMap {
    pub fn hash<'a>(&'a self, file: &str) -> Option<Cow<'a, str>> {
        self.0.get(file.to_lowercase().as_str()).map(Into::into)
    }

    pub fn files(&self) -> impl Iterator<Item = &String> {
        self.0.keys()
    }

    pub fn values(&self) -> impl Iterator<Item = (&String, &String)> {
        self.0.iter()
    }

    pub fn new_single(file: String, hash: String) -> Self {
        Self(HashMap::from_iter([(file, hash)]))
    }

    pub fn with(mut self, file: String, hash: String) -> Self {
        self.0.insert(file.to_lowercase(), hash);
        self
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum SceneIpfsLocation {
    Hash(String),
    Urn(String),
}

#[derive(Resource, Clone)]
pub struct IpfsResource {
    pub inner: Arc<IpfsIo>,
}

impl std::ops::Deref for IpfsResource {
    type Target = IpfsIo;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[derive(SystemParam)]
pub struct IpfsAssetServer<'w, 's> {
    server: Res<'w, AssetServer>,
    ipfs: Res<'w, IpfsResource>,
    _p: PhantomData<&'s ()>,
}

impl IpfsAssetServer<'_, '_> {
    pub fn load_content_file<T: Asset>(
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
        // let path = format!("$ipfs/$entity//{hash}.{file_name}");
        // Ok(self.load(path))
        let ipfs_path = IpfsPath::new(IpfsType::new_content_file(
            content_hash.to_owned(),
            file_path.to_owned(),
        ));
        Ok(self.server.load(PathBuf::from(&ipfs_path)))
    }

    pub fn load_content_file_with_settings<T: Asset, S: Settings>(
        &self,
        file_path: &str,
        content_hash: &str,
        settings: impl Fn(&mut S) + Send + Sync + 'static,
    ) -> Result<Handle<T>, anyhow::Error> {
        let ipfs_path = IpfsPath::new(IpfsType::new_content_file(
            content_hash.to_owned(),
            file_path.to_owned(),
        ));
        Ok(self
            .server
            .load_with_settings(PathBuf::from(&ipfs_path), settings))
    }

    pub fn load_urn<T: IpfsAsset>(&self, urn: &str) -> Result<Handle<T>, anyhow::Error> {
        let ext = T::ext();
        let ipfs_path = IpfsPath::new_from_urn::<T>(urn)?;
        let hash = ipfs_path
            .context_free_hash()?
            .ok_or(anyhow::anyhow!("urn did not resolve to a hash"))?;
        let path = format!("$ipfs/$entity/{hash}.{ext}");

        if let Some(base_url) = ipfs_path.base_url() {
            // update the context
            let ipfs_io = self.ipfs();
            let mut context = ipfs_io.context.blocking_write();
            context.modifiers.insert(
                hash,
                IpfsModifier {
                    base_url: Some(base_url.to_owned()),
                },
            );
        }
        Ok(self.server.load(path))
    }

    pub fn load_url_uncached<T: IpfsAsset>(&self, url: &str) -> Handle<T> {
        let ext = T::ext();
        let ipfs_path = IpfsPath::new_from_url_uncached(url, ext);
        self.server.load(PathBuf::from(&ipfs_path))
    }

    pub fn load_hash<T: IpfsAsset>(&self, hash: &str) -> Handle<T> {
        let ext = T::ext();
        let path = format!("$ipfs/$entity/{hash}.{ext}");
        self.server.load(path)
    }

    pub fn active_endpoint(&self) -> Option<String> {
        self.ipfs()
            .realm_config_receiver
            .borrow()
            .as_ref()
            .and_then(|(_, _, about)| about.content.as_ref())
            .map(|content| format!("{}/entities/active", &content.public_url))
    }

    pub fn ipfs(&self) -> &Arc<IpfsIo> {
        &self.ipfs.inner
    }

    pub fn asset_server(&self) -> &AssetServer {
        &self.server
    }

    pub fn ipfs_cache_path(&self) -> &Path {
        self.ipfs().cache_path()
    }

    pub fn load_state(&self, id: impl Into<UntypedAssetId>) -> LoadState {
        self.server.load_state(id)
    }

    pub fn is_connected(&self) -> bool {
        self.ipfs.realm_config_receiver.borrow().is_some()
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EndpointConfig {
    pub healthy: bool,
    pub public_url: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommsConfig {
    pub healthy: bool,
    pub protocol: String,
    pub fixed_adapter: Option<String>,
    pub adapter: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    pub left: i32,
    pub right: i32,
    pub top: i32,
    pub bottom: i32,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct MapData {
    pub minimap_enabled: Option<bool>,
    pub sizes: Vec<Region>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfiguration {
    pub scenes_urn: Option<Vec<String>>,
    pub realm_name: Option<String>,
    pub network_id: Option<u32>,
    pub city_loader_content_server: Option<String>,
    pub map: Option<MapData>,
    pub local_scene_parcels: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ServerAbout {
    pub content: Option<EndpointConfig>,
    pub comms: Option<CommsConfig>,
    pub configurations: Option<ServerConfiguration>,
    pub lambdas: Option<EndpointConfig>,
}

impl ServerAbout {
    pub fn content_url(&self) -> Option<&str> {
        self.content.as_ref().map(|c| c.public_url.as_str())
    }
}

impl Default for ServerAbout {
    fn default() -> Self {
        Self {
            content: None,
            comms: Some(CommsConfig {
                healthy: true,
                protocol: "v3".to_owned(),
                fixed_adapter: Some("offline:offline".to_owned()),
                adapter: None,
            }),
            configurations: Default::default(),
            lambdas: Default::default(),
        }
    }
}

pub struct IpfsIoPlugin {
    pub preview: bool,
    pub assets_root: Option<String>,
    pub starting_realm: Option<String>,
    pub content_server_override: Option<String>,
    pub num_slots: usize,
}

impl Plugin for IpfsIoPlugin {
    fn build(&self, app: &mut App) {
        info!("remote server: {:?}", self.starting_realm);

        let file_path = self.assets_root.clone().unwrap_or("assets".to_owned());
        #[cfg(feature="native")]
        let default_reader = FileAssetReader::new(file_path.clone());
        #[cfg(feature="wasm")]
        let default_reader = HttpWasmAssetReader::new(file_path.clone());
        let cache_root = if self.assets_root.is_some() {
            // use app folder for unit tests
            default_reader.root_path().join("cache")
        } else {
            project_directories().data_local_dir().join("cache")
        };
        info!("cache folder {cache_root:?}");
        std::fs::create_dir_all(&cache_root)
            .unwrap_or_else(|_| panic!("failed to write to assets folder {cache_root:?}"));

        let ipfs_io = IpfsIo::new(
            self.preview,
            Box::new(default_reader),
            cache_root,
            HashMap::default(),
            self.num_slots,
        );
        let ipfs_io = Arc::new(ipfs_io);
        let passthrough = PassThroughReader {
            inner: ipfs_io.clone(),
        };

        app.insert_resource(IpfsResource { inner: ipfs_io });

        #[cfg(feature = "hot_reload")]
        app.register_asset_source(
            AssetSourceId::Default,
            AssetSource::build()
                .with_reader(move || Box::new(passthrough.clone()))
                .with_watcher(AssetSource::get_default_watcher(
                    file_path,
                    Duration::from_millis(300),
                )),
        );
        #[cfg(not(feature = "hot_reload"))]
        app.register_asset_source(
            AssetSourceId::Default,
            AssetSource::build().with_reader(move || Box::new(passthrough.clone())),
        );

        app.add_event::<ChangeRealmEvent>();
        app.init_resource::<CurrentRealm>();
        app.add_systems(PostUpdate, (change_realm, clean_cache));

        app.add_console_command::<ChangeRealmCommand, _>(change_realm_command);
    }

    fn finish(&self, app: &mut App) {
        app.init_asset::<EntityDefinition>()
            .init_asset::<SceneJsFile>()
            .init_asset_loader::<EntityDefinitionLoader>()
            .init_asset_loader::<SceneJsLoader>();

        if let Some(realm) = &self.starting_realm {
            let ipfs = app.world().resource::<IpfsResource>().clone();
            let realm = realm.clone();
            let content_server_override = self.content_server_override.clone();
            IoTaskPool::get()
                .spawn_compat(async move {
                    ipfs.set_realm(realm, content_server_override).await;
                })
                .detach();
        }
    }
}

/// Switch to a new realm
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/changerealm")]
struct ChangeRealmCommand {
    new_realm: String,
    content_server_override: Option<String>,
}

fn change_realm_command(
    mut input: ConsoleCommand<ChangeRealmCommand>,
    mut writer: EventWriter<ChangeRealmEvent>,
) {
    if let Some(Ok(command)) = input.take() {
        writer.send(ChangeRealmEvent {
            new_realm: command.new_realm,
            content_server_override: command.content_server_override,
        });
        input.ok();
    }
}

#[derive(Event, Clone)]
pub struct ChangeRealmEvent {
    pub new_realm: String,
    pub content_server_override: Option<String>,
}

#[derive(Resource, Default, Debug)]
pub struct CurrentRealm {
    pub about_url: String,
    pub address: String,
    pub config: ServerConfiguration,
    pub comms: Option<CommsConfig>,
    pub public_url: String,
}

#[allow(clippy::type_complexity)]
pub fn change_realm(
    mut change_realm_requests: EventReader<ChangeRealmEvent>,
    ipfs: Res<IpfsResource>,
    mut realm_change: Local<
        Option<tokio::sync::watch::Receiver<Option<(String, String, ServerAbout)>>>,
    >,
    mut current_realm: ResMut<CurrentRealm>,
    mut print: EventWriter<PrintConsoleLine>,
) {
    match *realm_change {
        None => *realm_change = Some(ipfs.realm_config_receiver.clone()),
        Some(ref mut realm_change) => {
            if realm_change.has_changed().unwrap_or_default() {
                if let Some((about_url, realm, about)) = &*realm_change.borrow_and_update() {
                    *current_realm = CurrentRealm {
                        about_url: about_url.clone(),
                        address: realm.clone(),
                        config: about.configurations.clone().unwrap_or_default(),
                        comms: about.comms.clone(),
                        public_url: about
                            .content
                            .as_ref()
                            .map(|c| c.public_url.clone())
                            .unwrap_or_default(),
                    };

                    match about.configurations {
                        Some(_) => print.send(PrintConsoleLine::new(
                            format!("Realm set to `{realm}`").into(),
                        )),
                        None => print.send(PrintConsoleLine::new(
                            format!("Failed to set realm `{realm}`").into(),
                        )),
                    };
                }
            }
        }
    }

    if !change_realm_requests.is_empty() {
        let ipfs = ipfs.clone();
        let request = change_realm_requests.read().last().unwrap();

        let new_realm = &request.new_realm;
        let new_realm = if new_realm.ends_with(".dcl.eth") && !new_realm.starts_with("https://") {
            format!("https://worlds-content-server.decentraland.org/world/{new_realm}")
        } else {
            new_realm.to_owned()
        };
        let content_server_override = request.content_server_override.to_owned();
        IoTaskPool::get()
            .spawn_compat(async move {
                ipfs.set_realm(new_realm, content_server_override).await;
            })
            .detach();
    }
}

pub struct IpfsModifier {
    pub base_url: Option<String>,
}

#[derive(Clone)]
pub struct IpfsEntity {
    pub collection: ContentMap,
    pub metadata: Option<String>,
}

#[derive(Default)]
pub struct IpfsContext {
    about_url: String,
    base_url: String,
    entities: HashMap<String, IpfsEntity>,
    about: Option<ServerAbout>,
    modifiers: HashMap<String, IpfsModifier>,
    failed_remotes: HashMap<String, Instant>,
    num_slots: usize,
}

fn clean_cache(mut exit: EventReader<AppExit>, config: Res<AppConfig>, ipfas: IpfsAssetServer) {
    if exit.read().last().is_some() {
        ipfas.ipfs().trim_cache(config.cache_bytes);
    }
}

pub struct IpfsIo {
    is_preview: bool, // determines whether we always retry failed assets immediately
    default_io: Box<dyn ErasedAssetReader>,
    default_fs_path: PathBuf,
    realm_config_receiver: tokio::sync::watch::Receiver<Option<(String, String, ServerAbout)>>,
    realm_config_sender: tokio::sync::watch::Sender<Option<(String, String, ServerAbout)>>,
    context: RwLock<IpfsContext>,
    request_slots: tokio::sync::Semaphore,
    reqno: AtomicU16,
    static_files: HashMap<&'static str, &'static str>,
    client: reqwest::Client,
}

impl IpfsIo {
    pub fn new(
        is_preview: bool,
        default_io: Box<dyn ErasedAssetReader>,
        default_fs_path: PathBuf,
        static_paths: HashMap<&'static str, &'static str>,
        num_slots: usize,
    ) -> Self {
        let (sender, receiver) = tokio::sync::watch::channel(None);

        Self {
            is_preview,
            default_io,
            default_fs_path,
            realm_config_receiver: receiver,
            realm_config_sender: sender,
            context: RwLock::new(IpfsContext {
                num_slots,
                ..Default::default()
            }),
            request_slots: tokio::sync::Semaphore::new(num_slots),
            reqno: default(),
            static_files: static_paths,
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .use_native_tls()
                .user_agent("DCLExplorer/0.1")
                .build()
                .unwrap(),
        }
    }

    pub fn trim_cache(&self, max_size: u64) {
        let Ok(folder) = std::fs::read_dir(self.cache_path()) else {
            return;
        };

        let mut files = folder
            .filter_map(|f| {
                let Ok(f) = f else { return None };

                let Ok(metadata) = f.metadata() else {
                    return None;
                };

                if metadata.is_file() {
                    let accessed = metadata.accessed().unwrap_or(SystemTime::UNIX_EPOCH);
                    Some((accessed, (metadata.len(), f.path())))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        files.sort_by_key(|(time, _)| *time);

        let mut total_size = 0;
        for (_, (_, to_delete)) in files.iter().rev().skip_while(|(_, (size, path))| {
            total_size += *size;
            debug!("keeping {path:?}, total now {total_size}/{max_size}");
            total_size < max_size
        }) {
            if let Err(e) = std::fs::remove_file(to_delete) {
                warn!("failed to remove cache file {to_delete:?}: {e}");
            }
        }
    }

    pub fn set_concurrent_remote_count(&self, count: usize) {
        let mut context = self.context.blocking_write();
        if count == context.num_slots {
            return;
        }

        if count > context.num_slots {
            self.request_slots.add_permits(count - context.num_slots);
            context.num_slots = count;
        } else {
            let freed = self.request_slots.forget_permits(context.num_slots - count);
            if freed != context.num_slots - count {
                // we couldn't free enough permits, never mind
                warn!(
                    "could only free {freed} permits, will catch up on settings apply or restart"
                );
            }
            context.num_slots -= freed;
        }
    }

    pub async fn set_realm(&self, new_realm: String, content_server_override: Option<String>) {
        let res = self
            .set_realm_inner(new_realm.clone(), content_server_override)
            .await;
        if let Err(e) = res {
            error!("failed to set realm: {e}");
            self.realm_config_sender
                .send(Some((new_realm.clone(), new_realm, Default::default())))
                .expect("channel closed");
        }
    }

    pub fn set_realm_about(&self, about: ServerAbout) {
        let mut write = self.context.blocking_write();
        write.base_url = String::default();
        write.about = Some(about.clone());
        self.realm_config_sender
            .send(Some((
                "manual value".to_owned(),
                "manual value".to_owned(),
                about,
            )))
            .expect("channel closed");
    }

    pub async fn get_realm_info(&self) -> (String, Option<ServerAbout>) {
        let context = self.context.read().await;
        (context.base_url.clone(), context.about.clone())
    }

    async fn set_realm_inner(
        &self,
        new_realm: String,
        content_server_override: Option<String>,
    ) -> Result<(), anyhow::Error> {
        self.realm_config_sender.send(None).expect("channel closed");
        let mut write = self.context.write().await;
        if write.about.is_some() {
            info!("disconnecting");
        }

        write.about = None;
        drop(write);

        let mut retries = 0;
        let mut about;
        let mut final_url;
        loop {
            let about_raw = self
                .client
                .get(format!("{new_realm}/about"))
                .send()
                .await
                .map_err(|e| anyhow!(e))?;
            if about_raw.status() != StatusCode::OK {
                return Err(anyhow!("status: {}", about_raw.status()));
            }
            final_url = about_raw.url().to_string();

            about = about_raw
                .json::<ServerAbout>()
                .await
                .map_err(|e| anyhow!(e))?;
            if about.configurations.as_ref().is_some_and(|config| {
                config
                    .scenes_urn
                    .as_ref()
                    .is_some_and(|scenes| !scenes.is_empty())
                    || config.map.is_some()
            }) {
                break;
            }
            // with no scenes and no map data, we will not have much of value
            // sometimes this occurs for misdeployed load balancers, so let's retry a couple of times
            retries += 1;
            if retries == 3 {
                break;
            }
        }

        let mut write = self.context.write().await;
        if let (Some(cs), Some(content)) = (&content_server_override, about.content.as_mut()) {
            content.public_url = format!("{cs}/content/");
        }
        let base_url = content_server_override
            .as_deref()
            .or_else(|| about.content_url())
            .map(|c| c.strip_suffix("/content/").unwrap_or(c));
        write.base_url = base_url.unwrap_or(&final_url).to_owned();
        write.about_url = final_url.clone();
        write.about = Some(about.clone());
        self.realm_config_sender
            .send(Some((final_url, write.base_url.clone(), about)))
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
        metadata: Option<String>,
    ) {
        let mut write = self.context.blocking_write();

        let entity = IpfsEntity {
            collection,
            metadata,
        };

        if let Some(modifier) = modifier {
            write.modifiers.insert(hash.clone(), modifier);
        }

        write.entities.insert(hash, entity);
    }

    pub fn cache_path(&self) -> &Path {
        self.default_fs_path.as_path()
    }

    // load entities from pointers and cache urls
    pub fn active_entities(
        self: &Arc<Self>,
        request: ActiveEntitiesRequest,
        endpoint: Option<&str>,
    ) -> ActiveEntityTask {
        let active_url = match endpoint {
            Some(url) => Some(url.to_owned()),
            None => self
                .realm_config_receiver
                .borrow()
                .as_ref()
                .and_then(|(_, _, about)| about.content.as_ref())
                .map(|content| content.public_url.to_owned()),
        }
        .map(|url| format!("{url}/entities/active"));

        let cache_path = self.cache_path().to_owned();

        match request {
            ActiveEntitiesRequest::Pointers(pointers) => {
                let client = self.client.clone();
                IoTaskPool::get().spawn_compat(async move {
                    let active_url = active_url.ok_or(anyhow!("not connected"))?;
                    let body = serde_json::to_string(&ActiveEntitiesPointersRequest { pointers })?;
                    let response = client
                        .post(active_url)
                        .header("content-type", "application/json")
                        .body(body)
                        .send()
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
                            let mut file = async_fs::File::create(&cache_path).await?;
                            let mut buf = Vec::default();
                            serde_json::to_writer(&mut buf, &entity)?;
                            file.write_all(&buf).await?;
                            file.sync_all().await?;
                            // let file = std::fs::File::create(&cache_path)?;
                            // serde_json::to_writer(file, &entity)?;
                        }

                        // return active entity struct
                        res.push(EntityDefinition {
                            id: entity.id.unwrap(),
                            pointers: entity.pointers,
                            metadata: entity.metadata,
                            content: ContentMap(HashMap::from_iter(
                                entity.content.into_iter().map(|ipfs| {
                                    (normalize_path(&ipfs.file).to_lowercase(), ipfs.hash)
                                }),
                            )),
                        });
                    }

                    Ok(res)
                })
            }
            ActiveEntitiesRequest::Urns(paths) => {
                // we simulate the active entities functionality we want here (load all scenes from a set of urns)
                let ipfs = self.clone();
                IoTaskPool::get().spawn_compat(async move {
                    let mut results = Vec::default();
                    let loader = EntityDefinitionLoader;

                    for path in paths.iter() {
                        let hash = path.context_free_hash().unwrap().unwrap();
                        let pathbuf = std::path::PathBuf::from(path);
                        let Ok(mut reader) = AssetReader::read(&*ipfs, &pathbuf).await else {
                            continue;
                        };

                        if let Ok(ent) = loader.load_internal(&mut reader, &(), || hash).await {
                            results.push(ent)
                        }
                    }

                    Ok(results)
                })
            }
        }
    }

    pub fn client(&self) -> reqwest::Client {
        self.client.clone()
    }

    pub async fn async_request(
        &self,
        request: reqwest::Request,
        client: reqwest::Client,
    ) -> Result<reqwest::Response, anyhow::Error> {
        // get semaphore to limit concurrent requests
        let _permit = self.request_slots.acquire().await.map_err(|e| anyhow!(e))?;

        client.execute(request).await.map_err(|e| anyhow!(e))
    }

    pub async fn ipfs_hash(&self, ipfs_path: &IpfsPath) -> Option<String> {
        ipfs_path.hash(&*self.context.read().await)
    }

    pub async fn entity_definition(&self, hash: &str) -> Option<(IpfsEntity, String)> {
        let context = self.context.read().await;
        Some((
            context.entities.get(hash)?.clone(),
            context
                .modifiers
                .get(hash)
                .and_then(|m| m.base_url.as_deref())
                .or_else(|| context.about.as_ref().and_then(ServerAbout::content_url))
                .map(ToOwned::to_owned)
                .unwrap_or_default(),
        ))
    }

    pub fn base_urls(&self) -> HashMap<String, String> {
        self.context
            .blocking_read()
            .modifiers
            .iter()
            .filter_map(|(k, v)| {
                v.base_url
                    .as_ref()
                    .map(|burl| (k.to_owned(), burl.to_owned()))
            })
            .collect()
    }

    // note - blocking. use from a blockable thread
    pub fn content_url(&self, file_path: &str, content_hash: &str) -> Option<String> {
        let ipfs_path = IpfsPath::new(IpfsType::new_content_file(
            content_hash.to_owned(),
            file_path.to_owned(),
        ));
        let res = ipfs_path.to_url(&self.context.blocking_read()).ok();
        res
    }

    pub fn about_url(&self) -> Option<String> {
        self.realm_config_receiver
            .borrow()
            .as_ref()
            .map(|(about_url, _, _)| about_url.clone())
    }

    pub fn lambda_endpoint(&self) -> Option<String> {
        self.realm_config_receiver
            .borrow()
            .as_ref()
            .and_then(|(_, _, about)| about.lambdas.as_ref())
            .map(|l| l.public_url.clone())
    }

    pub fn contents_endpoint(&self) -> Option<String> {
        self.realm_config_receiver
            .borrow()
            .as_ref()
            .and_then(|(_, _, about)| about.content.as_ref())
            .map(|content| format!("{}/contents/", &content.public_url))
    }

    pub fn entities_endpoint(&self) -> Option<String> {
        self.realm_config_receiver
            .borrow()
            .as_ref()
            .and_then(|(_, _, about)| about.content.as_ref())
            .map(|content| format!("{}/entities/", &content.public_url))
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
struct ActiveEntitiesPointersRequest {
    pointers: Vec<String>,
}

pub enum ActiveEntitiesRequest {
    Pointers(Vec<String>),
    Urns(Vec<IpfsPath>),
}

#[derive(Deserialize, Debug)]
pub struct ActiveEntitiesResponse(Vec<EntityDefinitionJson>);

impl AssetReader for IpfsIo {
    fn read<'a>(
        &'a self,
        path: &'a std::path::Path,
    ) -> impl ConditionalSendFuture<Output = Result<Box<Reader<'a>>, bevy::asset::io::AssetReaderError>>
    {
        Box::pin(async_compat::Compat::new(async move {
            let wrap_err = |e| {
                bevy::asset::io::AssetReaderError::Io(Arc::new(std::io::Error::other(format!(
                    "w: {e}"
                ))))
            };

            debug!("request: {:?}", path);

            let maybe_ipfs_path = IpfsPath::new_from_path(path).map_err(wrap_err)?;
            debug!("ipfs: {maybe_ipfs_path:?}");
            let ipfs_path = match maybe_ipfs_path {
                Some(ipfs_path) => ipfs_path,
                // non-ipfs files are loaded as normal
                None => return self.default_io.read(path).await,
            };

            let hash = ipfs_path.hash(&*self.context.read().await);

            if let Some(hash) = &hash {
                debug!("hash: {}", hash);
                if !hash.starts_with("b64") {
                    if let Ok(mut res) = self.default_io.read(&self.cache_path().join(hash)).await {
                        let mut daft_buffer = Vec::default();
                        res.read_to_end(&mut daft_buffer).await?;
                        let reader: Box<Reader> = Box::new(Cursor::new(daft_buffer));
                        return Ok(reader);
                    }
                }
            };

            debug!(
                "remote ({})",
                hash.as_ref()
                    .map(|h| format!("hash {h} not found"))
                    .unwrap_or_else(|| "uncached".to_owned())
            );

            let token = self.reqno.fetch_add(1, atomic::Ordering::SeqCst);

            // wait till connected
            self.connected().await.map_err(wrap_err)?;

            let context = self.context.read().await;
            let remote = ipfs_path.to_url(&context).map_err(wrap_err);

            if remote.is_err() {
                // check for default file
                if let Some(static_path) = ipfs_path
                    .filename()
                    .and_then(|file_path| self.static_files.get(file_path.as_ref()))
                {
                    return self.default_io.read(Path::new(static_path)).await;
                }
            }
            let remote = remote?;

            let fail_time = context.failed_remotes.get(&remote).cloned();
            drop(context);

            if let Some(fail_time) = fail_time {
                // wait 10 secs before retrying failed assets
                if self.is_preview
                    || Instant::now()
                        .checked_duration_since(fail_time)
                        .unwrap_or_default()
                        > Duration::from_secs(10)
                {
                    self.context.write().await.failed_remotes.remove(&remote);
                } else {
                    return Err(AssetReaderError::Io(Arc::new(std::io::Error::other(
                        format!("(repeat request for failed `{remote}`)"),
                    ))));
                }
            }

            debug!("[{token:?}]: remote url: `{remote}` awaiting semaphore");
            // get semaphore to limit concurrent requests
            let _permit = self.request_slots.acquire().await.map_err(|e| {
                AssetReaderError::Io(Arc::new(std::io::Error::new(ErrorKind::Interrupted, e)))
            })?;
            debug!("[{token:?}]: remote url: `{remote}` proceeding");

            let mut attempt = 0;
            let mut no_cache = false;
            let data = loop {
                attempt += 1;

                let request = self
                    .client
                    .get(&remote)
                    .timeout(Duration::from_secs(5 + 30 * attempt))
                    .build()
                    .map_err(|e| {
                        AssetReaderError::Io(Arc::new(std::io::Error::other(format!(
                            "[{token:?}]: {e}"
                        ))))
                    })?;

                let response = self.client.execute(request).await;

                debug!("[{token:?}]: attempt {attempt}: request: {remote}, response: {response:?}");

                let response = match response {
                    Err(e) if e.is_timeout() && attempt <= 3 => {
                        warn!("[{token:?}] timeout requesting `{remote}`, retrying");
                        continue;
                    }
                    Err(e) => {
                        self.context
                            .write()
                            .await
                            .failed_remotes
                            .insert(remote.clone(), Instant::now());
                        return Err(AssetReaderError::Io(Arc::new(std::io::Error::other(
                            format!("[{token:?}]: server responded `{e}` requesting `{remote}`"),
                        ))));
                    }
                    Ok(response) if !matches!(response.status(), StatusCode::OK) => {
                        self.context
                            .write()
                            .await
                            .failed_remotes
                            .insert(remote.clone(), Instant::now());
                        return Err(AssetReaderError::Io(Arc::new(std::io::Error::other(
                            format!(
                                "[{token:?}]: server responded with status {} requesting `{}`",
                                response.status(),
                                remote,
                            ),
                        ))));
                    }
                    Ok(response) => response,
                };

                if let Some(cache_control) = response.headers().get("cache-control") {
                    if cache_control
                        .to_str()
                        .unwrap_or_default()
                        .contains("no-store")
                    {
                        no_cache = true;
                    }
                }

                let data = response.bytes().await;

                match data {
                    Ok(data) => break data,
                    Err(e) => {
                        if e.is_timeout() && attempt <= 3 {
                            warn!("[{token:?}] timeout retrieving `{remote}`, retrying");
                            continue;
                        }
                        self.context
                            .write()
                            .await
                            .failed_remotes
                            .insert(remote.clone(), Instant::now());
                        return Err(AssetReaderError::Io(Arc::new(std::io::Error::other(
                            format!("[{token:?}] failed to convert to bytes: `{remote}`: {e}"),
                        ))));
                    }
                }
            };

            if let Some(hash) = hash {
                if !no_cache && ipfs_path.should_cache(&hash) {
                    let mut cache_path = PathBuf::from(self.cache_path());
                    cache_path.push(format!("{}.part", hash));
                    let cache_path_str = cache_path.to_string_lossy().into_owned();
                    // ignore errors trying to cache
                    match async_fs::File::create(&cache_path).await {
                        Err(e) => {
                            warn!("failed to create cache `{cache_path_str}`: {e}");
                        }
                        Ok(mut f) => {
                            if let Err(e) = f.write_all(&data).await {
                                warn!("failed to write cache `{cache_path_str}`: {e}");
                            } else if let Err(e) = f.sync_all().await {
                                warn!("failed to sync cache `{cache_path_str}`: {e}");
                            } else {
                                let mut final_path = cache_path.clone();
                                final_path.pop();
                                final_path.push(hash);
                                if let Err(e) = async_fs::rename(cache_path, &final_path).await {
                                    warn!("failed to rename cache item `{cache_path_str}`: {e}");
                                } else {
                                    debug!("cached ok `{}`", final_path.to_string_lossy());
                                }
                            }
                        }
                    }
                }
            }

            debug!("[{token:?}]: completed remote url: `{remote}`");
            let reader: Box<Reader> = Box::new(Cursor::new(data));
            Ok(reader)
        }))
    }

    fn read_meta<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl ConditionalSendFuture<Output = Result<Box<bevy::asset::io::Reader<'a>>, AssetReaderError>>
    {
        self.default_io.read_meta(path)
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl ConditionalSendFuture<Output = Result<bool, AssetReaderError>> {
        self.default_io.is_directory(path)
    }

    fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl ConditionalSendFuture<Output = Result<Box<bevy::asset::io::PathStream>, AssetReaderError>>
    {
        self.default_io.read_directory(path)
    }
}

#[derive(Clone)]
pub struct PassThroughReader {
    inner: Arc<IpfsIo>,
}

impl AssetReader for PassThroughReader {
    fn read<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl ConditionalSendFuture<Output = Result<Box<Reader<'a>>, AssetReaderError>> {
        AssetReader::read(&*self.inner, path)
    }

    fn read_meta<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl ConditionalSendFuture<Output = Result<Box<Reader<'a>>, AssetReaderError>> {
        Box::pin(async move {
            if IpfsPath::new_from_path(path).is_ok() {
                Err(AssetReaderError::NotFound(path.to_owned()))
            } else {
                AssetReader::read_meta(&*self.inner, path).await
            }
        })
    }

    fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl ConditionalSendFuture<Output = Result<Box<bevy::asset::io::PathStream>, AssetReaderError>>
    {
        AssetReader::read_directory(&*self.inner, path)
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl ConditionalSendFuture<Output = Result<bool, AssetReaderError>> {
        AssetReader::is_directory(&*self.inner, path)
    }
}
