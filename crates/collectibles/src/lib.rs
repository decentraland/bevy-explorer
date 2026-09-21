pub mod base_wearables;
pub mod emotes;
pub mod ext;
pub mod urn;
pub mod wearables;

use std::{marker::PhantomData, path::PathBuf};

use bevy::{
    diagnostic::FrameCount,
    ecs::system::SystemParam,
    platform::collections::{HashMap, HashSet},
    prelude::*,
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
    /// The off-chain collections a bare name (`"sittingChair2"`) may come from. A bare name parses
    /// to a urn under `UNKNOWN_SOURCE_COLLECTION`; the manager requests the name under every
    /// source collection instead and aliases the unknown-source urn to whichever has it. Lookups
    /// by the unknown-source urn then work, and `CollectibleManager::source_urn` gives the real one.
    fn source_collections() -> &'static [&'static str];
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
    pub thumbnail: String,
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
    /// requested urn -> the `source_collections` urn it resolved to
    aliases: HashMap<CollectibleUrn<T>, CollectibleUrn<T>>,
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
            aliases: Default::default(),
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

    /// The urn to name a collectible by outside the manager (on the wire): the source-collection
    /// urn it resolved to, else the bare name it was requested by, else the urn as given.
    pub fn source_urn<'a>(&'a self, urn: &'a CollectibleUrn<T>) -> &'a str {
        match self.collectibles.aliases.get(urn) {
            Some(source) => source.as_str(),
            None => urn.unknown_source_name().unwrap_or(urn.as_str()),
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
                            base_url: Some(base_wearables::content_url()),
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
                for urn in &requested_entities {
                    debug!("missing {urn}");
                    collectibles
                        .pointers
                        .insert(urn.clone(), PointerResult::Missing);
                }

                // an unknown-source urn is never returned, so it always lands here: resolve it to
                // whichever source collection has the name
                for urn in requested_entities {
                    let found = source_urns::<T>(&urn).find_map(|source| {
                        match collectibles.pointers.get(&source) {
                            Some(PointerResult::Hash(hash)) => Some((source, hash.clone())),
                            _ => None,
                        }
                    });
                    if let Some((source, hash)) = found {
                        debug!("{urn} -> {source}");
                        collectibles
                            .pointers
                            .insert(urn.clone(), PointerResult::Hash(hash));
                        collectibles.aliases.insert(urn, source);
                    }
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

        let mut requested = requested
            .into_iter()
            .filter(|r| !collectibles.pointers.contains_key(r))
            .collect::<HashSet<_>>();

        // fetch an unknown-source name under every source collection, in the same round trip
        let sources = requested
            .iter()
            .flat_map(source_urns::<T>)
            .filter(|s| !collectibles.pointers.contains_key(s))
            .collect::<Vec<_>>();
        requested.extend(sources);

        // an unknown-source urn stays in the set (to be resolved from the results) but isn't a
        // pointer any server knows
        let pointers = requested
            .iter()
            .filter(|urn| urn.unknown_source_name().is_none())
            .map(ToString::to_string)
            .collect::<Vec<_>>();

        if !pointers.is_empty() {
            debug!("requesting: {:?} ({:?})", requested, pointers);
            *active_task = Some((
                ipfas
                    .ipfs()
                    .active_entities(ipfs::ActiveEntitiesRequest::Pointers(pointers), None),
                requested,
            ));
        }
    }
}

/// The places an unknown-source name may be found: the name under each of the type's
/// `source_collections`. A urn under a real collection names exactly one place, so none.
fn source_urns<T: CollectibleType>(
    urn: &CollectibleUrn<T>,
) -> impl Iterator<Item = CollectibleUrn<T>> + '_ {
    urn.unknown_source_name().into_iter().flat_map(|name| {
        T::source_collections()
            .iter()
            .filter_map(move |collection| CollectibleUrn::new(&format!("{collection}:{name}")).ok())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources<T: CollectibleType>(urn: &str) -> Vec<String> {
        source_urns(&CollectibleUrn::<T>::new(urn).unwrap())
            .map(String::from)
            .collect()
    }

    #[test]
    fn bare_emote_names_are_tried_under_every_source_collection() {
        let bare = EmoteUrn::new("sittingChair2").unwrap();
        assert_eq!(
            bare.as_str(),
            "urn:decentraland:off-chain:unknown-source:sittingchair2"
        );
        assert_eq!(bare.unknown_source_name(), Some("sittingchair2"));
        assert_eq!(
            sources::<Emote>("sittingChair2"),
            [
                "urn:decentraland:off-chain:base-emotes:sittingchair2",
                "urn:decentraland:off-chain:base-scene-emotes:sittingchair2"
            ]
        );
        assert!(EmoteUrn::new("urn:decentraland:off-chain:base-emotes:wave")
            .unwrap()
            .unknown_source_name()
            .is_none());
        assert!(sources::<Emote>("urn:decentraland:off-chain:base-scene-emotes:punch").is_empty());
        assert!(sources::<Emote>("urn:decentraland:matic:collections-v2:0xabc:0").is_empty());
        assert!(sources::<wearables::Wearable>(
            "urn:decentraland:off-chain:base-avatars:f_eyes_00"
        )
        .is_empty());
    }
}
