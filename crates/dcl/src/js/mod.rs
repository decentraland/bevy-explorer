use std::{cell::RefCell, rc::Rc, sync::Arc};

use anyhow::anyhow;
use bevy::log::debug;
use common::structs::{CameraFov, GlobalCrdtStateUpdate, TimeOfDay};
use dcl_component::{
    proto_components::sdk::components::PbPlayerIdentityData, DclReader, FromDclReader,
    SceneComponentId, SceneEntityId,
};
use ipfs::SceneJsFile;
use system_bridge::SystemApi;
use tokio::sync::{mpsc::UnboundedReceiver, Mutex};

use crate::{
    interface::{crdt_context::CrdtContext, CrdtComponentInterfaces, CrdtType},
    RendererResponse, RpcCalls, SceneElapsedTime, SceneLogLevel, SceneLogMessage,
    SceneResourceCounters, SceneResponse,
};

use super::interface::CrdtStore;

pub mod engine;
pub mod portables;
pub mod restricted_actions;
pub mod runtime;
pub mod user_identity;

pub mod adaption_layer_helper;
pub mod comms;
pub mod ethereum_controller;
pub mod events;
pub mod fetch;
pub mod player;
pub mod system_api;
pub mod testing;

#[cfg(target_arch = "wasm32")]
mod response_channel {
    // wasm randomly freezes if we use tokio channels here. no idea why.
    pub type SceneResponseSender = std::sync::mpsc::SyncSender<super::SceneResponse>;
    pub type SceneResponseReceiver = std::sync::mpsc::Receiver<super::SceneResponse>;
    pub type TryRecvError = std::sync::mpsc::TryRecvError;

    pub fn scene_response_channel() -> (super::SceneResponseSender, super::SceneResponseReceiver) {
        std::sync::mpsc::sync_channel(1000)
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod response_channel {
    // we can't use std channels here because the IPC layer wants to select on multiple tokio sources
    pub type SceneResponseSender = tokio::sync::mpsc::Sender<super::SceneResponse>;
    pub type SceneResponseReceiver = tokio::sync::mpsc::Receiver<super::SceneResponse>;
    pub type TryRecvError = tokio::sync::mpsc::error::TryRecvError;

    pub fn scene_response_channel() -> (super::SceneResponseSender, super::SceneResponseReceiver) {
        tokio::sync::mpsc::channel(1000)
    }
}

pub use response_channel::*;

// signal that the scene should exit. set cooperatively by the ops (renderer channel
// closed, contract-breach policy kills) or externally by the scene host (watchdog
// kill, heap cap) — external setters pair it with v8 terminate_execution since the
// scene may never run an op again. once set, the crdt ops go inert and the scene
// loop tears the runtime down.
#[derive(Clone, Default)]
pub struct KillFlag(pub std::sync::Arc<std::sync::atomic::AtomicBool>);

impl KillFlag {
    pub fn kill(&self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn killed(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

pub struct RendererStore(pub CrdtStore);

// Sidecar holding only the components the scene→renderer filter drops (unrecognized / custom).
// Merged into the inspector snapshot so custom components are visible as raw bytes; never
// pushed to the renderer.
#[derive(Default)]
pub struct FilteredCrdtStore(pub CrdtStore);

// Parallel CrdtContext tracking every scene entity — recognized *and* filtered (custom-only) — so
// the inspector can allocate fresh entity ids via `new_in_range` without colliding with entities
// the main entity_map never sees. Allocation only; its census is discarded.
pub struct AllocatorContext(pub CrdtContext);

pub struct SuperUserScene(pub tokio::sync::mpsc::UnboundedSender<SystemApi>);
impl std::ops::Deref for SuperUserScene {
    type Target = tokio::sync::mpsc::UnboundedSender<SystemApi>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// marker to notify that the scene/renderer interface functions were used
pub struct CommunicatedWithRenderer;

// scene-elapsed time (seconds) of the last SceneResponse::Stats flush
pub struct SceneStatsFlush(pub f32);

// Set by `crdt_send_to_renderer`, cleared at each tick boundary by the scene loop: a scene
// may send at most one CRDT batch per tick (the response channel is sized for that contract,
// and un-awaited send spam could otherwise fill it). A second send in one tick is a breach
// that shuts the scene down.
pub struct CrdtSentThisTick;

// Ingest total (SceneResourceCounters::crdt_bytes) at which the retained store could next
// reach its cap. Retained bytes grow at most 1:1 with ingest, so after a measurement finds
// `retained` bytes the store cannot breach until `cap - retained` more bytes arrive. Storing
// that projected point lets `crdt_send_to_renderer` skip the O(entries) walk in proportion to
// how far below the cap the scene sits.
pub struct CrdtStoreNextCheck(pub u64);

pub trait State {
    fn borrow<T: 'static>(&self) -> &T;
    fn try_borrow<T: 'static>(&self) -> Option<&T>;
    fn borrow_mut<T: 'static>(&mut self) -> &mut T;
    fn try_borrow_mut<T: 'static>(&mut self) -> Option<&mut T>;
    fn has<T: 'static>(&self) -> bool;
    fn put<T: 'static>(&mut self, value: T);
    fn take<T: 'static>(&mut self) -> T;
    fn try_take<T: 'static>(&mut self) -> Option<T>;
}

#[cfg(not(target_arch = "wasm32"))]
use std::ops::{Deref, DerefMut};
#[cfg(not(target_arch = "wasm32"))]
impl State for deno_core::OpState {
    fn borrow<T: 'static>(&self) -> &T {
        self.deref().borrow()
    }

    fn try_borrow<T: 'static>(&self) -> Option<&T> {
        self.deref().try_borrow()
    }

    fn borrow_mut<T: 'static>(&mut self) -> &mut T {
        self.deref_mut().borrow_mut()
    }

    fn try_borrow_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.deref_mut().try_borrow_mut()
    }

    fn has<T: 'static>(&self) -> bool {
        self.deref().has::<T>()
    }

    fn put<T: 'static>(&mut self, value: T) {
        self.deref_mut().put(value)
    }

