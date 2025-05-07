use std::f32::consts::FRAC_PI_2;

use bevy::{
    prelude::*,
    render::mesh::{skinning::SkinnedMesh, VertexAttributeValues},
    utils::HashMap,
};

use common::{sets::SceneSets, structs::AppConfig};

use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{pb_mesh_renderer, PbMeshRenderer},
    SceneComponentId,
};
use scene_material::{SceneBound, SceneMaterial};

use crate::{gltf_resolver::GltfMeshResolver, renderer_context::RendererSceneContext, SceneEntity};

use self::truncated_cone::TruncatedCone;

use super::AddCrdtInterfaceExt;

pub mod truncated_cone;
pub struct MeshDefinitionPlugin;

#[derive(Component, Debug)]
pub enum MeshDefinition {
    Box { uvs: Vec<[f32; 2]> },
    Cylinder { radius_top: f32, radius_bottom: f32 },
    Plane { uvs: Vec<[f32; 2]> },
    Sphere,
    Gltf { src: String, name: String },
}

#[derive(Resource)]
pub struct MeshPrimitiveDefaults {
    boxx: Handle<Mesh>,
    plane: Handle<Mesh>,
    cylinder: Handle<Mesh>,
    sphere: Handle<Mesh>,
}

impl From<PbMeshRenderer> for MeshDefinition {
    fn from(value: PbMeshRenderer) -> Self {
        match value.mesh {
            Some(pb_mesh_renderer::Mesh::Box(pb_mesh_renderer::BoxMesh { uvs })) => Self::Box {
                uvs: uvs
                    .chunks(2)
                    .map(|chunk| <[f32; 2]>::try_from(chunk).unwrap_or_default())
                    .collect(),
            },
            Some(pb_mesh_renderer::Mesh::Cylinder(pb_mesh_renderer::CylinderMesh {
                radius_bottom,
                radius_top,
            })) => Self::Cylinder {
                radius_top: radius_top.unwrap_or(0.5),
                radius_bottom: radius_bottom.unwrap_or(0.5),
            },
            Some(pb_mesh_renderer::Mesh::Plane(pb_mesh_renderer::PlaneMesh { uvs })) => {
                Self::Plane {
                    uvs: uvs
                        .chunks(2)
                        .map(|chunk| <[f32; 2]>::try_from(chunk).unwrap_or_default())
                        .collect(),
                }
            }
            Some(pb_mesh_renderer::Mesh::Sphere(pb_mesh_renderer::SphereMesh {})) => Self::Sphere,
            Some(pb_mesh_renderer::Mesh::Gltf(pb_mesh_renderer::GltfMesh { gltf_src, name })) => {
                Self::Gltf {
                    src: gltf_src,
                    name,
                }
            }
            _ => Self::Box {
                uvs: Vec::default(),
            },
        }
    }
}

impl Plugin for MeshDefinitionPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbMeshRenderer, MeshDefinition>(
            SceneComponentId::MESH_RENDERER,
            ComponentPosition::EntityOnly,
        );

        let generate_tangents = |mut mesh: Mesh| {
            mesh.generate_tangents().unwrap();
            mesh
        };
        let cube_uvs = |mut mesh: Mesh| {
            let Some(VertexAttributeValues::Float32x2(ref mut uvs)) =
                mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0)
            else {
                panic!()
            };
            let mut uvs = uvs.iter_mut();
            for uv in uvs.by_ref().take(4) {
                *uv = [uv[0], 1.0 - uv[1]];
            }
            for uv in uvs.by_ref().take(4) {
                *uv = [1.0 - uv[0], 1.0 - uv[1]];
            }
            for uv in uvs.by_ref().take(4) {
                *uv = [uv[0], 1.0 - uv[1]];
            }
            for uv in uvs.by_ref().take(4) {
                *uv = [1.0 - uv[0], 1.0 - uv[1]];
            }
            for uv in uvs.by_ref().take(4) {
                *uv = [1.0 - uv[1], uv[0]];
            }
            for uv in uvs.by_ref().take(4) {
                *uv = [uv[1], uv[0]];
            }
            mesh
        };
        let plane_uvs = |mut mesh: Mesh| {
            let Some(VertexAttributeValues::Float32x2(ref mut uvs)) =
                mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0)
            else {
                panic!()
            };
            uvs[0] = [0.0, 1.0];
            uvs[1] = [1.0, 1.0];
            uvs[2] = [1.0, 0.0];
            uvs[3] = [0.0, 0.0];
            uvs[4] = [0.0, 0.0];
            uvs[5] = [1.0, 0.0];
            uvs[6] = [1.0, 1.0];
            uvs[7] = [0.0, 1.0];
            mesh
        };
        let yflip_uvs = |mut mesh: Mesh| {
            let Some(VertexAttributeValues::Float32x2(ref mut uvs)) =
                mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0)
            else {
                panic!()
            };
            for uv in uvs.iter_mut() {
                uv[1] = 1.0 - uv[1];
            }
            mesh
        };
        let xyflip_uvs = |mut mesh: Mesh| {
            let Some(VertexAttributeValues::Float32x2(ref mut uvs)) =
                mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0)
            else {
                panic!()
            };
            for uv in uvs.iter_mut() {
                uv[0] = 1.0 - uv[0];
                uv[1] = 1.0 - uv[1];
            }
            mesh
        };
        let mut assets = app.world_mut().resource_mut::<Assets<Mesh>>();
        let boxx = assets.add(generate_tangents(cube_uvs(Cuboid::default().into())));
        let cylinder = assets.add(generate_tangents(yflip_uvs(Cylinder::default().into())));
        let plane = assets.add(generate_tangents(plane_uvs(
            Cuboid::default()
                .mesh()
                .build()
                .scaled_by(Vec3::new(1.0, 1.0, 0.0)),
        )));
        let sphere = assets.add(xyflip_uvs(generate_tangents(
            Sphere::new(0.5)
                .mesh()
                .uv(36, 18)
                .rotated_by(Quat::from_rotation_x(-FRAC_PI_2)),
        )));
        app.insert_resource(MeshPrimitiveDefaults {
            boxx,
            plane,
            cylinder,
            sphere,
        });

        app.add_systems(Update, update_mesh.in_set(SceneSets::PostLoop));
    }
}

