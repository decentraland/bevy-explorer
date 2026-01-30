use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use common::{
    inputs::InputMap,
    structs::{HoverAction, HoverEventInfo, HoverInfo, HoverTargetType, PrimaryUser, ToolTips, TooltipSource},
};
use comms::global_crdt::ForeignPlayer;

use crate::{
    renderer_context::RendererSceneContext,
    update_scene::pointer_results::{IaToCommon, PointerTarget, PointerTargetInfo, PointerTargetType},
    ContainerEntity, ContainingScene, SceneEntity,
};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{
        common::InputAction, pb_pointer_events::Entry, PbPointerEvents,
    },
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

        app.init_resource::<HoverInfo>();
        app.add_systems(Update, (generate_hover_info, propagate_avatar_events));
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

fn input_binding_string(input_map: &InputMap, action: InputAction) -> String {
    input_map
        .get_input(action.to_common())
        .map(|b| {
            let button_str = serde_json::to_string(&b).unwrap();
            let button_str = button_str.strip_prefix("\"").unwrap_or(&button_str);
            button_str.strip_suffix("\"").unwrap_or(button_str).to_owned()
        })
        .unwrap_or_else(|| {
            if action == InputAction::IaAny {
                "(ANY)"
            } else {
                "(No binding)"
            }
            .to_owned()
        })
}

#[allow(clippy::too_many_arguments)]
fn generate_hover_info(
    pointer_events: Query<&PointerEvents>,
    hover_target: Res<PointerTarget>,
    input_map: Res<InputMap>,
    mut hover_info: ResMut<HoverInfo>,
    mut tooltip: ResMut<ToolTips>,
    container_entities: Query<&ContainerEntity>,
    containing_scenes: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
    mut prev_scene_root: Local<Option<Entity>>,
) {
    // Reset hover info
    hover_info.target_type = None;
    hover_info.distance = 0.0;
    hover_info.actions.clear();
    hover_info.outside_scene = false;

    let mut texts = Vec::default();

    if let Some(PointerTargetInfo {
        container,
        distance,
        ty,
        ..
    }) = hover_target.0.clone()
    {
        // Set target type
        hover_info.target_type = Some(match ty {
            PointerTargetType::World => HoverTargetType::World,
            PointerTargetType::Ui => HoverTargetType::Ui,
            PointerTargetType::Avatar => HoverTargetType::Avatar,
        });
        hover_info.distance = distance.0;

        // Determine if player is outside the scene containing the entity
        // For avatars, they're global so outside_scene is always false
        if ty != PointerTargetType::Avatar {
            if let (Ok(player_entity), Ok(container_entity)) =
                (player.single(), container_entities.get(container))
            {
                let player_scenes = containing_scenes.get(player_entity);
                hover_info.outside_scene = !player_scenes.contains(&container_entity.root);
                *prev_scene_root = Some(container_entity.root);
            }
        } else {
            *prev_scene_root = None;
        }

        if let Ok(pes) = pointer_events.get(container) {
            for pe in pes.iter() {
                if let Some(info) = pe.event_info.as_ref() {
                    let action = info.button();
                    let max_distance = info.max_distance.unwrap_or(10.0);
                    let show_feedback = info.show_feedback.unwrap_or(true);
                    let hover_text = info.hover_text.clone().unwrap_or_default();

                    // Add to HoverInfo
                    let too_far = distance.0 > max_distance;
                    hover_info.actions.push(HoverAction {
                        event_type: pe.event_type() as u32,
                        event_info: HoverEventInfo {
                            button: action as u32,
                            hover_text: hover_text.clone(),
                            show_feedback,
                            show_highlight: true, // not in proto, default to true
                            max_distance,
                        },
                        too_far,
                    });

                    // Add to texts for ToolTips (backward compatibility)
                    if show_feedback && !hover_text.is_empty() {
                        let binding = input_binding_string(&input_map, action);
                        let in_range = max_distance > distance.0;
                        texts.push((format!("{binding} : {hover_text}"), in_range));
                    }
                }
            }

            // make unique
            texts = texts
                .into_iter()
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
        }
    } else if let Some(scene_root) = *prev_scene_root {
        // No target, but we had a previous scene - calculate if player is outside that scene
        // This is needed for exit events (entered: false) to report the correct outside_scene value
        if let Ok(player_entity) = player.single() {
            let player_scenes = containing_scenes.get(player_entity);
            hover_info.outside_scene = !player_scenes.contains(&scene_root);
        }
    }

    tooltip
        .0
        .insert(TooltipSource::Label("pointer_events"), texts);
}
