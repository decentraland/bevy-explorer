// TODO
// - gltf collider flags
// - clean up of cached colliders (when mesh is unloaded?)
use std::collections::BTreeMap;

use bevy::{
    core_pipeline::tonemapping::{DebandDither, Tonemapping},
    gltf::{Gltf, GltfExtras},
    prelude::*,
    reflect::{TypePath, TypeUuid},
    render::{
        camera::CameraRenderGraph,
        mesh::{skinning::SkinnedMesh, VertexAttributeValues},
        primitives::Frustum,
        view::{ColorGrading, NoFrustumCulling, VisibleEntities},
    },
    scene::InstanceId,
    tasks::{AsyncComputeTaskPool, Task},
    utils::{HashMap, HashSet},
};
use futures_lite::future;
use rapier3d::{parry::transformation::ConvexHullError, prelude::*};
use serde::Deserialize;

use crate::{renderer_context::RendererSceneContext, ContainerEntity, SceneEntity, SceneSets};
use common::util::TryInsertEx;
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::sdk::components::{
        common::LoadingState, ColliderLayer, PbGltfContainer, PbGltfContainerLoadingState,
    },
    SceneComponentId, SceneEntityId,
};
use ipfs::{EntityDefinition, IpfsLoaderExt};

use super::{
    mesh_collider::{MeshCollider, MeshColliderShape},
    AddCrdtInterfaceExt, mesh_collider_conversion::calculate_mesh_collider,
};

pub struct GltfDefinitionPlugin;

#[derive(Component, Debug)]
pub struct GltfDefinition(PbGltfContainer);

impl From<PbGltfContainer> for GltfDefinition {
    fn from(value: PbGltfContainer) -> Self {
        Self(value)
    }
}

impl Plugin for GltfDefinitionPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbGltfContainer, GltfDefinition>(
            SceneComponentId::GLTF_CONTAINER,
            ComponentPosition::EntityOnly,
        );

        app.add_systems(Update, update_gltf.in_set(SceneSets::PostLoop));
        app.add_systems(
            Update,
            attach_ready_colliders
                .after(update_gltf)
                .in_set(SceneSets::PostLoop),
        );
        app.add_asset::<GltfCachedShape>();
        app.init_resource::<MeshToShape>();
        app.add_systems(Update, check_gltfs_ready.in_set(SceneSets::PostInit));
        app.add_systems(Update, update_container_finished.in_set(SceneSets::Input));
    }
}

#[derive(TypeUuid, TypePath)]
#[uuid = "09e7812e-ea71-4046-a9be-65565257d459"]
pub enum GltfCachedShape {
    Shape(Result<SharedShape, ConvexHullError>),
    Task(Task<Result<SharedShape, ConvexHullError>>),
}

#[derive(Component)]
pub struct PendingGltfCollider {
    h_shape: Handle<GltfCachedShape>,
    h_mesh: Handle<Mesh>,
    collision_mask: u32,
    mesh_name: Option<String>,
    index: u32,
}

#[derive(Component, Debug)]
pub struct GltfEntity {
    pub container_id: SceneEntityId,
}

#[derive(Component)]
struct GltfLoaded(Option<InstanceId>);
#[derive(Component, Default)]
pub struct GltfProcessed {
    pub instance_id: Option<InstanceId>,
    pub animation_roots: HashSet<(Entity, Name)>,
}

#[derive(Resource, Default)]
pub struct MeshToShape(HashMap<Handle<Mesh>, Handle<GltfCachedShape>>);

