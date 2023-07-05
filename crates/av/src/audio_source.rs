use std::time::Duration;

use bevy::prelude::*;
use bevy_kira_audio::{
    prelude::{AudioEmitter, AudioReceiver},
    AudioControl, AudioInstance, AudioTween,
};
use common::{structs::PrimaryCameraRes, util::TryInsertEx};
use dcl_component::proto_components::sdk::components::PbAudioSource;
use ipfs::IpfsLoaderExt;
use scene_runner::{renderer_context::RendererSceneContext, SceneEntity};

#[derive(Component, Debug)]
pub struct AudioSource(PbAudioSource);

impl From<PbAudioSource> for AudioSource {
    fn from(value: PbAudioSource) -> Self {
        Self(value)
    }
}

pub(crate) fn setup_audio(mut commands: Commands, camera: Res<PrimaryCameraRes>) {
    commands.entity(camera.0).try_insert(AudioReceiver);
}

#[allow(clippy::type_complexity)]
pub(crate) fn update_audio(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &SceneEntity,
            &AudioSource,
            Option<&Handle<bevy_kira_audio::AudioSource>>,
            Option<&mut AudioEmitter>,
        ),
        Changed<AudioSource>,
    >,
    scenes: Query<&RendererSceneContext>,
    audio: Res<bevy_kira_audio::Audio>,
    asset_server: Res<AssetServer>,
    mut audio_instances: ResMut<Assets<AudioInstance>>,
) {
    for (ent, scene_ent, audio_source, maybe_source, maybe_emitter) in query.iter_mut() {
        // preload clips
        let h_audio = match maybe_source {
            Some(instance) => instance.clone_weak(),
            None => {
                let Ok(scene) = scenes.get(scene_ent.root) else {
                    warn!("failed to load audio source scene");
                    continue;
                };

                let Ok(handle) = asset_server.load_content_file(&audio_source.0.audio_clip_url, &scene.hash) else {
                    warn!("failed to load content file");
                    continue;
                };

                let h_audio = handle.clone_weak();
                commands.entity(ent).try_insert(handle);
                h_audio
            }
        };

        if audio_source.0.playing() {
            // stop previous - is this right?
            if let Some(emitter) = maybe_emitter {
                for h_instance in emitter.instances.iter() {
                    if let Some(instance) = audio_instances.get_mut(h_instance) {
                        instance.stop(AudioTween::linear(Duration::ZERO));
                    }
                }
            }

            let mut instance = &mut audio.play(h_audio);
            if audio_source.0.r#loop() {
                instance = instance.looped();
            }

            if let Some(volume) = audio_source.0.volume {
                instance = instance
                    .with_volume(bevy_kira_audio::prelude::Volume::Amplitude(volume as f64));
            }

            let instance = instance.handle();
            commands.entity(ent).try_insert(AudioEmitter {
                instances: vec![instance],
            });
        } else if let Some(mut emitter) = maybe_emitter {
            // stop running
            for h_instance in emitter.instances.iter() {
                if let Some(instance) = audio_instances.get_mut(h_instance) {
                    instance.stop(AudioTween::linear(Duration::ZERO));
                }
            }
            emitter.instances.clear();
        }
    }
}