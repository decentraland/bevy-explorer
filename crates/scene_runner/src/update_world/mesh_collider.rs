use std::marker::PhantomData;

use bevy::{
    diagnostic::FrameCount,
    math::{DQuat, DVec3, Vec3},
    pbr::{wireframe::Wireframe, NotShadowCaster, NotShadowReceiver},
    platform::collections::{HashMap, HashSet},
    prelude::*,
    render::mesh::VertexAttributeValues,
};
use bevy_console::ConsoleCommand;
use rapier3d_f64::{
    parry::{
        self,
        query::{Ray, ShapeCastOptions},
        shape::{Ball, Capsule},
    },
    prelude::*,
};

use crate::{
    gltf_resolver::GltfMeshResolver,
    update_world::{
        gltf_container::mesh_to_parry_shape, mesh_renderer::truncated_cone::TruncatedCone,
        transform_and_parent::PostUpdateSets,
    },
    ContainerEntity, DeletedSceneEntities, PrimaryUser, RendererSceneContext, SceneLoopSchedule,
    SceneSets,
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

pub type RapierReal = f64;

pub trait ToRapier<T2, R> {
    fn to_rapier(self) -> R;
}

impl<T, R> ToRapier<T, R> for T
where
    R: From<T>,
{
    fn to_rapier(self) -> R {
        self.into()
    }
}

impl<R> ToRapier<DVec3, R> for Vec3
where
    R: From<DVec3>,
{
    fn to_rapier(self) -> R {
        self.as_dvec3().into()
    }
}

impl<R> ToRapier<DQuat, R> for Quat
where
    R: From<DQuat>,
{
    fn to_rapier(self) -> R {
        self.as_dquat().into()
    }
}

pub trait ToBevy<T2> {
    type T;
    fn to_bevy(self) -> Self::T;
}

impl ToBevy<f64> for f64 {
    type T = f32;
    fn to_bevy(self) -> Self::T {
        self as f32
    }
}

impl<R> ToBevy<DVec3> for R
where
    DVec3: From<R>,
{
    type T = Vec3;
    fn to_bevy(self) -> Self::T {
        DVec3::from(self).as_vec3()
    }
}

impl<R> ToBevy<DQuat> for R
where
    DQuat: From<R>,
{
    type T = Quat;
    fn to_bevy(self) -> Self::T {
        DQuat::from(self).as_quat()
    }
}

use super::AddCrdtInterfaceExt;

pub struct MeshColliderPlugin;

pub trait ColliderType: Clone + 'static {
    fn is_trigger() -> bool;
    fn primitive_debug_color() -> Color;
    fn gltf_debug_color() -> Color;
}

#[derive(Clone, Debug)]
pub struct CtCollider;
impl ColliderType for CtCollider {
    fn is_trigger() -> bool {
        false
    }

    fn primitive_debug_color() -> Color {
        Color::srgba(1.0, 0.0, 0.0, 0.2)
    }

    fn gltf_debug_color() -> Color {
        Color::srgba(0.0, 1.0, 0.0, 0.2)
    }
}

#[derive(Component, Clone, Debug)]
pub struct MeshCollider<T: ColliderType> {
    pub shape: MeshColliderShape,
    pub collision_mask: u32,
    pub mesh_name: Option<String>,
    pub index: u32,
    pub _p: PhantomData<fn() -> T>,
}

impl Default for MeshCollider<CtCollider> {
    fn default() -> Self {
        Self {
            shape: MeshColliderShape::Box,
            collision_mask: ColliderLayer::ClPointer as u32 | ColliderLayer::ClPhysics as u32,
            mesh_name: Default::default(),
            index: Default::default(),
            _p: Default::default(),
        }
    }
}

#[derive(Component)]
pub struct DisableCollisions;

#[derive(Clone, Debug, Default)]
pub enum MeshColliderShape {
    #[default]
    Box,
    Cylinder {
        radius_top: f32,
        radius_bottom: f32,
    },
    Plane,
    Sphere,
    Shape(SharedShape, Handle<Mesh>),
    GltfShape {
        gltf_src: String,
        name: String,
    },
}

