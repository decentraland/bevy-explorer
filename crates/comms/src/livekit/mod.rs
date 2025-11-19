#[cfg(not(target_arch = "wasm32"))]
pub mod native;
#[cfg(target_arch = "wasm32")]
pub mod web;

// --server https://worlds-content-server.decentraland.org/world/shibu.dcl.eth --location 1,1

use bevy::prelude::*;
use tokio::sync::mpsc::Receiver;

use dcl_component::proto_components::kernel::comms::rfc4;

use crate::{
    profile::CurrentUserProfile, ChannelControl, NetworkMessage, Transport, TransportType,
};
use common::structs::MicState;

// main.rs or lib.rs

pub struct LivekitPlugin;

impl Plugin for LivekitPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(not(target_arch = "wasm32"))]
        app.add_plugins(native::NativeLivekitPlugin);
        #[cfg(target_arch = "wasm32")]
        app.add_plugins(web::WebLivekitPlugin);

        app.add_systems(Update, start_livekit);
        app.add_event::<StartLivekit>();
        app.init_resource::<MicState>();
    }
}

#[derive(Event)]
pub struct StartLivekit {
    pub entity: Entity,
    pub address: String,
}

#[derive(Component)]
pub struct LivekitTransport {
    address: String,
    receiver: Option<Receiver<NetworkMessage>>,
    control_receiver: Option<Receiver<ChannelControl>>,
    #[expect(dead_code, reason = "Will be used eventually")]
    retries: usize,
}

#[derive(Component)]
pub struct LivekitConnection;

fn start_livekit(
    mut commands: Commands,
    mut room_events: EventReader<StartLivekit>,
    current_profile: Res<CurrentUserProfile>,
) {
    for ev in room_events.read() {
        info!("starting livekit protocol");
        let (sender, receiver) = tokio::sync::mpsc::channel(1000);
        let (control_sender, control_receiver) = tokio::sync::mpsc::channel(10);

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
                control: Some(control_sender),
                foreign_aliases: Default::default(),
            },
            LivekitTransport::new(ev.address.to_owned(), receiver, control_receiver),
        ));
    }
}
