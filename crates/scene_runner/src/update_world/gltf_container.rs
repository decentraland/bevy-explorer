// TODO
// - gltf collider flags
// - clean up of cached colliders (when mesh is unloaded?)
use std::{
    collections::BTreeMap,
    hash::{Hash, Hasher},
};

use bevy::{
    asset::LoadState,
    gltf::{Gltf, GltfExtras, GltfLoaderSettings},
    pbr::{ExtendedMaterial, Material},
    prelude::*,
    render::{
        mesh::{skinning::SkinnedMesh, Indices, VertexAttributeValues},
        render_asset::RenderAssetUsages,
        texture::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
        view::NoFrustumCulling,
    },
    scene::{scene_spawner_system, InstanceId},
    utils::{HashMap, HashSet},
};
use common::structs::AppConfig;
use rapier3d_f64::prelude::*;
use serde::Deserialize;
use ui_core::ModifyComponentExt;

use crate::{
    renderer_context::RendererSceneContext, update_world::material::BaseMaterial, ContainerEntity,
    SceneEntity, SceneSets,
};
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::{
        common::{Texture, TextureFilterMode, TextureUnion, TextureWrapMode},
        sdk::components::{
            common::LoadingState, pb_material, ColliderLayer, GltfNodeStateValue,
            MaterialTransparencyMode, PbGltfContainer, PbGltfContainerLoadingState, PbGltfNode,
            PbGltfNodeState, PbMaterial,
        },
    },
    transform_and_parent::DclTransformAndParent,
    SceneComponentId, SceneEntityId,
};
use ipfs::{ipfs_path::IpfsPath, EntityDefinition, IpfsAssetServer};
use scene_material::{SceneBound, SceneMaterial};

use super::{
    mesh_collider::{MeshCollider, MeshColliderShape},
    transform_and_parent::TransformHelperPub,
    AddCrdtInterfaceExt, ComponentTracker,
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

        app.add_crdt_lww_component::<PbGltfNode, GltfNodeRequest>(
            SceneComponentId::GLTF_NODE,
            ComponentPosition::EntityOnly,
        );

        app.add_systems(Update, update_gltf.in_set(SceneSets::PostLoop));
        app.add_systems(SpawnScene, update_ready_gltfs.after(scene_spawner_system));
        app.add_systems(Update, check_gltfs_ready.in_set(SceneSets::PostInit));
        app.add_systems(
            Update,
            (
                expose_gltfs,
                update_gltf_linked_transforms,
                update_gltf_linked_visibility,
            )
                .chain()
                .in_set(SceneSets::PostLoop),
        );
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
    pub named_nodes: HashMap<String, Entity>,
}

