use bevy::{
    diagnostic::FrameCount,
    ecs::entity::Entities,
    input::InputSystem,
    math::FloatOrd,
    platform::collections::{HashMap, HashSet},
    prelude::*,
    render::mesh::{Indices, VertexAttributeValues},
    ui::{CameraCursorPosition, RelativeCursorPosition, UiSystem},
};
use bevy_console::ConsoleCommand;
use comms::global_crdt::ForeignPlayer;
use console::DoAddConsoleCommand;
use system_bridge::{HoverAction, HoverEvent, SystemApi};

use crate::{
    gltf_resolver::GltfMeshResolver,
    update_world::{
        mesh_collider::{
            ColliderId, CtCollider, MeshCollider, MeshColliderShape, SceneColliderData,
        },
        pointer_events::PointerEvents,
        scene_ui::UiLink,
    },
    ContainerEntity, ContainingScene, PrimaryUser, RendererSceneContext, SceneEntity, SceneSets,
    PARCEL_SIZE,
};
use common::{
    dynamics::{PLAYER_COLLIDER_HEIGHT, PLAYER_COLLIDER_RADIUS},
    inputs::{Action, CommonInputAction, POINTER_SET},
    rpc::RpcStreamSender,
    structs::{CursorLocks, DebugInfo, MonotonicTimestamp, PointerTargetType, PrimaryCamera},
    util::DespawnWith,
};
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::{
        common::Vector3,
        sdk::components::{
            common::{InputAction, InteractionType, PointerEventType, RaycastHit},
            pb_pointer_events::Entry,
            ColliderLayer, PbPointerEventsResult,
        },
    },
    SceneComponentId, SceneEntityId,
};
use input_manager::{InputManager, InputPriority, InputType, MouseInteractionComponent};

pub trait IaToDcl {
    fn to_dcl(&self) -> InputAction;
}

impl IaToDcl for CommonInputAction {
    fn to_dcl(&self) -> InputAction {
        match self {
            CommonInputAction::IaPointer => InputAction::IaPointer,
            CommonInputAction::IaPrimary => InputAction::IaPrimary,
            CommonInputAction::IaSecondary => InputAction::IaSecondary,
            CommonInputAction::IaAny => InputAction::IaAny,
            CommonInputAction::IaForward => InputAction::IaForward,
            CommonInputAction::IaBackward => InputAction::IaBackward,
            CommonInputAction::IaRight => InputAction::IaRight,
            CommonInputAction::IaLeft => InputAction::IaLeft,
            CommonInputAction::IaJump => InputAction::IaJump,
            CommonInputAction::IaWalk => InputAction::IaWalk,
            CommonInputAction::IaAction3 => InputAction::IaAction3,
            CommonInputAction::IaAction4 => InputAction::IaAction4,
            CommonInputAction::IaAction5 => InputAction::IaAction5,
            CommonInputAction::IaAction6 => InputAction::IaAction6,
            CommonInputAction::IaModifier => InputAction::IaModifier,
        }
    }
}

pub trait IaToCommon {
    fn to_common(&self) -> CommonInputAction;
}

impl IaToCommon for InputAction {
    fn to_common(&self) -> CommonInputAction {
        match self {
            InputAction::IaPointer => CommonInputAction::IaPointer,
            InputAction::IaPrimary => CommonInputAction::IaPrimary,
            InputAction::IaSecondary => CommonInputAction::IaSecondary,
            InputAction::IaAny => CommonInputAction::IaAny,
            InputAction::IaForward => CommonInputAction::IaForward,
            InputAction::IaBackward => CommonInputAction::IaBackward,
            InputAction::IaRight => CommonInputAction::IaRight,
            InputAction::IaLeft => CommonInputAction::IaLeft,
            InputAction::IaJump => CommonInputAction::IaJump,
            InputAction::IaWalk => CommonInputAction::IaWalk,
            InputAction::IaAction3 => CommonInputAction::IaAction3,
            InputAction::IaAction4 => CommonInputAction::IaAction4,
            InputAction::IaAction5 => CommonInputAction::IaAction5,
            InputAction::IaAction6 => CommonInputAction::IaAction6,
            InputAction::IaModifier => CommonInputAction::IaModifier,
        }
    }
}

pub struct PointerResultPlugin;

impl Plugin for PointerResultPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PointerTarget>()
            .init_resource::<PointerRay>()
            .init_resource::<PointerDragTarget>()
            .init_resource::<UiPointerTarget>()
            .init_resource::<WorldPointerTarget>()
            .init_resource::<DebugPointers>()
            .init_resource::<AvatarColliders>()
            .init_resource::<ProximityCandidates>()
            .init_resource::<MonotonicTimestamp<PbPointerEventsResult>>();

        app.add_systems(
            PreUpdate,
            (
                update_pointer_target,
                update_manual_cursor,
                collect_proximity_candidates,
            )
                .chain()
                .after(InputSystem)
                .before(UiSystem::Focus),
        );
        app.add_systems(
            Update,
            (
                resolve_pointer_target,
                send_hover_events,
                send_proximity_events,
                send_action_events,
                debug_pointer,
                handle_hover_stream,
            )
                .chain()
                .in_set(SceneSets::Input),
        );
        app.add_console_command::<DebugPointerCommand, _>(debug_pointer_command);
    }
}

#[derive(Debug, Resource, Clone, PartialEq)]
pub struct PointerTargetInfo {
    pub container: Entity,
    pub mesh_name: Option<String>,
    /// Distance from avatar to nearest point on the target's collider.
    /// Compared against `Info::max_player_distance`.
    pub distance: FloatOrd,
    /// Distance from active camera origin to the hit point along the pointer ray.
    /// Compared against `Info::max_distance`. Same value populated into `RaycastHit::length`.
    pub camera_distance: FloatOrd,
    pub in_scene: bool,
    pub position: Option<Vec3>,
    pub normal: Option<Vec3>,
    pub face: Option<usize>,
    pub ty: PointerTargetType,
}

/// Returns true if the pointer-event entry's distance restrictions are satisfied.
///
/// Per protocol:
/// - neither field set    → camera_distance ≤ 10
/// - only max_distance    → camera_distance ≤ max_distance
/// - only max_player_dist → player_distance ≤ max_player_distance
/// - both set             → either check passes (OR)
pub fn passes_distance_check(
    event_info: Option<&dcl_component::proto_components::sdk::components::pb_pointer_events::Info>,
    camera_distance: f32,
    player_distance: f32,
) -> bool {
    let max_camera = event_info.and_then(|i| i.max_distance);
    let max_player = event_info.and_then(|i| i.max_player_distance);
    match (max_camera, max_player) {
        (None, None) => camera_distance <= 10.0,
        (Some(c), None) => camera_distance <= c,
        (None, Some(p)) => player_distance <= p,
        (Some(c), Some(p)) => camera_distance <= c || player_distance <= p,
    }
}

