use bevy::prelude::*;
use common::{
    structs::{AudioDecoderError, PrimaryCamera, PrimaryUser},
    util::TryInsertEx,
};
use comms::global_crdt::ForeignAudioSource;
use kira::{manager::backend::DefaultBackend, sound::streaming::StreamingSoundData, tween::Tween};
use scene_runner::{ContainingScene, SceneEntity};
use tokio::sync::mpsc::error::TryRecvError;

use crate::stream_processor::AVCommand;

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
pub struct AudioSpawned(
    Option<<StreamingSoundData<AudioDecoderError> as kira::sound::SoundData>::Handle>,
);

// TODO integrate better with bevy_kira_audio to avoid logic on a main-thread system (NonSendMut forces this system to the main thread)
pub fn spawn_audio_streams(
    mut commands: Commands,
    mut streams: Query<(
        Entity,
        &SceneEntity,
        &mut AudioSink,
        Option<&mut AudioSpawned>,
    )>,
    mut audio_manager: NonSendMut<bevy_kira_audio::audio_output::AudioOutput<DefaultBackend>>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
) {
    let containing_scenes = player
        .get_single()
        .ok()
        .map(|player| containing_scene.get(player))
        .unwrap_or_default();

    for (ent, scene, mut stream, mut maybe_spawned) in streams.iter_mut() {
        if maybe_spawned.is_none() || stream.is_changed() {
            match stream.sound_data.try_recv() {
                Ok(sound_data) => {
                    info!("{ent:?} received sound data!");
                    let handle = audio_manager
                        .manager
                        .as_mut()
                        .unwrap()
                        .play(sound_data)
                        .unwrap();
                    commands.entity(ent).try_insert(AudioSpawned(Some(handle)));
                }
                Err(TryRecvError::Disconnected) => {
                    commands.entity(ent).try_insert(AudioSpawned(None));
                }
                Err(TryRecvError::Empty) => {
                    debug!("{ent:?} waiting for sound data");
                    commands.entity(ent).remove::<AudioSpawned>();
                }
            }
        }

        let volume = stream.volume;
        if let Some(handle) = maybe_spawned.as_mut().and_then(|a| a.0.as_mut()) {
            if containing_scenes.contains(&scene.root) {
                let _ = handle.set_volume(volume as f64, Tween::default());
            } else {
                let _ = handle.set_volume(0.0, Tween::default());
            }
        }
    }
}

const MAX_CHAT_DISTANCE: f32 = 25.0;

pub fn spawn_and_locate_foreign_streams(
    mut commands: Commands,
    mut streams: Query<(
        Entity,
        &GlobalTransform,
        &mut ForeignAudioSource,
        Option<&mut AudioSpawned>,
    )>,
    mut audio_manager: NonSendMut<bevy_kira_audio::audio_output::AudioOutput<DefaultBackend>>,
    receiver: Query<&GlobalTransform, With<PrimaryCamera>>,
) {
    let Ok(receiver_transform) = receiver.get_single() else {
        return;
    };

    for (ent, emitter_transform, mut stream, mut maybe_spawned) in streams.iter_mut() {
        match stream.0.try_recv() {
            Ok(sound_data) => {
                info!("{ent:?} received foreign sound data!");
                let handle = audio_manager
                    .manager
                    .as_mut()
                    .unwrap()
                    .play(sound_data)
                    .unwrap();
                commands.entity(ent).try_insert(AudioSpawned(Some(handle)));
            }
            Err(TryRecvError::Disconnected) => (),
            Err(TryRecvError::Empty) => (),
        }

        if let Some(handle) = maybe_spawned.as_mut().and_then(|a| a.0.as_mut()) {
            let sound_path = emitter_transform.translation() - receiver_transform.translation();
            let volume = (1. - sound_path.length() / MAX_CHAT_DISTANCE)
                .clamp(0., 1.)
                .powi(2);

            let right_ear_angle = receiver_transform.right().angle_between(sound_path);
            let panning = (right_ear_angle.cos() + 1.) / 2.;

            let _ = handle.set_volume(volume as f64, Tween::default());
            let _ = handle.set_panning(panning as f64, Tween::default());
        }
    }
}
