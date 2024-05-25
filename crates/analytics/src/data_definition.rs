use serde::Serialize;

#[derive(Serialize)]
pub struct SegmentMetricEventBody {
    #[serde(rename = "type")]
    r#type: String,
    event: String,
    #[serde(rename = "userId")]
    user_id: String,
    properties: serde_json::Value,
}

#[derive(Serialize)]
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
}

impl SegmentEventCommonExplorerFields {
    pub fn new(session_id: String, version: String) -> Self {
        Self {
            dcl_eth_address: "".into(),
            dcl_is_guest: true,
            realm: "".into(),
            position: "".into(),
            dcl_renderer_type: "dao-bevy".into(),
            session_id,
            renderer_version: version,
        }
    }
}

pub enum SegmentEvent {
    PerformanceMetrics(SegmentEventPerformanceMetrics),
    ExplorerError(SegmentEventExplorerError),
    ExplorerSceneLoadTimes(SegmentEventExplorerSceneLoadTimes),
    ExplorerMoveToParcel(SegmentEventExplorerMoveToParcel),
    SystemInfoReport(SegmentEventSystemInfoReport),
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
    scene_hash: String,
    // Time to load in seconds.
    elapsed: f32,
    // Boolean flag indicating wether the scene loaded without errors.
    success: bool,
}

// TODO: maybe important what realm?
#[derive(Serialize)]
pub struct SegmentEventExplorerMoveToParcel {
    // Parcel where the user is coming from.
    old_parcel: String,
}

#[derive(Serialize)]
pub struct SegmentEventSystemInfoReport {
    // Processor used by the user.
    processor_type: String,
    // How many processors are available in user’s device.
    processor_count: u32,
    // Graphic Device used by the user.
    graphics_device_name: String,
    // Graphic device memory in mb.
    graphics_memory_mb: u32,
    // RAM memory in mb.
    system_memory_size_mb: u32,
}

pub fn build_segment_event_batch_item(
    user_id: String,
    common: &SegmentEventCommonExplorerFields,
    event_data: SegmentEvent,
) -> SegmentMetricEventBody {
    let (event_name, event_properties) = match event_data {
        SegmentEvent::PerformanceMetrics(event) => (
            "Performance Metrics".to_string(),
            serde_json::to_value(event).unwrap(),
        ),
        SegmentEvent::ExplorerError(event) => (
            "Explorer Error".to_string(),
            serde_json::to_value(event).unwrap(),
        ),
        SegmentEvent::ExplorerSceneLoadTimes(event) => (
            "Explorer Scene Load Times".to_string(),
            serde_json::to_value(event).unwrap(),
        ),
        SegmentEvent::ExplorerMoveToParcel(event) => (
            "Explorer Move To Parcel".to_string(),
            serde_json::to_value(event).unwrap(),
        ),
        SegmentEvent::SystemInfoReport(event) => (
            "System Info Report".to_string(),
            serde_json::to_value(event).unwrap(),
        ),
    };

    let mut properties = serde_json::to_value(common).unwrap();
    // merge specific event properties with common properties
    for (k, v) in event_properties.as_object().unwrap().iter() {
        properties[k] = v.clone();
    }

    SegmentMetricEventBody {
        r#type: "track".to_string(),
        event: event_name,
        user_id,
        properties,
    }
}
