use bevy::{
    math::Vec3,
    pbr::{wireframe::Wireframe, NotShadowCaster, NotShadowReceiver},
    platform::collections::{HashMap, HashSet},
    prelude::*,
    render::mesh::VertexAttributeValues,
};
use bevy_console::ConsoleCommand;
use rapier3d::{
    control::KinematicCharacterController,
    parry::{
        query::{NonlinearRigidMotion, ShapeCastHit, ShapeCastOptions, ShapeCastStatus},
        shape::{Ball, Capsule},
    },
    prelude::*,
};

use crate::{
    gltf_resolver::GltfMeshResolver,
    update_world::{
        gltf_container::mesh_to_parry_shape, mesh_renderer::truncated_cone::TruncatedCone,
    },
    ContainerEntity, ContainingScene, DeletedSceneEntities, PrimaryUser, RendererSceneContext,
    SceneLoopSchedule, SceneSets,
};
use common::{
    dynamics::{PLAYER_COLLIDER_HEIGHT, PLAYER_COLLIDER_OVERLAP, PLAYER_COLLIDER_RADIUS},
    sets::SceneLoopSets,
};
use console::DoAddConsoleCommand;
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{pb_mesh_collider, ColliderLayer, PbMeshCollider},
    SceneComponentId, SceneEntityId,
};

use super::AddCrdtInterfaceExt;

pub struct MeshColliderPlugin;

#[derive(Component, Clone, Debug)]
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

#[derive(Component)]
pub struct DisableCollisions;

// #[derive(Debug)]
#[derive(Clone, Debug)]
pub enum MeshColliderShape {
    Box,
    Cylinder { radius_top: f32, radius_bottom: f32 },
    Plane,
    Sphere,
    Shape(SharedShape, Handle<Mesh>),
    GltfShape { gltf_src: String, name: String },
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
                radius_top: radius_top.unwrap_or(0.5),
                radius_bottom: radius_bottom.unwrap_or(0.5),
            },
            Some(pb_mesh_collider::Mesh::Gltf(pb_mesh_collider::GltfMesh { gltf_src, name })) => {
                MeshColliderShape::GltfShape { gltf_src, name }
            }
            _ => MeshColliderShape::Box,
        };

        Self {
            shape,
            // TODO update to u32
            collision_mask: value
                .collision_mask
                .unwrap_or(ColliderLayer::ClPointer as u32 | ColliderLayer::ClPhysics as u32),
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

        // collider components are created in SceneSets::Loop (by PbMeshCollider messages) and
        // in SceneSets::PostLoop (by gltf processing).
        // they are positioned in SceneSets::PostInit and
        // they are used in SceneSets::Input (for raycasts).
        // we want to avoid using CoreSet::PostUpdate as that's where we create/destroy scenes,
        // so we use SceneSets::Init for adding colliders to the scene collider data (qbvh).
        app.add_systems(
            Update,
            (update_colliders, propagate_disabled)
                .chain()
                .in_set(SceneSets::Init),
        );
        app.add_systems(
            Update,
            update_scene_collider_data.in_set(SceneSets::PostInit),
        );

        // collider deletion has to occur within the scene loop, as the DeletedSceneEntities resource is only
        // valid within the loop
        app.world_mut()
            .resource_mut::<SceneLoopSchedule>()
            .schedule
            .add_systems(remove_deleted_colliders.in_set(SceneLoopSets::UpdateWorld));

        // update collider transforms before queries and scenes are run, but after global transforms are updated (at end of prior frame)
        app.add_systems(
            Update,
            update_collider_transforms.in_set(SceneSets::PostInit),
        );

        app.init_resource::<DebugColliders>();
        app.add_console_command::<DebugColliderCommand, _>(debug_colliders);
        app.add_systems(Update, render_debug_colliders);
    }
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
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
    pub face: Option<usize>,
}

struct ColliderState {
    entity: Option<Entity>,
    base_collider: Collider,
    translation: Vec3,
    rotation: Quat,
    scale: Vec3,
}

#[derive(Component, Default)]
pub struct SceneColliderData {
    collider_set: ColliderSet,
    scaled_collider: bimap::BiMap<ColliderId, ColliderHandle>,
    collider_state: HashMap<ColliderId, ColliderState>,
    query_state_valid_at: Option<u32>,
    query_state: Option<rapier3d::pipeline::QueryPipeline>,
    dummy_rapier_structs: (IslandManager, RigidBodySet),
    disabled: HashSet<ColliderHandle>,
}

const SCALE_EPSILON: f32 = 0.001;

pub trait ScaleShapeExt {
    fn scale_ext(&self, req_scale: Vec3) -> SharedShape;
}

