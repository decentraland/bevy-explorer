use bevy::prelude::*;

use common::{structs::AvatarDynamicState, util::QuatNormalizeExt};

use comms::{
    global_crdt::{ForeignPlayer, PlayerPositionEvent},
    movement_compressed::Temporal,
};
use dcl_component::{transform_and_parent::DclTransformAndParent, SceneEntityId};
use scene_runner::{
    renderer_context::RendererSceneContext, update_world::mesh_collider::SceneColliderData,
    ContainingScene,
};

pub struct PlayerMovementPlugin;

impl Plugin for PlayerMovementPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                update_foreign_user_target_position,
                update_foreign_user_actual_position,
            )
                .chain(),
        );
    }
}

#[derive(Component)]
struct PlayerTargetPosition {
    time: f32,
    timestamp: Option<f32>,
    velocity: Option<Vec3>,
    translation: Vec3,
    rotation: Quat,
    index: Option<u32>,
    update_freq: f32,
    grounded: Option<bool>,
    jumping: Option<bool>,
}

fn update_foreign_user_target_position(
    mut commands: Commands,
    mut move_events: EventReader<PlayerPositionEvent>,
    mut players: Query<(&ForeignPlayer, Option<&mut PlayerTargetPosition>)>,
) {
    for ev in move_events.read() {
        let dcl_transform = DclTransformAndParent {
            translation: ev.translation,
            rotation: ev.rotation,
            scale: Vec3::ONE,
            parent: SceneEntityId::WORLD_ORIGIN,
        };

        let bevy_trans = dcl_transform.to_bevy_transform();

        if let Ok((_player, maybe_pos)) = players.get_mut(ev.player) {
            if let Some(mut pos) = maybe_pos {
                let mut is_valid = false;
                if ev.index.is_some_and(|eix| {
                    pos.timestamp.is_none() && pos.index.is_none_or(|pix| eix > pix)
                }) {
                    // we're using only position based updates, and this index is higher than previous
                    is_valid = true;
                }
                if let Some(timestamp) = ev.timestamp {
                    if pos.timestamp.is_none_or(|pts| {
                        let threshold = Temporal::TIMESTAMP_MAX * 0.25;
                        (timestamp > pts && timestamp < pts + threshold)
                            || (timestamp + Temporal::TIMESTAMP_MAX > pts
                                && timestamp + Temporal::TIMESTAMP_MAX < pts + threshold)
                    }) {
                        // we're using movement compressed, and this is a "later" timestamp
                        // TODO: we can avoid using out-of-order messages as well by checking threshold vs prev
                        is_valid = true;
                    } else {
                        debug!(
                            "invalid timestamp: ev: {:?}, last: {:?}",
                            timestamp, pos.timestamp
                        );
                    }
                }

                if is_valid {
                    const LAG_DECAY_SECS: f32 = 1.5;
                    let delta = ev.time - pos.time;
                    let update_freq = LAG_DECAY_SECS
                        / ((LAG_DECAY_SECS - delta).max(0.0) / pos.update_freq
                            + (LAG_DECAY_SECS / delta).min(1.0));
                    *pos = PlayerTargetPosition {
                        time: ev.time,
                        timestamp: ev.timestamp,
                        velocity: ev.velocity,
                        translation: bevy_trans.translation,
                        rotation: bevy_trans.rotation.normalize_or_identity(),
                        index: ev.index,
                        update_freq,
                        grounded: ev.grounded,
                        jumping: ev.jumping,
                    }
                }
            } else {
                commands.entity(ev.player).try_insert((
                    PlayerTargetPosition {
                        time: ev.time,
                        timestamp: ev.timestamp,
                        velocity: ev.velocity,
                        translation: bevy_trans.translation,
                        rotation: bevy_trans.rotation,
                        index: ev.index,
                        update_freq: 0.01,
                        grounded: ev.grounded,
                        jumping: ev.jumping,
                    },
                    AvatarDynamicState::default(),
                ));
            }
        }
    }
}

