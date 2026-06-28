use std::str::FromStr;

use bevy::{
    asset::WasmLoaderHandle,
    log::{Level, LogPlugin},
    prelude::*,
    render::{render_resource::PipelineCompilationHandler, renderer::RenderDevice},
    tasks::BoxedFuture,
    winit::{UpdateMode, WinitSettings},
};
use bevy_console::ConsoleConfiguration;
use common::{
    rpc::RpcResultSender,
    structs::{
        AppConfig, CurrentRealm, IVec2Arg, PreviewMode, PrimaryUser, StartupScene, StartupScenes,
    },
};
use dcl_wasm::init_runtime;
use futures_lite::io::AsyncReadExt;
use once_cell::sync::OnceCell;
use scene_runner::vec3_to_parcel;
use system_bridge::{SystemApi, SystemBridge};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::js_sys;

use crate::{DecentralandApp, DecentralandAppConfig, DecentralandArguments};

static WASM_ASSET_LOADER_HANDLE: OnceCell<WasmLoaderHandle> = OnceCell::new();
static INIT_DATA: OnceCell<AppConfig> = OnceCell::new();
static CONSOLE_BRIDGE_SENDER: OnceCell<tokio::sync::mpsc::UnboundedSender<SystemApi>> =
    OnceCell::new();

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = window, js_name = _buildEngineApi)]
    fn build_engine_api(json: &str);
}

#[wasm_bindgen(js_namespace = window)]
extern "C" {
    #[wasm_bindgen(js_name = set_url_params)]
    fn set_url_params(
        x: i32,
        y: i32,
        realm: String,
        system_scene: Option<String>,
        portables: Option<String>,
        is_preview: bool,
    );

    #[wasm_bindgen(js_name = "allowADummyPipeline")]
    fn allow_a_dummy_pipeline();

    #[wasm_bindgen(js_name = "lastPipelineWasValid")]
    fn last_pipeline_was_valid() -> bool;

    #[wasm_bindgen(js_name = "waitForPipelines")]
    fn wait_for_async_pipelines() -> js_sys::Promise;

    /// Ping the JS-side watchdog once per frame. If these stop arriving (e.g. the
    /// main thread is deadlocked waiting on a lock held by a crashed worker), the
    /// watchdog surfaces the crash overlay. Defined in index.html before the engine runs.
    #[wasm_bindgen(js_name = "__engineHeartbeat")]
    fn engine_heartbeat();
}

/// call from a separate worker to initialize a channel for asset load processing
#[wasm_bindgen]
pub fn init_asset_load_thread() {
    let asset_server_channel = bevy::asset::init_thread_loader();
    let Ok(()) = WASM_ASSET_LOADER_HANDLE.set(asset_server_channel) else {
        panic!("can't init wasm loader");
    };
}

#[wasm_bindgen]
pub async fn engine_init() -> Result<JsValue, JsValue> {
    console_error_panic_hook::set_once();

    let mut file = match web_fs::File::open("config.json").await {
        Ok(f) => f,
        Err(e) => {
            warn!("no config found: {e:?}");
            return Ok("No Config".into());
        }
    };
    let mut buf = String::new();
    if let Err(e) = file.read_to_string(&mut buf).await {
        warn!("failed to read config.json: {e:?}");
        return Ok("failed to read".into());
    }

    let Ok(config) = serde_json::from_str(&buf) else {
        warn!("failed to deserialize app config, using default");
        return Ok("failed to deserialize".into());
    };

    let _ = INIT_DATA.set(config);

    Ok("Config loaded".into())
}

#[expect(clippy::too_many_arguments)]
#[wasm_bindgen]
pub fn engine_run(
    platform: &str,
    server: &str,
    location: &str,
    system_scene: &str,
    portables: &str,
    with_thread_loader: bool,
    is_preview: bool,
    gpu_bytes_per_frame: usize,
    params: &str,
) {
    init_runtime();

    let default_filter = "symphonia=warn";
    let filter = match std::option_env!("RUST_LOG") {
        Some(env) if !env.is_empty() => format!("{default_filter},{env}"),
        _ => default_filter.to_string(),
    };
    let decentraland_app = DecentralandApp::new(LogPlugin {
        level: Level::INFO,
        filter,
        custom_layer: |_| None,
    });

    let decentraland_app_config = decentraland_app_config(
        server,
        location,
        system_scene,
        portables,
        gpu_bytes_per_frame,
        is_preview,
        params,
        with_thread_loader.then(|| WASM_ASSET_LOADER_HANDLE.get().unwrap().clone()),
    );

    let mut app = decentraland_app.build(decentraland_app_config);

    // on wasm we need to explicitly specify key binds for the platform
    let text_bindings = if platform.contains("mac") {
        bevy_simple_text_input::TextInputNavigationBindings::macos_default()
    } else {
        bevy_simple_text_input::TextInputNavigationBindings::non_macos_default()
    };
    app.insert_resource(text_bindings);

    app.add_systems(Update, update_winit_fps)
        .add_systems(Update, update_url_params)
        .add_systems(Last, engine_heartbeat_system);

    app.add_systems(
        Update,
        extract_js_api.run_if(|mut once: Local<bool>| {
            let run = !*once;
            *once = true;
            run
        }),
    );

    let bridge_sender = app.world().resource::<SystemBridge>().sender.clone();
    let _ = CONSOLE_BRIDGE_SENDER.set(bridge_sender);

    app.run();
}