impl ScaleShapeExt for dyn Shape {
    fn scale_ext(&self, req_scale: Vec3) -> SharedShape {
        match self.as_typed_shape() {
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
                            shape.0.scale_ext(req_scale),
                        )
                    })
                    .collect();
                SharedShape::compound(scaled_items)
            }
            TypedShape::TriMesh(trimesh) => {
                SharedShape::new(trimesh.clone().scaled(&req_scale.into()))
            }
            _ => panic!(),
        }
    }
}

impl SceneColliderData {
    pub fn set_collider(
        &mut self,
        id: &ColliderId,
        new_collider: Collider,
        entity: Option<Entity>,
    ) {
        self.remove_collider(id);

        self.collider_state.insert(
            id.to_owned(),
            ColliderState {
                entity,
                base_collider: new_collider.clone(),
                translation: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            },
        );

        let handle = self.collider_set.insert(new_collider);
        self.scaled_collider.insert(id.to_owned(), handle);
        self.query_state_valid_at = None;
        debug!("set {id:?} collider");
    }

    pub fn update_collider_transform(
        &mut self,
        id: &ColliderId,
        transform: &GlobalTransform,
        cast_with: Option<&ColliderSet>,
    ) -> (Option<Transform>, Option<ShapeCastHit>) {
        if let Some(handle) = self.get_collider_handle(id) {
            if let Some(collider) = self.collider_set.get_mut(handle) {
                self.query_state_valid_at = None;
                let (req_scale, req_rotation, req_translation) =
                    transform.to_scale_rotation_translation();
                let ColliderState {
                    base_collider,
                    translation: init_translation,
                    scale: init_scale,
                    rotation: init_rotation,
                    ..
                } = self.collider_state.get(id).unwrap();

                let mut cast_result = None;
                let mut new_scale = *init_scale;
                if (req_scale - *init_scale).length_squared() > SCALE_EPSILON {
                    new_scale = req_scale;
                    // colliders don't have a scale, we have to modify the shape directly when scale changes (significantly)
                    collider.set_shape(base_collider.shape().scale_ext(req_scale));
                } else if self.disabled.contains(&handle) {
                    // don't shapecast
                } else if let Some(colliders) = cast_with {
                    // if scale doesn't change then just shapecast to hit colliders
                    let mut pipeline = QueryPipeline::new();
                    pipeline.update(colliders);
                    let euler_axes =
                        (req_rotation * init_rotation.inverse()).to_euler(EulerRot::XYZ);
                    cast_result = pipeline
                        .nonlinear_cast_shape(
                            &self.dummy_rapier_structs.1,
                            colliders,
                            &NonlinearRigidMotion {
                                start: Isometry::from_parts(
                                    (*init_translation).into(),
                                    (*init_rotation).into(),
                                ),
                                local_center: Default::default(),
                                linvel: (req_translation - *init_translation).into(),
                                angvel: [euler_axes.0, euler_axes.1, euler_axes.2].into(),
                            },
                            collider.shape(),
                            0.0,
                            1.0,
                            true,
                            QueryFilter::default(),
                        )
                        .map(|(_, toi)| toi);
                }
                let initial_transform = Transform {
                    translation: *init_translation,
                    rotation: *init_rotation,
                    scale: *init_scale,
                };

                let state_mut = self.collider_state.get_mut(id).unwrap();
                state_mut.translation = req_translation;
                state_mut.rotation = req_rotation;
                state_mut.scale = new_scale;

                collider.set_position(Isometry::from_parts(
                    req_translation.into(),
                    req_rotation.into(),
                ));
                return (Some(initial_transform), cast_result);
            }
        }

        (None, None)
    }

    fn update_pipeline(&mut self, scene_frame: u32) {
        if self.query_state_valid_at != Some(scene_frame) {
            if self.query_state.is_none() {
                self.query_state = Some(Default::default());
            }
            self.query_state
                .as_mut()
                .unwrap()
                .update(&self.collider_set);
            self.query_state_valid_at = Some(scene_frame);
        }
    }

    pub fn force_update(&mut self) {
        if self.query_state.is_none() {
            self.query_state = Some(Default::default());
        }

        self.query_state
            .as_mut()
            .unwrap()
            .update(&self.collider_set);
    }