#[derive(Component)]
pub struct RetryMeshDefinition;

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn update_mesh(
    mut commands: Commands,
    new_primitives: Query<
        (
            Entity,
            &SceneEntity,
            &MeshDefinition,
            Option<&Handle<SceneMaterial>>,
            Option<&Handle<Mesh>>,
        ),
        Or<(Changed<MeshDefinition>, With<RetryMeshDefinition>)>,
    >,
    mut removed_primitives: RemovedComponents<MeshDefinition>,
    mut meshes: ResMut<Assets<Mesh>>,
    defaults: Res<MeshPrimitiveDefaults>,
    mut default_material: Local<HashMap<Entity, Handle<SceneMaterial>>>,
    mut materials: ResMut<Assets<SceneMaterial>>,
    scenes: Query<&RendererSceneContext>,
    config: Res<AppConfig>,
    mut gltf_mesh_resolver: GltfMeshResolver,
) {
    for (ent, scene_ent, prim, maybe_material, maybe_existing_mesh) in new_primitives.iter() {
        commands.entity(ent).remove::<RetryMeshDefinition>();
        let handle = match prim {
            MeshDefinition::Box { uvs } => {
                if uvs.len() != 24 {
                    defaults.boxx.clone()
                } else {
                    let mut mesh = Mesh::from(bevy::math::primitives::Cuboid::default());
                    let Some(VertexAttributeValues::Float32x2(mesh_uvs)) =
                        mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0)
                    else {
                        panic!("uvs are not f32x2")
                    };
                    for (from, to) in [
                        (0, 17),
                        (1, 16),
                        (2, 19),
                        (3, 18),
                        (4, 22),
                        (5, 23),
                        (6, 20),
                        (7, 21),
                        (8, 14),
                        (9, 13),
                        (10, 12),
                        (11, 15),
                        (12, 9),
                        (13, 10),
                        (14, 11),
                        (15, 8),
                        (16, 4),
                        (17, 5),
                        (18, 6),
                        (19, 7),
                        (20, 3),
                        (21, 2),
                        (22, 1),
                        (23, 0),
                    ] {
                        mesh_uvs[to][0] = uvs[from][0];
                        mesh_uvs[to][1] = 1.0 - uvs[from][1];
                    }
                    meshes.add(mesh)
                }
            }
            MeshDefinition::Cylinder {
                radius_bottom,
                radius_top,
            } => {
                if *radius_bottom == 0.5 && *radius_top == 0.5 {
                    defaults.cylinder.clone()
                } else {
                    meshes.add(Mesh::from(TruncatedCone {
                        base_radius: *radius_bottom,
                        tip_radius: *radius_top,
                        ..Default::default()
                    }))
                }
            }
            MeshDefinition::Plane { uvs } => {
                if uvs.len() != 8 {
                    defaults.plane.clone()
                } else {
                    let mut mesh = Cuboid::default()
                        .mesh()
                        .build()
                        .scaled_by(Vec3::new(1.0, 1.0, 0.0));
                    let Some(VertexAttributeValues::Float32x2(mesh_uvs)) =
                        mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0)
                    else {
                        panic!("uvs are not f32x2")
                    };
                    for (from, to) in [
                        (0, 0),
                        (1, 3),
                        (2, 2),
                        (3, 1),
                        (4, 6),
                        (5, 5),
                        (6, 4),
                        (7, 7),
                    ] {
                        mesh_uvs[to][0] = uvs[from][0];
                        mesh_uvs[to][1] = 1.0 - uvs[from][1];
                    }
                    meshes.add(mesh)
                }
            }
            MeshDefinition::Sphere => defaults.sphere.clone(),
            MeshDefinition::Gltf { src, name } => {
                let Ok(scene) = scenes.get(scene_ent.root) else {
                    continue;
                };
                let Ok(maybe_mesh) = gltf_mesh_resolver.resolve_mesh(src, &scene.hash, name) else {
                    warn!("failed to load gltf for mesh");
                    continue;
                };
                let Some(h_mesh) = maybe_mesh else {
                    commands.entity(ent).try_insert(RetryMeshDefinition);
                    continue;
                };
                // remove skin if mesh changed
                if maybe_existing_mesh != Some(&h_mesh) {
                    commands.entity(ent).remove::<SkinnedMesh>();
                }
                h_mesh
            }
        };
        commands.entity(ent).try_insert(handle);

        if maybe_material.is_none() {
            let mat = default_material.entry(scene_ent.root).or_insert_with(|| {
                let bounds = scenes
                    .get(scene_ent.root)
                    .map(|c| c.bounds.clone())
                    .unwrap_or_default();
                materials.add(SceneMaterial {
                    base: StandardMaterial {
                        ..Default::default()
                    },
                    extension: SceneBound::new(bounds, config.graphics.oob),
                })
            });

            commands.entity(ent).try_insert(mat.clone());
        }
    }

    for ent in removed_primitives.read() {
        if let Some(mut e) = commands.get_entity(ent) {
            e.remove::<Handle<Mesh>>();
        }
    }

    default_material.retain(|scene, _| scenes.get(*scene).is_ok());
}
