use async_tungstenite::{async_std::ConnectStream, WebSocketStream};
use bevy::{
    asset::{Assets, Handle}, core_pipeline::{
        bloom::Bloom,
        prepass::{DepthPrepass, NormalPrepass},
        tonemapping::{DebandDither, Tonemapping},
    }, ecs::{bundle::Bundle, system::{Res, ResMut, SystemParam}}, pbr::ShadowFilteringMethod, render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection}
};
use bevy_kira_audio::AudioControl;
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

pub type AudioInstanceHandle = Handle<bevy_kira_audio::AudioInstance>;

#[derive(SystemParam)]
pub struct AudioManager<'w> {
    pub instances: ResMut<'w, Assets<bevy_kira_audio::AudioInstance>>,
    pub sounds: Res<'w, Assets<bevy_kira_audio::AudioSource>>,
    pub audio: Res<'w, bevy_kira_audio::Audio>,
}

impl<'w> AudioManager<'w> {
    pub fn play(&self, handle: Handle<bevy_kira_audio::AudioSource>, volume: f32, panning: f32) -> AudioInstanceHandle {
        self.audio.play(handle).with_volume(volume as f64).with_panning(panning as f64).handle()
    }

    pub fn stop(&mut self, instance: &AudioInstanceHandle) {
        if let Some(instance) = self.instances.get_mut(instance) {
            instance.stop(bevy_kira_audio::AudioTween::default())
        }
    }

    pub fn set_volume_and_panning(&mut self, instance: &AudioInstanceHandle, volume: f32, panning: f32) {
        if let Some(instance) = self.instances.get_mut(instance) {
            instance.set_volume(volume as f64, bevy_kira_audio::AudioTween::default());
            instance.set_panning(panning as f64, bevy_kira_audio::AudioTween::default());
        }
    }
}