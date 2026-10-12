//! Livekit on the web: the page owns the livekit-client objects (the engine runs on a worker,
//! which has no WebRTC) and the engine drives them through plain-data handles keyed by page
//! ids. Commands go engine → page and events/replies come back over `futures_channel`
//! (lock-free: the page must never block). Replies to async requests and room events are
//! dispatched into engine-local tokio channels by [`dispatch_page_events`] each frame, so the
//! page never touches a tokio waker.

pub mod host;
mod local_audio_track;
mod local_participant;
mod local_track;
mod local_track_publication;
mod participant;
mod remote_audio_track;
mod remote_participant;
mod remote_track;
mod remote_track_publication;
mod remote_video_track;
mod room;
mod room_event;
mod track_sid;
mod track_source;

use std::{
    error::Error,
    fmt::{Display, Formatter},
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex,
    },
};

use bevy::{platform::collections::HashMap, prelude::*};
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender};
use media::MediaId;
use once_cell::sync::OnceCell;
use tokio::sync::{mpsc, oneshot};

pub use crate::livekit::web::{
    local_audio_track::LocalAudioTrack, local_participant::LocalParticipant,
    local_track::LocalTrack, local_track_publication::LocalTrackPublication,
    participant::Participant, remote_audio_track::RemoteAudioTrack,
    remote_participant::RemoteParticipant, remote_track::RemoteTrack,
    remote_track_publication::RemoteTrackPublication, remote_video_track::RemoteVideoTrack,
    room::Room, room_event::RoomEvent, track_sid::TrackSid, track_source::TrackSource,
};

/// Engine-allocated id of a room.
pub type RoomId = u32;
/// Page-allocated id of a participant / publication / track object.
pub type ObjectId = u32;
pub type RequestId = u32;

pub type RoomResult<T> = Result<T, RoomError>;

#[derive(Debug, Default, Clone)]
pub struct RoomOptions {
    pub auto_subscribe: bool,
    pub adaptive_stream: bool,
    pub dynacast: bool,
    // pub e2ee: Option<E2eeOptions>,
    // pub rtc_config: RtcConfiguration,
    pub join_retries: u32,
}

#[derive(Debug, Clone)]
pub struct RoomError(pub String);

impl Display for RoomError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for RoomError {}

#[derive(Debug, Clone)]
pub struct DataPacket {
    pub payload: Vec<u8>,
    pub topic: Option<String>,
    pub reliable: bool,
    pub destination_identities: Vec<ParticipantIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deref)]
pub struct ParticipantIdentity(pub String);

impl std::fmt::Display for ParticipantIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Audio,
    Video,
}

