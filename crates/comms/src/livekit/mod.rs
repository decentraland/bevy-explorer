// --server https://worlds-content-server.decentraland.org/world/shibu.dcl.eth --location 1,1

#[cfg(not(target_arch = "wasm32"))]
mod kira_bridge;
#[cfg(not(target_arch = "wasm32"))]
pub mod livekit_video_bridge;
mod mic;
pub mod participant;
pub mod plugin;
pub mod room;
pub mod track;
#[cfg(all(feature = "livekit", target_arch = "wasm32"))]
pub mod web;

use bevy::platform::sync::Arc;
use bevy::prelude::*;
use tokio::{runtime::Runtime, sync::mpsc};

use crate::{ChannelControl, NetworkMessage};

#[derive(Event)]
pub struct StartLivekit {
    pub entity: Entity,
    pub address: String,
}

#[derive(Component)]
pub struct LivekitTransport {
    pub address: String,
    pub retries: usize,
}

#[derive(Component, Deref, DerefMut)]
pub struct LivekitChannelControl {
    receiver: mpsc::Receiver<ChannelControl>,
}

#[derive(Component, Deref, DerefMut)]
pub struct LivekitNetworkMessage {
    receiver: mpsc::Receiver<NetworkMessage>,
}

#[derive(Clone, Component, Deref, DerefMut)]
pub struct LivekitRuntime(Arc<Runtime>);

#[derive(Component)]
pub struct LivekitConnection;

#[macro_export]
macro_rules! make_hooks {
    ($inserted:ty, ($($to_remove:ty),+)) => {
        impl $inserted {
            fn on_add(mut deferred_world: DeferredWorld, hook_context: HookContext) {
                let entity = hook_context.entity;

                deferred_world.commands().entity(entity).try_remove::<($($to_remove),+)>();
            }
        }
    };
}
