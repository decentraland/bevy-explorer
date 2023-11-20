use crate::{
    stream_processor::AVCommand,
    video_context::{VideoData, VideoInfo},
    video_stream::{av_sinks, VideoSink},
};
use bevy::{
    core::FrameCount,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
};
use common::sets::SceneSets;
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::sdk::components::{PbAudioStream, PbVideoEvent, PbVideoPlayer},
    SceneComponentId,
};
use ipfs::IpfsResource;
use scene_runner::{
    renderer_context::RendererSceneContext,
    update_world::{material::VideoTextureOutput, AddCrdtInterfaceExt},
    ContainerEntity,
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
                        width,
                        height,
                        depth_or_array_layers: 1,
                    });
                    sink.length = Some(length);
                    sink.rate = Some(rate);
                }
                Ok(VideoData::Frame(frame, time)) => {
                    last_frame_received = Some(frame);
                    sink.current_time = time;
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
                .copy_from_slice(frame.data(0));
        }

        if let Some(state) = new_state {
            if let Ok(mut context) = scenes.get_mut(container.root) {
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
            }
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn update_video_players(
    mut commands: Commands,
    video_players: Query<
        (
            Entity,
            &ContainerEntity,
            &AVPlayer,
            Option<&VideoSink>,
            Option<&VideoTextureOutput>,
        ),
        Changed<AVPlayer>,
    >,
    mut images: ResMut<Assets<Image>>,
    ipfs: Res<IpfsResource>,
    scenes: Query<&RendererSceneContext>,
) {
    for (ent, container, player, maybe_sink, maybe_texture) in video_players.iter() {
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
                        &Color::PINK.as_rgba_u32().to_le_bytes(),
                        TextureFormat::Rgba8UnormSrgb,
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

            let (video_sink, audio_sink) = av_sinks(
                ipfs.clone(),
                player.source.src.clone(),
                context.hash.clone(),
                image_handle,
                player.source.volume.unwrap_or(1.0),
                player.source.playing.unwrap_or(true),
                player.source.r#loop.unwrap_or(false),
            );
            let video_output = VideoTextureOutput(video_sink.image.clone());
            commands
                .entity(ent)
                .try_insert((video_sink, video_output, audio_sink));
            debug!("{ent:?} has {}", player.source.src);
        } else {
            let sink = maybe_sink.as_ref().unwrap();
            if player.source.playing.unwrap_or(true) {
                let _ = sink.command_sender.try_send(AVCommand::Play);
            } else {
                let _ = sink.command_sender.try_send(AVCommand::Pause);
            }
            let _ = sink
                .command_sender
                .try_send(AVCommand::Repeat(player.source.r#loop.unwrap_or(false)));
        }
    }
}
