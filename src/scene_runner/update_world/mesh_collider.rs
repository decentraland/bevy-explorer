use bevy::{
    pbr::{wireframe::Wireframe, NotShadowCaster, NotShadowReceiver},
    prelude::*,
    render::mesh::VertexAttributeValues,
    utils::{HashMap, HashSet},
};
use bevy_console::ConsoleCommand;
use rapier3d::{
    control::{EffectiveCharacterMovement, KinematicCharacterController},
    parry::{
        query::{NonlinearRigidMotion, TOIStatus},
        shape::{Ball, Capsule},
    },
    prelude::*,
};

use crate::{
    avatar::AvatarDynamicState,
    console::DoAddConsoleCommand,
    dcl::interface::ComponentPosition,
    dcl_component::{
        proto_components::sdk::components::{pb_mesh_collider, ColliderLayer, PbMeshCollider},
        SceneComponentId, SceneEntityId,
    },
    scene_runner::{
        update_world::mesh_renderer::truncated_cone::TruncatedCone, ContainerEntity,
        ContainingScene, DeletedSceneEntities, PrimaryUser, RendererSceneContext, SceneSets,
    },
    user_input::dynamics::{
        PLAYER_COLLIDER_HEIGHT, PLAYER_COLLIDER_OVERLAP, PLAYER_COLLIDER_RADIUS,
        PLAYER_GROUND_THRESHOLD,
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
        app.add_system(update_colliders.in_set(SceneSets::Init));
        app.add_system(update_scene_collider_data.in_set(SceneSets::PostInit));

        // update collider transforms before queries and scenes are run, but after global transforms are updated (at end of prior frame)
        app.add_system(update_collider_transforms.in_set(SceneSets::PostInit));

        app.init_resource::<DebugColliders>();
        app.add_console_command::<DebugColliderCommand, _>(debug_colliders);
        app.add_system(render_debug_colliders);
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
}

struct ColliderState {
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
}

const SCALE_EPSILON: f32 = 0.001;

impl SceneColliderData {
    pub fn set_collider(&mut self, id: &ColliderId, new_collider: Collider) {
        self.remove_collider(id);

        self.collider_state.insert(
            id.to_owned(),
            ColliderState {
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
    ) -> (Option<Transform>, Option<TOI>) {
        if let Some(handle) = self.get_collider_handle(id) {
            if let Some(collider) = self.collider_set.get_mut(handle) {
                let (req_scale, req_rotation, req_translation) =
                    transform.to_scale_rotation_translation();
                let ColliderState {
                    base_collider,
                    translation: init_translation,
                    scale: init_scale,
                    rotation: init_rotation,
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

                let mut cast_result = None;
                if (req_scale - *init_scale).length_squared() > SCALE_EPSILON {
                    // colliders don't have a scale, we have to modify the shape directly when scale changes (significantly)
                    collider.set_shape(scale_shape(base_collider.shape(), req_scale));
                } else if let Some(colliders) = cast_with {
                    // if scale doesn't change then just shapecast to hit colliders
                    let mut pipeline = QueryPipeline::new();
                    pipeline.update(&self.dummy_rapier_structs.1, colliders);
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
                            base_collider.shape(),
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
                state_mut.scale = req_scale;

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
                .update(&self.dummy_rapier_structs.1, &self.collider_set);
            self.query_state_valid_at = Some(scene_frame);
        }
    }

    pub fn cast_ray_nearest(
        &mut self,
        scene_time: u32,
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

    pub fn get_groundheight(&mut self, scene_time: u32, origin: Vec3) -> Option<(f32, ColliderId)> {
        self.update_pipeline(scene_time);
        let contact = self.query_state.as_ref().unwrap().cast_shape(
            &self.dummy_rapier_structs.1,
            &self.collider_set,
            &(origin + Vec3::Y * PLAYER_COLLIDER_RADIUS).into(),
            &(-Vec3::Y).into(),
            &Ball::new(PLAYER_COLLIDER_RADIUS),
            10.0,
            true,
            QueryFilter::default(),
        );

        contact.map(|(handle, toi)| (toi.toi, self.get_id(handle).unwrap().clone()))
    }

    pub fn move_character(
        &mut self,
        scene_time: u32,
        origin: Vec3,
        direction: Vec3,
        character: &KinematicCharacterController,
    ) -> EffectiveCharacterMovement {
        self.update_pipeline(scene_time);
        character.move_shape(
            0.00,
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
            QueryFilter::default(),
            |_| {},
        )
    }

    pub fn cast_ray_all(
        &mut self,
        scene_time: u32,
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

    pub fn closest_point<F: Fn(&ColliderId) -> bool>(
        &mut self,
        scene_time: u32,
        origin: Vec3,
        filter: F,
    ) -> Option<Vec3> {
        self.update_pipeline(scene_time);

        // rapier's api demands a fat pointer for whatever reason
        let predicate: &dyn Fn(ColliderHandle, &Collider) -> bool =
            &|h: ColliderHandle, _: &Collider| self.get_id(h).map_or(false, &filter);
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
            .map(|(_, point)| point.point.into())
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

        scene_data.set_collider(&collider_id, collider);
        commands.entity(ent).insert(HasCollider(collider_id));
    }

    // remove colliders
    // any entities with a live collider handle that don't have a mesh collider or a mesh definition
    for (ent, container, collider) in colliders_without_source.iter() {
        let Ok((mut scene_data, _)) = scene_data.get_mut(container.root) else {
            warn!("missing scene root for {container:?}");
            continue;
        };

        scene_data.remove_collider(&collider.0);
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
    containing_scene: ContainingScene,
    mut player: Query<(Entity, &mut Transform, &mut AvatarDynamicState), With<PrimaryUser>>,
) {
    let mut containing_scenes = HashSet::default();
    let mut parent_collider = None;
    let mut player_transform = None;

    if let Ok((player, transform, state)) = player.get_single_mut() {
        player_transform = Some(transform);
        if state.ground_height < PLAYER_GROUND_THRESHOLD {
            parent_collider = state.ground_collider.clone();
        }
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
                .map(|t| t.translation + Vec3::Y)
                .unwrap_or_default()
                .into(),
            Default::default(),
        ))
        .build(),
    );

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
                    TOIStatus::Penetrating => {
                        // skip push for penetrating collider
                    }
                    _ => {
                        // get contact point at toi
                        // use 0.9 cap to avoid clipping
                        let ratio = toi.toi.min(0.9);
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
                            warn!("disregarding push due to large delta: {}, toi: {}, normal dot1: {}, 2: {}", req_translation, toi.toi, dot_w_normal1, dot_w_normal2);
                            continue;
                        }
                        // add extra 0.1 due to character controller offset / collider size difference
                        player_transform.as_mut().unwrap().translation +=
                            req_translation.normalize_or_zero() * (req_translation.length() + 0.01);
                        debug!(
                            "[{:?} - scale = {}] push {} = {} -> 1 = {}",
                            collider.0, new_scale, ratio, contact_at_toi, contact_at_end
                        );
                        debug!("toi: {toi:?}");
                    }
                }
            }

            if Some((&container.root, &collider.0)) == parent_collider.as_ref().map(|(a, b)| (a, b))
            {
                let player_transform = player_transform.as_deref_mut().unwrap();
                let player_global_transform = GlobalTransform::from(*player_transform);
                let relative_position =
                    player_global_transform.reparented_to(&original_transform.into());
                let new_position = global_transform.mul_transform(relative_position);
                let (_, rotation, translation) = new_position.to_scale_rotation_translation();
                let planar_direction = ((rotation * -Vec3::Z) * (Vec3::X + Vec3::Z)).normalize();
                let planar_rotation = Transform::default()
                    .looking_at(planar_direction, Vec3::Y)
                    .rotation;
                player_transform.translation = translation;
                player_transform.rotation = planar_rotation;
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
    with_collider: Query<(Entity, &MeshCollider), With<HasCollider>>,
    changed_collider: Query<Entity, Changed<MeshCollider>>,
    player: Query<Entity, With<PrimaryUser>>,
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

    if let Ok(player) = player.get_single() {
        if !debug_entities.contains_key(&player) {
            let h_mesh = meshes.add(
                bevy::prelude::shape::Capsule {
                    radius: PLAYER_COLLIDER_RADIUS,
                    rings: 1,
                    depth: PLAYER_COLLIDER_HEIGHT - PLAYER_COLLIDER_RADIUS * 2.0,
                    ..Default::default()
                }
                .into(),
            );
            let debug_ent = commands
                .spawn((
                    PbrBundle {
                        mesh: h_mesh,
                        material: debug_material.as_ref().unwrap().clone(),
                        transform: Transform::from_translation(Vec3::Y),
                        ..Default::default()
                    },
                    Wireframe,
                    NotShadowCaster,
                    NotShadowReceiver,
                ))
                .id();

            commands.entity(player).add_child(debug_ent);

            debug_entities.insert(player, debug_ent);
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
