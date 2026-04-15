use bevy::prelude::*;
use bevy_console::ConsoleCommand;
use common::{
    rpc::{RpcCall, RpcResultSender, SpawnResponse},
    structs::{EmoteCommand, PrimaryUser},
};
use console::{DoAddConsoleCommand, PendingConsoleResponses};
use dcl_component::transform_and_parent::DclTranslation;

pub struct AgentCommandsPlugin;

impl Plugin for AgentCommandsPlugin {
    fn build(&self, app: &mut App) {
        app.add_console_command::<MovePlayerToCommand, _>(move_player_to_cmd);
        app.add_console_command::<WalkPlayerToCommand, _>(walk_player_to_cmd);
        app.add_console_command::<PlayerPositionCommand, _>(player_position_cmd);
        app.add_console_command::<ListPortablesCommand, _>(list_portables_cmd);
        app.add_console_command::<ConnectedPlayersCommand, _>(connected_players_cmd);
        app.add_console_command::<TriggerEmoteCommand, _>(trigger_emote_cmd);
        app.add_console_command::<GetUserDataCommand, _>(get_user_data_cmd);
    }
}

// --- /move_player_to ---

/// Move the player to a DCL world-space position, with optional linear interpolation.
/// Coordinates are in Decentraland world space (x right, y up, z forward).
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/move_player_to")]
struct MovePlayerToCommand {
    #[arg(allow_hyphen_values(true))]
    x: f32,
    #[arg(allow_hyphen_values(true))]
    y: f32,
    #[arg(allow_hyphen_values(true))]
    z: f32,
    /// Duration in seconds for linear interpolation. Omit for instant teleport.
    #[arg(allow_hyphen_values(true))]
    duration: Option<f32>,
}

fn move_player_to_cmd(
    mut input: ConsoleCommand<MovePlayerToCommand>,
    mut events: EventWriter<RpcCall>,
    mut pending: ResMut<PendingConsoleResponses>,
) {
    if let Some(Ok(command)) = input.take() {
        let to = DclTranslation([command.x, command.y, command.z]).to_bevy_translation();
        let (response, rx) = RpcResultSender::<bool>::channel();
        events.write(RpcCall::MovePlayer {
            scene: None,
            to,
            looking_at: None,
            duration: command.duration,
            response: Some(response),
        });
        let (x, y, z) = (command.x, command.y, command.z);
        let has_duration = command.duration.is_some();
        pending.push_receiver(rx, move |success| {
            if success {
                Ok(if has_duration {
                    format!("arrived at ({x}, {y}, {z})")
                } else {
                    format!("moved to ({x}, {y}, {z})")
                })
            } else {
                Err("move cancelled".to_string())
            }
        });
    }
}

// --- /walk_player_to ---

/// Walk the player to a DCL world-space position using the movement scene controller.
/// Coordinates are in Decentraland world space (x right, y up, z forward).
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/walk_player_to")]
struct WalkPlayerToCommand {
    #[arg(allow_hyphen_values(true))]
    x: f32,
    #[arg(allow_hyphen_values(true))]
    y: f32,
    #[arg(allow_hyphen_values(true))]
    z: f32,
    /// Timeout in seconds before the walk is cancelled. Omit for no timeout.
    #[arg(allow_hyphen_values(true))]
    timeout: Option<f32>,
}

fn walk_player_to_cmd(
    mut input: ConsoleCommand<WalkPlayerToCommand>,
    mut events: EventWriter<RpcCall>,
    mut pending: ResMut<PendingConsoleResponses>,
) {
    if let Some(Ok(command)) = input.take() {
        let to = DclTranslation([command.x, command.y, command.z]).to_bevy_translation();
        let (response, rx) = RpcResultSender::<bool>::channel();
        events.write(RpcCall::WalkPlayer {
            scene: None,
            to,
            stop_threshold: 0.5,
            timeout: command.timeout,
            response,
        });
        let (x, y, z) = (command.x, command.y, command.z);
        pending.push_receiver(rx, move |success| {
            if success {
                Ok(format!("arrived at ({x}, {y}, {z})"))
            } else {
                Err("walk failed or timed out".to_string())
            }
        });
    }
}

