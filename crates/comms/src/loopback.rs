//! An in-engine scene room between an authoritative scene and its server copy: two `Transport`
//! halves with a LiveKit scene room's packets, identities and delivery rules.

use alloy_core::primitives::Address;
use bevy::{platform::collections::HashMap, prelude::*};
use common::util::ModifyComponentExt;
use dcl_component::proto_components::kernel::comms::rfc4;
use prost::Message as _;
use tokio::sync::mpsc;
use wallet::Wallet;

use crate::{
    global_crdt::{GlobalCrdtState, NetworkUpdate, NonPlayerUpdate, PlayerMessage, PlayerUpdate},
    pulse::plugin::LocalListenerLive,
    NetworkMessage, NetworkMessageRecipient, SceneRoom, Transport, TransportType,
};

pub const AUTH_SERVER_IDENTITY: &str = "authoritative-server";

pub struct LoopbackPlugin;

impl Plugin for LoopbackPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalSceneServers>()
            .add_systems(Update, pump_loopbacks);
    }
}

/// Server copies running in this engine, by scene hash.
#[derive(Resource, Default)]
pub struct LocalSceneServers(pub HashMap<String, LocalSceneServer>);

pub struct LocalSceneServer {
    pub context: Entity,
    pub server_end: Entity,
}

#[derive(Component)]
pub struct LoopbackEnd {
    receiver: mpsc::Receiver<NetworkMessage>,
    peer: Option<Entity>,
    server: bool,
    joined: bool,
}

fn spawn_end(
    commands: &mut Commands,
    hash: &str,
    context: Entity,
    peer: Option<Entity>,
    server: bool,
) -> Entity {
    let (sender, receiver) = mpsc::channel(1000);
    commands
        .spawn((
            Transport {
                transport_type: TransportType::SceneLoopback,
                sender,
                control: None,
                context,
            },
            SceneRoom(hash.to_owned()),
            LoopbackEnd {
                receiver,
                peer,
                server,
                joined: false,
            },
        ))
        .id()
}

/// The server copy's half of `hash`'s room, feeding the copy's own `context`.
pub fn spawn_server_end(commands: &mut Commands, hash: &str, context: Entity) -> Entity {
    spawn_end(commands, hash, context, None, true)
}

/// Join the local player to `server_end`'s room, feeding the client's `context`.
pub fn connect_client_end(
    commands: &mut Commands,
    hash: &str,
    server_end: Entity,
    context: Entity,
) -> Entity {
    let client_end = spawn_end(commands, hash, context, Some(server_end), false);
    commands
        .entity(server_end)
        .modify_component(move |end: &mut LoopbackEnd| end.peer = Some(client_end));
    client_end
}

/// Who receives a packet published on one half: the room holds only the local player and the
/// server, and like LiveKit a publisher never receives its own packet.
fn delivered(from_server: bool, recipient: NetworkMessageRecipient, me: Address) -> bool {
    match recipient {
        NetworkMessageRecipient::All => true,
        NetworkMessageRecipient::Peer(address) => from_server && address == me,
        NetworkMessageRecipient::AuthServer => !from_server,
    }
}

fn pump_loopbacks(
    mut ends: Query<(Entity, &mut LoopbackEnd)>,
    transports: Query<&Transport>,
    contexts: Query<&GlobalCrdtState>,
    wallet: Res<Wallet>,
    time: Res<Time>,
    pulse_live: Res<LocalListenerLive>,
) {
    let me = wallet.address();
    let now = time.elapsed_secs_f64();
    for (entity, mut end) in &mut ends {
        // the player's half went away: the server sees them leave the room
        if let (true, Some(gone), Some(me)) = (end.server, end.peer, me) {
            if !transports.contains(gone) {
                end.peer = None;
                if let Some(own) = transports
                    .get(entity)
                    .ok()
                    .and_then(|t| contexts.get(t.context).ok())
                {
                    let _ = own.get_sender().try_send(NetworkUpdate::PlayerLeft {
                        transport_id: entity,
                        address: me,
                    });
                }
            }
        }
        let peer = end.peer.filter(|peer| transports.contains(*peer));
        let target = peer
            .and_then(|peer| transports.get(peer).ok())
            .and_then(|transport| contexts.get(transport.context).ok())
            .map(GlobalCrdtState::get_sender);
        let from_server = end.server;
        // the room announces the player to the server as soon as they join, like a
        // LiveKit participant-connected event
        if let (false, false, Some(peer), Some(target), Some(me)) =
            (from_server, end.joined, peer, target.as_ref(), me)
        {
            end.joined = target
                .try_send(
                    PlayerUpdate {
                        transport_id: peer,
                        message: PlayerMessage::Joined,
                        address: me,
                    }
                    .into(),
                )
                .is_ok();
        }
        while let Ok(outgoing) = end.receiver.try_recv() {
            let (Some(peer), Some(target), Some(me)) = (peer, target.as_ref(), me) else {
                continue;
            };
            if !delivered(from_server, outgoing.recipient, me) {
                continue;
            }
            let Some(message) = outgoing
                .message
                .to_rfc4()
                .and_then(|bytes| rfc4::Packet::decode(bytes.as_slice()).ok())
                .and_then(|packet| packet.message)
            else {
                continue;
            };
            let update: NetworkUpdate = if from_server {
                NonPlayerUpdate {
                    transport_id: peer,
                    address: AUTH_SERVER_IDENTITY.to_owned(),
                    message,
                }
                .into()
            } else {
                // movement reaches a server over Pulse; the room carries it only while the
                // in-engine listener is down, so a solo preview works offline
                let message = match message {
                    rfc4::packet::Message::Movement(_) if pulse_live.0 => continue,
                    rfc4::packet::Message::Movement(movement) => PlayerMessage::Movement {
                        movement: Box::new(movement),
                        teleport: false,
                        timestamp: now,
                    },
                    message => PlayerMessage::PlayerData(message),
                };
                PlayerUpdate {
                    transport_id: peer,
                    message,
                    address: me,
                }
                .into()
            };
            if target.try_send(update).is_err() {
                warn!("scene loopback: receiving context is full, dropping a packet");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivers_like_a_livekit_scene_room() {
        let me = Address::repeat_byte(1);
        let other = Address::repeat_byte(2);
        use NetworkMessageRecipient::*;
        // client -> server: broadcasts and server-targeted writes reach the server
        assert!(delivered(false, All, me));
        assert!(delivered(false, AuthServer, me));
        assert!(!delivered(false, Peer(other), me));
        // server -> clients: broadcasts and packets addressed to the local player
        assert!(delivered(true, All, me));
        assert!(delivered(true, Peer(me), me));
        assert!(!delivered(true, Peer(other), me));
        assert!(!delivered(true, AuthServer, me));
    }
}
