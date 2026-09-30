use crate::core::prelude::RenderProcessAppBuilder;
use cef::args::Args;
use cef::{api_hash, execute_process, sys};

pub mod app;
pub mod emit;
pub mod listen;
pub mod render_process_handler;

/// Execute the CEF render process.
pub fn execute_render_process() {
    let args = Args::new();
    #[cfg(target_os = "macos")]
    let _loader = {
        // bundle-relative (Contents/Frameworks) with a dev fallback at ~/.local/share/cef —
        // same resolution as the browser process, so a bare helper next to the host binary
        // works in dev without an app bundle.
        let loader = crate::core::debug::DebugLibraryLoader::new();
        assert!(loader.load());
        loader
    };
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let mut app = RenderProcessAppBuilder::build();
    execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
}