/// Default `max_player_distance` applied to PROXIMITY entries that leave the field
/// unset. Roughly mirrors unity-explorer's broad-phase cap, but applied here as a
/// per-entry fallback rather than a hard global cap.
pub const PROXIMITY_DEFAULT_MAX_DISTANCE: f32 = 3.0;

/// Resolve the effective player-distance threshold for a PROXIMITY entry.
///
/// `None` → fallback to `PROXIMITY_DEFAULT_MAX_DISTANCE`.
/// `Some(0)` → never qualifies (an explicit disable, since real distances are > 0).
/// `Some(x)` → use x.
pub fn proximity_threshold(
    info: Option<&dcl_component::proto_components::sdk::components::pb_pointer_events::Info>,
) -> f32 {
    info.and_then(|i| i.max_player_distance)
        .unwrap_or(PROXIMITY_DEFAULT_MAX_DISTANCE)
}

/// Returns true if a PROXIMITY entry's player-distance check passes.
pub fn passes_proximity_distance_check(
    info: Option<&dcl_component::proto_components::sdk::components::pb_pointer_events::Info>,
    player_distance: f32,
) -> bool {
    let threshold = proximity_threshold(info);
    threshold > 0.0 && player_distance <= threshold
}

#[derive(Default, Debug, Resource, Clone, PartialEq)]
pub struct PointerTarget(pub Option<PointerTargetInfo>);

#[derive(Default, Debug, Resource, Clone, PartialEq)]
pub struct PointerDragTarget {
    entities: HashMap<InputAction, (PointerTargetInfo, ActionCandidateMode, bool)>,
}

#[derive(Default, Debug, Resource, Clone, PartialEq, Eq)]
pub struct UiPointerTarget(pub UiPointerTargetValue);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum UiPointerTargetValue {
    #[default]
    None,
    Primary(Entity, Option<String>),
    World(Entity, Option<String>),
}

impl UiPointerTargetValue {
    pub fn entity(&self) -> Option<Entity> {
        match self {
            UiPointerTargetValue::None => None,
            UiPointerTargetValue::Primary(entity, _) | UiPointerTargetValue::World(entity, _) => {
                Some(*entity)
            }
        }
    }

    pub fn set_label(&mut self, label: Option<String>) {
        match self {
            UiPointerTargetValue::None => todo!(),
            UiPointerTargetValue::Primary(_, ref mut l)
            | UiPointerTargetValue::World(_, ref mut l) => *l = label,
        }
    }
}

#[derive(Resource, Default)]
pub struct AvatarColliders {
    pub collider_data: SceneColliderData,
    pub lookup: HashMap<ColliderId, Entity>,
}

#[derive(Default, Debug, Resource, Clone, PartialEq)]
pub struct WorldPointerTarget(pub Option<PointerTargetInfo>);

#[derive(Resource, Default)]
pub struct PointerRay(pub Option<Ray3d>);

/// Entities currently within `max_player_distance` of the avatar that carry at
/// least one PROXIMITY pointer-event entry. Distance is the closest-point distance
/// from the avatar's center (transform + half player collider height) to the
/// entity's collider geometry. Recomputed each frame in `PreUpdate`.
#[derive(Resource, Default, Debug, Clone)]
pub struct ProximityCandidates(pub Vec<ProximityCandidate>);

#[derive(Debug, Clone, Copy)]
pub struct ProximityCandidate {
    pub entity: Entity,
    pub distance: f32,
}

#[allow(clippy::too_many_arguments)]
fn update_pointer_target(
    camera: Query<(&Camera, &GlobalTransform), With<PrimaryCamera>>,
    player: Query<(Entity, &GlobalTransform), With<PrimaryUser>>,
    windows: Query<&Window>,
    containing_scenes: ContainingScene,
    mut scenes: Query<(Entity, &mut RendererSceneContext, &mut SceneColliderData)>,
    mut avatar_colliders: ResMut<AvatarColliders>,
    mut world_target: ResMut<WorldPointerTarget>,
    mut pointer_ray: ResMut<PointerRay>,
) {
    pointer_ray.0 = None;

    let Ok((camera, camera_position)) = camera.single() else {
        // can't do much without a camera
        return;
    };
    let Ok((player, player_transform)) = player.single() else {
        return;
    };
    let player_translation = player_transform.translation();

    // get new 3d hover target
    let Ok(window) = windows.single() else {
        return;
    };
    let cursor_position = if window.cursor_options.grab_mode == bevy::window::CursorGrabMode::Locked
    {
        // if pointer locked, just middle
        Vec2::new(window.width(), window.height()) / 2.0
    } else {
        let Some(cursor_position) = window.cursor_position() else {
            // outside window
            return;
        };
        cursor_position
    };

    let Ok(ray) = camera.viewport_to_world(camera_position, cursor_position) else {
        error!("no ray, not sure why that would happen");
        return;
    };

    pointer_ray.0 = Some(ray);

    let nearby_scenes = containing_scenes.get_area(player, PARCEL_SIZE);
    let containing_scenes = containing_scenes.get_area(player, PLAYER_COLLIDER_RADIUS);
    let maybe_nearest_hit = scenes
        .iter_mut()
        .filter(|(scene_entity, ..)| nearby_scenes.contains(scene_entity))
        .fold(
            None,
            |maybe_prior_nearest, (scene_entity, _, mut collider_data)| {
                let maybe_nearest = collider_data.cast_ray_nearest(
                    ray.origin,
                    ray.direction.into(),
                    f32::MAX,
                    ColliderLayer::ClPointer as u32,
                    true,
                    false,
                    None,
                );

                match (maybe_nearest, maybe_prior_nearest) {
                    // no prior result? this'll do
                    (Some(hit), None) => Some((scene_entity, hit)),
                    // new result is better
                    (Some(hit), Some((_, prior_hit))) if hit.toi < prior_hit.toi => {
                        Some((scene_entity, hit))
                    }
                    // prior result was at least as good
                    (_, otherwise) => otherwise,
                }
            },
        );

    let maybe_nearest_avatar = avatar_colliders.collider_data.cast_ray_nearest(
        ray.origin,
        ray.direction.into(),
        maybe_nearest_hit
            .as_ref()
            .map(|(_, hit)| hit.toi)
            .unwrap_or(f32::MAX),
        u32::MAX,
        true,
        false,
        None,
    );

    world_target.0 = None;
    if let Some(avatar_hit) = maybe_nearest_avatar {
        let nearest_point = avatar_colliders
            .collider_data
            .closest_point(player_translation, |cid| cid == &avatar_hit.id)
            .unwrap_or(player_translation);
        let distance = (nearest_point - player_translation).length();

        let avatar = avatar_colliders.lookup.get(&avatar_hit.id).unwrap();

        world_target.0 = Some(PointerTargetInfo {
            container: *avatar,
            mesh_name: None,
            distance: FloatOrd(distance),
            camera_distance: FloatOrd(avatar_hit.toi),
            in_scene: true,
            position: Some(ray.origin + ray.direction * avatar_hit.toi),
            normal: Some(avatar_hit.normal.normalize_or_zero()),
            face: avatar_hit.face,
            ty: PointerTargetType::Avatar,
        });
    } else if let Some((scene_entity, hit)) = maybe_nearest_hit {
        let (_, context, mut collider_data) = scenes.get_mut(scene_entity).unwrap();

        // get player distance
        let nearest_point = collider_data
            .closest_point(player_translation, |cid| cid == &hit.id)
            .unwrap_or(player_translation);
        let distance = (nearest_point - player_translation).length();

        if let Some(container) = context.bevy_entity(hit.id.entity) {
            let mesh_name = hit.id.name;
            world_target.0 = Some(PointerTargetInfo {
                container,
                mesh_name,
                distance: FloatOrd(distance),
                camera_distance: FloatOrd(hit.toi),
                in_scene: containing_scenes.contains(&scene_entity),
                position: Some(ray.origin + ray.direction * hit.toi),
                normal: Some(hit.normal.normalize_or_zero()),
                face: hit.face,
                ty: PointerTargetType::World,
            });
        } else {
            warn!("hit some dead entity?");
        }
    }
}

