pub mod participant;
pub mod track;

use std::sync::Arc;

use bevy::{platform::collections::HashMap, prelude::*};
use ethers_core::types::H160;
use http::Uri;
use kira::sound::streaming::StreamingSoundData;
use prost::{DecodeError, Message};
use tokio::{
    runtime::Runtime,
    sync::{
        mpsc::{
            error::{TryRecvError, TrySendError},
            Receiver, Sender, UnboundedReceiver,
        },
        Mutex,
    },
    task::JoinHandle,
};

use common::{
    structs::{AudioDecoderError, MicState},
    util::AsH160,
};
use dcl_component::proto_components::kernel::comms::rfc4;

use crate::{
    global_crdt::{
        GlobalCrdtState, LocalAudioFrame, LocalAudioSource, PlayerMessage, PlayerUpdate,
    },
    livekit::{
        native::{
            participant::{Participant, Participants, PublishingTracks},
            track::{Track, Tracks},
        },
        Connected, Disconnected, LivekitTransport, Reconnecting, Transporting,
    },
    ChannelControl, NetworkMessage,
};

use livekit::{
    id::{ParticipantIdentity, TrackSid},
    options::TrackPublishOptions,
    prelude::{LocalParticipant, RemoteTrackPublication},
    track::{LocalAudioTrack, LocalTrack, RemoteAudioTrack, TrackKind, TrackSource},
    webrtc::{
        audio_source::native::NativeAudioSource,
        prelude::{AudioFrame, AudioSourceOptions, RtcAudioSource},
    },
    ConnectionState, Room, RoomEvent, RoomOptions, RoomResult,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Plugin for Livekit connectivity on the native client.
pub(super) struct NativeLivekitPlugin;

impl Plugin for NativeLivekitPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(Update, NativeLivekitSystems::PollReceivers);

        app.add_plugins(participant::ParticipantPlugin);
        app.add_plugins(track::TrackPlugin);
        app.add_plugins(MicPlugin);

        app.add_systems(
            Update,
            (
                poll_connecting_livekit_rooms,
                publish_local_participant_mic,
                (poll_room_events, poll_outgoing_data, poll_control_messages)
                    .in_set(NativeLivekitSystems::PollReceivers),
            ),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
enum NativeLivekitSystems {
    PollReceivers,
}

impl LivekitTransport {
    /// Builds the [`LivekitTransport`] and spawns a task
    /// to connect to the Livekit Room.
    pub fn build_transport(
        address: String,
        receiver: Receiver<NetworkMessage>,
        control_receiver: Receiver<ChannelControl>,
    ) -> impl Bundle {
        (
            Self {
                address: address.to_owned(),
                receiver,
                control_receiver,
                retries: 0,
            },
            ConnectingLivekitRoom::new(&address),
        )
    }
}

/// The room connection from the [`LivekitTransport`].
#[derive(Component)]
struct LivekitRoom {
    room: Room,
    room_event_receiver: UnboundedReceiver<RoomEvent>,
}

/// Async runtime for tasks of this [`LivekitRoom`].
#[derive(Component, Deref)]
struct LivekitRuntime(Runtime);

/// A task to connect to a Livekit Room.
#[derive(Component)]
struct ConnectingLivekitRoom {
    task: JoinHandle<RoomResult<(Room, UnboundedReceiver<RoomEvent>)>>,
    /// Tokio [`Runtime`] where the tasks of this room will run on.
    ///
    /// Wrapped in an [`Option`] so that it can be [`Option::take`].
    runtime: Option<Runtime>,
}

impl ConnectingLivekitRoom {
    fn new(remote_address: &str) -> Self {
        let url = Uri::try_from(remote_address).unwrap();
        let address = format!(
            "{}://{}{}",
            url.scheme_str().unwrap_or_default(),
            url.host().unwrap_or_default(),
            url.path()
        );
        let params: HashMap<_, _, bevy::platform::hash::FixedHasher> =
            HashMap::from_iter(url.query().unwrap_or_default().split('&').flat_map(|par| {
                par.split_once('=')
                    .map(|(a, b)| (a.to_owned(), b.to_owned()))
            }));
        debug!("{params:?}");
        let token = params.get("access_token").cloned().unwrap_or_default();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();

        let task = runtime.spawn(async move {
            // Inside an async closure so that `address` and `token`
            // are captured
            livekit::prelude::Room::connect(
                &address,
                &token,
                RoomOptions {
                    auto_subscribe: false,
                    adaptive_stream: false,
                    dynacast: false,
                    ..Default::default()
                },
            )
            .await
        });

        Self {
            task,
            runtime: Some(runtime),
        }
    }
}

/// Track that the local participant has published on this [`LivekitRoom`].
#[derive(Component)]
struct LocalParticipantPublishedTrack(JoinHandle<()>);

/// Poll the connections to a Livekit Room.
///
/// When the connection is established, the [`LivekitRoom`] component
/// is added to the entity with [`LivekitTransport`].
fn poll_connecting_livekit_rooms(
    mut commands: Commands,
    connecting_live_kit_rooms: Populated<(Entity, &mut ConnectingLivekitRoom)>,
) {
    for (entity, mut connecting_room) in connecting_live_kit_rooms.into_inner() {
        if connecting_room.task.is_finished() {
            let ConnectingLivekitRoom { task, runtime, .. } = connecting_room.as_mut();
            let Some(runtime) = runtime.take() else {
                unreachable!("A ConnectingLivekitRoom should never have a None runtime.");
            };
            let result = runtime.block_on(task).unwrap();

            let mut entity_commands = commands.entity(entity);
            entity_commands.remove::<ConnectingLivekitRoom>();

            match result {
                Ok((room, room_event_receiver)) => {
                    debug!("Connected to livekit room {}.", room.name());
                    entity_commands.insert((
                        LivekitRoom {
                            room,
                            room_event_receiver,
                        },
                        LivekitRuntime(runtime),
                    ));
                }
                Err(room_err) => {
                    error!("Failed to connect to livekit room due to {room_err}.");
                }
            }
        }
    }
}

fn publish_local_participant_mic(
    mut commands: Commands,
    rooms: Populated<
        (Entity, &LivekitRoom, &LivekitRuntime),
        Without<LocalParticipantPublishedTrack>,
    >,
    mic: Res<crate::global_crdt::LocalAudioSource>,
) {
    for (entity, room, runtime) in rooms.into_inner() {
        let LivekitRoom { room, .. } = &room;
        let task = runtime.spawn(mic_consumer_thread(
            mic.subscribe(),
            room.local_participant(),
        ));
        commands
            .entity(entity)
            .insert(LocalParticipantPublishedTrack(task));
    }
}

fn poll_room_events(
    mut commands: Commands,
    rooms: Query<(Entity, &mut LivekitRoom, &LivekitRuntime)>,
    player_state: Res<GlobalCrdtState>,
    mut participants: Participants,
    mut tracks: Tracks,
) {
    for (entity, mut room, runtime) in rooms {
        let LivekitRoom {
            room_event_receiver,
            ..
        } = room.as_mut();
        match room_event_receiver.try_recv() {
            Ok(event) => {
                trace!("in: {:?}", event);
                match event {
                    livekit::RoomEvent::Connected {
                        participants_with_tracks,
                    } => {
                        for (participant, publications) in participants_with_tracks {
                            let participant = participants.new_participant(entity, participant);
                            for publication in publications {
                                tracks.track_published(participant, entity, publication.clone());
                                tracks.subscribe(publication);
                            }
                        }
                    }
                    livekit::RoomEvent::ParticipantConnected(participant) => {
                        let meta = participant.metadata();
                        let identity = participant.identity();

                        participants.new_participant(entity, participant);

                        if !meta.is_empty() {
                            if let Some(address) = identity.0.as_str().as_h160() {
                                let sender = player_state.get_sender();
                                runtime.spawn(async move {
                                    if let Err(e) = sender
                                        .send(PlayerUpdate {
                                            transport_id: entity,
                                            message: PlayerMessage::MetaData(meta),
                                            address,
                                        })
                                        .await
                                    {
                                        warn!("app pipe broken ({e}), existing loop");
                                    }
                                });
                            }
                        }
                    }
                    livekit::RoomEvent::ParticipantDisconnected(participant) => {
                        participants.participant_disconnected(participant);
                    }
                    livekit::RoomEvent::DataReceived {
                        payload,
                        participant,
                        ..
                    } => {
                        if let Some(address) = participant
                            .and_then(|participant| participant.identity().0.as_str().as_h160())
                        {
                            let sender = player_state.get_sender();

                            match send_data_packet(entity, address, sender, payload.as_slice()) {
                                Ok(_) => (),
                                Err(e) => {
                                    error!("{e}");
                                    // TODO how to react?
                                }
                            };
                        }
                    }
                    livekit::RoomEvent::TrackPublished {
                        publication,
                        participant,
                    } => {
                        if let Some(participant_id) = participants.get(&participant) {
                            tracks.track_published(participant_id, entity, publication.clone());
                            tracks.subscribe(publication);
                        } else {
                            error!(
                                "Received a publication from {} ({}) but it is not mapped.",
                                participant.identity(),
                                participant.sid()
                            );
                        }
                    }
                    livekit::RoomEvent::TrackUnpublished { publication, .. } => {
                        tracks.track_unpublished(publication);
                        // debug!("unpub: {publication:?}");
                        // if let Some(address) = participant.identity().0.as_str().as_h160() {
                        //     if matches!(publication.kind(), TrackKind::Audio) {
                        //         let _ = sender
                        //             .send(PlayerUpdate {
                        //                 transport_id,
                        //                 message: PlayerMessage::AudioStreamUnavailable {
                        //                     transport: transport_id,
                        //                 },
                        //                 address,
                        //             })
                        //             .await;
                        //     }
                        // }
                    }
                    livekit::RoomEvent::TrackSubscribed {
                        track, publication, ..
                    } => {
                        tracks.subscribed(track, publication);
                        // if let Some(address) = participant.identity().0.as_str().as_h160() {
                        //     let sid = track.sid();
                        //     match track {
                        //         livekit::track::RemoteTrack::Audio(audio) => {
                        //             let Some(channel) = audio_channels.remove(&address) else {
                        //                 warn!("no channel for subscribed audio");
                        //                 publication.set_subscribed(false);
                        //                 continue;
                        //             };
                        //             let handle = runtime.spawn(subscribe_remote_track_audio(
                        //                 audio,
                        //                 channel,
                        //                 publication,
                        //             ));
                        //             track_tasks.insert(sid, handle);
                        //         }
                        //         _ => warn!("not processing video tracks"),
                        //     }
                        // }
                    }
                    livekit::RoomEvent::TrackUnsubscribed { publication, .. } => {
                        tracks.unsubscribed(publication);
                    }
                    livekit::RoomEvent::TrackSubscriptionFailed {
                        error, track_sid, ..
                    } => {
                        error!(
                            "Failed to subscribe to track {} with: {}.",
                            track_sid, error
                        );
                        tracks.unsubscribed_track_sid(track_sid);
                    }
                    livekit::RoomEvent::ConnectionStateChanged(ConnectionState::Connected) => {
                        commands.entity(entity).insert(Connected);
                    }
                    livekit::RoomEvent::ConnectionStateChanged(ConnectionState::Reconnecting) => {
                        commands.entity(entity).insert(Reconnecting);
                    }
                    livekit::RoomEvent::ConnectionStateChanged(ConnectionState::Disconnected) => {
                        commands.entity(entity).insert(Disconnected);
                    }
                    _ => {
                        trace!("Event: {:?}", event);
                    }
                };
            }
            Err(TryRecvError::Disconnected) => {
                error!("RoomEvent channel has disconnected.");
                // TODO how to react?
            }
            Err(TryRecvError::Empty) => {
                // Do nothing
            }
        }
    }
}

/// Poll messages to be published by the local participant into the [`LivekitTransport`].
fn poll_outgoing_data(rooms: Query<(&mut LivekitTransport, &LivekitRoom, &LivekitRuntime)>) {
    for (mut transport, room, runtime) in rooms {
        let LivekitRoom { room, .. } = &room;

        match transport.receiver.try_recv() {
            Ok(message) => {
                let local_participant = room.local_participant();
                trace!(
                    "{} is publishing data to {}",
                    local_participant.sid(),
                    room.name()
                );

                let destination_identities = if let Some(address) = message.recipient {
                    vec![ParticipantIdentity(format!("{address:#x}"))]
                } else {
                    default()
                };

                let packet = livekit::DataPacket {
                    payload: message.data,
                    topic: None,
                    reliable: !message.unreliable,
                    destination_identities,
                };

                runtime.spawn(async move {
                    if let Err(e) = local_participant.publish_data(packet).await {
                        error!("An outgoing message was lost due to {e}.");
                    };
                });
            }
            Err(TryRecvError::Disconnected) => {
                error!("Outgoing message channel is disconnected.");
                // TODO how to respond.
            }
            Err(TryRecvError::Empty) => {
                // do nothing
            }
        }
    }
}

fn poll_control_messages(
    rooms: Query<(&mut LivekitTransport, &Transporting)>,
    participants: Query<(&Participant, &PublishingTracks)>,
    mut tracks: Tracks,
) {
    for (mut transport, transporting) in rooms {
        match transport.control_receiver.try_recv() {
            Ok(message) => {
                match message {
                    ChannelControl::Subscribe(address, sender) => {
                        let Some((_, published_tracks)) = participants
                            .iter_many(transporting.collection())
                            .find(|(participant, _)| {
                                participant
                                    .identity()
                                    .as_str()
                                    .as_h160()
                                    .filter(|h160| h160 == &address)
                                    .is_some()
                            })
                        else {
                            error!("Could not find participant {} in transport.", address);
                            continue;
                        };

                        let Some((_, track, _, _, _)) = tracks
                            .iter_many(published_tracks.collection())
                            .find(|(_, track, _, _, _)| track.kind() == TrackKind::Audio)
                        else {
                            error!("Participant {} did not publish any audio track.", address);
                            continue;
                        };

                        let remote_publication = (*track).clone();
                        tracks.attach_sender_to_audio_track(remote_publication, sender);
                    }
                    ChannelControl::Unsubscribe(address) => {
                        debug!("unsubscribe");
                        // address_channel_control_handler(address, None, &room, &mut audio_channels);
                    }
                    ChannelControl::StreamerSubscribe(streamer, sender) => (),
                    ChannelControl::StreamerUnsubscribe(streamer) => (),
                };
            }
            Err(TryRecvError::Disconnected) => {
                error!("Control channel is disconnected.");
                // TODO how to respond?
            }
            Err(TryRecvError::Empty) => {
                // Do nothing
            }
        }
    }
}

struct MicPlugin;

impl Plugin for MicPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send_resource::<MicStream>();
        app.add_systems(Update, update_mic);
    }
}

