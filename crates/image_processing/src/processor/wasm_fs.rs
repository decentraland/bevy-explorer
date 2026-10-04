use bevy::log::tracing;
use ipfs::web_cache;
use js_sys::{ArrayBuffer, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

// the service worker stores the raw fetch in its cache: we read it, process, and write the
// processed bytes back over the same entry so subsequent loads are served pre-processed.

pub async fn read_file(filename: &str) -> Result<Vec<u8>, anyhow::Error> {
    read_file_internal(filename).await.map_err(|e| {
        tracing::error!("{e:?}");
        anyhow::anyhow!("{e:?}")
    })
}

async fn read_file_internal(filename: &str) -> Result<Vec<u8>, JsValue> {
    let cache = web_cache::open().await?;
    let (_, response) = web_cache::find(&cache, filename).await?;

    let buffer_promise = response.array_buffer()?;
    let buffer_val = JsFuture::from(buffer_promise).await?;
    let buffer: ArrayBuffer = buffer_val.dyn_into()?;

    // Copy from JS heap to Rust heap
    let type_array = Uint8Array::new(&buffer);
    let mut data = vec![0u8; type_array.length() as usize];
    type_array.copy_to(&mut data);
    Ok(data)
}

pub async fn write_file(filename: &str, data: &[u8]) -> Result<(), anyhow::Error> {
    write_file_internal(filename, data).await.map_err(|e| {
        tracing::error!("{e:?}");
        anyhow::anyhow!("{e:?}")
    })
}

pub async fn write_file_internal(filename: &str, data: &[u8]) -> Result<(), JsValue> {
    let cache = web_cache::open().await?;
    let (key, old_res) = web_cache::find(&cache, filename).await?;
    web_cache::put(&cache, &key, &old_res, data).await
}
