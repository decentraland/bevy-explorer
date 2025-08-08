use super::WorkerContext;
use bevy::{log::warn, platform::collections::HashMap};
use dcl::interface::crdt_context::CrdtContext;
use futures_lite::io::{AsyncReadExt, AsyncWriteExt};
use serde::{Deserialize, Serialize};
use wallet::Wallet;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

// wrap localStorage to include player address in all operations
// TODO: init / store

#[derive(Default, Serialize, Deserialize, Clone)]
struct LocalStorage(HashMap<String, String>);

pub async fn init(state: &WorkerContext) {
    let scene_urn = state.state.borrow().borrow::<CrdtContext>().hash.clone();

    if let Ok(mut existing) = web_fs::File::open(format!("local_storage/{scene_urn}")).await {
        let mut buf = String::default();
        if let Err(e) = existing.read_to_string(&mut buf).await {
            warn!("failed to read storage: {e:?}");
            return;
        }
        let Ok(storage) = serde_json::from_str::<LocalStorage>(&buf) else {
            warn!("failed to deserialize storage");
            return;
        };

        state.state.borrow_mut().put(storage);
    }
}

fn write(state: &WorkerContext) {
    let scene_urn = state.state.borrow().borrow::<CrdtContext>().hash.clone();
    let storage = state.state.borrow().borrow::<LocalStorage>().clone();

    spawn_local(async move {
        let Ok(data) = serde_json::to_string(&storage) else {
            warn!("failed to serialize storage");
            return;
        };

        let _ = web_fs::create_dir_all("local_storage").await;
        let Ok(mut file) = web_fs::File::create(format!("local_storage/{scene_urn}")).await else {
            warn!("failed to write storage");
            return;
        };

        let _ = file.write_all(data.as_bytes()).await;
    })
}

fn address(state: &WorkerContext) -> String {
    state
        .state
        .borrow()
        .borrow::<Wallet>()
        .address()
        .map(|a| format!("{a:#x}"))
        .unwrap_or_default()
}

fn strip_prefix(key: &mut String) {
    *key = key.split_once(':').unwrap().1.to_owned()
}

fn with_storage<R>(state: &WorkerContext, f: impl Fn(&mut HashMap<String, String>) -> R) -> R {
    f(&mut state
        .state
        .borrow_mut()
        .borrow_mut_or_default::<LocalStorage>()
        .0)
}

// returns filtered keys matching current user, including the prefix
fn iterate_keys(state: &WorkerContext) -> Vec<String> {
    let address = address(state);
    let address = &address;
    with_storage(state, |storage| {
        storage
            .keys()
            .filter(|k| k.starts_with(address))
            .map(ToOwned::to_owned)
            .collect()
    })
}

#[wasm_bindgen]
pub fn op_webstorage_length(state: &WorkerContext) -> u32 {
    iterate_keys(state).len() as u32
}

#[wasm_bindgen]
pub fn op_webstorage_key(state: &WorkerContext, index: u32) -> Option<String> {
    let mut key = iterate_keys(state)
        .get(index as usize)
        .map(ToOwned::to_owned);
    key.iter_mut().for_each(strip_prefix);
    key
}

#[wasm_bindgen]
pub fn op_webstorage_set(state: &WorkerContext, key_name: &str, value: &str) {
    let address = address(state);
    with_storage(state, |storage| {
        storage.insert(format!("{address}:{key_name}"), value.to_owned())
    });
    write(state);
}

#[wasm_bindgen]
pub fn op_webstorage_get(state: &WorkerContext, key_name: &str) -> Option<String> {
    let address = address(state);
    with_storage(state, |storage| {
        storage
            .get(&format!("{address}:{key_name}"))
            .map(ToOwned::to_owned)
    })
}

#[wasm_bindgen]
pub fn op_webstorage_remove(state: &WorkerContext, key_name: &str) {
    let address = address(state);
    with_storage(state, |storage| {
        storage.remove(&format!("{address}:{key_name}"))
    });
    write(state);
}

#[wasm_bindgen]
pub fn op_webstorage_clear(state: &WorkerContext) {
    let keys = iterate_keys(state);
    with_storage(state, move |storage| {
        for key in &keys {
            storage.remove(key);
        }
    });
    write(state);
}

#[wasm_bindgen]
pub fn op_webstorage_iterate_keys(state: &WorkerContext) -> Vec<String> {
    let mut keys = iterate_keys(state);
    keys.iter_mut().for_each(strip_prefix);
    keys
}

#[wasm_bindgen]
pub fn op_webstorage_has(state: &WorkerContext, key_name: &str) -> bool {
    let address = address(state);
    with_storage(state, |storage| {
        storage.contains_key(&format!("{address}:{key_name}"))
    })
}
