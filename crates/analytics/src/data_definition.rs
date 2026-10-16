//! The Segment events, mirroring godot-explorer's `data_definition.rs` field-for-field so one
//! dashboard reads both clients (`dcl_renderer_type` tells them apart).

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

/// The os natively; wasm32-unknown-unknown has no os name (`std::env::consts::OS` is empty).
#[cfg(not(target_arch = "wasm32"))]
const PLATFORM: &str = std::env::consts::OS;
#[cfg(target_arch = "wasm32")]
const PLATFORM: &str = "web";

#[derive(Serialize)]
pub struct SegmentMetricEventBody {
    #[serde(rename = "type")]
    pub r#type: String,
    pub event: String,
    #[serde(rename = "userId")]
    pub user_id: String,
    // Minted when the event is queued; Segment deduplicates on it, so retries are idempotent.
    #[serde(rename = "messageId")]
    pub message_id: String,
    // Queue time, not flush time: Segment pairs it with the batch's `sentAt` to correct clock skew.
    pub timestamp: String,
    pub properties: serde_json::Value,
}

#[derive(Serialize, Clone)]
// Same for all events sent from the explorer
pub struct SegmentEventCommonExplorerFields {
    // User’s wallet id, even for guests.
    pub dcl_eth_address: String,
    // If the user is a guest or not.
    pub dcl_is_guest: bool,
    // Realm where the user is connected.
    pub realm: String,
    // Current user position.
    pub position: String,
    // What type of client was used to render the world (Web/Native/VR)
    pub dcl_renderer_type: String,
    // Explorer’s unique session id.
    pub session_id: String,
    // Explorer’s release used.
    pub renderer_version: String,
    // Device UTC offset in minutes, where `local = utc + offset`.
    pub utc_offset_minutes: i32,
}

impl SegmentEventCommonExplorerFields {
    pub fn new(session_id: String, version: String) -> Self {
        let dcl_renderer_type = format!("dao-bevy-{PLATFORM}");

        Self {
            dcl_eth_address: "unauthenticated".into(),
            dcl_is_guest: false,
            realm: "no-realm".into(),
            position: "no-position".into(),
            dcl_renderer_type,
            session_id,
            renderer_version: version,
            utc_offset_minutes: chrono::Local::now().offset().local_minus_utc() / 60,
        }
    }
}

pub enum SegmentEvent {
    PerformanceMetrics(SegmentEventPerformanceMetrics),
    ExplorerError(SegmentEventExplorerError),
    ExplorerSceneLoadTimes(SegmentEventExplorerSceneLoadTimes),
    ExplorerMoveToParcel(String, SegmentEventExplorerMoveToParcel),
    SystemInfoReport(SegmentEventSystemInfoReport),
    ChatMessageSent(SegmentEventChatMessageSent),
    ClickButton(SegmentEventClickButton),
    ScreenViewed(SegmentEventScreenViewed),
    RequestFriend(SegmentEventRequestFriend),
    AcceptFriend(SegmentEventAcceptFriend),
    BlockUser(SegmentEventBlockUser),
    Unfriend(SegmentEventUnfriend),
    InstallAttribution(SegmentEventInstallAttribution),
    GuestWalletCreation(SegmentEventGuestWalletCreation),
    Loading(Box<SegmentEventLoading>),
    AppOpened(SegmentEventAppOpened),
    FirstMoveInWorld(SegmentEventFirstMoveInWorld),
}

impl SegmentEvent {
    /// An event the web page reports through `/track`: only the screen-side events, in the
    /// shape mobile sends them.
    pub fn from_page(name: &str, properties: serde_json::Value) -> Result<Self, String> {
        fn parse<T: serde::de::DeserializeOwned>(
            properties: serde_json::Value,
        ) -> Result<T, String> {
            serde_json::from_value(properties).map_err(|e| e.to_string())
        }
        match name {
            "Screen Viewed" => parse(properties).map(Self::ScreenViewed),
            "Click Button" => parse(properties).map(Self::ClickButton),
            "Install Attribution" => parse(properties).map(Self::InstallAttribution),
            "App Opened" => parse(properties).map(Self::AppOpened),
            "Guest Wallet Creation" => parse(properties).map(Self::GuestWalletCreation),
            other => Err(format!("`{other}` is not a page event")),
        }
    }
}

