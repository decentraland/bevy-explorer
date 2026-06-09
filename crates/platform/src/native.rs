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
        Tonemapping::TonyMcMapface,
        DebandDither::Enabled,
        ColorGrading {
            global: ColorGradingGlobal {
                exposure: -0.5,
                ..Default::default()
            },
            shadows: ColorGradingSection {
                gamma: 0.75,
                ..Default::default()
            },
            midtones: ColorGradingSection {
                gamma: 0.75,
                ..Default::default()
            },
            highlights: ColorGradingSection {
                gamma: 0.75,
                ..Default::default()
            },
        },
        Bloom {
            intensity: 0.15,
            ..Bloom::OLD_SCHOOL
        },
        ShadowFilteringMethod::Gaussian,
        DepthPrepass,
        NormalPrepass,
    )
}

// Persist a scene's composite. For a local scene the hash is `b64-<base64(project path)>`, so we
// write straight to `<project>/assets/scene/main.composite` and return the path. A remote/deployed
// scene (content hash) has no local target, so saving is refused — clone it locally to edit.
pub async fn save_scene_composite(scene_hash: String, bytes: Vec<u8>) -> Result<String, String> {
    // Save is only offered for a local scene (it writes straight to the scene folder). For a
    // remote/deployed scene there's nowhere to write back to — clone it locally first.
    let Some(path) = local_composite_path(&scene_hash) else {
        return Err(
            "save is only supported for a local scene — clone it locally before editing"
                .to_string(),
        );
    };
    std::fs::write(&path, &bytes)
        .map(|_| path.display().to_string())
        .map_err(|e| format!("write failed ({}): {e}", path.display()))
}

// The local project root for a `b64-<base64(path)>` scene hash, else None (a remote scene).
pub fn local_scene_root(scene_hash: &str) -> Option<std::path::PathBuf> {
    use base64::{prelude::BASE64_URL_SAFE_NO_PAD, Engine};
    let encoded = scene_hash.strip_prefix("b64-")?;
    let decoded = BASE64_URL_SAFE_NO_PAD.decode(encoded).ok()?;
    let project = String::from_utf8(decoded).ok()?;
    Some(std::path::PathBuf::from(project))
}

// The local target path for a `b64-<base64(path)>` scene hash, else None.
fn local_composite_path(scene_hash: &str) -> Option<std::path::PathBuf> {
    Some(
        local_scene_root(scene_hash)?
            .join("assets")
            .join("scene")
            .join("main.composite"),
    )
}

// Write bytes into a local scene project at `rel_path` (relative to the project root), creating
// parent dirs. Used to persist imported assets into the edited scene so it renders on a normal
// (non-live) load. Err for a remote (non-local) scene.
pub async fn write_scene_file(
    scene_hash: &str,
    rel_path: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let Some(root) = local_scene_root(scene_hash) else {
        return Err("not a local scene".to_string());
    };
    let dest = root.join(rel_path);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    std::fs::write(&dest, bytes).map_err(|e| format!("write {}: {e}", dest.display()))
}
