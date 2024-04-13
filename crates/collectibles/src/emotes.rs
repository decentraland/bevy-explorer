use std::any::TypeId;

use bevy::{
    asset::{LoadState, LoadedFolder},
    gltf::Gltf,
    prelude::*,
    utils::{hashbrown::HashSet, HashMap},
};
use common::{profile::AvatarEmote, util::TaskExt};
use comms::profile::UserProfile;
use ipfs::{ActiveEntityTask, ContentMap, EntityDefinition, IpfsAssetServer};
use serde::{Deserialize, Serialize};

use once_cell::sync::Lazy;

use crate::{
    urn::{CollectibleInstance, CollectibleUrn},
    CollectibleType,
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
        app.init_resource::<EmoteLoadData>();
        app.init_resource::<AvatarAnimations>();
        app.add_systems(
            Update,
            (
                fetch_scene_emotes,
                fetch_emotes,
                fetch_emote_details,
                load_animations,
            ),
        );
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct EmoteMarker;

impl CollectibleType for EmoteMarker {
    fn base_collection() -> Option<&'static str> {
        Some("urn:decentraland:off-chain:base-emotes")
    }
}

pub type EmoteUrn = CollectibleUrn<EmoteMarker>;
pub type EmoteInstance = CollectibleInstance<EmoteMarker>;

impl EmoteUrn {
    pub fn scene_emote(&self) -> Option<&str> {
        self.skip_take(0, 5)
            .rsplit_once(':')
            .filter(|(base, _)| *base == "urn:decentraland:off-chain:scene-emote")
            .map(|(_, emote)| emote)
    }
}

#[derive(Debug)]
pub struct AvatarAnimation {
    pub hash: Option<String>,
    pub name: String,
    pub description: String,
    pub clips: HashMap<String, Handle<AnimationClip>>,
    pub thumbnail: Option<Handle<Image>>,
    pub repeat: bool,
}

#[derive(Resource, Default, Debug)]
pub struct AvatarAnimations(pub HashMap<EmoteUrn, AvatarAnimation>);

impl AvatarAnimations {
    pub fn get_server(&self, urn: impl AsRef<EmoteUrn>) -> Option<&AvatarAnimation> {
        let res = self.0.get(urn.as_ref());
        if res.is_none() {
            println!(
                "failed to get {:?}, contents: {:?}",
                urn.as_ref(),
                self.0.keys().collect::<Vec<_>>()
            );
        }
        res
    }

    pub fn get_scene_or_server(
        &self,
        urn: impl AsRef<EmoteUrn>,
        load_data: &mut EmoteLoadData,
    ) -> Option<&AvatarAnimation> {
        let urn = urn.as_ref();
        match self.0.get(urn) {
            Some(anim) => Some(anim),
            None => {
                if urn.scene_emote().is_some() {
                    load_data.requested_scene_emotes.insert(urn.clone());
                }
                None
            }
        }
    }
}

