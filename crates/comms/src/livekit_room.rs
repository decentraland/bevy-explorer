// --server https://worlds-content-server.decentraland.org/world/shibu.dcl.eth --location 1,1

use std::sync::Arc;

use bevy::prelude::*;
use tokio::sync::{
    mpsc::{Receiver, Sender},
    Mutex,
};

use dcl_component::proto_components::kernel::comms::rfc4;

use crate::{
    global_crdt::{LocalAudioFrame, LocalAudioSource, MicState},
    profile::CurrentUserProfile,
    Transport, TransportType,
};

use super::{
    global_crdt::{GlobalCrdtState, PlayerUpdate},
    NetworkMessage,
};

// main.rs or lib.rs

#[cfg(target_arch = "wasm32")]
pub use crate::livekit_web::livekit_handler_inner;

#[cfg(not(target_arch = "wasm32"))]
pub use crate::livekit_native::livekit_handler_inner;

pub struct LivekitPlugin;

impl Plugin for LivekitPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (connect_livekit, start_livekit));
        app.add_event::<StartLivekit>();
        app.init_resource::<MicState>();
        #[cfg(target_arch = "wasm32")]
        app.add_plugins(crate::livekit_web::MicPlugin);
        #[cfg(not(target_arch = "wasm32"))]
        app.add_plugins(crate::livekit_native::MicPlugin);
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

#[allow(clippy::type_complexity)]
fn connect_livekit(
    mut commands: Commands,
    mut new_livekits: Query<(Entity, &mut LivekitTransport), Without<LivekitConnection>>,
    player_state: Res<GlobalCrdtState>,
    #[cfg(not(target_arch = "wasm32"))] mic: Res<LocalAudioSource>,
) {
    for (transport_id, mut new_transport) in new_livekits.iter_mut() {
        debug!("spawn lk connect");
        let remote_address = new_transport.address.to_owned();
        let receiver = new_transport.receiver.take().unwrap();
        let sender = player_state.get_sender();

        #[cfg(not(target_arch = "wasm32"))]
        {
            let subscription = mic.subscribe();
            std::thread::spawn(move || {
                livekit_handler(transport_id, remote_address, receiver, sender, subscription)
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
            // For WASM, we directly call the handler which will spawn the async task
            if let Err(e) = livekit_handler_inner(transport_id, &remote_address, receiver, sender) {
                warn!("Failed to start livekit connection: {e}");
            }
        }

        commands.entity(transport_id).try_insert(LivekitConnection);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn livekit_handler(
    transport_id: Entity,
    remote_address: String,
    receiver: Receiver<NetworkMessage>,
    sender: Sender<PlayerUpdate>,
    mic: tokio::sync::broadcast::Receiver<LocalAudioFrame>,
) {
    let receiver = Arc::new(Mutex::new(receiver));

    loop {
        if let Err(e) = livekit_handler_inner(
            transport_id,
            &remote_address,
            receiver.clone(),
            sender.clone(),
            mic.resubscribe(),
        ) {
            warn!("livekit error: {e}");
        }
        if receiver.blocking_lock().is_closed() {
            // caller closed the channel
            return;
        }
        warn!("livekit connection dropped, reconnecting");
    }
}
