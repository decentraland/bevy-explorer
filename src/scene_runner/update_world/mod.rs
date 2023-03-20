use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use bevy::prelude::*;

use crate::{
    dcl::{
        crdt::lww::CrdtLWWState,
        interface::{lww_interface::CrdtLWWInterface, CrdtComponentInterfaces},
    },
    dcl_component::{
        transform_and_parent::DclTransformAndParent, DclReader, FromDclReader, SceneComponentId,
    },
};

use self::transform_and_parent::process_transform_and_parent_updates;

use super::{DeletedSceneEntities, RendererSceneContext, SceneLoopSchedule, SceneLoopSets};

pub mod transform_and_parent;

#[derive(Component, Default)]
pub struct CrdtLWWStateComponent<T>(pub CrdtLWWState<T>);

impl<T> DerefMut for CrdtLWWStateComponent<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> Deref for CrdtLWWStateComponent<T> {
    type Target = CrdtLWWState<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// plugin to manage some commands from the scene script
pub struct SceneOutputPlugin;

impl Plugin for SceneOutputPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_interface::<DclTransformAndParent>(SceneComponentId(1));
        app.world
            .resource_mut::<SceneLoopSchedule>()
            .schedule
            .add_system(process_transform_and_parent_updates.in_set(SceneLoopSets::UpdateWorld));
    }
}

// a helper to automatically apply engine component updates
pub trait AddCrdtInterfaceExt {
    fn add_crdt_lww_interface<T: FromDclReader>(&mut self, id: SceneComponentId);

    fn add_crdt_lww_component<T: FromDclReader + Component + std::fmt::Debug>(
        &mut self,
        id: SceneComponentId,
    );
}

impl AddCrdtInterfaceExt for App {
    fn add_crdt_lww_interface<T: FromDclReader>(&mut self, id: SceneComponentId) {
        // store a writer
        let mut res = self.world.resource_mut::<CrdtComponentInterfaces>();
        let inner = std::mem::take(&mut res.0);
        let Ok(mut inner) = Arc::try_unwrap(inner) else { panic!() };
        inner.insert(id, Box::<CrdtLWWInterface<T>>::default());
        res.0 = Arc::new(inner);
    }

    fn add_crdt_lww_component<T: FromDclReader + Component + std::fmt::Debug>(
        &mut self,
        id: SceneComponentId,
    ) {
        self.add_crdt_lww_interface::<T>(id);
        // add a system to process the update
        self.world
            .resource_mut::<SceneLoopSchedule>()
            .schedule
            .add_system(process_crdt_lww_updates::<T>.in_set(SceneLoopSets::UpdateWorld));
    }
}

// a default system for processing LWW comonent updates
pub(crate) fn process_crdt_lww_updates<T: FromDclReader + Component + std::fmt::Debug>(
    mut commands: Commands,
    mut scenes: Query<(
        Entity,
        &RendererSceneContext,
        &mut CrdtLWWStateComponent<T>,
        &DeletedSceneEntities,
    )>,
) {
    for (_root, scene_context, mut updates, deleted_entities) in scenes.iter_mut() {
        // remove crdt state for dead entities
        for deleted in &deleted_entities.0 {
            updates.last_write.remove(deleted);
        }

        for (scene_entity, entry) in updates.last_write.iter_mut() {
            let Some(entity) = scene_context.bevy_entity(*scene_entity) else {
                warn!("skipping {} update for missing entity {:?}", std::any::type_name::<T>(), scene_entity);
                continue;
            };
            if entry.is_some {
                match T::from_reader(&mut DclReader::new(&entry.data)) {
                    Ok(t) => {
                        debug!(
                            "[{:?}] {} -> {:?}",
                            scene_entity,
                            std::any::type_name::<T>(),
                            t
                        );
                        commands.entity(entity).insert(t);
                    }
                    Err(e) => {
                        warn!(
                            "failed to deserialize {} from buffer: {:?}",
                            std::any::type_name::<T>(),
                            e
                        );
                    }
                };
            } else {
                commands.entity(entity).remove::<T>();
            }
        }
    }
}
