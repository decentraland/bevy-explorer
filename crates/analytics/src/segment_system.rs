use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use chrono::{DateTime, SecondsFormat, Utc};
use common::util::{reqwest_client, TaskCompat, TaskExt};
use uuid::Uuid;

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
    // Nothing is sent before the user accepted the terms; events queue up meanwhile.
    consent: bool,
}

#[derive(Resource)]
pub struct SegmentMetricTimer(Timer);

/// An event plus what was captured the moment it was queued, so a batch keeps per-event
/// chronology and a retry re-sends the same `messageId`.
pub struct QueuedSegmentEvent {
    pub event: SegmentEvent,
    pub created_at: DateTime<Utc>,
    pub message_id: String,
    /// Overrides the common `session_id`: a page event queued before this engine session
    /// existed keeps its own.
    pub session_id: Option<String>,
}

#[derive(Resource, Default)]
pub struct SegmentMetricsEvents {
    events: VecDeque<QueuedSegmentEvent>,
    serialized_events: Vec<String>,
    // Events of a batch whose send failed, re-queued on the next tick with their original
    // messageId/timestamp.
    retry_buffer: Arc<Mutex<Vec<String>>>,
    overflow_logged: bool,
}

const SEGMENT_EVENT_SIZE_LIMIT_BYTES: usize = 32000;
const SEGMENT_BATCH_SIZE_LIMIT_BYTES: usize = 500000;
// The post-consent backlog goes out in several requests, each within the client's timeout.
const SEGMENT_BATCH_MAX_EVENTS: usize = 200;
/// Events held while consent is pending (or the network is down); the oldest go first.
pub const MAX_QUEUED_EVENTS: usize = 2000;
pub const MAX_RETRY_BUFFER_EVENTS: usize = 100;

impl SegmentMetricsEvents {
    pub fn add_event(&mut self, event: SegmentEvent) {
        self.queue(QueuedSegmentEvent {
            event,
            created_at: Utc::now(),
            message_id: Uuid::new_v4().to_string(),
            session_id: None,
        });
    }

    /// Queue an event the web page stamped itself (see `page_events`).
    pub fn add_page_event(
        &mut self,
        name: &str,
        properties: serde_json::Value,
        created_at: DateTime<Utc>,
        message_id: String,
        session_id: String,
    ) -> Result<(), String> {
        self.queue(QueuedSegmentEvent {
            event: SegmentEvent::from_page(name, properties)?,
            created_at,
            message_id,
            session_id: Some(session_id),
        });
        Ok(())
    }

    pub fn queued(&self) -> usize {
        self.events.len()
    }

    fn queue(&mut self, event: QueuedSegmentEvent) {
        if self.events.len() >= MAX_QUEUED_EVENTS {
            self.events.pop_front();
            if !self.overflow_logged {
                warn!("segment queue exceeded {MAX_QUEUED_EVENTS} events; dropping the oldest");
                self.overflow_logged = true;
            }
        }
        self.events.push_back(event);
    }
}

impl SegmentConfig {
    pub fn new(user_id: String, session_id: String, version: String) -> Self {
        Self {
            user_id,
            common: SegmentEventCommonExplorerFields::new(session_id, version),
            write_key: "EAdAcIyGP6lIQAfpFF2BXpNzpj7XNWMm".into(),
            consent: false,
        }
    }

    pub fn set_consent(&mut self, consent: bool) {
        if self.consent != consent {
            info!("segment consent set to {consent}");
        }
        self.consent = consent;
    }

