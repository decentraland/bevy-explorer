//! The headless scene runner: a render-free app that runs scenes, as a scene server
//! (`common::structs::server_mode`) or a synthetic client.
//!
//! - [`app`]: the app itself, shared by both entries below
//! - `src/bin/headless.rs`: the native entry, a process (args, the orchestrator protocol)
//! - [`web`]: the web entry, a second instance of the engine module in the page

mod app;
mod logs;
#[cfg(target_arch = "wasm32")]
mod web;

pub use app::{assemble, config};
pub use logs::{drain_scene_logs, scene_log_level, SceneLogReceivers};
