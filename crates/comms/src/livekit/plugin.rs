use bevy::platform::sync::Arc;
use bevy::prelude::*;
use dcl_component::proto_components::kernel::comms::rfc4;
use tokio::runtime::Builder;

#[cfg(not(target_arch = "wasm32"))]
use crate::livekit::native::connect_livekit;
#[cfg(target_arch = "wasm32")]
use crate::livekit::web::connect_livekit;
use crate::{
    livekit::{
        mic::MicPlugin, room::LivekitRoomPlugin, LivekitRuntime, LivekitTransport, StartLivekit,
    },
    profile::CurrentUserProfile,
    NetworkMessage, Transport, TransportType,
};

pub struct LivekitPlugin;

impl Plugin for LivekitPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MicPlugin);
        app.add_plugins(LivekitRoomPlugin);

        app.add_systems(Update, (connect_livekit, start_livekit));
        app.add_event::<StartLivekit>();
    }
}

pub fn start_livekit(
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

        #[cfg(not(target_arch = "wasm32"))]
        let runtime = Arc::new(
            Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap(),
        );
        #[cfg(target_arch = "wasm32")]
        let runtime = Arc::new(
            Builder::new_current_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap(),
        );

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
            LivekitRuntime(runtime),
        ));
    }
}