#[derive(Component, Debug)]
pub struct ResolveCursor {
    pub camera: Entity,
    pub texture_size: Vec2,
}

fn update_manual_cursor(
    world_target: Res<WorldPointerTarget>,
    uis: Query<(
        &GlobalTransform,
        &MeshCollider<CtCollider>,
        &Mesh3d,
        &ResolveCursor,
        &ContainerEntity,
    )>,
    meshes: Res<Assets<Mesh>>,
    mut cursors: Query<&mut CameraCursorPosition>,
    mut gltf_resolver: GltfMeshResolver,
    scenes: Query<&RendererSceneContext>,
) {
    for mut cursor in cursors.iter_mut() {
        cursor.0 = None;
    }

    let Some(world_target) = world_target.0.as_ref() else {
        return;
    };

    let Ok((gt, collider, render_mesh, resolve, container)) = uis.get(world_target.container)
    else {
        return;
    };

    let face_uvs = |h_mesh: &Handle<Mesh>| -> Option<Vec2> {
        let mesh = meshes.get(h_mesh)?;
        let Some(VertexAttributeValues::Float32x3(posns)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            return None;
        };
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            return None;
        };

        let face = world_target.face?;

        let indices: [usize; 3] = match mesh.indices() {
            Some(Indices::U16(ixs)) => [
                *ixs.get(face * 3)? as usize,
                *ixs.get(face * 3 + 1)? as usize,
                *ixs.get(face * 3 + 2)? as usize,
            ],
            Some(Indices::U32(ixs)) => [
                *ixs.get(face * 3)? as usize,
                *ixs.get(face * 3 + 1)? as usize,
                *ixs.get(face * 3 + 2)? as usize,
            ],
            None => [face * 3, face * 3 + 1, face * 3 + 2],
        };

        let posns: [Vec3; 3] = [
            gt.transform_point(Vec3::from(posns[indices[0]])),
            gt.transform_point(Vec3::from(posns[indices[1]])),
            gt.transform_point(Vec3::from(posns[indices[2]])),
        ];
        let target = world_target.position.unwrap();
        barycentric_coords(&posns, target).map(|bary_coords| {
            Vec2::from(uvs[indices[0]]) * bary_coords.x
                + Vec2::from(uvs[indices[1]]) * bary_coords.y
                + Vec2::from(uvs[indices[2]]) * bary_coords.z
        })
    };

    let uv = match &collider.shape {
        MeshColliderShape::Shape(_, h_mesh) => face_uvs(h_mesh),
        MeshColliderShape::GltfShape { gltf_src, name } => {
            let Ok(scene) = scenes.get(container.root) else {
                warn!("no scene");
                return;
            };
            let Ok(Some(h_mesh)) = gltf_resolver.resolve_mesh(gltf_src, &scene.hash, name) else {
                warn!("pending gltf mesh");
                return;
            };

            face_uvs(&h_mesh)
        }
        // for primitive colliders we check every face of the rendered mesh
        _ => {
            let Some(mesh) = meshes.get(render_mesh) else {
                return;
            };
            let Some(VertexAttributeValues::Float32x3(posns)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                return;
            };
            let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
            else {
                return;
            };

            let faces = match mesh.indices() {
                Some(indices) => 0..indices.len() / 3,
                None => 0..posns.len() / 3,
            };

            let mut distances_and_barycoords = faces
                .into_iter()
                .filter_map(|face| {
                    let indices: [usize; 3] = match mesh.indices() {
                        Some(Indices::U16(ixs)) => [
                            *ixs.get(face * 3)? as usize,
                            *ixs.get(face * 3 + 1)? as usize,
                            *ixs.get(face * 3 + 2)? as usize,
                        ],
                        Some(Indices::U32(ixs)) => [
                            *ixs.get(face * 3)? as usize,
                            *ixs.get(face * 3 + 1)? as usize,
                            *ixs.get(face * 3 + 2)? as usize,
                        ],
                        None => [face * 3, face * 3 + 1, face * 3 + 2],
                    };

                    let posns: [Vec3; 3] = [
                        gt.transform_point(Vec3::from(posns[indices[0]])),
                        gt.transform_point(Vec3::from(posns[indices[1]])),
                        gt.transform_point(Vec3::from(posns[indices[2]])),
                    ];
                    let target = world_target.position.unwrap();
                    barycentric_coords(&posns, target).map(|bary_coords| {
                        let uvs = Vec2::from(uvs[indices[0]]) * bary_coords.x
                            + Vec2::from(uvs[indices[1]]) * bary_coords.y
                            + Vec2::from(uvs[indices[2]]) * bary_coords.z;
                        let bary_pos = posns[0] * bary_coords.x
                            + posns[1] * bary_coords.y
                            + posns[2] * bary_coords.z;
                        let distance = bary_pos.distance(target);
                        (distance, uvs)
                    })
                })
                .collect::<Vec<_>>();

            distances_and_barycoords.sort_by_key(|(distance, _)| -FloatOrd(*distance));
            distances_and_barycoords.pop().map(|(_, uvs)| uvs)
        }
    };

    let Some(uv) = uv else {
        return;
    };

    debug!("cursor uv: {}", uv);

    let Ok(mut cursor) = cursors.get_mut(resolve.camera) else {
        return;
    };

    cursor.0 = Some(uv * resolve.texture_size);
}

