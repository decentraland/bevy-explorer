use std::path::Path;

use avatar::AvatarDynamicState;
use bevy::{
    math::Vec3Swizzles,
    prelude::*,
    tasks::{IoTaskPool, Task},
    utils::HashMap,
};
use common::{
    rpc::{PortableLocation, RestrictedAction, SpawnResponse},
    sets::SceneSets,
    structs::{PrimaryCamera, PrimaryUser},
    util::TaskExt,
};
use comms::{global_crdt::ForeignPlayer, profile::CurrentUserProfile};
use ipfs::{ipfs_path::IpfsPath, ChangeRealmEvent, EntityDefinition, ServerAbout};
use isahc::{http::StatusCode, AsyncReadResponseExt};
use scene_runner::{
    initialize_scene::{LiveScenes, PortableScenes, PortableSource, SceneLoading, PARCEL_SIZE},
    renderer_context::RendererSceneContext,
    ContainingScene,
};
use ui_core::dialog::SpawnDialog;
use wallet::Wallet;

pub struct RestrictedActionsPlugin;

impl Plugin for RestrictedActionsPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<RestrictedAction>();
        app.add_systems(
            Update,
            (
                move_player,
                move_camera,
                change_realm,
                external_url,
                spawn_portable,
                kill_portable,
                list_portables,
                get_user_data,
                op_get_connected_players,
            )
                .in_set(SceneSets::PostLoop),
        );
    }
}

fn move_player(
    mut commands: Commands,
    mut events: EventReader<RestrictedAction>,
    scenes: Query<&RendererSceneContext>,
    mut player: Query<(Entity, &mut Transform, &mut AvatarDynamicState), With<PrimaryUser>>,
    containing_scene: ContainingScene,
) {
    for (root, transform) in events.iter().filter_map(|ev| match ev {
        RestrictedAction::MovePlayer { scene, to } => Some((scene, to)),
        _ => None,
    }) {
        let Ok(scene) = scenes.get(*root) else {
            continue;
        };

        if !player
            .get_single()
            .ok()
            .map_or(false, |(e, ..)| containing_scene.get(e).contains(root))
        {
            warn!("invalid move request from non-containing scene");
            warn!("request from {root:?}");
            warn!(
                "containing scenes {:?}",
                player.get_single().map(|p| containing_scene.get(p.0))
            );
            return;
        }

        let mut target_transform = *transform;
        target_transform.translation +=
            (scene.base * IVec2::new(1, -1)).as_vec2().extend(0.0).xzy() * PARCEL_SIZE;

        if transform.translation.clamp(
            Vec3::new(0.0, f32::MIN, -PARCEL_SIZE),
            Vec3::new(PARCEL_SIZE, f32::MAX, 0.0),
        ) != transform.translation
        {
            commands.spawn_dialog_two(
                "Teleport".into(),
                "The scene wants to teleport you to another location".into(),
                "Let's go!",
                move |mut player: Query<&mut Transform, With<PrimaryUser>>| {
                    *player.single_mut() = target_transform;
                },
                "No thanks",
                || {},
            );
        } else {
            let (_, mut player_transform, mut dynamics) = player.single_mut();
            dynamics.velocity =
                transform.rotation * player_transform.rotation.inverse() * dynamics.velocity;

            *player_transform = target_transform;
        }
    }
}

fn move_camera(mut events: EventReader<RestrictedAction>, mut camera: Query<&mut PrimaryCamera>) {
    for rotation in events.iter().filter_map(|ev| match ev {
        RestrictedAction::MoveCamera(rotation) => Some(rotation),
        _ => None,
    }) {
        let (yaw, pitch, roll) = rotation.to_euler(EulerRot::YXZ);

        let mut camera = camera.single_mut();
        camera.yaw = yaw;
        camera.pitch = pitch;
        camera.roll = roll;
    }
}

fn change_realm(
    mut commands: Commands,
    mut events: EventReader<RestrictedAction>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
) {
    for (scene, to, message, response) in events.iter().filter_map(|ev| match ev {
        RestrictedAction::ChangeRealm {
            scene,
            to,
            message,
            response,
        } => Some((scene, to, message, response)),
        _ => None,
    }) {
        if !player
            .get_single()
            .ok()
            .map_or(false, |e| containing_scene.get(e).contains(scene))
        {
            warn!("invalid changeRealm request from non-containing scene");
            return;
        }

        let new_realm = to.clone();
        let response_ok = response.clone();
        let response_fail = response.clone();

        commands.spawn_dialog_two(
            "Change Realm".into(),
            format!(
                "The scene wants to move you to a new realm\n`{}`\n{}",
                to.clone(),
                if let Some(message) = message {
                    message
                } else {
                    ""
                }
            ),
            "Let's go!",
            move |mut writer: EventWriter<ChangeRealmEvent>| {
                writer.send(ChangeRealmEvent {
                    new_realm: new_realm.clone(),
                });
                response_ok.send(Ok(()));
            },
            "No thanks",
            move || {
                response_fail.send(Err(String::default()));
            },
        );
    }
}

