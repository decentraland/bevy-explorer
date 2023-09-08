use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use bevy::{ecs::system::EntityCommands, prelude::*, utils::HashMap};

use common::util::TryInsertEx;
use dcl::{
    crdt::lww::CrdtLWWState,
    interface::{ComponentPosition, CrdtStore, CrdtType},
};
use dcl_component::{DclReader, FromDclReader, SceneComponentId};

use self::{
    animation::AnimatorPlugin, billboard::BillboardPlugin, camera_mode_area::CameraModeAreaPlugin,
    gltf_container::GltfDefinitionPlugin, material::MaterialDefinitionPlugin,
    mesh_collider::MeshColliderPlugin, mesh_renderer::MeshDefinitionPlugin,
    pointer_events::PointerEventsPlugin, raycast::RaycastPlugin, scene_ui::SceneUiPlugin,
    text_shape::TextShapePlugin, transform_and_parent::TransformAndParentPlugin,
    visibility::VisibilityComponentPlugin,
};

use super::{DeletedSceneEntities, RendererSceneContext, SceneLoopSchedule, SceneLoopSets};

pub mod animation;
pub mod billboard;
pub mod camera_mode_area;
pub mod gltf_container;
pub mod material;
pub mod mesh_collider;
pub mod mesh_collider_conversion;
pub mod mesh_renderer;
pub mod pointer_events;
pub mod raycast;
pub mod scene_ui;
pub mod text_shape;
pub mod transform_and_parent;
pub mod visibility;

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
            .map(|state| commands.try_insert(CrdtLWWStateComponent::<T>::new(state)));
    }
}

#[derive(Resource, Default)]
pub struct CrdtExtractors(
    pub HashMap<SceneComponentId, Box<dyn CrdtInterface + Send + Sync + 'static>>,
);

// plugin to manage some commands from the scene script
pub struct SceneOutputPlugin;

#[derive(Resource)]
pub struct NoGltf(pub bool);

impl Plugin for SceneOutputPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TransformAndParentPlugin);
        app.add_plugins(MeshDefinitionPlugin);
        app.add_plugins(MaterialDefinitionPlugin);
        app.add_plugins(MeshColliderPlugin);

        if !app
            .world
            .get_resource::<NoGltf>()
            .map_or(false, |no_gltf| no_gltf.0)
        {
            app.add_plugins(GltfDefinitionPlugin);
        }
        app.add_plugins(AnimatorPlugin);
        app.add_plugins(BillboardPlugin);
        app.add_plugins(RaycastPlugin);
        app.add_plugins(PointerEventsPlugin);
        app.add_plugins(SceneUiPlugin);
        app.add_plugins(TextShapePlugin);
        app.add_plugins(CameraModeAreaPlugin);
        app.add_plugins(VisibilityComponentPlugin);
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
        let existing = self.world.resource_mut::<CrdtExtractors>().0.insert(
            id,
            Box::new(CrdtLWWInterface::<D> {
                position,
                _marker: PhantomData,
            }),
        );

        assert!(existing.is_none(), "duplicate registration for {id:?}");
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
            .add_systems(process_crdt_lww_updates::<D, C>.in_set(SceneLoopSets::UpdateWorld));
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
                warn!(
                    "skipping {} update for missing entity {:?}",
                    std::any::type_name::<D>(),
                    scene_entity
                );
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
                                commands.entity(entity).try_insert(c);
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
