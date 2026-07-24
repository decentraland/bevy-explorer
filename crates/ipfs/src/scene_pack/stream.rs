use std::sync::{atomic::Ordering, Arc};

use anyhow::anyhow;
use bevy::prelude::*;
use web_time::Duration;

use crate::IpfsIo;

use super::{
    purge_pack_sw_cache, ArcSlice, PackData, PackSlice, PackState, ScenePack, PACK_PROFILE,
};

const PACK_IDLE_ABORT_SECS: f32 = 5.0;
const DRIVE_TICK: Duration = Duration::from_secs(1);
const STALL_TICKS: u32 = 10;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Phase {
    Streaming,
    Complete,
    Aborted,
    Failed,
}

#[derive(Clone, Copy)]
pub(super) struct StreamProgress {
    received: u64,
    phase: Phase,
}

pub(super) struct StreamingBuf {
    buf: Vec<u8>,
    progress: tokio::sync::watch::Receiver<StreamProgress>,
}

struct WaitingGuard<'a>(&'a ScenePack);

impl<'a> WaitingGuard<'a> {
    fn new(pack: &'a ScenePack) -> Self {
        pack.waiting.fetch_add(1, Ordering::SeqCst);
        Self(pack)
    }
}

impl Drop for WaitingGuard<'_> {
    fn drop(&mut self) {
        self.0.waiting.fetch_sub(1, Ordering::SeqCst);
    }
}

fn configured_idle_abort() -> Option<Duration> {
    let secs = idle_abort_override().unwrap_or(PACK_IDLE_ABORT_SECS);
    (secs > 0.0).then(|| Duration::from_secs_f32(secs))
}

#[cfg(target_arch = "wasm32")]
fn idle_abort_override() -> Option<f32> {
    let search = web_sys::window()?.location().search().ok()?;
    search
        .trim_start_matches('?')
        .split('&')
        .find_map(|kv| kv.strip_prefix("packs_idle="))
        .and_then(|v| v.parse().ok())
}

#[cfg(not(target_arch = "wasm32"))]
fn idle_abort_override() -> Option<f32> {
    std::env::var("SCENE_PACK_IDLE_ABORT_SECS")
        .ok()?
        .parse()
        .ok()
}

fn payload_received(pack: &ScenePack, wire_len: usize) -> u64 {
    (wire_len as u64)
        .saturating_sub(pack.payload_base)
        .min(pack.payload())
}

fn should_abort(pack: &ScenePack, idle_abort: Option<Duration>) -> bool {
    let Some(idle) = idle_abort else {
        return false;
    };
    if pack.waiting.load(Ordering::SeqCst) != 0 {
        return false;
    }
    if pack.available.load(Ordering::Relaxed) < pack.prefix_end {
        return false;
    }
    let now_ms = pack.created.elapsed().as_millis() as u64;
    now_ms.saturating_sub(pack.last_read.load(Ordering::Relaxed)) > idle.as_millis() as u64
}

fn fetch_error<E: std::fmt::Display>(e: platform::FetchError<E>) -> anyhow::Error {
    match e {
        platform::FetchError::Headers => anyhow!("timed out awaiting headers"),
        platform::FetchError::Send(e) => anyhow!("{e}"),
        platform::FetchError::Status(s) => anyhow!("status {s}"),
        platform::FetchError::Stalled => anyhow!("stalled"),
        platform::FetchError::Body(e) => anyhow!(e),
    }
}

enum OpenError {
    Fetch(anyhow::Error),
    Parse(anyhow::Error),
}

enum Outcome {
    Complete,
    Aborted,
    Failed(anyhow::Error),
    Surplus,
}

