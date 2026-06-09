use base64::{prelude::BASE64_STANDARD, Engine};
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use bevy_console::ConsoleCommand;
use console::{DoAddConsoleCommand, PendingConsoleResponses};
use dcl::interface::CrdtStore;
use dcl_component::{
    transform_and_parent::DclTransformAndParent, ComponentNameRegistry, CrdtType, DclReader,
    FromDclReader, SceneComponentId, SceneCrdtTimestamp, SceneEntityId,
};
use scene_runner::{renderer_context::FROZEN_BLOCK, update_world::CrdtExtractors};

use crate::{
    active_scene::SceneResolver, read_commands::parse_entity_id, snapshot::PendingEntityAllocations,
};

/// Accumulates immediate Bevy-side updates so a batch of component writes/removals in one
/// frame is delivered as a single `CrdtStateComponent` per component, rather than each
/// overwriting the previous on the scene root (`process_crdt_lww_updates` only sees the
/// last insert). Staged updates are flushed once, after the batch loop.
#[derive(Default)]
struct StagedBevyUpdates {
    updates: CrdtStore,
    touched: Vec<SceneComponentId>,
    scene_root: Option<Entity>,
}

impl StagedBevyUpdates {
    /// Apply one component write (`Some(data)`) or removal (`None`) to the active scene's
    /// CRDT store, and stage the matching Bevy-side update. Returns the resolve error, if
    /// any (so the caller can reply per-invocation).
    fn apply(
        &mut self,
        resolver: &mut SceneResolver,
        crdt_interfaces: &CrdtExtractors,
        component_id: SceneComponentId,
        crdt_type: CrdtType,
        eid: SceneEntityId,
        data: Option<&[u8]>,
    ) -> Result<(), String> {
        let (scene_entity, mut ctx) = resolver.resolve_mut()?;
        ctx.crdt_store.force_update(
            component_id,
            crdt_type,
            eid,
            data.map(DclReader::new).as_mut(),
        );

        // Stage for immediate application to the Bevy entity so the renderer reflects the
        // change without waiting for the scene to echo it back. Pure scene-side components
        // have no entry in CrdtExtractors and are applied when the scene echoes the write.
        if crdt_interfaces.0.contains_key(&component_id) {
            self.updates.force_update(
                component_id,
                crdt_type,
                eid,
                data.map(DclReader::new).as_mut(),
            );
            if !self.touched.contains(&component_id) {
                self.touched.push(component_id);
            }
            self.scene_root = Some(scene_entity);
        }
        Ok(())
    }

    /// Flush staged updates: one `CrdtStateComponent` per component, carrying every entity
    /// touched this run, so a batch applies in full rather than only the last invocation.
    fn flush(mut self, commands: &mut Commands, crdt_interfaces: &CrdtExtractors) {
        let Some(scene_entity) = self.scene_root else {
            return;
        };
        let mut entity_commands = commands.entity(scene_entity);
        for component_id in &self.touched {
            if let Some(interface) = crdt_interfaces.0.get(component_id) {
                interface.updates_to_entity(*component_id, &mut self.updates, &mut entity_commands);
            }
        }
    }
}

pub fn add_write_commands(app: &mut App) {
    app.add_console_command::<SetComponentCommand, _>(set_component_cmd);
    app.add_console_command::<SetComponentRawCommand, _>(set_component_raw_cmd);
    app.add_console_command::<NewEntityCommand, _>(new_entity_cmd);
    app.add_console_command::<SaveCompositeCommand, _>(save_composite_cmd);
    app.add_console_command::<DeleteComponentCommand, _>(delete_component_cmd);
    app.add_console_command::<DeleteEntityCommand, _>(delete_entity_cmd);
    app.add_console_command::<FreezeSceneCommand, _>(freeze_scene_cmd);
    app.add_console_command::<UnfreezeSceneCommand, _>(unfreeze_scene_cmd);
    app.add_console_command::<TickSceneCommand, _>(tick_scene_cmd);
}

