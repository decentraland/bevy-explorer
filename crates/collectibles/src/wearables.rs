use std::str::FromStr;

use crate::{base_wearables, CollectibleInstance, CollectibleUrn};
use anyhow::anyhow;
use bevy::{
    gltf::Gltf,
    prelude::*,
    tasks::{IoTaskPool, Task},
    utils::{HashMap, HashSet},
};
use isahc::AsyncReadResponseExt;
use serde::Deserialize;

use common::util::TaskExt;
use ipfs::{ActiveEntityTask, IpfsAssetServer, IpfsModifier};

pub struct WearablePlugin;

impl Plugin for WearablePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WearablePointers>();
        app.init_resource::<RequestedWearables>();
        app.init_resource::<WearableCollections>();
        app.add_systems(
            Update,
            (load_base_wearables, load_collections, load_wearables),
        );
    }
}

#[derive(Resource, Default)]
pub struct WearableCollections(pub HashMap<String, String>);

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct WearableMarker;

pub type WearableUrn = CollectibleUrn<WearableMarker>;
pub type WearableInstance = CollectibleInstance<WearableMarker>;

#[derive(Debug, Clone)]
pub struct WearableMetaAndHash {
    pub meta: WearableMeta,
    pub hash: String,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
enum WearablePointerResult {
    Exists(WearableMetaAndHash),
    Missing,
}

impl WearablePointerResult {
    pub fn hash(&self) -> Result<&str, ()> {
        match self {
            WearablePointerResult::Exists(WearableMetaAndHash { hash, .. }) => Ok(hash),
            WearablePointerResult::Missing => Err(()),
        }
    }

    pub fn meta(&self) -> Result<&WearableMeta, ()> {
        match self {
            WearablePointerResult::Exists(WearableMetaAndHash { meta, .. }) => Ok(meta),
            WearablePointerResult::Missing => Err(()),
        }
    }

    pub fn get(&self) -> Result<&WearableMetaAndHash, ()> {
        match self {
            WearablePointerResult::Exists(mah) => Ok(mah),
            WearablePointerResult::Missing => Err(()),
        }
    }
}

#[derive(Resource, Default, Debug)]
pub struct WearablePointers {
    data: HashMap<WearableUrn, WearablePointerResult>,
}

impl WearablePointers {
    pub fn get(
        &self,
        collectible: impl AsRef<WearableUrn>,
    ) -> Option<Result<&WearableMetaAndHash, ()>> {
        self.data
            .get(collectible.as_ref())
            .map(WearablePointerResult::get)
    }

    pub fn meta(&self, collectible: impl AsRef<WearableUrn>) -> Option<Result<&WearableMeta, ()>> {
        self.data
            .get(collectible.as_ref())
            .map(WearablePointerResult::meta)
    }

    pub fn hash(&self, collectible: impl AsRef<WearableUrn>) -> Option<Result<&str, ()>> {
        self.data
            .get(collectible.as_ref())
            .map(WearablePointerResult::hash)
    }

    fn insert(&mut self, collectible: impl Into<WearableUrn>, wearable: WearablePointerResult) {
        self.data.insert(collectible.into(), wearable);
    }

