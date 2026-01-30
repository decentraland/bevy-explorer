pub(super) mod plugin;

use bevy::{
    ecs::{component::HookContext, world::DeferredWorld},
    platform::sync::Arc,
    prelude::*,
};
#[cfg(not(target_arch = "wasm32"))]
use livekit::{Room, RoomEvent, RoomResult};
use tokio::{sync::mpsc, task::JoinHandle};

#[cfg(target_arch = "wasm32")]
use crate::livekit::web::{Room, RoomEvent, RoomResult};
use crate::livekit::LivekitTransport;

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
            .remove::<(Connecting, Reconnecting, Disconnected)>();
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

/// Marks that a [`LivekitRoom`] is connecting
#[derive(Component)]
#[component(on_add=Self::on_add, on_remove=Self::on_remove)]
pub struct Connecting;

impl Connecting {
    pub fn on_add(mut deferred_world: DeferredWorld, hook_context: HookContext) {
        let entity = hook_context.entity;
        let Some(transport) = deferred_world.entity(entity).get::<LivekitTransport>() else {
            error!("Connecting room {entity} did not have LivekitTransport.");
            deferred_world.commands().send_event(AppExit::from_code(1));
            return;
        };
        debug!("Room {} connecting.", transport.address);

        deferred_world
            .commands()
            .entity(entity)
            .remove::<(Connected, Reconnecting, Disconnected)>();
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

/// Marks that a [`LivekitRoom`] as
/// attempting to reconnect
#[derive(Component)]
#[component(on_add=Self::on_add, on_remove=Self::on_remove)]
pub struct Reconnecting;

impl Reconnecting {
    pub fn on_add(mut deferred_world: DeferredWorld, hook_context: HookContext) {
        let entity = hook_context.entity;
        let Some(room) = deferred_world.entity(entity).get::<LivekitRoom>() else {
            error!("Reconnecting room {entity} did not have LivekitRoom.");
            deferred_world.commands().send_event(AppExit::from_code(1));
            return;
        };
        debug!("Room {} is reconnecting.", room.name());

        deferred_world
            .commands()
            .entity(entity)
            .remove::<(Connected, Connecting, Disconnected)>();
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

/// Marks that a [`LivekitRoom`] as
/// attempting to disconnected
#[derive(Component)]
#[component(on_add=Self::on_add, on_remove=Self::on_remove)]
pub struct Disconnected;

impl Disconnected {
    pub fn on_add(mut deferred_world: DeferredWorld, hook_context: HookContext) {
        let entity = hook_context.entity;
        let Some(room) = deferred_world.entity(entity).get::<LivekitRoom>() else {
            error!("Disconnected room {entity} did not have LivekitRoom.");
            deferred_world.commands().send_event(AppExit::from_code(1));
            return;
        };
        debug!("Room {} is disconnected.", room.name());

        deferred_world
            .commands()
            .entity(entity)
            .remove::<(Connected, Connecting, Reconnecting)>();
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

#[derive(Component, Deref, DerefMut)]
struct ConnectingLivekitRoom(JoinHandle<RoomResult<(Room, mpsc::UnboundedReceiver<RoomEvent>)>>);

impl Drop for ConnectingLivekitRoom {
    fn drop(&mut self) {
        self.0.abort()
    }
}
