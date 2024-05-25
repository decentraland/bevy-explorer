use crate::{
    frame::FrameMetricPlugin,
    primary_player_parcel_position::primary_player_parcel_position_system,
    segment_system::SegmentMetricPlugin,
};
use bevy::prelude::*;
pub struct MetricsPlugin;

impl Plugin for MetricsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((FrameMetricPlugin, SegmentMetricPlugin));
        app.add_systems(Update, primary_player_parcel_position_system);
    }
}
