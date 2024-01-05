use common::rpc::{RPCSendableMessage, RpcCall};
use deno_core::{
    anyhow::{self, anyhow},
    error::AnyError,
    op, Op, OpDecl, OpState,
};
use ethers_providers::{Provider, Ws};
use std::{cell::RefCell, rc::Rc, sync::Arc};
use tokio::sync::Mutex;

const PROVIDER_URL: &str = "wss://rpc.decentraland.org/mainnet?project=kernel-local";

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![op_send_async::DECL]
}

#[op]
async fn op_send_async(
    state: Rc<RefCell<OpState>>,
    method: String,
    params: String,
) -> Result<serde_json::Value, AnyError> {
    let params: Vec<serde_json::Value> = serde_json::from_str(&params)?;

    match method.as_str() {
        "eth_sendTransaction" | "eth_signTypedData_v4" => {
            let (sx, rx) = tokio::sync::oneshot::channel::<Result<serde_json::Value, String>>();

            state
                .borrow_mut()
                .borrow_mut::<Vec<RpcCall>>()
                .push(RpcCall::SendAsync {
                    body: RPCSendableMessage {
                        jsonrpc: "2.0".into(),
                        id: 1,
                        method,
                        params,
                    },
                    response: sx.into(),
                });

            rx.await.map_err(|e| anyhow!(e))?.map_err(|e| anyhow!(e))
        }
        _ => {
            let provider = {
                let mut state = state.borrow_mut();

                if !state.has::<Arc<EthereumProvider>>() {
                    state.put(Arc::<EthereumProvider>::default());
                }
                state.borrow::<Arc<EthereumProvider>>().clone()
            };

            provider
                .send_async(method.as_str(), params.as_slice())
                .await
        }
    }
}

#[derive(Default)]
pub struct EthereumProvider {
    provider: Mutex<Option<Provider<Ws>>>,
}

impl EthereumProvider {
    pub fn new() -> Self {
        Self {
            provider: Mutex::new(None),
        }
    }

    pub async fn send_async(
        &self,
        method: &str,
        params: &[serde_json::Value],
    ) -> Result<serde_json::Value, anyhow::Error> {
        let mut this_provider = self.provider.lock().await;

        let provider = match &*this_provider {
            Some(p) => p,
            None => this_provider.insert(Provider::<Ws>::connect(PROVIDER_URL).await?),
        };

        let result = provider.request(method, params).await;

        match result {
            Err(e) => {
                *this_provider = None;
                Err(anyhow::Error::new(e))
            }
            Ok(result) => Ok(result),
        }
    }
}
