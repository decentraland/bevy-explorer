use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
    utils::HashMap,
};
use isahc::http::Uri;
use livekit::{RoomOptions, DataPacketKind};
use prost::Message;
use tokio::sync::mpsc::{Receiver, Sender};

use dcl_component::proto_components::kernel::comms::rfc4;
use common::util::AsH160;

use super::{
    global_crdt::{GlobalCrdtState, PlayerUpdate},
    NetworkMessage,
};

pub struct LivekitPlugin;

impl Plugin for LivekitPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, connect_livekit);
    }
}

#[derive(Component)]
pub struct LivekitTransport {
    pub address: String,
    pub receiver: Option<Receiver<NetworkMessage>>,
    pub retries: usize,
}

#[derive(Component)]
pub struct LivekitConnection(Task<()>);

#[allow(clippy::type_complexity)]
fn connect_livekit(
    mut commands: Commands,
    mut new_livekits: Query<(Entity, &mut LivekitTransport), Without<LivekitConnection>>,
    player_state: Res<GlobalCrdtState>,
) {
    for (transport_id, mut new_transport) in new_livekits.iter_mut() {
        println!("spawn lk connect");
        let remote_address = new_transport.address.to_owned();
        let receiver = new_transport.receiver.take().unwrap();
        let sender = player_state.get_sender();

        let task = IoTaskPool::get().spawn(livekit_handler(
            transport_id,
            remote_address,
            receiver,
            sender,
        ));
        commands
            .entity(transport_id)
            .insert(LivekitConnection(task));
    }
}

async fn livekit_handler(
    transport_id: Entity,
    remote_address: String,
    receiver: Receiver<NetworkMessage>,
    sender: Sender<PlayerUpdate>,
) {
    if let Err(e) = livekit_handler_inner(transport_id, remote_address, receiver, sender).await {
        warn!("livekit error: {e}");
    }
    warn!("thread exit")
}

async fn livekit_handler_inner(
    transport_id: Entity,
    remote_address: String,
    mut app_rx: Receiver<NetworkMessage>,
    sender: Sender<PlayerUpdate>,
) -> Result<(), anyhow::Error> {
    println!(">> lk connect async : {remote_address}");

    let url = Uri::try_from(remote_address).unwrap();
    let address = format!(
        "{}://{}{}",
        url.scheme_str().unwrap_or_default(),
        url.host().unwrap_or_default(),
        url.path()
    );
    let params = HashMap::from_iter(url.query().unwrap_or_default().split('&').flat_map(|par| {
        par.split_once('=')
            .map(|(a, b)| (a.to_owned(), b.to_owned()))
    }));
    println!("{params:?}");
    let token = params.get("access_token").cloned().unwrap_or_default();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let task = rt.spawn(async move {
        let (room, mut network_rx) = livekit::prelude::Room::connect(&address, &token, RoomOptions{ auto_subscribe: false, adaptive_stream: false, dynacast: false }).await.unwrap();

        'stream: loop {
            tokio::select!(
                incoming = network_rx.recv() => {
                    println!("in: {:?}", incoming);
                    let Some(incoming) = incoming else {
                        println!("network pipe broken, exiting loop");
                        break 'stream;
                    };

                    match incoming {
                        livekit::RoomEvent::DataReceived { payload, participant, .. } => {
                            if let Some(address) = participant.identity().0.as_str().as_h160() {
                                let packet = match rfc4::Packet::decode(payload.as_slice()) {
                                    Ok(packet) => packet,
                                    Err(e) => {
                                        warn!("unable to parse packet body: {e}");
                                        continue;
                                    }
                                };
                                let Some(message) = packet.message else {
                                    warn!("received empty packet body");
                                    continue;
                                };
                                warn!("received packet {message:?} from {address}");
                                if let Err(e) = sender.send(PlayerUpdate {
                                    transport_id,
                                    message,
                                    address,
                                }).await {
                                    warn!("app pipe broken ({e}), existing loop");
                                    break 'stream;
                                }
                            }
                        },
                        _ => { println!("Event: {:?}", incoming); }
                    };
                }
                outgoing = app_rx.recv() => {
                    let Some(outgoing) = outgoing else {
                        println!("app pipe broken, exiting loop");
                        break 'stream;
                    };

                    let kind = if outgoing.unreliable {
                        DataPacketKind::Lossy
                    } else {
                        DataPacketKind::Reliable
                    };
                    if let Err(e) = room.local_participant().publish_data(outgoing.data, kind, Default::default()).await {
                        println!("outgoing failed: {e}; not exiting loop though since it often fails at least once or twice at the start...");
                        // break 'stream;
                    };
                }
            );
        }

        println!("closing room");
        room.close().await.unwrap();
        println!("leaving");
    });

    println!("blocking");
    rt.block_on(task).unwrap();
    println!("ok out");
    Ok(())
}
