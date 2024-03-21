use anyhow::{anyhow, bail};
use async_tungstenite::tungstenite::{client::IntoClientRequest, http::HeaderValue};
use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
    utils::HashMap,
};
use futures_lite::future;
use futures_util::{pin_mut, select, stream::StreamExt, FutureExt, SinkExt};
use ipfs::CurrentRealm;
use prost::Message;
use serde_json::json;
use tokio::sync::mpsc::{Receiver, Sender};

use common::rpc::{RpcCall, RpcEventSender};
use wallet::Wallet;

use crate::{AdapterManager, Transport, TransportType};

use super::NetworkMessage;

use dcl_component::{
    proto_components::{
        common::Position,
        kernel::comms::{
            rfc4,
            v3::{
                client_packet, server_packet, ChallengeRequestMessage, ChallengeResponseMessage,
                ClientPacket, Heartbeat, ServerPacket, SignedChallengeMessage,
            },
        },
    },
    DclReader,
};

pub struct ArchipelagoPlugin;

impl Plugin for ArchipelagoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                connect_websocket,
                reconnect_websocket,
                start_archipelago,
                manage_islands,
            ),
        );

        app.add_event::<StartArchipelago>();
        app.init_resource::<IslandChannel>();
    }
}

#[derive(Event)]
pub struct StartArchipelago {
    pub address: String,
}

pub struct StartIsland {
    owner: Entity,
    connect_str: String,
    name: String,
}

#[derive(Resource)]
pub struct IslandChannel {
    sender: tokio::sync::mpsc::Sender<StartIsland>,
    receiver: tokio::sync::mpsc::Receiver<StartIsland>,
}

impl Default for IslandChannel {
    fn default() -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(100);
        Self { sender, receiver }
    }
}

#[derive(Component)]
pub struct ArchipelagoTransport {
    pub address: String,
    pub receiver: Option<Receiver<NetworkMessage>>,
    pub retries: usize,
}

#[derive(Component)]
pub struct ArchipelagoConnection(Task<(Receiver<NetworkMessage>, anyhow::Error)>);

pub fn start_archipelago(mut commands: Commands, mut archi_events: EventReader<StartArchipelago>) {
    if let Some(ev) = archi_events.read().last() {
        info!("starting archipelago protocol");
        let (sender, receiver) = tokio::sync::mpsc::channel(1000);

        commands.spawn((
            Transport {
                transport_type: TransportType::Archipelago,
                sender,
                foreign_aliases: Default::default(),
            },
            ArchipelagoTransport {
                address: ev.address.to_owned(),
                receiver: Some(receiver),
                retries: 0,
            },
        ));
    }
}

fn manage_islands(
    mut commands: Commands,
    mut manager: AdapterManager,
    mut channel: ResMut<IslandChannel>,
    mut current_island: Local<HashMap<Entity, Entity>>,
    current_realm: Res<CurrentRealm>,
    mut senders: Local<Vec<RpcEventSender>>,
    mut events: EventReader<RpcCall>,
) {
    for sender in events.read().filter_map(|ev| match ev {
        RpcCall::SubscribeRealmChanged { sender } => Some(sender),
        _ => None,
    }) {
        senders.push(sender.clone());
    }

    while let Ok(island) = channel.receiver.try_recv() {
        if let Some(entity) = current_island.remove(&island.owner) {
            commands.entity(entity).despawn_recursive();
        }
        if let Some(entity) = manager.connect(&island.connect_str) {
            current_island.insert(island.owner, entity);
        }

        let mut event_data = None;
        senders.retain_mut(|sender| {
            let message = event_data.get_or_insert_with(|| {
                let server = current_realm.config.realm_name.as_deref().unwrap_or_default();
                let room = &island.name;
                json!({
                    "serverName": server,
                    "room": room,
                    "displayName": format!("{server}-{room}"),
                    "domain": current_realm.address.strip_suffix("/content").unwrap_or(&current_realm.address),
                })
                .to_string()
            }).clone();
            let _ = sender.send(message);
            !sender.is_closed()
        });
    }
}

#[allow(clippy::type_complexity)]
fn connect_websocket(
    mut commands: Commands,
    mut new_websockets: Query<(Entity, &mut ArchipelagoTransport), Without<ArchipelagoConnection>>,
    wallet: Res<Wallet>,
    island_channel: Res<IslandChannel>,
) {
    for (transport_id, mut new_transport) in new_websockets.iter_mut() {
        let remote_address = new_transport.address.to_owned();
        let wallet = wallet.clone();
        let receiver = new_transport.receiver.take().unwrap();
        let sender = island_channel.sender.clone();
        let task = IoTaskPool::get().spawn(archipelago_handler(
            transport_id,
            remote_address,
            wallet,
            receiver,
            sender,
        ));
        commands
            .entity(transport_id)
            .try_insert(ArchipelagoConnection(task));
    }
}

fn reconnect_websocket(
    mut websockets: Query<(
        Entity,
        &mut ArchipelagoTransport,
        &mut ArchipelagoConnection,
    )>,
    wallet: Res<Wallet>,
    island_channel: Res<IslandChannel>,
) {
    for (transport_id, mut transport, mut conn) in websockets.iter_mut() {
        if transport.retries < 3 {
            if conn.0.is_finished() {
                transport.retries += 1;
                let (receiver, err) = future::block_on(future::poll_once(&mut conn.0)).unwrap();
                warn!("archipelago error: {err}, retrying [{}]", transport.address);
                let remote_address = transport.address.to_owned();
                let wallet = wallet.clone();
                let sender = island_channel.sender.clone();
                let task = IoTaskPool::get().spawn(archipelago_handler(
                    transport_id,
                    remote_address,
                    wallet,
                    receiver,
                    sender,
                ));
                conn.0 = task;
            }
        } else if transport.retries == 3 && conn.0.is_finished() {
            transport.retries += 1;
            let (_, err) = future::block_on(future::poll_once(&mut conn.0)).unwrap();
            warn!("archipelago error: {err}, giving up");
        }
    }
}