/// Send a console command to the engine from JavaScript.
/// `command_line` is the full command string, e.g. `"/teleport 10 20"`.
/// Returns a Promise that resolves with the command output or rejects with an error message.
#[wasm_bindgen]
pub async fn engine_console_command(command_line: String) -> Result<JsValue, JsValue> {
    let mut parts = command_line.split_whitespace();
    let Some(cmd) = parts.next() else {
        return Err(JsValue::from_str("empty command"));
    };
    let cmd = if cmd.starts_with('/') {
        cmd.to_string()
    } else {
        format!("/{cmd}")
    };
    let args: Vec<String> = parts.map(String::from).collect();

    let Some(sender) = CONSOLE_BRIDGE_SENDER.get() else {
        return Err(JsValue::from_str("engine not initialized"));
    };

    let (sx, rx) = RpcResultSender::channel();
    sender
        .send(SystemApi::ConsoleCommand(cmd, args, sx))
        .map_err(|_| JsValue::from_str("engine channel closed"))?;

    rx.await
        .map_err(|_| JsValue::from_str("command response dropped"))?
        .map(|s| JsValue::from_str(&s))
        .map_err(|e| JsValue::from_str(&e))
}

/// Extract console command metadata from clap and store as JSON for the JS API.
fn extract_js_api(config: Res<ConsoleConfiguration>) {
    let commands: Vec<serde_json::Value> = config
        .commands
        .iter()
        .map(|(name, cmd)| {
            let trailing = cmd.is_trailing_var_arg_set();
            let positional: Vec<_> = cmd
                .get_arguments()
                .filter(|a| a.get_long().is_none() && a.get_short().is_none())
                .collect();
            let last_id = positional.last().map(|a| a.get_id().as_str());
            let args: Vec<serde_json::Value> = positional
                .iter()
                .map(|arg| {
                    let id = arg.get_id().as_str();
                    let kind = if (trailing && Some(id) == last_id) || id == "json" {
                        "json"
                    } else if id == "entity" {
                        "entity"
                    } else {
                        "string"
                    };
                    let mut arg_json = serde_json::json!({
                        "name": id,
                        "kind": kind,
                        "optional": !arg.is_required_set(),
                    });
                    if let Some(help) = arg.get_help() {
                        arg_json["help"] = serde_json::Value::String(help.to_string());
                    }
                    arg_json
                })
                .collect();
            let mut cmd_json = serde_json::json!({ "cmd": name, "args": args });
            if let Some(about) = cmd.get_about() {
                cmd_json["help"] = serde_json::Value::String(about.to_string());
            }
            cmd_json
        })
        .collect();
    let json = serde_json::to_string(&commands).unwrap_or_default();
    build_engine_api(&json);
}

/// Pings the JS watchdog each frame so it can detect a stalled engine loop.
fn engine_heartbeat_system() {
    engine_heartbeat();
}

pub fn update_winit_fps(config: Res<AppConfig>, mut winit: ResMut<WinitSettings>) {
    if config.is_changed() {
        let target = config.graphics.fps_target;
        let delay_micros = 1_000_000.0 / target as f32;
        winit.focused_mode = UpdateMode::Reactive {
            wait: std::time::Duration::from_micros((delay_micros) as u64),
            react_to_device_events: false,
            react_to_user_events: false,
            react_to_window_events: false,
        };
        winit.unfocused_mode = winit.focused_mode;
    }
}

#[derive(PartialEq, Default, Clone)]
struct UrlParams {
    parcel: IVec2,
    server: String,
    ui_scene: Option<String>,
    portables: Option<String>,
    preview: bool,
}