/// Walks all entities carrying `PointerEvents` with at least one PROXIMITY entry
/// and computes the closest-point distance from each entity's collider geometry
/// to the avatar's center. Entities whose loosest entry threshold is satisfied
/// are pushed into `ProximityCandidates` for downstream priority resolution and
/// enter/leave dispatch.
fn collect_proximity_candidates(
    player: Query<(Entity, &GlobalTransform), With<PrimaryUser>>,
    mut scenes: Query<&mut SceneColliderData>,
    pointer_events: Query<(Entity, &SceneEntity, &PointerEvents)>,
    containing_scenes: ContainingScene,
    mut candidates: ResMut<ProximityCandidates>,
) {
    candidates.0.clear();
    let Ok((player, player_transform)) = player.single() else {
        return;
    };
    let player_center = player_transform.translation() + Vec3::Y * (PLAYER_COLLIDER_HEIGHT * 0.5);
    let nearby_scenes = containing_scenes.get_area(player, PARCEL_SIZE);

    for (entity, scene_entity, pe) in pointer_events.iter() {
        if !nearby_scenes.contains(&scene_entity.root) {
            continue;
        }
        // Loosest player-distance threshold across this entity's PROXIMITY entries.
        // Used as a per-entity broad-phase gate; per-entry checks happen downstream.
        let mut max_threshold: f32 = 0.0;
        for entry in pe.iter() {
            if entry.interaction_type != Some(InteractionType::Proximity as i32) {
                continue;
            }
            let t = proximity_threshold(entry.event_info.as_ref());
            if t > max_threshold {
                max_threshold = t;
            }
        }
        if max_threshold <= 0.0 {
            continue;
        }

        let Ok(mut collider_data) = scenes.get_mut(scene_entity.root) else {
            continue;
        };
        let entity_id = scene_entity.id;
        let Some(closest) =
            collider_data.closest_point(player_center, |cid| cid.entity == entity_id)
        else {
            continue;
        };
        let distance = (closest - player_center).length();
        if distance <= max_threshold {
            candidates.0.push(ProximityCandidate { entity, distance });
        }
    }
}

fn barycentric_coords(posns: &[Vec3; 3], target: Vec3) -> Option<Vec3> {
    let v0 = posns[1] - posns[0];
    let v1 = posns[2] - posns[0];
    let v2 = target - posns[0];
    let d00 = v0.dot(v0);
    let d01 = v0.dot(v1);
    let d11 = v1.dot(v1);
    let d20 = v2.dot(v0);
    let d21 = v2.dot(v1);
    let denom = d00 * d11 - d01 * d01;
    let v = (d11 * d20 - d01 * d21) / denom;
    let w = (d00 * d21 - d01 * d20) / denom;
    let u = 1.0 - v - w;
    if u < 0.0 || v < 0.0 || w < 0.0 || u > 1.0 || v > 1.0 || w > 1.0 {
        return None;
    }
    Some(Vec3::new(u, v, w))
}

fn resolve_pointer_target(
    ui_roots: Query<&Interaction, With<MouseInteractionComponent>>,
    entities: &Entities,
    world_target: Res<WorldPointerTarget>,
    mut ui_target: ResMut<UiPointerTarget>,
    mut target: ResMut<PointerTarget>,
    mut prev: Local<Option<Entity>>,
    tick: Res<FrameCount>,
) {
    if !ui_roots
        .iter()
        .any(|root| !matches!(root, Interaction::None))
    {
        target.0 = None;
        if prev.is_some() {
            debug!("[{}] {prev:?} -> None", tick.0);
        }
        *prev = None;
        return;
    }

    if let UiPointerTargetValue::Primary(entity, _) | UiPointerTargetValue::World(entity, _) =
        &ui_target.0
    {
        if !entities.contains(*entity) {
            ui_target.0 = UiPointerTargetValue::None;
        }
    }

    match &ui_target.0 {
        UiPointerTargetValue::Primary(e, mesh) => {
            target.0 = Some(PointerTargetInfo {
                container: *e,
                distance: FloatOrd(0.0),
                camera_distance: FloatOrd(0.0),
                in_scene: true,
                mesh_name: mesh.clone(),
                position: None,
                normal: None,
                face: None,
                ty: PointerTargetType::Ui,
            });
        }
        UiPointerTargetValue::World(e, mesh) => {
            let (distance, camera_distance, in_scene) = world_target
                .0
                .as_ref()
                .map(|t| (t.distance, t.camera_distance, t.in_scene))
                .unwrap_or((FloatOrd(0.0), FloatOrd(0.0), false));

            target.0 = Some(PointerTargetInfo {
                container: *e,
                distance,
                camera_distance,
                in_scene,
                mesh_name: mesh.clone(),
                position: None,
                normal: None,
                face: None,
                ty: PointerTargetType::Ui,
            });
        }
        UiPointerTargetValue::None => target.0.clone_from(&world_target.0),
    }

    let new = target.0.as_ref().map(|t| t.container);
    if *prev != new {
        debug!("[{}] {prev:?} -> {new:?}", tick.0);
    }
    *prev = new;
}

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/debug_pointer")]
struct DebugPointerCommand {
    show: Option<bool>,
}

#[derive(Resource, Default)]
struct DebugPointers(bool);

fn debug_pointer_command(
    mut input: ConsoleCommand<DebugPointerCommand>,
    mut debug: ResMut<DebugPointers>,
) {
    if let Some(Ok(command)) = input.take() {
        let new_state = command.show.unwrap_or(!debug.0);
        debug.0 = new_state;
        input.ok();
    }
}