#[derive(Default)]
struct MicStream(Option<cpal::Stream>);

fn update_mic(
    mic: Res<LocalAudioSource>,
    mut last_name: Local<String>,
    mut stream: NonSendMut<MicStream>,
    mic_state: Res<MicState>,
) {
    let mut mic_state = mic_state.inner.blocking_write();
    let default_host = cpal::default_host();
    let default_input = default_host.default_input_device();
    if let Some(input) = default_input {
        if let Ok(name) = input.name() {
            mic_state.available = true;

            if name == *last_name && mic_state.enabled {
                return;
            }

            // drop old stream
            stream.0 = None;
            // send termination frame
            let _ = mic.sender.send(LocalAudioFrame {
                data: Default::default(),
                sample_rate: 0,
                num_channels: 0,
                samples_per_channel: 0,
            });

            if !mic_state.enabled {
                "disabled".clone_into(&mut last_name);
                return;
            }

            let config = input.default_input_config().unwrap();
            let sender = mic.sender.clone();
            let num_channels = config.channels() as u32;
            let sample_rate = config.sample_rate().0;
            let new_stream = input
                .build_input_stream(
                    &config.into(),
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        if sender
                            .send(LocalAudioFrame {
                                data: data.to_owned(),
                                sample_rate,
                                num_channels,
                                samples_per_channel: data.len() as u32 / num_channels,
                            })
                            .is_err()
                        {
                            warn!("mic channel closed?");
                        }
                    },
                    |err: cpal::StreamError| {
                        warn!("mic error: {err}");
                    },
                    None,
                )
                .unwrap();
            match new_stream.play() {
                Ok(()) => {
                    stream.0 = Some(new_stream);
                    info!("set mic to {name}");
                    *last_name = name;
                }
                Err(e) => {
                    warn!("failed to stream mic: {e}");
                }
            }

            return;
        }
    }

    // faild to find input - drop old stream
    stream.0 = None;
    "no device".clone_into(&mut last_name);
    mic_state.available = false;
}

