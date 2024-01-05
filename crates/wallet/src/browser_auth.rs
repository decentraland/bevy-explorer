use anyhow::anyhow;
use bevy::prelude::*;
use common::{structs::ChainLink, util::AsH160};
use ethers_core::types::{Signature, H160};
use ethers_signers::{LocalWallet, Signer};
use isahc::{config::Configurable, http::StatusCode, AsyncReadResponseExt, RequestExt};
use std::{str::FromStr, time::Duration};

use rand::{thread_rng, Rng};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Deserialize, Debug)]
pub struct RemoteWalletResponse<T> {
    pub ok: bool,
    pub reason: Option<String>,
    pub response: Option<T>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SignResponseData {
    pub account: String,
    pub signature: String,
    pub chain_id: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RPCSendableMessage {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Vec<serde_json::Value>, // Using serde_json::Value for unknown[]
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RemoteWalletRequest {
    #[serde(rename = "send-async", rename_all = "camelCase")]
    SendAsync {
        body: RPCSendableMessage,
        #[serde(skip_serializing_if = "Option::is_none")]
        by_address: Option<String>,
    },
    #[serde(rename = "sign", rename_all = "camelCase")]
    Sign {
        b64_message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        by_address: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterRequestBody {
    pub id: String,
    pub request: RemoteWalletRequest,
}

const AUTH_FRONT_URL: &str = "https://auth.dclexplorer.com/";
const AUTH_SERVER_ENDPOINT_URL: &str = "https://auth-server.dclexplorer.com/task/";
const AUTH_SERVER_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const AUTH_SERVER_TIMEOUT: Duration = Duration::from_secs(600);

pub enum RemoteReportState {
    OpenUrl { url: String, description: String },
}

pub fn gen_id() -> String {
    rand::thread_rng()
        .sample_iter(rand::distributions::Alphanumeric)
        .take(56)
        .collect::<Vec<u8>>()
        .into_iter()
        .map(|byte| byte as char)
        .collect()
}

async fn fetch_server<T>(req_id: String) -> Result<T, anyhow::Error>
where
    T: DeserializeOwned + Unpin + std::fmt::Debug,
{
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

        let url = format!("{AUTH_SERVER_ENDPOINT_URL}/{req_id}/response");
        let response = isahc::Request::get(&url)
            .timeout(AUTH_SERVER_TIMEOUT)
            .body(())?
            .send_async()
            .await;

        match response {
            Ok(mut response) => {
                if response.status().is_success() {
                    match response.json::<RemoteWalletResponse<T>>().await {
                        Ok(rwr) => match (rwr.response, rwr.reason) {
                            (Some(t), _) => return Ok(t),
                            (_, Some(r)) => anyhow::bail!("remote server returned error: {}", r),
                            _ => anyhow::bail!("invalid response (no response or reason)"),
                        },
                        Err(e) => {
                            anyhow::bail!("error parsing json as RemoteWalletResponse: {:?}", e)
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

async fn register_request(
    req_id: String,
    request: RemoteWalletRequest,
) -> Result<(), anyhow::Error> {
    let body = RegisterRequestBody {
        id: req_id,
        request,
    };
    let body = serde_json::to_string(&body).expect("valid json");
    let response = isahc::Request::post(AUTH_SERVER_ENDPOINT_URL)
        .timeout(AUTH_SERVER_RETRY_INTERVAL)
        .header("Content-Type", "application/json")
        .body(body)?
        .send_async()
        .await?;

    if response.status().is_success() {
        Ok(())
    } else {
        error!("Error registering request: {:?}", response);
        Err(anyhow::Error::msg("couldn't get response"))
    }
}

async fn generate_and_report_request(
    request: RemoteWalletRequest,
) -> Result<String, anyhow::Error> {
    let req_id = gen_id();
    register_request(req_id.clone(), request).await?;
    let open_url = format!("{AUTH_FRONT_URL}remote-wallet/{req_id}");

    debug!("sign url {:?}", open_url);
    opener::open_browser(open_url)?;

    Ok(req_id)
}

pub async fn remote_sign_message(
    payload: &[u8],
    by_signer: Option<H160>,
) -> Result<(H160, Signature, u64), anyhow::Error> {
    let by_address = by_signer.map(|address| format!("{:#x}", address));
    let b64_message = data_encoding::BASE64URL_NOPAD.encode(payload);

    let req_id = generate_and_report_request(RemoteWalletRequest::Sign {
        b64_message,
        by_address,
    })
    .await?;
    let sign_payload = fetch_server::<SignResponseData>(req_id).await?;
    let Some(account) = sign_payload.account.as_h160() else {
        anyhow::bail!("invalid address from server: {}", sign_payload.account);
    };
    let Ok(signature) = Signature::from_str(sign_payload.signature.as_str()) else {
        anyhow::bail!("error parsing signature from server");
    };

    Ok((account, signature, sign_payload.chain_id))
}

pub async fn remote_send_async(
    message: RPCSendableMessage,
    by_signer: Option<H160>,
) -> Result<serde_json::Value, anyhow::Error> {
    let by_address = by_signer.map(|address| format!("{:#x}", address));
    let req_id = generate_and_report_request(RemoteWalletRequest::SendAsync {
        body: message,
        by_address,
    })
    .await?;

    fetch_server::<serde_json::Value>(req_id).await
}

fn get_ephemeral_message(ephemeral_address: &str, expiration: std::time::SystemTime) -> String {
    let datetime: chrono::DateTime<chrono::Utc> = expiration.into();
    let formatted_time = datetime.format("%Y-%m-%dT%H:%M:%S%.3fZ");
    format!(
        "Decentraland Login\nEphemeral address: {ephemeral_address}\nExpiration: {formatted_time}",
    )
}

pub async fn try_create_remote_ephemeral(
) -> Result<(H160, LocalWallet, Vec<ChainLink>, u64), anyhow::Error> {
    let ephemeral_wallet = LocalWallet::new(&mut thread_rng());
    let ephemeral_address = format!("{:#x}", ephemeral_wallet.address());
    let expiration = std::time::SystemTime::now() + std::time::Duration::from_secs(30 * 24 * 3600);
    let message = get_ephemeral_message(ephemeral_address.as_str(), expiration);

    let (signer, signature, chain_id) = remote_sign_message(message.as_bytes(), None).await?;
    let delegate = ChainLink {
        ty: "ECDSA_EPHEMERAL".to_owned(),
        payload: message,
        signature: format!("0x{}", signature),
    };
    Ok((signer, ephemeral_wallet, vec![delegate], chain_id))
}
