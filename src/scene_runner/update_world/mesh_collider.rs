use std::borrow::Borrow;

use bevy::{
    pbr::{wireframe::Wireframe, NotShadowCaster, NotShadowReceiver},
    prelude::*,
    render::mesh::VertexAttributeValues,
    utils::HashMap,
};
use bevy_console::ConsoleCommand;
use rapier3d::prelude::*;

use crate::{
    console::DoAddConsoleCommand,
    dcl::interface::ComponentPosition,
    dcl_component::{
        proto_components::sdk::components::{pb_mesh_collider, ColliderLayer, PbMeshCollider},
        SceneComponentId, SceneEntityId,
    },
    scene_runner::{
        update_world::mesh_renderer::truncated_cone::TruncatedCone, ContainerEntity,
        DeletedSceneEntities, RendererSceneContext, SceneSets,
    },
};

use super::AddCrdtInterfaceExt;

pub struct MeshColliderPlugin;

#[derive(Component)]
pub struct MeshCollider {
    pub shape: MeshColliderShape,
    pub collision_mask: u32,
    pub mesh_name: Option<String>,
    pub index: u32,
}

impl Default for MeshCollider {
    fn default() -> Self {
        Self {
            shape: MeshColliderShape::Box,
            collision_mask: ColliderLayer::ClPointer as u32 | ColliderLayer::ClPhysics as u32,
            mesh_name: Default::default(),
            index: Default::default(),
        }
    }
}

// #[derive(Debug)]
pub enum MeshColliderShape {
    Box,
    Cylinder { radius_top: f32, radius_bottom: f32 },
    Plane,
    Sphere,
    Shape(SharedShape, Handle<Mesh>),
}

impl From<PbMeshCollider> for MeshCollider {
    fn from(value: PbMeshCollider) -> Self {
        let shape = match value.mesh {
            Some(pb_mesh_collider::Mesh::Box(_)) => MeshColliderShape::Box,
            Some(pb_mesh_collider::Mesh::Plane(_)) => MeshColliderShape::Plane,
            Some(pb_mesh_collider::Mesh::Sphere(_)) => MeshColliderShape::Sphere,
            Some(pb_mesh_collider::Mesh::Cylinder(pb_mesh_collider::CylinderMesh {
                radius_bottom,
                radius_top,
            })) => MeshColliderShape::Cylinder {
                radius_top: radius_top.unwrap_or(1.0),
                radius_bottom: radius_bottom.unwrap_or(1.0),
            },
            _ => MeshColliderShape::Box,
        };

        Self {
            shape,
            // TODO update to u32
            collision_mask: value
                .collision_mask
                .unwrap_or(ColliderLayer::ClPointer as i32 | ColliderLayer::ClPhysics as i32)
                as u32,
            ..Default::default()
        }
    }
}

impl Plugin for MeshColliderPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbMeshCollider, MeshCollider>(
            SceneComponentId::MESH_COLLIDER,
            ComponentPosition::EntityOnly,
        );

        app.add_system(update_scene_collider_data.in_set(SceneSets::PostInit));
        app.add_system(update_colliders.in_set(SceneSets::PostLoop));

        // update collider transforms before queries and scenes are run, but after global transforms are updated (at end of prior frame)
        app.add_system(update_collider_transforms.in_set(SceneSets::PostInit));

        app.init_resource::<DebugColliders>();
        app.add_console_command::<DebugColliderCommand, _>(debug_colliders);
        app.add_system(render_debug_colliders);
    }
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Component)]
pub struct ColliderId {
    pub entity: SceneEntityId,
    pub name: Option<String>,
    pub index: u32,
}

impl ColliderId {
    pub fn new(entity: SceneEntityId, name: Option<String>, index: u32) -> Self {
        Self {
            entity,
            name,
            index,
        }
    }
}

#[derive(Debug)]
pub struct RaycastResult {
    pub id: ColliderId,
    pub toi: f32,
    pub normal: Vec3,
}

struct ColliderState {
    base_collider: Collider,
    scale: Vec3,
}

#[derive(Component, Default)]
pub struct SceneColliderData {
    collider_set: ColliderSet,
    scaled_collider: bimap::BiMap<ColliderId, ColliderHandle>,
    collider_state: HashMap<ColliderId, ColliderState>,
    query_state_valid_at: Option<f32>,
    query_state: Option<rapier3d::pipeline::QueryPipeline>,
    dummy_rapier_structs: (IslandManager, RigidBodySet),
}

const SCALE_EPSILON: f32 = 0.001;

