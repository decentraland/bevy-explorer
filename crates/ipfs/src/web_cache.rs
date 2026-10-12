//! The service worker's asset cache (deploy/web/service_worker.js), as the engine reaches it.
//! The cache name, the keys and the header here MUST match the worker's.
//!
//! The worker only answers from the cache; the engine fills it from what it downloads. An entry
//! under the shared key answers for every content server, so it is only written once the bytes
//! are checked against the hash (`content_hash`), and under that hash; a url's response goes
//! under the url.

// stand-in origin for shared entries: `.invalid` is reserved, so no server's own entries land here
const SHARED_ORIGIN: &str = "https://shared.invalid";

// The shared key, then the url's own. The shared key is the last path segment alone: the worker
// looks a request up by it, whatever the rest of the url says.
fn keys(url: &str) -> Option<[String; 2]> {
    let url = url::Url::parse(url).ok()?;
    let name = url.path().rsplit('/').next().unwrap_or_default();
    let query = url
        .query()
        .filter(|query| !query.is_empty())
        .map(|query| format!("?{query}"))
        .unwrap_or_default();
    Some([
        format!("{SHARED_ORIGIN}/{name}"),
        format!(
            "{}{}{query}",
            url.origin().ascii_serialization(),
            url.path()
        ),
    ])
}

// Whether a request for `url` is one the worker would answer from the shared entry for `hash`.
// A url is the server's to choose (its base url, then the hash), and the path of
// `…/contents/<other hash>#/contents/<hash>` ends at the other hash: sharing that response would
// file it under a hash nobody checked it against.
fn names(url: &str, hash: &str) -> bool {
    keys(url).is_some_and(|[shared, _]| shared == format!("{SHARED_ORIGIN}/{hash}"))
}

#[cfg(target_arch = "wasm32")]
pub use io::*;

#[cfg(target_arch = "wasm32")]
mod io {
    use super::{keys, names};
    use js_sys::Uint8Array;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Cache, CacheStorage, Headers, Response, ResponseInit};

    const CACHE_NAME: &str = "ipfs-path-cache-v2";
    /// On a response the worker answered from its cache.
    pub const CACHED_HEADER: &str = "x-ipfs-cached";

    pub async fn open() -> Result<Cache, JsValue> {
        // a window or a worker
        let caches: CacheStorage =
            js_sys::Reflect::get(&js_sys::global(), &"caches".into())?.dyn_into()?;
        JsFuture::from(caches.open(CACHE_NAME)).await?.dyn_into()
    }

    /// The entry under `key`.
    pub async fn get(cache: &Cache, key: &str) -> Result<Option<Response>, JsValue> {
        let response = JsFuture::from(cache.match_with_str(key)).await?;
        if response.is_undefined() {
            return Ok(None);
        }
        Ok(Some(response.dyn_into()?))
    }

    /// The cached entry for `url`, and the key it is under.
    pub async fn find(cache: &Cache, url: &str) -> Result<(String, Response), JsValue> {
        for key in keys(url).ok_or("not a url")? {
            if let Some(response) = get(cache, &key).await? {
                return Ok((key, response));
            }
        }
        Err("no previous cache item".into())
    }

    /// The cached bytes for `url`, under either key; `None` when there are none (or no cache).
    pub async fn read(url: &str) -> Option<Vec<u8>> {
        async fn read(url: &str) -> Result<Vec<u8>, JsValue> {
            let cache = open().await?;
            let (_, response) = find(&cache, url).await?;
            bytes(&response).await
        }
        read(url).await.ok()
    }

    /// The body of a cached response.
    pub async fn bytes(response: &Response) -> Result<Vec<u8>, JsValue> {
        let buffer: js_sys::ArrayBuffer =
            JsFuture::from(response.array_buffer()?).await?.dyn_into()?;
        let array = Uint8Array::new(&buffer);
        let mut data = vec![0u8; array.length() as usize];
        array.copy_to(&mut data);
        Ok(data)
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

    /// Stores `data` under `key` as a fresh response.
    async fn store(cache: &Cache, key: &str, data: &[u8]) -> Result<(), JsValue> {
        let headers = Headers::new()?;
        headers.set("Content-Type", "application/octet-stream")?;
        headers.set("Content-Length", &data.len().to_string())?;

        let init = ResponseInit::new();
        init.set_status(200);
        init.set_headers(&headers);

        let body = Uint8Array::from(data);
        let response = Response::new_with_opt_buffer_source_and_init(Some(&body), &init)?;
        JsFuture::from(cache.put_with_str(key, &response)).await?;
        Ok(())
    }

    /// Stores `data`, the bytes `url` answered with, under the url's own key.
    pub async fn keep(url: &str, data: &[u8]) -> Result<(), JsValue> {
        let [_, own] = keys(url).ok_or("not a url")?;
        let cache = open().await?;
        store(&cache, &own, data).await
    }

    /// Stores `data`, the bytes `url` answered with, under the shared key for `hash`, if
    /// `matches_hash`: whether they are what `hash` names. The url's own entry, if an older
    /// worker left one, goes: it is dead weight beside the shared one.
    pub async fn share(
        url: &str,
        hash: &str,
        data: &[u8],
        matches_hash: impl FnOnce() -> bool,
    ) -> Result<(), JsValue> {
        if !names(url, hash) || !matches_hash() {
            return Ok(());
        }
        let [shared, own] = keys(url).ok_or("not a url")?;
        let cache = open().await?;
        store(&cache, &shared, data).await?;
        JsFuture::from(cache.delete_with_str(&own)).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_key_is_the_hash_whichever_server_and_path() {
        let [catalyst, _] = keys("https://peer.example/content/contents/bafkreihash").unwrap();
        let [worlds, own] = keys("https://worlds.example/contents/bafkreihash?v=1").unwrap();
        assert_eq!(catalyst, "https://shared.invalid/bafkreihash");
        assert_eq!(worlds, catalyst);
        assert_eq!(own, "https://worlds.example/contents/bafkreihash?v=1");
    }

    #[test]
    fn a_url_only_names_the_hash_its_path_ends_with() {
        assert!(names(
            "https://peer.example/contents/bafkreihash",
            "bafkreihash"
        ));
        // the worker keys this request by `bafkreitarget`
        let url = "https://evil.example/contents/bafkreitarget#/contents/bafkreihash";
        assert!(!names(url, "bafkreihash"));
        assert_eq!(
            keys(url).unwrap(),
            [
                "https://shared.invalid/bafkreitarget".to_owned(),
                "https://evil.example/contents/bafkreitarget".to_owned()
            ]
        );
        assert!(!names(
            "https://evil.example/contents/bafkreitarget?/bafkreihash",
            "bafkreihash"
        ));
        assert!(!names(
            "https://peer.example/contents/bafkreihash/",
            "bafkreihash"
        ));
    }
}
