// https://github.com/decentraland/hammurabi/pull/33/files#diff-18afcd5f94e3688aad1ba36fa1db3e09b472b271d1e0cf5aeb59ebd32f43a328

use async_tungstenite::tungstenite::http::Uri;
use bevy::prelude::warn;
use surf::StatusCode;

use crate::wallet::{SimpleAuthChain, Wallet};

#[derive(Debug, serde::Deserialize)]
pub struct SignedLoginResponse {
    pub message: Option<String>,
    #[serde(rename = "fixedAdapter")]
    pub fixed_adapter: Option<String>,
}

#[derive(serde::Serialize)]
pub struct SignedLoginMeta {
    pub intent: String,
    pub signer: String,
    #[serde(rename = "isGuest")]
    is_guest: bool,
    origin: String,
}

impl SignedLoginMeta {
    pub fn new(is_guest: bool, origin: Uri) -> Self {
        let origin = origin.into_parts();

        Self {
            intent: "dcl:explorer:comms-handshake".to_owned(),
            signer: "dcl:explorer".to_owned(),
            is_guest,
            origin: format!("{}://{}", origin.scheme.unwrap(), origin.authority.unwrap()),
        }
    }
}

pub async fn signed_login(
    uri: Uri,
    wallet: Wallet,
    meta: SignedLoginMeta,
) -> Result<SignedLoginResponse, anyhow::Error> {
    let unix_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();

    let meta = serde_json::to_string(&meta).unwrap();

    let payload = format!("post:{}:{}:{}", uri.path(), unix_time, meta).to_lowercase();
    let signature = wallet.sign_message(&payload).await.unwrap();
    let auth_chain = SimpleAuthChain::new(wallet.address(), payload, signature);

    let mut builder = surf::post(uri.to_string());

    for (key, value) in auth_chain.headers() {
        builder = builder.header(key.as_str(), value)
    }

    let req = builder
        .header("x-identity-timestamp", format!("{unix_time}"))
        .header("x-identity-metadata", meta);

    let mut res = req.await.map_err(|e| anyhow::anyhow!(e))?;

    if res.status() != StatusCode::Ok {
        warn!("signed fetch failed: {res:#?}");
        return Err(anyhow::anyhow!("status: {}", res.status()));
    }

    res.body_json().await.map_err(|e| anyhow::anyhow!(e))
}
