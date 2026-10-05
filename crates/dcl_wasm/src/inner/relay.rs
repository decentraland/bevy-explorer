// The engine half of a sandbox worker's scene relay (A). Instantiated WITHOUT the
// wasm-bindgen glue (sandbox_worker.js `relayStart`), so every export takes and returns
// numbers only. It holds the scene's engine channel ends and relays every message to and
// from the scene runtime (B, crates/dcl_scene_wasm), which runs the scene in its own
// memory. A drives both directions; the copy imports are the only things in the worker
// that can reach engine memory, and only A imports them.
//
// Nothing here may panic or unwind: A runs on engine memory, and a trap part way through
// would leave its state (and any lock it held) behind. The worker catches anything thrown
// by B before it reaches A, and calls `relay_fail` instead.

use std::{cell::RefCell, collections::VecDeque};

use serde::Deserialize;

use common::{
    rpc::{rmp_encode, IpcMessage, ResponseContext, RpcCall, RpcResultSender, ENGINE_IPC_CONTEXT},
    structs::GlobalCrdtStateUpdate,
};
use dcl::{
    js::{
        comms::{MAX_COMMS_MESSAGE_BYTES, MAX_SEND_MESSAGES_PER_TICK},
        KillFlag, SceneResponseSender,
    },
    RendererResponse, SceneId, SceneResponse, MAX_RPC_CALLS_PER_TICK,
};
use system_bridge::SystemApi;
use tokio::sync::{broadcast, mpsc};
use wasm_bindgen::prelude::*;

use super::{
    relay_proto::{RelayInit, ToEngine, ToScene},
    SceneInitializationData, SCENE_QUEUE,
};

// swapped for the copy module's and B's exports in a relay worker (see relay_imports.js)
#[wasm_bindgen(module = "/src/relay_imports.js")]
extern "C" {
    fn relay_copy_to_b(src: u32, dst: u32, len: u32) -> u32;
    fn relay_copy_from_b(dst: u32, src: u32, len: u32) -> u32;
    fn relay_b_alloc(len: u32) -> u32;
    fn relay_b_deliver(ptr: u32, len: u32) -> u32;
    fn relay_b_pump() -> u32;
    fn relay_b_next_len() -> u32;
    fn relay_b_next_ptr() -> u32;
    fn relay_b_pop();
}

// Everything from the scene runtime is untrusted: scene code can reach its memory. These bound
// what it can make the engine side do; the sizes follow the native sidecar's per-scene limits.
// A scene reply is at most a CRDT batch (capped at 64 MiB by dcl::js::engine) plus its logs and
// rpc calls.
const MAX_MESSAGE_BYTES: u32 = 80 * 1024 * 1024;
// decoding recurses on this thread's stack, which lives in engine memory with no guard page
const MAX_DECODE_DEPTH: usize = 32;
// per pump, so one scene can't hold the loop; the rest wait in the scene runtime's queue
const MAX_MESSAGES_PER_PUMP: usize = 64;
// rpc channels the scene may hold open at once (pending results plus live subscriptions)
const MAX_IPC_CHANNELS: usize = 4096;
// a scene's error message, as forwarded
const MAX_ERROR_BYTES: usize = 8 * 1024;

struct Relay {
    scene_id: SceneId,
    thread_rx: mpsc::UnboundedReceiver<RendererResponse>,
    // attaches the scene's id to everything forwarded; the scene never supplies it
    renderer_sender: SceneResponseSender,
    global_rx: broadcast::Receiver<GlobalCrdtStateUpdate>,
    // engine-side ends of the rpc senders decoded from the scene (ENGINE_IPC_CONTEXT router)
    ipc_rx: mpsc::UnboundedReceiver<(u64, IpcMessage)>,
    kill_flag: KillFlag,
    // the engine's SystemApi sender, held only for the super-user scene
    super_user: Option<mpsc::UnboundedSender<SystemApi>>,
    // decoding a remote rpc sender spawns its close watcher
    runtime: tokio::runtime::Runtime,
    // scene ticks the engine is ready for: its first, then one per engine tick delivered
    tick_credits: u32,
    // a scene tick that arrived before the engine's; nothing more is pulled until it goes
    held_tick: Option<SceneResponse>,
    // the reply channel of an immediate call the engine hasn't answered; nothing more is pulled
    // until it closes
    awaiting_reply: Option<u64>,
    // engine requests the scene hasn't answered: snapshots, and the slot count of each
    // allocation in order; a reply without one ends the scene
    snapshot_requests: u32,
    allocation_requests: VecDeque<usize>,
    // only a test scene may ask for snapshot comparisons
    testing: bool,
    // set when the scene broke a relay limit; the worker then tears the scene down
    ended: bool,
}

