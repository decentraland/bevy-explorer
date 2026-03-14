use bevy::prelude::*;
use bevy_console::ConsoleCommand;
use console::{DoAddConsoleCommand, PendingConsoleResponses};
use dcl_component::{ComponentNameRegistry, SceneComponentId, SceneEntityId};
use scene_runner::renderer_context::RendererSceneContext;

use crate::{
    active_scene::{ActiveInspectionScene, SceneResolver},
    snapshot::PendingSnapshotRequests,
};

pub fn add_read_commands(app: &mut App) {
    app.add_console_command::<SetSceneCommand, _>(set_scene_cmd);
    app.add_console_command::<SceneStatsCommand, _>(scene_stats_cmd);
    // TODO: /scene_logs always returns empty even when logs exist — investigate
    // app.add_console_command::<SceneLogsCommand, _>(scene_logs_cmd);
    app.add_console_command::<SceneEntitiesCommand, _>(scene_entities_cmd);
    app.add_console_command::<EntityComponentsCommand, _>(entity_components_cmd);
    app.add_console_command::<InspectComponentCommand, _>(inspect_component_cmd);
    app.add_console_command::<SceneTreeCommand, _>(scene_tree_cmd);
}

// --- /set_scene ---

/// Set the active inspection scene by title prefix or hash. Omit to reset to player's current scene.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/set_scene")]
struct SetSceneCommand {
    /// Scene title prefix or hash (omit to follow the player)
    pattern: Option<String>,
}

fn set_scene_cmd(
    mut input: ConsoleCommand<SetSceneCommand>,
    mut active: ResMut<ActiveInspectionScene>,
    scenes: Query<(Entity, &RendererSceneContext)>,
) {
    if let Some(Ok(cmd)) = input.take() {
        let Some(pattern) = cmd.pattern else {
            active.0 = None;
            input.reply_ok("active scene reset to player location");
            return;
        };

        let pattern_lower = pattern.to_lowercase();
        let matches: Vec<_> = scenes
            .iter()
            .filter(|(_, ctx)| {
                ctx.hash == pattern || ctx.title.to_lowercase().contains(&pattern_lower)
            })
            .collect();

        match matches.len() {
            0 => input.reply_failed(format!("no scene matching '{pattern}'")),
            1 => {
                let (ent, ctx) = matches[0];
                let title = ctx.title.clone();
                active.0 = Some(ent);
                input.reply_ok(format!("active scene set to '{title}'"));
            }
            _ => {
                let names: Vec<_> = matches.iter().map(|(_, c)| c.title.as_str()).collect();
                input.reply_failed(format!("ambiguous: matches {}", names.join(", ")));
            }
        }
    }
}

// --- /scene_stats ---

/// Show stats for the active scene
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/scene_stats")]
struct SceneStatsCommand;

fn scene_stats_cmd(mut input: ConsoleCommand<SceneStatsCommand>, resolver: SceneResolver) {
    if let Some(Ok(_)) = input.take() {
        match resolver.resolve() {
            Err(e) => input.reply_failed(e),
            Ok((_, ctx)) => {
                let entity_count = ctx.live_scene_entities().count();
                let blocked = if ctx.blocked.is_empty() {
                    "running".to_string()
                } else {
                    format!("blocked({:?})", ctx.blocked)
                };
                input.reply_ok(format!(
                    "scene: '{}'\nhash: {}\ntick: {}\nentities: {}\nstatus: {}\nbroken: {}\nin_flight: {}",
                    ctx.title, ctx.hash, ctx.tick_number, entity_count, blocked, ctx.broken, ctx.in_flight,
                ));
            }
        }
    }
}

// --- /scene_logs ---

/// Print recent log entries from the active scene
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/scene_logs")]
struct SceneLogsCommand {
    /// Number of entries to show (default 20)
    #[arg(default_value = "20")]
    count: usize,
}

