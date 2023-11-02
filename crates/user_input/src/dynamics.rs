use bevy::{
    core::FrameCount,
    math::{DVec3, Vec3Swizzles},
    prelude::*,
};
use rapier3d_f64::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};

use common::{
    dynamics::{
        GRAVITY, MAX_CLIMBABLE_INCLINE, MAX_FALL_SPEED, MAX_STEP_HEIGHT, PLAYER_COLLIDER_OVERLAP,
        PLAYER_GROUND_THRESHOLD,
    },
    structs::PrimaryUser,
};

use avatar::AvatarDynamicState;
use scene_runner::{
    renderer_context::RendererSceneContext,
    update_world::mesh_collider::{GroundCollider, SceneColliderData},
    ContainingScene, OutOfWorld,
};

pub fn update_user_position(
    mut player: Query<
        (
            Entity,
            &PrimaryUser,
            &mut Transform,
            &mut AvatarDynamicState,
            &mut GroundCollider,
        ),
        Without<OutOfWorld>,
    >,
    mut scene_datas: Query<(
        &mut RendererSceneContext,
        &mut SceneColliderData,
        &GlobalTransform,
    )>,
    containing_scenes: ContainingScene,
    time: Res<Time>,
    _frame: Res<FrameCount>,
) {
    let Ok((user_ent, user, mut transform, mut dynamic_state, mut ground_collider)) =
        player.get_single_mut()
    else {
        return;
    };

    let dt = time.delta_seconds();
    // we apply half gravity before motion and half after to avoid (significant) max height difference due to frame rate
    let half_g_force = dt * GRAVITY * 0.5;
    if dynamic_state.velocity.y != 0.0 {
        dynamic_state.velocity.y -= half_g_force;
    }

    // rotate towards velocity vec
    let target_xz = dynamic_state.velocity.xz() * dt;
    if target_xz.length() > 0.0 {
        let target_rotation = Transform::default()
            .looking_at(dynamic_state.velocity * (Vec3::X + Vec3::Z), Vec3::Y)
            .rotation;

        transform.rotation = transform.rotation.lerp(target_rotation, dt * 10.0);
    }

    let mut target_motion = dynamic_state.velocity * dt;
    dynamic_state.ground_height = transform.translation.y;
    ground_collider.0 = None;

    // check containing scenes
    for scene in containing_scenes.get(user_ent) {
        let Ok((context, mut collider_data, _scene_transform)) = scene_datas.get_mut(scene) else {
            continue;
        };

        // setup physics controller
        let mut controller = KinematicCharacterController {
            offset: CharacterLength::Absolute(PLAYER_COLLIDER_OVERLAP as f64),
            slide: true,
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(MAX_STEP_HEIGHT as f64),
                min_width: CharacterLength::Relative(0.75),
                include_dynamic_bodies: true,
            }),
            max_slope_climb_angle: MAX_CLIMBABLE_INCLINE as f64,
            min_slope_slide_angle: MAX_CLIMBABLE_INCLINE as f64,
            snap_to_ground: Some(CharacterLength::Absolute(0.1)),
            ..Default::default()
        };

        // unset autostep when jumping
        if dynamic_state.velocity.y > 0.0 {
            controller.autostep = None;
        }

        // get allowed movement
        let eff_movement = collider_data.move_character(
            context.last_update_frame,
            transform.translation,
            target_motion,
            &controller,
        );

        target_motion = DVec3::from(eff_movement.translation).as_vec3();

        // calculate ground height
        if let Some((height, collider)) =
            collider_data.get_groundheight(context.last_update_frame, transform.translation)
        {
            if height < dynamic_state.ground_height {
                dynamic_state.ground_height = height;
                if height < PLAYER_GROUND_THRESHOLD {
                    ground_collider.0 = Some((scene, collider));
                }
            }
        }
    }

    debug!(
        "dynamics: {} -> {}",
        transform.translation,
        transform.translation + target_motion
    );

    transform.translation += target_motion;
    transform.translation.y = transform.translation.y.max(0.0);

    // update vertical velocity
    if dynamic_state.ground_height <= 0.0 || transform.translation.y == 0.0 {
        // on the floor, set vertical velocity to zero
        dynamic_state.velocity.y = dynamic_state.velocity.y.max(0.0);
    } else if target_motion.y.abs() < (0.5 * dynamic_state.velocity.y * dt).abs() {
        // vertical motion was blocked by something, use the effective motion
        dynamic_state.velocity.y = target_motion.y / dt - half_g_force;
    } else {
        dynamic_state.velocity.y -= half_g_force;
    }

    // cap fall speed
    dynamic_state.velocity.y = dynamic_state.velocity.y.max(-MAX_FALL_SPEED);

    // friction
    let mult = user.friction.recip().powf(dt);
    dynamic_state.velocity.x *= mult;
    dynamic_state.velocity.z *= mult;

    if dynamic_state.velocity.xz().length_squared() < 1e-3 {
        dynamic_state.velocity.x = 0.0;
        dynamic_state.velocity.z = 0.0;
    }
}