fn update_url_params(
    player: Query<&GlobalTransform, With<PrimaryUser>>,
    current_realm: Res<CurrentRealm>,
    startup_scenes: Option<Res<StartupScenes>>,
    preview: Res<PreviewMode>,
    mut prev: Local<UrlParams>,
) {
    let parcel = vec3_to_parcel(player.single().map(|p| p.translation()).unwrap_or_default());
    let Some(server) = current_realm.about_url.strip_suffix("/about") else {
        return;
    };
    let (ui_scene, portables) = if let Some(s) = startup_scenes {
        let scenes = s
            .scenes
            .iter()
            .map(|scene| scene.source.clone())
            .collect::<Vec<_>>();

        (
            scenes.first().cloned(),
            scenes.get(1..).map(|scenes| scenes.join(";")),
        )
    } else {
        (None, None)
    };
    let preview = preview.is_preview;

    let params = UrlParams {
        parcel,
        server: server.to_owned(),
        ui_scene,
        portables,
        preview,
    };

    if params != *prev {
        *prev = params.clone();
        set_url_params(
            params.parcel.x,
            params.parcel.y,
            params.server,
            params.ui_scene,
            params.portables,
            params.preview,
        );
    }
}

#[expect(clippy::too_many_arguments)]
fn decentraland_app_config(
    server: &str,
    location: &str,
    ui_scene: &str,
    portables: &str,
    gpu_bytes_per_frame: usize,
    is_preview: bool,
    params: &str,
    wasm_loader_handle: Option<WasmLoaderHandle>,
) -> DecentralandAppConfig {
    let app_config = decentraland_serialized_app_config();
    let arguments = decentraland_app_arguments(
        server,
        location,
        ui_scene,
        portables,
        gpu_bytes_per_frame,
        is_preview,
        params,
    );

    DecentralandAppConfig::new(app_config, arguments, wasm_loader_handle)
}

fn decentraland_serialized_app_config() -> AppConfig {
    INIT_DATA.get().cloned().unwrap_or_else(|| AppConfig {
        graphics: common::structs::GraphicsSettings {
            shadow_distance: 20.0,
            shadow_settings: common::structs::ShadowSetting::Low,
            ..Default::default()
        },
        ..Default::default()
    })
}

fn decentraland_app_arguments(
    server: &str,
    location: &str,
    ui_scene: &str,
    portables: &str,
    gpu_bytes_per_frame: usize,
    is_preview: bool,
    params: &str,
) -> DecentralandArguments {
    DecentralandArguments {
        server: Some(server.to_owned()),
        content_server_override: None,
        location: IVec2Arg::from_str(location)
            .map(|location_arg| location_arg.0)
            .ok(),
        startup_scenes: Some(
            portables
                .split(";")
                .map(|portable| StartupScene {
                    source: portable.to_owned(),
                    super_user: false,
                    preview: false,
                    hot_reload: None,
                    hash: None,
                })
                .collect::<Vec<_>>(),
        )
        .filter(|startup_scenes| !startup_scenes.is_empty()),
        ui_scene: (!ui_scene.is_empty()).then(|| ui_scene.to_owned()),
        scene_params: Some(params.to_owned()),
        scene_threads: None,
        scene_load_distance: None,
        scene_unload_extra_distance: None,
        scene_imposter_bake: None,
        scene_imposter_distances: None,
        scene_imposter_multisample: None,
        vsync: None,
        fps_target: None,
        gpu_bytes_per_frame: Some(gpu_bytes_per_frame),
        is_preview,
        sysinfo_visible: false,
        scene_log_to_console: false,
        startup_scenes_preview: false,
        no_avatar: false,
        no_gltf: false,
        no_fog: false,
        log_fps: Some(false),
        inspect: None,
        test_mode: false,
        test_scenes: None,
        login: false,
        emote_wheel: false,
        chat: false,
        permissions: false,
        profile: false,
        nametags: false,
        tooltips: false,
        loading_scene: false,
    }
}

pub struct PipelineHandler;

impl PipelineCompilationHandler for PipelineHandler {
    fn precreate_render_pipeline<'a>(
        &self,
        device: &'a RenderDevice,
        desc: &'a wgpu::RenderPipelineDescriptor,
    ) -> BoxedFuture<'a, ()> {
        Box::pin(async {
            allow_a_dummy_pipeline();
            let _ = device.create_render_pipeline(desc);
            if !last_pipeline_was_valid() {
                let _ = wasm_bindgen_futures::JsFuture::from(wait_for_async_pipelines()).await;
            }
        })
    }
}
