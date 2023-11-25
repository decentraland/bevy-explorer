// --server https://worlds-content-server.decentraland.org/world/shibu.dcl.eth --location 1,1

use std::sync::Arc;

use async_tungstenite::tungstenite::http::Uri;
use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
    utils::HashMap,
};
use futures_lite::StreamExt;
use livekit::{
    options::TrackPublishOptions,
    track::{LocalAudioTrack, LocalTrack, TrackSource},
    webrtc::{
        audio_source::native::NativeAudioSource,
        prelude::{AudioFrame, AudioSourceOptions, RtcAudioSource},
    },
    DataPacketKind, RoomOptions,
};
use prost::Message;
use tokio::sync::mpsc::{error::TryRecvError, Receiver, Sender};

use common::{structs::AudioDecoderError, util::AsH160};
use dcl_component::proto_components::kernel::comms::rfc4;

use crate::{
    global_crdt::{LocalAudioFrame, LocalAudioSource, PlayerMessage},
    profile::CurrentUserProfile,
    Transport, TransportType,
};

use super::{
    global_crdt::{GlobalCrdtState, PlayerUpdate},
    NetworkMessage,
};

pub struct LivekitPlugin;

impl Plugin for LivekitPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (connect_livekit, start_livekit));
        app.add_event::<StartLivekit>();
    }
}

#[derive(Event)]
pub struct StartLivekit {
    pub entity: Entity,
    pub address: String,
}

#[derive(Component)]
pub struct LivekitTransport {
    pub address: String,
    pub receiver: Option<Receiver<NetworkMessage>>,
    pub retries: usize,
}

#[derive(Component)]
pub struct LivekitConnection(Task<()>);

pub fn start_livekit(
    mut commands: Commands,
    mut room_events: EventReader<StartLivekit>,
    current_profile: Res<CurrentUserProfile>,
) {
    if let Some(ev) = room_events.read().last() {
        info!("starting livekit protocol");
        let (sender, receiver) = tokio::sync::mpsc::channel(1000);

        // queue a profile version message
        let response = rfc4::Packet {
            message: Some(rfc4::packet::Message::ProfileVersion(
                rfc4::AnnounceProfileVersion {
                    profile_version: current_profile.0.version,
                },
            )),
        };
        let _ = sender.try_send(NetworkMessage::reliable(&response));

        commands.entity(ev.entity).try_insert((
            Transport {
                transport_type: TransportType::Livekit,
                sender,
                foreign_aliases: Default::default(),
            },
            LivekitTransport {
                address: ev.address.to_owned(),
                receiver: Some(receiver),
                retries: 0,
            },
        ));
    }
}

#[allow(clippy::type_complexity)]
fn connect_livekit(
    mut commands: Commands,
    mut new_livekits: Query<(Entity, &mut LivekitTransport), Without<LivekitConnection>>,
    player_state: Res<GlobalCrdtState>,
    mic: Res<LocalAudioSource>,
) {
    for (transport_id, mut new_transport) in new_livekits.iter_mut() {
        debug!("spawn lk connect");
        let remote_address = new_transport.address.to_owned();
        let receiver = new_transport.receiver.take().unwrap();
        let sender = player_state.get_sender();

        let task = IoTaskPool::get().spawn(livekit_handler(
            transport_id,
            remote_address,
            receiver,
            sender,
            mic.subscribe(),
        ));
        commands
            .entity(transport_id)
            .insert(LivekitConnection(task));
    }
}

async fn livekit_handler(
    transport_id: Entity,
    remote_address: String,
    receiver: Receiver<NetworkMessage>,
    sender: Sender<PlayerUpdate>,
    mic: tokio::sync::broadcast::Receiver<LocalAudioFrame>,
) {
    if let Err(e) = livekit_handler_inner(transport_id, remote_address, receiver, sender, mic).await
    {
        warn!("livekit error: {e}");
    }
    warn!("livekit thread exit");
}

