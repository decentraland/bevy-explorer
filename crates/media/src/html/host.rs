//! The page side: owns the media elements and serves [`HostCommand`]s. Runs as a task on the
//! page's event loop; nothing here may block.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use bevy::{log::debug, platform::collections::HashMap};
use common::util::ReportErr;
use dcl_component::proto_components::sdk::components::VideoState;
use futures_channel::mpsc::{UnboundedSender, unbounded};
use futures_lite::StreamExt;
use js_sys::{Function, Reflect};
use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};
use wasm_bindgen_futures::JsFuture;
use web_sys::{HtmlMediaElement, HtmlVideoElement, VideoFrame, Worker};

use super::{HOST, HostCommand, HostHandle, MediaEvent, MediaId};
use crate::AVCommand;

type RcClosure = Rc<RefCell<Option<Closure<dyn FnMut(f64, JsValue)>>>>;

// Defined by engine.js on the page (HLS-aware source assignment).
#[wasm_bindgen(js_namespace = window)]
extern "C" {
    #[wasm_bindgen(js_name = setVideoSource)]
    fn set_video_source(elt: &HtmlVideoElement, src: &str);
}

thread_local! {
    /// The page's media elements by id; other page code may add to it ([`adopt_video_element`]).
    static MEDIA: RefCell<HashMap<MediaId, HostMedia>> = RefCell::new(HashMap::default());
    /// The event sender and render worker, set by [`media_host_main`].
    static PAGE: RefCell<Option<(UnboundedSender<MediaEvent>, Worker)>> = const { RefCell::new(None) };
}

/// Page entry: starts serving media commands, transferring video frames to `render_worker`.
/// Call after the module is initialised on the page.
pub fn media_host_main(render_worker: Worker) {
    let (commands_tx, mut commands_rx) = unbounded::<HostCommand>();
    let (events_tx, events_rx) = unbounded::<MediaEvent>();
    if HOST
        .set(HostHandle {
            commands: commands_tx,
            events: std::sync::Mutex::new(Some(events_rx)),
        })
        .is_err()
    {
        panic!("media host already started");
    }
    PAGE.with(|page| *page.borrow_mut() = Some((events_tx.clone(), render_worker.clone())));

    // A `play()` the browser refused (no user gesture yet) is retried on the next gesture.
    let document = web_sys::window().unwrap().document().unwrap();
    let on_gesture = Closure::<dyn FnMut()>::new(|| {
        MEDIA.with(|media| {
            for m in media.borrow().values() {
                if m.wants_play.get() && m.play_refused.get() {
                    try_play(&m.media, &m.play_refused);
                }
            }
        });
    });
    for event in ["pointerdown", "keydown"] {
        document
            .add_event_listener_with_callback(event, on_gesture.as_ref().unchecked_ref())
            .report();
    }
    on_gesture.forget();

    wasm_bindgen_futures::spawn_local(async move {
        while let Some(command) = commands_rx.next().await {
            MEDIA.with(|media| {
                let mut media = media.borrow_mut();
                match command {
                    HostCommand::Create { id, url, has_video } => {
                        media.insert(
                            id,
                            HostMedia::new(
                                id,
                                url,
                                has_video,
                                events_tx.clone(),
                                render_worker.clone(),
                            ),
                        );
                    }
                    HostCommand::Command(id, command) => {
                        if let Some(m) = media.get(&id) {
                            m.command(command);
                        }
                    }
                    HostCommand::Drop(id) => {
                        media.remove(&id);
                    }
                }
            });
        }
    });
}

/// Registers an existing page `<video>` element (e.g. one a livekit track is attached to) as the
/// media `id` ([`super::adopt_video`]), so its frames flow to the render worker like any other
/// media. The host owns the element from here.
pub fn adopt_video_element(id: MediaId, element: HtmlVideoElement) {
    let Some((events, render_worker)) = PAGE.with(|page| page.borrow().clone()) else {
        debug!("no media host: cannot adopt video element {id}");
        return;
    };
    let media = HostMedia::from_element(id, element, None, true, events, render_worker);
    MEDIA.with(|m| m.borrow_mut().insert(id, media));
}