#[derive(Deserialize)]
struct DclNodeExtras {
    dcl_collision_mask: Option<u32>,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_gltf(
    mut commands: Commands,
    new_gltfs: Query<
        (
            Entity,
            &SceneEntity,
            &GltfDefinition,
            Option<&GltfLoaded>,
            Option<&GltfProcessed>,
        ),
        Changed<GltfDefinition>,
    >,
    unprocessed_gltfs: Query<
        (Entity, &SceneEntity, &Handle<Gltf>),
        (With<GltfDefinition>, Without<GltfLoaded>),
    >,
    ready_gltfs: Query<
        (Entity, &SceneEntity, &GltfLoaded, &GltfDefinition),
        Without<GltfProcessed>,
    >,
    gltf_spawned_entities: Query<(
        Option<&Name>,
        &Transform,
        &Parent,
        Option<&AnimationPlayer>,
        Option<&Handle<Mesh>>,
        Option<&GltfExtras>,
        Option<&SkinnedMesh>,
    )>,
    scene_def_handles: Query<&Handle<EntityDefinition>>,
    (scene_defs, asset_server, gltfs): (
        Res<Assets<EntityDefinition>>,
        Res<AssetServer>,
        Res<Assets<Gltf>>,
    ),
    mut scene_spawner: ResMut<SceneSpawner>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cached_shapes: ResMut<Assets<GltfCachedShape>>,
    mut shape_lookup: ResMut<MeshToShape>,
    mut contexts: Query<&mut RendererSceneContext>,
    _debug_query: Query<(
        Entity,
        Option<&Name>,
        Option<&Children>,
        Option<&SkinnedMesh>,
        &Transform,
    )>,
    mut instances_to_despawn_when_ready: Local<Vec<InstanceId>>,
) {
    // clean up old instances
    instances_to_despawn_when_ready.retain(|instance| {
        if scene_spawner.instance_is_ready(*instance) {
            for entity in scene_spawner.iter_instance_entities(*instance) {
                if let Some(mut commands) = commands.get_entity(entity) {
                    // have to do this non-recursively and safely because we may have removed some entities already
                    commands.despawn();
                }
            }
            false
        } else {
            true
        }
    });

    let mut set_state = |scene_ent: &SceneEntity, current_state: LoadingState| {
        if let Ok(mut context) = contexts.get_mut(scene_ent.root) {
            context.update_crdt(
                SceneComponentId::GLTF_CONTAINER_LOADING_STATE,
                CrdtType::LWW_ANY,
                scene_ent.id,
                &PbGltfContainerLoadingState {
                    current_state: current_state as i32,
                },
            );
        };
    };

    for (ent, scene_ent, gltf, maybe_loaded, maybe_processed) in new_gltfs.iter() {
        debug!("{} has {}", scene_ent.id, gltf.0.src);

        if let Some(GltfLoaded(Some(instance))) = maybe_loaded {
            // clean up from loaded state
            // instances_to_despawn_when_ready.push(*instance);
            scene_spawner.despawn_instance(*instance);
        }
        if let Some(GltfProcessed {
            instance_id: Some(instance),
            ..
        }) = maybe_processed
        {
            // clean up from processed state
            instances_to_despawn_when_ready.push(*instance);
        }
        commands
            .entity(ent)
            .remove::<GltfLoaded>()
            .remove::<GltfProcessed>();

        let Ok(h_scene_def) = scene_def_handles.get(scene_ent.root) else {
            warn!("no scene definition found, can't process file request");
            continue;
        };

        let Some(scene_def) = scene_defs.get(h_scene_def) else {
            warn!("scene definition not loaded, can't process file request");
            continue;
        };

        let h_gltf = match asset_server.load_content_file::<Gltf>(&gltf.0.src, &scene_def.id) {
            Ok(h_gltf) => h_gltf,
            Err(e) => {
                warn!("gltf content file not found: {e}");
                set_state(scene_ent, LoadingState::NotFound);
                commands.entity(ent).remove::<GltfLoaded>();
                continue;
            }
        };

        set_state(scene_ent, LoadingState::Loading);
        commands
            .entity(ent)
            .try_insert(h_gltf)
            .remove::<GltfLoaded>();
    }

    for (ent, scene_ent, h_gltf) in unprocessed_gltfs.iter() {
        match asset_server.get_load_state(h_gltf) {
            bevy::asset::LoadState::Loaded => (),
            bevy::asset::LoadState::Failed => {
                warn!("failed to process gltf");
                set_state(scene_ent, LoadingState::FinishedWithError);
                commands.entity(ent).try_insert(GltfLoaded(None));
                continue;
            }
            _ => continue,
        }

        let gltf = gltfs.get(h_gltf).unwrap();
        let gltf_scene_handle = gltf.default_scene.as_ref();

        match gltf_scene_handle {
            Some(gltf_scene_handle) => {
                let instance_id = scene_spawner.spawn_as_child(gltf_scene_handle.clone_weak(), ent);
                commands
                    .entity(ent)
                    .try_insert(GltfLoaded(Some(instance_id)));
            }
            None => {
                warn!("no default scene found in gltf.");
                set_state(scene_ent, LoadingState::FinishedWithError);
                commands.entity(ent).try_insert(GltfLoaded(None));
            }
        }
    }

    for (bevy_scene_entity, dcl_scene_entity, loaded, definition) in ready_gltfs.iter() {
        if loaded.0.is_none() {
            // nothing to process
            commands
                .entity(bevy_scene_entity)
                .try_insert(GltfProcessed::default());
            continue;
        }
        let instance = loaded.0.as_ref().unwrap();
        if scene_spawner.instance_is_ready(*instance) {
            let mut animation_roots = HashSet::default();
            let mut pending_colliders = HashSet::default();

            // let graph = _node_graph(&_debug_query, bevy_scene_entity);
            // println!("{bevy_scene_entity:?}");
            // println!("{graph}");

            // special behaviours, mainly from ADR-215
            // position
            // - children of root nodes -> rotate (why ?! - probably bevy rhs coordinate system specific)
            // skinned mesh
            // - fix zero bone weights (bevy specific, unity and three.js do this automatically)
            // - ignore any mask bits, never create collider
            // colliders
            // - name == *_collider -> not visible
            // - node extras.dcl_collider_mask -> specifies collider mask
            // - name != *_collider -> default collider mask 0
            // - name == *_collider -> default collider mask CL_PHYSICS
            // - PbGltfContainer.disable_physics_colliders -> mask &= ~CL_PHYSICS (switch off physics bit)
            // - PbGltfContainer.create_pointer_colliders && name != *collider -> mask |= CL_POINTERS (switch on pointers bit)
            // - if mask != 0 create collider

            // create a counter per name so we can make unique collider handles
            let mut collider_counter: HashMap<_, u32> = HashMap::default();

            for spawned_ent in scene_spawner.iter_instance_entities(*instance) {
                // delete any cameras
                commands.entity(spawned_ent).remove::<(
                    Camera,
                    CameraRenderGraph,
                    Projection,
                    VisibleEntities,
                    Frustum,
                    Camera3d,
                    Tonemapping,
                    DebandDither,
                    ColorGrading,
                )>();

                // add a container node so other systems can reference the root
                commands.entity(spawned_ent).try_insert(ContainerEntity {
                    container: bevy_scene_entity,
                    root: dcl_scene_entity.root,
                    container_id: dcl_scene_entity.id,
                });

                if let Ok((
                    maybe_name,
                    transform,
                    parent,
                    maybe_player,
                    maybe_h_mesh,
                    maybe_extras,
                    maybe_skin,
                )) = gltf_spawned_entities.get(spawned_ent)
                {
                    // children of root nodes -> rotate
                    if parent.get() == bevy_scene_entity {
                        let mut rotated = *transform;
                        rotated
                            .rotate_around(Vec3::ZERO, Quat::from_rotation_y(std::f32::consts::PI));
                        commands.entity(spawned_ent).try_insert(rotated);
                    }

                    // if there is an animation player, record the entity (bevy-specific hack)
                    if maybe_player.is_some() {
                        if let Some(name) = maybe_name {
                            animation_roots.insert((spawned_ent, name.clone()));
                        }
                    }

                    // if there is no mesh, there's nothing further to do
                    let Some(h_mesh) = maybe_h_mesh else {
                        continue;
                    };
                    let Some(mesh_data) = meshes.get_mut(h_mesh) else {
                        error!("gltf contained mesh not loaded?!");
                        continue;
                    };

                    let has_joints = mesh_data.attribute(Mesh::ATTRIBUTE_JOINT_INDEX).is_some();
                    let has_weights = mesh_data.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT).is_some();
                    let has_skin = maybe_skin.is_some();
                    let is_skinned = has_skin && has_joints && has_weights;
                    if is_skinned {
                        // bevy doesn't calculate culling correctly for skinned entities
                        commands.entity(spawned_ent).try_insert(NoFrustumCulling);
                    } else {
                        // bevy crashes if unskinned models have joints and weights, or if skinned models don't
                        if has_joints {
                            mesh_data.remove_attribute(Mesh::ATTRIBUTE_JOINT_INDEX);
                        }
                        if has_weights {
                            mesh_data.remove_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT);
                        }
                        if has_skin {
                            commands.entity(spawned_ent).remove::<SkinnedMesh>();
                        }
                    }

                    let mut collider_base_name =
                        maybe_name.and_then(|name| name.as_str().strip_suffix("_collider"));

                    if collider_base_name.is_none() {
                        // check parent name also
                        collider_base_name = gltf_spawned_entities
                            .get_component::<Name>(parent.get())
                            .map(|name| name.as_str().strip_suffix("_collider"))
                            .unwrap_or(None)
                    }
                    let is_collider = collider_base_name.is_some();

                    if is_collider {
                        // make invisible by removing mesh handle
                        // TODO - this will break with toggling, we need to store the handle somewhere
                        commands.entity(spawned_ent).remove::<Handle<Mesh>>();
                    }

                    // get specified or default collider bits
                    // try mesh node first
                    let collider_bits = maybe_extras
                        .and_then(|extras| {
                            serde_json::from_str::<DclNodeExtras>(&extras.value).ok()
                        })
                        .and_then(|extras| extras.dcl_collision_mask)
                        .unwrap_or_else(|| {
                            // then try parent node
                            gltf_spawned_entities
                                .get_component::<GltfExtras>(parent.get())
                                .ok()
                                .and_then(|extras| {
                                    serde_json::from_str::<DclNodeExtras>(&extras.value).ok()
                                })
                                .and_then(|extras| extras.dcl_collision_mask)
                                .unwrap_or({
                                    //fall back to container-specified default
                                    if is_collider {
                                        definition.0.invisible_meshes_collision_mask.unwrap_or(
                                            // colliders default to physics + pointers
                                            ColliderLayer::ClPhysics as u32
                                                | ColliderLayer::ClPointer as u32,
                                        )
                                    } else {
                                        definition.0.visible_meshes_collision_mask.unwrap_or(
                                            // non-colliders default to nothing
                                            0,
                                        )
                                    }
                                })
                        });

                    if collider_bits != 0 && !is_skinned {
                        // get or create handle to collider shape
                        let h_shape = match shape_lookup.0.get(h_mesh) {
                            Some(cached_shape_handle)
                                if cached_shapes.get(cached_shape_handle).is_some() =>
                            {
                                cached_shape_handle.clone()
                            }
                            _ => {
                                // asynchronously create the collider
                                let VertexAttributeValues::Float32x3(positions_ref) =
                                    mesh_data.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
                                else {
                                    panic!()
                                };

                                let positions = positions_ref.to_owned();
                                let indices = mesh_data.indices().map(ToOwned::to_owned);

                                let task = AsyncComputeTaskPool::get().spawn(calculate_mesh_collider(positions, indices, transform.scale, false, collider_base_name.as_ref().unwrap().to_string()));

                                let h_shape = cached_shapes.add(GltfCachedShape::Task(task));
                                shape_lookup.0.insert(h_mesh.clone(), h_shape.clone());
                                h_shape
                            }
                        };

                        let index = collider_counter
                            .entry(collider_base_name.to_owned())
                            .or_default();
                        *index += 1u32;

                        pending_colliders.insert(h_shape.clone_weak());

                        commands
                            .entity(spawned_ent)
                            .try_insert(PendingGltfCollider {
                                h_shape,
                                h_mesh: h_mesh.clone_weak(),
                                collision_mask: collider_bits,
                                mesh_name: collider_base_name.map(ToOwned::to_owned),
                                index: *index,
                            });
                    }
                }
            }

            commands.entity(bevy_scene_entity).try_insert((
                GltfProcessed {
                    animation_roots,
                    instance_id: Some(*instance),
                },
                PendingGltfColliders(pending_colliders),
            ));
        }
    }
}

