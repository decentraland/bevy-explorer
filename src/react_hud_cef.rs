// Run the react-web HUD over the NATIVE engine via CEF offscreen rendering: CEF paints the HUD
// into a bevy Image shown as a fullscreen UI node over the 3D view, and input routes inside the
// engine — the alpha of the HUD texture at the cursor decides HUD vs world, so per-pixel
// passthrough is a texture read, with no OS window layering or focus juggling.
//
// Transport: the page's BroadcastChannel Envelopes ride cef_offscreen's IPC (`window.cef.emit` /
// `window.cef.listen('bridge', ..)`, wired by react-web's cefNativeBridge when it detects
// `window.cef`), and this file relays them like react_hud.rs does. With `--ui <bridge-scene>` the
// SDK7 bridge-scene owns the wire protocol (this file is a pure Envelope pipe via
// BridgeToPage/GetBridgeStream); without it a built-in fallback handles login, chat and
// scene-loading directly (mirroring the earlier wry-overlay POC, PR #912).
//
// Run (files only, no servers):
//   cd react-web && npm run bundle:native   # page -> assets/react-hud, scene -> assets/bridge-scene
//   cargo run --release --features react-hud-cef --bin decentra-bevy -- --server <realm>
// The page loads from cef://localhost/react-hud (the bevy assets dir) and the bridge-scene loads
// as a file realm from assets/bridge-scene. Overrides: REACT_HUD_URL (e.g. a vite dev server with
// ?native=1 for HMR), --ui <url|dir|none>. The CEF framework loads bundle-relative
// (Contents/Frameworks) with a dev fallback at ~/.local/share/cef (export-cef-dir).
//
// Keys are forwarded to the page only while the cursor is over the HUD or a text field is
// focused (CefInputGate); the engine still receives them in parallel (engine-side suppression
// while typing is a known gap). Mouse wheel over the HUD also reaches the engine.

use bevy::asset::RenderAssetUsages;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::FocusPolicy;
use bevy::window::{CursorGrabMode, PrimaryWindow, WindowResized};
use cef_offscreen::prelude::{
    Browsers, CefInputGate, CefOffscreenPlugin, CefWebviewUri, HostEmitEvent, JsEmitEventPlugin,
    RenderTexture, WebviewSize,
};
use common::rpc::{RpcResultReceiver, RpcResultSender, RpcStreamReceiver, RpcStreamSender};
use common::structs::PrimaryUser;
use input_manager::MouseInteractionComponent;
use system_bridge::{ChatMessage, SceneLoadingUi, SystemApi};

pub struct ReactHudCefPlugin;

impl Plugin for ReactHudCefPlugin {
    fn build(&self, app: &mut App) {
        // Needed to read engine fps for the perf overlay; may already be added by --log_fps/preview.
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        app.add_plugins((
            CefOffscreenPlugin,
            JsEmitEventPlugin::<PageEnvelope>::default(),
        ));
        app.add_systems(Startup, spawn_hud);
        app.add_systems(
            Update,
            (
                update_hud_texture,
                route_mouse,
                pump_bridge,
                pump_streams,
                player_ready,
                resize_hud,
                push_engine_fps,
            ),
        );
        app.add_observer(on_page_envelope);
    }
}

// The page posts {to:'scene'} Envelopes via window.cef.emit; bevy_cef delivers each as a trigger
// on the webview entity with the raw JSON payload.
#[derive(Event, serde::Deserialize, Debug)]
struct PageEnvelope(serde_json::Value);

/// Marker for the fullscreen UI node displaying the HUD texture.
#[derive(Component)]
struct HudUiNode;

