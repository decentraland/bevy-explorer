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
use common::{debug_panic, util::ReportErr};
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::sdk::components::{PbVideoEvent, VideoState},
    SceneComponentId,
};
use ipfs::IpfsResource;
use livestream_manager::ReceiverVolume;
use media::{AVCommand, VideoData, VideoInfo};
use scene_runner::{
    renderer_context::RendererSceneContext,
    update_world::material::{update_materials, VideoTextureOutput},
    ContainerEntity,
};

use crate::{
    video_stream::{av_sinks, noop_sinks},
    AVPlayer, AVPlayerConfig, AVPlayerSinks, AVSinks, AudioStream, ShouldBePlaying, Stream,
    VideoPlayer, LIVEKIT_VIDEO_STREAM,
};

pub struct VideoPlayerPlugin;

impl Plugin for VideoPlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, play_videos.before(update_materials));

        app.add_observer(new_player_source::<AudioStream>);
        app.add_observer(new_player_source::<VideoPlayer>);
        app.add_observer(player_source_replaced::<AudioStream>);
        app.add_observer(player_source_replaced::<VideoPlayer>);
        app.add_observer(player_config_added::<AudioStream>);
        app.add_observer(player_config_added::<VideoPlayer>);
        app.add_observer(player_position_added::<AudioStream>);
        app.add_observer(player_position_added::<VideoPlayer>);
        app.add_observer(av_sinks_on_remove::<AudioStream>);
        app.add_observer(av_sinks_on_remove::<VideoPlayer>);
        app.add_observer(av_player_should_be_playing_on_add::<AudioStream>);
        app.add_observer(av_player_should_be_playing_on_add::<VideoPlayer>);
        app.add_observer(av_player_should_be_playing_on_remove::<AudioStream>);
        app.add_observer(av_player_should_be_playing_on_remove::<VideoPlayer>);
    }
}

#[expect(clippy::type_complexity)]
fn new_player_source<T: AVPlayer>(
    trigger: Trigger<OnInsert, T::Source>,
    mut commands: Commands,
    av_players: Query<(
        &T::Source,
        &ContainerEntity,
        Option<&VideoTextureOutput>,
        Option<&AVSinks<T>>,
        Has<Stream>,
    )>,
    scenes: Query<&RendererSceneContext>,
    mut images: ResMut<Assets<Image>>,
    ipfs: Res<IpfsResource>,
) {
    let entity = trigger.target();

    let Ok((source, container_entity, mut maybe_video_texture_output, maybe_sinks, has_stream)) =
        av_players.get(entity)
    else {
        unreachable!("Infallible query");
    };
    let Ok(context) = scenes.get(container_entity.root) else {
        debug_panic!(
            "{} has an invalid link to RendererSceneContext",
            disqualified::ShortName::of::<T::Source>()
        );
    };

    if let Some(sinks) = maybe_sinks {
        if let Some(audio_sink) = &sinks.audio {
            audio_sink.command_sender.send(AVCommand::Dispose).report();
        }
        if let Some(video_sink) = &sinks.video {
            video_sink.command_sender.send(AVCommand::Dispose).report();
        }
    }

    let livestream = &**source == LIVEKIT_VIDEO_STREAM;
    if T::ALLOWS_LIVESTREAM && livestream != has_stream {
        if livestream {
            debug!(
                "{} {} now a stream.",
                disqualified::ShortName::of::<T>(),
                entity
            );
            commands.entity(entity).try_insert(Stream);
        } else {
            debug!(
                "{} {} no longer a stream.",
                disqualified::ShortName::of::<T>(),
                entity
            );
            commands.entity(entity).remove::<Stream>();
            let _ = maybe_video_texture_output.take();
        }
    }

    let mut create_image_handle = || match maybe_video_texture_output {
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
        Some(texture) => {
            debug!("Reusing VideoTextureOutput.");
            texture.0.clone()
        }
    };

    let (video_sink, audio_sink) = match &(**source) {
        "" => noop_sinks((**source).to_owned(), create_image_handle(), 1.),
        LIVEKIT_VIDEO_STREAM if T::ALLOWS_LIVESTREAM => return,
        other => av_sinks(
            (*ipfs).clone(),
            other.to_owned(),
            context.hash.clone(),
            create_image_handle(),
            1.,
            true,
            false,
        ),
    };

    debug!(
        "Creating new sinks for {} targeting \"{}\"",
        entity,
        &(**source)
    );
    let video_output = VideoTextureOutput(video_sink.image.clone());
    commands.entity(entity).try_insert((
        video_output,
        T::build_sink_component(audio_sink, video_sink),
    ));
}

