use bevy::ecs::component::HookContext;
use bevy::prelude::*;
use bevy::{ecs::world::DeferredWorld, platform::collections::HashMap};
use common::util::AsH160;
use dcl_component::proto_components::kernel::comms::rfc4;
use http::Uri;
use prost::Message;
use tokio::sync::mpsc;
#[cfg(not(target_arch = "wasm32"))]
use tokio::sync::mpsc::UnboundedReceiver;
#[cfg(not(target_arch = "wasm32"))]
use {
    livekit::{id::TrackSid, track::TrackKind, Room, RoomEvent, RoomOptions, RoomResult},
    {bevy::platform::sync::Arc, tokio::task::JoinHandle},
};
#[cfg(target_arch = "wasm32")]
use {
    tokio::sync::oneshot,
    wasm_bindgen::{
        convert::{FromWasmAbi, IntoWasmAbi},
        JsValue,
    },
    wasm_bindgen_futures::spawn_local,
};

#[cfg(target_arch = "wasm32")]
use crate::livekit::web::{connect_room, recv_room_event, room_name, RoomEvent};
#[cfg(not(target_arch = "wasm32"))]
use crate::livekit::{kira_bridge::kira_thread, participant};
use crate::{
    global_crdt::{GlobalCrdtState, PlayerMessage, PlayerUpdate},
    livekit::{LivekitRuntime, LivekitTransport},
};

#[cfg(target_arch = "wasm32")]
type JsValueAbi = <JsValue as IntoWasmAbi>::Abi;

#[derive(Component)]
pub struct LivekitRoom {
    pub room_name: String,
    #[cfg(not(target_arch = "wasm32"))]
    pub room: Arc<Room>,
    #[cfg(target_arch = "wasm32")]
    pub room: JsValueAbi,
    #[cfg(not(target_arch = "wasm32"))]
    pub room_event_receiver: mpsc::UnboundedReceiver<RoomEvent>,
}

impl LivekitRoom {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn get_room(&self) -> Arc<Room> {
        self.room.clone()
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for LivekitRoom {
    fn drop(&mut self) {
        // Build the value to drop the Abi memory
        let _ = unsafe { JsValue::from_abi(self.room) };
    }
}

/// Marks that a [`LivekitRoom`] as connected
#[derive(Component)]
#[component(on_add=Self::on_add, on_remove=Self::on_remove)]
pub struct Connected;

impl Connected {
    pub fn on_add(mut deferred_world: DeferredWorld, hook_context: HookContext) {
        let entity = hook_context.entity;
        debug!("Room {entity} connected.");

        deferred_world
            .commands()
            .entity(entity)
            .remove::<Connecting>();
    }

    pub fn on_remove(mut deferred_world: DeferredWorld, hook_context: HookContext) {
        let entity = hook_context.entity;

        // This hook will also run on despawn
        // so `try_remove` is used
        deferred_world
            .commands()
            .entity(entity)
            .try_remove::<LivekitRoom>();
    }
}

/// Marks that a [`LivekitRoom`] as connecting or
/// attempting to reconnect
#[derive(Component)]
#[component(on_add=Self::on_add, on_remove=Self::on_remove)]
pub struct Connecting;

impl Connecting {
    pub fn on_add(mut deferred_world: DeferredWorld, hook_context: HookContext) {
        let entity = hook_context.entity;
        debug!("Room {entity} is connecting.");

        deferred_world
            .commands()
            .entity(entity)
            .remove::<Connected>();
    }

    pub fn on_remove(mut deferred_world: DeferredWorld, hook_context: HookContext) {
        let entity = hook_context.entity;

        // This hook will also run on despawn
        // so `try_remove` is used
        deferred_world
            .commands()
            .entity(entity)
            .try_remove::<ConnectingLivekitRoom>();
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default, Resource, Deref, DerefMut)]
struct LivekitRoomTrackTask(HashMap<TrackSid, JoinHandle<()>>);

#[derive(Component, Deref, DerefMut)]
struct ConnectingLivekitRoom(
    #[cfg(not(target_arch = "wasm32"))]
    JoinHandle<RoomResult<(Room, UnboundedReceiver<RoomEvent>)>>,
    #[cfg(target_arch = "wasm32")] oneshot::Receiver<anyhow::Result<JsValueAbi>>,
);

#[cfg(not(target_arch = "wasm32"))]
impl Drop for ConnectingLivekitRoom {
    fn drop(&mut self) {
        self.0.abort()
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for ConnectingLivekitRoom {
    fn drop(&mut self) {
        let (_, mut receiver) = oneshot::channel();
        std::mem::swap(&mut receiver, &mut self.0);
        if !receiver.is_terminated() {
            spawn_local(async move {
                if let Ok(Ok(js_value_abi)) = receiver.await {
                    let _ = unsafe { JsValue::from_abi(js_value_abi) };
                }
            })
        }
    }
}

pub struct LivekitRoomPlugin;

impl Plugin for LivekitRoomPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(not(target_arch = "wasm32"))]
        app.init_resource::<LivekitRoomTrackTask>();

