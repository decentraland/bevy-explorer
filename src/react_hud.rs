// POC: run the react-web HUD over the NATIVE bevy engine.
//
// The engine renders the 3D world to its winit window via wgpu; we host a transparent `wry` WebView
// in a borderless child window layered over it (macOS — a child *view* of bevy's Metal window never
// swaps its rendered surface to screen) and run the real react-web app inside it. Instead of the
// browser build's iframe + BroadcastChannel + SDK7 bridge scene, here a JS SHIM bridges the app's
// BroadcastChannel to wry's IPC, and THIS file is the "scene" — a native relay that maps the wire
// protocol to the engine's SystemApi.
//
// Run: serve react-web, then `cargo run --no-default-features --features react-hud`.
//   The webview loads REACT_HUD_URL (default: vite dev server with ?native=1).
//
// Implemented: login (getPreviousLogin / loginGuest / loginPrevious / logout), player-ready,
// scene-loading + chat streams, sendChat, and an F1 toggle (HUD <-> world control, since wry 0.45
// has no per-element cursor passthrough). Other domains (profile/friends/…) are not relayed yet.

use std::sync::mpsc::{channel, Receiver};

use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowResized};
use bevy::winit::WinitWindows;
use common::rpc::{RpcResultReceiver, RpcResultSender, RpcStreamReceiver, RpcStreamSender};
use common::structs::PrimaryUser;
use system_bridge::{ChatMessage, SceneLoadingUi, SystemApi};
use wry::dpi::{LogicalPosition, LogicalSize, Position, Size};
use wry::{Rect, WebView, WebViewBuilder};

pub struct ReactHudPlugin;

impl Plugin for ReactHudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, attach_react_hud); // exclusive (main-thread webview create)
        app.add_systems(
            Update,
            (
                pump_ipc,
                pump_streams,
                pump_bridge,
                player_ready,
                resize_react_hud,
                #[cfg(target_os = "macos")]
                mouse_passthrough,
            ),
        );
    }
}

struct ReactHudFailed;

// All !Send state for the overlay + relay lives in one NonSend resource.
struct ReactHud {
    webview: WebView,
    ipc_rx: Receiver<String>,
    chat_rx: RpcStreamReceiver<ChatMessage>,
    loading_rx: RpcStreamReceiver<SceneLoadingUi>,
    // Outstanding login RPCs awaiting an engine result, paired with the page's rpc id.
    pending_prev: Vec<(String, RpcResultReceiver<Option<String>>)>,
    pending_login: Vec<(String, RpcResultReceiver<Result<(), String>>)>,
    player_ready_sent: bool,
    // The page's bridge listener is live once it has sent us anything; until then events like
    // playerReady would be fired into the void (the page hasn't subscribed yet).
    page_seen: bool,
    logical_size: (f64, f64),
    // The borderless child NSWindow hosting the webview, and bevy's window (macOS), 0 elsewhere.
    overlay_window: usize,
    parent_window: usize,
    // Last cursor position sent for a hit-test, to avoid re-querying when the cursor is still.
    last_cursor: (i32, i32),
    // Cursor currently over a HUD element (overlay captures + holds key), vs the world (engine).
    over_ui: bool,
    // A HUD text field is focused — keep key on the overlay even when the cursor leaves it.
    text_focused: bool,
    // Consecutive "over world" hit-tests; used for capture->passthrough hysteresis.
    world_streak: u32,
    // When the super-user bridge-scene is driving (--ui <bridge-scene>), this is its page->scene
    // stream; react_hud becomes a pure Envelope pipe and the per-domain fallback is bypassed.
    bridge_sender: Option<RpcStreamSender<String>>,
}

// macOS: a webview attached as a child VIEW of bevy's Metal window never swaps its rendered surface
// to screen (bevy presents directly, bypassing the AppKit compositing pass), so the HUD freezes on a
// stale frame. Instead we host the webview in its own borderless, transparent child NSWindow layered
// over bevy's window — the window server composites that normally, like any web view.
#[cfg(target_os = "macos")]
mod overlay {
    use cocoa::base::{id, nil, NO, YES};
    use cocoa::foundation::NSRect;
    use objc::declare::ClassDecl;
    use objc::runtime::{Class, Object, Sel, BOOL};
    use objc::{class, msg_send, sel, sel_impl};
    use raw_window_handle::{
        AppKitWindowHandle, HandleError, HasWindowHandle, RawWindowHandle, WindowHandle,
    };
    use std::ffi::c_void;
    use std::ptr::NonNull;
    use std::sync::Once;

