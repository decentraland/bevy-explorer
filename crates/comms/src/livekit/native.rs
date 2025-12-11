use bevy::{
    platform::{collections::HashMap, sync::Arc},
    prelude::*,
};
use ethers_core::types::H160;
use futures_lite::StreamExt;
use kira::sound::streaming::StreamingSoundData;
use livekit::webrtc::{native::yuv_helper, prelude::VideoBuffer};
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        Mutex,
    },
    task::JoinHandle,
};

use common::structs::{AudioDecoderError};

use crate::{
    global_crdt::{LocalAudioFrame},
    livekit::{
        kira_bridge::kira_thread, LivekitConnection, LivekitRoom, LivekitRuntime, LivekitTransport,
    },
    ChannelControl, NetworkMessage,
};

use livekit::{
    id::{ParticipantIdentity, TrackSid},
    options::TrackPublishOptions,
    prelude::RemoteTrackPublication,
    track::{LocalAudioTrack, LocalTrack, RemoteTrack, RemoteVideoTrack, TrackKind, TrackSource},
    webrtc::{
        audio_source::native::NativeAudioSource,
        prelude::{AudioFrame, AudioSourceOptions, I420Buffer, RtcAudioSource},
    },
    Room,
};


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

pub(super) fn connect_livekit(
    mut commands: Commands,
    mut new_livekits: Query<
        (Entity, &mut LivekitTransport, &LivekitRoom, &LivekitRuntime),
        Without<LivekitConnection>,
    >,
    mic: Res<crate::global_crdt::LocalAudioSource>,
) {
    for (transport_id, mut new_transport, livekit_room, livekit_runtime) in new_livekits.iter_mut()
    {
        debug!("spawn lk connect");
        let receiver = new_transport.receiver.take().unwrap();
        let control_receiver = new_transport.control_receiver.take().unwrap();
        let livekit_room = livekit_room.get_room();
        let livekit_runtime = livekit_runtime.clone();

        let subscription = mic.subscribe();
        std::thread::spawn(move || {
            livekit_handler(
                receiver,
                control_receiver,
                subscription,
                livekit_room,
                livekit_runtime,
            )
        });

        commands.entity(transport_id).try_insert(LivekitConnection);
    }
}

fn livekit_handler(
    receiver: Receiver<NetworkMessage>,
    control_receiver: Receiver<ChannelControl>,
    mic: tokio::sync::broadcast::Receiver<LocalAudioFrame>,
    room: Arc<Room>,
    runtime: LivekitRuntime,
) {
    let receiver = Arc::new(Mutex::new(receiver));
    let control_receiver = Arc::new(Mutex::new(control_receiver));

    loop {
        if let Err(e) = livekit_handler_inner(
            receiver.clone(),
            control_receiver.clone(),
            mic.resubscribe(),
            room.clone(),
            runtime.clone(),
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
    app_rx: Arc<Mutex<Receiver<NetworkMessage>>>,
    control_rx: Arc<Mutex<Receiver<ChannelControl>>>,
    mut mic: tokio::sync::broadcast::Receiver<LocalAudioFrame>,
    livekit_room: Arc<Room>,
    runtime: LivekitRuntime,
) -> Result<(), anyhow::Error> {
    let mut audio_channels: HashMap<
        H160,
        tokio::sync::oneshot::Sender<StreamingSoundData<AudioDecoderError>>,
    > = HashMap::new();
    let mut streamer_audio_channel: Option<JoinHandle<()>> = None;
    let mut streamer_video_channel: Option<JoinHandle<()>> = None;

    let rt2 = runtime.clone();

    let task = runtime.spawn(async move {
        let local_participant = livekit_room.local_participant();

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


        let mut app_rx = app_rx.lock().await;
        let mut control_rx = control_rx.lock().await;
        'stream: loop {
            tokio::select!(
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
                    if let Err(_e) = livekit_room.local_participant().publish_data(packet).await {
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
                            participant_audio_subscribe(&livekit_room, address, Some(sender), &mut audio_channels).await
                        },
                        ChannelControl::VoiceUnsubscribe(address) => participant_audio_subscribe(&livekit_room, address, None, &mut audio_channels).await,
                        ChannelControl::StreamerSubscribe(audio, video) => {
                            streamer_audio_subscribe(&livekit_room, Some(audio), &mut streamer_audio_channel).await;
                            streamer_video_subscribe(&livekit_room, Some(video), &mut streamer_video_channel).await;
                        }
                        ChannelControl::StreamerUnsubscribe => {
                            streamer_audio_subscribe(&livekit_room, None, &mut streamer_audio_channel).await;
                            streamer_video_subscribe(&livekit_room, None, &mut streamer_video_channel).await;
                        }
                    };
                }
            );
        }

        livekit_room.close().await.unwrap();
    });

    runtime.block_on(task).unwrap();
    Ok(())
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
