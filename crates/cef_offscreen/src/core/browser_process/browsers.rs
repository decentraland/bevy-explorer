use crate::core::browser_process::ClientHandlerBuilder;
#[cfg(not(target_os = "macos"))]
use crate::core::browser_process::cef_command::CefCommand;
use crate::core::browser_process::client_handler::{IpcEventRaw, JsEmitEventHandler};
use crate::core::prelude::IntoString;
use crate::core::prelude::*;
#[cfg(not(target_os = "macos"))]
use async_channel::Receiver;
use async_channel::{Sender, TryRecvError};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use cef::{
    Browser, BrowserHost, BrowserSettings, Client, CompositionUnderline, ImplBrowser,
    ImplBrowserHost, ImplFrame, ImplListValue, ImplProcessMessage, ImplRequestContext,
    MouseButtonType, ProcessId, Range, RequestContext, RequestContextSettings, WindowInfo,
    browser_host_create_browser_sync, process_message_create,
};
use cef_dll_sys::{cef_event_flags_t, cef_mouse_button_type_t};
#[allow(deprecated)]
use raw_window_handle::RawWindowHandle;
use std::cell::Cell;
use std::rc::Rc;

mod devtool_render_handler;
mod keyboard;

use crate::core::browser_process::browsers::devtool_render_handler::DevToolRenderHandlerBuilder;
use crate::core::browser_process::display_handler::{
    DisplayHandlerBuilder, SystemCursorIconSenderInner,
};
pub use keyboard::*;

pub struct WebviewBrowser {
    pub client: Browser,
    pub host: BrowserHost,
    pub size: SharedViewSize,
}

/// Editing commands the host can inject (see [`Browsers::execute_edit_command`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditCommand {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
}

/// CEF-facing browser state. Every method here touches `CefBrowser`/`CefBrowserHost`,
/// which CEF only permits on the browser-process UI thread. On macOS (external message
/// pump) that's the app main thread, so [`Browsers`] owns this directly; on
/// Windows/Linux (multi-threaded message loop) the UI thread belongs to CEF, so this
/// lives in a CEF-thread `thread_local` and is driven by `CefCommand`s (see
/// `cef_thread`).
pub(crate) struct BrowsersInner {
    browsers: HashMap<Entity, WebviewBrowser>,
    texture_sender: TextureSender,
    ime_caret: SharedImeCaret,
    dev_tools_message_id: Cell<i32>,
}

impl BrowsersInner {
    pub(crate) fn new(texture_sender: TextureSender) -> Self {
        Self {
            browsers: HashMap::default(),
            texture_sender,
            ime_caret: Rc::new(Cell::new(0)),
            dev_tools_message_id: Cell::new(0),
        }
    }
}

impl BrowsersInner {
    #[allow(clippy::too_many_arguments)]
    pub fn create_browser(
        &mut self,
        webview: Entity,
        uri: &str,
        webview_size: Vec2,
        requester: Requester,
        ipc_event_sender: Sender<IpcEventRaw>,
        system_cursor_icon_sender: SystemCursorIconSenderInner,
        _window_handle: Option<RawWindowHandle>,
    ) {
        let mut context = Self::request_context(requester);
        let size = Rc::new(Cell::new(webview_size));
        let browser = browser_host_create_browser_sync(
            Some(&WindowInfo {
                windowless_rendering_enabled: true as _,
                // macOS: the host pumps explicit begin-frames in step with the app frame
                // loop. Windows/Linux run CEF's multi-threaded message loop, where CEF
                // schedules its own paints at `windowless_frame_rate`; external
                // begin-frames would have to be marshalled cross-thread every frame for
                // no benefit (and a stalled app would freeze the HUD).
                external_begin_frame_enabled: cfg!(target_os = "macos") as _,
                #[cfg(target_os = "macos")]
                parent_view: match _window_handle {
                    Some(RawWindowHandle::AppKit(handle)) => handle.ns_view.as_ptr(),
                    Some(RawWindowHandle::Win32(handle)) => handle.hwnd.get() as _,
                    Some(RawWindowHandle::Xlib(handle)) => handle.window as _,
                    Some(RawWindowHandle::Wayland(handle)) => handle.surface.as_ptr(),
                    _ => std::ptr::null_mut(),
                },
                // shared_texture_enabled: true as _,
                ..Default::default()
            }),
            Some(&mut self.client_handler(
                webview,
                size.clone(),
                ipc_event_sender,
                system_cursor_icon_sender,
            )),
            Some(&uri.into()),
            Some(&BrowserSettings {
                windowless_frame_rate: 60,
                ..Default::default()
            }),
            None,
            context.as_mut(),
        );
        // Don't panic: on Windows/Linux this runs inside a CEF task callback, and
        // unwinding across that FFI boundary is UB.
        let Some(browser) = browser else {
            error!("Failed to create browser for webview {webview:?}");
            return;
        };
        let Some(host) = browser.host() else {
            error!("Failed to get browser host for webview {webview:?}");
            return;
        };
        self.browsers.insert(
            webview,
            WebviewBrowser {
                host,
                client: browser,
                size,
            },
        );
    }

