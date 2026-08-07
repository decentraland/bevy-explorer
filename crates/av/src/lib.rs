#[cfg(test)]
pub mod test;

// util
#[cfg(feature = "ffmpeg")]
pub mod audio_context;
#[cfg(feature = "ffmpeg")]
pub mod audio_sink;
#[cfg(feature = "ffmpeg")]
pub mod ffmpeg_util;
#[cfg(feature = "ffmpeg")]
pub mod stream_processor;
#[cfg(feature = "ffmpeg")]
pub mod video_context;
#[cfg(feature = "ffmpeg")]
pub mod video_stream;

// audio source (non-streaming audio)
pub mod audio_loader;
pub mod audio_source;
#[cfg(not(feature = "html"))]
pub mod audio_source_native;
#[cfg(feature = "html")]
pub mod audio_source_wasm;

// video
#[cfg(feature = "html")]
pub mod html_video_player;
#[cfg(feature = "ffmpeg")]
pub mod video_player;

#[cfg(feature = "av_player_debug")]
pub mod av_player_debug;

use std::{borrow::Borrow, marker::PhantomData, ops::Deref};

#[cfg(feature = "ffmpeg")]
use crate::{audio_sink::AudioSink, video_stream::VideoSink};
use audio_source::AudioSourcePlugin;
#[cfg(not(feature = "html"))]
use audio_source_native::AudioSourcePluginImpl;
#[cfg(feature = "ffmpeg")]
use bevy::ecs::component::Mutable;
use bevy::{math::FloatOrd, prelude::*};
use common::{
    sets::SceneSets,
    structs::{AppConfig, PrimaryUser},
};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{PbAudioStream, PbVideoPlayer},
    SceneComponentId,
};
use livestream_manager::{ActiveReceiver, ReceiverImage};
use scene_runner::{
    update_world::{material::VideoTextureOutput, AddCrdtInterfaceExt},
    ContainerEntity, ContainingScene,
};

#[cfg(feature = "ffmpeg")]
use {
    audio_sink::{spawn_and_locate_foreign_streams, spawn_audio_streams},
    video_player::VideoPlayerPlugin,
};
#[cfg(feature = "html")]
use {
    // foreign players
    audio_source_wasm::AudioSourcePluginImpl,
    html_video_player::VideoPlayerPlugin,
};

const LIVEKIT_VIDEO_STREAM: &str = "livekit-video://current-stream";

pub trait AVPlayer: Component {
    type Source: Component + Deref<Target = str> + PartialEq;
    type Config: Component + AVPlayerConfig + PartialEq;
    type Position: Component + Deref<Target = f32> + PartialEq;

    fn url(&self) -> &str;
    fn source(&self) -> Self::Source;
    fn config(&self) -> Self::Config;
    fn position(&self) -> Self::Position;

    #[cfg(feature = "ffmpeg")]
    fn build_sink_component(audio_sink: AudioSink, video_sink: VideoSink) -> AVSinks<Self>
    where
        Self: Sized;

    #[cfg(feature = "html")]
    fn has_video() -> bool;
}

pub trait AVPlayerConfig {
    fn playing(&self) -> bool;
    fn volume(&self) -> f32;
    fn playback_rate(&self) -> f32;
    fn r#loop(&self) -> bool;
}

#[cfg(feature = "ffmpeg")]
pub trait AVPlayerSinks: Component<Mutability = Mutable> {
    fn audio_sink(&self) -> Option<&AudioSink>;
    fn audio_sink_mut(&mut self) -> Option<&mut AudioSink>;
    fn video_sink(&self) -> Option<&VideoSink>;
    fn video_sink_mut(&mut self) -> Option<&mut VideoSink>;
}

#[cfg(feature = "ffmpeg")]
#[derive(Component)]
pub struct AVSinks<T: AVPlayer> {
    pub audio: Option<AudioSink>,
    pub video: Option<VideoSink>,
    pub _phantom: PhantomData<T>,
}