#[derive(Resource)]
struct ReactHudCef {
    hud: Entity,
    image: Handle<Image>,
    // Cursor currently over an opaque HUD pixel (engine world input gated off via Interaction on
    // the fullscreen node), vs over the transparent world area.
    over_ui: bool,
    chat_rx: RpcStreamReceiver<ChatMessage>,
    loading_rx: RpcStreamReceiver<SceneLoadingUi>,
    // Outstanding login RPCs awaiting an engine result, paired with the page's rpc id.
    pending_prev: Vec<(String, RpcResultReceiver<Option<String>>)>,
    pending_login: Vec<(String, RpcResultReceiver<Result<(), String>>)>,
    player_ready_sent: bool,
    // The page's bridge listener is live once it has sent us anything; until then events like
    // playerReady would be fired into the void (the page hasn't subscribed yet).
    page_seen: bool,
    // A HUD text field is focused (page focusin/focusout via the shim) — keep forwarding keys to
    // CEF even when the cursor is over the world, so chat typing isn't cut off mid-word.
    text_focused: bool,
    // When the super-user bridge-scene is driving (--ui <bridge-scene>), this is its page->scene
    // stream; the relay becomes a pure Envelope pipe and the per-domain fallback is bypassed.
    bridge_sender: Option<RpcStreamSender<String>>,
}

fn react_hud_url() -> Option<String> {
    // explicit override, e.g. a live vite server (REACT_HUD_URL=http://localhost:5173/?native=1)
    // for HMR against the native engine.
    if let Ok(url) = std::env::var("REACT_HUD_URL") {
        return Some(url);
    }
    // Bundled page (npm run bundle:native → assets/react-hud) served over the cef:// scheme —
    // dev and prod run the same embedded build, no server. The path check mirrors the bevy
    // asset root: cwd for packaged runs, the checkout for `cargo run` from any directory.
    let mut roots = vec![
        std::path::PathBuf::from("."),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")),
    ];
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
    {
        roots.push(dir);
    }
    if roots
        .iter()
        .any(|root| root.join("assets/react-hud/index.html").is_file())
    {
        return Some("cef://localhost/react-hud/index.html?native=1".to_string());
    }
    None
}

fn spawn_hud(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut sys: EventWriter<SystemApi>,
) {
    let Some(url) = react_hud_url() else {
        error!(
            "[react-hud-cef] no HUD page: run `npm run bundle:native` in react-web (or set \
             REACT_HUD_URL); the app will run without a HUD"
        );
        return;
    };

    let (w, h) = windows
        .single()
        .map(|w| (w.width(), w.height()))
        .unwrap_or((1280.0, 720.0));

    // Placeholder until the first CEF paint arrives (update_hud_texture replaces it wholesale).
    let image = images.add(Image::new_fill(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::all(),
    ));

    info!("[react-hud-cef] loading {url}");
    let hud = commands
        .spawn((CefWebviewUri::new(url), WebviewSize(Vec2::new(w, h))))
        .id();

    commands.spawn((
        HudUiNode,
        ImageNode::new(image.clone()),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..Default::default()
        },
        GlobalZIndex(1000),
        FocusPolicy::Block,
        MouseInteractionComponent,
        // Present initially (login screen is fully HUD); route_mouse inserts/removes it as the
        // cursor crosses opaque/transparent HUD pixels — its presence is what gates engine world
        // input (resolve_pointer_target treats any hovered MouseInteractionComponent as UI).
        Interaction::default(),
    ));

    // Subscribe to the engine streams the HUD needs (fallback relay, as react_hud.rs).
    let (chat_tx, chat_rx) = RpcStreamSender::channel();
    let (loading_tx, loading_rx) = RpcStreamSender::channel();
    sys.write(SystemApi::GetChatStream(chat_tx));
    sys.write(SystemApi::GetSceneLoadingUiStream(loading_tx));

    commands.insert_resource(ReactHudCef {
        hud,
        image,
        over_ui: true,
        chat_rx,
        loading_rx,
        pending_prev: Vec::new(),
        pending_login: Vec::new(),
        player_ready_sent: false,
        page_seen: false,
        text_focused: false,
        bridge_sender: None,
    });
}

