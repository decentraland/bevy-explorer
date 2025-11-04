// --server https://worlds-content-server.decentraland.org/world/shibu.dcl.eth --location 1,1

use bevy::prelude::*;
use tokio::sync::mpsc::Receiver;

use dcl_component::proto_components::kernel::comms::rfc4;

use common::structs::MicState;
use crate::{
    profile::CurrentUserProfile, NetworkMessage, Transport, TransportType,
};

// main.rs or lib.rs

#[cfg(target_arch = "wasm32")]
pub use crate::livekit_web::{connect_livekit, MicPlugin};

#[cfg(not(target_arch = "wasm32"))]
pub use crate::livekit_native::{connect_livekit, MicPlugin};

pub struct LivekitPlugin;

impl Plugin for LivekitPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (connect_livekit, start_livekit));
        app.add_event::<StartLivekit>();
        app.init_resource::<MicState>();
        app.add_plugins(MicPlugin);
    }
}

#[derive(Event)]
pub struct StartLivekit {
    pub entity: Entity,
    pub address: String,
}

#[derive(Component)]
pub struct LivekitTransport {
    pub address: String,
    pub receiver: Option<Receiver<NetworkMessage>>,
    pub retries: usize,
}

#[derive(Component)]
pub struct LivekitConnection;

pub fn start_livekit(
    mut commands: Commands,
    mut room_events: EventReader<StartLivekit>,
    current_profile: Res<CurrentUserProfile>,
) {
    if let Some(ev) = room_events.read().last() {
        info!("starting livekit protocol");
        let (sender, receiver) = tokio::sync::mpsc::channel(1000);

        let Some(current_profile) = current_profile.profile.as_ref() else {
            return;
        };

        // queue a profile version message
        let response = rfc4::Packet {
            message: Some(rfc4::packet::Message::ProfileVersion(
                rfc4::AnnounceProfileVersion {
                    profile_version: current_profile.version,
                },
            )),
            protocol_version: 100,
        };
        let _ = sender.try_send(NetworkMessage::reliable(&response));

        commands.entity(ev.entity).try_insert((
            Transport {
                transport_type: TransportType::Livekit,
                sender,
                foreign_aliases: Default::default(),
            },
            LivekitTransport {
                address: ev.address.to_owned(),
                receiver: Some(receiver),
                retries: 0,
            },
        ));
    }
}
