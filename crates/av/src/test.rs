use bevy::{
    diagnostic::FrameCountPlugin,
    log::{Level, LogPlugin},
    platform::collections::HashSet,
    prelude::*,
    state::app::StatesPlugin,
};
use common::structs::{AudioSettings, PrimaryCameraRes};
use dcl::SceneId;
use dcl_component::{
    proto_components::sdk::components::{PbAudioStream, PbVideoPlayer},
    SceneEntityId,
};
use ipfs::IpfsIoPlugin;
use livestream_manager::{plugin::LivestreamManagerPlugin, ActiveReceiver, ReceiverImage};
use scene_runner::{
    renderer_context::RendererSceneContext, update_world::material::VideoTextureOutput,
    ContainerEntity, SceneRunnerPlugin,
};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::*;

#[cfg(feature = "html")]
use crate::html_video_player::HtmlMediaEntity;
#[cfg(feature = "ffmpeg")]
use crate::AVSinks;
use crate::{
    AVPlayer, AVPlayerPlugin, AudioStream, InScene, ShouldBePlaying, Stream, VideoPlayer,
    LIVEKIT_VIDEO_STREAM,
};

#[cfg(target_arch = "wasm32")]
wasm_bindgen_test_configure!(run_in_browser);

const EXAMPLE_VIDEO: &str = "https://example.com/video.mp4";

macro_rules! test_component {
    ($app:expr, $entity:expr, $component:ty, $expected:expr) => {
        assert_eq!(
            $app.world().entity($entity).contains::<$component>(),
            $expected
        );
    };
}

#[cfg(any(feature = "ffmpeg", feature = "html"))]
#[expect(clippy::too_many_arguments)]
fn test_components<T: AVPlayer>(
    app: &mut App,
    entity: Entity,
    component: bool,
    source: bool,
    config: bool,
    position: bool,
    stream: bool,
    in_scene: bool,
    should_be_playing: bool,
    active_receiver: bool,
    receiver_image: bool,
    video_texture_output: bool,
    media: bool,
) {
    test_component!(app, entity, T, component);
    test_component!(app, entity, T::Source, source);
    test_component!(app, entity, T::Config, config);
    test_component!(app, entity, T::Position, position);
    test_component!(app, entity, Stream, stream);
    test_component!(app, entity, InScene, in_scene);
    test_component!(app, entity, ShouldBePlaying<T>, should_be_playing);
    test_component!(app, entity, ActiveReceiver, active_receiver);
    test_component!(app, entity, ReceiverImage, receiver_image);
    test_component!(app, entity, VideoTextureOutput, video_texture_output);
    #[cfg(feature = "ffmpeg")]
    test_component!(app, entity, AVSinks<T>, media);
    #[cfg(feature = "html")]
    test_component!(app, entity, HtmlMediaEntity<T>, media);
}

#[cfg(any(feature = "ffmpeg", feature = "html"))]
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
        FrameCountPlugin,
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
    app.init_resource::<AudioSettings>();

    app.finish();

    app.world_mut().run_schedule(Startup);

    #[cfg(target_arch = "wasm32")]
    {
        let set_video_source = wasm_bindgen::closure::Closure::new(Box::new(set_video_source)
            as Box<dyn FnMut(wasm_bindgen::JsValue, wasm_bindgen::JsValue)>)
        .into_js_value();
        let window = web_sys::window().unwrap();
        js_sys::Reflect::set(
            &window,
            &wasm_bindgen::JsValue::from_str("setVideoSource"),
            &set_video_source,
        )
        .unwrap();
    }

    app
}

#[cfg(target_arch = "wasm32")]
fn set_video_source(_p0: wasm_bindgen::JsValue, _p1: wasm_bindgen::JsValue) {
    debug!("set_video_source");
}

