// https://github.com/decentraland/hammurabi/pull/33/files#diff-18afcd5f94e3688aad1ba36fa1db3e09b472b271d1e0cf5aeb59ebd32f43a328

use super::{sign_request, SignedLoginMeta, Wallet};
use bevy::log::warn;
use common::util::reqwest_client;
use http::{StatusCode, Uri};

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedLoginResponse {
    pub message: Option<String>,
    pub fixed_adapter: Option<String>,
}

pub async fn signed_login(
    uri: Uri,
    wallet: Wallet,
    meta: SignedLoginMeta,
) -> Result<SignedLoginResponse, anyhow::Error> {
    let auth_chain = sign_request("post", &uri, &wallet, serde_json::to_string(&meta)?).await?;

    let mut request = reqwest_client().post(uri.to_string());

    for (key, value) in auth_chain {
        request = request.header(key, value)
    }

    let res = request.send().await?;

    if res.status() != StatusCode::OK {
        warn!("signed fetch failed: {res:#?}");
        return Err(anyhow::anyhow!("status: {}", res.status()));
    }

    res.json().await.map_err(|e| anyhow::anyhow!(e))
}
