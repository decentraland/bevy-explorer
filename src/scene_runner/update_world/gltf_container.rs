use std::collections::BTreeMap;

use bevy::{
    gltf::{Gltf, GltfExtras},
    prelude::*,
    reflect::TypeUuid,
    render::mesh::{Indices, VertexAttributeValues},
    scene::InstanceId,
    tasks::{AsyncComputeTaskPool, Task},
    utils::{HashMap, HashSet},
};
use futures_lite::future;
use nalgebra::Point;
use rapier3d::prelude::*;
use serde::Deserialize;

use crate::{
    dcl::interface::ComponentPosition,
    dcl_component::{
        proto_components::sdk::components::{ColliderLayer, PbGltfContainer},
        SceneComponentId, SceneEntityId,
    },
    ipfs::{IpfsLoaderExt, SceneDefinition},
    scene_runner::{ContainerEntity, SceneEntity, SceneSets},
};

use super::{
    mesh_collider::{MeshCollider, MeshColliderShape},
    AddCrdtInterfaceExt,
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

        app.add_system(update_gltf.in_set(SceneSets::PostLoop));
        app.add_system(attach_ready_colliders.in_set(SceneSets::PostLoop));
        app.add_asset::<GltfCachedShape>();
        app.init_resource::<MeshToShape>();
    }
}

#[derive(TypeUuid)]
#[uuid = "09e7812e-ea71-4046-a9be-65565257d459"]
pub enum GltfCachedShape {
    Shape(SharedShape),
    Task(Task<SharedShape>),
}