    #[cfg(target_os = "macos")]
    pub fn send_external_begin_frame(&mut self) {
        for browser in self.browsers.values_mut() {
            browser.host.send_external_begin_frame();
        }
    }

    pub fn send_mouse_move(
        &self,
        webview: &Entity,
        modifiers: u32,
        position: Vec2,
        mouse_leave: bool,
    ) {
        if let Some(browser) = self.get_focused_browser(webview) {
            let mouse_event = cef::MouseEvent {
                x: position.x as i32,
                y: position.y as i32,
                modifiers,
            };
            browser
                .host
                .send_mouse_move_event(Some(&mouse_event), mouse_leave as _);
        }
    }

    pub fn send_mouse_click(
        &self,
        webview: &Entity,
        position: Vec2,
        button: MouseButton,
        mouse_up: bool,
        click_count: i32,
    ) {
        if let Some(browser) = self.get_focused_browser(webview) {
            let mouse_event = cef::MouseEvent {
                x: position.x as i32,
                y: position.y as i32,
                modifiers: match button {
                    MouseButton::Right => cef_event_flags_t::EVENTFLAG_RIGHT_MOUSE_BUTTON,
                    MouseButton::Middle => cef_event_flags_t::EVENTFLAG_MIDDLE_MOUSE_BUTTON,
                    _ => cef_event_flags_t::EVENTFLAG_LEFT_MOUSE_BUTTON,
                } as _, // No modifiers for simplicity
            };
            let mouse_button = match button {
                MouseButton::Right => cef_mouse_button_type_t::MBT_RIGHT,
                MouseButton::Middle => cef_mouse_button_type_t::MBT_MIDDLE,
                // Back/Forward/Other have no CEF equivalent; treat as primary
                _ => cef_mouse_button_type_t::MBT_LEFT,
            };
            browser.host.set_focus(true as _);
            browser.host.send_mouse_click_event(
                Some(&mouse_event),
                MouseButtonType::from(mouse_button),
                mouse_up as _,
                click_count,
            );
        }
    }

    /// [`SendMouseWheelEvent`](https://cef-builds.spotifycdn.com/docs/106.1/classCefBrowserHost.html#acd5d057bd5230baa9a94b7853ba755f7)
    pub fn send_mouse_wheel(&self, webview: &Entity, position: Vec2, delta: Vec2) {
        if let Some(browser) = self.get_focused_browser(webview) {
            let mouse_event = cef::MouseEvent {
                x: position.x as i32,
                y: position.y as i32,
                modifiers: 0,
            };
            browser
                .host
                .send_mouse_wheel_event(Some(&mouse_event), delta.x as _, delta.y as _);
        }
    }

    #[inline]
    pub fn send_key(&self, webview: &Entity, event: cef::KeyEvent) {
        if let Some(browser) = self.get_focused_browser(webview) {
            browser.host.send_key_event(Some(&event));
        }
    }