impl IpfsIo {
    pub(super) async fn run_pack_stream(
        &self,
        base: &str,
        scene_hash: &str,
        fetching: tokio::sync::watch::Sender<()>,
    ) {
        let url = format!("{base}/bvwebgpu/{PACK_PROFILE}/{scene_hash}.pack");
        // a release + re-prefetch installs a fresh Fetching slot; a stale driver
        // must never resolve a slot it does not own
        let own_slot = fetching.subscribe();
        let (stream, pack, buf) = match self.open_pack_stream(&url, scene_hash).await {
            Ok(opened) => opened,
            Err(e) => {
                match e {
                    OpenError::Fetch(e) => info!("scene pack unavailable for {scene_hash}: {e}"),
                    OpenError::Parse(e) => warn!("invalid scene pack for {scene_hash}: {e}"),
                }
                purge_pack_sw_cache(&url).await;
                let mut packs = self.scene_packs.packs.write().await;
                if matches!(packs.get(scene_hash), Some(PackState::Fetching(rx)) if rx.same_channel(&own_slot))
                {
                    packs.insert(scene_hash.to_owned(), PackState::Failed);
                }
                return;
            }
        };
        let total = buf.len();
        let received = payload_received(&pack, total);
        let (progress_tx, progress_rx) = tokio::sync::watch::channel(StreamProgress {
            received,
            phase: Phase::Streaming,
        });
        pack.available.store(received, Ordering::Relaxed);
        *pack.data.write().await = PackData::Streaming(StreamingBuf {
            buf,
            progress: progress_rx,
        });
        pack.last_use
            .store(self.scene_packs.bump_use(), Ordering::Relaxed);
        {
            let mut packs = self.scene_packs.packs.write().await;
            if !matches!(packs.get(scene_hash), Some(PackState::Fetching(rx)) if rx.same_channel(&own_slot))
            {
                return;
            }
            packs.insert(scene_hash.to_owned(), PackState::Ready(pack.clone()));
        }
        drop(fetching);
        info!(
            "scene pack ready for {}: {} files, {} bytes",
            pack.entity,
            pack.entries.len(),
            pack.size
        );
        self.drive_pack_stream(&pack, stream, progress_tx, total)
            .await;
    }

    async fn open_pack_stream(
        &self,
        url: &str,
        entity: &str,
    ) -> Result<(platform::FetchStream, Arc<ScenePack>, Vec<u8>), OpenError> {
        let request = self
            .client
            .get(url)
            .header("X-IPFS", "true")
            .build()
            .map_err(|e| OpenError::Fetch(anyhow!(e)))?;
        let mut stream = platform::fetch_stream(
            self.client.execute(request),
            Duration::from_secs(30),
            Duration::from_secs(10),
        )
        .await
        .map_err(|e| OpenError::Fetch(fetch_error(e)))?;

        let mut buf = Vec::new();
        let index_end = loop {
            match ScenePack::index_end(&buf).map_err(OpenError::Parse)? {
                Some(end) if buf.len() >= end => break end,
                _ => {}
            }
            match stream.next_chunk().await {
                Ok(Some(chunk)) => buf.extend_from_slice(&chunk),
                Ok(None) => return Err(OpenError::Parse(anyhow!("truncated before index"))),
                Err(e) => return Err(OpenError::Fetch(fetch_error(e))),
            }
        };
        let pack = ScenePack::parse_index(url.to_owned(), entity, &buf[..index_end])
            .map_err(OpenError::Parse)?;
        if buf.len() as u64 > pack.size {
            return Err(OpenError::Parse(anyhow!("payload length mismatch")));
        }
        Ok((stream, Arc::new(pack), buf))
    }

