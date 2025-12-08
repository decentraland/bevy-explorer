// --server https://worlds-content-server.decentraland.org/world/shibu.dcl.eth --location 1,1

#[cfg(all(feature = "livekit", not(target_arch = "wasm32")))]
pub mod native;
pub mod plugin;
#[cfg(all(feature = "livekit", target_arch = "wasm32"))]
pub mod web;

#[cfg(not(target_arch = "wasm32"))]
use bevy::platform::sync::Arc;

use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::Runtime;
use tokio::sync::mpsc::Receiver;

use crate::{ChannelControl, NetworkMessage};

#[derive(Event)]
pub struct StartLivekit {
    pub entity: Entity,
    pub address: String,
}

#[derive(Component)]
pub struct LivekitTransport {
    pub address: String,
    pub receiver: Option<Receiver<NetworkMessage>>,
    pub control_receiver: Option<Receiver<ChannelControl>>,
    pub retries: usize,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Deref, DerefMut)]
struct LivekitRuntime(Arc<Runtime>);

#[derive(Component)]
pub struct LivekitConnection;