impl From<PbMeshCollider> for MeshCollider<CtCollider> {
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

pub fn add_collider_systems<T: ColliderType>(app: &mut App) {
    // collider components are created in SceneSets::Loop (by PbMeshCollider messages) and
    // in SceneSets::PostLoop (by gltf processing).
    // they are positioned in SceneSets::PostInit and
    // they are used in SceneSets::Input (for raycasts).
    // we want to avoid using CoreSet::PostUpdate as that's where we create/destroy scenes,
    // so we use SceneSets::Init for adding colliders to the scene collider data (qbvh).
    app.add_systems(Update, update_colliders::<T>.in_set(SceneSets::Init));

    // update collider transforms before queries and scenes are run, but after global transforms are updated (at end of prior frame)
    app.add_systems(
        PostUpdate,
        update_collider_transforms::<T>.in_set(PostUpdateSets::ColliderUpdate),
    );

    // show debugs whenever
    app.add_systems(Update, render_debug_colliders::<T>);
}

impl Plugin for MeshColliderPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbMeshCollider, MeshCollider<CtCollider>>(
            SceneComponentId::MESH_COLLIDER,
            ComponentPosition::EntityOnly,
        );

        app.add_systems(
            Update,
            propagate_disabled::<CtCollider>.in_set(SceneSets::Init),
        );

        app.add_systems(
            Update,
            update_scene_collider_data.in_set(SceneSets::PostInit),
        );

        // in postinit - update collider transforms
        add_collider_systems::<CtCollider>(app);

        // collider deletion has to occur within the scene loop, as the DeletedSceneEntities resource is only
        // valid within the loop
        app.world_mut()
            .resource_mut::<SceneLoopSchedule>()
            .schedule
            .add_systems(remove_deleted_colliders.in_set(SceneLoopSets::UpdateWorld));

        app.init_resource::<DebugColliders>();
        app.add_console_command::<DebugColliderCommand, _>(debug_colliders);
    }
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Default)]
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
    pub position: Vec3,
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
    query_state: Option<QueryPipeline>,
    dummy_rapier_structs: (IslandManager, RigidBodySet),
    disabled: HashSet<ColliderHandle>,
}

const SCALE_EPSILON: f32 = 0.001;
const RAYCAST_EPSILON: f64 = 0.0001;

pub trait ScaleShapeExt {
    fn scale_ext(&self, req_scale: Vec3) -> SharedShape;
}

