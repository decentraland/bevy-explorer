pub(super) mod plugin;

use bevy::{
    ecs::{component::HookContext, world::DeferredWorld},
    platform::collections::HashMap,
    prelude::*,
};
use tokio::sync::mpsc;
#[cfg(not(target_arch = "wasm32"))]
use {
    bevy::platform::sync::Arc,
    livekit::{id::TrackSid, Room, RoomEvent, RoomResult},
    tokio::{sync::mpsc::UnboundedReceiver, task::JoinHandle},
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
use crate::livekit::web::{close_room, connect_room, recv_room_event, room_name, RoomEvent};

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
        let room = unsafe { JsValue::from_abi(self.room) };
        spawn_local(async move {
            let _room = room;
            // Just a bit of delay so that the call `close_room`
            // has time to finish
            futures_lite::future::yield_now().await;
        });
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
