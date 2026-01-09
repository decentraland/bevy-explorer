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

use audio_source::AudioSourcePlugin;
#[cfg(not(feature = "html"))]
use audio_source_native::AudioSourcePluginImpl;
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

#[derive(Component, Debug)]
#[component(immutable)]
pub struct AVPlayer {
    // note we reuse PbVideoPlayer for audio as well
    pub source: PbVideoPlayer,
    #[cfg(feature = "html")]
    pub has_video: bool,
}

impl From<PbVideoPlayer> for AVPlayer {
    fn from(value: PbVideoPlayer) -> Self {
        Self {
            source: value,
            #[cfg(feature = "html")]
            has_video: true,
        }
    }
}

impl From<PbAudioStream> for AVPlayer {
    fn from(value: PbAudioStream) -> Self {
        Self {
            source: PbVideoPlayer {
                src: value.url,
                playing: value.playing,
                volume: value.volume,
                ..Default::default()
            },
            #[cfg(feature = "html")]
            has_video: false,
        }
    }
}

/// Marks whether an [`AVPlayer`] should be playing
#[derive(Debug, Component)]
pub struct ShouldBePlaying;

/// Marks whether an [`AVPlayer`] is in the same scene as the [`PrimaryUser`]
#[derive(Debug, Component)]
pub struct InScene;

#[derive(Default)]
pub struct AVPlayerPlugin;

impl Plugin for AVPlayerPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(any(feature = "ffmpeg", feature = "html"))]
        app.add_plugins(VideoPlayerPlugin);
        app.add_plugins(AudioSourcePlugin);
        app.add_plugins(AudioSourcePluginImpl);

        app.add_crdt_lww_component::<PbVideoPlayer, AVPlayer>(
            SceneComponentId::VIDEO_PLAYER,
            ComponentPosition::EntityOnly,
        );
        app.add_crdt_lww_component::<PbAudioStream, AVPlayer>(
            SceneComponentId::AUDIO_STREAM,
            ComponentPosition::EntityOnly,
        );

        #[cfg(feature = "ffmpeg")]
        app.add_systems(
            PostUpdate,
            (spawn_audio_streams, spawn_and_locate_foreign_streams).chain(),
        );
        app.add_systems(
            Update,
            (av_player_is_in_scene, av_player_should_be_playing).in_set(SceneSets::PostLoop),
        );

        #[cfg(feature = "ffmpeg")]
        app.add_observer(audio_sink::change_audio_sink_volume);
    }
}

fn av_player_is_in_scene(
    mut commands: Commands,
    av_players: Query<(Entity, &ContainerEntity, &AVPlayer)>,
    user: Query<&GlobalTransform, With<PrimaryUser>>,
    containing_scene: ContainingScene,
) {
    // disable distant av
    let Ok(user) = user.single() else {
        return;
    };
    let containing_scenes = containing_scene.get_position(user.translation());

    for (ent, container, _) in av_players
        .iter()
        .filter(|(_, _, player)| player.source.playing.unwrap_or(true))
    {
        if containing_scenes.contains(&container.root) {
            commands.entity(ent).insert_if_new(InScene);
        } else {
            commands.entity(ent).remove::<InScene>();
        }
    }
}

fn av_player_should_be_playing(
    mut commands: Commands,
    av_players: Query<(Entity, &AVPlayer, Has<InScene>, &GlobalTransform)>,
    user: Query<&GlobalTransform, With<PrimaryUser>>,
    config: Res<AppConfig>,
) {
    // disable distant av
    let Ok(user) = user.single() else {
        return;
    };

    let mut sorted_players = av_players
        .iter()
        .filter_map(|(ent, player, in_scene, transform)| {
            if player.source.playing.unwrap_or(true) {
                let distance = transform.translation().distance(user.translation());
                Some((in_scene, distance, ent))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    // prioritise av in current scene (false < true), then by distance
    sorted_players.sort_by_key(|(in_scene, distance, _)| (!in_scene, FloatOrd(*distance)));

    // Removing first for better Trigger ordering
    for ent in sorted_players
        .iter()
        .skip(config.max_videos)
        .map(|(_, _, ent)| *ent)
    {
        commands.entity(ent).try_remove::<ShouldBePlaying>();
    }

    for ent in sorted_players
        .iter()
        .take(config.max_videos)
        .map(|(_, _, ent)| *ent)
    {
        commands.entity(ent).try_insert(ShouldBePlaying);
    }
}
