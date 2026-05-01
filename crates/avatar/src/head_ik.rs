use bevy::prelude::*;
use common::{
    sets::PostUpdateSets,
    structs::{AttachPoints, HeadSync},
};

use crate::AvatarShape;

pub struct HeadIkPlugin;

impl Plugin for HeadIkPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (cache_head_ik_rig, apply_head_ik)
                .chain()
                .in_set(PostUpdateSets::InverseKinematics),
        );
    }
}

#[derive(Component)]
pub struct HeadIkRig {
    /// The avatar's head bone, found via the `avatar_head` attach point's
    /// parent. The attach point is reparented onto the bone during avatar
    /// build (see `lib.rs::reparent_attach_point`).
    pub head: Entity,
    /// Bones that share the gaze swing, ordered root→tip (e.g.
    /// `[upper_chest, neck, head]`). Each entry is `(bone, yaw_weight,
    /// pitch_weight)`; weights per axis sum to ~1 across the chain so the
    /// total swing is preserved. Spine takes more yaw, head takes more
    /// pitch — matches Unity's TwistChain split without needing a generic
    /// chain solver.
    chain: Vec<(Entity, f32, f32)>,
    /// Smoothed yaw stored as an offset from the avatar's body forward
    /// (DCL convention). Storing relative — not absolute world yaw — means
    /// body rotation propagates to the head instantly without the smoother
    /// having to chase a moving target.
    yaw_offset_deg: f32,
    pitch_deg: f32,
}

/// Exponential-smoothing time constant (seconds) — time for the smoothed
/// value to cover ~63% of the gap to target. Tuned to absorb 10Hz network
/// jitter on remotes while still feeling responsive on the local rig.
const SMOOTH_TAU: f32 = 0.12;

/// Pitch is a vertical look angle, not a wrap-around heading — clamp before
/// feeding into `from_euler` so values near or past ±90° (gimbal lock) don't
/// flip the head bone through a full revolution.
const PITCH_CLAMP_DEG: f32 = 50.0;

/// Maximum yaw deviation from the avatar's body forward — beyond this the
/// head holds at the limit (waiting for the body to catch up).
const YAW_CLAMP_DEG: f32 = 70.0;

/// Above this deviation the head IK is disabled entirely (caller is looking
/// well past anything the neck could plausibly reach).
const YAW_DISABLE_DEG: f32 = 110.0;

fn wrap_180(deg: f32) -> f32 {
    let mut d = deg % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d < -180.0 {
        d += 360.0;
    }
    d
}

/// Exponential approach toward `target`, treating both as degrees on a
/// circle: takes the short way around the ±180° wrap.
fn smooth_angle(current: f32, target: f32, alpha: f32) -> f32 {
    current + wrap_180(target - current) * alpha
}

#[allow(clippy::type_complexity)]
fn cache_head_ik_rig(
    mut commands: Commands,
    needs_rig: Query<(Entity, &AttachPoints), (With<AvatarShape>, Without<HeadIkRig>)>,
    has_rig: Query<(Entity, &HeadIkRig), With<AvatarShape>>,
    parents: Query<&ChildOf>,
    transforms: Query<&Transform>,
) {
    // Invalidate stale cache: bone entity got despawned by a wearable swap.
    for (avatar, rig) in &has_rig {
        if transforms.get(rig.head).is_err() {
            commands.entity(avatar).remove::<HeadIkRig>();
        }
    }

    for (avatar, ap) in &needs_rig {
        // The follower entity for the head was reparented onto the head bone, so
        // the bone is the follower's parent. Skip until the GLB has loaded and
        // reparenting has happened — until then the head follower is still a
        // direct child of the avatar entity itself.
        let Ok(head_bone) = parents.get(ap.head).map(|c| c.parent()) else {
            continue;
        };
        if head_bone == avatar {
            continue;
        }
        // Walk up to find neck (parent of head) and an upper-chest/spine bone
        // (grandparent). Bail out of the chain at any step that lands on the
        // avatar entity itself — that means we've run out of skeleton.
        let neck = parents
            .get(head_bone)
            .map(|c| c.parent())
            .ok()
            .filter(|e| *e != avatar);
        let chest = neck
            .and_then(|n| parents.get(n).map(|c| c.parent()).ok())
            .filter(|e| *e != avatar);

        let chain = match (chest, neck) {
            (Some(chest), Some(neck)) => {
                vec![(chest, 0.3, 0.2), (neck, 0.3, 0.3), (head_bone, 0.4, 0.5)]
            }
            (None, Some(neck)) => vec![(neck, 0.5, 0.4), (head_bone, 0.5, 0.6)],
            _ => vec![(head_bone, 1.0, 1.0)],
        };

        info!(
            "head_ik: cached rig for {:?} (chain: {:?})",
            avatar,
            chain.iter().map(|(e, _, _)| e).collect::<Vec<_>>()
        );
        commands.entity(avatar).try_insert(HeadIkRig {
            head: head_bone,
            chain,
            yaw_offset_deg: 0.0,
            pitch_deg: 0.0,
        });
    }
}

