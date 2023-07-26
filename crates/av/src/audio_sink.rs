use bevy::prelude::*;
use common::{structs::PrimaryUser, util::TryInsertEx};
use kira::{manager::backend::DefaultBackend, sound::streaming::StreamingSoundData, tween::Tween};
use scene_runner::{ContainingScene, SceneEntity};
use tokio::sync::mpsc::error::TryRecvError;

use crate::{audio_context::AudioDecoderError, stream_processor::AVCommand};

#[derive(Component)]
pub struct AudioSink {
    pub volume: f32,
    pub command_sender: tokio::sync::mpsc::Sender<AVCommand>,
    pub sound_data: tokio::sync::mpsc::Receiver<StreamingSoundData<AudioDecoderError>>,
    pub handle: Option<<StreamingSoundData<AudioDecoderError> as kira::sound::SoundData>::Handle>,
}

impl AudioSink {
    pub fn new(
        volume: f32,
        command_sender: tokio::sync::mpsc::Sender<AVCommand>,
        receiver: tokio::sync::mpsc::Receiver<StreamingSoundData<AudioDecoderError>>,
    ) -> Self {
        Self {
            volume,
            command_sender,
            sound_data: receiver,
            handle: None,
        }
    }
}

#[derive(Component)]
pub struct AudioSpawned;

// TODO integrate better with bevy_kira_audio to avoid logic on a main-thread system (NonSendMut forces this system to the main thread)
pub fn spawn_audio_streams(
    mut commands: Commands,
    mut streams: Query<(Entity, &SceneEntity, &mut AudioSink, Option<&AudioSpawned>)>,
    mut audio_manager: NonSendMut<bevy_kira_audio::audio_output::AudioOutput<DefaultBackend>>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
) {
    let containing_scene = player
        .get_single()
        .ok()
        .and_then(|player| containing_scene.get(player));

    for (ent, scene, mut stream, maybe_spawned) in streams.iter_mut() {
        if maybe_spawned.is_none() {
            match stream.sound_data.try_recv() {
                Ok(sound_data) => {
                    info!("{ent:?} received sound data!");
                    let handle = audio_manager
                        .manager
                        .as_mut()
                        .unwrap()
                        .play(sound_data)
                        .unwrap();
                    stream.handle = Some(handle);
                    commands.entity(ent).try_insert(AudioSpawned);
                }
                Err(TryRecvError::Disconnected) => {
                    commands.entity(ent).try_insert(AudioSpawned);
                }
                Err(TryRecvError::Empty) => {
                    debug!("{ent:?} waiting for sound data");
                }
            }
        }

        let volume = stream.volume;
        if let Some(handle) = stream.handle.as_mut() {
            if Some(scene.root) == containing_scene {
                let _ = handle.set_volume(volume as f64, Tween::default());
            } else {
                let _ = handle.set_volume(0.0, Tween::default());
            }
        }
    }
}
