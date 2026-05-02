use bevy::prelude::*;
use common::{
    sets::PostUpdateSets,
    structs::{PointAtSync, PrimaryUser},
};
use dcl_component::transform_and_parent::DclTranslation;

use crate::{two_bone_ik::solve_two_bone, AvatarShape};

pub struct PointAtIkPlugin;

impl Plugin for PointAtIkPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (cache_point_at_rig, apply_point_at_ik)
                .chain()
                .in_set(PostUpdateSets::InverseKinematics),
        );
    }
}

#[derive(Component)]
pub struct PointAtIkRig {
    /// Upper-arm bone (root of the 2-bone IK chain).
    pub upper: Entity,
    /// Forearm bone (mid).
    pub lower: Entity,
    /// Hand bone (end-effector).
    pub hand: Entity,
    /// Finger bones with the per-bone curl angle (degrees) and local-space
    /// rotation axis to apply at full IK weight. Negative angles straighten
    /// — the index finger gets a small back-bend so it doesn't read as a
    /// relaxed claw. Most fingers curl on local Z but the thumb is laid
    /// out differently in the rig and needs its own axis. Bones missing
    /// from the rig drop out silently.
    curled_fingers: Vec<(Entity, f32, Vec3)>,
    /// Smoothed IK weight. Ramps toward `ENGAGED_WEIGHT_TARGET` (>1.0) while
    /// engaged and 0 otherwise; we clamp to `[0, 1]` for display, so the
    /// "above 1" reserve acts as a time-based hysteresis dampener — brief
    /// excursions out of the cone bleed off the reserve before the displayed
    /// weight starts dropping.
    weight: f32,
    /// Smoothed gaze target in bevy world space. Lerps toward the latest
    /// target each frame so that re-aiming during a held drag (or a fresh
    /// click while the previous gesture is still ramping out) glides the
    /// hand to the new spot instead of snapping. `None` while idle; init
    /// to the current target on the first pointing frame, cleared once the
    /// weight finishes ramping back to zero.
    smoothed_target: Option<Vec3>,
    /// Hysteresis flag: once the gaze deviates past the trigger threshold the
    /// body commits to a full rotation toward the target rather than partial
    /// catching-up. Cleared once we land within the finish threshold.
    is_rotating: bool,
    /// Our own copy of the avatar's bevy world yaw (radians). The avatar
    /// movement system rewrites `Transform.rotation` from a stored
    /// orientation each frame, and movement-scene round-trips can lag, so
    /// reading the transform isn't a reliable source for our slew. We hold
    /// our own state from the first frame of a local pointing gesture and
    /// override the transform every frame while it's `Some`. Cleared when
    /// the gesture ends.
    body_yaw: Option<f32>,
}

/// Time constant for the weight ramp. Combined with `ENGAGED_WEIGHT_TARGET`
/// = 3.0 this gives ~165ms ramp-in to display=1.0 and ~440ms of "above 1.0"
/// reserve that has to bleed off after disengagement before the displayed
/// weight starts dropping.
const WEIGHT_TAU: f32 = 0.4;

/// Time constant for the gaze-target lerp. Same feel as the weight ramp:
/// re-aiming visibly slews the hand rather than snapping.
const TARGET_TAU: f32 = 0.15;

/// Hint direction in world space for which side of the swing plane the elbow
/// bends to. Pointing "down" sends the elbow under the shoulder for natural
/// forward-arm gestures.
const POLE_DIR_WORLD: Vec3 = Vec3::NEG_Y;

/// Yaw deviation (degrees, body forward → target, signed) past which the
/// body commits to rotating to face the target. Asymmetric — values
/// derived empirically from observing Unity's behavior (~+17° / -53°);
/// settings asset has presumably moved since the source we read.
/// Once triggered the body still slews fully to face the target; only the
/// trigger angle differs per side.
const BODY_ROTATE_TRIGGER_LEFT_DEG: f32 = 17.5;
const BODY_ROTATE_TRIGGER_RIGHT_DEG: f32 = 53.0;

/// Slack around the cone edges to absorb rounding-error mismatches between
/// clients. Display the arm in a slightly wider cone (so we don't go silent
/// while another client is showing it), and trigger rotation in a slightly
/// narrower cone (so we don't sit still while another client thinks we
/// should be turning).
const CONE_TOLERANCE_DEG: f32 = 2.0;