    extern "C" fn yes(_: &Object, _: Sel) -> BOOL {
        YES
    }

    // The overlay is a non-activating NSPanel: it receives clicks WITHOUT stealing focus from bevy's
    // window (so no key-focus fight — the previous design re-grabbed key every frame and every click
    // got eaten as a re-activation). canBecomeKeyWindow=YES so it can still take key for chat typing
    // (only when a text field needs it, via becomesKeyOnlyIfNeeded).
    fn overlay_class() -> *const Class {
        static mut CLASS: *const Class = std::ptr::null();
        static REGISTER: Once = Once::new();
        REGISTER.call_once(|| unsafe {
            let mut decl = ClassDecl::new("ReactHudOverlayPanel", class!(NSPanel)).unwrap();
            decl.add_method(
                sel!(canBecomeKeyWindow),
                yes as extern "C" fn(&Object, Sel) -> BOOL,
            );
            CLASS = decl.register();
        });
        unsafe { CLASS }
    }

    // A raw AppKit content-view handle so wry can host the webview inside our overlay window.
    pub struct ContentView(pub usize);
    impl HasWindowHandle for ContentView {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            let ptr = NonNull::new(self.0 as *mut c_void).ok_or(HandleError::Unavailable)?;
            let h = AppKitWindowHandle::new(ptr);
            // SAFETY: the pointer is a live NSView for the overlay window's lifetime.
            Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::AppKit(h)) })
        }
    }

    unsafe fn parent_window(parent: &impl HasWindowHandle) -> Option<id> {
        let handle = parent.window_handle().ok()?;
        let RawWindowHandle::AppKit(h) = handle.as_raw() else {
            return None;
        };
        let ns_view = h.ns_view.as_ptr() as id;
        let win: id = msg_send![ns_view, window];
        if win.is_null() {
            None
        } else {
            Some(win)
        }
    }

    unsafe fn content_rect(pwin: id) -> NSRect {
        let pframe: NSRect = msg_send![pwin, frame];
        msg_send![pwin, contentRectForFrameRect: pframe]
    }

    // Create a borderless transparent NON-ACTIVATING child panel over the parent's content area.
    // Returns (overlay_window, overlay_content_view, parent_window) pointers.
    pub fn create(parent: &impl HasWindowHandle) -> Option<(usize, usize, usize)> {
        unsafe {
            let pwin = parent_window(parent)?;
            let rect = content_rect(pwin);

            let win: id = msg_send![overlay_class() as id, alloc];
            // NSWindowStyleMaskNonactivatingPanel(128) | borderless(0); backing 2 = buffered.
            let win: id =
                msg_send![win, initWithContentRect: rect styleMask: 128u64 backing: 2u64 defer: NO];
            let _: () = msg_send![win, setOpaque: NO];
            let clear: id = msg_send![class!(NSColor), clearColor];
            let _: () = msg_send![win, setBackgroundColor: clear];
            let _: () = msg_send![win, setHasShadow: NO];
            let _: () = msg_send![win, setAcceptsMouseMovedEvents: YES]; // CSS :hover tracking
            // Take key only when a control (text field) needs it — clicks on buttons/world don't steal
            // focus, so bevy keeps keyboard for movement; chat typing still works.
            let _: () = msg_send![win, setBecomesKeyOnlyIfNeeded: YES];
            let cv: id = msg_send![win, contentView];
            let _: () = msg_send![cv, setWantsLayer: YES];

            // Add as a child (follows the parent's moves); order it just above the parent WITHOUT
            // making it key (non-activating — no focus steal, no key-fight).
            let _: () = msg_send![pwin, addChildWindow: win ordered: 1i64]; // NSWindowAbove
            let _: () = msg_send![win, setIgnoresMouseEvents: NO];
            let _: () = msg_send![win, orderFront: nil];
            Some((win as usize, cv as usize, pwin as usize))
        }
    }

    // Keep the panel ordered above bevy's window WITHOUT taking key focus (non-activating). The old
    // Per-pixel mouse routing: capture over HUD, pass through to the engine over the world.
    pub fn set_ignore_mouse(window: usize, ignore: bool) {
        if window == 0 {
            return;
        }
        unsafe {
            let _: () = msg_send![window as id, setIgnoresMouseEvents: if ignore { YES } else { NO }];
        }
    }

    // Give key (keyboard) focus to the overlay (HUD: hover/click/typing).
    pub fn make_key(window: usize) {
        if window == 0 {
            return;
        }
        unsafe {
            let _: () = msg_send![window as id, makeKeyAndOrderFront: nil];
        }
    }

    // Hand key focus back to bevy's window (movement/camera keyboard). Use makeKeyWindow, NOT
    // makeKeyAndOrderFront — ordering bevy front drops the child panel BEHIND it (occlusionVisible=false
    // => HUD vanishes). We only need keyboard focus to move; the panel stays pinned above for visuals.
    pub fn make_parent_key(parent: usize) {
        if parent == 0 {
            return;
        }
        unsafe {
            let _: () = msg_send![parent as id, makeKeyWindow];
        }
    }

    // The wry webview NSView (last subview of the overlay's content view).
    unsafe fn webview_view(window: id) -> Option<id> {
        let content: id = msg_send![window, contentView];
        let subviews: id = msg_send![content, subviews];
        if subviews.is_null() {
            return None;
        }
        let count: usize = msg_send![subviews, count];
        if count == 0 {
            return None;
        }
        Some(msg_send![subviews, objectAtIndex: count - 1])
    }

    // Make the webview exactly fill the overlay's content view and STAY filling across resizes. wry
    // sets the webview's autoresizingMask to 31 (all margins flexible), so on resize it drifts off and
    // the cursor/click coordinates desync. Pin it to width+height-sizable with a fixed origin (18) and
    // snap it to the current content bounds.
    pub fn fit_webview(window: usize) {
        if window == 0 {
            return;
        }
        unsafe {
            let win = window as id;
            let content: id = msg_send![win, contentView];
            let bounds: NSRect = msg_send![content, bounds];
            if let Some(wv) = webview_view(win) {
                // NSViewWidthSizable(2) | NSViewHeightSizable(16)
                let _: () = msg_send![wv, setAutoresizingMask: 18u64];
                let _: () = msg_send![wv, setFrame: bounds];
            } else {
                bevy::log::warn!("[overlay] webview subview not found while fitting to content");
            }
        }
    }

    // Cursor position in CSS/logical px (top-left origin) within the overlay window. Correct because
    // fit_webview keeps the webview exactly filling the overlay (overlay frame == webview screen rect).
    pub fn cursor_in(window: usize) -> Option<(f64, f64)> {
        if window == 0 {
            return None;
        }
        unsafe {
            let p: cocoa::foundation::NSPoint = msg_send![class!(NSEvent), mouseLocation];
            let f: NSRect = msg_send![window as id, frame];
            let x = p.x - f.origin.x;
            let y = f.size.height - (p.y - f.origin.y); // screen is bottom-left; web is top-left
            if x < 0.0 || y < 0.0 || x > f.size.width || y > f.size.height {
                None
            } else {
                Some((x, y))
            }
        }
    }

    // Keep the overlay aligned with the parent's content area (parent moves track automatically as a
    // child window; resizes do not, so call this on resize).
    pub fn reposition(window: usize, parent: &impl HasWindowHandle) {
        if window == 0 {
            return;
        }
        unsafe {
            let Some(pwin) = parent_window(parent) else {
                return;
            };
            let rect = content_rect(pwin);
            let win = window as id;
            let _: () = msg_send![win, setFrame: rect display: YES];
        }
    }

    // Re-pin the panel directly above bevy's window. macOS can drop the child below the parent across
    // a resize/maximize (occlusionVisible=false => the HUD vanishes behind the 3D view). Re-adding the
    // child window re-asserts "just above the parent" WITHOUT an orderFront race. Call this ONLY on
    // resize, never per-frame — a periodic raise reorders the window mid-click and eats the click.
    pub fn pin_above(window: usize, parent: usize) {
        if window == 0 || parent == 0 {
            return;
        }
        unsafe {
            // Re-establish the child link (re-orders above the parent) AND orderFront so the panel
            // actually re-shows after the app was deactivated (alt-tab away hides child windows; a bare
            // re-add doesn't necessarily bring a non-activating panel back on screen). Transition-only
            // call (resize / refocus / unlock), never per-frame, so this orderFront can't eat a click.
            let _: () = msg_send![parent as id, addChildWindow: window as id ordered: 1i64];
            let _: () = msg_send![window as id, orderFront: nil];
        }
    }

    // Keep WebKit painting when bevy isn't the frontmost app: opt the process out of App Nap and
    // disable the WKWebView's occlusion-based render throttle. `view` is the overlay content view
    // (the webview is its last subview).
    pub fn keep_rendering(view: &impl HasWindowHandle) -> String {
        let mut notes = Vec::new();
        unsafe {
            let pinfo: id = msg_send![class!(NSProcessInfo), processInfo];
            // NSActivityUserInitiated (0x00FFFFFF) | NSActivityLatencyCritical (0xFF00000000)
            let options: u64 = 0x00FF_FFFF | 0xFF_0000_0000;
            let reason = std::ffi::CString::new("react-hud keep rendering").unwrap();
            let reason: id = msg_send![class!(NSString), stringWithUTF8String: reason.as_ptr()];
            let token: id = msg_send![pinfo, beginActivityWithOptions: options reason: reason];
            let _: id = msg_send![token, retain]; // leak so it stays in effect
            notes.push("appnap-off".to_string());

            let Ok(handle) = view.window_handle() else {
                return "no handle".into();
            };
            let RawWindowHandle::AppKit(h) = handle.as_raw() else {
                return "not appkit".into();
            };
            let ns_view = h.ns_view.as_ptr() as id;
            let subviews: id = msg_send![ns_view, subviews];
            let count: usize = if subviews.is_null() {
                0
            } else {
                msg_send![subviews, count]
            };
            if count > 0 {
                let wv: id = msg_send![subviews, objectAtIndex: count - 1];
                let responds: bool =
                    msg_send![wv, respondsToSelector: sel!(_setWindowOcclusionDetectionEnabled:)];
                if responds {
                    let _: () = msg_send![wv, _setWindowOcclusionDetectionEnabled: NO];
                    notes.push("wv:occlusion-off".into());
                }
            }
        }
        notes.join(", ")
    }
}