#[derive(Deserialize)]
struct DclNodeExtras {
    dcl_collision_mask: Option<u32>,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_gltf(
    mut commands: Commands,
    mut commands2: Commands,
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
    scene_def_handles: Query<&Handle<EntityDefinition>>,
    (scene_defs, gltfs, images, ipfas, mut base_mats): (
        Res<Assets<EntityDefinition>>,
        Res<Assets<Gltf>>,
        Res<Assets<Image>>,
        IpfsAssetServer,
        ResMut<Assets<StandardMaterial>>,
    ),
    mut scene_spawner: ResMut<SceneSpawner>,
    mut contexts: Query<(Entity, &mut RendererSceneContext, Has<SceneResourceLookup>)>,
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
        if let Ok((root, mut context, has_material_lookup)) = contexts.get_mut(scene_ent.root) {
            context.update_crdt(
                SceneComponentId::GLTF_CONTAINER_LOADING_STATE,
                CrdtType::LWW_ANY,
                scene_ent.id,
                &PbGltfContainerLoadingState {
                    current_state: current_state as i32,
                    named_nodes: Default::default(),
                },
            );

            if !has_material_lookup {
                commands2
                    .entity(root)
                    .try_insert(SceneResourceLookup::default());
            }
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

        let h_gltf = ipfas.load_content_file_with_settings::<Gltf, GltfLoaderSettings>(
            &gltf.0.src,
            &scene_def.id,
            |s| {
                s.load_cameras = false;
                s.load_lights = false;
                s.load_materials = RenderAssetUsages::RENDER_WORLD;
            },
        );

        let h_gltf = match h_gltf {
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
            bevy::asset::LoadState::Loading => continue,
            other => {
                warn!("unexpected load state: {other:?}");
                continue;
            }
        }

        let gltf = gltfs.get(h_gltf).unwrap();
        let gltf_scene_handle = gltf.default_scene.as_ref();

        // validate texture types
        for h_mat in gltf.materials.iter() {
            let Some(mat) = base_mats.get_mut(h_mat) else {
                continue;
            };

            if let Some(h_base) = mat.base_color_texture.as_ref() {
                if ipfas.asset_server().get_load_state(h_base) == Some(LoadState::Loading) {
                    continue;
                }

                if let Some(texture) = images.get(h_base) {
                    if texture.texture_descriptor.format.sample_type(None, None)
                        != Some(bevy::render::render_resource::TextureSampleType::Float {
                            filterable: true,
                        })
                    {
                        warn!("invalid format for base color texture, disabling");
                        mat.base_color_texture = None;
                    }
                }
            }
        }

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
}

pub struct CachedMeshData {
    pub mesh_id: AssetId<Mesh>,
    maybe_collider: Option<Handle<Mesh>>,
}

#[derive(Component, Default)]
pub struct SceneResourceLookup {
    pub materials: HashMap<Handle<StandardMaterial>, Handle<SceneMaterial>>,
    pub meshes: HashMap<u64, CachedMeshData>,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_ready_gltfs(
    mut commands: Commands,
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
    (base_mats, mut bound_mats): (Res<Assets<StandardMaterial>>, ResMut<Assets<SceneMaterial>>),
    scene_spawner: Res<SceneSpawner>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut contexts: Query<(
        &mut RendererSceneContext,
        &mut SceneResourceLookup,
        &mut ComponentTracker,
    )>,
    _debug_query: Query<(
        Entity,
        Option<&Name>,
        Option<&Children>,
        Option<&SkinnedMesh>,
        &Transform,
    )>,
    asset_server: Res<AssetServer>,
    config: Res<AppConfig>,
) {
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

            let Ok((mut context, mut resource_lookup, mut tracker)) =
                contexts.get_mut(dcl_scene_entity.root)
            else {
                continue;
            };

            let mut named_nodes = HashMap::default();

            for spawned_ent in scene_spawner.iter_instance_entities(*instance) {
                // delete any base materials
                commands
                    .entity(spawned_ent)
                    .remove::<(Handle<StandardMaterial>,)>();

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
                    // collect named nodes to push to scene on request
                    if let Some(name) = maybe_name {
                        named_nodes.insert(name.as_str().to_owned(), spawned_ent);
                    }

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
                            debug!("animator found on {name} node of {}", definition.0.src);
                            animation_roots.insert((spawned_ent, name.clone()));
                            *tracker.0.entry("Animations").or_default() += 1;
                        }
                    }

                    // if there is no mesh, there's nothing further to do
                    let Some(h_gltf_mesh) = maybe_h_mesh else {
                        continue;
                    };
                    let Some(mesh_data) = meshes.get_mut(h_gltf_mesh) else {
                        error!("gltf contained mesh not loaded?!");
                        continue;
                    };

                    let hash = &mut std::hash::DefaultHasher::new();
                    for (attr_id, data) in mesh_data.attributes() {
                        attr_id.hash(hash);
                        data.get_bytes().hash(hash);
                    }

                    let has_joints = mesh_data.attribute(Mesh::ATTRIBUTE_JOINT_INDEX).is_some();
                    let has_weights = mesh_data.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT).is_some();
                    let has_skin = maybe_skin.is_some();
                    let is_skinned = has_skin && has_joints && has_weights;
                    is_skinned.hash(hash);

                    mesh_data.primitive_topology().hash(hash);

                    if let Some(indices) = mesh_data.indices() {
                        indices.iter().for_each(|index| index.hash(hash));
                    }

                    let hash = hash.finish();

                    let cached_data = resource_lookup.meshes.get(&hash).and_then(|data| {
                        asset_server
                            .get_id_handle(data.mesh_id)
                            .map(|h| (h, &data.maybe_collider))
                    });

                    // note: disable cache for meshes with morph targets as we don't include them in the hash
                    let (h_mesh, cached_collider) =
                        match (mesh_data.has_morph_targets(), cached_data) {
                            (false, Some((h_mesh, cached_collider))) => {
                                // overwrite with cached handle
                                commands.entity(spawned_ent).insert(h_mesh.clone());
                                (h_mesh, cached_collider.clone())
                            }
                            _ => {
                                mesh_data.normalize_joint_weights();

                                if !is_skinned {
                                    // bevy crashes if unskinned models have joints and weights, or if skinned models don't
                                    if has_joints {
                                        mesh_data.remove_attribute(Mesh::ATTRIBUTE_JOINT_INDEX);
                                    }
                                    if has_weights {
                                        mesh_data.remove_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT);
                                    }
                                }

                                resource_lookup.meshes.insert(
                                    hash,
                                    CachedMeshData {
                                        mesh_id: h_gltf_mesh.id(),
                                        maybe_collider: None,
                                    },
                                );
                                *tracker.0.entry("Unique Meshes").or_default() += 1;
                                (h_gltf_mesh.clone(), None)
                            }
                        };
                    *tracker.0.entry("Total Meshes").or_default() += 1;