#[derive(Component)]
pub struct PendingGltfCollider {
    h_shape: Handle<GltfCachedShape>,
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
    pub animation_roots: HashSet<Entity>,
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
    new_gltfs: Query<(Entity, &SceneEntity, &GltfDefinition), Changed<GltfDefinition>>,
    unprocessed_gltfs: Query<(Entity, &SceneEntity, &Handle<Gltf>), Without<GltfLoaded>>,
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
    )>,
    scene_def_handles: Query<&Handle<SceneDefinition>>,
    scene_defs: Res<Assets<SceneDefinition>>,
    asset_server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut scene_spawner: ResMut<SceneSpawner>,
    mesh_handles: Query<(Option<&Handle<Mesh>>, &Transform, &Parent)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cached_shapes: ResMut<Assets<GltfCachedShape>>,
    mut shape_lookup: ResMut<MeshToShape>,
    _debug_name_query: Query<(Entity, Option<&Name>, Option<&Children>)>,
) {
    // TODO: clean up old gltf data

    for (ent, scene_ent, gltf) in new_gltfs.iter() {
        debug!("{} has {}", scene_ent.id, gltf.0.src);

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
                commands.entity(ent).remove::<GltfLoaded>();
                continue;
            }
        };

        commands.entity(ent).insert(h_gltf).remove::<GltfLoaded>();
    }

    for (ent, _scene_ent, h_gltf) in unprocessed_gltfs.iter() {
        match asset_server.get_load_state(h_gltf) {
            bevy::asset::LoadState::Loaded => (),
            bevy::asset::LoadState::Failed => {
                warn!("failed to process gltf");
                commands.entity(ent).insert(GltfLoaded(None));
                continue;
            }
            _ => continue,
        }

        let gltf = gltfs.get(h_gltf).unwrap();
        let gltf_scene_handle = gltf.default_scene.as_ref();

        match gltf_scene_handle {
            Some(gltf_scene_handle) => {
                let instance_id = scene_spawner.spawn_as_child(gltf_scene_handle.clone_weak(), ent);
                commands.entity(ent).insert(GltfLoaded(Some(instance_id)));
            }
            None => {
                warn!("no default scene found in gltf.");
                commands.entity(ent).insert(GltfLoaded(None));
            }
        }
    }

    for (bevy_scene_entity, dcl_scene_entity, loaded, _definition) in ready_gltfs.iter() {
        if loaded.0.is_none() {
            // nothing to process
            commands
                .entity(bevy_scene_entity)
                .insert(GltfProcessed::default());
            continue;
        }
        let instance = loaded.0.as_ref().unwrap();
        if scene_spawner.instance_is_ready(*instance) {
            let mut animation_roots = HashSet::default();

            // let graph = node_graph(&debug_name_query, bevy_scene_entity);
            // println!("{bevy_scene_entity:?}");
            // println!("{graph}");

            // special behaviours, mainly from ADR-215
            // position
            // children of root nodes -> rotate (why ?!)
            // skinned mesh
            // fix zero bone weights
            // ignore any mask bits, never create collider
            // colliders
            // name == *_collider -> not visible
            // node extras.dcl_collider_mask -> specifies collider mask
            // name != *_collider -> default collider mask 0
            // name == *_collider -> default collider mask CL_PHYSICS
            // PbGltfContainer.disable_physics_colliders -> mask &= ~CL_PHYSICS (switch off physics bit)
            // PbGltfContainer.create_pointer_colliders && name != *collider -> mask |= CL_POINTERS (switch on pointers bit)
            // if mask != 0 create collider

            // create a counter per name so we can make unique collider handles
            let mut collider_counter: HashMap<_, u32> = HashMap::default();

            for spawned_ent in scene_spawner.iter_instance_entities(*instance) {
                // add a container node so other systems can reference the root
                commands.entity(spawned_ent).insert(ContainerEntity {
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
                )) = gltf_spawned_entities.get(spawned_ent)
                {
                    // children of root nodes -> rotate
                    if parent.get() == bevy_scene_entity {
                        let mut rotated = *transform;
                        rotated
                            .rotate_around(Vec3::ZERO, Quat::from_rotation_y(std::f32::consts::PI));
                        commands.entity(spawned_ent).insert(rotated);
                    }

                    // if there is an animation player, record the entity (bevy-specific hack)
                    if maybe_player.is_some() {
                        animation_roots.insert(spawned_ent);
                    }

                    // if there is no mesh, there's nothing further to do
                    let Some(h_mesh) = maybe_h_mesh else {
                        continue;
                    };
                    let Some(mesh_data) = meshes.get(h_mesh) else {
                        error!("gltf contained mesh not loaded?!");
                        continue;
                    };

                    let is_skinned = mesh_data.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT).is_some();

                    let is_collider = maybe_name
                        .map(|name| name.as_str().ends_with("_collider"))
                        .unwrap_or(false);

                    if is_collider {
                        // make invisible by removing mesh handle
                        // TODO - this will break with toggling, we need to store the handle somewhere
                        commands.entity(spawned_ent).remove::<Handle<Mesh>>();
                    }

                    // get specified or default collider bits
                    let mut collider_bits = maybe_extras
                        .and_then(|extras| {
                            serde_json::from_str::<DclNodeExtras>(&extras.value).ok()
                        })
                        .and_then(|extras| extras.dcl_collision_mask)
                        .unwrap_or({
                            if is_collider {
                                // colliders default to physics
                                ColliderLayer::ClPhysics as u32
                            } else {
                                // non-colliders default to nothing
                                0
                            }
                        });

                    // TODO plug in disable_physics_colliders once proto message is updated
                    if false {
                        // switch off physics bit
                        collider_bits &= !(ColliderLayer::ClPhysics as u32);
                    }

                    // TODO plug in create_pointer_colliders once proto message is updated
                    if false {
                        // switch on pointer bit
                        collider_bits |= ColliderLayer::ClPointer as u32;
                    }

                    if collider_bits != 0 && !is_skinned {
                        if let Some(cached_shape_handle) = shape_lookup.0.get(h_mesh) {
                            // already calculating or calculated, just attach a handle
                            commands
                                .entity(spawned_ent)
                                .insert(cached_shape_handle.clone());
                            continue;
                        }

                        // create the collider
                        let scale = transform.scale;
                        let VertexAttributeValues::Float32x3(positions) = mesh_data.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() else { panic!() };
                        let vertices: Vec<_> = positions
                            .iter()
                            .map(|p| Point::from([p[0] * scale.x, p[1] * scale.y, p[2] * scale.z]))
                            .collect();
                        let indices: Vec<_> = match mesh_data.indices() {
                            Some(Indices::U16(u16s)) => u16s
                                .chunks_exact(3)
                                .map(|ix| [ix[0] as u32, ix[1] as u32, ix[2] as u32])
                                .collect(),
                            Some(Indices::U32(u32s)) => u32s
                                .chunks_exact(3)
                                .map(|ix| [ix[0], ix[1], ix[2]])
                                .collect(),
                            None => (0u32..positions.len() as u32)
                                .collect::<Vec<_>>()
                                .chunks_exact(3)
                                .map(|ix| [ix[0], ix[1], ix[2]])
                                .collect(),
                        };

                        let task = AsyncComputeTaskPool::get().spawn(async move {
                            SharedShape::convex_decomposition(&vertices, &indices)
                        });
                        let h_shape = cached_shapes.add(GltfCachedShape::Task(task));
                        shape_lookup.0.insert(h_mesh.clone(), h_shape.clone());

                        let base_name = maybe_name
                            .unwrap()
                            .strip_suffix("_collider")
                            .unwrap_or_else(|| maybe_name.unwrap());
                        let index = collider_counter.entry(base_name).or_default();
                        *index += 1u32;

                        commands.entity(spawned_ent).insert(PendingGltfCollider {
                            h_shape,
                            collision_mask: collider_bits,
                            mesh_name: Some(base_name.to_owned()),
                            index: *index,
                        });
                    }

                    if is_skinned {
                        // fix zero joint weights, same way as unity and three.js
                        // TODO: remove if https://github.com/bevyengine/bevy/pull/8316 is merged
                        if let Some(VertexAttributeValues::Float32x4(joint_weights)) = mesh_handles
                            .get(spawned_ent)
                            .ok()
                            .and_then(|(h_mesh, ..)| h_mesh)
                            .and_then(|h_mesh| meshes.get_mut(h_mesh))
                            .and_then(|mesh| mesh.attribute_mut(Mesh::ATTRIBUTE_JOINT_WEIGHT))
                        {
                            for weights in joint_weights
                                .iter_mut()
                                .filter(|weights| *weights == &[0.0, 0.0, 0.0, 0.0])
                            {
                                weights[0] = 1.0;
                            }
                        }
                    }
                }
            }

            commands
                .entity(bevy_scene_entity)
                .insert(GltfProcessed { animation_roots });
        }
    }
}

