#[cfg(not(target_arch = "wasm32"))]
use std::{borrow::Cow, sync::Arc};

use bevy::{ecs::relationship::Relationship, prelude::*};
use common::structs::MicState;
use tokio::task::JoinHandle;
#[cfg(target_arch = "wasm32")]
use {
    bevy::render::view::RenderLayers,
    common::{structs::AudioSettings, util::VolumePanning},
    wasm_bindgen::prelude::*,
};
#[cfg(not(target_arch = "wasm32"))]
use {
    cpal::{
        traits::{DeviceTrait, HostTrait, StreamTrait},
        Device,
    },
    livekit::{
        id::TrackSid,
        options::TrackPublishOptions,
        participant::LocalParticipant,
        track::{LocalAudioTrack, LocalTrack, TrackSource},
        webrtc::{
            audio_source::native::NativeAudioSource,
            prelude::{AudioFrame, AudioSourceOptions, RtcAudioSource},
        },
    },
    tokio::sync::broadcast,
};

#[cfg(target_arch = "wasm32")]
use crate::{
    global_crdt::{ForeignAudioSource, ForeignPlayer},
    livekit::web::{set_microphone_enabled, set_participant_spatial_audio},
};
use crate::{
    global_crdt::{LocalAudioFrame, LocalAudioSource},
    livekit::{
        participant::{HostedBy, LivekitParticipant, Local as LivekitLocalParticipant},
        room::LivekitRoom,
        LivekitRuntime,
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

        app.add_systems(Startup, create_microphone_entity);
        app.add_systems(
            Update,
            (
                #[cfg(not(target_arch = "wasm32"))]
                verify_microphone_device_health,
                verify_availability,
                update_mic,
            )
                .chain(),
        );
        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(Update, (create_mic_thread, verify_health_of_mic_worker));
        #[cfg(target_arch = "wasm32")]
        app.add_systems(Update, locate_foreign_streams);

        app.add_observer(availability_changed);
        app.add_observer(enabled_changed);
    }
}

#[derive(Component)]
struct Microphone;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Deref, DerefMut)]
struct MicrophoneDevice(Device);

#[derive(Component, Deref)]
#[component(immutable)]
struct Available(bool);

