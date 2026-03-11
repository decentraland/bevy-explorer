use bevy::prelude::*;
use bevy_console::ConsoleCommand;
use console::{DoAddConsoleCommand, PendingConsoleResponses};

use crate::{LiveSceneInfo, SystemApi};

pub struct AgentCommandsPlugin;

impl Plugin for AgentCommandsPlugin {
    fn build(&self, app: &mut App) {
        app.add_console_command::<LoginGuestCommand, _>(login_guest_cmd);
        app.add_console_command::<LoginPreviousCommand, _>(login_previous_cmd);
        app.add_console_command::<LogoutCommand, _>(logout_cmd);
        app.add_console_command::<LiveScenesCommand, _>(live_scenes_cmd);
        app.add_console_command::<ChatCommand, _>(chat_cmd);
    }
}

// --- /login_guest ---

/// Login as guest (session-only, profile will not persist)
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/login_guest")]
struct LoginGuestCommand;

fn login_guest_cmd(
    mut input: ConsoleCommand<LoginGuestCommand>,
    mut events: EventWriter<SystemApi>,
) {
    if let Some(Ok(_)) = input.take() {
        events.write(SystemApi::LoginGuest);
        input.reply_ok("logging in as guest");
    }
}

// --- /login_previous ---

/// Login using previously saved credentials
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/login_previous")]
struct LoginPreviousCommand;

fn login_previous_cmd(
    mut input: ConsoleCommand<LoginPreviousCommand>,
    mut events: EventWriter<SystemApi>,
    mut pending: ResMut<PendingConsoleResponses>,
) {
    if let Some(Ok(_)) = input.take() {
        let (sx, rx) = common::rpc::RpcResultSender::<Result<(), String>>::channel();
        events.write(SystemApi::LoginPrevious(sx));
        pending.push_receiver(rx, |result| match result {
            Ok(()) => Ok("logged in with previous credentials".to_string()),
            Err(e) => Err(e),
        });
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
