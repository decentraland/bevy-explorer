use std::sync::{atomic::Ordering, Arc};
use std::time::Duration;

use super::{
    test::{build_pack, run_fetch, test_io},
    testkit::{
        full, idle_env, is_streaming, pack_route, ready_pack, serve_scripted, spawn_fetch,
        stream_fixture, wire_start, Gate, Script,
    },
    PackData, PackState, ScenePack,
};

#[test]
fn dribbles_prime_chunks_across_header_boundaries() {
    let _env = idle_env("5");
    let (entity, bytes) = stream_fixture();
    let (base, _) = serve_scripted(pack_route(&entity), bytes.clone(), vec![full(7)]);
    let io = Arc::new(test_io());
    async_std::task::block_on(platform::compat(async {
        run_fetch(&io, &base, &entity).await;
        let pack = ready_pack(&io, &entity).await;
        assert!(matches!(&*pack.data.read().await, PackData::Resident(_)));
        assert_eq!(pack.available.load(Ordering::Relaxed), pack.payload());
        for (cid, len) in [("bafyjson", 2), ("bafyglb", 100), ("bafymp3", 4096)] {
            let served = io.scene_pack_read(&entity, cid).await.unwrap();
            assert_eq!(served.as_ref().len(), len);
        }
    }));
}

#[test]
fn dribbles_1k_chunks_serving_prefix_before_large_tail() {
    let _env = idle_env("5");
    let mp3: Vec<u8> = (0..65536usize).map(|i| (i * 7 % 256) as u8).collect();
    let files: &[(&str, &str, &[u8])] = &[
        ("scene.json", "bafyjson", b"{}"),
        ("models/big.glb", "bafyglb", b"glb bytes for the dribble"),
        ("sounds/huge.mp3", "bafymp3", &mp3),
    ];
    let entity = "dribble-entity".to_owned();
    let bytes = build_pack(&entity, files);
    let hold = wire_start(&bytes, &entity, "bafymp3");
    let gate = Gate::new();
    let (base, _) = serve_scripted(
        pack_route(&entity),
        bytes.clone(),
        vec![Script {
            hold_at: Some((hold, gate.clone())),
            ..full(1024)
        }],
    );
    let io = Arc::new(test_io());
    async_std::task::block_on(platform::compat(async {
        let driver = spawn_fetch(&io, &base, &entity).await;
        let served = io.scene_pack_read(&entity, "bafyjson").await.unwrap();
        assert_eq!(served.as_ref(), b"{}");
        let pack = ready_pack(&io, &entity).await;
        assert!(is_streaming(&pack).await, "tail held: must still stream");
        gate.open();
        driver.await;
        assert_eq!(pack.available.load(Ordering::Relaxed), pack.payload());
        let tail = io.scene_pack_read(&entity, "bafymp3").await.unwrap();
        assert_eq!(tail.as_ref(), &mp3[..]);
    }));
}

#[test]
fn no_abort_during_index_fetch() {
    let _env = idle_env("0.2");
    let (entity, bytes) = stream_fixture();
    let gate = Gate::new();
    let (base, _) = serve_scripted(
        pack_route(&entity),
        bytes.clone(),
        vec![Script {
            hold_at: Some((20, gate.clone())),
            ..full(512)
        }],
    );
    let io = Arc::new(test_io());
    async_std::task::block_on(platform::compat(async {
        let driver = spawn_fetch(&io, &base, &entity).await;
        async_std::task::sleep(Duration::from_millis(1000)).await;
        assert!(
            matches!(
                io.scene_packs.packs.read().await.get(&entity),
                Some(PackState::Fetching(_))
            ),
            "idle abort must not fire before the index is parsed"
        );
        gate.open();
        assert!(
            platform::with_timeout(Duration::from_secs(15), driver)
                .await
                .is_ok(),
            "driver never finished"
        );
        let pack = ready_pack(&io, &entity).await;
        assert!(pack.available.load(Ordering::Relaxed) >= pack.prefix_end);
        let served = io.scene_pack_read(&entity, "bafyjson").await.unwrap();
        assert_eq!(served.as_ref(), b"{}");
    }));
}

#[test]
fn abort_mid_entry_keeps_prefix_and_misses_partial_tail() {
    let _env = idle_env("0.2");
    let (entity, bytes) = stream_fixture();
    let hold = wire_start(&bytes, &entity, "bafymp3") + 100;
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
        let mp3 = {
            let idx = pack.entry_by_cid("bafymp3").unwrap();
            (pack.entries[idx].off, pack.entries[idx].len)
        };
        let available = pack.available.load(Ordering::Relaxed);
        assert!(
            available > mp3.0 && available < mp3.0 + mp3.1,
            "abort must have cut mid-entry: {available} vs {mp3:?}"
        );
        let start = web_time::Instant::now();
        assert!(io.scene_pack_read(&entity, "bafymp3").await.is_none());
        assert!(
            start.elapsed() < Duration::from_millis(500),
            "partial-entry miss must not wait"
        );
        assert_eq!(
            io.scene_pack_read(&entity, "bafyjson")
                .await
                .unwrap()
                .as_ref(),
            b"{}"
        );
        assert_eq!(
            io.scene_pack_read(&entity, "bafyglb")
                .await
                .unwrap()
                .as_ref()
                .len(),
            100
        );
        gate.open();
        for _ in 0..100 {
            if broken.load(Ordering::SeqCst) {
                break;
            }
            async_std::task::sleep(Duration::from_millis(50)).await;
        }
        assert!(broken.load(Ordering::SeqCst), "server never saw the abort");
    }));
}

