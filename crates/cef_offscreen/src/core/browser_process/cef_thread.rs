//! CEF-UI-thread-resident browser state for the multi-threaded message loop
//! (Windows/Linux).
//!
//! [`BrowsersInner`] holds the actual (`!Send`) CEF browser objects. CEF only allows
//! browser calls on the browser-process UI thread — off it they fail, silently in
//! release builds (e.g. `CreateBrowserSync` returns null) — and with
//! `multi_threaded_message_loop` that thread belongs to CEF, so the state lives here in
//! a `thread_local` rather than in the bevy world. The bevy-side
//! [`Browsers`](super::Browsers) enqueues [`CefCommand`]s and the webview plugin posts a
//! drain task at the end of each frame to execute them. Ported from not-elm/bevy_cef's
//! `cef_thread.rs` (see the provenance note in lib.rs).

use std::cell::RefCell;

use async_channel::Receiver;
use bevy::log::warn;
use cef::rc::{Rc, RcImpl};
use cef::{ImplTask, Task, ThreadId, WrapTask, sys};

use super::browsers::BrowsersInner;
use super::cef_command::CefCommand;
use super::renderer_handler::TextureSender;

thread_local! {
    static CEF_BROWSERS: RefCell<Option<BrowsersInner>> = const { RefCell::new(None) };
}

/// Post a task initialising the thread-local [`BrowsersInner`] on the CEF UI thread.
/// Must run once, after `cef::initialize`. Tasks posted to a thread run in posting
/// order, so posting this before any drain task is enough to sequence them.
pub(crate) fn post_init(texture_sender: TextureSender) {
    let mut task = InitBrowsersTask::build(texture_sender);
    if cef::post_task(ui_thread(), Some(&mut task)) == 0 {
        warn!("cef: failed to post browser-state init task to the CEF UI thread");
    }
}

/// Post a task draining `rx` and executing every queued [`CefCommand`] on the CEF UI
/// thread.
pub(crate) fn post_drain(rx: Receiver<CefCommand>) {
    let mut task = DrainCommandsTask::build(rx);
    if cef::post_task(ui_thread(), Some(&mut task)) == 0 {
        warn!("cef: failed to post command drain task to the CEF UI thread");
    }
}

fn ui_thread() -> ThreadId {
    ThreadId::from(cef_dll_sys::cef_thread_id_t::TID_UI)
}

impl BrowsersInner {
    fn execute(&mut self, command: CefCommand) {
        match command {
            CefCommand::CreateBrowser {
                webview,
                uri,
                size,
                requester,
                ipc_event_sender,
                cursor_icon_sender,
            } => self.create_browser(
                webview,
                &uri,
                size,
                requester,
                ipc_event_sender,
                cursor_icon_sender,
                None,
            ),
            CefCommand::MouseMove {
                webview,
                modifiers,
                position,
                mouse_leave,
            } => self.send_mouse_move(&webview, modifiers, position, mouse_leave),
            CefCommand::MouseClick {
                webview,
                position,
                button,
                mouse_up,
                click_count,
            } => self.send_mouse_click(&webview, position, button, mouse_up, click_count),
            CefCommand::MouseWheel {
                webview,
                position,
                delta,
            } => self.send_mouse_wheel(&webview, position, delta),
            CefCommand::Key { webview, event } => self.send_key(&webview, event),
            CefCommand::EditCommand { webview, command } => {
                self.execute_edit_command(&webview, command)
            }
            CefCommand::EditorCommands { webview, commands } => {
                let commands: Vec<&str> = commands.iter().map(String::as_str).collect();
                self.execute_editor_commands(&webview, &commands)
            }
            CefCommand::EmitEvent { webview, id, event } => self.emit_event(&webview, id, &event),
            CefCommand::Resize { webview, size } => self.resize(&webview, size),
            CefCommand::Close { webview } => self.close(&webview),
            CefCommand::ShowDevtool { webview } => self.show_devtool(&webview),
            CefCommand::CloseDevtools { webview } => self.close_devtools(&webview),
            CefCommand::Reload => self.reload(),
            CefCommand::ImeSetComposition { text, cursor_utf16 } => {
                self.set_ime_composition(&text, cursor_utf16)
            }
            CefCommand::ImeFinishComposition { keep_selection } => {
                self.ime_finish_composition(keep_selection)
            }
            CefCommand::ImeCommitText { text } => self.set_ime_commit_text(&text),
        }
    }
}

struct InitBrowsersTask {
    object: *mut RcImpl<sys::cef_task_t, Self>,
    texture_sender: TextureSender,
}

impl InitBrowsersTask {
    fn build(texture_sender: TextureSender) -> Task {
        Task::new(Self {
            object: std::ptr::null_mut(),
            texture_sender,
        })
    }
}

impl Rc for InitBrowsersTask {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl WrapTask for InitBrowsersTask {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_task_t, Self>) {
        self.object = object;
    }
}

impl Clone for InitBrowsersTask {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };
        Self {
            object,
            texture_sender: self.texture_sender.clone(),
        }
    }
}

impl ImplTask for InitBrowsersTask {
    fn execute(&self) {
        CEF_BROWSERS.with(|browsers| {
            *browsers.borrow_mut() = Some(BrowsersInner::new(self.texture_sender.clone()));
        });
    }

    #[inline]
    fn get_raw(&self) -> *mut sys::_cef_task_t {
        self.object.cast()
    }
}

struct DrainCommandsTask {
    object: *mut RcImpl<sys::cef_task_t, Self>,
    rx: Receiver<CefCommand>,
}

impl DrainCommandsTask {
    fn build(rx: Receiver<CefCommand>) -> Task {
        Task::new(Self {
            object: std::ptr::null_mut(),
            rx,
        })
    }
}

impl Rc for DrainCommandsTask {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl WrapTask for DrainCommandsTask {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_task_t, Self>) {
        self.object = object;
    }
}

impl Clone for DrainCommandsTask {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };
        Self {
            object,
            rx: self.rx.clone(),
        }
    }
}

impl ImplTask for DrainCommandsTask {
    fn execute(&self) {
        CEF_BROWSERS.with(|browsers| {
            let mut browsers = browsers.borrow_mut();
            let Some(browsers) = browsers.as_mut() else {
                // Can't happen with the init task posted first, but don't panic on the
                // CEF thread (unwinding across the FFI callback boundary is UB).
                warn!("cef: drain task ran before browser-state init; commands left queued");
                return;
            };
            while let Ok(command) = self.rx.try_recv() {
                browsers.execute(command);
            }
        });
    }

    #[inline]
    fn get_raw(&self) -> *mut sys::_cef_task_t {
        self.object.cast()
    }
}
