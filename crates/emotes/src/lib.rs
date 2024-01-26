use bevy::{
    gltf::Gltf,
    prelude::*,
    utils::{hashbrown::HashSet, HashMap},
};
use common::util::TaskExt;
use comms::profile::UserProfile;
use ipfs::{ActiveEntityTask, EntityDefinition, IpfsAssetServer};
use itertools::Itertools;
use serde::Deserialize;

pub struct EmotesPlugin;

impl Plugin for EmotesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EmoteLoadData>();
        app.init_resource::<AvatarAnimations>();
        app.add_systems(Update, (fetch_emotes, fetch_emote_details));
    }
}

pub struct AvatarAnimation {
    pub name: String,
    pub description: String,
    pub clip: Handle<AnimationClip>,
    pub thumbnail: Handle<Image>,
}

#[derive(Resource, Default)]
pub struct AvatarAnimations(pub HashMap<String, AvatarAnimation>);

#[derive(Resource, Default)]
pub struct EmoteLoadData {
    loaded: HashSet<String>,
    unprocessed: Vec<EntityDefinition>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EmoteMeta {
    name: String,
    description: String,
    thumbnail: String,
}

fn fetch_emotes(
    profiles: Query<&UserProfile>,
    mut defs: ResMut<EmoteLoadData>,
    ipfas: IpfsAssetServer,
    mut task: Local<Option<ActiveEntityTask>>,
) {
    let required_emote_urns = profiles
        .iter()
        .flat_map(|p| p.content.avatar.emotes.as_ref())
        .flatten()
        .map(|e| &e.urn)
        .map(|urn| urn.splitn(7, ':').take(6).join(":"))
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
            Err(e) => warn!("active entities task failed: {e}"),
        }
        *task = None;
    }

    if task.is_none() {
        let missing_urns = required_emote_urns
            .into_iter()
            .filter(|urn| urn.contains(':') && !defs.loaded.contains(urn))
            .collect::<Vec<_>>();
        if !missing_urns.is_empty() {
            *task = Some(
                ipfas
                    .ipfs()
                    .active_entities(ipfs::ActiveEntitiesRequest::Pointers(missing_urns), None),
            );
        }
    }
}

fn fetch_emote_details(
    mut defs: ResMut<EmoteLoadData>,
    mut avatar_anims: ResMut<AvatarAnimations>,
    mut loading_gltf: Local<Vec<(EntityDefinition, Handle<Gltf>)>>,
    ipfas: IpfsAssetServer,
    gltfs: Res<Assets<Gltf>>,
) {
    for def in defs.unprocessed.drain(..) {
        let Some(first_glb) = def
            .content
            .files()
            .find(|f| f.to_lowercase().ends_with(".glb"))
        else {
            warn!("no glb found in emote content map");
            continue;
        };

        ipfas.ipfs().add_collection(
            def.id.clone(),
            def.content.clone(),
            None,
            def.metadata.as_ref().map(serde_json::Value::to_string),
        );
        let gltf = ipfas.load_content_file(first_glb, &def.id).unwrap();
        loading_gltf.push((def, gltf));
    }

    *loading_gltf = loading_gltf
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
                                clip: anim.1.clone(),
                                thumbnail: ipfas
                                    .load_content_file(&metadata.thumbnail, &def.id)
                                    .unwrap(),
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
