use std::{path::PathBuf, str::FromStr};

use crate::{
    urn::{CollectibleInstance, CollectibleUrn},
    Collectible, CollectibleData, CollectibleType, Collectibles, CollectiblesTypePlugin,
};
use anyhow::anyhow;
use bevy::{
    asset::AssetLoader,
    core::FrameCount,
    gltf::{Gltf, GltfLoaderSettings},
    prelude::*,
    render::render_asset::RenderAssetUsages,
    tasks::{IoTaskPool, Task},
    utils::{HashMap, HashSet},
};
use isahc::AsyncReadResponseExt;
use serde::Deserialize;

use common::util::TaskExt;
use ipfs::{
    ipfs_path::{IpfsPath, IpfsType},
    EntityDefinitionLoader,
};

pub struct WearablePlugin;

impl Plugin for WearablePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(CollectiblesTypePlugin::<Wearable>::default())
            .init_resource::<WearableCollections>();
        app.register_asset_loader(WearableLoader);
        app.register_asset_loader(WearableMetaLoader);
        app.add_systems(Update, (load_collections, retain_wearables));
    }
}

#[derive(Resource, Default)]
pub struct WearableCollections(pub HashMap<String, String>);

pub type WearableUrn = CollectibleUrn<Wearable>;
pub type WearableInstance = CollectibleInstance<Wearable>;

#[derive(Deserialize, Debug, Component, Clone)]
pub struct WearableMeta {
    pub id: String,
    pub name: String,
    pub description: String,
    pub thumbnail: String,
    pub rarity: Option<String>,
    pub data: WearableData,
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

#[derive(Debug, TypePath, Clone)]
pub struct Wearable {
    pub category: WearableCategory,
    pub hides: HashSet<WearableCategory>,
    pub model: Option<Handle<Gltf>>,
    pub texture: Option<Handle<Image>>,
    pub mask: Option<Handle<Image>>,
}

#[derive(Clone, Debug)]
pub struct WearableExtraData {
    pub category: WearableCategory,
}

impl CollectibleType for Wearable {
    type Meta = WearableMeta;
    type ExtraData = WearableExtraData;

    fn base_collection() -> Option<&'static str> {
        None
    }

    fn extension() -> &'static str {
        "wearable"
    }

    fn data_extension() -> &'static str {
        "wearable_data"
    }
}

fn content_file_path(file_path: impl Into<String>, content_hash: impl Into<String>) -> PathBuf {
    let ipfs_path = IpfsPath::new(IpfsType::new_content_file(
        content_hash.into(),
        file_path.into(),
    ));
    PathBuf::from(&ipfs_path)
}

struct WearableLoader;

impl AssetLoader for WearableLoader {
    type Asset = Collectible<Wearable>;

    type Settings = ();

    type Error = anyhow::Error;

    fn load<'a>(
        &'a self,
        reader: &'a mut bevy::asset::io::Reader,
        settings: &'a Self::Settings,
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> bevy::utils::BoxedFuture<'a, Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            let mut entity = EntityDefinitionLoader
                .load(reader, settings, load_context)
                .await?;
            let metadata = entity.metadata.ok_or(anyhow!("no metadata?"))?;
            let meta = serde_json::from_value::<WearableMeta>(metadata)?;

            let category = meta.data.category;
            let thumbnail = load_context.load(content_file_path(&meta.thumbnail, &entity.id));

            let mut representations = HashMap::default();

            for representation in meta.data.representations.into_iter() {
                let (model, texture, mask) = if category.is_texture {
                    // don't validate the main file, as some base wearables have no extension on the main_file member (Eyebrows_09 e.g)
                    let texture = representation
                        .contents
                        .iter()
                        .find(|f| {
                            f.to_lowercase().ends_with(".png")
                                && !f.to_lowercase().ends_with("_mask.png")
                        })
                        .map(|f| load_context.load(content_file_path(f, &entity.id)));
                    let mask = representation
                        .contents
                        .iter()
                        .find(|f| f.to_lowercase().ends_with("_mask.png"))
                        .map(|f| load_context.load(content_file_path(f, &entity.id)));

                    (None, texture, mask)
                } else {
                    if !representation.main_file.to_lowercase().ends_with(".glb") {
                        return Err(anyhow!(
                            "expected .glb main file, found {}",
                            representation.main_file
                        ));
                    }

                    let model = load_context.load_with_settings::<Gltf, GltfLoaderSettings>(
                        content_file_path(&representation.main_file, &entity.id),
                        |s| {
                            s.load_cameras = false;
                            s.load_lights = false;
                            s.load_materials = RenderAssetUsages::RENDER_WORLD;
                        },
                    );

                    (Some(model), None, None)
                };

                for body_shape in representation.body_shapes {
                    let mut hides = HashSet::from_iter(meta.data.hides.clone());
                    hides.extend(representation.override_hides.clone());

                    // add all hides for skin
                    if category == WearableCategory::SKIN {
                        hides.extend(WearableCategory::iter());
                    }

                    // remove self
                    hides.remove(&category);

                    representations.insert(
                        body_shape.to_lowercase(),
                        Wearable {
                            category,
                            hides,
                            model: model.clone(),
                            texture: texture.clone(),
                            mask: mask.clone(),
                        },
                    );
                }
            }

            Ok(Collectible {
                data: CollectibleData {
                    thumbnail,
                    hash: entity.id,
                    urn: entity.pointers.pop().unwrap_or_default(),
                    name: meta.name,
                    description: meta.description,
                    available_representations: representations.keys().cloned().collect(),
                    extra_data: WearableExtraData { category },
                },
                representations,
            })
        })
    }
}

#[derive(Component)]
pub struct UsedWearables(pub HashSet<CollectibleUrn<Wearable>>);

fn retain_wearables(
    used: Query<&UsedWearables>,
    mut collectibles: ResMut<Collectibles<Wearable>>,
    frame: Res<FrameCount>,
) {
    let urns = used.iter().fold(
        HashSet::<&CollectibleUrn<Wearable>>::default(),
        |mut urns, used| {
            urns.extend(used.0.iter());
            urns
        },
    );

    collectibles.retain(frame.0, |urn| urns.contains(urn));
}

struct WearableMetaLoader;

impl AssetLoader for WearableMetaLoader {
    type Asset = CollectibleData<Wearable>;

    type Settings = ();

    type Error = anyhow::Error;

    fn load<'a>(
        &'a self,
        reader: &'a mut bevy::asset::io::Reader,
        settings: &'a Self::Settings,
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> bevy::utils::BoxedFuture<'a, Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            let mut entity = EntityDefinitionLoader
                .load(reader, settings, load_context)
                .await?;
            let metadata = entity.metadata.ok_or(anyhow!("no metadata?"))?;
            let meta = serde_json::from_value::<WearableMeta>(metadata)?;

            let category = meta.data.category;
            let thumbnail = load_context.load(content_file_path(&meta.thumbnail, &entity.id));

            let available_representations = meta
                .data
                .representations
                .into_iter()
                .flat_map(|rep| {
                    rep.body_shapes
                        .into_iter()
                        .map(|shape| shape.to_lowercase())
                })
                .collect();

            Ok(CollectibleData {
                thumbnail,
                hash: entity.id,
                urn: entity.pointers.pop().unwrap_or_default(),
                name: meta.name,
                description: meta.description,
                available_representations,
                extra_data: WearableExtraData { category },
            })
        })
    }
}
