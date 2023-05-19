use std::ops::RangeInclusive;

use bevy::{prelude::*, utils::HashMap};
use bimap::BiMap;
use ethers::types::Address;
use tokio::sync::{broadcast, mpsc};

use crate::{
    camera_controller::GroundHeight,
    dcl::{
        crdt::{append_component, put_component},
        interface::{crdt_context::CrdtContext, CrdtStore, CrdtType},
        SceneId,
    },
    dcl_component::{
        proto_components::{
            kernel::comms::rfc4::{self, packet::Message},
            sdk::components::PbPlayerIdentityData,
        },
        transform_and_parent::{DclQuat, DclTransformAndParent, DclTranslation},
        DclReader, DclWriter, SceneComponentId, SceneEntityId, ToDclWriter,
    },
};

use super::profile::{ProfileEvent, ProfileEventType};

const FOREIGN_PLAYER_RANGE: RangeInclusive<u16> = 6..=406;

pub struct GlobalCrdtPlugin;

impl Plugin for GlobalCrdtPlugin {
    fn build(&self, app: &mut App) {
        let (ext_sender, ext_receiver) = mpsc::channel(1000);
        let (int_sender, int_receiver) = broadcast::channel(1000);
        // leak the receiver so it never gets dropped
        Box::leak(Box::new(int_receiver));
        app.insert_resource(GlobalCrdtState {
            ext_receiver,
            ext_sender,
            int_sender,
            context: CrdtContext::new(SceneId::DUMMY),
            store: Default::default(),
            lookup: Default::default(),
        });
        app.add_system(process_transport_updates);
        app.add_system(despawn_players);
        app.add_event::<PlayerPositionEvent>();
    }
}

#[derive(Debug)]
pub struct PlayerUpdate {
    pub transport_id: Entity,
    pub message: rfc4::packet::Message,
    pub address: Address,
}

#[derive(Resource)]
pub struct GlobalCrdtState {
    // receiver from sockets
    ext_receiver: mpsc::Receiver<PlayerUpdate>,
    // sender for sockets to post to
    ext_sender: mpsc::Sender<PlayerUpdate>,
    // sender for broadcast updates
    int_sender: broadcast::Sender<Vec<u8>>,
    // receiver for broadcast updates (we keep it to ensure it doesn't get closed)
    context: CrdtContext,
    store: CrdtStore,
    lookup: BiMap<Address, Entity>,
}

impl GlobalCrdtState {
    // get a channel to which updates can be sent
    pub fn get_sender(&self) -> mpsc::Sender<PlayerUpdate> {
        self.ext_sender.clone()
    }

    // get a channel from which crdt updates can be received
    pub fn subscribe(&self) -> (CrdtStore, broadcast::Receiver<Vec<u8>>) {
        (self.store.clone(), self.int_sender.subscribe())
    }

    pub fn update_crdt(
        &mut self,
        component_id: SceneComponentId,
        crdt_type: CrdtType,
        id: SceneEntityId,
        data: &impl ToDclWriter,
    ) {
        let mut buf = Vec::new();
        DclWriter::new(&mut buf).write(data);
        let timestamp =
            self.store
                .force_update(component_id, crdt_type, id, Some(&mut DclReader::new(&buf)));
        let crdt_message = match crdt_type {
            CrdtType::LWW(_) => put_component(&id, &component_id, &timestamp, Some(&buf)),
            CrdtType::GO(_) => append_component(&id, &component_id, &buf),
        };
        if let Err(e) = self.int_sender.send(crdt_message) {
            error!("failed to send foreign player update to scenes: {e}");
        }
    }
}

#[derive(Component, Debug)]
pub struct ForeignPlayer {
    pub address: Address,
    pub transport_id: Entity,
    pub last_update: f32,
    pub scene_id: SceneEntityId,
    pub profile_version: u32,
}

pub struct PlayerPositionEvent {
    pub index: u32,
    pub player: Entity,
    pub time: f32,
    pub translation: DclTranslation,
    pub rotation: DclQuat,
}

#[derive(Component)]
pub struct TransportRef(Entity);

