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
    cache_dir: std::path::PathBuf,
    #[cfg(target_os = "macos")]
    _loader: Box<DebugLibraryLoader>,
}

impl Plugin for MessageLoopPlugin {
    fn build(&self, app: &mut App) {
        app.insert_non_send_resource(RunOnMainThread)
            .insert_resource(CefCacheDir(self.cache_dir.clone()))
            .add_systems(Update, cef_shutdown.run_if(on_event::<AppExit>));

        #[cfg(target_os = "macos")]
        app.add_systems(Main, cef_do_message_loop_work);
        // .before so cef_shutdown's on_event condition sees the AppExit written here in the
        // same frame — the runner exits right after it, so a lost race skips cef shutdown
        #[cfg(unix)]
        app.add_systems(Update, exit_on_shutdown_signal.before(cef_shutdown));
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

        reap_abandoned_cache_dirs();
        let cache_dir = cef_cache_dir();

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
            root_cache_path: cache_dir.to_str().unwrap_or_default().into(),
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
        #[cfg(unix)]
        unix::install_shutdown_signal_handlers();

        Self {
            _app: Box::new(app),
            cache_dir,
            #[cfg(target_os = "macos")]
            _loader: Box::new(_loader),
        }
    }
}

/// The CEF profile lives at `$TMPDIR/cef-cache-<exe>-<pid>`, one dir per running client.
/// A shared path puts every client on the same Chromium process singleton (and, on macos, the
/// same mach service name, hashed from the path), so a second client - or a stuck one still
/// holding the lock - fails to start: `bootstrap_check_in: Permission denied` and
/// `Failed to open UKM database: database is locked`. Nothing in the profile has to outlive the
/// run, so the dir is removed again in `cef_shutdown`.
#[derive(Resource)]
struct CefCacheDir(std::path::PathBuf);

fn cef_cache_dir() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("{}{}", cef_cache_prefix(), std::process::id()))
}

fn cef_cache_prefix() -> String {
    format!(
        "cef-cache-{}-",
        std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "app".to_string())
    )
}

/// A client that dies without running `cef_shutdown` (`kill -9`, a crash) strands its profile,
/// ~70MB a time. Drop the ones whose owning process is gone before adding another.
#[cfg(unix)]
fn reap_abandoned_cache_dirs() {
    let prefix = cef_cache_prefix();
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.strip_prefix(&prefix))
            .and_then(|pid| pid.parse::<libc::pid_t>().ok())
            .filter(|pid| *pid > 0)
        else {
            continue;
        };
        // signal 0 only probes: ESRCH means the owner is gone, so nothing is still writing there
        // (EPERM - a live process we don't own - leaves it alone).
        let gone = unsafe { libc::kill(pid, 0) } == -1
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
        if gone {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

// No cheap liveness probe without libc; stray profiles wait for %TEMP% to be cleared.
#[cfg(not(unix))]
fn reap_abandoned_cache_dirs() {}

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

#[cfg(unix)]
fn exit_on_shutdown_signal(mut sent: Local<bool>, mut exit: EventWriter<AppExit>) {
    if !*sent && unix::shutdown_signal_received() {
        *sent = true;
        exit.write_default();
    }
}

fn cef_shutdown(_: NonSend<RunOnMainThread>, cache_dir: Res<CefCacheDir>) {
    shutdown();
    let _ = std::fs::remove_dir_all(&cache_dir.0);
}

#[cfg(unix)]
mod unix {
    use core::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;

    static SHUTDOWN_SIGNAL: AtomicBool = AtomicBool::new(false);

    pub fn shutdown_signal_received() -> bool {
        SHUTDOWN_SIGNAL.load(Ordering::Relaxed)
    }

    extern "C" fn flag_shutdown_signal(signal: libc::c_int) {
        if SHUTDOWN_SIGNAL.swap(true, Ordering::Relaxed) {
            // a repeated signal means the graceful exit isn't getting there; bail out hard
            unsafe { libc::_exit(128 + signal) };
        }
    }

    /// Chromium installs SIGINT/SIGTERM/SIGHUP handlers during cef initialize, and they tear the
    /// process down through Chromium's own quit path rather than ours, so ctrl+c never reaches
    /// bevy: cef shutdown doesn't run, and on macos it wedges outright. There the handler calls
    /// -[NSApplication terminate:] from the message pump, re-entering winit's
    /// applicationWillTerminate delegate from inside cef_do_message_loop_work (itself inside a
    /// winit event-loop callback); it panics across a cannot-unwind boundary and the abort()
    /// hangs in __pthread_kill, leaving an unkillable zombie. Replace the handlers: flag the
    /// signal and let the app wind down through AppExit, which already runs cef shutdown.
    pub fn install_shutdown_signal_handlers() {
        unsafe {
            for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
                libc::signal(sig, flag_shutdown_signal as *const () as libc::sighandler_t);
            }
        }
    }
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
