// TODO
// - gltf collider flags
// - clean up of cached colliders (when mesh is unloaded?)
use std::{
    collections::BTreeMap,
    f32::consts::PI,
    hash::{Hash, Hasher},
};

use bevy::{
    animation::AnimationTarget,
    asset::LoadState,
    gltf::{Gltf, GltfExtras, GltfLoaderSettings},
    pbr::ExtendedMaterial,
    prelude::*,
    render::{
        mesh::{skinning::SkinnedMesh, Indices, VertexAttributeValues},
        render_asset::RenderAssetUsages,
        view::NoFrustumCulling,
    },
    scene::{scene_spawner_system, InstanceId},
    transform::TransformSystem,
    utils::HashMap,
};
use common::{anim_last_system, structs::AppConfig, util::ModifyComponentExt};
use rapier3d_f64::prelude::*;
use serde::Deserialize;

use crate::{
    renderer_context::RendererSceneContext,
    update_world::{
        lights::{Light, SpotlightAngles},
        material::{dcl_material_from_standard_material, BaseMaterial},
    },
    ContainerEntity, SceneEntity, SceneSets,
};
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::sdk::components::{
        common::LoadingState, pb_material, pb_mesh_collider, pb_mesh_renderer, ColliderLayer,
        GltfNodeStateValue, PbGltfContainer, PbGltfContainerLoadingState, PbGltfNode,
        PbGltfNodeState, PbLight, PbMaterial, PbMeshCollider, PbMeshRenderer, PbSpotlight,
    },
    transform_and_parent::DclTransformAndParent,
    SceneComponentId, SceneEntityId,
};
use ipfs::{EntityDefinition, IpfsAssetServer};
use scene_material::{SceneBound, SceneMaterial};

use super::{
    animation::Clips,
    lights::LightEntity,
    mesh_collider::{MeshCollider, MeshColliderShape},
    transform_and_parent::TransformHelperPub,
    AddCrdtInterfaceExt, ComponentTracker,
};

pub struct GltfDefinitionPlugin;

