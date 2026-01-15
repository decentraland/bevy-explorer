use std::borrow::Cow;

use bevy::{
    asset::RenderAssetTransferPriority, color::palettes::basic, diagnostic::FrameCount, prelude::*, render::{
        render_asset::RenderAssetUsages,
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
    }
};
use common::sets::SceneSets;
use comms::livekit_native::LivekitVideoFrame;
#[cfg(feature = "livekit")]
use comms::{livekit_room::LivekitTransport, SceneRoom, Transport};
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::sdk::components::{PbVideoEvent, VideoState},
    SceneComponentId,
};
use ipfs::IpfsResource;
use scene_runner::{
    renderer_context::RendererSceneContext, update_world::material::VideoTextureOutput,
    ContainerEntity,
};

#[cfg(feature = "livekit")]
use crate::video_stream::streamer_sinks;
use crate::{
    audio_sink::{AudioSink, ChangeAudioSinkVolume},
    av_player_is_in_scene,
    stream_processor::AVCommand,
    video_context::{VideoData, VideoInfo},
    video_stream::{av_sinks, noop_sinks, VideoSink},
    AVPlayer, ShouldBePlaying,
};

pub struct VideoPlayerPlugin;

impl Plugin for VideoPlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init_ffmpeg);
        app.add_systems(Update, play_videos);
        app.add_systems(
            Update,
            rebuild_sinks
                .before(av_player_is_in_scene)
                .in_set(SceneSets::PostLoop),
        );

        app.add_observer(av_player_on_insert);
        app.add_observer(av_player_should_be_playing_on_add);
        app.add_observer(av_player_should_be_playing_on_remove);
    }
}

fn init_ffmpeg() {
    ffmpeg_next::init().unwrap();
    ffmpeg_next::log::set_level(ffmpeg_next::log::Level::Error);
}

fn av_player_on_insert(
    trigger: Trigger<OnInsert, AVPlayer>,
    mut commands: Commands,
    mut av_players: Query<(&AVPlayer, Option<&AudioSink>, Option<&VideoSink>)>,
) {
    let entity = trigger.target();
    let Ok((av_player, maybe_audio_sink, maybe_video_sink)) = av_players.get_mut(entity) else {
        return;
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
            let _ = video_sink
                .command_sender
                .try_send(AVCommand::Repeat(av_player.source.r#loop.unwrap_or(false)));

            if av_player.source.playing.unwrap_or(true) {
                debug!("scene requesting start of video for {entity}");
                let _ = video_sink.command_sender.try_send(AVCommand::Play);
            } else {
                debug!("scene stopping video {entity}");
                let _ = video_sink.command_sender.try_send(AVCommand::Pause);
            }
        }
        if let Some(audio_sink) = maybe_audio_sink {
            commands.trigger_targets(
                ChangeAudioSinkVolume {
                    volume: av_player.source.volume.unwrap_or(1.),
                },
                entity,
            );

            if av_player.source.playing.unwrap_or(true) {
                debug!("scene requesting start of audio for {entity}");
                let _ = audio_sink.command_sender.try_send(AVCommand::Play);
            } else {
                debug!("scene stopping audio {entity}");
                let _ = audio_sink.command_sender.try_send(AVCommand::Pause);
            }
        }
    } else {
        if maybe_audio_sink.is_some() || maybe_video_sink.is_some() {
            debug!("Removing sinks of {entity} due to diverging source.");
        }
        if let Some(video_sink) = maybe_video_sink {
            let _ = video_sink.command_sender.try_send(AVCommand::Dispose);
        }
        if let Some(audio_sink) = maybe_audio_sink {
            let _ = audio_sink.command_sender.try_send(AVCommand::Dispose);
        }
        commands
            .entity(trigger.target())
            .try_remove::<(AudioSink, VideoSink)>();
    }
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
        let _ = audio_sink.command_sender.try_send(AVCommand::Play);
    }
    if let Some(video_sink) = maybe_video_sink {
        let _ = video_sink.command_sender.try_send(AVCommand::Play);
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
        let _ = audio_sink.command_sender.try_send(AVCommand::Pause);
    }
    if let Some(video_sink) = maybe_video_sink {
        let _ = video_sink.command_sender.try_send(AVCommand::Pause);
    }
}