// what an engine message lets the scene send back
enum Reply {
    Tick,
    Snapshot,
    Allocation(usize),
}

impl Relay {
    // end the scene as a broken one, the way a native scene that hits a sidecar limit ends
    fn end(&mut self, reason: &str) {
        count(|s| s.rejected += 1.0);
        if !self.ended {
            self.ended = true;
            let _ = self.renderer_sender.try_send(SceneResponse::Error(format!(
                "scene ended by the relay: {reason}"
            )));
        }
    }

    fn forward(&mut self, response: SceneResponse) -> bool {
        let sent = self.renderer_sender.try_send(response).is_ok();
        if !sent {
            count(|s| s.failures += 1.0);
        }
        sent
    }

    // forward a scene tick if the engine is ready for it, else hold it. false if held.
    fn forward_tick(&mut self, response: SceneResponse) -> bool {
        if self.tick_credits == 0 {
            self.held_tick = Some(response);
            return false;
        }
        self.tick_credits -= 1;
        self.forward(response);
        true
    }
}

#[derive(Default)]
struct Stats {
    to_scene: f64,
    to_engine: f64,
    bytes: f64,
    failures: f64,
    rejected: f64,
    system_api: f64,
}

thread_local! {
    static RELAY: RefCell<Option<Relay>> = const { RefCell::new(None) };
    static STATS: RefCell<Stats> = RefCell::new(Stats::default());
}

fn count(f: impl FnOnce(&mut Stats)) {
    let _ = STATS.try_with(|stats| stats.try_borrow_mut().map(|mut stats| f(&mut stats)));
}

fn with_relay<R>(f: impl FnOnce(&mut Relay) -> R) -> Option<R> {
    RELAY
        .try_with(|relay| {
            relay
                .try_borrow_mut()
                .ok()
                .and_then(|mut relay| relay.as_mut().map(f))
        })
        .ok()
        .flatten()
}

fn ipc_channel_open(id: u64) -> bool {
    ENGINE_IPC_CONTEXT
        .try_with(|ctx| {
            ctx.try_borrow().ok().and_then(|ctx| {
                ctx.as_ref()
                    .map(|ctx| ctx.ipc_channel_registry.contains_key(&id))
            })
        })
        .ok()
        .flatten()
        .unwrap_or(false)
}

fn with_ipc_context(f: impl FnOnce(&mut ResponseContext)) {
    let _ = ENGINE_IPC_CONTEXT.try_with(|ctx| {
        if let Ok(mut ctx) = ctx.try_borrow_mut() {
            if let Some(ctx) = ctx.as_mut() {
                f(ctx);
            }
        }
    });
}

/// Err if the scene runtime handed back a buffer outside its own memory.
fn to_b(message: &ToScene) -> Result<(), &'static str> {
    let Ok(bytes) = rmp_encode(message) else {
        count(|s| s.failures += 1.0);
        return Ok(());
    };
    let len = bytes.len() as u32;
    let dst = relay_b_alloc(len);
    if relay_copy_to_b(bytes.as_ptr() as u32, dst, len) == 0 {
        return Err("allocation outside its memory");
    }
    count(|s| {
        s.to_scene += 1.0;
        s.bytes += len as f64;
    });
    if relay_b_deliver(dst, len) == 0 {
        count(|s| s.failures += 1.0);
    }
    Ok(())
}

