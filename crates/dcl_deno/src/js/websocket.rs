use std::{cell::RefCell, collections::HashSet, net::SocketAddr, rc::Rc, time::Duration};

use common::{
    rpc::{RpcCall, RpcResultSender},
    util::UrlLoopbackExt,
};
use deno_core::{anyhow, error::AnyError, op2, ByteString, JsBuffer, OpDecl, OpState, ResourceId};
use deno_websocket::{CreateResponse, WebSocketPermissions};

use dcl::{interface::crdt_context::CrdtContext, RpcCalls, SceneResourceCounters};

const MAX_OPEN_SOCKETS: usize = 32;
const MAX_WS_BUFFERED_BYTES: usize = 8 * 1024 * 1024;
const WS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
// Inbound message size (per frame and per fragmented total) is capped at 1 MiB by the
// fastwebsockets fork default (see the workspace [patch.crates-io] entry); an oversized
// message errors and closes the socket without the payload ever being buffered.

// list of op declarations
pub fn override_ops() -> Vec<OpDecl> {
    vec![
        op_ws_create::<WebSocketPerms>(),
        op_ws_send_binary(),
        op_ws_send_binary_ab(),
        op_ws_send_text(),
        op_ws_send_binary_async(),
        op_ws_send_text_async(),
        op_ws_close(),
        op_ws_next_event(),
    ]
}

// per-scene: each scene runs in its own isolate with its own OpState
#[derive(Default)]
struct SceneWsState {
    open: HashSet<ResourceId>,
    // Reserved slots for handshakes still in flight, so a burst of `new WebSocket()` can't
    // race past the cap before any rid exists.
    connecting: usize,
}

fn scene_ws_state(state: &mut OpState) -> &mut SceneWsState {
    if state.try_borrow::<SceneWsState>().is_none() {
        state.put(SceneWsState::default());
    }
    state.borrow_mut::<SceneWsState>()
}

fn release_ws_slot(state: &mut OpState, ws_rid: ResourceId) {
    if let Some(ws_state) = state.try_borrow_mut::<SceneWsState>() {
        ws_state.open.remove(&ws_rid);
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

    fn check_resolved(&mut self, addrs: &[SocketAddr], _api_name: &str) -> Result<(), AnyError> {
        // These are the exact addresses the handshake is about to dial, so vetting them here
        // closes the DNS-rebind window a pre-flight check leaves open. `preview` widens the
        // allowance to the local network, never link-local / metadata.
        common::util::validate_public_addrs(addrs, self.preview)
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
        })?;
    let permit = rx.await?;
    if !permit {
        anyhow::bail!("User denied fetch request");
    }

    // SSRF guard — every scene, every mode (same rule as fetch). Enforced at the connection
    // itself: `WebSocketPerms::resolve_checked` (called inside the deno handshake) resolves +
    // validates and returns only public addresses, and those exact addresses are what get
    // dialled — so it cannot be beaten by a rebinding nameserver the way a pre-flight can.
    // `preview` widens the allowance to the local network there (never link-local / metadata).

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
    {
        let mut op_state = state.borrow_mut();
        let ws_state = scene_ws_state(&mut op_state);
        if ws_state.open.len() + ws_state.connecting >= MAX_OPEN_SOCKETS {
            anyhow::bail!("WebSocket refused: scene already holds {MAX_OPEN_SOCKETS} open sockets");
        }
        ws_state.connecting += 1;
    }

    // Bound the handshake so a black-hole host cannot pin the connect (and its slot).
    let result = tokio::time::timeout(
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
    .await;

    {
        let mut op_state = state.borrow_mut();
        let ws_state = scene_ws_state(&mut op_state);
        ws_state.connecting = ws_state.connecting.saturating_sub(1);
    }

    let response = match result {
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

    // Track the live socket so its slot is held until close/terminal-event/teardown.
    if let Some(ws_rid) = ws_rid {
        let mut op_state = state.borrow_mut();
        scene_ws_state(&mut op_state).open.insert(ws_rid);
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

// The async send ops (used by `WebSocketStream`, and reachable directly through the ops
// table) must apply the same outbound-buffer cap as the sync sends above; otherwise a scene
// can send without limit through them and defeat the bound.
#[op2(async)]
pub async fn op_ws_send_binary_async(
    state: Rc<RefCell<OpState>>,
    #[smi] rid: ResourceId,
    #[buffer] data: JsBuffer,
) -> Result<(), AnyError> {
    guard_ws_buffer(&mut state.borrow_mut(), rid, data.len())?;
    deno_websocket::op_ws_send_binary_async__raw_fn(state, rid, data).await
}

#[op2(async)]
pub async fn op_ws_send_text_async(
    state: Rc<RefCell<OpState>>,
    #[smi] rid: ResourceId,
    #[string] data: String,
) -> Result<(), AnyError> {
    guard_ws_buffer(&mut state.borrow_mut(), rid, data.len())?;
    deno_websocket::op_ws_send_text_async__raw_fn(state, rid, data).await
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
    // every close path goes through op_ws_close, but the JS glue's error branch tears the
    // socket down with core.tryClose alone, so the slot must be freed here.
    if kind == deno_websocket::MessageKind::Error as u16 {
        release_ws_slot(&mut state.borrow_mut(), rid);
    }
    kind
}
