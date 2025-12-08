use bevy::prelude::*;
use common::structs::MicState;
use dcl_component::proto_components::kernel::comms::rfc4;

#[cfg(not(target_arch = "wasm32"))]
pub use crate::livekit::native::{connect_livekit, MicPlugin};
#[cfg(target_arch = "wasm32")]
pub use crate::livekit_web::{connect_livekit, MicPlugin};
use crate::{
    livekit::{LivekitTransport, StartLivekit},
    profile::CurrentUserProfile,
    NetworkMessage, Transport, TransportType,
};

pub struct LivekitPlugin;

impl Plugin for LivekitPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (connect_livekit, start_livekit));
        app.add_event::<StartLivekit>();
        app.init_resource::<MicState>();
        app.add_plugins(MicPlugin);
    }
}

pub fn start_livekit(
    mut commands: Commands,
    mut room_events: EventReader<StartLivekit>,
    current_profile: Res<CurrentUserProfile>,
) {
    if let Some(ev) = room_events.read().last() {
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
            LivekitTransport {
                address: ev.address.to_owned(),
                receiver: Some(receiver),
                control_receiver: Some(control_receiver),
                retries: 0,
            },
        ));
    }
}
