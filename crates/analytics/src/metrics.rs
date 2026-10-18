use crate::{
    crash::refresh_crash_snapshot, frame::FrameMetricPlugin, in_world::report_in_world,
    loading::report_loading_screen,
    primary_player_parcel_position::primary_player_parcel_position_system,
    segment_system::SegmentMetricPlugin,
};
use bevy::prelude::*;
use common::structs::ReturnedToWorld;
pub struct MetricsPlugin;

impl Plugin for MetricsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((FrameMetricPlugin, SegmentMetricPlugin));
        // written by restricted_actions; registered here too so the reader never depends on it
        app.add_event::<ReturnedToWorld>();
        app.add_systems(
            Update,
            (
                primary_player_parcel_position_system,
                report_in_world,
                report_loading_screen,
                refresh_crash_snapshot,
            ),
        );
        crate::page_events::register(app);
    }
}