#[test]
fn streamed_garbage_header_fails_pack() {
    let _env = idle_env("5");
    let entity = "garbage-entity".to_owned();
    let (base, _) = serve_scripted(pack_route(&entity), vec![0xAB; 64], vec![full(16)]);
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
fn streamed_corrupt_index_json_fails_pack() {
    let _env = idle_env("5");
    let (entity, mut bytes) = stream_fixture();
    bytes[20] ^= 0xff;
    let (base, _) = serve_scripted(pack_route(&entity), bytes, vec![full(64)]);
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
fn streamed_surplus_bytes_fail_pack() {
    let _env = idle_env("5");
    let (entity, bytes) = stream_fixture();
    let mut surplus = bytes.clone();
    surplus.extend_from_slice(&[0xEE; 64]);
    let (base, _) = serve_scripted(
        pack_route(&entity),
        bytes,
        vec![Script {
            body: Some(surplus),
            ..full(512)
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
fn double_read_of_pending_entry_both_resolve() {
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
        let readers: Vec<_> = (0..2)
            .map(|_| {
                let io = io.clone();
                let entity = entity.clone();
                async_std::task::spawn(platform::compat(async move {
                    io.scene_pack_read(&entity, "bafymp3").await
                }))
            })
            .collect();
        async_std::task::sleep(Duration::from_millis(500)).await;
        let pack = ready_pack(&io, &entity).await;
        assert!(is_streaming(&pack).await);
        assert_eq!(
            pack.waiting.load(Ordering::SeqCst),
            2,
            "both reads must be registered as pending"
        );
        gate.open();
        for reader in readers {
            let tail = platform::with_timeout(Duration::from_secs(15), reader)
                .await
                .ok()
                .expect("pending read deadlocked")
                .unwrap();
            assert_eq!(tail.as_ref().len(), 4096);
        }
        driver.await;
        assert_eq!(pack.waiting.load(Ordering::SeqCst), 0);
    }));
}

#[test]
fn evicted_mid_stream_pending_readers_recover_via_rematerialize() {
    let _env = idle_env("5");
    let (entity, bytes) = stream_fixture();
    let hold = wire_start(&bytes, &entity, "bafymp3");
    let gate = Gate::new();
    let (base, _) = serve_scripted(
        pack_route(&entity),
        bytes.clone(),
        vec![
            Script {
                hold_at: Some((hold, gate.clone())),
                ..full(512)
            },
            full(512),
        ],
    );
    let io = Arc::new(test_io());
    async_std::task::block_on(platform::compat(async {
        let driver = spawn_fetch(&io, &base, &entity).await;
        let pack = ready_pack(&io, &entity).await;
        let spawn_reader = |io: &Arc<crate::IpfsIo>, entity: &str| {
            let io = io.clone();
            let entity = entity.to_owned();
            async_std::task::spawn(platform::compat(async move {
                io.scene_pack_read(&entity, "bafymp3").await
            }))
        };
        let waiter = spawn_reader(&io, &entity);
        async_std::task::sleep(Duration::from_millis(300)).await;
        *pack.data.write().await = PackData::Evicted;
        let late = spawn_reader(&io, &entity);
        async_std::task::sleep(Duration::from_millis(200)).await;
        gate.open();
        for reader in [waiter, late] {
            let tail = platform::with_timeout(Duration::from_secs(20), reader)
                .await
                .ok()
                .expect("reader deadlocked across eviction")
                .unwrap();
            assert_eq!(tail.as_ref().len(), 4096);
        }
        platform::with_timeout(Duration::from_secs(15), driver)
            .await
            .ok()
            .expect("stale driver never exited");
        assert!(matches!(&*pack.data.read().await, PackData::Resident(_)));
        assert_eq!(pack.available.load(Ordering::Relaxed), pack.payload());
        assert_eq!(pack.waiting.load(Ordering::SeqCst), 0);
    }));
}

#[test]
fn stale_driver_error_leaves_new_fetch_alone() {
    let _env = idle_env("5");
    let (entity, bytes) = stream_fixture();
    let gate = Gate::new();
    let (base, _) = serve_scripted(
        pack_route(&entity),
        bytes.clone(),
        vec![Script {
            hold_at: Some((20, gate.clone())),
            truncate_at: Some(21),
            ..full(512)
        }],
    );
    let io = Arc::new(test_io());
    async_std::task::block_on(platform::compat(async {
        let driver = spawn_fetch(&io, &base, &entity).await;
        async_std::task::sleep(Duration::from_millis(300)).await;
        let (_tx2, rx2) = tokio::sync::watch::channel(());
        io.scene_packs
            .packs
            .write()
            .await
            .insert(entity.clone(), PackState::Fetching(rx2.clone()));
        gate.open();
        platform::with_timeout(Duration::from_secs(15), driver)
            .await
            .ok()
            .expect("stale driver never exited");
        match io.scene_packs.packs.read().await.get(&entity) {
            Some(PackState::Fetching(rx)) => {
                assert!(rx.same_channel(&rx2), "slot swapped to a foreign channel")
            }
            other => panic!(
                "stale driver clobbered the fresh fetch: {:?}",
                other.map(|s| match s {
                    PackState::Fetching(_) => "fetching",
                    PackState::Ready(_) => "ready",
                    PackState::Failed => "failed",
                })
            ),
        }
    }));
}

#[test]
fn stale_driver_success_leaves_new_fetch_alone() {
    let _env = idle_env("5");
    let (entity, bytes) = stream_fixture();
    let gate = Gate::new();
    let (base, _) = serve_scripted(
        pack_route(&entity),
        bytes.clone(),
        vec![Script {
            hold_at: Some((20, gate.clone())),
            ..full(512)
        }],
    );
    let io = Arc::new(test_io());
    async_std::task::block_on(platform::compat(async {
        let driver = spawn_fetch(&io, &base, &entity).await;
        async_std::task::sleep(Duration::from_millis(300)).await;
        let (_tx2, rx2) = tokio::sync::watch::channel(());
        io.scene_packs
            .packs
            .write()
            .await
            .insert(entity.clone(), PackState::Fetching(rx2.clone()));
        gate.open();
        platform::with_timeout(Duration::from_secs(15), driver)
            .await
            .ok()
            .expect("stale driver never exited");
        assert!(
            matches!(
                io.scene_packs.packs.read().await.get(&entity),
                Some(PackState::Fetching(rx)) if rx.same_channel(&rx2)
            ),
            "stale driver must not flip a slot it does not own"
        );
    }));
}

#[test]
fn zero_length_entry_serves_during_and_after_stream() {
    let _env = idle_env("5");
    let glb: Vec<u8> = (0..64u8).collect();
    let (entity, bytes, index_end) = (1..17)
        .find_map(|i| {
            let entity = format!("zlen-{}", "x".repeat(i));
            let files: &[(&str, &str, &[u8])] = &[
                ("empty.bin", "bafyempty", b""),
                ("models/pad.glb", "bafyglb", &glb),
            ];
            let bytes = build_pack(&entity, files);
            let end = ScenePack::index_end(&bytes).unwrap().unwrap();
            (end % 16 != 0).then_some((entity, bytes, end))
        })
        .unwrap();
    let gate = Gate::new();
    let (base, _) = serve_scripted(
        pack_route(&entity),
        bytes.clone(),
        vec![Script {
            hold_at: Some((index_end, gate.clone())),
            ..full(512)
        }],
    );
    let io = Arc::new(test_io());
    async_std::task::block_on(platform::compat(async {
        let driver = spawn_fetch(&io, &base, &entity).await;
        let pack = ready_pack(&io, &entity).await;
        assert!(
            pack.payload_base > index_end as u64,
            "fixture must leave a padding gap after the index"
        );
        assert!(is_streaming(&pack).await);
        let empty = platform::with_timeout(
            Duration::from_secs(2),
            io.scene_pack_read(&entity, "bafyempty"),
        )
        .await
        .ok()
        .expect("zero-length read must not wait")
        .unwrap();
        assert_eq!(empty.as_ref().len(), 0);
        gate.open();
        driver.await;
        let served = io.scene_pack_read(&entity, "bafyglb").await.unwrap();
        assert_eq!(served.as_ref(), &glb[..]);
        let empty = io.scene_pack_read(&entity, "bafyempty").await.unwrap();
        assert_eq!(empty.as_ref().len(), 0);
    }));
}

#[test]
fn corrupt_rematerialization_is_reverified_and_failed() {
    let _env = idle_env("5");
    let (entity, bytes) = stream_fixture();
    let mut corrupt = bytes.clone();
    let mp3_wire = wire_start(&bytes, &entity, "bafymp3");
    corrupt[mp3_wire] ^= 0xff;
    let (base, _) = serve_scripted(
        pack_route(&entity),
        bytes.clone(),
        vec![
            full(512),
            Script {
                body: Some(corrupt),
                ..full(512)
            },
        ],
    );
    let io = Arc::new(test_io());
    async_std::task::block_on(platform::compat(async {
        run_fetch(&io, &base, &entity).await;
        let pack = ready_pack(&io, &entity).await;
        let served = io.scene_pack_read(&entity, "bafymp3").await.unwrap();
        assert_eq!(served.as_ref().len(), 4096);
        *pack.data.write().await = PackData::Evicted;
        assert!(
            io.scene_pack_read(&entity, "bafymp3").await.is_none(),
            "refetched corruption must not ride an earlier verification"
        );
        assert!(matches!(
            io.scene_packs.packs.read().await.get(&entity),
            Some(PackState::Failed)
        ));
    }));
}
