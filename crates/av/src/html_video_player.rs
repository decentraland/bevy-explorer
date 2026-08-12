use std::{
    cell::RefCell,
    marker::PhantomData,
    rc::Rc,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
};

use bevy::{
    asset::RenderAssetTransferPriority,
    color::palettes::basic,
    diagnostic::FrameCount,
    platform::collections::HashMap,
    prelude::*,
    render::{
        render_asset::{RenderAssetUsages, RenderAssets},
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
        renderer::{RenderQueue, WgpuWrapper},
        texture::GpuImage,
        Render, RenderApp, RenderSet,
    },
};
use common::{debug_panic, sets::SceneSets, structs::AudioSettings, util::ReportErr};
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::sdk::components::{PbAudioEvent, PbVideoEvent, VideoState},
    SceneComponentId,
};
use ipfs::IpfsResource;
use livestream_manager::ReceiverImage;
use scene_runner::{
    renderer_context::RendererSceneContext,
    update_world::material::{update_materials, VideoTextureOutput},
    ContainerEntity,
};
use wasm_bindgen::prelude::wasm_bindgen;
use web_sys::{
    js_sys::{self, Reflect},
    wasm_bindgen::{prelude::Closure, JsCast, JsValue},
    HtmlMediaElement, HtmlVideoElement, VideoFrame,
};

use crate::{
    audio_stream_should_be_playing, video_player_should_be_playing, AVPlayer, AVPlayerConfig,
    AudioStream, ShouldBePlaying, Stream, VideoPlayer, LIVEKIT_VIDEO_STREAM,
};

type RcClosure = Rc<RefCell<Option<Closure<dyn FnMut(f64, JsValue)>>>>;

const VIDEO_CONTAINER_ID: &str = "video-player-container";
const STREAM_CONTAINER_ID: &str = "stream-player-container";

pub struct VideoPlayerPlugin;

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
                players_waiting_for_stream,
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

        app.add_observer(new_player_source::<AudioStream>);
        app.add_observer(new_player_source::<VideoPlayer>);
        app.add_observer(player_source_removed::<AudioStream>);
        app.add_observer(player_source_removed::<VideoPlayer>);
        app.add_observer(player_config_added::<AudioStream>);
        app.add_observer(player_config_added::<VideoPlayer>);
        app.add_observer(player_position_added::<AudioStream>);
        app.add_observer(player_position_added::<VideoPlayer>);
        app.add_observer(av_player_should_be_playing_on_add::<AudioStream>);
        app.add_observer(av_player_should_be_playing_on_add::<VideoPlayer>);
        app.add_observer(av_player_should_be_playing_on_remove::<AudioStream>);
        app.add_observer(av_player_should_be_playing_on_remove::<VideoPlayer>);
        app.add_observer(receiver_image_added);

        let (sx, rx) = tokio::sync::mpsc::unbounded_channel();

        app.insert_resource(FrameCopyRequestQueue(sx));

        let render_app = app.sub_app_mut(RenderApp);
        render_app
            .insert_resource(FrameCopyReceiveQueue(rx))
            .add_systems(Render, perform_video_copies.in_set(RenderSet::Queue));
    }
}

#[derive(Resource)]
pub struct FrameCopyRequestQueue(tokio::sync::mpsc::UnboundedSender<FrameCopyRequest>);

#[derive(Resource)]
pub struct FrameCopyReceiveQueue(tokio::sync::mpsc::UnboundedReceiver<FrameCopyRequest>);

pub struct FrameCopyRequest {
    video_frame: WgpuWrapper<VideoFrame>,
    target: AssetId<Image>,
}

#[derive(Component)]
pub struct HtmlMediaEntity<T: AVPlayer> {
    source: String,
    media: HtmlMediaElement,
    video: Option<HtmlVideoElement>,
    image: Option<Handle<Image>>,
    size: Option<(u32, u32)>,
    last_state: VideoState,
    last_reported_time: f32,
    current_time: f32,
    new_frame_time: Arc<AtomicU32>,
    state: Arc<Mutex<VideoState>>,
    _closures: Vec<Closure<dyn FnMut()>>,
    frame_closure: RcClosure,
    frame_callback_handle: Rc<RefCell<Option<u32>>>,
    _phantom: PhantomData<T>,
}

