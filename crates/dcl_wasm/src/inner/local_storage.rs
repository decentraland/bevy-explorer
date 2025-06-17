use super::WorkerContext;
use bevy::platform::collections::HashMap;
use wallet::Wallet;
use wasm_bindgen::prelude::*;

// wrap localStorage to include player address in all operations
// TODO: init / store

#[derive(Default)]
struct LocalStorage(HashMap<String, String>);

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
}

#[wasm_bindgen]
pub fn op_webstorage_clear(state: &WorkerContext) {
    let keys = iterate_keys(state);
    with_storage(state, move |storage| {
        for key in &keys {
            storage.remove(key);
        }
    });
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
