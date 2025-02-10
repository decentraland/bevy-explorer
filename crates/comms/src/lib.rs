pub mod archipelago;
pub mod broadcast_position;
pub mod global_crdt;

#[cfg(feature = "livekit")]
pub mod livekit_room;

pub mod movement_compressed;
pub mod preview;
pub mod profile;
pub mod signed_login;
#[cfg(test)]
mod test;
pub mod websocket_room;

use std::marker::PhantomData;

use bevy::{
    ecs::system::SystemParam,
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use bimap::BiMap;
use common::util::TaskExt;
use ethers_core::types::{Address, H160};
use isahc::{
    http::{StatusCode, Uri},
    AsyncReadResponseExt, RequestExt,
};
use preview::PreviewPlugin;
use serde::{Deserialize, Serialize};
use signed_login::{SignedLoginPlugin, StartSignedLogin};
use tokio::sync::mpsc::Sender;

use dcl_component::{DclWriter, ToDclWriter};
use ipfs::CurrentRealm;
use wallet::{sign_request, Wallet};

use self::{
    archipelago::{ArchipelagoPlugin, StartArchipelago},
    broadcast_position::BroadcastPositionPlugin,
    global_crdt::GlobalCrdtPlugin,
    profile::UserProfilePlugin,
    websocket_room::{StartWsRoom, WebsocketRoomPlugin},
};

#[cfg(feature = "livekit")]
use self::livekit_room::{LivekitPlugin, StartLivekit};

const GATEKEEPER_URL: &str = "https://comms-gatekeeper.decentraland.org/get-scene-adapter";

pub mod chat_marker_things {
    pub const EMOTE: char = '␐';

    pub const ALL: [char; 3] = [EMOTE, '␑', '␆'];
}

pub struct CommsPlugin;

impl Plugin for CommsPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<SetCurrentScene>()
            .init_resource::<SceneRoomConnection>();

        app.add_plugins((
            WebsocketRoomPlugin,
            SignedLoginPlugin,
            ArchipelagoPlugin,
            BroadcastPositionPlugin,
            GlobalCrdtPlugin,
            UserProfilePlugin,
            PreviewPlugin,
        ));

        #[cfg(feature = "livekit")]
        app.add_plugins(LivekitPlugin);

        app.add_systems(Update, (process_realm_change, connect_scene_room));
    }
}

#[derive(PartialEq, Eq)]
pub enum TransportType {
    WebsocketRoom,
    Livekit,
    Archipelago,
    SceneRoom,
}

pub struct NetworkMessage {
    pub data: Vec<u8>,
    pub unreliable: bool,
    pub recipient: Option<H160>,
}

impl NetworkMessage {
    pub fn unreliable<D: ToDclWriter>(message: &D) -> Self {
        let mut data = Vec::new();
        let mut writer = DclWriter::new(&mut data);
        message.to_writer(&mut writer);
        Self {
            data,
            unreliable: true,
            recipient: None,
        }
    }

    pub fn reliable<D: ToDclWriter>(message: &D) -> Self {
        Self {
            unreliable: false,
            ..Self::unreliable(message)
        }
    }

    pub fn targetted_reliable<D: ToDclWriter>(message: &D, recipient: Option<H160>) -> Self {
        Self {
            unreliable: false,
            recipient,
            ..Self::unreliable(message)
        }
    }
}

#[derive(Component)]
pub struct Transport {
    pub transport_type: TransportType,
    pub sender: Sender<NetworkMessage>,
    pub foreign_aliases: BiMap<u32, Address>,
}

fn process_realm_change(
    mut commands: Commands,
    realm: Res<CurrentRealm>,
    adapters: Query<Entity, With<Transport>>,
    mut manager: AdapterManager,
    wallet: Res<Wallet>,
) {
    if realm.is_changed() || wallet.is_changed() {
        for adapter in adapters.iter() {
            commands.entity(adapter).despawn_recursive();
        }

        if wallet.address().is_none() {
            info!("disconnecting comms, no identity");
            return;
        }

        if let Some(comms) = realm.comms.as_ref() {
            if let Some(adapter) = comms.adapter.as_ref() {
                let real_adapter = adapter
                    .split_once(':')
                    .map(|(_, tail)| tail)
                    .unwrap_or(adapter.as_str());
                manager.connect(real_adapter);
            } else if let Some(adapter) = comms.fixed_adapter.as_ref() {
                manager.connect(adapter);
            }
        } else {
            debug!("missing comms!");
        }
    }
}

