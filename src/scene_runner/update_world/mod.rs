use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use bevy::{ecs::system::EntityCommands, prelude::*, utils::HashMap};

use crate::{
    dcl::{
        crdt::lww::CrdtLWWState,
        interface::{ComponentPosition, CrdtStore, CrdtType},
    },
    dcl_component::{
        transform_and_parent::DclTransformAndParent, DclReader, FromDclReader, SceneComponentId,
    },
};

use self::{
    billboard::BillboardPlugin, mesh_collider::MeshColliderPlugin,
    mesh_renderer::MeshDefinitionPlugin, raycast::RaycastPlugin,
    transform_and_parent::process_transform_and_parent_updates,
};

use super::{DeletedSceneEntities, RendererSceneContext, SceneLoopSchedule, SceneLoopSets};

pub mod billboard;
pub mod mesh_collider;
pub mod mesh_renderer;
pub mod raycast;
pub mod transform_and_parent;

#[derive(Component, Default)]
pub struct CrdtLWWStateComponent<T> {
    pub state: CrdtLWWState,
    _marker: PhantomData<T>,
}

impl<T> DerefMut for CrdtLWWStateComponent<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

impl<T> Deref for CrdtLWWStateComponent<T> {
    type Target = CrdtLWWState;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl<T> CrdtLWWStateComponent<T> {
    pub fn new(state: CrdtLWWState) -> Self {
        Self {
            state,
            _marker: PhantomData,
        }
    }
}

// trait for enacpsulating the processing of a crdt message
pub trait CrdtInterface {
    fn crdt_type(&self) -> CrdtType;

    // push updates onto a bevy entity
    fn updates_to_entity(
        &self,
        component_id: SceneComponentId,
        type_map: &mut CrdtStore,
        commands: &mut EntityCommands,
    );
}

pub struct CrdtLWWInterface<T: FromDclReader> {
    position: ComponentPosition,
    _marker: PhantomData<T>,
}

impl<T: FromDclReader> CrdtInterface for CrdtLWWInterface<T> {
    fn crdt_type(&self) -> CrdtType {
        CrdtType::LWW(self.position)
    }

    fn updates_to_entity(
        &self,
        component_id: SceneComponentId,
        type_map: &mut CrdtStore,
        commands: &mut EntityCommands,
    ) {
        type_map
            .lww
            .remove(&component_id)
            .map(|state| commands.insert(CrdtLWWStateComponent::<T>::new(state)));
    }
}

#[derive(Resource, Default)]
pub struct CrdtExtractors(
    pub HashMap<SceneComponentId, Box<dyn CrdtInterface + Send + Sync + 'static>>,
);

// plugin to manage some commands from the scene script
pub struct SceneOutputPlugin;

impl Plugin for SceneOutputPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_interface::<DclTransformAndParent>(
            SceneComponentId::TRANSFORM,
            ComponentPosition::EntityOnly,
        );
        app.world
            .resource_mut::<SceneLoopSchedule>()
            .schedule
            .add_system(process_transform_and_parent_updates.in_set(SceneLoopSets::UpdateWorld));

        app.add_plugin(MeshDefinitionPlugin);
        app.add_plugin(MeshColliderPlugin);
        app.add_plugin(BillboardPlugin);
        app.add_plugin(RaycastPlugin);
    }
}

// a helper to automatically apply engine component updates
pub trait AddCrdtInterfaceExt {
    fn add_crdt_lww_interface<D: FromDclReader>(
        &mut self,
        id: SceneComponentId,
        position: ComponentPosition,
    );

    fn add_crdt_lww_component<D: FromDclReader + std::fmt::Debug, C: Component + TryFrom<D>>(
        &mut self,
        id: SceneComponentId,
        position: ComponentPosition,
    ) where
        <C as TryFrom<D>>::Error: std::fmt::Display;
}

impl AddCrdtInterfaceExt for App {
    fn add_crdt_lww_interface<D: FromDclReader>(
        &mut self,
        id: SceneComponentId,
        position: ComponentPosition,
    ) {
        // store a writer
        self.world.resource_mut::<CrdtExtractors>().0.insert(
            id,
            Box::new(CrdtLWWInterface::<D> {
                position,
                _marker: PhantomData,
            }),
        );
    }

    fn add_crdt_lww_component<D: FromDclReader + std::fmt::Debug, C: Component + TryFrom<D>>(
        &mut self,
        id: SceneComponentId,
        position: ComponentPosition,
    ) where
        <C as TryFrom<D>>::Error: std::fmt::Display,
    {
        self.add_crdt_lww_interface::<D>(id, position);
        // add a system to process the update
        self.world
            .resource_mut::<SceneLoopSchedule>()
            .schedule
            .add_system(process_crdt_lww_updates::<D, C>.in_set(SceneLoopSets::UpdateWorld));
    }
}

// a default system for processing LWW comonent updates
pub(crate) fn process_crdt_lww_updates<
    D: FromDclReader + std::fmt::Debug,
    C: Component + TryFrom<D>,
>(
    mut commands: Commands,
    mut scenes: Query<(
        Entity,
        &RendererSceneContext,
        &mut CrdtLWWStateComponent<D>,
        &DeletedSceneEntities,
    )>,
) where
    <C as TryFrom<D>>::Error: std::fmt::Display,
{
    for (_root, scene_context, mut updates, deleted_entities) in scenes.iter_mut() {
        // remove crdt state for dead entities
        for deleted in &deleted_entities.0 {
            updates.last_write.remove(deleted);
        }

        for (scene_entity, entry) in std::mem::take(&mut updates.last_write) {
            let Some(entity) = scene_context.bevy_entity(scene_entity) else {
                warn!("skipping {} update for missing entity {:?}", std::any::type_name::<D>(), scene_entity);
                continue;
            };
            if entry.is_some {
                match D::from_reader(&mut DclReader::new(&entry.data)) {
                    Ok(d) => {
                        debug!(
                            "[{:?}] {} -> {:?}",
                            scene_entity,
                            std::any::type_name::<D>(),
                            d
                        );
                        match C::try_from(d) {
                            Ok(c) => {
                                commands.entity(entity).insert(c);
                            }
                            Err(e) => {
                                warn!(
                                    "Error converting {} to {}: {}",
                                    std::any::type_name::<D>(),
                                    std::any::type_name::<C>(),
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            "failed to deserialize {} from buffer: {:?}",
                            std::any::type_name::<D>(),
                            e
                        );
                    }
                };
            } else {
                commands.entity(entity).remove::<C>();
            }
        }
    }
}