        app.add_observer(initiate_room_connection);
        app.add_observer(connect_to_livekit_room);

        app.add_systems(Update, (poll_connecting_rooms, process_room_events).chain());
    }
}

fn initiate_room_connection(trigger: Trigger<OnAdd, LivekitTransport>, mut commands: Commands) {
    commands.entity(trigger.target()).insert(Connecting);
}

fn connect_to_livekit_room(
    trigger: Trigger<OnAdd, Connecting>,
    mut commands: Commands,
    livekit_transports: Query<(&LivekitTransport, &LivekitRuntime)>,
) {
    let entity = trigger.target();
    #[cfg_attr(
        target_arch = "wasm32",
        expect(unused_variables, reason = "Runtime is used only on native")
    )]
    let Ok((livekit_transport, livekit_runtime)) = livekit_transports.get(entity) else {
        error!("{entity} does not have a LivekitRuntime.");
        return;
    };

    let remote_address = &livekit_transport.address;
    debug!(">> lk connect async : {remote_address}");

    let url = Uri::try_from(remote_address).unwrap();
    let address = format!(
        "{}://{}{}",
        url.scheme_str().unwrap_or_default(),
        url.host().unwrap_or_default(),
        url.path()
    );
    let params: HashMap<_, _, bevy::platform::hash::FixedHasher> =
        HashMap::from_iter(url.query().unwrap_or_default().split('&').flat_map(|par| {
            par.split_once('=')
                .map(|(a, b)| (a.to_owned(), b.to_owned()))
        }));
    debug!("{params:?}");
    let token = params.get("access_token").cloned().unwrap_or_default();

    #[cfg(not(target_arch = "wasm32"))]
    commands.entity(entity).insert(ConnectingLivekitRoom(
        livekit_runtime.spawn(connect_to_room(address, token)),
    ));
    #[cfg(target_arch = "wasm32")]
    {
        let (sender, receiver) = oneshot::channel();

        spawn_local(connect_to_room(address, token, sender));

        commands
            .entity(entity)
            .insert(ConnectingLivekitRoom(receiver));
    }
}