// Copy each CEF OSR paint into the HUD image (BGRA, logical-pixel sized — the buffer is 1:1 with
// WebviewSize). RenderAssetUsages::all() keeps the CPU copy that route_mouse alpha-tests.
fn update_hud_texture(
    mut er: EventReader<RenderTexture>,
    state: Option<Res<ReactHudCef>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(state) = state else { return };
    for tex in er.read() {
        if tex.webview != state.hud {
            continue;
        }
        let Some(image) = images.get_mut(&state.image) else {
            continue;
        };
        if image.width() == 1 {
            info!(
                "[react-hud-cef] first paint: {}x{} ({} bytes)",
                tex.width,
                tex.height,
                tex.buffer.len()
            );
        }
        *image = Image::new(
            Extent3d {
                width: tex.width,
                height: tex.height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            tex.buffer.clone(),
            TextureFormat::Bgra8UnormSrgb,
            RenderAssetUsages::all(),
        );
    }
}

// Alpha of the HUD texture at a logical-pixel position; the texture maps 1:1 onto the window.
fn hud_alpha_at(image: &Image, pos: Vec2) -> u8 {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let (x, y) = (pos.x as usize, pos.y as usize);
    if x >= w || y >= h {
        return 0;
    }
    image
        .data
        .as_ref()
        .and_then(|data| data.get((y * w + x) * 4 + 3))
        .copied()
        .unwrap_or(0)
}

// Per-pixel input routing: over an opaque HUD pixel, forward mouse to CEF and gate engine world
// input (via Interaction on the fullscreen node); over transparent pixels the engine keeps the
// mouse. While the cursor is pointer-locked (camera mouse-look) everything goes to the engine.
#[allow(clippy::too_many_arguments)]
fn route_mouse(
    state: Option<ResMut<ReactHudCef>>,
    images: Res<Assets<Image>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    browsers: NonSend<Browsers>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut wheel: EventReader<MouseWheel>,
    hud_nodes: Query<Entity, With<HudUiNode>>,
    mut gate: ResMut<CefInputGate>,
    mut commands: Commands,
) {
    let Some(mut state) = state else { return };
    let Ok(window) = windows.single() else {
        return;
    };
    let locked = window.cursor_options.grab_mode == CursorGrabMode::Locked;
    let cursor = if locked {
        None
    } else {
        window.cursor_position()
    };

    let over = cursor
        .and_then(|c| images.get(&state.image).map(|img| hud_alpha_at(img, c)))
        .is_some_and(|alpha| alpha > 8);

    // keys go to the page while the cursor is over the HUD or a text field holds focus; world-
    // control keys otherwise stay out of the page (no HUD hotkeys while walking)
    gate.keyboard = over || state.text_focused;

    if over != state.over_ui {
        state.over_ui = over;
        if let Ok(node) = hud_nodes.single() {
            if over {
                commands.entity(node).insert(Interaction::default());
            } else {
                commands.entity(node).remove::<Interaction>();
                // Tell the page the pointer left so CSS :hover state clears.
                let leave = cursor.unwrap_or(Vec2::new(-1.0, -1.0));
                browsers.send_mouse_move(&state.hud, [], leave, true);
            }
        }
    }

    let Some(cursor) = cursor else { return };
    if !over {
        return;
    }
    browsers.send_mouse_move(&state.hud, buttons.get_pressed(), cursor, false);
    for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
        if buttons.just_pressed(button) {
            browsers.send_mouse_click(&state.hud, cursor, button, false);
        }
        if buttons.just_released(button) {
            browsers.send_mouse_click(&state.hud, cursor, button, true);
        }
    }
    for ev in wheel.read() {
        browsers.send_mouse_wheel(&state.hud, cursor, Vec2::new(ev.x, ev.y));
    }
}

// Deliver a full Envelope (JSON string) to the page (the shim's cef.listen('bridge') feeds it to
// the BroadcastChannel).
fn to_page_raw(commands: &mut Commands, hud: Entity, envelope_json: String) {
    commands.trigger_targets(
        HostEmitEvent {
            id: "bridge".to_string(),
            payload: envelope_json,
        },
        hud,
    );
}
fn to_page(commands: &mut Commands, hud: Entity, msg: serde_json::Value) {
    let env = serde_json::json!({ "to": "page", "msg": msg }).to_string();
    to_page_raw(commands, hud, env);
}
fn rpc_res(commands: &mut Commands, hud: Entity, id: &str, value: serde_json::Value) {
    to_page(
        commands,
        hud,
        serde_json::json!({ "kind": "rpc:res", "id": id, "ok": true, "value": value }),
    );
}

