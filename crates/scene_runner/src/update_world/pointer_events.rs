use bevy::{
    app::{HierarchyPropagatePlugin, Propagate},
    ecs::entity::EntityHashSet,
    math::FloatOrd,
    platform::collections::{HashMap, HashSet},
    prelude::*,
    render::mesh::MeshTag,
};
use common::{
    inputs::InputMap,
    structs::{PointerTargetType, ToolTips, TooltipSource},
};
use comms::global_crdt::ForeignPlayer;
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{
        common::InputAction,
        pb_pointer_events::{Entry, Info},
        PbPointerEvents,
    },
    SceneComponentId,
};
use scene_material::{SceneMaterial, SCENE_MATERIAL_OUTLINE_GREEN_MESH_TAG};

use crate::{
    renderer_context::RendererSceneContext,
    update_scene::pointer_results::{
        event_category, resolve_action_winner, ActionCandidateMode, ActionCategory, IaToCommon,
        PointerTarget, PointerTargetInfo, ProximityCandidates,
    },
    SceneEntity,
};

use super::AddCrdtInterfaceExt;

pub struct PointerEventsPlugin;

/// Bevy entities the editor wants outlined as its active selection, driven by the `/highlight`
/// console command. Unioned into the highlight pass below independent of pointer hover/proximity,
/// so the selection outline persists without writing to the scene's `PointerEvents` (and so never
/// enters the scene snapshot or the save).
#[derive(Resource, Default)]
pub struct EditorHighlight(pub EntityHashSet);

impl Plugin for PointerEventsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(HierarchyPropagatePlugin::<SelectionOutline>::default());

        app.init_resource::<Highlit>();
        app.init_resource::<EditorHighlight>();

        app.add_crdt_lww_component::<PbPointerEvents, PointerEvents>(
            SceneComponentId::POINTER_EVENTS,
            ComponentPosition::EntityOnly,
        );

        app.add_systems(
            Update,
            (hover_text, propagate_avatar_events, entity_highlighting),
        );
        app.add_observer(selection_outline_on_add);
        app.add_observer(selection_outline_on_remove);
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

#[derive(Default, Clone, Copy, PartialEq, Eq, Component)]
struct SelectionOutline;

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

    // Collect every `(action-category, button)` bucket present across the
    // candidate set and pre-resolve the priority winner for each. Tier-2
    // tooltips only surface on the winning entity.
    let mut buckets: HashSet<(ActionCategory, InputAction)> = HashSet::new();
    let mut collect_buckets = |entity: Entity| {
        if let Ok((_, _, pe)) = pointer_events.get(entity) {
            for entry in pe.iter() {
                let Some(category) = event_category(entry.event_type) else {
                    continue;
                };
                let button = entry
                    .event_info
                    .as_ref()
                    .and_then(|i| i.button.map(|_| i.button()))
                    .unwrap_or(InputAction::IaAny);
                buckets.insert((category, button));
            }
        }
    };
    if let Some(info) = hover_target.0.as_ref() {
        collect_buckets(info.container);
    }
    for cand in &proximity.0 {
        collect_buckets(cand.entity);
    }

    let mut winners: HashMap<(ActionCategory, InputAction), Option<Entity>> = HashMap::new();
    for &(category, button) in &buckets {
        let winner = resolve_action_winner(
            hover_target.0.as_ref(),
            &proximity,
            &pointer_events,
            category,
            button,
        )
        .map(|(info, _)| info.container);
        winners.insert((category, button), winner);
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
            if let Some(category) = event_category(entry.event_type) {
                let button = event_info
                    .button
                    .map(|_| event_info.button())
                    .unwrap_or(InputAction::IaAny);
                let winner = winners.get(&(category, button)).copied().flatten();
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

#[derive(Default, Resource, Deref, DerefMut)]
struct Highlit(EntityHashSet);

fn entity_highlighting(
    mut commands: Commands,
    pointer_events: Query<&PointerEvents, Without<ForeignPlayer>>,
    hover_target: Res<PointerTarget>,
    proximity: Res<ProximityCandidates>,
    mut highlit: ResMut<Highlit>,
    editor: Res<EditorHighlight>,
) {
    let mut highlight_pass = EntityHashSet::new();

    let mut test_and_insert = |entity: Entity| {
        let Ok(pointer_events) = pointer_events.get(entity) else {
            return;
        };
        if pointer_events.iter().any(|entry| {
            entry.event_info.as_ref().is_some_and(|info| {
                info.show_feedback != Some(false) && info.show_highlight != Some(false)
            })
        }) {
            highlight_pass.insert(entity);
        }
    };

    if let Some(ref hover_target) = hover_target.0 {
        test_and_insert(hover_target.container);
    }
    for candidate in &proximity.0 {
        test_and_insert(candidate.entity);
    }
    // editor selection — always outlined, no PointerEvents / hover required.
    highlight_pass.extend(editor.0.iter().copied());

    let new_highlights = highlight_pass.difference(&highlit);
    for entity in new_highlights {
        debug!("Highlighting {}", entity);
        commands
            .entity(*entity)
            .try_insert(Propagate(SelectionOutline));
    }

    let expired_highlights = highlit.difference(&highlight_pass);
    for entity in expired_highlights {
        debug!("Highlight of {} expired.", entity);
        commands
            .entity(*entity)
            .try_remove::<Propagate<SelectionOutline>>();
    }

    **highlit = highlight_pass;
}

fn selection_outline_on_add(
    trigger: Trigger<OnAdd, SelectionOutline>,
    mut meshes: Query<(&mut Mesh3d, &mut MeshTag), With<MeshMaterial3d<SceneMaterial>>>,
) {
    let entity = trigger.target();
    let Ok((mut mesh_3d, mut mesh_tag)) = meshes.get_mut(entity) else {
        return;
    };
    debug!("Selection outline on {entity}.");
    mesh_3d.set_changed();
    mesh_tag.0 |= SCENE_MATERIAL_OUTLINE_GREEN_MESH_TAG;
}

fn selection_outline_on_remove(
    trigger: Trigger<OnRemove, SelectionOutline>,
    mut meshes: Query<(&mut Mesh3d, &mut MeshTag), With<MeshMaterial3d<SceneMaterial>>>,
) {
    let entity = trigger.target();
    let Ok((mut mesh_3d, mut mesh_tag)) = meshes.get_mut(entity) else {
        return;
    };
    debug!("Selection outline removed from {entity}.");
    mesh_3d.set_changed();
    mesh_tag.0 &= !SCENE_MATERIAL_OUTLINE_GREEN_MESH_TAG;
}
