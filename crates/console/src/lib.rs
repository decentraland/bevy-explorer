use bevy::{ecs::system::ScheduleSystem, prelude::*, scene::scene_spawner_system};
use bevy_console::{
    Command, ConsoleCommand, ConsoleCommandEntered, ConsoleConfiguration, ConsoleResponder,
    ConsoleSet, PrintConsoleLine,
};
use clap::Parser;
use common::{rpc::RpcResultReceiver, sets::SceneSets};
use std::sync::Mutex;

pub trait DoAddConsoleCommand {
    fn add_console_command<T: Command, U>(
        &mut self,
        system: impl IntoScheduleConfigs<ScheduleSystem, U>,
    ) -> &mut Self;
}

// hook console commands
#[cfg(not(test))]
impl DoAddConsoleCommand for App {
    fn add_console_command<T: bevy_console::Command, U>(
        &mut self,
        system: impl IntoScheduleConfigs<ScheduleSystem, U>,
    ) -> &mut Self {
        bevy_console::AddConsoleCommand::add_console_command::<T, U>(self, system)
    }
}

#[cfg(test)]
impl DoAddConsoleCommand for App {
    fn add_console_command<T: bevy_console::Command, U>(
        &mut self,
        _: impl IntoScheduleConfigs<ScheduleSystem, U>,
    ) -> &mut Self {
        // do nothing
        self
    }
}

pub struct ConsolePlugin {
    // add the bevy_console command plumbing; false (tests) registers the bare events only
    pub add_bevy_console: bool,
}

impl Plugin for ConsolePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ConsoleConfiguration {
            // keys: vec![KeyCode::Backquote],
            // we don't want people using the actual console, just the command / print interface which gets piped to chat
            keys: vec![],
            left_pos: 0.0,
            top_pos: 0.0,
            height: 100.0,
            width: 5000.0,
            ..Default::default()
        });

        if self.add_bevy_console {
            app.add_plugins(bevy_console::ConsolePlugin);
        } else {
            app.add_event::<ConsoleCommandEntered>();
            app.add_event::<PrintConsoleLine>();
        }

        app.add_systems(
            Update,
            remove_default_commands.run_if(|mut once: Local<bool>| {
                let run = !*once;
                *once = true;
                run
            }),
        )
        .add_console_command::<ClearCommand, _>(clear_command)
        .add_console_command::<HelpCommand, _>(help_command)
        .add_console_command::<ExitCommand, _>(exit_command)
        .init_resource::<PendingCommands>()
        .add_systems(Update, send_pending);

        app.init_resource::<PendingConsoleResponses>()
            .add_systems(Update, poll_console_responses);

        app.configure_sets(
            Update,
            (
                ConsoleSet::ConsoleUI,
                ConsoleSet::Commands,
                ConsoleSet::PostCommands.before(SceneSets::Init),
            )
                .after(scene_spawner_system),
        );
    }
}

fn remove_default_commands(mut config: ResMut<ConsoleConfiguration>) {
    for command_name in ["/help", "/clear", "/exit"] {
        if let Some(res) = config.commands.remove(&command_name[1..]) {
            config.commands.insert(command_name, res);
        }
    }
}

#[derive(Resource, Default)]
pub struct PendingCommands(Vec<String>);

//re-add default commands, unfortunately have to copy/paste
#[derive(Parser, ConsoleCommand)]
#[command(name = "/clear")]
pub(crate) struct ClearCommand;

pub(crate) fn clear_command(
    mut cmd: ConsoleCommand<ClearCommand>,
    mut pending: ResMut<PendingCommands>,
) {
    if let Some(Ok(_)) = cmd.take() {
        pending.0.push("clear".to_owned());
    }
}

/// List available commands, or show detailed help for a specific command
#[derive(Parser, ConsoleCommand)]
#[command(name = "/help")]
pub(crate) struct HelpCommand {
    /// Command to show help for
    command: Option<String>,
}

pub(crate) fn help_command(
    mut cmd: ConsoleCommand<HelpCommand>,
    mut config: ResMut<ConsoleConfiguration>,
) {
    match cmd.take() {
        Some(Ok(HelpCommand {
            command: Some(name),
        })) => match config.commands.get_mut(name.as_str()) {
            Some(command_info) => {
                cmd.reply(command_info.render_long_help().to_string());
                cmd.ok();
            }
            None => {
                cmd.reply(format!("Command '{name}' does not exist"));
                cmd.failed();
            }
        },
        Some(Ok(HelpCommand { command: None })) => {
            cmd.reply("Available commands:");
            let longest = config.commands.keys().map(|n| n.len()).max().unwrap_or(0);
            for (name, command_info) in &config.commands {
                let about = command_info
                    .get_about()
                    .map(|a| a.to_string())
                    .unwrap_or_default();
                cmd.reply(format!(
                    "  {name}{} - {about}",
                    " ".repeat(longest - name.len())
                ));
            }
            cmd.ok();
        }
        _ => {}
    }
}

