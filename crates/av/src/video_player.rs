use std::borrow::Cow;

use bevy::{
    asset::RenderAssetTransferPriority,
    color::palettes::basic,
    diagnostic::FrameCount,
    prelude::*,
    render::{
        render_asset::RenderAssetUsages,
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
    },
};
use common::{sets::SceneSets, util::ReportErr};
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::sdk::components::{PbVideoEvent, VideoState},
    SceneComponentId,
};
use ipfs::IpfsResource;
use scene_runner::{
    renderer_context::RendererSceneContext,
    update_world::material::{update_materials, VideoTextureOutput},
    ContainerEntity,
};
#[cfg(feature = "livekit")]
use {
    bevy::ecs::relationship::Relationship,
    comms::livekit::participant::{ChangeVolume, StreamImage, StreamViewer},
};

use crate::{
    audio_sink::ChangeAudioSinkVolume,
    audio_stream_should_be_playing,
    stream_processor::AVCommand,
    video_context::{VideoData, VideoInfo},
    video_player_should_be_playing,
    video_stream::{av_sinks, noop_sinks},
    AVPlayer, AVPlayerSinks, AudioStream, InScene, VideoPlayer, VideoPlayerSinks,
};

pub struct VideoPlayerPlugin;

impl Plugin for VideoPlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init_ffmpeg);
        app.add_systems(Update, play_videos.before(update_materials));
        app.add_systems(
            Update,
            (
                rebuild_sinks::<AudioStream>.after(audio_stream_should_be_playing),
                rebuild_sinks::<VideoPlayer>.after(video_player_should_be_playing),
            )
                .chain()
                .after(play_videos)
                .in_set(SceneSets::PostLoop),
        );

        app.add_observer(av_player_on_insert::<AudioStream>);
        app.add_observer(av_player_on_insert::<VideoPlayer>);
        app.add_observer(av_player_on_remove::<AudioStream>);
        app.add_observer(av_player_on_remove::<VideoPlayer>);
        app.add_observer(av_player_should_be_playing_on_add::<AudioStream>);
        app.add_observer(av_player_should_be_playing_on_add::<VideoPlayer>);
        app.add_observer(av_player_should_be_playing_on_remove::<AudioStream>);
        app.add_observer(av_player_should_be_playing_on_remove::<VideoPlayer>);
        #[cfg(feature = "livekit")]
        app.add_observer(copy_stream_image);
    }
}

fn init_ffmpeg() {
    ffmpeg_next::init().unwrap();
    ffmpeg_next::log::set_level(ffmpeg_next::log::Level::Error);
}

#[cfg(not(feature = "livekit"))]
type AVPlayerOnInsertQuery<'a, T> = (&'a T, Option<&'a <T as AVPlayer>::Sinks>);
#[cfg(feature = "livekit")]
type AVPlayerOnInsertQuery<'a, T> = (
    &'a T,
    Option<&'a StreamViewer>,
    Option<&'a <T as AVPlayer>::Sinks>,
);