fn attach_ready_colliders(
    mut commands: Commands,
    mut pending_colliders: Query<(Entity, &mut PendingGltfCollider)>,
    mut cached_shapes: ResMut<Assets<GltfCachedShape>>,
    // mut last_len: Local<usize>,
    // time: Res<Time>,
) {
    // let len = pending_colliders.iter().count();
    // if len != *last_len {
    //     println!("{len} ({})", time.elapsed_seconds());
    //     *last_len = len;
    // }

    for (entity, pending) in pending_colliders.iter_mut() {
        let Some(cached_shape) = cached_shapes.get_mut(&pending.h_shape) else {
            panic!("shape or task should have been added")
        };

        let maybe_shape = match cached_shape {
            GltfCachedShape::Shape(shape) => Some(shape.clone()),
            GltfCachedShape::Task(task) => {
                if task.is_finished() {
                    let shape = future::block_on(future::poll_once(task)).unwrap();
                    *cached_shape = GltfCachedShape::Shape(shape.clone());
                    Some(shape)
                } else {
                    None
                }
            }
        };

        if let Some(maybe_shape) = maybe_shape {
            match maybe_shape {
                Ok(shape) => {
                    commands
                        .entity(entity)
                        .try_insert(MeshCollider {
                            shape: MeshColliderShape::Shape(shape, pending.h_mesh.clone()),
                            collision_mask: pending.collision_mask,
                            mesh_name: pending.mesh_name.clone(),
                            index: pending.index,
                        })
                        .remove::<PendingGltfCollider>();
                }
                Err(e) => {
                    commands.entity(entity).remove::<PendingGltfCollider>();
                    warn!("failed to generate collider for {entity:?}: {e}")
                }
            }
        }
    }
}