/// Weight target while engaged. Driving above 1.0 (clamped on display) gives
/// a time-based dampener: brief excursions out of the display cone don't
/// pull the displayed weight below 1.0 because we have headroom to burn
/// before crossing it. Acts like hysteresis without an explicit flag.
const ENGAGED_WEIGHT_TARGET: f32 = 3.0;

/// Half-space limits on the upper-arm direction (shoulder→elbow) in the
/// avatar's body-local frame. Applied after the 2-bone IK solves: if the
/// solver wants to swing the upper arm such that its direction crosses
/// these limits, we project the direction onto the boundary plane and
/// rebuild the shoulder swing rotation. Keeps the right arm out of the
/// torso when the IK target sits in the rear half-space (smoothing-
/// through-body during a body rotation) or right under the body center
/// (click at feet, where target is at the avatar entity origin).
///
/// `MIN_X` keeps the upper arm from crossing the body's centerline to the
/// left; `MAX_Z` keeps it from rotating behind the body. Both are direction
/// components on a unit vector, in body-local frame (mesh forward = -Z,
/// mesh right = +X).
const UPPER_ARM_MIN_X_LOCAL: f32 = 0.1;
const UPPER_ARM_MAX_Z_LOCAL: f32 = 0.2;

/// Once rotating, we keep going until we're within this many degrees of
/// the target — gives a deliberate "turn-and-point" feel rather than a
/// stuttery catch-up.
const BODY_ROTATE_FINISH_DEG: f32 = 1.0;

/// Slew rate for body rotation. Matches Unity's PointAt rotation speed.
const BODY_ROTATE_DEG_PER_SEC: f32 = 250.0;

/// Per-bone curl applied at full IK weight: angle (degrees, positive curls
/// into a fist, negative straightens) around a per-bone local-space axis.
/// Most segments curl on local Z, but the thumb is laid out differently in
/// the rig — adjust its axis here without affecting the rest. Mixamo-style
/// rigs expose these as `avatar_righthand{finger}{1,2,3}`; bones not present
/// in the rig drop out silently.
const FINGER_CURLS: &[(&str, f32, Vec3)] = &[
    ("avatar_righthandthumb1", 0.0, Vec3::Z),
    ("avatar_righthandthumb2", -60.0, Vec3::X),
    ("avatar_righthandthumb3", -60.0, Vec3::X),
    ("avatar_righthandthumb4", -60.0, Vec3::X),
    ("avatar_righthandindex1", -10.0, Vec3::Z),
    ("avatar_righthandindex2", -10.0, Vec3::Z),
    ("avatar_righthandindex3", -10.0, Vec3::Z),
    ("avatar_righthandmiddle1", 80.0, Vec3::Z),
    ("avatar_righthandmiddle2", 80.0, Vec3::Z),
    ("avatar_righthandmiddle3", 80.0, Vec3::Z),
    ("avatar_righthandring1", 80.0, Vec3::Z),
    ("avatar_righthandring2", 80.0, Vec3::Z),
    ("avatar_righthandring3", 80.0, Vec3::Z),
    ("avatar_righthandpinky1", 80.0, Vec3::Z),
    ("avatar_righthandpinky2", 80.0, Vec3::Z),
    ("avatar_righthandpinky3", 80.0, Vec3::Z),
];

#[allow(clippy::type_complexity)]
fn cache_point_at_rig(
    mut commands: Commands,
    needs_rig: Query<Entity, (With<AvatarShape>, Without<PointAtIkRig>)>,
    has_rig: Query<(Entity, &PointAtIkRig), With<AvatarShape>>,
    children_q: Query<&Children>,
    name_q: Query<&Name>,
    transforms: Query<&Transform>,
) {
    // Invalidate stale cache: bones got despawned by a wearable swap.
    for (avatar, rig) in &has_rig {
        let alive = [rig.upper, rig.lower, rig.hand]
            .iter()
            .chain(rig.curled_fingers.iter().map(|(e, _, _)| e))
            .all(|e| transforms.get(*e).is_ok());
        if !alive {
            commands.entity(avatar).remove::<PointAtIkRig>();
        }
    }

    for avatar in &needs_rig {
        let upper = find_bone(avatar, "avatar_rightarm", &children_q, &name_q);
        let lower = find_bone(avatar, "avatar_rightforearm", &children_q, &name_q);
        let hand = find_bone(avatar, "avatar_righthand", &children_q, &name_q);
        if let (Some(upper), Some(lower), Some(hand)) = (upper, lower, hand) {
            let curled_fingers: Vec<(Entity, f32, Vec3)> = FINGER_CURLS
                .iter()
                .filter_map(|(name, deg, axis)| {
                    find_bone(avatar, name, &children_q, &name_q).map(|e| (e, *deg, *axis))
                })
                .collect();
            info!(
                "point_at_ik: cached rig for {:?} (arm: {:?}, forearm: {:?}, hand: {:?}, fingers: {})",
                avatar, upper, lower, hand,
                curled_fingers.len()
            );
            commands.entity(avatar).try_insert(PointAtIkRig {
                upper,
                lower,
                hand,
                curled_fingers,
                weight: 0.0,
                smoothed_target: None,
                is_rotating: false,
                body_yaw: None,
            });
        }
    }
}

