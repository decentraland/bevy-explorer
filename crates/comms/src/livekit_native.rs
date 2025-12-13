use std::sync::Arc;

use bevy::{platform::collections::HashMap, prelude::*};
use ethers_core::types::H160;
use futures_lite::StreamExt;
use http::Uri;
use kira::sound::streaming::StreamingSoundData;
use livekit::webrtc::{native::yuv_helper, prelude::VideoBuffer};
use prost::Message;
use tokio::{
    sync::{
        mpsc::{error::TryRecvError, Receiver, Sender},
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
    livekit_room::{LivekitConnection, LivekitTransport},
    ChannelControl, NetworkMessage, NetworkMessageRecipient,
};

use livekit::{
    id::{ParticipantIdentity, TrackSid},
    options::TrackPublishOptions,
    prelude::RemoteTrackPublication,
    track::{
        LocalAudioTrack, LocalTrack, RemoteAudioTrack, RemoteTrack, RemoteVideoTrack, TrackKind,
        TrackSource,
    },
    webrtc::{
        audio_source::native::NativeAudioSource,
        prelude::{AudioFrame, AudioSourceOptions, I420Buffer, RtcAudioSource},
    },
    Room, RoomOptions,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub struct MicPlugin;

impl Plugin for MicPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send_resource::<MicStream>();
        app.add_systems(Update, update_mic);
    }
}

#[derive(Default)]
pub struct MicStream(Option<cpal::Stream>);

#[derive(Deref)]
pub struct LivekitVideoFrame {
    #[deref]
    buffer: I420Buffer,
    timestamp: i64,
}

impl LivekitVideoFrame {
    pub fn timestamp(&self) -> i64 {
        self.timestamp
    }

    pub fn width(&self) -> u32 {
        self.buffer.width()
    }

    pub fn height(&self) -> u32 {
        self.buffer.height()
    }

    pub fn rgba_data(&self) -> Vec<u8> {
        let width = self.buffer.width();
        let height = self.buffer.height();
        let stride = width * 4;

        let (stride_y, stride_u, stride_v) = self.buffer.strides();
        let (data_y, data_u, data_v) = self.buffer.data();

        let mut dst = vec![0; (width * height * 4) as usize];

        yuv_helper::i420_to_abgr(
            data_y,
            stride_y,
            data_u,
            stride_u,
            data_v,
            stride_v,
            &mut dst,
            stride,
            width as i32,
            height as i32,
        );

        dst
    }
}

