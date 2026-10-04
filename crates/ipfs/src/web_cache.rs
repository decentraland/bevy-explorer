//! The service worker's asset cache (deploy/web/service_worker.js), as the engine reaches it.
//! The cache name, the keys and the header here MUST match the worker's.
//!
//! The worker stores each response it fetches under its origin. An entry under the shared key
//! answers for every content server, so it is only written here, once the bytes are checked
//! against the hash (`content_hash`).

use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Cache, CacheStorage, Response, ResponseInit};

const CACHE_NAME: &str = "ipfs-path-cache-v2";
// stand-in origin for shared entries: `.invalid` is reserved, so no server's own entries land here
const SHARED_ORIGIN: &str = "https://shared.invalid";
/// On a response the worker answered from its cache.
pub const CACHED_HEADER: &str = "x-ipfs-cached";

// the shared key, then the origin's own
fn keys(url: &str) -> Result<[String; 2], JsValue> {
    let url = web_sys::Url::new(url)?;
    let path = format!("{}{}", url.pathname(), url.search());
    Ok([
        format!("{SHARED_ORIGIN}{path}"),
        format!("{}{path}", url.origin()),
    ])
}

pub async fn open() -> Result<Cache, JsValue> {
    // a window or a worker
    let caches: CacheStorage =
        js_sys::Reflect::get(&js_sys::global(), &"caches".into())?.dyn_into()?;
    JsFuture::from(caches.open(CACHE_NAME)).await?.dyn_into()
}

async fn get(cache: &Cache, key: &str) -> Result<Option<Response>, JsValue> {
    let response = JsFuture::from(cache.match_with_str(key)).await?;
    if response.is_undefined() {
        return Ok(None);
    }
    Ok(Some(response.dyn_into()?))
}

/// The cached entry for `url`, and the key it is under.
pub async fn find(cache: &Cache, url: &str) -> Result<(String, Response), JsValue> {
    for key in keys(url)? {
        if let Some(response) = get(cache, &key).await? {
            return Ok((key, response));
        }
    }
    Err("no previous cache item".into())
}

/// Replaces the body of the entry under `key`.
pub async fn put(cache: &Cache, key: &str, old: &Response, data: &[u8]) -> Result<(), JsValue> {
    let headers = old.headers();
    headers.set("Content-Length", &data.len().to_string())?;

    let init = ResponseInit::new();
    init.set_status(old.status());
    init.set_status_text(&old.status_text());
    init.set_headers(&headers);

    let body = Uint8Array::from(data);
    let response = Response::new_with_opt_buffer_source_and_init(Some(&body), &init)?;
    JsFuture::from(cache.put_with_str(key, &response)).await?;
    Ok(())
}

/// Moves `url`'s entry from its origin to the shared key. `data` is the response's bytes, which
/// the caller has checked against the hash in `url`.
pub async fn share(url: &str, data: &[u8]) -> Result<(), JsValue> {
    let cache = open().await?;
    let [shared, origin] = keys(url)?;
    // nothing stored: an uncacheable response, or the page is not under this worker
    let Some(stored) = get(&cache, &origin).await? else {
        return Ok(());
    };
    put(&cache, &shared, &stored, data).await?;
    JsFuture::from(cache.delete_with_str(&origin)).await?;
    Ok(())
}
