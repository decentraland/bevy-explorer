use bevy::{platform::collections::HashMap, prelude::*, render::view::RenderLayers};
use common::{
    structs::{AudioEmitter, AudioSettings, AudioType, PrimaryUser, SystemAudio},
    util::VolumePanning,
};
use ipfs::IpfsAssetServer;
use scene_runner::{ContainingScene, SceneEntity};

use crate::audio_host_wasm::{send, AudioCommand, InstanceId};

pub struct AudioSourcePluginImpl;

impl Plugin for AudioSourcePluginImpl {
    fn build(&self, app: &mut App) {
        // still use kira for audio source asset management
        app.init_resource::<WebAudio>();
        app.add_systems(
            PostUpdate,
            (
                manage_audio_sources,
                play_system_audio,
                |mut ctx: ResMut<WebAudio>,
                 asset_events: EventReader<AssetEvent<bevy_kira_audio::AudioSource>>,
                 assets: Res<Assets<bevy_kira_audio::AudioSource>>| {
                    ctx.tick(asset_events, assets);
                },
            )
                .chain()
                .after(TransformSystem::TransformPropagate),
        );
    }
}

/// The engine's view of the page's audio graphs ([`crate::audio_host_wasm`]).
#[derive(Resource, Default)]
pub struct WebAudio {
    /// Sounds the page has been given, with their durations.
    buffers: HashMap<AssetId<bevy_kira_audio::AudioSource>, f64>,
    graphs: HashMap<Entity, (AssetId<bevy_kira_audio::AudioSource>, AudioGraph)>,
    next_instance: InstanceId,
}

impl WebAudio {
    fn tick(
        &mut self,
        mut asset_events: EventReader<AssetEvent<bevy_kira_audio::AudioSource>>,
        assets: Res<Assets<bevy_kira_audio::AudioSource>>,
    ) {
        for ev in asset_events.read() {
            match ev {
                AssetEvent::Added { id }
                | AssetEvent::Modified { id }
                | AssetEvent::LoadedWithDependencies { id } => {
                    let Some(asset) = assets.get(*id) else {
                        continue;
                    };
                    let frames = asset
                        .sound
                        .frames
                        .iter()
                        .map(|f| (f.left + f.right) / 2.0)
                        .collect::<Vec<_>>();
                    let sample_rate = asset.sound.sample_rate as f32;
                    self.buffers
                        .insert(*id, frames.len() as f64 / sample_rate as f64);
                    send(AudioCommand::LoadBuffer {
                        id: *id,
                        sample_rate,
                        frames,
                    });
                }
                AssetEvent::Removed { id } => {
                    self.buffers.remove(id);
                    send(AudioCommand::DropBuffer { id: *id });
                }
                _ => (),
            }
        }
    }

    fn start(
        &mut self,
        id: AssetId<bevy_kira_audio::AudioSource>,
        offset: Option<f32>,
    ) -> Option<AudioGraph> {
        let duration = *self.buffers.get(&id)?;
        let instance = self.next_instance;
        self.next_instance += 1;
        send(AudioCommand::Start {
            instance,
            buffer: id,
            offset,
        });
        Some(AudioGraph {
            instance,
            elapsed_time: 0.0,
            duration,
        })
    }
}

pub struct AudioGraph {
    pub instance: InstanceId,
    pub elapsed_time: f64,
    pub duration: f64,
}

impl AudioGraph {
    pub fn stop(&self) {
        send(AudioCommand::Stop {
            instance: self.instance,
        });
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
        Or<(
            Changed<AudioEmitter>,
            With<Playing>,
            // Include entities queued for retry: on wasm we can't start
            // playback until the web AudioBuffer is decoded, so the first
            // frame often fails and we insert RetryEmitter without a Playing
            // marker. Without this term the retry never fires.
            With<RetryEmitter>,
        )>,
    >,
    mut audio: ResMut<WebAudio>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
    settings: Res<AudioSettings>,
    pan: VolumePanning,
    time: Res<Time>,
) {
    let current_scenes = player
        .single()
        .ok()
        .map(|p| containing_scene.get(p))
        .unwrap_or_default();

    let mut prev_instances = std::mem::take(&mut audio.graphs);
    let elapsed = time.delta_secs_f64();

    for (ent, emitter, maybe_gt, maybe_scene_ent, maybe_layers, maybe_retry) in query.iter() {
        commands.entity(ent).try_remove::<RetryEmitter>();

        if !emitter.playing {
            if let Some((_, instance)) = prev_instances.remove(&ent) {
                instance.stop();
            }

            commands.entity(ent).try_remove::<Playing>();
            continue;
        }

        let existing = match prev_instances.remove(&ent) {
            Some((id, instance)) => {
                if id == emitter.handle.id()
                    && !(emitter.is_changed() && emitter.seek_time.is_some())
                    && (emitter.r#loop || instance.elapsed_time < instance.duration)
                {
                    // reuse existing only if same source AND still playing
                    Some(&mut audio.graphs.entry(ent).insert((id, instance)).into_mut().1)
                } else {
                    instance.stop();
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

        let instance = match existing {
            Some(existing) => {
                existing.elapsed_time += elapsed * emitter.playback_speed as f64;
                existing
            }
            None => {
                commands.entity(ent).try_insert(Playing);

                let Some(new_instance) = audio.start(emitter.handle.id(), emitter.seek_time) else {
                    commands.entity(ent).try_insert(RetryEmitter);
                    continue;
                };

                &mut audio
                    .graphs
                    .entry(ent)
                    .insert((emitter.handle.id(), new_instance))
                    .into_mut()
                    .1
            }
        };

        if emitter.is_changed() || maybe_retry.is_some() {
            send(AudioCommand::SetPlayback {
                instance: instance.instance,
                looping: emitter.r#loop,
                rate: emitter.playback_speed,
            });
        }

        // PannerNode is -1 to 1, vs kira range of 0 to 1
        send(AudioCommand::Set {
            instance: instance.instance,
            gain: source_volume * emitter_volume,
            pan: panning * 2.0 - 1.0,
        });
    }

    for (_, (_, instance)) in prev_instances.drain() {
        instance.stop();
    }
}

#[derive(Component)]
pub struct SystemSound;

#[expect(clippy::type_complexity, reason = "Queries are complex")]
fn play_system_audio(
    mut commands: Commands,
    mut events: EventReader<SystemAudio>,
    ipfas: IpfsAssetServer,
    stopped_playing: Query<Entity, (With<SystemSound>, Without<RetryEmitter>, Without<Playing>)>,
) {
    for event in events.read() {
        let handle = ipfas.asset_server().load(&event.0);
        debug!("play system audio {}", event.0);
        commands.spawn(AudioEmitter {
            handle,
            global: true,
            ty: AudioType::System,
            ..Default::default()
        });
    }

    for ent in stopped_playing.iter() {
        debug!("drop system audio");
        commands.entity(ent).despawn();
    }
}
