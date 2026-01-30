//! Wrapper around
#![cfg_attr(
    not(target_arch = "wasm32"),
    doc = "[`Runtime`](tokio::runtime::Runtime)"
)]
#![cfg_attr(
    target_arch = "wasm32",
    doc = "[`LocalRuntime`](tokio::runtime::LocalRuntime)"
)]
//! for use by Livekit rooms.

use std::future::Future;

use bevy::{platform::sync::Arc, prelude::*};
#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::Runtime;
use tokio::{runtime::Builder, task::JoinHandle};
#[cfg(target_arch = "wasm32")]
use tokio::{
    runtime::{LocalOptions, LocalRuntime},
    task::yield_now,
};

#[derive(Clone, Resource)]
pub struct LivekitRuntime(
    #[cfg(not(target_arch = "wasm32"))] Arc<Runtime>,
    #[cfg(target_arch = "wasm32")] Arc<LocalRuntime>,
);

impl LivekitRuntime {
    pub fn new() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let runtime = Arc::new(
            Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap(),
        );
        #[cfg_attr(
            target_arch = "wasm32",
            expect(clippy::arc_with_non_send_sync, reason = "This is a bit hacky")
        )]
        #[cfg(target_arch = "wasm32")]
        let runtime = Arc::new(
            Builder::new_current_thread()
                .worker_threads(1)
                .enable_all()
                .build_local(&LocalOptions::default())
                .unwrap(),
        );

        Self(runtime)
    }
}

impl Default for LivekitRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl LivekitRuntime {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.0.spawn(future)
    }

    #[cfg(target_arch = "wasm32")]
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        self.0.spawn_local(future)
    }

    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        self.0.block_on(future)
    }
}

/// SAFETY: This will be fine while WASM remains single threaded
#[cfg(target_arch = "wasm32")]
unsafe impl Send for LivekitRuntime {}

/// SAFETY: This will be fine while WASM remains single threaded
#[cfg(target_arch = "wasm32")]
unsafe impl Sync for LivekitRuntime {}

pub struct LivekitRuntimePlugin;

impl Plugin for LivekitRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LivekitRuntime>();
        #[cfg(target_arch = "wasm32")]
        app.add_systems(First, yield_to_runtime);
    }
}

/// Tokio uses cooperative scheduling, so unless we explicitly yield time for
/// it, it will never poll the tasks on wasm, native does not have this issue
/// as it is multithreaded
#[cfg(target_arch = "wasm32")]
fn yield_to_runtime(livekit_runtime: Res<LivekitRuntime>) {
    livekit_runtime.block_on(yield_now());
}
