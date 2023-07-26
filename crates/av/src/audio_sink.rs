use bevy::prelude::*;
use common::util::TryInsertEx;
use kira::{manager::backend::DefaultBackend, sound::streaming::StreamingSoundData};

use crate::{audio_context::AudioDecoderError, stream_processor::AVCommand};

#[derive(Component)]
pub struct AudioSink {
    pub command_sender: tokio::sync::mpsc::Sender<AVCommand>,
    pub sound_data: tokio::sync::mpsc::Receiver<StreamingSoundData<AudioDecoderError>>,
    pub handle: Option<<StreamingSoundData<AudioDecoderError> as kira::sound::SoundData>::Handle>,
}

impl AudioSink {
    pub fn new(
        command_sender: tokio::sync::mpsc::Sender<AVCommand>,
        receiver: tokio::sync::mpsc::Receiver<StreamingSoundData<AudioDecoderError>>,
    ) -> Self {
        Self {
            command_sender,
            sound_data: receiver,
            handle: None,
        }
    }
}

#[derive(Component)]
pub struct AudioSpawned;

pub fn spawn_audio_streams(
    mut commands: Commands,
    mut streams: Query<(Entity, &mut AudioSink), Without<AudioSpawned>>,
    mut audio_manager: NonSendMut<bevy_kira_audio::audio_output::AudioOutput<DefaultBackend>>,
) {
    for (ent, mut stream) in streams.iter_mut() {
        if let Ok(sound_data) = stream.sound_data.try_recv() {
            error!("running some sound data!");
            let handle = audio_manager
                .manager
                .as_mut()
                .unwrap()
                .play(sound_data)
                .unwrap();
            stream.handle = Some(handle);
            commands.entity(ent).try_insert(AudioSpawned);
        } else {
            error!(
                "waiting for sound data!: {:?}",
                stream.sound_data.try_recv().err()
            );
        }
    }
}
