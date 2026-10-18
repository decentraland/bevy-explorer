use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use chrono::{DateTime, Utc};
use common::{
    structs::AppConfig,
    util::{reqwest_client, TaskCompat, TaskExt},
};
use std::{collections::VecDeque, time::Duration};

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
        .init_resource::<SegmentMetricsEvents>()
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

/// An event and the moment it was queued, the time it carries when it goes out later: behind
/// the consent gate a queue can hold the whole sign-in.
struct QueuedEvent {
    event: SegmentEvent,
    queued_at: DateTime<Utc>,
}

#[derive(Resource, Default)]
pub struct SegmentMetricsEvents {
    events: VecDeque<QueuedEvent>,
    serialized_events: Vec<String>,
    overflow_logged: bool,
}

/// Events held while consent is pending; past this the oldest go.
const MAX_QUEUED_EVENTS: usize = 2000;

// The browser build goes through the first-party proxy, which ad blockers leave alone.
#[cfg(target_arch = "wasm32")]
const SEGMENT_BATCH_URL: &str = "https://api.e.decentraland.org/v1/batch";
#[cfg(not(target_arch = "wasm32"))]
const SEGMENT_BATCH_URL: &str = "https://api.segment.io/v1/batch";
const SEGMENT_EVENT_SIZE_LIMIT_BYTES: usize = 32000;
const SEGMENT_BATCH_SIZE_LIMIT_BYTES: usize = 500000;

impl SegmentMetricsEvents {
    pub fn add_event(&mut self, event: SegmentEvent) {
        if self.events.len() >= MAX_QUEUED_EVENTS {
            self.events.pop_front();
            if !self.overflow_logged {
                warn!("segment queue exceeded {MAX_QUEUED_EVENTS} events; dropping the oldest");
                self.overflow_logged = true;
            }
        }
        self.events.push_back(QueuedEvent {
            event,
            queued_at: Utc::now(),
        });
    }
}

impl SegmentConfig {
    pub fn new(
        user_id: String,
        session_id: String,
        version: String,
        campaign_anon_user_id: Option<String>,
    ) -> Self {
        Self {
            user_id,
            common: SegmentEventCommonExplorerFields::new(
                session_id,
                version,
                campaign_anon_user_id,
            ),
            write_key: "EAdAcIyGP6lIQAfpFF2BXpNzpj7XNWMm".into(),
        }
    }

    pub fn common(&self) -> &SegmentEventCommonExplorerFields {
        &self.common
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
    app_config: Res<AppConfig>,
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

        // nothing leaves before the user accepted the terms; events keep queuing meanwhile
        if !app_config.terms_accepted() {
            return;
        }

        if !metrics.events.is_empty() {
            let mut accumulated_length: usize =
                metrics.serialized_events.iter().map(|s| s.len()).sum();

            while let Some(QueuedEvent { event, queued_at }) = metrics.events.pop_front() {
                let raw_event = build_segment_event_batch_item(
                    config.user_id.clone(),
                    &config.common,
                    event,
                    queued_at,
                );

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
        .post(SEGMENT_BATCH_URL)
        .header("Content-Type", "application/json")
        .body(json_body.clone())
        .send()
        .await?;

    if response.status().is_success() {
        debug!(
            "successfully sent segment event, status: {}, payload  {}",
            response.status(),
            json_body
        );
        Ok(())
    } else {
        Err(anyhow::anyhow!("Failed to send segment event"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_definition::SegmentEventTimeSpentInWorld;

    // Held events go out oldest first; past the cap the oldest are the ones dropped.
    #[test]
    fn queue_is_fifo_and_capped() {
        let mut metrics = SegmentMetricsEvents::default();
        for i in 0..=MAX_QUEUED_EVENTS {
            metrics.add_event(SegmentEvent::TimeSpentInWorld(
                SegmentEventTimeSpentInWorld {
                    time_spent: i as f32,
                    realm_name: "main".into(),
                },
            ));
        }
        assert_eq!(metrics.events.len(), MAX_QUEUED_EVENTS);
        let spent = |q: &QueuedEvent| match &q.event {
            SegmentEvent::TimeSpentInWorld(e) => e.time_spent,
            _ => unreachable!(),
        };
        assert_eq!(spent(metrics.events.front().unwrap()), 1.0);
        assert_eq!(
            spent(metrics.events.back().unwrap()),
            MAX_QUEUED_EVENTS as f32
        );
        assert!(metrics.overflow_logged);
    }
}
