use bevy::prelude::*;

use crate::{
    comms::global_crdt::{ForeignPlayer, PlayerPositionEvent},
    dcl_component::{transform_and_parent::DclTransformAndParent, SceneEntityId},
};

pub struct PlayerMovementPlugin;

impl Plugin for PlayerMovementPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems((update_avatar_target_position, update_avatar_actual_position).chain());
    }
}

#[derive(Component)]
struct PlayerTargetPosition {
    time: f32,
    translation: Vec3,
    rotation: Quat,
    index: u32,
}

#[derive(Component)]
pub struct Velocity(pub f32);

fn update_avatar_target_position(
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
                        rotation: bevy_trans.rotation,
                        index: ev.index,
                    }
                }
            } else {
                commands.entity(ev.player).insert((
                    PlayerTargetPosition {
                        time: ev.time,
                        translation: bevy_trans.translation,
                        rotation: bevy_trans.rotation,
                        index: ev.index,
                    },
                    Velocity(0.0),
                ));
            }
        }
    }
}

fn update_avatar_actual_position(
    mut avatars: Query<(&PlayerTargetPosition, &mut Transform, &mut Velocity)>,
    time: Res<Time>,
) {
    for (target, mut actual, mut vel) in avatars.iter_mut() {
        // arrive at target position by time + 0.5
        let walk_time_left = target.time + 0.5 - time.elapsed_seconds();
        if walk_time_left <= 0.0 {
            actual.translation = target.translation;
            vel.0 = 0.0;
        } else {
            let walk_fraction = (time.delta_seconds() / walk_time_left).min(1.0);
            let delta = (target.translation - actual.translation) * walk_fraction;
            vel.0 = delta.length() / time.delta_seconds();
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
    }
}
