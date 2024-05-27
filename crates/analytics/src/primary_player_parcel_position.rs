// Write a system that fetch the primary player parcel position and send it to the server

use crate::data_definition::{SegmentEvent, SegmentEventExplorerMoveToParcel};
use crate::segment_system::{SegmentConfig, SegmentMetricsEvents};
use bevy::prelude::*;
use common::structs::PrimaryUser;

// TODO: already defined in scene_runner
const PARCEL_SIZE: f32 = 16.0;

pub fn primary_player_parcel_position_system(
    player: Query<&GlobalTransform, With<PrimaryUser>>,
    mut last_parcel_position: Local<IVec2>,
    mut metrics: ResMut<SegmentMetricsEvents>,
    mut segment_config: Option<ResMut<SegmentConfig>>,
) {
    let Ok(player) = player.get_single() else {
        return;
    };
    let parcel_position = ((player.translation().xz() * Vec2::new(1.0, -1.0)) / PARCEL_SIZE)
        .floor()
        .as_ivec2();
    if parcel_position != *last_parcel_position {
        let current = format!("{},{}", last_parcel_position.x, last_parcel_position.y);
        let next = format!("{},{}", parcel_position.x, parcel_position.y);
        metrics.add_event(SegmentEvent::ExplorerMoveToParcel(
            next.clone(),
            SegmentEventExplorerMoveToParcel {
                old_parcel: current,
            },
        ));

        if let Some(ref mut segment_config) = segment_config {
            segment_config.update_position(next);
        }

        *last_parcel_position = parcel_position;
    }
}
