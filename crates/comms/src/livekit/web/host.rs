//! The page side: owns the livekit-client objects and serves [`LivekitCommand`]s. Runs as a
//! task on the page's event loop; nothing here may block.

use std::{cell::RefCell, rc::Rc};

use bevy::{log::debug, platform::collections::HashMap, prelude::error};
use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_lite::StreamExt;
use js_sys::{Object, Reflect};
use wasm_bindgen::{closure::Closure, prelude::wasm_bindgen, JsCast, JsValue};
use web_sys::HtmlVideoElement;

use super::{
    AudioCaptureOptions, ConnectedRoom, DataPacket, HostHandle, LivekitCommand, LivekitEvent,
    LocalAudioTrack, LocalParticipant, LocalTrackPublication, ObjectId, Participant,
    ParticipantIdentity, ParticipantSid, RemoteAudioTrack, RemoteParticipant, RemoteTrack,
    RemoteTrackPublication, RemoteVideoTrack, Reply, RequestId, RoomEvent, RoomId, RoomOptions,
    TrackKind, TrackPublishOptions, TrackSid, TrackSource, HOST,
};

#[wasm_bindgen(module = "/livekit_web_bindings.js")]
extern "C" {
    #[wasm_bindgen(js_name = "setupMicrophonePermission")]
    fn setup_microphone_permission(on_change: &Closure<dyn FnMut(String)>);
    fn is_microphone_available() -> bool;
    #[wasm_bindgen(js_name = "promptMicrophonePermission")]
    fn prompt_microphone_permission();

    #[wasm_bindgen(catch)]
    async fn room_connect(
        url: &str,
        token: &str,
        room_options: JsValue,
        room_connect_options: JsValue,
        handler: &Closure<dyn FnMut(JsValue)>,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch)]
    async fn room_close(room: &JsValue) -> Result<(), JsValue>;
    fn room_drop(room: &JsValue);
    fn room_name(room: &JsValue) -> String;
    fn room_local_participant(room: &JsValue) -> JsValue;

    #[wasm_bindgen(catch)]
    async fn local_participant_publish_data(
        local_participant: &JsValue,
        data: &[u8],
        reliable: bool,
        topic: Option<String>,
        destination_identities: Vec<String>,
    ) -> Result<(), JsValue>;
    #[wasm_bindgen(catch)]
    async fn local_participant_publish_track(
        local_participant: &JsValue,
        local_track: &JsValue,
        track_publish_options: JsValue,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch)]
    async fn local_participant_unpublish_track(
        local_participant: &JsValue,
        local_track: &JsValue,
    ) -> Result<JsValue, JsValue>;
    fn local_participant_sid(local_participant: &JsValue) -> String;
    fn local_participant_identity(local_participant: &JsValue) -> String;
    fn local_participant_metadata(local_participant: &JsValue) -> Option<String>;

    fn remote_participant_is_local(remote_participant: &JsValue) -> bool;
    fn remote_participant_sid(remote_participant: &JsValue) -> String;
    fn remote_participant_identity(remote_participant: &JsValue) -> String;
    fn remote_participant_metadata(remote_participant: &JsValue) -> Option<String>;

    fn remote_track_publication_sid(remote_track_publication: &JsValue) -> String;
    fn remote_track_publication_kind(remote_track_publication: &JsValue) -> String;
    fn remote_track_publication_source(remote_track_publication: &JsValue) -> String;
    fn remote_track_publication_set_subscribed(
        remote_track_publication: &JsValue,
        subscribed: bool,
    );
    fn remote_track_publication_track(remote_track_publication: &JsValue) -> JsValue;

    async fn local_audio_track_new(options: JsValue) -> JsValue;

    fn local_track_publication_sid(local_track_publication: &JsValue) -> String;
    fn local_track_publication_kind(local_track_publication: &JsValue) -> String;
    fn local_track_publication_source(local_track_publication: &JsValue) -> String;

    fn remote_track_pan_and_volume(remote_track: &JsValue, pan: f32, volume: f32);
    fn remote_audio_track_set_volume(remote_audio_track: &JsValue, volume: f32);
}

/// The property the page stamps on livekit objects with their [`ObjectId`].
const ID_PROPERTY: &str = "__bevyLivekitId";

/// The page's livekit objects by id, plus snapshots of them for the engine.
#[derive(Default)]
pub(super) struct Registry {
    next_id: ObjectId,
    /// Remote participants, publications, remote tracks and local tracks.
    objects: HashMap<ObjectId, JsValue>,
}

