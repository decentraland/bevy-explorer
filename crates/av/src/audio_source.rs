use std::time::Duration;

use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
    render::view::RenderLayers,
};
use bevy_kira_audio::{AudioControl, AudioInstance, AudioTween};
use common::{
    sets::SetupSets,
    structs::{AudioEmitter, AudioSettings, PrimaryCameraRes, PrimaryUser, SystemAudio},
    util::{AudioReceiver, VolumePanning},
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
        app.add_plugins(bevy_kira_audio::AudioPlugin);

        app.add_event::<SystemAudio>();
        app.add_crdt_lww_component::<PbAudioSource, AudioSource>(
            SceneComponentId::AUDIO_SOURCE,
            ComponentPosition::EntityOnly,
        );
        app.add_systems(
            PostUpdate,
            (
                create_audio_sources,
                update_audio_sources,
                play_system_audio,
                remove_dead_audio_assets,
            )
                .after(TransformSystem::TransformPropagate),
        );
        app.add_systems(Startup, setup_audio.in_set(SetupSets::Main));
    }
}

fn setup_audio(mut commands: Commands, camera: Res<PrimaryCameraRes>) {
    commands
        .entity(camera.0)
        .try_insert(AudioReceiver::default());
}

#[derive(Component)]
pub struct AudioSourceState {
    handle: Handle<bevy_kira_audio::AudioSource>,
    clip_url: String,
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn create_audio_sources(
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
    settings: Res<AudioSettings>,
) {
    let current_scenes = player
        .single()
        .ok()
        .map(|p| containing_scene.get(p))
        .unwrap_or_default();

    let gt = cam.single().unwrap_or(&GlobalTransform::IDENTITY);

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

                debug!("clip {:?}", audio_source.0);
                new_state = Some(AudioSourceState {
                    handle,
                    clip_url: audio_source.0.audio_clip_url.clone(),
                });

                new_state.as_mut().unwrap()
            }
        };

        if audio_source.0.playing() {
            debug!(
                "play {:?} @ [{:?}] {} vs {}",
                audio_source.0,
                ent,
                egt.translation(),
                gt.translation()
            );

            let volume = if current_scenes.contains(&scene_ent.root) {
                audio_source.0.volume.unwrap_or(1.0) * settings.scene()
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
                    if let Some(time) = audio_source.0.current_time {
                        if time < 1e6 {
                            playing_instance.seek_to(time as f64);
                        } else {
                            warn!(
                                "ignoring ridiculous time offset {} for audio clip `{}`",
                                time, audio_source.0.audio_clip_url
                            );
                        }
                    }
                }
                None => {
                    let mut new_instance = &mut audio.play(state.handle.clone());
                    debug!("created {:?}", new_instance.handle());
                    if audio_source.0.r#loop() {
                        new_instance = new_instance.looped();
                    }
                    new_instance = new_instance
                        .with_volume(bevy_kira_audio::prelude::Volume::Amplitude(volume as f64));
                    new_instance =
                        new_instance.with_playback_rate(audio_source.0.pitch.unwrap_or(1.0) as f64);

                    if let Some(time) = audio_source.0.current_time {
                        if time < 1e6 {
                            new_instance.start_from(time as f64);
                        } else {
                            warn!(
                                "ignoring ridiculous start time {} for audio clip `{}`",
                                time, audio_source.0.audio_clip_url
                            );
                        }
                    }

                    commands.entity(ent).try_insert(AudioEmitter {
                        instances: vec![new_instance.handle()],
                    });
                }
            };
        } else if let Some(emitter) = maybe_emitter {
            debug!("stop {:?} ({:?})", audio_source.0, emitter.instances);
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

fn remove_dead_audio_assets(mut audio_instances: ResMut<Assets<AudioInstance>>) {
    let mut dead = HashSet::new();
    for (h, instance) in audio_instances.iter() {
        if instance.state() == bevy_kira_audio::PlaybackState::Stopped {
            dead.insert(h);
        }
    }

    for h in dead {
        audio_instances.remove(h);
    }
}

fn play_system_audio(
    mut events: EventReader<SystemAudio>,
    audio: Res<bevy_kira_audio::Audio>,
    ipfas: IpfsAssetServer,
    audio_instances: Res<Assets<AudioInstance>>,
    settings: Res<AudioSettings>,
    mut playing: Local<HashSet<Handle<AudioInstance>>>,
) {
    for event in events.read() {
        let h_clip = ipfas.asset_server().load(&event.0);
        let volume = settings.system();
        let h_instance = audio
            .play(h_clip)
            .with_volume(bevy_kira_audio::prelude::Volume::Amplitude(volume as f64))
            .handle();
        playing.insert(h_instance);
        debug!("play system audio {}", event.0);
    }

    playing.retain(|h_instance| {
        let retain = audio_instances
            .get(h_instance)
            .is_some_and(|instance| instance.state().position().is_some());
        if !retain {
            debug!("drop system audio");
        }
        retain
    })
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_audio_sources(
    mut query: Query<(
        Entity,
        Option<&SceneEntity>,
        Option<&AudioSource>,
        &mut AudioEmitter,
        Option<&RenderLayers>,
        &GlobalTransform,
    )>,
    mut audio_instances: ResMut<Assets<AudioInstance>>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
    mut prev_scenes: Local<HashSet<Entity>>,
    pan: VolumePanning,
    settings: Res<AudioSettings>,
    mut all_instances: Local<HashMap<Entity, Vec<Handle<AudioInstance>>>>,
) {
    let current_scenes = player
        .single()
        .ok()
        .map(|p| containing_scene.get(p))
        .unwrap_or_default();

    let mut prev_instances = std::mem::take(&mut *all_instances);

    for (ent, maybe_scene, maybe_source, mut emitter, layers, transform) in query.iter_mut() {
        if maybe_scene.is_none_or(|scene| current_scenes.contains(&scene.root)) {
            let (volume, panning) = if maybe_source.is_some_and(|source| source.0.global()) {
                (
                    maybe_source
                        .and_then(|source| source.0.volume)
                        .unwrap_or(1.0),
                    0.5,
                )
            } else {
                let volume_adjust = if maybe_scene.is_some() {
                    settings.scene()
                } else {
                    settings.avatar()
                };

                let (volume, panning) = pan.volume_and_panning(transform.translation(), layers);

                (volume * volume_adjust, panning)
            };

            for h_instance in emitter.instances.iter_mut() {
                if let Some(instance) = audio_instances.get_mut(h_instance) {
                    instance.set_volume(volume as f64, AudioTween::linear(Duration::ZERO));
                    instance.set_panning(panning as f64, AudioTween::default());
                }
            }
        } else if maybe_scene.is_some_and(|scene| prev_scenes.contains(&scene.root)) {
            debug!("set zero [{:?}] ({:?})", ent, emitter.instances);
            for h_instance in &emitter.instances {
                if let Some(instance) = audio_instances.get_mut(h_instance) {
                    instance.set_volume(0.0, AudioTween::default());
                }
            }
        }

        // remove old audios
        if let Some(prev_instances) = prev_instances.remove(&ent) {
            let current_ids = emitter
                .instances
                .iter()
                .map(|h| h.id())
                .collect::<HashSet<_>>();

            for h_instance in prev_instances {
                if !current_ids.contains(&h_instance.id()) {
                    debug!("stop removed {:?}", h_instance);
                    if let Some(instance) = audio_instances.get_mut(h_instance.id()) {
                        instance.stop(AudioTween::default());
                    }
                }
            }
        }

        all_instances.insert(ent, emitter.instances.clone());
    }

    for (_ent, prev_instances) in prev_instances {
        for h_instance in prev_instances {
            if let Some(instance) = audio_instances.get_mut(h_instance.id()) {
                debug!("stop dropped {:?}", h_instance);
                instance.stop(AudioTween::default());
            }
        }
    }

    *prev_scenes = current_scenes;
}
