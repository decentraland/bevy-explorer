use bevy::{
    color::palettes::basic,
    diagnostic::FrameCount,
    math::FloatOrd,
    platform::collections::HashMap,
    prelude::*,
    render::{
        render_asset::RenderAssetUsages,
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
    },
};
use common::{
    sets::SceneSets,
    structs::{AppConfig, PrimaryUser},
};
#[cfg(feature = "livekit")]
use comms::{livekit::LivekitTransport, SceneRoom, Transport};
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::sdk::components::{PbAudioStream, PbVideoEvent, PbVideoPlayer, VideoState},
    SceneComponentId,
};
use ipfs::IpfsResource;
use scene_runner::{
    renderer_context::RendererSceneContext,
    update_world::{material::VideoTextureOutput, AddCrdtInterfaceExt},
    ContainerEntity, ContainingScene,
};

#[cfg(feature = "livekit")]
use crate::video_stream::streamer_sinks;
use crate::{
    stream_processor::AVCommand,
    video_context::{VideoData, VideoInfo},
    video_stream::{av_sinks, noop_sinks, VideoSink},
};

pub struct VideoPlayerPlugin;

impl Plugin for VideoPlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbVideoPlayer, AVPlayer>(
            SceneComponentId::VIDEO_PLAYER,
            ComponentPosition::EntityOnly,
        );
        app.add_crdt_lww_component::<PbAudioStream, AVPlayer>(
            SceneComponentId::AUDIO_STREAM,
            ComponentPosition::EntityOnly,
        );
        app.add_systems(Startup, init_ffmpeg);
        app.add_systems(Update, play_videos);
        app.add_systems(Update, update_video_players.in_set(SceneSets::PostLoop));
    }
}

#[derive(Component, Debug)]
pub struct AVPlayer {
    // note we reuse PbVideoPlayer for audio as well
    pub source: PbVideoPlayer,
}

impl From<PbVideoPlayer> for AVPlayer {
    fn from(value: PbVideoPlayer) -> Self {
        Self { source: value }
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
        }
    }
}

fn init_ffmpeg() {
    ffmpeg_next::init().unwrap();
    ffmpeg_next::log::set_level(ffmpeg_next::log::Level::Error);
}

fn play_videos(
    mut images: ResMut<Assets<Image>>,
    mut q: Query<(&mut VideoSink, &ContainerEntity)>,
    mut scenes: Query<&mut RendererSceneContext>,
    frame: Res<FrameCount>,
) {
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
                    last_frame_received = Some(frame.data(0).to_vec());
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

                    last_frame_received = Some(frame.rgba_data());
                    sink.current_time = frame.timestamp() as f64;
                }
                Ok(VideoData::State(state)) => new_state = Some(state),
                Err(_) => break,
            }
        }

        if let Some(frame) = last_frame_received {
            debug!("set frame on {:?}", sink.image);
            images
                .get_mut(&sink.image)
                .unwrap()
                .data
                .as_mut()
                .unwrap()
                .copy_from_slice(frame.as_slice());
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
                debug!("send current time = {}", sink.current_time);
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

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn update_video_players(
    mut commands: Commands,
    video_players: Query<(
        Entity,
        &ContainerEntity,
        Ref<AVPlayer>,
        Option<&VideoSink>,
        Option<&VideoTextureOutput>,
        &GlobalTransform,
    )>,
    mut images: ResMut<Assets<Image>>,
    ipfs: Res<IpfsResource>,
    scenes: Query<&RendererSceneContext>,
    #[cfg(feature = "livekit")] mut scene_rooms: Query<
        &mut Transport,
        (With<LivekitTransport>, With<SceneRoom>),
    >,
    config: Res<AppConfig>,
    mut system_paused: Local<HashMap<Entity, Option<tokio::sync::mpsc::Sender<AVCommand>>>>,
    containing_scene: ContainingScene,
    user: Query<&GlobalTransform, With<PrimaryUser>>,
) {
    let mut previously_stopped = std::mem::take(&mut *system_paused);

    for (ent, container, player, maybe_sink, maybe_texture, _) in video_players.iter() {
        if maybe_sink.map(|sink| &sink.source) != Some(&player.source.src) {
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
            previously_stopped.insert(ent, Some(video_sink.command_sender.clone()));
            let video_output = VideoTextureOutput(video_sink.image.clone());
            commands
                .entity(ent)
                .try_insert((video_sink, video_output, audio_sink));
            debug!("{ent:?} has {}", player.source.src);
        } else if player.is_changed() {
            let sink = maybe_sink.as_ref().unwrap();
            if player.source.playing.unwrap_or(true) {
                debug!("scene requesting start for {ent:?}");
                previously_stopped.insert(ent, None);
            } else {
                debug!("scene stopping {ent:?}");
                let _ = sink.command_sender.try_send(AVCommand::Pause);
            }
            let _ = sink
                .command_sender
                .try_send(AVCommand::Repeat(player.source.r#loop.unwrap_or(false)));
        }
    }

    // disable distant av
    let Ok(user) = user.single() else {
        return;
    };

    let containing_scenes = containing_scene.get_position(user.translation());

    let mut sorted_players = video_players
        .iter()
        .filter_map(|(ent, container, player, _, _, transform)| {
            if player.source.playing.unwrap_or(true) {
                let in_scene = containing_scenes.contains(&container.root);
                let distance = transform.translation().distance(user.translation());
                Some((in_scene, distance, ent))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    // prioritise av in current scene (false < true), then by distance
    sorted_players.sort_by_key(|(in_scene, distance, _)| (!in_scene, FloatOrd(*distance)));

    let should_be_playing = sorted_players
        .iter()
        .take(config.max_videos)
        .map(|(_, _, ent)| *ent);
    let should_be_stopped = sorted_players
        .iter()
        .skip(config.max_videos)
        .map(|(_, _, ent)| *ent);

    for ent in should_be_playing {
        if let Some(maybe_new_sender) = previously_stopped.get(&ent) {
            let sender = maybe_new_sender
                .as_ref()
                .unwrap_or_else(|| &video_players.get(ent).unwrap().3.unwrap().command_sender);
            debug!("starting {ent:?}");
            let _ = sender.try_send(AVCommand::Play);
        }
    }

    for ent in should_be_stopped {
        if !previously_stopped.contains_key(&ent) {
            if let Some(sink) = video_players.get(ent).unwrap().3 {
                info!("system stopping {ent:?}");
                let _ = sink.command_sender.try_send(AVCommand::Pause);
            }
        }
        system_paused.insert(ent, None);
    }
}