impl Registry {
    /// The object's id, allocated (and stamped on it) on first sight.
    fn id_of(&mut self, object: &JsValue) -> ObjectId {
        let id = Reflect::get(object, &ID_PROPERTY.into())
            .ok()
            .and_then(|id| id.as_f64())
            .map(|id| id as ObjectId)
            .unwrap_or_else(|| {
                self.next_id += 1;
                let _ = Reflect::set(object, &ID_PROPERTY.into(), &JsValue::from(self.next_id));
                self.next_id
            });
        self.objects.entry(id).or_insert_with(|| object.clone());
        id
    }

    /// A clone (the registry's borrow must not be held while calling into livekit-client, whose
    /// events re-enter it).
    fn get(&self, id: ObjectId) -> Option<JsValue> {
        let object = self.objects.get(&id).cloned();
        if object.is_none() {
            debug!("unknown livekit object {id}");
        }
        object
    }

    pub(super) fn release(&mut self, id: ObjectId) {
        self.objects.remove(&id);
    }

    pub(super) fn remote_participant(&mut self, object: &JsValue) -> RemoteParticipant {
        RemoteParticipant {
            id: self.id_of(object),
            sid: ParticipantSid(remote_participant_sid(object)),
            identity: ParticipantIdentity(remote_participant_identity(object)),
            metadata: remote_participant_metadata(object).unwrap_or_default(),
        }
    }

    pub(super) fn participant(&mut self, object: &JsValue, room: RoomId) -> Participant {
        if remote_participant_is_local(object) {
            Participant::Local(local_participant(object, room))
        } else {
            Participant::Remote(self.remote_participant(object))
        }
    }

    pub(super) fn remote_track_publication(&mut self, object: &JsValue) -> RemoteTrackPublication {
        let track = remote_track_publication_track(object);
        let track = (!(track.is_null() || track.is_undefined()))
            .then(|| self.remote_track(&track))
            .flatten();
        RemoteTrackPublication {
            id: self.id_of(object),
            sid: remote_track_publication_sid(object),
            kind: TrackKind::from_js_str(&remote_track_publication_kind(object)),
            source: TrackSource::from_js_str(&remote_track_publication_source(object)),
            track,
        }
    }

    pub(super) fn remote_track(&mut self, object: &JsValue) -> Option<RemoteTrack> {
        let id = self.id_of(object);
        let sid = get_string(object, "sid").unwrap_or_default();
        match get_string(object, "kind").as_deref() {
            Some("audio") => Some(RemoteTrack::Audio(RemoteAudioTrack { id, sid })),
            Some("video") => Some(RemoteTrack::Video(RemoteVideoTrack { id, sid })),
            Some(other) => {
                error!("Unknown RemoteTrack kind {other}.");
                None
            }
            None => {
                error!("RemoteTrack did not have kind field.");
                None
            }
        }
    }
}

fn get_string(object: &JsValue, key: &str) -> Option<String> {
    Reflect::get(object, &key.into())
        .ok()
        .and_then(|value| value.as_string())
}

fn local_participant(object: &JsValue, room: RoomId) -> LocalParticipant {
    LocalParticipant {
        room,
        sid: ParticipantSid(local_participant_sid(object)),
        identity: ParticipantIdentity(local_participant_identity(object)),
        metadata: local_participant_metadata(object).unwrap_or_default(),
    }
}

fn local_track_publication(object: &JsValue) -> LocalTrackPublication {
    LocalTrackPublication {
        sid: local_track_publication_sid(object),
        kind: TrackKind::from_js_str(&local_track_publication_kind(object)),
        source: TrackSource::from_js_str(&local_track_publication_source(object)),
    }
}

fn js_error(err: JsValue) -> String {
    format!("{err:?}")
}

struct HostRoom {
    room: JsValue,
    _handler: Closure<dyn FnMut(JsValue)>,
}

#[derive(Default)]
struct Host {
    registry: Registry,
    rooms: HashMap<RoomId, HostRoom>,
}

type SharedHost = Rc<RefCell<Host>>;

/// Page entry: starts serving livekit commands. Call after the module is initialised on the
/// page and before the engine worker starts.
pub fn livekit_host_main() {
    let (commands_tx, mut commands_rx) = unbounded::<LivekitCommand>();
    let (events_tx, events_rx) = unbounded::<LivekitEvent>();
    if HOST
        .set(HostHandle {
            commands: commands_tx,
            events: std::sync::Mutex::new(Some(events_rx)),
        })
        .is_err()
    {
        panic!("livekit host already started");
    }

    wasm_bindgen_futures::spawn_local(async move {
        let host: SharedHost = Rc::default();
        while let Some(command) = commands_rx.next().await {
            serve(command, &host, &events_tx);
        }
    });
}