    pub fn consent(&self) -> bool {
        self.consent
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
    mut send_task: Local<Option<Task<Result<(), anyhow::Error>>>>,
    mut had_consent: Local<bool>,
) {
    // consent granted flushes what queued up without waiting for the timer
    let consent_granted = config.consent && !*had_consent;
    *had_consent = config.consent;
    if !timer.0.tick(time.delta()).just_finished() && !consent_granted {
        return;
    }

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

    if !config.consent {
        return;
    }

    let metrics = &mut *metrics;
    {
        let mut retry = metrics.retry_buffer.lock().unwrap();
        if !retry.is_empty() {
            debug!("re-queuing {} events from the retry buffer", retry.len());
            metrics.serialized_events.append(&mut retry);
        }
    }

    if metrics.events.is_empty() && metrics.serialized_events.is_empty() {
        return;
    }

    let mut accumulated_length: usize = metrics.serialized_events.iter().map(|s| s.len()).sum();

    while let Some(queued) = metrics.events.pop_front() {
        let raw_event = build_segment_event_batch_item(
            config.user_id.clone(),
            &config.common,
            queued.event,
            queued.created_at,
            queued.message_id,
            queued.session_id,
        );

        let json_body = serde_json::to_string(&raw_event).expect("Failed to serialize event body");

        if json_body.len() > SEGMENT_EVENT_SIZE_LIMIT_BYTES {
            error!("Event too large: {}", json_body.len());
            continue;
        }

        if accumulated_length + json_body.len() > SEGMENT_BATCH_SIZE_LIMIT_BYTES
            || metrics.serialized_events.len() >= SEGMENT_BATCH_MAX_EVENTS
        {
            *send_task = Some(spawn_send(
                config.write_key.clone(),
                std::mem::take(&mut metrics.serialized_events),
                metrics.retry_buffer.clone(),
            ));
            // this event waits for the next send
            metrics.serialized_events.push(json_body);
            return;
        }

        accumulated_length += json_body.len();
        metrics.serialized_events.push(json_body);
    }

    if !metrics.serialized_events.is_empty() {
        *send_task = Some(spawn_send(
            config.write_key.clone(),
            std::mem::take(&mut metrics.serialized_events),
            metrics.retry_buffer.clone(),
        ));
    }
}

fn spawn_send(
    write_key: String,
    events: Vec<String>,
    retry_buffer: Arc<Mutex<Vec<String>>>,
) -> Task<Result<(), anyhow::Error>> {
    IoTaskPool::get()
        .spawn_compat(async move { send_segment_batch(&write_key, events, retry_buffer).await })
}

async fn send_segment_batch(
    write_key: &str,
    events: Vec<String>,
    retry_buffer: Arc<Mutex<Vec<String>>>,
) -> Result<(), anyhow::Error> {
    // Segment pairs the batch's `sentAt` with each event's `timestamp` to correct clock skew.
    let sent_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let json_body = format!(
        "{{\"writeKey\":\"{}\",\"sentAt\":\"{}\",\"batch\":[{}]}}",
        write_key,
        sent_at,
        events.join(",")
    );

    let response = reqwest_client()
        .post("https://api.segment.io/v1/batch")
        .header("Content-Type", "application/json")
        .body(json_body.clone())
        .send()
        .await;

    match response {
        Ok(response) if response.status().is_success() => {
            debug!(
                "successfully sent segment batch, status: {}, payload {}",
                response.status(),
                json_body
            );
            Ok(())
        }
        // a 4xx is a rejected payload: re-sending it would never succeed (bar a timeout or
        // a rate limit, which the retry buffer handles with the 5xx)
        Ok(response)
            if response.status().is_client_error()
                && !matches!(response.status().as_u16(), 408 | 429) =>
        {
            Err(anyhow::anyhow!(
                "segment rejected the batch ({} events): {}",
                events.len(),
                response.status()
            ))
        }
        Ok(response) => {
            let status = response.status();
            let count = events.len();
            buffer_for_retry(&retry_buffer, events);
            Err(anyhow::anyhow!(
                "segment batch ({count} events) failed with {status}; buffered for retry"
            ))
        }
        Err(e) => {
            let count = events.len();
            buffer_for_retry(&retry_buffer, events);
            Err(anyhow::anyhow!(
                "segment batch ({count} events) failed to send; buffered for retry: {e}"
            ))
        }
    }
}

fn buffer_for_retry(retry_buffer: &Mutex<Vec<String>>, events: Vec<String>) {
    let mut retry = retry_buffer.lock().unwrap();
    retry.extend(events);
    if retry.len() > MAX_RETRY_BUFFER_EVENTS {
        let overflow = retry.len() - MAX_RETRY_BUFFER_EVENTS;
        retry.drain(0..overflow);
        warn!("segment retry buffer exceeded {MAX_RETRY_BUFFER_EVENTS} events; dropped {overflow} oldest");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_event_with_unlisted_name_or_missing_fields_is_rejected() {
        let mut metrics = SegmentMetricsEvents::default();
        let now = Utc::now();

        assert!(metrics
            .add_page_event(
                "Performance Metrics",
                serde_json::json!({}),
                now,
                "m1".into(),
                "s".into()
            )
            .is_err());
        assert!(metrics
            .add_page_event(
                "Screen Viewed",
                serde_json::json!({ "button_text": "no screen name" }),
                now,
                "m2".into(),
                "s".into()
            )
            .is_err());
        assert_eq!(metrics.queued(), 0);

        metrics
            .add_page_event(
                "Screen Viewed",
                serde_json::json!({ "screen_name": "LOBBY" }),
                now,
                "m3".into(),
                "s".into(),
            )
            .unwrap();
        assert_eq!(metrics.queued(), 1);
    }
}
