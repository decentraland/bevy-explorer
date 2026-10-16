use crate::{
    frame::FrameMetricPlugin, in_world::report_in_world,
    primary_player_parcel_position::primary_player_parcel_position_system,
    segment_system::SegmentMetricPlugin,
};
use bevy::prelude::*;
pub struct MetricsPlugin;

impl Plugin for MetricsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((FrameMetricPlugin, SegmentMetricPlugin));
        app.add_systems(
            Update,
            (primary_player_parcel_position_system, report_in_world),
        );
        crate::page_events::register(app);
    }
}
