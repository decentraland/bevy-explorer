use anyhow::anyhow;
use bevy::prelude::*;
use common::{rpc::RPCSendableMessage, structs::ChainLink, util::AsH160};
use ethers_core::types::{Signature, H160};
use ethers_signers::{LocalWallet, Signer};
use http::StatusCode;
use std::{str::FromStr, time::Duration};

use rand::thread_rng;
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

const AUTH_FRONT_URL: &str = "https://decentraland.org/auth/requests";
const AUTH_SERVER_ENDPOINT_URL: &str = "https://auth-api.decentraland.org/requests";
const AUTH_SERVER_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const AUTH_SERVER_TIMEOUT: Duration = Duration::from_secs(600);

async fn fetch_server(req_id: String) -> Result<(H160, serde_json::Value), anyhow::Error> {
    let start_time = std::time::Instant::now();
    let mut attempt = 0;
    loop {
        debug!("trying req_id {:?} attempt ${attempt}", req_id);
        if std::time::Instant::now()
            .checked_duration_since(start_time)
            .unwrap_or_default()
            >= AUTH_SERVER_TIMEOUT
        {
            return Err(anyhow!("timed out awaiting response"));
        }
        attempt += 1;

        let url = format!("{AUTH_SERVER_ENDPOINT_URL}/{req_id}");
        let response = reqwest::Client::builder()
            .use_native_tls()
            .timeout(AUTH_SERVER_TIMEOUT)
            .build()
            .unwrap()
            .get(&url)
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
        .timeout(AUTH_SERVER_TIMEOUT)
        .build()
        .unwrap()
        .post(AUTH_SERVER_ENDPOINT_URL)
        .header("Content-Type", "application/json")
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
    let url = format!("{AUTH_FRONT_URL}/{request_id}?targetConfigId=alternative");
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

fn get_ephemeral_message(ephemeral_address: &str, expiration: std::time::SystemTime) -> String {
    let datetime: chrono::DateTime<chrono::Utc> = expiration.into();
    let formatted_time = datetime.format("%Y-%m-%dT%H:%M:%S%.3fZ");
    format!(
        "Decentraland Login\nEphemeral address: {ephemeral_address}\nExpiration: {formatted_time}",
    )
}

pub struct RemoteEphemeralRequest {
    pub code: Option<i32>,
    request_id: String,
    message: String,
    ephemeral_wallet: LocalWallet,
}

pub async fn init_remote_ephemeral_request() -> Result<RemoteEphemeralRequest, anyhow::Error> {
    let ephemeral_wallet = LocalWallet::new(&mut thread_rng());
    let ephemeral_address = format!("{:#x}", ephemeral_wallet.address());
    let expiration = std::time::SystemTime::now() + std::time::Duration::from_secs(30 * 24 * 3600);
    let message = get_ephemeral_message(ephemeral_address.as_str(), expiration);

    let request = CreateRequest {
        method: "dcl_personal_sign".to_owned(),
        params: vec![message.clone().into()],
        auth_chain: None,
    };
    init_request(request)
        .await
        .map(|init| RemoteEphemeralRequest {
            code: init.code,
            request_id: init.request_id,
            message,
            ephemeral_wallet,
        })
}

pub async fn finish_remote_ephemeral_request(
    request: RemoteEphemeralRequest,
) -> Result<(H160, LocalWallet, Vec<ChainLink>, u64), anyhow::Error> {
    let RemoteEphemeralRequest {
        request_id,
        message,
        ephemeral_wallet,
        ..
    } = request;

    let (signer, result) = finish_request(request_id).await?;
    let signature = Signature::from_str(result.as_str().ok_or(anyhow!("result is not a string"))?)?;

    let delegate = ChainLink {
        ty: "ECDSA_EPHEMERAL".to_owned(),
        payload: message,
        signature: format!("0x{}", signature),
    };
    Ok((signer, ephemeral_wallet, vec![delegate], 1))
}