#[allow(clippy::type_complexity)]
pub(crate) fn apply_point_at_ik(
    time: Res<Time>,
    mut avatars: Query<
        (Entity, &mut PointAtIkRig, &PointAtSync, Has<PrimaryUser>),
        With<AvatarShape>,
    >,
    parents: Query<&ChildOf>,
    mut tx: ParamSet<(Query<&mut Transform>, TransformHelper)>,
) {
    let dt = time.delta_secs();
    let alpha = if dt > 0.0 {
        1.0 - (-dt / WEIGHT_TAU).exp()
    } else {
        0.0
    };
    let target_alpha = if dt > 0.0 {
        1.0 - (-dt / TARGET_TAU).exp()
    } else {
        0.0
    };
    let mut writes: Vec<(Entity, Quat)> = Vec::new();
    // (finger_bone, axis, scaled_angle_rad) collected alongside arm writes;
    // applied as a post-multiplied curl onto whatever the animation set, so
    // weight=0 leaves the bone alone.
    let mut finger_curls: Vec<(Entity, Vec3, f32)> = Vec::new();

    for (avatar_entity, mut rig, point_at, is_local) in &mut avatars {
        // PointAtSync.target_world is in DCL convention (z mirrored from bevy).
        let target_world = DclTranslation([
            point_at.target_world.x,
            point_at.target_world.y,
            point_at.target_world.z,
        ])
        .to_bevy_translation();

        // Compute the body-forward → target yaw delta (used by both local
        // body rotation and the arm-IK gate below). For foreign players we
        // don't drive rotation, but we still measure the delta so the arm
        // only engages when their network-driven body actually faces the
        // target.
        let mut body_facing_ok = false;
        if point_at.is_pointing {
            if let Ok(avatar_g) = tx.p1().compute_global_transform(avatar_entity) {
                let avatar_pos = avatar_g.translation();
                let dx = target_world.x - avatar_pos.x;
                let dz = target_world.z - avatar_pos.z;
                if dx * dx + dz * dz > 1e-2 {
                    // Avatar mesh forward is bevy -Z. Under Y rotation by θ,
                    // -Z maps to (-sin θ, 0, -cos θ), so the yaw that aims
                    // the avatar at (dx, _, dz) is atan2(-dx, -dz).
                    let desired_yaw = (-dx).atan2(-dz);
                    // For the local player, prefer our own tracked yaw —
                    // movement-scene round-trips can lag and the transform
                    // here may be a frame behind. Foreign players fall back
                    // to the transform since we don't drive their rotation.
                    let cur_yaw = if is_local {
                        rig.body_yaw
                            .unwrap_or_else(|| avatar_g.rotation().to_euler(EulerRot::YXZ).0)
                    } else {
                        avatar_g.rotation().to_euler(EulerRot::YXZ).0
                    };
                    let delta_deg = wrap_180_deg((desired_yaw - cur_yaw).to_degrees());

                    // Cone matches Unity's asymmetric extents: 17.5° on the
                    // anti-clockwise side, 53° on the clockwise side. Net
                    // effect is the cone center sits ~18° toward the right
                    // shoulder — pointing slightly right of forward is the
                    // "comfortable" direction for a right-handed gesture.
                    // Display in a slightly wider cone, trigger rotation in
                    // a slightly narrower one, so per-client rounding can't
                    // cause one of us to display while the other doesn't or
                    // one of us to be turning while the other isn't.
                    let display_cone = (-BODY_ROTATE_TRIGGER_RIGHT_DEG - CONE_TOLERANCE_DEG)
                        ..=(BODY_ROTATE_TRIGGER_LEFT_DEG + CONE_TOLERANCE_DEG);
                    let trigger_cone = (-BODY_ROTATE_TRIGGER_RIGHT_DEG + CONE_TOLERANCE_DEG)
                        ..=(BODY_ROTATE_TRIGGER_LEFT_DEG - CONE_TOLERANCE_DEG);
                    let in_display_cone = display_cone.contains(&delta_deg);
                    let needs_to_rotate = !trigger_cone.contains(&delta_deg);
                    body_facing_ok = in_display_cone;

                    if is_local {
                        if !rig.is_rotating && needs_to_rotate {
                            rig.is_rotating = true;
                        }
                        let new_yaw = if rig.is_rotating {
                            let max_step = BODY_ROTATE_DEG_PER_SEC * dt;
                            let step = delta_deg.clamp(-max_step, max_step);
                            let stepped = cur_yaw + step.to_radians();
                            if delta_deg.abs() <= BODY_ROTATE_FINISH_DEG {
                                rig.is_rotating = false;
                            }
                            stepped
                        } else {
                            cur_yaw
                        };
                        rig.body_yaw = Some(new_yaw);
                    }
                } else {
                    // Target overhead/below — body orientation doesn't matter.
                    body_facing_ok = true;
                    rig.is_rotating = false;
                }
            }
        } else {
            rig.is_rotating = false;
            rig.body_yaw = None;
        }

        // Arm IK weight target: pointing only counts when the body is also
        // facing the target. Driven above 1.0 (clamped on display) so
        // brief excursions out of the cone don't immediately drop the
        // displayed weight below 1.0 — gives a time-based dampener.
        let target_weight = if body_facing_ok {
            ENGAGED_WEIGHT_TARGET
        } else {
            0.0
        };
        rig.weight += (target_weight - rig.weight) * alpha;
        let display_weight = rig.weight.clamp(0.0, 1.0);

        // Smooth the IK target so re-aiming or starting a fresh gesture
        // mid-ramp-out glides the hand instead of snapping. Initialize on
        // first activation, hold while ramping out, clear once disengaged.
        let solve_target = if point_at.is_pointing {
            let new_target = match rig.smoothed_target {
                Some(prev) => prev + (target_world - prev) * target_alpha,
                None => target_world,
            };
            rig.smoothed_target = Some(new_target);
            new_target
        } else {
            let held = rig.smoothed_target.unwrap_or(target_world);
            if display_weight < 1e-3 {
                rig.smoothed_target = None;
            }
            held
        };

        // Write the body-yaw override now (before reading bone globals) so
        // the IK math sees bones rooted at our intended avatar orientation.
        // Otherwise the IK would solve for shoulder positions based on the
        // movement-system yaw, then we'd rotate the avatar at end-of-loop
        // and the shoulder would shift out from under the hand — visible
        // arm jitter while the body is mid-turn.
        if let Some(yaw) = rig.body_yaw {
            if let Ok(mut t) = tx.p0().get_mut(avatar_entity) {
                t.rotation = Quat::from_rotation_y(yaw);
            }
        }

        if display_weight < 1e-3 {
            // Once the arm has fully ramped out, drop the body override so
            // movement reclaims control.
            if !point_at.is_pointing {
                rig.body_yaw = None;
            }
            continue;
        }

        // Read pre-IK globals via TransformHelper. Writes are deferred to a
        // single pass at the end so the chain math sees a consistent
        // pre-IK state.
        let helper = tx.p1();
        let Ok(upper_g) = helper.compute_global_transform(rig.upper) else {
            continue;
        };
        let Ok(lower_g) = helper.compute_global_transform(rig.lower) else {
            continue;
        };
        let Ok(hand_g) = helper.compute_global_transform(rig.hand) else {
            continue;
        };
        let Ok(upper_parent) = parents.get(rig.upper).map(|c| c.parent()) else {
            continue;
        };
        let Ok(upper_parent_g) = helper.compute_global_transform(upper_parent) else {
            continue;
        };

        let a = upper_g.translation();
        let b = lower_g.translation();
        let c = hand_g.translation();
        let l_ab = (b - a).length();
        let l_bc = (c - b).length();

        let Some((r_upper, r_lower)) =
            solve_two_bone(a, b, c, solve_target, l_ab, l_bc, POLE_DIR_WORLD)
        else {
            continue;
        };

        // Clamp the resulting upper-arm direction to a body-local half-space
        // so the arm doesn't bend through the torso. The clamp runs in the
        // post-yaw-write body frame (we wrote `avatar.transform.rotation`
        // above), so as the body rotates the half-space rotates with it.
        let r_upper = if let Ok(body_g) = tx.p1().compute_global_transform(avatar_entity) {
            let cur_dir_world = (b - a).normalize_or_zero();
            constrain_upper_arm(r_upper, cur_dir_world, body_g.rotation())
        } else {
            r_upper
        };

        let r_upper_w = Quat::IDENTITY.slerp(r_upper, display_weight);
        let r_lower_w = Quat::IDENTITY.slerp(r_lower, display_weight);

        let cur_upper_g_rot = upper_g.compute_transform().rotation;
        let cur_lower_g_rot = lower_g.compute_transform().rotation;
        let parent_g_rot = upper_parent_g.compute_transform().rotation;

        let new_upper_g_rot = r_upper_w * cur_upper_g_rot;
        let new_lower_g_rot = r_lower_w * r_upper_w * cur_lower_g_rot;

        let upper_local = parent_g_rot.inverse() * new_upper_g_rot;
        let lower_local = new_upper_g_rot.inverse() * new_lower_g_rot;

        writes.push((rig.upper, upper_local));
        writes.push((rig.lower, lower_local));
        for &(finger, deg, axis) in &rig.curled_fingers {
            finger_curls.push((finger, axis, deg.to_radians() * display_weight));
        }
    }

    let mut transforms = tx.p0();
    for (bone, rot) in writes {
        if let Ok(mut t) = transforms.get_mut(bone) {
            t.rotation = rot;
        }
    }
    // Curl post-multiplied onto whatever animation set: identity at angle=0
    // (weight 0 or zero entry in FINGER_CURLS), per-bone signed degrees and
    // axis at full weight.
    for (finger, axis, angle) in finger_curls {
        if let Ok(mut t) = transforms.get_mut(finger) {
            t.rotation *= Quat::from_axis_angle(axis, angle);
        }
    }
}