#[derive(Serialize, Event, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SetCurrentScene {
    pub realm_name: String,
    pub scene_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct GatekeeperResponse {
    adapter: String,
}

#[derive(Component)]
pub struct SceneRoom;

#[derive(Resource, Default)]
pub struct SceneRoomConnection(pub Option<(SetCurrentScene, String, Entity)>);

#[allow(clippy::type_complexity)]
fn connect_scene_room(
    mut commands: Commands,
    mut manager: AdapterManager,
    mut gatekeeper_task: Local<Option<Task<Result<(String, SetCurrentScene), anyhow::Error>>>>,
    mut current: ResMut<SceneRoomConnection>,
    mut scene: EventReader<SetCurrentScene>,
    wallet: Res<Wallet>,
) {
    if let Some(ev) = scene.read().last().cloned() {
        if let Some((existing, room, entity)) = current.0.take() {
            if existing == ev {
                current.0 = Some((existing, room, entity));
                return;
            }
            if let Some(commands) = commands.get_entity(entity) {
                commands.despawn_recursive();
            }
            warn!("disconnected scene channel {ev:?}");
        }
        if ev.scene_id.is_empty() {
            *gatekeeper_task = None;
        } else {
            let wallet = wallet.clone();
            let uri = Uri::try_from(GATEKEEPER_URL).unwrap();
            *gatekeeper_task = Some(IoTaskPool::get().spawn(async move {
                let headers = sign_request("POST", &uri, &wallet, &ev).await?;

                let mut request =
                    isahc::Request::post(uri).header("Content-Type", "application/json");
                for (k, v) in headers {
                    request = request.header(k, v);
                }
                let mut response = request.body(())?.send_async().await?;

                if response.status() != StatusCode::OK {
                    return Err(anyhow::anyhow!("status: {}", response.status()));
                }

                Ok((response.json::<GatekeeperResponse>().await?.adapter, ev))
            }));
        }
    }

    if let Some(mut task) = gatekeeper_task.take() {
        match task.complete() {
            None => *gatekeeper_task = Some(task),
            Some(Err(e)) => warn!("failed to get scene room from gatekeeper: {e}"),
            Some(Ok((adapter, ev))) => {
                if let Some(ent) = manager.connect(&adapter) {
                    warn!("added scene channel {ev:?}");
                    current.0 = Some((ev, adapter, ent));
                    commands.entity(ent).insert(SceneRoom);
                }
            }
        }
    }
}

#[derive(SystemParam)]
pub struct AdapterManager<'w, 's> {
    #[cfg(feature = "livekit")]
    commands: Commands<'w, 's>,
    ws_room_events: EventWriter<'w, StartWsRoom>,
    #[cfg(feature = "livekit")]
    livekit_events: EventWriter<'w, StartLivekit>,
    archipelago_events: EventWriter<'w, StartArchipelago>,
    // can't use event writer due to conflict on Res<Events>
    pub signed_login_events: ResMut<'w, Events<StartSignedLogin>>,
    #[system_param(ignore)]
    _p: PhantomData<&'s ()>,
}

impl AdapterManager<'_, '_> {
    pub fn connect(&mut self, adapter: &str) -> Option<Entity> {
        let Some((protocol, address)) = adapter.split_once(':') else {
            warn!("unrecognised adapter string: {adapter}");
            return None;
        };

        match protocol {
            "ws-room" => {
                self.ws_room_events.send(StartWsRoom {
                    address: address.to_owned(),
                });
            }
            "signed-login" => {
                self.signed_login_events.send(StartSignedLogin {
                    address: address.to_owned(),
                });
            }
            #[cfg(feature = "livekit")]
            "livekit" => {
                let entity = self.commands.spawn_empty().id();
                self.livekit_events.send(StartLivekit {
                    entity,
                    address: address.to_owned(),
                });
                return Some(entity);
            }
            #[cfg(not(feature = "livekit"))]
            "livekit" => {
                info!("livekit not enabled: comms offline");
            }
            "offline" => {
                info!("comms offline");
            }
            "archipelago" => {
                debug!("arch starting: {address}");
                self.archipelago_events.send(StartArchipelago {
                    address: address.to_owned(),
                });
            }
            "fixed-adapter" => {
                // fixed-adapter should be ignored and we use the tail as the full protocol:address
                return self.connect(address);
            }
            _ => {
                warn!("unrecognised adapter protocol: {protocol}");
            }
        }

        None
    }
}
