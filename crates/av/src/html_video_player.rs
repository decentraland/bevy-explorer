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
    math::FloatOrd,
    platform::collections::HashSet,
    prelude::*,
    render::{
        render_asset::{RenderAssetUsages, RenderAssets},
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
        renderer::{RenderQueue, WgpuWrapper},
        texture::GpuImage,
        Render, RenderApp, RenderSet,
    },
};
use common::{
    sets::SceneSets,
    structs::{AppConfig, PrimaryUser},
};
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::sdk::components::{
        PbAudioEvent, PbAudioStream, PbVideoEvent, PbVideoPlayer, VideoState,
    },
    SceneComponentId,
};
use ipfs::IpfsResource;
use scene_runner::{
    renderer_context::RendererSceneContext,
    update_world::{material::VideoTextureOutput, AddCrdtInterfaceExt},
    ContainerEntity, ContainingScene,
};
use wasm_bindgen::prelude::wasm_bindgen;
use web_sys::{
    js_sys::{self, Reflect},
    wasm_bindgen::{prelude::Closure, JsValue},
    HtmlMediaElement,
};
use web_sys::{wasm_bindgen::JsCast, HtmlVideoElement};

pub struct VideoPlayerPlugin;

const VIDEO_CONTAINER_ID: &str = "video-player-container";

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
            }
        }

        app.add_crdt_lww_component::<PbVideoPlayer, AVPlayer>(
            SceneComponentId::VIDEO_PLAYER,
            ComponentPosition::EntityOnly,
        );

        app.add_crdt_lww_component::<PbAudioStream, AVPlayer>(
            SceneComponentId::AUDIO_STREAM,
            ComponentPosition::EntityOnly,
        );
        app.add_systems(Update, update_av_players.in_set(SceneSets::PostLoop));

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
    video: WgpuWrapper<HtmlVideoElement>,
    target: AssetId<Image>,
    size: Extent3d,
}

#[derive(Component, Debug)]
pub struct AVPlayer {
    // note we reuse PbVideoPlayer for audio as well
    pub source: PbVideoPlayer,
    pub has_video: bool,
}

