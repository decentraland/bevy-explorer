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
        info!(
            "head_ik: cached rig for {:?} (head bone: {:?})",
            avatar, head_bone
        );
        commands.entity(avatar).try_insert(HeadIkRig {
            head: head_bone,
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

        // Wire format: DCL yaw (N=0, E=90, S=180, W=270) in a left-handed,
        // +Z-forward frame. To render in bevy's right-handed -Z-forward frame
        // we mirror the Z-axis (negates the Y-rotation sign) and add 180° so
        // the head bone's rest-pose forward aligns with the gaze target.
        let world_yaw_dcl = dcl_avatar_yaw + rig.yaw_offset_deg;
        let yaw = -world_yaw_dcl.to_radians() + std::f32::consts::PI;
        let pitch = rig
            .pitch_deg
            .clamp(-PITCH_CLAMP_DEG, PITCH_CLAMP_DEG)
            .to_radians();
        let target_world = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);

        // Convert world target into local space: parent's current world rotation
        // is the basis the bone's local rotation is composed against.
        let Ok(head_parent) = parents.get(rig.head).map(|c| c.parent()) else {
            continue;
        };
        let Ok(parent_global) = tx.p1().compute_global_transform(head_parent) else {
            continue;
        };
        let parent_world_rot = parent_global.compute_transform().rotation;
        let target_local = parent_world_rot.inverse() * target_world;
        writes.push((rig.head, target_local));
    }

    let mut transforms = tx.p0();
    for (head, rot) in writes {
        if let Ok(mut t) = transforms.get_mut(head) {
            t.rotation = rot;
        }
    }
}
