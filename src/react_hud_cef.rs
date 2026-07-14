// Run the react-web HUD over the NATIVE engine via CEF offscreen rendering: CEF paints the HUD
// into a bevy Image shown as a fullscreen UI node over the 3D view, and input routes inside the
// engine — the alpha of the HUD texture at the cursor decides HUD vs world, so per-pixel
// passthrough is a texture read, with no OS window layering or focus juggling.
//
// Transport: the page's BroadcastChannel Envelopes ride cef_offscreen's IPC (`window.cef.emit` /
// `window.cef.listen('bridge', ..)`, wired by react-web's cefNativeBridge when it detects
// `window.cef`), and this file relays them. With `--ui <bridge-scene>` the
// SDK7 bridge-scene owns the wire protocol (this file is a pure Envelope pipe via
// BridgeToPage/GetBridgeStream); until it connects (scene boot takes seconds; the page is up
// immediately) a built-in fallback handles the login rpcs, so the login screen is responsive
// from the first frame. The fallback is a boot shim, not a HUD backend: it subscribes to no
// engine streams, and the page assumes "loading" until the bridge-scene reports real state —
// so with no bridge-scene at all (--ui none, missing bundle) login works but the world stays
// behind the loading screen.
//
// Run (files only, no servers):
//   cd react-web && npm run bundle:native   # page -> assets/react-hud, scene -> assets/bridge-scene
//   cargo run --release --features react-hud-cef --bin decentra-bevy -- --server <realm>
// The page loads from cef://localhost/react-hud (the bevy assets dir) and the bridge-scene loads
// as a file realm from assets/bridge-scene. Overrides: REACT_HUD_URL (e.g. a vite dev server with
// ?native=1 for HMR), --ui <url|dir|none>. The CEF framework loads bundle-relative
// (Contents/Frameworks) with a dev fallback at ~/.local/share/cef (export-cef-dir).
//
// Input mirrors web semantics (where the page and the engine canvas share the document): all
// keys and unlocked-cursor mouse events are forwarded to the page — so outside-clicks defocus
// text fields and page hotkeys (e.g. B for emotes) always work — while the engine takes world
// input per-pixel (opaque HUD pixel under the cursor gates it off) and drops key-bound actions
// while a HUD text field holds focus (InputPriorities reservation). Mouse wheel over the HUD
// still also reaches the engine (known gap).

use bevy::asset::RenderAssetUsages;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::FocusPolicy;
use bevy::window::{CursorGrabMode, PrimaryWindow, WindowResized};
use cef_offscreen::prelude::{
    Browsers, CefOffscreenPlugin, CefWebviewUri, HostEmitEvent, JsEmitEventPlugin, RenderTexture,
    WebviewSize,
};
use common::rpc::{RpcResultReceiver, RpcResultSender, RpcStreamSender};
use common::structs::PrimaryUser;
use input_manager::{InputPriorities, InputPriority, InputType, MouseInteractionComponent};
use system_bridge::SystemApi;

pub struct ReactHudCefPlugin {
    /// An explicit --server destination, if given. Injected into the page URL as ?realm= — the
    /// page skips its post-login places picker for it (parity with ?realm= on web); the native
    /// driver knows the engine is already there, so it keeps the realm rather than re-switching.
    pub server: Option<String>,
}

/// Options threaded from the plugin into [`spawn_hud`].
#[derive(Resource)]
struct ReactHudOptions {
    server: Option<String>,
}

