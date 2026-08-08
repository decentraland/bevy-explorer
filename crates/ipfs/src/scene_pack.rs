use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    Arc,
};

use anyhow::anyhow;
use bevy::{platform::collections::HashMap, prelude::*, tasks::IoTaskPool};
use common::util::TaskCompat;
use multihash_codetable::MultihashDigest;
use platform::AsyncRwLock;
use serde::Deserialize;
use web_time::Instant;

use crate::IpfsIo;

mod stream;
use stream::StreamingBuf;

#[cfg(test)]
mod harden;
#[cfg(test)]
mod testkit;

pub const PACK_MAGIC: &[u8; 8] = b"DCLBVPK\0";
pub const PACK_VERSION: u32 = 1;
pub const PACK_PROFILE: &str = "bv4";
const MAX_INDEX_LEN: usize = 16 * 1024 * 1024;

#[cfg(target_arch = "wasm32")]
const PACK_RESIDENT_BUDGET: u64 = 128 * 1024 * 1024;
#[cfg(not(target_arch = "wasm32"))]
const PACK_RESIDENT_BUDGET: u64 = 512 * 1024 * 1024;

#[derive(Deserialize)]
struct PackIndex {
    v: u32,
    profile: String,
    entity: String,
    payload: u64,
    files: Vec<PackIndexEntry>,
}

#[derive(Deserialize)]
struct PackIndexEntry {
    path: String,
    cid: String,
    off: u64,
    len: u64,
    kind: String,
    c: u8,
    sha256: String,
}

struct PackEntry {
    path: String,
    off: u64,
    len: u64,
    sha256: [u8; 32],
    verified: AtomicU64,
}

enum PackData {
    Streaming(StreamingBuf),
    Resident(Arc<Vec<u8>>),
    #[cfg(not(target_arch = "wasm32"))]
    File(std::path::PathBuf),
    Evicted,
}

pub struct ScenePack {
    url: String,
    entity: String,
    size: u64,
    payload_base: u64,
    prefix_end: u64,
    entries: Vec<PackEntry>,
    by_cid: HashMap<String, usize>,
    by_path: HashMap<String, usize>,
    data: AsyncRwLock<PackData>,
    refetch: tokio::sync::Mutex<()>,
    last_use: AtomicU64,
    available: AtomicU64,
    waiting: AtomicUsize,
    last_read: AtomicU64,
    /// materialization counter: bumped before each refetch installs data, so a
    /// verify racing an eviction can never stamp `verified` for bytes it did
    /// not hash (entry.verified stores the generation it was verified against)
    generation: AtomicU64,
    created: Instant,
}

pub(crate) enum PackState {
    Fetching(tokio::sync::watch::Receiver<()>),
    Ready(Arc<ScenePack>),
    Failed,
}

pub(crate) struct ScenePackRegistry {
    base_url: Option<String>,
    packs: AsyncRwLock<HashMap<String, PackState>>,
    use_counter: AtomicU64,
}

impl ScenePackRegistry {
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            base_url: base_url
                .map(|b| b.trim_end_matches('/').to_owned())
                .filter(|b| !b.is_empty()),
            packs: AsyncRwLock::new(HashMap::default()),
            use_counter: AtomicU64::new(0),
        }
    }

    fn bump_use(&self) -> u64 {
        self.use_counter.fetch_add(1, Ordering::Relaxed) + 1
    }
}

fn decode_sha256(hex: &str) -> Result<[u8; 32], anyhow::Error> {
    anyhow::ensure!(hex.len() == 64 && hex.is_ascii(), "bad sha256 encoding");
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)?;
    }
    Ok(out)
}

impl ScenePack {
    pub(crate) fn index_end(bytes: &[u8]) -> Result<Option<usize>, anyhow::Error> {
        if bytes.len() < 16 {
            return Ok(None);
        }
        anyhow::ensure!(&bytes[0..8] == PACK_MAGIC, "bad magic");
        let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        anyhow::ensure!(version == PACK_VERSION, "unsupported version {version}");
        let index_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        anyhow::ensure!(index_len <= MAX_INDEX_LEN, "index too large");
        Ok(Some(16 + index_len))
    }

