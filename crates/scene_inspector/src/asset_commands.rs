use std::sync::Arc;

// `ContentMap`'s inner map is bevy_platform's HashMap — use the same type for the map we pass into
// `ContentMap(..)` (the scene-collection merge in /init_asset).
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use bevy_console::ConsoleCommand;
use console::{DoAddConsoleCommand, PendingConsoleResponses};
use ipfs::{ContentMap, IpfsIo, IpfsResource};
use platform::AsyncRwLock;

use crate::active_scene::SceneResolver;

const CATALOG_URL: &str = "https://builder-items.decentraland.org/asset-packs/latest/catalog.json";
const CONTENTS_BASE: &str = "https://builder-items.decentraland.org/contents/";

/// One catalog asset: its slim metadata plus the raw composite and the path→hash content map.
#[derive(Clone)]
pub struct CatalogAsset {
    pub id: String,
    pub name: String,
    pub category: String,
    pub tags: Vec<String>,
    pub pack: String,
    pub composite: serde_json::Value,
    pub contents: HashMap<String, String>,
}

/// The parsed asset-packs catalog, populated by `/asset_catalog` and read by `/init_asset`. Shared
/// behind an `Arc<AsyncRwLock>` so the commands' async tasks can populate/read it off the schedule.
#[derive(Resource, Clone, Default)]
pub struct AssetCatalog(pub Arc<AsyncRwLock<Vec<CatalogAsset>>>);

pub fn add_asset_commands(app: &mut App) {
    app.init_resource::<AssetCatalog>();
    app.add_console_command::<AssetCatalogCommand, _>(asset_catalog_cmd);
    app.add_console_command::<InitAssetCommand, _>(init_asset_cmd);
    app.add_console_command::<SceneContentCommand, _>(scene_content_cmd);
}

// --- /scene_content ---

/// List the current scene's content files (content-map paths, sorted) as a JSON array of strings,
/// for the editor's content-file pickers (gltf/audio/video/texture). Includes imported assets
/// (merged into the scene collection). Paths are lowercased, as stored; the editor filters by
/// extension per the field's semantic.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/scene_content")]
struct SceneContentCommand;

fn scene_content_cmd(
    mut input: ConsoleCommand<SceneContentCommand>,
    ipfs: Res<IpfsResource>,
    resolver: SceneResolver,
    mut console_responses: ResMut<PendingConsoleResponses>,
) {
    if let Some(Ok(_)) = input.take() {
        let scene_hash = match resolver.resolve() {
            Ok((_, ctx)) => ctx.hash.clone(),
            Err(e) => {
                input.reply_failed(e);
                return;
            }
        };
        let io = ipfs.inner.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        IoTaskPool::get()
            .spawn(async move {
                // refresh from the dev server first so files added to the project outside the
                // editor (the dev server's content map is the whole project glob) are picked up.
                io.refresh_scene_collection(&scene_hash).await;
                let files = io.scene_content_files(&scene_hash).await;
                let _ = tx.send(serde_json::to_string(&files).map_err(|e| e.to_string()));
            })
            .detach();
        console_responses.push_oneshot(rx, |r| r, input.take_responder());
    }
}

// --- /asset_catalog ---

/// Fetch the asset-packs catalog and return a slim asset index
/// (`[{id,name,category,tags,pack,thumbnail}]`, thumbnail = a CDN url). Run once before
/// `/init_asset`. The full catalog (with composites + content maps) is cached in [`AssetCatalog`]
/// for `/init_asset` to look up by id.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/asset_catalog")]
struct AssetCatalogCommand;