fn external_url(
    mut commands: Commands,
    mut events: EventReader<RestrictedAction>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
) {
    for (scene, url, response) in events.iter().filter_map(|ev| match ev {
        RestrictedAction::ExternalUrl {
            scene,
            url,
            response,
        } => Some((scene, url, response)),
        _ => None,
    }) {
        if !player
            .get_single()
            .ok()
            .map_or(false, |e| containing_scene.get(e).contains(scene))
        {
            warn!("invalid changeRealm request from non-containing scene");
            return;
        }

        let url = url.clone();
        let response_ok = response.clone();
        let response_fail = response.clone();

        commands.spawn_dialog_two(
            "Open External Link".into(),
            format!(
                "The scene wants to display a link in an external application\n`{}`",
                url.clone(),
            ),
            "Ok",
            move || {
                let result = opener::open(Path::new(&url)).map_err(|e| e.to_string());
                response_ok.send(result);
            },
            "Cancel",
            move || {
                response_fail.send(Err(String::default()));
            },
        );
    }
}

type SpawnResponseChannel = Option<tokio::sync::oneshot::Sender<Result<SpawnResponse, String>>>;

#[allow(clippy::type_complexity)]
fn spawn_portable(
    mut portables: ResMut<PortableScenes>,
    mut events: EventReader<RestrictedAction>,
    mut pending_lookups: Local<
        Vec<(
            Task<Result<(String, PortableSource), String>>,
            SpawnResponseChannel,
        )>,
    >,
    mut pending_responses: Local<HashMap<String, SpawnResponseChannel>>,
    live_scenes: Res<LiveScenes>,
    scenes: Query<(Option<&RendererSceneContext>, Option<&SceneLoading>)>,
) {
    // process incoming events
    for (location, spawner, response) in events.iter().filter_map(|ev| match ev {
        RestrictedAction::SpawnPortable {
            location,
            spawner,
            response,
        } => Some((location, spawner, response)),
        _ => None,
    }) {
        match location {
            PortableLocation::Urn(urn) => {
                let hacked_urn = urn.replace('?', "?=&");

                let Ok(path) = IpfsPath::new_from_urn::<EntityDefinition>(&hacked_urn) else {
                    response.send(Err("failed to parse urn".to_owned()));
                    continue;
                };

                let Ok(Some(hash)) = path.context_free_hash() else {
                    response.send(Err("failed to resolve content hash from urn".to_owned()));
                    continue;
                };

                portables.0.insert(
                    hash.clone(),
                    PortableSource {
                        pid: hacked_urn,
                        parent_scene: spawner.clone(),
                        ens: None,
                    },
                );
                pending_responses.insert(hash, Some(response.take()));
            }
            PortableLocation::Ens(ens) => {
                let spawner = spawner.clone();
                let ens = ens.clone();
                pending_lookups.push((
                    IoTaskPool::get().spawn(async move {
                        let mut about = isahc::get_async(format!(
                            "https://worlds-content-server.decentraland.org/world/{ens}/about"
                        ))
                        .await
                        .map_err(|e| e.to_string())?;
                        if about.status() != StatusCode::OK {
                            return Err(format!("status: {}", about.status()));
                        }

                        let about = about
                            .json::<ServerAbout>()
                            .await
                            .map_err(|e| e.to_string())?;
                        let Some(config) = about.configurations else {
                            return Err("No configurations on server/about".to_owned());
                        };
                        let Some(scenes) = config.scenes_urn else {
                            return Err("No scenesUrn on server/about/configurations".to_owned());
                        };
                        let Some(urn) = scenes.get(0) else {
                            return Err("Empty scenesUrn on server/about/configurations".to_owned());
                        };
                        let hacked_urn = urn.replace('?', "?=&");

                        let Ok(path) = IpfsPath::new_from_urn::<EntityDefinition>(&hacked_urn)
                        else {
                            return Err("failed to parse urn".to_owned());
                        };

                        let Ok(Some(hash)) = path.context_free_hash() else {
                            return Err("failed to resolve content hash from urn".to_owned());
                        };

                        Ok((
                            hash,
                            PortableSource {
                                pid: hacked_urn,
                                parent_scene: spawner.clone(),
                                ens: Some(ens),
                            },
                        ))
                    }),
                    Some(response.take()),
                ));
            }
        }
    }

    // process pending lookups
    pending_lookups.retain_mut(|(ref mut task, ref mut response)| {
        if let Some(result) = task.complete() {
            match result {
                Ok((hash, source)) => {
                    portables.0.insert(hash.clone(), source);
                    pending_responses.insert(hash, response.take());
                }
                Err(e) => {
                    let _ = response
                        .take()
                        .unwrap()
                        .send(Err(format!("failed to lookup ens: {e}")));
                }
            }
            false
        } else {
            true
        }
    });

    pending_responses.retain(|hash, sender| {
        let mut fail = |msg: String| -> bool {
            let _ = sender.take().unwrap().send(Err(msg));
            portables.0.remove(hash);
            false
        };

        let Some(ent) = live_scenes.0.get(hash) else {
            debug!("no scene yet");
            return true;
        };

        let Ok((maybe_context, maybe_loading)) = scenes.get(*ent) else {
            // with no context and no load state something went wrong
            return fail("failed to start loading".to_owned());
        };

        if matches!(maybe_loading, Some(SceneLoading::Failed)) {
            return fail("failed to load".to_owned());
        }

        if let Some(context) = maybe_context {
            if let Some(source) = portables.0.get(hash) {
                let _ = sender.take().unwrap().send(Ok(SpawnResponse {
                    pid: source.pid.clone(),
                    parent_cid: source.parent_scene.clone().unwrap_or_default(),
                    name: context.title.clone(),
                    ens: source.ens.clone(),
                }));
            } else {
                let _ = sender
                    .take()
                    .unwrap()
                    .send(Err("killed before load completed".to_owned()));
            }
            return false;
        }

        debug!("waiting for context, load state is {maybe_loading:?}");
        true
    });
}

