use anyhow::anyhow;
use bevy::prelude::*;
use common::{rpc::RPCSendableMessage, structs::ChainLink, util::AsH160};
use ethers_core::types::H160;
use ethers_signers::LocalWallet;
use http::StatusCode;
use std::{str::FromStr, time::Duration};

use serde::{Deserialize, Serialize};

use crate::SimpleAuthChain;
#[allow(unused_imports)]
use platform::ReqwestBuilderExt;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateRequest {
    pub method: String,
    pub params: Vec<serde_json::Value>, // Using serde_json::Value for unknown[]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_chain: Option<SimpleAuthChain>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializedRequest {
    request_id: String,
    code: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct ServerResponse {
    sender: String,
    result: Option<serde_json::Value>,
    error: Option<ServerResponseError>,
}

#[derive(Debug, Deserialize)]
struct ServerResponseError {
    message: String,
}

fn auth_front_url() -> String {
    common::base_domain::url(common::base_domain::Service::AuthPage, "/requests")
}
fn auth_server_endpoint_url() -> String {
    common::base_domain::url(common::base_domain::Service::AuthApi, "/requests")
}
const AUTH_SERVER_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const AUTH_SERVER_TIMEOUT: Duration = Duration::from_secs(600);

async fn fetch_server(req_id: String) -> Result<(H160, serde_json::Value), anyhow::Error> {
    let start_time = web_time::Instant::now();
    let mut attempt = 0;
    loop {
        debug!("trying req_id {:?} attempt ${attempt}", req_id);
        if web_time::Instant::now()
            .checked_duration_since(start_time)
            .unwrap_or_default()
            >= AUTH_SERVER_TIMEOUT
        {
            return Err(anyhow!("timed out awaiting response"));
        }
        attempt += 1;

        let url = format!("{}/{req_id}", auth_server_endpoint_url());
        let response = reqwest::Client::builder()
            .use_native_tls()
            .build()
            .unwrap()
            .get(&url)
            .timeout(AUTH_SERVER_TIMEOUT)
            .send()
            .await;

        match response {
            Ok(response) => {
                if response.status().is_success() {
                    let text = response.text().await?;
                    if text.is_empty() {
                        async_std::task::sleep(AUTH_SERVER_RETRY_INTERVAL).await;
                        continue;
                    }
                    match serde_json::from_str::<ServerResponse>(&text) {
                        Ok(inner) => {
                            if let Some(t) = inner.result {
                                return Ok((
                                    inner.sender.as_h160().ok_or(anyhow!(
                                        "valid response but couldn't convert signer: {:?}",
                                        inner.sender
                                    ))?,
                                    t,
                                ));
                            }
                            if let Some(err) = inner.error {
                                anyhow::bail!("remote server returned error: {}", err.message);
                            }
                            anyhow::bail!("invalid response (no response or reason)");
                        }
                        Err(e) => {
                            anyhow::bail!("error parsing json as ServerResponse: {:?}", e)
                        }
                    }
                } else {
                    if response.status() == StatusCode::NOT_FOUND {
                        async_std::task::sleep(AUTH_SERVER_RETRY_INTERVAL).await;
                        continue;
                    }

                    anyhow::bail!("Success fetching task but then fail: {:?}", response);
                }
            }
            Err(error) => {
                if error.is_timeout() {
                    continue;
                }
                anyhow::bail!("Error fetching task: {:?}", error);
            }
        }
    }
}

async fn init_request(request: CreateRequest) -> Result<InitializedRequest, anyhow::Error> {
    let body = serde_json::to_string(&request).expect("valid json");

    let response = reqwest::Client::builder()
        .use_native_tls()
        .build()
        .unwrap()
        .post(auth_server_endpoint_url())
        .header("Content-Type", "application/json")
        .timeout(AUTH_SERVER_TIMEOUT)
        .body(body)
        .send()
        .await?;

    if response.status().is_success() {
        let response = response.json::<InitializedRequest>().await?;
        Ok(response)
    } else {
        let status_code = response.status().as_u16();
        let response = response.text().await?;
        anyhow::bail!("Error creating request {status_code}: ${response}")
    }
}

async fn finish_request(request_id: String) -> Result<(H160, serde_json::Value), anyhow::Error> {
    let url = format!(
        "{}/{request_id}?targetConfigId=alternative",
        auth_front_url()
    );
    opener::open_browser(url)?;

    fetch_server(request_id).await
}

pub async fn remote_send_async(
    message: RPCSendableMessage,
    auth_chain: Option<SimpleAuthChain>,
) -> Result<serde_json::Value, anyhow::Error> {
    let req = init_request(CreateRequest {
        method: message.method,
        params: message.params,
        auth_chain,
    })
    .await?;

    println!(
        "send_async code (tbd integrate into flow properly when there's a test case): {:?}",
        req.code
    );

    finish_request(req.request_id)
        .await
        .map(|(_, payload)| payload)
}

/// The auth-server sign-in relay (`dcl_personal_sign` via `/requests`) was retired in 2026-07.
/// Sign-in now runs entirely in the browser: the auth site builds the AuthIdentity itself,
/// stores it with `POST /identities`, and hands the id back through a `decentraland://` deep
/// link, which reaches us via the launcher's bridge file (see `platform::deeplink`). We then
/// collect the identity with `GET /identities/{id}`; it is single-use, expires after an hour,
/// and must be fetched from the same IP that created it.
pub struct RemoteEphemeralRequest {
    /// Client-minted UUID v4 (the site rejects anything else); it is embedded in the auth URL
    /// and echoed back on the deep link as `authRequestId`, so we only consume our own link.
    request_id: String,
}

pub async fn init_remote_ephemeral_request() -> Result<RemoteEphemeralRequest, anyhow::Error> {
    platform::deeplink::ensure_scheme_handler()?;
    Ok(RemoteEphemeralRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
    })
}