#[derive(Serialize)]
pub struct SegmentEventPerformanceMetrics {
    // Total number of frames measured for this event.
    pub samples: u32,
    // Total length of the performance report.
    pub total_time: f32,
    // Amount of hiccups in 1000 frames.
    pub hiccups_in_thousand_frames: u32,
    // Total time length of hiccups measured in seconds.
    pub hiccups_time: f32,
    // Minimum delta (difference) between frames in milliseconds
    pub min_frame_time: f32,
    // Maximum delta (difference) between frames in milliseconds
    pub max_frame_time: f32,
    // Average delta (difference) between frames in milliseconds
    pub mean_frame_time: f32,
    // Median delta (difference) between frames in milliseconds
    pub median_frame_time: f32,
    // Percentile 1 of the delta (difference) between frames in milliseconds
    pub p1_frame_time: f32,
    // Percentile 5 of the delta (difference) between frames in milliseconds
    pub p5_frame_time: f32,
    // Percentile 10 of the delta (difference) between frames in milliseconds
    pub p10_frame_time: f32,
    // Percentile 20 of the delta (difference) between frames in milliseconds
    pub p20_frame_time: f32,
    // Percentile 50 of the delta (difference) between frames in milliseconds
    pub p50_frame_time: f32,
    // Percentile 75 of the delta (difference) between frames in milliseconds
    pub p75_frame_time: f32,
    // Percentile 80 of the delta (difference) between frames in milliseconds
    pub p80_frame_time: f32,
    // Percentile 90 of the delta (difference) between frames in milliseconds
    pub p90_frame_time: f32,
    // Percentile 95 of the delta (difference) between frames in milliseconds
    pub p95_frame_time: f32,
    // Percentile 99 of the delta (difference) between frames in milliseconds
    pub p99_frame_time: f32,
    // How many users where nearby the current user
    pub player_count: i32,
    // Javascript heap memory used by the scenes in kilo bytes
    pub used_jsheap_size: i32,
    // Memory used only by the explorer in kilo bytes
    pub memory_usage: i32,
}

#[derive(Serialize)]
pub struct SegmentEventExplorerError {
    // Generic or Fatal.
    pub error_type: String,
    // Error description.
    pub error_message: String,
    // Error’s stack
    pub error_stack: String,
}

#[derive(Serialize)]
pub struct SegmentEventExplorerSceneLoadTimes {
    // Unique hash for the scene.
    pub scene_hash: String,
    // Time to load in seconds.
    pub elapsed: f32,
    // Boolean flag indicating wether the scene loaded without errors.
    pub success: bool,
}

// TODO: maybe important what realm?
#[derive(Serialize)]
pub struct SegmentEventExplorerMoveToParcel {
    // Parcel where the user is coming from.
    pub old_parcel: String,
}

