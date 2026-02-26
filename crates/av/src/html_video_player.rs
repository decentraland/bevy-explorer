use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
};

use bevy::{
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
use common::{sets::SceneSets, structs::AudioSettings, util::ReportErr};
#[cfg(feature = "livekit")]
use comms::livekit::participant::StreamViewer;
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::sdk::components::{PbAudioEvent, PbVideoEvent, VideoState},
    SceneComponentId,
};
use ipfs::IpfsResource;
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
    av_player_is_in_scene, av_player_should_be_playing, AVPlayer, InScene, ShouldBePlaying,
};

type RcClosure = Rc<RefCell<Option<Closure<dyn FnMut(f64, JsValue)>>>>;

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
                rebuild_html_media_entities.before(av_player_is_in_scene),
                update_av_players
                    .before(update_materials)
                    .after(av_player_should_be_playing),
            )
                .chain()
                .in_set(SceneSets::PostLoop),
        );
        app.add_systems(
            Update,
            update_html_video_player_volumes.run_if(resource_exists_and_changed::<AudioSettings>),
        );

        let (sx, rx) = tokio::sync::mpsc::unbounded_channel();

        app.insert_resource(FrameCopyRequestQueue(sx));

        app.add_observer(av_player_on_insert);
        app.add_observer(av_player_on_remove);

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
pub struct HtmlMediaEntity {
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
}

/// safety: engine is single threaded
unsafe impl Sync for HtmlMediaEntity {}
unsafe impl Send for HtmlMediaEntity {}

// This block imports the global JS function we defined in main.js
#[wasm_bindgen(js_namespace = window)]
extern "C" {
    #[wasm_bindgen(js_name = setVideoSource)]
    fn set_video_source(elt: &HtmlVideoElement, src: &str);
}

impl HtmlMediaEntity {
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

impl Drop for HtmlMediaEntity {
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

fn av_player_on_insert(
    trigger: Trigger<OnInsert, AVPlayer>,
    mut commands: Commands,
    mut av_players: Query<(&AVPlayer, &mut HtmlMediaEntity)>,
    audio_settings: Res<AudioSettings>,
) {
    info!("AVPlayer updated.");
    let entity = trigger.target();
    let Ok((av_player, mut html_media_entity)) = av_players.get_mut(entity) else {
        return;
    };

    // This forces an update on the entity
    commands.entity(entity).try_remove::<ShouldBePlaying>();
    if av_player.source.src == html_media_entity.source {
        debug!("Updating html media entity {entity}.");
        let av_player_volume = av_player.source.volume.unwrap_or(1.0);
        html_media_entity.stop();
        html_media_entity.set_loop(av_player.source.r#loop.unwrap_or(false));
        html_media_entity.set_volume(av_player_volume * audio_settings.scene());
    } else {
        debug!("Removing html media entity {entity} due to diverging source.");
        commands
            .entity(trigger.target())
            .try_remove::<HtmlMediaEntity>();
    }
}

fn av_player_on_remove(trigger: Trigger<OnRemove, AVPlayer>, mut commands: Commands) {
    let entity = trigger.target();
    commands.entity(entity).try_remove::<(
        InScene,
        ShouldBePlaying,
        HtmlMediaEntity,
        VideoTextureOutput,
    )>();
    #[cfg(feature = "livekit")]
    commands.entity(entity).try_remove::<StreamViewer>();
}

fn rebuild_html_media_entities(
    mut commands: Commands,
    av_players: Populated<
        (
            Entity,
            &ContainerEntity,
            &AVPlayer,
            Option<&VideoTextureOutput>,
        ),
        Without<HtmlMediaEntity>,
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

        let source = ipfs
            .content_url(&player.source.src, &context.hash)
            .unwrap_or_else(|| player.source.src.clone());

        if player.has_video {
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

            let mut video = if player.source.src.starts_with("livekit-video://") {
                let Some(video) =
                    HtmlMediaEntity::new_stream(player.source.src.clone(), image_handle.clone())
                else {
                    continue;
                };
                debug!("stream video {}", player.source.src);
                video
            } else if player.source.src.is_empty() {
                debug!("noop video {}", player.source.src);
                HtmlMediaEntity::new_noop(player.source.src.clone(), image_handle.clone())
            } else {
                debug!("https video {}", player.source.src);
                HtmlMediaEntity::new_video(&source, player.source.src.clone(), image_handle.clone())
            };

            let video_volume = player.source.volume.unwrap_or(1.0);
            video.set_loop(player.source.r#loop.unwrap_or(false));
            video.set_volume(video_volume * scene_volume);
            let video_output = VideoTextureOutput(image_handle);

            commands.entity(ent).try_insert((video, video_output));
        } else {
            let mut audio = HtmlMediaEntity::new_audio(&source, player.source.src.clone());
            let audio_volume = player.source.volume.unwrap_or(1.0);
            audio.set_loop(player.source.r#loop.unwrap_or(false));
            audio.set_volume(audio_volume * scene_volume);

            commands.entity(ent).try_insert(audio);
        }
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_av_players(
    mut commands: Commands,
    mut av_players: Query<(
        Entity,
        &ContainerEntity,
        &AVPlayer,
        Option<&mut HtmlMediaEntity>,
        Has<ShouldBePlaying>,
    )>,
    mut images: ResMut<Assets<Image>>,
    mut scenes: Query<&mut RendererSceneContext>,
    send_queue: Res<FrameCopyRequestQueue>,
    frame: Res<FrameCount>,
) {
    for (ent, container, player, maybe_av, should_be_playing) in av_players.iter_mut() {
        let Some(mut av) = maybe_av else { continue };

        let state = av.state();

        if av.source.starts_with("livekit-video://") && state == VideoState::VsError {
            error!("Stream is erroring, retrying.");
            commands.entity(ent).try_remove::<HtmlMediaEntity>();
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
                        let video_size =
                            (visible_rect.width() as u32, visible_rect.height() as u32);

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
                                .insert(VideoTextureOutput(image.clone()));
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

            if player.has_video {
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

fn update_html_video_player_volumes(
    audio_settings: Res<AudioSettings>,
    html_video_players: Query<(&AVPlayer, &mut HtmlMediaEntity)>,
) {
    let scene_volume = audio_settings.scene();
    for (av_player, html_video_player) in html_video_players {
        let volume = av_player.source.volume.unwrap_or(1.0);
        html_video_player.set_volume(volume * scene_volume);
    }
}
