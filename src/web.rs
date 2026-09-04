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
    structs::{AppConfig, CurrentRealm, EditorMode, PreviewMode, PrimaryUser, StartupScenes},
};
use dcl_wasm::init_runtime;
use futures_lite::io::AsyncReadExt;
use input_manager::InputPriorities;
use once_cell::sync::OnceCell;
use scene_runner::vec3_to_parcel;
use system_bridge::{SystemApi, SystemBridge};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::js_sys;

use system_api_types::launch_options::LaunchOptions;

use crate::{DecentralandApp, DecentralandAppConfig, DecentralandArguments};

static WASM_ASSET_LOADER_HANDLE: OnceCell<WasmLoaderHandle> = OnceCell::new();
static INIT_DATA: OnceCell<AppConfig> = OnceCell::new();
static CONSOLE_BRIDGE_SENDER: OnceCell<tokio::sync::mpsc::UnboundedSender<SystemApi>> =
    OnceCell::new();
/// The options the page launched with; the url sync echoes them back with the live values
/// (realm, position, …) swapped in.
static LAUNCH_OPTIONS: OnceCell<LaunchOptions> = OnceCell::new();

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = window, js_name = _buildEngineApi)]
    fn build_engine_api(json: &str);
}

#[wasm_bindgen(js_namespace = window)]
extern "C" {
    /// The engine's current launch options as JSON — the `engine_run` options object minus the
    /// page-derived keys — for boot.js to mirror into the page url.
    #[wasm_bindgen(js_name = set_url_params)]
    fn set_url_params(options_json: &str);

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

    /// Mirror "an engine text field holds keyboard focus" to the page (boot.js stores it
    /// on window.__engineTextFocus). The react HUD's hotkey handlers see key events before
    /// the engine does (capture-phase window listener vs the canvas), and an engine-rendered
    /// text field is invisible to their DOM-focus checks — this flag is how they know to
    /// leave keys alone while the user types into scene UI.
    #[wasm_bindgen(js_name = "__setEngineTextFocus")]
    fn set_engine_text_focus(focused: bool);

    /// The ?baseDomain= entry param, captured by boot.js (web parity with --base-domain).
    /// `catch` so a host page without boot.js just falls through to the default domain.
    #[wasm_bindgen(js_name = "__baseDomain", catch)]
    fn base_domain_param() -> Result<String, JsValue>;
}

/// Latch the base domain before any backend URL is composed. Called at the top of BOTH wasm
/// entry points: engine_init's config deserialization already materializes
/// `AppConfig::default()` fields, so engine_run alone would be too late.
fn apply_base_domain() {
    if let Ok(domain) = base_domain_param() {
        if !domain.is_empty() {
            if let Err(e) = common::base_domain::set(&domain) {
                warn!("ignoring baseDomain param: {e}");
            }
        }
    }
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
    apply_base_domain();

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

    let Ok(mut config) = serde_json::from_str::<AppConfig>(&buf) else {
        warn!("failed to deserialize app config, using default");
        return Ok("failed to deserialize".into());
    };
    config.reset_outdated_settings();

    let _ = INIT_DATA.set(config);

    Ok("Config loaded".into())
}

/// The persisted home scene — realm + "x,y" parcel — as a JSON string, falling back to the
/// derived defaults. Valid after [`engine_init`] (it reads the loaded config); exposed so the
/// HUD's places picker can target home from "Skip" BEFORE the engine is launched.
#[wasm_bindgen]
pub fn engine_home_scene() -> String {
    let (realm, parcel) = INIT_DATA
        .get()
        .map(|config| (config.home_realm(), config.home_location()))
        .unwrap_or_else(|| {
            let config = AppConfig::default();
            (config.home_realm(), config.home_location())
        });
    serde_json::json!({ "realm": realm, "parcel": format!("{},{}", parcel.x, parcel.y) })
        .to_string()
}

/// Bytes of gpu uploads per frame on web — was a constant the page passed in; the engine owns it.
const WEB_GPU_BYTES_PER_FRAME: usize = 10_000_000;

// Type the `engine_run` parameter in the generated .d.ts — keep in step with
// `system_api_types::launch_options::LaunchOptions` (the web param table's source).
#[wasm_bindgen(typescript_custom_section)]
const ENGINE_RUN_OPTIONS_TS: &str = r#"
export interface EngineRunOptions {
    realm?: string;
    position?: string;
    systemScene?: string;
    portables?: string;
    preview?: boolean;
    editor?: boolean;
    sceneParams?: string;
    pulseServer?: string;
    imposterSource?: string;
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "EngineRunOptions")]
    pub type EngineRunOptionsJs;
}