                    if is_skinned {
                        // bevy doesn't calculate culling correctly for skinned entities
                        commands.entity(spawned_ent).try_insert(NoFrustumCulling);
                    } else if maybe_skin.is_some() {
                        // remove skin data if mesh doesn't have all required data
                        commands.entity(spawned_ent).remove::<SkinnedMesh>();
                    }

                    // substitute material
                    if let Some(h_material) = maybe_material {
                        let h_scene_material = if let Some(h_scene_material) =
                            resource_lookup.materials.get(h_material)
                        {
                            h_scene_material.clone()
                        } else {
                            let Some(base) = base_mats.get(h_material) else {
                                panic!();
                            };
                            let h_scene_material = bound_mats.add(ExtendedMaterial {
                                base: base.clone(),
                                extension: SceneBound::new(context.bounds, config.graphics.oob),
                            });
                            resource_lookup
                                .materials
                                .insert(h_material.clone(), h_scene_material.clone());

                            *tracker.0.entry("Unique Materials").or_default() += 1;
                            h_scene_material
                        };
                        commands.entity(spawned_ent).insert(h_scene_material);
                    }
                    *tracker.0.entry("Materials").or_default() += 1;

                    // process collider
                    let mut collider_base_name = maybe_name
                        .map(Name::as_str)
                        .filter(|name| name.contains("_collider"));

