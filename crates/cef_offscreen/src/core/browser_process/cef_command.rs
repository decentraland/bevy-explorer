//! `Send`-able commands for driving CEF browsers under the multi-threaded message loop
//! (Windows/Linux), where browser objects live on CEF's internal UI thread. The
//! bevy-side [`Browsers`](super::Browsers) enqueues these and `cef_thread` drains and
//! executes them on the CEF UI thread. (macOS pumps CEF on the main thread and calls
//! into the browser state directly — no commands involved.)

use super::EditCommand;
use super::client_handler::IpcEventRaw;
use super::display_handler::SystemCursorIconSenderInner;
use super::localhost::Requester;
use async_channel::Sender;
use bevy::prelude::*;

pub(crate) enum CefCommand {
    CreateBrowser {
        webview: Entity,
        uri: String,
        size: Vec2,
        requester: Requester,
        ipc_event_sender: Sender<IpcEventRaw>,
        cursor_icon_sender: SystemCursorIconSenderInner,
    },
    MouseMove {
        webview: Entity,
        modifiers: u32,
        position: Vec2,
        mouse_leave: bool,
    },
    MouseClick {
        webview: Entity,
        position: Vec2,
        button: MouseButton,
        mouse_up: bool,
        click_count: i32,
    },
    MouseWheel {
        webview: Entity,
        position: Vec2,
        delta: Vec2,
    },
    Key {
        webview: Entity,
        event: cef::KeyEvent,
    },
    EditCommand {
        webview: Entity,
        command: EditCommand,
    },
    EditorCommands {
        webview: Entity,
        commands: Vec<String>,
    },
    EmitEvent {
        webview: Entity,
        id: String,
        event: serde_json::Value,
    },
    Resize {
        webview: Entity,
        size: Vec2,
    },
    Close {
        webview: Entity,
    },
    ShowDevtool {
        webview: Entity,
    },
    CloseDevtools {
        webview: Entity,
    },
    Reload,
    ImeSetComposition {
        text: String,
        cursor_utf16: Option<u32>,
    },
    ImeFinishComposition {
        keep_selection: bool,
    },
    ImeCommitText {
        text: String,
    },
}

// Commands cross from bevy's main thread to the CEF UI thread; everything they carry must
// be Send. The hop happens through cef::post_task — an FFI boundary the compiler can't
// check — so assert it here instead.
fn _assert_send(command: CefCommand) -> impl Send {
    command
}