fn apply_head_ik(
    time: Res<Time>,
    mut avatars: Query<(Entity, &mut HeadIkRig, &HeadSync), With<AvatarShape>>,
    parents: Query<&ChildOf>,
    mut tx: ParamSet<(Query<&mut Transform>, TransformHelper)>,
) {
    let dt = time.delta_secs();
    let alpha = if dt > 0.0 {
        1.0 - (-dt / SMOOTH_TAU).exp()
    } else {
        0.0
    };

    let mut writes: Vec<(Entity, Quat)> = Vec::new();

    for (avatar_entity, mut rig, head_sync) in &mut avatars {
        // Avatar body forward (DCL convention: sign-flipped Y from bevy
        // world). Used as the constraint reference and as the neutral pose
        // we blend back to when the gaze input is off or out of range.
        let Ok(avatar_global) = tx.p1().compute_global_transform(avatar_entity) else {
            continue;
        };
        let bevy_yaw = avatar_global.rotation().to_euler(EulerRot::YXZ).0;
        let dcl_avatar_yaw = -bevy_yaw.to_degrees();

        // Gaze drives the head only while at least one flag is enabled and
        // the requested yaw stays within the reachable cone; otherwise the
        // target is the neutral (zero offset, level) pose. Both blend-in
        // and blend-out flow through the same smoothing path below.
        let yaw_dev = wrap_180(head_sync.yaw_deg - dcl_avatar_yaw);
        let active =
            (head_sync.yaw_enabled || head_sync.pitch_enabled) && yaw_dev.abs() <= YAW_DISABLE_DEG;
        let (target_yaw_offset, target_pitch) = if active {
            (
                yaw_dev.clamp(-YAW_CLAMP_DEG, YAW_CLAMP_DEG),
                head_sync.pitch_deg,
            )
        } else {
            (0.0, 0.0)
        };

        rig.yaw_offset_deg = smooth_angle(rig.yaw_offset_deg, target_yaw_offset, alpha);
        rig.pitch_deg = smooth_angle(rig.pitch_deg, target_pitch, alpha);

        // World-space swing applied as a delta to each bone's rest pose.
        // Yaw axis is world up; pitch axis is the avatar's right vector
        // (perpendicular to body forward in the horizontal plane). Sign on
        // yaw flips DCL → bevy handedness.
        let bevy_yaw_swing = -rig.yaw_offset_deg.to_radians();
        let pitch_swing = -rig
            .pitch_deg
            .clamp(-PITCH_CLAMP_DEG, PITCH_CLAMP_DEG)
            .to_radians();
        let avatar_world_rot = avatar_global.rotation();
        let avatar_right_world = avatar_world_rot * Vec3::X;

        for &(bone, w_yaw, w_pitch) in &rig.chain {
            // Each bone gets a fraction of the total swing applied in WORLD
            // space, then conjugated into its parent's local frame so writing
            // a local rotation gives the intended world delta. Computing all
            // contributions against the pre-IK parent globals (TransformHelper
            // sees no writes until after this loop) keeps the chain math
            // consistent: the cumulative world rotation at the head equals
            // the sum of weighted yaw/pitch around the same axes.
            let Ok(parent) = parents.get(bone).map(|c| c.parent()) else {
                continue;
            };
            let Ok(parent_global) = tx.p1().compute_global_transform(parent) else {
                continue;
            };
            let Ok(bone_global) = tx.p1().compute_global_transform(bone) else {
                continue;
            };
            let parent_world_rot = parent_global.rotation();
            let bone_local_rot = parent_world_rot.inverse() * bone_global.rotation();

            let delta_world = Quat::from_axis_angle(Vec3::Y, bevy_yaw_swing * w_yaw)
                * Quat::from_axis_angle(avatar_right_world, pitch_swing * w_pitch);
            let delta_local = parent_world_rot.inverse() * delta_world * parent_world_rot;
            writes.push((bone, delta_local * bone_local_rot));
        }
    }

    let mut transforms = tx.p0();
    for (bone, rot) in writes {
        if let Ok(mut t) = transforms.get_mut(bone) {
            t.rotation = rot;
        }
    }
}
