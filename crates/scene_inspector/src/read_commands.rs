use base64::{prelude::BASE64_STANDARD, Engine};
use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use bevy_console::ConsoleCommand;
use console::{DoAddConsoleCommand, PendingConsoleResponses};
use dcl::interface::CrdtStore;
use dcl_component::{
    component_name_registry::InspectFn, ComponentNameRegistry, SceneComponentId, SceneEntityId,
};
use ipfs::IpfsResource;
use scene_runner::renderer_context::RendererSceneContext;

use crate::{
    active_scene::{ActiveInspectionScene, SceneResolver},
    snapshot::PendingSnapshotRequests,
};

pub fn add_read_commands(app: &mut App) {
    app.add_console_command::<SetSceneCommand, _>(set_scene_cmd);
    app.add_console_command::<SceneStatsCommand, _>(scene_stats_cmd);
    app.add_console_command::<SceneTargetCommand, _>(scene_target_cmd);
    app.add_console_command::<SceneLogsCommand, _>(scene_logs_cmd);
    app.add_console_command::<SceneEntitiesCommand, _>(scene_entities_cmd);
    app.add_console_command::<EntityComponentsCommand, _>(entity_components_cmd);
    app.add_console_command::<InspectComponentCommand, _>(inspect_component_cmd);
    app.add_console_command::<SceneTreeCommand, _>(scene_tree_cmd);
    app.add_console_command::<CrdtSnapshotCommand, _>(crdt_snapshot_cmd);
    app.add_console_command::<CrdtInitialCommand, _>(crdt_initial_cmd);
    app.add_console_command::<ComponentNamesCommand, _>(component_names_cmd);
    app.add_console_command::<ComponentDefaultCommand, _>(component_default_cmd);
    app.add_console_command::<ComponentSchemaCommand, _>(component_schema_cmd);
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

// --- /scene_target ---

/// Return the active scene's identity/location JSON: `{ hash, root, projectId, parcels, title }`.
/// `root` is the absolute local project folder for a `dcl start` scene (else null), recovered from
/// the scene's content hashes via `IpfsIo::local_project_root` — the robust key-anchored decode the
/// editor can't do scene-side (it can't see the content hashes, only their keys). Static identity, so
/// unlike `/scene_stats` (live runtime status) it's async and read once per scene rather than polled.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/scene_target")]
struct SceneTargetCommand;

fn scene_target_cmd(
    mut input: ConsoleCommand<SceneTargetCommand>,
    ipfs: Res<IpfsResource>,
    resolver: SceneResolver,
    mut console_responses: ResMut<PendingConsoleResponses>,
) {
    if let Some(Ok(_)) = input.take() {
        let scene_hash = match resolver.resolve() {
            Ok((_, ctx)) => ctx.hash.clone(),
            Err(e) => {
                input.reply_failed(e);
                return;
            }
        };
        let io = ipfs.inner.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        IoTaskPool::get()
            .spawn(async move {
                let _ = tx.send(Ok(crate::asset_commands::scene_target_json(
                    &io,
                    &scene_hash,
                )
                .await));
            })
            .detach();
        console_responses.push_oneshot(rx, |r| r, input.take_responder());
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
                    if filter_id.is_none_or(|fid| fid == *cid) {
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
                    if filter_id.is_none_or(|fid| fid == *cid) {
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
                let mut lines: Vec<String> = entities.iter().map(entity_alias).collect();
                lines.sort();
                Ok(lines.join("\n"))
            };
            let _ = tx.send(result);
        }) {
            Ok(()) => console_responses.push_oneshot(rx, |r| r, input.take_responder()),
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
            Ok(()) => console_responses.push_oneshot(rx, |r| r, input.take_responder()),
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
            Ok(()) => console_responses.push_oneshot(rx, |r| r, input.take_responder()),
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
                    if filter_id.is_none_or(|fid| fid == *cid) {
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
                    if filter_id.is_none_or(|fid| fid == *cid) {
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
            Ok(()) => console_responses.push_oneshot(rx, |r| r, input.take_responder()),
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

// --- snapshot serialization (shared by /crdt_snapshot and /crdt_initial) ---

/// The (id, name, inspect) tuples for every registry component, used to render recognized
/// components as JSON in a snapshot.
fn snapshot_entries(
    registry: &ComponentNameRegistry,
) -> Vec<(SceneComponentId, String, InspectFn)> {
    registry
        .all_id_name_pairs()
        .map(|(id, name)| {
            (
                id,
                name.to_owned(),
                registry.get_by_id(id).unwrap().inspect.clone(),
            )
        })
        .collect()
}

/// Serialize a CRDT store into the inspector snapshot JSON shape:
/// `{ "<entityId>": { "<ComponentName>": <json>, "<numeric-id>": "<ts>:<base64>", ... }, ... }`.
/// Recognized components are emitted as JSON via their `inspect` fn; custom (unrecognized)
/// components as raw `"<lww-timestamp>:<base64>"` keyed by numeric id (grow-only as an array of
/// those). The timestamp lets the editor write back via /set_component_raw with a newer one and
/// win LWW; component names are never all-digits, so the editor can tell the two apart.
fn build_snapshot_json(
    crdt: &CrdtStore,
    entries: &[(SceneComponentId, String, InspectFn)],
) -> String {
    let mut entity_map: std::collections::BTreeMap<
        u32,
        serde_json::Map<String, serde_json::Value>,
    > = std::collections::BTreeMap::new();

    for (cid, name, inspect) in entries {
        if let Some(lww) = crdt.lww.get(cid) {
            for (eid, entry) in &lww.last_write {
                if !entry.is_some {
                    continue;
                }
                let entity_id = eid.as_proto_u32().unwrap_or(eid.id as u32);
                if let Ok(json_str) = inspect(&entry.data) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                        entity_map
                            .entry(entity_id)
                            .or_default()
                            .insert(name.clone(), val);
                    }
                }
            }
        }
    }

    let known: std::collections::HashSet<SceneComponentId> =
        entries.iter().map(|(cid, _, _)| *cid).collect();
    for (cid, lww) in &crdt.lww {
        if known.contains(cid) {
            continue;
        }
        for (eid, entry) in &lww.last_write {
            if !entry.is_some {
                continue;
            }
            let entity_id = eid.as_proto_u32().unwrap_or(eid.id as u32);
            let encoded = format!(
                "{}:{}",
                entry.timestamp.0,
                BASE64_STANDARD.encode(&entry.data)
            );
            entity_map
                .entry(entity_id)
                .or_default()
                .insert(cid.0.to_string(), serde_json::Value::String(encoded));
        }
    }
    for (cid, go) in &crdt.go {
        if known.contains(cid) {
            continue;
        }
        for (eid, values) in &go.0 {
            let entity_id = eid.as_proto_u32().unwrap_or(eid.id as u32);
            let arr: Vec<serde_json::Value> = values
                .iter()
                .map(|e| {
                    serde_json::Value::String(format!("0:{}", BASE64_STANDARD.encode(&e.data)))
                })
                .collect();
            entity_map
                .entry(entity_id)
                .or_default()
                .insert(cid.0.to_string(), serde_json::Value::Array(arr));
        }
    }

    serde_json::to_string(&entity_map).unwrap_or_default()
}

// --- /crdt_snapshot ---

/// Return the full live CRDT state as structured JSON: { entityId: { ComponentName: value, ... } }
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/crdt_snapshot")]
struct CrdtSnapshotCommand;

fn crdt_snapshot_cmd(
    mut input: ConsoleCommand<CrdtSnapshotCommand>,
    resolver: SceneResolver,
    registry: Res<ComponentNameRegistry>,
    mut pending: ResMut<PendingSnapshotRequests>,
    mut console_responses: ResMut<PendingConsoleResponses>,
) {
    if let Some(Ok(_)) = input.take() {
        let entries = snapshot_entries(&registry);
        let (tx, rx) = tokio::sync::oneshot::channel();
        match resolver.request_snapshot(&mut pending, move |crdt| {
            let _ = tx.send(Ok(build_snapshot_json(crdt, &entries)));
        }) {
            Ok(()) => console_responses.push_oneshot(rx, |r| r, input.take_responder()),
            Err(e) => input.reply_failed(e),
        }
    }
}

// --- /crdt_initial ---

/// Return the scene's authored baseline CRDT — its main.crdt as loaded, before any tick — in the
/// same JSON shape as /crdt_snapshot. The inspector diffs the live snapshot against this to tell
/// what an edit actually changed (vs runtime churn) when saving. Read synchronously from the
/// engine context; replies `{}` if the scene had no main.crdt.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/crdt_initial")]
struct CrdtInitialCommand;

fn crdt_initial_cmd(
    mut input: ConsoleCommand<CrdtInitialCommand>,
    resolver: SceneResolver,
    registry: Res<ComponentNameRegistry>,
) {
    if let Some(Ok(_)) = input.take() {
        let ctx = match resolver.resolve() {
            Ok((_, ctx)) => ctx,
            Err(e) => {
                input.reply_failed(e);
                return;
            }
        };
        match ctx.initial_crdt.as_ref() {
            Some(initial) => {
                let entries = snapshot_entries(&registry);
                input.reply_ok(build_snapshot_json(initial, &entries));
            }
            None => input.reply_ok("{}"),
        }
    }
}

// --- /component_names ---

/// List the names of all editable (writable) components, as a JSON array. Used by the
/// inspector's "add component" picker; registry-only, so it needs no scene.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/component_names")]
struct ComponentNamesCommand;

fn component_names_cmd(
    mut input: ConsoleCommand<ComponentNamesCommand>,
    registry: Res<ComponentNameRegistry>,
) {
    if let Some(Ok(_)) = input.take() {
        let mut names: Vec<&str> = registry
            .all_names()
            .filter(|name| {
                registry
                    .get_by_name(name)
                    .is_some_and(|e| e.write.is_some())
            })
            .collect();
        names.sort_unstable();
        input.reply_ok(serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_string()));
    }
}

// --- /component_default ---

/// Return a component's default value as JSON: every field present at its zero/default
/// (the serde shape is full — unset scalars are 0/""/false, optional/message/oneof fields
/// are null, repeated are []), so the editor can render all fields when adding a new one.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/component_default")]
struct ComponentDefaultCommand {
    /// Component name (PascalCase)
    component: String,
}

fn component_default_cmd(
    mut input: ConsoleCommand<ComponentDefaultCommand>,
    registry: Res<ComponentNameRegistry>,
) {
    if let Some(Ok(cmd)) = input.take() {
        let entry = match registry.get_by_name(&cmd.component) {
            Some(e) => e,
            None => {
                input.reply_failed(format!("unknown component '{}'", cmd.component));
                return;
            }
        };
        let Some(default) = &entry.default else {
            input.reply_failed(format!("'{}' has no default (read-only)", cmd.component));
            return;
        };

        match default() {
            Ok(json) => input.reply_ok(json),
            Err(e) => input.reply_failed(format!("default failed: {e}")),
        }
    }
}

// --- /component_schema ---

/// Return the structural component schema (typed fields, enum value-lists, optionality) as JSON, or
/// all components if omitted. The editor applies the curated overlay (semantics/ranges/defaults/
/// placement/requires) itself. Generated at build time and embedded; registry-free, needs no scene.
/// Transform is not included (it's not a proto message — owned by the editor scene).
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/component_schema")]
struct ComponentSchemaCommand {
    /// Component name (PascalCase); omit for the full set
    component: Option<String>,
}

fn component_schema_cmd(mut input: ConsoleCommand<ComponentSchemaCommand>) {
    if let Some(Ok(cmd)) = input.take() {
        match cmd.component {
            Some(name) => match dcl_component::component_schema::schema_for(&name) {
                Some(json) => input.reply_ok(json),
                None => input.reply_failed(format!("no schema for component '{name}'")),
            },
            None => input.reply_ok(dcl_component::component_schema::all_schemas_json().to_string()),
        }
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