fn av_player_on_insert<T: AVPlayer>(
    trigger: Trigger<OnInsert, T>,
    mut commands: Commands,
    av_players: Query<AVPlayerOnInsertQuery<T>>,
) {
    let entity = trigger.target();
    let Ok(query) = av_players.get(entity) else {
        unreachable!("Infallible query.");
    };
    #[cfg(not(feature = "livekit"))]
    let (av_player, maybe_sinks) = query;
    #[cfg(feature = "livekit")]
    let (av_player, maybe_stream_viewer, maybe_sinks) = query;

    let mayve_audio_sink = maybe_sinks.and_then(|sinks| sinks.audio_sink());
    let maybe_video_sink = maybe_sinks.and_then(|sinks| sinks.video_sink());

    let source = av_player.source();
    let equal_sink = maybe_video_sink
        .as_ref()
        .filter(|video_sink| source == video_sink.source)
        .is_some();
    let livekit_stream = source.starts_with("livekit-video://");
    if !source.is_empty() && (equal_sink || livekit_stream) {
        if livekit_stream {
            #[cfg(feature = "livekit")]
            if let Some(stream_viewer) = maybe_stream_viewer {
                debug!("Updating volume of stream.");
                commands.trigger_targets(ChangeVolume(av_player.volume()), stream_viewer.get());
            }
        } else {
            debug!("Updating sinks of {entity}.");
            // This forces an update on the entity
            commands.entity(entity).try_remove::<T::ShouldBePlaying>();
            if let Some(video_sink) = maybe_video_sink {
                video_sink
                    .command_sender
                    .send(AVCommand::Repeat(av_player.r#loop()))
                    .report();
                video_sink.command_sender.send(AVCommand::Pause).report();
            }
            if let Some(audio_sink) = mayve_audio_sink {
                commands.trigger_targets(
                    ChangeAudioSinkVolume {
                        volume: av_player.volume(),
                    },
                    entity,
                );
                audio_sink.command_sender.send(AVCommand::Pause).report();
            }
        }
    } else {
        if mayve_audio_sink.is_some() || maybe_video_sink.is_some() {
            debug!("Removing sinks of {entity} due to diverging source.");
        }
        if let Some(video_sink) = maybe_video_sink {
            video_sink.command_sender.send(AVCommand::Dispose).report();
        }
        if let Some(audio_sink) = mayve_audio_sink {
            audio_sink.command_sender.send(AVCommand::Dispose).report();
        }
        debug!("{entity:?} has {}.", av_player.source());
        commands
            .entity(entity)
            .try_remove::<(T::Sinks, T::ShouldBePlaying)>();
        #[cfg(feature = "livekit")]
        commands.entity(entity).try_remove::<StreamViewer>();
    }
}

fn av_player_on_remove<T: AVPlayer>(trigger: Trigger<OnRemove, T>, mut commands: Commands) {
    let entity = trigger.target();
    commands
        .entity(entity)
        .try_remove::<(InScene, T::ShouldBePlaying, T::Sinks, VideoTextureOutput)>();
    #[cfg(feature = "livekit")]
    commands
        .entity(entity)
        .try_remove::<(StreamViewer, StreamImage)>();
}

fn av_player_should_be_playing_on_add<T: AVPlayer>(
    trigger: Trigger<OnAdd, T::ShouldBePlaying>,
    av_players: Query<&T::Sinks, With<T>>,
) {
    let entity = trigger.target();
    let Ok(sinks) = av_players.get(entity) else {
        return;
    };

    if let Some(audio_sink) = sinks.audio_sink() {
        audio_sink.command_sender.send(AVCommand::Play).report();
    }
    if let Some(video_sink) = sinks.video_sink() {
        video_sink.command_sender.send(AVCommand::Play).report();
    }
}

fn av_player_should_be_playing_on_remove<T: AVPlayer>(
    trigger: Trigger<OnRemove, T::ShouldBePlaying>,
    av_players: Query<&T::Sinks, With<T>>,
) {
    let entity = trigger.target();
    let Ok(sinks) = av_players.get(entity) else {
        return;
    };

    if let Some(audio_sink) = sinks.audio_sink() {
        audio_sink.command_sender.send(AVCommand::Pause).report();
    }
    if let Some(video_sink) = sinks.video_sink() {
        video_sink.command_sender.send(AVCommand::Pause).report();
    }
}

fn play_videos(
    mut images: ResMut<Assets<Image>>,
    mut q: Query<(
        &mut VideoPlayerSinks,
        &ContainerEntity,
        &mut VideoTextureOutput,
    )>,
    mut scenes: Query<&mut RendererSceneContext>,
    frame: Res<FrameCount>,
) {
    enum FrameSource {
        Video(ffmpeg_next::frame::Video),
    }

    impl FrameSource {
        fn data(&self) -> Cow<'_, [u8]> {
            match self {
                FrameSource::Video(video) => Cow::Borrowed(video.data(0)),
            }
        }
    }

    for (mut video_player_sinks, container, mut output) in q.iter_mut() {
        let Some(sink) = video_player_sinks.video_sink_mut() else {
            continue;
        };

        let mut last_frame_received = None;
        let mut new_state = None;
        loop {
            match sink.video_receiver.try_recv() {
                Ok(VideoData::Info(VideoInfo {
                    width,
                    height,
                    rate,
                    length,
                })) => {
                    debug!("resize");
                    let image = images.get_mut(&sink.image).unwrap();
                    let target_extent = Extent3d {
                        width: width.max(16),
                        height: height.max(16),
                        depth_or_array_layers: 1,
                    };
                    if image.texture_descriptor.size != target_extent {
                        debug!("resize {target_extent:?}");
                        image.data = None;
                        image.texture_descriptor.size = target_extent;
                        image.transfer_priority = RenderAssetTransferPriority::Immediate;
                    }
                    sink.length = Some(length);
                    sink.rate = Some(rate);
                }
                Ok(VideoData::Frame(frame, time)) => {
                    last_frame_received = Some(FrameSource::Video(frame));
                    sink.current_time = time;
                }
                Ok(VideoData::State(state)) => new_state = Some(state),
                Err(_) => break,
            }
        }

        if let Some(frame) = last_frame_received {
            trace!("set frame on {:?}", sink.image);

            let image = images.get_mut(&sink.image).unwrap();

            match &mut image.data {
                Some(data) => {
                    data.copy_from_slice(&frame.data());
                    image.transfer_priority = RenderAssetTransferPriority::Priority(-2);
                }
                None => {
                    image.data = Some(frame.data().into_owned());
                    output.set_changed();
                }
            }
        }

        const VIDEO_REPORT_FREQUENCY: f64 = 1.0;
        if new_state.is_none()
            && (sink.current_time > sink.last_reported_time + VIDEO_REPORT_FREQUENCY
                || sink.current_time < sink.last_reported_time)
        {
            new_state = Some(VideoState::VsPlaying);
        }

        if let Some(state) = new_state {
            if let Ok(mut context) = scenes.get_mut(container.root) {
                trace!("send current time = {}", sink.current_time);
                let event = PbVideoEvent {
                    timestamp: frame.0,
                    tick_number: context.tick_number,
                    current_offset: sink.current_time as f32,
                    video_length: sink.length.unwrap_or(-1.0) as f32,
                    state: state.into(),
                };
                context.update_crdt(
                    SceneComponentId::VIDEO_EVENT,
                    CrdtType::GO_ANY,
                    container.container_id,
                    &event,
                );
                sink.last_reported_time = sink.current_time;
            }
        }
    }
}

#[cfg(not(feature = "livekit"))]
type RebuildSinkFilter<T> = (Without<T>,);
#[cfg(feature = "livekit")]
type RebuildSinkFilter<T> = (Without<T>, Without<StreamViewer>);

#[expect(clippy::type_complexity)]
fn rebuild_sinks<T: AVPlayer>(
    mut commands: Commands,
    video_players: Populated<
        (
            Entity,
            &ContainerEntity,
            &T,
            Option<&VideoTextureOutput>,
            Has<T::ShouldBePlaying>,
        ),
        RebuildSinkFilter<T::Sinks>,
    >,
    scenes: Query<&RendererSceneContext>,
    ipfs: Res<IpfsResource>,
    mut images: ResMut<Assets<Image>>,
) {
    for (ent, container, player, maybe_texture, should_be_playing) in video_players.iter() {
        trace!("Rebuilding sinks for {}.", ent);
        let mut create_image_handle = || match maybe_texture {
            None => {
                let mut image = Image::new_fill(
                    bevy::render::render_resource::Extent3d {
                        width: 8,
                        height: 8,
                        depth_or_array_layers: 1,
                    },
                    TextureDimension::D2,
                    &basic::FUCHSIA.to_u8_array(),
                    TextureFormat::Rgba8UnormSrgb,
                    RenderAssetUsages::all(),
                );
                image.texture_descriptor.usage =
                    TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING;
                image.transfer_priority = RenderAssetTransferPriority::Immediate;
                images.add(image)
            }
            Some(texture) => texture.0.clone(),
        };

        let Ok(context) = scenes.get(container.root) else {
            continue;
        };

        let source = player.source();
        let source_playing = player.playing();
        let (video_sink, audio_sink) = if source.starts_with("livekit-video://") {
            // Done in observers
            continue;
        } else if source.is_empty() {
            let (video_sink, audio_sink) =
                noop_sinks(source.to_owned(), create_image_handle(), player.volume());
            debug!(
                "spawned noop sink for scene @ {} (playing={})",
                context.base, source_playing
            );
            (video_sink, audio_sink)
        } else {
            let (video_sink, audio_sink) = av_sinks(
                ipfs.clone(),
                source.to_owned(),
                context.hash.clone(),
                create_image_handle(),
                player.volume(),
                should_be_playing && source_playing,
                player.r#loop(),
            );
            debug!(
                "spawned av thread for scene @ {} (playing={})",
                context.base, source_playing
            );
            (video_sink, audio_sink)
        };
        let video_output = VideoTextureOutput(video_sink.image.clone());
        commands.entity(ent).try_insert((
            video_output,
            T::build_sink_component(audio_sink, video_sink),
        ));
    }
}

#[cfg(feature = "livekit")]
fn copy_stream_image(
    trigger: Trigger<OnInsert, StreamImage>,
    mut commands: Commands,
    stream_viewers: Query<&StreamImage, With<StreamViewer>>,
) {
    let entity = trigger.target();
    let Ok(stream_image) = stream_viewers.get(entity) else {
        // StreamImage added to something that is not a StreamViewer
        return;
    };
    debug!("Adding VideoTextureOutput to {entity}.");
    commands
        .entity(entity)
        .try_insert(VideoTextureOutput((**stream_image).clone()));
}