pub fn process_transport_updates(
    mut commands: Commands,
    mut state: ResMut<GlobalCrdtState>,
    mut players: Query<&mut ForeignPlayer>,
    time: Res<Time>,
    mut profile_events: EventWriter<ProfileEvent>,
    mut position_events: EventWriter<PlayerPositionEvent>,
) {
    let mut created_this_frame = HashMap::default();

    while let Ok(update) = state.ext_receiver.try_recv() {
        // create/update timestamp/transport_id on the foreign player
        let (entity, scene_id) =
            if let Some((entity, scene_id)) = created_this_frame.get(&update.address) {
                (*entity, *scene_id)
            } else if let Some(existing) = state.lookup.get_by_left(&update.address) {
                let mut foreign_player = players.get_mut(*existing).unwrap();
                foreign_player.last_update = time.elapsed_seconds();
                foreign_player.transport_id = update.transport_id;
                (*existing, foreign_player.scene_id)
            } else {
                let Some(next_free) = state.context.new_in_range(&FOREIGN_PLAYER_RANGE) else {
                    warn!("no space for any more players!");
                    continue;
                };

                state.update_crdt(
                    SceneComponentId::PLAYER_IDENTITY_DATA,
                    CrdtType::LWW_ANY,
                    next_free,
                    &PbPlayerIdentityData {
                        address: format!("{:#x}", update.address),
                    },
                );

                let new_entity = commands
                    .spawn((
                        SpatialBundle::default(),
                        GroundHeight(0.0),
                        ForeignPlayer {
                            address: update.address,
                            transport_id: update.transport_id,
                            last_update: time.elapsed_seconds(),
                            scene_id: next_free,
                            profile_version: 0,
                        },
                    ))
                    .id();

                state.lookup.insert(update.address, new_entity);

                info!(
                    "creating new player: {} -> {:?} / {}",
                    update.address, new_entity, next_free
                );
                created_this_frame.insert(update.address, (new_entity, next_free));
                (new_entity, next_free)
            };

        // process update
        match update.message {
            Message::Position(pos) => {
                let dcl_transform = DclTransformAndParent {
                    translation: DclTranslation([pos.position_x, pos.position_y, pos.position_z]),
                    rotation: DclQuat([
                        pos.rotation_x,
                        pos.rotation_y,
                        pos.rotation_z,
                        pos.rotation_w,
                    ]),
                    scale: Vec3::ONE,
                    parent: SceneEntityId::WORLD_ORIGIN,
                };
                debug!(
                    "player: {:#x} -> {}",
                    update.address,
                    Vec3::new(pos.position_x, pos.position_y, pos.position_z)
                );
                // commands
                //     .entity(entity)
                //     .insert(dcl_transform.to_bevy_transform());
                state.update_crdt(
                    SceneComponentId::TRANSFORM,
                    CrdtType::LWW_ANY,
                    scene_id,
                    &dcl_transform,
                );
                position_events.send(PlayerPositionEvent {
                    index: pos.index,
                    time: time.elapsed_seconds(),
                    player: entity,
                    translation: DclTranslation([pos.position_x, pos.position_y, pos.position_z]),
                    rotation: DclQuat([
                        pos.rotation_x,
                        pos.rotation_y,
                        pos.rotation_z,
                        pos.rotation_w,
                    ]),
                })
            }
            Message::ProfileVersion(version) => {
                profile_events.send(ProfileEvent {
                    sender: entity,
                    event: ProfileEventType::Version(version),
                });
            }
            Message::ProfileRequest(request) => {
                profile_events.send(ProfileEvent {
                    sender: entity,
                    event: ProfileEventType::Request(request),
                });
            }
            Message::ProfileResponse(response) => {
                profile_events.send(ProfileEvent {
                    sender: entity,
                    event: ProfileEventType::Response(response),
                });
            }
            Message::Chat(_) => (),
            Message::Scene(_) => (),
            Message::Voice(_) => (),
        }
    }
}

fn despawn_players(
    mut commands: Commands,
    players: Query<(Entity, &ForeignPlayer)>,
    mut state: ResMut<GlobalCrdtState>,
    time: Res<Time>,
) {
    for (entity, player) in players.iter() {
        if player.last_update + 5.0 < time.elapsed_seconds() {
            if let Some(commands) = commands.get_entity(entity) {
                info!("removing stale player: {entity:?} : {player:?}");
                commands.despawn_recursive();
            }

            state.lookup.remove_by_right(&entity);
        }
    }
}