pub async fn finish_remote_ephemeral_request(
    request: RemoteEphemeralRequest,
) -> Result<(H160, LocalWallet, Vec<ChainLink>, u64), anyhow::Error> {
    let RemoteEphemeralRequest { request_id } = request;

    // `bridgeOnly`: an installed launcher must relay the link to us rather than start unity
    let url = format!(
        "{}/{request_id}?targetConfigId=alternative&flow=deeplink&bridgeOnly",
        auth_front_url()
    );
    info!("opening {url} for sign-in");
    opener::open_browser(url)?;

    let identity_id = platform::deeplink::await_signin(&request_id, AUTH_SERVER_TIMEOUT).await?;
    info!("sign-in link received for request {request_id}, fetching identity");
    let identity = fetch_identity(&identity_id).await?;
    let (signer, ephemeral_wallet, delegates) = auth_identity_parts(identity)?;
    Ok((signer, ephemeral_wallet, delegates, 1))
}

#[derive(Deserialize)]
struct IdentityResponse {
    identity: AuthIdentity,
}

/// The standard Decentraland AuthIdentity, as served by `GET /identities/{id}` and as the web
/// page stores it in localStorage.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthIdentity {
    ephemeral_identity: EphemeralIdentity,
    auth_chain: Vec<ChainLink>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EphemeralIdentity {
    private_key: String,
}

async fn fetch_identity(identity_id: &str) -> Result<AuthIdentity, anyhow::Error> {
    let url = common::base_domain::url(
        common::base_domain::Service::AuthApi,
        &format!("/identities/{identity_id}"),
    );
    let response = reqwest::Client::builder()
        .use_native_tls()
        .build()
        .unwrap()
        .get(url)
        .timeout(AUTH_SERVER_TIMEOUT)
        .send()
        .await?;

    match response.status() {
        status if status.is_success() => Ok(response.json::<IdentityResponse>().await?.identity),
        StatusCode::NOT_FOUND => anyhow::bail!("sign-in was not found or was already used"),
        StatusCode::GONE => anyhow::bail!("sign-in expired before it was collected"),
        StatusCode::FORBIDDEN => anyhow::bail!(
            "sign-in was created from a different IP address (a VPN or private relay may be interfering)"
        ),
        status => {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("error fetching sign-in identity {status}: {body}")
        }
    }
}

/// Split an AuthIdentity into the pieces the wallet needs: the root (signer) address, the
/// ephemeral LocalWallet, and the delegate chain (everything except the SIGNER link).
pub fn auth_identity_parts(
    identity: AuthIdentity,
) -> Result<(H160, LocalWallet, Vec<ChainLink>), anyhow::Error> {
    let signer = identity
        .auth_chain
        .iter()
        .find(|link| link.ty == "SIGNER")
        .ok_or_else(|| anyhow!("identity missing SIGNER link"))?;
    let root_address = signer
        .payload
        .as_h160()
        .ok_or_else(|| anyhow!("bad root address: {:?}", signer.payload))?;

    let key_hex = identity
        .ephemeral_identity
        .private_key
        .trim()
        .trim_start_matches("0x");
    let ephemeral_wallet =
        LocalWallet::from_str(key_hex).map_err(|e| anyhow!("bad ephemeral key: {e}"))?;

    let delegates: Vec<ChainLink> = identity
        .auth_chain
        .into_iter()
        .filter(|link| link.ty != "SIGNER")
        .collect();
    if delegates.is_empty() {
        anyhow::bail!("identity missing ephemeral delegate link");
    }

    Ok((root_address, ephemeral_wallet, delegates))
}

/// Decode the base64(JSON) form the web page forwards from localStorage.
pub fn parse_auth_identity(payload: &str) -> Result<AuthIdentity, anyhow::Error> {
    use base64::Engine as _;
    let json = base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .map_err(|e| anyhow!("bad identity base64: {e}"))?;
    serde_json::from_slice(&json).map_err(|e| anyhow!("bad identity json: {e}"))
}
