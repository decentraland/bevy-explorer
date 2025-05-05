use bevy::prelude::*;

use common::structs::{AvatarDynamicState, PrimaryUser};
use dcl_component::{
    proto_components::kernel::comms::rfc4,
    transform_and_parent::{DclQuat, DclTranslation},
};

use crate::{
    global_crdt::GlobalCrdtState,
    movement_compressed::{Movement, Temporal},
    TransportType,
};

use super::{NetworkMessage, Transport};

pub struct BroadcastPositionPlugin;

impl Plugin for BroadcastPositionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, broadcast_position);
    }
}

const STATIC_FREQ: f64 = 1.0;
const DYNAMIC_FREQ: f64 = 0.1;

fn broadcast_position(
    player: Query<(&GlobalTransform, &AvatarDynamicState), With<PrimaryUser>>,
    transports: Query<&Transport>,
    mut last_position: Local<(Vec3, Quat, Vec3)>,
    mut last_sent: Local<f64>,
    mut last_index: Local<u32>,
    time: Res<Time>,
    global_crdt: Res<GlobalCrdtState>,
) {
    let Ok((player, dynamics)) = player.get_single() else {
        return;
    };
    let time = time.elapsed_seconds_f64();
    let elapsed = time - *last_sent;
    if elapsed < DYNAMIC_FREQ {
        return;
    }

    let (_, rotation, translation) = player.to_scale_rotation_translation();
    if elapsed < STATIC_FREQ
        && (translation - last_position.0).length_squared() < 0.01
        && rotation == last_position.1
        && (dynamics.velocity - last_position.2).length_squared() < 0.01
    {
        return;
    }

    // OLD CLIENT MESSAGES
    // (bevy uses the old version only if no new ones are received from a particular player,
    // so it doesn't use them between bevy instances)
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

    debug!("sending position: {position_packet:?}");
    let packet = rfc4::Packet {
        message: Some(rfc4::packet::Message::Position(position_packet)),
        protocol_version: 100,
    };

    for transport in transports
        .iter()
        .filter(|t| t.transport_type != TransportType::SceneRoom)
    {
        if let Err(e) = transport
            .sender
            .try_send(NetworkMessage::unreliable(&packet))
        {
            warn!("failed to update to transport: {e}");
        }
    }

    // NEW CLIENT MESSAGES
    let movement = Movement::new(
        translation,
        dynamics.velocity,
        global_crdt.realm_bounds.0,
        global_crdt.realm_bounds.1,
    );
    let temporal = Temporal::from_parts(
        time,
        false,
        rotation.to_euler(bevy::math::EulerRot::YXZ).0,
        movement.velocity_tier(),
        dynamics.move_kind,
        dynamics.ground_height < 0.2,
    );

    let movement_compressed = crate::movement_compressed::MovementCompressed { temporal, movement };

    let movement_packet = rfc4::MovementCompressed {
        temporal_data: i32::from_le_bytes(movement_compressed.temporal.into_bytes()),
        movement_data: i64::from_le_bytes(movement_compressed.movement.into_bytes()),
    };

    debug!("sending compressed: {movement_packet:?}");
    crate::movement_compressed::MovementCompressed::from_proto(movement_packet.clone());
    debug!("---");
    let packet = rfc4::Packet {
        message: Some(rfc4::packet::Message::MovementCompressed(movement_packet)),
        protocol_version: 100,
    };

    for transport in transports.iter() {
        if let Err(e) = transport
            .sender
            .try_send(NetworkMessage::unreliable(&packet))
        {
            warn!("failed to update to transport: {e}");
        }
    }

    *last_position = (translation, rotation, dynamics.velocity);
    *last_index += 1;
    *last_sent = time;
}
