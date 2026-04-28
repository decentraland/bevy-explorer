pub mod avatar_movement;
pub mod camera;
pub mod player_input;

use bevy::{app::Propagate, ecs::query::Has, prelude::*, render::view::RenderLayers};

use bevy_console::ConsoleCommand;
use camera::update_cursor_lock;
use common::{
    sets::{PostUpdateSets, SceneSets},
    structs::{
        CursorLocks, EngineMovementControl, PlayerModifiers, PrimaryCamera, PrimaryUser,
        PRIMARY_AVATAR_LIGHT_LAYER_INDEX,
    },
};
use console::DoAddConsoleCommand;
use scene_runner::{update_scene::pointer_lock::update_pointer_lock, OutOfWorld};

use crate::avatar_movement::AvatarMovementPlugin;

use self::camera::{update_camera, update_camera_position};

static TRANSITION_TIME: f32 = 0.5;

// plugin to pass user input messages to the scene
pub struct UserInputPlugin;

impl Plugin for UserInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(AvatarMovementPlugin);
        app.add_systems(
            Update,
            update_camera
                .chain()
                .in_set(SceneSets::Input)
                .after(update_pointer_lock),
        );
        app.add_systems(Update, manage_player_visibility.in_set(SceneSets::PostLoop));
        app.add_systems(
            PostUpdate,
            (
                update_camera_position.in_set(PostUpdateSets::CameraUpdate),
                update_cursor_lock,
            )
                .chain(),
        );
        app.init_resource::<EngineMovementControl>()
            .init_resource::<CursorLocks>();
        app.add_console_command::<NoClipCommand, _>(no_clip);
        app.add_console_command::<SpeedCommand, _>(speed_cmd);
        app.add_console_command::<JumpCommand, _>(jump_cmd);
    }
}

#[allow(clippy::type_complexity)]
fn manage_player_visibility(
    camera: Query<&GlobalTransform, With<PrimaryCamera>>,
    mut player: Query<
        (
            &GlobalTransform,
            &mut Visibility,
            &mut Propagate<RenderLayers>,
            Has<OutOfWorld>,
            &PlayerModifiers,
        ),
        With<PrimaryUser>,
    >,
) {
    if let (Ok(cam_transform), Ok((player_transform, mut vis, mut layers, is_oow, modifiers))) =
        (camera.single(), player.single_mut())
    {
        #[allow(clippy::collapsible_else_if)]
        if is_oow || modifiers.hide {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            return;
        } else {
            if *vis != Visibility::Inherited {
                *vis = Visibility::Inherited;
            }
        }

        let distance =
            (cam_transform.translation() - player_transform.translation() - Vec3::Y * 1.81)
                .length();

        #[allow(clippy::collapsible_else_if)]
        if distance < 0.5 {
            layers.0 = layers
                .0
                .clone()
                .with(PRIMARY_AVATAR_LIGHT_LAYER_INDEX)
                .without(0);
        } else {
            layers.0 = layers
                .0
                .clone()
                .with(0)
                .without(PRIMARY_AVATAR_LIGHT_LAYER_INDEX);
        }
    }
}

// turn clipping on/off
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/idnoclip")]
pub(crate) struct NoClipCommand {
    clip: Option<bool>,
}

pub(crate) fn no_clip(
    mut input: ConsoleCommand<NoClipCommand>,
    mut control: ResMut<EngineMovementControl>,
) {
    if let Some(Ok(command)) = input.take() {
        let currently_clipping = !control.suppress_clipping.contains("noclip");
        let new_clipping = command.clip.unwrap_or(!currently_clipping);
        if new_clipping {
            control.suppress_clipping.remove("noclip");
        } else {
            control.suppress_clipping.insert("noclip");
        }
        input.reply_ok(format!("clipping set to {}", new_clipping));
    }
}

// set speed and friction
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/speed")]
pub(crate) struct SpeedCommand {
    walk: f32,
    jog: f32,
    run: f32,
}

pub(crate) fn speed_cmd(
    mut input: ConsoleCommand<SpeedCommand>,
    mut user: Query<&mut PrimaryUser>,
) {
    if let Some(Ok(command)) = input.take() {
        let mut user = user.single_mut().unwrap();
        user.run_speed = command.run;
        user.walk_speed = command.walk;
        user.jog_speed = command.jog;
        input.reply_ok(format!(
            "run speed: {}, jog speed: {}, walk speed: {}",
            command.run, command.walk, command.jog
        ));
    }
}

// set jump height, gravity, max fall speed
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/jump")]
pub(crate) struct JumpCommand {
    jump_height: f32,
    run_jump_height: f32,
}

pub(crate) fn jump_cmd(mut input: ConsoleCommand<JumpCommand>, mut user: Query<&mut PrimaryUser>) {
    if let Some(Ok(command)) = input.take() {
        let mut user = user.single_mut().unwrap();
        user.jump_height = command.jump_height;
        user.run_jump_height = command.run_jump_height;
        input.reply_ok(format!(
            "jump height: {}, running jump height: {}",
            command.jump_height, command.run_jump_height
        ));
    }
}
