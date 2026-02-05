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
#[cfg(feature = "livekit")]
use comms::livekit::participant::{StreamImage, StreamViewer};
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

use crate::{
    audio_sink::{AudioSink, ChangeAudioSinkVolume},
    av_player_should_be_playing,
    stream_processor::AVCommand,
    video_context::{VideoData, VideoInfo},
    video_stream::{av_sinks, noop_sinks, VideoSink},
    AVPlayer, InScene, ShouldBePlaying,
};

pub struct VideoPlayerPlugin;

impl Plugin for VideoPlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init_ffmpeg);
        app.add_systems(Update, play_videos.before(update_materials));
        app.add_systems(
            Update,
            rebuild_sinks
                .after(play_videos)
                .after(av_player_should_be_playing)
                .in_set(SceneSets::PostLoop),
        );

        app.add_observer(av_player_on_insert);
        app.add_observer(av_player_on_remove);
        app.add_observer(av_player_should_be_playing_on_add);
        app.add_observer(av_player_should_be_playing_on_remove);
        #[cfg(feature = "livekit")]
        app.add_observer(copy_stream_image);
    }
}

fn init_ffmpeg() {
    ffmpeg_next::init().unwrap();
    ffmpeg_next::log::set_level(ffmpeg_next::log::Level::Error);
}

