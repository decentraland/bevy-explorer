use std::marker::PhantomData;

use bevy::{prelude::*, render::view::RenderLayers};
use common::{
    debug_panic,
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

use crate::{stream_processor::AVCommand, AVPlayer, AVPlayerSinks, AVSinks, InScene};

pub struct AudioSink {
    pub volume: f32,
    pub command_sender: tokio::sync::mpsc::UnboundedSender<AVCommand>,
    pub sound_data: tokio::sync::mpsc::Receiver<StreamingSoundData<AudioDecoderError>>,
    pub handle: Option<<StreamingSoundData<AudioDecoderError> as kira::sound::SoundData>::Handle>,
}

impl AudioSink {
    pub fn new(
        volume: f32,
        command_sender: tokio::sync::mpsc::UnboundedSender<AVCommand>,
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
pub struct AudioSpawned<T> {
    handle: Option<<StreamingSoundData<AudioDecoderError> as kira::sound::SoundData>::Handle>,
    _phantom: PhantomData<T>,
}

impl<T> AudioSpawned<T> {
    pub fn new(
        handle: Option<<StreamingSoundData<AudioDecoderError> as kira::sound::SoundData>::Handle>,
    ) -> Self {
        Self {
            handle,
            _phantom: Default::default(),
        }
    }
}

impl<T> Drop for AudioSpawned<T> {
    fn drop(&mut self) {
        if let Some(mut handle) = self.handle.take() {
            handle.stop(Tween::default());
        }
    }
}

#[derive(Event)]
pub struct ChangeAudioSinkVolume {
    pub volume: f32,
}

// TODO integrate better with bevy_kira_audio to avoid logic on a main-thread system (NonSendMut forces this system to the main thread)
#[expect(clippy::type_complexity)]
pub fn spawn_audio_streams<T: AVPlayer>(
    mut commands: Commands,
    mut streams: Query<(
        Entity,
        &SceneEntity,
        &mut AVSinks<T>,
        Option<&mut AudioSpawned<T>>,
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

    for (ent, scene, mut av_player_sinks, mut maybe_spawned) in streams.iter_mut() {
        let changed = av_player_sinks.is_changed();
        let stream = av_player_sinks.audio_sink_mut().unwrap();
        if maybe_spawned.is_none() || changed {
            match stream.sound_data.try_recv() {
                Ok(sound_data) => {
                    info!("{ent:?} received sound data!");
                    let handle = audio_manager
                        .manager
                        .as_mut()
                        .unwrap()
                        .play(sound_data)
                        .unwrap();
                    commands
                        .entity(ent)
                        .try_insert(AudioSpawned::<T>::new(Some(handle)));
                }
                Err(TryRecvError::Disconnected) => {
                    commands
                        .entity(ent)
                        .try_insert(AudioSpawned::<T>::new(None));
                }
                Err(TryRecvError::Empty) => {
                    trace!("{ent:?} waiting for sound data");
                    commands.entity(ent).remove::<AudioSpawned<T>>();
                }
            }
        }

        let volume = stream.volume * settings.scene();
        if let Some(handle) = maybe_spawned.as_mut().and_then(|a| a.handle.as_mut()) {
            if containing_scenes.contains(&scene.root) {
                handle.set_volume(volume as f64, Tween::default());
            } else {
                handle.set_volume(0.0, Tween::default());
            }
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn spawn_and_locate_foreign_streams<T: AVPlayer>(
    mut commands: Commands,
    mut streams: Query<(
        Entity,
        &GlobalTransform,
        Option<&RenderLayers>,
        &mut ForeignAudioSource,
        Option<&mut AudioSpawned<T>>,
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
                .handle
                .as_ref()
                .is_some_and(|h| !matches!(h.state(), PlaybackState::Playing))
            {
                spawned.handle = None;
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

            commands
                .entity(ent)
                .try_insert(AudioSpawned::<T>::new(Some(handle)));
        }

        if let Some(handle) = maybe_spawned.as_mut().and_then(|a| a.handle.as_mut()) {
            let (volume, panning) =
                pan.volume_and_panning(emitter_transform.translation(), render_layers);
            let volume = volume * settings.voice();

            handle.set_volume(volume as f64, Tween::default());
            handle.set_panning(panning as f64, Tween::default());
        }
    }
}

#[expect(clippy::type_complexity)]
pub fn change_audio_sink_volume<T: AVPlayer>(
    trigger: Trigger<ChangeAudioSinkVolume>,
    mut audio_sinks: Query<(Mut<AVSinks<T>>, Option<&mut AudioSpawned<T>>, Has<InScene>)>,
    audio_settings: Res<AudioSettings>,
) {
    let entity = trigger.target();
    if entity == Entity::PLACEHOLDER {
        debug_panic!("ChangeAudioSinkVolume is an entity event. Trigger it with `Commands::trigger_targets`.");
    }
    let ChangeAudioSinkVolume { volume } = trigger.event();

    let Ok((mut av_player_sinks, maybe_audio_spawned, in_scene)) = audio_sinks.get_mut(entity)
    else {
        debug_panic!("{entity} is not an AudioSink.");
    };

    // AudioSink is causing problems with change detection
    // so we bypass it here
    let av_player_sinks = av_player_sinks.bypass_change_detection();
    let audio_sink = av_player_sinks.audio_sink_mut().unwrap();
    audio_sink.volume = *volume;

    if let Some(mut audio_spawned) = maybe_audio_spawned {
        if let Some(handle) = audio_spawned.handle.as_mut() {
            if in_scene {
                handle.set_volume((volume * audio_settings.scene()) as f64, Tween::default());
            } else {
                handle.set_volume(0.0, Tween::default());
            }
        }
    }
}