    fn take<T: 'static>(&mut self) -> T {
        self.deref_mut().take()
    }

    fn try_take<T: 'static>(&mut self) -> Option<T> {
        self.deref_mut().try_take()
    }
}

#[allow(clippy::too_many_arguments)]
pub fn init_state(
    state: &mut impl State,
    initial_crdt_store: CrdtStore,
    scene_context: CrdtContext,
    storage_root: String,
    scene_js: SceneJsFile,
    crdt_component_interfaces: CrdtComponentInterfaces,
    thread_sx: SceneResponseSender,
    thread_rx: UnboundedReceiver<RendererResponse>,
    global_update_receiver: tokio::sync::broadcast::Receiver<GlobalCrdtStateUpdate>,
    super_user: Option<tokio::sync::mpsc::UnboundedSender<SystemApi>>,
    scene_origin: bevy::prelude::Vec3,
    kill_flag: KillFlag,
) {
    // Allocator context: a parallel CrdtContext used solely for entity allocation. It's populated
    // with every entity (recognized + filtered) on the send path, but the scene's authored entities
    // load from main.crdt — the scene receives them as initial state and never re-sends them — so
    // seed those here, otherwise new_in_range would hand back ids that already exist.
    let mut allocator = AllocatorContext(CrdtContext::new(
        scene_context.scene_id,
        scene_context.hash.clone(),
        scene_context.title.clone(),
        scene_context.testing,
        scene_context.preview,
        scene_context.is_server,
    ));
    for lww in initial_crdt_store.lww.values() {
        for entity in lww.last_write.keys() {
            allocator.0.init(*entity);
        }
    }
    for go in initial_crdt_store.go.values() {
        for entity in go.0.keys() {
            allocator.0.init(*entity);
        }
    }
    // flush the seeded entities into the live table so new_in_range avoids them; the census's
    // `born` is exactly the unique set we just seeded.
    let census = allocator.0.take_census();
    debug!(
        "allocator seeded with {} authored entities from main.crdt",
        census.born.len()
    );
    state.put(scene_context);
    state.put(allocator);
    state.put(scene_js);
    state.put(storage_root);
    state.put(crdt_component_interfaces);
    state.put(thread_sx);
    state.put(Arc::new(Mutex::new(thread_rx)));
    state.put(global_update_receiver);
    state.put(CrdtStore::default());
    state.put(RpcCalls::default());
    state.put(RendererStore(initial_crdt_store));
    state.put(FilteredCrdtStore::default());
    state.put(Vec::<SceneLogMessage>::default());
    state.put(SceneResourceCounters::default());
    state.put(SceneStatsFlush(0.0));
    state.put(SceneElapsedTime(0.0));
    state.put(TimeOfDay { time: 0. });
    state.put(CameraFov::default());
    state.put(dcl_component::SceneOrigin(scene_origin));
    state.put(kill_flag);
    if let Some(super_user) = super_user {
        state.put(SuperUserScene(super_user));
    }
}

