pub(super) mod plugin;

use bevy::{
    ecs::{component::HookContext, world::DeferredWorld},
    platform::{collections::HashMap, sync::Arc},
    prelude::*,
};
#[cfg(not(target_arch = "wasm32"))]
use livekit::{id::TrackSid, Room, RoomEvent, RoomResult};
use tokio::{sync::mpsc, task::JoinHandle};

#[cfg(target_arch = "wasm32")]
use crate::livekit::web::{Room, RoomEvent, RoomResult, TrackSid};

#[derive(Component, Deref)]
pub struct LivekitRoom {
    #[deref]
    room: Arc<Room>,
    room_event_receiver: mpsc::UnboundedReceiver<RoomEvent>,
}

/// Marks that a [`LivekitRoom`] as connected
#[derive(Component)]
#[component(on_add=Self::on_add, on_remove=Self::on_remove)]
pub struct Connected;

impl Connected {
    pub fn on_add(mut deferred_world: DeferredWorld, hook_context: HookContext) {
        let entity = hook_context.entity;
        let Some(room) = deferred_world.entity(entity).get::<LivekitRoom>() else {
            error!("Connected room {entity} did not have LivekitRoom.");
            deferred_world.commands().send_event(AppExit::from_code(1));
            return;
        };
        debug!("Room {} connected.", room.name());

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

#[derive(Default, Resource, Deref, DerefMut)]
struct LivekitRoomTrackTask(HashMap<TrackSid, JoinHandle<()>>);

#[derive(Component, Deref, DerefMut)]
struct ConnectingLivekitRoom(JoinHandle<RoomResult<(Room, mpsc::UnboundedReceiver<RoomEvent>)>>);

impl Drop for ConnectingLivekitRoom {
    fn drop(&mut self) {
        self.0.abort()
    }
}
