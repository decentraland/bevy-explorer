pub mod base_wearables;
pub mod emotes;
pub mod urn;
pub mod wearables;

use std::{marker::PhantomData, path::PathBuf};

use bevy::{
    core::FrameCount,
    ecs::system::SystemParam,
    prelude::*,
    utils::{HashMap, HashSet},
};

use common::util::TaskExt;
pub use emotes::*;
use ipfs::{
    ipfs_path::{IpfsPath, IpfsType},
    ActiveEntityTask, IpfsAssetServer, IpfsModifier,
};
use serde::Deserialize;
use urn::CollectibleUrn;
use wearables::WearablePlugin;

pub struct CollectiblesPlugin;

impl Plugin for CollectiblesPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_plugins((EmotesPlugin, WearablePlugin));
    }
}

pub struct CollectiblesTypePlugin<T: CollectibleType>(PhantomData<fn() -> T>);

impl<T: CollectibleType> Default for CollectiblesTypePlugin<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<T: CollectibleType> Plugin for CollectiblesTypePlugin<T> {
    fn build(&self, app: &mut App) {
        app.init_resource::<Collectibles<T>>()
            .init_asset::<Collectible<T>>()
            .init_asset::<CollectibleData<T>>()
            .add_systems(Update, request_collectibles::<T>);
    }
}

pub trait CollectibleType: std::fmt::Debug + TypePath + Send + Sync + 'static {
    type Meta: for<'a> Deserialize<'a>;
    type ExtraData: Clone + Send + Sync + 'static;
    fn base_collection() -> Option<&'static str>;
    fn extension() -> &'static str;
    fn data_extension() -> &'static str;
}

pub enum PointerResult<T: CollectibleType> {
    Hash(String),
    Builtin(Handle<Collectible<T>>),
    Missing,
}

#[derive(Asset, TypePath)]
pub struct Collectible<T: CollectibleType> {
    pub representations: HashMap<String, T>,
    pub data: CollectibleData<T>,
}

#[derive(Debug, Asset, TypePath)]
pub struct CollectibleData<T: CollectibleType> {
    pub hash: String,
    pub urn: String,
    pub thumbnail: Handle<Image>,
    pub available_representations: HashSet<String>,
    pub name: String,
    pub description: String,
    pub extra_data: T::ExtraData,
}

impl<T: CollectibleType> Clone for CollectibleData<T> {
    fn clone(&self) -> Self {
        Self {
            hash: self.hash.clone(),
            urn: self.urn.clone(),
            thumbnail: self.thumbnail.clone(),
            available_representations: self.available_representations.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            extra_data: self.extra_data.clone(),
        }
    }
}

#[derive(Resource)]
pub struct Collectibles<T: CollectibleType> {
    pointers: HashMap<CollectibleUrn<T>, PointerResult<T>>,
    pointer_request: HashSet<CollectibleUrn<T>>,
    cache: HashMap<CollectibleUrn<T>, (u32, Handle<Collectible<T>>)>,
    data_cache: HashMap<CollectibleUrn<T>, (u32, Handle<CollectibleData<T>>)>,
}

impl<T: CollectibleType> Collectibles<T> {
    pub fn retain(&mut self, frame: u32, f: impl Fn(&CollectibleUrn<T>) -> bool) {
        let count = self.cache.len();
        self.cache
            .retain(|urn, (expiry, _)| *expiry >= frame || f(urn));
        if self.cache.len() != count {
            debug!(
                "{}/{} {} remain",
                self.cache.len(),
                count,
                std::any::type_name::<T>()
            );
        }
    }
}

impl<T: CollectibleType> Default for Collectibles<T> {
    fn default() -> Self {
        Self {
            pointers: Default::default(),
            pointer_request: Default::default(),
            cache: Default::default(),
            data_cache: Default::default(),
        }
    }
}