pub const DEFAULT_SCENE_LOG_MAX_LINES: usize = 10_000;
pub const DEFAULT_SCENE_LOG_MAX_LINE_BYTES: usize = 8 * 1024;

pub fn resolve_log_budget(env: impl Fn(&str) -> Option<String>) -> (usize, usize) {
    let read = |key: &str, default: usize| {
        env(key)
            .and_then(|v| v.parse().ok())
            .filter(|v| *v > 0)
            .unwrap_or(default)
    };
    (
        read("DCL_SCENE_LOG_MAX_LINES", DEFAULT_SCENE_LOG_MAX_LINES),
        read(
            "DCL_SCENE_LOG_MAX_LINE_BYTES",
            DEFAULT_SCENE_LOG_MAX_LINE_BYTES,
        ),
    )
}

fn scene_log_budget() -> (usize, usize) {
    static BUDGET: std::sync::OnceLock<(usize, usize)> = std::sync::OnceLock::new();
    *BUDGET.get_or_init(|| resolve_log_budget(|k| std::env::var(k).ok()))
}

fn push_scene_log(state: &mut impl State, level: SceneLogLevel, message: String, timestamp: f64) {
    let (max_lines, max_line_bytes) = scene_log_budget();
    push_scene_log_bounded(state, level, message, timestamp, max_lines, max_line_bytes)
}

fn push_scene_log_bounded(
    state: &mut impl State,
    level: SceneLogLevel,
    mut message: String,
    timestamp: f64,
    max_lines: usize,
    max_line_bytes: usize,
) {
    let emitted = message.len() as u64;

    if message.len() > max_line_bytes {
        let mut cut = max_line_bytes;
        while cut > 0 && !message.is_char_boundary(cut) {
            cut -= 1;
        }
        message.truncate(cut);
        message.push_str("…[truncated]");
    }

    let counters = state.borrow_mut::<SceneResourceCounters>();
    counters.log_lines += 1;
    counters.log_bytes += emitted;

    if state.borrow::<Vec<SceneLogMessage>>().len() >= max_lines {
        state.borrow_mut::<SceneResourceCounters>().log_dropped += 1;
        return;
    }

    state
        .borrow_mut::<Vec<SceneLogMessage>>()
        .push(SceneLogMessage {
            timestamp,
            level,
            message,
        })
}

pub fn op_log(state: Rc<RefCell<impl State>>, message: String) {
    debug!("op_log {}", message);
    let time = state.borrow().borrow::<SceneElapsedTime>().0;
    let mut state = state.borrow_mut();
    push_scene_log(&mut *state, SceneLogLevel::Log, message, time as f64)
}

pub fn op_error(state: Rc<RefCell<impl State>>, message: String) {
    debug!("op_error");
    let time = state.borrow().borrow::<SceneElapsedTime>().0;
    let mut state = state.borrow_mut();
    push_scene_log(&mut *state, SceneLogLevel::SceneError, message, time as f64)
}

pub fn player_identity(state: &impl State) -> Result<PbPlayerIdentityData, anyhow::Error> {
    let renderer_store = state.borrow::<RendererStore>();
    let Some(player_identity) = renderer_store.0.get(
        SceneComponentId::PLAYER_IDENTITY_DATA,
        CrdtType::LWW_ANY,
        SceneEntityId::PLAYER,
    ) else {
        anyhow::bail!("no player identity!");
    };
    PbPlayerIdentityData::from_reader(&mut DclReader::new(player_identity))
        .map_err(|e| anyhow!(format!("{e:?}")))
}