/// The scene runtime's next outbound message, if any. Err if it breaks a relay limit.
fn from_b() -> Result<Option<Vec<u8>>, &'static str> {
    let len = relay_b_next_len();
    if len == 0 {
        return Ok(None);
    }
    if len > MAX_MESSAGE_BYTES {
        return Err("message too large");
    }
    let src = relay_b_next_ptr();
    let mut bytes = Vec::new();
    if bytes.try_reserve_exact(len as usize).is_err() {
        return Err("out of memory");
    }
    bytes.resize(len as usize, 0);
    if relay_copy_from_b(bytes.as_mut_ptr() as u32, src, len) == 0 {
        return Err("message outside its memory");
    }
    relay_b_pop();
    count(|s| {
        s.to_engine += 1.0;
        s.bytes += len as f64;
    });
    Ok(Some(bytes))
}

// the scene runtime's per-tick rpc and message bus limits, which the engine relies on, checked
// again here
fn tick_limit_broken(calls: &[RpcCall]) -> Option<&'static str> {
    if calls.len() > MAX_RPC_CALLS_PER_TICK {
        return Some("too many rpc calls");
    }
    let mut sends = 0;
    for call in calls {
        if let RpcCall::SendMessageBus { data, .. } = call {
            sends += 1;
            // the message plus its one-byte type tag
            if data.len() > MAX_COMMS_MESSAGE_BYTES + 1 {
                return Some("message bus send too large");
            }
        }
    }
    if sends > MAX_SEND_MESSAGES_PER_TICK {
        return Some("too many message bus sends");
    }
    None
}

fn decode(bytes: &[u8]) -> Result<ToEngine, rmp_serde::decode::Error> {
    let mut de = rmp_serde::Deserializer::from_read_ref(bytes);
    de.set_max_depth(MAX_DECODE_DEPTH);
    ToEngine::deserialize(&mut de)
}

/// Takes the next relay scene off the queue and hands its init data to B. 1 on success.
#[wasm_bindgen]
pub fn relay_init() -> u32 {
    let Some(data) = SCENE_QUEUE
        .get()
        .and_then(|queue| queue.try_lock().ok().and_then(|mut queue| queue.pop()))
    else {
        return 0;
    };
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread().build() else {
        return 0;
    };
    let SceneInitializationData {
        initial_crdt_store,
        scene_context,
        thread_rx,
        scene_js,
        crdt_component_interfaces,
        renderer_sender,
        global_update_receiver,
        storage_root,
        scene_origin,
        super_user,
        kill_flag,
        ipc_close,
        ..
    } = data;
    let is_super = super_user.is_some();

    // per-relay (per-thread) registry: ids the scene picks can only collide with its own
    let (ipc_sx, ipc_rx) = mpsc::unbounded_channel();
    let context = ResponseContext {
        ipc_channel_registry: Default::default(),
        ipc_router: ipc_sx,
        max_channels: Some(MAX_IPC_CHANNELS),
        parent_token: Some(ipc_close),
    };
    if ENGINE_IPC_CONTEXT
        .try_with(|ctx| ctx.try_borrow_mut().map(|mut ctx| *ctx = Some(context)))
        .ok()
        .and_then(Result::ok)
        .is_none()
    {
        return 0;
    }

    let relay = Relay {
        scene_id: scene_context.scene_id,
        testing: scene_context.testing,
        thread_rx,
        renderer_sender,
        global_rx: global_update_receiver,
        ipc_rx,
        kill_flag,
        super_user,
        runtime,
        tick_credits: 1,
        held_tick: None,
        awaiting_reply: None,
        snapshot_requests: 0,
        allocation_requests: VecDeque::new(),
        ended: false,
    };
    let sent = to_b(&ToScene::Init(Box::new(RelayInit {
        initial_crdt_store,
        scene_context,
        scene_js: scene_js.0.to_string(),
        crdt_component_interfaces,
        storage_root,
        scene_origin,
        is_super,
    })));
    let installed = RELAY
        .try_with(|slot| slot.try_borrow_mut().map(|mut slot| *slot = Some(relay)))
        .ok()
        .and_then(Result::ok)
        .is_some();
    if !installed {
        return 0;
    }
    if let Err(reason) = sent {
        with_relay(|relay| relay.end(reason));
    }
    1
}