async fn livekit_handler_inner(
    transport_id: Entity,
    remote_address: String,
    mut app_rx: Receiver<NetworkMessage>,
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
    let params = HashMap::from_iter(url.query().unwrap_or_default().split('&').flat_map(|par| {
        par.split_once('=')
            .map(|(a, b)| (a.to_owned(), b.to_owned()))
    }));
    debug!("{params:?}");
    let token = params.get("access_token").cloned().unwrap_or_default();

    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap(),
    );

    let rt2 = rt.clone();

    let task = rt.spawn(async move {
        let (room, mut network_rx) = livekit::prelude::Room::connect(&address, &token, RoomOptions{ auto_subscribe: true, adaptive_stream: false, dynacast: false }).await.unwrap();
        let native_source = NativeAudioSource::new(AudioSourceOptions{
            echo_cancellation: true,
            noise_suppression: true,
            auto_gain_control: true,
        });
        let mic_track = LocalTrack::Audio(LocalAudioTrack::create_audio_track("mic", RtcAudioSource::Native(native_source.clone())));
        room.local_participant().publish_track(mic_track, TrackPublishOptions{ source: TrackSource::Microphone, ..Default::default() }).await.unwrap();

        rt2.spawn(async move {
            while let Ok(frame) = mic.recv().await {
                let data = frame.data.iter().map(|f| (f * i16::MAX as f32) as i16).collect();
                native_source.capture_frame(&AudioFrame {
                    data,
                    sample_rate: frame.sample_rate,
                    num_channels: frame.num_channels,
                    samples_per_channel: frame.data.len() as u32,
                })
            }
        });

        'stream: loop {
            tokio::select!(
                incoming = network_rx.recv() => {
                    debug!("in: {:?}", incoming);
                    let Some(incoming) = incoming else {
                        debug!("network pipe broken, exiting loop");
                        break 'stream;
                    };

                    match incoming {
                        livekit::RoomEvent::DataReceived { payload, participant, .. } => {
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
                                warn!("received packet {message:?} from {address}");
                                if let Err(e) = sender.send(PlayerUpdate {
                                    transport_id,
                                    message: PlayerMessage::PlayerData(message),
                                    address,
                                }).await {
                                    warn!("app pipe broken ({e}), existing loop");
                                    break 'stream;
                                }
                            }
                        },
                        livekit::RoomEvent::TrackSubscribed { track, publication: _, participant } => {
                            if let Some(address) = participant.identity().0.as_str().as_h160() {
                                match track {
                                    livekit::track::RemoteTrack::Audio(audio) => {
                                        let sender = sender.clone();
                                        rt2.spawn(async move {
                                            let mut x = livekit::webrtc::audio_stream::native::NativeAudioStream::new(audio.rtc_track());

                                            // get first frame to set sample rate
                                            let Some(frame) = x.next().await else {
                                                warn!("dropped audio track without samples");
                                                return;
                                            };

                                            let (frame_sender, frame_receiver) = tokio::sync::mpsc::channel(10);

                                            let bridge = LivekitKiraBridge {
                                                sample_rate: frame.sample_rate,
                                                receiver: frame_receiver,
                                            };

                                            let sound_data = kira::sound::streaming::StreamingSoundData::from_decoder(
                                                bridge,
                                                kira::sound::streaming::StreamingSoundSettings::new(),
                                            );

                                            let _ = sender.send(PlayerUpdate {
                                                transport_id,
                                                message: PlayerMessage::AudioStream(Box::new(sound_data)),
                                                address,
                                            }).await;

                                            while let Some(frame) = x.next().await {
                                                match frame_sender.try_send(frame) {
                                                    Ok(()) => (),
                                                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                                                        warn!("livekit audio receiver buffer full, dropping frame");
                                                    },
                                                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                                                        warn!("livekit audio receiver dropped, exiting task");
                                                        return;
                                                    },
                                                }
                                            }

                                            warn!("track ended, exiting task");
                                        });
                                    },
                                    _ => warn!("not processing video tracks"),
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

                    let kind = if outgoing.unreliable {
                        DataPacketKind::Lossy
                    } else {
                        DataPacketKind::Reliable
                    };
                    if let Err(_e) = room.local_participant().publish_data(outgoing.data, kind, Default::default()).await {
                        // debug!("outgoing failed: {_e}; not exiting loop though since it often fails at least once or twice at the start...");
                        break 'stream;
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
    sample_rate: u32,
    receiver: tokio::sync::mpsc::Receiver<AudioFrame>,
}

impl kira::sound::streaming::Decoder for LivekitKiraBridge {
    type Error = AudioDecoderError;

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn num_frames(&self) -> usize {
        u32::MAX as usize
    }

    fn decode(&mut self) -> Result<Vec<kira::dsp::Frame>, Self::Error> {
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
                        frames.push(kira::dsp::Frame::new(sample, sample));
                    }
                }
                Err(TryRecvError::Disconnected) => return Err(AudioDecoderError::StreamClosed),
                Err(TryRecvError::Empty) => return Ok(frames),
            }
        }
    }

    fn seek(&mut self, _: usize) -> Result<usize, Self::Error> {
        Err(AudioDecoderError::Other("Can't seek".to_owned()))
    }
}