#[cfg(test)]
mod scene_log_budget_tests {
    use super::*;
    use std::any::{Any, TypeId};
    use std::collections::HashMap;

    #[derive(Default)]
    struct TestState(HashMap<TypeId, Box<dyn Any>>);

    impl State for TestState {
        fn borrow<T: 'static>(&self) -> &T {
            self.try_borrow().expect("absent")
        }
        fn try_borrow<T: 'static>(&self) -> Option<&T> {
            self.0
                .get(&TypeId::of::<T>())
                .map(|v| v.downcast_ref().unwrap())
        }
        fn borrow_mut<T: 'static>(&mut self) -> &mut T {
            self.try_borrow_mut().expect("absent")
        }
        fn try_borrow_mut<T: 'static>(&mut self) -> Option<&mut T> {
            self.0
                .get_mut(&TypeId::of::<T>())
                .map(|v| v.downcast_mut().unwrap())
        }
        fn has<T: 'static>(&self) -> bool {
            self.0.contains_key(&TypeId::of::<T>())
        }
        fn put<T: 'static>(&mut self, value: T) {
            self.0.insert(TypeId::of::<T>(), Box::new(value));
        }
        fn take<T: 'static>(&mut self) -> T {
            self.try_take().expect("absent")
        }
        fn try_take<T: 'static>(&mut self) -> Option<T> {
            self.0
                .remove(&TypeId::of::<T>())
                .map(|v| *v.downcast().unwrap())
        }
    }

    fn state() -> TestState {
        let mut s = TestState::default();
        s.put(Vec::<SceneLogMessage>::default());
        s.put(SceneResourceCounters::default());
        s
    }

    fn log(s: &mut TestState, msg: &str, max_lines: usize, max_bytes: usize) {
        push_scene_log_bounded(
            s,
            SceneLogLevel::Log,
            msg.to_owned(),
            0.0,
            max_lines,
            max_bytes,
        )
    }

    #[test]
    fn line_cap_sheds_and_is_counted() {
        let mut s = state();
        for _ in 0..10 {
            log(&mut s, "x", 4, 1024);
        }
        let stored = s.borrow::<Vec<SceneLogMessage>>().len();
        let c = s.borrow::<SceneResourceCounters>();
        assert_eq!(stored, 4, "buffer must stop at the cap");
        assert_eq!(c.log_lines, 10, "every emitted line is still counted");
        assert_eq!(c.log_dropped, 6, "shed lines are visible as dropped");
        assert_eq!(
            c.log_lines - c.log_dropped,
            stored as u64,
            "lines minus dropped must equal what reached the renderer"
        );
    }

    #[test]
    fn long_line_is_clamped_but_charged_in_full() {
        let mut s = state();
        let huge = "a".repeat(100_000);
        log(&mut s, &huge, 10, 1024);
        let stored = &s.borrow::<Vec<SceneLogMessage>>()[0].message;
        assert!(
            stored.len() < 1200,
            "stored line must be clamped, got {}",
            stored.len()
        );
        assert!(
            stored.ends_with("…[truncated]"),
            "clamping must be visible to the reader"
        );
        assert_eq!(
            s.borrow::<SceneResourceCounters>().log_bytes,
            100_000,
            "counters must charge what the scene emitted, not what survived"
        );
    }

    #[test]
    fn tick_cost_is_bounded_by_lines_times_line_bytes() {
        let mut s = state();
        let huge = "b".repeat(50_000);
        for _ in 0..50 {
            log(&mut s, &huge, 8, 256);
        }
        let held: usize = s
            .borrow::<Vec<SceneLogMessage>>()
            .iter()
            .map(|l| l.message.len())
            .sum();
        assert!(
            held <= 8 * (256 + "…[truncated]".len()),
            "one tick held {held} bytes, over the lines x line-bytes bound"
        );
    }

    #[test]
    fn clamp_never_splits_a_utf8_char() {
        let mut s = state();
        log(&mut s, &"é".repeat(100), 10, 15);
        let stored = &s.borrow::<Vec<SceneLogMessage>>()[0].message;
        assert!(stored.is_char_boundary(stored.len()));
        assert!(stored.starts_with('é'));
    }

    #[test]
    fn budget_resolution_defaults_and_rejects_zero() {
        let none = resolve_log_budget(|_| None);
        assert_eq!(
            none,
            (
                DEFAULT_SCENE_LOG_MAX_LINES,
                DEFAULT_SCENE_LOG_MAX_LINE_BYTES
            )
        );
        let zero = resolve_log_budget(|_| Some("0".into()));
        assert_eq!(zero, none, "zero must not disable the buffer entirely");
        let junk = resolve_log_budget(|_| Some("banana".into()));
        assert_eq!(junk, none);
        let set = resolve_log_budget(|k| match k {
            "DCL_SCENE_LOG_MAX_LINES" => Some("5".into()),
            "DCL_SCENE_LOG_MAX_LINE_BYTES" => Some("64".into()),
            _ => None,
        });
        assert_eq!(set, (5, 64));
    }

    // full op-state for driving `crdt_send_to_renderer`.
    fn crdt_state() -> TestState {
        use crate::interface::crdt_context::CrdtContext;
        let ctx = || {
            CrdtContext::new(
                crate::SceneId::DUMMY,
                Default::default(),
                Default::default(),
                false,
                false,
                false,
            )
        };
        let mut s = state();
        s.put(crate::SceneElapsedTime(0.0));
        s.put(ctx());
        s.put(crate::CrdtStore::default());
        s.put(FilteredCrdtStore::default());
        s.put(AllocatorContext(ctx()));
        s.put(crate::CrdtComponentInterfaces::default());
        s.put(crate::RpcCalls::default());
        s.put(SceneStatsFlush(0.0));
        s.put(KillFlag::default());
        s
    }

    // An oversized batch is refused at ingress: the scene is reported as errored and flagged
    // for shutdown, and no Ok frame is built — so the offending scene dies without a giant
    // frame ever reaching the shared connection.
    #[test]
    fn oversized_crdt_batch_terminates_the_scene() {
        let (sx, mut rx) = scene_response_channel();
        let mut s = crdt_state();
        s.put(sx);
        let state = std::rc::Rc::new(std::cell::RefCell::new(s));

        let oversized = vec![0u8; super::engine::MAX_CRDT_BATCH_BYTES + 1];
        super::engine::crdt_send_to_renderer(state.clone(), &oversized);

        match rx.try_recv() {
            Ok(crate::SceneResponse::Error(id, msg)) => {
                assert_eq!(id, crate::SceneId::DUMMY);
                assert!(msg.contains("CRDT batch"), "unexpected message: {msg}");
            }
            other => panic!("expected Error, got {other:?}"),
        }
        assert!(
            state.borrow().borrow::<KillFlag>().killed(),
            "oversized batch must flag the scene for shutdown"
        );
        assert!(
            rx.try_recv().is_err(),
            "no Ok frame must be built for a rejected batch"
        );

        // the follow-up recv must not park on the renderer channel (the renderer marks the
        // scene broken and never responds): it returns an empty batch immediately — the test
        // state holds no renderer channel at all, so reaching for it would panic — letting
        // the tick unwind to the scene loop's kill-flag check.
        let batch = futures_lite::future::block_on(super::engine::op_crdt_recv_from_renderer(
            state.clone(),
        ));
        assert!(
            batch.is_empty(),
            "a shutting-down scene must receive an empty batch"
        );

        // and further sends are inert: a scene that ignores the unwind can neither grow the
        // store nor push frames at the broken scene.
        super::engine::crdt_send_to_renderer(state, &[]);
        assert!(
            rx.try_recv().is_err(),
            "a shutting-down scene must not emit further frames"
        );
    }

    // Engine-initiated deletes reach the stores only via the census (the scene never sends a
    // DeleteEntity message for them) — the sidecar must be reaped there too, so custom
    // components aren't retained for dead entities.
    #[test]
    fn census_deaths_reap_the_filtered_store() {
        use crate::interface::{crdt_context::CrdtContext, CrdtType};
        use dcl_component::{DclReader, SceneComponentId, SceneCrdtTimestamp, SceneEntityId};

        let (sx, _rx) = scene_response_channel();
        let mut s = crdt_state();
        s.put(sx);

        let entity = SceneEntityId {
            id: 600,
            generation: 0,
        };
        s.borrow_mut::<FilteredCrdtStore>().0.try_update(
            SceneComponentId(9999),
            CrdtType::LWW_ANY,
            entity,
            SceneCrdtTimestamp(1),
            Some(&mut DclReader::new(&[0u8; 16])),
        );
        // mimic an engine-initiated delete: killed directly in the context, no DeleteEntity
        // stream message
        {
            let ctx = s.borrow_mut::<CrdtContext>();
            ctx.init(entity);
            ctx.take_census();
            ctx.kill(entity);
        }
        let state = std::rc::Rc::new(std::cell::RefCell::new(s));

        super::engine::crdt_send_to_renderer(state.clone(), &[]);

        assert_eq!(
            state
                .borrow()
                .borrow::<FilteredCrdtStore>()
                .0
                .retained_data_bytes(),
            0,
            "dead entities' custom components must not be retained"
        );
    }

    // A scene may send at most one CRDT batch per tick: a second send with no intervening
    // tick boundary is a contract breach that terminates the scene — un-awaited send spam
    // must not stack frames onto the bounded response channel.
    #[test]
    fn second_send_in_one_tick_terminates_the_scene() {
        let (sx, mut rx) = scene_response_channel();
        let mut s = crdt_state();
        s.put(sx);
        let state = std::rc::Rc::new(std::cell::RefCell::new(s));

        super::engine::crdt_send_to_renderer(state.clone(), &[]);
        assert!(matches!(rx.try_recv(), Ok(crate::SceneResponse::Ok(..))));

        super::engine::crdt_send_to_renderer(state.clone(), &[]);
        match rx.try_recv() {
            Ok(crate::SceneResponse::Error(id, msg)) => {
                assert_eq!(id, crate::SceneId::DUMMY);
                assert!(
                    msg.contains("more than one CRDT batch"),
                    "unexpected message: {msg}"
                );
            }
            other => panic!("expected Error, got {other:?}"),
        }
        assert!(
            state.borrow().borrow::<KillFlag>().killed(),
            "a second send in one tick must flag the scene for shutdown"
        );
    }

    // The scene loop clears the allowance at each tick boundary, so one send per tick flows.
    #[test]
    fn one_send_per_tick_boundary_is_fine() {
        let (sx, mut rx) = scene_response_channel();
        let mut s = crdt_state();
        s.put(sx);
        let state = std::rc::Rc::new(std::cell::RefCell::new(s));

        for _ in 0..3 {
            super::engine::crdt_send_to_renderer(state.clone(), &[]);
            assert!(matches!(rx.try_recv(), Ok(crate::SceneResponse::Ok(..))));
            // what the scene loop does at each tick boundary
            state.borrow_mut().try_take::<CrdtSentThisTick>();
        }
        assert!(!state.borrow().borrow::<KillFlag>().killed());
    }

    // A normal batch flows through untouched: an Ok frame is produced and the scene is not
    // flagged for shutdown.
    #[test]
    fn normal_crdt_batch_is_forwarded_and_scene_survives() {
        let (sx, mut rx) = scene_response_channel();
        let mut s = crdt_state();
        s.put(sx);
        let state = std::rc::Rc::new(std::cell::RefCell::new(s));

        super::engine::crdt_send_to_renderer(state.clone(), &[]);

        match rx.try_recv() {
            Ok(crate::SceneResponse::Ok(id, ..)) => assert_eq!(id, crate::SceneId::DUMMY),
            other => panic!("expected Ok, got {other:?}"),
        }
        assert!(
            !state.borrow().borrow::<KillFlag>().killed(),
            "a normal batch must not shut the scene down"
        );
    }
}
