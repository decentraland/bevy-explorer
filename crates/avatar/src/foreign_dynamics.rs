use bevy::prelude::*;

use common::{
    structs::{
        avatar_tilt_quat, AvatarDynamicState, MoveKind, SceneDrivenAnim,
        SceneDrivenAnimationRequest,
    },
    util::QuatNormalizeExt,
};

use comms::global_crdt::{ForeignPlayer, PlayerPositionEvent, PlayerSceneAnimEvent};
use dcl_component::{transform_and_parent::DclTransformAndParent, SceneEntityId};
use scene_runner::{update_world::mesh_collider::SceneColliderData, ContainingScene};

/// Largest forward timestamp jump accepted as normal progress; a bigger leap is treated as a
/// garbage stamp. Generous (the server clock can gap during a brief stall) but bounded so a single
/// absurd-future stamp can't lock out every subsequent real update.
const TIMESTAMP_FORWARD_LIMIT_SECS: f32 = 60.0;
/// A backward jump beyond this reads as a wrap (`movement_compressed`'s range) or a sender restart,
/// so we re-sync to it instead of freezing; smaller backward steps are reordered datagrams.
const TIMESTAMP_RESET_SECS: f32 = 60.0;

pub struct PlayerMovementPlugin;

impl Plugin for PlayerMovementPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                update_foreign_user_target_position,
                update_foreign_scene_anim,
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
    update_freq: f32,
    grounded: Option<bool>,
    // Jump / DoubleJump / Glide inferred from the rfc4::Movement packet. Applied to the
    // avatar's move_kind so the velocity picker can drive jump_time (Jump) or select the
    // matching emote (DoubleJump / Glide); None resets a previously-applied remote state
    // back to Idle so the picker reclaims.
    remote_move_kind: Option<MoveKind>,
}

/// Pending scene-driven animation for a foreign player, fed by [`PlayerSceneAnimEvent`] (which rides
/// LiveKit, separate from the Pulse position stream) and applied in sync with the position packet
/// whose motion it belongs to, so the clip starts/stops with the visible avatar. Ordering/dedup is
/// already done in `global_crdt` (by the sender's sequence), so events arrive here in order.
#[derive(Component, Default)]
struct PendingSceneAnim {
    /// The animation awaiting its movement trigger (`None` = clear). Fired in sync with the position
    /// packet whose motion it belongs to: the next packet arms it ([`fire_on_next_movement`]) and the
    /// one after fires it, delaying the clip by ~one packet interval so it lands with the interpolated
    /// (lagged) body rather than ahead of it. As a backstop — a static sender stops sending packets —
    /// it also fires once `update_freq` elapses from receipt. `None` once applied.
    pending: Option<Option<SceneDrivenAnimationRequest>>,
    /// Local time the pending anim was received, for the `update_freq` backstop.
    received_at: f32,
    /// Whether the next movement packet should fire `pending` (vs. merely arm it; see `pending`).
    fire_on_next_movement: bool,
    /// Render-only lean from the latest scene-anim event, composed onto the interpolated yaw each
    /// frame. Tracked continuously (not gated on the apply delay like the clip) since it's a
    /// pose overlay, not a transition. `IDENTITY` = upright.
    tilt: Quat,
}

fn update_foreign_user_target_position(
    mut commands: Commands,
    mut move_events: EventReader<PlayerPositionEvent>,
    mut players: Query<(
        &ForeignPlayer,
        Option<&mut PlayerTargetPosition>,
        Option<&mut PendingSceneAnim>,
    )>,
) {
    for ev in move_events.read() {
        let dcl_transform = DclTransformAndParent {
            translation: ev.translation,
            rotation: ev.rotation,
            scale: Vec3::ONE,
            parent: SceneEntityId::WORLD_ORIGIN,
        };

        let bevy_trans = dcl_transform.to_bevy_transform();

        if let Ok((_player, maybe_pos, maybe_pending)) = players.get_mut(ev.player) {
            if let Some(mut pos) = maybe_pos {
                // The server tick is monotonic seconds: accept a strictly-newer stamp within a sane
                // forward window, treat a large backward jump as a sender restart (re-sync rather
                // than freeze), and reject small backward steps as reordered/duplicate datagrams.
                let is_valid = pos.timestamp.is_none_or(|pts| {
                    let forward = ev.timestamp - pts;
                    (forward > 0.0 && forward < TIMESTAMP_FORWARD_LIMIT_SECS)
                        || forward < -TIMESTAMP_RESET_SECS
                });
                if is_valid {
                    const LAG_DECAY_SECS: f32 = 1.5;
                    let delta = ev.time - pos.time;
                    let update_freq = LAG_DECAY_SECS
                        / ((LAG_DECAY_SECS - delta).max(0.0) / pos.update_freq
                            + (LAG_DECAY_SECS / delta).min(1.0));
                    *pos = PlayerTargetPosition {
                        time: ev.time,
                        timestamp: Some(ev.timestamp),
                        velocity: Some(ev.velocity),
                        translation: bevy_trans.translation,
                        rotation: bevy_trans.rotation.normalize_or_identity(),
                        update_freq,
                        grounded: ev.grounded,
                        remote_move_kind: ev.remote_move_kind,
                    };
                    // Movement trigger: the first packet after an anim arrives arms it; the next one
                    // fires it, in sync with the motion (see `PendingSceneAnim`).
                    if let Some(mut pending) = maybe_pending {
                        if pending.pending.is_some() {
                            if pending.fire_on_next_movement {
                                let anim = pending.pending.take().unwrap();
                                commands
                                    .entity(ev.player)
                                    .try_insert(SceneDrivenAnim { active: anim });
                            } else {
                                pending.fire_on_next_movement = true;
                            }
                        }
                    }
                } else {
                    debug!(
                        "invalid timestamp: ev: {}, last: {:?}",
                        ev.timestamp, pos.timestamp
                    );
                }
            } else {
                commands.entity(ev.player).try_insert((
                    PlayerTargetPosition {
                        time: ev.time,
                        timestamp: Some(ev.timestamp),
                        velocity: Some(ev.velocity),
                        translation: bevy_trans.translation,
                        rotation: bevy_trans.rotation,
                        update_freq: 0.01,
                        grounded: ev.grounded,
                        remote_move_kind: ev.remote_move_kind,
                    },
                    AvatarDynamicState::default(),
                    PendingSceneAnim::default(),
                ));
            }
        }
    }
}