    async fn drive_pack_stream(
        &self,
        pack: &Arc<ScenePack>,
        mut stream: platform::FetchStream,
        progress: tokio::sync::watch::Sender<StreamProgress>,
        mut total: usize,
    ) {
        let idle_abort = configured_idle_abort();
        // a forced eviction + rematerialize can install a fresh buffer while
        // this driver still runs; only the materialization it created may be
        // appended to or finalized
        let own = progress.subscribe();
        let mut stall_ticks = 0u32;
        let outcome = loop {
            if total as u64 == pack.size {
                break Outcome::Complete;
            }
            if should_abort(pack, idle_abort) {
                break Outcome::Aborted;
            }
            match platform::with_timeout(DRIVE_TICK, stream.next_chunk()).await {
                Err(platform::Elapsed) => {
                    stall_ticks += 1;
                    if stall_ticks >= STALL_TICKS {
                        break Outcome::Failed(anyhow!("stalled"));
                    }
                }
                Ok(Ok(Some(chunk))) => {
                    stall_ticks = 0;
                    let mut data = pack.data.write().await;
                    let PackData::Streaming(live) = &mut *data else {
                        return;
                    };
                    if !live.progress.same_channel(&own) {
                        return;
                    }
                    live.buf.extend_from_slice(&chunk);
                    total = live.buf.len();
                    if total as u64 > pack.size {
                        break Outcome::Surplus;
                    }
                    let received = payload_received(pack, total);
                    pack.available.store(received, Ordering::Relaxed);
                    drop(data);
                    progress.send_modify(|p| p.received = received);
                }
                Ok(Ok(None)) => {
                    break if total as u64 == pack.size {
                        Outcome::Complete
                    } else {
                        Outcome::Failed(anyhow!("closed early"))
                    };
                }
                Ok(Err(e)) => break Outcome::Failed(fetch_error(e)),
            }
        };
        drop(stream);
        match outcome {
            Outcome::Complete => {
                self.finalize_stream_data(pack, true, &own).await;
                progress.send_modify(|p| p.phase = Phase::Complete);
                info!(
                    "scene pack stream complete for {}: {}/{} bytes",
                    pack.entity, total, pack.size
                );
            }
            Outcome::Aborted => {
                self.finalize_stream_data(pack, false, &own).await;
                progress.send_modify(|p| p.phase = Phase::Aborted);
                info!(
                    "scene pack stream aborted for {}: {}/{} bytes",
                    pack.entity, total, pack.size
                );
            }
            Outcome::Failed(e) => {
                self.finalize_stream_data(pack, false, &own).await;
                progress.send_modify(|p| p.phase = Phase::Failed);
                warn!(
                    "scene pack stream failed for {}: {e} ({}/{} bytes)",
                    pack.entity, total, pack.size
                );
            }
            Outcome::Surplus => {
                self.finalize_stream_data(pack, false, &own).await;
                progress.send_modify(|p| p.phase = Phase::Failed);
                warn!(
                    "scene pack stream overran for {}: {}/{} bytes",
                    pack.entity, total, pack.size
                );
                self.fail_scene_pack(&pack.entity, &pack.url).await;
            }
        }
    }

