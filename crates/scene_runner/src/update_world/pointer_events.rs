use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use common::{
    inputs::InputMap,
    structs::{ToolTips, TooltipSource},
};
use comms::global_crdt::ForeignPlayer;

use crate::{
    renderer_context::RendererSceneContext,
    update_scene::pointer_results::{IaToCommon, PointerTarget, PointerTargetInfo},
    SceneEntity,
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

#[allow(clippy::too_many_arguments)]
fn hover_text(
    pointer_events: Query<&PointerEvents>,
    hover_target: Res<PointerTarget>,
    input_map: Res<InputMap>,
    mut tooltip: ResMut<ToolTips>,
) {
    let mut texts = Vec::default();

    if let Some(PointerTargetInfo {
        container,
        distance,
        ..
    }) = hover_target.0
    {
        if let Ok(pes) = pointer_events.get(container) {
            texts = pes
                .iter()
                .flat_map(|pe| {
                    if let Some(info) = pe.event_info.as_ref() {
                        if info.show_feedback.unwrap_or(true) {
                            if let Some(text) = info.hover_text.as_ref() {
                                let button = input_map
                                    .get_input(info.button().to_common())
                                    .map(|b| {
                                        let button_str = serde_json::to_string(&b).unwrap();
                                        let button_str =
                                            button_str.strip_prefix("\"").unwrap_or(&button_str);
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
                                    });
                                return Some((
                                    format!("{button} : {text}"),
                                    info.max_distance.unwrap_or(10.0) > distance.0,
                                ));
                            }
                        }
                    }
                    None
                })
                .collect::<Vec<_>>();
        }
    }

    tooltip
        .0
        .insert(TooltipSource::Label("pointer_events"), texts);
}
