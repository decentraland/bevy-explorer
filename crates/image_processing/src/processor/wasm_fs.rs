use bevy::log::tracing;
use js_sys::{ArrayBuffer, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Cache, DedicatedWorkerGlobalScope, Response, ResponseInit};

// must match key used in service_worker.js
fn key(filename: &str) -> Result<String, JsValue> {
    let url = web_sys::Url::new(filename)?;
    Ok(format!("{}{}", url.pathname(), url.search()))
}

pub async fn read_file(filename: &str) -> Result<Vec<u8>, anyhow::Error> {
    read_file_internal(filename).await.map_err(|e| {
        tracing::error!("{e:?}");
        anyhow::anyhow!("{e:?}")
    })
}

async fn read_file_internal(filename: &str) -> Result<Vec<u8>, JsValue> {
    let filename = &key(filename)?;
    let global = js_sys::global().dyn_into::<DedicatedWorkerGlobalScope>()?;
    let caches = global.caches()?;
    let cache: Cache = JsFuture::from(caches.open("ipfs-path-cache-v1"))
        .await?
        .dyn_into()?;

    // try to find existing item
    let match_promise = cache.match_with_str(filename);

    let response_val = JsFuture::from(match_promise).await?;
    if response_val.is_undefined() {
        return Err("no previous cache item".into());
    }

    let response: Response = response_val.dyn_into()?;

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
    let filename = &key(filename)?;

    // 1. Open the Cache
    let global = js_sys::global().dyn_into::<DedicatedWorkerGlobalScope>()?;
    let caches = global.caches()?;
    let cache: Cache = JsFuture::from(caches.open("ipfs-path-cache-v1"))
        .await?
        .dyn_into()?;

    // 2. Try to find existing item
    let match_promise = cache.match_with_str(filename);

    let old_response_val = JsFuture::from(match_promise).await?;
    if old_response_val.is_undefined() {
        return Err("no previous cache item".into());
    }

    let old_res: Response = old_response_val.dyn_into()?;
    let headers = old_res.headers();
    let status = old_res.status();
    let status_text = old_res.status_text();

    headers.set("Content-Length", &data.len().to_string())?;

    let init = ResponseInit::new();
    init.set_status(status);
    init.set_status_text(&status_text);
    init.set_headers(&headers);

    let js_body = js_sys::Uint8Array::from(data);
    let new_response = Response::new_with_opt_buffer_source_and_init(Some(&js_body), &init)?;

    let put_promise = cache.put_with_str(filename, &new_response);
    JsFuture::from(put_promise).await?;

    Ok(())
}
