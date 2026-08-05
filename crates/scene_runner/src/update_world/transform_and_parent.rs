use std::marker::PhantomData;

use bevy::{
    ecs::system::SystemParam,
    platform::{
        collections::{hash_map::Entry, HashMap, HashSet},
        hash::FixedHasher,
    },
    prelude::*,
    transform::systems::{mark_dirty_trees, propagate_parent_transforms, sync_simple_transforms},
};
use common::{anim_last_system, sets::PostUpdateSets, util::ModifyComponentExt};
use dcl::{
    crdt::lww::CrdtLWWState,
    interface::{ComponentPosition, CrdtType},
};

use crate::{
    initialize_scene::process_scene_lifecycle,
    primary_entities::PrimaryEntities,
    update_world::{gltf_container::GltfLinkSet, visibility::VisibilityComponent},
    DeletedSceneEntities, RendererSceneContext, SceneEntity, SceneLoopSchedule, TargetParent,
};
use common::sets::SceneLoopSets;
use dcl_component::{
    transform_and_parent::DclTransformAndParent, DclReader, DclWriter, FromDclReader,
    SceneComponentId, SceneEntityId,
};

use super::{AddCrdtInterfaceExt, CrdtStateComponent};

pub struct TransformAndParentPlugin;

impl Plugin for TransformAndParentPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            PostUpdate,
            (
                PostUpdateSets::EarlyTransformPropagate,
                PostUpdateSets::ColliderUpdate,
                PostUpdateSets::PlayerUpdate,
                PostUpdateSets::CameraUpdate,
                PostUpdateSets::InverseKinematics,
                PostUpdateSets::Nametag,
                PostUpdateSets::AttachSync,
                PostUpdateSets::Billboard,
            )
                .chain()
                .after(GltfLinkSet)
                .after(anim_last_system!())
                .before(TransformSystem::TransformPropagate)
                .before(process_scene_lifecycle),
        );

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
                parent_position_sync::<AvatarAttachStage>,
                parent_position_sync::<SceneProxyStage>,
            )
                .in_set(PostUpdateSets::AttachSync),
        );

        // rerun the entire transform tree update
        // TODO efficiency, either:
        // - make propagate_parent_transforms generic over TransformTreeChanged type?
        // - only update things with colliders below?
        // - manually calculate collider global transforms?
        app.add_systems(
            PostUpdate,
            (
                mark_dirty_trees,
                propagate_parent_transforms,
                sync_simple_transforms,
            )
                .chain()
                .in_set(PostUpdateSets::EarlyTransformPropagate),
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
        // avoid triggering change detection when there is nothing to do
        if updates.last_write.is_empty() && deleted_entities.0.is_empty() {
            continue;
        }

        // remove crdt state for dead entities
        for deleted in &deleted_entities.0 {
            updates.last_write.remove(deleted);
        }

        for (scene_entity, entry) in std::mem::take(&mut updates.last_write) {
            // (formerly synced the return-crdt timestamp from the staged entry here;
            // redundant now that receive's sync_lww_timestamps_from + force_update's
            // current-timestamp bump keep it correct, and harmful for engine-initiated
            // writes whose staged timestamp is a fresh low value)

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
            let mut parents = HashMap::new();

            // entities that we know connect ultimately to the root
            let mut valid_entities: HashSet<_, FixedHasher> =
                HashSet::from_iter(std::iter::once(root));
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
                    commands
                        .entity(*entity)
                        .try_insert(ChildOf(parents[entity]));
                    //  record validity of the chain
                    valid_entities.extend(checklist);
                    // remove from the unparented list
                    false
                } else {
                    debug!("{:?}: not valid, setting parent to {:?}", entity, root);
                    // this entity (and all checked entities) end in a cycle
                    // parent to the root
                    commands.entity(*entity).try_insert(ChildOf(root));
                    // mark as invalid
                    invalid_entities.extend(checklist);
                    // keep the entity in the unparented list to recheck at the next hierarchy update
                    true
                }
            });
        }
    }
}

// sync an entity's transform with a given target, without blowing the native
// hierarchy so we still catch it when we despawn the scene, etc.
// since this runs before global hierarchy update we calculate the full target
// transform by walking up the tree of local transforms. this will be slow so
// should only be used with shallow entities ... hands are not very shallow
// so TODO we might want to fully replace the hierarchy propagation at some point.
// also this will lag if the parent of the syncee is moving so they should
// be parented to the scene root generally.
#[derive(Component)]
pub struct ParentPositionSync<T: ParentPositionSyncStage> {
    pub sync_to: Entity,
    /// when set, the synced transform is also written back to the owning
    /// scene's crdt, relative to this entity (normally the attached player's
    /// root). unity semantics, which the sdk world-transform helpers build on:
    /// rotation is made relative to the target, translation is the unrotated
    /// world-axis delta from the target.
    pub translation_target: Option<Entity>,
    _p: PhantomData<fn() -> T>,
}

