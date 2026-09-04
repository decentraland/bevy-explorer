use std::marker::PhantomData;

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
use media::AVCommand;
use scene_runner::{ContainingScene, SceneEntity};
use tokio::sync::mpsc::error::TryRecvError;

use crate::{AVPlayer, AVPlayerSinks, AVSinks, AudioStream, VideoPlayer};

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

pub struct AudioSinkPlugin;

impl Plugin for AudioSinkPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(av_sinks_inserted::<AudioStream>);
        app.add_observer(av_sinks_inserted::<VideoPlayer>);
        app.add_observer(av_sinks_replaced::<AudioStream>);
        app.add_observer(av_sinks_replaced::<VideoPlayer>);

        app.add_systems(
            PostUpdate,
            (
                (
                    update_audio_player_volume::<AudioStream>,
                    update_audio_player_volume::<VideoPlayer>,
                ),
                (
                    av_sink_waiting_sound_data::<AudioStream>,
                    av_sink_waiting_sound_data::<VideoPlayer>,
                ),
            )
                .chain(),
        );
        app.add_systems(
            PostUpdate,
            (
                spawn_new_foreign_audio_sources,
                check_foreign_audio_source_still_playing,
                update_foreign_audio_player_volume,
            )
                .chain(),
        );
    }
}

#[derive(Component)]
pub struct AudioSpawned<T> {
    handle: <StreamingSoundData<AudioDecoderError> as kira::sound::SoundData>::Handle,
    _phantom: PhantomData<T>,
}

impl<T> AudioSpawned<T> {
    pub fn new(
        handle: <StreamingSoundData<AudioDecoderError> as kira::sound::SoundData>::Handle,
    ) -> Self {
        Self {
            handle,
            _phantom: Default::default(),
        }
    }
}

impl<T> Drop for AudioSpawned<T> {
    fn drop(&mut self) {
        self.handle.stop(Tween::default());
    }
}

#[derive(Component)]
struct WaitingSoundData;

fn av_sinks_inserted<T: AVPlayer>(
    trigger: Trigger<OnInsert, AVSinks<T>>,
    mut commands: Commands,
    mut av_sinks: Query<&mut AVSinks<T>>,
    mut audio_manager: NonSendMut<bevy_kira_audio::audio_output::AudioOutput<DefaultBackend>>,
) {
    let entity = trigger.target();
    let Ok(mut av_sinks) = av_sinks.get_mut(entity) else {
        unreachable!("Infallible query");
    };
    debug!(
        "AVSink<{}> inserted to {}",
        disqualified::ShortName::of::<T>(),
        entity
    );

    if let Some(audio_sink) = av_sinks.audio_sink_mut() {
        if let Ok(sound_data) = audio_sink.sound_data.try_recv() {
            start_sound::<T>(entity, &mut commands, &mut audio_manager, sound_data);
        } else {
            commands.entity(entity).try_insert(WaitingSoundData);
        }
    }
}

fn av_sinks_replaced<T: AVPlayer>(trigger: Trigger<OnReplace, AVSinks<T>>, mut commands: Commands) {
    let entity = trigger.target();
    debug!(
        "AVSink<{}> replaced on {}",
        disqualified::ShortName::of::<T>(),
        entity
    );
    commands.entity(entity).try_remove::<AudioSpawned<T>>();
}

fn start_sound<T: AVPlayer>(
    entity: Entity,
    commands: &mut Commands,
    audio_manager: &mut bevy_kira_audio::audio_output::AudioOutput<DefaultBackend>,
    sound_data: StreamingSoundData<AudioDecoderError>,
) {
    info!("{entity:?} received sound data!");
    let handle = audio_manager
        .manager
        .as_mut()
        .unwrap()
        .play(sound_data)
        .unwrap();
    commands
        .entity(entity)
        .try_insert(AudioSpawned::<T>::new(handle));
}

