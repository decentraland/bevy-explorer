use bevy::{platform::sync::Arc, prelude::*};
use livekit::{
    id::TrackSid,
    options::TrackPublishOptions,
    track::{LocalAudioTrack, LocalTrack, TrackSource},
    webrtc::{
        audio_source::native::NativeAudioSource,
        prelude::{AudioFrame, AudioSourceOptions, RtcAudioSource},
    },
    Room,
};

use crate::{
    global_crdt::LocalAudioFrame,
    livekit::{room::LivekitRoom, LivekitConnection, LivekitRuntime},
};

pub(super) fn connect_livekit(
    mut commands: Commands,
    mut new_livekits: Query<(Entity, &LivekitRoom, &LivekitRuntime), Without<LivekitConnection>>,
    mic: Res<crate::global_crdt::LocalAudioSource>,
) {
    for (transport_id, livekit_room, livekit_runtime) in new_livekits.iter_mut() {
        debug!("spawn lk connect");
        let livekit_room = livekit_room.get_room();
        let livekit_runtime = livekit_runtime.clone();

        let subscription = mic.subscribe();
        std::thread::spawn(move || livekit_handler(subscription, livekit_room, livekit_runtime));

        commands.entity(transport_id).try_insert(LivekitConnection);
    }
}

fn livekit_handler(
    mic: tokio::sync::broadcast::Receiver<LocalAudioFrame>,
    room: Arc<Room>,
    runtime: LivekitRuntime,
) {
    loop {
        if let Err(e) = livekit_handler_inner(mic.resubscribe(), room.clone(), runtime.clone()) {
            warn!("livekit error: {e}");
        }
        if mic.is_closed() {
            break;
        }
        warn!("livekit connection dropped, reconnecting");
    }
}

fn livekit_handler_inner(
    mut mic: tokio::sync::broadcast::Receiver<LocalAudioFrame>,
    livekit_room: Arc<Room>,
    runtime: LivekitRuntime,
) -> Result<(), anyhow::Error> {
    let rt2 = runtime.clone();

    let task = runtime.spawn(async move {
        let local_participant = livekit_room.local_participant();

        let mut native_source: Option<NativeAudioSource> = None;
        let mut mic_sid: Option<TrackSid> = None;

        let task = rt2.spawn(async move {
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
        });
        task.await.unwrap();
    });

    runtime.block_on(task).unwrap();
    Ok(())
}