/// Device hardware, once per launch. Every unknown is an explicit `null` — never `""` or `-1` —
/// so a warehouse column can be typed and filtered on IS NULL.
#[derive(Serialize)]
pub struct SegmentEventSystemInfoReport {
    // Build.BRAND ("samsung") / "Apple"; null on desktop.
    pub device_brand: Option<String>,
    // Build.MODEL ("SM-A536B") / hw.machine ("iPhone14,5"); null on desktop.
    pub device_model: Option<String>,
    // "Android 14", "iOS 18.1", "macOS 15.6.1".
    pub os_version: Option<String>,
    // Build.SOC_MANUFACTURER ("Samsung", "Qualcomm") / "Apple"; null on desktop and Android < 12.
    pub soc_manufacturer: Option<String>,
    // The chipset id the device-support lookup keyed on. Null off Android.
    pub soc_model: Option<String>,
    // CPU name as the OS reports it: brand string on desktop, primary ABI on Android, null on iOS.
    pub processor_type: Option<String>,
    // Logical cores.
    pub processor_count: Option<u32>,
    // Fastest core's max clock; Android only.
    pub processor_max_freq_mhz: Option<u32>,
    // GPU as the renderer sees it ("Mali-G68", "Adreno (TM) 610", "Apple A15 GPU").
    pub graphics_device_name: Option<String>,
    // "ARM", "Qualcomm", "Apple", "NVIDIA".
    pub graphics_device_vendor: Option<String>,
    // Graphics API + version in use ("Vulkan 1.1.128", "Metal 3.1", "OpenGL ES 3.2 ...").
    pub graphics_api_version: Option<String>,
    // Total RAM.
    pub system_memory_size_mb: Option<u32>,
    // Verdict of the launch-time device-support checks: "supported" | "below_minspec" |
    // "end_of_support" | "not_checked".
    pub device_support: String,
}

// Same shape as godot's event
#[derive(Serialize)]
pub struct SegmentEventChatMessageSent {
    // Length of the message sent.
    pub length: u32,
    // Whether it is Public or Private.
    pub channel: String,
    // Whether the message typed is a command or not (if applies).
    pub is_command: bool,
    // Whether the message is Private or not.
    pub is_private: bool,
    // ID of the Community the message was sent to. Otherwise NULL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community_id: Option<String>,
    // Whether the message contains a mention from another User (i.e. @XYZ).
    pub is_mention: bool,
    // If the user is not in world, this is the screen where the event is fired.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_name: Option<String>,
}

