use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy_console::ConsoleCommand;
use console::DoAddConsoleCommand;
use dcl::interface::CrdtStore;
use dcl_component::{
    transform_and_parent::DclTransformAndParent, ComponentNameRegistry, DclReader, FromDclReader,
    SceneComponentId, SceneEntityId,
};
use scene_runner::{renderer_context::FROZEN_BLOCK, update_world::CrdtExtractors};

use crate::{active_scene::SceneResolver, read_commands::parse_entity_id};

pub fn add_write_commands(app: &mut App) {
    app.add_console_command::<SetComponentCommand, _>(set_component_cmd);
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

        let write_fn = match &entry.write {
            Some(f) => f.clone(),
            None => {
                input.reply_failed(format!("'{}' is read-only", cmd.component));
                return;
            }
        };

        let component_id = entry.id;
        let crdt_type = entry.crdt_type;
        let json = cmd.json.join(" ");

        let bytes = match write_fn(&json) {
            Ok(b) => b,
            Err(e) => {
                input.reply_failed(format!("invalid JSON: {e}"));
                return;
            }
        };

        match resolver.resolve_mut() {
            Err(e) => input.reply_failed(e),
            Ok((scene_entity, mut ctx)) => {
                ctx.crdt_store.force_update(
                    component_id,
                    crdt_type,
                    eid,
                    Some(&mut DclReader::new(&bytes)),
                );

                // Also apply immediately to the Bevy entity so the renderer reflects the change
                // without waiting for the scene to echo it back. Pure scene-side components have
                // no entry in CrdtExtractors and will be applied when the scene echoes the write.
                if let Some(interface) = crdt_interfaces.0.get(&component_id) {
                    let mut mini_crdt = CrdtStore::default();
                    mini_crdt.force_update(
                        component_id,
                        crdt_type,
                        eid,
                        Some(&mut DclReader::new(&bytes)),
                    );
                    interface.updates_to_entity(
                        component_id,
                        &mut mini_crdt,
                        &mut commands.entity(scene_entity),
                    );
                }

                input.reply_ok(format!("set {}.{} = {json}", cmd.entity, cmd.component));
            }
        }
    }
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
    if let Some(Ok(cmd)) = input.take() {
        let eid = match parse_entity_id(&cmd.entity) {
            Ok(e) => e,
            Err(e) => {
                input.reply_failed(e);
                return;
            }
        };

        match resolver.resolve_mut() {
            Err(e) => input.reply_failed(e),
            Ok((_scene_entity, mut ctx)) => {
                if ctx.bevy_entity(eid).is_none() {
                    input.reply_failed(format!("entity {} does not exist", cmd.entity));
                    return;
                }

                let to_delete = if cmd.recursive {
                    collect_descendants(&ctx.crdt_store, eid)
                } else {
                    HashSet::from([eid])
                };

                let count = to_delete.len();
                ctx.crdt_store.clean_up(&to_delete);
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
