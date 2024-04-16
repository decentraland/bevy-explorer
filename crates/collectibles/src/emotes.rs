use std::{any::TypeId, path::PathBuf};

use anyhow::anyhow;
use bevy::{
    asset::{AssetLoader, LoadState, LoadedFolder},
    gltf::Gltf,
    prelude::*,
    utils::HashMap,
};
use ipfs::{
    ipfs_path::{IpfsPath, IpfsType},
    EntityDefinitionLoader,
};
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
    gltfs: Res<Assets<Gltf>>,
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
                        for (name, h_clip) in anims {
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

                            let urn = EmoteUrn::new(&name).unwrap();
                            debug!("loaded default anim {:?}", urn);

                            let emote = Emote {
                                avatar_animation: h_clip.clone(),
                                default_repeat: repeat,
                                props: None,
                                prop_animation: None,
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
    pub avatar_animation: Handle<AnimationClip>,
    pub default_repeat: bool,
    pub props: Option<Handle<Gltf>>,
    pub prop_animation: Option<Handle<AnimationClip>>,
    pub sound: Option<Handle<AudioSource>>,
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

fn content_file_path(file_path: impl Into<String>, content_hash: impl Into<String>) -> PathBuf {
    let ipfs_path = IpfsPath::new(IpfsType::new_content_file(
        content_hash.into(),
        file_path.into(),
    ));
    PathBuf::from(&ipfs_path)
}

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

            let thumbnail = load_context.load(content_file_path(&meta.thumbnail, &entity.id));

            let mut representations = HashMap::default();

            for representation in meta.emote_extended_data.representations.into_iter() {
                let loaded_asset = load_context
                    .load_direct(content_file_path(representation.main_file, &entity.id))
                    .await?;

                let gltf = loaded_asset
                    .take::<Gltf>()
                    .ok_or_else(|| anyhow!("emote gltf load failed"))?;
                let avatar_animation = gltf
                    .named_animations
                    .iter()
                    .find(|(name, _)| name.ends_with("_Avatar"))
                    .map(|(_, handle)| handle)
                    .ok_or(anyhow!("no animation"))?
                    .clone();
                let prop_animation = gltf
                    .named_animations
                    .iter()
                    .find(|(name, _)| name.ends_with("_Prop"))
                    .map(|(_, handle)| handle)
                    .cloned();

                let props = if !gltf.meshes.is_empty() {
                    Some(load_context.get_label_handle("gltf"))
                } else {
                    None
                };

                let sound = representation
                    .contents
                    .iter()
                    .find(|f| f.ends_with(".mp3") || f.ends_with(".ogg"))
                    .map(|af| load_context.load(content_file_path(af, &entity.id)));

                load_context.add_labeled_asset("gltf".to_owned(), gltf);

                for body_shape in representation.body_shapes {
                    representations.insert(
                        body_shape.to_lowercase(),
                        Emote {
                            avatar_animation: avatar_animation.clone(),
                            default_repeat: meta.emote_extended_data.loops,
                            props: props.clone(),
                            prop_animation: prop_animation.clone(),
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