// Bridge mode: forward the super-user bridge-scene's page-bound Envelopes to the webview, and
// capture its page->scene stream (used by on_page_envelope to deliver page messages to the scene).
fn pump_bridge(
    state: Option<ResMut<ReactHudCef>>,
    mut events: EventReader<SystemApi>,
    mut commands: Commands,
) {
    let Some(mut state) = state else { return };
    let hud = state.hud;
    for ev in events.read() {
        match ev {
            SystemApi::BridgeToPage(env) => to_page_raw(&mut commands, hud, env.clone()),
            SystemApi::GetBridgeStream(sx) => {
                info!("[react-hud-cef] bridge-scene connected (piping Envelopes)");
                state.bridge_sender = Some(sx.clone());
            }
            _ => {}
        }
    }
}

// page -> engine: same fallback relay as react_hud.rs pump_ipc (login/chat domains).
fn on_page_envelope(
    trigger: Trigger<PageEnvelope>,
    state: Option<ResMut<ReactHudCef>>,
    mut sys: EventWriter<SystemApi>,
    mut commands: Commands,
) {
    let Some(mut state) = state else { return };
    state.page_seen = true;
    let env = &trigger.event().0;
    debug!("[react-hud-cef] page -> engine: {env}");
    // engine-addressed control messages (not for the bridge scene)
    if env.get("to").and_then(|t| t.as_str()) == Some("engine") {
        let msg = env.get("msg").cloned().unwrap_or_default();
        if msg.get("kind").and_then(|k| k.as_str()) == Some("textFocus") {
            state.text_focused = msg
                .get("focused")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
        }
        return;
    }
    if env.get("to").and_then(|t| t.as_str()) != Some("scene") {
        return;
    }
    // Bridge mode: the bridge-scene owns the wire protocol — pipe the raw Envelope straight to it
    // and skip the per-domain fallback below.
    if let Some(sender) = &state.bridge_sender {
        let _ = sender.send(env.to_string()); // scene gone => drop; nothing to recover
        return;
    }
    let hud = state.hud;
    let v = env.get("msg").cloned().unwrap_or_else(|| env.clone());
    match v.get("kind").and_then(|k| k.as_str()).unwrap_or("") {
        "rpc:req" => {
            let id = v
                .get("id")
                .and_then(|i| i.as_str())
                .unwrap_or("")
                .to_string();
            match v.get("method").and_then(|m| m.as_str()).unwrap_or("") {
                "getPreviousLogin" => {
                    // getPreviousLogin is the page's startup signal, sent right after it
                    // subscribes. React StrictMode mounts twice, so the live listener may be the
                    // SECOND one — re-arm playerReady so it's resent to whoever is now subscribed.
                    state.player_ready_sent = false;
                    let (s, r) = RpcResultSender::channel();
                    sys.write(SystemApi::GetPreviousLogin(s));
                    state.pending_prev.push((id, r));
                }
                "loginGuest" => {
                    sys.write(SystemApi::LoginGuest);
                    rpc_res(&mut commands, hud, &id, serde_json::Value::Null);
                }
                "loginPrevious" | "loginIdentity" => {
                    let (s, r) = RpcResultSender::channel();
                    sys.write(SystemApi::LoginPrevious(s));
                    state.pending_login.push((id, r));
                }
                "logout" => {
                    sys.write(SystemApi::Logout);
                    rpc_res(&mut commands, hud, &id, serde_json::Value::Null);
                }
                "loginCancel" => rpc_res(&mut commands, hud, &id, serde_json::Value::Null),
                _ => {}
            }
        }
        "sendChat" => {
            let message = v.get("message").and_then(|m| m.as_str()).unwrap_or("");
            let channel = v
                .get("channel")
                .and_then(|c| c.as_str())
                .unwrap_or("Nearby");
            sys.write(SystemApi::SendChat(
                message.to_string(),
                channel.to_string(),
            ));
        }
        _ => {} // other domains (profile/friends/...) not relayed yet
    }
}