pub fn update_mic(
    mic: Res<LocalAudioSource>,
    mut last_name: Local<String>,
    mut stream: NonSendMut<MicStream>,
    mut mic_state: ResMut<MicState>,
) {
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

#[allow(clippy::type_complexity)]
pub fn connect_livekit(
    mut commands: Commands,
    mut new_livekits: Query<(Entity, &mut LivekitTransport), Without<LivekitConnection>>,
    player_state: Res<GlobalCrdtState>,
    mic: Res<crate::global_crdt::LocalAudioSource>,
) {
    for (transport_id, mut new_transport) in new_livekits.iter_mut() {
        debug!("spawn lk connect");
        let remote_address = new_transport.address.to_owned();
        let receiver = new_transport.receiver.take().unwrap();
        let control_receiver = new_transport.control_receiver.take().unwrap();
        let sender = player_state.get_sender();

        let subscription = mic.subscribe();
        std::thread::spawn(move || {
            livekit_handler(
                transport_id,
                remote_address,
                receiver,
                control_receiver,
                sender,
                subscription,
            )
        });

        commands.entity(transport_id).try_insert(LivekitConnection);
    }
}

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
    mut mic: tokio::sync::broadcast::Receiver<LocalAudioFrame>,
) -> Result<(), anyhow::Error> {
    debug!(">> lk connect async : {remote_address}");

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
    let mut streamer_audio_channel: Option<JoinHandle<()>> = None;
    let mut streamer_video_channel: Option<JoinHandle<()>> = None;

    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap(),
    );

    let rt2 = rt.clone();

    let task = rt.spawn(async move {
        let (room, mut network_rx) = livekit::prelude::Room::connect(&address, &token, RoomOptions{ auto_subscribe: false, adaptive_stream: false, dynacast: false, ..Default::default() }).await.unwrap();
        let local_participant = room.local_participant();

        let mut native_source: Option<NativeAudioSource> = None;
        let mut mic_sid: Option<TrackSid> = None;

        rt2.spawn(async move {
            while let Ok(frame) = mic.recv().await {
                let data = frame.data.iter().map(|f| (f * i16::MAX as f32) as i16).collect();
                if native_source.as_ref().is_none_or(|ns| ns.sample_rate() != frame.sample_rate || ns.num_channels() != frame.num_channels) {
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
                        AudioSourceOptions{
                            echo_cancellation: true,
                            noise_suppression: true,
                            auto_gain_control: true,
                        },
                        frame.sample_rate,
                        frame.num_channels,
                        None
                    ));
                    let mic_track = LocalTrack::Audio(LocalAudioTrack::create_audio_track("mic", RtcAudioSource::Native(new_source.clone())));
                    mic_sid = Some(local_participant.publish_track(mic_track, TrackPublishOptions{ source: TrackSource::Microphone, ..Default::default() }).await.unwrap().sid());
                    debug!("set sid");
                }
                if let Err(e) = native_source.as_mut().unwrap().capture_frame(&AudioFrame {
                    data,
                    sample_rate: frame.sample_rate,
                    num_channels: frame.num_channels,
                    samples_per_channel: frame.data.len() as u32 / frame.num_channels,
                }).await {
                    warn!("failed to capture from mic: {e}");
                };
            }
        });

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
                                    for publication in publications {
                                        debug!("initial pub: {publication:?}");
                                        if matches!(publication.kind(), TrackKind::Audio) {
                                            let _ = sender.send(PlayerUpdate {
                                                transport_id,
                                                message: PlayerMessage::AudioStreamAvailable { transport: transport_id },
                                                address,
                                            }).await;
                                        }
                                    }
                                } else if participant.identity().as_str().ends_with("-streamer") {
                                    for publication in publications {
                                        publication.set_subscribed(true);
                                    }
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
                            debug!("pub: {publication:?}");
                            if let Some(address) = participant.identity().0.as_str().as_h160() {
                                // publication.
                                if matches!(publication.kind(), TrackKind::Audio) {
                                    let _ = sender.send(PlayerUpdate {
                                        transport_id,
                                        message: PlayerMessage::AudioStreamAvailable { transport: transport_id },
                                        address,
                                    }).await;
                                }
                            } else if participant.identity().as_str().ends_with("-streamer") {
                                publication.set_subscribed(true);
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
                                        let handle = rt2.spawn(kira_thread(audio, publication, channel));
                                        track_tasks.insert(sid, handle);

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

                    let destination_identities = match outgoing.recipient {
                        NetworkMessageRecipient::All => Vec::default(),
                        NetworkMessageRecipient::Peer(address) => vec![ParticipantIdentity(format!("{address:#x}"))],
                        NetworkMessageRecipient::AuthServer => vec![ParticipantIdentity("authoritative-server".to_string())],
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
                        ChannelControl::VoiceSubscribe(address, sender) => {
                            participant_audio_subscribe(&room, address, Some(sender), &mut audio_channels).await
                        },
                        ChannelControl::VoiceUnsubscribe(address) => participant_audio_subscribe(&room, address, None, &mut audio_channels).await,
                        ChannelControl::StreamerSubscribe(audio, video) => {
                            streamer_audio_subscribe(&room, Some(audio), &mut streamer_audio_channel).await;
                            streamer_video_subscribe(&room, Some(video), &mut streamer_video_channel).await;
                        }
                        ChannelControl::StreamerUnsubscribe => {
                            streamer_audio_subscribe(&room, None, &mut streamer_audio_channel).await;
                            streamer_video_subscribe(&room, None, &mut streamer_video_channel).await;
                        }
                    };
                }
            );
        }

        room.close().await.unwrap();
    });

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

