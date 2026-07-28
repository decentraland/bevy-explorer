use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    time::Duration,
};

use common::{
    rpc::{RpcCall, RpcResultSender},
    util::UrlLoopbackExt,
};
use deno_core::{anyhow, error::AnyError, op2, ByteString, OpDecl, OpState, Resource, ResourceId};
use deno_websocket::{CreateResponse, WebSocketPermissions};

use dcl::{interface::crdt_context::CrdtContext, RpcCalls, SceneResourceCounters};

const MAX_OPEN_SOCKETS: usize = 32;
const MAX_WS_BUFFERED_BYTES: usize = 8 * 1024 * 1024;
const WS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_WS_MESSAGE_BYTES: usize = 1024 * 1024;

// list of op declarations
pub fn override_ops() -> Vec<OpDecl> {
    vec![
        op_ws_create::<WebSocketPerms>(),
        op_ws_send_binary(),
        op_ws_send_binary_ab(),
        op_ws_send_text(),
        op_ws_close(),
        op_ws_next_event(),
        op_ws_get_buffer_as_string(),
    ]
}

#[derive(Default)]
struct WsOpenSockets {
    per_scene: HashMap<u64, Rc<Cell<usize>>>,
    guards: HashMap<ResourceId, ResourceId>,
}

// One open-socket slot; the count is decremented once, on Drop, so isolate teardown of a
// panicking scene still reclaims the slot.
struct WsSlotGuard {
    counter: Rc<Cell<usize>>,
}

impl WsSlotGuard {
    fn acquire(counter: Rc<Cell<usize>>) -> Self {
        counter.set(counter.get() + 1);
        Self { counter }
    }
}

impl Drop for WsSlotGuard {
    fn drop(&mut self) {
        self.counter.set(self.counter.get().saturating_sub(1));
    }
}

impl Resource for WsSlotGuard {
    fn name(&self) -> Cow<'_, str> {
        "dclWsSlotGuard".into()
    }
}

fn scene_socket_counter(state: &mut OpState, scene_key: u64) -> Rc<Cell<usize>> {
    if state.try_borrow::<WsOpenSockets>().is_none() {
        state.put(WsOpenSockets::default());
    }
    state
        .borrow_mut::<WsOpenSockets>()
        .per_scene
        .entry(scene_key)
        .or_default()
        .clone()
}

fn release_ws_slot(state: &mut OpState, ws_rid: ResourceId) {
    let guard_rid = match state.try_borrow_mut::<WsOpenSockets>() {
        Some(reg) => reg.guards.remove(&ws_rid),
        None => return,
    };
    if let Some(guard_rid) = guard_rid {
        let _ = state.resource_table.take::<WsSlotGuard>(guard_rid);
    }
}

pub struct WebSocketPerms {
    pub preview: bool,
}

impl WebSocketPermissions for WebSocketPerms {
    fn check_net_url(
        &mut self,
        url: &deno_core::url::Url,
        _api_name: &str,
    ) -> Result<(), AnyError> {
        // scene permissions must be handled asynchronously, so we check them in op_ws_create
        // (which we replace with our own op)
        // must use `wss`
        if self.preview || url.scheme() == "wss" || url.is_loopback() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("URL scheme must be `wss`"))
        }
    }
}

#[op2(async)]
#[serde]
pub async fn op_ws_create<WP>(
    state: Rc<RefCell<OpState>>,
    #[string] api_name: String,
    #[string] url: String,
    #[string] protocols: String,
    #[smi] cancel_handle: Option<ResourceId>,
    #[serde] headers: Option<Vec<(ByteString, ByteString)>>,
) -> Result<CreateResponse, AnyError>
where
    WP: WebSocketPermissions + 'static,
{
    // check permission
    let scene = state.borrow_mut().borrow::<CrdtContext>().scene_id.0;
    let (sx, rx) = RpcResultSender::channel();
    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::RequestGenericPermission {
            scene,
            ty: common::structs::PermissionType::Websocket,
            message: Some(url.clone()),
            response: sx,
        });
    let permit = rx.await?;
    if !permit {
        anyhow::bail!("User denied fetch request");
    }

    // SSRF guard — SERVER MODE ONLY (client/web keep the browser-like behaviour). The
    // scheme check above still lets `wss://<private-ip>` (or loopback in non-preview)
    // through; resolve and refuse any non-public destination on the shared server.
    let (is_server, allow_loopback) = {
        let op_state = state.borrow();
        let ctx = op_state.borrow::<CrdtContext>();
        (ctx.is_server, ctx.preview)
    };
    if is_server {
        common::util::assert_public_url(&url, allow_loopback).await?;
    }

    // set default headers
    let mut headers = headers.unwrap_or_default();
    if !headers
        .iter()
        .any(|(key, _)| key == &ByteString::from("user-agent"))
    {
        headers.push(("user-agent".into(), "DCLExplorer/0.1".into()));
    }
    if !headers
        .iter()
        .any(|(key, _)| key == &ByteString::from("accept"))
    {
        headers.push(("accept".into(), "*/*".into()));
    }

    // Reserve a per-scene slot (reject, never queue, at the cap) so an unbounded
    // `new WebSocket()` loop cannot outrun the process FD ulimit.
    let guard = {
        let mut op_state = state.borrow_mut();
        let counter = scene_socket_counter(&mut op_state, scene.to_bits());
        if counter.get() >= MAX_OPEN_SOCKETS {
            anyhow::bail!("WebSocket refused: scene already holds {MAX_OPEN_SOCKETS} open sockets");
        }
        WsSlotGuard::acquire(counter)
    };

    // Bound the handshake so a black-hole host cannot pin the connect (and its slot); on timeout
    // or error `guard` drops here and releases the slot.
    let response = match tokio::time::timeout(
        WS_HANDSHAKE_TIMEOUT,
        deno_websocket::op_ws_create__raw_fn::<WP>(
            state.clone(),
            api_name,
            url,
            protocols,
            cancel_handle,
            Some(headers),
        ),
    )
    .await
    {
        Ok(result) => result?,
        Err(_) => anyhow::bail!("WebSocket handshake timed out"),
    };

    state
        .borrow_mut()
        .borrow_mut::<SceneResourceCounters>()
        .ws_opened += 1;

    let ws_rid = serde_json::to_value(&response)
        .ok()
        .and_then(|v| v.get("rid").and_then(|r| r.as_u64()))
        .map(|rid| rid as ResourceId);

    // Park the guard keyed to the live socket so its slot is held for the socket's lifetime and
    // released on close/terminal-event/teardown.
    if let Some(ws_rid) = ws_rid {
        let mut op_state = state.borrow_mut();
        let guard_rid = op_state.resource_table.add(guard);
        if op_state.try_borrow::<WsOpenSockets>().is_none() {
            op_state.put(WsOpenSockets::default());
        }
        op_state
            .borrow_mut::<WsOpenSockets>()
            .guards
            .insert(ws_rid, guard_rid);
    }

    Ok(response)
}

