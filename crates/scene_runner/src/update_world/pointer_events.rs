use bevy::{
    math::FloatOrd,
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use common::{
    inputs::InputMap,
    structs::{PointerTargetType, ToolTips, TooltipSource},
};
use comms::global_crdt::ForeignPlayer;

use crate::{
    renderer_context::RendererSceneContext,
    update_scene::pointer_results::{
        resolve_action_winner, ActionCandidateMode, IaToCommon, PointerTarget, PointerTargetInfo,
        ProximityCandidates,
    },
    SceneEntity,
};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{
        common::{InputAction, PointerEventType},
        pb_pointer_events::{Entry, Info},
        PbPointerEvents,
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

/// Tier-2 (button-driven action) event types whose tooltips are restricted to
/// the priority winner of the corresponding `(event_type, button)` bucket.
fn action_event_type(event_type: i32) -> Option<PointerEventType> {
    match event_type {
        x if x == PointerEventType::PetDown as i32 => Some(PointerEventType::PetDown),
        x if x == PointerEventType::PetUp as i32 => Some(PointerEventType::PetUp),
        x if x == PointerEventType::PetDrag as i32 => Some(PointerEventType::PetDrag),
        x if x == PointerEventType::PetDragLocked as i32 => Some(PointerEventType::PetDragLocked),
        x if x == PointerEventType::PetDragEnd as i32 => Some(PointerEventType::PetDragEnd),
        _ => None,
    }
}

fn format_button_label(input_map: &InputMap, info: &Info) -> String {
    input_map
        .get_input(info.button().to_common())
        .map(|b| {
            let button_str = serde_json::to_string(&b).unwrap();
            let button_str = button_str.strip_prefix("\"").unwrap_or(&button_str);
            button_str
                .strip_suffix("\"")
                .unwrap_or(button_str)
                .to_owned()
        })
        .unwrap_or_else(|| {
            if info.button() == InputAction::IaAny {
                "(ANY)"
            } else {
                "(No binding)"
            }
            .to_owned()
        })
}

#[allow(clippy::too_many_arguments)]
fn hover_text(
    pointer_events: Query<(Option<&SceneEntity>, Option<&ForeignPlayer>, &PointerEvents)>,
    hover_target: Res<PointerTarget>,
    proximity: Res<ProximityCandidates>,
    input_map: Res<InputMap>,
    mut tooltip: ResMut<ToolTips>,
) {
    let mut texts = Vec::<(String, bool)>::default();

    // Collect every `(action-event-type, button)` bucket present across the
    // candidate set and pre-resolve the priority winner for each. Tier-2
    // tooltips only surface on the winning entity.
    let mut buckets: HashSet<(PointerEventType, InputAction)> = HashSet::new();
    let mut collect_buckets = |entity: Entity| {
        if let Ok((_, _, pe)) = pointer_events.get(entity) {
            for entry in pe.iter() {
                let Some(et) = action_event_type(entry.event_type) else {
                    continue;
                };
                let button = entry
                    .event_info
                    .as_ref()
                    .and_then(|i| i.button.map(|_| i.button()))
                    .unwrap_or(InputAction::IaAny);
                buckets.insert((et, button));
            }
        }
    };
    if let Some(info) = hover_target.0.as_ref() {
        collect_buckets(info.container);
    }
    for cand in &proximity.0 {
        collect_buckets(cand.entity);
    }

    let mut winners: HashMap<(i32, i32), Option<Entity>> = HashMap::new();
    for &(et, button) in &buckets {
        let winner = resolve_action_winner(
            hover_target.0.as_ref(),
            &proximity,
            &pointer_events,
            et,
            button,
        )
        .map(|(info, _)| info.container);
        winners.insert((et as i32, button as i32), winner);
    }

    let mut process = |entity: Entity, mode: ActionCandidateMode, info: &PointerTargetInfo| {
        let Ok((_, _, pe)) = pointer_events.get(entity) else {
            return;
        };
        for entry in pe.iter().filter(|e| mode.matches_entry(e)) {
            let Some(event_info) = entry.event_info.as_ref() else {
                continue;
            };
            if !event_info.show_feedback.unwrap_or(true) {
                continue;
            }
            let Some(text) = event_info.hover_text.as_ref() else {
                continue;
            };

            // Tier-2 entries only show on the winning entity for their bucket.
            if let Some(action_et) = action_event_type(entry.event_type) {
                let button = event_info
                    .button
                    .map(|_| event_info.button())
                    .unwrap_or(InputAction::IaAny);
                let winner = winners
                    .get(&(action_et as i32, button as i32))
                    .copied()
                    .flatten();
                if winner != Some(entity) {
                    continue;
                }
            }

            let button_label = format_button_label(&input_map, event_info);
            let in_range =
                mode.passes_distance(Some(event_info), info.camera_distance.0, info.distance.0);
            texts.push((format!("{button_label} : {text}"), in_range));
        }
    };

    if let Some(info) = hover_target.0.as_ref() {
        process(info.container, ActionCandidateMode::Cursor, info);
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
        process(cand.entity, ActionCandidateMode::Proximity, &synthetic);
    }

    // make unique
    texts = texts
        .into_iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    tooltip
        .0
        .insert(TooltipSource::Label("pointer_events"), texts);
}