fn kill_portable(mut portables: ResMut<PortableScenes>, mut events: EventReader<RestrictedAction>) {
    for (location, response) in events.iter().filter_map(|ev| match ev {
        RestrictedAction::KillPortable { location, response } => Some((location, response)),
        _ => None,
    }) {
        match location {
            PortableLocation::Urn(urn) => {
                let hacked_urn = urn.replace('?', "?=&");

                let Ok(path) = IpfsPath::new_from_urn::<EntityDefinition>(&hacked_urn) else {
                    response.send(false);
                    continue;
                };

                let Ok(Some(hash)) = path.context_free_hash() else {
                    response.send(false);
                    continue;
                };

                response.send(portables.0.remove(&hash).is_some());
            }
            _ => unimplemented!(),
        }
    }
}

fn list_portables(
    portables: ResMut<PortableScenes>,
    mut events: EventReader<RestrictedAction>,
    live_scenes: Res<LiveScenes>,
    contexts: Query<&RendererSceneContext>,
) {
    for response in events.iter().filter_map(|ev| match ev {
        RestrictedAction::ListPortables { response } => Some(response),
        _ => None,
    }) {
        println!("listing portables");
        let portables = portables
            .0
            .iter()
            .map(|(hash, source)| {
                let context = live_scenes
                    .0
                    .get(hash)
                    .and_then(|ent| contexts.get(*ent).ok());

                SpawnResponse {
                    pid: source.pid.clone(),
                    name: context.map_or(String::default(), |c| c.title.to_owned()),
                    parent_cid: source.parent_scene.clone().unwrap_or_default(),
                    ens: source.ens.clone(),
                }
            })
            .collect();
        response.send(portables);
    }
}

fn get_user_data(profile: Res<CurrentUserProfile>, mut events: EventReader<RestrictedAction>) {
    for response in events.iter().filter_map(|ev| match ev {
        RestrictedAction::GetUserData { response } => Some(response),
        _ => None,
    }) {
        response.send(profile.0.content.clone());
    }
}

fn op_get_connected_players(
    me: Res<Wallet>,
    others: Query<&ForeignPlayer>,
    mut events: EventReader<RestrictedAction>,
) {
    for response in events.iter().filter_map(|ev| match ev {
        RestrictedAction::GetConnectedPlayers { response } => Some(response),
        _ => None,
    }) {
        let results = others
            .iter()
            .map(|f| format!("{:#x}", f.address))
            .chain(Some(format!("{:#x}", me.address())))
            .collect();
        response.send(results);
    }
}