fn scene_logs_cmd(mut input: ConsoleCommand<SceneLogsCommand>, resolver: SceneResolver) {
    if let Some(Ok(cmd)) = input.take() {
        match resolver.resolve() {
            Err(e) => input.reply_failed(e),
            Ok((_, ctx)) => {
                let (missed, entries, _) = ctx.logs.read();
                let entries: Vec<_> = entries.into_iter().rev().take(cmd.count).rev().collect();
                if entries.is_empty() {
                    input.reply_ok("(no logs)");
                    return;
                }
                let mut out = String::new();
                if missed > 0 {
                    out.push_str(&format!("(+{missed} older entries not retained)\n"));
                }
                for entry in &entries {
                    out.push_str(&format!(
                        "[{:.2}] {:?}: {}\n",
                        entry.timestamp, entry.level, entry.message
                    ));
                }
                input.reply_ok(out.trim_end());
            }
        }
    }
}

// --- /scene_entities ---

/// List live entities in the active scene, optionally filtered by component name
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/scene_entities")]
struct SceneEntitiesCommand {
    /// Filter to entities that have this component (PascalCase)
    component: Option<String>,
}

fn scene_entities_cmd(
    mut input: ConsoleCommand<SceneEntitiesCommand>,
    resolver: SceneResolver,
    registry: Res<ComponentNameRegistry>,
    mut pending: ResMut<PendingSnapshotRequests>,
    mut console_responses: ResMut<PendingConsoleResponses>,
) {
    if let Some(Ok(cmd)) = input.take() {
        let filter_id: Option<SceneComponentId> = match &cmd.component {
            None => None,
            Some(name) => match registry.get_by_name(name) {
                Some(entry) => Some(entry.id),
                None => {
                    input.reply_failed(format!("unknown component '{name}'"));
                    return;
                }
            },
        };

        let (tx, rx) = tokio::sync::oneshot::channel();
        match resolver.request_snapshot(&mut pending, move |crdt| {
            let entities: Vec<SceneEntityId> = crdt
                .lww
                .iter()
                .flat_map(|(cid, state)| {
                    if filter_id.map_or(true, |fid| fid == *cid) {
                        state
                            .last_write
                            .iter()
                            .filter(|(_, e)| e.is_some)
                            .map(|(eid, _)| *eid)
                            .collect::<Vec<_>>()
                    } else {
                        vec![]
                    }
                })
                .chain(crdt.go.iter().flat_map(|(cid, state)| {
                    if filter_id.map_or(true, |fid| fid == *cid) {
                        state.0.keys().copied().collect::<Vec<_>>()
                    } else {
                        vec![]
                    }
                }))
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            let result = if entities.is_empty() {
                Ok("(no entities)".to_string())
            } else {
                let mut lines: Vec<String> = entities.iter().map(|e| entity_alias(e)).collect();
                lines.sort();
                Ok(lines.join("\n"))
            };
            let _ = tx.send(result);
        }) {
            Ok(()) => console_responses.push_oneshot(rx, |r| r),
            Err(e) => input.reply_failed(e),
        }
    }
}

// --- /entity_components ---

/// List all components attached to a scene entity
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/entity_components")]
struct EntityComponentsCommand {
    /// Entity id (u16) or alias: root, player, camera
    entity: String,
}

fn entity_components_cmd(
    mut input: ConsoleCommand<EntityComponentsCommand>,
    resolver: SceneResolver,
    registry: Res<ComponentNameRegistry>,
    mut pending: ResMut<PendingSnapshotRequests>,
    mut console_responses: ResMut<PendingConsoleResponses>,
) {
    if let Some(Ok(cmd)) = input.take() {
        let eid = match parse_entity_id(&cmd.entity) {
            Ok(e) => e,
            Err(e) => {
                input.reply_failed(e);
                return;
            }
        };

        let id_to_name: std::collections::HashMap<_, _> = registry
            .all_id_name_pairs()
            .map(|(id, name)| (id, name.to_string()))
            .collect();
        let entity_str = cmd.entity.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        match resolver.request_snapshot(&mut pending, move |crdt| {
            let mut components: Vec<String> = Vec::new();

            for (cid, state) in &crdt.lww {
                if state.last_write.get(&eid).is_some_and(|e| e.is_some) {
                    let name = id_to_name.get(cid).map(|s| s.as_str()).unwrap_or("unknown");
                    components.push(format!("{name} ({}) [LWW]", cid.0));
                }
            }
            for (cid, state) in &crdt.go {
                if state.0.contains_key(&eid) {
                    let name = id_to_name.get(cid).map(|s| s.as_str()).unwrap_or("unknown");
                    components.push(format!("{name} ({}) [GO]", cid.0));
                }
            }

            let result = if components.is_empty() {
                Ok(format!("entity {entity_str} has no components"))
            } else {
                components.sort();
                Ok(components.join("\n"))
            };
            let _ = tx.send(result);
        }) {
            Ok(()) => console_responses.push_oneshot(rx, |r| r),
            Err(e) => input.reply_failed(e),
        }
    }
}

