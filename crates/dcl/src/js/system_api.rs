use bevy::log::debug;
use common::{
    inputs::{Action, BindingsData, InputIdentifier, SystemActionEvent},
    profile::SerializedProfile,
    rpc::RpcCall,
};
use dcl_component::proto_components::{
    common::Vector2,
    sdk::components::{PbAvatarBase, PbAvatarEquippedData},
};
use http::Uri;
use ipfs::IpfsResource;
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, rc::Rc};
use system_bridge::{
    settings::{SettingInfo, Settings},
    ChatMessage, HomeScene, LiveSceneInfo, SetAvatarData, SystemApi,
};
use tokio::sync::mpsc::UnboundedReceiver;
use wallet::{sign_request, Wallet};

use crate::{interface::crdt_context::CrdtContext, RpcCalls};

use super::{State, SuperUserScene};

pub async fn op_check_for_update(
    state: Rc<RefCell<impl State>>,
) -> Result<(String, String), anyhow::Error> {
    debug!("op_check_for_update");
    let (sx, rx) = tokio::sync::oneshot::channel();

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::CheckForUpdate(sx.into()))?;

    Ok(rx
        .await
        .map_err(|e| anyhow::anyhow!(e))?
        .unwrap_or_default())
}

pub async fn op_motd(state: Rc<RefCell<impl State>>) -> Result<String, anyhow::Error> {
    debug!("op_motd");
    let (sx, rx) = tokio::sync::oneshot::channel();

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::MOTD(sx.into()))?;

    rx.await.map_err(|e| anyhow::anyhow!(e))
}

pub fn op_get_current_login(state: &mut impl State) -> Option<String> {
    state
        .borrow::<Wallet>()
        .address()
        .map(|h160| format!("{h160:#x}"))
}

pub async fn op_get_previous_login(
    state: Rc<RefCell<impl State>>,
) -> Result<Option<String>, anyhow::Error> {
    debug!("op_get_previous_login");
    let (sx, rx) = tokio::sync::oneshot::channel();

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::GetPreviousLogin(sx.into()))?;

    rx.await.map_err(|e| anyhow::anyhow!(e))
}

pub async fn op_login_previous(state: Rc<RefCell<impl State>>) -> Result<(), anyhow::Error> {
    debug!("op_login_previous");
    let (sx, rx) = tokio::sync::oneshot::channel();

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::LoginPrevious(sx.into()))?;

    rx.await
        .map_err(|e| anyhow::anyhow!(e))?
        .map_err(|e| anyhow::anyhow!(e))
}

#[derive(Default)]
pub struct NewLogin {
    code: Option<tokio::sync::oneshot::Receiver<Result<Option<i32>, String>>>,
    result: Option<tokio::sync::oneshot::Receiver<Result<(), String>>>,
}

pub fn new_login(state: &mut impl State) -> &mut NewLogin {
    if !state.has::<NewLogin>() {
        state.put(NewLogin::default());
    }

    let mut login = state.take::<NewLogin>();

    if login.code.is_none() && login.result.is_none() {
        let (sc, code) = tokio::sync::oneshot::channel();
        let (sx, result) = tokio::sync::oneshot::channel();
        state
            .borrow_mut::<SuperUserScene>()
            .send(SystemApi::LoginNew(sc.into(), sx.into()))
            .unwrap();

        login.code = Some(code);
        login.result = Some(result);
    }

    state.put(login);
    state.borrow_mut()
}

pub async fn op_login_new_code(
    state: Rc<RefCell<impl State>>,
) -> Result<Option<String>, anyhow::Error> {
    debug!("op_login_new_code");

    let rx = {
        let mut state = state.borrow_mut();
        let login = new_login(&mut *state);
        login.code.take().unwrap()
    };

    rx.await
        .map_err(|e| anyhow::anyhow!(e))?
        .map_err(|e| anyhow::anyhow!(e))
        .map(|code| code.map(|c| format!("{c}")))
}

pub async fn op_login_new_success(state: Rc<RefCell<impl State>>) -> Result<(), anyhow::Error> {
    debug!("op_login_new_success");

    let rx = {
        let mut state = state.borrow_mut();
        let login = new_login(&mut *state);
        login.result.take().unwrap()
    };

    rx.await
        .map_err(|e| anyhow::anyhow!(e))?
        .map_err(|e| anyhow::anyhow!(e))
}

pub fn op_login_guest(state: &mut impl State) {
    debug!("op_login_guest");
    state
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::LoginGuest)
        .unwrap();
}