    pub fn execute_edit_command(&self, webview: &Entity, command: EditCommand) {
        if let Some(browser) = self.browsers.get(webview)
            && let Some(frame) = browser.client.focused_frame()
        {
            match command {
                EditCommand::Undo => frame.undo(),
                EditCommand::Redo => frame.redo(),
                EditCommand::Cut => frame.cut(),
                EditCommand::Copy => frame.copy(),
                EditCommand::Paste => frame.paste(),
                EditCommand::SelectAll => frame.select_all(),
            }
        }
    }

    pub fn execute_editor_commands(&self, webview: &Entity, commands: &[&str]) {
        if let Some(browser) = self.browsers.get(webview) {
            let id = self.dev_tools_message_id.get().wrapping_add(1).max(1);
            self.dev_tools_message_id.set(id);
            let message = serde_json::json!({
                "id": id,
                "method": "Input.dispatchKeyEvent",
                "params": {
                    "type": "rawKeyDown",
                    "windowsVirtualKeyCode": 0,
                    "nativeVirtualKeyCode": 0,
                    "commands": commands,
                }
            });
            browser
                .host
                .send_dev_tools_message(Some(message.to_string().as_bytes()));
        }
    }

    pub fn emit_event(&self, webview: &Entity, id: impl Into<String>, event: &serde_json::Value) {
        if let Some(mut process_message) =
            process_message_create(Some(&PROCESS_MESSAGE_HOST_EMIT.into()))
            && let Some(argument_list) = process_message.argument_list()
            && let Some(browser) = self.browsers.get(webview)
            && let Some(frame) = browser.client.main_frame()
        {
            argument_list.set_string(0, Some(&id.into().as_str().into()));
            argument_list.set_string(1, Some(&event.to_string().as_str().into()));
            frame.send_process_message(
                ProcessId::from(cef_dll_sys::cef_process_id_t::PID_RENDERER),
                Some(&mut process_message),
            );
        };
    }

    pub fn resize(&self, webview: &Entity, size: Vec2) {
        if let Some(browser) = self.browsers.get(webview) {
            browser.size.set(size);
            browser.host.was_resized();
        }
    }

    /// Closes the browser associated with the given webview entity.
    ///
    /// The browser will be removed from the hash map after closing.
    pub fn close(&mut self, webview: &Entity) {
        if let Some(browser) = self.browsers.remove(webview) {
            browser.host.close_browser(true as _);
            debug!("Closed browser with webview: {:?}", webview);
        }
    }

    /// Shows the DevTools for the specified webview.
    pub fn show_devtool(&self, webview: &Entity) {
        let Some(browser) = self.browsers.get(webview) else {
            return;
        };
        browser.host.show_dev_tools(
            Some(&WindowInfo::default()),
            Some(&mut ClientHandlerBuilder::new(DevToolRenderHandlerBuilder::build()).build()),
            Some(&BrowserSettings::default()),
            None,
        );
    }

    /// Closes the DevTools for the specified webview.
    pub fn close_devtools(&self, webview: &Entity) {
        if let Some(browser) = self.browsers.get(webview) {
            browser.host.close_dev_tools();
        }
    }

    #[inline]
    pub fn reload(&self) {
        for browser in self.browsers.values() {
            if let Some(frame) = browser.client.main_frame() {
                let url = frame.url().into_string();
                info!("Reloading browser with URL: {}", url);
                frame.load_url(Some(&url.as_str().into()));
            }
        }
    }

    /// ## Reference
    ///
    /// - [`ImeSetComposition`](https://cef-builds.spotifycdn.com/docs/122.0/classCefBrowserHost.html#a567b41fb2d3917843ece3b57adc21ebe)
    pub fn set_ime_composition(&self, text: &str, cursor_utf16: Option<u32>) {
        let underlines = make_underlines_for(text, cursor_utf16.map(|i| (i, i)));
        let i = text.encode_utf16().count();
        let selection_range = Range {
            from: i as _,
            to: i as _,
        };
        let replacement_range = self.ime_caret_range();
        for browser in self
            .browsers
            .values()
            .filter(|b| b.client.focused_frame().is_some())
        {
            browser.host.ime_set_composition(
                Some(&text.into()),
                underlines.len(),
                Some(&underlines[0]),
                Some(&replacement_range),
                Some(&selection_range),
            );
        }
    }

