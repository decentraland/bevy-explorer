use bevy::{
    log::{Level, LogPlugin},
    platform::collections::HashSet,
    prelude::*,
    state::app::StatesPlugin,
};
use common::structs::PrimaryCameraRes;
use dcl::SceneId;
use dcl_component::{proto_components::sdk::components::PbVideoPlayer, SceneEntityId};
#[cfg(feature = "ffmpeg")]
use ffmpeg_next::format::input;
use ipfs::IpfsIoPlugin;
use livestream_manager::{plugin::LivestreamManagerPlugin, ActiveReceiver, ReceiverImage};
use scene_runner::{
    renderer_context::RendererSceneContext, update_world::material::VideoTextureOutput,
    ContainerEntity, SceneRunnerPlugin,
};

#[cfg(feature = "ffmpeg")]
use crate::video_context::VideoContext;
use crate::{
    AVPlayerPlugin, AVSinks, InScene, ShouldBePlaying, Stream, VideoPlayer, VideoPlayerConfig,
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

macro_rules! test_components {
    (
        $app:expr,
        $entity:expr,
        $video_player:tt,
        $video_player_source:tt,
        $video_player_config:tt,
        $video_player_position:tt,
        $stream:tt,
        $in_scene:tt,
        $should_be_playing:tt,
        $active_receiver:tt,
        $receiver_image:tt,
        $video_texture_output:tt,
        $av_sinks:tt
    ) => {
        test_component!($app, $entity, VideoPlayer, $video_player);
        test_component!($app, $entity, VideoPlayerSource, $video_player_source);
        test_component!($app, $entity, VideoPlayerConfig, $video_player_config);
        test_component!($app, $entity, VideoPlayerPosition, $video_player_position);
        test_component!($app, $entity, Stream, $stream);
        test_component!($app, $entity, InScene, $in_scene);
        test_component!(
            $app,
            $entity,
            ShouldBePlaying<VideoPlayer>,
            $should_be_playing
        );
        test_component!($app, $entity, ActiveReceiver, $active_receiver);
        test_component!($app, $entity, ReceiverImage, $receiver_image);
        test_component!($app, $entity, VideoTextureOutput, $video_texture_output);
        test_component!($app, $entity, AVSinks<VideoPlayer>, $av_sinks);
    };
}

macro_rules! test_component {
    ($app:expr, $entity:expr, $component:ty, true) => {
        assert!($app.world().entity($entity).contains::<$component>());
    };
    ($app:expr, $entity:expr, $component:ty, false) => {
        assert!(!$app.world().entity($entity).contains::<$component>());
    };
}

fn min_test_app() -> App {
    let mut app = App::new();

    app.add_plugins((
        LogPlugin {
            level: Level::DEBUG,
            ..Default::default()
        },
        IpfsIoPlugin {
            preview: false,
            starting_realm: None,
            content_server_override: None,
            assets_root: Default::default(),
            num_slots: 1,
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

    let renderer_context = app
        .world_mut()
        .spawn(RendererSceneContext::new(
            SceneId::DUMMY,
            "hash".to_owned(),
            "storage_root".to_owned(),
            false,
            0,
            "title".to_owned(),
            IVec2::splat(0),
            HashSet::from_iter([IVec2::splat(0)]),
            vec![],
            vec![],
            Entity::PLACEHOLDER,
            0.,
            false,
            "sdk_version",
            false,
            false,
        ))
        .id();

    let video_player = app
        .world_mut()
        .spawn((
            VideoPlayer(PbVideoPlayer {
                src: "https://example.com".to_owned(),
                ..Default::default()
            }),
            ContainerEntity {
                container: renderer_context,
                root: renderer_context,
                container_id: SceneEntityId::new(0, 0),
            },
        ))
        .id();

    test_components!(
        app,
        video_player,
        true,
        true,
        true,
        true,
        false,
        false,
        false,
        false,
        false,
        true,
        true
    );
}

#[test]
fn test_insert_empty_video_player() {
    let mut app = min_test_app();

    let renderer_context = app
        .world_mut()
        .spawn(RendererSceneContext::new(
            SceneId::DUMMY,
            "hash".to_owned(),
            "storage_root".to_owned(),
            false,
            0,
            "title".to_owned(),
            IVec2::splat(0),
            HashSet::from_iter([IVec2::splat(0)]),
            vec![],
            vec![],
            Entity::PLACEHOLDER,
            0.,
            false,
            "sdk_version",
            false,
            false,
        ))
        .id();

    let video_player = app
        .world_mut()
        .spawn((
            VideoPlayer(PbVideoPlayer {
                src: "".to_owned(),
                ..Default::default()
            }),
            ContainerEntity {
                container: renderer_context,
                root: renderer_context,
                container_id: SceneEntityId::new(0, 0),
            },
        ))
        .id();

    test_components!(
        app,
        video_player,
        true,
        true,
        true,
        true,
        false,
        false,
        false,
        false,
        false,
        true,
        true
    );
}

#[test]
fn test_insert_livekit_video_player() {
    let mut app = min_test_app();

    let renderer_context = app
        .world_mut()
        .spawn(RendererSceneContext::new(
            SceneId::DUMMY,
            "hash".to_owned(),
            "storage_root".to_owned(),
            false,
            0,
            "title".to_owned(),
            IVec2::splat(0),
            HashSet::from_iter([IVec2::splat(0)]),
            vec![],
            vec![],
            Entity::PLACEHOLDER,
            0.,
            false,
            "sdk_version",
            false,
            false,
        ))
        .id();

    let video_player = app
        .world_mut()
        .spawn((
            VideoPlayer(PbVideoPlayer {
                src: LIVEKIT_VIDEO_STREAM.to_owned(),
                ..Default::default()
            }),
            ContainerEntity {
                container: renderer_context,
                root: renderer_context,
                container_id: SceneEntityId::new(0, 0),
            },
        ))
        .id();

    test_components!(
        app,
        video_player,
        true,
        true,
        true,
        true,
        true,
        false,
        false,
        false,
        false,
        false,
        false
    );
}

#[test]
fn test_removing_video_player() {
    let mut app = min_test_app();

    let renderer_context = app
        .world_mut()
        .spawn(RendererSceneContext::new(
            SceneId::DUMMY,
            "hash".to_owned(),
            "storage_root".to_owned(),
            false,
            0,
            "title".to_owned(),
            IVec2::splat(0),
            HashSet::from_iter([IVec2::splat(0)]),
            vec![],
            vec![],
            Entity::PLACEHOLDER,
            0.,
            false,
            "sdk_version",
            false,
            false,
        ))
        .id();

    let video_player = app
        .world_mut()
        .spawn((
            VideoPlayer(PbVideoPlayer {
                src: "https://example.com".to_owned(),
                ..Default::default()
            }),
            ContainerEntity {
                container: renderer_context,
                root: renderer_context,
                container_id: SceneEntityId::new(0, 0),
            },
        ))
        .id();

    test_components!(
        app,
        video_player,
        true,
        true,
        true,
        true,
        false,
        false,
        false,
        false,
        false,
        true,
        true
    );

    app.world_mut()
        .entity_mut(video_player)
        .remove::<VideoPlayer>();

    test_components!(
        app,
        video_player,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false
    );
}

#[test]
fn test_removing_livekit_video_player() {
    let mut app = min_test_app();

    let renderer_context = app
        .world_mut()
        .spawn(RendererSceneContext::new(
            SceneId::DUMMY,
            "hash".to_owned(),
            "storage_root".to_owned(),
            false,
            0,
            "title".to_owned(),
            IVec2::splat(0),
            HashSet::from_iter([IVec2::splat(0)]),
            vec![],
            vec![],
            Entity::PLACEHOLDER,
            0.,
            false,
            "sdk_version",
            false,
            false,
        ))
        .id();

    let video_player = app
        .world_mut()
        .spawn((
            VideoPlayer(PbVideoPlayer {
                src: LIVEKIT_VIDEO_STREAM.to_owned(),
                ..Default::default()
            }),
            ContainerEntity {
                container: renderer_context,
                root: renderer_context,
                container_id: SceneEntityId::new(0, 0),
            },
        ))
        .id();

    test_components!(
        app,
        video_player,
        true,
        true,
        true,
        true,
        true,
        false,
        false,
        false,
        false,
        false,
        false
    );

    app.world_mut()
        .entity_mut(video_player)
        .remove::<VideoPlayer>();

    test_components!(
        app,
        video_player,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false
    );
}

#[test]
fn test_livekit_video_player_without_should_be_playing_should_not_be_receiver() {
    let mut app = min_test_app();

    let renderer_context = app
        .world_mut()
        .spawn(RendererSceneContext::new(
            SceneId::DUMMY,
            "hash".to_owned(),
            "storage_root".to_owned(),
            false,
            0,
            "title".to_owned(),
            IVec2::splat(0),
            HashSet::from_iter([IVec2::splat(0)]),
            vec![],
            vec![],
            Entity::PLACEHOLDER,
            0.,
            false,
            "sdk_version",
            false,
            false,
        ))
        .id();

    let video_player = app
        .world_mut()
        .spawn((
            VideoPlayer(PbVideoPlayer {
                src: LIVEKIT_VIDEO_STREAM.to_owned(),
                ..Default::default()
            }),
            ContainerEntity {
                container: renderer_context,
                root: renderer_context,
                container_id: SceneEntityId::new(0, 0),
            },
        ))
        .id();

    test_components!(
        app,
        video_player,
        true,
        true,
        true,
        true,
        true,
        false,
        false,
        false,
        false,
        false,
        false
    );

    app.world_mut().entity_mut(video_player).insert(InScene);

    test_components!(
        app,
        video_player,
        true,
        true,
        true,
        true,
        true,
        true,
        false,
        false,
        false,
        false,
        false
    );

    app.world_mut()
        .entity_mut(video_player)
        .insert(ShouldBePlaying::<VideoPlayer>::default());

    test_components!(
        app,
        video_player,
        true,
        true,
        true,
        true,
        true,
        true,
        true,
        true,
        true,
        true,
        false
    );

    app.world_mut()
        .entity_mut(video_player)
        .remove::<ShouldBePlaying<VideoPlayer>>();

    test_components!(
        app,
        video_player,
        true,
        true,
        true,
        true,
        true,
        true,
        false,
        false,
        false,
        false,
        false
    );

    app.world_mut().entity_mut(video_player).remove::<InScene>();

    test_components!(
        app,
        video_player,
        true,
        true,
        true,
        true,
        true,
        false,
        false,
        false,
        false,
        false,
        false
    );
}
