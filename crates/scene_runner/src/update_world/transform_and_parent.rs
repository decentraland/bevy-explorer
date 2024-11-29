use std::marker::PhantomData;

use bevy::{
    ecs::system::SystemParam,
    prelude::*,
    transform::TransformSystem,
    utils::{Entry, HashMap, HashSet},
};
use common::{anim_last_system, util::ModifyComponentExt};
use dcl::{crdt::lww::CrdtLWWState, interface::ComponentPosition};

use crate::{
    primary_entities::PrimaryEntities, DeletedSceneEntities, RendererSceneContext, SceneEntity,
    SceneLoopSchedule, TargetParent,
};
use common::sets::SceneLoopSets;
use dcl_component::{
    transform_and_parent::DclTransformAndParent, DclReader, FromDclReader, SceneComponentId,
    SceneEntityId,
};

use super::{gltf_container::GltfLinkSet, AddCrdtInterfaceExt, CrdtStateComponent};

pub struct TransformAndParentPlugin;

impl Plugin for TransformAndParentPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_interface::<DclTransformAndParent>(
            SceneComponentId::TRANSFORM,
            ComponentPosition::EntityOnly,
        );
        app.world_mut()
            .resource_mut::<SceneLoopSchedule>()
            .schedule
            .add_systems(process_transform_and_parent_updates.in_set(SceneLoopSets::UpdateWorld));
        app.add_systems(
            PostUpdate,
            (
                parent_position_sync::<AvatarAttachStage>
                    .after(anim_last_system!())
                    .after(GltfLinkSet)
                    .before(TransformSystem::TransformPropagate),
                parent_position_sync::<SceneProxyStage>
                    .after(anim_last_system!())
                    .after(GltfLinkSet)
                    .after(parent_position_sync::<AvatarAttachStage>)
                    .before(TransformSystem::TransformPropagate),
            ),
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
    // mut restricted_actions: EventWriter<RpcCall>,
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
                    // scenes just modify the player transform for fun, so we can't do this...

                    // restricted_actions.send(RpcCall::MovePlayer {
                    //     scene: root,
                    //     to: transform,
                    // });
                }
                // SceneEntityId::CAMERA => {
                // restricted_actions.send(RpcCall::MoveCamera {
                //     scene: root,
                //     to: transform.rotation,
                // });
                // }
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
pub struct ParentPositionSync<T: ParentPositionSyncStage>(pub Entity, PhantomData<fn() -> T>);

impl<T: ParentPositionSyncStage> ParentPositionSync<T> {
    pub fn new(parent: Entity) -> Self {
        Self(parent, Default::default())
    }
}

pub trait ParentPositionSyncStage: 'static {}

pub struct AvatarAttachStage;
impl ParentPositionSyncStage for AvatarAttachStage {}

pub struct SceneProxyStage;
impl ParentPositionSyncStage for SceneProxyStage {}

pub fn parent_position_sync<T: ParentPositionSyncStage>(
    mut commands: Commands,
    syncees: Query<(Entity, &ParentPositionSync<T>, &Parent)>,
    globals: Query<&GlobalTransform>,
    gt_helper: TransformHelperPub,
) {
    for (ent, sync, parent) in syncees.iter() {
        let Ok(parent_transform) = globals.get(parent.get()) else {
            continue;
        };

        let Ok(gt) = gt_helper.compute_global_transform(sync.0, None) else {
            continue;
        };

        let transform = gt.reparented_to(parent_transform);

        commands
            .entity(ent)
            .modify_component(move |t: &mut Transform| *t = transform.with_scale(t.scale));
    }
}

/// System parameter for computing up-to-date [`GlobalTransform`]s.
///
/// Computing an entity's [`GlobalTransform`] can be expensive so it is recommended
/// you use the [`GlobalTransform`] component stored on the entity, unless you need
/// a [`GlobalTransform`] that reflects the changes made to any [`Transform`]s since
/// the last time the transform propagation systems ran.
#[derive(SystemParam)]
pub struct TransformHelperPub<'w, 's> {
    pub parent_query: Query<'w, 's, &'static Parent>,
    pub transform_query: Query<'w, 's, &'static Transform>,
}

impl TransformHelperPub<'_, '_> {
    /// Computes the [`GlobalTransform`] of the given entity from the [`Transform`] component on it and its ancestors.
    pub fn compute_global_transform(
        &self,
        entity: Entity,
        up_to: Option<Entity>,
    ) -> Result<GlobalTransform, anyhow::Error> {
        if up_to == Some(entity) {
            return Ok(GlobalTransform::IDENTITY);
        }

        let transform = self.transform_query.get(entity)?;

        let mut global_transform = GlobalTransform::from(*transform);

        for entity in self.parent_query.iter_ancestors(entity) {
            if Some(entity) == up_to {
                return Ok(global_transform);
            }

            let transform = self.transform_query.get(entity)?;
            global_transform = *transform * global_transform;
        }

        Ok(global_transform)
    }

    pub fn compute_global_transform_with_overrides(
        &self,
        entity: Entity,
        up_to: Option<Entity>,
        overrides: &HashMap<Entity, Transform>,
    ) -> Result<GlobalTransform, anyhow::Error> {
        if up_to == Some(entity) {
            return Ok(GlobalTransform::IDENTITY);
        }

        let transform = overrides
            .get(&entity)
            .unwrap_or(self.transform_query.get(entity)?);

        let mut global_transform = GlobalTransform::from(*transform);

        for entity in self.parent_query.iter_ancestors(entity) {
            if Some(entity) == up_to {
                return Ok(global_transform);
            }

            let transform = overrides
                .get(&entity)
                .unwrap_or(self.transform_query.get(entity)?);
            global_transform = *transform * global_transform;
        }

        Ok(global_transform)
    }

    /// Computes the [`GlobalTransform`] of the given entity from the [`Transform`] component on it and its ancestors.
    pub fn compute_global_transform_with_ancestors(
        &self,
        entity: Entity,
        up_to: Option<Entity>,
    ) -> Result<(GlobalTransform, Vec<Entity>), anyhow::Error> {
        if up_to == Some(entity) {
            return Ok((GlobalTransform::IDENTITY, Vec::default()));
        }

        let transform = self.transform_query.get(entity)?;
        let mut ancestors = Vec::default();

        let mut global_transform = GlobalTransform::from(*transform);

        for entity in self.parent_query.iter_ancestors(entity) {
            if Some(entity) == up_to {
                return Ok((global_transform, ancestors));
            }

            let transform = self.transform_query.get(entity)?;
            global_transform = *transform * global_transform;
            ancestors.push(entity);
        }

        Ok((global_transform, ancestors))
    }
}
