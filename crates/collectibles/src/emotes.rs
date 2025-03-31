use std::any::TypeId;

use anyhow::anyhow;
use bevy::{
    asset::{AssetLoader, LoadState, LoadedFolder},
    gltf::Gltf,
    prelude::*,
    utils::{ConditionalSendFuture, HashMap, HashSet},
};
use ipfs::EntityDefinitionLoader;
use serde::{Deserialize, Serialize};

use once_cell::sync::Lazy;

use crate::{
    urn::{CollectibleInstance, CollectibleUrn},
    Collectible, CollectibleData, CollectibleError, CollectibleManager, CollectibleType,
    CollectiblesTypePlugin,
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
        app.init_resource::<BaseEmotes>();
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

#[derive(Resource, Default)]
pub struct BaseEmotes(pub HashSet<CollectibleUrn<Emote>>);

#[allow(clippy::type_complexity)]
fn load_animations(
    asset_server: Res<AssetServer>,
    mut gltfs: ResMut<Assets<Gltf>>,
    mut state: Local<AnimLoadState>,
    folders: Res<Assets<LoadedFolder>>,
    mut emotes: CollectibleManager<Emote>,
    mut base_emotes: ResMut<BaseEmotes>,
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
                        for (clip_name, h_clip) in anims.clone() {
                            let Some((
                                network_name,
                                friendly_name,
                                repeat,
                                is_male,
                                is_female,
                                register_base,
                                sound,
                            )) = DEFAULT_ANIMATION_LOOKUP
                                .iter()
                                .find(|(_, anim)| {
                                    anim.male.eq_ignore_ascii_case(&clip_name)
                                        || anim.female.eq_ignore_ascii_case(&clip_name)
                                })
                                .map(|(urn, anim)| {
                                    (
                                        urn.to_string(),
                                        anim.name,
                                        anim.repeat,
                                        anim.male.eq_ignore_ascii_case(&clip_name),
                                        anim.female.eq_ignore_ascii_case(&clip_name),
                                        anim.register_base_emote,
                                        &anim.sound,
                                    )
                                })
                            else {
                                continue;
                            };

                            let new_gltf = Gltf {
                                named_animations: HashMap::from_iter([(
                                    "_Avatar".into(),
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

                            let urn = EmoteUrn::new(&network_name).unwrap();
                            debug!("loaded default anim {:?} from {:?}", urn, h_gltf.path());

                            let sound = sound
                                .iter()
                                .map(|(t, fs)| {
                                    (
                                        *t,
                                        fs.iter()
                                            .map(|f| {
                                                asset_server.load(format!("sounds/avatar/{f}.wav"))
                                            })
                                            .collect(),
                                    )
                                })
                                .collect();

                            let emote = Emote {
                                gltf: new_gltf,
                                default_repeat: repeat,
                                sound,
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
                                    thumbnail: if register_base {
                                        asset_server.load(format!(
                                            "animations/thumbnails/{network_name}.png"
                                        ))
                                    } else {
                                        Handle::default()
                                    },
                                    available_representations: representations
                                        .keys()
                                        .cloned()
                                        .collect(),
                                    name: friendly_name.to_owned(),
                                    description: Default::default(),
                                    extra_data: (),
                                },
                                representations,
                            };

                            if register_base {
                                base_emotes.0.insert(urn.clone());
                            }

                            emotes.add_builtin(urn, collectible);

                            debug!("added animation {network_name} from {:?}", h_clip.path());
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
    name: &'static str,
    repeat: bool,
    register_base_emote: bool,
    sound: Vec<(f32, Vec<String>)>,
}

impl DefaultAnim {
    fn new(
        name: &'static str,
        male: &'static str,
        female: &'static str,
        repeat: bool,
        base: bool,
    ) -> Self {
        Self {
            name,
            male,
            female,
            repeat,
            register_base_emote: base,
            sound: Default::default(),
        }
    }

    fn with_sounds(self, sounds: &[(f32, &[&'static str])]) -> Self {
        Self {
            sound: sounds
                .iter()
                .map(|(t, files)| (*t, files.iter().map(|s| s.to_string()).collect()))
                .collect(),
            ..self
        }
    }
}

static DEFAULT_ANIMATION_LOOKUP: Lazy<HashMap<&str, DefaultAnim>> = Lazy::new(|| {
    HashMap::from_iter([
        (
            "wave",
            DefaultAnim::new("Wave", "Wave_Male", "Wave_Female", false, true),
        ),
        (
            "fistpump",
            DefaultAnim::new("Fist Pump", "M_FistPump", "F_FistPump", false, true),
        ),
        (
            "robot",
            DefaultAnim::new("Robot", "M_RobotDance", "F_RobotDance", true, true),
        ),
        (
            "raiseHand",
            DefaultAnim::new("Raise Hand", "Raise_Hand", "Raise_Hand", false, true),
        ),
        (
            "clap",
            DefaultAnim::new("Clap", "clap", "clap", false, true),
        ),
        (
            "money",
            DefaultAnim::new(
                "Money",
                "Armature|Throw Money-Emote_v02|BaseLayer",
                "Armature|Throw Money-Emote_v02|BaseLayer",
                false,
                true,
            ),
        ),
        (
            "kiss",
            DefaultAnim::new("Kiss", "kiss", "kiss", false, true),
        ),
        (
            "hammer",
            DefaultAnim::new(
                "Hammer",
                "Armature|mchammer-dance_v02|BaseLayer",
                "Armature|mchammer-dance_v02|BaseLayer",
                true,
                true,
            ),
        ),
        (
            "tik",
            DefaultAnim::new(
                "Tik",
                "Armature|tik-tok-dance_v02|BaseLayer",
                "Armature|tik-tok-dance_v02|BaseLayer",
                true,
                true,
            ),
        ),
        (
            "tektonik",
            DefaultAnim::new(
                "Tektonic",
                "Armature|tektonik-dance_v01|BaseLayer",
                "Armature|tektonik-dance_v01|BaseLayer",
                true,
                true,
            ),
        ),
        (
            "dontsee",
            DefaultAnim::new("Don't See", "dont_wanna_see", "dont_wanna_see", false, true),
        ),
        (
            "handsair",
            DefaultAnim::new(
                "Hands Air",
                "Hands_In_The_Air",
                "Hands_In_The_Air",
                true,
                true,
            ),
        ),
        (
            "shrug",
            DefaultAnim::new("Shrug", "shrug", "shrug", false, true),
        ),
        (
            "disco",
            DefaultAnim::new("Disco", "disco_dance", "disco_dance", true, true),
        ),
        (
            "headexplode",
            DefaultAnim::new("Head Explode", "explode", "f_head_explode", false, true),
        ),
        (
            "dance",
            DefaultAnim::new("Dance", "Dance_Male", "Dance_Female", true, true),
        ),
        // base animations, not emotes
        (
            "idle_male",
            DefaultAnim::new("idle_male", "Idle_Male", "Idle_Male", true, false),
        ),
        (
            "walk",
            DefaultAnim::new("walk", "Walk", "Walk", true, false)
                .with_sounds(&[(0.41, WALK_STEPS), (0.91, WALK_STEPS)]),
        ),
        (
            "run",
            DefaultAnim::new("run", "Run", "Run", true, false)
                .with_sounds(&[(0.21, RUN_STEPS), (0.54, RUN_STEPS)]),
        ),
        (
            "jump",
            DefaultAnim::new("jump", "Jump", "Jump", false, false).with_sounds(&[
                (
                    0.0,
                    &[
                        "avatar_footstep_jump01",
                        "avatar_footstep_jump02",
                        "avatar_footstep_jump03",
                    ],
                ),
                (0.6, &["avatar_footstep_land01", "avatar_footstep_land02"]),
            ]),
        ),
    ])
});

static RUN_STEPS: &[&str] = &[
    "avatar_footstep_run01",
    "avatar_footstep_run02",
    "avatar_footstep_run03",
    "avatar_footstep_run04",
    "avatar_footstep_run05",
    "avatar_footstep_run06",
    "avatar_footstep_run07",
    "avatar_footstep_run08",
];

static WALK_STEPS: &[&str] = &[
    "avatar_footstep_walk01",
    "avatar_footstep_walk02",
    "avatar_footstep_walk03",
    "avatar_footstep_walk04",
    "avatar_footstep_walk05",
    "avatar_footstep_walk06",
    "avatar_footstep_walk07",
    "avatar_footstep_walk08",
];

#[derive(PartialEq, Debug, TypePath, Clone)]
pub struct Emote {
    pub gltf: Handle<Gltf>,
    pub default_repeat: bool,
    pub sound: Vec<(f32, Vec<Handle<bevy_kira_audio::AudioSource>>)>,
}

impl Emote {
    pub fn avatar_animation(
        &self,
        gltfs: &Assets<Gltf>,
    ) -> Result<Option<Handle<AnimationClip>>, CollectibleError> {
        let gltf = gltfs.get(self.gltf.id()).ok_or(CollectibleError::Loading)?;
        if let Some(anim) = gltf
            .named_animations
            .iter()
            .find(|(name, _)| name.ends_with("_Avatar"))
            .map(|(_, handle)| handle)
            .cloned()
        {
            Ok(Some(anim))
        } else if gltf.named_animations.len() == 1 {
            Ok(Some(gltf.named_animations.iter().next().unwrap().1.clone()))
        } else {
            Ok(None)
        }
    }

    pub fn prop_scene(
        &self,
        gltfs: &Assets<Gltf>,
    ) -> Result<Option<Handle<Scene>>, CollectibleError> {
        let gltf = gltfs.get(self.gltf.id()).ok_or(CollectibleError::Loading)?;
        Ok(if gltf.meshes.is_empty() {
            None
        } else {
            gltf.default_scene.clone()
        })
    }

    pub fn prop_anim(
        &self,
        gltfs: &Assets<Gltf>,
    ) -> Result<Option<Handle<AnimationClip>>, CollectibleError> {
        Ok(gltfs
            .get(self.gltf.id())
            .ok_or(CollectibleError::Loading)?
            .named_animations
            .iter()
            .find(|(name, _)| name.ends_with("_Prop"))
            .map(|(_, handle)| handle)
            .cloned())
    }

    pub fn audio(
        &self,
        audio: &Assets<bevy_kira_audio::AudioSource>,
        after: f32,
    ) -> Result<Option<(f32, Handle<bevy_kira_audio::AudioSource>)>, CollectibleError> {
        self.sound
            .iter()
            .find(|(t, _)| *t > after)
            .map(|(t, clips)| {
                if clips.is_empty() {
                    return Ok(None);
                }
                let clip = &clips[fastrand::usize(0..clips.len())];
                if audio.get(clip.id()).is_some() {
                    Ok(Some((*t, clip.clone())))
                } else {
                    Err(CollectibleError::Loading)
                }
            })
            .unwrap_or(Ok(None))
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
    ) -> impl ConditionalSendFuture<Output = Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            let mut entity = EntityDefinitionLoader
                .load(reader, settings, load_context)
                .await?;
            let metadata = entity.metadata.ok_or(anyhow!("no metadata?"))?;
            debug!("meta: {metadata:#?}");
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
                            sound: vec![(0.0, sound.iter().cloned().collect())],
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
    ) -> impl ConditionalSendFuture<Output = Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            let mut entity = EntityDefinitionLoader
                .load(reader, settings, load_context)
                .await?;
            let metadata = entity.metadata.ok_or(anyhow!("no metadata?"))?;
            debug!("meta: {metadata:#?}");
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