impl SceneColliderData {
    pub fn set_collider(&mut self, id: &ColliderId, new_collider: Collider) {
        self.remove_collider(id);

        self.collider_state.insert(
            id.to_owned(),
            ColliderState {
                base_collider: new_collider.clone(),
                scale: Vec3::ONE,
            },
        );
        let handle = self.collider_set.insert(new_collider);
        self.scaled_collider.insert(id.to_owned(), handle);
        self.query_state_valid_at = None;
        debug!("set {id:?} collider");
    }

    pub fn update_collider_transform(&mut self, id: &ColliderId, transform: &GlobalTransform) {
        if let Some(handle) = self.get_collider(id) {
            if let Some(collider) = self.collider_set.get_mut(handle) {
                let (req_scale, rotation, translation) = transform.to_scale_rotation_translation();
                let ColliderState {
                    base_collider,
                    scale,
                } = self.collider_state.get(id).unwrap();

                fn scale_shape(s: &dyn Shape, req_scale: Vec3) -> SharedShape {
                    match s.as_typed_shape() {
                        TypedShape::Ball(b) => match b.scaled(&req_scale.into(), 5).unwrap() {
                            itertools::Either::Left(ball) => SharedShape::new(ball),
                            itertools::Either::Right(convex) => SharedShape::new(convex),
                        },
                        TypedShape::Cuboid(c) => SharedShape::new(c.scaled(&req_scale.into())),
                        TypedShape::ConvexPolyhedron(p) => {
                            SharedShape::new(p.clone().scaled(&req_scale.into()).unwrap())
                        }
                        TypedShape::Compound(c) => {
                            let scaled_items = c
                                .shapes()
                                .iter()
                                .map(|(iso, shape)| {
                                    let mut vector = iso.translation.vector;
                                    // TODO gotta be a clean way to do this
                                    vector[0] *= req_scale.x;
                                    vector[1] *= req_scale.y;
                                    vector[2] *= req_scale.z;
                                    (
                                        Isometry {
                                            rotation: iso.rotation,
                                            translation: Translation { vector },
                                        },
                                        scale_shape(shape.0.as_ref(), req_scale),
                                    )
                                })
                                .collect();
                            SharedShape::compound(scaled_items)
                        }
                        _ => panic!(),
                    }
                }

                // colliders don't have a scale, we have to modify the shape directly when scale changes (significantly)
                if (req_scale - *scale).length_squared() > SCALE_EPSILON {
                    collider.set_shape(scale_shape(base_collider.shape(), req_scale));
                }
                self.collider_state.get_mut(id).unwrap().scale = req_scale;

                collider.set_position(Isometry::from_parts(translation.into(), rotation.into()));
            }
        }
    }

    fn update_pipeline(&mut self, scene_time: f32) {
        if self.query_state_valid_at != Some(scene_time) {
            if self.query_state.is_none() {
                self.query_state = Some(Default::default());
            }
            self.query_state
                .as_mut()
                .unwrap()
                .update(&self.dummy_rapier_structs.1, &self.collider_set);
            self.query_state_valid_at = Some(scene_time);
        }
    }

    pub fn cast_ray_nearest(
        &mut self,
        scene_time: f32,
        origin: Vec3,
        direction: Vec3,
        distance: f32,
        collision_mask: u32,
    ) -> Option<RaycastResult> {
        let ray = rapier3d::prelude::Ray {
            origin: origin.into(),
            dir: direction.into(),
        };
        self.update_pipeline(scene_time);

        self.query_state
            .as_ref()
            .unwrap()
            .cast_ray_and_get_normal(
                &self.dummy_rapier_structs.1,
                &self.collider_set,
                &ray,
                distance,
                true,
                QueryFilter::default().groups(InteractionGroups::new(
                    Group::from_bits_truncate(collision_mask),
                    Group::all(),
                )),
            )
            .map(|(handle, intersection)| RaycastResult {
                id: self.get_id(handle).unwrap().clone(),
                toi: intersection.toi,
                normal: Vec3::from(intersection.normal),
            })
    }

