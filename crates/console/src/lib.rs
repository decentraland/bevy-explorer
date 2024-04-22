use bevy::{prelude::*, scene::scene_spawner_system};
use bevy_console::{
    Command, ConsoleCommand, ConsoleCommandEntered, ConsoleConfiguration, ConsoleSet,
    PrintConsoleLine,
};
use clap::Parser;

use common::sets::SceneSets;

pub trait DoAddConsoleCommand {
    fn add_console_command<T: Command, U>(
        &mut self,
        system: impl IntoSystemConfigs<U>,
    ) -> &mut Self;
}

// hook console commands
#[cfg(not(test))]
impl DoAddConsoleCommand for App {
    fn add_console_command<T: bevy_console::Command, U>(
        &mut self,
        system: impl IntoSystemConfigs<U>,
    ) -> &mut Self {
        bevy_console::AddConsoleCommand::add_console_command::<T, U>(self, system)
    }
}

#[cfg(test)]
impl DoAddConsoleCommand for App {
    fn add_console_command<T: bevy_console::Command, U>(
        &mut self,
        _: impl IntoSystemConfigs<U>,
    ) -> &mut Self {
        // do nothing
        self
    }
}

pub struct ConsolePlugin {
    pub add_egui: bool,
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

        if self.add_egui {
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

//re-add default commands, unfortunately have to copy/paste
#[derive(Parser, ConsoleCommand)]
#[command(name = "/help")]
pub(crate) struct HelpCommand;

pub(crate) fn help_command(
    mut cmd: ConsoleCommand<HelpCommand>,
    mut pending: ResMut<PendingCommands>,
) {
    if let Some(Ok(_)) = cmd.take() {
        pending.0.push("help".to_owned());
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
        sender.send(ConsoleCommandEntered {
            command_name,
            args: Default::default(),
        });
    }
}
