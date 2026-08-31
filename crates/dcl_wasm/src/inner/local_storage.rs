use super::WorkerContext;
use bevy::{
    log::{debug, warn},
    platform::collections::HashMap,
};
use dcl::{interface::crdt_context::CrdtContext, js::player_identity};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{
    FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemGetDirectoryOptions,
    FileSystemGetFileOptions, FileSystemWritableFileStream, WorkerGlobalScope,
};

// wrap localStorage to include player address in all operations
// TODO: init / store

#[derive(Default, Serialize, Deserialize, Clone)]
struct LocalStorage(HashMap<String, String>);

const STORAGE_DIR: &str = "local_storage";

// The scene's storage directory, held open for the life of the worker.
//
// Deliberately NOT `web_fs`: that resolves navigator.storage.getDirectory() — the OPFS *root*, which
// also holds config.json and the ipfs cache — on every single open, so it would need OPFS reachable
// from the scene's realm the whole time the scene runs. sandbox_worker.js scrubs the API off the
// worker global instead, and this handle keeps working afterwards because it is a live object rather
// than a fresh lookup. It also can't be walked upwards (`..` is rejected), so it grants the subtree
// and nothing else.
thread_local! {
    static DIR: RefCell<Option<FileSystemDirectoryHandle>> = const { RefCell::new(None) };
}

async fn open_dir() -> Result<FileSystemDirectoryHandle, JsValue> {
    let global = js_sys::global().unchecked_into::<WorkerGlobalScope>();
    let root: FileSystemDirectoryHandle =
        JsFuture::from(global.navigator().storage().get_directory())
            .await?
            .unchecked_into();

    let options = FileSystemGetDirectoryOptions::new();
    options.set_create(true);
    Ok(
        JsFuture::from(root.get_directory_handle_with_options(STORAGE_DIR, &options))
            .await?
            .unchecked_into(),
    )
}

async fn read(scene_urn: &str) -> Result<Option<String>, JsValue> {
    let Some(dir) = DIR.with(|dir| dir.borrow().clone()) else {
        return Ok(None);
    };
    // A scene that has never stored anything has no file; that isn't an error.
    let Ok(handle) = JsFuture::from(dir.get_file_handle(scene_urn)).await else {
        return Ok(None);
    };
    let file = JsFuture::from(handle.unchecked_into::<FileSystemFileHandle>().get_file()).await?;
    let text = JsFuture::from(file.unchecked_into::<web_sys::File>().text()).await?;
    Ok(text.as_string())
}

async fn store(scene_urn: &str, data: &str) -> Result<(), JsValue> {
    let Some(dir) = DIR.with(|dir| dir.borrow().clone()) else {
        return Err(JsValue::from_str("scene storage unavailable"));
    };
    let options = FileSystemGetFileOptions::new();
    options.set_create(true);
    let handle: FileSystemFileHandle =
        JsFuture::from(dir.get_file_handle_with_options(scene_urn, &options))
            .await?
            .unchecked_into();

    let writable: FileSystemWritableFileStream = JsFuture::from(handle.create_writable())
        .await?
        .unchecked_into();
    JsFuture::from(writable.write_with_str(data)?).await?;
    JsFuture::from(writable.close()).await?;
    Ok(())
}

pub async fn init(state: &WorkerContext) {
    let scene_urn = state.state.borrow().borrow::<CrdtContext>().hash.clone();

    // Unconditional, and ahead of the read: this is what captures the handle, and it has to happen
    // while navigator.storage is still there. wasm_init_scene (our caller) returns before
    // sandbox_worker.js builds the js context and scrubs, so this is the last chance to take it —
    // including for a scene with nothing stored yet, which still needs the handle to write later.
    match open_dir().await {
        Ok(dir) => DIR.with(|slot| *slot.borrow_mut() = Some(dir)),
        Err(e) => {
            warn!("failed to open scene storage: {e:?}");
            return;
        }
    }

    let buf = match read(&scene_urn).await {
        Ok(Some(buf)) => buf,
        Ok(None) => return,
        Err(e) => {
            warn!("failed to read storage: {e:?}");
            return;
        }
    };

    let Ok(storage) = serde_json::from_str::<LocalStorage>(&buf) else {
        warn!("failed to deserialize storage");
        return;
    };

    state.state.borrow_mut().put(storage);
}

fn write(state: &WorkerContext) {
    let scene_urn = state.state.borrow().borrow::<CrdtContext>().hash.clone();
    let storage = state.state.borrow().borrow::<LocalStorage>().clone();

    spawn_local(async move {
        let Ok(data) = serde_json::to_string(&storage) else {
            warn!("failed to serialize storage");
            return;
        };

        if let Err(e) = store(&scene_urn, &data).await {
            warn!("failed to write storage: {e:?}");
        }
    })
}

fn address(state: &WorkerContext) -> String {
    let address = player_identity(&*state.state.borrow())
        .map(|id| id.address)
        .unwrap_or_default();
    debug!("local storage address: {address:?}");
    address
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