    /// ## Reference
    ///
    /// [`ImeSetComposition`](https://cef-builds.spotifycdn.com/docs/122.0/classCefBrowserHost.html#a567b41fb2d3917843ece3b57adc21ebe)
    pub fn ime_finish_composition(&self, keep_selection: bool) {
        for browser in self
            .browsers
            .values()
            .filter(|b| b.client.focused_frame().is_some())
        {
            browser.host.ime_finish_composing_text(keep_selection as _);
        }
    }

    pub fn set_ime_commit_text(&self, text: &str) {
        let replacement_range = self.ime_caret_range();
        for browser in self
            .browsers
            .values()
            .filter(|b| b.client.focused_frame().is_some())
        {
            browser
                .host
                .ime_commit_text(Some(&text.into()), Some(&replacement_range), 0)
        }
    }

    fn request_context(requester: Requester) -> Option<RequestContext> {
        let mut context = cef::request_context_create_context(
            Some(&RequestContextSettings::default()),
            Some(&mut RequestContextHandlerBuilder::build()),
        );
        if let Some(context) = context.as_mut() {
            context.register_scheme_handler_factory(
                Some(&SCHEME_CEF.into()),
                Some(&HOST_CEF.into()),
                Some(&mut LocalSchemaHandlerBuilder::build(requester)),
            );
        }
        context
    }

    fn client_handler(
        &self,
        webview: Entity,
        size: SharedViewSize,
        ipc_event_sender: Sender<IpcEventRaw>,
        system_cursor_icon_sender: SystemCursorIconSenderInner,
    ) -> Client {
        ClientHandlerBuilder::new(RenderHandlerBuilder::build(
            webview,
            self.texture_sender.clone(),
            size.clone(),
            self.ime_caret.clone(),
        ))
        .with_display_handler(DisplayHandlerBuilder::build(system_cursor_icon_sender))
        .with_message_handler(JsEmitEventHandler::new(webview, ipc_event_sender))
        .build()
    }

    #[inline]
    fn ime_caret_range(&self) -> Range {
        let caret = self.ime_caret.get();
        Range {
            from: caret,
            to: caret,
        }
    }

    #[inline]
    fn get_focused_browser(&self, webview: &Entity) -> Option<&WebviewBrowser> {
        self.browsers
            .get(webview)
            .and_then(|b| b.client.focused_frame().is_some().then_some(b))
    }
}

/// Bevy-side handle to the CEF browsers, kept as a `NonSend` resource. The API is the
/// same on all platforms; what differs is where the calls run. CEF only allows browser
/// calls on the browser-process UI thread — off it they fail, silently in release
/// builds (`CreateBrowserSync` returns null). On macOS the external message pump makes
/// the app main thread that thread, so methods call [`BrowsersInner`] directly; on
/// Windows/Linux the multi-threaded message loop owns it, so methods enqueue
/// `CefCommand`s that the webview plugin flushes to the CEF UI thread each frame
/// (see `cef_thread`).
pub struct Browsers {
    #[cfg(target_os = "macos")]
    inner: BrowsersInner,
    #[cfg(not(target_os = "macos"))]
    commands: Sender<CefCommand>,
    #[cfg(not(target_os = "macos"))]
    commands_rx: Receiver<CefCommand>,
    #[cfg(not(target_os = "macos"))]
    texture_sender: TextureSender,
    receiver: TextureReceiver,
}