                    if collider_base_name.is_none() {
                        // check parent name also
                        collider_base_name = gltf_spawned_entities
                            .get(parent.get())
                            .ok()
                            .and_then(|(name, ..)| name)
                            .map(|name| name.as_str())
                            .filter(|name| name.contains("_collider"))
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
                                .get(parent.get())
                                .ok()
                                .and_then(|tpl| tpl.5)
                                .and_then(|extras| {
                                    serde_json::from_str::<DclNodeExtras>(&extras.value).ok()
                                })
                                .and_then(|extras| extras.dcl_collision_mask)
                                .unwrap_or({
                                    //fall back to container-specified default
                                    if is_collider {
                                        definition.0.invisible_meshes_collision_mask.unwrap_or(
                                            // colliders default to physics + pointers
                                            if is_skinned {
                                                // if skinned, maybe foundation uses 0 default?
                                                0
                                            } else {
                                                ColliderLayer::ClPhysics as u32
                                                    | ColliderLayer::ClPointer as u32
                                            },
                                        )
                                    } else {
                                        definition.0.visible_meshes_collision_mask.unwrap_or(
                                            // non-colliders default to nothing
                                            0,
                                        )
                                    }
                                })
                        });

                    if collider_bits != 0
                    /* && !is_skinned */
                    {
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

                        let shape = SharedShape::trimesh_with_flags(
                            positions_parry,
                            indices_parry,
                            TriMeshFlags::all(),
                        );

                        let index = collider_counter
                            .entry(collider_base_name.to_owned())
                            .or_default();
                        *index += 1u32;

                        let h_collider = if is_skinned {
                            match cached_collider {
                                Some(collider) => collider,
                                None => {
                                    let mut new_mesh = Mesh::new(
                                        mesh_data.primitive_topology(),
                                        RenderAssetUsages::RENDER_WORLD,
                                    );
                                    if let Some(indices) = mesh_data.indices().cloned() {
                                        new_mesh.insert_indices(indices);
                                    }
                                    for (attribute_id, data) in mesh_data.attributes() {
                                        let attribute = match attribute_id {
                                            id if id == Mesh::ATTRIBUTE_JOINT_INDEX.id => continue,
                                            id if id == Mesh::ATTRIBUTE_JOINT_WEIGHT.id => continue,
                                            id if id == Mesh::ATTRIBUTE_POSITION.id => {
                                                Mesh::ATTRIBUTE_POSITION
                                            }
                                            id if id == Mesh::ATTRIBUTE_NORMAL.id => {
                                                Mesh::ATTRIBUTE_NORMAL
                                            }
                                            id if id == Mesh::ATTRIBUTE_UV_0.id => {
                                                Mesh::ATTRIBUTE_UV_0
                                            }
                                            id if id == Mesh::ATTRIBUTE_UV_1.id => {
                                                Mesh::ATTRIBUTE_UV_1
                                            }
                                            id if id == Mesh::ATTRIBUTE_TANGENT.id => {
                                                Mesh::ATTRIBUTE_TANGENT
                                            }
                                            id if id == Mesh::ATTRIBUTE_COLOR.id => {
                                                Mesh::ATTRIBUTE_COLOR
                                            }
                                            _ => {
                                                warn!("unrecognised vertex attribute {attribute_id:?}");
                                                continue;
                                            }
                                        };

                                        new_mesh.insert_attribute(attribute, data.clone());
                                    }
                                    let h_collider = meshes.add(new_mesh);

                                    if let Some(data) = resource_lookup.meshes.get_mut(&hash) {
                                        data.maybe_collider = Some(h_collider.clone());
                                    }

                                    h_collider
                                }
                            }
                        } else {
                            h_mesh.clone()
                        };

                        commands.entity(spawned_ent).try_insert(MeshCollider {
                            shape: MeshColliderShape::Shape(shape, h_collider),
                            collision_mask: collider_bits,
                            mesh_name: collider_base_name.map(ToOwned::to_owned),
                            index: *index,
                        });
                    }
                }
            }

            context.update_crdt(
                SceneComponentId::GLTF_CONTAINER_LOADING_STATE,
                CrdtType::LWW_ANY,
                dcl_scene_entity.id,
                &PbGltfContainerLoadingState {
                    current_state: LoadingState::Finished as i32,
                    named_nodes: named_nodes.keys().cloned().collect(),
                },
            );
            commands
                .entity(bevy_scene_entity)
                .try_insert(GltfProcessed {
                    animation_roots,
                    instance_id: Some(*instance),
                    named_nodes,
                });
            *tracker.0.entry("Live Meshes").or_default() = resource_lookup
                .meshes
                .iter()
                .filter(|(_, data)| meshes.get(data.mesh_id).is_some())
                .count();
        }
    }
}

pub const GLTF_LOADING: &str = "gltfs loading";

#[derive(Component)]
pub struct GltfLoadingCount(pub usize);

fn check_gltfs_ready(
    mut commands: Commands,
    mut scenes: Query<(
        Entity,
        &mut RendererSceneContext,
        Option<&mut GltfLoadingCount>,
    )>,
    unready_gltfs: Query<&SceneEntity, (With<GltfDefinition>, Without<GltfProcessed>)>,
) {
    let mut unready_scenes = HashMap::<Entity, usize>::default();

    for ent in &unready_gltfs {
        *unready_scenes.entry(ent.root).or_default() += 1;
    }

    for (root, mut context, maybe_count) in scenes.iter_mut() {
        if context.tick_number <= 5 {
            if let Some(n) = unready_scenes.get(&root) {
                debug!("{root:?} blocked on gltfs");
                context.blocked.insert(GLTF_LOADING);
                if let Some(mut count) = maybe_count {
                    count.0 = *n;
                } else {
                    commands.entity(root).try_insert(GltfLoadingCount(*n));
                }
                continue;
            }
        }

        context.blocked.remove(GLTF_LOADING);
        if let Some(mut count) = maybe_count {
            count.0 = 0;
        }
    }
}

// debug show the gltf graph
#[allow(clippy::type_complexity, deprecated)]
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