// --- /player_position ---

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/player_position")]
struct PlayerPositionCommand;

fn player_position_cmd(
    mut input: ConsoleCommand<PlayerPositionCommand>,
    player: Query<&Transform, With<PrimaryUser>>,
) {
    if let Some(Ok(_)) = input.take() {
        match player.single() {
            Ok(transform) => {
                let p = transform.translation;
                input.reply_ok(format!("({}, {}, {})", p.x, p.y, -p.z));
            }
            Err(_) => input.reply_failed("player not found"),
        }
    }
}

// --- /list_portables ---

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/list_portables")]
struct ListPortablesCommand;

fn list_portables_cmd(
    mut input: ConsoleCommand<ListPortablesCommand>,
    mut events: EventWriter<RpcCall>,
    mut pending: ResMut<PendingConsoleResponses>,
) {
    if let Some(Ok(_)) = input.take() {
        let (response, rx) = RpcResultSender::<Vec<SpawnResponse>>::channel();
        events.write(RpcCall::ListPortables { response });
        pending.push_receiver(rx, |portables| {
            if portables.is_empty() {
                Ok("no portables running".to_string())
            } else {
                Ok(portables
                    .iter()
                    .map(|p| format!("{} ({})", p.ens.as_deref().unwrap_or(&p.name), p.pid))
                    .collect::<Vec<_>>()
                    .join(", "))
            }
        });
    }
}

// --- /connected_players ---

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/connected_players")]
struct ConnectedPlayersCommand;

fn connected_players_cmd(
    mut input: ConsoleCommand<ConnectedPlayersCommand>,
    mut events: EventWriter<RpcCall>,
    mut pending: ResMut<PendingConsoleResponses>,
) {
    if let Some(Ok(_)) = input.take() {
        let (response, rx) = RpcResultSender::<Vec<String>>::channel();
        events.write(RpcCall::GetConnectedPlayers { response });
        pending.push_receiver(rx, |players| {
            if players.is_empty() {
                Ok("no other players connected".to_string())
            } else {
                Ok(players.join(", "))
            }
        });
    }
}

// --- /emote ---

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/emote")]
struct TriggerEmoteCommand {
    urn: String,
    #[arg(long, default_value_t = false)]
    r#loop: bool,
}

fn trigger_emote_cmd(
    mut input: ConsoleCommand<TriggerEmoteCommand>,
    mut player: Query<(Entity, Option<&EmoteCommand>), With<PrimaryUser>>,
    mut commands: Commands,
) {
    if let Some(Ok(command)) = input.take() {
        match player.single_mut() {
            Ok((entity, maybe_prev)) => {
                commands.entity(entity).try_insert(EmoteCommand {
                    urn: command.urn.clone(),
                    r#loop: command.r#loop,
                    timestamp: maybe_prev
                        .map(|prev| prev.timestamp + 1)
                        .unwrap_or_default(),
                });
                input.reply_ok(format!("playing emote {}", command.urn));
            }
            Err(_) => input.reply_failed("player not found"),
        }
    }
}

// --- /get_user_data ---

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/get_user_data")]
struct GetUserDataCommand {
    /// Wallet address to look up. Omit to get your own profile.
    address: Option<String>,
}

fn get_user_data_cmd(
    mut input: ConsoleCommand<GetUserDataCommand>,
    mut events: EventWriter<RpcCall>,
    mut pending: ResMut<PendingConsoleResponses>,
) {
    if let Some(Ok(command)) = input.take() {
        let (response, rx) =
            RpcResultSender::<Result<common::profile::SerializedProfile, ()>>::channel();
        events.write(RpcCall::GetUserData {
            user: command.address.clone(),
            scene: Entity::PLACEHOLDER,
            response,
        });
        let label = command.address.unwrap_or_else(|| "self".to_string());
        pending.push_receiver(rx, move |result| match result {
            Ok(profile) => Ok(format!(
                "{} ({}): v{}, web3={}",
                profile.name,
                profile.eth_address,
                profile.version,
                profile.has_connected_web3.unwrap_or(false),
            )),
            Err(()) => Err(format!("profile not found for {label}")),
        });
    }
}