/// safety: engine is single threaded
unsafe impl<T: AVPlayer> Sync for HtmlMediaEntity<T> {}
unsafe impl<T: AVPlayer> Send for HtmlMediaEntity<T> {}

// This block imports the global JS function we defined in main.js
#[wasm_bindgen(js_namespace = window)]
extern "C" {
    #[wasm_bindgen(js_name = setVideoSource)]
    fn set_video_source(elt: &HtmlVideoElement, src: &str);
}

impl<T: AVPlayer> HtmlMediaEntity<T> {
    fn common_init(source: String, media: HtmlMediaElement) -> Self {
        let mut closures = Vec::default();
        let state = Arc::new(Mutex::new(VideoState::VsLoading));

        fn register_callback<'a>(
            closures: &'a mut Vec<Closure<dyn FnMut()>>,
            state: &Arc<Mutex<VideoState>>,
            new_state: VideoState,
        ) -> Option<&'a js_sys::Function> {
            let state = state.clone();
            let closure = Closure::wrap(Box::new({
                move || {
                    let mut state = state.lock().unwrap();
                    *state = new_state;
                    debug!("state -> {new_state:?}");
                }
            }) as Box<dyn FnMut()>);
            closures.push(closure);
            closures.last().map(move |c| c.as_ref().unchecked_ref())
        }

        media.set_oncanplay(register_callback(
            &mut closures,
            &state,
            VideoState::VsReady,
        ));
        media.set_onabort(register_callback(
            &mut closures,
            &state,
            VideoState::VsError,
        ));
        media.set_onerror(register_callback(
            &mut closures,
            &state,
            VideoState::VsError,
        ));
        media.set_onwaiting(register_callback(
            &mut closures,
            &state,
            VideoState::VsBuffering,
        ));
        media.set_onplaying(register_callback(
            &mut closures,
            &state,
            VideoState::VsPlaying,
        ));
        media.set_onpause(register_callback(
            &mut closures,
            &state,
            VideoState::VsPaused,
        ));
        media.set_onended(register_callback(
            &mut closures,
            &state,
            VideoState::VsPaused,
        ));

        Self {
            source,
            media,
            video: None,
            image: None,
            size: None,
            last_state: VideoState::VsNone,
            last_reported_time: -1.0,
            current_time: -1.0,
            new_frame_time: Default::default(),
            state,
            _closures: closures,
            frame_closure: Default::default(),
            frame_callback_handle: Default::default(),
            _phantom: Default::default(),
        }
    }

    pub fn new_audio(url: &str, source: String) -> Self {
        let media = web_sys::window()
            .unwrap()
            .document()
            .and_then(|doc| {
                let container = doc
                    .get_element_by_id(VIDEO_CONTAINER_ID)
                    .expect("video container should exist");
                let video = doc.create_element("audio").unwrap();
                container.append_child(&video).unwrap();
                video.dyn_into::<HtmlMediaElement>().ok()
            })
            .expect("Couldn't create video element");

        media.set_src(url);

        Self::common_init(source, media)
    }

    pub fn new_video(url: &str, source: String, image: Handle<Image>) -> Self {
        let media = web_sys::window()
            .unwrap()
            .document()
            .and_then(|doc| {
                let container = doc
                    .get_element_by_id(VIDEO_CONTAINER_ID)
                    .expect("video container should exist");
                let video = doc.create_element("video").unwrap();
                container.append_child(&video).unwrap();
                video.dyn_into::<HtmlMediaElement>().ok()
            })
            .expect("Couldn't create video element");

        let video = media.clone().dyn_into::<HtmlVideoElement>().unwrap();

        video.set_cross_origin(Some("anonymous"));

        let frame_time = Arc::new(AtomicU32::default());

        // video frame callback - no wasm_bindgen for this!
        let rvc_prop = Reflect::get(&video, &"requestVideoFrameCallback".into()).unwrap();
        if rvc_prop.is_undefined() {
            panic!("no requestVideoFrameCallback");
        }
        let rvc_fn = rvc_prop.dyn_into::<web_sys::js_sys::Function>().unwrap();

        let callback: RcClosure = Rc::new(RefCell::new(None));
        let callback_handle: Rc<RefCell<Option<u32>>> = Rc::new(RefCell::new(None));
        let callback_clone = callback.clone();
        let handle_clone = callback_handle.clone();
        let frame_time_clone = frame_time.clone();
        let rvc_clone = rvc_fn.clone();

        *callback.borrow_mut() = Some(Closure::wrap(Box::new({
            let video = video.clone();
            move |_now: f64, metadata: JsValue| {
                trace!("frame received");
                if let Some(media_time) = Reflect::get(&metadata, &"mediaTime".into())
                    .ok()
                    .and_then(|mt| mt.as_f64())
                {
                    trace!("frame received -> {media_time}");
                    frame_time_clone.store(
                        (media_time as f32).to_bits(),
                        std::sync::atomic::Ordering::Relaxed,
                    );
                };

                if let Some(cb) = callback_clone.borrow().as_ref() {
                    if let Ok(new_handle) = rvc_clone.call1(&video, cb.as_ref().unchecked_ref()) {
                        *handle_clone.borrow_mut() = new_handle.as_f64().map(|f| f as u32);
                    }
                } else {
                    debug!("no cb - dropping");
                }
            }
        }) as Box<dyn FnMut(f64, JsValue)>));
        let initial_handle = rvc_fn
            .call1(
                &video,
                callback.borrow().as_ref().unwrap().as_ref().unchecked_ref(),
            )
            .unwrap();
        *callback_handle.borrow_mut() = initial_handle.as_f64().map(|f| f as u32);

        set_video_source(&video, url);

        let mut slf = Self::common_init(source, media);
        slf.video = Some(video);
        slf.image = Some(image);
        slf.new_frame_time = frame_time;
        slf.frame_closure = callback;
        slf.frame_callback_handle = callback_handle;
        slf
    }

    pub fn new_stream(source: String, image: Handle<Image>) -> Option<Self> {
        let media = web_sys::window().unwrap().document().and_then(|doc| {
            let container = doc
                .get_element_by_id(STREAM_CONTAINER_ID)
                .expect("streamer video container should exist");
            let video = container
                .get_elements_by_tag_name("video")
                .get_with_index(0)?;
            video.dyn_into::<HtmlMediaElement>().ok()
        })?;

        let video = media.clone().dyn_into::<HtmlVideoElement>().unwrap();

        let frame_time = Arc::new(AtomicU32::default());

        // video frame callback - no wasm_bindgen for this!
        let rvc_prop = Reflect::get(&video, &"requestVideoFrameCallback".into()).unwrap();
        if rvc_prop.is_undefined() {
            panic!("no requestVideoFrameCallback");
        }
        let rvc_fn = rvc_prop.dyn_into::<web_sys::js_sys::Function>().unwrap();

        let callback: RcClosure = Rc::new(RefCell::new(None));
        let callback_handle: Rc<RefCell<Option<u32>>> = Rc::new(RefCell::new(None));
        let callback_clone = callback.clone();
        let handle_clone = callback_handle.clone();
        let frame_time_clone = frame_time.clone();
        let rvc_clone = rvc_fn.clone();

        *callback.borrow_mut() = Some(Closure::wrap(Box::new({
            let video = video.clone();
            move |_now: f64, metadata: JsValue| {
                trace!("stream frame received");
                if let Some(media_time) = Reflect::get(&metadata, &"mediaTime".into())
                    .ok()
                    .and_then(|mt| mt.as_f64())
                {
                    trace!("stream frame received -> {media_time}");
                    frame_time_clone.store(
                        (media_time as f32).to_bits(),
                        std::sync::atomic::Ordering::Relaxed,
                    );
                };

                if let Some(cb) = callback_clone.borrow().as_ref() {
                    if let Ok(new_handle) = rvc_clone.call1(&video, cb.as_ref().unchecked_ref()) {
                        *handle_clone.borrow_mut() = new_handle.as_f64().map(|f| f as u32);
                    }
                } else {
                    warn!("no stream cb - dropping");
                }
            }
        }) as Box<dyn FnMut(f64, JsValue)>));
        let initial_handle = rvc_fn
            .call1(
                &video,
                callback.borrow().as_ref().unwrap().as_ref().unchecked_ref(),
            )
            .unwrap();
        *callback_handle.borrow_mut() = initial_handle.as_f64().map(|f| f as u32);

        let mut slf = Self::common_init(source, media);
        slf.video = Some(video);
        slf.image = Some(image);
        slf.new_frame_time = frame_time;
        slf.frame_closure = callback;
        slf.frame_callback_handle = callback_handle;

        // Hack to force a callback trigger
        slf.stop();
        slf.play();

        Some(slf)
    }

    pub fn new_noop(source: String, image: Handle<Image>) -> Self {
        let media = web_sys::window()
            .unwrap()
            .document()
            .and_then(|doc| {
                let container = doc
                    .get_element_by_id(VIDEO_CONTAINER_ID)
                    .expect("video container should exist");
                let video = doc.create_element("video").unwrap();
                container.append_child(&video).unwrap();
                video.dyn_into::<HtmlMediaElement>().ok()
            })
            .expect("Couldn't create video element");

        let mut slf = Self::common_init(source, media);
        slf.video = None;
        slf.image = Some(image);
        slf
    }

    pub fn set_loop(&mut self, looping: bool) {
        self.media.set_loop(looping)
    }

    pub fn set_volume(&self, volume: f32) {
        self.media.set_volume(volume.clamp(0.0, 1.0) as f64)
    }

    pub fn play(&mut self) {
        debug!("called play");
        self.media.play().report();
    }

    pub fn stop(&mut self) {
        debug!("called stop");
        self.media.pause().report();
    }

    pub fn state(&self) -> VideoState {
        *self.state.lock().unwrap()
    }
}