fn debug_pointer(
    debug: Res<DebugPointers>,
    mut debug_info: ResMut<DebugInfo>,
    ui_target: Res<UiPointerTarget>,
    pointer_target: Res<PointerTarget>,
    target: Query<&ContainerEntity>,
    scene: Query<&RendererSceneContext>,
    colliders: Query<&MeshCollider<CtCollider>>,
) {
    if debug.0 {
        let info = if let UiPointerTargetValue::Primary(ui_ent, mesh) = &ui_target.0 {
            if let Ok(target) = target.get(*ui_ent) {
                if let Ok(scene) = scene.get(target.root) {
                    format!(
                        "ui element {}-{mesh:?} from scene {}",
                        target.container_id, scene.title
                    )
                } else {
                    format!("ui element {} unknown scene", target.container_id)
                }
            } else {
                format!("ui element (not found - bevy entity {ui_ent:?})")
            }
        } else if let Some(PointerTargetInfo {
            container,
            ref mesh_name,
            distance,
            ..
        }) = pointer_target.0
        {
            if let Ok(target) = target.get(container) {
                if let Ok(scene) = scene.get(target.root) {
                    format!(
                        "world entity {}-{:?} from scene {}, [{}], container: {:?}",
                        target.container_id,
                        mesh_name,
                        scene.title,
                        distance.0,
                        colliders.get(container)
                    )
                } else {
                    format!(
                        "world entity {}-{:?} unknown scene [{}]",
                        target.container_id, mesh_name, distance.0
                    )
                }
            } else {
                format!("world entity (not found - bevy entity {container:?})")
            }
        } else {
            "none".to_owned()
        };
        debug_info.info.insert("pointer", info);
    } else {
        debug_info.info.remove(&"pointer");
    }
}

fn send_hover_events(
    new_target: Res<PointerTarget>,
    pointer_requests: Query<(Option<&SceneEntity>, Option<&ForeignPlayer>, &PointerEvents)>,
    mut scenes: Query<(&mut RendererSceneContext, &GlobalTransform)>,
    timestamp: Res<MonotonicTimestamp<PbPointerEventsResult>>,
    mut input_manager: InputManager,
    mut previously_entered: Local<HashSet<(Entity, Option<String>, PointerTargetType)>>,
    scene_ui_ent: Query<&UiLink>,
    linked: Query<(&ChildOf, &DespawnWith, Option<&RelativeCursorPosition>)>,
) {
    debug!("hover target : {:?}", new_target);

    let container = new_target
        .0
        .as_ref()
        .map(|t| (t.container, t.mesh_name.clone(), t.ty));
    let mut new_entities = HashSet::from_iter(container.clone());
    if let Some((e, _, ty)) = container.as_ref().filter(|c| c.1.is_some()) {
        new_entities.insert((*e, None, *ty));
    }

    let mut ui_entity = container
        .and_then(|(c, _, _)| scene_ui_ent.get(c).ok())
        .map(|link| link.ui_entity);

    // walk up parent ui nodes
    while let Some((next, scene_ent, maybe_cursor)) =
        ui_entity.and_then(|ui_entity| linked.get(ui_entity).ok())
    {
        if maybe_cursor.is_some_and(|cursor| cursor.mouse_over()) {
            new_entities.insert((scene_ent.0, None, PointerTargetType::Ui));
        }

        ui_entity = Some(next.parent());
    }

    input_manager.priorities().release_all(InputPriority::Scene);
    for (entity, mesh, ty) in previously_entered.difference(&new_entities) {
        send_hover_event(
            &pointer_requests,
            &mut scenes,
            &timestamp,
            &PointerTargetInfo {
                container: *entity,
                mesh_name: mesh.clone(),
                distance: FloatOrd(0.0),
                camera_distance: FloatOrd(0.0),
                in_scene: true,
                position: None,
                normal: None,
                face: None,
                ty: *ty,
            },
            PointerEventType::PetHoverLeave,
        );
    }

    for (entity, mesh, ty) in new_entities.difference(&previously_entered) {
        if let Some(info) = new_target.0.as_ref() {
            if let Some(action) = send_hover_event(
                &pointer_requests,
                &mut scenes,
                &timestamp,
                &PointerTargetInfo {
                    container: *entity,
                    mesh_name: mesh.clone(),
                    ty: *ty,
                    ..info.clone()
                },
                PointerEventType::PetHoverEnter,
            ) {
                input_manager.priorities().reserve(
                    InputType::Action(Action::Scene(action.to_common())),
                    InputPriority::Scene,
                );
            }
        }
    }

    debug!("{:?}", new_entities);
    *previously_entered = new_entities;
}

/// Source of an action-event candidate, controlling which distance gate applies
/// and which `interaction_type` entries are eligible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionCandidateMode {
    Cursor,
    Proximity,
}

impl ActionCandidateMode {
    pub fn matches_entry(self, entry: &Entry) -> bool {
        let entry_proximity = entry.interaction_type == Some(InteractionType::Proximity as i32);
        match self {
            ActionCandidateMode::Cursor => !entry_proximity,
            ActionCandidateMode::Proximity => entry_proximity,
        }
    }

    pub fn passes_distance(
        self,
        info: Option<&dcl_component::proto_components::sdk::components::pb_pointer_events::Info>,
        camera_distance: f32,
        player_distance: f32,
    ) -> bool {
        match self {
            ActionCandidateMode::Cursor => {
                passes_distance_check(info, camera_distance, player_distance)
            }
            ActionCandidateMode::Proximity => {
                passes_proximity_distance_check(info, player_distance)
            }
        }
    }

    /// Default `priority` for entries that leave the field unset. Cursor
    /// defaults to `u32::MAX` so cursor interactions win by default — a scene
    /// must explicitly set a cursor entry's priority to demote it below a
    /// competing proximity entry. Proximity defaults to 0 per the proto.
    pub fn default_priority(self) -> u32 {
        match self {
            ActionCandidateMode::Cursor => u32::MAX,
            ActionCandidateMode::Proximity => 0,
        }
    }
}

