#[cfg(not(target_arch = "wasm32"))]
use std::{borrow::Cow, sync::Arc};

use bevy::prelude::*;
use common::structs::MicState;
use tokio::task::JoinHandle;
#[cfg(target_arch = "wasm32")]
use {
    bevy::render::view::RenderLayers,
    common::{structs::AudioSettings, util::AsH160, util::VolumePanning},
    wasm_bindgen::prelude::*,
};
#[cfg(not(target_arch = "wasm32"))]
use {
    cpal::{
        traits::{DeviceTrait, HostTrait, StreamTrait},
        Device,
    },
    livekit::{
        options::TrackPublishOptions,
        participant::Participant,
        track::{LocalAudioTrack, LocalTrack, TrackSource},
        webrtc::{
            audio_source::native::NativeAudioSource,
            prelude::{AudioFrame, AudioSourceOptions, RtcAudioSource},
        },
    },
    tokio::sync::broadcast,
};

#[cfg(not(target_arch = "wasm32"))]
use crate::global_crdt::{LocalAudioFrame, LocalAudioSource};
use crate::livekit::{
    participant::{LivekitParticipant, Local as LivekitLocalParticipant},
    LivekitRuntime,
};
#[cfg(target_arch = "wasm32")]
use crate::{
    global_crdt::{ForeignAudioSource, ForeignPlayer},
    livekit::{
        track::{Audio, LivekitTrack, Publishing, Subscribed},
        web::{
            AudioCaptureOptions, LocalAudioTrack, LocalTrack, Participant, TrackPublishOptions,
            TrackSource,
        },
    },
};

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(module = "/livekit_web_bindings.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    pub fn is_microphone_available() -> Result<bool, JsValue>;
}

pub struct MicPlugin;

impl Plugin for MicPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(not(target_arch = "wasm32"))]
        app.init_non_send_resource::<MicStream>();

        app.init_state::<MicrophoneAvailability>();
        app.init_state::<MicrophoneState>();

        app.add_systems(
            Update,
            (
                verify_availability.run_if(in_state(MicrophoneAvailability::Unavailable)),
                verify_microphone_device_health.run_if(in_state(MicrophoneAvailability::Available)),
                (microphone_disabled, verify_enabled).run_if(
                    in_state(MicrophoneAvailability::Available)
                        .and(in_state(MicrophoneState::Disabled)),
                ),
                (microphone_enabled, verify_disabled).run_if(in_state(MicrophoneState::Enabled)),
                (
                    poll_local_audio_track_futures,
                    publish_tracks,
                    unpublish_tracks,
                )
                    .run_if(in_state(MicrophoneAvailability::Available)),
            )
                .chain(),
        );
        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(OnEnter(MicrophoneState::Enabled), build_cpal_stream);
        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(OnEnter(MicrophoneState::Disabled), drop_cpal_stream);

        #[cfg(target_arch = "wasm32")]
        app.add_systems(Update, locate_foreign_streams);
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, States)]
enum MicrophoneAvailability {
    #[default]
    Unavailable,
    Available,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, States)]
enum MicrophoneState {
    #[default]
    Disabled,
    Enabled,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource, Deref, DerefMut)]
struct MicrophoneDevice(Device);

#[derive(Component)]
struct ParticipantWithTrack;

#[derive(Component, Deref, DerefMut)]
struct LocalAudioTrackFuture(JoinHandle<LocalAudioTrack>);

#[derive(Component, Deref)]
struct MicrophoneLocalTrack(LocalAudioTrack);

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default, Deref, DerefMut)]
struct MicStream(Option<cpal::Stream>);

#[cfg(not(target_arch = "wasm32"))]
fn verify_availability(mut commands: Commands, mut mic_state: ResMut<MicState>) {
    let default_host = cpal::default_host();
    let maybe_device = default_host.default_input_device();

    if let Some(device) = maybe_device {
        debug!(
            "Default microphone '{}' set as input device.",
            device
                .name()
                .expect("Shouldn't became unavailable in such a sort span.")
        );
        mic_state.available = true;
        commands.set_state(MicrophoneAvailability::Available);
        commands.insert_resource(MicrophoneDevice(device));
    }
}

