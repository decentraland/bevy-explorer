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

#[cfg(feature = "av_player_debug")]
pub mod av_player_debug;

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
#[cfg(feature = "livekit")]
use {
    bevy::ecs::relationship::Relationship,
    comms::livekit::participant::{StreamViewer, Streamer},
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
        #[cfg(feature = "livekit")]
        {
            app.add_observer(stream_should_be_played);
            app.add_observer(stream_shouldnt_be_played);
            app.add_observer(streamer_joined);
        }

        #[cfg(feature = "av_player_debug")]
        app.add_plugins(av_player_debug::AvPlayerDebugPlugin);
    }
}

fn av_player_is_in_scene(
    mut commands: Commands,
    av_players: Query<(Entity, &ContainerEntity, &AVPlayer, Has<InScene>)>,
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
        .filter(|(_, _, player, _)| player.source.playing.unwrap_or(true))
    {
        let contained = containing_scenes.contains(&container.root);
        if contained && !has_in_scene {
            // Only call `insert` on those that do not have `InScene`
            commands.entity(ent).insert(InScene);
        } else if !contained && has_in_scene {
            // Only call `remove` on those that have `InScene`
            commands.entity(ent).remove::<InScene>();
        }
    }
}

#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn av_player_should_be_playing(
    mut commands: Commands,
    av_players: Query<(
        Entity,
        &AVPlayer,
        Has<InScene>,
        Has<ShouldBePlaying>,
        &GlobalTransform,
    )>,
    user: Single<&GlobalTransform, With<PrimaryUser>>,
    config: Res<AppConfig>,
) {
    let mut sorted_players = av_players
        .iter()
        .filter_map(
            |(ent, player, has_in_scene, has_should_be_playing, transform)| {
                if player.source.playing.unwrap_or(true) {
                    let distance =
                        if !has_in_scene && player.source.src.starts_with("livekit-video://") {
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
        commands.entity(ent).try_remove::<ShouldBePlaying>();
    }

    for ent in sorted_players
        .iter()
        .take(config.max_videos)
        // Only call `insert` on those that do not have `ShouldBePlaying`
        // The `filter` MUST be after the `take`
        .filter(|(_, has_should_be_playing, _, _)| !*has_should_be_playing)
        .map(|(_, _, _, ent)| *ent)
    {
        commands.entity(ent).try_insert(ShouldBePlaying);
    }
}

#[cfg(feature = "livekit")]
fn stream_should_be_played(
    trigger: Trigger<OnAdd, ShouldBePlaying>,
    mut commands: Commands,
    av_players: Query<&AVPlayer>,
    streamer: Single<Entity, With<Streamer>>,
) {
    let entity = trigger.target();
    let Ok(av_player) = av_players.get(entity) else {
        unreachable!("ShouldBePlaying must only be added to AVPlayers.");
    };

    if av_player.source.src.starts_with("livekit-video://") {
        debug!("AVPlayer {entity} should be playing. Linking to the stream.");
        commands
            .entity(entity)
            .insert(<StreamViewer as Relationship>::from(*streamer));
    }
}

#[cfg(feature = "livekit")]
fn stream_shouldnt_be_played(
    trigger: Trigger<OnRemove, ShouldBePlaying>,
    mut commands: Commands,
    av_players: Query<(&AVPlayer, Has<StreamViewer>)>,
    mut removed_av_players: RemovedComponents<AVPlayer>,
) {
    let entity = trigger.target();
    if removed_av_players
        .read()
        .any(|removed| removed == entity)
    {
        return;
    }
    let Ok((av_player, has_stream_viewer)) = av_players.get(entity) else {
        unreachable!("ShouldBePlaying must have only been added to AVPlayers.");
    };
    if !has_stream_viewer {
        // Noop if AVPlayer does not have `StreamViewer`
        return;
    }

    if av_player.source.src.starts_with("livekit-video://") {
        debug!("AVPlayer {entity} no longer playing. Unlinking to the stream.");
        commands.entity(entity).try_remove::<StreamViewer>();
    }
}

#[cfg(feature = "livekit")]
fn streamer_joined(
    trigger: Trigger<OnAdd, Streamer>,
    mut commands: Commands,
    av_players: Query<(Entity, &AVPlayer), With<ShouldBePlaying>>,
) {
    let entity = trigger.target();
    debug!("Streamer {entity} has connected. Linking to AVPlayers in range.");

    for (av_player_entity, av_player) in av_players {
        if av_player.source.src.starts_with("livekit-video://") {
            commands
                .entity(av_player_entity)
                .insert(<StreamViewer as Relationship>::from(entity));
        }
    }
}