#[derive(Resource, Default)]
pub struct EmoteLoadData {
    requested_scene_emotes: HashSet<EmoteUrn>,
    unprocessed: Vec<EntityDefinition>,
    loading_gltf: Vec<(EntityDefinition, Handle<Gltf>)>,
    loaded: HashSet<EmoteUrn>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EmoteMeta {
    name: String,
    description: String,
    thumbnail: Option<String>,
}

fn fetch_emotes(
    profiles: Query<&UserProfile>,
    mut defs: ResMut<EmoteLoadData>,
    ipfas: IpfsAssetServer,
    mut task: Local<Option<ActiveEntityTask>>,
) {
    if !ipfas.is_connected() {
        return;
    }

    let required_emote_urns: HashSet<CollectibleUrn<EmoteMarker>> = profiles
        .iter()
        .flat_map(|p| p.content.avatar.emotes.as_ref())
        .flatten()
        .filter_map(|AvatarEmote { urn, .. }| EmoteUrn::new(urn).ok())
        .collect::<HashSet<_>>();

    if let Some(result) = task.as_mut().and_then(|t| t.complete()) {
        match result {
            Ok(res) => {
                for def in res.iter() {
                    debug!("found emote: {:#?}", def);
                }

                defs.loaded.extend(res.iter().filter_map(|def| {
                    def.pointers
                        .first()
                        .cloned()
                        .and_then(|p| EmoteUrn::try_from(p.as_str()).ok())
                }));
                defs.unprocessed.extend(res);
            }
            Err(e) => warn!("emote active entities task failed: {e}"),
        }
        *task = None;
    }

    if task.is_none() {
        let missing_urns = required_emote_urns
            .iter()
            .filter(|urn| !defs.loaded.contains(*urn))
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        if !missing_urns.is_empty() {
            debug!("fetching emotes: {missing_urns:?}");
            *task = Some(
                ipfas
                    .ipfs()
                    .active_entities(ipfs::ActiveEntitiesRequest::Pointers(missing_urns), None),
            );
        }
    }
}

fn fetch_scene_emotes(mut defs: ResMut<EmoteLoadData>) {
    for scene_emote in std::mem::take(&mut defs.requested_scene_emotes) {
        if !defs.loaded.contains(&scene_emote) {
            // loaded, remove from requested list
            // new
            let Some(scene_emote) = scene_emote.scene_emote() else {
                warn!("invalid scene emote {:?}", scene_emote);
                continue;
            };
            let Some((hash, _)) = scene_emote.split_once('-') else {
                warn!("malformed scene emote `{scene_emote}`");
                continue;
            };

            defs.unprocessed.push(EntityDefinition {
                id: format!("urn:decentraland:off-chain:scene-emote:{}", hash),
                pointers: vec![format!(
                    "urn:decentraland:off-chain:scene-emote:{}-true",
                    hash
                )],
                content: ContentMap::new_single("scene_emote.glb".to_owned(), hash.to_owned()),
                metadata: Some(
                    serde_json::to_value(&EmoteMeta {
                        name: format!("{}-true", hash.to_owned()),
                        description: format!("{}-true", hash.to_owned()),
                        thumbnail: None,
                    })
                    .unwrap(),
                ),
            });
            defs.loaded.insert(
                EmoteUrn::new(
                    format!(
                        "urn:decentraland:off-chain:scene-emote:{}-true",
                        hash.to_owned()
                    )
                    .as_str(),
                )
                .unwrap(),
            );

            defs.unprocessed.push(EntityDefinition {
                id: format!("urn:decentraland:off-chain:scene-emote:{}", hash),
                pointers: vec![format!(
                    "urn:decentraland:off-chain:scene-emote:{}-false",
                    hash
                )],
                content: ContentMap::new_single("scene_emote.glb".to_owned(), hash.to_owned()),
                metadata: Some(
                    serde_json::to_value(&EmoteMeta {
                        name: format!("{}-false", hash.to_owned()),
                        description: format!("{}-false", hash.to_owned()),
                        thumbnail: None,
                    })
                    .unwrap(),
                ),
            });
            defs.loaded.insert(
                EmoteUrn::new(
                    format!(
                        "urn:decentraland:off-chain:scene-emote:{}-false",
                        hash.to_owned()
                    )
                    .as_str(),
                )
                .unwrap(),
            );
        }
    }
}

fn fetch_emote_details(
    mut defs: ResMut<EmoteLoadData>,
    mut avatar_anims: ResMut<AvatarAnimations>,
    ipfas: IpfsAssetServer,
    gltfs: Res<Assets<Gltf>>,
    clips: Res<Assets<AnimationClip>>,
) {
    for def in std::mem::take(&mut defs.unprocessed) {
        // let metadata: EmoteMeta = def
        //     .metadata
        //     .and_then(|m| serde_json::from_value(m).ok())
        //     .unwrap_or_default();

        let Some(first_glb) = def
            .content
            .files()
            .find(|f| f.to_lowercase().ends_with(".glb"))
        else {
            warn!("no glb found in emote content map");
            continue;
        };

        ipfas
            .ipfs()
            .add_collection(def.id.clone(), def.content.clone(), None, None);
        debug!("added collection {} -> {:?}", def.id, def.content);
        debug!("loading gltf {}", first_glb);
        let gltf = ipfas.load_content_file(first_glb, &def.id).unwrap();
        defs.loading_gltf.push((def, gltf));
    }

    defs.loading_gltf = defs
        .loading_gltf
        .drain(..)
        .flat_map(
            |(def, h_gltf)| match ipfas.asset_server().load_state(&h_gltf) {
                bevy::asset::LoadState::Loading => Some((def, h_gltf)),
                bevy::asset::LoadState::Loaded => {
                    let metadata: EmoteMeta = def
                        .metadata
                        .and_then(|m| serde_json::from_value(m).ok())
                        .unwrap_or_default();
                    let anims = &gltfs.get(h_gltf).unwrap().named_animations;
                    let not_starting_pose_anims = anims
                        .iter()
                        .filter(|(name, _)| *name != "Starting_Pose")
                        .filter(|(_, h_anim)| {
                            clips
                                .get(*h_anim)
                                .map_or(false, |clip| clip.compatible_with(&Name::new("Armature")))
                        })
                        .collect::<Vec<_>>();
                    if not_starting_pose_anims.len() != 1 {
                        warn!(
                            "{} anims has {} valid members ({:?}) of {} total",
                            def.id,
                            not_starting_pose_anims.len(),
                            not_starting_pose_anims,
                            anims.len(),
                        );
                    }
                    let Some(urn) = def.pointers.first().and_then(|p| EmoteUrn::new(p).ok()) else {
                        warn!("invalid emote pointer {:?}", def.pointers.first());
                        return None;
                    };
                    if let Some(anim) = not_starting_pose_anims.first() {
                        debug!("added emote {:?}", urn);
                        avatar_anims.0.insert(
                            urn,
                            AvatarAnimation {
                                hash: Some(def.id.clone()),
                                name: metadata.name,
                                description: metadata.description,
                                clips: HashMap::from_iter(
                                    base_bodyshapes()
                                        .into_iter()
                                        .map(|body| (body, anim.1.clone())),
                                ),
                                thumbnail: metadata
                                    .thumbnail
                                    .as_ref()
                                    .map(|thumb| ipfas.load_content_file(thumb, &def.id).unwrap()),
                                repeat: false, // TODO: parse extended data
                            },
                        );
                    }
                    None
                }
                bevy::asset::LoadState::NotLoaded | bevy::asset::LoadState::Failed => {
                    warn!("failed to load animation gltf for {}", def.id);
                    None
                }
            },
        )
        .collect();
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
    mut animations: ResMut<AvatarAnimations>,
    mut defs: ResMut<EmoteLoadData>,
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
                                .unwrap_or((name.to_owned(), false, false, false));

                            let urn = EmoteUrn::new(&name).unwrap();
                            debug!("loaded default anim {:?}", urn);

                            let anim = animations.0.entry(urn.clone()).or_insert_with(|| {
                                AvatarAnimation {
                                    hash: None,
                                    name: name.clone(),
                                    description: name.clone(),
                                    clips: HashMap::from_iter(
                                        base_bodyshapes()
                                            .into_iter()
                                            .map(|body| (body, h_clip.clone())),
                                    ),
                                    thumbnail: Some(
                                        asset_server
                                            .load(format!("animations/thumbnails/{name}_256.png")),
                                    ),
                                    repeat,
                                }
                            });

                            if is_female {
                                anim.clips
                                    .insert(base_bodyshapes().remove(0), h_clip.clone());
                            }
                            if is_male {
                                anim.clips
                                    .insert(base_bodyshapes().remove(1), h_clip.clone());
                            }
                            defs.loaded.insert(urn);
                            debug!("added animation {name}: {anim:?} from {:?}", h_clip.path());
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
