use bevy::{
    core::FrameCount,
    input::InputSystem,
    math::FloatOrd,
    prelude::*,
    render::mesh::{Indices, VertexAttributeValues},
    ui::{ManualCursorPosition, UiSystem},
    utils::HashSet,
};
use bevy_console::ConsoleCommand;
use console::DoAddConsoleCommand;

use crate::{
    gltf_resolver::GltfMeshResolver,
    update_world::{
        mesh_collider::{MeshCollider, MeshColliderShape, SceneColliderData},
        pointer_events::PointerEvents,
    },
    ContainerEntity, ContainingScene, DebugInfo, PrimaryUser, RendererSceneContext, SceneEntity,
    SceneSets,
};
use common::{dynamics::PLAYER_COLLIDER_RADIUS, structs::PrimaryCamera};
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::{
        common::Vector3,
        sdk::components::{
            common::{InputAction, PointerEventType, RaycastHit},
            ColliderLayer, PbPointerEventsResult,
        },
    },
    SceneComponentId, SceneEntityId,
};
use input_manager::{AcceptInput, InputManager};

pub struct PointerResultPlugin;

impl Plugin for PointerResultPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PointerTarget>()
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
}

#[derive(Default, Debug, Resource, Clone, PartialEq)]
pub struct PointerTarget(pub Option<PointerTargetInfo>);

#[derive(Default, Debug, Resource, Clone, PartialEq, Eq)]
pub enum UiPointerTarget {
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
    let Ok((camera, camera_position)) = camera.get_single() else {
        // can't do much without a camera
        return;
    };
    let Ok((player, player_transform)) = player.get_single() else {
        return;
    };
    let player_translation = player_transform.translation();

    // get new 3d hover target
    let Ok(window) = windows.get_single() else {
        return;
    };
    let cursor_position = if window.cursor.grab_mode == bevy::window::CursorGrabMode::Locked {
        // if pointer locked, just middle
        Vec2::new(window.width(), window.height()) / 2.0
    } else {
        let Some(cursor_position) = window.cursor_position() else {
            // outside window
            return;
        };
        cursor_position
    };

    let Some(ray) = camera.viewport_to_world(camera_position, cursor_position) else {
        error!("no ray, not sure why that would happen");
        return;
    };