/// Round-trip the page's object through JSON rather than `serde_wasm_bindgen::from_value`: that
/// only visits the struct's own fields, so `deny_unknown_fields` would never see a misspelt key.
fn parse_options(options: &JsValue) -> Result<LaunchOptions, JsValue> {
    let json = String::from(js_sys::JSON::stringify(options)?);
    LaunchOptions::from_json(&json)
        .map(LaunchOptions::without_empty_strings)
        .map_err(|e| JsValue::from_str(&format!("engine_run: invalid options: {e}")))
}

/// Launch the engine. Throws (rejects the launch) on an invalid options object.
#[wasm_bindgen]
pub fn engine_run(options: EngineRunOptionsJs) -> Result<(), JsValue> {
    let options = parse_options(&options)?;
    let _ = LAUNCH_OPTIONS.set(options.clone());
    apply_base_domain();
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

    let decentraland_app_config = DecentralandAppConfig::new(
        decentraland_serialized_app_config(),
        decentraland_app_arguments(&options),
        Some(WASM_ASSET_LOADER_HANDLE.get().unwrap().clone()),
    );

    let mut app = decentraland_app.build(decentraland_app_config);

    // on wasm we need to explicitly specify key binds for the platform
    let user_agent = web_sys::window()
        .and_then(|w| w.navigator().user_agent().ok())
        .unwrap_or_default();
    let text_bindings = if user_agent.contains("Mac") {
        bevy_simple_text_input::TextInputNavigationBindings::macos_default()
    } else {
        bevy_simple_text_input::TextInputNavigationBindings::non_macos_default()
    };
    app.insert_resource(text_bindings);

    app.add_systems(Update, update_winit_fps)
        .add_systems(Update, update_url_params)
        .add_systems(Update, update_text_focus)
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
    Ok(())
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

fn update_text_focus(priorities: Res<InputPriorities>, mut prev: Local<bool>) {
    let focused = priorities.keyboard_claimed();
    if focused != *prev {
        *prev = focused;
        set_engine_text_focus(focused);
    }
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

/// Keeps the page url in step with the engine: the launch options with the live realm,
/// position, ui scene, portables and mode swapped in, sent whole so every param given at
/// launch (pulse server, imposter source, …) is retained alongside them.
fn update_url_params(
    player: Query<&GlobalTransform, With<PrimaryUser>>,
    current_realm: Res<CurrentRealm>,
    startup_scenes: Option<Res<StartupScenes>>,
    preview: Res<PreviewMode>,
    editor: Res<EditorMode>,
    mut prev: Local<Option<LaunchOptions>>,
) {
    // realms with fixed scene urns (worlds) spawn at their base scene and ignore an explicit
    // position (see load_active_entities' base-position handling) - don't write one into the url
    let position_honoured = current_realm
        .config
        .scenes_urn
        .as_ref()
        .is_none_or(Vec::is_empty);
    let position = position_honoured.then(|| {
        let parcel = vec3_to_parcel(player.single().map(|p| p.translation()).unwrap_or_default());
        format!("{},{}", parcel.x, parcel.y)
    });
    let Some(server) = current_realm.about_url.strip_suffix("/about") else {
        return;
    };
    let (system_scene, portables) = if let Some(s) = startup_scenes {
        // the ui scene is the super-user scene inserted at index 0 — when one was given at all
        // (?systemScene=none boots with only regular startup scenes/portables)
        let ui_scene = s
            .scenes
            .first()
            .filter(|scene| scene.super_user)
            .map(|scene| scene.source.clone());
        let portables = s
            .scenes
            .iter()
            .skip(ui_scene.is_some() as usize)
            .map(|scene| scene.source.clone())
            .collect::<Vec<_>>();

        (
            ui_scene,
            (!portables.is_empty()).then(|| portables.join(";")),
        )
    } else {
        (None, None)
    };

    let options = LaunchOptions {
        realm: Some(server.to_owned()),
        position,
        system_scene,
        // the default set is omitted so the canonical url stays clean (the page doesn't know it)
        portables: portables.filter(|p| p != system_api_types::web_params::DEFAULT_PORTABLES),
        preview: preview.is_preview,
        editor: editor.0,
        ..LAUNCH_OPTIONS.get().cloned().unwrap_or_default()
    };

    if prev.as_ref() != Some(&options) {
        let mut json = serde_json::to_value(&options).unwrap_or_default();
        if let Some(map) = json.as_object_mut() {
            // the page-derived keys are the loader's to compute, never url state
            for param in system_api_types::web_params::web_params() {
                if param.delivery == system_api_types::web_params::Delivery::Page {
                    map.remove(&param.name);
                }
            }
        }
        set_url_params(&json.to_string());
        *prev = Some(options);
    }
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

fn decentraland_app_arguments(options: &LaunchOptions) -> DecentralandArguments {
    DecentralandArguments {
        launch: options.clone(),
        gpu_bytes_per_frame: Some(WEB_GPU_BYTES_PER_FRAME),
        log_fps: Some(false),
        // wasm has no engine-managed HUD: the react page hosting the engine is the HUD
        hud: false,
        ..Default::default()
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