async fn archipelago_handler(
    transport_id: Entity,
    remote_address: String,
    wallet: Wallet,
    mut receiver: Receiver<NetworkMessage>,
    sender: Sender<StartIsland>,
) -> (Receiver<NetworkMessage>, anyhow::Error) {
    let res =
        archipelago_handler_inner(transport_id, remote_address, wallet, &mut receiver, sender)
            .await;
    (receiver, res.err().unwrap_or(anyhow!("connection closed")))
}

async fn archipelago_handler_inner(
    transport_id: Entity,
    remote_address: String,
    wallet: Wallet,
    receiver: &mut Receiver<NetworkMessage>,
    sender: Sender<StartIsland>,
) -> Result<(), anyhow::Error> {
    debug!(">> stream connect async : {remote_address}");

    let remote_address = if remote_address.starts_with("ws:") || remote_address.starts_with("wss:")
    {
        remote_address
    } else {
        format!("wss://{remote_address}")
    };

    let mut request = remote_address.into_client_request()?;
    request.headers_mut().append(
        "Sec-WebSocket-Protocol",
        HeaderValue::from_static("archipelago"),
    );

    let (mut stream, response) = async_tungstenite::async_std::connect_async(request).await?;
    debug!("<< stream connected, response: {response:?}");

    // send peer identification
    let ident = ClientPacket {
        message: Some(client_packet::Message::ChallengeRequest(
            ChallengeRequestMessage {
                address: format!(
                    "{:#x}",
                    wallet.address().ok_or(anyhow!("wallet not connected"))?
                ),
            },
        )),
    };
    stream.send(ident.encode_to_vec().into()).await?;
    debug!(">> challeng request sent: {ident:?}");

    // challenge / welcome
    loop {
        let Some(response) = stream.next().await else {
            bail!("stream closed unexpectedly awaiting challenge")
        };
        let response = response?;
        let response = ServerPacket::decode(response.into_data().as_slice())?;
        let Some(message) = response.message else {
            bail!("received empty packet")
        };

        match message {
            server_packet::Message::Welcome(welcome) => {
                debug!("<< welcome received: {welcome:?}");
                break;
            }
            server_packet::Message::ChallengeResponse(ChallengeResponseMessage {
                challenge_to_sign,
                ..
            }) => {
                // send challenge response
                debug!("<< challenge received; {challenge_to_sign}");

                // sign challenge
                let chain = wallet.sign_message(challenge_to_sign).await?;
                let auth_chain_json = serde_json::to_string(&chain)?;
                debug!(">> auth chain created: {auth_chain_json}");

                // send response
                let message = ClientPacket {
                    message: Some(client_packet::Message::SignedChallenge(
                        SignedChallengeMessage { auth_chain_json },
                    )),
                };
                let message = message.encode_to_vec();
                stream.send(message.into()).await?;
                debug!(">> auth chain sent");
            }
            _ => bail!("unexpected message during handshake: {message:?}"),
        }
    }

    let (mut write, mut read) = stream.split();

    // wrap and transmit outbound heartbeat
    let f_write = async move {
        while let Some(next) = receiver.recv().await {
            let Ok(rfc4::Packet {
                message: Some(rfc4::packet::Message::Position(pos)),
            }) = DclReader::new(&next.data).read()
            else {
                // skip non-position messages
                continue;
            };

            let packet = ClientPacket {
                message: Some(client_packet::Message::Heartbeat(Heartbeat {
                    position: Some(Position {
                        x: pos.position_x,
                        y: pos.position_y,
                        z: pos.position_z,
                    }),
                    desired_room: None,
                })),
            };
            let mut buf = Vec::default();
            packet.encode(&mut buf)?;
            write.send(buf.into()).await?;
        }

        Ok::<(), anyhow::Error>(())
    }
    .fuse();

    // unwrap and forward inbound messages
    let f_read = async move {
        // let mut island = None;

        while let Some(next) = read.next().await {
            let next = next?;
            let next = ServerPacket::decode(next.into_data().as_slice())?;
            let Some(message) = next.message else {
                bail!("received empty packet")
            };

            match message {
                server_packet::Message::ChallengeResponse(_)
                | server_packet::Message::Welcome(_) => {
                    warn!("unexpected bau message: {message:?}");
                    continue;
                }
                server_packet::Message::IslandChanged(change) => {
                    sender
                        .send(StartIsland {
                            owner: transport_id,
                            connect_str: change.conn_str,
                            name: change.island_id,
                        })
                        .await?;
                }
                server_packet::Message::LeftIsland(_)
                | server_packet::Message::JoinIsland(_)
                | server_packet::Message::Kicked(_) => {
                    warn!("message: {:?}", message);
                }
            }
        }

        Ok(())
    }
    .fuse();

    // until either stream is broken
    pin_mut!(f_read, f_write);
    select! {
        read_res = f_read => read_res,
        write_res = f_write => write_res,
    }
}
