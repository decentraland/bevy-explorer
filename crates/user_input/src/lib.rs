pub mod camera;
pub mod dynamics;
pub mod player_input;

use bevy::prelude::*;

use common::{
    sets::SceneSets,
    structs::{PrimaryCamera, PrimaryUser},
};
use input_manager::should_accept_key;

use self::{
    camera::{update_camera, update_camera_position},
    dynamics::update_user_position,
    player_input::update_user_velocity,
};

// plugin to pass user input messages to the scene
pub struct UserInputPlugin;

impl Plugin for UserInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                update_user_velocity.run_if(should_accept_key),
                update_user_position,
                update_camera,
                update_camera_position,
            )
                .chain()
                .in_set(SceneSets::Input),
        );
        app.add_systems(Update, hide_player_in_first_person);
    }
}

fn hide_player_in_first_person(
    camera: Query<&GlobalTransform, With<PrimaryCamera>>,
    mut player: Query<(&GlobalTransform, &mut Visibility), With<PrimaryUser>>,
) {
    if let (Ok(cam_transform), Ok((player_transform, mut vis))) =
        (camera.get_single(), player.get_single_mut())
    {
        let distance =
            (cam_transform.translation() - player_transform.translation() - Vec3::Y * 1.81)
                .length();
        if distance < 0.5 && *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        } else if distance > 0.5 && *vis != Visibility::Inherited {
            *vis = Visibility::Inherited;
        }
    }
}