// --- /set_component ---

/// Set a component value on a scene entity
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/set_component")]
struct SetComponentCommand {
    /// Entity id or alias: root, player, camera
    entity: String,
    /// Component name (PascalCase)
    component: String,
    /// Component value as JSON
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 1..)]
    json: Vec<String>,
}

fn set_component_cmd(
    mut input: ConsoleCommand<SetComponentCommand>,
    registry: Res<ComponentNameRegistry>,
    mut resolver: SceneResolver,
    crdt_interfaces: Res<CrdtExtractors>,
    mut commands: Commands,
) {
    let mut staged = StagedBevyUpdates::default();

    while let Some(Ok(cmd)) = input.take() {
        let eid = match parse_entity_id(&cmd.entity) {
            Ok(e) => e,
            Err(e) => {
                input.reply_failed(e);
                continue;
            }
        };

        let entry = match registry.get_by_name(&cmd.component) {
            Some(e) => e,
            None => {
                input.reply_failed(format!("unknown component '{}'", cmd.component));
                continue;
            }
        };

        let write_fn = match &entry.write {
            Some(f) => f.clone(),
            None => {
                input.reply_failed(format!("'{}' is read-only", cmd.component));
                continue;
            }
        };

        let json = cmd.json.join(" ");
        let bytes = match write_fn(&json) {
            Ok(b) => b,
            Err(e) => {
                input.reply_failed(format!("invalid JSON: {e}"));
                continue;
            }
        };

        match staged.apply(
            &mut resolver,
            &crdt_interfaces,
            entry.id,
            entry.crdt_type,
            eid,
            Some(bytes.as_slice()),
        ) {
            Ok(()) => input.reply_ok(format!("set {}.{} = {json}", cmd.entity, cmd.component)),
            Err(e) => input.reply_failed(e),
        }
    }

    staged.flush(&mut commands, &crdt_interfaces);
}

// --- /new_entity ---

/// Allocate one or more fresh scene entity ids from the scene's own allocator (collision-free and
/// correctly generationed) and instantiate each scene-side with the given component, so `@dcl/ecs`
/// adopts it. Replies with a JSON array of the allocated entity ids (proto-u32 form, matching the
/// snapshot's keys). The editor uses these as the ids to write the rest of an entity's components.
///
/// `component` MUST be a custom (non-engine-recognized) component — e.g. core-schema::Name. Engine
/// components flow renderer→scene one-way (never echoed back), so an instantiation written with one
/// would be lost; only a custom component round-trips.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/new_entity")]
struct NewEntityCommand {
    /// Numeric component id to instantiate each entity with (e.g. core-schema::Name)
    component: u32,
    /// base64-encoded component value bytes
    data: String,
    /// number of entities to allocate
    count: usize,
}