impl TrackKind {
    fn from_js_str(kind: &str) -> Self {
        match kind {
            "audio" => Self::Audio,
            "video" => Self::Video,
            other => {
                error!("TrackKind was not a known kind. Was '{other}'. Assuming Audio.");
                Self::Audio
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deref)]
pub struct ParticipantSid(String);

impl std::fmt::Display for ParticipantSid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Kind of the packet.
///
/// Keep in track with
/// [https://github.com/livekit/protocol/blob/e7532dfc617d0c920eb905a93b6ca0d3ca4033e9/protobufs/livekit_models.proto#L324]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataPacketKind {
    Reliable = 0,
    Lossy = 1,
}

#[derive(Debug)]
pub enum ConnectionState {
    Connected,
    Reconnecting,
    Disconnected,
}

#[derive(Debug, Clone)]
pub struct LocalVideoTrack {
    id: ObjectId,
}

#[derive(Debug)]
pub struct TrackPublishOptions {
    // pub video_encoding: Option<VideoEncoding>,
    // pub audio_encoding: Option<AudioEncoding>,
    // pub video_codec: VideoCodec,
    // pub dtx: bool,
    // pub red: bool,
    pub simulcast: Option<bool>,
    pub source: TrackSource,
    pub stream: Option<String>,
    pub preconnect_buffer: Option<bool>,
}

impl Default for TrackPublishOptions {
    fn default() -> Self {
        Self {
            // video_encoding: None,
            // audio_encoding: None,
            // video_codec: VideoCodec::VP8,
            // dtx: true,
            // red: true,
            simulcast: Some(true),
            source: TrackSource::Unknown,
            stream: Some("".to_string()),
            preconnect_buffer: Some(false),
        }
    }
}

#[derive(Debug, Default)]
pub struct AudioCaptureOptions {
    pub auto_gain_control: Option<bool>,
    pub channel_count: Option<u64>,
    pub echo_cancellation: Option<bool>,
    pub latency: Option<f64>,
    pub noise_suppression: Option<bool>,
    pub voice_isolation: Option<bool>,
    pub sample_rate: Option<u64>,
    pub sample_size: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionQuality {
    Excellent,
    Good,
    Poor,
    Lost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(i32)]
pub enum DisconnectReason {
    UnknownReason = 0,
    /// the client initiated the disconnect
    ClientInitiated = 1,
    /// another participant with the same identity has joined the room
    DuplicateIdentity = 2,
    /// the server instance is shutting down
    ServerShutdown = 3,
    /// RoomService.RemoveParticipant was called
    ParticipantRemoved = 4,
    /// RoomService.DeleteRoom was called
    RoomDeleted = 5,
    /// the client is attempting to resume a session, but server is not aware of it
    StateMismatch = 6,
    /// client was unable to connect fully
    JoinFailure = 7,
    /// Cloud-only, the server requested Participant to migrate the connection elsewhere
    Migration = 8,
    /// the signal websocket was closed unexpectedly
    SignalClose = 9,
    /// the room was closed, due to all Standard and Ingress participants having left
    RoomClosed = 10,
}

impl DisconnectReason {
    fn from_i32(int: i32) -> Option<Self> {
        Some(match int {
            0 => DisconnectReason::UnknownReason,
            1 => DisconnectReason::ClientInitiated,
            2 => DisconnectReason::DuplicateIdentity,
            3 => DisconnectReason::ServerShutdown,
            4 => DisconnectReason::ParticipantRemoved,
            5 => DisconnectReason::RoomDeleted,
            6 => DisconnectReason::StateMismatch,
            7 => DisconnectReason::JoinFailure,
            8 => DisconnectReason::Migration,
            9 => DisconnectReason::SignalClose,
            10 => DisconnectReason::RoomClosed,
            _ => return None,
        })
    }
}

/// Engine → page.
#[derive(Debug)]
pub enum LivekitCommand {
    Connect {
        request: RequestId,
        room: RoomId,
        url: String,
        token: String,
        options: RoomOptions,
    },
    Close {
        request: RequestId,
        room: RoomId,
    },
    /// The engine dropped its handle: release the livekit room.
    DropRoom(RoomId),
    PublishData {
        request: RequestId,
        room: RoomId,
        packet: DataPacket,
    },
    SetSubscribed {
        publication: ObjectId,
        subscribed: bool,
    },
    PanAndVolume {
        track: ObjectId,
        pan: f32,
        volume: f32,
    },
    SetVolume {
        track: ObjectId,
        volume: f32,
    },
    /// Hands the remote video track's element to the media host as media `media_id`.
    AttachVideo {
        track: ObjectId,
        media_id: MediaId,
    },
    CreateLocalAudioTrack {
        request: RequestId,
        options: AudioCaptureOptions,
    },
    PublishTrack {
        request: RequestId,
        room: RoomId,
        track: ObjectId,
        options: TrackPublishOptions,
    },
    UnpublishTrack {
        request: RequestId,
        room: RoomId,
        track: ObjectId,
    },
    SetupMicrophonePermission,
    PromptMicrophonePermission,
}

/// Page → engine.
#[derive(Debug)]
pub enum LivekitEvent {
    Reply {
        request: RequestId,
        reply: Reply,
    },
    Room {
        room: RoomId,
        event: RoomEvent,
    },
    /// The browser's microphone availability / permission (`"granted"`, `"prompt"`,
    /// `"denied"`) changed.
    Microphone {
        available: bool,
        permission: String,
    },
}

/// The page's answer to a request.
#[derive(Debug)]
pub enum Reply {
    Connect(Result<ConnectedRoom, String>),
    Close(Result<(), String>),
    PublishData(Result<(), String>),
    LocalAudioTrack(Result<LocalAudioTrack, String>),
    Publish(Result<LocalTrackPublication, String>),
}

#[derive(Debug)]
pub struct ConnectedRoom {
    pub name: String,
    pub local_participant: LocalParticipant,
}

/// Set by the page ([`host::livekit_host_main`]) before the engine starts.
struct HostHandle {
    commands: UnboundedSender<LivekitCommand>,
    events: Mutex<Option<UnboundedReceiver<LivekitEvent>>>,
}

static HOST: OnceCell<HostHandle> = OnceCell::new();
static NEXT_ROOM_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_REQUEST_ID: AtomicU32 = AtomicU32::new(1);
/// Engine-side: requests awaiting their [`Reply`].
static PENDING: Mutex<Option<HashMap<RequestId, oneshot::Sender<Reply>>>> = Mutex::new(None);
/// Engine-side: each room's event queue (the receiver is what [`Room::connect`] returns).
static ROOM_EVENTS: Mutex<Option<HashMap<RoomId, mpsc::UnboundedSender<RoomEvent>>>> =
    Mutex::new(None);

fn send_command(command: LivekitCommand) {
    match HOST.get() {
        Some(host) => {
            let _ = host.commands.unbounded_send(command);
        }
        None => debug!("no livekit host: dropping {command:?}"),
    }
}

/// Sends the command built for a fresh request id and awaits the page's reply.
async fn request(command: impl FnOnce(RequestId) -> LivekitCommand) -> RoomResult<Reply> {
    let Some(host) = HOST.get() else {
        return Err(RoomError("no livekit host".to_owned()));
    };
    let request = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let (sender, receiver) = oneshot::channel();
    PENDING
        .lock()
        .unwrap()
        .get_or_insert_default()
        .insert(request, sender);
    if host.commands.unbounded_send(command(request)).is_err() {
        return Err(RoomError("livekit host is gone".to_owned()));
    }
    receiver
        .await
        .map_err(|_| RoomError("livekit host dropped the request".to_owned()))
}

fn register_room_events(room: RoomId) -> mpsc::UnboundedReceiver<RoomEvent> {
    let (sender, receiver) = mpsc::unbounded_channel();
    ROOM_EVENTS
        .lock()
        .unwrap()
        .get_or_insert_default()
        .insert(room, sender);
    receiver
}

fn unregister_room_events(room: RoomId) {
    if let Some(rooms) = ROOM_EVENTS.lock().unwrap().as_mut() {
        rooms.remove(&room);
    }
}

pub fn setup_microphone_permission() {
    send_command(LivekitCommand::SetupMicrophonePermission);
}

pub fn prompt_microphone_permission() {
    send_command(LivekitCommand::PromptMicrophonePermission);
}

/// The browser's latest word on the microphone, from [`LivekitEvent::Microphone`].
#[derive(Resource)]
pub struct MicrophoneStatus {
    pub available: bool,
    /// `"granted"`, `"prompt"` or `"denied"`
    pub permission: String,
}

impl Default for MicrophoneStatus {
    fn default() -> Self {
        Self {
            available: false,
            permission: "denied".to_owned(),
        }
    }
}

#[derive(Resource)]
pub struct LivekitEventReceiver(Option<UnboundedReceiver<LivekitEvent>>);

/// Takes the page → engine event receiver; call from the plugin's build.
pub fn take_event_receiver() -> LivekitEventReceiver {
    let events = HOST
        .get()
        .and_then(|host| host.events.lock().unwrap().take());
    if events.is_none() {
        warn!("no livekit host: livekit rooms will not connect");
    }
    LivekitEventReceiver(events)
}

/// Delivers the page's replies and room events into the engine-side channels awaiting them.
pub fn dispatch_page_events(
    mut receiver: ResMut<LivekitEventReceiver>,
    mut microphone: ResMut<MicrophoneStatus>,
) {
    let Some(receiver) = receiver.0.as_mut() else {
        return;
    };
    while let Ok(event) = receiver.try_recv() {
        match event {
            LivekitEvent::Reply { request, reply } => {
                let sender = PENDING
                    .lock()
                    .unwrap()
                    .as_mut()
                    .and_then(|pending| pending.remove(&request));
                match sender {
                    Some(sender) => {
                        // the requester may have been aborted
                        let _ = sender.send(reply);
                    }
                    None => debug!("no requester for reply {request}: {reply:?}"),
                }
            }
            LivekitEvent::Room { room, event } => {
                let mut guard = ROOM_EVENTS.lock().unwrap();
                let Some(sender) = guard.as_mut().and_then(|rooms| rooms.get(&room)) else {
                    debug!("event for unknown room {room}: {event:?}");
                    continue;
                };
                if sender.send(event).is_err() {
                    // the engine's LivekitRoom is gone
                    guard.as_mut().unwrap().remove(&room);
                }
            }
            LivekitEvent::Microphone {
                available,
                permission,
            } => {
                microphone.available = available;
                microphone.permission = permission;
            }
        }
    }
}
