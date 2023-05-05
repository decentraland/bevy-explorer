use bevy::prelude::*;

use crate::{
    dcl_component::{
        proto_components::kernel::comms::rfc4,
        transform_and_parent::{DclQuat, DclTranslation},
    },
    scene_runner::PrimaryCamera,
};

use super::{NetworkMessage, Transport};

pub struct BroadcastPositionPlugin;

impl Plugin for BroadcastPositionPlugin {
    fn build(&self, app: &mut App) {
        app.add_system(broadcast_position);
    }
}

const STATIC_FREQ: f64 = 1.0;
const DYNAMIC_FREQ: f64 = 0.1;

fn broadcast_position(
    player: Query<&GlobalTransform, With<PrimaryCamera>>,
    transports: Query<&Transport>,
    mut last_position: Local<(Vec3, Quat)>,
    mut last_sent: Local<f64>,
    mut last_index: Local<u32>,
    time: Res<Time>,
) {
    let Ok(player) = player.get_single() else {
        return;
    };
    let time = time.elapsed_seconds_f64();
    let elapsed = *last_sent - time;
    if elapsed < DYNAMIC_FREQ {
        return;
    }

    let (_, rotation, translation) = player.to_scale_rotation_translation();
    if elapsed < STATIC_FREQ && (translation, rotation) == *last_position {
        return;
    }

    let dcl_position = DclTranslation::from_bevy_translation(translation);
    let dcl_rotation = DclQuat::from_bevy_quat(rotation);
    let position_packet = rfc4::Position {
        index: *last_index,
        position_x: dcl_position.0[0],
        position_y: dcl_position.0[1],
        position_z: dcl_position.0[2],
        rotation_x: dcl_rotation.0[0],
        rotation_y: dcl_rotation.0[1],
        rotation_z: dcl_rotation.0[2],
        rotation_w: dcl_rotation.0[3],
    };

    let packet = rfc4::Packet {
        message: Some(rfc4::packet::Message::Position(position_packet)),
    };

    for transport in transports.iter() {
        if let Err(e) = transport
            .sender
            .try_send(NetworkMessage::unreliable(&packet))
        {
            warn!("failed to update to transport: {e}");
        }
    }

    *last_position = (translation, rotation);
    *last_index += 1;
    *last_sent = time;
}