// Refuse a send that would push still-pending outbound bytes past the buffered cap, so a scene
// writing faster than the peer drains cannot grow the sidecar heap without bound.
fn guard_ws_buffer(state: &mut OpState, rid: ResourceId, len: usize) -> Result<(), AnyError> {
    let buffered = deno_websocket::op_ws_get_buffered_amount__raw_fn(state, rid) as usize;
    if buffered.saturating_add(len) > MAX_WS_BUFFERED_BYTES {
        anyhow::bail!(
            "WebSocket send refused: outbound buffer exceeds {MAX_WS_BUFFERED_BYTES} bytes"
        );
    }
    Ok(())
}

#[op2]
pub fn op_ws_send_binary(
    state: &mut OpState,
    #[smi] rid: ResourceId,
    #[anybuffer] data: &[u8],
) -> Result<(), AnyError> {
    guard_ws_buffer(state, rid, data.len())?;
    deno_websocket::op_ws_send_binary__raw_fn(state, rid, data);
    Ok(())
}

#[op2(fast)]
pub fn op_ws_send_binary_ab(
    state: &mut OpState,
    #[smi] rid: ResourceId,
    #[arraybuffer] data: &[u8],
) -> Result<(), AnyError> {
    guard_ws_buffer(state, rid, data.len())?;
    deno_websocket::op_ws_send_binary_ab__raw_fn(state, rid, data);
    Ok(())
}

#[op2(fast)]
pub fn op_ws_send_text(
    state: &mut OpState,
    #[smi] rid: ResourceId,
    #[string] data: String,
) -> Result<(), AnyError> {
    guard_ws_buffer(state, rid, data.len())?;
    deno_websocket::op_ws_send_text__raw_fn(state, rid, data);
    Ok(())
}

#[op2(async(lazy))]
pub async fn op_ws_close(
    state: Rc<RefCell<OpState>>,
    #[smi] rid: ResourceId,
    #[smi] code: Option<u16>,
    #[string] reason: Option<String>,
) -> Result<(), AnyError> {
    release_ws_slot(&mut state.borrow_mut(), rid);
    deno_websocket::op_ws_close__raw_fn(state, rid, code, reason).await
}

#[op2(async)]
pub async fn op_ws_next_event(state: Rc<RefCell<OpState>>, #[smi] rid: ResourceId) -> u16 {
    let kind = deno_websocket::op_ws_next_event__raw_fn(state.clone(), rid).await;
    // >= 3 is Error or a close code: the read loop is finished, so free the slot now.
    if kind >= 3 {
        release_ws_slot(&mut state.borrow_mut(), rid);
    }
    kind
}

// Enforce the inbound message-size cap on a text frame (binary frames are opaque post-hoc and
// cannot be sized through the public API); an oversized message force-closes the socket.
#[op2]
#[string]
pub fn op_ws_get_buffer_as_string(state: &mut OpState, #[smi] rid: ResourceId) -> Option<String> {
    let data = deno_websocket::op_ws_get_buffer_as_string__raw_fn(state, rid);
    if let Some(text) = &data {
        if text.len() > MAX_WS_MESSAGE_BYTES {
            release_ws_slot(state, rid);
            if let Ok(resource) = state.resource_table.take_any(rid) {
                resource.close();
            }
            return None;
        }
    }
    data
}
