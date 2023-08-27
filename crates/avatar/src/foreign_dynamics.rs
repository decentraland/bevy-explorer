use bevy::prelude::*;

use common::{
    dynamics::MAX_FALL_SPEED,
    util::{QuatNormalizeExt, TryInsertEx},
};

use comms::global_crdt::{ForeignPlayer, PlayerPositionEvent};
use dcl_component::{transform_and_parent::DclTransformAndParent, SceneEntityId};
use scene_runner::{
    renderer_context::RendererSceneContext, update_world::mesh_collider::SceneColliderData,
    ContainingScene,
};

use super::AvatarDynamicState;

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
    translation: Vec3,
    rotation: Quat,
    index: u32,
}

fn update_foreign_user_target_position(
    mut commands: Commands,
    mut move_events: EventReader<PlayerPositionEvent>,
    mut players: Query<(&ForeignPlayer, Option<&mut PlayerTargetPosition>)>,
) {
    for ev in move_events.iter() {
        let dcl_transform = DclTransformAndParent {
            translation: ev.translation,
            rotation: ev.rotation,
            scale: Vec3::ONE,
            parent: SceneEntityId::WORLD_ORIGIN,
        };

        let bevy_trans = dcl_transform.to_bevy_transform();

        if let Ok((_player, maybe_pos)) = players.get_mut(ev.player) {
            if let Some(mut pos) = maybe_pos {
                if pos.index < ev.index {
                    *pos = PlayerTargetPosition {
                        time: ev.time,
                        translation: bevy_trans.translation,
                        rotation: bevy_trans.rotation.normalize_or_identity(),
                        index: ev.index,
                    }
                }
            } else {
                commands.entity(ev.player).try_insert((
                    PlayerTargetPosition {
                        time: ev.time,
                        translation: bevy_trans.translation,
                        rotation: bevy_trans.rotation,
                        index: ev.index,
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
        // arrive at target position by time + 0.5
        let walk_time_left = target.time + 0.5 - time.elapsed_seconds();
        if walk_time_left <= 0.0 {
            actual.translation = target.translation;
            dynamic_state.velocity = Vec3::ZERO;
        } else {
            let walk_fraction = (time.delta_seconds() / walk_time_left).min(1.0);
            let delta = (target.translation - actual.translation) * walk_fraction;
            dynamic_state.velocity = delta / time.delta_seconds();
            actual.translation += delta;
        }

        // turn a bit faster
        let turn_time_left = target.time + 0.2 - time.elapsed_seconds();
        if turn_time_left <= 0.0 {
            actual.rotation = target.rotation;
        } else {
            let turn_fraction = (time.delta_seconds() / turn_time_left).min(1.0);
            actual.rotation = actual.rotation.lerp(target.rotation, turn_fraction);
        }

        // update ground height
        // get containing scene
        match containing_scene
            .get(foreign_ent)
            .and_then(|scene| scene_datas.get_mut(scene).ok())
        {
            Some((context, mut collider_data, _scene_transform)) => {
                dynamic_state.ground_height = collider_data
                    .get_groundheight(context.last_update_frame, actual.translation)
                    .map(|(h, _)| h)
                    .unwrap_or(actual.translation.y);
            }
            None => {
                dynamic_state.ground_height = actual.translation.y;
            }
        };

        // fall
        if actual.translation.y > target.translation.y && dynamic_state.ground_height > 0.0 {
            let updated_y = target
                .translation
                .y
                .max(actual.translation.y - MAX_FALL_SPEED * time.delta_seconds())
                .max(actual.translation.y - dynamic_state.ground_height);

            dynamic_state.ground_height += updated_y - actual.translation.y;
            actual.translation.y = updated_y;
        }
    }
}