fn reply(events: &UnboundedSender<LivekitEvent>, request: RequestId, reply: Reply) {
    let _ = events.unbounded_send(LivekitEvent::Reply { request, reply });
}

fn serve(command: LivekitCommand, host: &SharedHost, events: &UnboundedSender<LivekitEvent>) {
    match command {
        LivekitCommand::Connect {
            request,
            room,
            url,
            token,
            options,
        } => {
            let host = host.clone();
            let events = events.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let result = connect(&host, &events, room, &url, &token, &options).await;
                reply(&events, request, Reply::Connect(result));
            });
        }
        LivekitCommand::Close { request, room } => {
            let js_room = host.borrow().rooms.get(&room).map(|r| r.room.clone());
            let events = events.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let result = match js_room {
                    Some(js_room) => room_close(&js_room).await.map_err(js_error),
                    None => Err(format!("unknown room {room}")),
                };
                reply(&events, request, Reply::Close(result));
            });
        }
        LivekitCommand::DropRoom(room) => {
            let host_room = host.borrow_mut().rooms.remove(&room);
            if let Some(host_room) = host_room {
                room_drop(&host_room.room);
            }
        }
        LivekitCommand::PublishData {
            request,
            room,
            packet,
        } => {
            let local_participant = host
                .borrow()
                .rooms
                .get(&room)
                .map(|r| room_local_participant(&r.room));
            let events = events.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let result = match local_participant {
                    Some(local_participant) => {
                        let DataPacket {
                            payload,
                            topic,
                            reliable,
                            destination_identities,
                        } = packet;
                        let destination_identities = destination_identities
                            .into_iter()
                            .map(|participant_identity| participant_identity.0)
                            .collect();
                        local_participant_publish_data(
                            &local_participant,
                            &payload,
                            reliable,
                            topic,
                            destination_identities,
                        )
                        .await
                        .map_err(js_error)
                    }
                    None => Err(format!("unknown room {room}")),
                };
                reply(&events, request, Reply::PublishData(result));
            });
        }
        LivekitCommand::SetSubscribed {
            publication,
            subscribed,
        } => {
            let publication = host.borrow().registry.get(publication);
            if let Some(publication) = publication {
                remote_track_publication_set_subscribed(&publication, subscribed);
            }
        }
        LivekitCommand::PanAndVolume { track, pan, volume } => {
            let track = host.borrow().registry.get(track);
            if let Some(track) = track {
                remote_track_pan_and_volume(&track, pan, volume);
            }
        }
        LivekitCommand::SetVolume { track, volume } => {
            let track = host.borrow().registry.get(track);
            if let Some(track) = track {
                remote_audio_track_set_volume(&track, volume);
            }
        }
        LivekitCommand::AttachVideo { track, media_id } => {
            let element = host
                .borrow()
                .registry
                .get(track)
                .and_then(|track| Reflect::get(&track, &"videoElement".into()).ok())
                .and_then(|element| element.dyn_into::<HtmlVideoElement>().ok());
            match element {
                Some(element) => media::host::adopt_video_element(media_id, element),
                None => error!("remote video track {track} has no video element to attach"),
            }
        }
        LivekitCommand::CreateLocalAudioTrack { request, options } => {
            let host = host.clone();
            let events = events.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let track = local_audio_track_new(audio_capture_options(&options)).await;
                let result = if track.is_null() || track.is_undefined() {
                    Err("could not create the local audio track".to_owned())
                } else {
                    let id = host.borrow_mut().registry.id_of(&track);
                    Ok(LocalAudioTrack {
                        id,
                        // assigned when the track is published
                        sid: TrackSid(get_string(&track, "sid").unwrap_or_default()),
                    })
                };
                reply(&events, request, Reply::LocalAudioTrack(result));
            });
        }
        LivekitCommand::PublishTrack {
            request,
            room,
            track,
            options,
        } => {
            let (local_participant, track) = local_participant_and_track(host, room, track);
            let events = events.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let result = match (local_participant, track) {
                    (Ok(local_participant), Ok(track)) => local_participant_publish_track(
                        &local_participant,
                        &track,
                        track_publish_options(&options),
                    )
                    .await
                    .map(|publication| local_track_publication(&publication))
                    .map_err(js_error),
                    (Err(err), _) | (_, Err(err)) => Err(err),
                };
                reply(&events, request, Reply::Publish(result));
            });
        }
        LivekitCommand::UnpublishTrack {
            request,
            room,
            track,
        } => {
            let (local_participant, js_track) = local_participant_and_track(host, room, track);
            let host = host.clone();
            let events = events.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let result = match (local_participant, js_track) {
                    (Ok(local_participant), Ok(js_track)) => {
                        let result =
                            local_participant_unpublish_track(&local_participant, &js_track)
                                .await
                                .map(|publication| local_track_publication(&publication))
                                .map_err(js_error);
                        // stopLocalTrackOnUnpublish: the track is done
                        host.borrow_mut().registry.release(track);
                        result
                    }
                    (Err(err), _) | (_, Err(err)) => Err(err),
                };
                reply(&events, request, Reply::Publish(result));
            });
        }
        LivekitCommand::SetupMicrophonePermission => {
            let events = events.clone();
            let on_change = Closure::new(move |permission: String| {
                let _ = events.unbounded_send(LivekitEvent::Microphone {
                    available: is_microphone_available(),
                    permission,
                });
            });
            setup_microphone_permission(&on_change);
            // lives as long as the page
            on_change.forget();
        }
        LivekitCommand::PromptMicrophonePermission => prompt_microphone_permission(),
    }
}

