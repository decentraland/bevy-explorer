// The CEF process machinery (browser + render process handler implementations), derived from
// bevy_cef_core. BRP support removed; several upstream fixes folded in.
pub mod browser_process;
#[cfg(target_os = "macos")]
pub mod debug;
pub mod render_process;
pub mod util;

pub mod prelude {
    pub use crate::core::browser_process::*;
    #[cfg(target_os = "macos")]
    pub use crate::core::debug::*;
    pub use crate::core::render_process::app::*;
    pub use crate::core::render_process::emit::*;
    pub use crate::core::render_process::execute_render_process;
    pub use crate::core::render_process::render_process_handler::*;
    pub use crate::core::util::*;
}
