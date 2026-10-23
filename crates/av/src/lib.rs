pub mod plugin;
#[cfg(test)]
pub mod test;

// util
#[cfg(feature = "ffmpeg")]
pub mod audio_sink;
#[cfg(any(feature = "ffmpeg", feature = "html"))]
pub mod video_stream;

// audio source (non-streaming audio)
#[cfg(feature = "html")]
pub mod audio_host_wasm;
pub mod audio_loader;
pub mod audio_source;
#[cfg(not(feature = "html"))]
pub mod audio_source_native;
#[cfg(feature = "html")]
pub mod audio_source_wasm;

// video
#[cfg(any(feature = "ffmpeg", feature = "html"))]
pub mod video_player;

#[cfg(feature = "av_player_debug")]
pub mod av_player_debug;

use std::{borrow::Borrow, marker::PhantomData, ops::Deref};

#[cfg(any(feature = "ffmpeg", feature = "html"))]
use bevy::ecs::component::Mutable;
use bevy::prelude::*;
#[cfg(any(feature = "ffmpeg", feature = "html"))]
use dcl_component::proto_components::sdk::components::VideoState;
use dcl_component::proto_components::sdk::components::{PbAudioStream, PbVideoPlayer};
#[cfg(any(feature = "ffmpeg", feature = "html"))]
use media::AVCommand;
#[cfg(any(feature = "ffmpeg", feature = "html"))]
use tokio::sync::watch;

#[cfg(feature = "ffmpeg")]
use crate::audio_sink::AudioSink;
#[cfg(any(feature = "ffmpeg", feature = "html"))]
use crate::video_stream::VideoSink;

const LIVEKIT_VIDEO_STREAM: &str = "livekit-video://current-stream";

pub trait AVPlayer: Component {
    type Source: Component + Deref<Target = str> + PartialEq;
    type Config: Component + AVPlayerConfig + PartialEq;
    type Position: Component + Deref<Target = f32> + PartialEq;

    const ALLOWS_LIVESTREAM: bool;

    fn url(&self) -> &str;
    fn source(&self) -> Self::Source;
    fn config(&self) -> Self::Config;
    fn position(&self) -> Self::Position;

    fn has_video() -> bool;
}

pub trait AVPlayerConfig {
    fn playing(&self) -> bool;
    fn volume(&self) -> f32;
    fn playback_rate(&self) -> f32;
    fn r#loop(&self) -> bool;
}

#[cfg(any(feature = "ffmpeg", feature = "html"))]
pub trait AVPlayerSinks: Component<Mutability = Mutable> {
    #[cfg(feature = "ffmpeg")]
    fn audio_sink(&self) -> Option<&AudioSink>;
    #[cfg(feature = "ffmpeg")]
    fn audio_sink_mut(&mut self) -> Option<&mut AudioSink>;
    fn video_sink(&self) -> Option<&VideoSink>;
    fn video_sink_mut(&mut self) -> Option<&mut VideoSink>;
}

/// The av backend's ends of a player; the sinks share one command channel. Audio is mixed in
/// kira on native and by the page element on the web, which has no audio sink.
#[cfg(any(feature = "ffmpeg", feature = "html"))]
#[derive(Component)]
pub struct AVSinks<T: AVPlayer> {
    #[cfg(feature = "ffmpeg")]
    pub audio: Option<AudioSink>,
    pub video: Option<VideoSink>,
    pub command_sender: tokio::sync::mpsc::UnboundedSender<AVCommand>,
    pub rate: Option<f64>,
    pub state: watch::Receiver<VideoState>,
    /// The last volume sent, to send only changes.
    pub sent_volume: Option<f32>,
    pub sent_state: VideoState,
    pub _phantom: PhantomData<T>,
}

#[cfg(any(feature = "ffmpeg", feature = "html"))]
impl<T: AVPlayer> AVPlayerSinks for AVSinks<T> {
    #[cfg(feature = "ffmpeg")]
    fn audio_sink(&self) -> Option<&AudioSink> {
        self.audio.as_ref()
    }

    #[cfg(feature = "ffmpeg")]
    fn audio_sink_mut(&mut self) -> Option<&mut AudioSink> {
        self.audio.as_mut()
    }

    fn video_sink(&self) -> Option<&VideoSink> {
        self.video.as_ref()
    }

    fn video_sink_mut(&mut self) -> Option<&mut VideoSink> {
        self.video.as_mut()
    }
}

#[derive(Component, Deref)]
#[component(immutable)]
pub struct AudioStream(PbAudioStream);

impl From<PbAudioStream> for AudioStream {
    fn from(value: PbAudioStream) -> Self {
        Self(value)
    }
}

impl AVPlayer for AudioStream {
    type Source = AudioStreamSource;
    type Config = AudioStreamConfig;
    type Position = AudioStreamPosition;

    const ALLOWS_LIVESTREAM: bool = false;

    fn url(&self) -> &str {
        &self.url
    }

    fn source(&self) -> Self::Source {
        AudioStreamSource(self.url.to_owned())
    }

