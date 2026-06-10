use async_tungstenite::{async_std::ConnectStream, WebSocketStream};
use bevy::{
    core_pipeline::{
        bloom::Bloom,
        prepass::{DepthPrepass, NormalPrepass},
        tonemapping::{DebandDither, Tonemapping},
    },
    ecs::bundle::Bundle,
    pbr::ShadowFilteringMethod,
    render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection},
};
use futures_util::{
    sink::SinkExt,
    stream::{SplitSink, SplitStream},
    StreamExt,
};
use serde::Serialize;
pub use tungstenite::client::IntoClientRequest;

pub struct WebSocket {
    inner: WebSocketStream<ConnectStream>,
}

impl WebSocket {
    pub async fn send(&mut self, item: tungstenite::Message) -> Result<(), anyhow::Error> {
        self.inner.send(item).await.map_err(|e| anyhow::anyhow!(e))
    }

    pub async fn next(&mut self) -> Option<Result<tungstenite::Message, anyhow::Error>> {
        self.inner
            .next()
            .await
            .map(|result| result.map_err(|e| anyhow::anyhow!(e)))
    }

    pub fn split(self) -> (WebSocketSplitSink, WebSocketSplitStream) {
        let (w, r) = self.inner.split();
        (
            WebSocketSplitSink { inner: w },
            WebSocketSplitStream { inner: r },
        )
    }
}

pub struct WebSocketSplitSink {
    inner: SplitSink<WebSocketStream<ConnectStream>, tungstenite::Message>,
}

impl WebSocketSplitSink {
    pub async fn send(&mut self, item: tungstenite::Message) -> Result<(), anyhow::Error> {
        self.inner.send(item).await.map_err(|e| anyhow::anyhow!(e))
    }
}

pub struct WebSocketSplitStream {
    inner: SplitStream<WebSocketStream<ConnectStream>>,
}

impl WebSocketSplitStream {
    pub async fn next(&mut self) -> Option<Result<tungstenite::Message, anyhow::Error>> {
        self.inner
            .next()
            .await
            .map(|result| result.map_err(|e| anyhow::anyhow!(e)))
    }
}

pub async fn websocket<R>(request: R) -> Result<WebSocket, anyhow::Error>
where
    R: IntoClientRequest + Unpin,
{
    let (stream, _response) = async_tungstenite::async_std::connect_async(request).await?;
    Ok(WebSocket { inner: stream })
}

pub trait ReqwestBuilderExt {}

pub fn compat<F>(f: F) -> async_compat::Compat<F> {
    async_compat::Compat::new(f)
}

pub fn project_directories() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("org", "decentraland", "BevyExplorer")
}

pub fn write_config_file<T: Serialize>(config: &T) {
    let config_file = project_directories()
        .unwrap()
        .config_dir()
        .join("config.json");

    if let Some(folder) = config_file.parent() {
        std::fs::create_dir_all(folder).unwrap();
    }
    let _ = std::fs::write(config_file, serde_json::to_string(config).unwrap());
}

#[derive(Default)]
pub struct AsyncRwLock<T>(tokio::sync::RwLock<T>);

impl<T> AsyncRwLock<T> {
    pub fn new(value: T) -> Self {
        Self(tokio::sync::RwLock::new(value))
    }

    pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, T> {
        self.0.read().await
    }

    pub async fn write(&self) -> tokio::sync::RwLockWriteGuard<'_, T> {
        self.0.write().await
    }

    pub fn blocking_read(&self) -> tokio::sync::RwLockReadGuard<'_, T> {
        self.0.blocking_read()
    }

    pub fn blocking_write(&self) -> tokio::sync::RwLockWriteGuard<'_, T> {
        self.0.blocking_write()
    }

    pub fn try_read(
        &self,
    ) -> Result<tokio::sync::RwLockReadGuard<'_, T>, tokio::sync::TryLockError> {
        self.0.try_read()
    }

    pub fn try_write(
        &self,
    ) -> Result<tokio::sync::RwLockWriteGuard<'_, T>, tokio::sync::TryLockError> {
        self.0.try_write()
    }
}

pub fn platform_pointer_is_locked(expected: bool) -> bool {
    expected
}

pub fn default_camera_components() -> impl Bundle {
    (
        Tonemapping::AcesFitted,
        DebandDither::Enabled,
        ColorGrading {
            // godot/unity parity: ACES with neutral grading; tweak live with
            // /tonemap /exposure /gamma /saturation
            global: ColorGradingGlobal {
                exposure: 0.0,
                ..Default::default()
            },
            shadows: ColorGradingSection::default(),
            midtones: ColorGradingSection::default(),
            highlights: ColorGradingSection::default(),
        },
        Bloom {
            // godot glow reference ~1.25 strength
            intensity: 0.25,
            ..Bloom::OLD_SCHOOL
        },
        ShadowFilteringMethod::Gaussian,
        DepthPrepass,
        NormalPrepass,
    )
}

// Persist a scene's composite. The destination is the active scene's project root, which the
// caller resolves (via `IpfsIo::local_project_root`) and passes in `scene_target` as `{root, …}`;
// we write straight to `<root>/assets/scene/main.composite` and return the path. A remote/deployed
// scene has no local root (`root` is null), so saving is refused — clone it locally to edit.
pub async fn save_scene_composite(
    _scene_hash: String,
    bytes: Vec<u8>,
    scene_target: String,
) -> Result<String, String> {
    let Some(path) = local_composite_path(&scene_target) else {
        return Err(
            "save is only supported for a local scene — clone it locally before editing"
                .to_string(),
        );
    };
    std::fs::write(&path, &bytes)
        .map(|_| path.display().to_string())
        .map_err(|e| format!("write failed ({}): {e}", path.display()))
}

// The local project root carried in the `scene_target` JSON (`{root, …}`, resolved by the caller
// via `IpfsIo::local_project_root` — which correctly decodes the dev server's standard-base64
// `b64-<path>-<machineId>` hashes). None for a remote/deployed scene, where `root` is null.
pub fn local_scene_root(scene_target: &str) -> Option<std::path::PathBuf> {
    let target: serde_json::Value = serde_json::from_str(scene_target).ok()?;
    let root = target.get("root")?.as_str()?;
    Some(std::path::PathBuf::from(root))
}

// The local target path under the scene's project root, else None.
fn local_composite_path(scene_target: &str) -> Option<std::path::PathBuf> {
    Some(
        local_scene_root(scene_target)?
            .join("assets")
            .join("scene")
            .join("main.composite"),
    )
}

// Write bytes into a local scene project at `rel_path` (relative to the project root), creating
// parent dirs. Used to persist imported assets into the edited scene so it renders on a normal
// (non-live) load. Err for a remote (non-local) scene.
pub async fn write_scene_file(
    _scene_hash: &str,
    rel_path: &str,
    bytes: &[u8],
    scene_target: &str,
) -> Result<(), String> {
    let Some(root) = local_scene_root(scene_target) else {
        return Err("not a local scene".to_string());
    };
    let dest = root.join(rel_path);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    std::fs::write(&dest, bytes).map_err(|e| format!("write {}: {e}", dest.display()))
}
