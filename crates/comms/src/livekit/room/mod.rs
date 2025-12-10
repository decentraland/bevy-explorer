use bevy::platform::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use bevy::platform::sync::Arc;
use bevy::prelude::*;
use http::Uri;
#[cfg(not(target_arch = "wasm32"))]
use livekit::{Room, RoomEvent, RoomOptions, RoomResult};
use tokio::{sync::mpsc, task::JoinHandle};
#[cfg(target_arch = "wasm32")]
use {
    common::util::AsH160,
    dcl_component::proto_components::kernel::comms::rfc4,
    prost::Message,
    tokio::sync::oneshot,
    wasm_bindgen::{
        closure::Closure,
        convert::{FromWasmAbi, IntoWasmAbi},
        JsValue,
    },
    wasm_bindgen_futures::spawn_local,
};

#[cfg(target_arch = "wasm32")]
use crate::livekit::web::{connect_room, room_name, RoomEvent};
use crate::livekit::{LivekitRuntime, LivekitTransport};

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

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Deref, DerefMut)]
struct ConnectingLivekitRoom(JoinHandle<RoomResult<Room>>);

#[cfg(target_arch = "wasm32")]
#[derive(Component, Deref, DerefMut)]
struct ConnectingLivekitRoom(oneshot::Receiver<anyhow::Result<JsValueAbi>>);

pub struct LivekitRoomPlugin;

impl Plugin for LivekitRoomPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(connect_to_room_on_transport_creation);

        app.add_systems(Update, poll_connecting_rooms);
    }
}

fn connect_to_room_on_transport_creation(
    trigger: Trigger<OnAdd, LivekitTransport>,
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
                Ok(poll_content) => {
                    let room_event_receiver = poll_content.subscribe();

                    commands
                        .entity(entity)
                        .insert(LivekitRoom {
                            room_name: poll_content.name(),
                            room: Arc::new(poll_content),
                            room_event_receiver,
                        })
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
                        .insert(LivekitRoom { room_name, room })
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
async fn connect_to_room(address: String, token: String) -> RoomResult<Room> {
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
    .map(|(room, _)| room)
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
