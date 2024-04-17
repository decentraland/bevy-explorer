use std::any::TypeId;

use anyhow::anyhow;
use bevy::{
    asset::{AssetLoader, LoadState, LoadedFolder},
    gltf::Gltf,
    prelude::*,
    utils::HashMap,
};
use ipfs::EntityDefinitionLoader;
use serde::{Deserialize, Serialize};

use once_cell::sync::Lazy;

use crate::{
    urn::{CollectibleInstance, CollectibleUrn},
    Collectible, CollectibleData, CollectibleManager, CollectibleType, CollectiblesTypePlugin,
};

pub fn base_bodyshapes() -> Vec<String> {
    vec![
        format!("urn:decentraland:off-chain:base-avatars:{}", "baseFemale").to_lowercase(),
        format!("urn:decentraland:off-chain:base-avatars:{}", "baseMale").to_lowercase(),
    ]
}

pub struct EmotesPlugin;

impl Plugin for EmotesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(CollectiblesTypePlugin::<Emote>::default());
        app.register_asset_loader(EmoteLoader);
        app.register_asset_loader(EmoteMetaLoader);
        app.add_systems(Update, (load_animations,));
    }
}

pub type EmoteUrn = CollectibleUrn<Emote>;
pub type EmoteInstance = CollectibleInstance<Emote>;

impl EmoteUrn {
    pub fn scene_emote(&self) -> Option<&str> {
        self.skip_take(0, 5)
            .rsplit_once(':')
            .filter(|(base, _)| *base == "urn:decentraland:off-chain:scene-emote")
            .map(|(_, emote)| emote)
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EmoteMeta {
    name: String,
    description: String,
    // rarity: Rarity,
    #[serde(rename = "emoteDataADR74")]
    emote_extended_data: EmoteExtendedData,
    thumbnail: String,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EmoteExtendedData {
    representations: Vec<EmoteRepresentation>,
    #[serde(rename = "loop")]
    loops: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EmoteRepresentation {
    pub body_shapes: Vec<String>,
    pub main_file: String,
    pub contents: Vec<String>,
}

#[derive(Default)]
enum AnimLoadState {
    #[default]
    Init,
    WaitingForFolder(Handle<LoadedFolder>),
    WaitingForGltfs(Vec<Handle<Gltf>>),
    Done,
}

#[allow(clippy::type_complexity)]
fn load_animations(
    asset_server: Res<AssetServer>,
    mut gltfs: ResMut<Assets<Gltf>>,
    mut state: Local<AnimLoadState>,
    folders: Res<Assets<LoadedFolder>>,
    mut emotes: CollectibleManager<Emote>,
) {
    match &mut *state {
        AnimLoadState::Init => {
            *state = AnimLoadState::WaitingForFolder(asset_server.load_folder("animations"));
        }
        AnimLoadState::WaitingForFolder(h_folder) => {
            if asset_server.load_state(h_folder.id()) == LoadState::Loaded {
                let folder = folders.get(h_folder.id()).unwrap();
                *state = AnimLoadState::WaitingForGltfs(
                    folder
                        .handles
                        .iter()
                        .filter(|h| h.type_id() == TypeId::of::<Gltf>())
                        .map(|h| h.clone().typed())
                        .collect(),
                )
            }
        }
        AnimLoadState::WaitingForGltfs(ref mut h_gltfs) => {
            h_gltfs.retain(
                |h_gltf| match gltfs.get(h_gltf).map(|gltf| &gltf.named_animations) {
                    Some(anims) => {
                        for (name, h_clip) in anims.clone() {
                            let (name, repeat, is_male, is_female) = DEFAULT_ANIMATION_LOOKUP
                                .iter()
                                .find(|(_, anim)| anim.male == name || anim.female == name)
                                .map(|(urn, anim)| {
                                    (
                                        urn.to_string(),
                                        anim.repeat,
                                        anim.male == name,
                                        anim.female == name,
                                    )
                                })
                                .unwrap_or((name.to_owned(), false, true, true));

                            let new_gltf = Gltf {
                                named_animations: HashMap::from_iter([(
                                    "_Avatar".to_owned(),
                                    h_clip.clone(),
                                )]),
                                scenes: Default::default(),
                                named_scenes: Default::default(),
                                meshes: Default::default(),
                                named_meshes: Default::default(),
                                materials: Default::default(),
                                named_materials: Default::default(),
                                nodes: Default::default(),
                                named_nodes: Default::default(),
                                default_scene: Default::default(),
                                animations: Default::default(),
                                source: Default::default(),
                            };
                            let new_gltf = gltfs.add(new_gltf);

                            let urn = EmoteUrn::new(&name).unwrap();
                            debug!("loaded default anim {:?} from {:?}", urn, h_gltf.path());

                            let emote = Emote {
                                gltf: new_gltf,
                                default_repeat: repeat,
                                sound: None,
                            };

                            let mut representations = HashMap::default();

                            if is_female {
                                representations.insert(base_bodyshapes().remove(0), emote.clone());
                            }
                            if is_male {
                                representations.insert(base_bodyshapes().remove(1), emote.clone());
                            }

                            let collectible = Collectible::<Emote> {
                                data: CollectibleData {
                                    hash: Default::default(),
                                    urn: urn.as_str().to_string(),
                                    thumbnail: asset_server
                                        .load(format!("animations/thumbnails/{name}_256.png")),
                                    available_representations: representations
                                        .keys()
                                        .cloned()
                                        .collect(),
                                    name: name.clone(),
                                    description: Default::default(),
                                    extra_data: (),
                                },
                                representations,
                            };

                            emotes.add_builtin(urn, collectible);

                            debug!("added animation {name} from {:?}", h_clip.path());
                        }
                        false
                    }
                    None => true,
                },
            );

            if h_gltfs.is_empty() {
                *state = AnimLoadState::Done;
            }
        }
        AnimLoadState::Done => {}
    }
}

struct DefaultAnim {
    male: &'static str,
    female: &'static str,
    repeat: bool,
}

impl DefaultAnim {
    fn new(male: &'static str, female: &'static str, repeat: bool) -> Self {
        Self {
            male,
            female,
            repeat,
        }
    }
}

static DEFAULT_ANIMATION_LOOKUP: Lazy<HashMap<&str, DefaultAnim>> = Lazy::new(|| {
    HashMap::from_iter([
        (
            "handsair",
            DefaultAnim::new("Hands_In_The_Air", "Hands_In_The_Air", false),
        ),
        ("wave", DefaultAnim::new("Wave_Male", "Wave_Female", false)),
        (
            "fistpump",
            DefaultAnim::new("M_FistPump", "F_FistPump", false),
        ),
        (
            "dance",
            DefaultAnim::new("Dance_Male", "Dance_Female", true),
        ),
        (
            "raiseHand",
            DefaultAnim::new("Raise_Hand", "Raise_Hand", false),
        ),
        // "clap" defaults
        (
            "money",
            DefaultAnim::new(
                "Armature|Throw Money-Emote_v02|BaseLayer",
                "Armature|Throw Money-Emote_v02|BaseLayer",
                false,
            ),
        ),
        // "kiss" defaults
        ("headexplode", DefaultAnim::new("explode", "explode", false)),
        // "shrug" defaults
    ])
});

#[derive(PartialEq, Eq, Hash, Debug, TypePath, Clone)]
pub struct Emote {
    pub gltf: Handle<Gltf>,
    pub default_repeat: bool,
    pub sound: Option<Handle<AudioSource>>,
}

impl Emote {
    pub fn avatar_animation(&self, gltfs: &Assets<Gltf>) -> Option<Handle<AnimationClip>> {
        gltfs
            .get(self.gltf.id())?
            .named_animations
            .iter()
            .find(|(name, _)| name.ends_with("_Avatar"))
            .map(|(_, handle)| handle)
            .cloned()
    }

    pub fn prop_scene(&self, gltfs: &Assets<Gltf>) -> Option<Handle<Gltf>> {
        let gltf = gltfs.get(self.gltf.id())?;
        if gltf.meshes.is_empty() {
            None
        } else {
            Some(self.gltf.clone())
        }
    }

    pub fn prop_anim(&self, gltfs: &Assets<Gltf>) -> Option<Handle<AnimationClip>> {
        gltfs
            .get(self.gltf.id())?
            .named_animations
            .iter()
            .find(|(name, _)| name.ends_with("_Prop"))
            .map(|(_, handle)| handle)
            .cloned()
    }

    pub fn audio(&self) -> Option<Handle<AudioSource>> {
        self.sound.clone()
    }
}

impl CollectibleType for Emote {
    type Meta = EmoteMeta;
    type ExtraData = ();

    fn base_collection() -> Option<&'static str> {
        Some("urn:decentraland:off-chain:base-emotes")
    }

    fn extension() -> &'static str {
        "emote"
    }

    fn data_extension() -> &'static str {
        "emote_data"
    }
}

pub struct EmoteLoader;

impl AssetLoader for EmoteLoader {
    type Asset = Collectible<Emote>;

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
            let meta = serde_json::from_value::<EmoteMeta>(metadata)?;

