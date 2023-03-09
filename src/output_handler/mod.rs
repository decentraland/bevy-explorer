use bevy::prelude::*;

use crate::scene_runner::{
    crdt::{lww::CrdtLWWState, AddCrdtInterfaceExt, FromProto},
    engine::ReadDclFormat,
    SceneComponentId, SceneContext, SceneEntityId, SceneSets,
};

// plugin to manage some commands from the scene script
pub struct SceneOutputPlugin;

impl Plugin for SceneOutputPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_interface::<TransformAndParent>(SceneComponentId(1));
        app.add_system(process_transform_and_parent_updates.in_set(SceneSets::HandleOutput));
    }
}

#[derive(Debug)]
struct TransformAndParent {
    transform: Transform,
    parent: SceneEntityId,
}

impl FromProto for TransformAndParent {
    fn from_proto(buf: &mut protobuf::CodedInputStream) -> Result<Self, protobuf::Error> {
        Ok(TransformAndParent {
            transform: Transform {
                translation: Vec3::new(
                    buf.read_be_float()?,
                    buf.read_be_float()?,
                    buf.read_be_float()?,
                ),
                rotation: Quat::from_xyzw(
                    buf.read_be_float()?,
                    buf.read_be_float()?,
                    buf.read_be_float()?,
                    buf.read_be_float()?,
                ),
                scale: Vec3::new(
                    buf.read_be_float()?,
                    buf.read_be_float()?,
                    buf.read_be_float()?,
                ),
            },
            parent: SceneEntityId(buf.read_be_u32()?),
        })
    }
}

fn process_transform_and_parent_updates(
    mut commands: Commands,
    mut scenes: Query<(
        Entity,
        &mut SceneContext,
        &mut CrdtLWWState<TransformAndParent>,
    )>,
) {
    for (root, mut entity_map, mut updates) in scenes.iter_mut() {
        // remove crdt state for dead entities
        updates
            .timestamps
            .retain(|ent, _| !entity_map.is_dead(*ent));

        for (scene_entity, value) in updates.values.drain() {
            debug!(
                "[{:?}] {} -> {:?}",
                scene_entity,
                std::any::type_name::<TransformAndParent>(),
                value
            );
            let Some(entity) = entity_map.bevy_entity(scene_entity) else {
                info!("skipping {} update for missing entity {:?}", std::any::type_name::<TransformAndParent>(), scene_entity);
                continue;
            };
            match value {
                Some(TransformAndParent { transform, parent }) => {
                    let parent = match entity_map.bevy_entity(parent) {
                        Some(parent) => parent,
                        None => {
                            // we are parented to something that doesn't yet exist, create it here
                            // TODO abstract out the new entity code (duplicated from process_lifecycle)
                            let new_entity = commands
                                .spawn(SpatialBundle::default())
                                .set_parent(root)
                                .id();
                            entity_map.live.insert(parent, new_entity);
                            new_entity
                        }
                    };
                    commands.entity(entity).insert(transform).set_parent(parent);
                }
                None => {
                    // insert a default transform and reparent to the root
                    commands
                        .entity(entity)
                        .insert(Transform::default())
                        .set_parent(root);
                }
            };
        }
    }
}