/// Project the upper-arm swing rotation `r_upper` so the resulting bone
/// direction stays in the body-local half-space `x >= UPPER_ARM_MIN_X_LOCAL,
/// z <= UPPER_ARM_MAX_Z_LOCAL`. `cur_dir_world` is the rest-pose
/// shoulder→elbow direction; the swing rotates that into the new direction
/// `r_upper * cur_dir_world`. If the new direction violates the constraints
/// we clamp the offending components in body-local frame, renormalize, and
/// rebuild `r_upper` as the rotation_arc from rest to the clamped direction.
fn constrain_upper_arm(r_upper: Quat, cur_dir_world: Vec3, body_rot: Quat) -> Quat {
    let new_dir_world = r_upper * cur_dir_world;
    let mut local = body_rot.inverse() * new_dir_world;
    let mut clamped = false;
    if local.x < UPPER_ARM_MIN_X_LOCAL {
        local.x = UPPER_ARM_MIN_X_LOCAL;
        clamped = true;
    }
    if local.z > UPPER_ARM_MAX_Z_LOCAL {
        local.z = UPPER_ARM_MAX_Z_LOCAL;
        clamped = true;
    }
    if !clamped {
        return r_upper;
    }
    let local = local.normalize_or_zero();
    if local.length_squared() < 0.5 {
        return r_upper;
    }
    let new_dir_world = body_rot * local;
    Quat::from_rotation_arc(cur_dir_world, new_dir_world)
}

fn wrap_180_deg(deg: f32) -> f32 {
    let mut d = deg % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d < -180.0 {
        d += 360.0;
    }
    d
}

fn find_bone(
    root: Entity,
    target_lower: &str,
    children: &Query<&Children>,
    names: &Query<&Name>,
) -> Option<Entity> {
    if let Ok(name) = names.get(root) {
        if name.as_str().to_lowercase() == target_lower {
            return Some(root);
        }
    }
    if let Ok(kids) = children.get(root) {
        for k in kids {
            if let Some(found) = find_bone(*k, target_lower, children, names) {
                return Some(found);
            }
        }
    }
    None
}
