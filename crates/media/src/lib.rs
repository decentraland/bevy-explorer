#[cfg(all(not(target_arch = "wasm32"), feature = "ffmpeg"))]
mod ffmpeg;
#[cfg(all(target_arch = "wasm32", feature = "html"))]
mod html;
pub mod plugin;

#[cfg(all(not(target_arch = "wasm32"), feature = "ffmpeg"))]
pub use ffmpeg::*;
#[cfg(all(target_arch = "wasm32", feature = "html"))]
pub use html::*;
