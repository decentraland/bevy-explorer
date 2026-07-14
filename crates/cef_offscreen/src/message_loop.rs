/// NonSend marker forcing CEF pump/shutdown systems onto the main thread.
pub struct RunOnMainThread;
use crate::core::prelude::*;
use bevy::prelude::*;
use cef::args::Args;
use cef::{Settings, api_hash, execute_process, initialize, shutdown, sys};

/// Controls the CEF message loop.
///
/// - Windows and Linux: Support [`multi_threaded_message_loop`](https://cef-builds.spotifycdn.com/docs/106.1/structcef__settings__t.html#a518ac90db93ca5133a888faa876c08e0), so it is used.
///   The CEF UI thread is then internal to CEF — and browser calls are only legal on it
///   (off-thread they fail silently in release builds), so `Browsers` marshals every
///   interaction there via `CefCommand`s (see `core::browser_process::cef_thread`).
/// - macOS: MTML is unsupported; calls [`CefDoMessageLoopWork`](https://cef-builds.spotifycdn.com/docs/106.1/cef__app_8h.html#a830ae43dcdffcf4e719540204cefdb61) every frame, making
///   the main thread the CEF UI thread, so browsers are driven directly.
pub struct MessageLoopPlugin {
    _app: Box<cef::App>,
    #[cfg(target_os = "macos")]
    _loader: Box<DebugLibraryLoader>,
}

impl Plugin for MessageLoopPlugin {
    fn build(&self, app: &mut App) {
        app.insert_non_send_resource(RunOnMainThread)
            .add_systems(Update, cef_shutdown.run_if(on_event::<AppExit>));

        #[cfg(target_os = "macos")]
        app.add_systems(Main, cef_do_message_loop_work);
    }
}

impl Default for MessageLoopPlugin {
    fn default() -> Self {
        #[cfg(target_os = "macos")]
        let _loader = {
            macos::install_cef_app_protocol();
            // resolves the framework bundle-relative (Contents/Frameworks) with a dev fallback
            // at ~/.local/share/cef
            let loader = DebugLibraryLoader::new();
            assert!(loader.load());
            loader
        };

        let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

        let args = Args::new();
        let mut app = BrowserProcessAppBuilder::build();
        let ret = execute_process(
            Some(args.as_main_args()),
            Some(&mut app),
            std::ptr::null_mut(),
        );
        assert_eq!(ret, -1, "cannot execute browser process");

        let settings = Settings {
            #[cfg(target_os = "macos")]
            framework_dir_path: bundled_framework_dir()
                .unwrap_or_else(debug_chromium_embedded_framework_dir_path)
                .to_str()
                .unwrap()
                .into(),
            browser_subprocess_path: render_process_path()
                .map(|p| p.to_str().unwrap_or_default().into())
                .unwrap_or_default(),
            // We never provide CEF sandbox info (initialize gets a null sandbox_info, and on
            // linux the SUID chrome-sandbox helper can't be shipped by a source build or an
            // AppImage — with the sandbox left on, the zygote host FATALs at startup:
            // zygote_host_impl_linux.cc "Check failed: . : No such file or directory").
            no_sandbox: true as _,
            // A per-executable cache dir: the default (shared) path triggers Chromium's process
            // singleton across different host apps, which can strand/kill renderer subprocesses.
            root_cache_path: std::env::temp_dir()
                .join(format!(
                    "cef-cache-{}",
                    std::env::current_exe()
                        .ok()
                        .and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()))
                        .unwrap_or_else(|| "app".to_string())
                ))
                .to_str()
                .unwrap_or_default()
                .into(),
            windowless_rendering_enabled: true as _,
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            multi_threaded_message_loop: true as _,
            #[cfg(target_os = "macos")]
            external_message_pump: true as _,
            ..Default::default()
        };
        assert_eq!(
            initialize(
                Some(args.as_main_args()),
                Some(&settings),
                Some(&mut app),
                std::ptr::null_mut(),
            ),
            1
        );
        Self {
            _app: Box::new(app),
            #[cfg(target_os = "macos")]
            _loader: Box::new(_loader),
        }
    }
}

// The packaged layout: <exe>/../Frameworks/Chromium Embedded Framework.framework.
#[cfg(target_os = "macos")]
fn bundled_framework_dir() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe
        .parent()?
        .parent()?
        .join("Frameworks")
        .join("Chromium Embedded Framework.framework");
    dir.is_dir().then_some(dir)
}

// Prefer a render-process helper shipped NEXT TO the host executable (`<exe>-cef`), so apps can
// build and ship it like any other workspace binary with no `cargo install` step. Falls back to
// the cargo-installed bevy_cef_debug_render_process (macOS debug), else None = CEF's default
// (re-exec the host executable — never what a bevy app wants, so ship the helper).
fn render_process_path() -> Option<std::path::PathBuf> {
    if let Ok(exe) = std::env::current_exe()
        && let (Some(dir), Some(stem)) = (exe.parent(), exe.file_stem().and_then(|s| s.to_str()))
    {
        // keep the host's extension (windows: decentra-bevy.exe -> decentra-bevy-cef.exe)
        let sibling = dir
            .join(format!("{stem}-cef"))
            .with_extension(exe.extension().unwrap_or_default());
        if sibling.is_file() {
            return Some(sibling);
        }
    }
    #[cfg(target_os = "macos")]
    {
        Some(debug_render_process_path())
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
fn cef_do_message_loop_work(_: NonSend<RunOnMainThread>) {
    cef::do_message_loop_work();
}

fn cef_shutdown(_: NonSend<RunOnMainThread>) {
    shutdown();
}

#[cfg(target_os = "macos")]
mod macos {
    use core::sync::atomic::AtomicBool;
    use objc::runtime::{Class, Object, Sel};
    use objc::{sel, sel_impl};
    use std::os::raw::c_char;
    use std::os::raw::c_void;
    use std::sync::atomic::Ordering;

    unsafe extern "C" {
        fn class_addMethod(
            cls: *const Class,
            name: Sel,
            imp: *const c_void,
            types: *const c_char,
        ) -> bool;
    }

    static IS_HANDLING_SEND_EVENT: AtomicBool = AtomicBool::new(false);

    extern "C" fn is_handling_send_event(_: &Object, _: Sel) -> bool {
        IS_HANDLING_SEND_EVENT.load(Ordering::Relaxed)
    }

    extern "C" fn set_handling_send_event(_: &Object, _: Sel, flag: bool) {
        IS_HANDLING_SEND_EVENT.swap(flag, Ordering::Relaxed);
    }

    pub fn install_cef_app_protocol() {
        unsafe {
            let cls = Class::get("NSApplication").expect("NSApplication クラスが見つかりません");
            #[allow(unexpected_cfgs)]
            let sel_name = sel!(isHandlingSendEvent);
            let success = class_addMethod(
                cls as *const _,
                sel_name,
                is_handling_send_event as *const c_void,
                c"c@:".as_ptr() as *const c_char,
            );
            assert!(success, "メソッド追加に失敗しました");

            #[allow(unexpected_cfgs)]
            let sel_set = sel!(setHandlingSendEvent:);
            let success2 = class_addMethod(
                cls as *const _,
                sel_set,
                set_handling_send_event as *const c_void,
                c"v@:c".as_ptr() as *const c_char,
            );
            assert!(
                success2,
                "Failed to add setHandlingSendEvent: to NSApplication"
            );
        }
    }
}
