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
pub struct SocialRuntime(
    #[cfg(not(target_arch = "wasm32"))] Arc<Runtime>,
    #[cfg(target_arch = "wasm32")] Arc<LocalRuntime>,
);

impl SocialRuntime {
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
            expect(clippy::arc_with_non_send_sync, reason = "single-threaded on WASM")
        )]
        #[cfg(target_arch = "wasm32")]
        let runtime = Arc::new(
            Builder::new_current_thread()
                .worker_threads(1)
                .enable_all()
                .build_local(LocalOptions::default())
                .unwrap(),
        );

        Self(runtime)
    }
}

impl Default for SocialRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl SocialRuntime {
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

/// SAFETY: WASM is single-threaded, so Send/Sync are safe.
#[cfg(target_arch = "wasm32")]
unsafe impl Send for SocialRuntime {}

/// SAFETY: WASM is single-threaded, so Send/Sync are safe.
#[cfg(target_arch = "wasm32")]
unsafe impl Sync for SocialRuntime {}

pub struct SocialRuntimePlugin;

impl Plugin for SocialRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SocialRuntime>();
        #[cfg(target_arch = "wasm32")]
        app.add_systems(First, yield_to_runtime);
    }
}

/// Tokio uses cooperative scheduling, so unless we explicitly yield time for
/// it, it will never poll the tasks on wasm.
#[cfg(target_arch = "wasm32")]
fn yield_to_runtime(social_runtime: Res<SocialRuntime>) {
    social_runtime.block_on(yield_now());
}
