pub mod camera;
pub mod dynamics;
pub mod player_input;

use bevy::{ecs::query::Has, prelude::*};

use common::{
    sets::SceneSets,
    structs::{PrimaryCamera, PrimaryUser},
};
use console::DoAddConsoleCommand;
use dynamics::{
    jump_cmd, no_clip, speed_cmd, JumpCommand, NoClipCommand, SpeedCommand, UserClipping,
};
use input_manager::should_accept_key;
use scene_runner::{update_world::avatar_modifier_area::PlayerModifiers, OutOfWorld};
use tween::update_system_tween;

use self::{
    camera::{update_camera, update_camera_position},
    dynamics::update_user_position,
    player_input::update_user_velocity,
};

static TRANSITION_TIME: f32 = 0.5;

// plugin to pass user input messages to the scene
pub struct UserInputPlugin;

impl Plugin for UserInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                update_user_velocity.run_if(should_accept_key),
                update_camera,
            )
                .chain()
                .in_set(SceneSets::Input),
        );
        app.add_systems(
            Update,
            (
                manage_player_visibility,
                update_user_position
                    .after(tween::update_tween)
                    .after(restricted_actions::move_player),
                update_camera_position
                    .after(restricted_actions::move_camera)
                    .before(update_system_tween),
            )
                .chain()
                .in_set(SceneSets::PostLoop),
        );
        app.insert_resource(UserClipping(true));
        app.init_resource::<CursorLocked>();
        app.add_console_command::<NoClipCommand, _>(no_clip);
        app.add_console_command::<SpeedCommand, _>(speed_cmd);
        app.add_console_command::<JumpCommand, _>(jump_cmd);
    }
}

#[derive(Resource, Default)]
pub struct CursorLocked(pub bool);

fn manage_player_visibility(
    camera: Query<&GlobalTransform, With<PrimaryCamera>>,
    mut player: Query<
        (
            &GlobalTransform,
            &mut Visibility,
            Has<OutOfWorld>,
            &PlayerModifiers,
        ),
        With<PrimaryUser>,
    >,
) {
    if let (Ok(cam_transform), Ok((player_transform, mut vis, is_oow, modifiers))) =
        (camera.get_single(), player.get_single_mut())
    {
        let distance =
            (cam_transform.translation() - player_transform.translation() - Vec3::Y * 1.81)
                .length();

        #[allow(clippy::collapsible_else_if)]
        if is_oow || modifiers.hide || distance < 0.5 {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
        } else {
            if *vis != Visibility::Inherited {
                *vis = Visibility::Inherited;
            }
        }
    }
}