fn new_entity_cmd(
    mut input: ConsoleCommand<NewEntityCommand>,
    resolver: SceneResolver,
    crdt_interfaces: Res<CrdtExtractors>,
    mut pending: ResMut<PendingEntityAllocations>,
    mut console_responses: ResMut<PendingConsoleResponses>,
) {
    if let Some(Ok(cmd)) = input.take() {
        // The instantiation must use a custom component — an engine-recognized one flows
        // renderer→scene one-way (never echoed back) and would be lost.
        if crdt_interfaces
            .0
            .contains_key(&SceneComponentId(cmd.component))
        {
            input.reply_failed(format!(
                "component {} is engine-recognized; instantiate with a custom component",
                cmd.component
            ));
            return;
        }
        let data = match BASE64_STANDARD.decode(cmd.data.as_bytes()) {
            Ok(d) => d,
            Err(e) => {
                input.reply_failed(format!("invalid base64: {e}"));
                return;
            }
        };
        let (tx, rx) = tokio::sync::oneshot::channel();
        match resolver.request_allocate_entity(
            &mut pending,
            SceneComponentId(cmd.component),
            data,
            cmd.count,
            move |ids| {
                let json = format!(
                    "[{}]",
                    ids.iter()
                        .map(|id| id.as_proto_u32().unwrap_or(id.id as u32).to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                );
                let _ = tx.send(Ok(json));
            },
        ) {
            Ok(()) => console_responses.push_oneshot(rx, |r| r, input.take_responder()),
            Err(e) => input.reply_failed(e),
        }
    }
}

// --- /set_component_raw ---

/// Set a custom (non-engine-managed) component on a scene entity from raw bytes — for the
/// components the engine doesn't recognize (core-schema::, asset-packs::, inspector::), which
/// `/set_component` can't address. The editor encodes the value with the SDK schema and sends it
/// base64, keyed by numeric component id. The value is written to the scene's CRDT store and
/// pushed over the normal engine→scene channel; the timestamp must exceed the component's current
/// LWW timestamp (reported by `/crdt_snapshot`) so the write wins.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/set_component_raw")]
struct SetComponentRawCommand {
    /// Entity id or alias: root, player, camera
    entity: String,
    /// Numeric component id
    component: u32,
    /// LWW timestamp (must be greater than the component's current timestamp)
    timestamp: u32,
    /// Component value as standard base64-encoded bytes
    data: String,
}

fn set_component_raw_cmd(
    mut input: ConsoleCommand<SetComponentRawCommand>,
    mut resolver: SceneResolver,
) {
    while let Some(Ok(cmd)) = input.take() {
        let eid = match parse_entity_id(&cmd.entity) {
            Ok(e) => e,
            Err(e) => {
                input.reply_failed(e);
                continue;
            }
        };

        let bytes = match BASE64_STANDARD.decode(cmd.data.trim()) {
            Ok(b) => b,
            Err(e) => {
                input.reply_failed(format!("invalid base64: {e}"));
                continue;
            }
        };

        match resolver.resolve_mut() {
            Err(e) => input.reply_failed(e),
            Ok((_scene_entity, mut ctx)) => {
                // Normal-mode write (timestamp-checked): applies only if newer, then is delivered
                // to the scene by the per-frame `take_updates`. Custom components aren't
                // renderer-managed, so no Bevy-side staging is needed.
                let applied = ctx.crdt_store.try_update(
                    SceneComponentId(cmd.component),
                    CrdtType::LWW_ANY,
                    eid,
                    SceneCrdtTimestamp(cmd.timestamp),
                    Some(&mut DclReader::new(&bytes)),
                );
                if applied {
                    input.reply_ok(format!(
                        "set {}.{} ({} bytes)",
                        cmd.entity,
                        cmd.component,
                        bytes.len()
                    ));
                } else {
                    input.reply_failed(format!(
                        "rejected: timestamp {} is not newer than the current value",
                        cmd.timestamp
                    ));
                }
            }
        }
    }
}

// --- /save_composite ---

/// Persist a composite the inspector built (the bytes arrive base64-encoded). The destination is
/// derived from the active scene: for a local scene it writes straight to its
/// assets/scene/main.composite; otherwise a save dialog (native) / directory picker (web) is
/// shown — see `platform::save_scene_composite`. Replies with the path written.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/save_composite")]
struct SaveCompositeCommand {
    /// Composite bytes as standard base64
    data: String,
}

fn save_composite_cmd(
    mut input: ConsoleCommand<SaveCompositeCommand>,
    resolver: SceneResolver,
    mut console_responses: ResMut<PendingConsoleResponses>,
) {
    if let Some(Ok(cmd)) = input.take() {
        let bytes = match BASE64_STANDARD.decode(cmd.data.trim()) {
            Ok(b) => b,
            Err(e) => {
                input.reply_failed(format!("invalid base64: {e}"));
                return;
            }
        };
        let hash = match resolver.resolve() {
            Ok((_, ctx)) => ctx.hash.clone(),
            Err(e) => {
                input.reply_failed(e);
                return;
            }
        };
        // The write (and the picker on web) is async — run it off the main schedule and reply when
        // it resolves. Imported-asset files are pushed to disk at import time (/init_asset), not
        // here, so this only writes the composite.
        let (tx, rx) = tokio::sync::oneshot::channel();
        IoTaskPool::get()
            .spawn(async move {
                let _ = tx.send(platform::save_scene_composite(hash, bytes).await);
            })
            .detach();
        console_responses.push_oneshot(rx, |r| r, input.take_responder());
    }
}

// --- /delete_component ---

/// Remove a component from a scene entity
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/delete_component")]
struct DeleteComponentCommand {
    /// Entity id or alias: root, player, camera
    entity: String,
    /// Component name (PascalCase)
    component: String,
}

fn delete_component_cmd(
    mut input: ConsoleCommand<DeleteComponentCommand>,
    registry: Res<ComponentNameRegistry>,
    mut resolver: SceneResolver,
    crdt_interfaces: Res<CrdtExtractors>,
    mut commands: Commands,
) {
    let mut staged = StagedBevyUpdates::default();

    while let Some(Ok(cmd)) = input.take() {
        let eid = match parse_entity_id(&cmd.entity) {
            Ok(e) => e,
            Err(e) => {
                input.reply_failed(e);
                continue;
            }
        };

        let entry = match registry.get_by_name(&cmd.component) {
            Some(e) => e,
            None => {
                input.reply_failed(format!("unknown component '{}'", cmd.component));
                continue;
            }
        };

        // A removal is a `None` write — only the LWW store tracks per-entity presence
        // (GO is append-only), and inspect-only components can't be edited at all.
        if entry.write.is_none() {
            input.reply_failed(format!("'{}' is read-only", cmd.component));
            continue;
        }
        if !matches!(entry.crdt_type, CrdtType::LWW(_)) {
            input.reply_failed(format!("'{}' is not deletable", cmd.component));
            continue;
        }

        match staged.apply(
            &mut resolver,
            &crdt_interfaces,
            entry.id,
            entry.crdt_type,
            eid,
            None,
        ) {
            Ok(()) => input.reply_ok(format!("removed {}.{}", cmd.entity, cmd.component)),
            Err(e) => input.reply_failed(e),
        }
    }

    staged.flush(&mut commands, &crdt_interfaces);
}

// --- /delete_entity ---

/// Delete a scene entity (and optionally its descendants)
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/delete_entity")]
struct DeleteEntityCommand {
    /// Entity id or alias: root, player, camera
    entity: String,
    /// Also delete all descendants
    #[arg(short, long)]
    recursive: bool,
}

/// Walk the CRDT Transform LWW entries to collect all descendants of `root_eid`.
fn collect_descendants(crdt_store: &CrdtStore, root_eid: SceneEntityId) -> HashSet<SceneEntityId> {
    let mut to_delete = HashSet::new();
    to_delete.insert(root_eid);

    let transform_lww = match crdt_store.lww.get(&SceneComponentId::TRANSFORM) {
        Some(lww) => lww,
        None => return to_delete,
    };

    // Iteratively find children whose parent is in the set
    let mut changed = true;
    while changed {
        changed = false;
        for (eid, entry) in &transform_lww.last_write {
            if to_delete.contains(eid) || !entry.is_some {
                continue;
            }
            if let Ok(t) = DclTransformAndParent::from_reader(&mut DclReader::new(&entry.data)) {
                if to_delete.contains(&t.parent()) {
                    to_delete.insert(*eid);
                    changed = true;
                }
            }
        }
    }

    to_delete
}

fn delete_entity_cmd(mut input: ConsoleCommand<DeleteEntityCommand>, mut resolver: SceneResolver) {
    while let Some(Ok(cmd)) = input.take() {
        let eid = match parse_entity_id(&cmd.entity) {
            Ok(e) => e,
            Err(e) => {
                input.reply_failed(e);
                continue;
            }
        };

        match resolver.resolve_mut() {
            Err(e) => input.reply_failed(e),
            Ok((_scene_entity, mut ctx)) => {
                if ctx.bevy_entity(eid).is_none() {
                    input.reply_failed(format!("entity {} does not exist", cmd.entity));
                    continue;
                }

                let to_delete = if cmd.recursive {
                    collect_descendants(&ctx.crdt_store, eid)
                } else {
                    HashSet::from([eid])
                };

                let count = to_delete.len();
                // outbound_died -> engine->scene DeleteEntity census (deletes it
                // scene-side); death_row -> engine-side despawn. No local clean_up
                // needed — the scene delete + despawn handle it.
                ctx.outbound_died.extend(to_delete.iter().copied());
                ctx.death_row.extend(to_delete);

                if cmd.recursive && count > 1 {
                    input.reply_ok(format!(
                        "deleted {} and {} descendants",
                        cmd.entity,
                        count - 1
                    ));
                } else {
                    input.reply_ok(format!("deleted {}", cmd.entity));
                }
            }
        }
    }
}

// --- /freeze_scene ---

/// Pause the active scene so it stops ticking
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/freeze_scene")]
struct FreezeSceneCommand;

fn freeze_scene_cmd(mut input: ConsoleCommand<FreezeSceneCommand>, mut resolver: SceneResolver) {
    if let Some(Ok(_)) = input.take() {
        match resolver.resolve_mut() {
            Err(e) => input.reply_failed(e),
            Ok((_scene_entity, mut ctx)) => {
                if ctx.blocked.contains(FROZEN_BLOCK) {
                    input.reply_failed("scene is already frozen");
                } else {
                    ctx.blocked.insert(FROZEN_BLOCK);
                    input.reply_ok(format!("frozen at tick {}", ctx.tick_number));
                }
            }
        }
    }
}

// --- /unfreeze_scene ---

/// Resume a frozen scene
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/unfreeze_scene")]
struct UnfreezeSceneCommand;

fn unfreeze_scene_cmd(
    mut input: ConsoleCommand<UnfreezeSceneCommand>,
    mut resolver: SceneResolver,
) {
    if let Some(Ok(_)) = input.take() {
        match resolver.resolve_mut() {
            Err(e) => input.reply_failed(e),
            Ok((_scene_entity, mut ctx)) => {
                if !ctx.blocked.contains(FROZEN_BLOCK) {
                    input.reply_failed("scene is not frozen");
                } else {
                    ctx.blocked.remove(FROZEN_BLOCK);
                    ctx.refreeze_at_tick = None;
                    input.reply_ok(format!("unfrozen at tick {}", ctx.tick_number));
                }
            }
        }
    }
}

// --- /tick_scene ---

/// Advance a frozen scene by N ticks (default 1)
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/tick_scene")]
struct TickSceneCommand {
    /// Number of ticks to advance
    #[arg(default_value = "1")]
    count: u32,
}

fn tick_scene_cmd(mut input: ConsoleCommand<TickSceneCommand>, mut resolver: SceneResolver) {
    if let Some(Ok(cmd)) = input.take() {
        if cmd.count == 0 {
            input.reply_failed("count must be > 0");
            return;
        }

        match resolver.resolve_mut() {
            Err(e) => input.reply_failed(e),
            Ok((_scene_entity, mut ctx)) => {
                if !ctx.blocked.contains(FROZEN_BLOCK) {
                    input.reply_failed("scene is not frozen (use /freeze_scene first)");
                    return;
                }

                ctx.blocked.remove(FROZEN_BLOCK);
                ctx.refreeze_at_tick = Some(ctx.tick_number + cmd.count);
                input.reply_ok(format!(
                    "advancing {} tick{} from {}",
                    cmd.count,
                    if cmd.count == 1 { "" } else { "s" },
                    ctx.tick_number
                ));
            }
        }
    }
}
