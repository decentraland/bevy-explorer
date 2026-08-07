use bevy::{
    log::{Level, LogPlugin},
    prelude::*,
    state::app::StatesPlugin,
};
use common::structs::PrimaryCameraRes;
use dcl_component::proto_components::sdk::components::PbVideoPlayer;
#[cfg(feature = "ffmpeg")]
use ffmpeg_next::format::input;
use livestream_manager::{plugin::LivestreamManagerPlugin, ActiveReceiver};
use scene_runner::{update_world::material::VideoTextureOutput, SceneRunnerPlugin};

#[cfg(feature = "ffmpeg")]
use crate::video_context::VideoContext;
use crate::{
    AVPlayerPlugin, InScene, ShouldBePlaying, Stream, VideoPlayer, VideoPlayerConfig,
    VideoPlayerPosition, VideoPlayerSource, LIVEKIT_VIDEO_STREAM,
};

#[cfg(feature = "ffmpeg")]
#[test]
fn test_ffmpeg() {
    let context = input(
        &"https://vz-7c61c1b5-d59.b-cdn.net/ccea595a-b910-4de6-b160-092819db021d/play_480p.mp4"
            .to_owned(),
    )
    .unwrap();
    let (sx, _rx) = tokio::sync::mpsc::channel(1);
    VideoContext::init(&context, sx).unwrap();
}

fn min_test_app() -> App {
    let mut app = App::new();

    app.add_plugins((
        LogPlugin {
            level: Level::DEBUG,
            ..Default::default()
        },
        AssetPlugin::default(),
        StatesPlugin,
    ));

    app.init_asset::<Image>();
    app.init_asset::<Shader>();
    app.init_asset::<Mesh>();

    app.add_plugins((
        SceneRunnerPlugin,
        LivestreamManagerPlugin,
        AVPlayerPlugin,
        bevy_console::ConsolePlugin,
    ));

    app.insert_resource(PrimaryCameraRes(Entity::PLACEHOLDER));

    app.finish();

    app.world_mut().run_schedule(Startup);

    app
}

#[test]
fn test_insert_video_player() {
    let mut app = min_test_app();

    let video_player = app
        .world_mut()
        .spawn(VideoPlayer(PbVideoPlayer {
            src: "https://example.com".to_owned(),
            ..Default::default()
        }))
        .id();

    assert!(app.world().entity(video_player).contains::<VideoPlayer>());
    assert!(app
        .world()
        .entity(video_player)
        .contains::<VideoPlayerSource>());
    assert!(app
        .world()
        .entity(video_player)
        .contains::<VideoPlayerConfig>());
    assert!(app
        .world()
        .entity(video_player)
        .contains::<VideoPlayerPosition>());
    assert!(!app.world().entity(video_player).contains::<Stream>());
    assert!(!app.world().entity(video_player).contains::<InScene>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<ShouldBePlaying<VideoPlayer>>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<VideoTextureOutput>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<ActiveReceiver>());
}

#[test]
fn test_insert_livekit_video_player() {
    let mut app = min_test_app();

    let video_player = app
        .world_mut()
        .spawn(VideoPlayer(PbVideoPlayer {
            src: LIVEKIT_VIDEO_STREAM.to_owned(),
            ..Default::default()
        }))
        .id();

    assert!(app.world().entity(video_player).contains::<VideoPlayer>());
    assert!(app
        .world()
        .entity(video_player)
        .contains::<VideoPlayerSource>());
    assert!(app
        .world()
        .entity(video_player)
        .contains::<VideoPlayerConfig>());
    assert!(app
        .world()
        .entity(video_player)
        .contains::<VideoPlayerPosition>());
    assert!(app.world().entity(video_player).contains::<Stream>());
    assert!(!app.world().entity(video_player).contains::<InScene>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<ShouldBePlaying<VideoPlayer>>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<VideoTextureOutput>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<ActiveReceiver>());
}

#[test]
fn test_removing_video_player() {
    let mut app = min_test_app();

    let video_player = app
        .world_mut()
        .spawn(VideoPlayer(PbVideoPlayer {
            src: "https://example.com".to_owned(),
            ..Default::default()
        }))
        .id();

    assert!(app.world().entity(video_player).contains::<VideoPlayer>());
    assert!(app
        .world()
        .entity(video_player)
        .contains::<VideoPlayerSource>());
    assert!(app
        .world()
        .entity(video_player)
        .contains::<VideoPlayerConfig>());
    assert!(app
        .world()
        .entity(video_player)
        .contains::<VideoPlayerPosition>());
    assert!(!app.world().entity(video_player).contains::<Stream>());
    assert!(!app.world().entity(video_player).contains::<InScene>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<ShouldBePlaying<VideoPlayer>>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<VideoTextureOutput>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<ActiveReceiver>());

    app.world_mut()
        .entity_mut(video_player)
        .remove::<VideoPlayer>();

    assert!(!app.world().entity(video_player).contains::<VideoPlayer>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<VideoPlayerSource>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<VideoPlayerConfig>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<VideoPlayerPosition>());
    assert!(!app.world().entity(video_player).contains::<Stream>());
    assert!(!app.world().entity(video_player).contains::<InScene>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<ShouldBePlaying<VideoPlayer>>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<VideoTextureOutput>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<ActiveReceiver>());
}

#[test]
fn test_removing_livekit_video_player() {
    let mut app = min_test_app();

    let video_player = app
        .world_mut()
        .spawn(VideoPlayer(PbVideoPlayer {
            src: LIVEKIT_VIDEO_STREAM.to_owned(),
            ..Default::default()
        }))
        .id();

    assert!(app.world().entity(video_player).contains::<VideoPlayer>());
    assert!(app
        .world()
        .entity(video_player)
        .contains::<VideoPlayerSource>());
    assert!(app
        .world()
        .entity(video_player)
        .contains::<VideoPlayerConfig>());
    assert!(app
        .world()
        .entity(video_player)
        .contains::<VideoPlayerPosition>());
    assert!(app.world().entity(video_player).contains::<Stream>());
    assert!(!app.world().entity(video_player).contains::<InScene>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<ShouldBePlaying<VideoPlayer>>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<VideoTextureOutput>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<ActiveReceiver>());

    app.world_mut()
        .entity_mut(video_player)
        .remove::<VideoPlayer>();

    assert!(!app.world().entity(video_player).contains::<VideoPlayer>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<VideoPlayerSource>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<VideoPlayerConfig>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<VideoPlayerPosition>());
    assert!(!app.world().entity(video_player).contains::<Stream>());
    assert!(!app.world().entity(video_player).contains::<InScene>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<ShouldBePlaying<VideoPlayer>>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<VideoTextureOutput>());
    assert!(!app
        .world()
        .entity(video_player)
        .contains::<ActiveReceiver>());
}