pub const GLTF_LOADING: &str = "gltfs loading";

fn check_gltfs_ready(
    mut scenes: Query<(Entity, &mut RendererSceneContext)>,
    unready_gltfs: Query<&SceneEntity, (With<GltfDefinition>, Without<GltfProcessed>)>,
    unready_colliders: Query<&ContainerEntity, With<PendingGltfCollider>>,
) {
    let mut unready_scenes = HashSet::default();

    for ent in &unready_gltfs {
        unready_scenes.insert(ent.root);
    }

    for ent in &unready_colliders {
        unready_scenes.insert(ent.root);
    }

    for (root, mut context) in scenes.iter_mut() {
        if unready_scenes.contains(&root) && context.tick_number == 0 {
            debug!("{root:?} blocked on gltfs");
            context.blocked.insert(GLTF_LOADING);
        } else {
            context.blocked.remove(GLTF_LOADING);
        }
    }
}

// list of pending shapes for a gltf container
#[derive(Component)]
pub struct PendingGltfColliders(pub HashSet<Handle<GltfCachedShape>>);

// set loading finished once all colliders resolved
fn update_container_finished(
    mut commands: Commands,
    mut loading_containers: Query<(Entity, &SceneEntity, &mut PendingGltfColliders)>,
    colliders: Res<Assets<GltfCachedShape>>,
    mut contexts: Query<&mut RendererSceneContext>,
) {
    for (entity, scene_ent, mut pending) in loading_containers.iter_mut() {
        pending.0.retain(|h_shape| match colliders.get(h_shape) {
            Some(shape) => matches!(shape, GltfCachedShape::Task(_)),
            None => true,
        });

        if pending.0.is_empty() {
            commands.entity(entity).remove::<PendingGltfColliders>();
            if let Ok(mut context) = contexts.get_mut(scene_ent.root) {
                context.update_crdt(
                    SceneComponentId::GLTF_CONTAINER_LOADING_STATE,
                    CrdtType::LWW_ANY,
                    scene_ent.id,
                    &PbGltfContainerLoadingState {
                        current_state: LoadingState::Finished as i32,
                    },
                );
            }
        }
    }
}

