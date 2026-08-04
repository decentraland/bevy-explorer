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

use std::marker::PhantomData;

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
use livestream_manager::ActiveReceiver;
use scene_runner::{update_world::AddCrdtInterfaceExt, ContainerEntity, ContainingScene};

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
    fn source(&self) -> &str;
    fn playing(&self) -> bool;
    fn volume(&self) -> f32;
    fn r#loop(&self) -> bool;

    #[cfg(feature = "ffmpeg")]
    fn build_sink_component(audio_sink: AudioSink, video_sink: VideoSink) -> AVSinks<Self>
    where
        Self: Sized;

    #[cfg(feature = "html")]
    fn has_video() -> bool;
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
    fn source(&self) -> &str {
        &self.url
    }

    fn playing(&self) -> bool {
        self.playing.unwrap_or(true)
    }

    fn volume(&self) -> f32 {
        self.volume.unwrap_or(1.)
    }

    fn r#loop(&self) -> bool {
        false
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
    fn source(&self) -> &str {
        &self.src
    }

    fn playing(&self) -> bool {
        self.playing.unwrap_or(true)
    }

    fn volume(&self) -> f32 {
        self.volume.unwrap_or(1.)
    }

    fn r#loop(&self) -> bool {
        self.r#loop.unwrap_or(false)
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
                (activate_receiver, deactivate_receiver),
            )
                .chain()
                .in_set(SceneSets::PostLoop),
        );

        #[cfg(feature = "ffmpeg")]
        app.add_observer(audio_sink::change_audio_sink_volume::<AudioStream>);
        #[cfg(feature = "ffmpeg")]
        app.add_observer(audio_sink::change_audio_sink_volume::<VideoPlayer>);

        #[cfg(feature = "av_player_debug")]
        app.add_plugins(av_player_debug::AvPlayerDebugPlugin);
    }
}

fn av_player_is_in_scene<T: AVPlayer>(
    mut commands: Commands,
    av_players: Query<(Entity, &ContainerEntity, &T, Has<InScene>)>,
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
        &VideoPlayer,
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
            |(ent, player, has_in_scene, has_should_be_playing, transform)| {
                if player.playing() {
                    let distance = if !has_in_scene && player.source() == LIVEKIT_VIDEO_STREAM {
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

#[expect(clippy::type_complexity)]
fn activate_receiver(
    mut commands: Commands,
    av_players: Populated<
        Entity,
        (
            With<VideoPlayer>,
            With<Stream>,
            With<ShouldBePlaying<VideoPlayer>>,
            Without<ActiveReceiver>,
        ),
    >,
) {
    for entity in av_players.into_inner() {
        commands.entity(entity).try_insert(ActiveReceiver);
    }
}

#[expect(clippy::type_complexity)]
fn deactivate_receiver(
    mut commands: Commands,
    av_players: Populated<
        Entity,
        (
            With<VideoPlayer>,
            Without<ShouldBePlaying<VideoPlayer>>,
            With<ActiveReceiver>,
        ),
    >,
) {
    for entity in av_players.into_inner() {
        commands.entity(entity).try_remove::<ActiveReceiver>();
    }
}