async fn connect(
    host: &SharedHost,
    events: &UnboundedSender<LivekitEvent>,
    room: RoomId,
    url: &str,
    token: &str,
    options: &RoomOptions,
) -> Result<ConnectedRoom, String> {
    let handler = Closure::new({
        let host = host.clone();
        let events = events.clone();
        move |room_event: JsValue| {
            let event = RoomEvent::from_js(&room_event, room, &mut host.borrow_mut().registry);
            if let Some(event) = event {
                let _ = events.unbounded_send(LivekitEvent::Room { room, event });
            }
        }
    });

    let js_room = room_connect(
        url,
        token,
        build_room_options(options),
        build_room_connect_options(options),
        &handler,
    )
    .await
    .map_err(js_error)?;

    let connected = ConnectedRoom {
        name: room_name(&js_room),
        local_participant: local_participant(&room_local_participant(&js_room), room),
    };
    host.borrow_mut().rooms.insert(
        room,
        HostRoom {
            room: js_room,
            _handler: handler,
        },
    );
    Ok(connected)
}

fn local_participant_and_track(
    host: &SharedHost,
    room: RoomId,
    track: ObjectId,
) -> (Result<JsValue, String>, Result<JsValue, String>) {
    let host = host.borrow();
    (
        host.rooms
            .get(&room)
            .map(|r| room_local_participant(&r.room))
            .ok_or_else(|| format!("unknown room {room}")),
        host.registry
            .get(track)
            .ok_or_else(|| format!("unknown local track {track}")),
    )
}

fn set(object: &Object, key: &str, value: impl Into<JsValue>) {
    let _ = Reflect::set(object, &JsValue::from_str(key), &value.into());
}

fn build_room_options(room_options: &RoomOptions) -> JsValue {
    let object = Object::new();
    set(&object, "adaptiveStream", room_options.adaptive_stream);
    set(&object, "dynacast", room_options.dynacast);
    set(&object, "stopLocalTrackOnUnpublish", true);
    set(&object, "disconnectOnPageLeave", true);
    set(&object, "webAudioMix", true);
    object.into()
}

fn build_room_connect_options(room_options: &RoomOptions) -> JsValue {
    let object = Object::new();
    set(&object, "autoSubscribe", room_options.auto_subscribe);
    object.into()
}

fn track_publish_options(options: &TrackPublishOptions) -> JsValue {
    let object = Object::new();
    if let Some(simulcast) = options.simulcast {
        set(&object, "simulcast", simulcast);
    }
    object.into()
}

fn audio_capture_options(options: &AudioCaptureOptions) -> JsValue {
    let object = Object::new();
    if let Some(v) = options.auto_gain_control {
        set(&object, "autoGainControl", v);
    }
    if let Some(v) = options.channel_count {
        set(&object, "channelCount", v as f64);
    }
    if let Some(v) = options.echo_cancellation {
        set(&object, "echoCancellation", v);
    }
    if let Some(v) = options.latency {
        set(&object, "latency", v);
    }
    if let Some(v) = options.noise_suppression {
        set(&object, "noiseSuppression", v);
    }
    if let Some(v) = options.voice_isolation {
        set(&object, "voiceIsolation", v);
    }
    if let Some(v) = options.sample_rate {
        set(&object, "sampleRate", v as f64);
    }
    if let Some(v) = options.sample_size {
        set(&object, "sampleSize", v as f64);
    }
    object.into()
}
