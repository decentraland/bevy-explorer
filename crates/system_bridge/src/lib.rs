pub mod agent_commands;
pub mod settings;

use std::collections::{HashMap, VecDeque};

use bevy::{
    app::{AppExit, Plugin, Update},
    ecs::{event::EventReader, system::Local},
    log::debug,
    math::Vec4,
    prelude::{Event, EventWriter, Res, ResMut, Resource},
};
use bevy_console::{ConsoleCommandEntered, ConsoleConfiguration, PrintConsoleLine};
use common::{
    inputs::{BindingsData, InputIdentifier, SystemActionEvent},
    rpc::{RpcResultSender, RpcStreamSender},
    structs::{
        AppConfig, MicState, PermissionLevel, PermissionType, PermissionUsed, PermissionValue,
    },
};
use dcl_component::proto_components::{
    common::Vector2,
    sdk::components::{pb_pointer_events, PbAvatarBase, PbAvatarEquippedData},
};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use settings::SettingBridgePlugin;

use crate::settings::SettingInfo;

pub struct SystemBridgePlugin {
    pub bare: bool,
}

impl Plugin for SystemBridgePlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_event::<SystemApi>();
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        app.insert_resource(SystemBridge { sender, receiver });
        app.init_resource::<SceneParams>();
        app.add_systems(
            Update,
            (
                post_events,
                handle_home_scene,
                handle_exit,
                handle_get_params,
                handle_file_pickers,
            ),
        );

        if self.bare {
            return;
        }

        app.add_plugins((SettingBridgePlugin, agent_commands::AgentCommandsPlugin));
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SetAvatarData {
    pub base: Option<PbAvatarBase>,
    pub equip: Option<PbAvatarEquippedData>,
    pub has_claimed_name: Option<bool>,
    pub profile_extras: Option<std::collections::HashMap<String, serde_json::Value>>,
}

/// One filter group for the native file picker (e.g. Images / .png .jpg).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PickFileFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PickFileOptions {
    pub title: Option<String>,
    pub filters: Vec<PickFileFilter>,
}