    fn config(&self) -> Self::Config {
        AudioStreamConfig {
            playing: self.playing.unwrap_or(true),
            volume: self.volume.unwrap_or(1.),
        }
    }

    fn position(&self) -> Self::Position {
        AudioStreamPosition(0.)
    }

    fn has_video() -> bool {
        false
    }
}

#[derive(Component, Deref)]
#[component(immutable)]
pub struct VideoPlayer(PbVideoPlayer);

impl From<PbVideoPlayer> for VideoPlayer {
    fn from(value: PbVideoPlayer) -> Self {
        Self(value)
    }
}

impl AVPlayer for VideoPlayer {
    type Source = VideoPlayerSource;
    type Config = VideoPlayerConfig;
    type Position = VideoPlayerPosition;

    const ALLOWS_LIVESTREAM: bool = true;

    fn url(&self) -> &str {
        &self.src
    }

    fn source(&self) -> Self::Source {
        VideoPlayerSource(self.src.to_owned())
    }

    fn config(&self) -> Self::Config {
        VideoPlayerConfig {
            playing: self.playing.unwrap_or(true),
            volume: self.volume.unwrap_or(1.),
            playback_rate: self.playback_rate.unwrap_or(1.),
            r#loop: self.r#loop.unwrap_or(false),
        }
    }

    fn position(&self) -> Self::Position {
        VideoPlayerPosition(self.position.unwrap_or(0.))
    }

    fn has_video() -> bool {
        true
    }
}

#[derive(Clone, PartialEq, Component)]
#[component(immutable)]
pub struct AudioStreamSource(String);

impl<T: Borrow<AudioStream>> From<T> for AudioStreamSource {
    fn from(value: T) -> Self {
        let borrow = value.borrow();
        Self(borrow.url.to_owned())
    }
}

impl Deref for AudioStreamSource {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_str()
    }
}

#[derive(Clone, Copy, PartialEq, Component)]
#[component(immutable)]
pub struct AudioStreamConfig {
    playing: bool,
    volume: f32,
}

impl<T: Borrow<AudioStream>> From<T> for AudioStreamConfig {
    fn from(value: T) -> Self {
        let borrow = value.borrow();
        Self {
            playing: borrow.playing.unwrap_or(true),
            volume: borrow.volume.unwrap_or(1.),
        }
    }
}

impl AVPlayerConfig for AudioStreamConfig {
    fn playing(&self) -> bool {
        self.playing
    }

    fn volume(&self) -> f32 {
        self.volume
    }

    fn playback_rate(&self) -> f32 {
        1.
    }

    fn r#loop(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, PartialEq, Component)]
#[component(immutable)]
pub struct AudioStreamPosition(f32);

impl<T: Borrow<AudioStream>> From<T> for AudioStreamPosition {
    fn from(_value: T) -> Self {
        Self(0.)
    }
}

impl Deref for AudioStreamPosition {
    type Target = f32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, PartialEq, Component)]
#[component(immutable)]
pub struct VideoPlayerSource(String);

impl Deref for VideoPlayerSource {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_str()
    }
}

impl<T: Borrow<VideoPlayer>> From<T> for VideoPlayerSource {
    fn from(value: T) -> Self {
        let borrow = value.borrow();
        Self(borrow.src.to_owned())
    }
}

#[derive(Clone, Copy, PartialEq, Component)]
#[component(immutable)]
pub struct VideoPlayerConfig {
    pub playing: bool,
    pub volume: f32,
    pub playback_rate: f32,
    pub r#loop: bool,
}

impl<T: Borrow<VideoPlayer>> From<T> for VideoPlayerConfig {
    fn from(value: T) -> Self {
        let borrow = value.borrow();
        Self {
            playing: borrow.playing.unwrap_or(true),
            volume: borrow.volume.unwrap_or(1.),
            playback_rate: borrow.playback_rate.unwrap_or(1.),
            r#loop: borrow.r#loop.unwrap_or(false),
        }
    }
}

impl AVPlayerConfig for VideoPlayerConfig {
    fn playing(&self) -> bool {
        self.playing
    }

    fn volume(&self) -> f32 {
        self.volume
    }

    fn playback_rate(&self) -> f32 {
        self.playback_rate
    }

    fn r#loop(&self) -> bool {
        self.r#loop
    }
}

#[derive(Clone, Copy, PartialEq, Component, Deref)]
#[component(immutable)]
pub struct VideoPlayerPosition(f32);

impl<T: Borrow<VideoPlayer>> From<T> for VideoPlayerPosition {
    fn from(value: T) -> Self {
        let borrow = value.borrow();
        Self(borrow.position.unwrap_or(0.))
    }
}

/// Marks whether an [`AVPlayer`] should be playing
#[derive(Debug, Component)]
pub struct ShouldBePlaying<T>(PhantomData<T>);

impl<T> Default for ShouldBePlaying<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

/// Marks whether an [`AVPlayer`] is in the same scene as the [`PrimaryUser`]
#[derive(Debug, Component)]
pub struct InScene;

#[derive(Debug, Component)]
pub struct Stream;
