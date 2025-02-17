use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use common::util::{reqwest_client, TaskCompat, TaskExt};
use std::time::Duration;

use crate::data_definition::{
    build_segment_event_batch_item, SegmentEvent, SegmentEventCommonExplorerFields,
};

pub struct SegmentMetricPlugin;

impl Plugin for SegmentMetricPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SegmentMetricTimer(Timer::new(
            Duration::from_secs(10),
            TimerMode::Repeating,
        )))
        .insert_resource(SegmentMetricsEvents {
            events: Vec::new(),
            serialized_events: Vec::new(),
        })
        .add_systems(Update, send_segment_metric_event);
    }
}

#[derive(Resource)]
pub struct SegmentConfig {
    user_id: String,
    write_key: String,
    common: SegmentEventCommonExplorerFields,
}

#[derive(Resource)]
pub struct SegmentMetricTimer(Timer);

#[derive(Resource)]
pub struct SegmentMetricsEvents {
    events: Vec<SegmentEvent>,
    serialized_events: Vec<String>,
}

const SEGMENT_EVENT_SIZE_LIMIT_BYTES: usize = 32000;
const SEGMENT_BATCH_SIZE_LIMIT_BYTES: usize = 500000;

impl SegmentMetricsEvents {
    pub fn add_event(&mut self, event: SegmentEvent) {
        self.events.push(event);
    }
}

impl SegmentConfig {
    pub fn new(user_id: String, session_id: String, version: String) -> Self {
        Self {
            user_id,
            common: SegmentEventCommonExplorerFields::new(session_id, version),
            write_key: "EAdAcIyGP6lIQAfpFF2BXpNzpj7XNWMm".into(),
        }
    }

    pub fn update_realm(&mut self, realm: String) {
        self.common.realm = realm;
    }

    pub fn update_identity(&mut self, dcl_eth_address: String, dcl_is_guest: bool) {
        self.common.dcl_eth_address = dcl_eth_address;
        self.common.dcl_is_guest = dcl_is_guest;
    }

    pub fn update_position(&mut self, position: String) {
        self.common.position = position;
    }
}

fn send_segment_metric_event(
    time: Res<Time>,
    mut timer: ResMut<SegmentMetricTimer>,
    mut metrics: ResMut<SegmentMetricsEvents>,
    config: Res<SegmentConfig>,
    mut send_task: Local<Option<Task<Result<(), anyhow::Error>>>>,
) {
    if timer.0.tick(time.delta()).just_finished() {
        if let Some(mut t) = send_task.take() {
            match t.complete() {
                Some(Ok(_)) => {}
                Some(Err(e)) => {
                    error!("send_segment_metric_event task error: {e}");
                }
                None => {
                    *send_task = Some(t);
                    return;
                }
            }
        }

        if !metrics.events.is_empty() {
            let mut accumulated_length: usize =
                metrics.serialized_events.iter().map(|s| s.len()).sum();

            while let Some(event) = metrics.events.pop() {
                let raw_event =
                    build_segment_event_batch_item(config.user_id.clone(), &config.common, event);

                let json_body =
                    serde_json::to_string(&raw_event).expect("Failed to serialize event body");

                if json_body.len() > SEGMENT_EVENT_SIZE_LIMIT_BYTES {
                    error!("Event too large: {}", json_body.len());
                    continue;
                }

                if accumulated_length + json_body.len() > SEGMENT_BATCH_SIZE_LIMIT_BYTES {
                    let write_key = config.write_key.clone();
                    let serialized_events = std::mem::take(&mut metrics.serialized_events);
                    *send_task = Some(IoTaskPool::get().spawn_compat(async move {
                        send_segment_batch(&write_key, &serialized_events).await
                    }));

                    // This events is queued until the next time is available to send events
                    metrics.serialized_events.push(json_body);
                    return;
                }

                accumulated_length += json_body.len();
                metrics.serialized_events.push(json_body);
            }

            if !metrics.serialized_events.is_empty() {
                let write_key = config.write_key.clone();
                let serialized_events = std::mem::take(&mut metrics.serialized_events);
                *send_task = Some(IoTaskPool::get().spawn_compat(async move {
                    send_segment_batch(&write_key, &serialized_events).await
                }));
            }
        }
    }
}

async fn send_segment_batch(write_key: &str, events: &[String]) -> Result<(), anyhow::Error> {
    let json_body = format!(
        "{{\"writeKey\":\"{}\",\"batch\":[{}]}}",
        write_key,
        events.join(",")
    );

    let response = reqwest_client()
        .post("https://api.segment.io/v1/batch")
        .header("Content-Type", "application/json")
        .body(json_body.clone())
        .send()
        .await?;

    if response.status().is_success() {
        info!(
            "successfully sent segment event, status: {}, payload  {}",
            response.status(),
            json_body
        );
        Ok(())
    } else {
        Err(anyhow::anyhow!("Failed to send segment event"))
    }
}
