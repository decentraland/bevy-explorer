use std::{cell::RefCell, rc::Rc};

use anyhow::anyhow;
use bevy::log::debug;
use common::{
    rpc::{RpcCall, RpcResultSender},
    structs::SceneMeta,
};
use http::Uri;
use serde::Serialize;

use crate::{
    interface::crdt_context::CrdtContext,
    js::{player_identity, runtime::realm_information, State},
    RpcCalls,
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

    let player_identity = player_identity(&*state.borrow())?;

    let urn = state.borrow().borrow::<CrdtContext>().hash.clone();

    let (sx, rx) = RpcResultSender::channel();
    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::EntityDefinition {
            urn: urn.clone(),
            response: sx,
        });

    let entity_definition = rx.await?.ok_or_else(|| anyhow!("no entity definition"))?;

    let scene_meta =
        serde_json::from_str::<SceneMeta>(&entity_definition.metadata.unwrap_or_default())?;

    let meta = SignedFetchMeta {
        origin: Some(realm_info.base_url.clone()),
        scene_id: Some(urn),
        parcel: Some(scene_meta.scene.base.clone()),
        tld: Some("org".to_owned()),
        network: Some("mainnet".to_owned()),
        is_guest: Some(player_identity.is_guest),
        realm: SignedFetchMetaRealm {
            hostname: realm_info.base_url,
            protocol: "v3".to_owned(),
            server_name: realm_info.realm_name,
        },
        signer: "decentraland-kernel-scene".to_owned(),
    };

    debug!("signed fetch meta {:?}", meta);

    let (sx, rx) = RpcResultSender::channel();

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::SignRequest {
            method: method.unwrap_or_else(|| String::from("get")),
            uri,
            meta: Some(serde_json::to_string(&meta).unwrap()),
            response: sx,
        });

    rx.await?.map_err(|e| anyhow!(e))
}
