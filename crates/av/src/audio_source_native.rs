use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
    render::view::RenderLayers,
};
use bevy_kira_audio::{AudioControl, AudioInstance, AudioTween};
use common::{
    structs::{AudioEmitter, AudioSettings, AudioType, OneShotAudio, PrimaryUser, SystemAudio},
    util::VolumePanning,
};
use ipfs::IpfsAssetServer;
use scene_runner::{ContainingScene, SceneEntity};

pub struct AudioSourcePluginImpl;

impl Plugin for AudioSourcePluginImpl {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (
                manage_audio_sources,
                despawn_finished_one_shots,
                play_system_audio,
            )
                .chain()
                .after(TransformSystem::TransformPropagate),
        );
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
        commands.entity(ent).try_remove::<RetryEmitter>();

        if !emitter.playing {
            if let Some((_, h_instance)) = prev_instances.remove(&ent) {
                if let Some(mut instance) = instance_assets.remove(h_instance.id()) {
                    instance.stop(AudioTween::default());
                }
            }

            commands.entity(ent).try_remove::<Playing>();
            continue;
        }

        let existing = match prev_instances.remove(&ent) {
            Some((id, h_instance)) => {
                if id == emitter.handle.id() {
                    let Some(instance) = instance_assets.get_mut(h_instance.id()) else {
                        commands.entity(ent).try_insert(RetryEmitter);
                        instances.insert(ent, (id, h_instance));
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
                        instance_assets.remove(h_instance.id());
                        None
                    }
                } else {
                    if let Some(mut instance) = instance_assets.remove(h_instance.id()) {
                        instance.stop(AudioTween::default());
                    }
                    None
                }
            }
            None => None,
        };

        if existing.is_none() && !emitter.is_changed() && maybe_retry.is_none() {
            commands.entity(ent).try_remove::<Playing>();
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
                commands.entity(ent).try_insert(Playing);
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

    for (_, (_, h_instance)) in prev_instances.drain() {
        if let Some(mut instance) = instance_assets.remove(h_instance.id()) {
            instance.stop(AudioTween::default());
        }
    }
}

// manage_audio_sources removes `Playing` once an instance stops, so one-shot emitters
// without it (and not waiting on a retry) are done
#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn despawn_finished_one_shots(
    mut commands: Commands,
    finished: Query<Entity, (With<OneShotAudio>, Without<RetryEmitter>, Without<Playing>)>,
) {
    for ent in finished.iter() {
        commands.entity(ent).despawn();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn despawns_only_finished_one_shot_emitters() {
        let mut app = App::new();
        app.add_systems(Update, despawn_finished_one_shots);

        let playing = app
            .world_mut()
            .spawn((AudioEmitter::default(), OneShotAudio, Playing))
            .id();
        let retrying = app
            .world_mut()
            .spawn((AudioEmitter::default(), OneShotAudio, Playing, RetryEmitter))
            .id();
        let finished = app
            .world_mut()
            .spawn((AudioEmitter::default(), OneShotAudio))
            .id();
        let persistent = app.world_mut().spawn(AudioEmitter::default()).id();

        app.update();

        assert!(app.world().get_entity(playing).is_ok());
        assert!(app.world().get_entity(retrying).is_ok());
        assert!(app.world().get_entity(finished).is_err());
        assert!(app.world().get_entity(persistent).is_ok());

        // once the instance stops, manage_audio_sources drops the marker
        app.world_mut().entity_mut(playing).remove::<Playing>();
        app.update();

        assert!(app.world().get_entity(playing).is_err());
    }
}
