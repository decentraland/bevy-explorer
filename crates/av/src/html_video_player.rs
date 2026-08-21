use std::{marker::PhantomData, sync::atomic::Ordering};

use bevy::{
    color::palettes::basic,
    diagnostic::FrameCount,
    prelude::*,
    render::{
        render_asset::RenderAssetUsages,
        render_resource::{TextureDimension, TextureFormat, TextureUsages},
        renderer::WgpuWrapper,
    },
};
use common::{sets::SceneSets, structs::AudioSettings, util::ReportErr};
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::sdk::components::{PbAudioEvent, PbVideoEvent, VideoState},
    SceneComponentId,
};
use ipfs::IpfsResource;
use media::{FrameCopyRequest, FrameCopyRequestQueue, HtmlMedia};
use scene_runner::{
    renderer_context::RendererSceneContext,
    update_world::material::{update_materials, VideoTextureOutput},
    ContainerEntity,
};
use web_sys::{wasm_bindgen::JsCast, VideoFrame};
#[cfg(feature = "livekit")]
use {
    bevy::ecs::relationship::Relationship,
    comms::livekit::participant::{ChangeVolume, StreamViewer},
};

use crate::{
    audio_stream_should_be_playing, av_player_is_in_scene, video_player_should_be_playing,
    AVPlayer, AudioStream, InScene, ShouldBePlaying, VideoPlayer,
};

pub struct VideoPlayerPlugin;

const VIDEO_CONTAINER_ID: &str = "video-player-container";
const STREAM_CONTAINER_ID: &str = "stream-player-container";

impl Plugin for VideoPlayerPlugin {
    fn build(&self, app: &mut App) {
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if document.get_element_by_id(VIDEO_CONTAINER_ID).is_none() {
                    let container = document.create_element("div").unwrap();
                    container.set_id(VIDEO_CONTAINER_ID);
                    let style = container.dyn_ref::<web_sys::HtmlElement>().unwrap().style();
                    style.set_property("display", "none").unwrap();

                    document.body().unwrap().append_child(&container).unwrap();
                }
                if document.get_element_by_id(STREAM_CONTAINER_ID).is_none() {
                    let container = document.create_element("div").unwrap();
                    container.set_id(STREAM_CONTAINER_ID);
                    let style = container.dyn_ref::<web_sys::HtmlElement>().unwrap().style();
                    style.set_property("display", "none").unwrap();

                    document.body().unwrap().append_child(&container).unwrap();
                }
            }
        }

        app.add_systems(
            Update,
            (
                (
                    rebuild_html_media_entities::<AudioStream>
                        .before(av_player_is_in_scene::<AudioStream>),
                    rebuild_html_media_entities::<VideoPlayer>
                        .before(av_player_is_in_scene::<VideoPlayer>),
                ),
                (
                    update_av_players::<AudioStream>.after(audio_stream_should_be_playing),
                    update_av_players::<VideoPlayer>.after(video_player_should_be_playing),
                )
                    .before(update_materials),
            )
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

        app.add_observer(av_player_on_insert::<AudioStream>);
        app.add_observer(av_player_on_insert::<VideoPlayer>);
        app.add_observer(av_player_on_remove::<AudioStream>);
        app.add_observer(av_player_on_remove::<VideoPlayer>);
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

#[cfg(not(feature = "livekit"))]
type AVPlayerOnInsertQuery<'a, T> = (&'a T, &'a mut HtmlMediaEntity<T>);
#[cfg(feature = "livekit")]
type AVPlayerOnInsertQuery<'a, T> = (&'a T, Option<&'a StreamViewer>, &'a mut HtmlMediaEntity<T>);