// #[allow(clippy::type_complexity)]
// fn connect_livekit(
//     mut commands: Commands,
//     mut new_livekits: Query<(Entity, &mut LivekitTransport), Without<LivekitConnection>>,
//     player_state: Res<GlobalCrdtState>,
//     mic: Res<crate::global_crdt::LocalAudioSource>,
// ) {
//     for (transport_id, mut new_transport) in new_livekits.iter_mut() {
//         debug!("spawn lk connect");
//         let remote_address = new_transport.address.to_owned();
//         let receiver = new_transport.receiver.take().unwrap();
//         let control_receiver = new_transport.control_receiver.take().unwrap();
//         let sender = player_state.get_sender();

//         let subscription = mic.subscribe();
//         std::thread::spawn(move || {
//             livekit_handler(
//                 transport_id,
//                 remote_address,
//                 receiver,
//                 control_receiver,
//                 sender,
//                 subscription,
//             )
//         });

//         commands.entity(transport_id).try_insert(LivekitConnection);
//     }
// }

fn livekit_handler(
    transport_id: Entity,
    remote_address: String,
    receiver: Receiver<NetworkMessage>,
    control_receiver: Receiver<ChannelControl>,
    sender: Sender<PlayerUpdate>,
    mic: tokio::sync::broadcast::Receiver<LocalAudioFrame>,
) {
    let receiver = Arc::new(Mutex::new(receiver));
    let control_receiver = Arc::new(Mutex::new(control_receiver));

    loop {
        if let Err(e) = livekit_handler_inner(
            transport_id,
            &remote_address,
            receiver.clone(),
            control_receiver.clone(),
            sender.clone(),
            mic.resubscribe(),
        ) {
            warn!("livekit error: {e}");
        }
        if receiver.blocking_lock().is_closed() {
            // caller closed the channel
            return;
        }
        warn!("livekit connection dropped, reconnecting");
    }
}

