pub mod teleport;

use std::path::{Path, PathBuf};

use avatar::AvatarDynamicState;
use bevy::{
    asset::LoadState,
    math::Vec3Swizzles,
    prelude::*,
    tasks::{IoTaskPool, Task},
    utils::{HashMap, HashSet},
};
use bevy_dui::{DuiCommandsExt, DuiProps, DuiRegistry};
use common::{
    profile::SerializedProfile,
    rpc::{PortableLocation, RpcCall, RpcEventSender, RpcResultSender, SpawnResponse},
    sets::SceneSets,
    structs::{PrimaryCamera, PrimaryUser},
    util::TaskExt,
};
use comms::{
    global_crdt::ForeignPlayer,
    profile::{CurrentUserProfile, UserProfile},
    NetworkMessage, Transport,
};
use dcl_component::proto_components::kernel::comms::rfc4;
use ethers_core::types::Address;
use ipfs::{ipfs_path::IpfsPath, ChangeRealmEvent, EntityDefinition, ServerAbout};
use isahc::{http::StatusCode, AsyncReadResponseExt};
use nft::asset_source::Nft;
use scene_runner::{
    initialize_scene::{
        LiveScenes, PortableScenes, PortableSource, SceneHash, SceneLoading, PARCEL_SIZE,
    },
    renderer_context::RendererSceneContext,
    update_world::gltf_container::{GltfDefinition, GltfProcessed},
    ContainingScene, SceneEntity,
};
use serde_json::json;
use teleport::{handle_out_of_world, teleport_player};
use ui_core::button::DuiButton;
use wallet::{browser_auth::remote_send_async, Wallet};

pub struct RestrictedActionsPlugin;

impl Plugin for RestrictedActionsPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<RpcCall>();
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
                get_connected_players,
                get_players_in_scene,
                event_player_connected,
                event_player_disconnected,
                event_player_moved_scene,
                event_scene_ready,
                send_scene_messages,
                teleport_player,
                handle_out_of_world,
                open_nft_dialog,
                show_nft_dialog,
                handle_eth_async,
            )
                .in_set(SceneSets::PostLoop),
        );
    }
}

