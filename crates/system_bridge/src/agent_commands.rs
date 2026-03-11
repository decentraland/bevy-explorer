use bevy::prelude::*;
use bevy_console::ConsoleCommand;
use console::{DoAddConsoleCommand, PendingConsoleResponses};

use crate::{LiveSceneInfo, SystemApi};

pub struct AgentCommandsPlugin;

impl Plugin for AgentCommandsPlugin {
    fn build(&self, app: &mut App) {
        app.add_console_command::<LogoutCommand, _>(logout_cmd);
        app.add_console_command::<LiveScenesCommand, _>(live_scenes_cmd);
        app.add_console_command::<ChatCommand, _>(chat_cmd);
    }
}

// --- /logout ---

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/logout")]
struct LogoutCommand;

fn logout_cmd(mut input: ConsoleCommand<LogoutCommand>, mut events: EventWriter<SystemApi>) {
    if let Some(Ok(_)) = input.take() {
        events.write(SystemApi::Logout);
        input.reply_ok("");
    }
}

// --- /live_scenes ---

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/live_scenes")]
struct LiveScenesCommand;

fn live_scenes_cmd(
    mut input: ConsoleCommand<LiveScenesCommand>,
    mut events: EventWriter<SystemApi>,
    mut pending: ResMut<PendingConsoleResponses>,
) {
    if let Some(Ok(_)) = input.take() {
        let (response, rx) = common::rpc::RpcResultSender::<Vec<LiveSceneInfo>>::channel();
        events.write(SystemApi::LiveSceneInfo(response));
        pending.push_receiver(rx, |scenes| {
            if scenes.is_empty() {
                Ok("no scenes loaded".to_string())
            } else {
                Ok(scenes
                    .iter()
                    .map(|s| {
                        format!(
                            "{} [{}{}{}]",
                            s.title,
                            s.hash,
                            if s.is_portable { ", portable" } else { "" },
                            if s.is_broken { ", broken" } else { "" },
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
        });
    }
}

// --- /chat ---

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/chat")]
struct ChatCommand {
    channel: String,
    message: String,
}

fn chat_cmd(mut input: ConsoleCommand<ChatCommand>, mut events: EventWriter<SystemApi>) {
    if let Some(Ok(command)) = input.take() {
        events.write(SystemApi::SendChat(
            command.message.clone(),
            command.channel.clone(),
        ));
        input.reply_ok(format!("[{}] {}", command.channel, command.message));
    }
}