#[cfg(feature = "ffmpeg")]
impl<T: AVPlayer> AVPlayerSinks for AVSinks<T> {
    fn audio_sink(&self) -> Option<&AudioSink> {
        self.audio.as_ref()
    }

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

    #[cfg(feature = "ffmpeg")]
    fn build_sink_component(audio_sink: AudioSink, _video_sink: VideoSink) -> AVSinks<Self> {
        AVSinks {
            audio: Some(audio_sink),
            video: None,
            _phantom: Default::default(),
        }
    }

    #[cfg(feature = "html")]
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

    #[cfg(feature = "ffmpeg")]
    fn build_sink_component(audio_sink: AudioSink, video_sink: VideoSink) -> AVSinks<Self> {
        AVSinks {
            audio: Some(audio_sink),
            video: Some(video_sink),
            _phantom: Default::default(),
        }
    }

    #[cfg(feature = "html")]
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

#[derive(Default)]
pub struct AVPlayerPlugin;

impl Plugin for AVPlayerPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(any(feature = "ffmpeg", feature = "html"))]
        app.add_plugins(VideoPlayerPlugin);
        app.add_plugins(AudioSourcePlugin);
        app.add_plugins(AudioSourcePluginImpl);

        app.add_crdt_lww_component::<PbAudioStream, AudioStream>(
            SceneComponentId::AUDIO_STREAM,
            ComponentPosition::EntityOnly,
        );
        app.add_crdt_lww_component::<PbVideoPlayer, VideoPlayer>(
            SceneComponentId::VIDEO_PLAYER,
            ComponentPosition::EntityOnly,
        );

        #[cfg(feature = "ffmpeg")]
        app.add_systems(
            PostUpdate,
            (
                (
                    spawn_audio_streams::<AudioStream>,
                    spawn_audio_streams::<VideoPlayer>,
                ),
                (
                    spawn_and_locate_foreign_streams::<AudioStream>,
                    spawn_and_locate_foreign_streams::<VideoPlayer>,
                ),
            )
                .chain(),
        );
        app.add_systems(
            Update,
            (
                (
                    av_player_is_in_scene::<AudioStream>,
                    av_player_is_in_scene::<VideoPlayer>,
                ),
                (
                    audio_stream_should_be_playing,
                    video_player_should_be_playing,
                ),
            )
                .chain()
                .in_set(SceneSets::PostLoop),
        );

        app.add_observer(av_player_on_insert::<AudioStream>);
        app.add_observer(av_player_on_insert::<VideoPlayer>);
        app.add_observer(av_player_on_remove::<AudioStream>);
        app.add_observer(av_player_on_remove::<VideoPlayer>);
        app.add_observer(stream_on_add::<VideoPlayer>);
        app.add_observer(stream_on_remove);
        app.add_observer(should_be_playing_on_add::<VideoPlayer>);
        app.add_observer(should_be_playing_on_remove::<VideoPlayer>);

        #[cfg(feature = "ffmpeg")]
        app.add_observer(audio_sink::change_audio_sink_volume::<AudioStream>);
        #[cfg(feature = "ffmpeg")]
        app.add_observer(audio_sink::change_audio_sink_volume::<VideoPlayer>);

        app.add_observer(receiver_image_added);
        app.add_observer(receiver_image_removed);

        app.add_systems(FixedUpdate, video_texture_output_image_changed);

        #[cfg(feature = "av_player_debug")]
        app.add_plugins(av_player_debug::AvPlayerDebugPlugin);
    }
}