impl Default for Browsers {
    fn default() -> Self {
        let (sender, receiver) = async_channel::unbounded::<RenderTexture>();
        #[cfg(target_os = "macos")]
        {
            Browsers {
                inner: BrowsersInner::new(sender),
                receiver,
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let (commands, commands_rx) = async_channel::unbounded();
            Browsers {
                commands,
                commands_rx,
                texture_sender: sender,
                receiver,
            }
        }
    }
}

impl Browsers {
    #[allow(clippy::too_many_arguments)]
    pub fn create_browser(
        &mut self,
        webview: Entity,
        uri: &str,
        webview_size: Vec2,
        requester: Requester,
        ipc_event_sender: Sender<IpcEventRaw>,
        system_cursor_icon_sender: SystemCursorIconSenderInner,
        _window_handle: Option<RawWindowHandle>,
    ) {
        #[cfg(target_os = "macos")]
        self.inner.create_browser(
            webview,
            uri,
            webview_size,
            requester,
            ipc_event_sender,
            system_cursor_icon_sender,
            _window_handle,
        );
        // the window handle is only consumed on macOS (parent_view); it isn't Send, so
        // it never rides a command
        #[cfg(not(target_os = "macos"))]
        self.send(CefCommand::CreateBrowser {
            webview,
            uri: uri.to_owned(),
            size: webview_size,
            requester,
            ipc_event_sender,
            cursor_icon_sender: system_cursor_icon_sender,
        });
    }

    #[cfg(target_os = "macos")]
    pub fn send_external_begin_frame(&mut self) {
        self.inner.send_external_begin_frame();
    }

    pub fn send_mouse_move<'a>(
        &self,
        webview: &Entity,
        buttons: impl IntoIterator<Item = &'a MouseButton>,
        position: Vec2,
        mouse_leave: bool,
    ) {
        let modifiers = modifiers_from_mouse_buttons(buttons);
        #[cfg(target_os = "macos")]
        self.inner
            .send_mouse_move(webview, modifiers, position, mouse_leave);
        #[cfg(not(target_os = "macos"))]
        self.send(CefCommand::MouseMove {
            webview: *webview,
            modifiers,
            position,
            mouse_leave,
        });
    }

    /// `click_count` follows Chromium's convention (1 = single, 2 = double-click word select,
    /// 3 = triple-click paragraph select); the caller synthesizes it from click cadence since
    /// there's no OS window to do it.
    pub fn send_mouse_click(
        &self,
        webview: &Entity,
        position: Vec2,
        button: MouseButton,
        mouse_up: bool,
        click_count: i32,
    ) {
        #[cfg(target_os = "macos")]
        self.inner
            .send_mouse_click(webview, position, button, mouse_up, click_count);
        #[cfg(not(target_os = "macos"))]
        self.send(CefCommand::MouseClick {
            webview: *webview,
            position,
            button,
            mouse_up,
            click_count,
        });
    }

    /// [`SendMouseWheelEvent`](https://cef-builds.spotifycdn.com/docs/106.1/classCefBrowserHost.html#acd5d057bd5230baa9a94b7853ba755f7)
    pub fn send_mouse_wheel(&self, webview: &Entity, position: Vec2, delta: Vec2) {
        #[cfg(target_os = "macos")]
        self.inner.send_mouse_wheel(webview, position, delta);
        #[cfg(not(target_os = "macos"))]
        self.send(CefCommand::MouseWheel {
            webview: *webview,
            position,
            delta,
        });
    }

    pub fn send_key(&self, webview: &Entity, event: cef::KeyEvent) {
        #[cfg(target_os = "macos")]
        self.inner.send_key(webview, event);
        #[cfg(not(target_os = "macos"))]
        self.send(CefCommand::Key {
            webview: *webview,
            event,
        });
    }

    /// Run an editing command on the webview's focused frame. Needed on macOS, where Blink
    /// leaves Cmd shortcuts (undo/copy/...) to the AppKit menu a windowed browser would have —
    /// in offscreen rendering there is none, so the host translates them itself.
    pub fn execute_edit_command(&self, webview: &Entity, command: EditCommand) {
        #[cfg(target_os = "macos")]
        self.inner.execute_edit_command(webview, command);
        #[cfg(not(target_os = "macos"))]
        self.send(CefCommand::EditCommand {
            webview: *webview,
            command,
        });
    }