impl From<PbVideoPlayer> for AVPlayer {
    fn from(value: PbVideoPlayer) -> Self {
        Self {
            source: value,
            has_video: true,
        }
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
            has_video: false,
        }
    }
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
    frame_closure: Rc<RefCell<Option<Closure<dyn FnMut(f64, JsValue)>>>>,
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

        let callback: Rc<RefCell<Option<Closure<dyn FnMut(f64, JsValue)>>>> =
            Rc::new(RefCell::new(None));
        let callback_handle: Rc<RefCell<Option<u32>>> = Rc::new(RefCell::new(None));
        let callback_clone = callback.clone();
        let handle_clone = callback_handle.clone();
        let frame_time_clone = frame_time.clone();
        let rvc_clone = rvc_fn.clone();

        *callback.borrow_mut() = Some(Closure::wrap(Box::new({
            let video = video.clone();
            move |_now: f64, metadata: JsValue| {
                debug!("frame received");
                if let Some(media_time) = Reflect::get(&metadata, &"mediaTime".into())
                    .ok()
                    .and_then(|mt| mt.as_f64())
                {
                    debug!("frame received -> {media_time}");
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

    pub fn set_loop(&mut self, looping: bool) {
        self.media.set_loop(looping)
    }

    pub fn set_volume(&self, volume: f32) {
        self.media.set_volume(volume as f64)
    }

    pub fn play(&mut self) {
        debug!("called play");
        let _ = self.media.play();
    }

    pub fn stop(&mut self) {
        debug!("called play");
        let _ = self.media.pause();
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
            Reflect::get(&video, &"cancelVideoFrameCallback".into())
                .unwrap()
                .dyn_into::<web_sys::js_sys::Function>()
                .unwrap()
                .call1(&video, &JsValue::from(handle))
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

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn update_av_players(
    mut commands: Commands,
    mut av_players: Query<(
        Entity,
        &ContainerEntity,
        Ref<AVPlayer>,
        Option<&mut HtmlMediaEntity>,
        Option<&VideoTextureOutput>,
        &GlobalTransform,
    )>,
    mut images: ResMut<Assets<Image>>,
    ipfs: Res<IpfsResource>,
    mut scenes: Query<&mut RendererSceneContext>,
    config: Res<AppConfig>,
    containing_scene: ContainingScene,
    user: Query<&GlobalTransform, With<PrimaryUser>>,
    send_queue: Res<FrameCopyRequestQueue>,
    frame: Res<FrameCount>,
) {
    for (ent, container, player, mut maybe_av, maybe_texture, _) in av_players.iter_mut() {
        if let Some(av) = maybe_av.as_mut().filter(|p| p.source == player.source.src) {
            if player.is_changed() {
                av.set_loop(player.source.r#loop.unwrap_or(false));
                av.set_volume(player.source.volume.unwrap_or(1.0));
            }
        } else {
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
                        image.immediate_upload = true;
                        images.add(image)
                    }
                    Some(texture) => texture.0.clone(),
                };

                let mut video = HtmlMediaEntity::new_video(
                    &source,
                    player.source.src.clone(),
                    image_handle.clone(),
                );

                video.set_loop(player.source.r#loop.unwrap_or(false));
                video.set_volume(player.source.volume.unwrap_or(1.0));
                let video_output = VideoTextureOutput(image_handle);

                commands.entity(ent).try_insert((video, video_output));
            } else {
                let mut audio = HtmlMediaEntity::new_audio(&source, player.source.src.clone());
                audio.set_loop(player.source.r#loop.unwrap_or(false));
                audio.set_volume(player.source.volume.unwrap_or(1.0));

                commands.entity(ent).try_insert(audio);
            }
        }
    }

    // disable distant av
    let Ok(user) = user.single() else {
        return;
    };

    let containing_scenes = containing_scene.get_position(user.translation());

    let mut sorted_players = av_players
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
        .map(|(_, _, ent)| *ent)
        .collect::<HashSet<_>>();

    for (ent, container, player, maybe_av, _, _) in av_players.iter_mut() {
        let Some(mut av) = maybe_av else { continue };

        let should_be_playing = should_be_playing.contains(&ent);

        let state = av.state();

        let is_playing = state == VideoState::VsPlaying;
        let can_play = match state {
            VideoState::VsReady | VideoState::VsPaused => true,
            _ => false,
        };

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
                        debug!("got new frame -> {new_time}");
                        let image_id = av.image.as_ref().unwrap().id();
                        let video_size = (video.video_width(), video.video_height());
                        let video = video.clone();

                        // check size
                        if av.size.is_none_or(|sz| sz != video_size) {
                            let Some(image) = images.get_mut(image_id) else {
                                warn!("no image!");
                                continue;
                            };

                            image.resize(Extent3d {
                                width: video_size.0.max(16),
                                height: video_size.1.max(16),
                                depth_or_array_layers: 1,
                            });
                            av.size = Some(video_size)
                        }

                        // queue copy
                        let _ = send_queue.0.send(FrameCopyRequest {
                            video: WgpuWrapper::new(video),
                            target: image_id,
                            size: wgpu::Extent3d {
                                width: video_size.0,
                                height: video_size.1,
                                depth_or_array_layers: 1,
                            },
                        });

                        av.current_time = new_time;
                    } else {
                        debug!("no frame (new_time == 0)");
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
            debug!("set {:?} {:?}", av.state(), av.current_time);

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
    while let Ok(request) = requests.0.try_recv() {
        let Some(gpu_image) = images.get(request.target) else {
            warn!("missing gpu image");
            continue;
        };
        let video = request.video.into_inner();
        let source_size = (video.video_width(), video.video_height());
        let target_size = (gpu_image.size.width, gpu_image.size.height);

        if source_size != target_size {
            warn!("skip frame {source_size:?} != {target_size:?}");
            continue;
        }

        render_queue.copy_external_image_to_texture(
            &wgpu::CopyExternalImageSourceInfo {
                source: wgpu::ExternalImageSource::HTMLVideoElement(video),
                origin: wgpu::Origin2d::ZERO,
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
            request.size,
        );
    }
}