fn livekit_handler_inner(
    transport_id: Entity,
    remote_address: &str,
    app_rx: Arc<Mutex<Receiver<NetworkMessage>>>,
    control_rx: Arc<Mutex<Receiver<ChannelControl>>>,
    sender: Sender<PlayerUpdate>,
    mic: tokio::sync::broadcast::Receiver<LocalAudioFrame>,
) -> Result<(), anyhow::Error> {
    debug!(">> lk connect async : {remote_address}");

    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap(),
    );

    let task = rt.spawn(livekit_handler_thread(
        rt.clone(),
        transport_id,
        remote_address.to_owned(),
        mic,
        sender,
        app_rx,
        control_rx,
    ));

    rt.block_on(task).unwrap();
    Ok(())
}

struct LivekitKiraBridge {
    started: bool,
    sample_rate: u32,
    receiver: tokio::sync::mpsc::Receiver<AudioFrame<'static>>,
}

impl kira::sound::streaming::Decoder for LivekitKiraBridge {
    type Error = AudioDecoderError;

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn num_frames(&self) -> usize {
        u32::MAX as usize
    }

    fn decode(&mut self) -> Result<Vec<kira::Frame>, Self::Error> {
        let mut frames = Vec::default();

        loop {
            match self.receiver.try_recv() {
                Ok(frame) => {
                    if frame.sample_rate != self.sample_rate {
                        warn!(
                            "sample rate changed?! was {}, now {}",
                            self.sample_rate, frame.sample_rate
                        );
                    }

                    if frame.num_channels != 1 {
                        warn!("frame has {} channels", frame.num_channels);
                    }

                    for i in 0..frame.samples_per_channel as usize {
                        let sample = frame.data[i] as f32 / i16::MAX as f32;
                        frames.push(kira::Frame::new(sample, sample));
                    }
                }
                Err(TryRecvError::Disconnected) => return Err(AudioDecoderError::StreamClosed),
                Err(TryRecvError::Empty) => return Ok(frames),
            }
        }
    }