async fn kira_thread(
    audio: RemoteAudioTrack,
    publication: RemoteTrackPublication,
    channel: tokio::sync::oneshot::Sender<StreamingSoundData<AudioDecoderError>>,
) {
    let mut stream =
        livekit::webrtc::audio_stream::native::NativeAudioStream::new(audio.rtc_track(), 48_000, 1);

    // get first frame to set sample rate
    let Some(frame) = stream.next().await else {
        warn!("dropped audio track without samples");
        return;
    };

    let (frame_sender, frame_receiver) = tokio::sync::mpsc::channel(1000);

    let bridge = LivekitKiraBridge {
        started: false,
        sample_rate: frame.sample_rate,
        receiver: frame_receiver,
    };

    debug!("recced with {} / {}", frame.sample_rate, frame.num_channels);

    let sound_data = kira::sound::streaming::StreamingSoundData::from_decoder(bridge);

    let res = channel.send(sound_data);

    if res.is_err() {
        warn!("failed to send subscribed audio data");
        publication.set_subscribed(false);
        return;
    }

    while let Some(frame) = stream.next().await {
        match frame_sender.try_send(frame) {
            Ok(()) => (),
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                warn!("livekit audio receiver buffer full, dropping frame");
                return;
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                warn!("livekit audio receiver dropped, exiting task");
                return;
            }
        }
    }

    warn!("track ended, exiting task");
}

async fn livekit_video_thread(
    video: RemoteVideoTrack,
    publication: RemoteTrackPublication,
    channel: Sender<LivekitVideoFrame>,
) {
    let mut stream =
        livekit::webrtc::video_stream::native::NativeVideoStream::new(video.rtc_track());

    warn!(
        "livekit track {:?} {} {:?}",
        publication.sid(),
        stream.track().enabled(),
        stream.track().state()
    );
    while let Some(frame) = stream.next().await {
        let buffer = frame.buffer.to_i420();
        let Err(err) = channel
            .send(LivekitVideoFrame {
                buffer,
                timestamp: frame.timestamp_us,
            })
            .await
        else {
            continue;
        };

        error!("Livekit video channel errored: {err}.");
        break;
    }

    warn!("video track {:?} ended, exiting task", publication.sid());
}

async fn participant_audio_subscribe(
    room: &Room,
    address: H160,
    channel: Option<tokio::sync::oneshot::Sender<StreamingSoundData<AudioDecoderError>>>,
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

async fn streamer_audio_subscribe(
    room: &Room,
    mut channel: Option<Sender<StreamingSoundData<AudioDecoderError>>>,
    audio_thread: &mut Option<JoinHandle<()>>,
) {
    let participants = room.remote_participants();
    let Some((participant_identity, participant)) = participants
        .iter()
        .find(|(participant_identity, _)| participant_identity.as_str().ends_with("-streamer"))
    else {
        warn!(
            "no streamer participant available: {:?}",
            room.remote_participants().keys().collect::<Vec<_>>()
        );
        return;
    };

    let publications = participant.track_publications();
    let Some((publication, audio)) = publications.values().find_map(|track| {
        if let Some(RemoteTrack::Audio(audio)) = track.track() {
            Some((track.clone(), audio))
        } else {
            None
        }
    }) else {
        warn!("no audio for {:#x?}", participant_identity.as_str());
        return;
    };

    let subscribe = channel.is_some();
    publication.set_subscribed(subscribe);
    debug!("setsub: {subscribe}");
    if let Some(old_thread) = audio_thread.take() {
        old_thread.abort();
    }
    if let Some(new_channel) = channel.take() {
        let (sender, receiver) = tokio::sync::oneshot::channel();

        *audio_thread = Some(tokio::spawn(kira_thread(audio, publication, sender)));

        let data = receiver.await.unwrap();
        new_channel.send(data).await.unwrap();
    }
}

async fn streamer_video_subscribe(
    room: &Room,
    mut channel: Option<Sender<LivekitVideoFrame>>,
    video_thread: &mut Option<JoinHandle<()>>,
) {
    let participants = room.remote_participants();
    let Some((participant_identity, participant)) = participants
        .iter()
        .find(|(participant_identity, _)| participant_identity.as_str().ends_with("-streamer"))
    else {
        warn!(
            "no streamer participant available: {:?}",
            room.remote_participants().keys().collect::<Vec<_>>()
        );
        return;
    };

    let publications = participant.track_publications();
    let Some((publication, video)) = publications.values().find_map(|track| {
        if let Some(RemoteTrack::Video(video)) = track.track() {
            Some((track.clone(), video))
        } else {
            None
        }
    }) else {
        warn!("no video for {:#x?}", participant_identity.as_str());
        return;
    };

    let subscribe = channel.is_some();
    publication.set_subscribed(subscribe);
    debug!("video setsub: {subscribe}");
    if let Some(old_thread) = video_thread.take() {
        old_thread.abort();
    }
    if let Some(new_channel) = channel.take() {
        *video_thread = Some(tokio::spawn(livekit_video_thread(
            video,
            publication,
            new_channel,
        )));
    }
}
