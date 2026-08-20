#[cfg(all(not(target_arch = "wasm32"), feature = "ffmpeg"))]
mod ffmpeg;

#[cfg(all(not(target_arch = "wasm32"), feature = "ffmpeg"))]
pub use ffmpeg::*;