// debug show the gltf graph
#[allow(clippy::type_complexity)]
fn _node_graph(
    scene_entity_query: &Query<(
        Entity,
        Option<&Name>,
        Option<&Children>,
        Option<&SkinnedMesh>,
        &Transform,
    )>,
    root: Entity,
) -> String {
    let mut graph_nodes = HashMap::default();
    let mut graph = petgraph::Graph::<_, ()>::new();
    let mut to_check = vec![root];

    while let Some(ent) = to_check.pop() {
        debug!("current: {ent:?}, to_check: {to_check:?}");
        let Ok((ent, name, maybe_children, maybe_skinned, transform)) = scene_entity_query.get(ent)
        else {
            return "?".to_owned();
        };

        let graph_node = *graph_nodes.entry(ent).or_insert_with(|| {
            graph.add_node(format!(
                "{ent:?}:{:?} {} [{:?}] ",
                name,
                if maybe_skinned.is_some() { "(*)" } else { "" },
                transform.scale
            ))
        });

        if let Some(children) = maybe_children {
            let sorted_children_with_name: BTreeMap<_, _> = children
                .iter()
                .map(|c| {
                    (
                        scene_entity_query
                            .get(*c)
                            .map(|q| q.1.map(|name| name.as_str().to_owned()))
                            .unwrap_or(Some(String::from("?"))),
                        (
                            c,
                            scene_entity_query.get_component::<SkinnedMesh>(*c).is_ok(),
                            scene_entity_query
                                .get_component::<Transform>(*c)
                                .map(|t| t.scale)
                                .unwrap_or(Vec3::ZERO),
                        ),
                    )
                })
                .collect();

            to_check.extend(sorted_children_with_name.values().map(|(ent, ..)| *ent));
            for (child_id, (child_ent, is_skinned, child_scale)) in
                sorted_children_with_name.into_iter()
            {
                let child_graph_node = *graph_nodes.entry(*child_ent).or_insert_with(|| {
                    graph.add_node(format!(
                        "{child_ent:?}:{:?} {} [{:?}]",
                        child_id,
                        if is_skinned { "(*)" } else { "" },
                        child_scale
                    ))
                });
                graph.add_edge(graph_node, child_graph_node, ());
            }
        }
    }

    let dot = petgraph::dot::Dot::with_config(&graph, &[petgraph::dot::Config::EdgeNoLabel]);
    format!("{:?}", dot)
}