#[derive(Component, Deref)]
#[component(immutable)]
struct Enabled(bool);

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
struct MicWorker {
    task: JoinHandle<()>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct MicStream(Option<cpal::Stream>);

fn create_microphone_entity(mut commands: Commands) {
    commands.spawn((Microphone, Available(false), Enabled(false)));
}

#[cfg(not(target_arch = "wasm32"))]
fn verify_microphone_device_health(
    mut commands: Commands,
    microphone: Single<(Entity, &MicrophoneDevice), With<Microphone>>,
) {
    let (entity, microphone_device) = microphone.into_inner();
    dbg!(microphone.id());
    if let Err(err) = microphone_device.name() {
        debug!("Microphone device became unavailable due to '{err}'.");
        commands
            .entity(entity)
            .remove::<MicrophoneDevice>()
            .insert(Available(false));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn verify_availability(
    mut commands: Commands,
    microphone: Single<Entity, (With<Microphone>, Without<MicrophoneDevice>)>,
    mut mic_state: ResMut<MicState>,
) {
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
        commands
            .entity(*microphone)
            .insert((MicrophoneDevice(device), Available(true)));
    }
}

#[cfg(target_arch = "wasm32")]
fn verify_availability(
    mut commands: Commands,
    microphone: Single<(Entity, &Available), With<Microphone>>,
    mut mic_state: ResMut<MicState>,
) {
    // Check if microphone is available in the browser
    let current_available = is_microphone_available().unwrap_or(false);
    let (entity, last_available) = microphone.into_inner();

    // Only update availability if it changed
    if **last_available != current_available {
        mic_state.available = current_available;
        commands.entity(entity).insert(Available(current_available));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn update_mic(
    microphone: Single<&mut MicrophoneDevice, With<Microphone>>,
    mic: Res<LocalAudioSource>,
    mut stream: NonSendMut<MicStream>,
    mic_state: Res<MicState>,
) {
    if let Ok(name) = microphone.name() {
        if mic_state.enabled {
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
            return;
        }

        let config = microphone.default_input_config().unwrap();
        let sender = mic.sender.clone();
        let num_channels = config.channels() as u32;
        let sample_rate = config.sample_rate().0;
        let new_stream = microphone
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
        match new_stream.play() {
            Ok(()) => {
                stream.0 = Some(new_stream);
                info!("set mic to {name}");
            }
            Err(e) => {
                warn!("failed to stream mic: {e}");
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn update_mic(
    mut commands: Commands,
    microphone: Single<(Entity, &Available, &Enabled), With<Microphone>>,
    mut mic_state: ResMut<MicState>,
) {
    // Check if microphone is available in the browser
    let current_available = is_microphone_available().unwrap_or(false);
    let (entity, last_available, last_enabled) = microphone.into_inner();

    // Only update availability if it changed
    if **last_available != current_available {
        mic_state.available = current_available;
        commands.entity(entity).insert(Available(current_available));
    }

    // Only update microphone enabled state if it changed
    if **last_enabled != mic_state.enabled {
        // if let Err(e) = set_microphone_enabled(mic_state.enabled) {
        //     warn!("Failed to set microphone state: {:?}", e);
        // }
        commands.entity(entity).insert(Enabled(mic_state.enabled));
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn create_mic_thread(
    mut commands: Commands,
    rooms: Query<&LivekitRoom>,
    participants: Populated<
        (Entity, &LivekitParticipant, &HostedBy),
        (With<LivekitLocalParticipant>, Without<MicWorker>),
    >,
    livekit_runtime: Res<LivekitRuntime>,
    local_audio_source: Res<LocalAudioSource>,
) {
    for (entity, participant, hosted_by) in participants.into_inner() {
        let Ok(room) = rooms.get(hosted_by.get()) else {
            error!("{entity} is not a LivekitRoom.");
            commands.send_event(AppExit::from_code(1));
            return;
        };

        let local_participant = room.local_participant();
        debug_assert_eq!(participant.sid(), local_participant.sid());

        debug!(
            "Starting mic thread for {} ({}) in room {}.",
            participant.sid(),
            participant.identity(),
            room.name()
        );
        let task = livekit_runtime.spawn(mic_thread(
            local_participant,
            local_audio_source.subscribe(),
        ));
        commands.entity(entity).insert(MicWorker { task });
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn verify_health_of_mic_worker(
    mut commands: Commands,
    participants: Populated<(Entity, &LivekitParticipant, &mut MicWorker)>,
) {
    for (entity, participant, mic_worker) in participants.into_inner() {
        if mic_worker.task.is_finished() {
            warn!(
                "Mic worker of {} ({}) has exited.",
                participant.sid(),
                participant.identity()
            );
            commands.entity(entity).try_remove::<MicWorker>();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn mic_thread(
    local_participant: LocalParticipant,
    mut mic: broadcast::Receiver<LocalAudioFrame>,
) {
    let mut native_source: Option<NativeAudioSource> = None;
    let mut mic_sid: Option<TrackSid> = None;

    while let Ok(frame) = mic.recv().await {
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
}

#[cfg(target_arch = "wasm32")]
fn locate_foreign_streams(
    mut streams: Query<(
        &GlobalTransform,
        Option<&RenderLayers>,
        &ForeignAudioSource,
        &ForeignPlayer,
    )>,
    pan: VolumePanning,
    settings: Res<AudioSettings>,
) {
    for (emitter_transform, render_layers, source, player) in streams.iter_mut() {
        if source.current_transport.is_some() {
            let (volume, panning) =
                pan.volume_and_panning(emitter_transform.translation(), render_layers);
            let volume = volume * settings.voice();

            update_participant_spatial_audio(
                &format!("{:#x}", player.address),
                -1.0 + 2.0 * panning,
                volume,
            );
        }
    }
}

// Public API for spatial audio control
#[cfg(target_arch = "wasm32")]
pub fn update_participant_spatial_audio(participant_identity: &str, pan: f32, volume: f32) {
    if let Err(e) = set_participant_spatial_audio(participant_identity, pan, volume) {
        warn!(
            "Failed to set spatial audio for {}: {:?}",
            participant_identity, e
        );
    }
}

fn availability_changed(
    _trigger: Trigger<OnInsert, Available>,
    microphone: Single<&Available, With<Microphone>>,
) {
    match *microphone {
        Available(true) => debug!("Microphone is now available."),
        Available(false) => debug!("Microphone is now unavailable."),
    }
}

fn enabled_changed(
    _trigger: Trigger<OnInsert, Enabled>,
    microphone: Single<&Enabled, With<Microphone>>,
) {
    match *microphone {
        Enabled(true) => debug!("Microphone is now enabled."),
        Enabled(false) => debug!("Microphone is now disabled."),
    }
}
