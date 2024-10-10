pub mod camera;
pub mod dynamics;
pub mod player_input;

use bevy::{
    ecs::query::Has,
    prelude::*,
    render::{camera::CameraUpdateSystem, view::RenderLayers},
    transform::TransformSystem,
};

use common::{
    anim_last_system,
    sets::SceneSets,
    structs::{PrimaryCamera, PrimaryUser, PRIMARY_AVATAR_LIGHT_LAYER},
};
use console::DoAddConsoleCommand;
use dynamics::{
    jump_cmd, no_clip, speed_cmd, JumpCommand, NoClipCommand, SpeedCommand, UserClipping,
};
use input_manager::should_accept_key;
use scene_runner::{
    update_world::{
        avatar_modifier_area::PlayerModifiers,
        gltf_container::GltfLinkSet,
        transform_and_parent::{parent_position_sync, AvatarAttachStage, SceneProxyStage},
    },
    OutOfWorld,
};

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
                    .before(CameraUpdateSystem),
            ),
        );
        app.insert_resource(UserClipping(true));
        app.add_console_command::<NoClipCommand, _>(no_clip);
        app.add_console_command::<SpeedCommand, _>(speed_cmd);
        app.add_console_command::<JumpCommand, _>(jump_cmd);
    }
}

#[allow(clippy::type_complexity)]
fn manage_player_visibility(
    mut commands: Commands,
    camera: Query<&GlobalTransform, With<PrimaryCamera>>,
    mut player: Query<
        (
            Entity,
            &GlobalTransform,
            &mut Visibility,
            Has<OutOfWorld>,
            &PlayerModifiers,
        ),
        With<PrimaryUser>,
    >,
    children: Query<&Children>,
    spotlights: Query<(), With<SpotLight>>,
) {
    if let (Ok(cam_transform), Ok((player, player_transform, mut vis, is_oow, modifiers))) =
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
            for child in children.iter_descendants(player) {
                // don't retarget the profile texture spotlight which we've attached to the avatar directly
                if spotlights.get(child).is_ok() {
                    continue;
                }
                if let Some(mut commands) = commands.get_entity(child) {
                    commands.insert(PRIMARY_AVATAR_LIGHT_LAYER);
                }
            }
        } else {
            for child in children.iter_descendants(player) {
                if spotlights.get(child).is_ok() {
                    continue;
                }
                if let Some(mut commands) = commands.get_entity(child) {
                    commands.insert(RenderLayers::default());
                }
            }
        }
    }
}