    async fn finalize_stream_data(
        &self,
        pack: &Arc<ScenePack>,
        complete: bool,
        own: &tokio::sync::watch::Receiver<StreamProgress>,
    ) {
        let mut data = pack.data.write().await;
        let PackData::Streaming(live) = &mut *data else {
            return;
        };
        if !live.progress.same_channel(own) {
            return;
        }
        let mut body = std::mem::take(&mut live.buf);
        pack.available
            .store(payload_received(pack, body.len()), Ordering::Relaxed);
        *data = if complete {
            self.completed_pack_data(pack, body)
        } else {
            // partial packs live on under the LRU budget, which accounts len();
            // drop the growth slack so capacity matches what is accounted
            body.shrink_to_fit();
            PackData::Resident(Arc::new(body))
        };
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn completed_pack_data(&self, pack: &ScenePack, body: Vec<u8>) -> PackData {
        if let Some(cache_path) = self.cache_path() {
            let tmp = cache_path.join(format!("{}.bvpack.part", pack.entity));
            let path = cache_path.join(format!("{}.bvpack", pack.entity));
            if std::fs::write(&tmp, &body)
                .and_then(|_| std::fs::rename(&tmp, &path))
                .is_ok()
            {
                return PackData::File(path);
            }
        }
        PackData::Resident(Arc::new(body))
    }

    #[cfg(target_arch = "wasm32")]
    fn completed_pack_data(&self, _pack: &ScenePack, body: Vec<u8>) -> PackData {
        PackData::Resident(Arc::new(body))
    }

    pub(super) async fn rematerialize_pack(
        &self,
        pack: &Arc<ScenePack>,
    ) -> Result<(), anyhow::Error> {
        let guard = pack.refetch.lock().await;
        if !matches!(&*pack.data.read().await, PackData::Evicted) {
            return Ok(());
        }
        let request = self
            .client
            .get(&pack.url)
            .header("X-IPFS", "true")
            .build()?;
        let mut stream = platform::fetch_stream(
            self.client.execute(request),
            Duration::from_secs(30),
            Duration::from_secs(10),
        )
        .await
        .map_err(fetch_error)?;
        let mut buf = Vec::new();
        loop {
            match ScenePack::index_end(&buf)? {
                Some(end) if buf.len() >= end => {
                    anyhow::ensure!(
                        (end.div_ceil(16) * 16) as u64 == pack.payload_base,
                        "refetched pack layout changed"
                    );
                    break;
                }
                _ => {}
            }
            match stream.next_chunk().await {
                Ok(Some(chunk)) => {
                    buf.extend_from_slice(&chunk);
                    anyhow::ensure!(buf.len() as u64 <= pack.size, "refetched pack size changed");
                }
                Ok(None) => anyhow::bail!("truncated before index"),
                Err(e) => return Err(fetch_error(e)),
            }
        }
        pack.generation.fetch_add(1, Ordering::SeqCst);
        let total = buf.len();
        let received = payload_received(pack, total);
        let (progress_tx, progress_rx) = tokio::sync::watch::channel(StreamProgress {
            received,
            phase: Phase::Streaming,
        });
        pack.available.store(received, Ordering::Relaxed);
        *pack.data.write().await = PackData::Streaming(StreamingBuf {
            buf,
            progress: progress_rx,
        });
        pack.last_use
            .store(self.scene_packs.bump_use(), Ordering::Relaxed);
        drop(guard);
        self.drive_pack_stream(pack, stream, progress_tx, total)
            .await;
        self.enforce_pack_budget().await;
        Ok(())
    }

    pub(super) async fn pack_entry_bytes(
        &self,
        pack: &Arc<ScenePack>,
        idx: usize,
    ) -> Result<Option<PackSlice>, anyhow::Error> {
        let (start, end) = pack.entry_range(idx);
        // zero-length entries need no payload bytes; the buffer may end before
        // payload_base, so indexing it at `start` would be out of bounds
        if start == end {
            return Ok(Some(PackSlice::Owned(Vec::new())));
        }
        let entry_end = {
            let entry = &pack.entries[idx];
            entry.off + entry.len
        };
        let mut wait_guard = None;
        let mut sender_gone = false;
        loop {
            let waiter = {
                let data = pack.data.read().await;
                match &*data {
                    PackData::Resident(bytes) => {
                        return Ok((entry_end <= pack.available.load(Ordering::Relaxed)).then(
                            || {
                                PackSlice::Mem(ArcSlice {
                                    data: bytes.clone(),
                                    start,
                                    end,
                                })
                            },
                        ));
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    PackData::File(path) => {
                        use std::io::{Read, Seek, SeekFrom};
                        let path = path.clone();
                        drop(data);
                        let mut file = std::fs::File::open(&path)?;
                        file.seek(SeekFrom::Start(start as u64))?;
                        let mut buf = vec![0u8; end - start];
                        file.read_exact(&mut buf)?;
                        return Ok(Some(PackSlice::Owned(buf)));
                    }
                    PackData::Streaming(live) => {
                        if entry_end <= pack.available.load(Ordering::Relaxed) {
                            return Ok(Some(PackSlice::Owned(live.buf[start..end].to_vec())));
                        }
                        if live.progress.borrow().phase != Phase::Streaming || sender_gone {
                            return Ok(None);
                        }
                        Some(live.progress.clone())
                    }
                    PackData::Evicted => None,
                }
            };
            if wait_guard.is_none() {
                wait_guard = Some(WaitingGuard::new(pack));
            }
            match waiter {
                Some(mut receiver) => {
                    receiver.borrow_and_update();
                    if entry_end <= pack.available.load(Ordering::Relaxed) {
                        continue;
                    }
                    if receiver.borrow().phase != Phase::Streaming {
                        continue;
                    }
                    if receiver.changed().await.is_err() {
                        sender_gone = true;
                    }
                }
                None => {
                    sender_gone = false;
                    self.rematerialize_pack(pack).await?;
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::scene_pack::{
        test::{build_pack, run_fetch, test_io},
        testkit::{
            full, idle_env, is_streaming, pack_route, ready_pack, serve_scripted, spawn_fetch,
            stream_fixture, wire_start, Gate, Script,
        },
        PackSlice, PackState,
    };
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    #[test]
    fn streams_serve_before_complete() {
        let _env = idle_env("5");
        let (entity, bytes) = stream_fixture();
        let hold = wire_start(&bytes, &entity, "bafymp3");
        let gate = Gate::new();
        let (base, _) = serve_scripted(
            pack_route(&entity),
            bytes.clone(),
            vec![Script {
                hold_at: Some((hold, gate.clone())),
                ..full(512)
            }],
        );
        let io = Arc::new(test_io());
        async_std::task::block_on(platform::compat(async {
            let driver = spawn_fetch(&io, &base, &entity).await;
            let served = io.scene_pack_read(&entity, "bafyjson").await.unwrap();
            assert_eq!(served.as_ref(), b"{}");
            let pack = ready_pack(&io, &entity).await;
            assert!(is_streaming(&pack).await, "tail held: still streaming");
            gate.open();
            driver.await;
            assert!(matches!(&*pack.data.read().await, PackData::Resident(_)));
            assert_eq!(pack.available.load(Ordering::Relaxed), pack.payload());
            let tail = io.scene_pack_read(&entity, "bafymp3").await.unwrap();
            assert_eq!(tail.as_ref().len(), 4096);
        }));
    }

    #[test]
    fn aborts_when_idle_after_prefix() {
        let _env = idle_env("0.2");
        let (entity, bytes) = stream_fixture();
        let hold = wire_start(&bytes, &entity, "bafymp3");
        let gate = Gate::new();
        let (base, broken) = serve_scripted(
            pack_route(&entity),
            bytes.clone(),
            vec![Script {
                hold_at: Some((hold, gate.clone())),
                ..full(512)
            }],
        );
        let io = Arc::new(test_io());
        async_std::task::block_on(platform::compat(async {
            let driver = spawn_fetch(&io, &base, &entity).await;
            let pack = ready_pack(&io, &entity).await;
            assert!(
                platform::with_timeout(Duration::from_secs(15), driver)
                    .await
                    .is_ok(),
                "driver did not abort"
            );
            assert!(matches!(&*pack.data.read().await, PackData::Resident(_)));
            let available = pack.available.load(Ordering::Relaxed);
            assert!(available >= pack.prefix_end && available < pack.payload());
            assert!(matches!(
                io.scene_packs.packs.read().await.get(&entity),
                Some(PackState::Ready(_))
            ));
            let start = web_time::Instant::now();
            assert!(io.scene_pack_read(&entity, "bafymp3").await.is_none());
            assert!(
                start.elapsed() < Duration::from_millis(500),
                "tail miss must not wait"
            );
            assert_eq!(
                io.scene_pack_read(&entity, "bafyjson")
                    .await
                    .unwrap()
                    .as_ref(),
                b"{}"
            );
            gate.open();
            for _ in 0..100 {
                if broken.load(Ordering::SeqCst) {
                    break;
                }
                async_std::task::sleep(Duration::from_millis(50)).await;
            }
            assert!(
                broken.load(Ordering::SeqCst),
                "server never saw the closed connection"
            );
        }));
    }

    #[test]
    fn pending_tail_read_blocks_abort() {
        let _env = idle_env("0.2");
        let (entity, bytes) = stream_fixture();
        let hold = wire_start(&bytes, &entity, "bafymp3");
        let gate = Gate::new();
        let (base, _) = serve_scripted(
            pack_route(&entity),
            bytes.clone(),
            vec![Script {
                hold_at: Some((hold, gate.clone())),
                ..full(512)
            }],
        );
        let io = Arc::new(test_io());
        async_std::task::block_on(platform::compat(async {
            let driver = spawn_fetch(&io, &base, &entity).await;
            let io2 = io.clone();
            let ent = entity.clone();
            let reader = async_std::task::spawn(platform::compat(async move {
                io2.scene_pack_read(&ent, "bafymp3").await
            }));
            async_std::task::sleep(Duration::from_millis(2500)).await;
            let pack = ready_pack(&io, &entity).await;
            assert!(
                is_streaming(&pack).await,
                "pending read must hold the stream open"
            );
            gate.open();
            let tail = reader.await.unwrap();
            assert_eq!(tail.as_ref().len(), 4096);
            driver.await;
        }));
    }

    #[test]
    fn no_abort_before_prefix_delivered() {
        let _env = idle_env("0.2");
        let glb: Vec<u8> = (0..200u8).map(|i| i.wrapping_mul(3)).collect();
        let files: &[(&str, &str, &[u8])] = &[
            ("scene.json", "bafyjson", b"{}"),
            ("models/big.glb", "bafyglb", &glb),
        ];
        let entity = "prefix-entity".to_owned();
        let bytes = build_pack(&entity, files);
        let hold = wire_start(&bytes, &entity, "bafyglb") + 10;
        let gate = Gate::new();
        let (base, _) = serve_scripted(
            pack_route(&entity),
            bytes.clone(),
            vec![Script {
                hold_at: Some((hold, gate.clone())),
                ..full(512)
            }],
        );
        let io = Arc::new(test_io());
        async_std::task::block_on(platform::compat(async {
            let driver = spawn_fetch(&io, &base, &entity).await;
            let pack = ready_pack(&io, &entity).await;
            async_std::task::sleep(Duration::from_millis(2500)).await;
            assert!(
                is_streaming(&pack).await,
                "must not abort before the prefix"
            );
            gate.open();
            driver.await;
            assert_eq!(pack.available.load(Ordering::Relaxed), pack.payload());
            let served = io.scene_pack_read(&entity, "bafyglb").await.unwrap();
            assert_eq!(served.as_ref(), &glb[..]);
        }));
    }

    #[test]
    fn truncation_keeps_delivered_entries() {
        let _env = idle_env("5");
        let (entity, bytes) = stream_fixture();
        let cut = wire_start(&bytes, &entity, "bafymp3") + 100;
        let (base, _) = serve_scripted(
            pack_route(&entity),
            bytes.clone(),
            vec![Script {
                truncate_at: Some(cut),
                ..full(512)
            }],
        );
        let io = Arc::new(test_io());
        async_std::task::block_on(platform::compat(async {
            run_fetch(&io, &base, &entity).await;
            assert!(matches!(
                io.scene_packs.packs.read().await.get(&entity),
                Some(PackState::Ready(_))
            ));
            let pack = ready_pack(&io, &entity).await;
            assert!(matches!(&*pack.data.read().await, PackData::Resident(_)));
            assert!(pack.available.load(Ordering::Relaxed) < pack.payload());
            assert!(io.scene_pack_read(&entity, "bafymp3").await.is_none());
            assert_eq!(
                io.scene_pack_read(&entity, "bafyjson")
                    .await
                    .unwrap()
                    .as_ref(),
                b"{}"
            );
        }));
    }

    #[test]
    fn index_truncation_fails_pack() {
        let _env = idle_env("5");
        let (entity, bytes) = stream_fixture();
        let (base, _) = serve_scripted(
            pack_route(&entity),
            bytes,
            vec![Script {
                truncate_at: Some(20),
                ..full(8)
            }],
        );
        let io = Arc::new(test_io());
        async_std::task::block_on(platform::compat(async {
            run_fetch(&io, &base, &entity).await;
            assert!(matches!(
                io.scene_packs.packs.read().await.get(&entity),
                Some(PackState::Failed)
            ));
            assert!(io.scene_pack_read(&entity, "bafyjson").await.is_none());
        }));
    }

    #[test]
    fn streamed_sha_corruption_fails_pack() {
        let _env = idle_env("5");
        let (entity, mut bytes) = stream_fixture();
        let json_start = wire_start(&bytes, &entity, "bafyjson");
        bytes[json_start] ^= 0xff;
        let (base, _) = serve_scripted(pack_route(&entity), bytes, vec![full(512)]);
        let io = Arc::new(test_io());
        async_std::task::block_on(platform::compat(async {
            run_fetch(&io, &base, &entity).await;
            assert!(io.scene_pack_read(&entity, "bafyjson").await.is_none());
            assert!(matches!(
                io.scene_packs.packs.read().await.get(&entity),
                Some(PackState::Failed)
            ));
        }));
    }

    #[test]
    fn complete_stream_finalizes_zero_copy() {
        let _env = idle_env("5");
        let (entity, bytes) = stream_fixture();
        let (base, _) = serve_scripted(pack_route(&entity), bytes, vec![full(512)]);
        let io = Arc::new(test_io());
        async_std::task::block_on(platform::compat(async {
            run_fetch(&io, &base, &entity).await;
            let a = io.scene_pack_read(&entity, "bafyjson").await.unwrap();
            let b = io.scene_pack_read(&entity, "bafyjson").await.unwrap();
            match (&a, &b) {
                (PackSlice::Mem(x), PackSlice::Mem(y)) => {
                    assert!(Arc::ptr_eq(&x.data, &y.data));
                }
                _ => panic!("expected shared resident slices"),
            }
        }));
    }

    #[test]
    fn evict_and_rematerialize_restores_then_partial_keeps() {
        let _env = idle_env("5");
        let (entity, bytes) = stream_fixture();
        let cut = wire_start(&bytes, &entity, "bafymp3") + 100;
        let (base, _) = serve_scripted(
            pack_route(&entity),
            bytes.clone(),
            vec![
                full(512),
                full(512),
                Script {
                    truncate_at: Some(cut),
                    ..full(512)
                },
            ],
        );
        let io = Arc::new(test_io());
        async_std::task::block_on(platform::compat(async {
            run_fetch(&io, &base, &entity).await;
            let pack = ready_pack(&io, &entity).await;

            *pack.data.write().await = PackData::Evicted;
            let tail = io.scene_pack_read(&entity, "bafymp3").await.unwrap();
            assert_eq!(tail.as_ref().len(), 4096);
            assert_eq!(pack.available.load(Ordering::Relaxed), pack.payload());

            *pack.data.write().await = PackData::Evicted;
            assert!(io.scene_pack_read(&entity, "bafymp3").await.is_none());
            assert!(matches!(
                io.scene_packs.packs.read().await.get(&entity),
                Some(PackState::Ready(_))
            ));
            assert!(pack.available.load(Ordering::Relaxed) < pack.payload());
            assert_eq!(
                io.scene_pack_read(&entity, "bafyjson")
                    .await
                    .unwrap()
                    .as_ref(),
                b"{}"
            );
        }));
    }

    #[test]
    fn idle_zero_disables_abort() {
        let _env = idle_env("0");
        let (entity, bytes) = stream_fixture();
        let hold = wire_start(&bytes, &entity, "bafymp3");
        let gate = Gate::new();
        let (base, _) = serve_scripted(
            pack_route(&entity),
            bytes.clone(),
            vec![Script {
                hold_at: Some((hold, gate.clone())),
                ..full(512)
            }],
        );
        let io = Arc::new(test_io());
        async_std::task::block_on(platform::compat(async {
            let driver = spawn_fetch(&io, &base, &entity).await;
            let pack = ready_pack(&io, &entity).await;
            async_std::task::sleep(Duration::from_millis(2500)).await;
            assert!(
                is_streaming(&pack).await,
                "abort disabled: stream must stay open"
            );
            gate.open();
            driver.await;
            assert_eq!(pack.available.load(Ordering::Relaxed), pack.payload());
        }));
    }
}