#[cfg(any(feature = "ffmpeg", feature = "html"))]
#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_insert_audio_stream() {
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

    let audio_stream = app
        .world_mut()
        .spawn((
            AudioStream(PbAudioStream {
                url: EXAMPLE_VIDEO.to_owned(),
                ..Default::default()
            }),
            ContainerEntity {
                container: renderer_context,
                root: renderer_context,
                container_id: SceneEntityId::new(0, 0),
            },
        ))
        .id();

    test_components::<AudioStream>(
        &mut app,
        audio_stream,
        true,
        true,
        true,
        true,
        false,
        false,
        false,
        false,
        false,
        cfg!(not(target_arch = "wasm32")),
        true,
    );
}

#[cfg(any(feature = "ffmpeg", feature = "html"))]
#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_insert_empty_audio_stream() {
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

    let audio_stream = app
        .world_mut()
        .spawn((
            AudioStream(PbAudioStream {
                url: "".to_owned(),
                ..Default::default()
            }),
            ContainerEntity {
                container: renderer_context,
                root: renderer_context,
                container_id: SceneEntityId::new(0, 0),
            },
        ))
        .id();

    test_components::<AudioStream>(
        &mut app,
        audio_stream,
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
        true,
    );
}

#[cfg(any(feature = "ffmpeg", feature = "html"))]
#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_insert_livekit_audio_stream() {
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

    let audio_stream = app
        .world_mut()
        .spawn((
            AudioStream(PbAudioStream {
                url: LIVEKIT_VIDEO_STREAM.to_owned(),
                ..Default::default()
            }),
            ContainerEntity {
                container: renderer_context,
                root: renderer_context,
                container_id: SceneEntityId::new(0, 0),
            },
        ))
        .id();

    test_components::<AudioStream>(
        &mut app,
        audio_stream,
        true,
        true,
        true,
        true,
        false,
        false,
        false,
        false,
        false,
        cfg!(not(target_arch = "wasm32")),
        true,
    );
}

#[cfg(any(feature = "ffmpeg", feature = "html"))]
#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
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
                src: EXAMPLE_VIDEO.to_owned(),
                ..Default::default()
            }),
            ContainerEntity {
                container: renderer_context,
                root: renderer_context,
                container_id: SceneEntityId::new(0, 0),
            },
        ))
        .id();

    test_components::<VideoPlayer>(
        &mut app,
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
        true,
    );
}

#[cfg(any(feature = "ffmpeg", feature = "html"))]
#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
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

    test_components::<VideoPlayer>(
        &mut app,
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
        true,
    );
}

#[cfg(any(feature = "ffmpeg", feature = "html"))]
#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
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

    test_components::<VideoPlayer>(
        &mut app,
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
        false,
    );
}

#[cfg(any(feature = "ffmpeg", feature = "html"))]
#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
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
                src: EXAMPLE_VIDEO.to_owned(),
                ..Default::default()
            }),
            ContainerEntity {
                container: renderer_context,
                root: renderer_context,
                container_id: SceneEntityId::new(0, 0),
            },
        ))
        .id();

    test_components::<VideoPlayer>(
        &mut app,
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
        true,
    );

    app.world_mut()
        .entity_mut(video_player)
        .remove::<VideoPlayer>();

    test_components::<VideoPlayer>(
        &mut app,
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
        false,
    );
}

#[cfg(any(feature = "ffmpeg", feature = "html"))]
#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
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

    test_components::<VideoPlayer>(
        &mut app,
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
        false,
    );

    app.world_mut()
        .entity_mut(video_player)
        .remove::<VideoPlayer>();

    test_components::<VideoPlayer>(
        &mut app,
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
        false,
    );
}

