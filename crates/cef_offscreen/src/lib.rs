//! CEF offscreen-rendered webview for bevy: paints a web page into a bevy `Image` via CEF's
//! offscreen rendering, serves bundled content over the `cef://localhost/` scheme (backed by the
//! bevy asset server), and bridges page JS <-> bevy through `window.cef.emit`/`listen`.
//!
//! Derived from [not-elm/bevy_cef](https://github.com/not-elm/bevy_cef) v0.1.0 (MIT OR
//! Apache-2.0 — see LICENSE-MIT / LICENSE-APACHE2 in this directory), trimmed to the
//! offscreen/fullscreen-HUD use case: worldspace (mesh/sprite) webviews, BRP/bevy_remote, page
//! navigation, zoom and audio-mute support were removed, and a number of fixes are folded in
//! (renderer V8 context leak, cursor-icon entity spray, cef:// scheme flags + query stripping,
//! per-app cache path, sibling helper + bundled framework resolution, CORS switch).
#![allow(clippy::type_complexity)]

mod components;
pub mod core;
mod cursor_icon;
mod ipc;
mod keyboard;
mod localhost;
mod message_loop;
mod webview;

use bevy::prelude::*;

pub mod prelude {
    pub use crate::CefOffscreenPlugin;
    pub use crate::components::*;
    pub use crate::core::browser_process::{Browsers, RenderTexture};
    #[cfg(target_os = "macos")]
    pub use crate::core::debug::DebugLibraryLoader;
    pub use crate::core::render_process::execute_render_process;
    pub use crate::ipc::*;
    pub use crate::keyboard::CefInputGate;
    pub use crate::webview::*;
}

/// Everything needed to host offscreen CEF webviews: CEF bootstrap + message pump, webview
/// lifecycle (spawn `CefWebviewUri` + `WebviewSize`), the cef:// asset scheme, js/host IPC,
/// keyboard forwarding (gated by [`keyboard::CefInputGate`]) and cursor-icon relay.
pub struct CefOffscreenPlugin;

impl Plugin for CefOffscreenPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            localhost::LocalHostPlugin,
            message_loop::MessageLoopPlugin::default(),
            components::WebviewCoreComponentsPlugin,
            webview::WebviewPlugin,
            ipc::IpcPlugin,
            keyboard::KeyboardPlugin,
            cursor_icon::SystemCursorIconPlugin,
        ));
    }
}
