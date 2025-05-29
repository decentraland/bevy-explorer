#[cfg(all(target_arch = "wasm32", not(feature = "wasm")))]
compile_error!("wasm feature must be enabled in wasm32 builds");

#[cfg(all(not(target_arch = "wasm32"), not(feature = "native")))]
compile_error!("native feature must be enabled in native builds");

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
pub use wasm::*;
