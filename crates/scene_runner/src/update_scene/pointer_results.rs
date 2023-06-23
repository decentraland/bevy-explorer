use bevy::{core::FrameCount, prelude::*, utils::HashSet};

use crate::{
    update_world::{mesh_collider::SceneColliderData, pointer_events::PointerEvents},
    ContainingScene, PrimaryUser, RendererSceneContext, SceneEntity, SceneSets,
};
use common::{dynamics::PLAYER_COLLIDER_RADIUS, structs::PrimaryCamera};
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::sdk::components::{
        common::{InputAction, PointerEventType, RaycastHit},
        ColliderLayer, PbPointerEventsResult,
    },
    SceneComponentId, SceneEntityId,
};
use input_manager::InputManager;

pub struct PointerResultPlugin;

impl Plugin for PointerResultPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PointerTarget>()
            .init_resource::<UiPointerTarget>();
        app.add_systems(
            (update_pointer_target, send_hover_events, send_action_events)
                .chain()
                .in_set(SceneSets::Input),
        );
    }
}

#[derive(Default, Debug, Resource, Clone, PartialEq, Eq)]
pub enum PointerTarget {
    #[default]
    None,
    Some {
        container: Entity,
        mesh_name: Option<String>,
    },
}

#[derive(Default, Debug, Resource, Clone, PartialEq, Eq)]
pub enum UiPointerTarget {
    #[default]
    None,
    Some(Entity),
}

fn update_pointer_target(
    camera: Query<(&Camera, &GlobalTransform), With<PrimaryCamera>>,
    player: Query<Entity, With<PrimaryUser>>,
    windows: Query<&Window>,
    containing_scenes: ContainingScene,
    mut scenes: Query<(Entity, &mut RendererSceneContext, &mut SceneColliderData)>,
    mut hover_target: ResMut<PointerTarget>,
    ui_target: Res<UiPointerTarget>,
) {
    let Ok((camera, camera_position)) = camera.get_single() else {
        // can't do much without a camera
        return
    };
    let Ok(player) = player.get_single() else {
        return;
    };

    // first check for ui target
    if let UiPointerTarget::Some(t) = *ui_target {
        *hover_target = PointerTarget::Some {
            container: t,
            mesh_name: None,
        };
        return;
    }

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
                    ray.direction,
                    f32::MAX,
                    ColliderLayer::ClPointer as u32 | ColliderLayer::ClPhysics as u32,
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

    *hover_target = PointerTarget::None;
    if let Some((scene_entity, hit)) = maybe_nearest_hit {
        let context = scenes
            .get_component::<RendererSceneContext>(scene_entity)
            .unwrap();
        if let Some(container) = context.bevy_entity(hit.id.entity) {
            let mesh_name = hit.id.name;
            *hover_target = PointerTarget::Some {
                container,
                mesh_name,
            };
        }
    }
}