fn av_player_on_insert(
    trigger: Trigger<OnInsert, AVPlayer>,
    mut commands: Commands,
    av_players: Query<(&AVPlayer, Option<&AudioSink>, Option<&VideoSink>)>,
) {
    let entity = trigger.target();
    let Ok((av_player, maybe_audio_sink, maybe_video_sink)) = av_players.get(entity) else {
        unreachable!("Infallible query.");
    };

    // This forces an update on the entity
    commands.entity(entity).try_remove::<ShouldBePlaying>();
    if !av_player.source.src.is_empty()
        && maybe_video_sink
            .as_ref()
            .filter(|video_sink| av_player.source.src == video_sink.source)
            .is_some()
    {
        debug!("Updating sinks of {entity}.");
        if let Some(video_sink) = maybe_video_sink {
            video_sink
                .command_sender
                .send(AVCommand::Repeat(av_player.source.r#loop.unwrap_or(false)))
                .report();
            video_sink.command_sender.send(AVCommand::Pause).report();
        }
        if let Some(audio_sink) = maybe_audio_sink {
            commands.trigger_targets(
                ChangeAudioSinkVolume {
                    volume: av_player.source.volume.unwrap_or(1.),
                },
                entity,
            );
            audio_sink.command_sender.send(AVCommand::Pause).report();
        }
    } else {
        if maybe_audio_sink.is_some() || maybe_video_sink.is_some() {
            debug!("Removing sinks of {entity} due to diverging source.");
        }
        if let Some(video_sink) = maybe_video_sink {
            video_sink.command_sender.send(AVCommand::Dispose).report();
        }
        if let Some(audio_sink) = maybe_audio_sink {
            audio_sink.command_sender.send(AVCommand::Dispose).report();
        }
        debug!("{entity:?} has {}.", av_player.source.src);
        commands
            .entity(trigger.target())
            .try_remove::<(AudioSink, VideoSink)>();
    }
}

fn av_player_on_remove(trigger: Trigger<OnRemove, AVPlayer>, mut commands: Commands) {
    let entity = trigger.target();
    commands.entity(entity).try_remove::<(
        InScene,
        ShouldBePlaying,
        AudioSink,
        VideoSink,
        VideoTextureOutput,
    )>();
    #[cfg(feature = "livekit")]
    commands
        .entity(entity)
        .try_remove::<(StreamViewer, StreamImage)>();
}

fn av_player_should_be_playing_on_add(
    trigger: Trigger<OnAdd, ShouldBePlaying>,
    mut commands: Commands,
    av_players: Query<(Option<&AudioSink>, Option<&VideoSink>)>,
) {
    let entity = trigger.target();
    let Ok((maybe_audio_sink, maybe_video_sink)) = av_players.get(entity) else {
        error!("ShouldBePlaying added to something that is not an AVPlayer.");
        commands.send_event(AppExit::from_code(1));
        return;
    };

    if let Some(audio_sink) = maybe_audio_sink {
        audio_sink.command_sender.send(AVCommand::Play).report();
    }
    if let Some(video_sink) = maybe_video_sink {
        video_sink.command_sender.send(AVCommand::Play).report();
    }
}

fn av_player_should_be_playing_on_remove(
    trigger: Trigger<OnRemove, ShouldBePlaying>,
    mut commands: Commands,
    av_players: Query<(Option<&AudioSink>, Option<&VideoSink>)>,
) {
    let entity = trigger.target();
    let Ok((maybe_audio_sink, maybe_video_sink)) = av_players.get(entity) else {
        error!("ShouldBePlaying added to something that is not an AVPlayer.");
        commands.send_event(AppExit::from_code(1));
        return;
    };

    if let Some(audio_sink) = maybe_audio_sink {
        audio_sink.command_sender.send(AVCommand::Pause).report();
    }
    if let Some(video_sink) = maybe_video_sink {
        video_sink.command_sender.send(AVCommand::Pause).report();
    }
}

fn play_videos(
    mut images: ResMut<Assets<Image>>,
    mut q: Query<(&mut VideoSink, &ContainerEntity, &mut VideoTextureOutput)>,
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

    for (mut sink, container, mut output) in q.iter_mut() {
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
type RebuildSinkFilter = (Without<AudioSink>, Without<VideoSink>);
#[cfg(feature = "livekit")]
type RebuildSinkFilter = (
    Without<AudioSink>,
    Without<VideoSink>,
    Without<StreamViewer>,
);

fn rebuild_sinks(
    mut commands: Commands,
    video_players: Populated<
        (
            Entity,
            &ContainerEntity,
            &AVPlayer,
            Option<&VideoTextureOutput>,
        ),
        RebuildSinkFilter,
    >,
    scenes: Query<&RendererSceneContext>,
    ipfs: Res<IpfsResource>,
    mut images: ResMut<Assets<Image>>,
) {
    for (ent, container, player, maybe_texture) in video_players.iter() {
        trace!("Rebuilding sinks for {}.", ent);
        let image_handle = match maybe_texture {
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

        let (video_sink, audio_sink) = if player.source.src.starts_with("livekit-video://") {
            // Done in observers
            continue;
        } else if player.source.src.is_empty() {
            let (video_sink, audio_sink) = noop_sinks(
                player.source.src.clone(),
                image_handle,
                player.source.volume.unwrap_or(1.0),
            );
            debug!(
                "spawned noop sink for scene @ {} (playing={})",
                context.base,
                player.source.playing.unwrap_or(true)
            );
            (video_sink, audio_sink)
        } else {
            let (video_sink, audio_sink) = av_sinks(
                ipfs.clone(),
                player.source.src.clone(),
                context.hash.clone(),
                image_handle,
                player.source.volume.unwrap_or(1.0),
                false,
                player.source.r#loop.unwrap_or(false),
            );
            debug!(
                "spawned av thread for scene @ {} (playing={})",
                context.base,
                player.source.playing.unwrap_or(true)
            );
            (video_sink, audio_sink)
        };
        let video_output = VideoTextureOutput(video_sink.image.clone());
        commands
            .entity(ent)
            .try_insert((video_sink, video_output, audio_sink));
    }
}

#[cfg(feature = "livekit")]
fn copy_stream_image(
    trigger: Trigger<OnAdd, StreamImage>,
    mut commands: Commands,
    stream_viewers: Query<&StreamImage, With<StreamViewer>>,
) {
    let entity = trigger.target();
    let Ok(stream_image) = stream_viewers.get(entity) else {
        // StreamImage added to something that is not a StreamViewer
        return;
    };
    commands
        .entity(entity)
        .insert(VideoTextureOutput((**stream_image).clone()));
}
