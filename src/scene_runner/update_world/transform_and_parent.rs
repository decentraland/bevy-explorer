use bevy::{
    prelude::*,
    utils::{Entry, HashMap, HashSet},
};

use crate::{
    dcl_component::{transform_and_parent::DclTransformAndParent, DclReader, FromDclReader},
    scene_runner::{
        ContainerEntity, DeletedSceneEntities, RendererSceneContext, SceneEntity, TargetParent,
    },
};

use super::CrdtLWWStateComponent;

pub(crate) fn process_transform_and_parent_updates(
    mut commands: Commands,
    mut scenes: Query<(
        Entity,
        &mut RendererSceneContext,
        &mut CrdtLWWStateComponent<DclTransformAndParent>,
        &DeletedSceneEntities,
    )>,
    mut entities: Query<(&mut Transform, &mut TargetParent), With<SceneEntity>>,
) {
    for (root, mut scene_context, mut updates, deleted_entities) in scenes.iter_mut() {
        // remove crdt state for dead entities
        for deleted in &deleted_entities.0 {
            updates.last_write.remove(deleted);
        }

        for (scene_entity, entry) in std::mem::take(&mut updates.last_write) {
            let Some(entity) = scene_context.bevy_entity(scene_entity) else {
                info!("skipping {} update for missing entity {:?}", std::any::type_name::<DclTransformAndParent>(), scene_entity);
                continue;
            };

            let (transform, new_target_parent) = if entry.is_some {
                match DclTransformAndParent::from_reader(&mut DclReader::new(&entry.data)) {
                    Ok(dcl_tp) => {
                        debug!(
                            "[{:?}] {} -> {:?}",
                            scene_entity,
                            std::any::type_name::<DclTransformAndParent>(),
                            dcl_tp
                        );

                        let new_target_parent = match scene_context.bevy_entity(dcl_tp.parent()) {
                            Some(parent) => parent,
                            None => {
                                if scene_context.is_dead(dcl_tp.parent()) {
                                    // parented to an already dead entity -> parent to root
                                    root
                                } else {
                                    // we are parented to something that doesn't yet exist, create it here
                                    // TODO abstract out the new entity code (duplicated from process_lifecycle)
                                    // TODO alternatively make new target an option and leave this unparented,
                                    // then try to look up the entity in the tree walk
                                    let new_entity = commands
                                        .spawn((
                                            SpatialBundle::default(),
                                            SceneEntity {
                                                scene_id: scene_context.scene_id,
                                                root,
                                                id: dcl_tp.parent(),
                                            },
                                            TargetParent(root),
                                        ))
                                        .set_parent(root)
                                        .id();
                                    commands.entity(new_entity).insert(ContainerEntity {
                                        root,
                                        container: new_entity,
                                        container_id: dcl_tp.parent(),
                                    });
                                    scene_context
                                        .associate_bevy_entity(dcl_tp.parent(), new_entity);
                                    new_entity
                                }
                            }
                        };

                        (dcl_tp.to_bevy_transform(), new_target_parent)
                    }
                    Err(e) => {
                        warn!(
                            "failed to deserialize {} from buffer: {:?}",
                            std::any::type_name::<DclTransformAndParent>(),
                            e
                        );
                        continue;
                    }
                }
            } else {
                (Transform::default(), root)
            };

            let Ok((mut target_transform, mut target_parent)) = entities.get_mut(entity) else {
                warn!("failed to find entity for transform update?!");
                continue;
            };
            *target_transform = transform;
            if new_target_parent != target_parent.0 {
                // update the target
                target_parent.0 = new_target_parent;
                // mark the entity as needing hierarchy check
                scene_context.unparented_entities.insert(entity);
                // mark the scene so hierarchy checking is performed
                scene_context.hierarchy_changed = true;
            }
        }
    }

    for (root, mut scene, ..) in scenes.iter_mut() {
        if scene.hierarchy_changed {
            scene.hierarchy_changed = false;

            // hashmap for parent lookup to avoid reusing query
            let mut parents = HashMap::default();

            // entities that we know connect ultimately to the root
            let mut valid_entities = HashSet::from_iter(std::iter::once(root));
            // entities that we know are part of a cycle (or lead to a cycle)
            let mut invalid_entities = HashSet::default();

            scene.unparented_entities.retain(|entity| {
                // entities in the current chain
                let mut checklist = HashSet::default();

                // walk until we reach a known valid/invalid entity or our starting point
                let mut pointer = *entity;
                while ![&valid_entities, &invalid_entities, &checklist]
                    .iter()
                    .any(|set| set.contains(&pointer))
                {
                    checklist.insert(pointer);
                    let parent = match parents.entry(pointer) {
                        Entry::Occupied(o) => o.into_mut(),
                        Entry::Vacant(v) => v.insert(
                            entities
                                .get(pointer)
                                .map(|(_, target_parent)| target_parent.0)
                                .unwrap_or(root),
                        ),
                    };
                    pointer = *parent;
                }

                if valid_entities.contains(&pointer) {
                    debug!(
                        "{:?}: valid, setting parent to {:?}",
                        entity, parents[entity]
                    );
                    // this entity (and all checked entities) link to the root
                    // apply parenting
                    commands.entity(*entity).set_parent(parents[entity]);
                    //  record validity of the chain
                    valid_entities.extend(checklist.into_iter());
                    // remove from the unparented list
                    false
                } else {
                    debug!("{:?}: not valid, setting parent to {:?}", entity, root);
                    // this entity (and all checked entities) end in a cycle
                    // parent to the root
                    commands.entity(*entity).set_parent(root);
                    // mark as invalid
                    invalid_entities.extend(checklist.into_iter());
                    // keep the entity in the unparented list to recheck at the next hierarchy update
                    true
                }
            });
        }
    }
}