    /// Run named Blink editor commands (e.g. "MoveToBeginningOfLineAndModifySelection") on the
    /// webview. Caret/selection movement has no CefFrame equivalent; the DevTools protocol's
    /// `Input.dispatchKeyEvent` `commands` field is the OSR channel for it (what Puppeteer uses
    /// for macOS editing emulation). The synthetic event carries no key, so the page sees
    /// nothing beyond the command's effect.
    pub fn execute_editor_commands(&self, webview: &Entity, commands: &[&str]) {
        #[cfg(target_os = "macos")]
        self.inner.execute_editor_commands(webview, commands);
        #[cfg(not(target_os = "macos"))]
        self.send(CefCommand::EditorCommands {
            webview: *webview,
            commands: commands.iter().map(|c| (*c).to_owned()).collect(),
        });
    }

    pub fn emit_event(&self, webview: &Entity, id: impl Into<String>, event: &serde_json::Value) {
        #[cfg(target_os = "macos")]
        self.inner.emit_event(webview, id, event);
        #[cfg(not(target_os = "macos"))]
        self.send(CefCommand::EmitEvent {
            webview: *webview,
            id: id.into(),
            event: event.clone(),
        });
    }

    pub fn resize(&self, webview: &Entity, size: Vec2) {
        #[cfg(target_os = "macos")]
        self.inner.resize(webview, size);
        #[cfg(not(target_os = "macos"))]
        self.send(CefCommand::Resize {
            webview: *webview,
            size,
        });
    }

    /// Closes the browser associated with the given webview entity.
    pub fn close(&mut self, webview: &Entity) {
        #[cfg(target_os = "macos")]
        self.inner.close(webview);
        #[cfg(not(target_os = "macos"))]
        self.send(CefCommand::Close { webview: *webview });
    }

    #[inline]
    pub fn try_receive_texture(&self) -> core::result::Result<RenderTexture, TryRecvError> {
        self.receiver.try_recv()
    }

    /// Shows the DevTools for the specified webview.
    pub fn show_devtool(&self, webview: &Entity) {
        #[cfg(target_os = "macos")]
        self.inner.show_devtool(webview);
        #[cfg(not(target_os = "macos"))]
        self.send(CefCommand::ShowDevtool { webview: *webview });
    }

    /// Closes the DevTools for the specified webview.
    pub fn close_devtools(&self, webview: &Entity) {
        #[cfg(target_os = "macos")]
        self.inner.close_devtools(webview);
        #[cfg(not(target_os = "macos"))]
        self.send(CefCommand::CloseDevtools { webview: *webview });
    }

    /// Reload every webview from its current URL (used for asset hot-reload).
    pub fn reload(&self) {
        #[cfg(target_os = "macos")]
        self.inner.reload();
        #[cfg(not(target_os = "macos"))]
        self.send(CefCommand::Reload);
    }

    /// ## Reference
    ///
    /// - [`ImeSetComposition`](https://cef-builds.spotifycdn.com/docs/122.0/classCefBrowserHost.html#a567b41fb2d3917843ece3b57adc21ebe)
    pub fn set_ime_composition(&self, text: &str, cursor_utf16: Option<u32>) {
        #[cfg(target_os = "macos")]
        self.inner.set_ime_composition(text, cursor_utf16);
        #[cfg(not(target_os = "macos"))]
        self.send(CefCommand::ImeSetComposition {
            text: text.to_owned(),
            cursor_utf16,
        });
    }

    /// ## Reference
    ///
    /// [`ImeFinishComposingText`](https://cef-builds.spotifycdn.com/docs/122.0/classCefBrowserHost.html#a567b41fb2d3917843ece3b57adc21ebe)
    pub fn ime_finish_composition(&self, keep_selection: bool) {
        #[cfg(target_os = "macos")]
        self.inner.ime_finish_composition(keep_selection);
        #[cfg(not(target_os = "macos"))]
        self.send(CefCommand::ImeFinishComposition { keep_selection });
    }