    pub fn contains_key(&self, collectible: impl AsRef<WearableUrn>) -> bool {
        self.data.contains_key(collectible.as_ref())
    }
}

#[derive(Deserialize, Debug, Component, Clone)]
pub struct WearableMeta {
    pub id: String,
    pub name: String,
    pub description: String,
    pub thumbnail: String,
    pub rarity: Option<String>,
    pub data: WearableData,
}

impl WearableMeta {
    pub fn hides(&self, body_shape: &CollectibleUrn<WearableMarker>) -> HashSet<WearableCategory> {
        // hides from data
        let mut hides = HashSet::from_iter(self.data.hides.clone());
        if let Some(repr) = self.data.representations.iter().find(|repr| {
            repr.body_shapes
                .iter()
                .any(|shape| body_shape == &WearableUrn::new(shape))
        }) {
            // add hides from representation
            hides.extend(repr.override_hides.clone());
        }

        // add all hides for skin
        if self.data.category == WearableCategory::SKIN {
            hides.extend(WearableCategory::iter());
        }

        // remove self
        hides.remove(&self.data.category);
        hides
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct WearableData {
    pub tags: Vec<String>,
    pub category: WearableCategory,
    pub representations: Vec<WearableRepresentation>,
    pub hides: Vec<WearableCategory>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WearableRepresentation {
    pub body_shapes: Vec<String>,
    pub main_file: String,
    pub override_replaces: Vec<WearableCategory>,
    pub override_hides: Vec<WearableCategory>,
    pub contents: Vec<String>,
}

fn load_base_wearables(
    mut once: Local<bool>,
    mut task: Local<Option<ActiveEntityTask>>,
    mut wearable_pointers: ResMut<WearablePointers>,
    ipfas: IpfsAssetServer,
) {
    if *once || ipfas.active_endpoint().is_none() {
        return;
    }

    match *task {
        None => {
            let pointers = base_wearables::base_wearable_urns()
                .iter()
                .map(ToString::to_string)
                .collect();
            *task = Some(ipfas.ipfs().active_entities(
                ipfs::ActiveEntitiesRequest::Pointers(pointers),
                Some(base_wearables::BASE_URL),
            ));
        }
        Some(ref mut active_task) => match active_task.complete() {
            None => (),
            Some(Err(e)) => {
                warn!("failed to acquire base wearables: {e}");
                *task = None;
                *once = true;
            }
            Some(Ok(active_entities)) => {
                debug!("first active entity: {:?}", active_entities.first());
                for entity in active_entities {
                    ipfas.ipfs().add_collection(
                        entity.id.clone(),
                        entity.content,
                        Some(IpfsModifier {
                            base_url: Some(base_wearables::CONTENT_URL.to_owned()),
                        }),
                        entity.metadata.as_ref().map(ToString::to_string),
                    );

                    let Some(metadata) = entity.metadata else {
                        warn!("no metadata on wearable");
                        continue;
                    };
                    let meta = match serde_json::from_value::<WearableMeta>(metadata.clone()) {
                        Ok(data) => data,
                        Err(e) => {
                            warn!("failed to deserialize wearable data: {e}");
                            continue;
                        }
                    };
                    if meta.name.contains("dungarees") {
                        debug!("dungarees: {:?}", metadata);
                    }
                    for pointer in entity.pointers {
                        wearable_pointers.insert(
                            pointer,
                            WearablePointerResult::Exists(WearableMetaAndHash {
                                meta: meta.clone(),
                                hash: entity.id.clone(),
                            }),
                        );
                    }
                }
                *task = None;
                *once = true;
            }
        },
    }
}

#[derive(Deserialize)]
struct Collection {
    pub id: String,
    pub name: String,
}

#[derive(Deserialize)]
struct Collections {
    collections: Vec<Collection>,
}

fn load_collections(
    mut once: Local<bool>,
    mut collections: ResMut<WearableCollections>,
    mut task: Local<Option<Task<Result<Collections, anyhow::Error>>>>,
) {
    if *once {
        return;
    }

    match *task {
        None => {
            let t: Task<Result<Collections, anyhow::Error>> = IoTaskPool::get().spawn(async move {
                let mut response =
                    isahc::get_async("https://realm-provider.decentraland.org/lambdas/collections")
                        .await
                        .map_err(|e| anyhow!(e))?;
                response.json::<Collections>().await.map_err(|e| anyhow!(e))
            });
            *task = Some(t)
        }
        Some(ref mut active_task) => match active_task.complete() {
            None => (),
            Some(Err(e)) => {
                warn!("failed to acquire collections: {e}");
                *task = None;
                *once = true;
            }
            Some(Ok(collections_result)) => {
                collections.0 = HashMap::from_iter(
                    collections_result
                        .collections
                        .into_iter()
                        .map(|c| (c.id, c.name)),
                );
                *task = None;
                *once = true;
            }
        },
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Debug)]
pub struct WearableCategory {
    pub slot: &'static str,
    pub is_texture: bool,
}

impl<'de> serde::Deserialize<'de> for WearableCategory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(WearableCategory::from_str(s.as_str()).unwrap_or(WearableCategory::UNKNOWN))
    }
}

impl WearableCategory {
    pub const UNKNOWN: WearableCategory = WearableCategory::texture("unknown");

    pub const EYES: WearableCategory = WearableCategory::texture("eyes");
    pub const EYEBROWS: WearableCategory = WearableCategory::texture("eyebrows");
    pub const MOUTH: WearableCategory = WearableCategory::texture("mouth");

