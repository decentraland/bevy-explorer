use bevy::{prelude::*, render::mesh::VertexAttributeValues};

use crate::{
    dcl::interface::ComponentPosition,
    dcl_component::{
        proto_components::sdk::components::{pb_mesh_renderer, PbMeshRenderer},
        SceneComponentId,
    },
    scene_runner::SceneSets,
};

use super::AddCrdtInterfaceExt;

pub struct MeshDefinitionPlugin;

#[derive(Component, Debug)]
pub enum MeshDefinition {
    Box { uvs: Vec<[f32; 2]> },
    Cylinder { radius_top: f32, radius_bottom: f32 },
    Plane { uvs: Vec<[f32; 2]> },
    Sphere,
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
                radius_top: radius_top.unwrap_or(1.0),
                radius_bottom: radius_bottom.unwrap_or(1.0),
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

        let mut assets = app.world.resource_mut::<Assets<Mesh>>();
        let boxx = assets.add(shape::Cube::default().into());
        let cylinder = assets.add(shape::Cylinder::default().into()); // TODO make a custom cylinder that supports different top and bottom radius
        let plane = assets.add(shape::Plane::default().into());
        let sphere = assets.add(shape::UVSphere::default().into());
        app.insert_resource(MeshPrimitiveDefaults {
            boxx,
            plane,
            cylinder,
            sphere,
        });

        app.add_system(update_mesh.in_set(SceneSets::PostLoop));
    }
}

fn update_mesh(
    mut commands: Commands,
    new_primitives: Query<(Entity, &MeshDefinition), Changed<MeshDefinition>>,
    mut removed_primitives: RemovedComponents<MeshDefinition>,
    mut meshes: ResMut<Assets<Mesh>>,
    defaults: Res<MeshPrimitiveDefaults>,
) {
    for (ent, prim) in new_primitives.iter() {
        let handle = match prim {
            MeshDefinition::Box { uvs } => {
                if uvs.is_empty() {
                    defaults.boxx.clone()
                } else {
                    let mut mesh = Mesh::from(shape::Cube::default());
                    let Some(VertexAttributeValues::Float32x2(mesh_uvs)) = mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0) else { panic!("uvs are not f32x2") };
                    for (attr, uv) in mesh_uvs.iter_mut().zip(uvs) {
                        *attr = *uv
                    }
                    meshes.add(mesh)
                }
            }
            MeshDefinition::Cylinder {
                radius_bottom,
                radius_top,
            } => {
                if *radius_bottom == 1.0 && *radius_top == 1.0 {
                    defaults.cylinder.clone()
                } else {
                    todo!()
                }
            }
            MeshDefinition::Plane { uvs } => {
                if uvs.is_empty() {
                    defaults.plane.clone()
                } else {
                    let mut mesh = Mesh::from(shape::Plane::default());
                    let Some(VertexAttributeValues::Float32x2(mesh_uvs)) = mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0) else { panic!("uvs are not f32x2") };
                    for (attr, uv) in mesh_uvs.iter_mut().zip(uvs) {
                        *attr = *uv
                    }
                    meshes.add(mesh)
                }
            }
            MeshDefinition::Sphere => defaults.sphere.clone(),
        };
        commands.entity(ent).insert(handle);
    }

    for ent in removed_primitives.iter() {
        if let Some(mut e) = commands.get_entity(ent) {
            e.remove::<Handle<Mesh>>();
        }
    }
}