impl<T: AVPlayer> Drop for HtmlMediaEntity<T> {
    fn drop(&mut self) {
        debug!("shutdown");
        if let (Some(video), Some(handle)) =
            (&self.video, self.frame_callback_handle.borrow_mut().take())
        {
            Reflect::get(video, &"cancelVideoFrameCallback".into())
                .unwrap()
                .dyn_into::<web_sys::js_sys::Function>()
                .unwrap()
                .call1(video, &JsValue::from(handle))
                .unwrap();
        }
        self.frame_closure.take();
        self.media.set_oncanplay(None);
        self.media.set_onabort(None);
        self.media.set_onerror(None);
        self.media.set_onwaiting(None);
        self.media.set_onplaying(None);
        self.media.set_onpause(None);
        self.media.set_onended(None);
        self.media.remove();
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

    let Ok((player_source, container_entity, maybe_video_texture_output, has_stream)) =
        av_players.get(entity)
    else {
        unreachable!("Infallible query");
    };
    let Ok(context) = scenes.get(container_entity.root) else {
        debug_panic!("AVPlayer has an invalid link to RendererSceneContext");
    };

    let livestream = &**player_source == LIVEKIT_VIDEO_STREAM;
    if livestream != has_stream {
        if livestream {
            debug!("AVPlayer {} now a stream.", entity);
            commands.entity(entity).insert(Stream);
        } else {
            debug!("AVPlayer {} no longer a stream.", entity);
            commands.entity(entity).remove::<Stream>();
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
        Some(texture) => texture.0.clone(),
    };

    debug!(
        "Creating new html media entity for {} targeting \"{}\"",
        entity,
        &(**player_source)
    );
    match &(**player_source) {
        LIVEKIT_VIDEO_STREAM => (),
        _ => {
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

fn player_source_removed<T: AVPlayer>(
    trigger: Trigger<OnRemove, T::Source>,
    mut commands: Commands,
) {
    let entity = trigger.target();
    commands.entity(entity).try_remove::<HtmlMediaEntity<T>>();
}

#[expect(clippy::type_complexity)]
fn player_config_added<T: AVPlayer>(
    trigger: Trigger<OnInsert, T::Config>,
    mut av_players: Query<(&T::Config, &mut HtmlMediaEntity<T>, Has<ShouldBePlaying<T>>)>,
) {
    let entity = trigger.target();
    let Ok((config, mut html_media_entity, has_should_be_playing)) = av_players.get_mut(entity)
    else {
        unreachable!("Infallible query");
    };

    if config.playing() && has_should_be_playing {
        html_media_entity.play();
    } else {
        html_media_entity.stop();
    }
    html_media_entity.set_volume(config.volume());
    html_media_entity.set_loop(config.r#loop());
}

fn player_position_added<T: AVPlayer>(
    trigger: Trigger<OnInsert, T::Position>,
    mut av_players: Query<(&T::Position, &mut HtmlMediaEntity<T>)>,
) {
    let entity = trigger.target();
    let Ok((position, mut html_media_entity)) = av_players.get_mut(entity) else {
        unreachable!("Infallible query");
    };

    debug!("Seeking AVPlayer to {}", **position);
    html_media_entity.current_time = **position;
}

fn av_player_should_be_playing_on_add<T: AVPlayer>(
    trigger: Trigger<OnAdd, ShouldBePlaying<T>>,
    mut av_players: Query<&mut HtmlMediaEntity<T>, With<T>>,
) {
    let entity = trigger.target();
    let Ok(mut html_media_entity) = av_players.get_mut(entity) else {
        return;
    };

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

    html_media_entity.stop();
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_av_players<T: AVPlayer>(
    mut commands: Commands,
    mut av_players: Query<
        (Entity, &ContainerEntity, Option<&mut HtmlMediaEntity<T>>),
        (With<T>, With<ShouldBePlaying<T>>),
    >,
    mut images: ResMut<Assets<Image>>,
    mut scenes: Query<&mut RendererSceneContext>,
    send_queue: Res<FrameCopyRequestQueue>,
    frame: Res<FrameCount>,
) {
    for (ent, container, maybe_av) in av_players.iter_mut() {
        let Some(mut av) = maybe_av else { continue };

        let state = av.state();

        if av.source == LIVEKIT_VIDEO_STREAM && state == VideoState::VsError {
            error!("Stream is erroring, retrying.");
            commands
                .entity(ent)
                .try_remove::<HtmlMediaEntity<T>>()
                .insert(WaitingForStream);
            continue;
        }

        let is_playing = state == VideoState::VsPlaying;

        if is_playing {
            #[allow(clippy::collapsible_else_if)]
            if let Some(video) = av.video.as_ref() {
                let new_time = av.new_frame_time.swap(0, Ordering::Relaxed);
                if new_time != 0 {
                    // new frame is ready
                    let new_time = f32::from_bits(new_time);
                    trace!("got new frame -> {new_time}");

                    let Ok(frame) = VideoFrame::new_with_html_video_element(video) else {
                        warn!("failed to extract frame");
                        continue;
                    };

                    let image_id = av.image.as_ref().unwrap().id();
                    let visible_rect = frame.visible_rect().unwrap();
                    let video_size = (visible_rect.width() as u32, visible_rect.height() as u32);

                    // check size
                    if av.size.is_none_or(|sz| sz != video_size) {
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
                        av.size = Some(video_size);
                        commands
                            .entity(ent)
                            .try_insert(VideoTextureOutput(image.clone()));
                        av.image = Some(image);

                        trace!("queue resized frame {:?}", video_size);
                    }

                    // queue copy
                    trace!("queue frame {:?}", video_size);
                    send_queue
                        .0
                        .send(FrameCopyRequest {
                            video_frame: WgpuWrapper::new(frame),
                            target: image_id,
                        })
                        .report();

                    av.current_time = new_time;
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
        if new_state != av.last_state
            || av.current_time > av.last_reported_time + AV_REPORT_FREQUENCY
            || av.current_time < av.last_reported_time
        {
            let Ok(mut context) = scenes.get_mut(container.root) else {
                continue;
            };
            let tick_number = context.tick_number;
            trace!("set {:?} {:?}", av.state(), av.current_time);

            if T::has_video() {
                context.update_crdt(
                    SceneComponentId::VIDEO_EVENT,
                    CrdtType::GO_ANY,
                    container.container_id,
                    &PbVideoEvent {
                        timestamp: frame.0,
                        tick_number,
                        current_offset: av.current_time,
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
            av.last_state = new_state;
            av.last_reported_time = av.current_time;
        }
    }
}

fn perform_video_copies(
    mut requests: ResMut<FrameCopyReceiveQueue>,
    images: Res<RenderAssets<GpuImage>>,
    render_queue: Res<RenderQueue>,
) {
    let mut latest_requests: HashMap<AssetId<Image>, FrameCopyRequest> = HashMap::new();

    while let Ok(request) = requests.0.try_recv() {
        if let Some(prev) = latest_requests.get(&request.target) {
            prev.video_frame.close();
        }
        latest_requests.insert(request.target, request);
    }

    for (_, request) in latest_requests.drain() {
        let frame_copy = request.video_frame.clone();
        let Some(gpu_image) = images.get(request.target) else {
            warn!("missing gpu image");
            continue;
        };
        let frame = request.video_frame.into_inner();
        let visible_rect = frame.visible_rect().unwrap();
        let source_size = (visible_rect.width() as u32, visible_rect.height() as u32);
        let target_size = (gpu_image.size.width, gpu_image.size.height);

        if source_size != target_size {
            warn!("skip frame {source_size:?} != {target_size:?}");
            continue;
        }

        trace!(
            "{:?}/{:?} perform {:?} -> {:?}",
            request.target,
            gpu_image.texture_view,
            source_size,
            target_size
        );

        render_queue.copy_external_image_to_texture(
            &wgpu::CopyExternalImageSourceInfo {
                source: wgpu::ExternalImageSource::VideoFrame(frame),
                origin: wgpu::Origin2d {
                    x: visible_rect.x() as u32,
                    y: visible_rect.y() as u32,
                },
                flip_y: false,
            },
            wgpu::CopyExternalImageDestInfo {
                texture: &gpu_image.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
                premultiplied_alpha: false, // Video frames are not typically premultiplied.
                color_space: wgpu::PredefinedColorSpace::Srgb,
            },
            Extent3d {
                width: source_size.0,
                height: source_size.1,
                depth_or_array_layers: 1,
            },
        );

        frame_copy.close();
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

#[derive(Component)]
struct WaitingForStream;

fn receiver_image_added(
    trigger: Trigger<OnAdd, ReceiverImage>,
    mut commands: Commands,
    video_players: Query<&ReceiverImage, With<VideoPlayer>>,
) {
    let entity = trigger.target();
    let Ok(receiver_image) = video_players.get(entity) else {
        unreachable!("Infallible query");
    };

    if let Some(html_media_entity) = HtmlMediaEntity::<VideoPlayer>::new_stream(
        LIVEKIT_VIDEO_STREAM.to_owned(),
        (*receiver_image).clone(),
    ) {
        commands.entity(entity).insert((
            html_media_entity,
            VideoTextureOutput((*receiver_image).clone()),
        ));
    } else {
        debug!("No stream available, waiting for it to become available");
        commands.entity(entity).insert(WaitingForStream);
    }
}

fn players_waiting_for_stream(
    mut commands: Commands,
    video_players: Populated<(Entity, &ReceiverImage), With<WaitingForStream>>,
) {
    for (entity, receiver_image) in video_players.into_inner() {
        if let Some(html_media_entity) = HtmlMediaEntity::<VideoPlayer>::new_stream(
            LIVEKIT_VIDEO_STREAM.to_owned(),
            (*receiver_image).clone(),
        ) {
            debug!("Stream became available");
            commands
                .entity(entity)
                .insert((
                    html_media_entity,
                    VideoTextureOutput((*receiver_image).clone()),
                ))
                .remove::<WaitingForStream>();
        }
    }
}