    let containing_scenes =
        HashSet::from_iter(containing_scenes.get_area(player, PLAYER_COLLIDER_RADIUS));
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
        &Handle<Mesh>,
        &ResolveCursor,
        &ContainerEntity,
    )>,
    meshes: Res<Assets<Mesh>>,
    mut cursors: Query<&mut ManualCursorPosition>,
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
    world_target: Res<WorldPointerTarget>,
    ui_target: Res<UiPointerTarget>,
    mut target: ResMut<PointerTarget>,
    accept_input: Res<AcceptInput>,
) {
    match *ui_target {
        UiPointerTarget::None => {
            // check for system ui
            if !accept_input.mouse {
                target.0 = None;
                return;
            }

            target.0.clone_from(&world_target.0);
        }
        UiPointerTarget::Primary(e) => {
            target.0 = Some(PointerTargetInfo {
                container: e,
                distance: FloatOrd(0.0),
                mesh_name: None,
                position: None,
                normal: None,
                face: None,
            });
        }
        UiPointerTarget::World(e) => {
            let distance = world_target
                .0
                .as_ref()
                .map(|t| t.distance)
                .unwrap_or(FloatOrd(0.0));

            target.0 = Some(PointerTargetInfo {
                container: e,
                distance,
                mesh_name: None,
                position: None,
                normal: None,
                face: None,
            });
        }
    }
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
) {
    if debug.0 {
        let info = if let UiPointerTarget::Primary(ui_ent) = *ui_target {
            if let Ok(target) = target.get(ui_ent) {
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
                        "world entity {}-{:?} from scene {}, [{}]",
                        target.container_id, mesh_name, scene.title, distance.0
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
    mut prior_target: Local<PointerTarget>,
    pointer_requests: Query<(&SceneEntity, Option<&PointerEvents>)>,
    mut scenes: Query<(&mut RendererSceneContext, &GlobalTransform)>,
    frame: Res<FrameCount>,
) {
    if *new_target == *prior_target {
        return;
    }

    debug!("hover target : {:?}", new_target);

    let mut send_event = |info: &PointerTargetInfo, ev_type: PointerEventType| {
        if let Ok((scene_entity, maybe_pe)) = pointer_requests.get(info.container) {
            if let Some(pe) = maybe_pe {
                let mut potential_entries = pe
                    .msg
                    .pointer_events
                    .iter()
                    .filter(|f| f.event_type == ev_type as i32)
                    .peekable();
                // check there's at least one potential request before doing any work
                if potential_entries.peek().is_some() {
                    let Ok((mut context, scene_transform)) = scenes.get_mut(scene_entity.root)
                    else {
                        return;
                    };

                    for ev in potential_entries {
                        let max_distance = ev
                            .event_info
                            .as_ref()
                            .and_then(|info| info.max_distance)
                            .unwrap_or(10.0);
                        if info.distance <= FloatOrd(max_distance) {
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
    };

    if new_target.0.as_ref().map(|t| t.container) != prior_target.0.as_ref().map(|t| t.container) {
        if let Some(info) = prior_target.0.as_ref() {
            send_event(info, PointerEventType::PetHoverLeave);
        }

        if let Some(info) = new_target.0.as_ref() {
            send_event(info, PointerEventType::PetHoverEnter);
        }
    }

    *prior_target = new_target.clone();
}

fn send_action_events(
    target: Res<PointerTarget>,
    pointer_requests: Query<(&SceneEntity, Option<&PointerEvents>)>,
    mut scenes: Query<(Entity, &mut RendererSceneContext, &GlobalTransform)>,
    input_mgr: InputManager,
    frame: Res<FrameCount>,
    time: Res<Time>,
) {
    let mut send_event = |info: &PointerTargetInfo,
                          ev_type: PointerEventType,
                          action: InputAction| {
        if let Ok((scene_entity, maybe_pe)) = pointer_requests.get(info.container) {
            if let Some(pe) = maybe_pe {
                let mut potential_entries = pe
                    .msg
                    .pointer_events
                    .iter()
                    .filter(|f| {
                        let event_button = f
                            .event_info
                            .as_ref()
                            .and_then(|info| info.button)
                            .unwrap_or(InputAction::IaAny as i32);
                        f.event_type == ev_type as i32
                            && (event_button == InputAction::IaAny as i32
                                || event_button == action as i32)
                    })
                    .peekable();
                // check there's at least one potential request before doing any work
                if potential_entries.peek().is_some() {
                    let Ok((_, mut context, scene_transform)) = scenes.get_mut(scene_entity.root)
                    else {
                        return;
                    };
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
                                    Vector3::world_vec_from_vec3(
                                        &(*p - scene_transform.translation()),
                                    )
                                }),
                                global_origin: None,
                                direction: None,
                                normal_hit: info.normal.as_ref().map(Vector3::world_vec_from_vec3),
                                length: info.distance.0,
                                mesh_name: info.mesh_name.clone(),
                                entity_id: scene_entity.id.as_proto_u32(),
                            };
                            debug!("pointer hit: {hit:?}");
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
                            context.last_action_event = Some(time.elapsed_seconds());
                        }
                    }
                }
            }
        } else {
            warn!(
                "failed to query entity for button event [{action:?} {ev_type:?}]: {:?}",
                info.container
            );
        }
    };

    // send event to hover target
    if let Some(info) = target.0.as_ref() {
        for down in input_mgr.iter_just_down() {
            send_event(info, PointerEventType::PetDown, *down);
        }

        for up in input_mgr.iter_just_up() {
            send_event(info, PointerEventType::PetUp, *up);
        }
    }

    // send events to scene roots
    if !input_mgr.any_just_acted() {
        return;
    }

    let scene_entity = target
        .0
        .as_ref()
        .and_then(|info| pointer_requests.get(info.container).map(|(e, _)| e).ok());
    let scene_root = scene_entity.map(|e| e.root);

    for (root_ent, mut context, scene_transform) in scenes.iter_mut() {
        let tick_number = context.tick_number;
        // we send the entity id to the containing scene, otherwise we send ROOT
        // as the entity id is only valid within the containing scene context
        let entity_id = if scene_root == Some(root_ent) {
            scene_entity.as_ref().unwrap().id.as_proto_u32()
        } else {
            SceneEntityId::ROOT.as_proto_u32()
        };

        let hit = RaycastHit {
            position: target.0.as_ref().and_then(|info| {
                info.position
                    .as_ref()
                    .map(|p| Vector3::world_vec_from_vec3(&(*p - scene_transform.translation())))
            }),
            global_origin: None,
            direction: None,
            normal_hit: target
                .0
                .as_ref()
                .and_then(|info| info.normal.as_ref().map(Vector3::world_vec_from_vec3)),
            length: target.0.as_ref().map_or(0.0, |info| info.distance.0),
            mesh_name: target.0.as_ref().and_then(|info| info.mesh_name.clone()),
            entity_id,
        };

        for down in input_mgr.iter_just_down() {
            context.update_crdt(
                SceneComponentId::POINTER_RESULT,
                CrdtType::GO_ENT,
                SceneEntityId::ROOT,
                &PbPointerEventsResult {
                    button: *down as i32,
                    hit: Some(hit.clone()),
                    state: PointerEventType::PetDown as i32,
                    timestamp: frame.0,
                    analog: None,
                    tick_number,
                },
            );
        }

        for up in input_mgr.iter_just_up() {
            context.update_crdt(
                SceneComponentId::POINTER_RESULT,
                CrdtType::GO_ENT,
                SceneEntityId::ROOT,
                &PbPointerEventsResult {
                    button: *up as i32,
                    hit: Some(hit.clone()),
                    state: PointerEventType::PetUp as i32,
                    timestamp: frame.0,
                    analog: None,
                    tick_number,
                },
            );
        }
    }
}
