use bevy::{platform::collections::HashMap, prelude::*, render::view::RenderLayers};
use common::{
    structs::{AudioDecoderError, AudioSettings, PrimaryUser},
    util::VolumePanning,
};
use comms::{SceneRoom, global_crdt::{ForeignAudioSource, ForeignPlayer}};
use kira::{manager::backend::DefaultBackend, sound::streaming::StreamingSoundData, tween::Tween};
use scene_runner::{ContainingScene, SceneEntity};
use system_bridge::{SystemApi, VoiceMessage};
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
                    debug!("{ent:?} waiting for sound data");
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
        match stream.receiver.try_recv() {
            Ok((sound_data, channel)) => {
                info!("{ent:?} received foreign sound data!");
                let handle = audio_manager
                    .manager
                    .as_mut()
                    .unwrap()
                    .play(sound_data)
                    .unwrap();

                commands.entity(ent).try_insert(AudioSpawned(Some(handle)));
                stream.active_transport = Some(channel);
            }
            Err(TryRecvError::Disconnected) => (),
            Err(TryRecvError::Empty) => (),
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

pub fn pipe_voice_to_scene(
    mut requests: EventReader<SystemApi>,
    sources: Query<(&ForeignPlayer, &ForeignAudioSource, &AudioSpawned), With<ForeignAudioSource>>,
    mut senders: Local<Vec<tokio::sync::mpsc::UnboundedSender<VoiceMessage>>>,
    mut current_active: Local<HashMap<ethers_core::types::Address, String>>,
    rooms: Query<&SceneRoom>,
) {
    senders.extend(requests.read().filter_map(|ev| {
        if let SystemApi::GetVoiceStream(sender) = ev {
            Some(sender.clone())
        } else {
            None
        }
    }));

    senders.retain(|s| !s.is_closed());

    let mut prev_active = std::mem::take(&mut *current_active);

    for (source, audio, spawned) in sources.iter() {
        let Some(handle) = spawned.0.as_ref() else {
            continue;
        };
        if handle.state() == kira::sound::PlaybackState::Playing {
            let channel = match audio.active_transport.and_then(|t| rooms.get(t).ok()) {
                Some(room) => room.0.clone(),
                None => "Nearby".to_string(),
            };
            if prev_active.remove(&source.address).as_ref() != Some(&channel) {
                for sender in senders.iter() {
                    let _ = sender.send(VoiceMessage {
                        sender_address: format!("{:#x}", source.address),
                        channel: channel.clone(),
                        active: true,
                    });
                }
            }

            current_active.insert(source.address, channel);
        }
    }

    for (address, channel) in prev_active.drain() {
        for sender in senders.iter() {
            let _ = sender.send(VoiceMessage {
                sender_address: format!("{address:#x}"),
                channel: channel.clone(),
                active: false,
            });
        }
    }
}
