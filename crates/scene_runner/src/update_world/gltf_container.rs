// TODO
// - gltf collider flags
// - clean up of cached colliders (when mesh is unloaded?)
use std::collections::BTreeMap;

use bevy::{
    core_pipeline::tonemapping::{DebandDither, Tonemapping},
    gltf::{Gltf, GltfExtras},
    pbr::ExtendedMaterial,
    prelude::*,
    render::{
        camera::CameraRenderGraph,
        mesh::{skinning::SkinnedMesh, Indices, VertexAttributeValues},
        primitives::Frustum,
        view::{ColorGrading, NoFrustumCulling, VisibleEntities},
    },
    scene::InstanceId,
    utils::{HashMap, HashSet},
};
use rapier3d_f64::prelude::*;
use serde::Deserialize;

use crate::{renderer_context::RendererSceneContext, ContainerEntity, SceneEntity, SceneSets};
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::sdk::components::{
        common::LoadingState, ColliderLayer, PbGltfContainer, PbGltfContainerLoadingState,
    },
    SceneComponentId, SceneEntityId,
};
use ipfs::{EntityDefinition, IpfsAssetServer};

use super::{
    mesh_collider::{MeshCollider, MeshColliderShape},
    scene_material::{SceneBound, SceneBoundPlugin, SceneMaterial},
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

        app.add_systems(Update, update_gltf.in_set(SceneSets::PostLoop));
        app.add_systems(Update, check_gltfs_ready.in_set(SceneSets::PostInit));

        app.add_plugins(SceneBoundPlugin);
    }
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
        (Entity, &SceneEntity, &Handle<Gltf>, &GltfDefinition),
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
        Option<&Handle<StandardMaterial>>,
    )>,
    scene_def_handles: Query<&Handle<EntityDefinition>>,
    (scene_defs, gltfs, ipfas, base_mats, mut bound_mats): (
        Res<Assets<EntityDefinition>>,
        Res<Assets<Gltf>>,
        IpfsAssetServer,
        Res<Assets<StandardMaterial>>,
        ResMut<Assets<SceneMaterial>>,
    ),
    mut scene_spawner: ResMut<SceneSpawner>,
    mut meshes: ResMut<Assets<Mesh>>,
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

        let h_gltf = match ipfas.load_content_file::<Gltf>(&gltf.0.src, &scene_def.id) {
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

    for (ent, scene_ent, h_gltf, def) in unprocessed_gltfs.iter() {
        match ipfas.load_state(h_gltf) {
            bevy::asset::LoadState::Loaded => (),
            bevy::asset::LoadState::Failed => {
                warn!("failed to process gltf: {}", def.0.src);
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

            let Ok(mut context) = contexts.get_mut(dcl_scene_entity.root) else {
                continue;
            };

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
                    Handle<StandardMaterial>,
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
                    maybe_material,
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

                    // fix up mesh
                    mesh_data.normalize_joint_weights();

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

                    // substitute material
                    if let Some(h_material) = maybe_material {
                        let Some(base) = base_mats.get(h_material) else {
                            panic!();
                        };
                        commands
                            .entity(spawned_ent)
                            .insert(bound_mats.add(ExtendedMaterial {
                                base: base.clone(),
                                extension: SceneBound {
                                    bounds: context.bounds,
                                },
                            }));
                    }

                    // process collider
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
                        // create collider shape
                        let VertexAttributeValues::Float32x3(positions_ref) =
                            mesh_data.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
                        else {
                            panic!("no positions")
                        };

                        let positions_parry: Vec<_> = positions_ref
                            .iter()
                            .map(|pos| Point::from([pos[0] as f64, pos[1] as f64, pos[2] as f64]))
                            .collect();

                        let indices: Vec<u32> = match mesh_data.indices() {
                            None => (0..positions_ref.len() as u32).collect(),
                            Some(Indices::U16(ixs)) => ixs.iter().map(|ix| *ix as u32).collect(),
                            Some(Indices::U32(ixs)) => ixs.to_vec(),
                        };
                        let indices_parry = indices
                            .chunks_exact(3)
                            .map(|chunk| chunk.try_into().unwrap())
                            .collect();

                        let shape = SharedShape::trimesh(positions_parry, indices_parry);

                        let index = collider_counter
                            .entry(collider_base_name.to_owned())
                            .or_default();
                        *index += 1u32;

                        commands.entity(spawned_ent).try_insert(MeshCollider {
                            shape: MeshColliderShape::Shape(shape, h_mesh.clone()),
                            collision_mask: collider_bits,
                            mesh_name: collider_base_name.map(ToOwned::to_owned),
                            index: *index,
                        });
                    }
                }
            }

            commands
                .entity(bevy_scene_entity)
                .try_insert((GltfProcessed {
                    animation_roots,
                    instance_id: Some(*instance),
                },));
            context.update_crdt(
                SceneComponentId::GLTF_CONTAINER_LOADING_STATE,
                CrdtType::LWW_ANY,
                dcl_scene_entity.id,
                &PbGltfContainerLoadingState {
                    current_state: LoadingState::Finished as i32,
                },
            );
        }
    }
}

pub const GLTF_LOADING: &str = "gltfs loading";

fn check_gltfs_ready(
    mut scenes: Query<(Entity, &mut RendererSceneContext)>,
    unready_gltfs: Query<&SceneEntity, (With<GltfDefinition>, Without<GltfProcessed>)>,
) {
    let mut unready_scenes = HashSet::default();

    for ent in &unready_gltfs {
        unready_scenes.insert(ent.root);
    }

    for (root, mut context) in scenes.iter_mut() {
        if unready_scenes.contains(&root) && context.tick_number <= 5 {
            debug!("{root:?} blocked on gltfs");
            context.blocked.insert(GLTF_LOADING);
        } else {
            context.blocked.remove(GLTF_LOADING);
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