// --- /inspect_component ---

/// Pretty-print a component's current value as JSON
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/inspect_component")]
struct InspectComponentCommand {
    /// Entity id or alias
    entity: String,
    /// Component name (PascalCase)
    component: String,
}

fn inspect_component_cmd(
    mut input: ConsoleCommand<InspectComponentCommand>,
    resolver: SceneResolver,
    registry: Res<ComponentNameRegistry>,
    mut pending: ResMut<PendingSnapshotRequests>,
    mut console_responses: ResMut<PendingConsoleResponses>,
) {
    if let Some(Ok(cmd)) = input.take() {
        let eid = match parse_entity_id(&cmd.entity) {
            Ok(e) => e,
            Err(e) => {
                input.reply_failed(e);
                return;
            }
        };

        let entry = match registry.get_by_name(&cmd.component) {
            Some(e) => e,
            None => {
                input.reply_failed(format!("unknown component '{}'", cmd.component));
                return;
            }
        };

        let component_id = entry.id;
        let inspect = entry.inspect.clone();
        let component_str = cmd.component.clone();
        let entity_str = cmd.entity.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        match resolver.request_snapshot(&mut pending, move |crdt| {
            // LWW
            if let Some(state) = crdt.lww.get(&component_id) {
                if let Some(lww_entry) = state.last_write.get(&eid) {
                    if lww_entry.is_some {
                        let result =
                            inspect(&lww_entry.data).map_err(|e| format!("deserialize error: {e}"));
                        let _ = tx.send(result);
                        return;
                    }
                }
            }
            // GO
            if let Some(state) = crdt.go.get(&component_id) {
                if let Some(entries) = state.0.get(&eid) {
                    let jsons: Vec<String> = entries
                        .iter()
                        .filter_map(|e| inspect(&e.data).ok())
                        .collect();
                    let _ = tx.send(Ok(format!("[{}]", jsons.join(","))));
                    return;
                }
            }
            let _ = tx.send(Err(format!(
                "entity {entity_str} has no component '{component_str}'"
            )));
        }) {
            Ok(()) => console_responses.push_oneshot(rx, |r| r),
            Err(e) => input.reply_failed(e),
        }
    }
}

// --- /scene_tree ---

/// Print the parent-child hierarchy of the active scene
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/scene_tree")]
struct SceneTreeCommand {
    /// Only show entities that have this component (plus their ancestors)
    component: Option<String>,
}