#[expect(clippy::type_complexity)]
fn update_audio_player_volume<T: AVPlayer>(
    mut streams: Query<
        (&SceneEntity, &mut AVSinks<T>, &mut AudioSpawned<T>),
        Without<ForeignAudioSource>,
    >,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
    settings: Res<AudioSettings>,
) {
    let containing_scenes = player
        .single()
        .ok()
        .map(|player| containing_scene.get(player))
        .unwrap_or_default();

    for (scene, mut av_player_sinks, mut audio_spawned) in streams.iter_mut() {
        if let Some(audio_sink) = av_player_sinks.audio_sink_mut() {
            let volume = audio_sink.volume * settings.scene();
            if containing_scenes.contains(&scene.root) {
                audio_spawned
                    .handle
                    .set_volume(volume as f64, Tween::default());
            } else {
                audio_spawned.handle.set_volume(0.0, Tween::default());
            }
        }
    }
}

fn spawn_new_foreign_audio_sources(
    mut commands: Commands,
    foreign_audio_sources: Populated<
        (Entity, &mut ForeignAudioSource),
        Without<AudioSpawned<ForeignAudioSource>>,
    >,
    mut audio_manager: NonSendMut<bevy_kira_audio::audio_output::AudioOutput<DefaultBackend>>,
) {
    for (entity, mut foreign_audio_source) in foreign_audio_sources.into_inner() {
        if let Some(sound_data) = foreign_audio_source
            .audio_receiver
            .as_mut()
            .and_then(|rx| rx.try_recv().ok())
        {
            info!("{entity:?} received foreign sound data!");
            let handle = audio_manager
                .manager
                .as_mut()
                .unwrap()
                .play(sound_data)
                .unwrap();

            commands
                .entity(entity)
                .try_insert(AudioSpawned::<ForeignAudioSource>::new(handle));
        }
    }
}

fn check_foreign_audio_source_still_playing(
    mut commands: Commands,
    foreign_audio_sources: Populated<
        (Entity, &AudioSpawned<ForeignAudioSource>),
        With<ForeignAudioSource>,
    >,
) {
    for (entity, audio_spawned) in foreign_audio_sources.into_inner() {
        if !matches!(audio_spawned.handle.state(), PlaybackState::Playing) {
            commands
                .entity(entity)
                .try_remove::<AudioSpawned<ForeignAudioSource>>();
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_foreign_audio_player_volume(
    mut streams: Query<
        (
            &GlobalTransform,
            Option<&RenderLayers>,
            &mut AudioSpawned<ForeignAudioSource>,
        ),
        With<ForeignAudioSource>,
    >,
    pan: VolumePanning,
    settings: Res<AudioSettings>,
) {
    for (emitter_transform, render_layers, mut audio_spawned) in streams.iter_mut() {
        let (volume, panning) =
            pan.volume_and_panning(emitter_transform.translation(), render_layers);
        let volume = volume * settings.voice();

        audio_spawned
            .handle
            .set_volume(volume as f64, Tween::default());
        audio_spawned
            .handle
            .set_panning(panning as f64, Tween::default());
    }
}

fn av_sink_waiting_sound_data<T: AVPlayer>(
    mut commands: Commands,
    av_sinks: Populated<(Entity, &mut AVSinks<T>), With<WaitingSoundData>>,
    mut audio_manager: NonSendMut<bevy_kira_audio::audio_output::AudioOutput<DefaultBackend>>,
) {
    for (entity, mut av_sink) in av_sinks.into_inner() {
        if let Some(audio_sink) = av_sink.audio_sink_mut() {
            match audio_sink.sound_data.try_recv() {
                Ok(sound_data) => {
                    start_sound::<T>(entity, &mut commands, &mut audio_manager, sound_data)
                }
                Err(TryRecvError::Disconnected) => {
                    debug!("Sound data receiver is disconnected.");
                    commands.entity(entity).try_remove::<WaitingSoundData>();
                }
                Err(TryRecvError::Empty) => {}
            }
        }
    }
}