impl ScaleShapeExt for dyn Shape {
    fn scale_ext(&self, req_scale_glam: Vec3) -> SharedShape {
        let req_scale = req_scale_glam.to_rapier();
        match self.as_typed_shape() {
            TypedShape::Ball(b) => match b.scaled(&req_scale, 5).unwrap() {
                itertools::Either::Left(ball) => SharedShape::new(ball),
                itertools::Either::Right(convex) => SharedShape::new(convex),
            },
            TypedShape::Cuboid(c) => SharedShape::new(c.scaled(&req_scale)),
            TypedShape::ConvexPolyhedron(p) => {
                SharedShape::new(p.clone().scaled(&req_scale).unwrap())
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
                            shape.0.scale_ext(req_scale_glam),
                        )
                    })
                    .collect();
                SharedShape::compound(scaled_items)
            }
            TypedShape::TriMesh(trimesh) => SharedShape::new(trimesh.clone().scaled(&req_scale)),
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
    ) -> Option<Transform> {
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

                let mut new_scale = *init_scale;
                if (req_scale - *init_scale).length_squared() > SCALE_EPSILON {
                    new_scale = req_scale;
                    // colliders don't have a scale, we have to modify the shape directly when scale changes (significantly)
                    collider.set_shape(base_collider.shape().scale_ext(req_scale));
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
                    req_translation.to_rapier(),
                    req_rotation.to_rapier(),
                ));
                return Some(initial_transform);
            }
        }

        None
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
        include_sensors: bool,
        specific_collider: Option<&ColliderId>,
    ) -> Option<RaycastResult> {
        let ray = Ray {
            origin: origin.to_rapier(),
            dir: direction.to_rapier(),
        };
        self.update_pipeline(scene_time);

        // collect colliders we started inside of, we must omit these from the query
        let mut inside = HashSet::new();
        if skip_inside {
            self.query_state.as_ref().unwrap().intersections_with_shape(
                &self.dummy_rapier_structs.1,
                &self.collider_set,
                &origin.to_rapier(),
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

        let specific_collider = specific_collider.and_then(|id| self.get_collider_handle(id));
        let predicate =
            |h, _: &_| !inside.contains(&h) && specific_collider.is_none_or(|sc| sc == h);
        let filter = QueryFilter::default()
            .groups(InteractionGroups::new(
                Group::from_bits_truncate(collision_mask),
                Group::from_bits_truncate(collision_mask),
            ))
            .predicate(&predicate);

        let filter = if include_sensors {
            filter
        } else {
            filter.exclude_sensors()
        };

        let mut closest: Option<(&ColliderId, RayIntersection)> = None;

        self.query_state.as_ref().unwrap().intersections_with_ray(
            &self.dummy_rapier_structs.1,
            &self.collider_set,
            &ray,
            distance.to_rapier(),
            true,
            filter,
            |handle, intersection| {
                if closest.is_some_and(|(_, prev_intersection)| {
                    intersection.time_of_impact > prev_intersection.time_of_impact + RAYCAST_EPSILON
                }) {
                    // too far to consider
                    return true;
                }

                let id = self.get_id(handle).unwrap();

                if closest.is_none_or(|(prev_id, prev_intersection)| {
                    // prev is too far to consider
                    intersection.time_of_impact < prev_intersection.time_of_impact - RAYCAST_EPSILON
                        // or entity is lower
                        || id.entity.id < prev_id.entity.id
                }) {
                    closest = Some((id, intersection));
                }
                true
            },
        );

        closest.map(|(id, intersection)| RaycastResult {
            id: id.clone(),
            toi: intersection.time_of_impact.to_bevy(),
            normal: intersection.normal.to_bevy(),
            face: if let FeatureId::Face(fix) = intersection.feature {
                Some(fix as usize)
            } else {
                None
            },
            position: origin + direction * intersection.time_of_impact.to_bevy(),
        })
    }

    pub fn get_ground(&mut self, scene_time: u32, origin: Vec3) -> Option<(f32, ColliderId)> {
        self.update_pipeline(scene_time);
        let contact = self.query_state.as_ref().unwrap().cast_shape(
            &self.dummy_rapier_structs.1,
            &self.collider_set,
            &(origin + Vec3::Y * PLAYER_COLLIDER_RADIUS).to_rapier(),
            &(-Vec3::Y).to_rapier(),
            &Ball::new(PLAYER_COLLIDER_RADIUS.to_rapier()),
            ShapeCastOptions {
                max_time_of_impact: 10.0,
                target_distance: 0.0,
                stop_at_penetration: true,
                compute_impact_geometry_on_penetration: true,
            },
            QueryFilter::default()
                .exclude_sensors()
                .predicate(&|h, _| self.collider_enabled(h)),
        );

        contact.map(|(handle, toi)| {
            (
                toi.time_of_impact.to_bevy(),
                self.get_id(handle).unwrap().clone(),
            )
        })
    }

    pub fn get_collider_entity(&self, id: &ColliderId) -> Option<Entity> {
        self.collider_state.get(id).and_then(|state| state.entity)
    }

    pub fn collider_enabled(&self, handle: ColliderHandle) -> bool {
        !self.disabled.contains(&handle)
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
        let ray = Ray {
            origin: origin.to_rapier(),
            dir: direction.to_rapier(),
        };
        let mut results = Vec::default();
        self.update_pipeline(scene_time);

        let mut inside = HashSet::new();
        if skip_inside {
            // collect colliders we started (nearly) inside of, we must omit these from the query
            self.query_state.as_ref().unwrap().intersections_with_shape(
                &self.dummy_rapier_structs.1,
                &self.collider_set,
                &origin.to_rapier(),
                &Ball::new(0.001),
                QueryFilter::default()
                    .groups(InteractionGroups::new(
                        Group::from_bits_truncate(collision_mask),
                        Group::from_bits_truncate(collision_mask),
                    ))
                    .exclude_sensors(),
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
            distance.to_rapier(),
            true,
            QueryFilter::default()
                .groups(InteractionGroups::new(
                    Group::from_bits_truncate(collision_mask),
                    Group::from_bits_truncate(collision_mask),
                ))
                .exclude_sensors()
                .predicate(&|h, _| !inside.contains(&h)),
            |handle, intersection| {
                results.push(RaycastResult {
                    id: self.get_id(handle).unwrap().clone(),
                    toi: intersection.time_of_impact.to_bevy(),
                    normal: intersection.normal.to_bevy(),
                    face: if let FeatureId::Face(fix) = intersection.feature {
                        Some(fix as usize)
                    } else {
                        None
                    },
                    position: origin + direction * intersection.time_of_impact.to_bevy(),
                });
                true
            },
        );

        results
    }

    pub fn cast_avatar_all(
        &mut self,
        scene_time: u32,
        origin: DVec3,
        direction: DVec3,
        distance: f64,
        collision_mask: u32,
        skip_inside: bool,
        include_sensors: bool,
        size_adjust: f32,
    ) -> Vec<RaycastResult> {
        let mut results = Vec::new();
        let mut ignore = HashSet::new();

        while let Some(result) = self.cast_avatar_nearest(
            scene_time,
            origin,
            direction,
            distance,
            collision_mask,
            skip_inside,
            include_sensors,
            ignore.iter().collect(),
            false,
            size_adjust,
        ) {
            ignore.insert(result.id.clone());
            results.push(result);
        }

        results
    }

    pub fn cast_avatar_nearest(
        &mut self,
        scene_time: u32,
        origin: DVec3,
        direction: DVec3,
        distance: f64,
        collision_mask: u32,
        skip_inside: bool,
        include_sensors: bool,
        specific_colliders: HashSet<&ColliderId>,
        include_specific: bool,
        size_adjust: f32,
    ) -> Option<RaycastResult> {
        self.update_pipeline(scene_time);

        let avatar_shape = Capsule::new_y(
            (PLAYER_COLLIDER_HEIGHT * 0.5 - PLAYER_COLLIDER_RADIUS).to_rapier(),
            (PLAYER_COLLIDER_RADIUS + size_adjust).to_rapier(),
        );

        // collect colliders we started inside of, we must omit these from the query
        let mut inside = HashSet::new();
        if skip_inside {
            self.query_state.as_ref().unwrap().intersections_with_shape(
                &self.dummy_rapier_structs.1,
                &self.collider_set,
                &(origin + DVec3::Y * PLAYER_COLLIDER_HEIGHT as f64 * 0.5).to_rapier(),
                &avatar_shape,
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

        let specific_colliders = specific_colliders
            .into_iter()
            .flat_map(|id| self.get_collider_handle(id))
            .collect::<HashSet<_>>();
        let predicate =
            |h, _: &_| !inside.contains(&h) && specific_colliders.contains(&h) == include_specific;
        let filter = QueryFilter::default()
            .groups(InteractionGroups::new(
                Group::from_bits_truncate(collision_mask),
                Group::from_bits_truncate(collision_mask),
            ))
            .predicate(&predicate);

        let filter = if include_sensors {
            filter
        } else {
            filter.exclude_sensors()
        };

        let result = self.query_state.as_ref().unwrap().cast_shape(
            &self.dummy_rapier_structs.1,
            &self.collider_set,
            &(origin + DVec3::Y * PLAYER_COLLIDER_HEIGHT as f64 * 0.5).to_rapier(),
            &direction.to_rapier(),
            &avatar_shape,
            ShapeCastOptions {
                max_time_of_impact: distance.to_rapier(),
                target_distance: 0.0,
                stop_at_penetration: true,
                compute_impact_geometry_on_penetration: true,
            },
            filter,
        );

        result.map(|(handle, intersection)| RaycastResult {
            id: self.get_id(handle).unwrap().clone(),
            toi: (intersection.time_of_impact / distance).to_bevy(),
            normal: intersection.normal1.to_bevy(),
            face: None,
            position: intersection.witness1.to_bevy(),
        })
    }

    fn avatar_intersections(
        &mut self,
        scene_time: u32,
        translation: DVec3,
        size: f32,
        mut cb: impl FnMut(&Self, ColliderHandle) -> bool,
    ) {
        self.update_pipeline(scene_time);

        let avatar_shape: &dyn parry::shape::Shape = if size == 0.0 {
            &parry::shape::Segment::new(
                (Vec3::NEG_Y * (PLAYER_COLLIDER_HEIGHT * 0.5 - PLAYER_COLLIDER_RADIUS)).to_rapier(),
                (Vec3::Y * (PLAYER_COLLIDER_HEIGHT * 0.5 - PLAYER_COLLIDER_RADIUS)).to_rapier(),
            )
        } else {
            &Capsule::new_y(
                (PLAYER_COLLIDER_HEIGHT * 0.5 - PLAYER_COLLIDER_RADIUS).to_rapier(),
                size.to_rapier(),
            )
        };

        self.query_state.as_ref().unwrap().intersections_with_shape(
            &self.dummy_rapier_structs.1,
            &self.collider_set,
            &(translation + DVec3::Y * PLAYER_COLLIDER_HEIGHT as f64 * 0.5).to_rapier(),
            avatar_shape,
            QueryFilter::default().groups(InteractionGroups::new(
                Group::from_bits_truncate(ColliderLayer::ClPhysics as u32 | GROUND_COLLISION_MASK),
                Group::from_bits_truncate(ColliderLayer::ClPhysics as u32 | GROUND_COLLISION_MASK),
            )),
            |h| cb(self, h),
        );
    }

    pub fn avatar_central_collisions(
        &mut self,
        scene_time: u32,
        translation: DVec3,
    ) -> HashSet<ColliderId> {
        let mut results = HashSet::new();
        self.avatar_intersections(scene_time, translation, 0.0, |slf, h| {
            results.insert(slf.get_id(h).unwrap().clone());
            true
        });

        results
    }

    pub fn avatar_constraints(&mut self, scene_time: u32, translation: DVec3) -> (DVec3, DVec3) {
        let avatar_inner_segment = parry::shape::Segment::new(
            (Vec3::Y * PLAYER_COLLIDER_RADIUS).to_rapier(),
            (Vec3::Y * (PLAYER_COLLIDER_HEIGHT - PLAYER_COLLIDER_RADIUS)).to_rapier(),
        );

        let mut constraint_min = DVec3::NEG_INFINITY;
        let mut constraint_max = DVec3::INFINITY;

        self.avatar_intersections(
            scene_time,
            translation,
            PLAYER_COLLIDER_RADIUS + PLAYER_COLLIDER_OVERLAP,
            |slf, h| {
                let collided = slf.collider_set.get(h).unwrap();
                let result = parry::query::closest_points(
                    &translation.to_rapier(),
                    &avatar_inner_segment,
                    collided.position(),
                    collided.shape(),
                    PLAYER_COLLIDER_RADIUS.to_rapier(),
                );

                let Ok(result) = result else {
                    panic!("{result:?}");
                };

                match result {
                    parry::query::ClosestPoints::Intersecting => (),
                    parry::query::ClosestPoints::WithinMargin(opoint, opoint1) => {
                        let offset = DVec3::from(opoint1 - opoint);
                        if offset != DVec3::ZERO {
                            let required_offset =
                                offset.normalize() * PLAYER_COLLIDER_RADIUS as f64;
                            let correction = offset - required_offset;

                            let mask_pos = correction.cmpgt(DVec3::ZERO);
                            let active_pos =
                                DVec3::select(mask_pos, correction, DVec3::NEG_INFINITY);
                            constraint_min = constraint_min.max(active_pos);

                            let mask_neg = correction.cmplt(DVec3::ZERO);
                            let active_neg = DVec3::select(mask_neg, correction, DVec3::INFINITY);
                            constraint_max = constraint_max.min(active_neg);
                        }
                    }
                    parry::query::ClosestPoints::Disjoint => (),
                }

                true
            },
        );

        (constraint_min, constraint_max)
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
                &origin.to_rapier(),
                true,
                q,
            )
            .map(|(_, point)| point.point.to_bevy())
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

    pub fn remove_colliders(&mut self, ids: &HashSet<SceneEntityId>) {
        let remove_keys = self
            .collider_state
            .keys()
            .filter(|k| ids.contains(&k.entity))
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

    fn intersect_trigger_internal(&self, collider: &Collider, mask: u32) -> Vec<ColliderId> {
        let mut results = Vec::default();

        self.query_state.as_ref().unwrap().intersections_with_shape(
            &self.dummy_rapier_structs.1,
            &self.collider_set,
            collider.position(),
            collider.shape(),
            QueryFilter::default()
                .groups(InteractionGroups {
                    memberships: Group::from_bits_truncate(mask),
                    filter: Group::from_bits_truncate(mask),
                })
                .exclude_sensors(),
            |ch| {
                let Some(collider) = self.get_id(ch) else {
                    warn!("missing collider for trigger intersection");
                    return true;
                };
                results.push(collider.clone());
                true
            },
        );
        results
    }

    pub fn intersect_id(&mut self, scene_time: u32, id: &ColliderId, mask: u32) -> Vec<ColliderId> {
        self.update_pipeline(scene_time);

        let Some(collider) = self.get_collider(id) else {
            return Vec::default();
        };

        self.intersect_trigger_internal(collider, mask)
    }

    pub fn intersect_collider(
        &mut self,
        scene_time: u32,
        collider: &Collider,
        mask: u32,
    ) -> Vec<ColliderId> {
        self.update_pipeline(scene_time);
        self.intersect_trigger_internal(collider, mask)
    }
}

pub const GROUND_COLLISION_MASK: u32 = 1 << 31;

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
                        .extend(-8.0)
                        .xzy()
                        .to_rapier(),
                )
                .collision_groups(InteractionGroups::new(
                    Group::from_bits_truncate(GROUND_COLLISION_MASK),
                    Group::from_bits_truncate(GROUND_COLLISION_MASK),
                ))
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
pub struct HasCollider<T: ColliderType>(pub ColliderId, PhantomData<fn() -> T>);

// collider state component
#[derive(Component)]
pub struct HasTrigger(pub ColliderId);

#[allow(clippy::type_complexity)]
fn update_colliders<T: ColliderType>(
    mut commands: Commands,
    // add colliders
    // any entity with a mesh collider that we're not already using, or where the mesh collider has changed
    new_colliders: Query<
        (Entity, &MeshCollider<T>, &ContainerEntity),
        Or<(Changed<MeshCollider<T>>, Without<HasCollider<T>>)>,
    >,
    // remove colliders
    // any entities with a live collider handle that don't have a mesh collider
    colliders_without_source: Query<
        (Entity, &ContainerEntity, &HasCollider<T>),
        Without<MeshCollider<T>>,
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
                        .map(|p| {
                            Point::from([p[0].to_rapier(), p[1].to_rapier(), p[2].to_rapier()])
                        })
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
        .sensor(T::is_trigger())
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
        commands
            .entity(ent)
            .try_insert(HasCollider::<T>(collider_id, Default::default()));
    }

    // remove colliders
    // any entities with a live collider handle that don't have a mesh collider or a mesh definition
    for (ent, container, collider) in colliders_without_source.iter() {
        let Ok((_, mut scene_data)) = scene_data.get_mut(container.root) else {
            warn!("missing scene root for {container:?}");
            continue;
        };

        scene_data.remove_collider(&collider.0);

        commands.entity(ent).remove::<HasCollider<T>>();
    }
}

fn remove_deleted_colliders(
    mut scene_data: Query<(&mut SceneColliderData, &DeletedSceneEntities)>,
) {
    for (mut scene_data, deleted_entities) in &mut scene_data {
        scene_data.remove_colliders(&deleted_entities.0);
    }
}

#[allow(clippy::type_complexity)]
fn propagate_disabled<T: ColliderType>(
    mut scene_datas: Query<(Entity, &mut SceneColliderData)>,
    q: Query<
        (&ContainerEntity, Option<&HasCollider<T>>, Option<&Children>),
        With<DisableCollisions>,
    >,
    r: Query<
        (Option<&HasCollider<T>>, Option<&Children>),
        Or<(With<Children>, With<HasCollider<T>>)>,
    >,
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

#[derive(Component)]
pub struct PreviousColliderTransform {
    pub prev_transform: GlobalTransform,
    pub updated: u32,
}

#[allow(clippy::type_complexity)]
pub fn update_collider_transforms<T: ColliderType>(
    mut commands: Commands,
    changed_colliders: Query<
        (Entity, &ContainerEntity, &HasCollider<T>, &GlobalTransform),
        (
            Or<(Changed<GlobalTransform>, Changed<HasCollider<T>>)>, // needs updating
        ),
    >,
    mut scene_data: Query<&mut SceneColliderData>,
    frame: Res<FrameCount>,
) {
    for (entity, container, collider, global_transform) in changed_colliders.iter() {
        let Ok(mut scene_data) = scene_data.get_mut(container.root) else {
            warn!("missing scene root for {container:?}");
            continue;
        };

        let maybe_original_transform =
            scene_data.update_collider_transform(&collider.0, global_transform);

        if let Some(original_transform) = maybe_original_transform {
            commands
                .entity(entity)
                .try_insert(PreviousColliderTransform {
                    prev_transform: GlobalTransform::from(original_transform),
                    updated: frame.0,
                });
        } else {
            commands
                .entity(entity)
                .try_remove::<PreviousColliderTransform>();
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
fn render_debug_colliders<T: ColliderType>(
    mut commands: Commands,
    debug: Res<DebugColliders>,
    mut debug_entities: Local<HashMap<Entity, Entity>>,
    with_collider: Query<
        (Entity, &MeshCollider<T>, &ContainerEntity),
        Or<(With<HasCollider<T>>, With<HasTrigger>)>,
    >,
    changed_collider: Query<Entity, Changed<MeshCollider<T>>>,
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
                base_color: T::primitive_debug_color(),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                depth_bias: 1000.0,
                ..Default::default()
            }),
            materials.add(StandardMaterial {
                base_color: T::gltf_debug_color(),
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
