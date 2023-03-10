use bevy::prelude::*;

use crate::{
    crdt::{lww::CrdtLWWState, AddCrdtInterfaceExt},
    dcl_component::{
        transform_and_parent::DclTransformAndParent, DclReader, FromDclReader, SceneComponentId,
    },
    scene_runner::{SceneContext, SceneSets},
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

                        let bevy_parent = match entity_map.bevy_entity(dcl_tp.parent()) {
                            Some(parent) => parent,
                            None => {
                                // we are parented to something that doesn't yet exist, create it here
                                // TODO abstract out the new entity code (duplicated from process_lifecycle)
                                let new_entity = commands
                                    .spawn(SpatialBundle::default())
                                    .set_parent(root)
                                    .id();
                                entity_map.live.insert(dcl_tp.parent(), new_entity);
                                new_entity
                            }
                        };

                        commands
                            .entity(entity)
                            .insert(transform)
                            .set_parent(bevy_parent);
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