    pub fn cast_ray_nearest(
        &mut self,
        scene_time: u32,
        origin: Vec3,
        direction: Vec3,
        distance: f32,
        collision_mask: u32,
        skip_inside: bool,
    ) -> Option<RaycastResult> {
        let ray = rapier3d::prelude::Ray {
            origin: origin.into(),
            dir: direction.into(),
        };
        self.update_pipeline(scene_time);

        // collect colliders we started inside of, we must omit these from the query
        let mut inside = HashSet::new();
        if skip_inside {
            self.query_state.as_ref().unwrap().intersections_with_shape(
                &self.dummy_rapier_structs.1,
                &self.collider_set,
                &origin.into(),
                &Ball::new(0.001),
                QueryFilter::default().groups(InteractionGroups::new(
                    Group::from_bits_truncate(collision_mask),
                    Group::from_bits_truncate(collision_mask),
                )),
                |h| {
                    inside.insert(h);
                    true
                },
            );
        }

        self.query_state
            .as_ref()
            .unwrap()
            .cast_ray_and_get_normal(
                &self.dummy_rapier_structs.1,
                &self.collider_set,
                &ray,
                distance,
                true,
                QueryFilter::default()
                    .groups(InteractionGroups::new(
                        Group::from_bits_truncate(collision_mask),
                        Group::from_bits_truncate(collision_mask),
                    ))
                    .predicate(&|h, _| !inside.contains(&h)),
            )
            .map(|(handle, intersection)| RaycastResult {
                id: self.get_id(handle).unwrap().clone(),
                toi: intersection.time_of_impact,
                normal: Vec3::from(intersection.normal),
                face: if let FeatureId::Face(fix) = intersection.feature {
                    Some(fix as usize)
                } else {
                    None
                },
            })
    }

    pub fn get_groundheight(&mut self, scene_time: u32, origin: Vec3) -> Option<(f32, ColliderId)> {
        self.update_pipeline(scene_time);
        let contact = self.query_state.as_ref().unwrap().cast_shape(
            &self.dummy_rapier_structs.1,
            &self.collider_set,
            &(origin + Vec3::Y * (PLAYER_COLLIDER_RADIUS - PLAYER_COLLIDER_OVERLAP)).into(),
            &(-Vec3::Y).into(),
            &Ball::new(PLAYER_COLLIDER_RADIUS - PLAYER_COLLIDER_OVERLAP),
            ShapeCastOptions {
                max_time_of_impact: 10.0,
                target_distance: 0.0,
                stop_at_penetration: true,
                compute_impact_geometry_on_penetration: true,
            },
            QueryFilter::default().predicate(&|h, _| self.collider_enabled(h)),
        );

        contact.map(|(handle, toi)| (toi.time_of_impact, self.get_id(handle).unwrap().clone()))
    }

    pub fn get_collider_entity(&self, id: &ColliderId) -> Option<Entity> {
        self.collider_state.get(id).and_then(|state| state.entity)
    }

    pub fn collider_enabled(&self, handle: ColliderHandle) -> bool {
        !self.disabled.contains(&handle)
    }

