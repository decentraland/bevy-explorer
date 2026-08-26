use std::{marker::PhantomData, sync::atomic::Ordering};

use bevy::{
    asset::RenderAssetTransferPriority,
    color::palettes::basic,
    diagnostic::FrameCount,
    prelude::*,
    render::{
        render_asset::RenderAssetUsages,
        render_resource::{TextureDimension, TextureFormat, TextureUsages},
        renderer::WgpuWrapper,
    },
};
use common::{debug_panic, sets::SceneSets, structs::AudioSettings, util::ReportErr};
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::sdk::components::{PbAudioEvent, PbVideoEvent, VideoState},
    SceneComponentId,
};
use ipfs::IpfsResource;
use livestream_manager::ReceiverVolume;
use media::{FrameCopyRequest, FrameCopyRequestQueue, HtmlMedia};
use scene_runner::{
    renderer_context::RendererSceneContext,
    update_world::material::{update_materials, VideoTextureOutput},
    ContainerEntity,
};
use web_sys::VideoFrame;

use crate::{
    audio_stream_should_be_playing, video_player_should_be_playing, AVPlayer, AVPlayerConfig,
    AudioStream, ShouldBePlaying, Stream, VideoPlayer, LIVEKIT_VIDEO_STREAM,
};

pub struct VideoPlayerPlugin;

impl Plugin for VideoPlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            ((
                update_av_players::<AudioStream>.after(audio_stream_should_be_playing),
                update_av_players::<VideoPlayer>.after(video_player_should_be_playing),
            )
                .before(update_materials),)
                .chain()
                .in_set(SceneSets::PostLoop),
        );
        app.add_systems(
            Update,
            (
                update_html_video_player_volumes::<AudioStream>,
                update_html_video_player_volumes::<VideoPlayer>,
            )
                .run_if(resource_exists_and_changed::<AudioSettings>),
        );

        app.add_observer(new_player_source::<AudioStream>);
        app.add_observer(new_player_source::<VideoPlayer>);
        app.add_observer(player_source_replaced::<AudioStream>);
        app.add_observer(player_source_replaced::<VideoPlayer>);
        app.add_observer(player_config_added::<AudioStream>);
        app.add_observer(player_config_added::<VideoPlayer>);
        app.add_observer(player_position_added::<AudioStream>);
        app.add_observer(player_position_added::<VideoPlayer>);
        app.add_observer(html_media_entity_on_remove::<AudioStream>);
        app.add_observer(html_media_entity_on_remove::<VideoPlayer>);
        app.add_observer(av_player_should_be_playing_on_add::<AudioStream>);
        app.add_observer(av_player_should_be_playing_on_add::<VideoPlayer>);
        app.add_observer(av_player_should_be_playing_on_remove::<AudioStream>);
        app.add_observer(av_player_should_be_playing_on_remove::<VideoPlayer>);
    }
}

#[derive(Component, Deref, DerefMut)]
pub struct HtmlMediaEntity<T: AVPlayer> {
    #[deref]
    media: HtmlMedia,
    _phantom: PhantomData<T>,
}

impl<T: AVPlayer> HtmlMediaEntity<T> {
    pub fn new_audio(url: &str, source: String) -> Self {
        Self {
            media: HtmlMedia::new_audio(url, source),
            _phantom: PhantomData,
        }
    }

    pub fn new_video(url: &str, source: String, image: Handle<Image>) -> Self {
        Self {
            media: HtmlMedia::new_video(url, source, image),
            _phantom: PhantomData,
        }
    }

    pub fn new_noop(source: String, image: Handle<Image>) -> Self {
        Self {
            media: HtmlMedia::new_noop(source, image),
            _phantom: PhantomData,
        }
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
        Has<Stream>,
    )>,
    scenes: Query<&RendererSceneContext>,
    mut images: ResMut<Assets<Image>>,
    ipfs: Res<IpfsResource>,
) {
    let entity = trigger.target();

    let Ok((player_source, container_entity, mut maybe_video_texture_output, has_stream)) =
        av_players.get(entity)
    else {
        unreachable!("Infallible query");
    };
    let Ok(context) = scenes.get(container_entity.root) else {
        debug_panic!("AVPlayer has an invalid link to RendererSceneContext");
    };

    let livestream = &**player_source == LIVEKIT_VIDEO_STREAM;
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

    match &(**player_source) {
        LIVEKIT_VIDEO_STREAM if T::ALLOWS_LIVESTREAM => (),
        _ => {
            debug!(
                "Creating new html media entity for {} targeting \"{}\"",
                entity,
                &(**player_source)
            );
            let source_url = &(**player_source);
            if source_url.is_empty() {
                let image = create_image_handle();
                let video_output = VideoTextureOutput(image.clone());
                commands.entity(entity).try_insert((
                    video_output,
                    HtmlMediaEntity::<T>::new_noop((**player_source).to_owned(), image.clone()),
                ));
            } else {
                let source = ipfs
                    .content_url(source_url, &context.hash)
                    .unwrap_or_else(|| source_url.to_owned());

                if T::has_video() {
                    let image = create_image_handle();
                    let video_output = VideoTextureOutput(image.clone());
                    commands.entity(entity).try_insert((
                        video_output,
                        HtmlMediaEntity::<T>::new_video(
                            &source,
                            source_url.to_owned(),
                            image.clone(),
                        ),
                    ));
                } else {
                    commands
                        .entity(entity)
                        .try_insert(HtmlMediaEntity::<T>::new_audio(
                            &source,
                            source_url.to_owned(),
                        ));
                };
            }
        }
    };
}

