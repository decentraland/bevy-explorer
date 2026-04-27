use bevy::{
    prelude::*,
    transform::systems::{mark_dirty_trees, propagate_parent_transforms, sync_simple_transforms},
};
use bevy_console::ConsoleCommand;
use common::structs::PrimaryUser;
use console::DoAddConsoleCommand;
use dcl_component::proto_components::sdk::components::ColliderLayer;
use scene_runner::{
    update_world::{
        mesh_collider::{SceneColliderData, GROUND_COLLISION_MASK},
        transform_and_parent::PostUpdateSets,
    },
    ContainingScene,
};

use crate::animate::ActiveEmote;

pub struct FootIkPlugin;

impl Plugin for FootIkPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FootIkConfig>();
        app.add_systems(
            PostUpdate,
            (
                cache_foot_ik_rig,
                apply_foot_ik,
                (
                    mark_dirty_trees,
                    propagate_parent_transforms,
                    sync_simple_transforms,
                )
                    .chain(),
            )
                .chain()
                .after(PostUpdateSets::PlayerUpdate)
                .before(PostUpdateSets::AttachSync),
        );
        app.add_console_command::<FootIkConsoleCommand, _>(foot_ik_console_command);
    }
}

#[derive(Resource)]
pub struct FootIkConfig {
    pub enabled: bool,
    /// Local-space Y of the foot bone when planted on flat ground.
    /// Hardcoded guess; tune by observation.
    pub plant_y: f32,
    /// Start the down-cast this far above the animated foot.
    pub raycast_up: f32,
    /// Search this far below the animated foot.
    pub raycast_down: f32,
    /// Foot's target above the player by more than this → leg disengages
    /// (target is "too high to step onto").
    pub max_step_up: f32,
    /// Maximum amount the hips will drop. Also acts as the per-leg "can reach"
    /// gate going downward: if a leg would need a larger pelvis drop than this
    /// to plant, the leg disengages (e.g. dangling off a cliff edge).
    pub max_pelvis_drop: f32,
    /// Floor on the per-emote transition_seconds used to ramp IK weight; avoids
    /// instantaneous snaps when an emote declares 0s.
    pub min_transition_seconds: f32,
    /// Maximum angle (degrees) the foot will tilt away from world-up to match
    /// the contact-normal of the ground beneath it. Caps stylised aesthetics
    /// (toes don't dive into steep slopes).
    pub max_foot_tilt_deg: f32,
    /// Time in seconds for a leg to engage/disengage when its reachability
    /// flips (e.g. crossing a cliff edge while turning).
    pub engage_transition_seconds: f32,
    /// Maximum vertical change per second of the foot's final world Y (the
    /// post-weight, post-IK output). Smooths step-discontinuities in the
    /// raycast result (cliff edges traversed by foot xz while turning) and
    /// is invariant under continuous platform motion (avatar moves with the
    /// platform, so the *relative* offset doesn't change). Snaps on the
    /// first engaged frame after a disengaged one.
    pub target_velocity_limit: f32,
}

impl Default for FootIkConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            plant_y: 0.091,
            raycast_up: 0.3,
            raycast_down: 0.6,
            max_step_up: 0.4,
            max_pelvis_drop: 0.4,
            min_transition_seconds: 0.05,
            max_foot_tilt_deg: 30.0,
            engage_transition_seconds: 0.5,
            target_velocity_limit: 1.5,
        }
    }
}

#[derive(Component)]
pub struct FootIkRig {
    pub hips: Entity,
    pub left: LegBones,
    pub right: LegBones,
}

#[derive(Clone, Copy)]
pub struct LegBones {
    pub upper: Entity,
    pub lower: Entity,
    pub foot: Entity,
}

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/footik")]
struct FootIkConsoleCommand {}

