use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy_console::ConsoleCommand;
use console::DoAddConsoleCommand;
use dcl::interface::CrdtStore;
use dcl_component::{
    transform_and_parent::DclTransformAndParent, ComponentNameRegistry, CrdtType, DclReader,
    FromDclReader, SceneComponentId, SceneEntityId,
};
use scene_runner::{renderer_context::FROZEN_BLOCK, update_world::CrdtExtractors};

use crate::{active_scene::SceneResolver, read_commands::parse_entity_id};

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
