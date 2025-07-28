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
    proto_components::sdk::components::{PbAudioStream, PbVideoEvent, PbVideoPlayer, VideoState},
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

        // TODO: we can't reuse the same html element for audio and video, so will have to do audio separately
        // app.add_crdt_lww_component::<PbAudioStream, AVPlayer>(
        //     SceneComponentId::AUDIO_STREAM,
        //     ComponentPosition::EntityOnly,
        // );
        app.add_systems(Update, update_video_players.in_set(SceneSets::PostLoop));

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

#[derive(Component)]
pub struct HtmlVideoEntity {
    source: String,
    video: HtmlVideoElement,
    image: Handle<Image>,
    size: Option<(u32, u32)>,
    last_state: VideoState,
    last_reported_time: f32,
    current_time: f32,
    new_frame_time: Arc<AtomicU32>,
    state: Arc<Mutex<VideoState>>,
    _closures: Vec<Closure<dyn FnMut()>>,
}

/// safety: engine is single threaded
unsafe impl Sync for HtmlVideoEntity {}
unsafe impl Send for HtmlVideoEntity {}

// This block imports the global JS function we defined in main.js
#[wasm_bindgen(js_namespace = window)]
extern "C" {
    #[wasm_bindgen(js_name = setVideoSource)]
    fn set_video_source(elt: &HtmlVideoElement, src: &str);
}

impl HtmlVideoEntity {
    pub fn new(url: String, source: String, image: Handle<Image>) -> Self {
        let frame_time = Arc::new(AtomicU32::default());
        let state = Arc::new(Mutex::new(VideoState::VsLoading));
        let mut closures = Vec::new();
        let video = web_sys::window()
            .unwrap()
            .document()
            .and_then(|doc| {
                let container = doc
                    .get_element_by_id(VIDEO_CONTAINER_ID)
                    .expect("video container should exist");
                let video = doc.create_element("video").unwrap();
                container.append_child(&video).unwrap();
                video.dyn_into::<HtmlVideoElement>().ok()
            })
            .expect("Couldn't create video element");

        video.set_cross_origin(Some("anonymous"));

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

        video.set_oncanplay(register_callback(
            &mut closures,
            &state,
            VideoState::VsReady,
        ));
        video.set_onabort(register_callback(
            &mut closures,
            &state,
            VideoState::VsError,
        ));
        video.set_onerror(register_callback(
            &mut closures,
            &state,
            VideoState::VsError,
        ));
        video.set_onwaiting(register_callback(
            &mut closures,
            &state,
            VideoState::VsBuffering,
        ));
        video.set_onplaying(register_callback(
            &mut closures,
            &state,
            VideoState::VsPlaying,
        ));
        video.set_onpause(register_callback(
            &mut closures,
            &state,
            VideoState::VsPaused,
        ));
        video.set_onended(register_callback(
            &mut closures,
            &state,
            VideoState::VsPaused,
        ));

        // no wasm_bindgen for this!
        let rvc_prop = Reflect::get(&video, &"requestVideoFrameCallback".into()).unwrap();
        if rvc_prop.is_undefined() {
            panic!("no requestVideoFrameCallback");
        }
        let rvc_fn = rvc_prop.dyn_into::<web_sys::js_sys::Function>().unwrap();

        let callback = Rc::new(RefCell::new(None));
        let callback_clone = callback.clone();
        let frame_time_clone = frame_time.clone();
        let rvc_clone = rvc_fn.clone();

        *callback.borrow_mut() = Some(
            Closure::wrap(Box::new({
                let video = video.clone();
                move |_now: f64, metadata: JsValue| {
                    if let Some(media_time) = Reflect::get(&metadata, &"mediaTime".into())
                        .ok()
                        .and_then(|mt| mt.as_f64())
                    {
                        frame_time_clone.store(
                            (media_time as f32).to_bits(),
                            std::sync::atomic::Ordering::Relaxed,
                        );
                    };

                    rvc_clone
                        .call1(&video, callback_clone.borrow().as_ref().unwrap())
                        .unwrap();
                }
            }) as Box<dyn FnMut(f64, JsValue)>)
            .into_js_value(),
        );
        rvc_fn
            .call1(&video, callback.borrow().as_ref().unwrap())
            .unwrap();

        set_video_source(&video, &url);

        Self {
            source,
            video,
            image,
            size: None,
            new_frame_time: frame_time,
            last_state: VideoState::VsNone,
            last_reported_time: -1.0,
            current_time: -1.0,
            state,
            _closures: closures,
        }
    }

