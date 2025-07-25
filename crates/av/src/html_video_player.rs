use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use bevy::{
    color::palettes::basic,
    diagnostic::FrameCount,
    math::FloatOrd,
    platform::collections::HashMap,
    prelude::*,
    render::{
        render_asset::{RenderAssetUsages, RenderAssets},
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
        renderer::RenderQueue,
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
use web_sys::{
    js_sys::Reflect,
    wasm_bindgen::{prelude::Closure, JsValue},
};
use web_sys::{wasm_bindgen::JsCast, HtmlVideoElement};

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
    video: HtmlVideoEntity,
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

#[derive(Component, Clone)]
pub struct HtmlVideoEntity {
    url: String,
    source: String,
    video: HtmlVideoElement,
    image: Handle<Image>,
    size: Option<(u32, u32)>,
    frame_ready: Arc<AtomicBool>,
}

/// safety: engine is single threaded
unsafe impl Sync for HtmlVideoEntity {}
unsafe impl Send for HtmlVideoEntity {}

impl HtmlVideoEntity {
    pub fn new(url: String, source: String, image: Handle<Image>) -> Self {
        let frame_ready = Arc::new(AtomicBool::default());
        let video = web_sys::window()
            .unwrap()
            .document()
            .and_then(|doc| {
                let video = doc.create_element("video").unwrap();
                doc.body().unwrap().append_child(&video).unwrap();
                video.dyn_into::<HtmlVideoElement>().ok()
            })
            .expect("Couldn't create video element");

        video.set_cross_origin(Some("anonymous"));
        video.set_src(&url);
        video.set_autoplay(true);

        // no wasm_bindgen for this!
        let rvc_prop = Reflect::get(&video, &"requestVideoFrameCallback".into()).unwrap();
        if rvc_prop.is_undefined() {
            panic!("no requestVideoFrameCallback");
        }
        let rvc_fn = rvc_prop.dyn_into::<web_sys::js_sys::Function>().unwrap();

        let callback = Rc::new(RefCell::new(None));
        let callback_clone = callback.clone();
        let ready_clone = frame_ready.clone();
        let rvc_clone = rvc_fn.clone();

        *callback.borrow_mut() = Some(
            Closure::wrap(Box::new({
                let video = video.clone();
                move |_now: f64, _metadata: JsValue| {
                    ready_clone.store(true, std::sync::atomic::Ordering::Relaxed);
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

        Self {
            url,
            source,
            video,
            image,
            size: None,
            frame_ready,
        }
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
    scenes: Query<&RendererSceneContext>,
    config: Res<AppConfig>,
    containing_scene: ContainingScene,
    user: Query<&GlobalTransform, With<PrimaryUser>>,
    mut send_queue: Res<FrameCopyRequestQueue>,
) {

    for (ent, container, player, mut maybe_html, maybe_texture, _) in video_players.iter_mut() {
        if maybe_html.as_ref().is_none_or(|html| html.source != player.source.src) {
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
                        TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT;
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
            let video_entity = HtmlVideoEntity::new(player.source.src.clone(), source, image_handle.clone());
            let video_output = VideoTextureOutput(image_handle);

            commands
                .entity(ent)
                .try_insert((video_entity, video_output));
        }

        if let Some(mut html_player) = maybe_html {
            if html_player.frame_ready.swap(false, Ordering::Relaxed) {
                // new frame is ready, queue a copy

                // check size
                let video_size = (
                    html_player.video.video_width(),
                    html_player.video.video_height(),
                );
                if html_player.size.is_none_or(|sz| sz != video_size) {
                    let Some(mut image) = images.get_mut(html_player.image.id()) else {
                        warn!("no image!");
                        continue;
                    };

                    image.resize(Extent3d {
                        width: video_size.0.max(16),
                        height: video_size.1.max(16),
                        depth_or_array_layers: 1,
                    });
                    html_player.size = Some(video_size)
                }

                // queue copy
                let _ = send_queue.0.send(FrameCopyRequest {
                    video: html_player.clone(),
                    target: html_player.image.id(),
                    size: wgpu::Extent3d {
                        width: video_size.0,
                        height: video_size.1,
                        depth_or_array_layers: 1,
                    },
                });
            }
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
            &wgpu::ImageCopyExternalImage {
                source: wgpu::ExternalImageSource::HTMLVideoElement(request.video.video),
                origin: wgpu::Origin2d::ZERO,
                flip_y: false,
            },
            wgpu::ImageCopyTextureTagged {
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
