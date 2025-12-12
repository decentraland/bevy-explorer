use bevy::prelude::*;
use common::structs::MicState;
#[cfg(not(target_arch = "wasm32"))]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
#[cfg(target_arch = "wasm32")]
use {
    bevy::render::view::RenderLayers,
    common::{structs::AudioSettings, util::VolumePanning},
};

#[cfg(not(target_arch = "wasm32"))]
use crate::global_crdt::{LocalAudioFrame, LocalAudioSource};
#[cfg(target_arch = "wasm32")]
use crate::{
    global_crdt::{ForeignAudioSource, ForeignPlayer},
    livekit::web::{
        is_microphone_available, set_microphone_enabled, set_participant_spatial_audio,
    },
};

pub struct MicPlugin;

impl Plugin for MicPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(not(target_arch = "wasm32"))]
        app.init_non_send_resource::<MicStream>();
        app.init_resource::<MicState>();

        app.add_systems(Update, update_mic);
        #[cfg(target_arch = "wasm32")]
        app.add_systems(Update, locate_foreign_streams);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct MicStream(Option<cpal::Stream>);

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
fn update_mic(
    mic_state: Res<MicState>,
    mut last_enabled: Local<Option<bool>>,
    mut last_available: Local<Option<bool>>,
) {
    let mut mic_state = mic_state.inner.blocking_write();
    // Check if microphone is available in the browser
    let current_available = is_microphone_available().unwrap_or(false);

    // Only update availability if it changed
    if last_available.is_none() || last_available.unwrap() != current_available {
        mic_state.available = current_available;
        *last_available = Some(current_available);
    }

    // Only update microphone enabled state if it changed
    if last_enabled.is_none() || last_enabled.unwrap() != mic_state.enabled {
        if let Err(e) = set_microphone_enabled(mic_state.enabled) {
            warn!("Failed to set microphone state: {:?}", e);
        }
        *last_enabled = Some(mic_state.enabled);
    }
}

#[cfg(target_arch = "wasm32")]
#[expect(clippy::type_complexity)]
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