fn update_foreign_user_actual_position(
    mut avatars: Query<(
        Entity,
        &PlayerTargetPosition,
        &mut Transform,
        &mut AvatarDynamicState,
    )>,
    mut scene_datas: Query<(
        &mut RendererSceneContext,
        &mut SceneColliderData,
        &GlobalTransform,
    )>,
    containing_scene: ContainingScene,
    time: Res<Time>,
) {
    for (foreign_ent, target, mut actual, mut dynamic_state) in avatars.iter_mut() {
        debug!(
            "positioning foreign {foreign_ent:?}, target {}, current {}",
            target.translation, actual.translation
        );

        if (actual.translation - target.translation).length() > 125.0 {
            actual.translation = target.translation;
            dynamic_state.velocity = target.velocity.unwrap_or_default();
        }

        let turn_time;
        if let Some(velocity) = target.velocity {
            let t0 = time.elapsed_secs();
            let t1 = target.time + target.update_freq;

            if t1 < t0 + time.delta_secs() * 2.0 {
                actual.translation = target.translation + velocity * (t0 - t1);
                dynamic_state.velocity = velocity;
                turn_time = 0.0;
            } else {
                // use some extrapolation but slow it down so we don't overcompensate for missed packets
                let dt = if (t1 - t0) < 1.0 {
                    t1 - t0
                } else {
                    (t1 - t0).sqrt()
                };

                let p0 = actual.translation;
                let p1 = target.translation;
                let dp = p1 - p0;

                let v_req = dp / dt;
                let v0 = dynamic_state.velocity;
                let v1 = velocity;

                let speed_without_middle = (v0 + v1) * 0.25;
                let req_middle = (v_req - speed_without_middle) * 2.0;
                dynamic_state.velocity +=
                    (req_middle - v0) * (time.delta_secs() / (dt * 0.5)).min(1.0);
                turn_time = dt.max(0.0);
                actual.translation += dynamic_state.velocity * time.delta_secs();
            }
        } else {
            // arrive at target position by time + 0.5
            let walk_time_left = target.time + 0.5 - time.elapsed_secs();
            if walk_time_left <= 0.0 {
                actual.translation = target.translation;
                dynamic_state.velocity = Vec3::ZERO;
            } else {
                let walk_fraction = (time.delta_secs() / walk_time_left).min(1.0);
                let delta = (target.translation - actual.translation) * walk_fraction;
                dynamic_state.velocity = delta / time.delta_secs();
                actual.translation += dynamic_state.velocity * time.delta_secs();
            }
            turn_time = target.time + 0.2 - time.elapsed_secs();
        }

        if turn_time <= 0.0 {
            actual.rotation = target.rotation;
        } else {
            let turn_fraction = (time.delta_secs() / turn_time).min(1.0);
            actual.rotation = actual.rotation.lerp(target.rotation, turn_fraction);
        }

        if let Some(jumping) = target.jumping {
            if jumping && dynamic_state.jump_time == -1.0 {
                dynamic_state.jump_time = time.elapsed_secs();
            }
        }

        if let Some(grounded) = target.grounded {
            dynamic_state.ground_height = if grounded { 0.0 } else { 1.0 };
            dynamic_state.jump_time = -1.0;
        } else {
            // update ground height
            dynamic_state.ground_height = actual.translation.y;
            // get containing scene
            containing_scene
                .get(foreign_ent)
                .into_iter()
                .for_each(|scene| {
                    if let Ok((context, mut collider_data, _scene_transform)) =
                        scene_datas.get_mut(scene)
                    {
                        if let Some(ground_height) = collider_data
                            .get_ground(context.last_update_frame, actual.translation)
                            .map(|(h, _)| h)
                        {
                            dynamic_state.ground_height =
                                dynamic_state.ground_height.min(ground_height);
                        }
                    }
                });

            // fall
            if actual.translation.y > target.translation.y && dynamic_state.ground_height > 0.0 {
                let updated_y = target
                    .translation
                    .y
                    .max(actual.translation.y - 15.0 * time.delta_secs())
                    .max(actual.translation.y - dynamic_state.ground_height);

                dynamic_state.ground_height += updated_y - actual.translation.y;
                actual.translation.y = updated_y;
            }
        }

        dynamic_state.force = dynamic_state.velocity.xz();
    }
}
