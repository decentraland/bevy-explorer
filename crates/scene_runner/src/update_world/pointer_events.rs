use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use common::{
    inputs::{CommonInputAction, HoverActionInfo, HoverEvent, InputMap},
    rpc::RpcStreamSender,
};
use comms::global_crdt::ForeignPlayer;
use system_bridge::SystemApi;

use crate::{
    renderer_context::RendererSceneContext,
    update_scene::pointer_results::{IaToCommon, PointerTarget, PointerTargetInfo},
    SceneEntity,
};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{pb_pointer_events::Entry, PbPointerEvents},
    SceneComponentId,
};

use super::AddCrdtInterfaceExt;

pub struct PointerEventsPlugin;

impl Plugin for PointerEventsPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbPointerEvents, PointerEvents>(
            SceneComponentId::POINTER_EVENTS,
            ComponentPosition::EntityOnly,
        );

        app.add_systems(Update, (hover_text, propagate_avatar_events));
    }
}

#[derive(Component, Debug)]
pub struct PointerEvents {
    pub msg: HashMap<Option<Entity>, PbPointerEvents>,
}

impl From<PbPointerEvents> for PointerEvents {
    fn from(pb_pointer_events: PbPointerEvents) -> Self {
        Self {
            msg: HashMap::from_iter([(None, pb_pointer_events)]),
        }
    }
}

impl PointerEvents {
    pub fn iter_with_scene(
        &self,
        default_scene: Option<Entity>,
    ) -> impl Iterator<Item = (Entity, &Entry)> {
        self.msg.iter().flat_map(move |(scene, events)| {
            events.pointer_events.iter().map(move |e| {
                (
                    *scene
                        .as_ref()
                        .unwrap_or_else(|| default_scene.as_ref().unwrap()),
                    e,
                )
            })
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.msg
            .iter()
            .flat_map(|(_, events)| events.pointer_events.iter())
    }
}

pub fn propagate_avatar_events(
    mut commands: Commands,
    mut foreign_players: Query<(Entity, &ForeignPlayer, Option<&mut PointerEvents>)>,
    scenes: Query<Entity, With<RendererSceneContext>>,
    new_events: Query<
        (&SceneEntity, &PointerEvents),
        (Without<ForeignPlayer>, Changed<PointerEvents>),
    >,
) {
    // remove expired
    let live_scenes = scenes.iter().collect::<HashSet<_>>();
    let mut foreign_player_lookup = HashMap::new();
    for (entity, foreign_player, maybe_pes) in foreign_players.iter_mut() {
        if let Some(mut pes) = maybe_pes {
            pes.msg
                .retain(|scene, _| scene.is_none_or(|scene| live_scenes.contains(&scene)));
        }

        foreign_player_lookup.insert(foreign_player.scene_id, entity);
    }

    // add new/updated
    for (scene_entity, new_events) in new_events.iter() {
        if let Some((foreign_entity, _, maybe_pes)) = foreign_player_lookup
            .get(&scene_entity.id)
            .and_then(|e| foreign_players.get_mut(*e).ok())
        {
            if let Some(mut pes) = maybe_pes {
                pes.msg.insert(
                    Some(scene_entity.root),
                    new_events.msg.get(&None).unwrap().clone(),
                );
            } else {
                commands.entity(foreign_entity).try_insert(PointerEvents {
                    msg: HashMap::from_iter([(
                        Some(scene_entity.root),
                        new_events.msg.get(&None).unwrap().clone(),
                    )]),
                });
            }
        }
    }
}

#[derive(Component)]
pub struct HoverText;

#[derive(Default)]
struct HoverStreamState {
    senders: Vec<RpcStreamSender<HoverEvent>>,
    previous_target: Option<(Entity, Option<String>)>,
}

#[allow(clippy::too_many_arguments)]
fn hover_text(
    pointer_events: Query<&PointerEvents>,
    hover_target: Res<PointerTarget>,
    input_map: Res<InputMap>,
    mut events: EventReader<SystemApi>,
    mut state: Local<HoverStreamState>,
) {
    // Collect new stream senders
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

    state.senders.extend(new_senders);
    state.senders.retain(|s| !s.is_closed());

    // Get current hover target info
    let current_target = hover_target
        .0
        .as_ref()
        .map(|info| (info.container, info.mesh_name.clone()));

    // Determine if hover target changed
    let target_changed = match (&state.previous_target, &current_target) {
        (None, None) => false,
        (Some(_), None) | (None, Some(_)) => true,
        (Some((prev_e, prev_m)), Some((cur_e, cur_m))) => prev_e != cur_e || prev_m != cur_m,
    };

    if !target_changed {
        return;
    }

    // Send leave event for previous target
    if let Some((prev_entity, prev_mesh)) = state.previous_target.take() {
        let actions = collect_actions(&pointer_events, prev_entity, 0.0, &input_map);
        let leave_event = HoverEvent {
            entered: false,
            mesh_name: prev_mesh,
            distance: 0.0,
            actions,
        };
        for sender in &state.senders {
            let _ = sender.send(leave_event.clone());
        }
    }

    // Send enter event for new target
    if let Some(PointerTargetInfo {
        container,
        mesh_name,
        distance,
        ..
    }) = hover_target.0.as_ref()
    {
        let actions = collect_actions(&pointer_events, *container, distance.0, &input_map);
        let enter_event = HoverEvent {
            entered: true,
            mesh_name: mesh_name.clone(),
            distance: distance.0,
            actions,
        };
        for sender in &state.senders {
            let _ = sender.send(enter_event.clone());
        }
        state.previous_target = Some((*container, mesh_name.clone()));
    }
}

fn collect_actions(
    pointer_events: &Query<&PointerEvents>,
    container: Entity,
    distance: f32,
    input_map: &InputMap,
) -> Vec<HoverActionInfo> {
    let Ok(pes) = pointer_events.get(container) else {
        return Vec::new();
    };

    pes.iter()
        .filter_map(|pe| {
            let info = pe.event_info.as_ref()?;
            let max_distance = info.max_distance.unwrap_or(10.0);
            if distance > max_distance {
                return None;
            }

            let action: CommonInputAction = info.button().to_common();
            let input_binding = input_map.get_input(action).map(|b| {
                let button_str = serde_json::to_string(&b).unwrap();
                let button_str = button_str.strip_prefix("\"").unwrap_or(&button_str);
                button_str
                    .strip_suffix("\"")
                    .unwrap_or(button_str)
                    .to_owned()
            });

            Some(HoverActionInfo {
                action,
                input_binding,
                hover_text: info.hover_text.clone(),
            })
        })
        .collect()
}