// Bridges the react-web app's BroadcastChannel to wry IPC, and routes F1 to the toggle. Injected
// into the page so the app itself needs no transport changes.
// NOTE: do NOT bail early on a missing window.ipc — at document-start (when init scripts run) wry's
// ipc object may not exist yet. We set up the channel listener unconditionally and resolve
// window.ipc lazily at post() time (by which point React has mounted and ipc is defined).
const SHIM: &str = r#"(function(){
  var ch=new BroadcastChannel('bevy-ui-bridge');
  function post(m){ if(window.ipc&&window.ipc.postMessage){ window.ipc.postMessage(m); } }
  ch.onmessage=function(e){var env=e.data; if(env&&env.to==='scene'){ post(JSON.stringify(env)); }};
  window.__bevyToPage=function(s){ try{ ch.postMessage(JSON.parse(s)); }catch(err){} };
  // Per-pixel input passthrough: native asks "is this point over a HUD element?" (elementFromPoint
  // honours pointer-events, so transparent/world areas report 0 and route to the engine).
  window.__bevyHit=function(x,y){ try{ var el=document.elementFromPoint(x,y);
    var over=!!el && el!==document.body && el!==document.documentElement && el.id!=='root'
      && getComputedStyle(el).pointerEvents!=='none';
    post('::hit '+(over?'1':'0')); }catch(e){ post('::hit 0'); } };
  // Keyboard routing: while a text field is focused, native keeps key on the webview (typing);
  // otherwise keys go to the engine (movement).
  function typing(t){ return !!t && (t.tagName==='INPUT'||t.tagName==='TEXTAREA'||t.isContentEditable); }
  document.addEventListener('focusin',function(e){ if(typing(e.target)) post('::key 1'); },true);
  document.addEventListener('focusout',function(){ setTimeout(function(){ if(!typing(document.activeElement)) post('::key 0'); },0); },true);
})();"#;

