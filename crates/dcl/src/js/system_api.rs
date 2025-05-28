use bevy::log::debug;
use common::inputs::{Action, BindingsData, InputIdentifier, SystemActionEvent};
use dcl_component::proto_components::{
    common::Vector2,
    sdk::components::{PbAvatarBase, PbAvatarEquippedData},
};
use deno_core::{anyhow, error::AnyError, op2, AsyncRefCell, OpDecl, OpState, ResourceId};
use http::Uri;
use ipfs::IpfsResource;
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, rc::Rc};
use system_bridge::{
    settings::{SettingInfo, Settings},
    ChatMessage, HomeScene, LiveSceneInfo, SetAvatarData, SystemApi,
};
use wallet::{sign_request, Wallet};

use super::SuperUserScene;

// list of op declarations
pub fn ops(super_user: bool) -> Vec<OpDecl> {
    if super_user {
        vec![
            op_check_for_update(),
            op_motd(),
            op_get_current_login(),
            op_get_previous_login(),
            op_login_previous(),
            op_login_new_code(),
            op_login_new_success(),
            op_login_cancel(),
            op_login_guest(),
            op_logout(),
            op_settings(),
            op_set_setting(),
            op_kernel_fetch_headers(),
            op_set_avatar(),
            op_native_input(),
            op_get_bindings(),
            op_set_bindings(),
            op_console_command(),
            op_live_scene_info(),
            op_get_home_scene(),
            op_set_home_scene(),
            op_get_realm_provider(),
            op_get_system_action_stream(),
            op_read_system_action_stream(),
            op_get_chat_stream(),
            op_read_chat_stream(),
            op_send_chat(),
        ]
    } else {
        Vec::default()
    }
}

#[op2(async)]
#[serde]
async fn op_check_for_update(state: Rc<RefCell<OpState>>) -> Result<(String, String), AnyError> {
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

#[op2(async)]
#[string]
async fn op_motd(state: Rc<RefCell<OpState>>) -> Result<String, AnyError> {
    debug!("op_motd");
    let (sx, rx) = tokio::sync::oneshot::channel();

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::MOTD(sx.into()))?;

    rx.await.map_err(|e| anyhow::anyhow!(e))
}

#[op2]
#[string]
fn op_get_current_login(state: &mut OpState) -> Option<String> {
    state
        .borrow::<Wallet>()
        .address()
        .map(|h160| format!("{h160:#x}"))
}

#[op2(async)]
#[string]
async fn op_get_previous_login(state: Rc<RefCell<OpState>>) -> Result<Option<String>, AnyError> {
    debug!("op_get_previous_login");
    let (sx, rx) = tokio::sync::oneshot::channel();

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::GetPreviousLogin(sx.into()))?;

    rx.await.map_err(|e| anyhow::anyhow!(e))
}

#[op2(async)]
#[serde]
async fn op_login_previous(state: Rc<RefCell<OpState>>) -> Result<(), AnyError> {
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
struct NewLogin {
    code: Option<tokio::sync::oneshot::Receiver<Result<Option<i32>, String>>>,
    result: Option<tokio::sync::oneshot::Receiver<Result<(), String>>>,
}

fn new_login(state: &mut OpState) -> &mut NewLogin {
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

#[op2(async)]
#[string]
async fn op_login_new_code(state: Rc<RefCell<OpState>>) -> Result<Option<String>, AnyError> {
    debug!("op_login_new_code");

    let rx = {
        let mut state = state.borrow_mut();
        let login = new_login(&mut state);
        login.code.take().unwrap()
    };

    rx.await
        .map_err(|e| anyhow::anyhow!(e))?
        .map_err(|e| anyhow::anyhow!(e))
        .map(|code| code.map(|c| format!("{c}")))
}

#[op2(async)]
#[string]
async fn op_login_new_success(state: Rc<RefCell<OpState>>) -> Result<(), AnyError> {
    debug!("op_login_new_success");

    let rx = {
        let mut state = state.borrow_mut();
        let login = new_login(&mut state);
        login.result.take().unwrap()
    };

    rx.await
        .map_err(|e| anyhow::anyhow!(e))?
        .map_err(|e| anyhow::anyhow!(e))
}

#[op2(fast)]
fn op_login_guest(state: &mut OpState) {
    debug!("op_login_guest");
    state
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::LoginGuest)
        .unwrap();
}

#[op2(fast)]
fn op_login_cancel(state: &mut OpState) {
    debug!("op_login_cancel");
    state
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::LoginCancel)
        .unwrap();
}

#[op2(fast)]
fn op_logout(state: &mut OpState) {
    debug!("op_logout");
    state
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::Logout)
        .unwrap();
}