    pub fn cast_ray_all(
        &mut self,
        scene_time: f32,
        origin: Vec3,
        direction: Vec3,
        distance: f32,
        collision_mask: u32,
    ) -> Vec<RaycastResult> {
        let ray = rapier3d::prelude::Ray {
            origin: origin.into(),
            dir: direction.into(),
        };
        let mut results = Vec::default();
        self.update_pipeline(scene_time);

        self.query_state.as_ref().unwrap().intersections_with_ray(
            &self.dummy_rapier_structs.1,
            &self.collider_set,
            &ray,
            distance,
            true,
            QueryFilter::default().groups(InteractionGroups::new(
                Group::from_bits_truncate(collision_mask),
                Group::all(),
            )),
            |handle, intersection| {
                results.push(RaycastResult {
                    id: self.get_id(handle).unwrap().clone(),
                    toi: intersection.toi,
                    normal: Vec3::from(intersection.normal),
                });
                true
            },
        );

        results
    }

    pub fn remove_collider(&mut self, id: &ColliderId) {
        if let Some(handle) = self.scaled_collider.get_by_left(id) {
            self.collider_set.remove(
                *handle,
                &mut self.dummy_rapier_structs.0,
                &mut self.dummy_rapier_structs.1,
                false,
            );
        }

        self.scaled_collider.remove_by_left(id);
        self.collider_state.remove(id);
    }

    // TODO use map of maps to make this faster?
    pub fn remove_colliders(&mut self, id: SceneEntityId) {
        let remove_keys = self
            .collider_state
            .keys()
            .filter(|k| k.entity == id)
            .cloned()
            .collect::<Vec<_>>();

        for key in remove_keys {
            self.remove_collider(&key);
        }
    }

    pub fn get_collider(&self, id: &ColliderId) -> Option<ColliderHandle> {
        self.scaled_collider.get_by_left(id).copied()
    }

    pub fn get_id(&self, handle: ColliderHandle) -> Option<&ColliderId> {
        self.scaled_collider.get_by_right(&handle)
    }
}

fn update_scene_collider_data(
    mut commands: Commands,
    scenes: Query<Entity, (With<RendererSceneContext>, Without<SceneColliderData>)>,
) {
    for scene_ent in scenes.iter() {
        commands
            .entity(scene_ent)
            .insert(SceneColliderData::default());
    }
}

// collider state component
#[derive(Component)]
pub struct HasCollider(ColliderId);