fn react_hud_url() -> String {
    std::env::var("REACT_HUD_URL").unwrap_or_else(|_| "http://localhost:5173/?native=1".to_string())
}

fn full_bounds(w: f64, h: f64) -> Rect {
    Rect {
        position: Position::Logical(LogicalPosition::new(0.0, 0.0)),
        size: Size::Logical(LogicalSize::new(w, h)),
    }
}
// "Hidden": pushed off-screen so it neither draws nor captures input (world-control mode).
#[cfg(not(target_os = "macos"))]
fn offscreen_bounds(w: f64, h: f64) -> Rect {
    Rect {
        position: Position::Logical(LogicalPosition::new(-(w + 100.0), 0.0)),
        size: Size::Logical(LogicalSize::new(w, h)),
    }
}

// Deliver a full Envelope (JSON string) to the page's BroadcastChannel via the shim.
fn to_page_raw(webview: &WebView, envelope_json: &str) {
    let arg = serde_json::to_string(envelope_json).unwrap_or_else(|_| "\"\"".into());
    let _ = webview.evaluate_script(&format!("window.__bevyToPage({arg})"));
}
// Fallback path: wrap a SceneToPage msg in a {to:'page'} Envelope and deliver it.
fn to_page(webview: &WebView, msg: serde_json::Value) {
    let env = serde_json::json!({ "to": "page", "msg": msg }).to_string();
    to_page_raw(webview, &env);
}
fn rpc_res(webview: &WebView, id: &str, value: serde_json::Value) {
    to_page(
        webview,
        serde_json::json!({ "kind": "rpc:res", "id": id, "ok": true, "value": value }),
    );
}