#[expect(clippy::type_complexity)]
fn av_player_on_insert<T: AVPlayer>(
    trigger: Trigger<OnInsert, T>,
    mut commands: Commands,
    mut av_players: Query<(
        &T,
        Option<&T::Source>,
        Option<&T::Config>,
        Option<&T::Position>,
        Has<Stream>,
    )>,
) {
    debug!("AVPlayer updated.");
    let entity = trigger.target();
    let Ok((av_player, maybe_source, mut maybe_config, mut maybe_position, has_stream)) =
        av_players.get_mut(entity)
    else {
        return;
    };

    let source_url = av_player.url();
    let livestream = source_url == LIVEKIT_VIDEO_STREAM;
    let mut entity_cmd = commands.entity(entity);

    if maybe_source.is_none_or(|src| &(**src) != source_url) {
        debug!("AVPlayer sources diverge");
        let new_source = av_player.source();

        if livestream != has_stream {
            if livestream {
                entity_cmd.insert(Stream);
            } else {
                entity_cmd.remove::<Stream>();
            }
        }

        // Order on insertion matters
        entity_cmd.insert(new_source);

        let _ = maybe_config.take();
        let _ = maybe_position.take();
    }

    let new_config = av_player.config();
    if maybe_config.is_none_or(|config| (*config) != new_config) {
        debug!("AVPlayer config update");
        entity_cmd.insert(new_config);
    }

    let new_position = av_player.position();
    if maybe_position.is_none_or(|position| ((**position) - *new_position).abs() >= 0.5) {
        debug!("AVPlayer position update");
        entity_cmd.insert(new_position);
    }
}

fn av_player_on_remove<T: AVPlayer>(trigger: Trigger<OnRemove, T>, mut commands: Commands) {
    let entity = trigger.target();
    commands.entity(entity).try_remove::<(
        T::Source,
        T::Config,
        T::Position,
        InScene,
        ShouldBePlaying<T>,
        Stream,
    )>();
}

#[expect(clippy::type_complexity)]
fn stream_on_add<T: AVPlayer>(
    trigger: Trigger<OnAdd, Stream>,
    mut commands: Commands,
    av_players: Query<Has<ShouldBePlaying<T>>, (With<T>, With<Stream>)>,
) {
    let entity = trigger.target();
    let Ok(has_should_be_playing) = av_players.get(entity) else {
        unreachable!("Infallible query");
    };

    if has_should_be_playing {
        commands.entity(entity).insert(ActiveReceiver);
    }
}

fn stream_on_remove(trigger: Trigger<OnRemove, Stream>, mut commands: Commands) {
    let entity = trigger.target();
    commands.entity(entity).try_remove::<ActiveReceiver>();
}

#[expect(clippy::type_complexity)]
fn should_be_playing_on_add<T: AVPlayer>(
    trigger: Trigger<OnAdd, ShouldBePlaying<T>>,
    mut commands: Commands,
    av_players: Query<Has<Stream>, (With<T>, With<ShouldBePlaying<T>>)>,
) {
    let entity = trigger.target();
    let Ok(has_stream) = av_players.get(entity) else {
        unreachable!("Infallible query");
    };

    if has_stream {
        commands.entity(entity).insert(ActiveReceiver);
    }
}

fn should_be_playing_on_remove<T: AVPlayer>(
    trigger: Trigger<OnRemove, ShouldBePlaying<T>>,
    mut commands: Commands,
) {
    let entity = trigger.target();
    commands.entity(entity).try_remove::<ActiveReceiver>();
}

fn av_player_is_in_scene<T: AVPlayer>(
    mut commands: Commands,
    av_players: Query<(Entity, &ContainerEntity, &T::Config, Has<InScene>)>,
    user: Query<&GlobalTransform, With<PrimaryUser>>,
    containing_scene: ContainingScene,
) {
    // disable distant av
    let Ok(user) = user.single() else {
        return;
    };
    let containing_scenes = containing_scene.get_position(user.translation());

    for (ent, container, _, has_in_scene) in av_players
        .iter()
        .filter(|(_, _, av_player, _)| av_player.playing())
    {
        let contained = containing_scenes.contains(&container.root);
        if contained && !has_in_scene {
            // Only call `insert` on those that do not have `InScene`
            commands.entity(ent).try_insert(InScene);
        } else if !contained && has_in_scene {
            // Only call `remove` on those that have `InScene`
            commands.entity(ent).remove::<InScene>();
        }
    }
}

