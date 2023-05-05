use std::ops::RangeInclusive;

use bevy::prelude::*;
use bimap::BiMap;
use ethers::types::H160;
use tokio::sync::{broadcast, mpsc};

use crate::{
    dcl::{
        interface::{crdt_context::CrdtContext, CrdtStore, CrdtType},
        SceneId,
    },
    dcl_component::{
        proto_components::kernel::comms::rfc4::{self, packet::Message},
        transform_and_parent::{DclQuat, DclTransformAndParent, DclTranslation},
        DclReader, SceneComponentId, SceneEntityId, ToDclWriter,
    },
};

const FOREIGN_PLAYER_RANGE: RangeInclusive<u16> = 6..=406;

pub struct ForeignPlayerPlugin;

impl Plugin for ForeignPlayerPlugin {
    fn build(&self, app: &mut App) {
        let (ext_sender, ext_receiver) = mpsc::channel(1000);
        let (int_sender, int_receiver) = broadcast::channel(1000);
        // leak the receiver so it never gets dropped
        Box::leak(Box::new(int_receiver));
        app.insert_resource(ForeignPlayerState {
            ext_receiver,
            ext_sender,
            int_sender,
            context: CrdtContext::new(SceneId::DUMMY),
            store: Default::default(),
            lookup: Default::default(),
        });
        app.add_system(process_updates);
    }
}

#[derive(Debug)]
pub struct PlayerUpdate {
    pub message: rfc4::packet::Message,
    pub address: H160,
}

#[derive(Resource)]
pub struct ForeignPlayerState {
    // receiver from sockets
    ext_receiver: mpsc::Receiver<PlayerUpdate>,
    // sender for sockets to post to
    ext_sender: mpsc::Sender<PlayerUpdate>,
    // sender for broadcast updates
    int_sender: broadcast::Sender<Vec<u8>>,
    // receiver for broadcast updates (we keep it to ensure it doesn't get closed)
    context: CrdtContext,
    store: CrdtStore,
    lookup: BiMap<H160, Entity>,
}

impl ForeignPlayerState {
    // get a channel to which updates can be sent
    pub fn get_sender(&self) -> mpsc::Sender<PlayerUpdate> {
        self.ext_sender.clone()
    }

    // get a channel from which crdt updates can be received
    pub fn get_receiver(&self) -> (CrdtStore, broadcast::Receiver<Vec<u8>>) {
        (self.store.clone(), self.int_sender.subscribe())
    }
}

#[derive(Component)]
pub struct ForeignPlayer {
    pub address: H160,
    pub last_update: f32,
    pub scene_id: SceneEntityId,
}

fn process_updates(
    mut commands: Commands,
    mut state: ResMut<ForeignPlayerState>,
    mut players: Query<&mut ForeignPlayer>,
    time: Res<Time>,
) {
    while let Ok(update) = state.ext_receiver.try_recv() {
        // create/update timestamp on the foreign player
        let (_entity, scene_id) = match state.lookup.get_by_left(&update.address) {
            Some(existing) => {
                let mut foreign_player = players.get_mut(*existing).unwrap();
                foreign_player.last_update = time.elapsed_seconds();
                (*existing, foreign_player.scene_id)
            }
            None => {
                let Some(next_free) = state.context.new_in_range(&FOREIGN_PLAYER_RANGE) else {
                    warn!("no space for any more players!");
                    continue;
                };

                let new_entity = commands
                    .spawn(ForeignPlayer {
                        address: update.address,
                        last_update: time.elapsed_seconds(),
                        scene_id: next_free,
                    })
                    .id();

                state.lookup.insert(update.address, new_entity);
                (new_entity, next_free)
            }
        };

        // process update
        match update.message {
            Message::Position(pos) => {
                let buf = DclTransformAndParent {
                    translation: DclTranslation([pos.position_x, pos.position_y, pos.position_z]),
                    rotation: DclQuat([
                        pos.rotation_x,
                        pos.rotation_y,
                        pos.rotation_z,
                        pos.rotation_w,
                    ]),
                    scale: Vec3::ONE,
                    parent: SceneEntityId::WORLD_ORIGIN,
                }
                .to_vec();
                state.store.force_update(
                    SceneComponentId::TRANSFORM,
                    CrdtType::LWW_ANY,
                    scene_id,
                    Some(&mut DclReader::new(&buf)),
                );
                if let Err(e) = state.int_sender.send(buf) {
                    error!("failed to send foreign player update: {e}");
                }
                debug!(
                    "player: {:#x} -> {}",
                    update.address,
                    Vec3::new(pos.position_x, pos.position_y, pos.position_z)
                );
            }
            Message::ProfileVersion(_) => (),
            Message::ProfileRequest(_) => (),
            Message::ProfileResponse(_) => (),
            Message::Chat(_) => (),
            Message::Scene(_) => (),
            Message::Voice(_) => (),
        }
    }
}