    pub(crate) fn parse_index(
        url: String,
        entity: &str,
        bytes: &[u8],
    ) -> Result<Self, anyhow::Error> {
        let index_end = Self::index_end(bytes)?
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| anyhow!("index out of bounds"))?;
        let index: PackIndex = serde_json::from_slice(&bytes[16..index_end])?;
        anyhow::ensure!(index.v == PACK_VERSION, "index version mismatch");
        anyhow::ensure!(
            index.profile == PACK_PROFILE,
            "unexpected profile {}",
            index.profile
        );
        anyhow::ensure!(index.entity == entity, "entity mismatch");
        let payload_base = index_end.div_ceil(16) * 16;

        let mut entries = Vec::with_capacity(index.files.len());
        let mut by_cid = HashMap::default();
        let mut by_path = HashMap::default();
        let mut next_off = 0u64;
        let mut prefix_end = 0u64;
        for file in &index.files {
            let end = file
                .off
                .checked_add(file.len)
                .ok_or_else(|| anyhow!("entry range overflow: {}", file.path))?;
            anyhow::ensure!(end <= index.payload, "entry out of bounds: {}", file.path);
            anyhow::ensure!(file.off % 16 == 0, "entry misaligned: {}", file.path);
            anyhow::ensure!(
                matches!(file.kind.as_str(), "glb" | "img" | "raw"),
                "unknown entry kind: {}",
                file.kind
            );
            anyhow::ensure!(file.c <= 3, "bad entry class: {}", file.path);
            if file.c <= 1 {
                prefix_end = prefix_end.max(end);
            }
            let idx = entries.len();
            match by_cid.get(&file.cid) {
                Some(prev) => {
                    let prev: &PackEntry = &entries[*prev];
                    anyhow::ensure!(
                        prev.off == file.off && prev.len == file.len,
                        "cid dedup mismatch: {}",
                        file.cid
                    );
                }
                None => {
                    anyhow::ensure!(
                        file.off >= next_off,
                        "blobs unsorted or overlapping: {}",
                        file.path
                    );
                    next_off = end;
                    by_cid.insert(file.cid.clone(), idx);
                }
            }
            anyhow::ensure!(
                by_path.insert(file.path.clone(), idx).is_none(),
                "duplicate path: {}",
                file.path
            );
            entries.push(PackEntry {
                path: file.path.clone(),
                off: file.off,
                len: file.len,
                sha256: decode_sha256(&file.sha256)?,
                verified: AtomicU64::new(0),
            });
        }

        Ok(Self {
            url,
            entity: entity.to_owned(),
            size: payload_base as u64 + index.payload,
            payload_base: payload_base as u64,
            prefix_end,
            entries,
            by_cid,
            by_path,
            data: AsyncRwLock::new(PackData::Evicted),
            refetch: tokio::sync::Mutex::new(()),
            last_use: AtomicU64::new(0),
            available: AtomicU64::new(0),
            waiting: AtomicUsize::new(0),
            last_read: AtomicU64::new(0),
            generation: AtomicU64::new(1),
            created: Instant::now(),
        })
    }

    pub fn parse(url: String, entity: &str, bytes: &[u8]) -> Result<Self, anyhow::Error> {
        anyhow::ensure!(bytes.len() >= 16, "pack too short");
        let pack = Self::parse_index(url, entity, bytes)?;
        anyhow::ensure!(bytes.len() as u64 == pack.size, "payload length mismatch");
        pack.available.store(pack.payload(), Ordering::Relaxed);
        Ok(pack)
    }

    pub fn entry_by_cid(&self, cid: &str) -> Option<usize> {
        self.by_cid.get(cid).copied()
    }

    pub fn entry_by_path(&self, path: &str) -> Option<usize> {
        self.by_path.get(path).copied()
    }

    fn entry_range(&self, idx: usize) -> (usize, usize) {
        let entry = &self.entries[idx];
        let start = (self.payload_base + entry.off) as usize;
        (start, start + entry.len as usize)
    }

    fn payload(&self) -> u64 {
        self.size - self.payload_base
    }

    fn note_read(&self) {
        self.last_read
            .store(self.created.elapsed().as_millis() as u64, Ordering::Relaxed);
    }
}

