use bevy::prelude::*;
use common::{sets::PostUpdateSets, structs::PointAtSync};
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
    /// Smoothed IK weight, ramped 0→1 when pointing engages and 1→0 when it
    /// disengages. Lets the animation pose retake the arm without a snap.
    weight: f32,
}

/// Time constant for the weight ramp (seconds for ~63% of the gap). Tuned to
/// feel responsive without snapping — Unity uses a similar ~0.5s ramp.
const WEIGHT_TAU: f32 = 0.15;

/// Hint direction in world space for which side of the swing plane the elbow
/// bends to. Pointing "down" sends the elbow under the shoulder for natural
/// forward-arm gestures.
const POLE_DIR_WORLD: Vec3 = Vec3::NEG_Y;

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
            });
        }
    }
}

#[allow(clippy::type_complexity)]
fn apply_point_at_ik(
    time: Res<Time>,
    mut avatars: Query<(&mut PointAtIkRig, &PointAtSync), With<AvatarShape>>,
    parents: Query<&ChildOf>,
    mut tx: ParamSet<(Query<&mut Transform>, TransformHelper)>,
) {
    let dt = time.delta_secs();
    let alpha = if dt > 0.0 {
        1.0 - (-dt / WEIGHT_TAU).exp()
    } else {
        0.0
    };
    let mut writes: Vec<(Entity, Quat)> = Vec::new();
    // (finger_bone, axis, scaled_angle_rad) collected alongside arm writes;
    // applied as a post-multiplied curl onto whatever the animation set, so
    // weight=0 leaves the bone alone.
    let mut finger_curls: Vec<(Entity, Vec3, f32)> = Vec::new();

    for (mut rig, point_at) in &mut avatars {
        // Smooth the IK weight toward 1 while pointing, 0 otherwise. This
        // alone provides the blend in/out — the swing math runs every frame,
        // and zero-weight slerp is a no-op.
        let target_weight = if point_at.is_pointing { 1.0 } else { 0.0 };
        rig.weight += (target_weight - rig.weight) * alpha;
        if rig.weight < 1e-3 {
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

        // PointAtSync.target_world is in DCL convention (z mirrored from bevy).
        let target_world = DclTranslation([
            point_at.target_world.x,
            point_at.target_world.y,
            point_at.target_world.z,
        ])
        .to_bevy_translation();

        let Some((r_upper, r_lower)) =
            solve_two_bone(a, b, c, target_world, l_ab, l_bc, POLE_DIR_WORLD)
        else {
            continue;
        };

        let r_upper_w = Quat::IDENTITY.slerp(r_upper, rig.weight);
        let r_lower_w = Quat::IDENTITY.slerp(r_lower, rig.weight);

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
            finger_curls.push((finger, axis, deg.to_radians() * rig.weight));
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