impl SegmentEventChatMessageSent {
    pub fn new(message: &str, is_private: bool) -> Self {
        Self {
            length: message.chars().count() as u32,
            channel: if is_private { "private" } else { "nearby" }.to_owned(),
            is_command: message.starts_with('/'),
            is_private,
            community_id: None,
            is_mention: message.contains('@'),
            screen_name: Some("CHAT".to_owned()),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct SegmentEventClickButton {
    // Text of the button clicked.
    pub button_text: String,
    // Screen Name where the user clicked the button.
    pub screen_name: String,
    // JSON with extra payload, in case we need to track additional metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_properties: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct SegmentEventScreenViewed {
    // Name of the screen viewed.
    pub screen_name: String,
    // JSON with extra payload, in case we need to track additional metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_properties: Option<String>,
}

#[derive(Serialize)]
pub struct SegmentEventRequestFriend {
    // Wallet address of the user receiving the friend request.
    pub receiver_id: String,
}

#[derive(Serialize)]
pub struct SegmentEventAcceptFriend {
    // Wallet address of the user whose friend request was accepted.
    pub receiver_id: String,
    // Server-side friendship ID (for metrics mapping).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub friendship_id: Option<String>,
}

#[derive(Serialize)]
pub struct SegmentEventBlockUser {
    // Wallet address of the user being blocked.
    pub receiver_id: String,
    // Whether the blocked user was a friend at the time of blocking.
    pub is_friend: bool,
}

#[derive(Serialize)]
pub struct SegmentEventUnfriend {
    // Wallet address of the user being unfriended.
    pub receiver_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct SegmentEventInstallAttribution {
    // Raw referrer string (e.g. "utm_source=youtube&utm_campaign=xyz")
    pub referrer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utm_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utm_medium: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utm_campaign: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utm_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utm_term: Option<String>,
    // Seconds since epoch when the referrer click happened
    pub click_timestamp: i64,
    // Seconds since epoch when the install began
    pub install_timestamp: i64,
    // Whether the app was launched as a Google Play Instant app
    pub google_play_instant: bool,
    // Which mechanism reported: "ga4f_deferred_deeplink" | "play_install_referrer" | "web_url" |
    // "none". `campaign_token` says whether a campaign was found.
    pub attribution_source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub campaign_token: Option<String>,
    // Raw GA4F deep link, when that is the source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deferred_deep_link: Option<String>,
    // Whether the referrer source actually answered; false makes the fields above placeholders.
    pub referrer_settled: bool,
}

#[derive(Serialize, Deserialize)]
pub struct SegmentEventGuestWalletCreation {
    // "success" | "failure".
    pub outcome: String,
    // Whether a NEW wallet was minted for this session; always false on failure.
    pub is_new_user: bool,
    // Failure bucket: "http_4xx" | "http_5xx" | "timeout" | "network" | "bad_response" |
    // "invalid_address" | "sign_message" | "ephemeral". None on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    // Upstream HTTP status when the failure was an HTTP error. None otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u32>,
    // Wall-clock duration of the whole attempt, in milliseconds.
    pub duration_ms: u32,
}

/// Loading-event grammar version. Bump when the field set below changes.
pub const LOADING_SCHEMA_VERSION: u32 = 1;

/// One "Loading Event" per step of a loading episode, discriminated by `type` and correlated by
/// `loading_id`. Every type-specific field is `Option` and only populated for its `type`.
#[derive(Serialize, Default)]
pub struct SegmentEventLoading {
    /// "started" | "progress" | "completed" | "asset_failure" | "realm_change_failed".
    #[serde(rename = "type")]
    pub event_type: String,
    /// Correlation id for one complete load (episode). Shared by every event of that load.
    pub loading_id: u64,
    /// Schema/grammar version (LOADING_SCHEMA_VERSION).
    pub schema: u32,
    /// Coarse realm class: "world" | "genesis" | "other" | "unknown" (no URL/PII).
    pub realm_bucket: String,

    // --- type = "started" (context) ---
    /// Navigation entry point: "on_explorer_ready" | "on_teleport" | "on_world" | "on_moveto" |
    /// "on_goto_realm" | "on_changerealm" | "auto" (background streaming, never user-visible).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    /// Whether the destination realm is a dcl world. Tracks `realm_bucket == "world"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_world: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,

    // --- type = "completed" / "progress" ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    /// How the load ended: "completion" | "wall_clock_timeout" | "error" | "superseded" |
    /// "cancelled".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dismissed_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_spawn_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub complete_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_rendered_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub about_end_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_25_30_band_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase_breakdown: Option<LoadingPhaseBreakdown>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_assets_pending_at_end: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_assets_pending_peak: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets_errored: Option<i64>,
    /// Mean download throughput across the load, in megabytes/s.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_avg_mb_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_peak_mb_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_stall_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frames: Option<i64>,

    // --- type = "asset_failure" | "realm_change_failed" ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<i64>,
}

impl SegmentEventLoading {
    /// Base event with the always-present fields set and every type-specific field left `None`.
    pub fn base(event_type: &str, loading_id: u64, realm_bucket: String) -> Self {
        Self {
            event_type: event_type.to_string(),
            loading_id,
            schema: LOADING_SCHEMA_VERSION,
            realm_bucket,
            ..Default::default()
        }
    }
}

/// Entry ms of each loading phase since load begin (-1 = phase never reached).
#[derive(Serialize)]
pub struct LoadingPhaseBreakdown {
    pub metadata: i64,
    pub spawning: i64,
    pub assets: i64,
    pub ready: i64,
    pub floating_islands: i64,
    pub done: i64,
}

impl Default for LoadingPhaseBreakdown {
    fn default() -> Self {
        Self {
            metadata: -1,
            spawning: -1,
            assets: -1,
            ready: -1,
            floating_islands: -1,
            done: -1,
        }
    }
}

/// The app entered the foreground: a cold process launch or a return from background.
#[derive(Serialize, Deserialize)]
pub struct SegmentEventAppOpened {
    // "cold" when this open started the process, "warm" when it came back from background.
    pub start_kind: String,
    // "push" | "deeplink" | "icon".
    pub trigger: String,
    // The session running the last time the app was seen alive. Empty on the first launch ever.
    pub prev_session_id: String,
    // Wall-clock seconds since that mark; -1 means unknown.
    pub seconds_since_last_seen: i64,
}

/// The first non-zero movement input after spawning in world, once per login session.
#[derive(Serialize)]
pub struct SegmentEventFirstMoveInWorld {
    // Parcel "x,y".
    pub position: String,
    pub realm: String,
    pub is_guest: bool,
}

pub fn build_segment_event_batch_item(
    user_id: String,
    common: &SegmentEventCommonExplorerFields,
    event_data: SegmentEvent,
    // Captured when the event was queued, NOT at flush, so a batch keeps per-event chronology.
    created_at: DateTime<Utc>,
    message_id: String,
    // A page event's own session, when it predates this engine session.
    session_id: Option<String>,
) -> SegmentMetricEventBody {
    let (event_name, event_properties, override_position) = match event_data {
        SegmentEvent::PerformanceMetrics(event) => (
            "Performance Metrics".to_string(),
            serde_json::to_value(event).unwrap(),
            None,
        ),
        SegmentEvent::ExplorerError(event) => (
            "Explorer Error".to_string(),
            serde_json::to_value(event).unwrap(),
            None,
        ),
        SegmentEvent::ExplorerSceneLoadTimes(event) => (
            "Explorer Scene Load Times".to_string(),
            serde_json::to_value(event).unwrap(),
            None,
        ),
        SegmentEvent::ExplorerMoveToParcel(current_position, event) => (
            "Explorer Move To Parcel".to_string(),
            serde_json::to_value(event).unwrap(),
            Some(current_position),
        ),
        SegmentEvent::SystemInfoReport(event) => (
            "System Info Report".to_string(),
            serde_json::to_value(event).unwrap(),
            None,
        ),
        SegmentEvent::ChatMessageSent(event) => (
            "Chat Message Sent".to_string(),
            serde_json::to_value(event).unwrap(),
            None,
        ),
        SegmentEvent::ClickButton(event) => (
            "Click Button".to_string(),
            serde_json::to_value(event).unwrap(),
            None,
        ),
        SegmentEvent::ScreenViewed(event) => (
            "Screen Viewed".to_string(),
            serde_json::to_value(event).unwrap(),
            None,
        ),
        SegmentEvent::RequestFriend(event) => (
            "Friend Request".to_string(),
            serde_json::to_value(event).unwrap(),
            None,
        ),
        SegmentEvent::AcceptFriend(event) => (
            "Friend Accept".to_string(),
            serde_json::to_value(event).unwrap(),
            None,
        ),
        SegmentEvent::BlockUser(event) => (
            "Block User".to_string(),
            serde_json::to_value(event).unwrap(),
            None,
        ),
        SegmentEvent::Unfriend(event) => (
            "Unfriend".to_string(),
            serde_json::to_value(event).unwrap(),
            None,
        ),
        SegmentEvent::InstallAttribution(event) => (
            "Install Attribution".to_string(),
            serde_json::to_value(event).unwrap(),
            None,
        ),
        SegmentEvent::GuestWalletCreation(event) => (
            "Guest Wallet Creation".to_string(),
            serde_json::to_value(event).unwrap(),
            None,
        ),
        SegmentEvent::Loading(event) => (
            "Loading Event".to_string(),
            serde_json::to_value(*event).unwrap(),
            None,
        ),
        SegmentEvent::AppOpened(event) => (
            "App Opened".to_string(),
            serde_json::to_value(event).unwrap(),
            None,
        ),
        SegmentEvent::FirstMoveInWorld(event) => (
            "First Move In World".to_string(),
            serde_json::to_value(event).unwrap(),
            None,
        ),
    };

    let mut properties = serde_json::to_value(common).unwrap();
    // merge specific event properties with common properties
    for (k, v) in event_properties.as_object().unwrap().iter() {
        properties[k] = v.clone();
    }

    if let Some(session_id) = session_id {
        properties["session_id"] = serde_json::Value::String(session_id);
    }

    if let Some(position) = override_position {
        properties["position"] = serde_json::Value::String(position);
    }

    // ISO-8601 UTC with millisecond precision, e.g. "2026-07-02T13:38:08.243Z".
    let iso_ts = created_at.to_rfc3339_opts(SecondsFormat::Millis, true);
    // Client-owned copy of the event time that no Segment clock-skew correction rewrites.
    properties["client_timestamp"] = serde_json::Value::String(iso_ts.clone());

    SegmentMetricEventBody {
        r#type: "track".to_string(),
        event: event_name,
        user_id,
        message_id,
        timestamp: iso_ts,
        properties,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn common() -> SegmentEventCommonExplorerFields {
        SegmentEventCommonExplorerFields {
            dcl_eth_address: "0xabc".into(),
            dcl_is_guest: true,
            realm: "no-realm".into(),
            position: "no-position".into(),
            dcl_renderer_type: "dao-bevy-web".into(),
            session_id: "session".into(),
            renderer_version: "1.0.0".into(),
            utc_offset_minutes: -180,
        }
    }

    #[test]
    fn batch_item_carries_queue_time_message_id_and_session_override() {
        let created_at = DateTime::parse_from_rfc3339("2026-07-02T13:38:08.243Z")
            .unwrap()
            .with_timezone(&Utc);
        let body = build_segment_event_batch_item(
            "user".into(),
            &common(),
            SegmentEvent::from_page(
                "Screen Viewed",
                serde_json::json!({ "screen_name": "LOBBY" }),
            )
            .unwrap(),
            created_at,
            "msg-1".into(),
            Some("page-session".into()),
        );
        let props = body.properties.as_object().unwrap();

        assert_eq!(body.r#type, "track");
        assert_eq!(body.event, "Screen Viewed");
        assert_eq!(body.message_id, "msg-1");
        assert_eq!(body.timestamp, "2026-07-02T13:38:08.243Z");
        assert_eq!(props["client_timestamp"], "2026-07-02T13:38:08.243Z");
        assert_eq!(props["session_id"], "page-session");
        assert_eq!(props["screen_name"], "LOBBY");
        assert_eq!(props["dcl_renderer_type"], "dao-bevy-web");
        assert_eq!(props["utc_offset_minutes"], -180);
    }

    // A warehouse column is only typed and IS NULL-filterable if every unknown lands as an
    // explicit null, so no field may be skipped or sent as "" / -1.
    #[test]
    fn system_info_report_sends_unknowns_as_null() {
        let event = SegmentEvent::SystemInfoReport(SegmentEventSystemInfoReport {
            device_brand: Some("samsung".into()),
            device_model: None,
            os_version: None,
            soc_manufacturer: None,
            soc_model: None,
            processor_type: None,
            processor_count: Some(8),
            processor_max_freq_mhz: None,
            graphics_device_name: None,
            graphics_device_vendor: None,
            graphics_api_version: None,
            system_memory_size_mb: None,
            device_support: "not_checked".into(),
        });

        let body = build_segment_event_batch_item(
            "user".into(),
            &common(),
            event,
            Utc::now(),
            "message".into(),
            None,
        );
        let props = body.properties.as_object().unwrap();

        assert_eq!(body.event, "System Info Report");
        assert_eq!(props["device_brand"], "samsung");
        assert_eq!(props["processor_count"], 8);
        assert_eq!(props["device_support"], "not_checked");
        assert_eq!(props["session_id"], "session");
        for key in [
            "device_model",
            "os_version",
            "soc_manufacturer",
            "soc_model",
            "processor_type",
            "processor_max_freq_mhz",
            "graphics_device_name",
            "graphics_device_vendor",
            "graphics_api_version",
            "system_memory_size_mb",
        ] {
            assert!(props[key].is_null(), "{key} must be an explicit null");
        }
    }
}