#[wasm_bindgen]
pub fn relay_scene_id() -> u64 {
    with_relay(|relay| relay.scene_id.0.to_bits()).unwrap_or(0)
}

#[wasm_bindgen]
pub fn relay_killed() -> u32 {
    with_relay(|relay| relay.ended || relay.kill_flag.killed()).unwrap_or(true) as u32
}

/// The scene runtime threw under a relay call (the worker caught it before it reached A).
#[wasm_bindgen]
pub fn relay_fail() {
    with_relay(|relay| relay.end("scene runtime failed"));
}

/// Poll both directions once, without blocking. Returns the number of messages moved.
#[wasm_bindgen]
pub fn relay_pump() -> u32 {
    with_relay(|relay| {
        if relay.ended {
            return 0;
        }
        let _runtime = relay.runtime.enter();
        // run the close watchers of senders decoded so far
        relay.runtime.block_on(tokio::task::yield_now());
        let mut moved = 0;

        // engine -> scene
        let mut sent = Ok(());
        while let Ok(response) = relay.thread_rx.try_recv() {
            moved += 1;
            let reply = match &response {
                RendererResponse::Ok(..) => Reply::Tick,
                RendererResponse::GetCrdtSnapshot => Reply::Snapshot,
                RendererResponse::AllocateEntity {
                    count,
                    explicit_ids,
                    ..
                } => Reply::Allocation(explicit_ids.as_ref().map_or(*count, Vec::len)),
            };
            let delivered = to_b(&ToScene::Update(response));
            if delivered.is_ok() {
                match reply {
                    Reply::Tick => relay.tick_credits = relay.tick_credits.saturating_add(1),
                    Reply::Snapshot => {
                        relay.snapshot_requests = relay.snapshot_requests.saturating_add(1)
                    }
                    Reply::Allocation(slots) => relay.allocation_requests.push_back(slots),
                }
            }
            sent = sent.and(delivered);
        }
        loop {
            match relay.global_rx.try_recv() {
                Ok(update) => {
                    moved += 1;
                    sent = sent.and(to_b(&ToScene::Global(update)));
                }
                Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
        while let Ok((id, message)) = relay.ipc_rx.try_recv() {
            moved += 1;
            // the scene side doesn't echo our close back, so release the entry here
            if matches!(message, IpcMessage::Closed) {
                with_ipc_context(|ctx| {
                    ctx.ipc_channel_registry.remove(&id);
                });
            }
            sent = sent.and(to_b(&ToScene::Ipc(id, message)));
        }
        if let Err(reason) = sent {
            relay.end(reason);
            return moved;
        }

        // scene -> engine, pulled only as fast as the engine ticks the scene
        if let Some(held) = relay.held_tick.take() {
            if !relay.forward_tick(held) {
                return moved;
            }
        }
        if let Some(id) = relay.awaiting_reply {
            if ipc_channel_open(id) {
                return moved;
            }
            relay.awaiting_reply = None;
        }
        relay_b_pump();
        for _ in 0..MAX_MESSAGES_PER_PUMP {
            let bytes = match from_b() {
                Ok(Some(bytes)) => bytes,
                Ok(None) => break,
                Err(reason) => {
                    relay.end(reason);
                    break;
                }
            };
            moved += 1;
            match decode(&bytes) {
                Ok(ToEngine::Response(response @ SceneResponse::Ok(..))) => {
                    let SceneResponse::Ok(.., calls) = &response else {
                        continue;
                    };
                    if let Some(reason) = tick_limit_broken(calls) {
                        relay.end(reason);
                        break;
                    }
                    if !relay.forward_tick(response) {
                        break;
                    }
                }
                // immediate calls go one at a time: nothing more is pulled until the engine
                // answers (or drops) this one. readFile is the only one.
                Ok(ToEngine::Response(
                    response @ SceneResponse::ImmediateRpcCall(RpcCall::ReadFile { .. }),
                )) => {
                    let SceneResponse::ImmediateRpcCall(RpcCall::ReadFile {
                        response: RpcResultSender::Remote { id, .. },
                        ..
                    }) = &response
                    else {
                        relay.end("immediate call without a reply channel");
                        break;
                    };
                    let id = *id;
                    if relay.forward(response) {
                        relay.awaiting_reply = Some(id);
                    }
                    break;
                }
                Ok(ToEngine::Response(SceneResponse::ImmediateRpcCall(_))) => {
                    relay.end("unexpected immediate call");
                    break;
                }
                Ok(ToEngine::Response(response @ SceneResponse::CrdtSnapshot(..))) => {
                    if relay.snapshot_requests == 0 {
                        relay.end("unrequested snapshot");
                        break;
                    }
                    relay.snapshot_requests -= 1;
                    relay.forward(response);
                }
                Ok(ToEngine::Response(SceneResponse::EntityAllocated(results))) => {
                    // one result per requested slot
                    if relay.allocation_requests.pop_front() != Some(results.len()) {
                        relay.end("unrequested allocation");
                        break;
                    }
                    relay.forward(SceneResponse::EntityAllocated(results));
                }
                // the scene's last word: forwarded once, then the scene is over
                Ok(ToEngine::Response(SceneResponse::Error(mut message))) => {
                    if message.len() > MAX_ERROR_BYTES {
                        let mut end = MAX_ERROR_BYTES;
                        while !message.is_char_boundary(end) {
                            end -= 1;
                        }
                        message.truncate(end);
                    }
                    relay.forward(SceneResponse::Error(message));
                    relay.ended = true;
                    break;
                }
                Ok(ToEngine::Response(SceneResponse::CompareSnapshot(_))) if !relay.testing => (),
                // no inspector on web
                Ok(ToEngine::Response(SceneResponse::WaitingForInspector)) => (),
                Ok(ToEngine::Response(response)) => {
                    relay.forward(response);
                }
                Ok(ToEngine::SystemApi(api)) => match relay.super_user.as_ref() {
                    // only the scene the engine made super-user may reach the system api
                    Some(sender) => {
                        count(|s| s.system_api += 1.0);
                        if sender.send(api).is_err() {
                            count(|s| s.failures += 1.0);
                        }
                    }
                    None => {
                        relay.end("system api from a scene without it");
                        break;
                    }
                },
                // the entry stays until the engine drops its end (its close, above), so the id
                // can't be reused while that end is live
                Ok(ToEngine::Ipc(id, IpcMessage::Closed)) => with_ipc_context(|ctx| {
                    if let Some(token) = ctx.ipc_channel_registry.get(&id) {
                        token.cancel();
                    }
                }),
                Ok(ToEngine::Ipc(..)) => count(|s| s.failures += 1.0),
                Err(_) => {
                    relay.end("undecodable message");
                    break;
                }
            }
        }
        moved
    })
    .unwrap_or(0)
}

/// 0 messages to the scene, 1 messages to the engine, 2 bytes copied, 3 failures,
/// 4 relay limits broken (each ends the scene), 5 system api calls forwarded.
#[wasm_bindgen]
pub fn relay_stat(index: u32) -> f64 {
    STATS
        .try_with(|stats| {
            stats.try_borrow().map_or(-1.0, |s| match index {
                0 => s.to_scene,
                1 => s.to_engine,
                2 => s.bytes,
                3 => s.failures,
                4 => s.rejected,
                5 => s.system_api,
                _ => -1.0,
            })
        })
        .unwrap_or(-1.0)
}

/// Drops the channel ends (the engine sees the scene go away). Call before thread destroy.
#[wasm_bindgen]
pub fn relay_drop() {
    let relay = RELAY
        .try_with(|slot| slot.try_borrow_mut().ok().and_then(|mut slot| slot.take()))
        .ok()
        .flatten();
    drop(relay);
    let _ = ENGINE_IPC_CONTEXT.try_with(|ctx| ctx.try_borrow_mut().map(|mut ctx| ctx.take()));
}
