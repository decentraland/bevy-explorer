use bevy::prelude::*;
use comms::global_crdt::{LocalAudioFrame, LocalAudioSource};
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

pub fn update_mic(
    mic: Res<LocalAudioSource>,
    mut last_name: Local<String>,
    mut stream: NonSendMut<MicStream>,
) {
    let default_host = cpal::default_host();
    let default_input = default_host.default_input_device();
    if let Some(input) = default_input {
        if let Ok(name) = input.name() {
            if name == *last_name {
                return;
            }

            // drop old stream
            stream.0 = None;

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
        }
    }
}
