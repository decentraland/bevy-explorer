use std::{
    collections::{BTreeMap, VecDeque},
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use bevy::{core::FrameCount, ecs::system::EntityCommands, prelude::*, utils::HashMap};

use dcl::{
    crdt::{growonly::CrdtGOState, lww::CrdtLWWState},
    interface::{ComponentPosition, CrdtStore, CrdtType},
};
use dcl_component::{DclReader, FromDclReader, SceneComponentId};
use scene_material::SceneMaterial;

use crate::ContainerEntity;

use self::{
    animation::AnimatorPlugin, avatar_modifier_area::AvatarModifierAreaPlugin,
    billboard::BillboardPlugin, camera_mode_area::CameraModeAreaPlugin,
    gltf_container::GltfDefinitionPlugin, material::MaterialDefinitionPlugin,
    mesh_collider::MeshColliderPlugin, mesh_renderer::MeshDefinitionPlugin,
    pointer_events::PointerEventsPlugin, raycast::RaycastPlugin, scene_ui::SceneUiPlugin,
    text_shape::TextShapePlugin, transform_and_parent::TransformAndParentPlugin,
    visibility::VisibilityComponentPlugin,
};

use super::{DeletedSceneEntities, RendererSceneContext, SceneLoopSchedule, SceneLoopSets};

pub mod animation;
pub mod avatar_modifier_area;
pub mod billboard;
pub mod camera_mode_area;
pub mod gltf_container;
pub mod lights;
pub mod material;
pub mod mesh_collider;
pub mod mesh_renderer;
pub mod pointer_events;
pub mod raycast;
pub mod scene_ui;
pub mod text_shape;
pub mod transform_and_parent;
pub mod visibility;

#[derive(Component, Default)]
pub struct CrdtStateComponent<C, T> {
    pub state: C,
    _marker: PhantomData<T>,
}

impl<C, T> DerefMut for CrdtStateComponent<C, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

impl<C, T> Deref for CrdtStateComponent<C, T> {
    type Target = C;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl<C, T> CrdtStateComponent<C, T> {
    pub fn new(state: C) -> Self {
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
            .map(|state| commands.try_insert(CrdtStateComponent::<CrdtLWWState, T>::new(state)));
    }
}

pub struct CrdtGOInterface<T: FromDclReader> {
    position: ComponentPosition,
    _marker: PhantomData<T>,
}

impl<T: FromDclReader> CrdtInterface for CrdtGOInterface<T> {
    fn crdt_type(&self) -> CrdtType {
        CrdtType::GO(self.position)
    }

    fn updates_to_entity(
        &self,
        component_id: SceneComponentId,
        type_map: &mut CrdtStore,
        commands: &mut EntityCommands,
    ) {
        type_map
            .go
            .remove(&component_id)
            .map(|state| commands.try_insert(CrdtStateComponent::<CrdtGOState, T>::new(state)));
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

#[derive(Resource, Default)]
pub struct TrackComponents(pub bool);

impl Plugin for SceneOutputPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TransformAndParentPlugin);
        app.add_plugins(MeshDefinitionPlugin);
        app.add_plugins(MaterialDefinitionPlugin);
        app.add_plugins(MeshColliderPlugin);

        if !app
            .world()
            .get_resource::<NoGltf>()
            .is_some_and(|no_gltf| no_gltf.0)
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
        app.add_plugins(AvatarModifierAreaPlugin);

        app.init_resource::<TrackComponents>();

        app.add_systems(
            PostUpdate,
            track_components::<Handle<Mesh>, true>.run_if(|track: Res<TrackComponents>| track.0),
        );
        app.add_systems(
            PostUpdate,
            track_components::<Handle<SceneMaterial>, true>
                .run_if(|track: Res<TrackComponents>| track.0),
        );
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

    fn add_crdt_go_component<
        D: FromDclReader + std::fmt::Debug,
        C: Component + DerefMut<Target = VecDeque<D>> + Default,
    >(
        &mut self,
        id: SceneComponentId,
        position: ComponentPosition,
    );
}

impl AddCrdtInterfaceExt for App {
    fn add_crdt_lww_interface<D: FromDclReader>(
        &mut self,
        id: SceneComponentId,
        position: ComponentPosition,
    ) {
        // store a writer
        let existing = self.world_mut().resource_mut::<CrdtExtractors>().0.insert(
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
        self.world_mut()
            .resource_mut::<SceneLoopSchedule>()
            .schedule
            .add_systems(process_crdt_lww_updates::<D, C>.in_set(SceneLoopSets::UpdateWorld));
        // add a tracker system
        self.add_systems(
            PostUpdate,
            track_components::<C, false>.run_if(|track: Res<TrackComponents>| track.0),
        );
    }

    fn add_crdt_go_component<
        D: FromDclReader + std::fmt::Debug,
        C: Component + DerefMut<Target = VecDeque<D>> + Default,
    >(
        &mut self,
        id: SceneComponentId,
        position: ComponentPosition,
    ) {
        // store a writer
        let existing = self.world_mut().resource_mut::<CrdtExtractors>().0.insert(
            id,
            Box::new(CrdtGOInterface::<D> {
                position,
                _marker: PhantomData,
            }),
        );

        assert!(existing.is_none(), "duplicate registration for {id:?}");

        self.world_mut()
            .resource_mut::<SceneLoopSchedule>()
            .schedule
            .add_systems(process_crdt_go_updates::<D, C>.in_set(SceneLoopSets::UpdateWorld));
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
        &mut CrdtStateComponent<CrdtLWWState, D>,
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

fn process_crdt_go_updates<
    D: FromDclReader + std::fmt::Debug,
    C: Component + DerefMut<Target = VecDeque<D>> + Default,
>(
    mut commands: Commands,
    mut scenes: Query<(
        Entity,
        &RendererSceneContext,
        &mut CrdtStateComponent<CrdtGOState, D>,
        &DeletedSceneEntities,
    )>,
    mut existing: Query<&mut C>,
) {
    for (_root, scene_context, mut updates, deleted_entities) in scenes.iter_mut() {
        // remove crdt state for dead entities
        for deleted in &deleted_entities.0 {
            updates.0.remove(deleted);
        }

        for (scene_entity, entries) in std::mem::take(&mut updates.0) {
            let Some(entity) = scene_context.bevy_entity(scene_entity) else {
                warn!(
                    "skipping {} update for missing entity {:?}",
                    std::any::type_name::<D>(),
                    scene_entity
                );
                continue;
            };

            let mut new = C::default();
            let mut target = &mut new;
            let mut exists = false;

            if let Ok(existing) = existing.get_mut(entity) {
                target = existing.into_inner();
                exists = true;
            }

            for entry in entries {
                match D::from_reader(&mut DclReader::new(&entry.data)) {
                    Ok(d) => target.push_back(d),
                    Err(e) => warn!(
                        "failed to deserialize {} from buffer: {:?}",
                        std::any::type_name::<D>(),
                        e
                    ),
                }
            }

            if !exists {
                commands.entity(entity).try_insert(new);
            }
        }
    }
}

#[derive(Component, Default, Debug)]
pub struct ComponentTracker(pub BTreeMap<&'static str, usize>);

pub fn track_components<C: Component, const ALLOW_UNALLOCATED: bool>(
    q: Query<Option<&ContainerEntity>, With<C>>,
    mut track: Query<(Entity, &mut ComponentTracker)>,
    frame: Res<FrameCount>,
) {
    if frame.0 % 100 != 0 {
        return;
    }

    let mut counts = HashMap::default();

    for container in q.iter() {
        let Some(container) = container else {
            if !ALLOW_UNALLOCATED {
                warn!("no container with {:?}", std::any::type_name::<C>());
            }
            continue;
        };
        *counts.entry(container.root).or_default() += 1;
    }

    for (ent, mut track) in track.iter_mut() {
        track.0.insert(
            std::any::type_name::<C>(),
            counts.remove(&ent).unwrap_or_default(),
        );
    }

    for (component, count) in counts.drain() {
        if !ALLOW_UNALLOCATED {
            warn!("{:?} unallocated {}", component, count);
        }
    }
}
