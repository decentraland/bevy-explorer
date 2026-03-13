use bevy::prelude::*;
use bevy_console::ConsoleCommand;
use console::DoAddConsoleCommand;
use dcl::interface::CrdtStore;
use dcl_component::{ComponentNameRegistry, DclReader};
use scene_runner::update_world::CrdtExtractors;

use crate::{active_scene::SceneResolver, read_commands::parse_entity_id};

pub fn add_write_commands(app: &mut App) {
    app.add_console_command::<SetComponentCommand, _>(set_component_cmd);
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