fn filtered_events<'a>(
    pointer_requests: &'a Query<(Option<&SceneEntity>, Option<&ForeignPlayer>, &PointerEvents)>,
    info: &PointerTargetInfo,
    mode: ActionCandidateMode,
    ev_type: PointerEventType,
    action: InputAction,
) -> impl Iterator<Item = (Entity, &'a Entry)> {
    let pe = pointer_requests
        .get(info.container)
        .ok()
        .map(|(maybe_scene_entity, _, pes)| {
            pes.iter_with_scene(maybe_scene_entity.map(|se| se.root))
        })
        .into_iter()
        .flatten();

    pe.filter(move |(_, f)| {
        if f.event_type != ev_type as i32 {
            return false;
        }
        if !mode.matches_entry(f) {
            return false;
        }
        let event_button = f
            .event_info
            .as_ref()
            .and_then(|info| info.button)
            .unwrap_or(InputAction::IaAny as i32);
        event_button == InputAction::IaAny as i32 || event_button == action as i32
    })
}

/// Build and send a `PbPointerEventsResult` CRDT for a single matched entry.
#[allow(clippy::too_many_arguments)]
fn write_pointer_result(
    context: &mut RendererSceneContext,
    scene_transform: &GlobalTransform,
    info: &PointerTargetInfo,
    scene_entity_id: SceneEntityId,
    button: InputAction,
    ev_type: PointerEventType,
    direction: Option<Vec2>,
    timestamp: &MonotonicTimestamp<PbPointerEventsResult>,
) {
    let tick_number = context.tick_number;
    context.update_crdt(
        SceneComponentId::POINTER_RESULT,
        CrdtType::GO_ENT,
        scene_entity_id,
        &PbPointerEventsResult {
            button: button as i32,
            hit: Some(RaycastHit {
                position: info
                    .position
                    .as_ref()
                    .map(|p| Vector3::world_vec_from_vec3(&(*p - scene_transform.translation()))),
                global_origin: None,
                direction: direction.map(|d| Vector3::abs_vec_from_vec3(&d.extend(0.0))),
                normal_hit: info.normal.as_ref().map(Vector3::world_vec_from_vec3),
                length: info.camera_distance.0,
                mesh_name: info.mesh_name.clone(),
                entity_id: scene_entity_id.as_proto_u32(),
            }),
            state: ev_type as i32,
            timestamp: timestamp.next_timestamp(),
            analog: None,
            tick_number,
        },
    );
}

fn resolve_scene_entity_id(
    maybe_scene_entity: Option<&SceneEntity>,
    maybe_foreign_player: Option<&ForeignPlayer>,
) -> SceneEntityId {
    maybe_scene_entity
        .map(|se| se.id)
        .unwrap_or_else(|| maybe_foreign_player.unwrap().scene_id)
}

/// Walks all pointer-event entries on the target. Returns the first in-range
/// configured button (used by the caller to reserve scene input priorities),
/// and emits a hover CRDT for each entry whose `event_type` matches `ev_type`.
fn send_hover_event(
    pointer_requests: &Query<(Option<&SceneEntity>, Option<&ForeignPlayer>, &PointerEvents)>,
    scenes: &mut Query<(&mut RendererSceneContext, &GlobalTransform)>,
    timestamp: &MonotonicTimestamp<PbPointerEventsResult>,
    info: &PointerTargetInfo,
    ev_type: PointerEventType,
) -> Option<InputAction> {
    let mut action = None;
    let Ok((maybe_scene_entity, maybe_foreign_player, pe)) = pointer_requests.get(info.container)
    else {
        return None;
    };

    for (scene, ev) in pe.iter_with_scene(maybe_scene_entity.map(|se| se.root)) {
        if !info.in_scene {
            continue;
        }
        // Hover is cursor-domain — skip entries flagged as proximity.
        if !ActionCandidateMode::Cursor.matches_entry(ev) {
            continue;
        }
        if !passes_distance_check(
            ev.event_info.as_ref(),
            info.camera_distance.0,
            info.distance.0,
        ) {
            continue;
        }
        action = Some(
            ev.event_info
                .as_ref()
                .and_then(|info| info.button.map(|_| info.button()))
                .unwrap_or(InputAction::IaAny),
        );

        if ev.event_type != ev_type as i32 {
            continue;
        }

        let Ok((mut context, scene_transform)) = scenes.get_mut(scene) else {
            continue;
        };

        write_pointer_result(
            &mut context,
            scene_transform,
            info,
            resolve_scene_entity_id(maybe_scene_entity, maybe_foreign_player),
            InputAction::IaPointer,
            ev_type,
            None,
            timestamp,
        );
    }

    action
}

/// Iterates entries pre-filtered by `(event_type, action)` and `mode`-appropriate
/// `interaction_type`, emitting a CRDT for each whose distance check passes.
/// Returns the last-written scene as the "consuming" scene, which the caller uses
/// to skip duplicate root-entity events.
#[allow(clippy::too_many_arguments)]
fn send_action_event(
    pointer_requests: &Query<(Option<&SceneEntity>, Option<&ForeignPlayer>, &PointerEvents)>,
    scenes: &mut Query<(Entity, &mut RendererSceneContext, &GlobalTransform)>,
    timestamp: &MonotonicTimestamp<PbPointerEventsResult>,
    time: &Time,
    info: &PointerTargetInfo,
    mode: ActionCandidateMode,
    ev_type: PointerEventType,
    action: InputAction,
    direction: Option<Vec2>,
) -> Option<Entity> {
    let Ok((maybe_scene_entity, maybe_foreign_player, _)) = pointer_requests.get(info.container)
    else {
        return None;
    };

    let mut potential_entries =
        filtered_events(pointer_requests, info, mode, ev_type, action).peekable();
    potential_entries.peek()?;

    let mut consumer = None;
    let scene_entity_id = resolve_scene_entity_id(maybe_scene_entity, maybe_foreign_player);
    for (scene, ev) in potential_entries {
        if !info.in_scene {
            continue;
        }
        if !mode.passes_distance(
            ev.event_info.as_ref(),
            info.camera_distance.0,
            info.distance.0,
        ) {
            continue;
        }
        let Ok((_, mut context, scene_transform)) = scenes.get_mut(scene) else {
            return None;
        };

        debug!("({:?} / {action:?}) pointer hit", ev_type);
        write_pointer_result(
            &mut context,
            scene_transform,
            info,
            scene_entity_id,
            action,
            ev_type,
            direction,
            timestamp,
        );
        context.last_action_event = Some(time.elapsed_secs());
        consumer = Some(scene);
    }
    consumer
}