            let thumbnail =
                load_context.load(load_context.path().parent().unwrap().join(&meta.thumbnail));

            let mut representations = HashMap::default();

            for representation in meta.emote_extended_data.representations.into_iter() {
                let gltf = load_context.load(
                    load_context
                        .path()
                        .parent()
                        .unwrap()
                        .join(&representation.main_file),
                );

                let sound = representation
                    .contents
                    .iter()
                    .find(|f| f.ends_with(".mp3") || f.ends_with(".ogg"))
                    .map(|af| load_context.load(load_context.path().parent().unwrap().join(af)));

                for body_shape in representation.body_shapes {
                    representations.insert(
                        body_shape.to_lowercase(),
                        Emote {
                            gltf: gltf.clone(),
                            default_repeat: meta.emote_extended_data.loops,
                            sound: sound.clone(),
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
                    extra_data: (),
                },
                representations,
            })
        })
    }
}

struct EmoteMetaLoader;

impl AssetLoader for EmoteMetaLoader {
    type Asset = CollectibleData<Emote>;

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
            let meta = serde_json::from_value::<EmoteMeta>(metadata)?;

            let thumbnail =
                load_context.load(load_context.path().parent().unwrap().join(&meta.thumbnail));

            let available_representations = meta
                .emote_extended_data
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
                extra_data: (),
            })
        })
    }
}