#[cfg(any(feature = "ffmpeg", feature = "html"))]
#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
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

    test_components::<VideoPlayer>(
        &mut app,
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
        false,
    );

    app.world_mut().entity_mut(video_player).insert(InScene);

    test_components::<VideoPlayer>(
        &mut app,
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
        false,
    );

    app.world_mut()
        .entity_mut(video_player)
        .insert(ShouldBePlaying::<VideoPlayer>::default());

    test_components::<VideoPlayer>(
        &mut app,
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
        false,
    );

    app.world_mut()
        .entity_mut(video_player)
        .remove::<ShouldBePlaying<VideoPlayer>>();

    test_components::<VideoPlayer>(
        &mut app,
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
        false,
    );

    app.world_mut().entity_mut(video_player).remove::<InScene>();

    test_components::<VideoPlayer>(
        &mut app,
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
        false,
    );
}

#[cfg(any(feature = "ffmpeg", feature = "html"))]
#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_source_change() {
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
            InScene,
            ShouldBePlaying::<VideoPlayer>::default(),
        ))
        .id();

    test_components::<VideoPlayer>(
        &mut app,
        video_player,
        true,
        true,
        true,
        true,
        false,
        true,
        true,
        false,
        false,
        true,
        true,
    );

    let noop_handle = app
        .world()
        .entity(video_player)
        .get::<VideoTextureOutput>()
        .unwrap()
        .0
        .clone();

    app.world_mut()
        .entity_mut(video_player)
        .insert(VideoPlayer(PbVideoPlayer {
            src: EXAMPLE_VIDEO.to_owned(),
            ..Default::default()
        }));

    test_components::<VideoPlayer>(
        &mut app,
        video_player,
        true,
        true,
        true,
        true,
        false,
        true,
        true,
        false,
        false,
        true,
        true,
    );

    let video_handle = app
        .world()
        .entity(video_player)
        .get::<VideoTextureOutput>()
        .unwrap()
        .0
        .clone();
    assert_eq!(noop_handle, video_handle);

    app.world_mut()
        .entity_mut(video_player)
        .insert(VideoPlayer(PbVideoPlayer {
            src: LIVEKIT_VIDEO_STREAM.to_owned(),
            ..Default::default()
        }));

    test_components::<VideoPlayer>(
        &mut app,
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
        false,
    );

    let stream_handle = app
        .world()
        .entity(video_player)
        .get::<VideoTextureOutput>()
        .unwrap()
        .0
        .clone();
    assert_ne!(video_handle, stream_handle);

    app.world_mut()
        .entity_mut(video_player)
        .insert(VideoPlayer(PbVideoPlayer {
            src: EXAMPLE_VIDEO.to_owned(),
            ..Default::default()
        }));

    test_components::<VideoPlayer>(
        &mut app,
        video_player,
        true,
        true,
        true,
        true,
        false,
        true,
        true,
        false,
        false,
        true,
        true,
    );

    let video_handle = app
        .world()
        .entity(video_player)
        .get::<VideoTextureOutput>()
        .unwrap()
        .0
        .clone();
    assert_ne!(stream_handle, video_handle);

    app.world_mut()
        .entity_mut(video_player)
        .insert(VideoPlayer(PbVideoPlayer {
            src: EXAMPLE_VIDEO.to_owned(),
            ..Default::default()
        }));

    test_components::<VideoPlayer>(
        &mut app,
        video_player,
        true,
        true,
        true,
        true,
        false,
        true,
        true,
        false,
        false,
        true,
        true,
    );

    let repeat_video_handle = app
        .world()
        .entity(video_player)
        .get::<VideoTextureOutput>()
        .unwrap()
        .0
        .clone();
    assert_eq!(video_handle, repeat_video_handle);
}

#[cfg(any(feature = "ffmpeg", feature = "html"))]
#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_despawn_av_player() {
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
                src: EXAMPLE_VIDEO.to_owned(),
                ..Default::default()
            }),
            ContainerEntity {
                container: renderer_context,
                root: renderer_context,
                container_id: SceneEntityId::new(0, 0),
            },
            InScene,
            ShouldBePlaying::<VideoPlayer>::default(),
        ))
        .id();

    app.world_mut().entity_mut(video_player).despawn();
}