#[expect(clippy::type_complexity)]
fn audio_stream_should_be_playing(
    mut commands: Commands,
    av_players: Query<(
        Entity,
        &AudioStream,
        Has<InScene>,
        Has<ShouldBePlaying<AudioStream>>,
    )>,
) {
    for (entity, audio_stream, in_scene, should_be_playing) in av_players {
        match (in_scene, should_be_playing, audio_stream.playing()) {
            (false, true, _) | (_, true, false) => {
                commands
                    .entity(entity)
                    .try_remove::<ShouldBePlaying<AudioStream>>();
            }
            (true, false, true) => {
                commands
                    .entity(entity)
                    .try_insert(ShouldBePlaying::<AudioStream>::default());
            }
            _ => (),
        }
    }
}

#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn video_player_should_be_playing(
    mut commands: Commands,
    av_players: Query<(
        Entity,
        &<VideoPlayer as AVPlayer>::Source,
        &<VideoPlayer as AVPlayer>::Config,
        Has<InScene>,
        Has<ShouldBePlaying<VideoPlayer>>,
        &GlobalTransform,
    )>,
    user: Single<&GlobalTransform, With<PrimaryUser>>,
    config: Res<AppConfig>,
) {
    let mut sorted_players = av_players
        .iter()
        .filter_map(
            |(
                ent,
                player_source,
                player_config,
                has_in_scene,
                has_should_be_playing,
                transform,
            )| {
                if player_config.playing() {
                    let distance = if !has_in_scene && &(**player_source) == LIVEKIT_VIDEO_STREAM {
                        f32::MAX
                    } else {
                        transform.translation().distance(user.translation())
                    };
                    Some((has_in_scene, has_should_be_playing, distance, ent))
                } else {
                    None
                }
            },
        )
        .collect::<Vec<_>>();

    // prioritise av in current scene (false < true), then by distance
    sorted_players.sort_by_key(|(in_scene, _, distance, _)| (!in_scene, FloatOrd(*distance)));

    // Removing first for better Trigger ordering
    for ent in sorted_players
        .iter()
        .skip(config.max_videos)
        // Only call remove on those that have `ShouldBePlaying`
        // The `filter` MUST be after the `skip`
        .filter(|(_, has_should_be_playing, _, _)| *has_should_be_playing)
        .map(|(_, _, _, ent)| *ent)
    {
        commands
            .entity(ent)
            .try_remove::<ShouldBePlaying<VideoPlayer>>();
    }

    for (ent, distance, has_should_be_playing) in sorted_players
        .iter()
        .take(config.max_videos)
        .map(|(_, has_should_be_playing, distance, ent)| (*ent, *distance, *has_should_be_playing))
    {
        if distance == f32::MAX {
            if has_should_be_playing {
                commands
                    .entity(ent)
                    .try_remove::<ShouldBePlaying<VideoPlayer>>();
            }
        } else {
            if !has_should_be_playing {
                commands
                    .entity(ent)
                    .try_insert(ShouldBePlaying::<VideoPlayer>::default());
            }
        }
    }
}

fn receiver_image_added(
    trigger: Trigger<OnAdd, ReceiverImage>,
    mut commands: Commands,
    video_players: Query<&ReceiverImage, (With<VideoPlayer>, With<Stream>)>,
) {
    let entity = trigger.target();

    if let Ok(receiver_image) = video_players.get(entity) {
        commands
            .entity(entity)
            .insert(VideoTextureOutput((*receiver_image).clone()));
    }
}

fn receiver_image_removed(trigger: Trigger<OnRemove, ReceiverImage>, mut commands: Commands) {
    let entity = trigger.target();
    commands.entity(entity).try_remove::<VideoTextureOutput>();
}

fn video_texture_output_image_changed(video_texture_outputs: Populated<&mut VideoTextureOutput>) {
    // TODO make it so that this workaround isn't needed
    for mut video_texture_output in video_texture_outputs.into_inner() {
        video_texture_output.set_changed();
    }
}
