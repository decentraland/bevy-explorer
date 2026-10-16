use crate::{
    frame::FrameMetricPlugin, page_events,
    primary_player_parcel_position::primary_player_parcel_position_system,
    segment_system::SegmentMetricPlugin, system_info::report_system_info,
};
use bevy::prelude::*;
pub struct MetricsPlugin;

impl Plugin for MetricsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((FrameMetricPlugin, SegmentMetricPlugin));
        app.add_systems(
            Update,
            (primary_player_parcel_position_system, report_system_info),
        );
        page_events::register(app);
    }
}