/// Calls `play()`, noting a refusal (autoplay policy) for the gesture retry.
fn try_play(media: &HtmlMediaElement, refused: &Rc<Cell<bool>>) {
    let Ok(promise) = media.play() else {
        return;
    };
    let refused = refused.clone();
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = JsFuture::from(promise).await {
            debug!("play refused: {e:?}");
            refused.set(true);
        }
    });
}

struct HostMedia {
    media: HtmlMediaElement,
    video: HtmlVideoElement,
    /// The engine's last word was play (cleared when the media ends).
    wants_play: Rc<Cell<bool>>,
    /// The last `play()` was refused.
    play_refused: Rc<Cell<bool>>,
    _closures: Vec<Closure<dyn FnMut()>>,
    frame_closure: RcClosure,
    frame_callback_handle: Rc<RefCell<Option<u32>>>,
}

impl HostMedia {
    fn new(
        id: MediaId,
        url: Option<String>,
        has_video: bool,
        events: UnboundedSender<MediaEvent>,
        render_worker: Worker,
    ) -> Self {
        let document = web_sys::window().unwrap().document().unwrap();
        let video = document
            .create_element("video")
            .unwrap()
            .dyn_into::<HtmlVideoElement>()
            .unwrap();
        Self::from_element(id, video, url, has_video, events, render_worker)
    }

    fn from_element(
        id: MediaId,
        video: HtmlVideoElement,
        url: Option<String>,
        has_video: bool,
        events: UnboundedSender<MediaEvent>,
        render_worker: Worker,
    ) -> Self {
        let media = video.clone().dyn_into::<HtmlMediaElement>().unwrap();
        let wants_play = Rc::new(Cell::new(false));
        let play_refused = Rc::new(Cell::new(false));

        let mut closures = Vec::default();
        let mut register = |new_state: VideoState| -> Function {
            let events = events.clone();
            let media = media.clone();
            let wants_play = wants_play.clone();
            let play_refused = play_refused.clone();
            let closure = Closure::wrap(Box::new(move || {
                debug!("media {id} state -> {new_state:?}");
                let _ = events.unbounded_send(MediaEvent::State {
                    id,
                    state: new_state,
                    duration: media.duration(),
                });
                match new_state {
                    // a `play()` issued before the source could play takes effect now
                    VideoState::VsReady if wants_play.get() && media.paused() => {
                        try_play(&media, &play_refused);
                    }
                    VideoState::VsPlaying => play_refused.set(false),
                    // `play()` on an ended element restarts it: only the engine may ask
                    VideoState::VsPaused if media.ended() => wants_play.set(false),
                    _ => (),
                }
            }) as Box<dyn FnMut()>);
            let function = closure.as_ref().unchecked_ref::<Function>().clone();
            closures.push(closure);
            function
        };
        media.set_oncanplay(Some(&register(VideoState::VsReady)));
        media.set_onabort(Some(&register(VideoState::VsError)));
        media.set_onerror(Some(&register(VideoState::VsError)));
        media.set_onwaiting(Some(&register(VideoState::VsBuffering)));
        media.set_onseeking(Some(&register(VideoState::VsSeeking)));
        media.set_onplaying(Some(&register(VideoState::VsPlaying)));
        media.set_onpause(Some(&register(VideoState::VsPaused)));
        media.set_onended(Some(&register(VideoState::VsPaused)));

        let mut slf = Self {
            media,
            video: video.clone(),
            wants_play,
            play_refused,
            _closures: closures,
            frame_closure: Default::default(),
            frame_callback_handle: Default::default(),
        };

        if let Some(url) = &url {
            set_video_source(&video, url);
        }
        // only a video's frames are read, which needs a CORS load: an audio stream from a server
        // without CORS headers still plays
        if has_video {
            slf.video_init(id, video, events, render_worker);
        }

        slf
    }

