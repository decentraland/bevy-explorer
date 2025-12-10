// --server https://worlds-content-server.decentraland.org/world/shibu.dcl.eth --location 1,1

#[cfg(all(feature = "livekit", not(target_arch = "wasm32")))]
pub mod native;
pub mod plugin;
mod room;
#[cfg(all(feature = "livekit", target_arch = "wasm32"))]
pub mod web;

use bevy::platform::sync::Arc;
use bevy::prelude::*;
use tokio::{runtime::Runtime, sync::mpsc::Receiver};

pub use crate::livekit::room::LivekitRoom;
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

#[derive(Clone, Component, Deref, DerefMut)]
struct LivekitRuntime(Arc<Runtime>);

#[derive(Component)]
pub struct LivekitConnection;
