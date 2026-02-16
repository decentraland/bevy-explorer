// ensuring that player moves with a ground collider (platform) is tricky. to ensure consistency we consider the process starting from the scene loop :

// * collider gts = entity gts = entity transforms
// * player gt = player transform / no penetration with colliders

// scene loop
// - update entity transforms (so entity transform != global transform != collider transform)
// - leave collider transforms in old state

// dynamics - setup
// - use last ground-collider transform
// - manually calculate entity global transform
// - record difference
// - apply difference to ground-collider collider transform, ignoring player (so ground-collider entity transform == collider transform)

// dynamics - update player
// - in: player intrinsic velocity
// - in: ground-collider difference, (we don't currently but could clamp to max fall speed (based on scene elapsed for last tick? absolute? both?))
// - clamp player intrinsic velocity by ground-collider (input velocity should still be constrained by what you are standing on)
// - clamp (player intrinsic velocity + gc difference) by all colliders except ground-collider
// - use clamped (motion + difference) to update position and intrinsic velocity

// postupdate

// - [bevy] animations
// - [gltf_container] sync gltf nodes
// ** all collidable (non-attached items) are in final positions
// - [here] dynamics - update player
// - [transform_and_parent] - update ParentPositionSync<AvatarAttachStage>
// - [user_input] - camera position
// - [transform_and_parent] - update ParentPositionSync<SceneProxyStage>
// - update global transforms
// - render (player position is updated for gc/platform only, all collider entity global transforms are updated to their new positions; collider transforms are not but this doesn't affect rendering)
// note player may intersect non-ground colliders (or weirdly rotating gcs), just for rendering

// postinit
// - update collider transforms (ground-collider won't change), checking for push/pen on player
// - ground-collider can't push player as transform is up to date already
// - ground-collider (and others) depenetrate player if intersecting

// * collider gts = entity gts = entity transforms
// * player gt = player transform / no penetration with colliders

// and back to scene loop

use bevy::{
    diagnostic::FrameCount,
    math::{Vec3, Vec3Swizzles},
    prelude::*,
};
use bevy_console::ConsoleCommand;

use common::{
    dynamics::{
        MAX_CLIMBABLE_INCLINE, MAX_STEP_HEIGHT, PLAYER_COLLIDER_OVERLAP, PLAYER_COLLIDER_RADIUS,
        PLAYER_GROUND_THRESHOLD,
    },
    structs::{AvatarDynamicState, PlayerModifiers, PrimaryUser},
};

use scene_runner::{
    renderer_context::RendererSceneContext,
    update_world::{
        avatar_movement::GroundCollider,
        mesh_collider::{ColliderId, SceneColliderData},
    },
    ContainingScene, OutOfWorld,
};

#[derive(Resource)]
pub struct UserClipping(pub bool);

const TICK_TIME: f32 = 1.0 / 720.0;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update_user_position(
) {

}

// turn clipping on/off
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/idnoclip")]
pub(crate) struct NoClipCommand {
    clip: Option<bool>,
}

pub(crate) fn no_clip(mut input: ConsoleCommand<NoClipCommand>, mut clip: ResMut<UserClipping>) {
    if let Some(Ok(command)) = input.take() {
        let new_state = command.clip.unwrap_or(!clip.0);
        clip.0 = new_state;
        input.reply_ok(format!("clipping set to {}", clip.0));
    }
}

// set speed and friction
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/speed")]
pub(crate) struct SpeedCommand {
    run: f32,
    friction: f32,
}

pub(crate) fn speed_cmd(
    mut input: ConsoleCommand<SpeedCommand>,
    mut user: Query<&mut PrimaryUser>,
) {
    if let Some(Ok(command)) = input.take() {
        let mut user = user.single_mut().unwrap();
        user.run_speed = command.run;
        user.friction = command.friction;
        input.reply_ok(format!(
            "run speed: {}, friction: {}",
            command.run, command.friction
        ));
    }
}

// set jump height, gravity, max fall speed
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/jump")]
pub(crate) struct JumpCommand {
    jump_height: f32,
    gravity: f32,
    fall_speed: f32,
}

pub(crate) fn jump_cmd(mut input: ConsoleCommand<JumpCommand>, mut user: Query<&mut PrimaryUser>) {
    if let Some(Ok(command)) = input.take() {
        let mut user = user.single_mut().unwrap();
        user.jump_height = command.jump_height;
        user.gravity = -command.gravity;
        user.fall_speed = -command.fall_speed;
        input.reply_ok(format!(
            "jump height: {}, gravity: -{}, max fallspeed: -{}",
            command.jump_height, command.gravity, command.fall_speed
        ));
    }
}