fn poll_connecting_rooms(
    mut commands: Commands,
    livekit_rooms: Populated<(Entity, &LivekitRuntime, &mut ConnectingLivekitRoom)>,
) {
    for (entity, livekit_runtime, mut connecting_livekit_room) in livekit_rooms.into_inner() {
        #[cfg(not(target_arch = "wasm32"))]
        let finished = connecting_livekit_room.is_finished();
        #[cfg(target_arch = "wasm32")]
        let finished = !connecting_livekit_room.is_empty();

        if finished {
            let Ok(poll) =
                livekit_runtime.block_on(connecting_livekit_room.as_deref_mut().as_mut())
            else {
                error!("Failed to poll ConnectingLivekitRoom.");
                continue;
            };

            match poll {
                #[cfg(not(target_arch = "wasm32"))]
                Ok((room, room_event_receiver)) => {
                    let local_participant = room.local_participant();
                    commands.trigger(participant::ParticipantConnected {
                        participant: local_participant.into(),
                        room: entity,
                    });

                    commands
                        .entity(entity)
                        .insert((
                            LivekitRoom {
                                room_name: room.name(),
                                room: Arc::new(room),
                                room_event_receiver,
                            },
                            Connected,
                        ))
                        .remove::<ConnectingLivekitRoom>();
                }
                #[cfg(target_arch = "wasm32")]
                Ok(room) => {
                    let js_room = unsafe { JsValue::from_abi(room) };
                    let room_name = room_name(&js_room);
                    // This prevents the memory for the object from being freed
                    let _ = js_room.into_abi();
                    commands
                        .entity(entity)
                        .insert((LivekitRoom { room_name, room }, Connected))
                        .remove::<ConnectingLivekitRoom>();
                }
                Err(err) => {
                    error!("Failed to connect to room due to '{err}'.");
                    commands.entity(entity).remove::<ConnectingLivekitRoom>();
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn connect_to_room(
    address: String,
    token: String,
) -> RoomResult<(Room, UnboundedReceiver<RoomEvent>)> {
    livekit::prelude::Room::connect(
        &address,
        &token,
        RoomOptions {
            auto_subscribe: false,
            adaptive_stream: false,
            dynacast: false,
            ..Default::default()
        },
    )
    .await
}

#[cfg(target_arch = "wasm32")]
async fn connect_to_room(
    address: String,
    token: String,
    sender: oneshot::Sender<anyhow::Result<JsValueAbi>>,
) {
    let res = connect_room(&address, &token)
        .await
        .map(IntoWasmAbi::into_abi)
        .map_err(|e| anyhow::anyhow!("Failed to connect room: {:?}", e));

    sender.send(res).unwrap();
}

fn process_room_events(
    mut commands: Commands,
    livekit_rooms: Query<(Entity, &mut LivekitRoom)>,
    #[cfg(not(target_arch = "wasm32"))] livekit_runtimes: Query<&LivekitRuntime>,
    player_state: Res<GlobalCrdtState>,
    #[cfg(not(target_arch = "wasm32"))] mut track_tasks: ResMut<LivekitRoomTrackTask>,
) {
    let sender = player_state.get_sender();
    #[cfg_attr(target_arch = "wasm32", expect(unused_mut))]
    for (entity, mut livekit_room) in livekit_rooms {
        #[cfg(not(target_arch = "wasm32"))]
        let Ok(runtime) = livekit_runtimes.get(entity) else {
            error!("LivekitRoom {entity} does not have a runtime.");
            continue;
        };

        #[cfg(not(target_arch = "wasm32"))]
        let mut puller = || livekit_room.room_event_receiver.try_recv();
        #[cfg(target_arch = "wasm32")]
        let puller = || {
            let room = unsafe { JsValue::from_abi(livekit_room.room) };
            let head = recv_room_event(&room);
            let _ = room.into_abi();
            head.ok_or(mpsc::error::TryRecvError::Empty)
        };

        while let Ok(room_event) = puller() {
            trace!("in: {:?}", room_event);

            match room_event {
                #[cfg(not(target_arch = "wasm32"))]
                RoomEvent::Connected {
                    participants_with_tracks,
                } => {
                    for (participant, publications) in participants_with_tracks {
                        commands.trigger(participant::ParticipantConnected {
                            participant: participant.clone().into(),
                            room: entity,
                        });
                        if let Some(address) = participant.identity().0.as_str().as_h160() {
                            for publication in publications {
                                debug!("initial pub: {publication:?}");
                                if matches!(publication.kind(), TrackKind::Audio) {
                                    let _ = sender
                                        .try_send(PlayerUpdate {
                                            transport_id: entity,
                                            message: PlayerMessage::AudioStreamAvailable {
                                                transport: entity,
                                            },
                                            address,
                                        })
                                        .inspect_err(|err| {
                                            error!("Failed to send player update due to '{err}'")
                                        });
                                }
                            }
                        } else if participant.identity().as_str().ends_with("-streamer") {
                            for publication in publications {
                                publication.set_subscribed(true);
                            }
                        }
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                RoomEvent::DataReceived {
                    payload,
                    participant,
                    ..
                } => {
                    if let Some(participant) = participant {
                        if let Some(address) = participant.identity().0.as_str().as_h160() {
                            let packet = match rfc4::Packet::decode(payload.as_slice()) {
                                Ok(packet) => packet,
                                Err(e) => {
                                    warn!("unable to parse packet body: {e}");
                                    continue;
                                }
                            };
                            let Some(message) = packet.message else {
                                warn!("received empty packet body");
                                continue;
                            };
                            trace!(
                                "[{}] received [{}] packet {message:?} from {address}",
                                entity,
                                packet.protocol_version
                            );
                            if let Err(e) = sender.try_send(PlayerUpdate {
                                transport_id: entity,
                                message: PlayerMessage::PlayerData(message),
                                address,
                            }) {
                                warn!("app pipe broken ({e}), existing loop");
                                break;
                            }
                        }
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                RoomEvent::TrackPublished {
                    publication,
                    participant,
                } => {
                    debug!("pub: {publication:?}");
                    if let Some(address) = participant.identity().0.as_str().as_h160() {
                        // publication.
                        if matches!(publication.kind(), TrackKind::Audio) {
                            let _ = sender.try_send(PlayerUpdate {
                                transport_id: entity,
                                message: PlayerMessage::AudioStreamAvailable { transport: entity },
                                address,
                            });
                        }
                    } else if participant.identity().as_str().ends_with("-streamer") {
                        publication.set_subscribed(true);
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                RoomEvent::TrackUnpublished {
                    publication,
                    participant,
                } => {
                    debug!("unpub: {publication:?}");
                    if let Some(address) = participant.identity().0.as_str().as_h160() {
                        if matches!(publication.kind(), TrackKind::Audio) {
                            let _ = sender.try_send(PlayerUpdate {
                                transport_id: entity,
                                message: PlayerMessage::AudioStreamUnavailable {
                                    transport: entity,
                                },
                                address,
                            });
                        }
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                RoomEvent::TrackSubscribed {
                    track,
                    publication,
                    participant,
                } => {
                    // if let Some(address) = participant.identity().0.as_str().as_h160() {
                    //     let sid = track.sid();
                    //     match track {
                    //         livekit::track::RemoteTrack::Audio(audio) => {
                    //             let Some(channel) = audio_channels.remove(&address) else {
                    //                 warn!("no channel for subscribed audio");
                    //                 publication.set_subscribed(false);
                    //                 continue;
                    //             };
                    //             let handle =
                    //                 runtime.spawn(kira_thread(audio, publication, channel));
                    //             track_tasks.insert(sid, handle);
                    //         }
                    //         _ => warn!("not processing video tracks"),
                    //     }
                    // }
                }
                #[cfg(not(target_arch = "wasm32"))]
                RoomEvent::TrackUnsubscribed { track, .. } => {
                    if let Some(handle) = track_tasks.remove(&track.sid()) {
                        handle.abort();
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                RoomEvent::ParticipantConnected(participant) => {
                    commands.trigger(participant::ParticipantConnected {
                        participant: participant.clone().into(),
                        room: entity,
                    });
                    let meta = participant.metadata();
                    if !meta.is_empty() {
                        if let Some(address) = participant.identity().0.as_str().as_h160() {
                            if let Err(e) = sender.try_send(PlayerUpdate {
                                transport_id: entity,
                                message: PlayerMessage::MetaData(meta),
                                address,
                            }) {
                                warn!("app pipe broken ({e}), existing loop");
                                break;
                            }
                        }
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                RoomEvent::ParticipantDisconnected(participant) => {
                    commands.trigger(participant::ParticipantDisconnected {
                        participant: participant.into(),
                        room: entity,
                    });
                }
                #[cfg(target_arch = "wasm32")]
                RoomEvent::DataReceived {
                    payload,
                    participant,
                    ..
                } => {
                    if let Some(address) = participant.identity.as_h160() {
                        if let Ok(packet) = rfc4::Packet::decode(payload.as_slice()) {
                            if let Some(message) = packet.message {
                                let _ = sender
                                    .try_send(PlayerUpdate {
                                        transport_id: entity,
                                        message: PlayerMessage::PlayerData(message),
                                        address,
                                    })
                                    .inspect_err(|err| {
                                        error!("Failed to send player update due to '{err}'")
                                    });
                            }
                        }
                    }
                }
                #[cfg(target_arch = "wasm32")]
                RoomEvent::TrackPublished {
                    participant, kind, ..
                } => {
                    debug!("pub {} {}", participant.identity, kind);
                    if let Some(address) = participant.identity.as_h160() {
                        if kind == "audio" {
                            let _ = sender
                                .try_send(PlayerUpdate {
                                    transport_id: entity,
                                    message: PlayerMessage::AudioStreamAvailable {
                                        transport: entity,
                                    },
                                    address,
                                })
                                .inspect_err(|err| {
                                    error!("Failed to send player update due to '{err}'")
                                });
                        }
                    }
                }
                #[cfg(target_arch = "wasm32")]
                RoomEvent::TrackUnpublished {
                    participant, kind, ..
                } => {
                    debug!("unpub {} {}", participant.identity, kind);
                    if let Some(address) = participant.identity.as_h160() {
                        if kind == "audio" {
                            let _ = sender
                                .try_send(PlayerUpdate {
                                    transport_id: entity,
                                    message: PlayerMessage::AudioStreamUnavailable {
                                        transport: entity,
                                    },
                                    address,
                                })
                                .inspect_err(|err| {
                                    error!("Failed to send player update due to '{err}'")
                                });
                        }
                    }
                }
                #[cfg(target_arch = "wasm32")]
                RoomEvent::TrackSubscribed { .. } => {
                    debug!("Track subscribed event - audio is handled in JavaScript");
                }
                #[cfg(target_arch = "wasm32")]
                RoomEvent::TrackUnsubscribed { .. } => {
                    debug!("Track unsubscribed event");
                }
                #[cfg(target_arch = "wasm32")]
                RoomEvent::ParticipantConnected { participant, .. } => {
                    if let Some(address) = participant.identity.as_h160() {
                        if !participant.metadata.is_empty() {
                            let _ = sender
                                .try_send(PlayerUpdate {
                                    transport_id: entity,
                                    message: PlayerMessage::MetaData(participant.metadata),
                                    address,
                                })
                                .inspect_err(|err| {
                                    error!("Failed to send player update due to '{err}'")
                                });
                        }
                    }
                }
                #[cfg(target_arch = "wasm32")]
                RoomEvent::ParticipantDisconnected { .. } => {
                    debug!("Participant disconnected");
                }
                #[cfg(not(target_arch = "wasm32"))]
                _ => {
                    debug!("Event: {:?}", room_event);
                }
            };
        }
    }
}
