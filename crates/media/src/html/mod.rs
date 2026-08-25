pub mod plugin;

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex, atomic::AtomicU32},
};

use bevy::{prelude::*, render::renderer::WgpuWrapper};
use common::util::ReportErr;
use dcl_component::proto_components::sdk::components::VideoState;
use js_sys::{Function, Reflect};
use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};
use web_sys::{HtmlMediaElement, HtmlVideoElement, VideoFrame};

pub type RcClosure = Rc<RefCell<Option<Closure<dyn FnMut(f64, JsValue)>>>>;

pub struct HtmlMedia {
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
unsafe impl Sync for HtmlMedia {}
unsafe impl Send for HtmlMedia {}

// This block imports the global JS function we defined in main.js
#[wasm_bindgen(js_namespace = window)]
extern "C" {
    #[wasm_bindgen(js_name = setVideoSource)]
    fn set_video_source(elt: &HtmlVideoElement, src: &str);
}

impl HtmlMedia {
    pub fn new_noop(source: String, image: Handle<Image>) -> Self {
        let window = web_sys::window().unwrap();
        let document = window.document().unwrap();
        let video = document.create_element("video").unwrap();
        let media = video.clone().dyn_into::<HtmlMediaElement>().unwrap();

        let mut slf = HtmlMedia::common_init(source, media);
        slf.set_image(Some(image));

        slf
    }

    pub fn new_audio(url: &str, source: String) -> Self {
        let window = web_sys::window().unwrap();
        let document = window.document().unwrap();
        let audio = document.create_element("audio").unwrap();
        let media = audio.dyn_into::<HtmlMediaElement>().unwrap();
        media.set_src(url);

        Self::common_init(source, media)
    }

    pub fn new_video(url: &str, source: String, image: Handle<Image>) -> Self {
        let window = web_sys::window().unwrap();
        let document = window.document().unwrap();
        let video = document.create_element("video").unwrap();
        let media = video.clone().dyn_into::<HtmlMediaElement>().unwrap();
        let video = video.dyn_into::<HtmlVideoElement>().unwrap();

        let mut slf = HtmlMedia::common_init(source, media);
        set_video_source(&video, url);
        slf.video_init(video);

        slf.set_image(Some(image));

        slf
    }

    pub fn audio_from_element(media: HtmlMediaElement, source: String) -> Self {
        Self::common_init(source, media)
    }

    pub fn video_from_element(
        media: HtmlMediaElement,
        source: String,
        image: Handle<Image>,
    ) -> Self {
        let video = media.clone().dyn_into::<HtmlVideoElement>().unwrap();

        let mut slf = HtmlMedia::common_init(source, media);
        slf.video_init(video);

        slf.set_image(Some(image));

        slf
    }

    fn common_init(source: String, media: HtmlMediaElement) -> Self {
        let mut closures = Vec::default();
        let state = Arc::new(Mutex::new(VideoState::VsLoading));

        fn register_callback<'a>(
            closures: &'a mut Vec<Closure<dyn FnMut()>>,
            state: &Arc<Mutex<VideoState>>,
            new_state: VideoState,
        ) -> Option<&'a Function> {
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

    fn video_init(&mut self, video: HtmlVideoElement) {
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

        self.set_new_frame_time(frame_time);
        self.set_frame_closure(callback);
        self.set_frame_callback_handle(callback_handle);
        self.set_video(Some(video));
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

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn duration(&self) -> f64 {
        self.media.duration()
    }

    pub fn video(&self) -> Option<&HtmlVideoElement> {
        self.video.as_ref()
    }

    pub fn image(&self) -> Option<&Handle<Image>> {
        self.image.as_ref()
    }

    pub fn size(&self) -> Option<(u32, u32)> {
        self.size
    }

    pub fn current_time(&self) -> f32 {
        self.current_time
    }

    pub fn last_reported_time(&self) -> f32 {
        self.last_reported_time
    }

    pub fn new_frame_time(&self) -> &AtomicU32 {
        &self.new_frame_time
    }

    pub fn last_state(&self) -> VideoState {
        self.last_state
    }

    pub fn set_video(&mut self, video: Option<HtmlVideoElement>) {
        self.video = video;
    }

    pub fn set_size(&mut self, size: Option<(u32, u32)>) {
        self.size = size;
    }

    pub fn set_current_time(&mut self, current_time: f32) {
        self.current_time = current_time;
    }

    pub fn set_last_reported_time(&mut self, current_time: f32) {
        self.last_reported_time = current_time;
    }

    pub fn set_new_frame_time(&mut self, new_frame_time: Arc<AtomicU32>) {
        self.new_frame_time = new_frame_time;
    }

    pub fn set_last_state(&mut self, state: VideoState) {
        self.last_state = state;
    }

    pub fn set_image(&mut self, image: Option<Handle<Image>>) {
        self.image = image;
    }

    pub fn set_frame_closure(&mut self, frame_closure: RcClosure) {
        self.frame_closure = frame_closure;
    }

    pub fn set_frame_callback_handle(&mut self, frame_callback_handle: Rc<RefCell<Option<u32>>>) {
        self.frame_callback_handle = frame_callback_handle;
    }
}

impl Drop for HtmlMedia {
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
            let _ = video.pause();
        }
        self.frame_closure.take();
        self.media.set_oncanplay(None);
        self.media.set_onabort(None);
        self.media.set_onerror(None);
        self.media.set_onwaiting(None);
        self.media.set_onplaying(None);
        self.media.set_onpause(None);
        self.media.set_onended(None);
        let _ = self.media.pause();
        self.media.remove();
    }
}

#[derive(Resource, Deref, DerefMut)]
pub struct FrameCopyRequestQueue(tokio::sync::mpsc::UnboundedSender<FrameCopyRequest>);

pub struct FrameCopyRequest {
    pub video_frame: WgpuWrapper<VideoFrame>,
    pub target: AssetId<Image>,
}