fn send_hover_events(
    player: Query<&GlobalTransform, With<PrimaryUser>>,
    new_target: Res<PointerTarget>,
    mut prior_target: Local<PointerTarget>,
    pointer_requests: Query<(&SceneEntity, Option<&PointerEvents>)>,
    mut scenes: Query<(&mut RendererSceneContext, &mut SceneColliderData)>,
    frame: Res<FrameCount>,
) {
    if *new_target == *prior_target {
        return;
    }

    debug!("hover target : {:?}", new_target);

    let Ok(player_position) = player.get_single() else {
        // can't do much without a camera
        return
    };

    let player_translation = player_position.translation();

    let mut send_event = |entity: &Entity,
                          mesh_name: &Option<String>,
                          ev_type: PointerEventType| {
        if let Ok((scene_entity, maybe_pe)) = pointer_requests.get(*entity) {
            if let Some(pe) = maybe_pe {
                let mut potential_entries = pe
                    .msg
                    .pointer_events
                    .iter()
                    .filter(|f| f.event_type == ev_type as i32)
                    .peekable();
                // check there's at least one potential request before doing any work
                if potential_entries.peek().is_some() {
                    let Ok((mut context, mut collider_data)) = scenes.get_mut(scene_entity.root) else { panic!() };
                    // get distance
                    let nearest_point = collider_data
                        .closest_point(context.last_update_frame, player_translation, |cid| {
                            cid.entity == scene_entity.id && &cid.name == mesh_name
                        })
                        .unwrap_or(player_translation);
                    let distance = (nearest_point - player_translation).length();

                    for ev in potential_entries {
                        let max_distance = ev
                            .event_info
                            .as_ref()
                            .and_then(|info| info.max_distance)
                            .unwrap_or(10.0);
                        if distance <= max_distance {
                            let tick_number = context.tick_number;
                            context.update_crdt(
                                SceneComponentId::POINTER_RESULT,
                                CrdtType::GO_ENT,
                                scene_entity.id,
                                &PbPointerEventsResult {
                                    button: InputAction::IaPointer as i32,
                                    hit: Some(RaycastHit {
                                        position: None,
                                        global_origin: None,
                                        direction: None,
                                        normal_hit: None,
                                        length: distance,
                                        mesh_name: mesh_name.clone(),
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
            warn!("failed to query entity for hover event {ev_type:?}: {entity:?}");
        }
    };

    if let PointerTarget::Some {
        container,
        mesh_name,
    } = &*prior_target
    {
        send_event(container, mesh_name, PointerEventType::PetHoverLeave);
    }

    if let PointerTarget::Some {
        container,
        mesh_name,
    } = &*new_target
    {
        send_event(container, mesh_name, PointerEventType::PetHoverEnter);
    }

    *prior_target = new_target.clone();
}

fn send_action_events(
    player: Query<&GlobalTransform, With<PrimaryUser>>,
    target: Res<PointerTarget>,
    pointer_requests: Query<(&SceneEntity, Option<&PointerEvents>)>,
    mut scenes: Query<(&mut RendererSceneContext, &mut SceneColliderData)>,
    input_mgr: InputManager,
    frame: Res<FrameCount>,
) {
    let Ok(player_position) = player.get_single() else {
        // can't do much without a camera
        return
    };

    let player_translation = player_position.translation();

    let mut send_event = |entity: &Entity,
                          mesh_name: &Option<String>,
                          ev_type: PointerEventType,
                          action: InputAction| {
        if let Ok((scene_entity, maybe_pe)) = pointer_requests.get(*entity) {
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
                    let Ok((mut context, mut collider_data)) = scenes.get_mut(scene_entity.root) else { panic!() };
                    // get distance
                    let nearest_point = collider_data
                        .closest_point(context.last_update_frame, player_translation, |cid| {
                            cid.entity == scene_entity.id && &cid.name == mesh_name
                        })
                        .unwrap_or(player_translation);
                    let distance = (nearest_point - player_translation).length();

                    for ev in potential_entries {
                        let max_distance = ev
                            .event_info
                            .as_ref()
                            .and_then(|info| info.max_distance)
                            .unwrap_or(10.0);
                        if distance <= max_distance {
                            let tick_number = context.tick_number;
                            context.update_crdt(
                                SceneComponentId::POINTER_RESULT,
                                CrdtType::GO_ENT,
                                scene_entity.id,
                                &PbPointerEventsResult {
                                    button: action as i32,
                                    hit: Some(RaycastHit {
                                        position: None,
                                        global_origin: None,
                                        direction: None,
                                        normal_hit: None,
                                        length: distance,
                                        mesh_name: mesh_name.clone(),
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
            warn!("failed to query entity for button event [{action:?} {ev_type:?}]: {entity:?}");
        }
    };

    // send event to hover target
    if let PointerTarget::Some {
        container,
        mesh_name,
    } = &*target
    {
        for down in input_mgr.iter_just_down() {
            send_event(container, mesh_name, PointerEventType::PetDown, *down);
        }

        for up in input_mgr.iter_just_up() {
            send_event(container, mesh_name, PointerEventType::PetUp, *up);
        }
    }

    // send events to scene roots
    for (mut context, _) in scenes.iter_mut() {
        let tick_number = context.tick_number;
        for down in input_mgr.iter_just_down() {
            context.update_crdt(
                SceneComponentId::POINTER_RESULT,
                CrdtType::GO_ENT,
                SceneEntityId::ROOT,
                &PbPointerEventsResult {
                    button: *down as i32,
                    hit: Some(RaycastHit {
                        position: None,
                        global_origin: None,
                        direction: None,
                        normal_hit: None,
                        length: 0.0,
                        mesh_name: None,
                        entity_id: SceneEntityId::ROOT.as_proto_u32(),
                    }),
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
                    hit: Some(RaycastHit {
                        position: None,
                        global_origin: None,
                        direction: None,
                        normal_hit: None,
                        length: 0.0,
                        mesh_name: None,
                        entity_id: SceneEntityId::ROOT.as_proto_u32(),
                    }),
                    state: PointerEventType::PetUp as i32,
                    timestamp: frame.0,
                    analog: None,
                    tick_number,
                },
            );
        }
    }
}
