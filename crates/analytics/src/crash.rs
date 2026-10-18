//! A crash report the host can post once the engine no longer can: the engine keeps what an
//! Explorer Error batch needs in a static snapshot, and [`crash_batch`] builds the request from
//! it. On the web the page thread holds a wasm instance on the shared memory, so `src/web.rs`
//! exposes it to boot.js's crash watchdog there. The trapped engine worker may hold any lock, so
//! the snapshot is only ever `try_lock`ed on that path.

use std::sync::Mutex;

use bevy::prelude::*;
use chrono::Utc;
use common::structs::AppConfig;

use crate::{
    data_definition::{
        build_segment_event_batch_item, SegmentEvent, SegmentEventCommonExplorerFields,
        SegmentEventExplorerError,
    },
    segment_system::{batch_body, SegmentConfig, SEGMENT_BATCH_URL},
};

struct Snapshot {
    user_id: String,
    write_key: String,
    /// the terms are accepted: nothing goes out otherwise, a crash included
    consent: bool,
    common: SegmentEventCommonExplorerFields,
}

static SNAPSHOT: Mutex<Option<Snapshot>> = Mutex::new(None);

pub fn refresh_crash_snapshot(config: Res<SegmentConfig>, app_config: Res<AppConfig>) {
    if !config.is_changed() && !app_config.is_changed() {
        return;
    }
    if let Ok(mut snapshot) = SNAPSHOT.lock() {
        *snapshot = Some(Snapshot {
            user_id: config.user_id().to_owned(),
            write_key: config.write_key().to_owned(),
            consent: app_config.terms_accepted(),
            common: config.common().clone(),
        });
    }
}

/// The Segment request for an Explorer Error, `(url, body)`, with the common fields as the engine
/// last knew them. None before the terms are accepted, or while the engine holds the snapshot.
pub fn crash_batch(message: &str, source: &str) -> Option<(String, String)> {
    let guard = SNAPSHOT.try_lock().ok()?;
    let snapshot = guard.as_ref().filter(|s| s.consent)?;
    let item = build_segment_event_batch_item(
        snapshot.user_id.clone(),
        &snapshot.common,
        SegmentEvent::ExplorerError(SegmentEventExplorerError {
            error_type: "Crash".to_owned(),
            error_message: format!("[{source}] {message}"),
            error_stack: String::new(),
        }),
        Utc::now(),
    );
    let item = serde_json::to_string(&item).ok()?;
    Some((
        SEGMENT_BATCH_URL.to_owned(),
        batch_body(&snapshot.write_key, &[item]),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // One static for the whole test binary: this is the only test that touches it.
    #[test]
    fn crash_batch_needs_consent_and_carries_the_snapshot() {
        let common = SegmentEventCommonExplorerFields::new("s".into(), "1.0".into(), None);
        let set = |consent| {
            *SNAPSHOT.lock().unwrap() = Some(Snapshot {
                user_id: "u".into(),
                write_key: "k".into(),
                consent,
                common: common.clone(),
            })
        };
        assert!(crash_batch("panicked at x", "panic").is_none());
        set(false);
        assert!(crash_batch("panicked at x", "panic").is_none());
        set(true);
        let (url, body) = crash_batch("panicked at x", "panic").unwrap();
        assert_eq!(url, SEGMENT_BATCH_URL);
        let body: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body["writeKey"], "k");
        let item = &body["batch"][0];
        assert_eq!(item["event"], "Explorer Error");
        assert_eq!(item["userId"], "u");
        assert_eq!(item["properties"]["session_id"], "s");
        assert_eq!(item["properties"]["error_type"], "Crash");
        assert_eq!(item["properties"]["error_message"], "[panic] panicked at x");
        assert!(item["timestamp"].is_string());
    }
}