#[derive(SystemSet, Debug, PartialEq, Eq, Hash, Clone)]
pub struct GltfLinkSet;

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
            (expose_gltfs, update_gltf_linked_visibility)
                .chain()
                .in_set(SceneSets::PostLoop),
        );
        app.add_systems(
            PostUpdate,
            (update_gltf_linked_transforms, apply_deferred)
                .chain()
                .in_set(GltfLinkSet)
                .after(anim_last_system!())
                .before(TransformSystem::TransformPropagate),
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
                    node_paths: Default::default(),
                    mesh_names: Default::default(),
                    material_names: Default::default(),
                    skin_names: Default::default(),
                    animation_names: Default::default(),
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
                s.load_lights = true;
                s.load_materials = RenderAssetUsages::RENDER_WORLD;
                s.include_source = true;
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
            bevy::asset::LoadState::Failed(e) => {
                warn!("failed to process gltf {}: {}", def.0.src, e);
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

#[derive(Component, Debug)]
pub struct GltfMaterialName(String);

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
        (
            Entity,
            &SceneEntity,
            &GltfLoaded,
            &GltfDefinition,
            &Handle<Gltf>,
        ),
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
        Option<&AnimationTarget>,
        Option<&DirectionalLight>,
        Option<&PointLight>,
        Option<&SpotLight>,
    )>,
    (base_mats, mut bound_mats, mut graphs, mut meshes): (
        Res<Assets<StandardMaterial>>,
        ResMut<Assets<SceneMaterial>>,
        ResMut<Assets<AnimationGraph>>,
        ResMut<Assets<Mesh>>,
    ),
    scene_spawner: Res<SceneSpawner>,
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
    gltfs: Res<Assets<Gltf>>,
    animation_clips: Res<Assets<AnimationClip>>,
) {
    for (bevy_scene_entity, dcl_scene_entity, loaded, definition, h_gltf) in ready_gltfs.iter() {
        if loaded.0.is_none() {
            // nothing to process
            commands
                .entity(bevy_scene_entity)
                .try_insert(GltfProcessed::default());
            continue;
        }
        let instance = loaded.0.as_ref().unwrap();
        if scene_spawner.instance_is_ready(*instance) {
            let gltf = gltfs.get(h_gltf).unwrap();

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

            // track if any animations exist
            let mut has_animations = false;

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
                    maybe_target,
                    maybe_directional,
                    maybe_point,
                    maybe_spot,
                )) = gltf_spawned_entities.get(spawned_ent)
                {
                    // remove directional lights
                    if maybe_directional.is_some() {
                        commands.entity(spawned_ent).despawn_recursive();
                        continue;
                    }

                    // enable shadows for other lights
                    if let Some(point) = maybe_point {
                        commands.entity(spawned_ent).try_insert((
                            PointLight {
                                shadows_enabled: true,
                                ..*point
                            },
                            LightEntity {
                                scene: dcl_scene_entity.root,
                            },
                        ));
                    }
                    if let Some(spot) = maybe_spot {
                        commands.entity(spawned_ent).try_insert((
                            SpotLight {
                                shadows_enabled: true,
                                ..*spot
                            },
                            LightEntity {
                                scene: dcl_scene_entity.root,
                            },
                        ));
                    }

                    // collect named nodes to push to scene on request
                    if let Some(name) = maybe_name {
                        let mut name = name.to_string();
                        let mut ptr = parent.get();
                        while ptr != bevy_scene_entity {
                            let (maybe_name, _, parent, ..) =
                                gltf_spawned_entities.get(ptr).unwrap();
                            if let Some(parent_name) = maybe_name {
                                name = format!("{parent_name}/{name}");
                            }
                            ptr = parent.get();
                        }
                        named_nodes.insert(name, spawned_ent);
                    }

                    // children of root nodes -> rotate
                    if parent.get() == bevy_scene_entity {
                        let mut rotated = *transform;
                        rotated
                            .rotate_around(Vec3::ZERO, Quat::from_rotation_y(std::f32::consts::PI));
                        commands.entity(spawned_ent).try_insert(rotated);
                    }

                    // retarget animations to our manually added root player
                    if let Some(target) = maybe_target {
                        commands.entity(spawned_ent).insert(AnimationTarget {
                            player: bevy_scene_entity,
                            ..*target
                        });
                    }

                    // if there is an animation player, record the entity (bevy-specific hack)
                    if maybe_player.is_some() {
                        if let Some(name) = maybe_name {
                            debug!("animator found on {name} node of {}", definition.0.src);
                            // animation_roots.insert((spawned_ent, name.clone()));
                            has_animations = true;
                            commands.entity(spawned_ent).remove::<AnimationPlayer>();
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
                        let material_name = gltf
                            .named_materials
                            .iter()
                            .find(|(_, handle)| handle == &h_material)
                            .map(|(name, _)| name.to_string())
                            .unwrap_or_else(|| {
                                let ix = gltf
                                    .materials
                                    .iter()
                                    .enumerate()
                                    .find(|(_, handle)| handle == &h_material)
                                    .map(|(ix, _)| ix)
                                    .unwrap_or(usize::MAX); // TODO handle inverted materials (make a list in Gltf, choose based on scale of the target?)
                                format!("Material{ix}")
                            });

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
                        commands
                            .entity(spawned_ent)
                            .insert(h_scene_material)
                            .insert(GltfMaterialName(material_name));
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
                        let shape = mesh_to_parry_shape(mesh_data);

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

            // collect named assets and assign names to unnamed
            let mesh_names = gltf
                .source
                .as_ref()
                .unwrap()
                .meshes()
                .enumerate()
                .map(|(ix, m)| {
                    m.name()
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| format!("Mesh{ix}"))
                })
                .collect::<Vec<_>>();

            let skin_names = gltf
                .source
                .as_ref()
                .unwrap()
                .skins()
                .enumerate()
                .map(|(ix, s)| {
                    s.name()
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| format!("Skin{ix}"))
                })
                .collect::<Vec<_>>();

            let material_names = gltf
                .source
                .as_ref()
                .unwrap()
                .materials()
                .enumerate()
                .map(|(ix, m)| {
                    m.name()
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| format!("Material{ix}"))
                })
                .collect::<Vec<_>>();

            let animation_names = gltf
                .source
                .as_ref()
                .unwrap()
                .animations()
                .enumerate()
                .map(|(ix, a)| {
                    a.name()
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| format!("Animation{ix}"))
                })
                .collect::<Vec<_>>();

            // collect named skins and assign names to unnamed meshes
            context.update_crdt(
                SceneComponentId::GLTF_CONTAINER_LOADING_STATE,
                CrdtType::LWW_ANY,
                dcl_scene_entity.id,
                &PbGltfContainerLoadingState {
                    current_state: LoadingState::Finished as i32,
                    node_paths: named_nodes.keys().cloned().collect(),
                    mesh_names,
                    material_names,
                    skin_names,
                    animation_names,
                },
            );
            commands
                .entity(bevy_scene_entity)
                .try_insert(GltfProcessed {
                    instance_id: Some(*instance),
                    named_nodes,
                });
            if has_animations && !gltf.animations.is_empty() {
                let mut graph = AnimationGraph::new();
                let animation_clips = Clips {
                    default: gltf
                        .animations
                        .first()
                        .cloned()
                        .map(|clip| graph.add_clip(clip, 1.0, graph.root)),
                    named: gltf
                        .named_animations
                        .iter()
                        .map(|(name, clip)| {
                            let duration = animation_clips
                                .get(clip)
                                .map(|clip| clip.duration())
                                .unwrap_or(0.0);
                            (
                                name.to_string(),
                                (graph.add_clip(clip.clone(), 1.0, graph.root), duration),
                            )
                        })
                        .collect(),
                };
                commands.entity(bevy_scene_entity).insert((
                    AnimationPlayer::default(),
                    graphs.add(graph),
                    animation_clips,
                ));
            }
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
                            scene_entity_query
                                .get(*c)
                                .map(|(_, _, _, skin, _)| skin.is_some())
                                .unwrap_or(false),
                            scene_entity_query
                                .get(*c)
                                .map(|(_, _, _, _, t)| t.scale)
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

