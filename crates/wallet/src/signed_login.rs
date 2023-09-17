// https://github.com/decentraland/hammurabi/pull/33/files#diff-18afcd5f94e3688aad1ba36fa1db3e09b472b271d1e0cf5aeb59ebd32f43a328

use super::{Wallet, sign_request, SignedLoginMeta};
use bevy::prelude::warn;
use isahc::{
    http::{Method, StatusCode, Uri},
    AsyncReadResponseExt, RequestExt,
};

#[derive(Debug, serde::Deserialize)]
pub struct SignedLoginResponse {
    pub message: Option<String>,
    #[serde(rename = "fixedAdapter")]
    pub fixed_adapter: Option<String>,
}

pub async fn signed_login(
    uri: Uri,
    wallet: Wallet,
    meta: SignedLoginMeta,
) -> Result<SignedLoginResponse, anyhow::Error> {
    let auth_chain = sign_request("post", &uri, &wallet, meta).await;

    let mut builder = isahc::Request::builder().method(Method::POST).uri(uri);

    for (key, value) in auth_chain {
        builder = builder.header(key, value)
    }

    let req = builder.body(())?;
    let mut res = req.send_async().await?;

    if res.status() != StatusCode::OK {
        warn!("signed fetch failed: {res:#?}");
        return Err(anyhow::anyhow!("status: {}", res.status()));
    }

    res.json().await.map_err(|e| anyhow::anyhow!(e))
}
