use bevy::{
    prelude::*,
    transform::systems::{propagate_transforms, sync_simple_transforms},
    utils::{Entry, HashMap, HashSet},
};
use dcl::{crdt::lww::CrdtLWWState, interface::ComponentPosition};

use crate::{
    primary_entities::PrimaryEntities, DeletedSceneEntities, RendererSceneContext, SceneEntity,
    SceneLoopSchedule, TargetParent,
};
use common::{rpc::RpcCall, sets::SceneLoopSets};
use dcl_component::{
    transform_and_parent::DclTransformAndParent, DclReader, FromDclReader, SceneComponentId,
    SceneEntityId,
};

use super::{AddCrdtInterfaceExt, CrdtStateComponent};

pub struct TransformAndParentPlugin;

impl Plugin for TransformAndParentPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_interface::<DclTransformAndParent>(
            SceneComponentId::TRANSFORM,
            ComponentPosition::EntityOnly,
        );
        app.world
            .resource_mut::<SceneLoopSchedule>()
            .schedule
            .add_systems(process_transform_and_parent_updates.in_set(SceneLoopSets::UpdateWorld));
        app.add_systems(
            PostUpdate,
            parent_position_sync
                .in_set(bevy::transform::TransformSystem::TransformPropagate)
                .before(sync_simple_transforms)
                .before(propagate_transforms),
        );
    }
}

#[allow(clippy::type_complexity)]
pub(crate) fn process_transform_and_parent_updates(
    mut commands: Commands,
    mut scenes: Query<(
        Entity,
        &mut RendererSceneContext,
        &mut CrdtStateComponent<CrdtLWWState, DclTransformAndParent>,
        &DeletedSceneEntities,
    )>,
    primaries: PrimaryEntities,
    mut scene_entities: Query<(&mut Transform, &mut TargetParent), With<SceneEntity>>,
    mut restricted_actions: EventWriter<RpcCall>,
) {
    for (root, mut scene_context, mut updates, deleted_entities) in scenes.iter_mut() {
        // remove crdt state for dead entities
        for deleted in &deleted_entities.0 {
            updates.last_write.remove(deleted);
        }

        for (scene_entity, entry) in std::mem::take(&mut updates.last_write) {
            // since transforms are 2-way we have to force update the return crdt with the current scene timestamps
            scene_context
                .crdt_store
                .lww
                .entry(SceneComponentId::TRANSFORM)
                .or_default()
                .update_lww_timestamp(scene_entity, entry.timestamp);

            let (transform, new_target_parent) = if entry.is_some {
                match DclTransformAndParent::from_reader(&mut DclReader::new(&entry.data)) {
                    Ok(dcl_tp) => {
                        debug!(
                            "[{:?}] {} ({:?}) -> {:?}",
                            scene_entity,
                            std::any::type_name::<DclTransformAndParent>(),
                            entry.timestamp,
                            dcl_tp
                        );

                        let new_target_parent = match scene_context.bevy_entity(dcl_tp.parent()) {
                            Some(parent) => parent,
                            None => {
                                if scene_context.is_dead(dcl_tp.parent()) {
                                    // parented to an already dead entity -> parent to root
                                    debug!("set child of dead id {}", dcl_tp.parent());
                                    root
                                } else {
                                    debug!("set child of missing id {}", dcl_tp.parent());
                                    // we are parented to something that doesn't yet exist, create it here
                                    scene_context.spawn_bevy_entity(
                                        &mut commands,
                                        root,
                                        dcl_tp.parent(),
                                        &primaries,
                                    )
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

            match scene_entity {
                SceneEntityId::PLAYER => {
                    restricted_actions.send(RpcCall::MovePlayer {
                        scene: root,
                        to: transform,
                    });
                }
                SceneEntityId::CAMERA => {
                    restricted_actions.send(RpcCall::MoveCamera {
                        scene: root,
                        to: transform.rotation,
                    });
                }
                _ => {
                    // normal scene-space entity
                    let Some(entity) = scene_context.bevy_entity(scene_entity) else {
                        info!(
                            "skipping {} update for missing entity {:?}",
                            std::any::type_name::<DclTransformAndParent>(),
                            scene_entity
                        );
                        continue;
                    };

                    let Ok((mut target_transform, mut target_parent)) =
                        scene_entities.get_mut(entity)
                    else {
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
                            scene_entities
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

// sync an entity's transform with a given target, without blowing the native
// hierarchy so we still catch it when we despawn_recursive the scene, etc.
// since this runs before global hierarchy update we calculate the full target
// transform by walking up the tree of local transforms. this will be slow so
// should only be used with shallow entities ... hands are not very shallow
// so TODO we might want to fully replace the hierarchy propagation at some point.
// also this will lag if the parent of the syncee is moving so they should
// be parented to the scene root generally.
#[derive(Component)]
pub struct ParentPositionSync(pub Entity);

fn parent_position_sync(
    mut syncees: Query<(&mut Transform, &ParentPositionSync, &Parent)>,
    globals: Query<&GlobalTransform>,
    locals: Query<(&Transform, Option<&Parent>), Without<ParentPositionSync>>,
) {
    for (mut transform, sync, parent) in syncees.iter_mut() {
        let Ok(parent_transform) = globals.get(parent.get()) else {
            continue;
        };
        let Ok((sync_transform, maybe_parent)) = locals.get(sync.0) else {
            continue;
        };

        let mut transforms = vec![sync_transform];
        let mut pointer = maybe_parent;
        while let Some(next_parent) = pointer {
            let Ok((next_transform, next_parent)) = locals.get(next_parent.get()) else {
                break;
            };

            transforms.push(next_transform);
            pointer = next_parent;
        }

        let mut final_target = GlobalTransform::default();
        while let Some(next_transform) = transforms.pop() {
            final_target = final_target.mul_transform(*next_transform);
        }

        let (_, final_rotation, final_translation) = final_target.to_scale_rotation_translation();
        *transform = GlobalTransform::from(
            Transform::from_translation(final_translation).with_rotation(final_rotation),
        )
        .reparented_to(parent_transform);
    }
}
