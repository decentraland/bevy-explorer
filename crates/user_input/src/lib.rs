pub mod camera;
pub mod dynamics;
pub mod player_input;

use bevy::{
    ecs::query::Has,
    prelude::*,
    render::{camera::CameraUpdateSystem, view::RenderLayers},
    transform::TransformSystem,
};

use camera::update_cursor_lock;
use common::{
    anim_last_system,
    sets::SceneSets,
    structs::{CursorLocks, PrimaryCamera, PrimaryUser, PRIMARY_AVATAR_LIGHT_LAYER_INDEX},
};
use console::DoAddConsoleCommand;
use dynamics::{
    jump_cmd, no_clip, speed_cmd, JumpCommand, NoClipCommand, SpeedCommand, UserClipping,
};
use scene_runner::{
    update_scene::pointer_lock::update_pointer_lock,
    update_world::{
        avatar_modifier_area::PlayerModifiers,
        gltf_container::GltfLinkSet,
        transform_and_parent::{parent_position_sync, AvatarAttachStage, SceneProxyStage},
    },
    OutOfWorld,
};
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
            (update_user_velocity, update_camera)
                .chain()
                .in_set(SceneSets::Input)
                .after(update_pointer_lock),
        );
        app.add_systems(Update, manage_player_visibility.in_set(SceneSets::PostLoop));
        app.add_systems(
            PostUpdate,
            (
                update_user_position
                    .after(anim_last_system!())
                    .after(GltfLinkSet)
                    .before(parent_position_sync::<AvatarAttachStage>)
                    .before(parent_position_sync::<SceneProxyStage>)
                    .before(TransformSystem::TransformPropagate),
                update_camera_position
                    .after(anim_last_system!())
                    .after(GltfLinkSet)
                    .after(update_user_position)
                    .after(parent_position_sync::<AvatarAttachStage>)
                    .before(parent_position_sync::<SceneProxyStage>)
                    .before(TransformSystem::TransformPropagate)
                    .before(CameraUpdateSystem)
                    .before(update_system_tween),
                update_cursor_lock.after(update_camera_position),
            ),
        );
        app.insert_resource(UserClipping(true))
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
            &mut propagate::Propagate<RenderLayers>,
            Has<OutOfWorld>,
            &PlayerModifiers,
        ),
        With<PrimaryUser>,
    >,
) {
    if let (Ok(cam_transform), Ok((player_transform, mut vis, mut layers, is_oow, modifiers))) =
        (camera.get_single(), player.get_single_mut())
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