impl Plugin for ReactHudCefPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ReactHudOptions {
            server: self.server.clone(),
        });
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
                push_text_focus,
            ),
        );
        // macOS only: Blink leaves Cmd editing shortcuts to the AppKit menu a windowed browser
        // would have; offscreen rendering has none, so translate them for the webview. Blink
        // handles the Ctrl equivalents itself on other platforms.
        #[cfg(target_os = "macos")]
        app.add_systems(Update, translate_edit_shortcuts);
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
    // Outstanding login RPCs awaiting an engine result, paired with the page's rpc id.
    pending_prev: Vec<(String, RpcResultReceiver<Option<String>>)>,
    pending_login: Vec<(String, RpcResultReceiver<Result<(), String>>)>,
    // loginNew verification codes awaiting the engine (forwarded to the page as 'loginCode'
    // messages mid-flight; the rpc result itself rides pending_login).
    pending_code: Vec<RpcResultReceiver<Result<Option<i32>, String>>>,
    player_ready_sent: bool,
    // The page's bridge listener is live once it has sent us anything; until then events like
    // playerReady would be fired into the void (the page hasn't subscribed yet).
    page_seen: bool,
    // A HUD text field is focused (page focusin/focusout via the shim) — the engine's key-bound
    // actions are suppressed so WASD etc. type instead of moving the avatar.
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
    options: Res<ReactHudOptions>,
) {
    let Some(mut url) = react_hud_url() else {
        error!(
            "[react-hud-cef] no HUD page: run `npm run bundle:native` in react-web (or set \
             REACT_HUD_URL); the app will run without a HUD"
        );
        return;
    };
    if let Some(server) = &options.server {
        url.push_str(if url.contains('?') { "&" } else { "?" });
        url.push_str("realm=");
        url.push_str(&urlencoding::encode(server));
    }

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
        // Block + Interaction present initially (login screen is fully HUD); route_mouse flips
        // BOTH as the cursor crosses opaque/transparent HUD pixels. Interaction presence gates
        // engine world input (resolve_pointer_target treats any hovered MouseInteractionComponent
        // as UI); FocusPolicy gates the scene-UI nodes stacked below this one.
        FocusPolicy::Block,
        MouseInteractionComponent,
        Interaction::default(),
    ));

    commands.insert_resource(ReactHudCef {
        hud,
        image,
        over_ui: true,
        pending_prev: Vec::new(),
        pending_login: Vec::new(),
        pending_code: Vec::new(),
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

// Click cadence for CEF's click_count (2 = word select, 3 = paragraph select): with no OS window
// under the webview, multi-click detection is the host's job.
#[derive(Default)]
struct MultiClick {
    button: Option<MouseButton>,
    count: i32,
    at: f64,
    pos: Vec2,
}

// Mouse routing: while the cursor is unlocked, everything is forwarded to the page (it sees the
// full document like on web — outside-clicks defocus text fields, :hover tracks the real cursor);
// what the ENGINE receives is gated per-pixel — an opaque HUD pixel under the cursor blocks world
// input (via Interaction on the fullscreen node). While pointer-locked (camera mouse-look) there
// is no cursor and the engine keeps everything.
#[allow(clippy::too_many_arguments)]
fn route_mouse(
    state: Option<ResMut<ReactHudCef>>,
    images: Res<Assets<Image>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    browsers: NonSend<Browsers>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut wheel: EventReader<MouseWheel>,
    hud_nodes: Query<Entity, With<HudUiNode>>,
    time: Res<Time>,
    mut clicks: Local<MultiClick>,
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

    if over != state.over_ui {
        state.over_ui = over;
        if let Ok(node) = hud_nodes.single() {
            if over {
                commands
                    .entity(node)
                    .insert((Interaction::default(), FocusPolicy::Block));
            } else {
                // FocusPolicy must flip with Interaction: ui_focus_system stops at the first
                // Block node in the stack whether or not it has an Interaction, and this node
                // covers everything at z1000 — a permanent Block starves every scene-UI node
                // (their hover/press never fires, scene UIs are completely dead).
                commands
                    .entity(node)
                    .remove::<Interaction>()
                    .insert(FocusPolicy::Pass);
            }
        }
    }

    let Some(cursor) = cursor else { return };
    browsers.send_mouse_move(&state.hud, buttons.get_pressed(), cursor, false);
    for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
        if buttons.just_pressed(button) {
            let now = time.elapsed_secs_f64();
            // same button, quick succession, near-stationary -> next multi-click step
            if clicks.button == Some(button)
                && now - clicks.at < 0.5
                && cursor.distance(clicks.pos) < 4.0
                && clicks.count < 3
            {
                clicks.count += 1;
            } else {
                clicks.count = 1;
            }
            *clicks = MultiClick {
                button: Some(button),
                count: clicks.count,
                at: now,
                pos: cursor,
            };
            browsers.send_mouse_click(&state.hud, cursor, button, false, clicks.count);
        }
        if buttons.just_released(button) {
            let count = if clicks.button == Some(button) {
                clicks.count
            } else {
                1
            };
            browsers.send_mouse_click(&state.hud, cursor, button, true, count);
        }
    }
    for ev in wheel.read() {
        // CEF takes pixel deltas. Notched mice report Line units (±1.0/notch) — unscaled that's
        // one PIXEL per notch, i.e. lists that barely move; 40 px is Blink's own line step.
        // Trackpads report Pixel units already.
        let pixels_per_unit = match ev.unit {
            MouseScrollUnit::Line => 40.0,
            MouseScrollUnit::Pixel => 1.0,
        };
        browsers.send_mouse_wheel(&state.hud, cursor, Vec2::new(ev.x, ev.y) * pixels_per_unit);
    }
}

// Map editing shortcuts to webview edit commands using the same platform-aware binding table the
// engine's own text boxes use (bevy_simple_text_input), so HUD and engine text editing stay
// consistent. First matching binding wins, mirroring the crate's own matcher (order carries
// meaning: e.g. Redo Cmd+Shift+Z is listed before Undo Cmd+Z, whose modifiers are a subset), and
// like the crate, Shift is not part of the bindings — it upgrades a caret move to a selection.
#[cfg(target_os = "macos")]
fn translate_edit_shortcuts(
    state: Option<Res<ReactHudCef>>,
    mut er: EventReader<bevy::input::keyboard::KeyboardInput>,
    input: Res<ButtonInput<KeyCode>>,
    bindings: Res<bevy_simple_text_input::TextInputNavigationBindings>,
    browsers: NonSend<Browsers>,
) {
    use bevy_simple_text_input::TextInputAction;
    use cef_offscreen::prelude::EditCommand;
    let Some(state) = state else { return };
    for event in er.read() {
        if event.state != bevy::input::ButtonState::Pressed {
            continue;
        }
        let action = bindings
            .0
            .iter()
            .filter(|(_, binding)| binding.modifiers().iter().all(|m| input.pressed(*m)))
            .find(|(_, binding)| binding.key() == event.key_code)
            .map(|(action, _)| action);
        let command = match action {
            Some(TextInputAction::Undo) => EditCommand::Undo,
            Some(TextInputAction::Redo) => EditCommand::Redo,
            Some(TextInputAction::Cut) => EditCommand::Cut,
            Some(TextInputAction::Copy) => EditCommand::Copy,
            Some(TextInputAction::Paste) => EditCommand::Paste,
            Some(TextInputAction::SelectAll) => EditCommand::SelectAll,
            // Caret movement: Cmd(meta)+arrow chords are AppKit's job in a windowed browser, so
            // Blink ignores them raw on mac — send the Blink editor command instead. Everything
            // else (plain/shift arrows, ALT+arrow word movement, backspace, enter) Blink handles
            // natively from the forwarded keydown; translating those too would double-execute.
            Some(nav) => {
                let select =
                    input.pressed(KeyCode::ShiftLeft) || input.pressed(KeyCode::ShiftRight);
                let command = match (nav, select) {
                    (TextInputAction::LineStart, false) => "MoveToBeginningOfLine",
                    (TextInputAction::LineStart, true) => "MoveToBeginningOfLineAndModifySelection",
                    (TextInputAction::LineEnd, false) => "MoveToEndOfLine",
                    (TextInputAction::LineEnd, true) => "MoveToEndOfLineAndModifySelection",
                    (TextInputAction::TextStart, false) => "MoveToBeginningOfDocument",
                    (TextInputAction::TextStart, true) => {
                        "MoveToBeginningOfDocumentAndModifySelection"
                    }
                    (TextInputAction::TextEnd, false) => "MoveToEndOfDocument",
                    (TextInputAction::TextEnd, true) => "MoveToEndOfDocumentAndModifySelection",
                    _ => continue,
                };
                browsers.execute_editor_commands(&state.hud, &[command]);
                continue;
            }
            None => continue,
        };
        browsers.execute_edit_command(&state.hud, command);
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

// page -> engine: textFocus control messages, the bridge-scene pipe, and the fallback login rpcs.
fn on_page_envelope(
    trigger: Trigger<PageEnvelope>,
    state: Option<ResMut<ReactHudCef>>,
    mut priorities: ResMut<InputPriorities>,
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
            // Keys still reach the page (CEF forwarding is unconditional), but the engine must
            // not act on them while typing. TextEntry is the level the rest of the engine keys
            // off: world consumers read at None/Scene, and input_manager's OS-shortcut
            // suppression (which clobbers raw Shift state, breaking chords like Cmd+Shift+Z)
            // switches off while the keyboard is claimed at >= TextEntry.
            if state.text_focused {
                priorities.reserve(InputType::Keyboard, InputPriority::TextEntry);
            } else {
                priorities.release(InputType::Keyboard, InputPriority::TextEntry);
            }
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
                    // false: never deploy a default profile over an unfetchable one without
                    // explicit user consent (the fallback relay has no consent UI)
                    sys.write(SystemApi::LoginPrevious(false, s));
                    state.pending_login.push((id, r));
                }
                "loginNew" => {
                    // Remote-wallet fresh sign-in: the engine opens the auth site in the user's
                    // external browser. The verification code goes to the page mid-flight as a
                    // 'loginCode' message; the rpc resolves with the final result.
                    let (sc, rc) = RpcResultSender::channel();
                    let (s, r) = RpcResultSender::channel();
                    sys.write(SystemApi::LoginNew(false, sc, s));
                    state.pending_code.push(rc);
                    state.pending_login.push((id, r));
                }
                "logout" => {
                    sys.write(SystemApi::Logout);
                    rpc_res(&mut commands, hud, &id, serde_json::Value::Null);
                }
                "loginCancel" => {
                    // Drops the engine's login task; an in-flight loginNew's result sender goes
                    // with it and pump_streams resolves that rpc as cancelled.
                    sys.write(SystemApi::LoginCancel);
                    rpc_res(&mut commands, hud, &id, serde_json::Value::Null);
                }
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

// engine -> page: resolved login RPCs. These are only armed while the fallback is driving
// (pre-bridge); if the bridge-scene takes the wire mid-flight they still resolve here.
fn pump_streams(state: Option<ResMut<ReactHudCef>>, mut commands: Commands) {
    let Some(mut state) = state else { return };
    let hud = state.hud;

    // forward loginNew verification codes as they arrive (a code error also lands on the
    // result sender, so the closed/error entries are just dropped here)
    let mut i = 0;
    while i < state.pending_code.len() {
        match state.pending_code[i].poll_once() {
            Ok(None) => i += 1,
            Ok(Some(Ok(code))) => {
                state.pending_code.remove(i);
                to_page(
                    &mut commands,
                    hud,
                    serde_json::json!({ "kind": "loginCode", "code": code.map(|c| c.to_string()) }),
                );
            }
            Ok(Some(Err(_))) | Err(()) => {
                state.pending_code.remove(i);
            }
        }
    }

    // resolve login RPCs whose engine result has arrived (a dropped sender resolves as no
    // previous user rather than leaving the page's rpc hanging)
    let mut i = 0;
    while i < state.pending_prev.len() {
        let user = match state.pending_prev[i].1.poll_once() {
            Ok(None) => {
                i += 1;
                continue;
            }
            Ok(Some(user)) => user,
            Err(()) => None,
        };
        let (id, _) = state.pending_prev.remove(i);
        rpc_res(
            &mut commands,
            hud,
            &id,
            serde_json::json!({ "userId": user }),
        );
    }
    let mut i = 0;
    while i < state.pending_login.len() {
        // A dropped sender (poll_once Err) means the engine's login task was cancelled
        // (LoginCancel) — resolve the rpc rather than leaking it.
        let result = match state.pending_login[i].1.poll_once() {
            Ok(None) => {
                i += 1;
                continue;
            }
            Ok(Some(result)) => result,
            Err(()) => Err("cancelled".to_string()),
        };
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

// Mirror of web.rs update_text_focus for the CEF page: key forwarding to the webview is
// unconditional (keyboard.rs in cef_offscreen), so while an ENGINE-side text field holds the
// keyboard (scene textinput, engine text box) the page must be told to treat keys as typing —
// otherwise useMenuShortcuts fires HUD panels off the typed letters. The shim writes this to
// `window.__engineTextFocus`, the same signal boot.js provides on web.
fn push_text_focus(
    state: Option<Res<ReactHudCef>>,
    priorities: Res<InputPriorities>,
    mut prev: Local<bool>,
    mut commands: Commands,
) {
    let Some(state) = state else { return };
    let focused = priorities.keyboard_claimed();
    if focused != *prev {
        *prev = focused;
        commands.trigger_targets(
            HostEmitEvent {
                id: "engineTextFocus".to_string(),
                payload: focused.to_string(),
            },
            state.hud,
        );
    }
}
