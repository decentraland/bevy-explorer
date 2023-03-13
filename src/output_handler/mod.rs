use bevy::prelude::*;

use crate::{
    crdt::{lww::CrdtLWWState, AddCrdtInterfaceExt},
    dcl_component::{
        transform_and_parent::DclTransformAndParent, DclReader, FromDclReader, SceneComponentId,
    },
    scene_runner::{DeletedSceneEntities, SceneContext, SceneEntity, SceneSets},
};

// plugin to manage some commands from the scene script
pub struct SceneOutputPlugin;

impl Plugin for SceneOutputPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_interface::<DclTransformAndParent>(SceneComponentId(1));
        app.add_system(process_transform_and_parent_updates.in_set(SceneSets::HandleOutput));
    }
}

fn process_transform_and_parent_updates(
    mut commands: Commands,
    mut scenes: Query<(
        Entity,
        &mut SceneContext,
        &mut CrdtLWWState<DclTransformAndParent>,
        Option<&DeletedSceneEntities>,
    )>,
) {
    for (root, mut scene_context, mut updates, maybe_deleted) in scenes.iter_mut() {
        if let Some(deleted_entities) = maybe_deleted {
            // remove crdt state for dead entities
            for deleted in &deleted_entities.0 {
                updates.last_write.remove(deleted);
            }
        }

        for (scene_entity, entry) in updates
            .last_write
            .iter_mut()
            .filter(|(_, entry)| entry.updated)
        {
            let Some(entity) = scene_context.bevy_entity(*scene_entity) else {
                info!("skipping {} update for missing entity {:?}", std::any::type_name::<DclTransformAndParent>(), scene_entity);
                continue;
            };
            if entry.is_some {
                match DclTransformAndParent::from_reader(&mut DclReader::new(&entry.data)) {
                    Ok(dcl_tp) => {
                        debug!(
                            "[{:?}] {} -> {:?}",
                            scene_entity,
                            std::any::type_name::<DclTransformAndParent>(),
                            dcl_tp
                        );

                        let transform = dcl_tp.to_bevy_transform();

                        let bevy_parent = match scene_context.bevy_entity(dcl_tp.parent()) {
                            Some(parent) => parent,
                            None => {
                                if scene_context.is_dead(dcl_tp.parent()) {
                                    // parented to an already dead entity -> parent to root
                                    root
                                } else {
                                    // we are parented to something that doesn't yet exist, create it here
                                    // TODO abstract out the new entity code (duplicated from process_lifecycle)
                                    let new_entity = commands
                                        .spawn((
                                            SpatialBundle::default(),
                                            SceneEntity {
                                                root,
                                                scene_id: dcl_tp.parent(),
                                            },
                                        ))
                                        .set_parent(root)
                                        .id();
                                    scene_context
                                        .associate_bevy_entity(dcl_tp.parent(), new_entity);
                                    new_entity
                                }
                            }
                        };

                        commands
                            .entity(entity)
                            .insert(transform)
                            .set_parent(bevy_parent); // TODO: consider checking if parent has changed
                    }
                    Err(e) => {
                        warn!(
                            "failed to deserialize {} from buffer: {:?}",
                            std::any::type_name::<DclTransformAndParent>(),
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