pub fn mesh_to_parry_shape(mesh_data: &Mesh) -> SharedShape {
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

    SharedShape::trimesh_with_flags(positions_parry, indices_parry, TriMeshFlags::empty())
}

#[derive(Component)]
pub struct GltfNodeRequest(String);

impl From<PbGltfNode> for GltfNodeRequest {
    fn from(value: PbGltfNode) -> Self {
        GltfNodeRequest(value.path)
    }
}

#[derive(Component)]
pub struct GltfNodeRequestRetry;
#[derive(Component)]
pub struct SceneNodeLink {
    gltf_entity: Entity,
    gltf_parent: Entity,
    scene_parent: (Entity, SceneEntityId),
}
#[derive(Component)]
pub struct RendererNodeLink(Entity);

#[derive(Debug)]
pub enum GltfLinkState<'a> {
    Pending,
    Failed(&'static str),
    Ready {
        gltf_entity: Entity,
        src: &'a str,
        gltf_node_parent: Entity,
        scene_parent: (Entity, SceneEntityId),
    },
}

#[derive(Component)]
pub struct HiddenMaterial(Handle<SceneMaterial>);

#[derive(Component)]
pub struct HiddenCollider(MeshCollider);

#[derive(Component)]
pub struct HiddenPointLight(PointLight);