fn attach_ready_colliders(
    mut commands: Commands,
    mut pending_colliders: Query<(Entity, &mut PendingGltfCollider)>,
    mut cached_shapes: ResMut<Assets<GltfCachedShape>>,
) {
    for (entity, pending) in pending_colliders.iter_mut() {
        let Some(cached_shape) = cached_shapes.get_mut(&pending.h_shape) else {
            panic!("shape or task should have been added")
        };

        let (maybe_shape, maybe_update_asset) = match cached_shape {
            GltfCachedShape::Shape(shape) => (Some(shape.clone()), None),
            GltfCachedShape::Task(task) => {
                if task.is_finished() {
                    let shape = future::block_on(future::poll_once(task)).unwrap();
                    (Some(shape.clone()), Some(shape))
                } else {
                    (None, None)
                }
            }
        };

        if let Some(shape) = maybe_shape {
            commands
                .entity(entity)
                .insert(MeshCollider {
                    shape: MeshColliderShape::Shape(shape),
                    collision_mask: pending.collision_mask,
                    mesh_name: pending.mesh_name.clone(),
                    index: pending.index,
                })
                .remove::<PendingGltfCollider>();
        }

        if let Some(shape) = maybe_update_asset {
            *cached_shapes.get_mut(&pending.h_shape).unwrap() = GltfCachedShape::Shape(shape);
        }
    }
}

fn _node_graph(
    scene_entity_query: &Query<(Entity, Option<&Name>, Option<&Children>)>,
    root: Entity,
) -> String {
    let mut graph_nodes = HashMap::default();
    let mut graph = petgraph::Graph::<_, ()>::new();
    let mut to_check = vec![root];

    while let Some(ent) = to_check.pop() {
        debug!("current: {ent:?}, to_check: {to_check:?}");
        let Ok((ent, name, maybe_children)) = scene_entity_query.get(ent) else {
            panic!()
        };

        let graph_node = *graph_nodes
            .entry(ent)
            .or_insert_with(|| graph.add_node(format!("{ent:?}:{:?}", name)));

        if let Some(children) = maybe_children {
            let sorted_children_with_name: BTreeMap<_, _> = children
                .iter()
                .map(|c| (scene_entity_query.get(*c).unwrap().1, c))
                .collect();

            to_check.extend(sorted_children_with_name.values().copied());
            for (child_id, child_ent) in sorted_children_with_name.into_iter() {
                let child_graph_node = *graph_nodes
                    .entry(*child_ent)
                    .or_insert_with(|| graph.add_node(format!("{child_ent:?}:{:?}", child_id)));
                graph.add_edge(graph_node, child_graph_node, ());
            }
        }
    }

    let dot = petgraph::dot::Dot::with_config(&graph, &[petgraph::dot::Config::EdgeNoLabel]);
    format!("{:?}", dot)
}
