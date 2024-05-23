use crate::{frame::FrameMetricPlugin, segment_system::SegmentMetricPlugin};
use bevy::prelude::*;
pub struct MetricsPlugin;

impl Plugin for MetricsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((FrameMetricPlugin, SegmentMetricPlugin));
    }
}