#[derive(Clone)]
pub struct ArcSlice {
    data: Arc<Vec<u8>>,
    start: usize,
    end: usize,
}

impl AsRef<[u8]> for ArcSlice {
    fn as_ref(&self) -> &[u8] {
        &self.data[self.start..self.end]
    }
}

pub enum PackSlice {
    Mem(ArcSlice),
    Owned(Vec<u8>),
}

impl AsRef<[u8]> for PackSlice {
    fn as_ref(&self) -> &[u8] {
        match self {
            PackSlice::Mem(slice) => slice.as_ref(),
            PackSlice::Owned(bytes) => bytes,
        }
    }
}

impl IpfsIo {
    pub fn prefetch_scene_pack(self: &Arc<Self>, scene_hash: &str) {
        let Some(base) = self.scene_packs.base_url.clone() else {
            return;
        };
        if self
            .context
            .blocking_read()
            .modifiers
            .get(scene_hash)
            .is_some_and(|modifier| modifier.base_url.is_some())
        {
            return;
        }
        let (sender, receiver) = tokio::sync::watch::channel(());
        {
            let mut packs = self.scene_packs.packs.blocking_write();
            if packs.contains_key(scene_hash) {
                return;
            }
            packs.insert(scene_hash.to_owned(), PackState::Fetching(receiver));
        }
        let io = self.clone();
        let scene_hash = scene_hash.to_owned();
        IoTaskPool::get()
            .spawn_compat(async move {
                io.run_pack_stream(&base, &scene_hash, sender).await;
                io.enforce_pack_budget().await;
            })
            .detach();
    }

    pub fn release_scene_pack(&self, scene_hash: &str) {
        self.scene_packs.packs.blocking_write().remove(scene_hash);
    }

    pub(crate) async fn scene_pack_read(&self, scene_hash: &str, cid: &str) -> Option<PackSlice> {
        self.scene_packs.base_url.as_ref()?;
        let mut waited = false;
        loop {
            let waiter = {
                let packs = self.scene_packs.packs.read().await;
                match packs.get(scene_hash)? {
                    PackState::Failed => return None,
                    PackState::Ready(pack) => {
                        let pack = pack.clone();
                        drop(packs);
                        return self.serve_pack_entry(scene_hash, &pack, cid).await;
                    }
                    PackState::Fetching(_) if waited => return None,
                    PackState::Fetching(receiver) => receiver.clone(),
                }
            };
            let mut waiter = waiter;
            let _ = waiter.changed().await;
            waited = true;
        }
    }

    async fn serve_pack_entry(
        &self,
        scene_hash: &str,
        pack: &Arc<ScenePack>,
        cid: &str,
    ) -> Option<PackSlice> {
        pack.note_read();
        let idx = pack.entry_by_cid(cid)?;
        pack.last_use
            .store(self.scene_packs.bump_use(), Ordering::Relaxed);
        let gen_before = pack.generation.load(Ordering::SeqCst);
        let slice = match self.pack_entry_bytes(pack, idx).await {
            Ok(Some(slice)) => slice,
            Ok(None) => return None,
            Err(e) => {
                warn!("scene pack read failed for {scene_hash}: {e}");
                self.fail_scene_pack(scene_hash, &pack.url).await;
                return None;
            }
        };
        // the slice's generation is pinned only when the counter did not move
        // across the read; otherwise re-verify and leave the stamp alone
        let generation = pack.generation.load(Ordering::SeqCst);
        let entry = &pack.entries[idx];
        if gen_before != generation || entry.verified.load(Ordering::SeqCst) != generation {
            let digest = multihash_codetable::Code::Sha2_256.digest(slice.as_ref());
            if digest.digest() != entry.sha256 {
                warn!(
                    "scene pack entry `{}` failed verification, disabling pack for {scene_hash}",
                    entry.path
                );
                self.fail_scene_pack(scene_hash, &pack.url).await;
                return None;
            }
            if gen_before == generation {
                entry.verified.store(generation, Ordering::SeqCst);
            }
        }
        Some(slice)
    }