fn player_source_replaced<T: AVPlayer>(
    trigger: Trigger<OnReplace, T::Source>,
    mut commands: Commands,
    av_players: Query<Option<&AVSinks<T>>, With<T::Source>>,
) {
    let entity = trigger.target();
    let Ok(maybe_sinks) = av_players.get(entity) else {
        unreachable!("Infallible query");
    };

    debug!(
        "{}'s {} was replaced.",
        entity,
        disqualified::ShortName::of::<T::Source>(),
    );
    commands.entity(entity).try_remove::<AVSinks<T>>();

    if let Some(sinks) = maybe_sinks {
        if let Some(audio_sink) = &sinks.audio {
            audio_sink.command_sender.send(AVCommand::Dispose).report();
        }
        if let Some(video_sink) = &sinks.video {
            video_sink.command_sender.send(AVCommand::Dispose).report();
        }
    }
}

#[expect(clippy::type_complexity)]
fn player_config_added<T: AVPlayer>(
    trigger: Trigger<OnInsert, T::Config>,
    mut av_players: Query<(
        &T::Config,
        Option<&mut AVSinks<T>>,
        Has<ShouldBePlaying<T>>,
        Has<Stream>,
        Option<&mut ReceiverVolume>,
    )>,
) {
    let entity = trigger.target();
    let Ok((config, maybe_sinks, has_should_be_playing, has_stream, maybe_receiver_volume)) =
        av_players.get_mut(entity)
    else {
        unreachable!("Infallible query");
    };
    let Some(mut sinks) = maybe_sinks else {
        if !has_stream {
            debug_panic!(
                "Non-stream {} did not have sinks.",
                disqualified::ShortName::of::<T::Source>()
            );
        }
        if let Some(mut receiver_volume) = maybe_receiver_volume {
            debug!("Updated volume of stream.");
            **receiver_volume = config.volume();
        }
        return;
    };

    if let Some(audio_sink) = &mut sinks.audio {
        audio_sink.volume = config.volume();
        if config.playing() && has_should_be_playing {
            audio_sink.command_sender.send(AVCommand::Play).report();
        } else {
            audio_sink.command_sender.send(AVCommand::Pause).report();
        }
        audio_sink
            .command_sender
            .send(AVCommand::Repeat(config.r#loop()))
            .report();
    }
    if let Some(video_sink) = &mut sinks.video {
        if config.playing() && has_should_be_playing {
            video_sink.command_sender.send(AVCommand::Play).report();
        } else {
            video_sink.command_sender.send(AVCommand::Pause).report();
        }
        video_sink
            .command_sender
            .send(AVCommand::Repeat(config.r#loop()))
            .report();
        video_sink.rate = Some(config.playback_rate() as f64);
    }
}

#[expect(clippy::type_complexity)]
fn player_position_added<T: AVPlayer>(
    trigger: Trigger<OnInsert, T::Position>,
    mut av_players: Query<(&T::Position, Option<&mut AVSinks<T>>, Has<Stream>)>,
) {
    let entity = trigger.target();
    let Ok((position, maybe_sinks, has_stream)) = av_players.get_mut(entity) else {
        unreachable!("Infallible query");
    };
    let Some(mut sinks) = maybe_sinks else {
        if !has_stream {
            debug_panic!(
                "Non-stream {} did not have sinks.",
                disqualified::ShortName::of::<T::Source>()
            );
        }
        return;
    };

    debug!(
        "Seeking {} to {}",
        disqualified::ShortName::of::<T::Source>(),
        (**position)
    );
    if let Some(audio_sink) = &mut sinks.audio {
        audio_sink
            .command_sender
            .send(AVCommand::Seek((**position) as f64))
            .report();
    }
    if let Some(video_sink) = &mut sinks.video {
        video_sink
            .command_sender
            .send(AVCommand::Seek((**position) as f64))
            .report();
    }
}

fn av_sinks_on_remove<T: AVPlayer>(trigger: Trigger<OnRemove, AVSinks<T>>, mut commands: Commands) {
    let entity = trigger.target();
    commands.entity(entity).try_remove::<VideoTextureOutput>();
}

fn av_player_should_be_playing_on_add<T: AVPlayer>(
    trigger: Trigger<OnAdd, ShouldBePlaying<T>>,
    av_players: Query<&AVSinks<T>, With<T>>,
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
    trigger: Trigger<OnRemove, ShouldBePlaying<T>>,
    av_players: Query<&AVSinks<T>, With<T>>,
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
        &mut AVSinks<VideoPlayer>,
        &ContainerEntity,
        &mut VideoTextureOutput,
    )>,
    mut scenes: Query<&mut RendererSceneContext>,
    frame: Res<FrameCount>,
) {
    enum FrameSource {
        Video(media::Video),
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
