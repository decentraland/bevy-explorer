use bevy::prelude::*;
use bevy_kira_audio::{
    prelude::{AudioEmitter, AudioReceiver},
    AudioControl, AudioInstance, AudioTween,
};
use common::{
    sets::{SceneSets, SetupSets},
    structs::{PrimaryCameraRes, PrimaryUser},
    util::TryInsertEx,
};
use dcl::interface::ComponentPosition;
use dcl_component::{proto_components::sdk::components::PbAudioSource, SceneComponentId};
use ipfs::IpfsLoaderExt;
use scene_runner::{
    renderer_context::RendererSceneContext, update_world::AddCrdtInterfaceExt, ContainingScene,
    SceneEntity,
};

#[derive(Component, Debug)]
pub struct AudioSource(PbAudioSource);

impl From<PbAudioSource> for AudioSource {
    fn from(value: PbAudioSource) -> Self {
        Self(value)
    }
}

pub struct AudioSourcePlugin;

impl Plugin for AudioSourcePlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbAudioSource, AudioSource>(
            SceneComponentId::AUDIO_SOURCE,
            ComponentPosition::EntityOnly,
        );
        app.add_systems(
            Update,
            (update_audio, update_source_volume).in_set(SceneSets::PostLoop),
        );
        app.add_systems(Startup, setup_audio.in_set(SetupSets::Main));
    }
}

fn setup_audio(mut commands: Commands, camera: Res<PrimaryCameraRes>) {
    commands.entity(camera.0).try_insert(AudioReceiver);
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_audio(
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
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
) {
    let current_scene = player
        .get_single()
        .ok()
        .and_then(|p| containing_scene.get(p));

    for (ent, scene_ent, audio_source, maybe_source, maybe_emitter) in query.iter_mut() {
        // preload clips
        let h_audio = match maybe_source {
            Some(instance) => instance.clone_weak(),
            None => {
                let Ok(scene) = scenes.get(scene_ent.root) else {
                    warn!("failed to load audio source scene");
                    continue;
                };

                let Ok(handle) =
                    asset_server.load_content_file(&audio_source.0.audio_clip_url, &scene.hash)
                else {
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
                        instance.stop(AudioTween::default());
                    }
                }
            }

            let mut instance = &mut audio.play(h_audio);
            if audio_source.0.r#loop() {
                instance = instance.looped();
            }

            let volume = if Some(scene_ent.root) == current_scene {
                audio_source.0.volume.unwrap_or(1.0)
            } else {
                0.0
            };
            instance =
                instance.with_volume(bevy_kira_audio::prelude::Volume::Amplitude(volume as f64));

            let instance = instance.handle();
            commands.entity(ent).try_insert(AudioEmitter {
                instances: vec![instance],
            });
        } else if let Some(mut emitter) = maybe_emitter {
            // stop running
            for h_instance in emitter.instances.iter() {
                if let Some(instance) = audio_instances.get_mut(h_instance) {
                    instance.stop(AudioTween::default());
                }
            }
            emitter.instances.clear();
        }
    }
}

fn update_source_volume(
    query: Query<(&SceneEntity, &AudioSource, &AudioEmitter, &GlobalTransform)>,
    mut audio_instances: ResMut<Assets<AudioInstance>>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
    mut prev_scene: Local<Option<Entity>>,
    receiver: Query<&GlobalTransform, With<AudioReceiver>>,
) {
    let current_scene = player
        .get_single()
        .ok()
        .and_then(|p| containing_scene.get(p));

    let Ok(receiver) = receiver.get_single() else {
        return;
    };

    for (scene, source, emitter, transform) in query.iter() {
        if current_scene == Some(scene.root) {
            let sound_path = transform.translation() - receiver.translation();
            let volume = (1. - sound_path.length() / 25.0).clamp(0., 1.).powi(2)
                * source.0.volume.unwrap_or(1.0);

            let right_ear_angle = receiver.right().angle_between(sound_path);
            let panning = (right_ear_angle.cos() + 1.) / 2.;

            for h_instance in &emitter.instances {
                if let Some(instance) = audio_instances.get_mut(h_instance) {
                    instance.set_volume(volume as f64, AudioTween::default());
                    instance.set_panning(panning as f64, AudioTween::default());
                }
            }
        } else if *prev_scene == Some(scene.root) {
            for h_instance in &emitter.instances {
                if let Some(instance) = audio_instances.get_mut(h_instance) {
                    instance.set_volume(0.0, AudioTween::default());
                }
            }
        }
    }

    *prev_scene = current_scene;
}