    async fn fail_scene_pack(&self, scene_hash: &str, url: &str) {
        if let Some(state) = self.scene_packs.packs.write().await.get_mut(scene_hash) {
            *state = PackState::Failed;
        }
        purge_pack_sw_cache(url).await;
    }

    async fn enforce_pack_budget(&self) {
        let packs: Vec<Arc<ScenePack>> = self
            .scene_packs
            .packs
            .read()
            .await
            .values()
            .filter_map(|state| match state {
                PackState::Ready(pack) => Some(pack.clone()),
                _ => None,
            })
            .collect();
        evict_packs_to_budget(packs, PACK_RESIDENT_BUDGET).await;
    }
}

async fn evict_packs_to_budget(packs: Vec<Arc<ScenePack>>, budget: u64) {
    let mut resident = Vec::new();
    for pack in packs {
        let len = match &*pack.data.read().await {
            PackData::Resident(bytes) => bytes.len() as u64,
            _ => continue,
        };
        resident.push((pack.last_use.load(Ordering::Relaxed), len, pack));
    }
    let mut total: u64 = resident.iter().map(|(_, len, _)| *len).sum();
    resident.sort_by_key(|(last_use, ..)| *last_use);
    let mut candidates = resident.iter();
    while total > budget && candidates.len() > 1 {
        let (_, len, pack) = candidates.next().unwrap();
        *pack.data.write().await = PackData::Evicted;
        total -= len;
    }
}

#[cfg(target_arch = "wasm32")]
async fn purge_pack_sw_cache(pack_url: &str) {
    use wasm_bindgen::JsCast;
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(caches) = window.caches() else {
        return;
    };
    let Ok(cache) = wasm_bindgen_futures::JsFuture::from(caches.open("ipfs-path-cache-v1")).await
    else {
        return;
    };
    let cache: web_sys::Cache = cache.unchecked_into();
    let key = url::Url::parse(pack_url)
        .map(|u| u.path().to_owned())
        .unwrap_or_else(|_| pack_url.to_owned());
    let _ = wasm_bindgen_futures::JsFuture::from(cache.delete_with_str(&key)).await;
}

#[cfg(not(target_arch = "wasm32"))]
async fn purge_pack_sw_cache(_pack_url: &str) {}

#[cfg(test)]
mod test {
    use super::*;

