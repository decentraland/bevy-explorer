// Lightweight message bus between the host page UI and the system (editor) scene.
// Both sides exchange opaque JSON payloads via console commands:
//   /editor_send <page|scene> <json>   push a message for the given target
//   /editor_poll <page|scene>          drain messages queued for the given target
// The page polls with target "page", the scene polls with target "scene".

use std::collections::VecDeque;

use bevy::prelude::*;
use bevy_console::ConsoleCommand;
use console::DoAddConsoleCommand;

const MAX_QUEUED_MESSAGES: usize = 256;

#[derive(Resource, Default)]
pub struct EditorMessageBus {
    to_page: VecDeque<String>,
    to_scene: VecDeque<String>,
}

impl EditorMessageBus {
    fn queue_mut(&mut self, target: &str) -> Option<&mut VecDeque<String>> {
        match target {
            "page" => Some(&mut self.to_page),
            "scene" => Some(&mut self.to_scene),
            _ => None,
        }
    }
}

pub fn add_message_bus_commands(app: &mut App) {
    app.init_resource::<EditorMessageBus>();
    app.add_console_command::<EditorSendCommand, _>(editor_send_cmd);
    app.add_console_command::<EditorPollCommand, _>(editor_poll_cmd);
}

// --- /editor_send ---

/// Queue a message on the editor ui bus for the given target ("page" or "scene")
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/editor_send")]
struct EditorSendCommand {
    /// recipient: "page" (host page ui) or "scene" (system scene)
    target: String,
    /// message payload (json)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 1..)]
    json: Vec<String>,
}

fn editor_send_cmd(
    mut input: ConsoleCommand<EditorSendCommand>,
    mut bus: ResMut<EditorMessageBus>,
) {
    if let Some(Ok(cmd)) = input.take() {
        let Some(queue) = bus.queue_mut(&cmd.target) else {
            input.reply_failed(format!(
                "unknown target '{}', expected 'page' or 'scene'",
                cmd.target
            ));
            return;
        };
        if queue.len() >= MAX_QUEUED_MESSAGES {
            queue.pop_front();
        }
        queue.push_back(cmd.json.join(" "));
        input.reply_ok("queued");
    }
}

// --- /editor_poll ---

/// Drain queued editor ui bus messages for the given target ("page" or "scene")
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/editor_poll")]
struct EditorPollCommand {
    /// recipient: "page" (host page ui) or "scene" (system scene)
    target: String,
}

fn editor_poll_cmd(
    mut input: ConsoleCommand<EditorPollCommand>,
    mut bus: ResMut<EditorMessageBus>,
) {
    if let Some(Ok(cmd)) = input.take() {
        let Some(queue) = bus.queue_mut(&cmd.target) else {
            input.reply_failed(format!(
                "unknown target '{}', expected 'page' or 'scene'",
                cmd.target
            ));
            return;
        };
        let messages: Vec<String> = queue.drain(..).collect();
        match serde_json::to_string(&messages) {
            Ok(json) => input.reply_ok(json),
            Err(e) => input.reply_failed(format!("serialize failed: {e}")),
        }
    }
}