//re-add default commands, unfortunately have to copy/paste
#[derive(Parser, ConsoleCommand)]
#[command(name = "/exit")]
pub(crate) struct ExitCommand;

pub(crate) fn exit_command(
    mut cmd: ConsoleCommand<ExitCommand>,
    mut pending: ResMut<PendingCommands>,
) {
    if let Some(Ok(_)) = cmd.take() {
        pending.0.push("exit".to_owned());
    }
}

pub fn send_pending(
    mut pending: ResMut<PendingCommands>,
    mut sender: EventWriter<ConsoleCommandEntered>,
) {
    for command_name in pending.0.drain(..) {
        sender.write(ConsoleCommandEntered {
            command_name,
            args: Default::default(),
            responder: None,
        });
    }
}

type ConsoleResponseFn = Box<dyn Fn() -> Option<Result<String, String>> + Send + Sync>;

struct PendingResponse {
    poll: ConsoleResponseFn,
    /// If set, the result is delivered to this responder; otherwise it is written to
    /// the console as reply lines followed by `[ok]` / `[failed]`.
    responder: Option<ConsoleResponder>,
}

/// Stores pending async console command responses. Register a receiver with
/// [`push_receiver`](PendingConsoleResponses::push_receiver) and a mapping function.
/// When the receiver resolves, the result is delivered to the command's
/// [`ConsoleResponder`] if one was supplied (programmatic invocation), or printed to
/// the console as reply lines followed by `[ok]` / `[failed]` (console invocation).
#[derive(Resource, Default)]
pub struct PendingConsoleResponses(Vec<PendingResponse>);

impl PendingConsoleResponses {
    /// Register an [`RpcResultReceiver`] to be polled each frame. When it resolves,
    /// `map` converts the value to `Ok(message)` or `Err(message)`, which is then
    /// delivered to `responder` (or printed to the console if `responder` is `None`).
    ///
    /// `responder` is typically obtained from [`ConsoleCommand::take_responder`].
    pub fn push_receiver<T, F>(
        &mut self,
        receiver: RpcResultReceiver<T>,
        map: F,
        responder: Option<ConsoleResponder>,
    ) where
        T: Send + 'static,
        F: Fn(T) -> Result<String, String> + Send + Sync + 'static,
    {
        let receiver = Mutex::new(receiver);
        self.0.push(PendingResponse {
            poll: Box::new(move || {
                let mut guard = receiver.lock().unwrap();
                match guard.poll_once() {
                    Ok(Some(val)) => Some(map(val)),
                    Ok(None) => None,
                    Err(()) => Some(Err("cancelled".to_string())),
                }
            }),
            responder,
        });
    }

    /// Register a raw tokio oneshot receiver to be polled each frame.
    pub fn push_oneshot<T, F>(
        &mut self,
        receiver: tokio::sync::oneshot::Receiver<T>,
        map: F,
        responder: Option<ConsoleResponder>,
    ) where
        T: Send + 'static,
        F: Fn(T) -> Result<String, String> + Send + Sync + 'static,
    {
        let receiver = Mutex::new(receiver);
        self.0.push(PendingResponse {
            poll: Box::new(move || {
                let mut guard = receiver.lock().unwrap();
                match guard.try_recv() {
                    Ok(val) => Some(map(val)),
                    Err(tokio::sync::oneshot::error::TryRecvError::Empty) => None,
                    Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                        Some(Err("cancelled".to_string()))
                    }
                }
            }),
            responder,
        });
    }
}

fn poll_console_responses(
    mut pending: ResMut<PendingConsoleResponses>,
    mut console: EventWriter<PrintConsoleLine>,
) {
    pending.0.retain(|entry| match (entry.poll)() {
        None => true,
        Some(result) => {
            match &entry.responder {
                Some(responder) => responder(result),
                None => {
                    let (msg, sentinel) = match result {
                        Ok(msg) => (msg, "[ok]"),
                        Err(msg) => (msg, "[failed]"),
                    };
                    if !msg.is_empty() {
                        console.write(PrintConsoleLine::new(msg));
                    }
                    console.write(PrintConsoleLine::new(sentinel.into()));
                }
            }
            false
        }
    });
}