/// File returned by the native picker. `bytes` is base64-encoded so the value
/// crosses the deno boundary as a `string` (we don't have a binary path yet
/// for op return values).
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PickedFile {
    pub name: String,
    pub mime: String,
    pub size: usize,
    /// Base64 of the raw file contents.
    pub bytes_base64: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LiveSceneInfo {
    pub hash: String,
    pub base_url: Option<String>,
    pub title: String,
    pub parcels: Vec<Vector2>,
    pub is_portable: bool,
    pub is_broken: bool,
    pub is_blocked: bool,
    pub is_super: bool,
    pub sdk_version: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HomeScene {
    pub realm: String,
    pub parcel: Vector2,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub sender_address: String,
    pub message: String,
    pub channel: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct VoiceMessage {
    pub sender_address: String,
    pub channel: String,
    pub active: bool,
}

#[derive(Hash, Clone, Copy, Serialize_repr, Deserialize_repr, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PointerTargetType {
    World = 0,
    Ui = 1,
    Avatar = 2,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HoverAction {
    #[serde(flatten)]
    pub event: pb_pointer_events::Entry,
    pub enabled: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HoverEvent {
    pub entered: bool,
    pub target_type: PointerTargetType,
    pub actions: Vec<HoverAction>,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneLoadingUi {
    pub visible: bool,
    pub title: String,
    pub pending_assets: Option<u32>,
}

#[derive(Event, Clone, Debug, Serialize, Deserialize)]
pub enum SystemApi {
    ConsoleCommand(String, Vec<String>, RpcResultSender<Result<String, String>>),
    CheckForUpdate(RpcResultSender<Option<(String, String)>>),
    MOTD(RpcResultSender<String>),
    GetPreviousLogin(RpcResultSender<Option<String>>),
    LoginPrevious(RpcResultSender<Result<(), String>>),
    LoginNew(
        RpcResultSender<Result<Option<i32>, String>>,
        RpcResultSender<Result<(), String>>,
    ),
    LoginGuest,
    LoginCancel,
    Logout,
    GetSettings(RpcResultSender<Vec<SettingInfo>>),
    SetSetting(String, f32),
    SetAvatar(SetAvatarData, RpcResultSender<Result<u32, String>>),
    GetNativeInput(RpcResultSender<InputIdentifier>),
    GetBindings(RpcResultSender<BindingsData>),
    SetBindings(BindingsData, RpcResultSender<()>),
    LiveSceneInfo(RpcResultSender<Vec<LiveSceneInfo>>),
    GetHomeScene(RpcResultSender<HomeScene>),
    SetHomeScene(HomeScene),
    GetSystemActionStream(RpcStreamSender<SystemActionEvent>),
    GetChatStream(RpcStreamSender<ChatMessage>),
    GetVoiceStream(RpcStreamSender<VoiceMessage>),
    GetHoverStream(RpcStreamSender<HoverEvent>),
    GetSceneLoadingUiStream(RpcStreamSender<SceneLoadingUi>),
    SendChat(String, String),
    Quit,
    GetPermissionRequestStream(RpcStreamSender<PermissionRequest>),
    SetSinglePermission(SetSinglePermission),
    SetPermanentPermission(SetPermanentPermission),
    GetPermissionUsedStream(RpcStreamSender<PermissionUsed>),
    GetPermanentPermissions(
        PermissionLevel,
        RpcResultSender<Vec<PermanentPermissionItem>>,
    ),
    SetInteractableArea(Vec4),
    GetMicState(RpcResultSender<MicState>),
    SetMicEnabled(bool),
    GetAvatarModifiers(RpcResultSender<Vec<AvatarModifierState>>),
    // Social / Friends
    GetFriends(RpcResultSender<Vec<FriendData>>),
    GetSentFriendRequests(RpcResultSender<Vec<FriendRequestData>>),
    GetReceivedFriendRequests(RpcResultSender<Vec<FriendRequestData>>),
    GetSocialInitialized(RpcResultSender<bool>),
    SendFriendRequest(String, Option<String>, RpcResultSender<Result<(), String>>),
    AcceptFriendRequest(String, RpcResultSender<Result<(), String>>),
    RejectFriendRequest(String, RpcResultSender<Result<(), String>>),
    CancelFriendRequest(String, RpcResultSender<Result<(), String>>),
    DeleteFriend(String, RpcResultSender<Result<(), String>>),
    GetFriendshipEventStream(RpcStreamSender<FriendshipEventUpdate>),
    GetMutualFriends(String, RpcResultSender<Vec<FriendData>>),
    GetOnlineFriends(RpcResultSender<Vec<FriendStatusData>>),
    GetFriendConnectivityStream(RpcStreamSender<FriendConnectivityEvent>),
    // Social / Blocking
    BlockUser(String, RpcResultSender<Result<(), String>>),
    UnblockUser(String, RpcResultSender<Result<(), String>>),
    GetBlockedUsers(RpcResultSender<Vec<BlockedUserData>>),
    GetParams(RpcResultSender<HashMap<String, String>>),
    /// Ask the OS to open a native single-file picker. Resolves to `None`
    /// when the user cancels.
    PickFile(
        PickFileOptions,
        RpcResultSender<Result<Option<PickedFile>, String>>,
    ),
    /// Same as `PickFile` but multi-select. Resolves to an empty vec when the
    /// user cancels.
    PickFiles(
        PickFileOptions,
        RpcResultSender<Result<Vec<PickedFile>, String>>,
    ),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AvatarModifierState {
    pub user_id: String,
    pub hide_avatar: bool,
    pub hide_profile: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NameColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendData {
    pub address: String,
    pub name: String,
    pub has_claimed_name: bool,
    pub profile_picture_url: String,
    pub name_color: Option<NameColor>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendRequestData {
    pub address: String,
    pub name: String,
    pub has_claimed_name: bool,
    pub profile_picture_url: String,
    pub name_color: Option<NameColor>,
    pub created_at: i64,
    pub message: Option<String>,
    pub id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all_fields = "camelCase")]
#[serde(tag = "type")]
pub enum FriendshipEventUpdate {
    #[serde(rename = "request")]
    Request {
        address: String,
        name: String,
        has_claimed_name: bool,
        profile_picture_url: String,
        name_color: Option<NameColor>,
        created_at: i64,
        message: Option<String>,
        id: String,
    },
    #[serde(rename = "accept")]
    Accept { address: String },
    #[serde(rename = "reject")]
    Reject { address: String },
    #[serde(rename = "cancel")]
    Cancel { address: String },
    #[serde(rename = "delete")]
    Delete { address: String },
    #[serde(rename = "block")]
    Block { address: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendStatusData {
    pub address: String,
    pub name: String,
    pub has_claimed_name: bool,
    pub profile_picture_url: String,
    pub name_color: Option<NameColor>,
    /// "online", "offline", or "away"
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedUserData {
    pub address: String,
    pub name: String,
    pub has_claimed_name: bool,
    pub profile_picture_url: String,
    pub name_color: Option<NameColor>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendConnectivityEvent {
    pub address: String,
    pub name: String,
    pub has_claimed_name: bool,
    pub profile_picture_url: String,
    pub name_color: Option<NameColor>,
    /// "online", "offline", or "away"
    pub status: String,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct SceneParams(pub HashMap<String, String>);

impl SceneParams {
    pub fn from_query_string(query: &str, decode: bool) -> Self {
        let map = query
            .split('&')
            .filter(|s| !s.is_empty())
            .filter_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                let key = parts.next()?.to_owned();
                let value = parts.next().unwrap_or("").to_owned();
                if decode {
                    Some((
                        urlencoding::decode(&key).unwrap_or_default().into_owned(),
                        urlencoding::decode(&value).unwrap_or_default().into_owned(),
                    ))
                } else {
                    Some((key, value))
                }
            })
            .collect();
        Self(map)
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PermanentPermissionItem {
    pub ty: PermissionType,
    pub allow: PermissionValue,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub ty: PermissionType,
    pub additional: Option<String>,
    pub scene: String,
    pub id: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetSinglePermission {
    pub id: usize,
    pub allow: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetPermanentPermission {
    pub ty: PermissionType,
    pub level: PermissionLevel,
    pub allow: Option<PermissionValue>,
}

#[derive(Resource)]
pub struct NativeUi {
    pub login: bool,
    pub emote_wheel: bool,
    pub chat: bool,
    pub permissions: bool,
    pub profile: bool,
    pub nametags: bool,
    pub tooltips: bool,
    pub loading_scene: bool,
}

#[derive(Resource)]
pub struct SystemBridge {
    pub sender: tokio::sync::mpsc::UnboundedSender<SystemApi>,
    receiver: tokio::sync::mpsc::UnboundedReceiver<SystemApi>,
}

pub fn post_events(
    mut bridge: ResMut<SystemBridge>,
    mut writer: EventWriter<SystemApi>,
    mut console: EventWriter<ConsoleCommandEntered>,
    mut console_response: Local<Option<RpcResultSender<Result<String, String>>>>,
    mut replies: EventReader<PrintConsoleLine>,
    mut pending: Local<VecDeque<(String, Vec<String>, RpcResultSender<Result<String, String>>)>>,
    console_config: Res<ConsoleConfiguration>,
) {
    while let Ok(ev) = bridge.receiver.try_recv() {
        if let SystemApi::ConsoleCommand(cmd, args, sender) = ev {
            debug!("system bridge (cc): {cmd} {args:?}");
            pending.push_back((cmd, args, sender));
        } else {
            debug!("system bridge: {ev:?}");
            writer.write(ev);
        }
    }

    if let Some(response) = console_response.take() {
        let mut reply = replies.read().collect::<Vec<_>>();
        match reply.pop() {
            Some(PrintConsoleLine { line }) if line.as_str() == "[ok]" => {
                response.send(Ok(reply
                    .into_iter()
                    .map(|l| l.line.clone())
                    .collect::<Vec<_>>()
                    .join("\n")));
            }
            Some(PrintConsoleLine { line }) if line.as_str() == "[failed]" => {
                response.send(Err(reply
                    .into_iter()
                    .map(|l| l.line.clone())
                    .collect::<Vec<_>>()
                    .join("\n")));
            }
            Some(PrintConsoleLine { line }) => {
                debug!("got {line}");
                *console_response = Some(response);
            }
            _ => {
                *console_response = Some(response);
            }
        }
    } else if let Some((cmd, args, sender)) = pending.pop_front() {
        if console_config.commands.contains_key(cmd.as_str()) {
            console.write(ConsoleCommandEntered {
                command_name: cmd,
                args,
            });
            *console_response = Some(sender);
        } else {
            sender.send(Err(format!(
                "Command not recognized: `{cmd}`. Recognized commands: {:?}",
                console_config.commands.keys().collect::<Vec<_>>()
            )));
        }
    }

    replies.clear();
}

fn handle_home_scene(mut ev: EventReader<SystemApi>, mut config: ResMut<AppConfig>) {
    for ev in ev.read() {
        match ev {
            SystemApi::GetHomeScene(rpc_result_sender) => rpc_result_sender.send(HomeScene {
                realm: config.server.clone(),
                parcel: config.location.as_vec2().into(),
            }),
            SystemApi::SetHomeScene(home_scene) => {
                config.server = home_scene.realm.clone();
                config.location = bevy::math::Vec2::from(&home_scene.parcel).as_ivec2();
                platform::write_config_file(&*config);
            }
            _ => (),
        }
    }
}

fn handle_exit(mut ev: EventReader<SystemApi>, mut exit: EventWriter<AppExit>) {
    if ev
        .read()
        .filter(|e| matches!(e, SystemApi::Quit))
        .last()
        .is_some()
    {
        exit.write_default();
    }
}

fn handle_get_params(mut ev: EventReader<SystemApi>, params: Res<SceneParams>) {
    for ev in ev.read() {
        if let SystemApi::GetParams(sender) = ev {
            sender.send(params.0.clone());
        }
    }
}

fn handle_file_pickers(mut ev: EventReader<SystemApi>) {
    for ev in ev.read().cloned() {
        match ev {
            SystemApi::PickFile(options, sender) => {
                spawn_pick_file(options, sender, false);
            }
            SystemApi::PickFiles(options, sender) => {
                spawn_pick_files(options, sender);
            }
            _ => (),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_pick_file(
    options: PickFileOptions,
    sender: RpcResultSender<Result<Option<PickedFile>, String>>,
    _multi: bool,
) {
    bevy::tasks::IoTaskPool::get()
        .spawn(async move {
            let result = async {
                let mut dialog = rfd::AsyncFileDialog::new();
                if let Some(title) = options.title.as_deref() {
                    dialog = dialog.set_title(title);
                }
                for filter in &options.filters {
                    let exts: Vec<&str> =
                        filter.extensions.iter().map(String::as_str).collect();
                    dialog = dialog.add_filter(&filter.name, &exts);
                }
                let picked = dialog.pick_file().await;
                match picked {
                    None => Ok::<Option<PickedFile>, String>(None),
                    Some(handle) => {
                        let name = handle.file_name();
                        let bytes = handle.read().await;
                        Ok(Some(encode_picked_file(name, bytes)))
                    }
                }
            }
            .await;
            sender.send(result);
        })
        .detach();
}

#[cfg(target_arch = "wasm32")]
fn spawn_pick_file(
    options: PickFileOptions,
    sender: RpcResultSender<Result<Option<PickedFile>, String>>,
    _multi: bool,
) {
    let req_id = web_picker::next_request_id();
    let receiver = web_picker::register_single(req_id);
    if let Err(e) = web_picker::post_pick_request("pickFile", req_id, &options) {
        sender.send(Err(e));
        return;
    }
    wasm_bindgen_futures::spawn_local(async move {
        let result = match receiver.await {
            Ok(r) => r,
            Err(_) => Err("file picker channel cancelled".to_owned()),
        };
        sender.send(result);
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_pick_files(
    options: PickFileOptions,
    sender: RpcResultSender<Result<Vec<PickedFile>, String>>,
) {
    bevy::tasks::IoTaskPool::get()
        .spawn(async move {
            let result = async {
                let mut dialog = rfd::AsyncFileDialog::new();
                if let Some(title) = options.title.as_deref() {
                    dialog = dialog.set_title(title);
                }
                for filter in &options.filters {
                    let exts: Vec<&str> =
                        filter.extensions.iter().map(String::as_str).collect();
                    dialog = dialog.add_filter(&filter.name, &exts);
                }
                let picked = dialog.pick_files().await.unwrap_or_default();
                let mut out = Vec::with_capacity(picked.len());
                for handle in picked {
                    let name = handle.file_name();
                    let bytes = handle.read().await;
                    out.push(encode_picked_file(name, bytes));
                }
                Ok::<Vec<PickedFile>, String>(out)
            }
            .await;
            sender.send(result);
        })
        .detach();
}

#[cfg(target_arch = "wasm32")]
fn spawn_pick_files(
    options: PickFileOptions,
    sender: RpcResultSender<Result<Vec<PickedFile>, String>>,
) {
    let req_id = web_picker::next_request_id();
    let receiver = web_picker::register_multi(req_id);
    if let Err(e) = web_picker::post_pick_request("pickFiles", req_id, &options) {
        sender.send(Err(e));
        return;
    }
    wasm_bindgen_futures::spawn_local(async move {
        let result = match receiver.await {
            Ok(r) => r,
            Err(_) => Err("file picker channel cancelled".to_owned()),
        };
        sender.send(result);
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn encode_picked_file(name: String, bytes: Vec<u8>) -> PickedFile {
    use base64::Engine;
    let size = bytes.len();
    let mime = guess_mime(&name);
    let bytes_base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    PickedFile {
        name,
        mime,
        size,
        bytes_base64,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn guess_mime(name: &str) -> String {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("pdf") => "application/pdf",
        Some("json") => "application/json",
        Some("txt") => "text/plain",
        _ => "application/octet-stream",
    }
    .to_owned()
}

/// On wasm the bevy-explorer runs inside a Web Worker, which cannot open a
/// native file picker. We forward each `pickFile` / `pickFiles` request to the
/// host page via `postMessage` and wait for a `pickFileResult` reply.
///
/// Wire-format (worker → main):
///   `{ type: 'pickFile' | 'pickFiles', reqId: number, options: PickFileOptions }`
/// Wire-format (main → worker):
///   `{ type: 'pickFileResult', reqId: number, files?: PickedFile[], error?: string, cancelled?: boolean }`
#[cfg(target_arch = "wasm32")]
mod web_picker {
    use super::{PickFileOptions, PickedFile};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Mutex;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    enum Pending {
        Single(tokio::sync::oneshot::Sender<Result<Option<PickedFile>, String>>),
        Multi(tokio::sync::oneshot::Sender<Result<Vec<PickedFile>, String>>),
    }

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    static LISTENER_INSTALLED: AtomicBool = AtomicBool::new(false);
    static PENDING: once_cell::sync::Lazy<Mutex<HashMap<u64, Pending>>> =
        once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

    pub fn next_request_id() -> u64 {
        NEXT_ID.fetch_add(1, Ordering::SeqCst)
    }

    pub fn register_single(
        req_id: u64,
    ) -> tokio::sync::oneshot::Receiver<Result<Option<PickedFile>, String>> {
        ensure_listener();
        let (sx, rx) = tokio::sync::oneshot::channel();
        PENDING.lock().unwrap().insert(req_id, Pending::Single(sx));
        rx
    }

    pub fn register_multi(
        req_id: u64,
    ) -> tokio::sync::oneshot::Receiver<Result<Vec<PickedFile>, String>> {
        ensure_listener();
        let (sx, rx) = tokio::sync::oneshot::channel();
        PENDING.lock().unwrap().insert(req_id, Pending::Multi(sx));
        rx
    }

    pub fn post_pick_request(
        kind: &str,
        req_id: u64,
        options: &PickFileOptions,
    ) -> Result<(), String> {
        let scope = js_sys::global()
            .dyn_into::<web_sys::DedicatedWorkerGlobalScope>()
            .map_err(|_| "pickFile: not running inside a Web Worker".to_owned())?;
        let payload = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&payload, &"type".into(), &JsValue::from_str(kind));
        let _ = js_sys::Reflect::set(
            &payload,
            &"reqId".into(),
            &JsValue::from_f64(req_id as f64),
        );
        let opts_js = serde_wasm_bindgen::to_value(options)
            .map_err(|e| format!("pickFile: serialize options failed: {e:?}"))?;
        let _ = js_sys::Reflect::set(&payload, &"options".into(), &opts_js);
        scope
            .post_message(&payload)
            .map_err(|e| format!("pickFile: postMessage failed: {e:?}"))
    }

    fn ensure_listener() {
        if LISTENER_INSTALLED.swap(true, Ordering::SeqCst) {
            return;
        }
        let scope = match js_sys::global().dyn_into::<web_sys::DedicatedWorkerGlobalScope>() {
            Ok(s) => s,
            Err(_) => return,
        };
        let cb = Closure::wrap(Box::new(move |ev: web_sys::MessageEvent| {
            on_message(ev.data());
        }) as Box<dyn FnMut(web_sys::MessageEvent)>);
        let _ = scope.add_event_listener_with_callback("message", cb.as_ref().unchecked_ref());
        cb.forget();
    }

    fn on_message(data: JsValue) {
        let ty = js_sys::Reflect::get(&data, &"type".into())
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        if ty != "pickFileResult" {
            return;
        }
        let req_id = match js_sys::Reflect::get(&data, &"reqId".into())
            .ok()
            .and_then(|v| v.as_f64())
        {
            Some(n) => n as u64,
            None => return,
        };
        let pending = PENDING.lock().unwrap().remove(&req_id);
        let Some(pending) = pending else {
            return;
        };

        let error = js_sys::Reflect::get(&data, &"error".into())
            .ok()
            .and_then(|v| v.as_string());
        let cancelled = js_sys::Reflect::get(&data, &"cancelled".into())
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        match pending {
            Pending::Single(sender) => {
                let result = if let Some(err) = error {
                    Err(err)
                } else if cancelled {
                    Ok(None)
                } else {
                    let files_js = js_sys::Reflect::get(&data, &"files".into()).ok();
                    parse_files(files_js)
                        .and_then(|mut v| Ok(v.pop()))
                };
                let _ = sender.send(result);
            }
            Pending::Multi(sender) => {
                let result = if let Some(err) = error {
                    Err(err)
                } else if cancelled {
                    Ok(Vec::new())
                } else {
                    let files_js = js_sys::Reflect::get(&data, &"files".into()).ok();
                    parse_files(files_js)
                };
                let _ = sender.send(result);
            }
        }
    }

    fn parse_files(files_js: Option<JsValue>) -> Result<Vec<PickedFile>, String> {
        let Some(files_js) = files_js else {
            return Ok(Vec::new());
        };
        if files_js.is_null() || files_js.is_undefined() {
            return Ok(Vec::new());
        }
        serde_wasm_bindgen::from_value(files_js)
            .map_err(|e| format!("pickFileResult: bad files payload: {e:?}"))
    }
}