#[allow(clippy::type_complexity)]
fn update_colliders(
    mut commands: Commands,
    // add colliders
    // any entity with a mesh collider that we're not already using, or where the mesh collider has changed
    new_colliders: Query<
        (Entity, &MeshCollider, &ContainerEntity),
        Or<(Changed<MeshCollider>, Without<HasCollider>)>,
    >,
    // remove colliders
    // any entities with a live collider handle that don't have a mesh collider
    colliders_without_source: Query<
        (Entity, &ContainerEntity, &HasCollider),
        Without<MeshCollider>,
    >,
    // remove colliders for deleted entities
    mut scene_data: Query<(&mut SceneColliderData, Option<&DeletedSceneEntities>)>,
) {
    // add colliders
    // any entity with a mesh collider that we're not using, or where the mesh collider has changed
    for (ent, collider_def, container) in new_colliders.iter() {
        let collider = match &collider_def.shape {
            MeshColliderShape::Box => ColliderBuilder::cuboid(0.5, 0.5, 0.5),
            MeshColliderShape::Cylinder { radius_top, radius_bottom } => {
                // TODO we could use explicit support points to make queries faster
                let mesh: Mesh = TruncatedCone{ base_radius: *radius_top, tip_radius: *radius_bottom, ..Default::default() }.into();
                let VertexAttributeValues::Float32x3(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() else { panic!() };
                ColliderBuilder::convex_hull(&positions.iter().map(|p| Point::from(*p)).collect::<Vec<_>>()).unwrap()
            }
            MeshColliderShape::Plane => ColliderBuilder::cuboid(0.5, 0.05, 0.5),
            MeshColliderShape::Sphere => ColliderBuilder::ball(0.5),
            MeshColliderShape::Shape(shape, _) => {
                ColliderBuilder::new(shape.clone())
            },
        }
        .collision_groups(InteractionGroups {
            memberships: Group::from_bits_truncate(collider_def.collision_mask),
            filter: Group::all(),
        })
        .build();

        let collider_id = ColliderId::new(
            container.container_id,
            collider_def.mesh_name.to_owned(),
            collider_def.index,
        );
        debug!("{:?} adding collider", collider_id);
        let Ok((mut scene_data, _)) = scene_data.get_mut(container.root) else {
            warn!("missing scene root for {collider_id:?}");
            continue;
        };

        scene_data.set_collider(collider_id.borrow(), collider);
        commands.entity(ent).insert(HasCollider(collider_id));
    }

    // remove colliders
    // any entities with a live collider handle that don't have a mesh collider or a mesh definition
    for (ent, container, collider) in colliders_without_source.iter() {
        let Ok((mut scene_data, _)) = scene_data.get_mut(container.root) else {
            warn!("missing scene root for {container:?}");
            continue;
        };

        scene_data.remove_collider(collider.0.borrow());
        commands.entity(ent).remove::<HasCollider>();
    }

    // remove colliders for deleted entities
    for (mut scene_data, maybe_deleted_entities) in &mut scene_data {
        if let Some(deleted_entities) = maybe_deleted_entities {
            for deleted_entity in &deleted_entities.0 {
                scene_data.remove_colliders(*deleted_entity);
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_collider_transforms(
    changed_colliders: Query<
        (&ContainerEntity, &HasCollider, &GlobalTransform),
        (
            Or<(Changed<GlobalTransform>, Added<HasCollider>)>, // needs updating
        ),
    >,
    mut scene_data: Query<&mut SceneColliderData>,
) {
    for (container, collider, global_transform) in changed_colliders.iter() {
        let Ok(mut scene_data) = scene_data.get_mut(container.root) else {
            warn!("missing scene root for {container:?}");
            continue;
        };

        scene_data.update_collider_transform(&collider.0, global_transform);
    }
}

#[derive(Resource, Default)]
struct DebugColliders(u32);

/// Display debug colliders
/// show: u32 to specify collider masks to show. if omitted, toggles between `all` and `none`
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/debug_colliders")]
struct DebugColliderCommand {
    show: Option<u32>,
}

fn debug_colliders(
    mut input: ConsoleCommand<DebugColliderCommand>,
    mut debug: ResMut<DebugColliders>,
) {
    if let Some(Ok(command)) = input.take() {
        let new_state = command
            .show
            .unwrap_or(if debug.0 == 0 { u32::MAX } else { 0 });
        debug.0 = new_state;
        input.reply_ok(format!(
            "showing debug colliders with mask matching: {new_state:#x}"
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn render_debug_colliders(
    mut commands: Commands,
    debug: Res<DebugColliders>,
    mut debug_entities: Local<HashMap<Entity, Entity>>,
    with_collider: Query<(Entity, &MeshCollider), With<HasCollider>>,
    changed_collider: Query<Entity, Changed<MeshCollider>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut debug_material: Local<Option<Handle<StandardMaterial>>>,
) {
    if debug.0 == 0 || debug.is_changed() {
        for (_, debug_ent) in debug_entities.drain() {
            if let Some(mut commands) = commands.get_entity(debug_ent) {
                commands.despawn();
            };
        }
        return;
    }

    if debug_material.is_none() {
        *debug_material = Some(materials.add(StandardMaterial {
            base_color: Color::rgba(1.0, 0.0, 0.0, 0.1),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            depth_bias: 1000.0,
            ..Default::default()
        }));
    }

    for collider in changed_collider.iter() {
        if let Some(debug_ent) = debug_entities.remove(&collider) {
            if let Some(mut commands) = commands.get_entity(debug_ent) {
                commands.despawn();
            }
        }
    }

    for (collider_ent, collider) in with_collider.iter() {
        if !debug_entities.contains_key(&collider_ent) && collider.collision_mask & debug.0 != 0 {
            let h_mesh = match &collider.shape {
                MeshColliderShape::Box => meshes.add(bevy::prelude::shape::Cube::default().into()),
                MeshColliderShape::Cylinder {
                    radius_top,
                    radius_bottom,
                } => meshes.add(
                    TruncatedCone {
                        base_radius: *radius_bottom,
                        tip_radius: *radius_top,
                        ..Default::default()
                    }
                    .into(),
                ),
                MeshColliderShape::Plane => {
                    meshes.add(bevy::prelude::shape::Quad::default().into())
                }
                MeshColliderShape::Sphere => {
                    meshes.add(bevy::prelude::shape::UVSphere::default().into())
                }
                MeshColliderShape::Shape(_, h_mesh) => {
                    let mut h_mesh = h_mesh.clone();
                    h_mesh.make_strong(&meshes);
                    h_mesh
                }
            };

            let debug_ent = commands
                .spawn((
                    PbrBundle {
                        mesh: h_mesh,
                        material: debug_material.as_ref().unwrap().clone(),
                        ..Default::default()
                    },
                    Wireframe,
                    NotShadowCaster,
                    NotShadowReceiver,
                ))
                .id();

            commands.entity(collider_ent).add_child(debug_ent);

            debug_entities.insert(collider_ent, debug_ent);
        }
    }
}
