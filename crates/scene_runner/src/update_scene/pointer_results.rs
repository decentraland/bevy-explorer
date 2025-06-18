use bevy::{
    diagnostic::FrameCount, ecs::entity::Entities, input::InputSystem, math::FloatOrd, platform::collections::{HashMap, HashSet}, prelude::*, render::mesh::{Indices, VertexAttributeValues}, ui::{CameraCursorPosition, RelativeCursorPosition, UiSystem}
};
use bevy_console::ConsoleCommand;
use console::DoAddConsoleCommand;

use crate::{
    gltf_resolver::GltfMeshResolver,
    update_world::{
        mesh_collider::{MeshCollider, MeshColliderShape, SceneColliderData},
        pointer_events::PointerEvents,
        scene_ui::UiLink,
    },
    ContainerEntity, ContainingScene, DebugInfo, PrimaryUser, RendererSceneContext, SceneEntity,
    SceneSets,
};
use common::{
    dynamics::PLAYER_COLLIDER_RADIUS,
    inputs::{Action, CommonInputAction, POINTER_SET},
    structs::{CursorLocks, PrimaryCamera},
    util::DespawnWith,
};
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::{
        common::Vector3,
        sdk::components::{
            common::{InputAction, PointerEventType, RaycastHit},
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
        }
    }
}

pub struct PointerResultPlugin;

