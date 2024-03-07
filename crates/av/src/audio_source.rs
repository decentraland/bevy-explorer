use bevy::{prelude::*, utils::HashSet};
use bevy_kira_audio::{
    prelude::{AudioEmitter, AudioReceiver},
    AudioControl, AudioInstance, AudioTween,
};
use common::{
    sets::{SceneSets, SetupSets},
    structs::{PrimaryCameraRes, PrimaryUser},
};
use dcl::interface::ComponentPosition;
use dcl_component::{proto_components::sdk::components::PbAudioSource, SceneComponentId};
use ipfs::IpfsAssetServer;
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

#[derive(Component)]
pub struct AudioSourceState {
    handle: Handle<bevy_kira_audio::AudioSource>,
    clip_url: String,
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_audio(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &SceneEntity,
            &AudioSource,
            Option<&mut AudioSourceState>,
            Option<&mut AudioEmitter>,
            &GlobalTransform,
        ),
        Changed<AudioSource>,
    >,
    scenes: Query<&RendererSceneContext>,
    audio: Res<bevy_kira_audio::Audio>,
    ipfas: IpfsAssetServer,
    mut audio_instances: ResMut<Assets<AudioInstance>>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
    cam: Query<&GlobalTransform, With<AudioReceiver>>,
) {
    let current_scenes = player
        .get_single()
        .ok()
        .map(|p| containing_scene.get(p))
        .unwrap_or_default();

    let gt = cam.get_single().unwrap_or(&GlobalTransform::IDENTITY);

    for (ent, scene_ent, audio_source, maybe_source, maybe_emitter, egt) in query.iter_mut() {
        let mut new_state = None;
        // preload clips
        let state = match maybe_source {
            Some(state) if state.clip_url == audio_source.0.audio_clip_url => state.into_inner(),
            _ => {
                // stop any previous different clips
                if let Some(emitter) = maybe_emitter.as_ref() {
                    for h_instance in emitter.instances.iter() {
                        if let Some(instance) = audio_instances.get_mut(h_instance) {
                            instance.stop(AudioTween::default());
                        }
                    }
                }

                let Ok(scene) = scenes.get(scene_ent.root) else {
                    warn!("failed to load audio source scene");
                    continue;
                };

                let Ok(handle) =
                    ipfas.load_content_file(&audio_source.0.audio_clip_url, &scene.hash)
                else {
                    warn!("failed to load content file");
                    continue;
                };

                new_state = Some(AudioSourceState {
                    handle,
                    clip_url: audio_source.0.audio_clip_url.clone(),
                });

                new_state.as_mut().unwrap()
            }
        };

        if audio_source.0.playing() {
            debug!(
                "play {:?} @ {} vs {}",
                audio_source.0,
                egt.translation(),
                gt.translation()
            );

            let volume = if current_scenes.contains(&scene_ent.root) {
                audio_source.0.volume.unwrap_or(1.0)
            } else {
                0.0
            };
            let playback_rate = audio_source.0.pitch.unwrap_or(1.0) as f64;

            // get existing audio or create new
            let maybe_playing_instance = maybe_emitter
                .as_ref()
                .and_then(|emitter| emitter.instances.first())
                .and_then(|h_instance| {
                    let instance = audio_instances.get_mut(h_instance)?;
                    matches!(
                        instance.state(),
                        bevy_kira_audio::PlaybackState::Playing { .. }
                    )
                    .then_some(instance)
                });

            match maybe_playing_instance {
                Some(playing_instance) => {
                    playing_instance.set_loop(audio_source.0.r#loop());
                    playing_instance.set_volume(
                        bevy_kira_audio::prelude::Volume::Amplitude(volume as f64),
                        AudioTween::default(),
                    );
                    playing_instance.set_playback_rate(playback_rate, AudioTween::default());
                }
                None => {
                    let mut new_instance = &mut audio.play(state.handle.clone());
                    if audio_source.0.r#loop() {
                        new_instance = new_instance.looped();
                    }
                    new_instance = new_instance
                        .with_volume(bevy_kira_audio::prelude::Volume::Amplitude(volume as f64));
                    new_instance =
                        new_instance.with_playback_rate(audio_source.0.pitch.unwrap_or(1.0) as f64);

                    commands.entity(ent).try_insert(AudioEmitter {
                        instances: vec![new_instance.handle()],
                    });
                }
            };
        } else if let Some(emitter) = maybe_emitter {
            debug!("stop {:?}", audio_source.0);
            // stop running
            for h_instance in emitter.instances.iter() {
                if let Some(instance) = audio_instances.get_mut(h_instance) {
                    instance.stop(AudioTween::default());
                }
            }
        }

        if let Some(new_state) = new_state {
            commands.entity(ent).try_insert(new_state);
        }
    }
}

fn update_source_volume(
    query: Query<(&SceneEntity, &AudioSource, &AudioEmitter, &GlobalTransform)>,
    mut audio_instances: ResMut<Assets<AudioInstance>>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
    mut prev_scenes: Local<HashSet<Entity>>,
    receiver: Query<&GlobalTransform, With<AudioReceiver>>,
) {
    let current_scenes = player
        .get_single()
        .ok()
        .map(|p| containing_scene.get(p))
        .unwrap_or_default();

    let Ok(receiver) = receiver.get_single() else {
        return;
    };

    for (scene, source, emitter, transform) in query.iter() {
        if current_scenes.contains(&scene.root) {
            let sound_path = transform.translation() - receiver.translation();
            let volume = (1. - sound_path.length() / 125.0).clamp(0., 1.).powi(2)
                * source.0.volume.unwrap_or(1.0);

            let panning = if sound_path.length() > f32::EPSILON {
                let right_ear_angle = receiver.right().angle_between(sound_path);
                (right_ear_angle.cos() + 1.) / 2.
            } else {
                0.5
            };

            for h_instance in &emitter.instances {
                if let Some(instance) = audio_instances.get_mut(h_instance) {
                    instance.set_volume(volume as f64, AudioTween::default());
                    instance.set_panning(panning as f64, AudioTween::default());
                }
            }
        } else if prev_scenes.contains(&scene.root) {
            for h_instance in &emitter.instances {
                if let Some(instance) = audio_instances.get_mut(h_instance) {
                    instance.set_volume(0.0, AudioTween::default());
                }
            }
        }
    }

    *prev_scenes = current_scenes;
}
