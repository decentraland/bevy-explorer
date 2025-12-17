use bevy::{prelude::*, render::view::RenderLayers};
use common::{
    structs::{AudioDecoderError, AudioSettings, PrimaryUser},
    util::VolumePanning,
};
use comms::global_crdt::ForeignAudioSource;
use kira::{
    manager::backend::DefaultBackend,
    sound::{streaming::StreamingSoundData, PlaybackState},
    tween::Tween,
};
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

impl Drop for AudioSpawned {
    fn drop(&mut self) {
        if let Some(mut handle) = self.0.take() {
            handle.stop(Tween::default());
        }
    }
}

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
    settings: Res<AudioSettings>,
) {
    if audio_manager.manager.is_none() {
        return;
    }

    let containing_scenes = player
        .single()
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
                    trace!("{ent:?} waiting for sound data");
                    commands.entity(ent).remove::<AudioSpawned>();
                }
            }
        }

        let volume = stream.volume * settings.scene();
        if let Some(handle) = maybe_spawned.as_mut().and_then(|a| a.0.as_mut()) {
            if containing_scenes.contains(&scene.root) {
                handle.set_volume(volume as f64, Tween::default());
            } else {
                handle.set_volume(0.0, Tween::default());
            }
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn spawn_and_locate_foreign_streams(
    mut commands: Commands,
    mut streams: Query<(
        Entity,
        &GlobalTransform,
        Option<&RenderLayers>,
        &mut ForeignAudioSource,
        Option<&mut AudioSpawned>,
    )>,
    mut audio_manager: NonSendMut<bevy_kira_audio::audio_output::AudioOutput<DefaultBackend>>,
    pan: VolumePanning,
    settings: Res<AudioSettings>,
) {
    if audio_manager.manager.is_none() {
        return;
    }

    for (ent, emitter_transform, render_layers, mut stream, mut maybe_spawned) in streams.iter_mut()
    {
        if let Some(spawned) = maybe_spawned.as_mut() {
            if spawned
                .0
                .as_ref()
                .is_some_and(|h| !matches!(h.state(), PlaybackState::Playing))
            {
                spawned.0 = None;
            }
        }

        if let Some(sound_data) = stream
            .audio_receiver
            .as_mut()
            .and_then(|rx| rx.try_recv().ok())
        {
            info!("{ent:?} received foreign sound data!");
            let handle = audio_manager
                .manager
                .as_mut()
                .unwrap()
                .play(sound_data)
                .unwrap();

            commands.entity(ent).try_insert(AudioSpawned(Some(handle)));
        }

        if let Some(handle) = maybe_spawned.as_mut().and_then(|a| a.0.as_mut()) {
            let (volume, panning) =
                pan.volume_and_panning(emitter_transform.translation(), render_layers);
            let volume = volume * settings.voice();

            handle.set_volume(volume as f64, Tween::default());
            handle.set_panning(panning as f64, Tween::default());
        }
    }
}
