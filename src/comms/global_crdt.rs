use std::ops::RangeInclusive;

use bevy::{prelude::*, utils::HashMap};
use bimap::BiMap;
use ethers::types::H160;
use tokio::sync::{broadcast, mpsc};

use crate::{
    dcl::{
        crdt::put_component,
        interface::{crdt_context::CrdtContext, CrdtStore, CrdtType},
        SceneId,
    },
    dcl_component::{
        proto_components::kernel::comms::rfc4::{self, packet::Message},
        transform_and_parent::{DclQuat, DclTransformAndParent, DclTranslation},
        DclReader, SceneComponentId, SceneEntityId, ToDclWriter,
    },
    scene_runner::update_world::{material::MaterialDefinition, mesh_renderer::MeshDefinition},
};

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
        app.add_system(process_updates);
    }
}

#[derive(Debug)]
pub struct PlayerUpdate {
    pub message: rfc4::packet::Message,
    pub address: H160,
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
    lookup: BiMap<H160, Entity>,
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
}

#[derive(Component)]
pub struct ForeignPlayer {
    pub address: H160,
    pub last_update: f32,
    pub scene_id: SceneEntityId,
}

fn process_updates(
    mut commands: Commands,
    mut state: ResMut<GlobalCrdtState>,
    mut players: Query<&mut ForeignPlayer>,
    time: Res<Time>,
) {
    let mut created_this_frame = HashMap::default();

    while let Ok(update) = state.ext_receiver.try_recv() {
        // create/update timestamp on the foreign player
        let (entity, scene_id) =
            if let Some((entity, scene_id)) = created_this_frame.get(&update.address) {
                (*entity, *scene_id)
            } else if let Some(existing) = state.lookup.get_by_left(&update.address) {
                let mut foreign_player = players.get_mut(*existing).unwrap();
                foreign_player.last_update = time.elapsed_seconds();
                (*existing, foreign_player.scene_id)
            } else {
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

                //hack in a marker for foreign avatar
                commands.entity(new_entity).insert((
                    SpatialBundle::default(),
                    MeshDefinition::Sphere {},
                    MaterialDefinition {
                        material: Color::rgba(1.0, 1.0, 0.0, 0.6).into(),
                        shadow_caster: true,
                    },
                ));

                debug!(
                    "created player entity: {} -> {:?},{}",
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
                let buf = dcl_transform.to_vec();
                let timestamp = state.store.force_update(
                    SceneComponentId::TRANSFORM,
                    CrdtType::LWW_ANY,
                    scene_id,
                    Some(&mut DclReader::new(&buf)),
                );
                commands
                    .entity(entity)
                    .insert(dcl_transform.to_bevy_transform());
                let crdt_message = put_component(
                    &scene_id,
                    &SceneComponentId::TRANSFORM,
                    &timestamp,
                    Some(&buf),
                );
                if let Err(e) = state.int_sender.send(crdt_message) {
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
