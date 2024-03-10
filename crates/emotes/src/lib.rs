use std::borrow::Cow;

use bevy::{
    gltf::Gltf,
    prelude::*,
    utils::{hashbrown::HashSet, HashMap},
};
use common::util::TaskExt;
use comms::profile::UserProfile;
use ipfs::{ActiveEntityTask, ContentMap, EntityDefinition, IpfsAssetServer};
use itertools::Itertools;
use serde::{Deserialize, Serialize};

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
            (fetch_scene_emotes, fetch_emotes, fetch_emote_details),
        );
    }
}

#[derive(Debug)]
pub struct AvatarAnimation {
    pub name: String,
    pub description: String,
    pub clips: HashMap<String, Handle<AnimationClip>>,
    pub thumbnail: Option<Handle<Image>>,
    pub repeat: bool,
}

#[derive(Resource, Default, Debug)]
pub struct AvatarAnimations(pub HashMap<String, AvatarAnimation>);

impl AvatarAnimations {
    pub fn get_server(&self, urn: &str) -> Option<&AvatarAnimation> {
        let urn = urn_for_emote_specifier(urn);
        self.0.get::<str>(&urn)
    }
    pub fn get_scene_or_server(
        &self,
        urn: &str,
        load_data: &mut EmoteLoadData,
    ) -> Option<&AvatarAnimation> {
        let urn = urn_for_emote_specifier(urn);

        match self.0.get::<str>(&urn) {
            Some(anim) => Some(anim),
            None => {
                let collection = urn.split(':').skip(3).take(1).next();
                if collection == Some("scene-emote") {
                    load_data.requested_scene_emotes.insert(urn.to_string());
                }
                None
            }
        }
    }
}

#[derive(Resource, Default)]
pub struct EmoteLoadData {
    requested_scene_emotes: HashSet<String>,
    unprocessed: Vec<EntityDefinition>,
    loading_gltf: Vec<(EntityDefinition, Handle<Gltf>)>,
    loaded: HashSet<String>,
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

    let required_emote_urns = profiles
        .iter()
        .flat_map(|p| p.content.avatar.emotes.as_ref())
        .flatten()
        .map(|e| &e.urn)
        .collect::<HashSet<_>>();

    if let Some(result) = task.as_mut().and_then(|t| t.complete()) {
        match result {
            Ok(res) => {
                for def in res.iter() {
                    debug!("found emote: {:#?}", def);
                }

                defs.loaded.extend(
                    res.iter()
                        .map(|def| def.pointers.first().cloned().unwrap_or_default()),
                );
                defs.unprocessed.extend(res);
            }
            Err(e) => warn!("emote active entities task failed: {e}"),
        }
        *task = None;
    }

    if task.is_none() {
        let missing_urns = required_emote_urns
            .into_iter()
            .map(|urn| urn_for_emote_specifier(urn).into_owned())
            .filter(|urn| !defs.loaded.contains(urn))
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
            let Some((base, _)) = scene_emote.rsplit_once('-') else {
                warn!("malformed scene emote `{scene_emote}`");
                continue;
            };

            let Some((_, hash)) = base.rsplit_once(':') else {
                warn!("malformed scene emote `{scene_emote}`");
                continue;
            };

            defs.unprocessed.push(EntityDefinition {
                id: base.to_owned(),
                pointers: vec![format!("{}-true", base.to_owned())],
                content: ContentMap::new_single("scene_emote.glb".to_owned(), hash.to_owned()),
                metadata: Some(
                    serde_json::to_value(&EmoteMeta {
                        name: format!("{}-true", base.to_owned()),
                        description: format!("{}-true", base.to_owned()),
                        thumbnail: None,
                    })
                    .unwrap(),
                ),
            });
            defs.loaded.insert(format!("{}-true", base.to_owned()));

            defs.unprocessed.push(EntityDefinition {
                id: base.to_owned(),
                pointers: vec![format!("{}-false", base.to_owned())],
                content: ContentMap::new_single("scene_emote.glb".to_owned(), hash.to_owned()),
                metadata: Some(
                    serde_json::to_value(&EmoteMeta {
                        name: format!("{}-false", base.to_owned()),
                        description: format!("{}-false", base.to_owned()),
                        thumbnail: None,
                    })
                    .unwrap(),
                ),
            });
            defs.loaded.insert(format!("{}-false", base.to_owned()));
        }
    }
}

fn fetch_emote_details(
    mut defs: ResMut<EmoteLoadData>,
    mut avatar_anims: ResMut<AvatarAnimations>,
    ipfas: IpfsAssetServer,
    gltfs: Res<Assets<Gltf>>,
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

    defs.loading_gltf =
        defs.loading_gltf
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
                            .filter(|a| a.0 != "Starting_Pose")
                            .collect::<Vec<_>>();
                        if not_starting_pose_anims.len() != 1 {
                            warn!(
                                "{} anims has {} members ({:?})",
                                def.id,
                                anims.len(),
                                anims.keys()
                            );
                        }
                        if let Some(anim) = not_starting_pose_anims.first() {
                            avatar_anims.0.insert(
                                def.pointers.first().cloned().unwrap_or_default(),
                                AvatarAnimation {
                                    name: metadata.name,
                                    description: metadata.description,
                                    clips: HashMap::from_iter(
                                        base_bodyshapes()
                                            .into_iter()
                                            .map(|body| (body, anim.1.clone())),
                                    ),
                                    thumbnail: metadata.thumbnail.as_ref().map(|thumb| {
                                        ipfas.load_content_file(thumb, &def.id).unwrap()
                                    }),
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

pub fn urn_for_emote_specifier(specifier: &str) -> Cow<str> {
    if !specifier.contains(':') {
        Cow::Owned(format!(
            "urn:decentraland:off-chain:base-emotes:{}",
            specifier
        ))
    } else if specifier.split(':').nth(6).is_some() {
        Cow::Owned(specifier.split(':').take(6).join(":"))
    } else {
        Cow::Borrowed(specifier)
    }
}
