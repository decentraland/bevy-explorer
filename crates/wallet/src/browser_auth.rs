use anyhow::anyhow;
use bevy::prelude::*;
use common::{structs::ChainLink, util::AsH160};
use ethers_core::types::{Signature, H160};
use ethers_signers::{LocalWallet, Signer};
use isahc::{config::Configurable, http::StatusCode, AsyncReadResponseExt, RequestExt};
use std::{str::FromStr, time::Duration};

use rand::{thread_rng, Rng};
use serde::{de::DeserializeOwned, Deserialize};

// #[derive(Deserialize, Debug)]
// #[serde(rename_all = "camelCase")]
// struct GetAccountResponseData {
//     address: String,
//     chain_id: u64,
// }

// #[derive(Deserialize, Debug)]
// struct GetAccountResponse {
//     data: GetAccountResponseData,
// }

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct SignToServerResponseData {
    account: String,
    signature: String,
    chain_id: u64,
}

#[derive(Deserialize, Debug)]
struct SignToServerResponse {
    data: SignToServerResponseData,
}

const AUTH_FRONT_URL: &str = "https://leanmendoza.github.io/decentraland-auth/";
const AUTH_SERVER_ENDPOINT_URL: &str = "https://services.aesir-online.net/dcltest/queue/task";
const AUTH_SERVER_RETRIES: u64 = 10;
const AUTH_SERVER_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const AUTH_SERVER_TIMEOUT: Duration = Duration::from_secs(120);

pub enum RemoteReportState {
    OpenUrl { url: String, description: String },
}

pub fn gen_session_id() -> String {
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
    T: DeserializeOwned + Unpin,
{
    let mut attempt = 0;
    loop {
        debug!("trying req_id {:?} attempt ${attempt}", req_id);
        if attempt >= AUTH_SERVER_RETRIES {
            return Err(anyhow!("timed out awaiting response"));
        }
        attempt += 1;

        let url = format!("{AUTH_SERVER_ENDPOINT_URL}/{req_id}");
        let response = isahc::Request::get(&url)
            .timeout(AUTH_SERVER_TIMEOUT)
            .body(())?
            .send_async()
            .await;

        match response {
            Ok(mut response) => {
                if response.status().is_success() {
                    match response.json::<T>().await {
                        Ok(response_data) => {
                            return Ok(response_data);
                        }
                        Err(error) => {
                            anyhow::bail!("error while parsing a task {:?}", error);
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

pub async fn remote_sign_message(
    payload: &[u8],
    by_signer: Option<H160>,
) -> Result<(H160, Signature, u64), anyhow::Error> {
    let address = if by_signer.is_some() {
        format!("{:#x}", by_signer.unwrap())
    } else {
        "".into()
    };
    let sign_payload_req_id = gen_session_id();
    let server_endpoint = urlencoding::encode(AUTH_SERVER_ENDPOINT_URL);
    let payload = data_encoding::BASE64URL_NOPAD.encode(payload);
    let open_url =
        format!("{AUTH_FRONT_URL}sign-to-server?id={sign_payload_req_id}&payload={payload}&address={address}&server-endpoint={server_endpoint}");

    debug!("sign url {:?}", open_url);

    opener::open_browser(open_url)?;

    let sign_payload = fetch_server::<SignToServerResponse>(sign_payload_req_id).await?;
    let Some(account) = sign_payload.data.account.as_h160() else {
        anyhow::bail!("invalid address from server: {}", sign_payload.data.account);
    };
    let Ok(signature) = Signature::from_str(sign_payload.data.signature.as_str()) else {
        anyhow::bail!("error parsing signature from server");
    };

    Ok((account, signature, sign_payload.data.chain_id))
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

// #[cfg(test)]
// mod test {
//     use super::*;

//     #[tokio::test]
//     async fn test_gen_id() {
//         let Ok((signer, signature, _chain_id)) =
//             remote_sign_message("hello".as_bytes(), None).await
//         else {
//             return;
//         };
//         info!("signer {:?} signature {:?}", signer, signature);
//     }
// }
