pub mod engine;
pub use engine::ImageProcessingPlugin;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::wasm_bindgen;

mod processor;

#[cfg(target_arch = "wasm32")]
use std::sync::{Arc, Mutex, OnceLock};
#[cfg(target_arch = "wasm32")]
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use bevy::{asset::AssetPath, prelude::*};

#[derive(Clone, Copy)]
pub enum ProcessingAssetType {
    Gltf,
    Image,
}

#[derive(Event, Clone)]
pub struct AssetForProcessing {
    pub cache_path: String,
    pub base_path: AssetPath<'static>,
    pub ty: ProcessingAssetType,
}

#[cfg(target_arch = "wasm32")]
static CHANNELS: OnceLock<
    Arc<
        Mutex<(
            Option<(
                UnboundedSender<AssetForProcessing>,
                UnboundedReceiver<Result<AssetForProcessing, ()>>,
            )>,
            Option<(
                UnboundedReceiver<AssetForProcessing>,
                UnboundedSender<Result<AssetForProcessing, ()>>,
            )>,
        )>,
    >,
> = OnceLock::new();

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn image_processor_init() {
    use tokio::sync::mpsc::unbounded_channel;

    let (req_sx, req_rx) = unbounded_channel();
    let (resp_sx, resp_rx) = unbounded_channel();

    CHANNELS
        .set(Arc::new(Mutex::new((
            Some((req_sx, resp_rx)),
            Some((req_rx, resp_sx)),
        ))))
        .unwrap();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn image_processor_run() {
    let (req_rx, resp_sx) = CHANNELS.get().unwrap().lock().unwrap().1.take().unwrap();
    processor::process_events(req_rx, resp_sx).await
}
