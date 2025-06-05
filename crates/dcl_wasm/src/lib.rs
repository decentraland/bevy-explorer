#[cfg(target_arch = "wasm32")]
pub mod inner;

#[cfg(target_arch = "wasm32")]
pub use inner::*;