#[derive(SystemParam)]
pub struct CollectibleManager<'w, 's, T: CollectibleType> {
    collectibles: ResMut<'w, Collectibles<T>>,
    assets: ResMut<'w, Assets<Collectible<T>>>,
    datas: ResMut<'w, Assets<CollectibleData<T>>>,
    ipfas: IpfsAssetServer<'w, 's>,
    frame: Res<'w, FrameCount>,
}

#[derive(Debug)]
pub enum CollectibleError {
    Loading,
    Missing,
    Failed,
    NoRepresentation,
}

const RETAIN_TICKS: u32 = 3;

impl<T: CollectibleType> CollectibleManager<'_, '_, T> {
    pub fn get_hash(
        &mut self,
        urn: impl AsRef<CollectibleUrn<T>>,
    ) -> Result<String, CollectibleError> {
        match self.collectibles.pointers.get(urn.as_ref()) {
            Some(PointerResult::Missing) => Err(CollectibleError::Missing),
            Some(PointerResult::Hash(hash)) => Ok(hash.to_owned()),
            Some(PointerResult::Builtin(_)) => Err(CollectibleError::NoRepresentation),
            None => {
                self.collectibles
                    .pointer_request
                    .insert(urn.as_ref().clone());
                Err(CollectibleError::Loading)
            }
        }
    }

    pub fn get_representation(
        &mut self,
        urn: impl AsRef<CollectibleUrn<T>>,
        body_shape: &str,
    ) -> Result<&T, CollectibleError> {
        match self.collectibles.pointers.get(urn.as_ref()) {
            Some(PointerResult::Missing) => Err(CollectibleError::Missing),
            Some(PointerResult::Hash(hash)) => {
                let hash = hash.clone();
                let (expiry, handle) = self
                    .collectibles
                    .cache
                    .entry(urn.as_ref().clone())
                    .or_insert_with(|| {
                        let ipfs_path = IpfsPath::new(IpfsType::new_content_file(
                            hash.clone(),
                            format!("collectible.{}", T::extension()),
                        ));

                        (
                            0,
                            self.ipfas
                                .asset_server()
                                .load::<Collectible<T>>(PathBuf::from(&ipfs_path)),
                        )
                    });

                *expiry = self.frame.0 + RETAIN_TICKS;

                if let Some(reps) = self.assets.get(handle.id()) {
                    match reps.representations.get(body_shape.to_lowercase().as_str()) {
                        Some(collectible) => Ok(collectible),
                        None => Err(CollectibleError::NoRepresentation),
                    }
                } else if let bevy::asset::LoadState::Failed(_) =
                    self.ipfas.asset_server().load_state(handle.id())
                {
                    Err(CollectibleError::Failed)
                } else {
                    Err(CollectibleError::Loading)
                }
            }
            Some(PointerResult::Builtin(handle)) => {
                let reps = self.assets.get(handle).unwrap();
                match reps.representations.get(body_shape) {
                    Some(collectible) => Ok(collectible),
                    None => Err(CollectibleError::NoRepresentation),
                }
            }
            None => {
                self.collectibles
                    .pointer_request
                    .insert(urn.as_ref().clone());
                Err(CollectibleError::Loading)
            }
        }
    }

    pub fn get_data(
        &mut self,
        urn: impl AsRef<CollectibleUrn<T>>,
    ) -> Result<&CollectibleData<T>, CollectibleError> {
        match self.collectibles.pointers.get(urn.as_ref()) {
            Some(PointerResult::Missing) => Err(CollectibleError::Missing),
            Some(PointerResult::Hash(hash)) => {
                let hash = hash.clone();
                let (expiry, handle) = self
                    .collectibles
                    .data_cache
                    .entry(urn.as_ref().clone())
                    .or_insert_with(|| {
                        let ipfs_path = IpfsPath::new(IpfsType::new_content_file(
                            hash,
                            format!("collectible.{}", T::data_extension()),
                        ));

                        (
                            0,
                            self.ipfas
                                .asset_server()
                                .load::<CollectibleData<T>>(PathBuf::from(&ipfs_path)),
                        )
                    });

                *expiry = self.frame.0 + RETAIN_TICKS;

                if let Some(reps) = self.datas.get(handle.id()) {
                    Ok(reps)
                } else if let bevy::asset::LoadState::Failed(_) =
                    self.ipfas.asset_server().load_state(handle.id())
                {
                    Err(CollectibleError::Failed)
                } else {
                    Err(CollectibleError::Loading)
                }
            }
            Some(PointerResult::Builtin(handle)) => Ok(&self.assets.get(handle).unwrap().data),
            None => {
                self.collectibles
                    .pointer_request
                    .insert(urn.as_ref().clone());
                Err(CollectibleError::Loading)
            }
        }
    }

