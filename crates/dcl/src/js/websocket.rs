use std::{cell::RefCell, rc::Rc};

use common::rpc::RpcCall;
use deno_core::{anyhow, error::AnyError, op2, ByteString, OpDecl, OpState, ResourceId};
use deno_websocket::{CreateResponse, WebSocketPermissions};
use tokio::sync::oneshot::channel;

use crate::{interface::crdt_context::CrdtContext, RpcCalls};

// list of op declarations
pub fn override_ops() -> Vec<OpDecl> {
    vec![op_ws_create::<WebSocketPerms>()]
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
        if self.preview || url.scheme() == "wss" {
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
    let scene = state.borrow_mut().borrow::<CrdtContext>().scene_id.0;
    let (sx, rx) = channel();
    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::RequestGenericPermission {
            scene,
            ty: common::structs::PermissionType::Websocket,
            message: Some(url.clone()),
            response: sx.into(),
        });
    let permit = rx.await?;
    if !permit {
        anyhow::bail!("User denied fetch request");
    }

    deno_websocket::op_ws_create__raw_fn::<WP>(
        state,
        api_name,
        url,
        protocols,
        cancel_handle,
        headers,
    )
    .await
}
