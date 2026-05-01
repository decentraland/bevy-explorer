use bevy::prelude::*;

/// World-space delta rotations for the root and mid bone of a 3-link chain
/// (root → mid → end), such that after applying them in order the end bone
/// reaches `target`. Returns `None` if the chain has degenerate lengths or
/// the geometry is otherwise unsolvable.
///
/// `pole_dir` selects which side of the swing plane the mid bone bends to —
/// for legs this is the body's forward direction, for arms a downward bias
/// keeps the elbow under the shoulder.
///
/// The returned rotations are *swings* relative to the current pose, not
/// absolute orientations. Call sites typically slerp them from `IDENTITY` by
/// an IK weight before composing onto the bone's current global rotation.
pub fn solve_two_bone(
    root: Vec3,
    mid: Vec3,
    end: Vec3,
    target: Vec3,
    l_root_mid: f32,
    l_mid_end: f32,
    pole_dir: Vec3,
) -> Option<(Quat, Quat)> {
    let at = target - root;
    let l_at_raw = at.length();
    if l_at_raw < 1e-4 {
        return None;
    }
    let l_at = l_at_raw.clamp(1e-4, l_root_mid + l_mid_end - 1e-4);
    let dir_at = at / l_at_raw;

    // Project the pole hint onto the plane perpendicular to the chain
    // direction so it picks a bend side without affecting the chain length.
    // Fall back to a Y-cross axis (and finally world X) if the hint collapses.
    let pole_perp = pole_dir - dir_at * dir_at.dot(pole_dir);
    let pole_perp = pole_perp.normalize_or_zero();
    let pole_perp = if pole_perp.length_squared() < 0.5 {
        let alt = Vec3::Y.cross(dir_at).normalize_or_zero();
        if alt.length_squared() < 0.5 {
            Vec3::X
        } else {
            alt
        }
    } else {
        pole_perp
    };

    // Law of cosines at the root to find the mid-bone position that puts
    // both segments on the swing plane and the end at `target`.
    let cos_a = ((l_root_mid * l_root_mid + l_at * l_at - l_mid_end * l_mid_end)
        / (2.0 * l_root_mid * l_at))
        .clamp(-1.0, 1.0);
    let sin_a = (1.0 - cos_a * cos_a).max(0.0).sqrt();
    let new_mid = root + dir_at * (l_root_mid * cos_a) + pole_perp * (l_root_mid * sin_a);

    let cur_dir_root_mid = (mid - root).normalize_or_zero();
    let new_dir_root_mid = (new_mid - root).normalize_or_zero();
    let r_root = Quat::from_rotation_arc(cur_dir_root_mid, new_dir_root_mid);

    let cur_dir_mid_end = (end - mid).normalize_or_zero();
    let dir_mid_end_after_root = r_root * cur_dir_mid_end;
    let new_dir_mid_end = (target - new_mid).normalize_or_zero();
    let r_mid = Quat::from_rotation_arc(dir_mid_end_after_root, new_dir_mid_end);

    Some((r_root, r_mid))
}