async fn load_settings(state: Rc<RefCell<OpState>>) -> Result<(), AnyError> {
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

#[op2(async)]
#[serde]
async fn op_settings(state: Rc<RefCell<OpState>>) -> Result<Vec<SettingInfo>, AnyError> {
    debug!("op_settings");
    load_settings(state.clone()).await?;
    Ok(state.borrow().borrow::<Settings>().get())
}

#[op2(async)]
#[serde]
async fn op_set_setting(
    state: Rc<RefCell<OpState>>,
    #[string] name: String,
    val: f32,
) -> Result<(), AnyError> {
    debug!("op_set_setting");
    load_settings(state.clone()).await?;
    state
        .borrow_mut()
        .borrow_mut::<Settings>()
        .set_value(&name, val)
}

#[op2(async)]
#[serde]
pub async fn op_kernel_fetch_headers(
    state: Rc<RefCell<OpState>>,
    #[string] uri: String,
    #[string] method: Option<String>,
    #[string] meta: Option<String>,
) -> Result<Vec<(String, String)>, AnyError> {
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

#[op2(async)]
pub async fn op_set_avatar(
    state: Rc<RefCell<OpState>>,
    #[serde] base: Option<PbAvatarBase>,
    #[serde] equip: Option<PbAvatarEquippedData>,
    has_claimed_name: Option<bool>,
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
            },
            sx.into(),
        ))?;

    rx.await?.map_err(|e| anyhow::anyhow!(e))
}

#[op2(async)]
#[string]
pub async fn op_native_input(state: Rc<RefCell<OpState>>) -> String {
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

#[op2(async)]
#[serde]
pub async fn op_get_bindings(state: Rc<RefCell<OpState>>) -> Result<JsBindingsData, anyhow::Error> {
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

#[op2(async)]
#[serde]
pub async fn op_set_bindings(
    state: Rc<RefCell<OpState>>,
    #[serde] bindings: JsBindingsData,
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

#[op2(async)]
#[string]
pub async fn op_console_command(
    state: Rc<RefCell<OpState>>,
    #[string] cmd: String,
    #[serde] args: Vec<String>,
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

#[op2(async)]
#[serde]
pub async fn op_live_scene_info(
    state: Rc<RefCell<OpState>>,
) -> Result<Vec<LiveSceneInfo>, anyhow::Error> {
    let (sx, rx) = tokio::sync::oneshot::channel();

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::LiveSceneInfo(sx.into()))
        .unwrap();

    rx.await.map_err(|e| anyhow::anyhow!(e))
}

#[op2(async)]
#[serde]
pub async fn op_get_home_scene(state: Rc<RefCell<OpState>>) -> Result<HomeScene, anyhow::Error> {
    let (sx, rx) = tokio::sync::oneshot::channel();

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::GetHomeScene(sx.into()))
        .unwrap();

    rx.await.map_err(|e| anyhow::anyhow!(e))
}

#[op2]
pub fn op_set_home_scene(
    state: Rc<RefCell<OpState>>,
    #[string] realm: String,
    #[serde] parcel: Vector2,
) {
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

#[op2(async)]
#[serde]
pub async fn op_get_realm_provider(
    state: Rc<RefCell<OpState>>,
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

pub struct StreamResource<T: 'static> {
    receiver: Rc<AsyncRefCell<tokio::sync::mpsc::UnboundedReceiver<T>>>,
}

impl<T: 'static> deno_core::Resource for StreamResource<T> {}

#[op2(async)]
#[serde]
pub async fn op_get_system_action_stream(state: Rc<RefCell<OpState>>) -> deno_core::ResourceId {
    let (sx, rx) = tokio::sync::mpsc::unbounded_channel();
    let rid = state.borrow_mut().resource_table.add(StreamResource {
        receiver: Rc::new(AsyncRefCell::new(rx)),
    });

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::GetSystemActionStream(sx))
        .unwrap();

    rid
}

#[op2(async)]
#[serde]
pub async fn op_read_system_action_stream(
    state: Rc<RefCell<OpState>>,
    #[serde] rid: ResourceId,
) -> Result<Option<SystemActionEvent>, deno_core::anyhow::Error> {
    let receiver = {
        let Ok(state) = state.try_borrow() else {
            return Ok(None);
        };

        let resource = state
            .resource_table
            .get::<StreamResource<SystemActionEvent>>(rid)?;
        resource.receiver.clone()
    };

    let mut rx = receiver.borrow_mut().await;

    let res = match rx.recv().await {
        Some(data) => Ok(Some(data)),
        None => Ok(None),
    };

    res
}

#[op2(async)]
#[serde]
pub async fn op_get_chat_stream(state: Rc<RefCell<OpState>>) -> deno_core::ResourceId {
    let (sx, rx) = tokio::sync::mpsc::unbounded_channel();
    let rid = state.borrow_mut().resource_table.add(StreamResource {
        receiver: Rc::new(AsyncRefCell::new(rx)),
    });

    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::GetChatStream(sx))
        .unwrap();

    rid
}

#[op2(async)]
#[serde]
pub async fn op_read_chat_stream(
    state: Rc<RefCell<OpState>>,
    #[serde] rid: ResourceId,
) -> Result<Option<ChatMessage>, deno_core::anyhow::Error> {
    let receiver = {
        let Ok(state) = state.try_borrow() else {
            return Ok(None);
        };

        let resource = state
            .resource_table
            .get::<StreamResource<ChatMessage>>(rid)?;
        resource.receiver.clone()
    };

    let mut rx = receiver.borrow_mut().await;

    let res = match rx.recv().await {
        Some(data) => Ok(Some(data)),
        None => Ok(None),
    };

    res
}

#[op2(fast)]
pub fn op_send_chat(
    state: Rc<RefCell<OpState>>,
    #[string] message: String,
    #[string] channel: String,
) {
    state
        .borrow_mut()
        .borrow_mut::<SuperUserScene>()
        .send(SystemApi::SendChat(message, channel))
        .unwrap();
}