#[derive(Component)]
pub struct HiddenSpotLight(SpotLight);

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
    parents: Query<(
        Option<&GltfDefinition>,
        Option<&GltfProcessed>,
        Option<&GltfNodeRequest>,
        &SceneEntity,
        &Parent,
    )>,
    already_linked: Query<&RendererNodeLink>,
    mut scenes: Query<&mut RendererSceneContext>,
    mut removed: RemovedComponents<GltfNodeRequest>,
    node_data: Query<(
        Option<&Handle<SceneMaterial>>,
        Option<&GltfMaterialName>,
        Option<&Handle<Mesh>>,
        Option<&SkinnedMesh>,
        Option<&MeshCollider>,
        Option<&Name>,
        Option<&PointLight>,
        Option<&SpotLight>,
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
        let mut scene_parent = None; // (gltf_path, entity, scene_entity_id)
        let state = loop {
            // walk up parents until we find a gltf container
            let Ok((maybe_gltf, maybe_processed, maybe_parent_request, scene_entity, next)) =
                parents.get(parent)
            else {
                break GltfLinkState::Failed(
                    "GltfNode is not a child of an entity with a GltfContainer or GltfNode",
                );
            };

            if maybe_parent_request.is_none() && maybe_gltf.is_none() {
                break GltfLinkState::Failed(
                    "GltfNode is not a child of an entity with a GltfContainer or GltfNode",
                );
            }

            if let Some(parent_request) = maybe_parent_request {
                // record scene parent
                if scene_parent.is_none() {
                    scene_parent = Some((parent_request.0.clone(), parent, scene_entity.id));
                }
            }

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

                        // make sure the new linked node is a child of the gltf parent (if any)
                        let gltf_node_parent = if let Some((parent_path, ..)) =
                            scene_parent.as_ref()
                        {
                            if req.0.strip_prefix(parent_path).is_none() {
                                break GltfLinkState::Failed(
                                    "requested GltfNode is not a descendent of the parent GltfNode",
                                );
                            }

                            let Some(parent_node) = processed.named_nodes.get(parent_path) else {
                                break GltfLinkState::Failed(
                                    "requested GltfNode parent GltfNode not found",
                                );
                            };

                            *parent_node
                        } else {
                            parent
                        };

                        break GltfLinkState::Ready {
                            gltf_entity: t,
                            src: &maybe_gltf.unwrap().0.src,
                            gltf_node_parent,
                            scene_parent: scene_parent
                                .map(|(_, scene_entity, scene_entity_id)| {
                                    (scene_entity, scene_entity_id)
                                })
                                .unwrap_or((parent, scene_entity.id)),
                        };
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
            GltfLinkState::Ready {
                gltf_entity,
                src,
                gltf_node_parent: gltf_parent,
                scene_parent,
            } => {
                scene.update_crdt(
                    SceneComponentId::GLTF_NODE_STATE,
                    CrdtType::LWW_ANY,
                    scene_ent.id,
                    &PbGltfNodeState {
                        state: GltfNodeStateValue::GnsvReady as i32,
                        error: None,
                    },
                );

                let Some(mut target_commands) = commands.get_entity(gltf_entity) else {
                    warn!("gltf node entity not found");
                    continue;
                };

                debug!("adding");
                debug!("link");
                target_commands.try_insert(RendererNodeLink(ent));

                let (
                    maybe_material,
                    maybe_mat_name,
                    maybe_mesh,
                    maybe_skin,
                    maybe_collider,
                    maybe_name,
                    maybe_point,
                    maybe_spot,
                ) = node_data.get(gltf_entity).unwrap_or_default();

                if let Some(mesh) = maybe_mesh {
                    debug!("link mesh");
                    commands.entity(ent).insert(mesh.clone());
                    // write to scene
                    scene.update_crdt(
                        SceneComponentId::MESH_RENDERER,
                        CrdtType::LWW_ANY,
                        scene_ent.id,
                        &PbMeshRenderer {
                            mesh: Some(pb_mesh_renderer::Mesh::Gltf(pb_mesh_renderer::GltfMesh {
                                gltf_src: src.to_owned(),
                                name: maybe_name
                                    .map(|n| n.to_string())
                                    .unwrap_or_else(|| "???".to_owned()),
                            })),
                        },
                    );
                }
                if let Some(skin) = maybe_skin {
                    debug!("link skin");
                    commands.entity(ent).insert(skin.clone());
                }
                if let Some(collider) = maybe_collider {
                    debug!("link collider");
                    // hide
                    commands
                        .entity(gltf_entity)
                        .remove::<MeshCollider>()
                        .insert(HiddenCollider(collider.clone()));
                    // copy
                    commands.entity(ent).insert(collider.clone());
                    // write to scene
                    scene.update_crdt(
                        SceneComponentId::MESH_COLLIDER,
                        CrdtType::LWW_ANY,
                        scene_ent.id,
                        &PbMeshCollider {
                            collision_mask: Some(collider.collision_mask),
                            mesh: Some(pb_mesh_collider::Mesh::Gltf(pb_mesh_collider::GltfMesh {
                                gltf_src: src.to_owned(),
                                name: collider
                                    .mesh_name
                                    .clone()
                                    .unwrap_or_else(|| "???".to_owned()),
                            })),
                        },
                    )
                }
                if let Some(material) = maybe_material {
                    debug!("link material ({} / {:?})", src, maybe_mat_name);
                    // hide
                    commands
                        .entity(gltf_entity)
                        .remove::<Handle<SceneMaterial>>()
                        .insert(HiddenMaterial(material.clone()));
                    // copy
                    commands.entity(ent).insert(material.clone());
                    // set base
                    let base = mats.get(material.id()).unwrap();
                    commands.entity(ent).insert(BaseMaterial {
                        material: base.base.clone(),
                        gltf: src.to_owned(),
                        name: maybe_mat_name.unwrap().0.clone(),
                    });

                    // write to scene
                    scene.update_crdt(
                        SceneComponentId::MATERIAL,
                        CrdtType::LWW_ANY,
                        scene_ent.id,
                        &PbMaterial {
                            material: Some(dcl_material_from_standard_material(
                                &base.base, &images,
                            )),
                            gltf: maybe_mat_name.map(|name| pb_material::GltfMaterial {
                                gltf_src: src.to_owned(),
                                name: name.0.clone(),
                            }),
                        },
                    );
                }

                if let Some(point) = maybe_point {
                    debug!("link point light ({})", src);
                    // hide
                    commands
                        .entity(gltf_entity)
                        .remove::<PointLight>()
                        .insert(HiddenPointLight(*point));
                    // copy
                    commands.entity(ent).insert(Light {
                        enabled: true,
                        illuminance: Some(point.intensity / (4.0 * PI)),
                        shadows: Some(true),
                        color: Some(point.color),
                    });
                    // write to scene
                    scene.update_crdt(
                        SceneComponentId::LIGHT,
                        CrdtType::LWW_ANY,
                        scene_ent.id,
                        &PbLight {
                            enabled: None,
                            illuminance: Some(point.intensity / (4.0 * PI)),
                            shadows: Some(true),
                            color: Some(point.color.into()),
                        },
                    );
                }

                if let Some(spot) = maybe_spot {
                    debug!("link spot light ({})", src);
                    // hide
                    commands
                        .entity(gltf_entity)
                        .remove::<SpotLight>()
                        .insert(HiddenSpotLight(*spot));
                    // copy
                    commands.entity(ent).insert((
                        Light {
                            enabled: true,
                            illuminance: Some(spot.intensity / (4.0 * PI)),
                            shadows: Some(true),
                            color: Some(spot.color),
                        },
                        SpotlightAngles {
                            inner_angle: spot.inner_angle,
                            outer_angle: spot.outer_angle,
                        },
                    ));
                    // write to scene
                    scene.update_crdt(
                        SceneComponentId::LIGHT,
                        CrdtType::LWW_ANY,
                        scene_ent.id,
                        &PbLight {
                            enabled: None,
                            illuminance: Some(spot.intensity / (4.0 * PI)),
                            shadows: Some(true),
                            color: Some(spot.color.into()),
                        },
                    );
                    scene.update_crdt(
                        SceneComponentId::SPOTLIGHT,
                        CrdtType::LWW_ANY,
                        scene_ent.id,
                        &PbSpotlight {
                            angle: spot.outer_angle,
                            inner_angle: Some(spot.inner_angle),
                        },
                    );
                }

                commands.entity(ent).insert(SceneNodeLink {
                    gltf_entity,
                    gltf_parent,
                    scene_parent,
                });
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
        Option<&HiddenCollider>,
        Option<&HiddenPointLight>,
        Option<&HiddenSpotLight>,
    )>,
    scene_nodes: Query<(&SceneEntity, Ref<Transform>, &Parent, &SceneNodeLink)>,
    mut scenes: Query<&mut RendererSceneContext>,
    gt_helper: TransformHelperPub,
    mut stored_transforms_and_parents: Local<HashMap<Entity, (Transform, Vec<Entity>)>>,
) {
    let mut prev_transforms_and_parents = std::mem::take(&mut *stored_transforms_and_parents);

    #[derive(Clone)]
    struct UpdateData {
        state: MoveState,
        gltf_parent: Entity,
        scene_entity: Entity,
        scene_entity_id: SceneEntityId,
        scene: Entity,
        transform_root: Entity,
        scene_parent_scene_id: SceneEntityId,
        root_relative_transform: Transform,
    }

    #[derive(Clone, PartialEq, Debug)]
    enum MoveState {
        Anim,
        Scene,
    }

    let mut node_movement_state = HashMap::default();

    // init parents and positions, check for changes
    for (
        gltf_entity,
        link,
        container,
        parent,
        maybe_hidden_material,
        maybe_hidden_collider,
        maybe_hidden_point,
        maybe_hidden_spot,
    ) in gltf_nodes.iter()
    {
        let unlink = |gltf_entity: Entity, commands: &mut Commands| {
            // scene link removed, reset
            commands.entity(gltf_entity).remove::<RendererNodeLink>();
            // unhide
            if let Some(hidden) = maybe_hidden_material {
                commands
                    .entity(gltf_entity)
                    .remove::<HiddenMaterial>()
                    .insert(hidden.0.clone());
            }
            if let Some(hidden) = maybe_hidden_collider {
                commands
                    .entity(gltf_entity)
                    .remove::<HiddenCollider>()
                    .insert(hidden.0.clone());
            }
            if let Some(hidden) = maybe_hidden_point {
                commands
                    .entity(gltf_entity)
                    .remove::<HiddenPointLight>()
                    .insert(hidden.0);
            }
            if let Some(hidden) = maybe_hidden_spot {
                commands
                    .entity(gltf_entity)
                    .remove::<HiddenSpotLight>()
                    .insert(hidden.0);
            }
        };

        let Ok((scene_entity_id, scene_transform, scene_parent, scene_link)) =
            scene_nodes.get(link.0)
        else {
            unlink(gltf_entity, &mut commands);
            warn!("no scene node");
            continue;
        };

        if scene_parent.get() != scene_link.scene_parent.0 {
            unlink(gltf_entity, &mut commands);
            warn!("linked entity moved out of container");
            continue;
        }

        match prev_transforms_and_parents.remove(&gltf_entity) {
            Some((prev_transform, parents)) => {
                let Ok(root_relative_gt) =
                    gt_helper.compute_global_transform(gltf_entity, Some(scene_link.gltf_parent))
                else {
                    warn!("failed to get gt");
                    continue;
                };
                let gltf_root_relative_transform = root_relative_gt.compute_transform();
                debug!("[{gltf_entity:?}] rrt {gltf_root_relative_transform:?}");
                debug!("[{gltf_entity:?}] prev: {prev_transform:?}");
                let update = if prev_transform != gltf_root_relative_transform {
                    Some((MoveState::Anim, gltf_root_relative_transform))
                } else if scene_transform.is_changed() && prev_transform != *scene_transform {
                    Some((MoveState::Scene, *scene_transform))
                } else {
                    None
                };

                if let Some((state, root_relative_transform)) = update {
                    node_movement_state.insert(
                        gltf_entity,
                        UpdateData {
                            state,
                            root_relative_transform,
                            gltf_parent: parent.get(),
                            scene_entity: link.0,
                            scene_entity_id: scene_entity_id.id,
                            scene: container.root,
                            transform_root: scene_link.gltf_parent,
                            scene_parent_scene_id: scene_link.scene_parent.1,
                        },
                    );
                    stored_transforms_and_parents
                        .insert(gltf_entity, (root_relative_transform, parents));
                } else {
                    stored_transforms_and_parents.insert(gltf_entity, (prev_transform, parents));
                }
            }
            None => {
                debug!("new link {gltf_entity:?}");
                let Ok((root_relative_gt, parents)) = gt_helper
                    .compute_global_transform_with_ancestors(
                        gltf_entity,
                        Some(scene_link.gltf_parent),
                    )
                else {
                    warn!("failed to get ancestors");
                    continue;
                };
                let root_relative_transform = root_relative_gt.compute_transform();
                stored_transforms_and_parents
                    .insert(gltf_entity, (root_relative_transform, parents));
                node_movement_state.insert(
                    gltf_entity,
                    UpdateData {
                        state: MoveState::Anim,
                        root_relative_transform,
                        gltf_parent: parent.get(),
                        scene_entity: link.0,
                        scene_entity_id: scene_entity_id.id,
                        scene: container.root,
                        transform_root: scene_link.gltf_parent,
                        scene_parent_scene_id: scene_link.scene_parent.1,
                    },
                );
            }
        }
    }

    let mut updated_transforms = HashMap::default();

    // update
    while !node_movement_state.is_empty() {
        node_movement_state = node_movement_state
            .clone()
            .into_iter()
            .filter_map(|(gltf_ent, data)| {
                match data.state {
                    MoveState::Anim => {
                        // update scene / scene entity
                        let Ok(mut scene) = scenes.get_mut(data.scene) else {
                            warn!("no scene");
                            return None;
                        };

                        scene.update_crdt(
                            SceneComponentId::TRANSFORM,
                            CrdtType::LWW_ANY,
                            data.scene_entity_id,
                            &DclTransformAndParent::from_bevy_transform_and_parent(
                                &data.root_relative_transform, // transform relative to container root
                                data.scene_parent_scene_id,
                            ),
                        );

                        debug!("[{gltf_ent:?}] r -> s {:?}", data.root_relative_transform);
                        commands.entity(data.scene_entity).modify_component(
                            move |t: &mut Transform| *t = data.root_relative_transform,
                        );

                        None
                    }
                    MoveState::Scene => {
                        let parents = &stored_transforms_and_parents.get(&gltf_ent).unwrap().1;
                        for parent in parents.iter() {
                            if node_movement_state.get(parent).is_some() {
                                // retain till all parents are processed
                                return Some((gltf_ent, data));
                            }
                        }

                        // update gltf entity
                        let Ok(parent_transform) = gt_helper
                            .compute_global_transform_with_overrides(
                                data.gltf_parent,
                                Some(data.transform_root),
                                &updated_transforms,
                            )
                        else {
                            return None;
                        };
                        let gltf_node_transform =
                            GlobalTransform::from(data.root_relative_transform)
                                .reparented_to(&parent_transform);

                        updated_transforms.insert(gltf_ent, gltf_node_transform);

                        commands
                            .entity(gltf_ent)
                            .modify_component(move |t: &mut Transform| *t = gltf_node_transform);

                        debug!("[{gltf_ent:?}] s -> r {:?}", gltf_node_transform);

                        // and update stored state with the rrt we will compute next frame, to avoid rounding errors
                        stored_transforms_and_parents.get_mut(&gltf_ent).unwrap().0 = gt_helper
                            .compute_global_transform_with_overrides(
                                gltf_ent,
                                Some(data.transform_root),
                                &updated_transforms,
                            )
                            .unwrap()
                            .compute_transform();
                        None
                    }
                }
            })
            .collect();
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
        if let Ok(mut target_vis) = gltf_nodes.get_mut(link.gltf_entity) {
            *target_vis = *vis;
        }
    }
}