// engine -> page: drain streams + resolved login RPCs. Fallback only — the bridge-scene owns
// these domains when it's driving.
fn pump_streams(state: Option<ResMut<ReactHudCef>>, mut commands: Commands) {
    let Some(mut state) = state else { return };
    if state.bridge_sender.is_some() {
        return;
    }
    let hud = state.hud;

    let mut chats = Vec::new();
    while let Ok(cm) = state.chat_rx.try_recv() {
        chats.push(cm);
    }
    let mut loadings = Vec::new();
    while let Ok(l) = state.loading_rx.try_recv() {
        loadings.push(l);
    }
    for cm in chats {
        to_page(
            &mut commands,
            hud,
            serde_json::json!({ "kind":"chat", "chat": { "sender": cm.sender_address, "message": cm.message, "channel": cm.channel } }),
        );
    }
    for l in loadings {
        to_page(
            &mut commands,
            hud,
            serde_json::json!({ "kind":"sceneLoading", "state": { "visible": l.visible, "realmConnected": l.realm_connected, "title": l.title, "pendingAssets": l.pending_assets } }),
        );
    }

    // resolve login RPCs whose engine result has arrived
    let mut i = 0;
    while i < state.pending_prev.len() {
        match state.pending_prev[i].1.try_recv() {
            Ok(user) => {
                let (id, _) = state.pending_prev.remove(i);
                rpc_res(
                    &mut commands,
                    hud,
                    &id,
                    serde_json::json!({ "userId": user }),
                );
            }
            _ => i += 1,
        }
    }
    let mut i = 0;
    while i < state.pending_login.len() {
        match state.pending_login[i].1.try_recv() {
            Ok(result) => {
                let (id, _) = state.pending_login.remove(i);
                let (success, error) = match result {
                    Ok(()) => (true, String::new()),
                    Err(e) => (false, e),
                };
                rpc_res(
                    &mut commands,
                    hud,
                    &id,
                    serde_json::json!({ "success": success, "error": error }),
                );
            }
            _ => i += 1,
        }
    }
}

// Tell the page the player has spawned (drives entering -> world), once per page subscription.
fn player_ready(
    state: Option<ResMut<ReactHudCef>>,
    players: Query<(), With<PrimaryUser>>,
    mut commands: Commands,
) {
    let Some(mut state) = state else { return };
    // Bridge mode: the bridge-scene emits playerReady itself.
    if state.bridge_sender.is_some() {
        return;
    }
    if state.player_ready_sent || !state.page_seen || players.is_empty() {
        return;
    }
    let hud = state.hud;
    to_page(
        &mut commands,
        hud,
        serde_json::json!({ "kind":"event", "name":"playerReady" }),
    );
    state.player_ready_sent = true;
}

// Track window resizes: update the webview size (bevy_cef pushes WasResized to CEF) and push the
// logical height so the HUD's --ui-scale stays correct (see useHudScale.ts).
fn resize_hud(
    mut resized: EventReader<WindowResized>,
    state: Option<Res<ReactHudCef>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut webviews: Query<&mut WebviewSize>,
    mut commands: Commands,
) {
    if resized.is_empty() {
        return;
    }
    resized.clear();
    let (Some(state), Ok(window)) = (state, windows.single()) else {
        return;
    };
    let size = Vec2::new(window.width(), window.height());
    if let Ok(mut ws) = webviews.get_mut(state.hud) {
        ws.0 = size;
    }
    commands.trigger_targets(
        HostEmitEvent {
            id: "uiHeight".to_string(),
            payload: format!("{:.0}", size.y),
        },
        state.hud,
    );
}

// Push bevy's measured render fps to the page (~2x/sec) so the React perf overlay shows real
// engine fps (see useFps.ts).
fn push_engine_fps(
    state: Option<Res<ReactHudCef>>,
    diagnostics: Res<DiagnosticsStore>,
    time: Res<Time>,
    mut acc: Local<f32>,
    mut commands: Commands,
) {
    let Some(state) = state else { return };
    *acc += time.delta_secs();
    if *acc < 0.5 {
        return;
    }
    *acc = 0.0;
    if let Some(fps) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
    {
        commands.trigger_targets(
            HostEmitEvent {
                id: "engineFps".to_string(),
                payload: format!("{fps:.0}"),
            },
            state.hud,
        );
    }
}