    fn seek(&mut self, seek: usize) -> Result<usize, Self::Error> {
        if !self.started && seek == 0 {
            return Ok(0);
        }
        Err(AudioDecoderError::Other(format!(
            "Can't seek (requested {seek})"
        )))
    }
}

async fn h160_track_publications(
    address: H160,
    publication: &RemoteTrackPublication,
    player_update_sender: &Sender<PlayerUpdate>,
    transport_id: Entity,
) {
    debug!("pub: {publication:?}");

    if matches!(publication.kind(), TrackKind::Audio) {
        let _ = player_update_sender
            .send(PlayerUpdate {
                transport_id,
                message: PlayerMessage::AudioStreamAvailable {
                    transport: transport_id,
                },
                address,
            })
            .await;
    }
}

fn streamer_track_publications<'a>(
    publications: impl IntoIterator<Item = &'a RemoteTrackPublication>,
) {
    for publication in publications.into_iter() {
        debug!(
            "streamer pub: {:?} {:?} {:?}",
            publication.sid(),
            publication.kind(),
            publication.source()
        );
        publication.set_subscribed(true);
    }
}

async fn livekit_handler_thread(
    runtime: Arc<Runtime>,
    transport_id: Entity,
    remote_address: String,
    mic: tokio::sync::broadcast::Receiver<LocalAudioFrame>,
    sender: Sender<PlayerUpdate>,
    app_rx: Arc<Mutex<Receiver<NetworkMessage>>>,
    control_rx: Arc<Mutex<Receiver<ChannelControl>>>,
) {
    let url = Uri::try_from(remote_address).unwrap();
    let address = format!(
        "{}://{}{}",
        url.scheme_str().unwrap_or_default(),
        url.host().unwrap_or_default(),
        url.path()
    );
    let params: HashMap<_, _, bevy::platform::hash::FixedHasher> =
        HashMap::from_iter(url.query().unwrap_or_default().split('&').flat_map(|par| {
            par.split_once('=')
                .map(|(a, b)| (a.to_owned(), b.to_owned()))
        }));
    debug!("{params:?}");
    let token = params.get("access_token").cloned().unwrap_or_default();

    let mut audio_channels: HashMap<
        H160,
        tokio::sync::oneshot::Sender<StreamingSoundData<AudioDecoderError>>,
    > = HashMap::new();

    let (room, mut network_rx) = livekit::prelude::Room::connect(
        &address,
        &token,
        RoomOptions {
            auto_subscribe: false,
            adaptive_stream: false,
            dynacast: false,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    runtime.spawn(mic_consumer_thread(mic, room.local_participant()));

    let mut track_tasks: HashMap<TrackSid, JoinHandle<()>> = HashMap::new();

    let mut app_rx = app_rx.lock().await;
    let mut control_rx = control_rx.lock().await;
    'stream: loop {
        tokio::select!(
            incoming = network_rx.recv() => {
                debug!("in: {:?}", incoming);
                let Some(incoming) = incoming else {
                    debug!("network pipe broken, exiting loop");
                    break 'stream;
                };

                match incoming {
                    livekit::RoomEvent::Connected { participants_with_tracks } => {
                        for (participant, publications) in participants_with_tracks {
                            if let Some(address) = participant.identity().0.as_str().as_h160() {
                                // h160_track_publications(address, &publications, &sender, transport_id);
                            } else if participant.identity().0.as_str().ends_with("-streamer") {
                                streamer_track_publications(&publications);
                            }
                        }
                    }
                    livekit::RoomEvent::DataReceived { payload, participant, .. } => {
                        if let Some(participant) = participant {
                            if let Some(address) = participant.identity().0.as_str().as_h160() {
                                let packet = match rfc4::Packet::decode(payload.as_slice()) {
                                    Ok(packet) => packet,
                                    Err(e) => {
                                        warn!("unable to parse packet body: {e}");
                                        continue;
                                    }
                                };
                                let Some(message) = packet.message else {
                                    warn!("received empty packet body");
                                    continue;
                                };
                                debug!("[{}] received [{}] packet {message:?} from {address}", transport_id, packet.protocol_version);
                                if let Err(e) = sender.send(PlayerUpdate {
                                    transport_id,
                                    message: PlayerMessage::PlayerData(message),
                                    address,
                                }).await {
                                    warn!("app pipe broken ({e}), existing loop");
                                    break 'stream;
                                }
                            }
                        }
                    },
                    livekit::RoomEvent::TrackPublished { publication, participant } => {
                        if let Some(address) = participant.identity().0.as_str().as_h160() {
                            // h160_track_publications(address, [&publication], & sender, transport_id);
                        } else if participant.identity().0.as_str().ends_with("-streamer") {
                            streamer_track_publications([&publication]);
                        }
                    }
                    livekit::RoomEvent::TrackUnpublished { publication, participant } => {
                        debug!("unpub: {publication:?}");
                        if let Some(address) = participant.identity().0.as_str().as_h160() {
                            if matches!(publication.kind(), TrackKind::Audio) {
                                let _ = sender.send(PlayerUpdate {
                                    transport_id,
                                    message: PlayerMessage::AudioStreamUnavailable { transport: transport_id },
                                    address,
                                }).await;
                            }
                        }
                    }
                    livekit::RoomEvent::TrackSubscribed { track, publication, participant } => {
                        if let Some(address) = participant.identity().0.as_str().as_h160() {
                            let sid = track.sid();
                            match track {
                                livekit::track::RemoteTrack::Audio(audio) => {
                                    let Some(channel) = audio_channels.remove(&address) else {
                                        warn!("no channel for subscribed audio");
                                        publication.set_subscribed(false);
                                        continue;
                                    };
                                    // let handle = runtime.spawn(subscribe_remote_track_audio(audio, channel, publication));
                                    // track_tasks.insert(sid, handle);

                                },
                                _ => warn!("not processing video tracks"),
                            }
                        }
                    }
                    livekit::RoomEvent::TrackUnsubscribed{ track, .. } => {
                        if let Some(handle) = track_tasks.remove(&track.sid()) {
                            handle.abort();
                        }
                    }
                    livekit::RoomEvent::ParticipantConnected(participant) => {
                        let meta = participant.metadata();
                        if !meta.is_empty() {
                            if let Some(address) = participant.identity().0.as_str().as_h160() {
                                if let Err(e) = sender.send(PlayerUpdate {
                                    transport_id,
                                    message: PlayerMessage::MetaData(meta),
                                    address,
                                }).await {
                                    warn!("app pipe broken ({e}), existing loop");
                                    break 'stream;
                                }
                            }
                        }
                    }
                    _ => { debug!("Event: {:?}", incoming); }
                };
            }
            outgoing = app_rx.recv() => {
                let Some(outgoing) = outgoing else {
                    debug!("app pipe broken, exiting loop");
                    break 'stream;
                };

                let destination_identities = if let Some(address) = outgoing.recipient {
                    vec![ParticipantIdentity(format!("{address:#x}"))]
                } else {
                    default()
                };

                let packet = livekit::DataPacket { payload: outgoing.data, topic: None, reliable: !outgoing.unreliable, destination_identities };
                if let Err(_e) = room.local_participant().publish_data(packet).await {
                    // debug!("outgoing failed: {_e}; not exiting loop though since it often fails at least once or twice at the start...");
                    break 'stream;
                };
            }
            control = control_rx.recv() => {
                let Some(control) = control else {
                    debug!("app pipe broken, exiting loop");
                    break 'stream;
                };

                match control {
                    ChannelControl::Subscribe(address, sender) => address_channel_control_handler(address, Some(sender), &room, &mut audio_channels),
                    ChannelControl::Unsubscribe(address) => address_channel_control_handler(address, None, &room, &mut audio_channels),
                    ChannelControl::StreamerSubscribe(streamer, sender) => (),
                    ChannelControl::StreamerUnsubscribe(streamer) => (),
                };

            }
        );
    }

    room.close().await.unwrap();
}

async fn mic_consumer_thread(
    mut mic: tokio::sync::broadcast::Receiver<LocalAudioFrame>,
    local_participant: LocalParticipant,
) {
    let mut native_source: Option<NativeAudioSource> = None;
    let mut mic_sid: Option<TrackSid> = None;

    while let Ok(frame) = mic.recv().await {
        let data = frame
            .data
            .iter()
            .map(|f| (f * i16::MAX as f32) as i16)
            .collect();
        if native_source.as_ref().is_none_or(|ns| {
            ns.sample_rate() != frame.sample_rate || ns.num_channels() != frame.num_channels
        }) {
            // update track
            if let Some(sid) = mic_sid.take() {
                if let Err(e) = local_participant.unpublish_track(&sid).await {
                    warn!("error unpublishing previous mic track: {e}");
                }
                debug!("unpub mic");
            }

            if frame.num_channels == 0 {
                native_source = None;
                continue;
            }

            let new_source = native_source.insert(NativeAudioSource::new(
                AudioSourceOptions {
                    echo_cancellation: true,
                    noise_suppression: true,
                    auto_gain_control: true,
                },
                frame.sample_rate,
                frame.num_channels,
                None,
            ));
            let mic_track = LocalTrack::Audio(LocalAudioTrack::create_audio_track(
                "mic",
                RtcAudioSource::Native(new_source.clone()),
            ));
            mic_sid = Some(
                local_participant
                    .publish_track(
                        mic_track,
                        TrackPublishOptions {
                            source: TrackSource::Microphone,
                            ..Default::default()
                        },
                    )
                    .await
                    .unwrap()
                    .sid(),
            );
            debug!("set sid");
        }
        if let Err(e) = native_source
            .as_mut()
            .unwrap()
            .capture_frame(&AudioFrame {
                data,
                sample_rate: frame.sample_rate,
                num_channels: frame.num_channels,
                samples_per_channel: frame.data.len() as u32 / frame.num_channels,
            })
            .await
        {
            warn!("failed to capture from mic: {e}");
        };
    }
}

fn address_channel_control_handler(
    address: H160,
    channel: Option<tokio::sync::oneshot::Sender<StreamingSoundData<AudioDecoderError>>>,
    room: &Room,
    audio_channels: &mut HashMap<
        H160,
        tokio::sync::oneshot::Sender<StreamingSoundData<AudioDecoderError>>,
    >,
) {
    let participants = room.remote_participants();
    let Some(participant) = participants.get(&ParticipantIdentity(format!("{address:#x}"))) else {
        warn!(
            "no participant {address:?}! available: {:?}",
            room.remote_participants().keys().collect::<Vec<_>>()
        );
        return;
    };

    let publications = participant.track_publications();
    let Some(track) = publications
        .values()
        .find(|track| matches!(track.kind(), TrackKind::Audio))
    else {
        warn!("no audio for {address:#x?}");
        return;
    };

    let subscribe = channel.is_some();
    track.set_subscribed(subscribe);
    debug!("setsub: {subscribe}");
    if let Some(channel) = channel {
        audio_channels.insert(address, channel);
    } else {
        audio_channels.remove(&address);
    }
}

#[derive(Debug)]
pub enum SendDataPacketError {
    Decode(DecodeError),
    Empty,
    Closed,
    Full,
}

impl From<TrySendError<PlayerUpdate>> for SendDataPacketError {
    fn from(value: TrySendError<PlayerUpdate>) -> Self {
        match value {
            TrySendError::Full(_) => Self::Full,
            TrySendError::Closed(_) => Self::Closed,
        }
    }
}

impl std::fmt::Display for SendDataPacketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decode(e) => writeln!(f, "{e}"),
            Self::Empty => writeln!(f, "Payload was empty."),
            Self::Closed => writeln!(f, "PlayerUpdate channel was closed."),
            Self::Full => writeln!(f, "PlayerUpdate channel was full."),
        }
    }
}

impl std::error::Error for SendDataPacketError {}

fn send_data_packet(
    transport: Entity,
    address: H160,
    sender: Sender<PlayerUpdate>,
    payload: &[u8],
) -> Result<(), SendDataPacketError> {
    let packet = match rfc4::Packet::decode(payload) {
        Ok(packet) => packet,
        Err(e) => {
            return Err(SendDataPacketError::Decode(e));
        }
    };
    let Some(message) = packet.message else {
        return Err(SendDataPacketError::Empty);
    };
    trace!(
        "[{}] received [{}] packet {message:?} from {address}",
        transport,
        packet.protocol_version
    );
    sender
        .try_send(PlayerUpdate {
            transport_id: transport,
            message: PlayerMessage::PlayerData(message),
            address,
        })
        .map_err(SendDataPacketError::from)
}
