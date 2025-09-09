use std::path::PathBuf;

use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
    render::view::RenderLayers,
};
use bevy_kira_audio::{AudioControl, AudioInstance, AudioTween};
use common::{
    sets::SetupSets,
    structs::{
        AudioEmitter, AudioSettings, AudioType, PrimaryCameraRes, PrimaryUser, SystemAudio,
        PRIMARY_AVATAR_LIGHT_LAYER_INDEX,
    },
    util::{AudioReceiver, VolumePanning},
};
use dcl::interface::ComponentPosition;
use dcl_component::{proto_components::sdk::components::PbAudioSource, SceneComponentId};
use ipfs::{
    ipfs_path::{IpfsPath, IpfsType},
    IpfsAssetServer,
};
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
                map_scene_audio_sources,
                manage_audio_sources,
                play_system_audio,
            )
                .chain()
                .after(TransformSystem::TransformPropagate),
        );
        app.add_systems(Startup, setup_audio.in_set(SetupSets::Main));
    }
}

fn setup_audio(mut commands: Commands, camera: Res<PrimaryCameraRes>) {
    commands.entity(camera.0).try_insert(AudioReceiver {
        layers: RenderLayers::default().with(PRIMARY_AVATAR_LIGHT_LAYER_INDEX),
    });
}

fn map_scene_audio_sources(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &SceneEntity,
            &AudioSource,
            Option<&mut AudioEmitter>,
        ),
        Changed<AudioSource>,
    >,
    ipfas: IpfsAssetServer,
    scenes: Query<&RendererSceneContext>,
) {
    for (ent, scene_ent, audio_source, maybe_emitter) in query.iter_mut() {
        let Ok(scene) = scenes.get(scene_ent.root) else {
            warn!("failed to load audio source scene");
            continue;
        };
        let ipfs_path = PathBuf::from(&IpfsPath::new(IpfsType::new_content_file(
            scene.hash.to_owned(),
            audio_source.0.audio_clip_url.to_owned(),
        )));

        let handle = maybe_emitter
            .and_then(|mut existing| {
                if existing.handle.path().is_none_or(|p| p.path() != ipfs_path) {
                    None
                } else {
                    Some(std::mem::take(&mut existing.handle))
                }
            })
            .unwrap_or_else(|| ipfas.asset_server().load(ipfs_path));

        let seek_time = audio_source.0.current_time.and_then(|time| {
            if time < 1e6 {
                Some(time)
            } else {
                warn!(
                    "ignoring ridiculous time offset {} for audio clip `{}`",
                    time, audio_source.0.audio_clip_url
                );
                None
            }
        });

        let emitter = AudioEmitter {
            handle,
            playing: audio_source.0.playing(),
            playback_speed: audio_source.0.pitch.unwrap_or(1.0),
            r#loop: audio_source.0.r#loop(),
            volume: audio_source.0.volume.unwrap_or(1.0),
            global: audio_source.0.global(),
            seek_time,
            ty: AudioType::Scene,
        };

        info!("emitter: {emitter:?}");

        commands.entity(ent).try_insert(emitter);
    }
}

#[derive(Component)]
pub struct Playing;

#[derive(Component)]
pub struct RetryEmitter;

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn manage_audio_sources(
    mut commands: Commands,
    query: Query<
        (
            Entity,
            Ref<AudioEmitter>,
            Option<&GlobalTransform>,
            Option<&SceneEntity>,
            Option<&RenderLayers>,
            Option<&RetryEmitter>,
        ),
        Or<(Changed<AudioEmitter>, With<Playing>)>,
    >,
    mut instances: Local<
        HashMap<Entity, (AssetId<bevy_kira_audio::AudioSource>, Handle<AudioInstance>)>,
    >,
    audio: Res<bevy_kira_audio::Audio>,
    mut instance_assets: ResMut<Assets<AudioInstance>>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
    settings: Res<AudioSettings>,
    pan: VolumePanning,
) {
    let current_scenes = player
        .single()
        .ok()
        .map(|p| containing_scene.get(p))
        .unwrap_or_default();

    let mut prev_instances = std::mem::take(&mut *instances);

    for (ent, emitter, maybe_gt, maybe_scene_ent, maybe_layers, maybe_retry) in query.iter() {
        commands.entity(ent).remove::<RetryEmitter>();

        if !emitter.playing {
            if let Some((_, instance)) = prev_instances.remove(&ent) {
                if let Some(instance) = instance_assets.get_mut(instance.id()) {
                    instance.stop(AudioTween::default());
                }
            }

            commands.entity(ent).remove::<Playing>();
            continue;
        }

        let existing = match prev_instances.remove(&ent) {
            Some((id, h_instance)) => {
                if id == emitter.handle.id() {
                    let Some(instance) = instance_assets.get_mut(h_instance.id()) else {
                        commands.entity(ent).try_insert(RetryEmitter);
                        continue;
                    };

                    if matches!(
                        instance.state(),
                        bevy_kira_audio::PlaybackState::Playing { .. }
                    ) {
                        // reuse existing only if same source AND still playing
                        instances.insert(ent, (id, h_instance));
                        Some(instance)
                    } else {
                        None
                    }
                } else {
                    if let Some(instance) = instance_assets.get_mut(h_instance.id()) {
                        instance.stop(AudioTween::default());
                    }
                    None
                }
            }
            None => None,
        };

        if existing.is_none() && !emitter.is_changed() && maybe_retry.is_none() {
            commands.entity(ent).remove::<Playing>();
            continue;
        }

        let source_volume = match emitter.ty {
            AudioType::Voice => settings.voice(),
            AudioType::System => settings.system(),
            AudioType::Avatar => settings.avatar(),
            AudioType::Scene => {
                if maybe_scene_ent.is_some_and(|se| current_scenes.contains(&se.root)) {
                    emitter.volume * settings.scene()
                } else {
                    0.0
                }
            }
        };

        let (emitter_volume, panning) = match (emitter.global, maybe_gt) {
            (false, Some(gt)) => pan.volume_and_panning(gt.translation(), maybe_layers),
            _ => (1.0, 0.5),
        };

        match existing {
            None => {
                commands.entity(ent).insert(Playing);
                let mut new_instance = audio.play(emitter.handle.clone());

                new_instance
                    .with_volume((source_volume * emitter_volume) as f64)
                    .with_panning(panning as f64)
                    .with_playback_rate(emitter.playback_speed as f64);

                if emitter.r#loop {
                    new_instance.looped();
                }

                if let Some(time) = emitter.seek_time {
                    new_instance.start_from(time as f64);
                }

                instances.insert(ent, (emitter.handle.id(), new_instance.handle()));
            }
            Some(instance) => {
                if emitter.is_changed() {
                    instance
                        .set_playback_rate(emitter.playback_speed as f64, AudioTween::default());
                    instance.set_loop(emitter.r#loop);
                    if let Some(time) = emitter.seek_time {
                        instance.seek_to(time as f64);
                    }
                }

                instance.set_volume(
                    (source_volume * emitter_volume) as f64,
                    AudioTween::default(),
                );
                instance.set_panning(panning as f64, AudioTween::default());
            }
        };
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