    pub fn add_builtin(&mut self, urn: CollectibleUrn<T>, value: Collectible<T>) {
        if let Some(PointerResult::Builtin(h)) = self.collectibles.pointers.get(&urn) {
            let asset = self.assets.get_mut(h).unwrap();
            asset.representations.extend(value.representations);
            asset
                .data
                .available_representations
                .extend(value.data.available_representations);
        } else {
            let handle = self.assets.add(value);
            self.collectibles
                .pointers
                .insert(urn, PointerResult::Builtin(handle));
        }
    }
}

pub fn request_collectibles<T: CollectibleType>(
    mut collectibles: ResMut<Collectibles<T>>,
    mut active_task: Local<Option<(ActiveEntityTask, HashSet<CollectibleUrn<T>>)>>,
    ipfas: IpfsAssetServer,
) {
    if let Some((mut task, mut requested_entities)) = active_task.take() {
        match task.complete() {
            Some(Ok(entities)) => {
                debug!("got results: {:?}", entities.len());

                for entity in entities {
                    let collection = entity
                        .content
                        .with(format!("collectible.{}", T::extension()), entity.id.clone())
                        .with(
                            format!("collectible.{}", T::data_extension()),
                            entity.id.clone(),
                        );

                    ipfas.ipfs().add_collection(
                        entity.id.clone(),
                        collection,
                        Some(IpfsModifier {
                            base_url: Some(base_wearables::CONTENT_URL.to_owned()),
                        }),
                        entity.metadata.as_ref().map(ToString::to_string),
                    );

                    let Some(metadata) = entity.metadata else {
                        warn!("no metadata on wearable");
                        continue;
                    };
                    debug!("loaded collectible {:?} -> {:?}", entity.pointers, metadata);
                    for pointer in entity.pointers.into_iter() {
                        let Ok(pointer) = CollectibleUrn::<T>::try_from(pointer.as_str()) else {
                            warn!("bad pointer: {}", pointer);
                            continue;
                        };

                        debug!("{} -> {}", pointer, entity.id);
                        requested_entities.remove(&pointer);
                        collectibles
                            .pointers
                            .insert(pointer, PointerResult::Hash(entity.id.clone()));
                    }
                }

                // any urns left in the hashset were requested but not returned
                for urn in requested_entities {
                    debug!("missing {urn}");
                    collectibles.pointers.insert(urn, PointerResult::Missing);
                }
            }
            Some(Err(e)) => {
                warn!("failed to resolve entities: {e}");
            }
            None => {
                debug!("waiting for collectible resolve");
                *active_task = Some((task, requested_entities));
            }
        }
    } else {
        let requested = std::mem::take(&mut collectibles.pointer_request);

        let requested = requested
            .into_iter()
            .filter(|r| !collectibles.pointers.contains_key(r))
            .collect::<HashSet<_>>();

        let pointers = requested
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();

        if !pointers.is_empty() {
            debug!("requesting: {:?} ({:?})", requested, pointers);
            *active_task = Some((
                ipfas.ipfs().active_entities(
                    ipfs::ActiveEntitiesRequest::Pointers(pointers),
                    None,
                    false,
                ),
                requested,
            ));
        }
    }
}
