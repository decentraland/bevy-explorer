use std::{cell::RefCell, rc::Rc};

use common::{profile::SerializedProfile, rpc::RpcCall};
use deno_core::{anyhow, error::AnyError, op, Op, OpDecl, OpState};
use serde::Serialize;

use crate::RpcCalls;

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![op_get_user_data::DECL, op_get_player_data::DECL]
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshots {
    face256: String,
    body: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AvatarForUserData {
    body_shape: String,
    skin_color: String,
    hair_color: String,
    eye_color: String,
    wearables: Vec<String>,
    snapshots: Option<Snapshots>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UserData {
    display_name: String,
    public_key: Option<String>,
    has_connected_web3: bool,
    user_id: String,
    version: i64,
    avatar: Option<AvatarForUserData>,
}

#[op]
async fn op_get_user_data(state: Rc<RefCell<OpState>>) -> Result<UserData, AnyError> {
    let (sx, rx) = tokio::sync::oneshot::channel::<Result<SerializedProfile, ()>>();

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::GetUserData {
            user: None, // current user
            response: sx.into(),
        });

    rx.await
        .map_err(|e| anyhow::anyhow!(e))?
        .map(Into::into)
        .map_err(|_| anyhow::anyhow!("Not found"))
}

#[op]
async fn op_get_player_data(state: Rc<RefCell<OpState>>, id: String) -> Result<UserData, AnyError> {
    let (sx, rx) = tokio::sync::oneshot::channel::<Result<SerializedProfile, ()>>();

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::GetUserData {
            user: Some(id),
            response: sx.into(),
        });

    rx.await
        .map_err(|e| anyhow::anyhow!(e))?
        .map(Into::into)
        .map_err(|_| anyhow::anyhow!("Not found"))
}

impl From<SerializedProfile> for UserData {
    fn from(profile: SerializedProfile) -> Self {
        Self {
            display_name: profile.name,
            public_key: Some(profile.eth_address.clone()),
            has_connected_web3: profile.has_connected_web3.unwrap_or_default(),
            user_id: profile.user_id.unwrap_or_default(),
            version: profile.version,
            avatar: Some(AvatarForUserData {
                body_shape: profile.avatar.body_shape.unwrap_or_default(),
                skin_color: serde_json::to_string(&profile.avatar.skin.unwrap_or_default())
                    .unwrap(),
                hair_color: serde_json::to_string(&profile.avatar.hair.unwrap_or_default())
                    .unwrap(),
                eye_color: serde_json::to_string(&profile.avatar.eyes.unwrap_or_default()).unwrap(),
                wearables: profile.avatar.wearables,
                snapshots: profile.avatar.snapshots.map(|s| Snapshots {
                    face256: s.face256,
                    body: s.body,
                }),
            }),
        }
    }
}
