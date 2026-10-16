//! "System Info Report", once per launch: what bevy's render adapter and the host say about the
//! device. Everything the engine cannot know (brand, model, memory, SoC) is an explicit null.

use bevy::{prelude::*, render::renderer::RenderAdapterInfo};

use crate::{
    data_definition::{SegmentEvent, SegmentEventSystemInfoReport},
    segment_system::SegmentMetricsEvents,
};

/// Runs until the adapter info exists (never, headless), then reports once.
pub fn report_system_info(
    adapter: Option<Res<RenderAdapterInfo>>,
    mut metrics: ResMut<SegmentMetricsEvents>,
    mut sent: Local<bool>,
) {
    if *sent {
        return;
    }
    let Some(adapter) = adapter else {
        return;
    };
    *sent = true;

    let info = &adapter.0;
    metrics.add_event(SegmentEvent::SystemInfoReport(
        SegmentEventSystemInfoReport {
            device_brand: None,
            device_model: None,
            os_version: os_version(),
            soc_manufacturer: None,
            soc_model: None,
            processor_type: None,
            processor_count: processor_count(),
            processor_max_freq_mhz: None,
            graphics_device_name: non_empty(info.name.clone()),
            graphics_device_vendor: vendor_name(info.vendor).map(str::to_owned),
            graphics_api_version: graphics_api_version(info.backend.to_str(), &info.driver_info),
            system_memory_size_mb: None,
            device_support: "not_checked".to_owned(),
        },
    ));
}

fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

// wgpu reports the PCI vendor id, godot the vendor's name; the dashboard wants the name.
fn vendor_name(vendor: u32) -> Option<&'static str> {
    match vendor {
        0x1002 => Some("AMD"),
        0x10DE => Some("NVIDIA"),
        0x8086 => Some("Intel"),
        0x106B => Some("Apple"),
        0x13B5 => Some("ARM"),
        0x5143 => Some("Qualcomm"),
        0x1010 => Some("Imagination"),
        0x14E4 => Some("Broadcom"),
        0x10005 => Some("Mesa"),
        _ => None,
    }
}

// Godot's spellings ("Vulkan 1.1.128", "Metal 3.1"), from wgpu's lowercase backend names.
fn graphics_api_version(backend: &str, driver_info: &str) -> Option<String> {
    let api = match backend {
        "vulkan" => "Vulkan",
        "metal" => "Metal",
        "dx12" => "Direct3D 12",
        "gl" => "OpenGL",
        "webgpu" => "WebGPU",
        _ => return None,
    };
    let driver_info = driver_info.trim();
    Some(if driver_info.is_empty() {
        api.to_owned()
    } else {
        format!("{api} {driver_info}")
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn os_version() -> Option<String> {
    non_empty(std::env::consts::OS.to_owned())
}

#[cfg(target_arch = "wasm32")]
fn os_version() -> Option<String> {
    Some("web".to_owned())
}

#[cfg(not(target_arch = "wasm32"))]
fn processor_count() -> Option<u32> {
    std::thread::available_parallelism()
        .ok()
        .map(|count| count.get() as u32)
}

// The engine runs in a worker, where `window()` is None; the page is the fallback.
#[cfg(target_arch = "wasm32")]
fn processor_count() -> Option<u32> {
    use wasm_bindgen::JsCast;
    let cores = js_sys::global()
        .dyn_into::<web_sys::WorkerGlobalScope>()
        .ok()
        .map(|scope| scope.navigator().hardware_concurrency())
        .or_else(|| web_sys::window().map(|window| window.navigator().hardware_concurrency()))?;
    (cores >= 1.0).then_some(cores as u32)
}