/// Maximum `priority` across this candidate's entries that match `(ev_type,
/// action)` after distance and interaction-type filtering. `None` means no entry
/// is eligible — the candidate doesn't compete in this bucket.
fn bucket_max_priority(
    pe: &PointerEvents,
    info: &PointerTargetInfo,
    mode: ActionCandidateMode,
    ev_type: PointerEventType,
    action: InputAction,
) -> Option<u32> {
    if !info.in_scene {
        return None;
    }
    pe.iter()
        .filter(|e| {
            if e.event_type != ev_type as i32 {
                return false;
            }
            if !mode.matches_entry(e) {
                return false;
            }
            let event_button = e
                .event_info
                .as_ref()
                .and_then(|i| i.button)
                .unwrap_or(InputAction::IaAny as i32);
            if event_button != InputAction::IaAny as i32 && event_button != action as i32 {
                return false;
            }
            mode.passes_distance(
                e.event_info.as_ref(),
                info.camera_distance.0,
                info.distance.0,
            )
        })
        .map(|e| {
            e.event_info
                .as_ref()
                .and_then(|i| i.priority)
                .unwrap_or_else(|| mode.default_priority())
        })
        .max()
}

/// Picks the winning `(info, mode)` for an action bucket across the cursor target
/// and the proximity candidate set. Highest priority wins; ties broken by entity
/// id (lower wins). Returns `None` if no candidate has an eligible entry.
pub fn resolve_action_winner(
    target: Option<&PointerTargetInfo>,
    proximity: &ProximityCandidates,
    pointer_requests: &Query<(Option<&SceneEntity>, Option<&ForeignPlayer>, &PointerEvents)>,
    ev_type: PointerEventType,
    action: InputAction,
) -> Option<(PointerTargetInfo, ActionCandidateMode)> {
    let mut best: Option<(u32, Entity, PointerTargetInfo, ActionCandidateMode)> = None;

    let mut consider =
        |entity: Entity, info: PointerTargetInfo, mode: ActionCandidateMode, prio: u32| {
            let beats = match best {
                None => true,
                Some((bp, be, _, _)) => prio > bp || (prio == bp && entity < be),
            };
            if beats {
                best = Some((prio, entity, info, mode));
            }
        };

    if let Some(info) = target {
        if let Ok((_, _, pe)) = pointer_requests.get(info.container) {
            if let Some(prio) =
                bucket_max_priority(pe, info, ActionCandidateMode::Cursor, ev_type, action)
            {
                consider(
                    info.container,
                    info.clone(),
                    ActionCandidateMode::Cursor,
                    prio,
                );
            }
        }
    }

    for cand in &proximity.0 {
        let synthetic = PointerTargetInfo {
            container: cand.entity,
            mesh_name: None,
            distance: FloatOrd(cand.distance),
            camera_distance: FloatOrd(0.0),
            in_scene: true,
            position: None,
            normal: None,
            face: None,
            ty: PointerTargetType::World,
        };
        if let Ok((_, _, pe)) = pointer_requests.get(cand.entity) {
            if let Some(prio) = bucket_max_priority(
                pe,
                &synthetic,
                ActionCandidateMode::Proximity,
                ev_type,
                action,
            ) {
                consider(cand.entity, synthetic, ActionCandidateMode::Proximity, prio);
            }
        }
    }

    best.map(|(_, _, info, mode)| (info, mode))
}

/// Tracks per-entity proximity range state across frames and emits
/// `PetProximityEnter` / `PetProximityLeave` CRDTs on transitions. Tier-1 (state)
/// events: not gated by priority or button. An entity is "in range" iff it
/// appears in `ProximityCandidates` (the collection system gates entry by the
/// entity's loosest configured threshold). On enter, every PROXIMITY entry of
/// type `PetProximityEnter` whose own per-entry gate currently passes fires its
/// CRDT. On leave, every `PetProximityLeave` entry fires unconditionally (we are,
/// by definition, outside all per-entry gates).
///
/// Per-entity granularity (rather than per-entry-index) matches `send_hover_events`
/// and is robust to scenes mutating their entry list.
fn send_proximity_events(
    candidates: Res<ProximityCandidates>,
    pointer_events: Query<(&SceneEntity, &PointerEvents)>,
    mut scenes: Query<(&mut RendererSceneContext, &GlobalTransform)>,
    timestamp: Res<MonotonicTimestamp<PbPointerEventsResult>>,
    mut prev_in_range: Local<HashSet<Entity>>,
) {
    let dist_by_entity: HashMap<Entity, f32> = candidates
        .0
        .iter()
        .map(|c| (c.entity, c.distance))
        .collect();
    let new_in_range: HashSet<Entity> = dist_by_entity.keys().copied().collect();

    let emit = |entity: Entity,
                pet: PointerEventType,
                dist: f32,
                scenes: &mut Query<(&mut RendererSceneContext, &GlobalTransform)>|
     -> Option<()> {
        let (scene_entity, pe) = pointer_events.get(entity).ok()?;
        let pb_pe = pe.msg.get(&None)?;
        for entry in pb_pe.pointer_events.iter() {
            if entry.event_type != pet as i32 {
                continue;
            }
            if entry.interaction_type != Some(InteractionType::Proximity as i32) {
                continue;
            }
            if pet == PointerEventType::PetProximityEnter
                && !passes_proximity_distance_check(entry.event_info.as_ref(), dist)
            {
                continue;
            }
            let Ok((mut context, scene_transform)) = scenes.get_mut(scene_entity.root) else {
                continue;
            };
            let button = entry
                .event_info
                .as_ref()
                .and_then(|i| i.button.map(|_| i.button()))
                .unwrap_or(InputAction::IaAny);
            let info = PointerTargetInfo {
                container: entity,
                mesh_name: None,
                distance: FloatOrd(dist),
                camera_distance: FloatOrd(0.0),
                in_scene: true,
                position: None,
                normal: None,
                face: None,
                ty: PointerTargetType::World,
            };
            write_pointer_result(
                &mut context,
                scene_transform,
                &info,
                scene_entity.id,
                button,
                pet,
                None,
                &timestamp,
            );
        }
        Some(())
    };

    for &entity in new_in_range.difference(&prev_in_range) {
        let dist = dist_by_entity.get(&entity).copied().unwrap_or(0.0);
        emit(
            entity,
            PointerEventType::PetProximityEnter,
            dist,
            &mut scenes,
        );
    }
    for &entity in prev_in_range.difference(&new_in_range) {
        emit(
            entity,
            PointerEventType::PetProximityLeave,
            0.0,
            &mut scenes,
        );
    }

    *prev_in_range = new_in_range;
}