pub fn op_login_cancel(state: &mut impl State) {
    debug!("op_login_cancel");
    state
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::LoginCancel)
        .unwrap();
}

pub fn op_logout(state: &mut impl State) {
    debug!("op_logout");
    state
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::Logout)
        .unwrap();
}

pub async fn load_settings(state: Rc<RefCell<impl State>>) -> Result<(), anyhow::Error> {
    if !state.borrow().has::<Settings>() {
        let (sx, rx) = tokio::sync::oneshot::channel();

        state
            .borrow_mut()
            .borrow_mut::<SuperUserScene>()
            .send(SystemApi::GetSettings(sx.into()))?;

        let settings = rx.await.map_err(|e| anyhow::anyhow!(e))?;
        state.borrow_mut().put(settings);
    }

    Ok(())
}

pub async fn op_settings(
    state: Rc<RefCell<impl State>>,
) -> Result<Vec<SettingInfo>, anyhow::Error> {
    debug!("op_settings");
    load_settings(state.clone()).await?;
    Ok(state.borrow().borrow::<Settings>().get())
}

pub async fn op_set_setting(
    state: Rc<RefCell<impl State>>,
    name: String,
    val: f32,
) -> Result<(), anyhow::Error> {
    debug!("op_set_setting");
    load_settings(state.clone()).await?;
    state
        .borrow_mut()
        .borrow_mut::<Settings>()
        .set_value(&name, val)
}

pub async fn op_kernel_fetch_headers(
    state: Rc<RefCell<impl State>>,
    uri: String,
    method: Option<String>,
    meta: Option<String>,
) -> Result<Vec<(String, String)>, anyhow::Error> {
    debug!("op_kernel_fetch_headers");

    let wallet = state.borrow().borrow::<Wallet>().clone();

    if let Some(meta) = meta {
        let meta: serde_json::Value = serde_json::from_str(&meta)?;

        sign_request(
            method.as_deref().unwrap_or("get"),
            &Uri::try_from(uri)?,
            &wallet,
            meta,
        )
        .await
    } else {
        sign_request(
            method.as_deref().unwrap_or("get"),
            &Uri::try_from(uri)?,
            &wallet,
            (),
        )
        .await
    }
}

pub async fn op_set_avatar(
    state: Rc<RefCell<impl State>>,
    base: Option<PbAvatarBase>,
    equip: Option<PbAvatarEquippedData>,
    has_claimed_name: Option<bool>,
    profile_extras: Option<std::collections::HashMap<String, serde_json::Value>>,
) -> Result<u32, anyhow::Error> {
    let (sx, rx) = tokio::sync::oneshot::channel();

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::SetAvatar(
            SetAvatarData {
                base,
                equip,
                has_claimed_name,
                profile_extras,
            },
            sx.into(),
        ))?;

    rx.await?.map_err(|e| anyhow::anyhow!(e))
}

pub async fn op_native_input(state: Rc<RefCell<impl State>>) -> String {
    let (sx, rx) = tokio::sync::oneshot::channel();

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::GetNativeInput(sx.into()))
        .unwrap();

    let identifier = rx.await.unwrap();
    serde_json::to_string(&identifier)
        .unwrap()
        .strip_prefix("\"")
        .unwrap()
        .strip_suffix("\"")
        .unwrap()
        .to_owned()
}

#[derive(Serialize, Deserialize)]
pub struct JsBindingsData {
    bindings: Vec<(Action, Vec<InputIdentifier>)>,
}

pub async fn op_get_bindings(
    state: Rc<RefCell<impl State>>,
) -> Result<JsBindingsData, anyhow::Error> {
    let (sx, rx) = tokio::sync::oneshot::channel();

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::GetBindings(sx.into()))
        .unwrap();

    rx.await.map_err(|e| anyhow::anyhow!(e)).map(|bd| {
        let mut bindings: Vec<_> = bd.bindings.into_iter().collect();
        bindings.sort_by_key(|k| k.0);
        JsBindingsData { bindings }
    })
}

pub async fn op_set_bindings(
    state: Rc<RefCell<impl State>>,
    bindings: JsBindingsData,
) -> Result<(), anyhow::Error> {
    let (sx, rx) = tokio::sync::oneshot::channel();

    let bindings = BindingsData {
        bindings: bindings.bindings.into_iter().collect(),
    };

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::SetBindings(bindings, sx.into()))
        .unwrap();

    rx.await.map_err(|e| anyhow::anyhow!(e))
}