fn move_player(
    mut events: EventReader<RpcCall>,
    scenes: Query<&RendererSceneContext>,
    mut player: Query<(Entity, &mut Transform, &mut AvatarDynamicState), With<PrimaryUser>>,
    containing_scene: ContainingScene,
) {
    for (root, transform) in events.read().filter_map(|ev| match ev {
        RpcCall::MovePlayer { scene, to } => Some((scene, to)),
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

        let target_scenes = containing_scene.get_position(target_transform.translation);
        if !target_scenes.contains(root) {
            warn!("move player request from {root:?} was outside scene bounds");
        } else {
            let (_, mut player_transform, mut dynamics) = player.single_mut();
            dynamics.velocity =
                transform.rotation * player_transform.rotation.inverse() * dynamics.velocity;

            *player_transform = target_transform;
            debug!("player transform to {:?}", target_transform);
        }
    }
}

fn move_camera(
    mut events: EventReader<RpcCall>,
    mut camera: Query<&mut PrimaryCamera>,
    player: Query<Entity, With<PrimaryUser>>,
    containing_scene: ContainingScene,
) {
    for (root, rotation) in events.read().filter_map(|ev| match ev {
        RpcCall::MoveCamera { scene, to } => Some((scene, to)),
        _ => None,
    }) {
        if !player
            .get_single()
            .ok()
            .map_or(false, |e| containing_scene.get(e).contains(root))
        {
            warn!("invalid camera move request from non-containing scene");
            warn!("request from {root:?}");
            warn!(
                "containing scenes {:?}",
                player.get_single().map(|p| containing_scene.get(p))
            );
            return;
        }

        let (yaw, pitch, roll) = rotation.to_euler(EulerRot::YXZ);

        let mut camera = camera.single_mut();
        camera.yaw = yaw;
        camera.pitch = pitch;
        camera.roll = roll;
    }
}

fn change_realm(
    mut commands: Commands,
    mut events: EventReader<RpcCall>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
    dui: Res<DuiRegistry>,
) {
    for (scene, to, message, response) in events.read().filter_map(|ev| match ev {
        RpcCall::ChangeRealm {
            scene,
            to,
            message,
            response,
        } => Some((scene, to, message, response.clone())),
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

        commands
            .spawn_template(
                &dui,
                "text-dialog",
                DuiProps::new()
                    .with_prop("title", "Change Realm".to_owned())
                    .with_prop(
                        "body",
                        format!(
                            "The scene wants to move you to a new realm\n`{}`\n{}",
                            to.clone(),
                            if let Some(message) = message {
                                message
                            } else {
                                ""
                            }
                        ),
                    )
                    .with_prop(
                        "buttons",
                        vec![
                            DuiButton::new_enabled_and_close(
                                "Let's go!",
                                move |mut writer: EventWriter<ChangeRealmEvent>| {
                                    writer.send(ChangeRealmEvent {
                                        new_realm: new_realm.clone(),
                                    });
                                    response_ok.send(Ok(()));
                                },
                            ),
                            DuiButton::new_enabled_and_close("No thanks", move || {
                                response_fail.send(Err(String::default()));
                            }),
                        ],
                    ),
            )
            .unwrap();
    }
}

fn external_url(
    mut commands: Commands,
    mut events: EventReader<RpcCall>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
    dui: Res<DuiRegistry>,
) {
    for (scene, url, response) in events.read().filter_map(|ev| match ev {
        RpcCall::ExternalUrl {
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

        commands
            .spawn_template(
                &dui,
                "text-dialog",
                DuiProps::new()
                    .with_prop("title", "Open External Link".to_owned())
                    .with_prop(
                        "body",
                        format!(
                            "The scene wants to display a link in an external application\n`{}`",
                            url.clone(),
                        ),
                    )
                    .with_prop(
                        "buttons",
                        vec![
                            DuiButton::new_enabled_and_close("Ok", move || {
                                let result =
                                    opener::open(Path::new(&url)).map_err(|e| e.to_string());
                                response_ok.send(result);
                            }),
                            DuiButton::new_enabled_and_close("Cancel", move || {
                                response_fail.send(Err(String::default()));
                            }),
                        ],
                    ),
            )
            .unwrap();
    }
}

type SpawnResponseChannel = Option<tokio::sync::oneshot::Sender<Result<SpawnResponse, String>>>;

#[allow(clippy::type_complexity)]
fn spawn_portable(
    mut portables: ResMut<PortableScenes>,
    mut events: EventReader<RpcCall>,
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
    for (location, spawner, response) in events.read().filter_map(|ev| match ev {
        RpcCall::SpawnPortable {
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
                        let Some(urn) = scenes.first() else {
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

fn kill_portable(mut portables: ResMut<PortableScenes>, mut events: EventReader<RpcCall>) {
    for (location, response) in events.read().filter_map(|ev| match ev {
        RpcCall::KillPortable { location, response } => Some((location, response)),
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
    mut events: EventReader<RpcCall>,
    live_scenes: Res<LiveScenes>,
    contexts: Query<&RendererSceneContext>,
) {
    for response in events.read().filter_map(|ev| match ev {
        RpcCall::ListPortables { response } => Some(response),
        _ => None,
    }) {
        debug!("listing portables");
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

#[allow(clippy::type_complexity)]
fn get_user_data(
    profile: Res<CurrentUserProfile>,
    others: Query<(&ForeignPlayer, &UserProfile)>,
    me: Res<Wallet>,
    mut events: EventReader<RpcCall>,
    mut pending_primary_requests: Local<
        Vec<(Entity, RpcResultSender<Result<SerializedProfile, ()>>)>,
    >,
    mut scenes: Query<&mut RendererSceneContext>,
) {
    for (user, scene, response) in events.read().filter_map(|ev| match ev {
        RpcCall::GetUserData {
            user,
            scene,
            response,
        } => Some((user, scene, response)),
        _ => None,
    }) {
        debug!("process get_user_data");
        match user {
            None => match profile.profile.as_ref() {
                Some(profile) => response.send(Ok(profile.content.clone())),
                None => {
                    if let Ok(mut ctx) = scenes.get_mut(*scene) {
                        // force scene to wait till user data is available
                        ctx.blocked.insert("get_user_data");
                    }
                    pending_primary_requests.push((*scene, response.clone()))
                }
            },
            Some(address) => {
                if let Some((_, profile)) = others
                    .iter()
                    .find(|(fp, _)| *address == format!("{:#x}", fp.address))
                {
                    response.send(Ok(profile.content.clone()));
                    return;
                }

                if let Some(my_address) = me.address() {
                    if *address == format!("{:#x}", my_address) {
                        match profile.profile.as_ref() {
                            Some(profile) => response.send(Ok(profile.content.clone())),
                            None => response.send(Err(())),
                        }
                        continue;
                    }
                }

                response.send(Err(()));
            }
        }
    }

    if !pending_primary_requests.is_empty() {
        if let Some(profile) = profile.profile.as_ref() {
            for (scene, sender) in pending_primary_requests.drain(..) {
                if let Ok(mut ctx) = scenes.get_mut(scene) {
                    ctx.blocked.remove("get_user_data");
                }
                sender.send(Ok(profile.content.clone()));
            }
        }
    }
}

fn get_connected_players(
    me: Res<Wallet>,
    others: Query<&ForeignPlayer>,
    mut events: EventReader<RpcCall>,
) {
    for response in events.read().filter_map(|ev| match ev {
        RpcCall::GetConnectedPlayers { response } => Some(response),
        _ => None,
    }) {
        let results = others
            .iter()
            .map(|f| format!("{:#x}", f.address))
            .chain(me.address().map(|address| format!("{:#x}", address)))
            .collect();
        response.send(results);
    }
}

fn get_players_in_scene(
    me: Query<Entity, With<PrimaryUser>>,
    wallet: Res<Wallet>,
    others: Query<(Entity, &ForeignPlayer)>,
    mut events: EventReader<RpcCall>,
    containing_scene: ContainingScene,
) {
    for (scene, response) in events.read().filter_map(|ev| match ev {
        RpcCall::GetPlayersInScene { scene, response } => Some((scene, response)),
        _ => None,
    }) {
        let mut results = Vec::default();
        if let Ok(player) = me.get_single() {
            if containing_scene.get(player).contains(scene) {
                if let Some(address) = wallet.address() {
                    results.push(format!("{:#x}", address));
                }
            }
        }

        results.extend(
            others
                .iter()
                .filter(|(e, _)| containing_scene.get(*e).contains(scene))
                .map(|(_, f)| format!("{:#x}", f.address)),
        );
        response.send(results);
    }
}

// todo: move this to global_crdt to do it all in one place?
fn event_player_connected(
    mut senders: Local<Vec<RpcEventSender>>,
    mut events: EventReader<RpcCall>,
    players: Query<&ForeignPlayer, Added<ForeignPlayer>>,
) {
    for sender in events.read().filter_map(|ev| match ev {
        RpcCall::SubscribePlayerConnected { sender } => Some(sender),
        _ => None,
    }) {
        senders.push(sender.clone());
    }

    senders.retain_mut(|sender| {
        for player in players.iter() {
            let data = json!({
                "userId": format!("{:#x}", player.address)
            })
            .to_string();

            let _ = sender.send(data);
        }
        !sender.is_closed()
    });
}

// todo: move this to global_crdt to do it all in one place?
fn event_player_disconnected(
    mut senders: Local<Vec<RpcEventSender>>,
    mut events: EventReader<RpcCall>,
    players: Query<(Entity, &ForeignPlayer), Added<ForeignPlayer>>,
    mut removed: RemovedComponents<ForeignPlayer>,
    mut last_players: Local<HashMap<Entity, Address>>,
) {
    // gather new receivers
    for sender in events.read().filter_map(|ev| match ev {
        RpcCall::SubscribePlayerDisconnected { sender } => Some(sender),
        _ => None,
    }) {
        senders.push(sender.clone());
    }

    // add new players to our local record
    for (ent, player) in players.iter() {
        last_players.insert(ent, player.address);
    }

    // gather addresses of removed players
    let removed = removed
        .read()
        .flat_map(|e| last_players.remove(&e))
        .collect::<Vec<_>>();

    senders.retain_mut(|sender| {
        for address in removed.iter() {
            let data = json!({
                "userId": format!("{:#x}", address)
            })
            .to_string();

            let _ = sender.send(data);
        }
        !sender.is_closed()
    });
}

#[allow(clippy::type_complexity)]
fn event_player_moved_scene(
    mut enter_senders: Local<HashMap<Entity, RpcEventSender>>,
    mut leave_senders: Local<HashMap<Entity, RpcEventSender>>,
    mut current_scene: Local<HashMap<Address, Entity>>,
    players: Query<(Entity, Option<&ForeignPlayer>), Or<(With<PrimaryUser>, With<ForeignPlayer>)>>,
    me: Res<Wallet>,
    containing_scene: ContainingScene,
    mut events: EventReader<RpcCall>,
) {
    // gather new receivers
    for (enter, scene, sender) in events.read().filter_map(|ev| match ev {
        RpcCall::SubscribePlayerEnteredScene { scene, sender } => Some((true, scene, sender)),
        RpcCall::SubscribePlayerLeftScene { scene, sender } => Some((false, scene, sender)),
        _ => None,
    }) {
        if enter {
            enter_senders.insert(*scene, sender.clone());
        } else {
            leave_senders.insert(*scene, sender.clone());
        }
    }

    // gather current scene
    let new_scene: HashMap<_, _> = players
        .iter()
        .flat_map(|(p, f)| {
            containing_scene.get_parcel(p).map(|parcel| {
                (
                    f.map(|f| f.address)
                        .unwrap_or(me.address().unwrap_or_default()),
                    parcel,
                )
            })
        })
        .collect();

    // gather diffs
    let mut left: HashMap<Entity, Vec<Address>> = HashMap::default();
    let mut entered: HashMap<Entity, Vec<Address>> = HashMap::default();

    for (address, scene) in current_scene.iter() {
        if new_scene.get(address) != Some(scene) {
            left.entry(*scene).or_default().push(*address);
        }
    }

    for (address, scene) in new_scene.iter() {
        if current_scene.get(address) != Some(scene) {
            entered.entry(*scene).or_default().push(*address);
        }
    }

    // send events
    for (mut senders, events) in [(leave_senders, left), (enter_senders, entered)] {
        senders.retain(|scene, sender| {
            if let Some(addresses) = events.get(scene) {
                for address in addresses {
                    let data = json!({
                        "userId": format!("{:#x}", address)
                    })
                    .to_string();

                    let _ = sender.send(data);
                }
            }
            !sender.is_closed()
        });
    }

    // update state
    *current_scene = new_scene;
}

// todo: move this to global_crdt to do it all in one place?
fn event_scene_ready(
    mut senders: Local<Vec<(Entity, RpcEventSender)>>,
    mut events: EventReader<RpcCall>,
    unready_gltfs: Query<&SceneEntity, (With<GltfDefinition>, Without<GltfProcessed>)>,
    mut previously_unready: Local<HashSet<Entity>>,
) {
    for (scene, sender) in events.read().filter_map(|ev| match ev {
        RpcCall::SubscribeSceneReady { scene, sender } => Some((scene, sender)),
        _ => None,
    }) {
        senders.push((*scene, sender.clone()));
        // add to the prev_unready set so that the event gets triggered even if
        // it is registered after all gltfs are loaded
        previously_unready.insert(*scene);
    }

    let mut now_unready = HashSet::default();

    for ent in &unready_gltfs {
        now_unready.insert(ent.root);
    }

    let now_ready = previously_unready
        .iter()
        .filter(|&s| !now_unready.contains(s))
        .collect::<HashSet<_>>();

    senders.retain_mut(|(scene, sender)| {
        if now_ready.contains(scene) {
            let _ = sender.send("{}".into());
        }

        !sender.is_closed()
    });

    drop(now_ready);
    *previously_unready = now_unready;
}

fn send_scene_messages(
    mut events: EventReader<RpcCall>,
    transports: Query<&Transport>,
    scenes: Query<&SceneHash>,
) {
    for (scene, data) in events.read().filter_map(|c| match c {
        RpcCall::SendMessageBus { scene, data } => Some((scene, data)),
        _ => None,
    }) {
        let Ok(hash) = scenes.get(*scene) else {
            continue;
        };

        debug!("messagebus sent from scene {}: {:?}", &hash.0, data);
        let message = rfc4::Packet {
            message: Some(rfc4::packet::Message::Scene(rfc4::Scene {
                scene_id: hash.0.clone(),
                data: data.clone(),
            })),
        };

        for transport in transports.iter() {
            let _ = transport
                .sender
                .try_send(NetworkMessage::reliable(&message));
        }
    }
}

fn open_nft_dialog(
    mut commands: Commands,
    mut events: EventReader<RpcCall>,
    containing_scene: ContainingScene,
    primary_user: Query<Entity, With<PrimaryUser>>,
    asset_server: Res<AssetServer>,
) {
    for (scene, urn, response) in events.read().filter_map(|c| match c {
        RpcCall::OpenNftDialog {
            scene,
            urn,
            response,
        } => Some((scene, urn, response)),
        _ => None,
    }) {
        let Ok(player) = primary_user.get_single() else {
            response.send(Err("No player".to_owned()));
            return;
        };

        if !containing_scene.get(player).contains(scene) {
            response.send(Err("Not in scene".to_owned()));
            return;
        }

        let h_nft = asset_server.load(format!("nft://{}.nft", urlencoding::encode(urn)));

        commands.spawn(NftDialogSpawn {
            h_nft,
            response: response.clone(),
        });
    }
}

#[derive(Component)]
pub struct NftDialogSpawn {
    h_nft: Handle<Nft>,
    response: RpcResultSender<Result<(), String>>,
}

fn show_nft_dialog(
    mut commands: Commands,
    q: Query<(Entity, &NftDialogSpawn)>,
    nfts: Res<Assets<Nft>>,
    asset_server: Res<AssetServer>,
    dui: Res<DuiRegistry>,
) {
    for (ent, nft_spawn) in q.iter() {
        if let Some(nft) = nfts.get(nft_spawn.h_nft.id()) {
            commands.entity(ent).remove::<NftDialogSpawn>();

            let url = &nft.image_url;
            let ipfs_path = IpfsPath::new_from_url(url, "image");
            let h_image = asset_server.load::<Image>(PathBuf::from(&ipfs_path));

            let creator = nft.creator.clone().unwrap_or("unknown".to_owned());

            let mut description = nft.description.clone().unwrap_or("???".to_owned());
            if description.len() > 500 {
                description = description
                    .chars()
                    .take(500)
                    .chain(std::iter::repeat('.').take(3))
                    .collect();
            }

            let link = nft.permalink.clone();

            commands
                .spawn_template(
                    &dui,
                    "nft-dialog",
                    DuiProps::new()
                        .with_prop(
                            "title",
                            nft.name.clone().unwrap_or("Unnamed Nft".to_owned()),
                        )
                        .with_prop("image", h_image)
                        .with_prop("creator", creator)
                        .with_prop("description", description)
                        .with_prop(
                            "buttons",
                            vec![
                                DuiButton::new("View on OpenSea.io", link.is_some(), move || {
                                    let _ = opener::open(link.as_ref().unwrap());
                                }),
                                DuiButton::close("Close"),
                            ],
                        ),
                )
                .unwrap();

            nft_spawn.response.clone().send(Ok(()));
        } else if asset_server.load_state(nft_spawn.h_nft.id()) == LoadState::Failed {
            commands.entity(ent).remove::<NftDialogSpawn>();
            commands
                .spawn_template(
                    &dui,
                    "text-dialog",
                    DuiProps::new()
                        .with_prop("title", "Failed to load NFT")
                        .with_prop("body", "Failed to load NFT")
                        .with_prop("buttons", vec![DuiButton::close("Shame")]),
                )
                .unwrap();
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn handle_eth_async(
    mut events: EventReader<RpcCall>,
    containing_scene: ContainingScene,
    primary_user: Query<Entity, With<PrimaryUser>>,
    scenes: Query<&RendererSceneContext>,
    wallet: Res<Wallet>,
    time: Res<Time>,
    mut tasks: Local<
        Vec<(
            RpcResultSender<Result<serde_json::Value, String>>,
            Task<Result<serde_json::Value, anyhow::Error>>,
        )>,
    >,
) {
    for (body, scene, response) in events.read().filter_map(|ev| match ev {
        RpcCall::SendAsync {
            body,
            scene,
            response,
        } => Some((body, scene, response)),
        _ => None,
    }) {
        debug!("[{:?}] handle_eth_async {:?}", scene, body);

        if primary_user
            .get_single()
            .map_or(true, |player| !containing_scene.get(player).contains(scene))
        {
            response.send(Err("player not in scene.".to_owned()));
            continue;
        }

        let last_action_time = scenes
            .get(*scene)
            .ok()
            .and_then(|scene| scene.last_action_event)
            .unwrap_or_default();
        if last_action_time < time.elapsed_seconds() - 5.0 {
            response.send(Err(format!(
                "no recent user activity (last action {}, time {}).",
                last_action_time,
                time.elapsed_seconds()
            )));
            continue;
        }

        if wallet.is_guest() || wallet.address().is_none() {
            response.send(Err("wallet not connected".to_owned()));
            continue;
        }

        tasks.push((
            response.clone(),
            IoTaskPool::get().spawn(remote_send_async(body.clone(), Some(wallet.auth_chain()))),
        ));
    }

    tasks.retain_mut(|(response, task)| {
        if let Some(result) = task.complete() {
            response.send(result.map_err(|e| e.to_string()));
            false
        } else {
            true
        }
    })
}
