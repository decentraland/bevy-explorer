use bevy::prelude::*;
use common::{
    inputs::SystemAction,
    sets::SceneSets,
    structs::{AvatarDynamicState, MoveKind, PointAtSync, PointerTargetType, PrimaryUser},
};
use dcl_component::transform_and_parent::DclTranslation;
use input_manager::{InputManager, InputPriority};
use scene_runner::update_scene::pointer_results::{PointerRay, WorldPointerTarget};

/// Once latched, pointing persists for this many seconds (matches unity's
/// `CharacterControllerSettings.PointAtDuration`). Cancelled early by leaving
/// the idle move-state.
const POINT_AT_DURATION: f32 = 10.0;

/// Fallback distance when the cursor ray doesn't hit anything. Pushed past
/// unity's 100m marker-visibility threshold so receivers don't draw a billboard
/// for "pointing into the sky".
const NO_HIT_DISTANCE: f32 = 200.0;

/// On release, if the cursor ray has swung more than this many degrees from
/// where it was at press, treat the gesture as a drag (cursor manipulation)
/// rather than a click — drop the latch instead of holding for the full
/// duration.
const DRAG_CANCEL_DEG: f32 = 2.0;

/// Pointing at another player's avatar from within this radius is rejected
/// — the arm would have to bend around them and reads weirdly. Matches
/// unity's `HandPointAtSystem.cs:150-154`.
const MIN_AVATAR_TARGET_DISTANCE: f32 = 2.0;

pub struct PointAtPlugin;

impl Plugin for PointAtPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, capture_point_at.in_set(SceneSets::Input));
    }
}

fn capture_point_at(
    input_manager: InputManager,
    world_target: Res<WorldPointerTarget>,
    pointer_ray: Res<PointerRay>,
    time: Res<Time>,
    mut player: Query<(&mut PointAtSync, &AvatarDynamicState, &GlobalTransform), With<PrimaryUser>>,
    mut latch_until: Local<f32>,
    mut press_origin_dir: Local<Option<Vec3>>,
) {
    let Ok((mut sync, dynamics, player_global)) = player.single_mut() else {
        return;
    };
    let now = time.elapsed_secs();
    let idle = dynamics.move_kind == MoveKind::Idle;

    // Drop the latch the moment we leave idle — pointing while running looks
    // wrong and unity gates the same way.
    if !idle {
        *latch_until = 0.0;
        *press_origin_dir = None;
        sync.is_pointing = false;
        return;
    }

    let action = SystemAction::PointAt;

    // Stash the ray direction at press so we can decide on release whether
    // this was a click or a drag.
    if input_manager.just_down(action, InputPriority::None) {
        *press_origin_dir = pointer_ray.0.as_ref().map(|r| *r.direction);
    }

    // On release, compare the ray direction now to where it was at press.
    // If the cursor swung past the threshold, drop the latch — the user was
    // manipulating the camera/cursor, not pointing.
    if input_manager.just_up(action) {
        if let (Some(origin_dir), Some(ray)) = (*press_origin_dir, pointer_ray.0.as_ref()) {
            let dot = origin_dir.dot(*ray.direction).clamp(-1.0, 1.0);
            if dot.acos().to_degrees() > DRAG_CANCEL_DEG {
                *latch_until = 0.0;
                *press_origin_dir = None;
                sync.is_pointing = false;
                return;
            }
        }
        *press_origin_dir = None;
    }

    // While the button is held, keep re-sampling the target so the user can
    // re-aim by dragging. On release the latch counts down with `target_world`
    // frozen at the last sample — point and walk away rather than tracking.
    if input_manager.is_down(action, InputPriority::None) {
        // If the cursor is over UI, the click belongs to that UI — don't
        // hijack it for a point-at gesture.
        if let Some(target) = world_target.0.as_ref() {
            if target.ty == PointerTargetType::Ui {
                return;
            }
            // Pointing at a player avatar from up close looks bad — the arm
            // has to bend around them. Bail rather than produce that.
            if target.ty == PointerTargetType::Avatar {
                if let Some(target_pos) = target.position {
                    if (target_pos - player_global.translation()).length()
                        < MIN_AVATAR_TARGET_DISTANCE
                    {
                        return;
                    }
                }
            }
        }

        // Prefer a real hit (scene/avatar collider). If the cursor is over
        // empty space, project the ray to the fallback distance — but if the
        // ray would dip below the ground plane before then, clamp to y=0 so
        // we point at a sensible piece of ground rather than under the world.
        let target_bevy = world_target
            .0
            .as_ref()
            .and_then(|t| t.position)
            .or_else(|| {
                pointer_ray.0.map(|r| {
                    let mut t = NO_HIT_DISTANCE;
                    if r.direction.y < 0.0 {
                        let to_ground = -r.origin.y / r.direction.y;
                        if to_ground > 0.0 && to_ground < t {
                            t = to_ground;
                        }
                    }
                    r.origin + *r.direction * t
                })
            });
        if let Some(target_bevy) = target_bevy {
            let dcl = DclTranslation::from_bevy_translation(target_bevy);
            sync.target_world = Vec3::new(dcl.0[0], dcl.0[1], dcl.0[2]);
            sync.is_pointing = true;
            *latch_until = now + POINT_AT_DURATION;
            return;
        }
    }

    if now >= *latch_until {
        sync.is_pointing = false;
    }
}