pub async fn op_console_command(
    state: Rc<RefCell<impl State>>,
    cmd: String,
    args: Vec<String>,
) -> Result<String, anyhow::Error> {
    let (sx, rx) = tokio::sync::oneshot::channel();

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::ConsoleCommand(
            format!("/{cmd}"),
            args,
            sx.into(),
        ))
        .unwrap();

    rx.await
        .map_err(|e| anyhow::anyhow!(e))?
        .map_err(|e| anyhow::anyhow!(e))
}

pub async fn op_live_scene_info(
    state: Rc<RefCell<impl State>>,
) -> Result<Vec<LiveSceneInfo>, anyhow::Error> {
    let (sx, rx) = tokio::sync::oneshot::channel();

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::LiveSceneInfo(sx.into()))
        .unwrap();

    rx.await.map_err(|e| anyhow::anyhow!(e))
}

pub async fn op_get_home_scene(state: Rc<RefCell<impl State>>) -> Result<HomeScene, anyhow::Error> {
    let (sx, rx) = tokio::sync::oneshot::channel();

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::GetHomeScene(sx.into()))
        .unwrap();

    rx.await.map_err(|e| anyhow::anyhow!(e))
}

pub fn op_set_home_scene(state: Rc<RefCell<impl State>>, realm: String, parcel: Vector2) {
    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::SetHomeScene(HomeScene { realm, parcel }))
        .unwrap();
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RealmProviderString {
    realm: String,
}

pub async fn op_get_realm_provider(
    state: Rc<RefCell<impl State>>,
) -> Result<RealmProviderString, anyhow::Error> {
    let url = state
        .borrow_mut()
        .borrow_mut::<IpfsResource>()
        .about_url()
        .ok_or(anyhow::anyhow!("not connected"))?;

    let url = url.strip_suffix("/about").unwrap_or(&url);

    Ok(RealmProviderString {
        realm: url.to_owned(),
    })
}

pub async fn op_get_system_action_stream(state: Rc<RefCell<impl State>>) -> u32 {
    let (sx, rx) = tokio::sync::mpsc::unbounded_channel();
    state.borrow_mut().put(rx);

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::GetSystemActionStream(sx))
        .unwrap();

    1
}

pub async fn op_read_system_action_stream(
    state: Rc<RefCell<impl State>>,
    _rid: u32,
) -> Result<Option<SystemActionEvent>, anyhow::Error> {
    let Some(mut receiver) = state
        .borrow_mut()
        .try_take::<UnboundedReceiver<SystemActionEvent>>()
    else {
        return Ok(None);
    };

    let res = match receiver.recv().await {
        Some(data) => Ok(Some(data)),
        None => Ok(None),
    };

    state.borrow_mut().put(receiver);

    res
}

pub async fn op_get_chat_stream(state: Rc<RefCell<impl State>>) -> u32 {
    let (sx, rx) = tokio::sync::mpsc::unbounded_channel();
    state.borrow_mut().put(rx);

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::GetChatStream(sx))
        .unwrap();

    2
}

pub async fn op_read_chat_stream(
    state: Rc<RefCell<impl State>>,
    _rid: u32,
) -> Result<Option<ChatMessage>, anyhow::Error> {
    let Some(mut receiver) = state
        .borrow_mut()
        .try_take::<UnboundedReceiver<ChatMessage>>()
    else {
        return Ok(None);
    };

    let res = match receiver.recv().await {
        Some(data) => Ok(Some(data)),
        None => Ok(None),
    };

    state.borrow_mut().put(receiver);

    res
}

pub fn op_send_chat(state: Rc<RefCell<impl State>>, message: String, channel: String) {
    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::SendChat(message, channel))
        .unwrap();
}

pub async fn op_get_profile_extras(
    state: Rc<RefCell<impl State>>,
) -> Result<std::collections::HashMap<String, serde_json::Value>, anyhow::Error> {
    let (sx, rx) = tokio::sync::oneshot::channel::<Result<SerializedProfile, ()>>();

    let scene = state.borrow().borrow::<CrdtContext>().scene_id.0;
    debug!("[{scene:?}] -> op_get_profile_extras");

    state
        .borrow_mut()
        .borrow_mut::<RpcCalls>()
        .push(RpcCall::GetUserData {
            user: None, // current user
            scene,
            response: sx.into(),
        });

    let profile = rx
        .await
        .map_err(|e| anyhow::anyhow!(e))?
        .map_err(|_| anyhow::anyhow!("Not found"))?;

    Ok(profile.extra_fields)
}

pub fn op_quit(state: Rc<RefCell<impl State>>) {
    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::Quit)
        .unwrap();
}