fn send_action_events(
    target: Res<PointerTarget>,
    proximity: Res<ProximityCandidates>,
    pointer_requests: Query<(Option<&SceneEntity>, Option<&ForeignPlayer>, &PointerEvents)>,
    mut scenes: Query<(Entity, &mut RendererSceneContext, &GlobalTransform)>,
    input_mgr: InputManager,
    timestamp: Res<MonotonicTimestamp<PbPointerEventsResult>>,
    time: Res<Time>,
    mut drag_target: ResMut<PointerDragTarget>,
    mut locks: ResMut<CursorLocks>,
    player: Query<Entity, With<PrimaryUser>>,
    containing_scenes: ContainingScene,
) {
    let mut events_and_consumers = Vec::default(); // (event type, button, option<consuming scene>)

    for down in input_mgr.iter_scene_just_down().map(IaToDcl::to_dcl) {
        let consumed_by = match resolve_action_winner(
            target.0.as_ref(),
            &proximity,
            &pointer_requests,
            PointerEventType::PetDown,
            down,
        ) {
            Some((info, mode)) => {
                let consumed = send_action_event(
                    &pointer_requests,
                    &mut scenes,
                    &timestamp,
                    &time,
                    &info,
                    mode,
                    PointerEventType::PetDown,
                    down,
                    None,
                );
                // Capture the winning candidate's info+mode for the lifetime of
                // the drag (so subsequent PetDrag/Locked CRDTs fire on the same
                // entity even if cursor / proximity state changes).
                if filtered_events(
                    &pointer_requests,
                    &info,
                    mode,
                    PointerEventType::PetDrag,
                    down,
                )
                .next()
                .is_some()
                {
                    debug!("added drag");
                    drag_target
                        .entities
                        .insert(down, (info.clone(), mode, false));
                }
                if filtered_events(
                    &pointer_requests,
                    &info,
                    mode,
                    PointerEventType::PetDragLocked,
                    down,
                )
                .next()
                .is_some()
                {
                    debug!("added drag lock");
                    drag_target
                        .entities
                        .insert(down, (info.clone(), mode, true));
                }
                consumed
            }
            None => None,
        };
        events_and_consumers.push((PointerEventType::PetDown, down, consumed_by));
    }

    for up in input_mgr.iter_scene_just_up().map(IaToDcl::to_dcl) {
        let consumed_by = match resolve_action_winner(
            target.0.as_ref(),
            &proximity,
            &pointer_requests,
            PointerEventType::PetUp,
            up,
        ) {
            Some((info, mode)) => send_action_event(
                &pointer_requests,
                &mut scenes,
                &timestamp,
                &time,
                &info,
                mode,
                PointerEventType::PetUp,
                up,
                None,
            ),
            None => None,
        };
        events_and_consumers.push((PointerEventType::PetUp, up, consumed_by));
    }

    // send any drags
    let frame_delta = input_mgr.get_analog(POINTER_SET, InputPriority::Scene);

    let mut any_drag_lock = false;
    for (input, (info, mode, lock)) in drag_target.entities.iter() {
        if frame_delta != Vec2::ZERO {
            send_action_event(
                &pointer_requests,
                &mut scenes,
                &timestamp,
                &time,
                info,
                *mode,
                if *lock {
                    PointerEventType::PetDragLocked
                } else {
                    PointerEventType::PetDrag
                },
                *input,
                Some(frame_delta),
            );
        }
        any_drag_lock |= lock;
    }

    // send drag ends
    for up in input_mgr.iter_scene_just_up() {
        let up = up.to_dcl();
        if let Some((info, mode, _)) = drag_target.entities.remove(&up) {
            send_action_event(
                &pointer_requests,
                &mut scenes,
                &timestamp,
                &time,
                &info,
                mode,
                PointerEventType::PetDragEnd,
                up,
                None,
            );
        }
    }

    if any_drag_lock {
        locks.0.insert("pointer");
    } else {
        locks.0.remove("pointer");
    }

    // send events to scene roots
    if events_and_consumers.is_empty() {
        return;
    }

    let Ok(player) = player.single() else {
        return;
    };
    let containing_scenes = containing_scenes.get_area(player, PARCEL_SIZE);

    for (entity, mut context, _) in scenes
        .iter_mut()
        .filter(|(scene, ..)| containing_scenes.contains(scene))
    {
        let tick_number = context.tick_number;

        for &(pet, button, maybe_consumer) in &events_and_consumers {
            if maybe_consumer == Some(entity) {
                continue;
            }

            context.update_crdt(
                SceneComponentId::POINTER_RESULT,
                CrdtType::GO_ENT,
                SceneEntityId::ROOT,
                &PbPointerEventsResult {
                    button: button as i32,
                    hit: None,
                    state: pet as i32,
                    timestamp: timestamp.next_timestamp(),
                    analog: None,
                    tick_number,
                },
            );
        }
    }
}

#[derive(Default, Clone)]
struct PreviousHoverState(Option<HoverEvent>);

fn handle_hover_stream(
    mut events: EventReader<SystemApi>,
    mut senders: Local<Vec<RpcStreamSender<HoverEvent>>>,
    target: Res<PointerTarget>,
    actions: Query<&PointerEvents>,
    mut prev_state: Local<PreviousHoverState>,
) {
    // Collect new senders
    let new_senders = events
        .read()
        .filter_map(|ev| {
            if let SystemApi::GetHoverStream(s) = ev {
                Some(s.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    senders.extend(new_senders);
    senders.retain(|s| !s.is_closed());

    if senders.is_empty() {
        return;
    }

    let event = target.0.as_ref().map(|t| HoverEvent {
        entered: true,
        target_type: t.ty,
        actions: actions
            .get(t.container)
            .map(|pe| {
                pe.iter()
                    .map(|event| HoverAction {
                        event: event.clone(),
                        enabled: t.in_scene
                            && passes_distance_check(
                                event.event_info.as_ref(),
                                t.camera_distance.0,
                                t.distance.0,
                            ),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    });

    if event != prev_state.0 {
        if let Some(mut prev) = prev_state.0.take() {
            prev.entered = false;
            for s in &senders {
                let _ = s.send(prev.clone());
            }
        }

        if let Some(ev) = &event {
            for s in &senders {
                let _ = s.send(ev.clone());
            }
        }

        prev_state.0 = event;
    }
}
