use bevy::prelude::*;

use crate::{
    data_definition::{SegmentEvent, SegmentEventPerformanceMetrics},
    segment_system::SegmentMetricsEvents,
};

#[derive(Resource)]
pub(crate) struct Frame {
    dt_ms_vec: Vec<f32>,
    hiccups_count: u32,
    hiccups_time_ms: f32,
    sum_dt: f32,
}

const HICCUP_THRESHOLD_MS: f32 = 50.0;
const FRAME_AMOUNT_TO_MEASURE: usize = 1000;
pub struct FrameMetricPlugin;

impl Plugin for FrameMetricPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Frame {
            dt_ms_vec: Vec::new(),
            hiccups_count: 0,
            hiccups_time_ms: 0.0,
            sum_dt: 0.0,
        })
        .add_systems(Update, metrics_frame_system);
    }
}

fn metrics_frame_system(
    time: Res<Time>,
    mut frame_data: ResMut<Frame>,
    mut metrics: ResMut<SegmentMetricsEvents>,
) {
    let dt = 1000.0 * time.delta_seconds() as f32;
    frame_data.sum_dt += dt;
    frame_data.dt_ms_vec.push(dt);
    if dt > HICCUP_THRESHOLD_MS {
        frame_data.hiccups_count += 1;
        frame_data.hiccups_time_ms += dt;
    }

    if frame_data.dt_ms_vec.len() >= FRAME_AMOUNT_TO_MEASURE {
        // All the dt >= 0
        frame_data
            .dt_ms_vec
            .sort_by(|a, b| a.partial_cmp(b).unwrap());

        let n_samples = frame_data.dt_ms_vec.len();
        let median_frame_time = frame_data.dt_ms_vec[n_samples / 2];
        let p1_frame_time = frame_data.dt_ms_vec[(n_samples * 1) / 100];
        let p5_frame_time = frame_data.dt_ms_vec[(n_samples * 5) / 100];
        let p10_frame_time = frame_data.dt_ms_vec[(n_samples * 10) / 100];
        let p20_frame_time = frame_data.dt_ms_vec[(n_samples * 20) / 100];
        let p50_frame_time = frame_data.dt_ms_vec[(n_samples * 50) / 100];
        let p75_frame_time = frame_data.dt_ms_vec[(n_samples * 75) / 100];
        let p80_frame_time = frame_data.dt_ms_vec[(n_samples * 80) / 100];
        let p90_frame_time = frame_data.dt_ms_vec[(n_samples * 90) / 100];
        let p95_frame_time = frame_data.dt_ms_vec[(n_samples * 95) / 100];
        let p99_frame_time = frame_data.dt_ms_vec[(n_samples * 99) / 100];

        metrics.add_event(SegmentEvent::PerformanceMetrics(
            SegmentEventPerformanceMetrics {
                samples: n_samples as u32,
                total_time: frame_data.sum_dt,
                hiccups_in_thousand_frames: frame_data.hiccups_count, // TODO: if FRAME_AMOUNT_TO_MEASURE is != 1000, this be measured in a different way
                hiccups_time: frame_data.hiccups_time_ms / 1000.0,
                min_frame_time: *frame_data.dt_ms_vec.first().unwrap(),
                max_frame_time: *frame_data.dt_ms_vec.last().unwrap(),
                mean_frame_time: frame_data.sum_dt / n_samples as f32,
                median_frame_time,
                p1_frame_time,
                p5_frame_time,
                p10_frame_time,
                p20_frame_time,
                p50_frame_time,
                p75_frame_time,
                p80_frame_time,
                p90_frame_time,
                p95_frame_time,
                p99_frame_time,

                // TODO
                player_count: -1,
                used_jsheap_size: -1,
                memory_usage: -1,
            },
        ));

        frame_data.dt_ms_vec.resize(0, 0.0);
        frame_data.hiccups_count = 0;
        frame_data.hiccups_time_ms = 0.0;
        frame_data.sum_dt = 0.0;
    }
}
