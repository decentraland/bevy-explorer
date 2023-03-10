use bevy::prelude::*;

use crate::scene_runner::{
    crdt::{lww::CrdtLWWState, AddCrdtInterfaceExt, FromDclReader},
    engine::{DclReader, DclReaderError},
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

impl FromDclReader for TransformAndParent {
    fn from_proto(buf: &mut DclReader) -> Result<Self, DclReaderError> {
        Ok(TransformAndParent {
            transform: Transform {
                translation: Vec3::new(buf.read_float()?, buf.read_float()?, buf.read_float()?),
                rotation: Quat::from_xyzw(
                    buf.read_float()?,
                    buf.read_float()?,
                    buf.read_float()?,
                    buf.read_float()?,
                ),
                scale: Vec3::new(buf.read_float()?, buf.read_float()?, buf.read_float()?),
            },
            parent: SceneEntityId(buf.read_u32()?),
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
            .last_write
            .retain(|ent, _| !entity_map.is_dead(*ent));

        for (scene_entity, entry) in updates
            .last_write
            .iter_mut()
            .filter(|(_, entry)| entry.updated)
        {
            let Some(entity) = entity_map.bevy_entity(*scene_entity) else {
                info!("skipping {} update for missing entity {:?}", std::any::type_name::<TransformAndParent>(), scene_entity);
                continue;
            };
            if entry.is_some {
                match TransformAndParent::from_proto(&mut DclReader::new(&entry.data)) {
                    Ok(tp) => {
                        debug!(
                            "[{:?}] {} -> {:?}",
                            scene_entity,
                            std::any::type_name::<TransformAndParent>(),
                            tp
                        );

                        let parent = match entity_map.bevy_entity(tp.parent) {
                            Some(parent) => parent,
                            None => {
                                // we are parented to something that doesn't yet exist, create it here
                                // TODO abstract out the new entity code (duplicated from process_lifecycle)
                                let new_entity = commands
                                    .spawn(SpatialBundle::default())
                                    .set_parent(root)
                                    .id();
                                entity_map.live.insert(tp.parent, new_entity);
                                new_entity
                            }
                        };

                        commands
                            .entity(entity)
                            .insert(tp.transform)
                            .set_parent(parent);
                    }
                    Err(e) => {
                        warn!(
                            "failed to deserialize {} from buffer: {:?}",
                            std::any::type_name::<TransformAndParent>(),
                            e
                        );
                    }
                }
            } else {
                // insert a default transform and reparent to the root
                commands
                    .entity(entity)
                    .insert(Transform::default())
                    .set_parent(root);
            }
        }
    }
}