impl<T: ParentPositionSyncStage> ParentPositionSync<T> {
    pub fn new(sync_to: Entity) -> Self {
        Self {
            sync_to,
            translation_target: None,
            _p: Default::default(),
        }
    }

    pub fn new_with_scene_writeback(sync_to: Entity, translation_target: Entity) -> Self {
        Self {
            sync_to,
            translation_target: Some(translation_target),
            _p: Default::default(),
        }
    }
}

pub trait ParentPositionSyncStage: 'static {}

pub struct AvatarAttachStage;
impl ParentPositionSyncStage for AvatarAttachStage {}

pub struct SceneProxyStage;
impl ParentPositionSyncStage for SceneProxyStage {}

#[allow(clippy::type_complexity)]
pub fn parent_position_sync<T: ParentPositionSyncStage>(
    mut commands: Commands,
    syncees: Query<(
        Entity,
        &ParentPositionSync<T>,
        &ChildOf,
        Option<&VisibilityComponent>,
        Option<(&SceneEntity, &Transform)>,
    )>,
    globals: Query<&GlobalTransform>,
    gt_helper: TransformHelperPub,
    inherited_visibility: Query<&InheritedVisibility>,
    mut contexts: Query<&mut RendererSceneContext>,
    mut buf: Local<Vec<u8>>,
) {
    for (ent, sync, parent, maybe_explicit_visibility, maybe_scene_entity) in syncees.iter() {
        let Ok(parent_transform) = globals.get(parent.parent()) else {
            continue;
        };

        let Ok(gt) = gt_helper.compute_global_transform(sync.sync_to, None) else {
            continue;
        };

        let transform = gt.reparented_to(parent_transform);

        // write the synced transform back to the owning scene, so it can read
        // the anchor pose. the value is relative to the translation target
        // (the attached player's root), matching unity and the sdk
        // world-transform helpers which compose
        // `player transform * entity transform` for entities with
        // AvatarAttach: rotation is made relative to the target, but the
        // translation delta is deliberately NOT rotated into the target's
        // frame - that's what unity writes, and scenes in the wild correct
        // for exactly that. scale and the declared parent are passed through
        // unchanged.
        if let (Some(target), Some((scene_entity, current_transform))) =
            (sync.translation_target, maybe_scene_entity)
        {
            if let (Ok(target_gt), Ok(mut context)) = (
                gt_helper.compute_global_transform(target, None),
                contexts.get_mut(scene_entity.root),
            ) {
                let (_, anchor_rotation, anchor_translation) = gt.to_scale_rotation_translation();
                let (_, target_rotation, target_translation) =
                    target_gt.to_scale_rotation_translation();

                let parent_id = context
                    .crdt_store
                    .get(
                        SceneComponentId::TRANSFORM,
                        CrdtType::LWW_ENT,
                        scene_entity.id,
                    )
                    .and_then(|data| {
                        DclTransformAndParent::from_reader(&mut DclReader::new(data)).ok()
                    })
                    .map(|t| t.parent)
                    .unwrap_or(SceneEntityId::ROOT);

                let dcl_transform = DclTransformAndParent::from_bevy_transform_and_parent(
                    &Transform {
                        translation: anchor_translation - target_translation,
                        rotation: target_rotation.inverse() * anchor_rotation,
                        scale: current_transform.scale,
                    },
                    parent_id,
                );

                buf.clear();
                DclWriter::new(&mut buf).write(&dcl_transform);
                context.crdt_store.update_if_different(
                    SceneComponentId::TRANSFORM,
                    CrdtType::LWW_ENT,
                    scene_entity.id,
                    Some(&mut DclReader::new(&buf)),
                );
            }
        }
        let maybe_override_visibility = if maybe_explicit_visibility.is_some() {
            None
        } else {
            let inherited_visibility = inherited_visibility.get(sync.sync_to).unwrap();

            Some(match inherited_visibility.get() {
                true => Visibility::Visible,
                false => Visibility::Hidden,
            })
        };

        commands
            .entity(ent)
            .modify_component(move |t: &mut Transform| *t = transform.with_scale(t.scale))
            .modify_component(move |v: &mut Visibility| {
                if let Some(override_visibility) = maybe_override_visibility {
                    *v = override_visibility;
                }
            });
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
    pub parent_query: Query<'w, 's, &'static ChildOf>,
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