/// Stash each incoming [`PlayerSceneAnimEvent`] as the player's pending animation, to be applied by
/// the movement trigger (see [`PendingSceneAnim`]). Events arrive in order; if a newer one lands
/// before the previous has fired, the previous is applied first (apply-before-overwrite) so it
/// isn't silently dropped.
fn update_foreign_scene_anim(
    mut commands: Commands,
    mut anim_events: EventReader<PlayerSceneAnimEvent>,
    mut players: Query<&mut PendingSceneAnim>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs();
    for ev in anim_events.read() {
        let tilt = avatar_tilt_quat(ev.tilt.0, ev.tilt.1);
        // The next movement packet arms the clip and the one after fires it (see `PendingSceneAnim`),
        // delaying the anim by ~one packet interval so it lands in sync with the interpolated body.
        if let Ok(mut slot) = players.get_mut(ev.player) {
            if let Some(anim) = slot.pending.take() {
                commands
                    .entity(ev.player)
                    .try_insert(SceneDrivenAnim { active: anim });
            }
            slot.pending = Some(ev.anim.clone());
            slot.received_at = now;
            slot.fire_on_next_movement = false;
            slot.tilt = tilt;
        } else {
            commands.entity(ev.player).try_insert(PendingSceneAnim {
                pending: Some(ev.anim.clone()),
                received_at: now,
                fire_on_next_movement: false,
                tilt,
            });
        }
    }
}

fn update_foreign_user_actual_position(
    mut commands: Commands,
    mut avatars: Query<(
        Entity,
        &PlayerTargetPosition,
        &mut Transform,
        &mut AvatarDynamicState,
        Option<&mut PendingSceneAnim>,
    )>,
    mut scene_datas: Query<(&mut SceneColliderData, &GlobalTransform)>,
    containing_scene: ContainingScene,
    time: Res<Time>,
) {
    for (foreign_ent, target, mut actual, mut dynamic_state, maybe_pending) in avatars.iter_mut() {
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

        // Compose the render-only lean onto the (yaw-only) target before interpolating, so the
        // existing rotation lerp carries the tilt as part of the rotation — tilt changes blend
        // smoothly and the lean never decays out of the yaw interpolation. `target.rotation`
        // itself stays yaw-only (that's the value scenes read via the CRDT transform).
        let tilt = maybe_pending
            .as_deref()
            .map(|p| p.tilt)
            .unwrap_or(Quat::IDENTITY);
        let target_rotation = target.rotation * tilt;
        if turn_time <= 0.0 {
            actual.rotation = target_rotation;
        } else {
            let turn_fraction = (time.delta_secs() / turn_time).min(1.0);
            actual.rotation = actual.rotation.lerp(target_rotation, turn_fraction);
        }

        // Apply the remote-derived move_kind. Jump drives jump_time (the velocity picker
        // reads it to size the jump clip); DoubleJump / Glide select the matching emote.
        // None clears a previously-applied DoubleJump / Glide so the picker reclaims.
        match target.remote_move_kind {
            Some(MoveKind::Jump) => {
                if dynamic_state.jump_time == -1.0 {
                    dynamic_state.jump_time = time.elapsed_secs();
                }
            }
            Some(k) => dynamic_state.move_kind = k,
            None => {
                if matches!(
                    dynamic_state.move_kind,
                    MoveKind::DoubleJump | MoveKind::Glide
                ) {
                    dynamic_state.move_kind = MoveKind::Idle;
                }
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
                    if let Ok((mut collider_data, _scene_transform)) = scene_datas.get_mut(scene) {
                        if let Some(ground_height) =
                            collider_data.get_ground(actual.translation).map(|(h, _)| h)
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

        // Backstop: the movement trigger fires the pending anim in sync with the motion, but a
        // static sender stops sending packets — so if `update_freq` elapses from receipt with no
        // trigger, apply it anyway rather than strand it.
        if let Some(mut pending) = maybe_pending {
            if pending.pending.is_some()
                && time.elapsed_secs() >= pending.received_at + target.update_freq
            {
                let anim = pending.pending.take().unwrap();
                commands
                    .entity(foreign_ent)
                    .try_insert(SceneDrivenAnim { active: anim });
            }
        }
    }
}