fn foot_ik_console_command(
    mut input: ConsoleCommand<FootIkConsoleCommand>,
    mut config: ResMut<FootIkConfig>,
) {
    if let Some(Ok(_)) = input.take() {
        config.enabled = !config.enabled;
        input.reply(format!(
            "foot IK {}",
            if config.enabled { "ON" } else { "OFF" }
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn cache_foot_ik_rig(
    mut commands: Commands,
    config: Res<FootIkConfig>,
    needs_rig: Query<Entity, (With<PrimaryUser>, Without<FootIkRig>)>,
    has_rig: Query<(Entity, &FootIkRig), With<PrimaryUser>>,
    children_q: Query<&Children>,
    name_q: Query<&Name>,
    globals: Query<&GlobalTransform>,
    mut log_counter: Local<u32>,
) {
    // Invalidate any cached rig whose bones no longer exist (e.g. a wearable
    // reload despawned the old armature). On the next frame the rebuild
    // pass below installs a fresh rig.
    for (avatar, rig) in &has_rig {
        let alive = [
            rig.hips,
            rig.left.upper,
            rig.left.lower,
            rig.left.foot,
            rig.right.upper,
            rig.right.lower,
            rig.right.foot,
        ]
        .iter()
        .all(|e| globals.get(*e).is_ok());
        if !alive {
            info!("foot_ik: invalidating stale rig on {:?}", avatar);
            commands.entity(avatar).remove::<FootIkRig>();
        }
    }

    for avatar in &needs_rig {
        let hips = find_bone(avatar, "avatar_hips", &children_q, &name_q);
        let lu = find_bone(avatar, "avatar_leftupleg", &children_q, &name_q);
        let ll = find_bone(avatar, "avatar_leftleg", &children_q, &name_q);
        let lf = find_bone(avatar, "avatar_leftfoot", &children_q, &name_q);
        let ru = find_bone(avatar, "avatar_rightupleg", &children_q, &name_q);
        let rl = find_bone(avatar, "avatar_rightleg", &children_q, &name_q);
        let rf = find_bone(avatar, "avatar_rightfoot", &children_q, &name_q);

        if let (Some(hips), Some(lu), Some(ll), Some(lf), Some(ru), Some(rl), Some(rf)) =
            (hips, lu, ll, lf, ru, rl, rf)
        {
            info!(
                "foot_ik: cached rig for {:?} (hips: {:?}, l: {:?}/{:?}/{:?}, r: {:?}/{:?}/{:?})",
                avatar, hips, lu, ll, lf, ru, rl, rf
            );
            commands.entity(avatar).try_insert(FootIkRig {
                hips,
                left: LegBones {
                    upper: lu,
                    lower: ll,
                    foot: lf,
                },
                right: LegBones {
                    upper: ru,
                    lower: rl,
                    foot: rf,
                },
            });
        } else if config.enabled {
            *log_counter = log_counter.wrapping_add(1);
            if *log_counter % 120 == 1 {
                let mut all_names = Vec::new();
                collect_descendant_names(avatar, &children_q, &name_q, &mut all_names, 0);
                warn!(
                    "foot_ik: missing bones (hips={} lu={} ll={} lf={} ru={} rl={} rf={}); descendant names ({}): {:?}",
                    hips.is_some(), lu.is_some(), ll.is_some(), lf.is_some(),
                    ru.is_some(), rl.is_some(), rf.is_some(),
                    all_names.len(),
                    all_names.iter().take(80).collect::<Vec<_>>(),
                );
            }
        }
    }
}

fn collect_descendant_names(
    root: Entity,
    children: &Query<&Children>,
    names: &Query<&Name>,
    out: &mut Vec<String>,
    depth: u32,
) {
    if depth > 8 {
        return;
    }
    if let Ok(name) = names.get(root) {
        out.push(name.as_str().to_string());
    }
    if let Ok(kids) = children.get(root) {
        for k in kids {
            collect_descendant_names(*k, children, names, out, depth + 1);
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

struct LegPlan {
    a: Vec3,
    b: Vec3,
    c: Vec3,
    target_c: Vec3,
    l_ab: f32,
    l_bc: f32,
    /// True if the leg can physically reach `target_c` within the configured
    /// step-up / pelvis-drop limits this frame.
    reach_ok: bool,
    /// Pelvis drop required for this leg to physically reach `target_c`.
    /// 0 if the leg can already reach without any drop.
    required_drop: f32,
    /// World-space normal of the ground surface beneath the foot.
    contact_normal: Vec3,
    cur_hip_global_rot: Quat,
    cur_knee_global_rot: Quat,
    cur_foot_global_rot: Quat,
}

#[derive(Default, Clone, Copy)]
struct LegEngState {
    /// Per-leg engagement, ramped over `engage_transition_seconds` toward
    /// 1.0 while `reach_ok` and 0.0 otherwise.
    engaged: f32,
    /// Last frame's final foot Y (animated_y + (target_y - animated_y) * w)
    /// after the velocity limit. Snapped to the desired value on the first
    /// engaged frame, then rate-limited per-frame thereafter.
    last_final_y: f32,
}

#[allow(clippy::too_many_arguments)]
fn apply_foot_ik(
    config: Res<FootIkConfig>,
    time: Res<Time>,
    primary: Query<(&FootIkRig, Option<&ActiveEmote>, &GlobalTransform), With<PrimaryUser>>,
    containing: ContainingScene,
    mut scenes: Query<&mut SceneColliderData>,
    parents: Query<&ChildOf>,
    globals: Query<&GlobalTransform>,
    mut transforms: Query<&mut Transform>,
    mut anim_w: Local<f32>,
    mut leg_state: Local<[LegEngState; 2]>,
    mut log_tick: Local<u32>,
) {
    if !config.enabled {
        // Reset ramps so re-enabling doesn't pop in at full strength.
        *anim_w = 0.0;
        *leg_state = [LegEngState::default(); 2];
        return;
    }
    *log_tick = log_tick.wrapping_add(1);
    let log_now = *log_tick % 60 == 1;

    let Ok((rig, active_emote, player_global)) = primary.single() else {
        if log_now {
            warn!("foot_ik: no primary user with FootIkRig");
        }
        return;
    };

    // Animation-driven IK strength: ramp toward 1.0 while the active emote is
    // an idle pose, otherwise toward 0.0, at a rate set by the emote's
    // declared transition_seconds. No active emote → ramp out.
    let target = active_emote
        .map(|e| if e.is_idle() { 1.0 } else { 0.0 })
        .unwrap_or(0.0);
    let transition = active_emote
        .map(|e| e.transition_seconds())
        .unwrap_or(config.min_transition_seconds)
        .max(config.min_transition_seconds);
    let dt = time.delta_secs();
    *anim_w = if target > *anim_w {
        (*anim_w + dt / transition).min(target)
    } else {
        (*anim_w - dt / transition).max(target)
    };
    let w_anim = *anim_w;

    let scene_ents: Vec<Entity> = containing
        .get_position(player_global.translation())
        .into_iter()
        .collect();

    // Pole hint: avatar's forward direction. The Decentraland avatar rig faces
    // local -Z (knees bend toward -Z), so use that as the pole.
    let pole_dir = player_global.compute_transform().rotation * Vec3::NEG_Z;
    let player_y = player_global.translation().y;

    // Pass 1: per-leg raycast. plan_leg returns geometry + reach_ok; we
    // always run it (even when w_anim is small) so engagement and rate-limit
    // state stay current across walk→idle transitions.
    let raw = [
        plan_leg(
            "L",
            rig.left,
            &config,
            player_y,
            &scene_ents,
            &mut scenes,
            &globals,
            log_now,
        ),
        plan_leg(
            "R",
            rig.right,
            &config,
            player_y,
            &scene_ents,
            &mut scenes,
            &globals,
            log_now,
        ),
    ];

    let eng_step = dt / config.engage_transition_seconds.max(1e-3);
    let mut effective: [Option<LegPlan>; 2] = [None, None];
    let mut leg_w = [0.0f32; 2];

    for i in 0..2 {
        let state = &mut leg_state[i];
        let was_engaged = state.engaged > 1e-3;

        // Update engagement target.
        let target_eng = match raw[i].as_ref() {
            Some(p) if p.reach_ok => 1.0,
            _ => 0.0,
        };
        state.engaged = if target_eng > state.engaged {
            (state.engaged + eng_step).min(target_eng)
        } else {
            (state.engaged - eng_step).max(target_eng)
        };

        // Engagement clamped by (not multiplied with) the animation weight —
        // both act as independent gates and the lower wins.
        let w = w_anim.min(state.engaged);
        leg_w[i] = w;

        if state.engaged <= 1e-3 {
            state.engaged = 0.0;
            continue;
        }
        let Some(p) = raw[i].as_ref() else {
            continue;
        };

        // Velocity limit on the foot's *final* world Y (linear approximation
        // of IK output: animated_y + (target_y - animated_y) * w). On the
        // first engaged frame, snap. Otherwise rate-limit the per-frame
        // change. Then back-derive a new target_y for the IK math so the
        // post-weight foot lands at the rate-limited final Y.
        let animated_y = p.c.y;
        let raw_target_y = p.target_c.y;
        let desired_final_y = animated_y + (raw_target_y - animated_y) * w;
        let final_y = if was_engaged {
            let max_step = config.target_velocity_limit * dt;
            let delta = (desired_final_y - state.last_final_y).clamp(-max_step, max_step);
            state.last_final_y + delta
        } else {
            desired_final_y
        };
        state.last_final_y = final_y;

        // Back-derive an IK target so that, after the slerp blend at weight w,
        // the foot lands at final_y. The IK math itself clamps l_at to within
        // physical reach, so we don't need (and must not apply) a hip-relative
        // clamp here — the pelvis is about to drop, expanding the reach.
        let new_target_y = if w > 1e-3 {
            animated_y + (final_y - animated_y) / w
        } else {
            raw_target_y
        };
        let target_c = Vec3::new(p.target_c.x, new_target_y, p.target_c.z);

        // Pelvis drop is sized to where the foot will *actually* end up
        // (final_y), using the raw raycast XZ.
        let total_reach = p.l_ab + p.l_bc;
        let total = (total_reach - 1e-3).max(0.0);
        let dx = p.a.x - p.target_c.x;
        let dz = p.a.z - p.target_c.z;
        let horiz2 = dx * dx + dz * dz;
        let hv = p.a.y - final_y;
        let inside = total * total - horiz2;
        let required_drop = if inside > 0.0 {
            (hv - inside.sqrt()).max(0.0)
        } else {
            0.0
        };

        effective[i] = Some(LegPlan {
            a: p.a,
            b: p.b,
            c: p.c,
            l_ab: p.l_ab,
            l_bc: p.l_bc,
            reach_ok: p.reach_ok,
            target_c,
            contact_normal: p.contact_normal,
            required_drop,
            cur_hip_global_rot: p.cur_hip_global_rot,
            cur_knee_global_rot: p.cur_knee_global_rot,
            cur_foot_global_rot: p.cur_foot_global_rot,
        });
    }

    if log_now {
        info!(
            "foot_ik: w_anim={:.2} engaged=[{:.2},{:.2}] leg_w=[{:.2},{:.2}]",
            w_anim, leg_state[0].engaged, leg_state[1].engaged, leg_w[0], leg_w[1]
        );
    }

    // Pass 2: pelvis drop = max required across engaged legs, scaled per leg.
    let mut pelvis_drop = 0.0f32;
    for i in 0..2 {
        if let Some(eff) = &effective[i] {
            pelvis_drop = pelvis_drop.max(eff.required_drop * leg_w[i]);
        }
    }
    if log_now {
        info!("foot_ik: pelvis_drop={:.3}", pelvis_drop);
    }

    // Apply pelvis drop to hips bone. Convert the world-Y delta into the hips'
    // parent-local frame using the parent's full affine inverse — this accounts
    // for cumulative ancestor scale (the avatar rig is imported with a ~0.01x
    // scale, so a rotation-only conversion would produce a ~1cm-instead-of-1m
    // delta).
    if pelvis_drop > 1e-4 {
        if let Ok(hips_parent) = parents.get(rig.hips) {
            if let Ok(parent_global) = globals.get(hips_parent.parent()) {
                if let Ok(mut t) = transforms.get_mut(rig.hips) {
                    let local_delta = parent_global
                        .affine()
                        .inverse()
                        .transform_vector3(Vec3::new(0.0, pelvis_drop, 0.0));
                    t.translation -= local_delta;
                }
            }
        }
    }

    let drop_vec = Vec3::new(0.0, pelvis_drop, 0.0);

    // Pass 3: per-leg IK using the rate-limited effective plans.
    let [eff_l, eff_r] = effective;
    for (leg, eff, w) in [(rig.left, eff_l, leg_w[0]), (rig.right, eff_r, leg_w[1])] {
        if let Some(plan) = eff {
            if w > 1e-3 {
                apply_leg_ik(
                    leg,
                    plan,
                    w,
                    drop_vec,
                    pole_dir,
                    &config,
                    &parents,
                    &globals,
                    &mut transforms,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn plan_leg(
    label: &str,
    leg: LegBones,
    config: &FootIkConfig,
    player_y: f32,
    scene_ents: &[Entity],
    scenes: &mut Query<&mut SceneColliderData>,
    globals: &Query<&GlobalTransform>,
    log_now: bool,
) -> Option<LegPlan> {
    let hip_g = globals.get(leg.upper).ok()?;
    let knee_g = globals.get(leg.lower).ok()?;
    let foot_g = globals.get(leg.foot).ok()?;
    let a = hip_g.translation();
    let b = knee_g.translation();
    let c = foot_g.translation();

    let origin = Vec3::new(c.x, c.y + config.raycast_up, c.z);
    let dir = Vec3::NEG_Y;
    let max_dist = config.raycast_up + config.raycast_down;

    let mut best: Option<(f32, Vec3)> = None;
    for scene_ent in scene_ents {
        let Ok(mut collider_data) = scenes.get_mut(*scene_ent) else {
            continue;
        };
        if let Some(hit) = collider_data.cast_ray_nearest(
            origin,
            dir,
            max_dist,
            ColliderLayer::ClPhysics as u32 | GROUND_COLLISION_MASK,
            true,
            false,
            None,
        ) {
            let hit_y = origin.y - hit.toi;
            if best.is_none_or(|(y, _)| hit_y > y) {
                best = Some((hit_y, hit.normal.try_normalize().unwrap_or(Vec3::Y)));
            }
        }
    }

    let Some((ground_y, contact_normal)) = best else {
        if log_now {
            info!(
                "foot_ik[{label}]: no ground hit (foot=({:.2},{:.2},{:.2}) origin_y={:.2} max_dist={:.2} scenes={})",
                c.x, c.y, c.z, origin.y, max_dist, scene_ents.len()
            );
        }
        return None;
    };

    let target_c = Vec3::new(c.x, ground_y + config.plant_y, c.z);

    // Required pelvis drop for this leg to physically reach the target
    // (leg fully extended, hip lowered just enough). 0 if reachable as-is.
    let l_ab = (b - a).length();
    let l_bc = (c - b).length();
    let total = (l_ab + l_bc - 1e-3).max(0.0);
    let dx = a.x - target_c.x;
    let dz = a.z - target_c.z;
    let horiz2 = dx * dx + dz * dz;
    let hv = a.y - target_c.y;
    let inside = total * total - horiz2;
    let required_drop = if inside > 0.0 {
        (hv - inside.sqrt()).max(0.0)
    } else {
        f32::INFINITY
    };

    // Reach gating: binary. Going up is gated by max_step_up; going down is
    // gated by whether the leg can plant within max_pelvis_drop of hip drop.
    let dy_player = target_c.y - player_y;
    let reach_ok = if dy_player >= 0.0 {
        dy_player <= config.max_step_up
    } else {
        required_drop <= config.max_pelvis_drop
    };
    if log_now {
        let dy_anim = target_c.y - c.y;
        info!(
            "foot_ik[{label}]: foot_y={:.3} ground_y={:.3} target_y={:.3} dy_anim={:.3} dy_player={:.3} required_drop={:.3} reach_ok={}",
            c.y, ground_y, target_c.y, dy_anim, dy_player, required_drop, reach_ok
        );
    }

    Some(LegPlan {
        a,
        b,
        c,
        target_c,
        l_ab,
        l_bc,
        reach_ok,
        required_drop,
        contact_normal,
        cur_hip_global_rot: hip_g.compute_transform().rotation,
        cur_knee_global_rot: knee_g.compute_transform().rotation,
        cur_foot_global_rot: foot_g.compute_transform().rotation,
    })
}

#[allow(clippy::too_many_arguments)]
fn apply_leg_ik(
    leg: LegBones,
    plan: LegPlan,
    w: f32,
    drop_vec: Vec3,
    pole_dir: Vec3,
    config: &FootIkConfig,
    parents: &Query<&ChildOf>,
    globals: &Query<&GlobalTransform>,
    transforms: &mut Query<&mut Transform>,
) {
    // After pelvis drop, all leg bones translate by -drop_vec in world space.
    let a = plan.a - drop_vec;
    let b = plan.b - drop_vec;
    let c = plan.c - drop_vec;
    let target_c = plan.target_c;

    let at = target_c - a;
    let l_at_raw = at.length();
    if l_at_raw < 1e-4 {
        return;
    }
    let l_at = l_at_raw.clamp(1e-4, plan.l_ab + plan.l_bc - 1e-4);
    let dir_at = at / l_at_raw;

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

    let cos_a = ((plan.l_ab * plan.l_ab + l_at * l_at - plan.l_bc * plan.l_bc)
        / (2.0 * plan.l_ab * l_at))
        .clamp(-1.0, 1.0);
    let sin_a = (1.0 - cos_a * cos_a).max(0.0).sqrt();
    let new_b = a + dir_at * (plan.l_ab * cos_a) + pole_perp * (plan.l_ab * sin_a);

    let cur_dir_ab = (b - a).normalize_or_zero();
    let new_dir_ab = (new_b - a).normalize_or_zero();
    let r_hip = Quat::from_rotation_arc(cur_dir_ab, new_dir_ab);

    let cur_dir_bc = (c - b).normalize_or_zero();
    let dir_bc_after_hip = r_hip * cur_dir_bc;
    let new_dir_bc = (target_c - new_b).normalize_or_zero();
    let r_knee = Quat::from_rotation_arc(dir_bc_after_hip, new_dir_bc);

    let r_hip_b = Quat::IDENTITY.slerp(r_hip, w);
    let r_knee_b = Quat::IDENTITY.slerp(r_knee, w);

    let new_hip_global_rot = r_hip_b * plan.cur_hip_global_rot;
    let new_knee_global_rot = r_knee_b * r_hip_b * plan.cur_knee_global_rot;

    let Ok(parent_of_hip) = parents.get(leg.upper) else {
        return;
    };
    let Ok(parent_global) = globals.get(parent_of_hip.parent()) else {
        return;
    };
    let parent_global_rot = parent_global.compute_transform().rotation;

    let new_hip_local_rot = parent_global_rot.inverse() * new_hip_global_rot;
    let new_knee_local_rot = new_hip_global_rot.inverse() * new_knee_global_rot;

    // Foot orientation: tilt the animated foot pose toward the contact normal,
    // capped at max_foot_tilt_deg, then write the foot's local rotation
    // (parent = knee bone in world after our update).
    let align_full = Quat::from_rotation_arc(Vec3::Y, plan.contact_normal);
    let (axis, angle) = align_full.to_axis_angle();
    let max_tilt = config.max_foot_tilt_deg.to_radians();
    let align_clamped = Quat::from_axis_angle(axis, angle.min(max_tilt));
    let align_blended = Quat::IDENTITY.slerp(align_clamped, w);
    let new_foot_global_rot = align_blended * plan.cur_foot_global_rot;
    let new_foot_local_rot = new_knee_global_rot.inverse() * new_foot_global_rot;

    if let Ok(mut t) = transforms.get_mut(leg.upper) {
        t.rotation = new_hip_local_rot;
    }
    if let Ok(mut t) = transforms.get_mut(leg.lower) {
        t.rotation = new_knee_local_rot;
    }
    if let Ok(mut t) = transforms.get_mut(leg.foot) {
        t.rotation = new_foot_local_rot;
    }
}