    pub fn depentrate_character(&mut self, scene_time: u32, origin: Vec3) -> Option<Vec3> {
        self.update_pipeline(scene_time);

        // check for initial penetration
        let pipeline = self.query_state.as_ref().unwrap();
        let initial_intersection = pipeline.intersection_with_shape(
            &self.dummy_rapier_structs.1,
            &self.collider_set,
            &Isometry {
                rotation: Default::default(),
                translation: (origin + Vec3::Y * 1.0).into(),
            },
            &Capsule::new_y(
                (PLAYER_COLLIDER_HEIGHT * 0.5 - PLAYER_COLLIDER_RADIUS) * 0.85,
                PLAYER_COLLIDER_RADIUS * 0.85,
            ),
            QueryFilter::default().groups(InteractionGroups::new(
                Group::from_bits_truncate(ColliderLayer::ClPhysics as u32),
                Group::from_bits_truncate(ColliderLayer::ClPhysics as u32),
            )),
        );
        initial_intersection.map(|_| {
            // check nearby points and eject
            fn fibonacci_sphere_points(samples: usize) -> Vec<Vec3> {
                let mut points = Vec::with_capacity(samples);
                let phi = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());

                for i in 0..samples {
                    let i_f = i as f32;
                    let n_f = samples as f32;

                    // y goes from 1 to -1
                    let y = 1.0 - (i_f / (n_f - 1.0)) * 2.0;

                    // radius at this height
                    let r_at_height = (1.0 - y * y).sqrt();

                    let theta = phi * i_f;

                    let x = theta.cos() * r_at_height;
                    let z = theta.sin() * r_at_height;

                    // scale and shift to world position
                    points.push(Vec3::new(x, y, z));
                }
                points
            }

            let mut distance = 0.5;
            let mut num_points = 15;
            loop {
                let sphere_offsets = fibonacci_sphere_points(num_points);
                for offset in sphere_offsets {
                    if (origin + offset).y < 0.0 {
                        continue;
                    }
                    if pipeline
                        .intersection_with_shape(
                            &self.dummy_rapier_structs.1,
                            &self.collider_set,
                            &Isometry {
                                rotation: Default::default(),
                                translation: (origin + Vec3::Y * 1.0 + offset * distance).into(),
                            },
                            &Capsule::new_y(
                                PLAYER_COLLIDER_HEIGHT * 0.5 - PLAYER_COLLIDER_RADIUS,
                                PLAYER_COLLIDER_RADIUS,
                            ),
                            QueryFilter::default().groups(InteractionGroups::new(
                                Group::from_bits_truncate(ColliderLayer::ClPhysics as u32),
                                Group::from_bits_truncate(ColliderLayer::ClPhysics as u32),
                            )),
                        )
                        .is_none()
                    {
                        return offset;
                    }
                }

                distance *= 2.0;
                num_points *= 2;
            }
        })
    }

    pub fn move_character(
        &mut self,
        dt: f32,
        scene_time: u32,
        origin: Vec3,
        direction: Vec3,
        character: &KinematicCharacterController,
        specific_collider: Option<&ColliderId>,
        include_specific_collider: bool,
    ) -> Vec3 {
        self.update_pipeline(scene_time);
        let specific_collider = specific_collider.map(|id| self.get_collider_handle(id).unwrap());
        if include_specific_collider && specific_collider.is_none() {
            return direction;
        }

        Vec3::from(
            character
                .move_shape(
                    dt,
                    &self.dummy_rapier_structs.1,
                    &self.collider_set,
                    self.query_state.as_ref().unwrap(),
                    &Capsule::new_y(
                        PLAYER_COLLIDER_HEIGHT * 0.5 - PLAYER_COLLIDER_RADIUS,
                        PLAYER_COLLIDER_RADIUS,
                    ),
                    &Isometry {
                        rotation: Default::default(),
                        translation: (origin + Vec3::Y * 1.0).into(),
                    },
                    direction.into(),
                    QueryFilter::default()
                        .groups(InteractionGroups::new(
                            Group::from_bits_truncate(ColliderLayer::ClPhysics as u32),
                            Group::from_bits_truncate(ColliderLayer::ClPhysics as u32),
                        ))
                        .predicate(&|h, _| {
                            ((specific_collider == Some(h)) == include_specific_collider)
                                && !self.disabled.contains(&h)
                        }),
                    |_| {},
                )
                .translation,
        )
    }

    pub fn cast_ray_all(
        &mut self,
        scene_time: u32,
        origin: Vec3,
        direction: Vec3,
        distance: f32,
        collision_mask: u32,
        skip_inside: bool,
    ) -> Vec<RaycastResult> {
        let ray = rapier3d::prelude::Ray {
            origin: origin.into(),
            dir: direction.into(),
        };
        let mut results = Vec::default();
        self.update_pipeline(scene_time);

        let mut inside = HashSet::new();
        if skip_inside {
            // collect colliders we started (nearly) inside of, we must omit these from the query
            self.query_state.as_ref().unwrap().intersections_with_shape(
                &self.dummy_rapier_structs.1,
                &self.collider_set,
                &origin.into(),
                &Ball::new(0.001),
                QueryFilter::default().groups(InteractionGroups::new(
                    Group::from_bits_truncate(collision_mask),
                    Group::from_bits_truncate(collision_mask),
                )),
                |h| {
                    inside.insert(h);
                    true
                },
            );
        }

        self.query_state.as_ref().unwrap().intersections_with_ray(
            &self.dummy_rapier_structs.1,
            &self.collider_set,
            &ray,
            distance,
            true,
            QueryFilter::default()
                .groups(InteractionGroups::new(
                    Group::from_bits_truncate(collision_mask),
                    Group::from_bits_truncate(collision_mask),
                ))
                .predicate(&|h, _| !inside.contains(&h)),
            |handle, intersection| {
                results.push(RaycastResult {
                    id: self.get_id(handle).unwrap().clone(),
                    toi: intersection.time_of_impact,
                    normal: Vec3::from(intersection.normal),
                    face: if let FeatureId::Face(fix) = intersection.feature {
                        Some(fix as usize)
                    } else {
                        None
                    },
                });
                true
            },
        );

        results
    }

    pub fn closest_point<F: Fn(&ColliderId) -> bool>(
        &mut self,
        scene_time: u32,
        origin: Vec3,
        filter: F,
    ) -> Option<Vec3> {
        self.update_pipeline(scene_time);

        // rapier's api demands a fat pointer for whatever reason
        let predicate: &dyn Fn(ColliderHandle, &Collider) -> bool =
            &|h: ColliderHandle, _: &Collider| self.get_id(h).is_some_and(&filter);
        let q = QueryFilter::new().predicate(&predicate);

        self.query_state
            .as_ref()
            .unwrap()
            .project_point(
                &self.dummy_rapier_structs.1,
                &self.collider_set,
                &Point::from(origin),
                true,
                q,
            )
            .map(|(_, point)| Vec3::from(point.point))
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
        self.query_state_valid_at = None;
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

    pub fn get_collider_handle(&self, id: &ColliderId) -> Option<ColliderHandle> {
        self.scaled_collider.get_by_left(id).copied()
    }

    pub fn get_collider(&self, id: &ColliderId) -> Option<&Collider> {
        self.get_collider_handle(id)
            .and_then(|h| self.collider_set.get(h))
    }

    pub fn get_id(&self, handle: ColliderHandle) -> Option<&ColliderId> {
        self.scaled_collider.get_by_right(&handle)
    }

    pub fn disable_player_collisions(&mut self, id: &ColliderId) {
        if let Some(h) = self.get_collider_handle(id) {
            self.disabled.insert(h);
        }
    }

    pub fn clear_disabled(&mut self) {
        self.disabled.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = &ColliderId> {
        self.scaled_collider.left_values()
    }
}

fn update_scene_collider_data(
    mut commands: Commands,
    scenes: Query<(Entity, &RendererSceneContext), Without<SceneColliderData>>,
) {
    for (scene_ent, ctx) in scenes.iter() {
        let mut scene_data = SceneColliderData::default();
        for (index, parcel) in ctx.parcels.iter().enumerate() {
            let floor_panel = ColliderBuilder::cuboid(8.0, 8.0, 8.0)
                .translation(
                    ((parcel.as_vec2() + Vec2::splat(0.5)) * Vec2::new(16.0, -16.0))
                        .extend(-8.0 + PLAYER_COLLIDER_OVERLAP)
                        .xzy()
                        .into(),
                )
                .build();
            scene_data.set_collider(
                &ColliderId {
                    entity: SceneEntityId::ROOT,
                    name: Some(format!("{parcel}")),
                    index: index as u32,
                },
                floor_panel,
                None,
            );
        }
        commands.entity(scene_ent).try_insert(scene_data);
    }
}

// collider state component
#[derive(Component)]
pub struct HasCollider(pub ColliderId);

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
    mut scene_data: Query<(&RendererSceneContext, &mut SceneColliderData)>,
    mut gltf_mesh_resolver: GltfMeshResolver,
    meshes: Res<Assets<Mesh>>,
) {
    gltf_mesh_resolver.begin_frame();

    // add colliders
    // any entity with a mesh collider that we're not using, or where the mesh collider has changed
    for (ent, collider_def, container) in new_colliders.iter() {
        let collider = match &collider_def.shape {
            MeshColliderShape::Box => ColliderBuilder::cuboid(0.5, 0.5, 0.5),
            MeshColliderShape::Cylinder {
                radius_top,
                radius_bottom,
            } => {
                // TODO we could use explicit support points to make queries faster
                let mesh: Mesh = TruncatedCone {
                    base_radius: *radius_bottom,
                    tip_radius: *radius_top,
                    ..Default::default()
                }
                .into();
                let VertexAttributeValues::Float32x3(positions) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
                else {
                    panic!()
                };
                ColliderBuilder::convex_hull(
                    &positions
                        .iter()
                        .map(|p| Point::from([p[0], p[1], p[2]]))
                        .collect::<Vec<_>>(),
                )
                .unwrap()
            }
            MeshColliderShape::Plane => ColliderBuilder::cuboid(0.5, 0.5, 0.005),
            MeshColliderShape::Sphere => ColliderBuilder::ball(0.5),
            MeshColliderShape::Shape(shape, _) => ColliderBuilder::new(shape.clone()),
            MeshColliderShape::GltfShape { gltf_src, name } => {
                let Ok((scene, _)) = scene_data.get(container.root) else {
                    continue;
                };
                let Ok(Some(h_mesh)) = gltf_mesh_resolver.resolve_mesh(gltf_src, &scene.hash, name)
                else {
                    continue;
                };
                let mesh = meshes.get(&h_mesh).unwrap();
                let shape = mesh_to_parry_shape(mesh);
                ColliderBuilder::new(shape)
            }
        }
        .collision_groups(InteractionGroups {
            memberships: Group::from_bits_truncate(collider_def.collision_mask),
            filter: Group::from_bits_truncate(collider_def.collision_mask),
        })
        .build();

        let collider_id = ColliderId::new(
            container.container_id,
            collider_def.mesh_name.to_owned(),
            collider_def.index,
        );
        debug!("{:?} adding collider", collider_id);
        let Ok((_, mut scene_data)) = scene_data.get_mut(container.root) else {
            warn!("missing scene root for {collider_id:?}");
            continue;
        };

        scene_data.set_collider(&collider_id, collider, Some(ent));
        commands.entity(ent).try_insert(HasCollider(collider_id));
    }

    // remove colliders
    // any entities with a live collider handle that don't have a mesh collider or a mesh definition
    for (ent, container, collider) in colliders_without_source.iter() {
        let Ok((_, mut scene_data)) = scene_data.get_mut(container.root) else {
            warn!("missing scene root for {container:?}");
            continue;
        };

        scene_data.remove_collider(&collider.0);
        commands.entity(ent).remove::<HasCollider>();
    }
}