#[cfg(target_arch = "wasm32")]
fn verify_availability(mut commands: Commands, mut mic_state: ResMut<MicState>) {
    // Check if microphone is available in the browser
    let current_available = is_microphone_available().unwrap_or(false);

    // Only update availability if it changed
    if current_available {
        mic_state.available = true;
        commands.set_state(MicrophoneAvailability::Available);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn verify_microphone_device_health(
    mut commands: Commands,
    microphone_device: Res<MicrophoneDevice>,
    mut mic_state: ResMut<MicState>,
) {
    if let Err(err) = microphone_device.name() {
        debug!("Microphone device became unavailable due to '{err}'.");
        mic_state.available = false;
        commands.set_state(MicrophoneAvailability::Unavailable);
        commands.remove_resource::<MicrophoneDevice>();
    }
}

#[cfg(target_arch = "wasm32")]
fn verify_microphone_device_health(mut commands: Commands, mut mic_state: ResMut<MicState>) {
    // Check if microphone is available in the browser
    let current_available = is_microphone_available().unwrap_or(false);

    if !current_available {
        debug!("Microphone became unavailable.");
        mic_state.available = false;
        commands.set_state(MicrophoneAvailability::Unavailable);
    }
}

fn verify_enabled(mut commands: Commands, mic_state: Res<MicState>) {
    if mic_state.enabled {
        debug!("Microphone is now enabled.");
        commands.set_state(MicrophoneState::Enabled);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn build_cpal_stream(
    mut commands: Commands,
    microphone_device: Res<MicrophoneDevice>,
    local_audio_source: Res<LocalAudioSource>,
    mut mic_stream: NonSendMut<MicStream>,
) {
    let Ok(config) = microphone_device
        .default_input_config()
        .inspect_err(|err| error!("{err}"))
    else {
        // Do not change state until `MicState::enabled` changes to prevent
        // log spam
        return;
    };
    let sender = local_audio_source.sender.clone();

    let num_channels = config.channels() as u32;
    let sample_rate = config.sample_rate().0;

    let new_stream = microphone_device
        .build_input_stream(
            &config.into(),
            move |data_f32: &[f32], _: &cpal::InputCallbackInfo| {
                let mut data_uninit = Arc::new_uninit_slice(data_f32.len());
                let data_slice = Arc::get_mut(&mut data_uninit).unwrap();
                for (src, dest) in data_f32.iter().zip(data_slice.iter_mut()) {
                    dest.write((*src * i16::MAX as f32).round() as i16);
                }
                // SAFETY: we have initialized all 'len' elements
                let data = unsafe { data_uninit.assume_init() };
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

    if let Err(err) = new_stream.play() {
        error!("{err}");
    } else {
        debug!("Cpal stream built.");
        commands.set_state(MicrophoneState::Enabled);
        **mic_stream = Some(new_stream);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn drop_cpal_stream(mut commands: Commands, mut mic_stream: NonSendMut<MicStream>) {
    **mic_stream = None;
    commands.set_state(MicrophoneState::Disabled);
}

fn verify_disabled(mut commands: Commands, mic_state: Res<MicState>) {
    if !mic_state.enabled {
        debug!("Microphone is now disabled.");
        commands.set_state(MicrophoneState::Disabled);
    }
}

fn microphone_enabled(
    mut commands: Commands,
    local_participants: Populated<
        Entity,
        (With<LivekitLocalParticipant>, Without<ParticipantWithTrack>),
    >,
) {
    commands.insert_batch(
        local_participants
            .iter()
            .collect::<Vec<_>>()
            .into_iter()
            .map(|entity| (entity, ParticipantWithTrack)),
    );
}

fn microphone_disabled(
    mut commands: Commands,
    local_participants: Populated<
        Entity,
        (With<LivekitLocalParticipant>, With<ParticipantWithTrack>),
    >,
) {
    for entity in local_participants.into_inner() {
        commands.entity(entity).remove::<ParticipantWithTrack>();
    }
}

#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn publish_tracks(
    mut commands: Commands,
    local_participants: Query<
        (Entity, &LivekitParticipant),
        (
            With<LivekitLocalParticipant>,
            With<ParticipantWithTrack>,
            Without<MicrophoneLocalTrack>,
            Without<LocalAudioTrackFuture>,
        ),
    >,
    livekit_runtime: Res<LivekitRuntime>,
    #[cfg(not(target_arch = "wasm32"))] local_audio_source: Res<LocalAudioSource>,
    #[cfg(not(target_arch = "wasm32"))] microphone_device: Res<MicrophoneDevice>,
) {
    #[cfg(not(target_arch = "wasm32"))]
    let Ok(config) = microphone_device
        .default_input_config()
        .inspect_err(|err| error!("{err}"))
    else {
        return;
    };

    for (entity, livekit_participant) in local_participants {
        if !matches!(**livekit_participant, Participant::Local(_)) {
            error!(
                "Participant {} ({}) has 'Local', but was remote.",
                livekit_participant.sid(),
                livekit_participant.identity()
            );
            commands.send_event(AppExit::from_code(1));
            return;
        }

        #[cfg(target_arch = "wasm32")]
        let local_audio_track_future = livekit_runtime.spawn(build_audio_local_track());
        #[cfg(not(target_arch = "wasm32"))]
        let local_audio_track_future = livekit_runtime.spawn(build_audio_local_track(
            local_audio_source.subscribe(),
            config.sample_rate().0,
            u32::from(config.channels()),
        ));

        commands.entity(entity).insert((
            ParticipantWithTrack,
            LocalAudioTrackFuture(local_audio_track_future),
        ));
    }
}

fn poll_local_audio_track_futures(
    mut commands: Commands,
    local_audio_tracks: Populated<(Entity, &LivekitParticipant, &mut LocalAudioTrackFuture)>,
    livekit_runtime: Res<LivekitRuntime>,
) {
    for (entity, livekit_participant, mut local_audio_track_future) in
        local_audio_tracks.into_inner()
    {
        let Participant::Local(ref local_participant) = **livekit_participant else {
            error!(
                "Participant {} ({}) has 'Local', but was remote.",
                livekit_participant.sid(),
                livekit_participant.identity()
            );
            commands.send_event(AppExit::from_code(1));
            return;
        };

        if local_audio_track_future.is_finished() {
            match livekit_runtime.block_on(&mut **local_audio_track_future) {
                Ok(local_audio_track) => {
                    let local_participant_clone = local_participant.clone();
                    let local_audio_track_clone = local_audio_track.clone();
                    livekit_runtime.spawn(async move {
                        if let Err(err) = local_participant_clone
                            .publish_track(
                                LocalTrack::Audio(local_audio_track_clone),
                                TrackPublishOptions {
                                    source: TrackSource::Microphone,
                                    ..Default::default()
                                },
                            )
                            .await
                        {
                            error!(
                                "Failed to publish local audio track for {} ({}) due to '{err}'.",
                                local_participant_clone.sid(),
                                local_participant_clone.identity()
                            );
                        }
                    });

                    commands
                        .entity(entity)
                        .insert(MicrophoneLocalTrack(local_audio_track))
                        .remove::<LocalAudioTrackFuture>();
                }
                Err(err) => {
                    error!(
                        "Failed to poll local audio track of {} ({}) due to '{err}'.",
                        livekit_participant.sid(),
                        livekit_participant.identity()
                    );
                    commands.send_event(AppExit::from_code(1));
                    return;
                }
            }
        }
    }
}

#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn unpublish_tracks(
    mut commands: Commands,
    local_participants: Populated<
        (Entity, &LivekitParticipant, &MicrophoneLocalTrack),
        (With<LivekitLocalParticipant>, Without<ParticipantWithTrack>),
    >,
    livekit_runtime: Res<LivekitRuntime>,
    #[cfg(not(target_arch = "wasm32"))] local_audio_source: Res<LocalAudioSource>,
) {
    for (entity, livekit_participant, microphone_local_track) in local_participants.into_inner() {
        let Participant::Local(ref local_participant) = **livekit_participant else {
            error!(
                "Participant {} ({}) has 'Local', but was remote.",
                livekit_participant.sid(),
                livekit_participant.identity()
            );
            commands.send_event(AppExit::from_code(1));
            return;
        };

        let local_participant_clone = local_participant.clone();
        let local_audio_track_clone = (*microphone_local_track).clone();
        livekit_runtime.spawn(async move {
            #[cfg(not(target_arch = "wasm32"))]
            let unpublished = local_participant_clone
                .unpublish_track(&local_audio_track_clone.sid())
                .await;
            #[cfg(target_arch = "wasm32")]
            let unpublished = local_participant_clone
                .unpublish_track(&LocalTrack::Audio(local_audio_track_clone))
                .await;

            if let Err(err) = unpublished {
                error!(
                    "Failed to unpublish local audio track of {} ({}) due to '{err}'.",
                    local_participant_clone.sid(),
                    local_participant_clone.identity()
                );
            }
        });

        #[cfg(not(target_arch = "wasm32"))]
        let _ = local_audio_source.sender.send(LocalAudioFrame {
            data: Default::default(),
            sample_rate: 0,
            num_channels: 0,
            samples_per_channel: 0,
        });

        commands.entity(entity).remove::<MicrophoneLocalTrack>();
    }
}

#[cfg(target_arch = "wasm32")]
fn locate_foreign_streams(
    mut streams: Query<(
        &GlobalTransform,
        Option<&RenderLayers>,
        &ForeignAudioSource,
        &ForeignPlayer,
    )>,
    participants: Query<(&LivekitParticipant, &Publishing)>,
    tracks: Query<&LivekitTrack, (With<Subscribed>, With<Audio>)>,
    pan: VolumePanning,
    settings: Res<AudioSettings>,
) {
    for (emitter_transform, render_layers, source, player) in streams.iter_mut() {
        if source.current_transport.is_some() {
            let Some(publishing) = participants
                .iter()
                .filter_map(|(participant, publishing)| {
                    if participant
                        .identity()
                        .as_str()
                        .as_h160()
                        .filter(|h160| *h160 == player.address)
                        .is_some()
                    {
                        Some(publishing.collection().to_vec())
                    } else {
                        None
                    }
                })
                .reduce(|mut accumulator, mut next| {
                    accumulator.append(&mut next);
                    accumulator
                })
            else {
                error!("No participant with address {}.", player.address);
                continue;
            };

            for livekit_track in tracks.iter_many(publishing) {
                let (volume, panning) =
                    pan.volume_and_panning(emitter_transform.translation(), render_layers);
                let volume = volume * settings.voice();

                if let Some(track) = livekit_track.track() {
                    track.pan_and_volume(panning, volume);
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn build_audio_local_track(
    mic_receiver: broadcast::Receiver<LocalAudioFrame>,
    sample_rate: u32,
    num_channels: u32,
) -> LocalAudioTrack {
    let new_source = NativeAudioSource::new(
        AudioSourceOptions {
            echo_cancellation: true,
            noise_suppression: true,
            auto_gain_control: true,
        },
        sample_rate,
        num_channels,
        None,
    );
    let local_audio_track =
        LocalAudioTrack::create_audio_track("mic", RtcAudioSource::Native(new_source.clone()));

    let local_audio_track_clone = local_audio_track.clone();
    tokio::task::spawn(async move {
        let mut mic_receiver = mic_receiver;
        while let Ok(frame) = mic_receiver.recv().await {
            if frame.sample_rate == 0 && frame.num_channels == 0 {
                // Termination frame
                break;
            }
            if let Err(e) = new_source
                .capture_frame(&AudioFrame {
                    data: Cow::Borrowed(&frame.data),
                    sample_rate: frame.sample_rate,
                    num_channels: frame.num_channels,
                    samples_per_channel: frame.samples_per_channel,
                })
                .await
            {
                warn!("failed to capture from mic: {e}");
            };
        }
        debug!(
            "Mic worker for local audio track {} closed.",
            local_audio_track_clone.sid()
        );
    });

    local_audio_track
}

#[cfg(target_arch = "wasm32")]
async fn build_audio_local_track() -> LocalAudioTrack {
    LocalAudioTrack::new(AudioCaptureOptions {
        echo_cancellation: Some(true),
        noise_suppression: Some(true),
        auto_gain_control: Some(true),
        ..Default::default()
    })
    .await
}