fn player_source_replaced<T: AVPlayer>(
    trigger: Trigger<OnReplace, T::Source>,
    mut commands: Commands,
) {
    let entity = trigger.target();

    debug!(
        "{}'s {} was replaced.",
        entity,
        disqualified::ShortName::of::<T::Source>(),
    );
    commands.entity(entity).try_remove::<HtmlMediaEntity<T>>();
}

#[expect(clippy::type_complexity)]
fn player_config_added<T: AVPlayer>(
    trigger: Trigger<OnInsert, T::Config>,
    mut av_players: Query<(
        &T::Config,
        Option<&mut HtmlMediaEntity<T>>,
        Has<ShouldBePlaying<T>>,
        Has<Stream>,
        Option<&mut ReceiverVolume>,
    )>,
    audio_settings: Res<AudioSettings>,
) {
    let entity = trigger.target();
    let Ok((
        config,
        maybe_html_media_entity,
        has_should_be_playing,
        has_stream,
        maybe_receiver_volume,
    )) = av_players.get_mut(entity)
    else {
        unreachable!("Infallible query");
    };
    if has_stream {
        if let Some(mut receiver_volume) = maybe_receiver_volume {
            debug!("Updated volume of stream.");
            **receiver_volume = config.volume();
        }
        if maybe_html_media_entity.is_none() {
            return;
        }
    }
    let Some(mut html_media_entity) = maybe_html_media_entity else {
        debug_panic!("Non-stream AVPlayer did not have html media entity.");
    };

    if config.playing() && has_should_be_playing {
        html_media_entity.play();
    } else {
        html_media_entity.stop();
    }
    html_media_entity.set_volume(config.volume() * audio_settings.scene());
    html_media_entity.set_loop(config.r#loop());
}

#[expect(clippy::type_complexity)]
fn player_position_added<T: AVPlayer>(
    trigger: Trigger<OnInsert, T::Position>,
    mut av_players: Query<(&T::Position, Option<&mut HtmlMediaEntity<T>>, Has<Stream>)>,
) {
    let entity = trigger.target();
    let Ok((position, maybe_html_media_entity, has_stream)) = av_players.get_mut(entity) else {
        unreachable!("Infallible query");
    };
    let Some(mut html_media_entity) = maybe_html_media_entity else {
        if !has_stream {
            debug_panic!("Non-stream AVPlayer did not have html media entity.");
        }
        return;
    };

    debug!("Seeking AVPlayer to {}", **position);
    html_media_entity.set_current_time(**position);
    html_media_entity.media.set_current_time(**position);
    if let Some(media) = &mut html_media_entity.video() {
        media.set_current_time((**position).into());
    }
}

fn html_media_entity_on_remove<T: AVPlayer>(
    trigger: Trigger<OnRemove, HtmlMediaEntity<T>>,
    mut commands: Commands,
) {
    let entity = trigger.target();
    debug!(
        "{} was removed.",
        disqualified::ShortName::of::<HtmlMediaEntity<T>>()
    );
    commands.entity(entity).try_remove::<VideoTextureOutput>();
}

fn av_player_should_be_playing_on_add<T: AVPlayer>(
    trigger: Trigger<OnAdd, ShouldBePlaying<T>>,
    mut av_players: Query<&mut HtmlMediaEntity<T>, With<T>>,
) {
    let entity = trigger.target();
    let Ok(mut html_media_entity) = av_players.get_mut(entity) else {
        return;
    };

    debug!(
        "{} now playing.",
        disqualified::ShortName::of::<HtmlMediaEntity<T>>()
    );
    html_media_entity.play();
}

fn av_player_should_be_playing_on_remove<T: AVPlayer>(
    trigger: Trigger<OnRemove, ShouldBePlaying<T>>,
    mut av_players: Query<&mut HtmlMediaEntity<T>, With<T>>,
) {
    let entity = trigger.target();
    let Ok(mut html_media_entity) = av_players.get_mut(entity) else {
        return;
    };

    debug!(
        "{} now paused.",
        disqualified::ShortName::of::<HtmlMediaEntity<T>>()
    );
    html_media_entity.stop();
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_av_players<T: AVPlayer>(
    mut commands: Commands,
    mut av_players: Query<
        (
            Entity,
            &ContainerEntity,
            &mut HtmlMediaEntity<T>,
            Has<ShouldBePlaying<T>>,
        ),
        With<T>,
    >,
    mut images: ResMut<Assets<Image>>,
    mut scenes: Query<&mut RendererSceneContext>,
    send_queue: Res<FrameCopyRequestQueue>,
    frame: Res<FrameCount>,
) {
    for (ent, container, mut av, has_should_be_playing) in av_players.iter_mut() {
        let state = av.state();

        let is_playing = state == VideoState::VsPlaying;

        if has_should_be_playing
            && (state == VideoState::VsLoading || state == VideoState::VsBuffering)
        {
            av.play();
        } else if !has_should_be_playing && is_playing {
            av.stop();
        }

        if is_playing {
            #[allow(clippy::collapsible_else_if)]
            if let Some(video) = av.video().as_ref() {
                let new_time = av.new_frame_time().swap(0, Ordering::Relaxed);
                if new_time != 0 {
                    // new frame is ready
                    let new_time = f32::from_bits(new_time);
                    trace!("got new frame -> {new_time}");

                    let Ok(frame) = VideoFrame::new_with_html_video_element(video) else {
                        warn!("failed to extract frame");
                        continue;
                    };

                    let image_id = av.image().as_ref().unwrap().id();
                    let visible_rect = frame.visible_rect().unwrap();
                    let video_size = (visible_rect.width() as u32, visible_rect.height() as u32);

                    // check size
                    if av.size().is_none_or(|sz| sz != video_size) {
                        let mut image = Image::new_fill(
                            bevy::render::render_resource::Extent3d {
                                width: video_size.0,
                                height: video_size.1,
                                depth_or_array_layers: 1,
                            },
                            TextureDimension::D2,
                            &basic::FUCHSIA.to_u8_array(),
                            TextureFormat::Rgba8UnormSrgb,
                            RenderAssetUsages::all(),
                        );
                        image.texture_descriptor.usage = TextureUsages::COPY_DST
                            | TextureUsages::TEXTURE_BINDING
                            | TextureUsages::RENDER_ATTACHMENT;
                        image.transfer_priority =
                            bevy::asset::RenderAssetTransferPriority::Immediate;
                        image.data = None;
                        let image = images.add(image);
                        av.set_size(Some(video_size));
                        commands
                            .entity(ent)
                            .try_insert(VideoTextureOutput(image.clone()));
                        av.set_image(Some(image));

                        trace!("queue resized frame {:?}", video_size);
                    }

                    // queue copy
                    trace!("queue frame {:?}", video_size);
                    send_queue
                        .send(FrameCopyRequest {
                            video_frame: WgpuWrapper::new(frame),
                            target: image_id,
                        })
                        .report();

                    av.set_current_time(new_time);
                } else {
                    trace!("no frame (new_time == 0)");
                }
            } else {
                debug!("no video");
                // we don't report audio timestamps, otherwise would need to grab it here
            }
        }

        const AV_REPORT_FREQUENCY: f32 = 1.0;
        let new_state = av.state();
        if new_state != av.last_state()
            || av.current_time() > av.last_reported_time() + AV_REPORT_FREQUENCY
            || av.current_time() < av.last_reported_time()
        {
            let Ok(mut context) = scenes.get_mut(container.root) else {
                continue;
            };
            let tick_number = context.tick_number;
            trace!("set {:?} {:?}", av.state(), av.current_time());

            if T::has_video() {
                context.update_crdt(
                    SceneComponentId::VIDEO_EVENT,
                    CrdtType::GO_ANY,
                    container.container_id,
                    &PbVideoEvent {
                        timestamp: frame.0,
                        tick_number,
                        current_offset: av.current_time(),
                        video_length: av.media.duration() as f32,
                        state: av.state() as i32,
                    },
                );
            } else {
                context.update_crdt(
                    SceneComponentId::AUDIO_EVENT,
                    CrdtType::GO_ANY,
                    container.container_id,
                    &PbAudioEvent {
                        timestamp: frame.0,
                        state: av.state() as i32, // a bit hacky - MediaState and VideoState have the same i32 representation
                    },
                )
            }
            av.set_last_state(new_state);
            let current_time = av.current_time();
            av.set_last_reported_time(current_time);
        }
    }
}

fn update_html_video_player_volumes<T: AVPlayer>(
    audio_settings: Res<AudioSettings>,
    html_video_players: Query<(&T::Config, &mut HtmlMediaEntity<T>)>,
) {
    let scene_volume = audio_settings.scene();
    for (av_player, html_video_player) in html_video_players {
        let volume = av_player.volume();
        html_video_player.set_volume(volume * scene_volume);
    }
}