    fn command(&self, command: AVCommand) {
        match command {
            AVCommand::Play => {
                self.wants_play.set(true);
                try_play(&self.media, &self.play_refused);
            }
            AVCommand::Pause => {
                self.wants_play.set(false);
                self.media.pause().report();
            }
            AVCommand::Repeat(looping) => self.media.set_loop(looping),
            AVCommand::Seek(time) => self.media.set_current_time(time),
            AVCommand::Volume(volume) => self.media.set_volume(volume.clamp(0.0, 1.0) as f64),
            // dispose arrives as `HostCommand::Drop`
            AVCommand::Dispose => (),
        }
    }

    fn video_init(
        &mut self,
        id: MediaId,
        video: HtmlVideoElement,
        events: UnboundedSender<MediaEvent>,
        render_worker: Worker,
    ) {
        video.set_cross_origin(Some("anonymous"));

        // video frame callback - no wasm_bindgen for this!
        let rvc_prop = Reflect::get(&video, &"requestVideoFrameCallback".into()).unwrap();
        if rvc_prop.is_undefined() {
            panic!("no requestVideoFrameCallback");
        }
        let rvc_fn = rvc_prop.dyn_into::<Function>().unwrap();

        let callback: RcClosure = Rc::new(RefCell::new(None));
        let callback_handle: Rc<RefCell<Option<u32>>> = Rc::new(RefCell::new(None));
        let callback_clone = callback.clone();
        let handle_clone = callback_handle.clone();
        let rvc_clone = rvc_fn.clone();

        *callback.borrow_mut() = Some(Closure::wrap(Box::new({
            let video = video.clone();
            move |_now: f64, metadata: JsValue| {
                let media_time = Reflect::get(&metadata, &"mediaTime".into())
                    .ok()
                    .and_then(|mt| mt.as_f64())
                    .unwrap_or_default();
                match VideoFrame::new_with_html_video_element(&video) {
                    Ok(frame) => {
                        let visible_rect = frame.visible_rect().unwrap();
                        let (width, height) =
                            (visible_rect.width() as u32, visible_rect.height() as u32);
                        let message = js_sys::Object::new();
                        Reflect::set(&message, &"type".into(), &"VIDEO_FRAME".into()).unwrap();
                        Reflect::set(&message, &"id".into(), &JsValue::from(id)).unwrap();
                        Reflect::set(&message, &"frame".into(), &frame).unwrap();
                        let transfer = js_sys::Array::of1(&frame);
                        if render_worker
                            .post_message_with_transfer(&message, &transfer)
                            .is_ok()
                        {
                            let _ = events.unbounded_send(MediaEvent::Frame {
                                id,
                                media_time,
                                width,
                                height,
                            });
                        } else {
                            frame.close();
                        }
                    }
                    Err(e) => debug!("media {id}: no frame: {e:?}"),
                }

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

        self.frame_closure = callback;
        self.frame_callback_handle = callback_handle;
    }
}

impl Drop for HostMedia {
    fn drop(&mut self) {
        debug!("shutdown");
        if let Some(handle) = self.frame_callback_handle.borrow_mut().take() {
            Reflect::get(&self.video, &"cancelVideoFrameCallback".into())
                .unwrap()
                .dyn_into::<Function>()
                .unwrap()
                .call1(&self.video, &JsValue::from(handle))
                .unwrap();
        }
        self.frame_closure.take();
        self.media.set_oncanplay(None);
        self.media.set_onabort(None);
        self.media.set_onerror(None);
        self.media.set_onwaiting(None);
        self.media.set_onseeking(None);
        self.media.set_onplaying(None);
        self.media.set_onpause(None);
        self.media.set_onended(None);
        let _ = self.media.pause();
        self.media.remove();
    }
}
