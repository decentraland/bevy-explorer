#[derive(Serialize)]
pub struct SegmentMetricEventBody {
    event: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "writeKey")]
    write_key: String,
    properties: serde_json::Value,
}

#[derive(Serialize)]
// Same for all events sent from the explorer
pub struct SegmentEventCommonExplorerFields {
    // User’s wallet id, even for guests.
    dcl_eth_address: String,
    // If the user is a guest or not.
    dcl_is_guest: bool,
    // Realm where the user is connected.
    realm: String,
    // Current user position.
    position: String,
    // What type of client was used to render the world (Web/Native/VR)
    dcl_renderer_type: String,
    // Explorer’s unique session id.
    session_id: String,
    // Explorer’s release used.
    renderer_version: String,
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
    samples: u32,
    // Total length of the performance report.
    total_time: f32,
    // Amount of hiccups in 1000 frames.
    hiccups_in_thousand_frames: u32,
    // Total time length of hiccups measured in seconds.
    hiccups_time: f32,
    // Minimum delta (difference) between frames in milliseconds
    min_frame_time: f32,
    // Maximum delta (difference) between frames in milliseconds
    max_frame_time: f32,
    // Average delta (difference) between frames in milliseconds
    mean_frame_time: f32,
    // Median delta (difference) between frames in milliseconds
    median_frame_time: f32,
    // Percentile 1 of the delta (difference) between frames in milliseconds
    p1_frame_time: f32,
    // Percentile 5 of the delta (difference) between frames in milliseconds
    p5_frame_time: f32,
    // Percentile 10 of the delta (difference) between frames in milliseconds
    p10_frame_time: f32,
    // Percentile 20 of the delta (difference) between frames in milliseconds
    p20_frame_time: f32,
    // Percentile 50 of the delta (difference) between frames in milliseconds
    p50_frame_time: f32,
    // Percentile 75 of the delta (difference) between frames in milliseconds
    p75_frame_time: f32,
    // Percentile 80 of the delta (difference) between frames in milliseconds
    p80_frame_time: f32,
    // Percentile 90 of the delta (difference) between frames in milliseconds
    p90_frame_time: f32,
    // Percentile 95 of the delta (difference) between frames in milliseconds
    p95_frame_time: f32,
    // Percentile 99 of the delta (difference) between frames in milliseconds
    p99_frame_time: f32,
    // How many users where nearby the current user
    player_count: u32,
    // Javascript heap memory used by the scenes in kilo bytes
    used_jsheap_size: u32,
    // Memory used only by the explorer in kilo bytes
    memory_usage: u32,
}

pub struct SegmentEventExplorerError {
    // Generic or Fatal.
    error_type: String,
    // Error description.
    error_message: String,
    // Error’s stack
    error_stack: String,
}

pub struct SegmentEventExplorerSceneLoadTimes {
    // Unique hash for the scene.
    scene_hash: String,
    // Time to load in seconds.
    elapsed: f32,
    // Boolean flag indicating wether the scene loaded without errors.
    success: bool,
}

// TODO: maybe important what realm?
pub struct SegmentEventExplorerMoveToParcel {
    // Parcel where the user is coming from.
    old_parcel: String,
}

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

pub fn build_segment_event(
    user_id: String,
    write_key: String,
    common: SegmentEventCommonExplorerFields,
    event: SegmentEvent,
) -> SegmentMetricEventBody {
    let (event, event_properties) = match event {
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

    let properties = serde_json::to_value(event);
    // merge specific event properties with common properties
    for (k, v) in event_properties {
        properties[k] = v;
    }

    SegmentMetricEventBody {
        event,
        user_id,
        write_key,
        properties,
    }
}

// {
//     "event": "Performance Metrics",
//     "userId": "019mr8mf4r",
//     "writeKey": "syp64BBsJUd6SHQRKv6b9G4Lgt3ny8Q8",
//     "properties": {
//         "dcl_eth_address": "0xD3971C8E02d7237d5B6dAC8292a9C99291C35bB4",
//         "dcl_is_guest": false,
//         "realm": "main",
//         "position": "0,0",
//         "dcl_renderer_type": "dao-bevy",
//         "session_id": "IWow2niw2311efioWE2NI23Ow432efn32iowe4334f",
//         "renderer_version": "bevy-1.0.0-commit-6c9d340b612549c3a4152423f37dde2a8b441d5d",

//         "samples": 1000,
//         "total_time": 16.67,
//         "hiccups_in_thousand_frames": 10,
//         "hiccups_time": 0.15,
//         "min_frame_time": 8.16,
//         "max_frame_time": 120.1,
//         "mean_frame_time": 16.67,
//         "median_frame_time": 16.67,
//         "p1_frame_time": 16.67,
//         "p5_frame_time": 16.67,
//         "p10_frame_time": 16.67,
//         "p20_frame_time": 16.67,
//         "p50_frame_time": 16.67,
//         "p75_frame_time": 16.67,
//         "p80_frame_time": 16.67,
//         "p90_frame_time": 16.67,
//         "p95_frame_time": 16.67,
//         "p99_frame_time": 16.67,

//         "player_count": 0,
//         "used_jsheap_size": 564213124,
//         "memory_usage": 1243156789
//     }
// }