    pub const FACIAL_HAIR: WearableCategory = WearableCategory::model("facial_hair");
    pub const HAIR: WearableCategory = WearableCategory::model("hair");
    pub const HAND_WEAR: WearableCategory = WearableCategory::model("hands_wear");
    pub const BODY_SHAPE: WearableCategory = WearableCategory::model("body_shape");
    pub const UPPER_BODY: WearableCategory = WearableCategory::model("upper_body");
    pub const LOWER_BODY: WearableCategory = WearableCategory::model("lower_body");
    pub const FEET: WearableCategory = WearableCategory::model("feet");
    pub const EARRING: WearableCategory = WearableCategory::model("earring");
    pub const EYEWEAR: WearableCategory = WearableCategory::model("eyewear");
    pub const HAT: WearableCategory = WearableCategory::model("hat");
    pub const HELMET: WearableCategory = WearableCategory::model("helmet");
    pub const MASK: WearableCategory = WearableCategory::model("mask");
    pub const TIARA: WearableCategory = WearableCategory::model("tiara");
    pub const TOP_HEAD: WearableCategory = WearableCategory::model("top_head");
    pub const SKIN: WearableCategory = WearableCategory::model("skin");

    const fn model(slot: &'static str) -> Self {
        Self {
            slot,
            is_texture: false,
        }
    }

    const fn texture(slot: &'static str) -> Self {
        Self {
            slot,
            is_texture: true,
        }
    }
}

impl FromStr for WearableCategory {
    type Err = anyhow::Error;

    fn from_str(slot: &str) -> Result<WearableCategory, Self::Err> {
        match slot {
            "body_shape" => Ok(Self::BODY_SHAPE),

            "hair" => Ok(Self::HAIR),
            "eyebrows" => Ok(Self::EYEBROWS),
            "eyes" => Ok(Self::EYES),
            "mouth" => Ok(Self::MOUTH),
            "facial_hair" => Ok(Self::FACIAL_HAIR),

            "upper_body" => Ok(Self::UPPER_BODY),
            "hands_wear" => Ok(Self::HAND_WEAR),
            "hands" => Ok(Self::HAND_WEAR), // legacy support
            "lower_body" => Ok(Self::LOWER_BODY),
            "feet" => Ok(Self::FEET),

            "hat" => Ok(Self::HAT),
            "eyewear" => Ok(Self::EYEWEAR),
            "earring" => Ok(Self::EARRING),
            "mask" => Ok(Self::MASK),
            "top_head" => Ok(Self::TOP_HEAD),
            "head" => Ok(Self::TOP_HEAD), // legacy support
            "tiara" => Ok(Self::TIARA),
            "helmet" => Ok(Self::HELMET),
            "skin" => Ok(Self::SKIN),

            _ => {
                warn!("unrecognised wearable category: {slot}");
                Err(anyhow::anyhow!("unrecognised wearable category: {slot}"))
            }
        }
    }
}

impl WearableCategory {
    pub fn iter() -> impl Iterator<Item = &'static WearableCategory> {
        [
            Self::BODY_SHAPE,
            Self::HAIR,
            Self::EYEBROWS,
            Self::EYES,
            Self::MOUTH,
            Self::FACIAL_HAIR,
            Self::UPPER_BODY,
            Self::HAND_WEAR,
            Self::LOWER_BODY,
            Self::FEET,
            Self::HAT,
            Self::EYEWEAR,
            Self::EARRING,
            Self::MASK,
            Self::TOP_HEAD,
            Self::TIARA,
            Self::HELMET,
            Self::SKIN,
        ]
        .iter()
    }

