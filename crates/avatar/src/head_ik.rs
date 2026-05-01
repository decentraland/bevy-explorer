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
        commands
            .entity(avatar)
            .try_insert(HeadIkRig { head: head_bone });
    }
}

fn apply_head_ik(
    avatars: Query<(&HeadIkRig, &HeadSync), With<AvatarShape>>,
    parents: Query<&ChildOf>,
    mut tx: ParamSet<(Query<&mut Transform>, TransformHelper)>,
) {
    let mut writes: Vec<(Entity, Quat)> = Vec::new();

    for (rig, head_sync) in &avatars {
        if !head_sync.yaw_enabled && !head_sync.pitch_enabled {
            continue;
        }

        // Wire format: DCL yaw (N=0, E=90, S=180, W=270) in a left-handed,
        // +Z-forward frame. To render in bevy's right-handed -Z-forward frame
        // we mirror the Z-axis (negates the Y-rotation sign) and add 180° so
        // the head bone's rest-pose forward aligns with the gaze target.
        let yaw = -head_sync.yaw_deg.to_radians() + std::f32::consts::PI;
        let pitch = head_sync.pitch_deg.to_radians();
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