fn scene_tree_cmd(
    mut input: ConsoleCommand<SceneTreeCommand>,
    resolver: SceneResolver,
    registry: Res<ComponentNameRegistry>,
    mut pending: ResMut<PendingSnapshotRequests>,
    mut console_responses: ResMut<PendingConsoleResponses>,
) {
    if let Some(Ok(cmd)) = input.take() {
        let filter_id: Option<SceneComponentId> = match &cmd.component {
            None => None,
            Some(name) => match registry.get_by_name(name) {
                Some(entry) => Some(entry.id),
                None => {
                    input.reply_failed(format!("unknown component '{name}'"));
                    return;
                }
            },
        };

        let (tx, rx) = tokio::sync::oneshot::channel();
        match resolver.request_snapshot(&mut pending, move |crdt| {
            let transform_id = SceneComponentId::TRANSFORM;

            // Build parent map: child id → parent id (skip root→root)
            let mut parent_map: std::collections::HashMap<u16, u16> =
                std::collections::HashMap::new();
            if let Some(state) = crdt.lww.get(&transform_id) {
                for (eid, lww_entry) in &state.last_write {
                    use dcl_component::transform_and_parent::DclTransformAndParent;
                    use dcl_component::{DclReader, FromDclReader};
                    if let Ok(t) =
                        DclTransformAndParent::from_reader(&mut DclReader::new(&lww_entry.data))
                    {
                        let parent_id = t.parent().id;
                        if eid.id != parent_id {
                            parent_map.insert(eid.id, parent_id);
                        }
                    }
                }
            }

            // Collect entities matching the filter
            let entities: std::collections::HashSet<u16> = crdt
                .lww
                .iter()
                .flat_map(|(cid, state)| {
                    if filter_id.map_or(true, |fid| fid == *cid) {
                        state
                            .last_write
                            .iter()
                            .filter(|(_, e)| e.is_some)
                            .map(|(eid, _)| eid.id)
                            .collect::<Vec<_>>()
                    } else {
                        vec![]
                    }
                })
                .chain(crdt.go.iter().flat_map(|(cid, state)| {
                    if filter_id.map_or(true, |fid| fid == *cid) {
                        state.0.keys().map(|e| e.id).collect::<Vec<_>>()
                    } else {
                        vec![]
                    }
                }))
                .collect();

            // Always walk ancestors so non-instantiated parents (e.g. player) are included.
            let mut visible: std::collections::HashSet<u16> = entities.clone();
            for &eid in &entities {
                let mut cur = eid;
                while let Some(&parent) = parent_map.get(&cur) {
                    if !visible.insert(parent) {
                        break;
                    }
                    cur = parent;
                }
            }

            // Build children map (root is always the tree root).
            let mut children: std::collections::HashMap<u16, Vec<u16>> =
                std::collections::HashMap::new();
            for &eid in &visible {
                if eid == SceneEntityId::ROOT.id {
                    continue;
                }
                let parent = parent_map
                    .get(&eid)
                    .copied()
                    .unwrap_or(SceneEntityId::ROOT.id);
                children.entry(parent).or_default().push(eid);
            }
            for v in children.values_mut() {
                v.sort();
            }

            if visible.is_empty() && !children.contains_key(&SceneEntityId::ROOT.id) {
                let _ = tx.send(Ok("(no entities)".to_string()));
                return;
            }
            let matched = if filter_id.is_some() {
                &entities
            } else {
                &std::collections::HashSet::new()
            };
            let mut out = String::new();
            print_tree(
                &mut out,
                SceneEntityId::ROOT.id,
                0,
                &children,
                &mut visible,
                matched,
            );
            let result = if out.is_empty() {
                Ok("(no entities)".to_string())
            } else {
                Ok(out.trim_end().to_string())
            };
            let _ = tx.send(result);
        }) {
            Ok(()) => console_responses.push_oneshot(rx, |r| r),
            Err(e) => input.reply_failed(e),
        }
    }
}

fn print_tree(
    out: &mut String,
    id: u16,
    depth: usize,
    children: &std::collections::HashMap<u16, Vec<u16>>,
    visible: &mut std::collections::HashSet<u16>,
    matched: &std::collections::HashSet<u16>,
) {
    // Remove from visible before recursing to break any cycles
    let was_visible = visible.remove(&id) || id == SceneEntityId::ROOT.id;
    if was_visible {
        let indent = "  ".repeat(depth);
        let eid = SceneEntityId { id, generation: 0 };
        let marker = if matched.contains(&id) { " *" } else { "" };
        out.push_str(&format!("{indent}{}{marker}\n", entity_alias(&eid)));
    }
    if let Some(kids) = children.get(&id).cloned() {
        for kid in kids {
            print_tree(out, kid, depth + 1, children, visible, matched);
        }
    }
}

// --- helpers ---

fn entity_alias(eid: &SceneEntityId) -> String {
    let id = eid.as_proto_u32().unwrap_or(eid.id as u32);
    match eid.id {
        0 => format!("root({id})"),
        1 => format!("player({id})"),
        2 => format!("camera({id})"),
        _ => format!("{id}"),
    }
}

pub fn parse_entity_id(s: &str) -> Result<SceneEntityId, String> {
    match s {
        "root" => Ok(SceneEntityId::ROOT),
        "player" => Ok(SceneEntityId::PLAYER),
        "camera" => Ok(SceneEntityId::CAMERA),
        other => {
            let id: u32 = other
                .parse()
                .map_err(|_| format!("expected entity id (u32 or alias), got '{other}'"))?;
            Ok(SceneEntityId::from_proto_u32(id))
        }
    }
}