fn asset_catalog_cmd(
    mut input: ConsoleCommand<AssetCatalogCommand>,
    ipfs: Res<IpfsResource>,
    catalog: Res<AssetCatalog>,
    mut console_responses: ResMut<PendingConsoleResponses>,
) {
    if let Some(Ok(_)) = input.take() {
        let client = ipfs.client();
        let store = catalog.0.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        IoTaskPool::get()
            .spawn(async move {
                let result: Result<String, String> = async {
                    let resp = client
                        .get(CATALOG_URL)
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
                    let packs = json
                        .get("assetPacks")
                        .and_then(|v| v.as_array())
                        .ok_or_else(|| "catalog has no assetPacks".to_string())?;
                    let mut assets: Vec<CatalogAsset> = Vec::new();
                    for pack in packs {
                        let pack_name = pack
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        for a in pack
                            .get("assets")
                            .and_then(|v| v.as_array())
                            .into_iter()
                            .flatten()
                        {
                            let id = a
                                .get("id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let contents: HashMap<String, String> = a
                                .get("contents")
                                .and_then(|v| v.as_object())
                                .map(|o| {
                                    o.iter()
                                        .filter_map(|(k, v)| {
                                            v.as_str().map(|s| (k.clone(), s.to_string()))
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                            assets.push(CatalogAsset {
                                id,
                                name: a
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                category: a
                                    .get("category")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                tags: a
                                    .get("tags")
                                    .and_then(|v| v.as_array())
                                    .map(|arr| {
                                        arr.iter()
                                            .filter_map(|t| t.as_str().map(String::from))
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                                pack: pack_name.clone(),
                                composite: a
                                    .get("composite")
                                    .cloned()
                                    .unwrap_or(serde_json::Value::Null),
                                contents,
                            });
                        }
                    }
                    let index: Vec<serde_json::Value> = assets
                        .iter()
                        .map(|a| {
                            let thumbnail = a
                                .contents
                                .get("thumbnail.png")
                                .map(|h| format!("{CONTENTS_BASE}{h}"));
                            serde_json::json!({
                                "id": a.id, "name": a.name, "category": a.category,
                                "tags": a.tags, "pack": a.pack, "thumbnail": thumbnail,
                            })
                        })
                        .collect();
                    let reply = serde_json::to_string(&index).map_err(|e| e.to_string())?;
                    *store.write().await = assets;
                    Ok(reply)
                }
                .await;
                let _ = tx.send(result);
            })
            .detach();
        console_responses.push_oneshot(rx, |r| r, input.take_responder());
    }
}

// --- /init_asset ---

/// Import a catalog asset into the current scene: fetch+cache each of its files, register them in
/// the scene's content map under `base_dir`, and return the (path-substituted) composite for the
/// editor to instance. Requires `/asset_catalog` to have been run first.
///
/// Collision-safety: `base_dir` defaults to `assets/imported/<asset_id>`, so every asset's files
/// occupy a disjoint, per-asset path namespace within the scene's collection — distinct assets
/// never share a path key, and re-importing the same asset re-inserts identical path→hash entries
/// (idempotent). The on-disk byte cache is keyed by content hash, so files shared between assets
/// dedupe rather than collide. (The merge into the scene collection itself is `extend`; the
/// namespace is what keeps it from shadowing the scene's own files.)
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/init_asset")]
struct InitAssetCommand {
    /// Catalog asset id
    asset_id: String,
    /// Destination base dir within the scene (default: assets/imported/<asset_id>)
    base_dir: Option<String>,
}

fn init_asset_cmd(
    mut input: ConsoleCommand<InitAssetCommand>,
    ipfs: Res<IpfsResource>,
    catalog: Res<AssetCatalog>,
    resolver: SceneResolver,
    mut console_responses: ResMut<PendingConsoleResponses>,
) {
    if let Some(Ok(cmd)) = input.take() {
        let scene_hash = match resolver.resolve() {
            Ok((_, ctx)) => ctx.hash.clone(),
            Err(e) => {
                input.reply_failed(e);
                return;
            }
        };
        let base_dir = cmd
            .base_dir
            .clone()
            .unwrap_or_else(|| format!("assets/imported/{}", cmd.asset_id));
        let asset_id = cmd.asset_id.clone();
        let client = ipfs.client();
        let io = ipfs.inner.clone();
        let store = catalog.0.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        IoTaskPool::get()
            .spawn(async move {
                let result: Result<String, String> = async {
                    let asset = {
                        let guard = store.read().await;
                        guard.iter().find(|a| a.id == asset_id).cloned()
                    }
                    .ok_or_else(|| {
                        format!("asset '{asset_id}' not in catalog (run /asset_catalog first)")
                    })?;
                    let mut merge: HashMap<String, String> = HashMap::new();
                    let mut written_rels: Vec<String> = Vec::new();
                    let mut errors: Vec<String> = Vec::new();
                    // Scene identity for the web save's project-folder matching (ignored on native).
                    let target = scene_target_json(&io, &scene_hash).await;
                    for (path, hash) in &asset.contents {
                        let rel = format!("{base_dir}/{path}");
                        // Fetch the bytes (the service worker serves them from cache on web) so we
                        // can both seed the native live-render cache AND write the file into the
                        // scene folder right now — the immediate push, so the asset renders without
                        // waiting for a save (and the dev server can serve it on the live merge).
                        let url = format!("{CONTENTS_BASE}{hash}");
                        match client.get(&url).send().await {
                            Ok(resp) => match resp.bytes().await {
                                Ok(bytes) => {
                                    let _ = io.cache_bytes(hash, &bytes).await; // no-op on web
                                    if let Err(e) = platform::write_scene_file(
                                        &scene_hash,
                                        &rel,
                                        &bytes,
                                        &target,
                                    )
                                    .await
                                    {
                                        if errors.len() < 5 {
                                            errors.push(format!("{rel}: {e}"));
                                        }
                                    } else {
                                        written_rels.push(rel.clone());
                                    }
                                }
                                Err(e) => {
                                    if errors.len() < 5 {
                                        errors.push(format!("{rel}: read: {e}"));
                                    }
                                }
                            },
                            Err(e) => {
                                if errors.len() < 5 {
                                    errors.push(format!("{rel}: fetch: {e}"));
                                }
                            }
                        }
                        // On a `dcl start` scene the dev server addresses files by a `b64-<path>`
                        // hash, not the source CID, and serves them by that — so map the path to the
                        // hash the dev server will use (else the live load 404s until a reload). On
                        // native/deployed scenes this is None and the source CID (served from the
                        // local cache we seeded above) is used.
                        let merge_hash = io
                            .local_b64_hash_for(&scene_hash, &rel)
                            .await
                            .unwrap_or_else(|| hash.clone());
                        merge.insert(rel.to_lowercase(), merge_hash);
                    }
                    io.merge_collection(&scene_hash, ContentMap(merge)).await;
                    // Wait for the dev server to index the freshly-written files before returning —
                    // the renderer's first load isn't retried on failure, so loading before the file
                    // is served 404s permanently (until a reload). Bounded so a genuinely-unservable
                    // file can't hang the import.
                    if !written_rels.is_empty() {
                        io.await_contents_available(
                            &written_rels,
                            &scene_hash,
                            std::time::Duration::from_secs(10),
                        )
                        .await;
                    }
                    let composite_str =
                        serde_json::to_string(&asset.composite).map_err(|e| e.to_string())?;
                    let composite_str = composite_str.replace("{assetPath}", &base_dir);
                    let composite: serde_json::Value =
                        serde_json::from_str(&composite_str).map_err(|e| e.to_string())?;
                    let reply = serde_json::json!({
                        "baseDir": base_dir, "composite": composite,
                        "written": written_rels.len(), "errors": errors,
                    });
                    serde_json::to_string(&reply).map_err(|e| e.to_string())
                }
                .await;
                let _ = tx.send(result);
            })
            .detach();
        console_responses.push_oneshot(rx, |r| r, input.take_responder());
    }
}

// --- scene target (web save folder matching) ---

/// Build the scene-identity JSON the web save uses to locate + verify the scene's project folder
/// under a granted directory handle: the clean project root (for navigation) plus a fingerprint —
/// `source.projectId` (authoritative), else `scene.parcels` + `display.title` — read from the
/// scene's scene.json metadata. Fields are null/empty on native or when the scene lacks them.
pub async fn scene_target_json(io: &IpfsIo, scene_hash: &str) -> String {
    let root = io.local_project_root(scene_hash).await;
    let meta: Option<serde_json::Value> = io
        .scene_metadata(scene_hash)
        .await
        .and_then(|m| serde_json::from_str(&m).ok());
    let str_at = |ptr: &str| {
        meta.as_ref()
            .and_then(|m| m.pointer(ptr))
            .and_then(|v| v.as_str())
            .map(String::from)
    };
    let parcels: Vec<String> = meta
        .as_ref()
        .and_then(|m| m.pointer("/scene/parcels"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    serde_json::json!({
        "root": root,
        "projectId": str_at("/source/projectId"),
        "parcels": parcels,
        "title": str_at("/display/title"),
    })
    .to_string()
}
