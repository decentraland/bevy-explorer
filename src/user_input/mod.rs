pub mod camera;
pub mod dynamics;
pub mod player_input;

use bevy::prelude::*;

use crate::{
    common::{PrimaryCamera, PrimaryUser},
    input_manager::AcceptInput,
    scene_runner::SceneSets,
};

use self::{
    camera::{update_camera, update_camera_position},
    dynamics::update_user_position,
    player_input::update_user_velocity,
};

// plugin to pass user input messages to the scene
pub struct UserInputPlugin;

pub fn should_accept_input(should_accept: Res<AcceptInput>) -> bool {
    should_accept.0
}

impl Plugin for UserInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            (
                update_camera.run_if(should_accept_input),
                update_camera_position,
                update_user_velocity.run_if(should_accept_input),
                update_user_position,
            )
                .chain()
                .in_set(SceneSets::Input),
        );
        app.add_system(hide_player_in_first_person);
    }
}

fn hide_player_in_first_person(
    camera: Query<&PrimaryCamera>,
    mut player: Query<&mut Visibility, With<PrimaryUser>>,
) {
    if let (Ok(cam), Ok(mut vis)) = (camera.get_single(), player.get_single_mut()) {
        if cam.distance < 0.1 && *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        } else if cam.distance > 0.1 && *vis != Visibility::Inherited {
            *vis = Visibility::Inherited;
        }
    }
}