impl Plugin for PointerResultPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PointerTarget>()
            .init_resource::<PointerDragTarget>()
            .init_resource::<UiPointerTarget>()
            .init_resource::<WorldPointerTarget>()
            .init_resource::<DebugPointers>();
        app.add_systems(
            PreUpdate,
            (update_pointer_target, update_manual_cursor)
                .chain()
                .after(InputSystem)
                .before(UiSystem::Focus),
        );
        app.add_systems(
            Update,
            (
                resolve_pointer_target,
                send_hover_events,
                send_action_events,
                debug_pointer,
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
    pub distance: FloatOrd,
    pub position: Option<Vec3>,
    pub normal: Option<Vec3>,
    pub face: Option<usize>,
    pub is_ui: Option<bool>,
}

#[derive(Default, Debug, Resource, Clone, PartialEq)]
pub struct PointerTarget(pub Option<PointerTargetInfo>);

#[derive(Default, Debug, Resource, Clone, PartialEq)]
pub struct PointerDragTarget {
    entities: HashMap<InputAction, (PointerTargetInfo, bool)>,
}

#[derive(Default, Debug, Resource, Clone, PartialEq, Eq)]
pub struct UiPointerTarget(pub UiPointerTargetValue);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum UiPointerTargetValue {
    #[default]
    None,
    Primary(Entity),
    World(Entity),
}

#[derive(Default, Debug, Resource, Clone, PartialEq)]
pub struct WorldPointerTarget(Option<PointerTargetInfo>);

#[allow(clippy::too_many_arguments)]
fn update_pointer_target(
    camera: Query<(&Camera, &GlobalTransform), With<PrimaryCamera>>,
    player: Query<(Entity, &GlobalTransform), With<PrimaryUser>>,
    windows: Query<&Window>,
    containing_scenes: ContainingScene,
    mut scenes: Query<(Entity, &mut RendererSceneContext, &mut SceneColliderData)>,
    mut world_target: ResMut<WorldPointerTarget>,
) {
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
    let cursor_position = if window.cursor_options.grab_mode == bevy::window::CursorGrabMode::Locked {
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

    let containing_scenes = containing_scenes.get_area(player, PLAYER_COLLIDER_RADIUS);
    let maybe_nearest_hit = scenes
        .iter_mut()
        .filter(|(scene_entity, ..)| containing_scenes.contains(scene_entity))
        .fold(
            None,
            |maybe_prior_nearest, (scene_entity, context, mut collider_data)| {
                let maybe_nearest = collider_data.cast_ray_nearest(
                    context.last_update_frame,
                    ray.origin,
                    ray.direction.into(),
                    f32::MAX,
                    ColliderLayer::ClPointer as u32,
                    true,
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

    world_target.0 = None;
    if let Some((scene_entity, hit)) = maybe_nearest_hit {
        let (_, context, mut collider_data) = scenes.get_mut(scene_entity).unwrap();

        // get player distance
        let nearest_point = collider_data
            .closest_point(context.last_update_frame, player_translation, |cid| {
                cid == &hit.id
            })
            .unwrap_or(player_translation);
        let distance = (nearest_point - player_translation).length();

        if let Some(container) = context.bevy_entity(hit.id.entity) {
            let mesh_name = hit.id.name;
            world_target.0 = Some(PointerTargetInfo {
                container,
                mesh_name,
                distance: FloatOrd(distance),
                position: Some(ray.origin + ray.direction * hit.toi),
                normal: Some(hit.normal.normalize_or_zero()),
                face: hit.face,
                is_ui: Some(false),
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
        &MeshCollider,
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

    if let UiPointerTargetValue::Primary(entity) | UiPointerTargetValue::World(entity) =
        &ui_target.0
    {
        if !entities.contains(*entity) {
            ui_target.0 = UiPointerTargetValue::None;
        }
    }

    match &ui_target.0 {
        UiPointerTargetValue::Primary(e) => {
            target.0 = Some(PointerTargetInfo {
                container: *e,
                distance: FloatOrd(0.0),
                mesh_name: None,
                position: None,
                normal: None,
                face: None,
                is_ui: Some(true),
            });
        }
        UiPointerTargetValue::World(e) => {
            let distance = world_target
                .0
                .as_ref()
                .map(|t| t.distance)
                .unwrap_or(FloatOrd(0.0));

            target.0 = Some(PointerTargetInfo {
                container: *e,
                distance,
                mesh_name: None,
                position: None,
                normal: None,
                face: None,
                is_ui: Some(true),
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
    }
}

fn debug_pointer(
    debug: Res<DebugPointers>,
    mut debug_info: ResMut<DebugInfo>,
    ui_target: Res<UiPointerTarget>,
    pointer_target: Res<PointerTarget>,
    target: Query<&ContainerEntity>,
    scene: Query<&RendererSceneContext>,
    colliders: Query<&MeshCollider>,
) {
    if debug.0 {
        let info = if let UiPointerTargetValue::Primary(ui_ent) = &ui_target.0 {
            if let Ok(target) = target.get(*ui_ent) {
                if let Ok(scene) = scene.get(target.root) {
                    format!(
                        "ui element {} from scene {}",
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
    pointer_requests: Query<(&SceneEntity, Option<&PointerEvents>)>,
    mut scenes: Query<(&mut RendererSceneContext, &GlobalTransform)>,
    frame: Res<FrameCount>,
    mut input_manager: InputManager,
    mut previously_entered: Local<HashSet<Entity>>,
    scene_ui_ent: Query<&UiLink>,
    linked: Query<(&ChildOf, &DespawnWith, Option<&RelativeCursorPosition>)>,
) {
    debug!("hover target : {:?}", new_target);

    let mut send_event =
        |info: &PointerTargetInfo, ev_type: PointerEventType| -> Option<InputAction> {
            let mut action = None;
            if let Ok((scene_entity, maybe_pe)) = pointer_requests.get(info.container) {
                if let Some(pe) = maybe_pe {
                    let mut potential_entries = pe.msg.pointer_events.iter().peekable();
                    // check there's at least one potential request before doing any work
                    if potential_entries.peek().is_some() {
                        let Ok((mut context, scene_transform)) = scenes.get_mut(scene_entity.root)
                        else {
                            return None;
                        };

                        for ev in potential_entries {
                            let max_distance = ev
                                .event_info
                                .as_ref()
                                .and_then(|info| info.max_distance)
                                .unwrap_or(10.0);
                            if info.distance <= FloatOrd(max_distance) {
                                action = Some(
                                    ev.event_info
                                        .as_ref()
                                        .and_then(|info| info.button.map(|_| info.button()))
                                        .unwrap_or(InputAction::IaAny),
                                );

                                if ev.event_type != ev_type as i32 {
                                    continue;
                                }

                                let tick_number = context.tick_number;
                                context.update_crdt(
                                    SceneComponentId::POINTER_RESULT,
                                    CrdtType::GO_ENT,
                                    scene_entity.id,
                                    &PbPointerEventsResult {
                                        button: InputAction::IaPointer as i32,
                                        hit: Some(RaycastHit {
                                            position: info.position.as_ref().map(|p| {
                                                Vector3::world_vec_from_vec3(
                                                    &(*p - scene_transform.translation()),
                                                )
                                            }),
                                            global_origin: None,
                                            direction: None,
                                            normal_hit: info
                                                .normal
                                                .as_ref()
                                                .map(Vector3::world_vec_from_vec3),
                                            length: info.distance.0,
                                            mesh_name: info.mesh_name.clone(),
                                            entity_id: scene_entity.id.as_proto_u32(),
                                        }),
                                        state: ev_type as i32,
                                        timestamp: frame.0,
                                        analog: None,
                                        tick_number,
                                    },
                                );
                            }
                        }
                    }
                }
            } else {
                warn!(
                    "failed to query entity for hover event {ev_type:?}: {:?}",
                    info.container
                );
            }

            action
        };

    let container = new_target.0.as_ref().map(|t| t.container);
    let mut new_entities = HashSet::from_iter(container);

    let mut ui_entity = container
        .and_then(|c| scene_ui_ent.get(c).ok())
        .map(|link| link.ui_entity);

    // walk up parent ui nodes
    while let Some((next, scene_ent, maybe_cursor)) =
        ui_entity.and_then(|ui_entity| linked.get(ui_entity).ok())
    {
        if maybe_cursor.is_some_and(|cursor| cursor.mouse_over()) {
            new_entities.insert(scene_ent.0);
        }

        ui_entity = Some(next.parent());
    }

    input_manager.priorities().release_all(InputPriority::Scene);
    for entity in previously_entered.difference(&new_entities) {
        send_event(
            &PointerTargetInfo {
                container: *entity,
                mesh_name: None,
                distance: FloatOrd(0.0),
                position: None,
                normal: None,
                face: None,
                is_ui: None,
            },
            PointerEventType::PetHoverLeave,
        );
    }

    for entity in new_entities.difference(&previously_entered) {
        if let Some(info) = new_target.0.as_ref() {
            if let Some(action) = send_event(
                &PointerTargetInfo {
                    container: *entity,
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

fn send_action_events(
    target: Res<PointerTarget>,
    pointer_requests: Query<(&SceneEntity, Option<&PointerEvents>)>,
    mut scenes: Query<(&mut RendererSceneContext, &GlobalTransform)>,
    input_mgr: InputManager,
    frame: Res<FrameCount>,
    time: Res<Time>,
    mut drag_target: ResMut<PointerDragTarget>,
    mut locks: ResMut<CursorLocks>,
) {
    fn filtered_events<'a>(
        pointer_requests: &'a Query<(&SceneEntity, Option<&PointerEvents>)>,
        info: &PointerTargetInfo,
        ev_type: PointerEventType,
        action: InputAction,
    ) -> impl Iterator<Item = &'a Entry> {
        let pe = pointer_requests
            .get(info.container)
            .ok()
            .and_then(|(_, pes)| pes)
            .map(|pes| pes.msg.pointer_events.iter())
            .unwrap_or_default();

        pe.filter(move |f| {
            let event_button = f
                .event_info
                .as_ref()
                .and_then(|info| info.button)
                .unwrap_or(InputAction::IaAny as i32);
            f.event_type == ev_type as i32
                && (event_button == InputAction::IaAny as i32 || event_button == action as i32)
        })
    }

    let mut send_event = |info: &PointerTargetInfo,
                          ev_type: PointerEventType,
                          action: InputAction,
                          direction: Option<Vec2>|
     -> bool {
        let Ok((scene_entity, _)) = pointer_requests.get(info.container) else {
            return false;
        };

        let mut potential_entries =
            filtered_events(&pointer_requests, info, ev_type, action).peekable();
        // check there's at least one potential request before doing any work
        if potential_entries.peek().is_some() {
            let Ok((mut context, scene_transform)) = scenes.get_mut(scene_entity.root) else {
                return false;
            };

            let mut consumed = false;
            for ev in potential_entries {
                let max_distance = ev
                    .event_info
                    .as_ref()
                    .and_then(|info| info.max_distance)
                    .unwrap_or(10.0);
                if info.distance.0 <= max_distance {
                    let tick_number = context.tick_number;
                    let hit = RaycastHit {
                        position: info.position.as_ref().map(|p| {
                            Vector3::world_vec_from_vec3(&(*p - scene_transform.translation()))
                        }),
                        global_origin: None,
                        direction: direction.map(|d| Vector3::abs_vec_from_vec3(&d.extend(0.0))),
                        normal_hit: info.normal.as_ref().map(Vector3::world_vec_from_vec3),
                        length: info.distance.0,
                        mesh_name: info.mesh_name.clone(),
                        entity_id: scene_entity.id.as_proto_u32(),
                    };
                    debug!("({:?} / {action:?}) pointer hit: {hit:?}", ev_type);
                    // send to target entity
                    context.update_crdt(
                        SceneComponentId::POINTER_RESULT,
                        CrdtType::GO_ENT,
                        scene_entity.id,
                        &PbPointerEventsResult {
                            button: action as i32,
                            hit: Some(hit),
                            state: ev_type as i32,
                            timestamp: frame.0,
                            analog: None,
                            tick_number,
                        },
                    );
                    context.last_action_event = Some(time.elapsed_secs());
                    consumed = true;
                }
            }
            consumed
        } else {
            false
        }
    };

    // send event to action target
    let mut unconsumed = Vec::default();
    if let Some(info) = target.0.as_ref() {
        let unconsumed_down = input_mgr
            .iter_scene_just_down()
            .map(IaToDcl::to_dcl)
            .filter(|down| {
                let consumed = send_event(info, PointerEventType::PetDown, *down, None);
                if filtered_events(&pointer_requests, info, PointerEventType::PetDrag, *down)
                    .next()
                    .is_some()
                {
                    debug!("added drag");
                    drag_target.entities.insert(*down, (info.clone(), false));
                }
                if filtered_events(
                    &pointer_requests,
                    info,
                    PointerEventType::PetDragLocked,
                    *down,
                )
                .next()
                .is_some()
                {
                    debug!("added drag lock");
                    drag_target.entities.insert(*down, (info.clone(), true));
                }

                !consumed
            })
            .map(|button| (PointerEventType::PetDown, button));
        unconsumed.extend(unconsumed_down);

        let unconsumed_up = input_mgr
            .iter_scene_just_up()
            .map(IaToDcl::to_dcl)
            .filter(|up| !send_event(info, PointerEventType::PetUp, *up, None))
            .map(|button| (PointerEventType::PetUp, button));
        unconsumed.extend(unconsumed_up);
    } else {
        unconsumed.extend(
            input_mgr
                .iter_scene_just_down()
                .map(IaToDcl::to_dcl)
                .map(|b| (PointerEventType::PetDown, b)),
        );
        unconsumed.extend(
            input_mgr
                .iter_scene_just_up()
                .map(IaToDcl::to_dcl)
                .map(|b| (PointerEventType::PetUp, b)),
        );
    }

    // send any drags
    let frame_delta = input_mgr.get_analog(POINTER_SET, InputPriority::Scene);

    let mut any_drag_lock = false;
    for (input, (info, lock)) in drag_target.entities.iter() {
        if frame_delta != Vec2::ZERO {
            send_event(
                info,
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
        if let Some((info, _)) = drag_target.entities.remove(&up) {
            send_event(&info, PointerEventType::PetDragEnd, up, None);
        }
    }

    if any_drag_lock {
        locks.0.insert("pointer");
    } else {
        locks.0.remove("pointer");
    }

    // send events to scene roots
    if unconsumed.is_empty() {
        return;
    }

    for (mut context, _) in scenes.iter_mut() {
        let tick_number = context.tick_number;

        for &(pet, button) in &unconsumed {
            context.update_crdt(
                SceneComponentId::POINTER_RESULT,
                CrdtType::GO_ENT,
                SceneEntityId::ROOT,
                &PbPointerEventsResult {
                    button: button as i32,
                    hit: None,
                    state: pet as i32,
                    timestamp: frame.0,
                    analog: None,
                    tick_number,
                },
            );
        }
    }
}
