use std::{cell::RefCell, rc::Rc};

use anyhow::anyhow;
use bevy::log::debug;
use common::structs::SceneMeta;
use http::Uri;
use ipfs::IpfsResource;
use serde::Serialize;
use wallet::{sign_request, Wallet};

use crate::{
    interface::crdt_context::CrdtContext,
    js::{runtime::realm_information, State},
};

#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SignedFetchMetaRealm {
    hostname: String,
    protocol: String,
    server_name: String,
}

#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SignedFetchMeta {
    origin: Option<String>,
    scene_id: Option<String>,
    parcel: Option<String>,
    tld: Option<String>,
    network: Option<String>,
    is_guest: Option<bool>,
    realm: SignedFetchMetaRealm,
    signer: String,
}

pub async fn op_signed_fetch_headers(
    state: Rc<RefCell<impl State>>,
    uri: String,
    method: Option<String>,
) -> Result<Vec<(String, String)>, anyhow::Error> {
    debug!("op_signed_fetch_headers");

    let is_preview = state.borrow().borrow::<CrdtContext>().preview;
    let scheme = Uri::try_from(&uri)?;
    let scheme = scheme.scheme_str();
    if !is_preview && !([Some("https"), Some("wss")].contains(&scheme)) {
        anyhow::bail!("URL scheme must be `https` (request `{}`)", uri);
    }

    let realm_info = realm_information(state.clone()).await?;
    let wallet = state.borrow().borrow::<Wallet>().clone();
    let urn = state.borrow().borrow::<CrdtContext>().hash.clone();
    let ipfs = state.borrow().borrow::<IpfsResource>().clone();
    let scene_meta = ipfs
        .entity_definition(&urn)
        .await
        .and_then(|(entity, _)| {
            serde_json::from_str::<SceneMeta>(&entity.metadata.unwrap_or_default()).ok()
        })
        .ok_or(anyhow!("failed to parse scene metadata"))?;

    let meta = SignedFetchMeta {
        origin: Some(realm_info.base_url.clone()),
        scene_id: Some(urn),
        parcel: Some(scene_meta.scene.base.clone()),
        tld: Some("org".to_owned()),
        network: Some("mainnet".to_owned()),
        is_guest: Some(wallet.is_guest()),
        realm: SignedFetchMetaRealm {
            hostname: realm_info.base_url,
            protocol: "v3".to_owned(),
            server_name: realm_info.realm_name,
        },
        signer: "decentraland-kernel-scene".to_owned(),
    };

    debug!("signed fetch meta {:?}", meta);

    sign_request(
        method.as_deref().unwrap_or("get"),
        &Uri::try_from(uri)?,
        &wallet,
        meta,
    )
    .await
}