    pub fn set_loop(&mut self, looping: bool) {
        self.video.set_loop(looping)
    }

    pub fn play(&mut self) {
        debug!("called play");
        let _ = self.video.play();
    }

    pub fn stop(&mut self) {
        debug!("called play");
        let _ = self.video.pause();
    }

    pub fn state(&self) -> VideoState {
        self.state.lock().unwrap().clone()
    }
}

impl Drop for HtmlVideoEntity {
    fn drop(&mut self) {
        self.video.set_oncanplay(None);
        self.video.set_onabort(None);
        self.video.set_onerror(None);
        self.video.set_onwaiting(None);
        self.video.set_onplaying(None);
        self.video.set_onpause(None);
        self.video.set_onended(None);
        self.video.remove();
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn update_video_players(
    mut commands: Commands,
    mut video_players: Query<(
        Entity,
        &ContainerEntity,
        Ref<AVPlayer>,
        Option<&mut HtmlVideoEntity>,
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
    for (ent, container, player, mut maybe_video, maybe_texture, _) in video_players.iter_mut() {
        if let Some(video) = maybe_video
            .as_mut()
            .filter(|p| p.source == player.source.src)
        {
            if player.is_changed() {
                video.set_loop(player.source.r#loop.unwrap_or(false));
            }
        } else {
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
                    images.add(image)
                }
                Some(texture) => texture.0.clone(),
            };

            let Ok(context) = scenes.get(container.root) else {
                continue;
            };

            let source = ipfs
                .content_url(&player.source.src, &context.hash)
                .unwrap_or_else(|| player.source.src.clone());
            let mut video =
                HtmlVideoEntity::new(player.source.src.clone(), source, image_handle.clone());

            video.set_loop(player.source.r#loop.unwrap_or(false));
            let video_output = VideoTextureOutput(image_handle);

            commands.entity(ent).try_insert((video, video_output));
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
        .map(|(_, _, ent)| *ent)
        .collect::<HashSet<_>>();

    for (ent, container, _, video, _, _) in video_players.iter_mut() {
        let Some(mut video) = video else { continue };

        let should_be_playing = should_be_playing.contains(&ent);

        let is_playing = video.state() == VideoState::VsPlaying;
        if !is_playing && should_be_playing {
            video.play()
        } else if is_playing {
            if !should_be_playing {
                video.stop();
            } else {
                let new_time = video.new_frame_time.swap(0, Ordering::Relaxed);
                if new_time != 0 {
                    // new frame is ready
                    let new_time = f32::from_bits(new_time);

                    // check size
                    let video_size = (video.video.video_width(), video.video.video_height());
                    if video.size.is_none_or(|sz| sz != video_size) {
                        let Some(image) = images.get_mut(video.image.id()) else {
                            warn!("no image!");
                            continue;
                        };

                        image.resize(Extent3d {
                            width: video_size.0.max(16),
                            height: video_size.1.max(16),
                            depth_or_array_layers: 1,
                        });
                        video.size = Some(video_size)
                    }

                    // queue copy
                    let _ = send_queue.0.send(FrameCopyRequest {
                        video: WgpuWrapper::new(video.video.clone()),
                        target: video.image.id(),
                        size: wgpu::Extent3d {
                            width: video_size.0,
                            height: video_size.1,
                            depth_or_array_layers: 1,
                        },
                    });

                    video.current_time = new_time;
                }
            }
        }

        const VIDEO_REPORT_FREQUENCY: f32 = 1.0;
        let new_state = video.state();
        if new_state != video.last_state
            || video.current_time > video.last_reported_time + VIDEO_REPORT_FREQUENCY
            || video.current_time < video.last_reported_time
        {
            let Ok(mut context) = scenes.get_mut(container.root) else {
                continue;
            };
            let tick_number = context.tick_number;
            debug!("set {:?} {:?}", video.state(), video.current_time);
            context.update_crdt(
                SceneComponentId::VIDEO_EVENT,
                CrdtType::GO_ANY,
                container.container_id,
                &PbVideoEvent {
                    timestamp: frame.0,
                    tick_number,
                    current_offset: video.current_time,
                    video_length: video.video.duration() as f32,
                    state: video.state() as i32,
                },
            );
            video.last_state = new_state;
            video.last_reported_time = video.current_time;
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
        render_queue.copy_external_image_to_texture(
            &wgpu::CopyExternalImageSourceInfo {
                source: wgpu::ExternalImageSource::HTMLVideoElement(request.video.into_inner()),
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