#[derive(Component)]
pub struct GltfNodeRequest(String);

impl From<PbGltfNode> for GltfNodeRequest {
    fn from(value: PbGltfNode) -> Self {
        GltfNodeRequest(value.name)
    }
}

#[derive(Component)]
pub struct GltfNodeRequestRetry;
#[derive(Component)]
pub struct SceneNodeLink(Entity);
#[derive(Component)]
pub struct RendererNodeLink(Entity);

#[derive(Debug)]
pub enum GltfLinkState {
    Pending,
    Failed(&'static str),
    Ready(Entity),
}

#[derive(Component)]
pub struct HiddenMaterial(Handle<SceneMaterial>);

fn expose_gltfs(
    mut commands: Commands,
    new_links: Query<
        (Entity, &SceneEntity, &GltfNodeRequest, &Parent),
        Or<(
            Changed<GltfNodeRequest>,
            Changed<Parent>,
            With<GltfNodeRequestRetry>,
        )>,
    >,
    parents: Query<(Option<&GltfDefinition>, Option<&GltfProcessed>, &Parent)>,
    already_linked: Query<&RendererNodeLink>,
    mut scenes: Query<&mut RendererSceneContext>,
    mut removed: RemovedComponents<GltfNodeRequest>,
    node_data: Query<(
        Option<&Handle<SceneMaterial>>,
        Option<&Handle<Mesh>>,
        Option<&SkinnedMesh>,
    )>,
    mats: Res<Assets<SceneMaterial>>,
    images: Res<Assets<Image>>,
) {
    for e in removed.read() {
        if let Some(mut commands) = commands.get_entity(e) {
            commands.remove::<SceneNodeLink>();
        }
    }

    for (ent, scene_ent, req, parent) in new_links.iter() {
        commands.entity(ent).remove::<SceneNodeLink>();

        let mut parent = parent.get();
        let state = loop {
            // walk up parents until we find a gltf
            let Ok((maybe_gltf, maybe_processed, next)) = parents.get(parent) else {
                break GltfLinkState::Failed("GltfNode is not a child of an entity with a Gltf");
            };

            if let Some(processed) = maybe_processed {
                commands.entity(ent).remove::<GltfNodeRequestRetry>();

                if processed.instance_id.is_none() {
                    // gltf didn't load
                    break GltfLinkState::Failed("Gltf failed to load");
                }

                // gltf loaded, try and get the named node
                let target = processed.named_nodes.get(&req.0).copied();
                match target {
                    None => {
                        warn!("no match for {:?} in {:?}", req.0, processed.named_nodes);
                        break GltfLinkState::Failed("requested node name not found in gtlf");
                    }
                    Some(t) => {
                        if already_linked.get(t).is_ok() {
                            break GltfLinkState::Failed("duplicate node name requested");
                        }
                        break GltfLinkState::Ready(t);
                    }
                }
            }

            if maybe_gltf.is_some() {
                // this is the gltf but it's not ready yet
                commands.entity(ent).insert(GltfNodeRequestRetry);
                break GltfLinkState::Pending;
            }

            // otherwise keep checking parents
            parent = next.get();
        };

        let Ok(mut scene) = scenes.get_mut(scene_ent.root) else {
            warn!("no scene");
            continue;
        };

        debug!("checking {} -> {:?}", req.0, state);
        match state {
            GltfLinkState::Pending => scene.update_crdt(
                SceneComponentId::GLTF_NODE_STATE,
                CrdtType::LWW_ANY,
                scene_ent.id,
                &PbGltfNodeState {
                    state: GltfNodeStateValue::GnsvPending as i32,
                    error: None,
                },
            ),
            GltfLinkState::Failed(err) => scene.update_crdt(
                SceneComponentId::GLTF_NODE_STATE,
                CrdtType::LWW_ANY,
                scene_ent.id,
                &PbGltfNodeState {
                    state: GltfNodeStateValue::GnsvFailed as i32,
                    error: Some(err.to_owned()),
                },
            ),
            GltfLinkState::Ready(e) => {
                scene.update_crdt(
                    SceneComponentId::GLTF_NODE_STATE,
                    CrdtType::LWW_ANY,
                    scene_ent.id,
                    &PbGltfNodeState {
                        state: GltfNodeStateValue::GnsvReady as i32,
                        error: None,
                    },
                );

                let Some(mut target_commands) = commands.get_entity(e) else {
                    warn!("gltf node entity not found");
                    continue;
                };

                debug!("adding");
                target_commands.try_insert(RendererNodeLink(ent));

                let (maybe_material, maybe_mesh, maybe_skin) = node_data.get(e).unwrap_or_default();

                if let Some(mesh) = maybe_mesh {
                    commands.entity(ent).insert(mesh.clone());
                }
                if let Some(skin) = maybe_skin {
                    commands.entity(ent).insert(skin.clone());
                }

                if let Some(material) = maybe_material {
                    // hide
                    commands
                        .entity(e)
                        .remove::<Handle<SceneMaterial>>()
                        .insert(HiddenMaterial(material.clone()));
                    // copy
                    commands.entity(ent).insert(material.clone());
                    // set base
                    let base = mats.get(material.id()).unwrap();
                    commands.entity(ent).insert(BaseMaterial(base.base.clone()));

                    // write to scene
                    let dcl_texture = |h: &Handle<Image>| -> TextureUnion {
                        let path = h.path().unwrap().path();
                        let ipfs_path = IpfsPath::new_from_path(path).unwrap().unwrap();
                        let src = ipfs_path.content_path().unwrap().to_owned();
                        let sampler = if let Some(Image {
                            sampler: ImageSampler::Descriptor(d),
                            ..
                        }) = images.get(h)
                        {
                            d
                        } else {
                            &ImageSamplerDescriptor::default()
                        };
                        TextureUnion {
                            tex: Some(dcl_component::proto_components::common::texture_union::Tex::Texture(Texture {
                                src,
                                wrap_mode: Some(match sampler.address_mode_u {
                                    ImageAddressMode::ClampToEdge => TextureWrapMode::TwmClamp,
                                    ImageAddressMode::Repeat => TextureWrapMode::TwmRepeat,
                                    ImageAddressMode::MirrorRepeat => TextureWrapMode::TwmMirror,
                                    ImageAddressMode::ClampToBorder => TextureWrapMode::TwmClamp,
                                } as i32),
                                filter_mode: Some(match sampler.mag_filter {
                                    ImageFilterMode::Nearest => TextureFilterMode::TfmPoint,
                                    ImageFilterMode::Linear => TextureFilterMode::TfmBilinear,
                                } as i32),
                            })),
                        }
                    };

                    let alpha_test = if let AlphaMode::Mask(m) = base.base.alpha_mode {
                        Some(m)
                    } else {
                        None
                    };

                    scene.update_crdt(
                        SceneComponentId::MATERIAL,
                        CrdtType::LWW_ANY,
                        scene_ent.id,
                        &PbMaterial {
                            material: Some(if base.base.unlit {
                                pb_material::Material::Unlit(pb_material::UnlitMaterial {
                                    texture: base.base.base_color_texture.as_ref().map(dcl_texture),
                                    alpha_test,
                                    cast_shadows: Some(true),
                                    diffuse_color: Some(base.base.base_color.into()),
                                })
                            } else {
                                pb_material::Material::Pbr(pb_material::PbrMaterial {
                                    texture: base.base.base_color_texture.as_ref().map(dcl_texture),
                                    alpha_test,
                                    cast_shadows: Some(true),
                                    alpha_texture: base
                                        .base
                                        .base_color_texture
                                        .as_ref()
                                        .map(dcl_texture),
                                    emissive_texture: base
                                        .base
                                        .emissive_texture
                                        .as_ref()
                                        .map(dcl_texture),
                                    bump_texture: base
                                        .base
                                        .normal_map_texture
                                        .as_ref()
                                        .map(dcl_texture),
                                    albedo_color: Some(base.base.base_color.into()),
                                    emissive_color: Some((base.base.emissive * 0.5).into()),
                                    reflectivity_color: None,
                                    transparency_mode: Some(match base.alpha_mode() {
                                        AlphaMode::Opaque => MaterialTransparencyMode::MtmOpaque,
                                        AlphaMode::Mask(_) => {
                                            MaterialTransparencyMode::MtmAlphaTest
                                        }
                                        _ => MaterialTransparencyMode::MtmAlphaBlend,
                                    }
                                        as i32),
                                    metallic: Some(base.base.metallic),
                                    roughness: Some(base.base.perceptual_roughness),
                                    specular_intensity: None,
                                    emissive_intensity: None,
                                    direct_intensity: None,
                                })
                            }),
                        },
                    );
                }

                commands.entity(ent).insert(SceneNodeLink(e));
            }
        }
    }
}

fn update_gltf_linked_transforms(
    mut commands: Commands,
    gltf_nodes: Query<(
        Entity,
        &RendererNodeLink,
        &ContainerEntity,
        &Parent,
        Option<&HiddenMaterial>,
    )>,
    scene_nodes: Query<(Entity, &SceneNodeLink, &SceneEntity, Ref<Transform>)>,
    mut scenes: Query<&mut RendererSceneContext>,
    gt_helper: TransformHelperPub,
    mut prev_transforms: Local<HashMap<Entity, Transform>>,
) {
    // check gltf nodes and update the scene nodes
    for (linked_entity, link, container, _, maybe_hidden) in gltf_nodes.iter() {
        let Ok(root_relative_gt) =
            gt_helper.compute_global_transform(linked_entity, Some(container.container))
        else {
            warn!("failed to get gt");
            continue;
        };
        let root_relative_transform = root_relative_gt.compute_transform();

        if Some(&root_relative_transform) != prev_transforms.get(&linked_entity) {
            // the gltf node has moved relative to the gltf root since last run, send update to scene node/scene
            let Ok((_, _, scene_node, _)) = scene_nodes.get(link.0) else {
                commands.entity(linked_entity).remove::<RendererNodeLink>();
                // unhide
                if let Some(hidden) = maybe_hidden {
                    commands
                        .entity(linked_entity)
                        .remove::<HiddenMaterial>()
                        .insert(hidden.0.clone());
                }
                warn!("no scene node");
                continue;
            };

            let Ok(mut scene) = scenes.get_mut(container.root) else {
                warn!("no scene");
                continue;
            };

            // update scene
            scene.update_crdt(
                SceneComponentId::TRANSFORM,
                CrdtType::LWW_ANY,
                scene_node.id,
                &DclTransformAndParent::from_bevy_transform_and_parent(
                    &root_relative_transform, // transform relative to container root
                    container.container_id,   // container root
                ),
            );

            // update local copy of the scene entity
            commands
                .entity(link.0)
                .modify_component(move |t: &mut Transform| *t = root_relative_transform);
            prev_transforms.insert(link.0, root_relative_transform);
            debug!("{linked_entity:?} anim -> scene");
        }
        prev_transforms.insert(linked_entity, root_relative_transform);
    }

    // check scene nodes and update the gltf nodes
    for (linked_entity, link, _, transform) in scene_nodes.iter() {
        if !transform.is_changed() {
            continue;
        }

        if Some(&*transform) != prev_transforms.get(&linked_entity) {
            if prev_transforms.contains_key(&linked_entity) {
                // skip scene update when there is also an animation update
                continue;
            }

            let Ok((_, _, container, parent, _)) = gltf_nodes.get(link.0) else {
                warn!("no gltf node");
                continue;
            };

            let Ok(parent_transform) =
                gt_helper.compute_global_transform(parent.get(), Some(container.container))
            else {
                warn!("failed to get gt");
                continue;
            };
            let gt = GlobalTransform::from(transform.compute_affine());
            let node_transform = gt.reparented_to(&parent_transform);

            // update gltf node
            commands
                .entity(link.0)
                .modify_component(move |t: &mut Transform| *t = node_transform);
            prev_transforms.insert(linked_entity, *transform);
            debug!("{linked_entity:?} scene -> gltf");
        }
        prev_transforms.insert(link.0, *transform);
    }
}

fn update_gltf_linked_visibility(
    mut gltf_nodes: Query<&mut Visibility, With<RendererNodeLink>>,
    scene_nodes: Query<
        (&SceneNodeLink, &Visibility),
        (
            Without<RendererNodeLink>,
            Or<(Changed<Visibility>, Changed<SceneNodeLink>)>,
        ),
    >,
) {
    for (link, vis) in scene_nodes.iter() {
        if let Ok(mut target_vis) = gltf_nodes.get_mut(link.0) {
            *target_vis = *vis;
        }
    }
}