fn remove_deleted_colliders(
    mut scene_data: Query<(&mut SceneColliderData, &DeletedSceneEntities)>,
) {
    for (mut scene_data, deleted_entities) in &mut scene_data {
        for deleted_entity in &deleted_entities.0 {
            scene_data.remove_colliders(*deleted_entity);
        }
    }
}

// (scene entity, collider id) of collider player is standing on
#[derive(Component, Default)]
pub struct GroundCollider(pub Option<(Entity, ColliderId, GlobalTransform)>);

#[allow(clippy::type_complexity)]
fn propagate_disabled(
    mut scene_datas: Query<(Entity, &mut SceneColliderData)>,
    q: Query<(&ContainerEntity, Option<&HasCollider>, Option<&Children>), With<DisableCollisions>>,
    r: Query<(Option<&HasCollider>, Option<&Children>), Or<(With<Children>, With<HasCollider>)>>,
) {
    let mut disable: HashMap<Entity, HashSet<&ColliderId>> = HashMap::new();
    for (container, maybe_collider, maybe_children) in q.iter() {
        let set = disable.entry(container.root).or_default();
        if let Some(collider) = maybe_collider {
            set.insert(&collider.0);
        }

        if let Some(children) = maybe_children {
            let mut list = children.iter().collect::<Vec<_>>();

            while let Some(child) = list.pop() {
                let Ok((maybe_id, maybe_children)) = r.get(child) else {
                    continue;
                };

                if let Some(id) = maybe_id {
                    set.insert(&id.0);
                }

                if let Some(children) = maybe_children {
                    list.extend(children.iter());
                }
            }
        }
    }

    for (ent, mut scene_data) in scene_datas.iter_mut() {
        scene_data.clear_disabled();
        if let Some(disabled) = disable.get(&ent) {
            disabled.iter().for_each(|id| {
                scene_data.disable_player_collisions(id);
            });
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_collider_transforms(
    changed_colliders: Query<
        (&ContainerEntity, &HasCollider, &GlobalTransform),
        (
            Or<(Changed<GlobalTransform>, Changed<HasCollider>)>, // needs updating
        ),
    >,
    mut scene_data: Query<&mut SceneColliderData>,
    containing_scene: ContainingScene,
    mut player: Query<(Entity, &mut Transform), With<PrimaryUser>>,
) {
    let mut containing_scenes = HashSet::new();
    let mut player_transform = None;

    if let Ok((player, transform)) = player.single_mut() {
        player_transform = Some(transform);
        containing_scenes.extend(containing_scene.get_area(player, PLAYER_COLLIDER_RADIUS));
    }

    let mut player_collider_set = ColliderSet::default();
    player_collider_set.insert(
        ColliderBuilder::new(SharedShape::capsule_y(
            PLAYER_COLLIDER_HEIGHT * 0.5 - PLAYER_COLLIDER_RADIUS,
            PLAYER_COLLIDER_RADIUS - PLAYER_COLLIDER_OVERLAP,
        ))
        .position(Isometry::from_parts(
            player_transform
                .as_ref()
                .map(|t| t.translation + PLAYER_COLLIDER_HEIGHT * 0.5 * Vec3::Y)
                .unwrap_or_default()
                .into(),
            Default::default(),
        ))
        .build(),
    );

    // closure to generate vector to (attempt to) fix a penetration with a scene collider
    // TODO perhaps store last collider position and move this to player dynamics?
    // not sure ... that would be better for colliders vs other stuff than player
    // but currently it uses pretty intimate knowledge of scene collider data
    let depenetration_vector =
        |scene_data: &mut SceneColliderData, translation: Vec3, toi: &ShapeCastHit| -> Vec3 {
            // just use the bottom sphere of the player collider
            let base_of_sphere = translation + PLAYER_COLLIDER_RADIUS * Vec3::Y;
            let closest_point = match toi.status {
                ShapeCastStatus::OutOfIterations | ShapeCastStatus::Converged => {
                    Vec3::from(toi.witness1)
                }
                ShapeCastStatus::Failed | ShapeCastStatus::PenetratingOrWithinTargetDist => {
                    scene_data.force_update();
                    match scene_data.query_state.as_ref().unwrap().project_point(
                        &scene_data.dummy_rapier_structs.1,
                        &scene_data.collider_set,
                        &base_of_sphere.into(),
                        true,
                        QueryFilter::default().predicate(&|h, _| scene_data.collider_enabled(h)),
                    ) {
                        Some((_, point)) => Vec3::from(point.point),
                        None => translation,
                    }
                }
            };
            let fix_dir = base_of_sphere - closest_point;
            let distance = (PLAYER_COLLIDER_RADIUS - fix_dir.length()).clamp(0.00, 1.0);
            debug!(
                "closest point: {}, dir: {fix_dir}, len: {distance}",
                closest_point
            );
            (fix_dir.normalize_or_zero() * distance)
                // constrain resulting position to above ground
                .max(Vec3::new(
                    f32::NEG_INFINITY,
                    -translation.y,
                    f32::NEG_INFINITY,
                ))
        };

    for (container, collider, global_transform) in changed_colliders.iter() {
        let Ok(mut scene_data) = scene_data.get_mut(container.root) else {
            warn!("missing scene root for {container:?}");
            continue;
        };

        let (maybe_original_transform, maybe_toi) = scene_data.update_collider_transform(
            &collider.0,
            global_transform,
            if containing_scenes.contains(&container.root) {
                Some(&player_collider_set)
            } else {
                None
            },
        );

        if let Some(original_transform) = maybe_original_transform {
            if let Some(toi) = maybe_toi {
                match toi.status {
                    ShapeCastStatus::PenetratingOrWithinTargetDist => {
                        // penetrating collider - use closest point to infer fix/depen direction
                        debug!(
                            "don't skip pen, player: {:?} [moving {:?}]",
                            player_transform.as_ref().unwrap().translation,
                            (&container.root, &collider.0),
                        );
                        let fix_vector = depenetration_vector(
                            &mut scene_data,
                            player_transform.as_ref().unwrap().translation,
                            &toi,
                        );
                        player_transform.as_mut().unwrap().translation += fix_vector;
                    }
                    _ => {
                        // get contact point at toi
                        // use 0.9 cap to avoid clipping
                        let ratio = toi.time_of_impact.min(0.9);
                        let relative_hit_point = Vec3::from(toi.witness2);
                        let (new_scale, new_rotation, new_translation) =
                            global_transform.to_scale_rotation_translation();
                        let transform_at_toi = Transform {
                            translation: original_transform.translation * (1.0 - ratio)
                                + new_translation * ratio,
                            rotation: original_transform.rotation.lerp(new_rotation, ratio),
                            // we use a unit scale because the scale is embedded in the collider mesh/support fn,
                            // so the witness point is actually wrt a unit scale
                            // we know that scale doesn't change because we can't shape-cast a non-constant shape anyway
                            // TODO fix this somehow (stepping in the update? yuck tho)
                            scale: Vec3::ONE,
                        };
                        let contact_at_toi = GlobalTransform::from(transform_at_toi)
                            .transform_point(relative_hit_point);
                        // get contact point at end
                        // unit scale - see above
                        let contact_at_end =
                            global_transform.transform_point(relative_hit_point) / new_scale;

                        // add diff as velocity or as motion?
                        let req_translation = contact_at_end - contact_at_toi;
                        let dot_w_normal1 = req_translation
                            .normalize_or_zero()
                            .dot(Vec3::from(toi.normal1));
                        let dot_w_normal2 = req_translation
                            .normalize_or_zero()
                            .dot(Vec3::from(toi.normal2));
                        if req_translation.length() > 1.0 {
                            // disregard too large deltas as the collider probably just warped
                            // TODO we could check this before updating based on translation?
                            warn!("disregarding push due to large delta: {}, toi: {}, normal dot1: {}, 2: {}", req_translation, toi.time_of_impact, dot_w_normal1, dot_w_normal2);
                            continue;
                        }
                        // add extra 0.01 due to character controller offset / collider size difference
                        debug!(
                            "old player: {:?}",
                            player_transform.as_ref().unwrap().translation
                        );
                        player_transform.as_mut().unwrap().translation += req_translation
                            .normalize_or_zero()
                            * (req_translation.length() + PLAYER_COLLIDER_OVERLAP);
                        debug!(
                            "[{:?} - scale = {}] push {} = {} -> 1 = {}",
                            collider.0, new_scale, ratio, contact_at_toi, contact_at_end
                        );
                        debug!("toi: {toi:?}");
                        debug!(
                            "new player: {:?}",
                            player_transform.as_ref().unwrap().translation
                        );

                        // check for intersection and move out until safe
                        let (_, player_collider) = player_collider_set.iter_mut().next().unwrap();
                        player_collider.set_position(Isometry::from_parts(
                            player_transform
                                .as_ref()
                                .map(|t| t.translation + Vec3::Y)
                                .unwrap_or_default()
                                .into(),
                            Default::default(),
                        ));
                        let new_toi = scene_data
                            .update_collider_transform(
                                &collider.0,
                                global_transform,
                                Some(&player_collider_set),
                            )
                            .1;

                        if let Some(new_toi) = new_toi {
                            debug!(
                                "update toi - can we fix it?: {:?}, player: {}",
                                new_toi,
                                player_transform.as_ref().unwrap().translation
                            );
                            let fix_vector = depenetration_vector(
                                &mut scene_data,
                                player_transform.as_ref().unwrap().translation,
                                &new_toi,
                            );
                            debug!("fix: {fix_vector}");
                            player_transform.as_mut().unwrap().translation += fix_vector;
                        }
                    }
                }
            }
        }
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
    with_collider: Query<(Entity, &MeshCollider, &ContainerEntity), With<HasCollider>>,
    changed_collider: Query<Entity, Changed<MeshCollider>>,
    player: Query<Entity, With<PrimaryUser>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut debug_materials: Local<Option<(Handle<StandardMaterial>, Handle<StandardMaterial>)>>,
    mut gltf_resolver: GltfMeshResolver,
    scenes: Query<&RendererSceneContext>,
) {
    gltf_resolver.begin_frame();

    if debug.0 == 0 || debug.is_changed() {
        for (_, debug_ent) in debug_entities.drain() {
            if let Ok(mut commands) = commands.get_entity(debug_ent) {
                commands.despawn();
            };
        }
        return;
    }

    if debug_materials.is_none() {
        *debug_materials = Some((
            materials.add(StandardMaterial {
                base_color: Color::srgba(1.0, 0.0, 0.0, 0.1),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                depth_bias: 1000.0,
                ..Default::default()
            }),
            materials.add(StandardMaterial {
                base_color: Color::srgba(0.0, 10.0, 0.0, 0.1),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                depth_bias: 1000.0,
                ..Default::default()
            }),
        ));
    }

    for collider in changed_collider.iter() {
        if let Some(debug_ent) = debug_entities.remove(&collider) {
            if let Ok(mut commands) = commands.get_entity(debug_ent) {
                commands.despawn();
            }
        }
    }

    if let Ok(player) = player.single() {
        if !debug_entities.contains_key(&player) {
            let h_mesh = meshes.add(
                Capsule3d::new(
                    PLAYER_COLLIDER_RADIUS,
                    PLAYER_COLLIDER_HEIGHT - PLAYER_COLLIDER_RADIUS * 2.0,
                )
                .mesh(),
            );
            let debug_ent = commands
                .spawn((
                    Mesh3d(h_mesh),
                    MeshMaterial3d(debug_materials.as_ref().unwrap().0.clone()),
                    Transform::from_translation(Vec3::Y),
                    Wireframe,
                    NotShadowCaster,
                    NotShadowReceiver,
                ))
                .id();

            commands.entity(player).add_child(debug_ent);

            debug_entities.insert(player, debug_ent);
        }
    }

    for (collider_ent, collider, container) in with_collider.iter() {
        if !debug_entities.contains_key(&collider_ent) && collider.collision_mask & debug.0 != 0 {
            let h_mesh = match &collider.shape {
                MeshColliderShape::Box => {
                    meshes.add(bevy::math::primitives::Cuboid::default().mesh())
                }
                MeshColliderShape::Cylinder {
                    radius_top,
                    radius_bottom,
                } => meshes.add(Mesh::from(TruncatedCone {
                    base_radius: *radius_bottom,
                    tip_radius: *radius_top,
                    ..Default::default()
                })),
                MeshColliderShape::Plane => meshes.add(Rectangle::default().mesh()),
                MeshColliderShape::Sphere => meshes.add(Sphere::default().mesh().uv(36, 18)),
                MeshColliderShape::Shape(_, h_mesh) => h_mesh.clone(),
                MeshColliderShape::GltfShape { gltf_src, name } => {
                    let Ok(scene) = scenes.get(container.root) else {
                        continue;
                    };
                    let Ok(Some(h_mesh)) = gltf_resolver.resolve_mesh(gltf_src, &scene.hash, name)
                    else {
                        continue;
                    };
                    h_mesh
                }
            };

            let debug_ent = if let MeshColliderShape::GltfShape { .. } = &collider.shape {
                commands
                    .spawn((
                        Mesh3d(h_mesh),
                        MeshMaterial3d(debug_materials.as_ref().unwrap().1.clone()),
                        Wireframe,
                        NotShadowCaster,
                        NotShadowReceiver,
                    ))
                    .id()
            } else {
                commands
                    .spawn((
                        Mesh3d(h_mesh),
                        MeshMaterial3d(debug_materials.as_ref().unwrap().0.clone()),
                        Wireframe,
                        NotShadowCaster,
                        NotShadowReceiver,
                    ))
                    .id()
            };
            commands.entity(collider_ent).add_child(debug_ent);

            debug_entities.insert(collider_ent, debug_ent);
        }
    }
}