fn play_videos(
    mut images: ResMut<Assets<Image>>,
    mut q: Query<(&mut VideoSink, &ContainerEntity)>,
    mut scenes: Query<&mut RendererSceneContext>,
    frame: Res<FrameCount>,
) {
    enum FrameSource {
        Video(ffmpeg_next::frame::Video),
        #[cfg(feature = "livekit")]
        Livekit(LivekitVideoFrame),
    }

    impl FrameSource {
        fn data(&self) -> Cow<'_, [u8]> {
            match self {
                FrameSource::Video(video) => Cow::Borrowed(video.data(0)),
                #[cfg(feature = "livekit")]
                FrameSource::Livekit(livekit_video_frame) => {
                    Cow::Owned(livekit_video_frame.rgba_data())
                }
            }
        }
    }

    for (mut sink, container) in q.iter_mut() {
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
                    images.get_mut(&sink.image).unwrap().resize(Extent3d {
                        width: width.max(16),
                        height: height.max(16),
                        depth_or_array_layers: 1,
                    });
                    sink.length = Some(length);
                    sink.rate = Some(rate);
                }
                Ok(VideoData::Frame(frame, time)) => {
                    last_frame_received = Some(FrameSource::Video(frame));
                    sink.current_time = time;
                }
                #[cfg(feature = "livekit")]
                Ok(VideoData::LivekitFrame(frame)) => {
                    let image = images.get_mut(&sink.image).unwrap();
                    let extent = image.size();
                    let width = frame.width();
                    let height = frame.height();
                    if extent.x != width || extent.y != height {
                        debug!("resize {width} {height}");
                        image.resize(Extent3d {
                            width: width.max(16),
                            height: height.max(16),
                            depth_or_array_layers: 1,
                        });
                    }

                    sink.current_time = frame.timestamp() as f64;
                    last_frame_received = Some(FrameSource::Livekit(frame));
                }
                Ok(VideoData::State(state)) => new_state = Some(state),
                Err(_) => break,
            }
        }

        if let Some(frame) = last_frame_received {
            trace!("set frame on {:?}", sink.image);
            images
                .get_mut(&sink.image)
                .unwrap()
                .data
                .as_mut()
                .unwrap()
                .copy_from_slice(&frame.data());
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

#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn rebuild_sinks(
    mut commands: Commands,
    video_players: Query<
        (
            Entity,
            &ContainerEntity,
            &AVPlayer,
            Option<&VideoTextureOutput>,
            &GlobalTransform,
        ),
        (Without<AudioSink>, Without<VideoSink>),
    >,
    #[cfg(feature = "livekit")] mut scene_rooms: Query<
        &mut Transport,
        (With<LivekitTransport>, With<SceneRoom>),
    >,
    scenes: Query<&RendererSceneContext>,
    ipfs: Res<IpfsResource>,
    mut images: ResMut<Assets<Image>>,
) {
    for (ent, container, player, maybe_texture, _) in video_players.iter() {
        debug!("Rebuilding sinks for {}.", ent);
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
                image.transfer_priority = RenderAssetTransferPriority::Priority(-2);
                images.add(image)
            }
            Some(texture) => texture.0.clone(),
        };

        let Ok(context) = scenes.get(container.root) else {
            continue;
        };

        let (video_sink, audio_sink) = if player.source.src.starts_with("livekit-video://") {
            #[cfg(feature = "livekit")]
            if let Ok(transport) = scene_rooms.single_mut() {
                if let Some(control_channel) = transport.control.clone() {
                    let (video_sink, audio_sink) = streamer_sinks(
                        control_channel,
                        player.source.src.clone(),
                        image_handle,
                        player.source.volume.unwrap_or(1.0),
                    );
                    debug!(
                        "spawned streamer thread for scene @ {} (playing={})",
                        context.base,
                        player.source.playing.unwrap_or(true)
                    );
                    (video_sink, audio_sink)
                } else {
                    error!("Transport did not have ChannelControl channel.");
                    noop_sinks(
                        player.source.src.clone(),
                        image_handle,
                        player.source.volume.unwrap_or(1.0),
                    )
                }
            } else {
                error!("Could not determinate the scene of the AvPlayer.");
                noop_sinks(
                    player.source.src.clone(),
                    image_handle,
                    player.source.volume.unwrap_or(1.0),
                )
            }
            #[cfg(not(feature = "livekit"))]
            noop_sinks(
                player.source.src.clone(),
                image_handle,
                player.source.volume.unwrap_or(1.0),
            )
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
        debug!("{ent:?} has {}", player.source.src);
    }
}
