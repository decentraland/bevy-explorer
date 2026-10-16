//! `/track <base64>`: the web page's analytics events, queued into the engine's Segment batch.
//! The payload is one base64 token (the console splits on whitespace) of a JSON array of
//! `{ event, properties, created_at, message_id, session_id }`, stamped by the page at call time.

use base64::Engine;
use bevy::prelude::*;
use bevy_console::ConsoleCommand;
use chrono::{DateTime, Utc};
use console::DoAddConsoleCommand;
use serde::Deserialize;

use crate::segment_system::SegmentMetricsEvents;

pub const MAX_TRACK_ITEMS: usize = 50;

pub fn register(app: &mut App) {
    app.add_console_command::<TrackCommand, _>(track_cmd);
}

/// Queue analytics events from the host page: standard base64 of a JSON array of
/// `{event, properties, created_at, message_id, session_id}`, at most 50 per command
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/track")]
struct TrackCommand {
    payload: String,
}

// Always an ok reply: the page treats a rejection as "the engine never got it" and re-sends,
// and a batch the engine turned down would never get better.
fn track_cmd(mut input: ConsoleCommand<TrackCommand>, mut metrics: ResMut<SegmentMetricsEvents>) {
    if let Some(Ok(command)) = input.take() {
        match ingest_track_payload(&command.payload, &mut metrics) {
            Ok(queued) => input.reply_ok(format!("ok {queued}")),
            Err(e) => {
                warn!("/track: {e}");
                input.reply_ok(format!("rejected: {e}"));
            }
        }
    }
}

#[derive(Deserialize)]
struct TrackItem {
    event: String,
    properties: serde_json::Value,
    created_at: String,
    message_id: String,
    session_id: String,
}

/// Queues every valid item and returns how many; an invalid item is skipped and named in the
/// error (with how many were still queued), which the rest of the batch survives.
pub fn ingest_track_payload(
    payload: &str,
    metrics: &mut SegmentMetricsEvents,
) -> Result<usize, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|e| format!("payload is not base64: {e}"))?;
    let items: Vec<serde_json::Value> =
        serde_json::from_slice(&bytes).map_err(|e| format!("payload is not a JSON array: {e}"))?;
    if items.len() > MAX_TRACK_ITEMS {
        return Err(format!(
            "{} events in one command, max {MAX_TRACK_ITEMS}",
            items.len()
        ));
    }

    let mut queued = 0;
    let mut first_rejected = None;
    for (index, item) in items.into_iter().enumerate() {
        match queue_item(item, metrics) {
            Ok(()) => queued += 1,
            Err(e) => {
                first_rejected.get_or_insert_with(|| format!("item {index} rejected: {e}"));
            }
        }
    }
    match first_rejected {
        None => Ok(queued),
        Some(e) => Err(format!("{e} (queued {queued})")),
    }
}

fn queue_item(item: serde_json::Value, metrics: &mut SegmentMetricsEvents) -> Result<(), String> {
    let item: TrackItem = serde_json::from_value(item).map_err(|e| e.to_string())?;
    let created_at = DateTime::parse_from_rfc3339(&item.created_at)
        .map_err(|e| format!("created_at `{}`: {e}", item.created_at))?
        .with_timezone(&Utc);
    metrics.add_page_event(
        &item.event,
        item.properties,
        created_at,
        item.message_id,
        item.session_id,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(items: serde_json::Value) -> String {
        base64::engine::general_purpose::STANDARD.encode(items.to_string())
    }

    #[test]
    fn a_rejected_item_does_not_drop_the_others() {
        let mut metrics = SegmentMetricsEvents::default();
        let ok = |event: &str| {
            serde_json::json!({
                "event": event,
                "properties": { "screen_name": "LOBBY", "button_text": "JUMP_IN" },
                "created_at": "2026-07-02T13:38:08.243Z",
                "message_id": "m",
                "session_id": "s",
            })
        };
        let bad_time = serde_json::json!({
            "event": "Screen Viewed",
            "properties": {},
            "created_at": "yesterday",
            "message_id": "m",
            "session_id": "s",
        });

        let payload = encode(serde_json::json!([
            ok("Screen Viewed"),
            bad_time,
            ok("Click Button")
        ]));
        let err = ingest_track_payload(&payload, &mut metrics).unwrap_err();
        assert!(
            err.starts_with("item 1 rejected: created_at `yesterday`"),
            "{err}"
        );
        assert_eq!(metrics.queued(), 2);

        let payload = encode(serde_json::json!([ok("Screen Viewed")]));
        assert_eq!(ingest_track_payload(&payload, &mut metrics), Ok(1));
        assert!(ingest_track_payload("not base64!", &mut metrics).is_err());
        assert_eq!(metrics.queued(), 3);
    }
}