    pub(crate) fn sha_hex(bytes: &[u8]) -> String {
        multihash_codetable::Code::Sha2_256
            .digest(bytes)
            .digest()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    pub(crate) fn align16(len: usize) -> usize {
        len.div_ceil(16) * 16
    }

    pub(crate) fn class_for(path: &str) -> u8 {
        match path.rsplit('.').next().unwrap_or("") {
            "js" | "json" | "crdt" | "wasm" => 0,
            "glb" | "gltf" | "bin" => 1,
            "mp3" | "wav" | "flac" | "aac" | "oga" | "opus" => 3,
            _ => 2,
        }
    }

    pub(crate) fn build_pack(entity: &str, files: &[(&str, &str, &[u8])]) -> Vec<u8> {
        let mut blob_class: std::collections::HashMap<&str, u8> = Default::default();
        let mut blob_min_path: std::collections::HashMap<&str, &str> = Default::default();
        for (path, cid, _) in files {
            let class = class_for(path);
            blob_class
                .entry(cid)
                .and_modify(|c| *c = (*c).min(class))
                .or_insert(class);
            blob_min_path
                .entry(cid)
                .and_modify(|p| *p = (*p).min(path))
                .or_insert(path);
        }
        let mut sorted = files.to_vec();
        sorted.sort_by_key(|(path, cid, bytes)| {
            (
                blob_class[cid],
                bytes.len(),
                blob_min_path[cid].to_owned(),
                path.to_owned(),
            )
        });

        let mut blob_offsets: std::collections::HashMap<&str, (u64, u64)> = Default::default();
        let mut blobs = Vec::new();
        let mut off = 0u64;
        for (_, cid, bytes) in &sorted {
            if !blob_offsets.contains_key(cid) {
                blob_offsets.insert(cid, (off, bytes.len() as u64));
                blobs.push(*bytes);
                off += align16(bytes.len()) as u64;
            }
        }
        let payload = blobs
            .last()
            .map(|last| off - align16(last.len()) as u64 + last.len() as u64)
            .unwrap_or(0);

        let file_entries = sorted
            .iter()
            .map(|(path, cid, bytes)| {
                let (off, len) = blob_offsets[cid];
                format!(
                    r#"{{"path":"{path}","cid":"{cid}","off":{off},"len":{len},"kind":"raw","c":{},"sha256":"{}"}}"#,
                    blob_class[cid],
                    sha_hex(bytes)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let index = format!(
            r#"{{"v":1,"profile":"bv4","entity":"{entity}","payload":{payload},"files":[{file_entries}]}}"#
        );

        let mut out = Vec::new();
        out.extend_from_slice(PACK_MAGIC);
        out.extend_from_slice(&PACK_VERSION.to_le_bytes());
        out.extend_from_slice(&(index.len() as u32).to_le_bytes());
        out.extend_from_slice(index.as_bytes());
        out.resize(align16(out.len()), 0);
        for bytes in &blobs {
            let aligned = align16(out.len());
            out.resize(aligned, 0);
            out.extend_from_slice(bytes);
        }
        out
    }

    fn fixture() -> (String, Vec<u8>) {
        let files: &[(&str, &str, &[u8])] = &[
            ("models/thing.glb", "bafyglb", b"glb bytes here padded"),
            ("scene.json", "bafyjson", b"{}"),
            ("textures/a.png", "bafypng", b"png bytes"),
            ("textures/a_copy.png", "bafypng", b"png bytes"),
        ];
        ("entity1".to_owned(), build_pack("entity1", files))
    }

    #[test]
    fn parses_valid_pack() {
        let (entity, bytes) = fixture();
        let pack = ScenePack::parse("http://x/p.pack".into(), &entity, &bytes).unwrap();
        assert_eq!(pack.entries.len(), 4);
        assert_eq!(pack.by_cid.len(), 3);
        assert_eq!(pack.size, bytes.len() as u64);
        let glb = pack.entry_by_cid("bafyglb").unwrap();
        assert_eq!(pack.entry_by_path("models/thing.glb"), Some(glb));
        let (start, end) = pack.entry_range(glb);
        assert_eq!(&bytes[start..end], b"glb bytes here padded");
        assert_eq!(
            pack.prefix_end,
            pack.entries[glb].off + pack.entries[glb].len
        );
        let a = pack.entry_by_path("textures/a.png").unwrap();
        let b = pack.entry_by_path("textures/a_copy.png").unwrap();
        assert_eq!(pack.entry_range(a), pack.entry_range(b));
        assert!(pack.entries[a].off > pack.entries[glb].off, "demand order");
    }

    #[test]
    fn rejects_corrupt_packs() {
        let (entity, bytes) = fixture();
        let parse = |bytes: &[u8]| ScenePack::parse("u".into(), &entity, bytes);

        assert!(parse(&bytes[0..12]).is_err());

        let mut bad_magic = bytes.clone();
        bad_magic[0] = b'X';
        assert!(parse(&bad_magic).is_err());

        let mut bad_version = bytes.clone();
        bad_version[8] = 9;
        assert!(parse(&bad_version).is_err());

        let mut oversize_index = bytes.clone();
        oversize_index[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse(&oversize_index).is_err());

        let truncated_payload = &bytes[0..bytes.len() - 1];
        assert!(parse(truncated_payload).is_err());

        assert!(ScenePack::parse("u".into(), "other-entity", &bytes).is_err());

        let patch_index = |from: &str, to: &str| {
            assert_eq!(from.len(), to.len());
            let index_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
            let index = std::str::from_utf8(&bytes[16..16 + index_len]).unwrap();
            let pos = index
                .find(from)
                .unwrap_or_else(|| panic!("fixture drifted: `{from}` not found"));
            let mut patched = bytes.clone();
            patched[16 + pos..16 + pos + to.len()].copy_from_slice(to.as_bytes());
            patched
        };
        assert!(parse(&patch_index(r#""off":16"#, r#""off":17"#)).is_err());
        assert!(parse(&patch_index(r#""off":48"#, r#""off":16"#)).is_err());
        assert!(parse(&patch_index(r#""off":48"#, r#""off":96"#)).is_err());
        assert!(parse(&patch_index(r#""c":2"#, r#""c":9"#)).is_err());
        assert!(parse(&patch_index(r#""c":0,"#, r#"      "#)).is_err());
        assert!(parse(&patch_index(r#""profile":"bv4""#, r#""profile":"bv3""#)).is_err());

        let glb_sha = sha_hex(b"glb bytes here padded");
        let bad_sha = format!("zz{}", &glb_sha[2..]);
        assert!(parse(&patch_index(&glb_sha, &bad_sha)).is_err());
    }

    #[test]
    fn slice_reader_matches_vec_reader() {
        use crate::AsyncCursor;
        use bevy::asset::io::{AsyncSeekForwardExt, Reader, VecReader};

        let (entity, bytes) = fixture();
        let pack = ScenePack::parse("u".into(), &entity, &bytes).unwrap();
        let idx = pack.entry_by_path("models/thing.glb").unwrap();
        let (start, end) = pack.entry_range(idx);

        async_std::task::block_on(async {
            let slice = PackSlice::Mem(ArcSlice {
                data: Arc::new(bytes.clone()),
                start,
                end,
            });
            let mut cursor = AsyncCursor::new(slice);
            let mut via_pack = Vec::new();
            Reader::read_to_end(&mut cursor, &mut via_pack)
                .await
                .unwrap();

            let mut vec_reader = VecReader::new(bytes[start..end].to_vec());
            let mut via_vec = Vec::new();
            Reader::read_to_end(&mut vec_reader, &mut via_vec)
                .await
                .unwrap();
            assert_eq!(via_pack, via_vec);

            let slice = PackSlice::Mem(ArcSlice {
                data: Arc::new(bytes.clone()),
                start,
                end,
            });
            let mut cursor = AsyncCursor::new(slice);
            cursor.seek_forward(4).await.unwrap();
            let mut tail = Vec::new();
            Reader::read_to_end(&mut cursor, &mut tail).await.unwrap();
            assert_eq!(tail, via_vec[4..].to_vec());
        });
    }

    #[test]
    fn evicts_least_recently_used_within_budget() {
        let (entity, bytes) = fixture();
        async_std::task::block_on(async {
            let mut packs = Vec::new();
            for last_use in 1..=3u64 {
                let pack = Arc::new(ScenePack::parse("u".into(), &entity, &bytes).unwrap());
                *pack.data.write().await = PackData::Resident(Arc::new(bytes.clone()));
                pack.last_use.store(last_use, Ordering::Relaxed);
                packs.push(pack);
            }
            evict_packs_to_budget(packs.clone(), packs[0].size).await;

            assert!(matches!(&*packs[0].data.read().await, PackData::Evicted));
            assert!(matches!(&*packs[1].data.read().await, PackData::Evicted));
            assert!(matches!(
                &*packs[2].data.read().await,
                PackData::Resident(_)
            ));

            evict_packs_to_budget(vec![packs[2].clone()], 1).await;
            assert!(matches!(
                &*packs[2].data.read().await,
                PackData::Resident(_)
            ));

            *packs[0].data.write().await = PackData::Resident(Arc::new(bytes.clone()));
            assert!(matches!(
                &*packs[0].data.read().await,
                PackData::Resident(_)
            ));
        });
    }

    pub(crate) fn test_io() -> IpfsIo {
        IpfsIo::new(
            false,
            Box::new(bevy::asset::io::memory::MemoryAssetReader {
                root: Default::default(),
            }),
            None,
            HashMap::default(),
            4,
            Some("http://localhost:1".to_owned()),
            #[cfg(feature = "ipfs_debug")]
            tokio::sync::mpsc::unbounded_channel().0,
        )
    }

    pub(crate) async fn run_fetch(io: &IpfsIo, base: &str, entity: &str) {
        let (sender, receiver) = tokio::sync::watch::channel(());
        io.scene_packs
            .packs
            .write()
            .await
            .insert(entity.to_owned(), PackState::Fetching(receiver));
        io.run_pack_stream(base, entity, sender).await;
    }

    fn serve_bytes(routes: Vec<(String, Option<Vec<u8>>)>) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut buf = [0u8; 2048];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);
                let path = request.split_whitespace().nth(1).unwrap_or("");
                let response = match routes
                    .iter()
                    .find(|(route, _)| route == path)
                    .and_then(|(_, body)| body.as_ref())
                {
                    Some(body) => {
                        let mut response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                        response.extend_from_slice(body);
                        response
                    }
                    None => {
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_vec()
                    }
                };
                let _ = stream.write_all(&response);
            }
        });
        base
    }

    #[test]
    fn fetch_rejects_absent_and_corrupt_packs() {
        let (entity, bytes) = fixture();
        let mut truncated = build_pack("bad-entity", &[("a.bin", "bafya", b"raw bytes")]);
        truncated.pop();
        let base = serve_bytes(vec![
            (
                format!("/bvwebgpu/{PACK_PROFILE}/{entity}.pack"),
                Some(bytes.clone()),
            ),
            (
                format!("/bvwebgpu/{PACK_PROFILE}/bad-entity.pack"),
                Some(truncated),
            ),
            (
                format!("/bvwebgpu/{PACK_PROFILE}/wrong-entity.pack"),
                Some(bytes),
            ),
        ]);
        let io = test_io();

        async_std::task::block_on(platform::compat(async {
            run_fetch(&io, &base, "absent-entity").await;
            assert!(matches!(
                io.scene_packs.packs.read().await.get("absent-entity"),
                Some(PackState::Failed)
            ));

            run_fetch(&io, &base, "wrong-entity").await;
            assert!(matches!(
                io.scene_packs.packs.read().await.get("wrong-entity"),
                Some(PackState::Failed)
            ));

            run_fetch(&io, &base, "bad-entity").await;
            assert!(matches!(
                io.scene_packs.packs.read().await.get("bad-entity"),
                Some(PackState::Ready(_))
            ));
            assert!(io.scene_pack_read("bad-entity", "bafya").await.is_none());

            run_fetch(&io, &base, &entity).await;
            assert!(matches!(
                io.scene_packs.packs.read().await.get(&entity),
                Some(PackState::Ready(_))
            ));
            let served = io.scene_pack_read(&entity, "bafyglb").await.unwrap();
            assert_eq!(served.as_ref(), b"glb bytes here padded");
        }));
    }

    #[test]
    fn serves_verified_entries_and_fails_on_corruption() {
        let (entity, bytes) = fixture();
        let pack = Arc::new(ScenePack::parse("u".into(), &entity, &bytes).unwrap());
        let io = test_io();

        async_std::task::block_on(platform::compat(async {
            *pack.data.write().await = PackData::Resident(Arc::new(bytes.clone()));
            io.scene_packs
                .packs
                .write()
                .await
                .insert(entity.clone(), PackState::Ready(pack.clone()));

            let served = io.scene_pack_read(&entity, "bafyglb").await.unwrap();
            assert_eq!(served.as_ref(), b"glb bytes here padded");
            assert!(io.scene_pack_read(&entity, "missing-cid").await.is_none());

            let mut corrupt = bytes.clone();
            let idx = pack.entry_by_cid("bafyjson").unwrap();
            let (start, _) = pack.entry_range(idx);
            corrupt[start] ^= 0xff;
            *pack.data.write().await = PackData::Resident(Arc::new(corrupt));
            assert!(io.scene_pack_read(&entity, "bafyjson").await.is_none());
            assert!(matches!(
                io.scene_packs.packs.read().await.get(&entity),
                Some(PackState::Failed)
            ));

            io.release_scene_pack(&entity);
            assert!(io.scene_pack_read(&entity, "bafyglb").await.is_none());

            let unreachable = Arc::new(
                ScenePack::parse("http://localhost:1/p.pack".into(), &entity, &bytes).unwrap(),
            );
            io.scene_packs
                .packs
                .write()
                .await
                .insert(entity.clone(), PackState::Ready(unreachable));
            assert!(io.scene_pack_read(&entity, "bafyglb").await.is_none());
            assert!(matches!(
                io.scene_packs.packs.read().await.get(&entity),
                Some(PackState::Failed)
            ));
        }));
    }
}