fn av_player_on_insert<T: AVPlayer>(
    trigger: Trigger<OnInsert, T>,
    mut commands: Commands,
    mut av_players: Query<AVPlayerOnInsertQuery<T>>,
    audio_settings: Res<AudioSettings>,
) {
    info!("AVPlayer updated.");
    let entity = trigger.target();
    let Ok(query) = av_players.get_mut(entity) else {
        return;
    };
    #[cfg(not(feature = "livekit"))]
    let (av_player, mut html_media_entity) = query;
    #[cfg(feature = "livekit")]
    let (av_player, maybe_stream_viewer, mut html_media_entity) = query;

    let source_url = av_player.source();

    if source_url == html_media_entity.source() {
        debug!("Updating html media entity {entity}.");
        let av_player_volume = av_player.volume();
        if source_url.starts_with("livekit-video://") {
            html_media_entity.set_loop(av_player.r#loop());
            html_media_entity.set_volume(av_player_volume * audio_settings.scene());
            #[cfg(feature = "livekit")]
            if let Some(stream_viewer) = maybe_stream_viewer {
                commands.trigger_targets(ChangeVolume(av_player_volume), stream_viewer.get());
            }
        } else {
            // This forces an update on the entity
            commands.entity(entity).try_remove::<ShouldBePlaying<T>>();
            html_media_entity.stop();
            html_media_entity.set_loop(av_player.r#loop());
            html_media_entity.set_volume(av_player_volume * audio_settings.scene());
        }
    } else {
        debug!("Removing html media entity {entity} due to diverging source.");
        commands
            .entity(trigger.target())
            .try_remove::<(HtmlMediaEntity<T>, ShouldBePlaying<T>)>();
        #[cfg(feature = "livekit")]
        commands.entity(entity).try_remove::<StreamViewer>();
    }
}

fn av_player_on_remove<T: AVPlayer>(trigger: Trigger<OnRemove, T>, mut commands: Commands) {
    let entity = trigger.target();
    commands.entity(entity).try_remove::<(
        InScene,
        ShouldBePlaying<T>,
        HtmlMediaEntity<T>,
        VideoTextureOutput,
    )>();
    #[cfg(feature = "livekit")]
    commands.entity(entity).try_remove::<StreamViewer>();
}

#[expect(clippy::type_complexity)]
fn rebuild_html_media_entities<T: AVPlayer>(
    mut commands: Commands,
    av_players: Populated<
        (Entity, &ContainerEntity, &T, Option<&VideoTextureOutput>),
        Without<HtmlMediaEntity<T>>,
    >,
    scenes: Query<&RendererSceneContext>,
    ipfs: Res<IpfsResource>,
    mut images: ResMut<Assets<Image>>,
    audio_settings: Res<AudioSettings>,
) {
    let scene_volume = audio_settings.scene();
    for (ent, container, player, maybe_texture) in av_players.iter() {
        let Ok(context) = scenes.get(container.root) else {
            continue;
        };

        let source_url = player.source();
        let source = ipfs
            .content_url(source_url, &context.hash)
            .unwrap_or_else(|| source_url.to_owned());

        if T::has_video() {
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
                    image.texture_descriptor.usage = TextureUsages::COPY_DST
                        | TextureUsages::TEXTURE_BINDING
                        | TextureUsages::RENDER_ATTACHMENT;
                    image.transfer_priority = bevy::asset::RenderAssetTransferPriority::Immediate;
                    image.data = None;
                    images.add(image)
                }
                Some(texture) => texture.0.clone(),
            };

            let mut video = if source_url.starts_with("livekit-video://") {
                continue;
            } else if source_url.is_empty() {
                debug!("noop video {}", source_url);
                HtmlMediaEntity::<T>::new_noop(source_url.to_owned(), image_handle.clone())
            } else {
                debug!("https video {}", source_url);
                HtmlMediaEntity::<T>::new_video(
                    &source,
                    source_url.to_owned(),
                    image_handle.clone(),
                )
            };

            let video_volume = player.volume();
            video.set_loop(player.r#loop());
            video.set_volume(video_volume * scene_volume);
            let video_output = VideoTextureOutput(image_handle);

            commands.entity(ent).try_insert((video, video_output));
        } else {
            let mut audio = HtmlMediaEntity::<T>::new_audio(&source, source_url.to_owned());
            let audio_volume = player.volume();
            audio.set_loop(player.r#loop());
            audio.set_volume(audio_volume * scene_volume);

            commands.entity(ent).try_insert(audio);
        }
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_av_players<T: AVPlayer>(
    mut commands: Commands,
    mut av_players: Query<
        (
            Entity,
            &ContainerEntity,
            Option<&mut HtmlMediaEntity<T>>,
            Has<ShouldBePlaying<T>>,
        ),
        With<T>,
    >,
    mut images: ResMut<Assets<Image>>,
    mut scenes: Query<&mut RendererSceneContext>,
    send_queue: Res<FrameCopyRequestQueue>,
    frame: Res<FrameCount>,
) {
    for (ent, container, maybe_av, should_be_playing) in av_players.iter_mut() {
        let Some(mut av) = maybe_av else { continue };

        let state = av.state();

        if av.source().starts_with("livekit-video://") && state == VideoState::VsError {
            error!("Stream is erroring, retrying.");
            commands.entity(ent).try_remove::<HtmlMediaEntity<T>>();
            continue;
        }

        let is_playing = state == VideoState::VsPlaying;
        let can_play = matches!(state, VideoState::VsReady | VideoState::VsPaused);

        if !is_playing && should_be_playing && can_play {
            av.play()
        } else if is_playing {
            if !should_be_playing {
                av.stop();
            } else {
                #[allow(clippy::collapsible_else_if)]
                if let Some(video) = av.video() {
                    let new_time = av.new_frame_time().swap(0, Ordering::Relaxed);
                    if new_time != 0 {
                        // new frame is ready
                        let new_time = f32::from_bits(new_time);
                        trace!("got new frame -> {new_time}");

                        let Ok(frame) = VideoFrame::new_with_html_video_element(video) else {
                            warn!("failed to extract frame");
                            continue;
                        };

                        let image_id = av.image().unwrap().id();
                        let visible_rect = frame.visible_rect().unwrap();
                        let video_size =
                            (visible_rect.width() as u32, visible_rect.height() as u32);

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
                        video_length: av.duration() as f32,
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
    html_video_players: Query<(&T, &mut HtmlMediaEntity<T>)>,
) {
    let scene_volume = audio_settings.scene();
    for (av_player, html_video_player) in html_video_players {
        let volume = av_player.volume();
        html_video_player.set_volume(volume * scene_volume);
    }
}