fn attach_react_hud(world: &mut World) {
    if world.get_non_send_resource::<ReactHud>().is_some()
        || world.get_non_send_resource::<ReactHudFailed>().is_some()
    {
        return;
    }
    let Some(entity) = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .iter(world)
        .next()
    else {
        return;
    };

    let (ipc_tx, ipc_rx) = channel::<String>();
    let (w, h, webview, overlay_window, parent_window) = {
        let Some(winit_windows) = world.get_non_send_resource::<WinitWindows>() else {
            return;
        };
        let Some(window) = winit_windows.get_window(entity) else {
            return;
        };
        let scale = window.scale_factor();
        let phys = window.inner_size();
        let (w, h) = (phys.width as f64 / scale, phys.height as f64 / scale);
        let url = react_hud_url();
        info!("[react-hud] loading {url}");

        // macOS: host the webview in its own borderless transparent child window over bevy's window.
        #[cfg(target_os = "macos")]
        let (built, overlay_win, parent_win) = {
            let Some((win, cv, parent)) = overlay::create(&**window) else {
                error!("[react-hud] overlay window create failed");
                world.insert_non_send_resource(ReactHudFailed);
                return;
            };
            let cv_handle = overlay::ContentView(cv);
            let built = WebViewBuilder::new_as_child(&cv_handle)
                .with_transparent(true)
                .with_devtools(false) // no right-click "Inspect Element" context menu
                .with_accept_first_mouse(true) // deliver clicks even when the overlay isn't key (post-resize)
                .with_bounds(full_bounds(w, h))
                .with_initialization_script(SHIM)
                .with_ipc_handler(move |req| {
                    let _ = ipc_tx.send(req.into_body());
                })
                .with_url(&url)
                .build();
            if built.is_ok() {
                overlay::keep_rendering(&cv_handle); // opt out of App Nap / occlusion render throttle
                overlay::fit_webview(win); // pin the webview to fill the overlay (survives resizes)
            }
            (built, win, parent)
        };
        // Other platforms: fall back to a child view of bevy's window.
        #[cfg(not(target_os = "macos"))]
        let (built, overlay_win, parent_win) = {
            let built = WebViewBuilder::new_as_child(&**window)
                .with_transparent(true)
                .with_devtools(false)
                .with_accept_first_mouse(true)
                .with_bounds(full_bounds(w, h))
                .with_initialization_script(SHIM)
                .with_ipc_handler(move |req| {
                    let _ = ipc_tx.send(req.into_body());
                })
                .with_url(&url)
                .build();
            (built, 0usize, 0usize)
        };

        match built {
            Ok(wv) => (w, h, wv, overlay_win, parent_win),
            Err(e) => {
                error!("[react-hud] webview attach failed: {e}");
                world.insert_non_send_resource(ReactHudFailed);
                return;
            }
        }
    };

    // Subscribe to the engine streams the HUD needs.
    let (chat_tx, chat_rx) = RpcStreamSender::channel();
    let (loading_tx, loading_rx) = RpcStreamSender::channel();
    world.send_event(SystemApi::GetChatStream(chat_tx));
    world.send_event(SystemApi::GetSceneLoadingUiStream(loading_tx));

    world.insert_non_send_resource(ReactHud {
        webview,
        ipc_rx,
        chat_rx,
        loading_rx,
        pending_prev: Vec::new(),
        pending_login: Vec::new(),
        player_ready_sent: false,
        page_seen: false,
        logical_size: (w, h),
        overlay_window,
        parent_window,
        last_cursor: (i32::MIN, i32::MIN),
        over_ui: true, // created with ignoresMouseEvents=NO so the login screen is clickable at once
        text_focused: false,
        world_streak: 0,
        bridge_sender: None,
    });
    info!("[react-hud] webview attached + relay live (F1 toggles HUD/world)");
}