    pub fn index(&self) -> Option<usize> {
        Self::iter().position(|w| w == self)
    }
}

#[derive(Debug, Clone)]
pub struct WearableDefinition {
    pub category: WearableCategory,
    pub hides: HashSet<WearableCategory>,
    pub model: Option<Handle<Gltf>>,
    pub texture: Option<Handle<Image>>,
    pub mask: Option<Handle<Image>>,
    pub thumbnail: Option<Handle<Image>>,
}

impl WearableDefinition {
    pub fn new(
        data: &WearableMetaAndHash,
        ipfas: &IpfsAssetServer,
        body_shape: &str,
    ) -> Option<WearableDefinition> {
        let Some(representation) = (if body_shape.is_empty() {
            Some(&data.meta.data.representations[0])
        } else {
            data.meta.data.representations.iter().find(|rep| {
                rep.body_shapes
                    .iter()
                    .any(|rep_shape| rep_shape.to_lowercase() == body_shape.to_lowercase())
            })
        }) else {
            warn!(
                "no representation for body shape {body_shape}, {:?}",
                data.meta
            );
            return None;
        };

        let category = data.meta.data.category;
        if category == WearableCategory::UNKNOWN {
            warn!("unknown wearable category");
            return None;
        }

        let hides = data.meta.hides(WearableInstance::new(body_shape).base());

        let (model, texture, mask) = if category.is_texture {
            // don't validate the main file, as some base wearables have no extension on the main_file member (Eyebrows_09 e.g)
            // if !representation.main_file.ends_with(".png") {
            //     warn!(
            //         "expected .png main file for category {}, found {}",
            //         category.slot, representation.main_file
            //     );
            //     return None;
            // }

            let texture = representation
                .contents
                .iter()
                .find(|f| {
                    f.to_lowercase().ends_with(".png") && !f.to_lowercase().ends_with("_mask.png")
                })
                .and_then(|f| ipfas.load_content_file::<Image>(f, &data.hash).ok());
            let mask = representation
                .contents
                .iter()
                .find(|f| f.to_lowercase().ends_with("_mask.png"))
                .and_then(|f| ipfas.load_content_file::<Image>(f, &data.hash).ok());

            (None, texture, mask)
        } else {
            if !representation.main_file.to_lowercase().ends_with(".glb") {
                warn!(
                    "expected .glb main file, found {}",
                    representation.main_file
                );
                return None;
            }

            let model = ipfas
                .load_content_file::<Gltf>(&representation.main_file, &data.hash)
                .ok();

            (model, None, None)
        };

        let thumbnail = ipfas
            .load_content_file::<Image>(&data.meta.thumbnail, &data.hash)
            .ok();

        Some(Self {
            category,
            hides,
            model,
            texture,
            mask,
            thumbnail,
        })
    }
}

#[derive(Resource, Default)]
pub struct RequestedWearables(pub HashSet<WearableUrn>);

fn load_wearables(
    mut requested_wearables: ResMut<RequestedWearables>,
    mut wearable_task: Local<Option<(ActiveEntityTask, HashSet<WearableUrn>)>>,
    mut wearable_pointers: ResMut<WearablePointers>,
    ipfas: IpfsAssetServer,
) {
    if let Some((mut task, mut wearables)) = wearable_task.take() {
        match task.complete() {
            Some(Ok(entities)) => {
                debug!("got results: {:?}", entities.len());

                for entity in entities {
                    ipfas.ipfs().add_collection(
                        entity.id.clone(),
                        entity.content,
                        Some(IpfsModifier {
                            base_url: Some(base_wearables::CONTENT_URL.to_owned()),
                        }),
                        entity.metadata.as_ref().map(ToString::to_string),
                    );

                    let Some(metadata) = entity.metadata else {
                        warn!("no metadata on wearable");
                        continue;
                    };
                    debug!("loaded wearable {:?} -> {:?}", entity.pointers, metadata);
                    let meta = match serde_json::from_value::<WearableMeta>(metadata) {
                        Ok(data) => data,
                        Err(e) => {
                            warn!("failed to deserialize wearable data: {e}");
                            continue;
                        }
                    };
                    for pointer in entity.pointers.into_iter().map(WearableUrn::from) {
                        debug!("{} -> {}", pointer, entity.id);
                        wearables.remove(&pointer);
                        wearable_pointers.insert(
                            pointer,
                            WearablePointerResult::Exists(WearableMetaAndHash {
                                meta: meta.clone(),
                                hash: entity.id.clone(),
                            }),
                        );
                    }
                }

                // any urns left in the hashset were requested but not returned
                for urn in wearables {
                    debug!("missing {urn}");
                    wearable_pointers.insert(urn, WearablePointerResult::Missing);
                }
            }
            Some(Err(e)) => {
                warn!("failed to resolve entities: {e}");
            }
            None => {
                debug!("waiting for wearable resolve");
                *wearable_task = Some((task, wearables));
            }
        }
    } else {
        let base_wearables = HashSet::from_iter(base_wearables::base_wearable_urns());
        let requested = requested_wearables
            .0
            .drain()
            .filter(|r| !wearable_pointers.contains_key(r))
            .filter(|urn| !base_wearables.contains(urn))
            .collect::<HashSet<_>>();
        let pointers = requested
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();

        if !requested.is_empty() {
            debug!("requesting: {:?}", requested);
            *wearable_task = Some((
                ipfas
                    .ipfs()
                    .active_entities(ipfs::ActiveEntitiesRequest::Pointers(pointers), None),
                requested,
            ));
        }
    }
}