    pub fn set_ime_commit_text(&self, text: &str) {
        #[cfg(target_os = "macos")]
        self.inner.set_ime_commit_text(text);
        #[cfg(not(target_os = "macos"))]
        self.send(CefCommand::ImeCommitText {
            text: text.to_owned(),
        });
    }

    #[cfg(not(target_os = "macos"))]
    fn send(&self, command: CefCommand) {
        // unbounded channel: try_send only fails if closed, and we hold the receiver too
        let _ = self.commands.try_send(command);
    }

    /// Post the one-off task that creates the CEF-thread browser state. Must be called
    /// after `cef::initialize` (MessageLoopPlugin) and before the first drain.
    #[cfg(not(target_os = "macos"))]
    pub(crate) fn post_init_task(&self) {
        super::cef_thread::post_init(self.texture_sender.clone());
    }

    /// True if commands are queued for the CEF UI thread.
    #[cfg(not(target_os = "macos"))]
    pub(crate) fn commands_pending(&self) -> bool {
        !self.commands_rx.is_empty()
    }

    /// Post a task to the CEF UI thread draining and executing all queued commands.
    #[cfg(not(target_os = "macos"))]
    pub(crate) fn post_drain_task(&self) {
        super::cef_thread::post_drain(self.commands_rx.clone());
    }
}

pub fn modifiers_from_mouse_buttons<'a>(buttons: impl IntoIterator<Item = &'a MouseButton>) -> u32 {
    let mut modifiers = cef_event_flags_t::EVENTFLAG_NONE as u32;
    for button in buttons {
        match button {
            MouseButton::Left => modifiers |= cef_event_flags_t::EVENTFLAG_LEFT_MOUSE_BUTTON as u32,
            MouseButton::Right => {
                modifiers |= cef_event_flags_t::EVENTFLAG_RIGHT_MOUSE_BUTTON as u32
            }
            MouseButton::Middle => {
                modifiers |= cef_event_flags_t::EVENTFLAG_MIDDLE_MOUSE_BUTTON as u32
            }
            _ => {}
        }
    }
    modifiers
}

pub fn make_underlines_for(
    text: &str,
    selection_utf16: Option<(u32, u32)>,
) -> Vec<CompositionUnderline> {
    let len16 = utf16_len(text);

    let base = CompositionUnderline {
        size: size_of::<CompositionUnderline>(),
        range: Range { from: 0, to: len16 },
        color: 0,
        background_color: 0,
        thick: 0,
        style: Default::default(),
    };

    if let Some((from, to)) = selection_utf16
        && from < to
    {
        let sel = CompositionUnderline {
            size: size_of::<CompositionUnderline>(),
            range: Range { from, to },
            color: 0,
            background_color: 0,
            thick: 1,
            style: Default::default(),
        };
        return vec![base, sel];
    }
    vec![base]
}

#[inline]
fn utf16_len(s: &str) -> u32 {
    s.encode_utf16().count() as u32
}

#[allow(dead_code)]
fn utf16_index_from_byte(s: &str, byte_idx: usize) -> u32 {
    s[..byte_idx].encode_utf16().count() as u32
}

#[cfg(test)]
mod tests {
    use crate::core::prelude::modifiers_from_mouse_buttons;
    use bevy::prelude::*;

    #[test]
    fn test_modifiers_from_mouse_buttons() {
        let buttons = vec![&MouseButton::Left, &MouseButton::Right];
        let modifiers = modifiers_from_mouse_buttons(buttons);
        assert_eq!(
            modifiers,
            cef_dll_sys::cef_event_flags_t::EVENTFLAG_LEFT_MOUSE_BUTTON as u32
                | cef_dll_sys::cef_event_flags_t::EVENTFLAG_RIGHT_MOUSE_BUTTON as u32
        );
    }
}