// page -> engine: handle incoming wire messages.
fn pump_ipc(hud: Option<NonSendMut<ReactHud>>, mut sys: EventWriter<SystemApi>) {
    let Some(mut hud) = hud else { return };
    let msgs: Vec<String> = hud.ipc_rx.try_iter().collect();
    if !msgs.is_empty() {
        hud.page_seen = true; // the page's bridge is live; events can now be delivered
    }
    for raw in msgs {
        // Per-pixel input routing. Over a HUD pixel the overlay captures the mouse AND takes key
        // focus (hover/click/typing); over the world, mouse + keyboard go to the engine. Capture
        // instantly; release to the world only after several consecutive "world" reads (hysteresis,
        // so transparent gaps in the emote wheel / modal padding don't flap and drop clicks).
        // Per-pixel MOUSE + KEY routing. Over a HUD pixel the panel captures the mouse AND takes key
        // focus — CSS :hover only updates for the KEY window, so without this hover is dead until a
        // click (the symptom the old non-activating-only design had). Over the world, mouse + key go
        // back to bevy (movement/camera). Key-switch ONLY on enter/leave transitions (guarded below),
        // so it's one switch per crossing, not the per-frame fight that swallowed clicks before.
        // Release to the world only after several consecutive "world" reads (hysteresis, so transparent
        // gaps in the emote wheel / modal padding don't flap and drop clicks).
        #[cfg(target_os = "macos")]
        if raw == "::hit 1" {
            hud.world_streak = 0;
            if !hud.over_ui {
                hud.over_ui = true;
                overlay::set_ignore_mouse(hud.overlay_window, false);
                overlay::make_key(hud.overlay_window); // key window => CSS :hover tracks the cursor
            }
            continue;
        }
        #[cfg(target_os = "macos")]
        if raw == "::hit 0" {
            if hud.over_ui {
                hud.world_streak += 1;
                if hud.world_streak >= 4 {
                    hud.over_ui = false;
                    overlay::set_ignore_mouse(hud.overlay_window, true);
                    // Don't yank key from a focused chat input just because the cursor drifted away.
                    if !hud.text_focused {
                        overlay::make_parent_key(hud.parent_window); // bevy gets movement keys back
                    }
                }
            }
            continue;
        }
        // Chat: a text field focused/blurred — take key for typing, then hand it back to the engine.
        #[cfg(target_os = "macos")]
        if raw == "::key 1" {
            hud.text_focused = true;
            overlay::make_key(hud.overlay_window);
            continue;
        }
        #[cfg(target_os = "macos")]
        if raw == "::key 0" {
            hud.text_focused = false;
            overlay::make_parent_key(hud.parent_window);
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        // Bridge mode: the super-user bridge-scene owns the wire protocol — pipe the raw
        // {to:'scene'} Envelope straight to it and skip the per-domain fallback below.
        if let Some(sender) = &hud.bridge_sender {
            if v.get("to").and_then(|t| t.as_str()) == Some("scene") {
                let _ = sender.send(raw); // scene gone => drop; nothing to recover
            }
            continue;
        }
        // Fallback (--ui none): unwrap the Envelope and handle a few domains directly.
        let v = v.get("msg").cloned().unwrap_or(v);
        match v.get("kind").and_then(|k| k.as_str()).unwrap_or("") {
            "rpc:req" => {
                let id = v.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                match v.get("method").and_then(|m| m.as_str()).unwrap_or("") {
                    "getPreviousLogin" => {
                        // getPreviousLogin is the page's startup signal, sent right after it
                        // subscribes. React StrictMode mounts twice, so the live listener may be the
                        // SECOND one — re-arm playerReady so it's resent to whoever is now subscribed
                        // (a one-shot fired before that strands the HUD on the loading screen).
                        hud.player_ready_sent = false;
                        let (s, r) = RpcResultSender::channel();
                        sys.write(SystemApi::GetPreviousLogin(s));
                        hud.pending_prev.push((id, r));
                    }
                    "loginGuest" => {
                        sys.write(SystemApi::LoginGuest);
                        rpc_res(&hud.webview, &id, serde_json::Value::Null);
                    }
                    "loginPrevious" | "loginIdentity" => {
                        let (s, r) = RpcResultSender::channel();
                        sys.write(SystemApi::LoginPrevious(s));
                        hud.pending_login.push((id, r));
                    }
                    "logout" => {
                        sys.write(SystemApi::Logout);
                        rpc_res(&hud.webview, &id, serde_json::Value::Null);
                    }
                    "loginCancel" => rpc_res(&hud.webview, &id, serde_json::Value::Null),
                    _ => {}
                }
            }
            "sendChat" => {
                let message = v.get("message").and_then(|m| m.as_str()).unwrap_or("");
                let channel = v.get("channel").and_then(|c| c.as_str()).unwrap_or("Nearby");
                sys.write(SystemApi::SendChat(message.to_string(), channel.to_string()));
            }
            _ => {} // other domains (profile/friends/…) not relayed yet
        }
    }
}

// Bridge mode: forward the super-user scene's page-bound Envelopes to the webview, and capture its
// page->scene stream (used by pump_ipc to deliver page messages to the scene).
fn pump_bridge(hud: Option<NonSendMut<ReactHud>>, mut events: EventReader<SystemApi>) {
    let Some(mut hud) = hud else { return };
    for ev in events.read() {
        match ev {
            SystemApi::BridgeToPage(env) => {
                to_page_raw(&hud.webview, env);
            }
            SystemApi::GetBridgeStream(sx) => {
                debug!("[react-hud] bridge-scene connected (piping Envelopes)");
                hud.bridge_sender = Some(sx.clone());
            }
            _ => {}
        }
    }
}

// engine -> page: drain streams + resolved login RPCs. Fallback only — the bridge-scene owns these
// domains when it's driving.
fn pump_streams(hud: Option<NonSendMut<ReactHud>>) {
    let Some(mut hud) = hud else { return };
    if hud.bridge_sender.is_some() {
        return;
    }

    let mut chats = Vec::new();
    while let Ok(cm) = hud.chat_rx.try_recv() {
        chats.push(cm);
    }
    let mut loadings = Vec::new();
    while let Ok(l) = hud.loading_rx.try_recv() {
        loadings.push(l);
    }
    for cm in chats {
        to_page(
            &hud.webview,
            serde_json::json!({ "kind":"chat", "chat": { "sender": cm.sender_address, "message": cm.message, "channel": cm.channel } }),
        );
    }
    for l in loadings {
        to_page(
            &hud.webview,
            serde_json::json!({ "kind":"sceneLoading", "state": { "visible": l.visible, "realmConnected": l.realm_connected, "title": l.title, "pendingAssets": l.pending_assets } }),
        );
    }

    // resolve login RPCs whose engine result has arrived
    let mut i = 0;
    while i < hud.pending_prev.len() {
        match hud.pending_prev[i].1.try_recv() {
            Ok(user) => {
                let (id, _) = hud.pending_prev.remove(i);
                rpc_res(&hud.webview, &id, serde_json::json!({ "userId": user }));
            }
            _ => i += 1,
        }
    }
    let mut i = 0;
    while i < hud.pending_login.len() {
        match hud.pending_login[i].1.try_recv() {
            Ok(result) => {
                let (id, _) = hud.pending_login.remove(i);
                let (success, error) = match result {
                    Ok(()) => (true, String::new()),
                    Err(e) => (false, e),
                };
                rpc_res(&hud.webview, &id, serde_json::json!({ "success": success, "error": error }));
            }
            _ => i += 1,
        }
    }
}

// Tell the page the player has spawned (drives entering -> world), once.
fn player_ready(hud: Option<NonSendMut<ReactHud>>, players: Query<(), With<PrimaryUser>>) {
    let Some(mut hud) = hud else { return };
    // Bridge mode: the bridge-scene emits playerReady itself.
    if hud.bridge_sender.is_some() {
        return;
    }
    // Wait for the page to subscribe — playerReady is a one-shot event; firing it before the page's
    // bridge listener exists strands the HUD on the loading screen (loadingNow stays true forever).
    if hud.player_ready_sent || !hud.page_seen || players.is_empty() {
        return;
    }
    to_page(
        &hud.webview,
        serde_json::json!({ "kind":"event", "name":"playerReady" }),
    );
    hud.player_ready_sent = true;
}

// Per-pixel mouse passthrough: when the cursor moves, ask the page whether it's over a HUD element
// (elementFromPoint honours pointer-events) and route mouse to the HUD or the engine accordingly.
// The reply (`::hit 0/1`) flips the overlay's ignoresMouseEvents in pump_ipc.
#[cfg(target_os = "macos")]
fn mouse_passthrough(
    hud: Option<NonSendMut<ReactHud>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut ticks: Local<u32>,
    mut was_locked: Local<bool>,
    mut was_focused: Local<bool>,
) {
    let Some(mut hud) = hud else { return };

    let win = windows.iter().next();
    // While bevy holds the cursor pointer-locked (camera/mouse-look mode), the OS cursor is frozen,
    // so a hit-test reads a stale point. Pass everything through to the engine.
    let locked = win
        .map(|w| w.cursor_options.grab_mode == bevy::window::CursorGrabMode::Locked)
        .unwrap_or(false);
    // App focus: alt-tab away drops it; alt-tab back (or clicking the window) restores it. On the way
    // back, bevy grabs key + z-order, leaving our over_ui state stale — the exact cause of "after
    // resizing / alt-tab it doesn't work until I right-click".
    let focused = win.map(|w| w.focused).unwrap_or(true);

    if locked {
        if !*was_locked {
            *was_locked = true;
            hud.over_ui = false;
            overlay::set_ignore_mouse(hud.overlay_window, true);
        }
        *was_focused = focused;
        return;
    }

    // Re-sync the panel after any transition that lets bevy steal key/z-order from it: leaving camera
    // lock (Escape / right-click), or the app regaining focus (alt-tab back, window resize re-focus).
    // Reset over_ui to false and invalidate last_cursor so the next hit-test runs its FULL enter
    // transition (re-takes key for CSS :hover + capture) instead of the stale half-state that used to
    // need a throwaway right-click. Re-pin above bevy so the HUD can't hide behind the 3D view.
    let unlocked = *was_locked;
    let refocused = focused && !*was_focused;
    *was_locked = false;
    *was_focused = focused;
    if unlocked || refocused {
        hud.over_ui = false;
        hud.world_streak = 0;
        hud.last_cursor = (i32::MIN, i32::MIN);
        // Default to PASS-THROUGH (not capture): the mouse + primary/secondary buttons + keyboard must
        // reach bevy so the scene's pointer events and camera mouse-look (cursor lock) keep working.
        // The hit-test below re-captures within a frame if the cursor is actually over a HUD element.
        overlay::set_ignore_mouse(hud.overlay_window, true);
        overlay::pin_above(hud.overlay_window, hud.parent_window);
    }

    // Re-evaluate ~6x/sec even without cursor movement, so a modal/menu that appears under a
    // stationary cursor (or any DOM change) still flips the overlay to capture in time for a click.
    *ticks = ticks.wrapping_add(1);
    let repoll = *ticks % 10 == 0;
    match overlay::cursor_in(hud.overlay_window) {
        Some((x, y)) => {
            let key = (x as i32, y as i32);
            if key == hud.last_cursor && !repoll {
                return; // cursor hasn't moved and not a periodic re-poll; last hit-test still valid
            }
            hud.last_cursor = key;
            let _ = hud
                .webview
                .evaluate_script(&format!("window.__bevyHit&&window.__bevyHit({x},{y})"));
        }
        None => {
            // Cursor left the window — make sure the HUD isn't holding mouse capture.
            if hud.last_cursor != (i32::MIN, i32::MIN) {
                hud.last_cursor = (i32::MIN, i32::MIN);
                overlay::set_ignore_mouse(hud.overlay_window, true);
            }
        }
    }
}

fn resize_react_hud(
    mut resized: EventReader<WindowResized>,
    hud: Option<NonSendMut<ReactHud>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    winit_windows: NonSend<WinitWindows>,
    entities: Query<Entity, With<PrimaryWindow>>,
) {
    if resized.is_empty() {
        return;
    }
    resized.clear();
    let (Some(mut hud), Ok(window)) = (hud, windows.single()) else {
        return;
    };
    let (w, h) = (window.width() as f64, window.height() as f64);
    hud.logical_size = (w, h);
    // macOS: resize the overlay to bevy's content area; the webview (pinned autoresize) refills, and
    // fit_webview snaps it exactly. Bevy auto-maximizes at startup, so this path runs every launch.
    #[cfg(target_os = "macos")]
    {
        if let Some(parent) = entities.single().ok().and_then(|e| winit_windows.get_window(e)) {
            overlay::reposition(hud.overlay_window, &**parent);
        }
        overlay::fit_webview(hud.overlay_window);
        overlay::pin_above(hud.overlay_window, hud.parent_window); // stay above bevy after maximize
        // A resize re-focuses/re-orders the window; reset over_ui + invalidate the hit-test so the next
        // mouse_passthrough re-takes key for hover/click (otherwise it stays dead until a right-click).
        // Default to pass-through so world/camera input isn't captured; the hit-test re-captures over HUD.
        hud.over_ui = false;
        hud.world_streak = 0;
        hud.last_cursor = (i32::MIN, i32::MIN);
        overlay::set_ignore_mouse(hud.overlay_window, true);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (&winit_windows, &entities);
        let _ = hud.webview.set_bounds(full_bounds(w, h));
    }
}
