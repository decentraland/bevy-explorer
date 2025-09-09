use std::time::Duration;

use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
    render::{renderer::WgpuWrapper, view::RenderLayers},
};
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
use web_sys::{AudioBuffer, AudioContext, GainNode, StereoPannerNode};

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
        app.init_non_send_resource::<HtmlAudioContext>();
        app.init_asset::<AudioBufferAsset>();
        app.add_event::<SystemAudio>();
        app.add_crdt_lww_component::<PbAudioSource, AudioSource>(
            SceneComponentId::AUDIO_SOURCE,
            ComponentPosition::EntityOnly,
        );
        // app.add_systems(
        //     PostUpdate,
        //     (
        //         create_audio_sources,
        //         update_audio_sources,
        //         play_system_audio,
        //         remove_dead_audio_assets,
        //     )
        //         .after(TransformSystem::TransformPropagate),
        // );
        app.add_systems(Startup, setup_audio.in_set(SetupSets::Main));
    }
}

#[derive(NonSend)]
pub struct HtmlAudioContext {
    context: AudioContext,
    buffers: HashMap<AssetId<bevy_kira_audio::AudioSource>, AudioBuffer>,
    graphs: Vec<Option<AudioSourceGraph>>,
}

impl Default for HtmlAudioContext {
    fn default() -> Self {
        Self {
            context: AudioContext::new().unwrap(),
            buffers: default(),
            graphs: default(),
        }
    }
}

impl HtmlAudioContext {
    fn start(&mut self, handle: Handle<bevy_kira_audio::AudioSource>, r#loop: bool) -> usize {
        let graph = AudioSourceGraph {
            handle,
            graph: None,
        };
        if let Some((ix, slot)) = self.graphs.iter_mut().enumerate().find(Option::is_none) {
            *slot = Some(graph);
            return ix;
        };

        self.graphs.push(Some(graph));
        self.graphs.len() - 1
    }

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
                    let Some(asset) = assets.get(id) else {
                        continue;
                    };
                    let frame_count = asset.sound.frames.len();
                    let buffer = self
                        .context
                        .create_buffer(1, frame_count, asset.sound.sample_rate as f32)
                        .unwrap();
                    let frames = asset
                        .sound
                        .frames
                        .iter()
                        .map(|f| (f.left + f.right) / 2.0)
                        .collect::<Vec<_>>();
                    buffer.copy_to_channel(&frames, 0).unwrap();
                    self.buffers.insert(id, buffer);
                }
                AssetEvent::Removed { id } => {
                    self.buffers.remove(id);
                }
                _ => (),
            }
        }

        for graph in self.graphs.iter_mut() {
            if let Some(graph) = graph {
                if graph.graph.is_none() {
                    if let Some(buffer) = self.buffers.get(graph.handle.id()) {
                        // make new graph
                        let source_node = self.context.create_buffer_source().unwrap();
                        source_node.set_buffer(Some(buffer));
                        if graph.r#loop {
                            source_node.set_loop(true);
                        }

                        let gain_node = context.create_gain().unwrap();
                        gain_node.gain().set_value(graph.volume);

                        let pan_node = context.create_pan().unwrap();
                        pan_node.pan().set_value(graph.pan);

                        source_node.connect_with_audio_node(gain_node).unwrap();
                        gain_node.connect_with_audio_node(pan_node).unwrap();
                        pan_node
                            .connect_with_audio_node(self.context.destination())
                            .unwrap();

                        graph.graph = Some(AudioGraphHtmlElements {
                            source_node,
                            gain_node,
                            panner_node,
                        });
                    }
                }
            }
        }
    }
}

impl HtmlAudioContext {
    fn get_buffer(
        &mut self,
        id: AssetId<bevy_kira_audio::AudioSource>,
        assets: String,
    ) -> Option<AudioBuffer> {
        self.buffers.entry(id).or_insert_with(|| {}).clone()
    }
}

fn setup_audio(mut commands: Commands, camera: Res<PrimaryCameraRes>) {
    commands
        .entity(camera.0)
        .try_insert(AudioReceiver::default());
}

pub struct AudioGraphHtmlElements {
    pub source_node: AudioBufferSourceNode,
    pub gain_node: GainNode,
    pub panner_node: StereoPannerNode,
}

pub struct AudioSourceGraph {
    handle: Handle<bevy_kira_audio::AudioSource>,
    graph: Option<AudioGraphHtmlElements>,
}

#[derive(Component)]
pub struct AudioSourceState {
    handle: Handle<bevy_kira_audio::AudioSource>,
    clip_url: String,
}

#[derive(Component)]
pub struct AudioInstance {
    graph_id: usize,
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
    audio: ResMut<AudioContext>,
    ipfas: IpfsAssetServer,
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
                        audio.stop(h_instance);
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
                .and_then(|h_instance| audio.instance(h_instance).playing.then_some(h_instance));

            match maybe_playing_instance {
                Some(playing_instance) => {
                    audio.set_instance(playing_instance, audio_source.0.r#loop(), volume, playback_rate);
                    if let Some(time) = audio_source.0.current_time {
                        if time < 1e6 {
                            audio.seek_to(time);
                        } else {
                            warn!(
                                "ignoring ridiculous time offset {} for audio clip `{}`",
                                time, audio_source.0.audio_clip_url
                            );
                        }
                    }
                }
                None => {
                    let mut new_instance = audio.play(state.handle.clone());
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
                audio.stop(h_instance);
            }
        }

        if let Some(new_state) = new_state {
            commands.entity(ent).try_insert(new_state);
        }
    }
}

// fn remove_dead_audio_assets(mut audio_instances: ResMut<Assets<AudioInstance>>) {
//     let mut dead = HashSet::new();
//     for (h, instance) in audio_instances.iter() {
//         if instance.state() == bevy_kira_audio::PlaybackState::Stopped {
//             dead.insert(h);
//         }
//     }

//     for h in dead {
//         audio_instances.remove(h);
//     }
// }

// fn play_system_audio(
//     mut events: EventReader<SystemAudio>,
//     audio: Res<bevy_kira_audio::Audio>,
//     ipfas: IpfsAssetServer,
//     audio_instances: Res<Assets<AudioInstance>>,
//     settings: Res<AudioSettings>,
//     mut playing: Local<HashSet<Handle<AudioInstance>>>,
// ) {
//     for event in events.read() {
//         let h_clip = ipfas.asset_server().load(&event.0);
//         let volume = settings.system();
//         let h_instance = audio
//             .play(h_clip)
//             .with_volume(bevy_kira_audio::prelude::Volume::Amplitude(volume as f64))
//             .handle();
//         playing.insert(h_instance);
//         debug!("play system audio {}", event.0);
//     }

//     playing.retain(|h_instance| {
//         let retain = audio_instances
//             .get(h_instance)
//             .is_some_and(|instance| instance.state().position().is_some());
//         if !retain {
//             debug!("drop system audio");
//         }
//         retain
//     })
// }

// #[allow(clippy::too_many_arguments, clippy::type_complexity)]
// fn update_audio_sources(
//     mut query: Query<(
//         Entity,
//         Option<&SceneEntity>,
//         Option<&AudioSource>,
//         &mut AudioEmitter,
//         Option<&RenderLayers>,
//         &GlobalTransform,
//     )>,
//     mut audio_instances: ResMut<Assets<AudioInstance>>,
//     containing_scene: ContainingScene,
//     player: Query<Entity, With<PrimaryUser>>,
//     mut prev_scenes: Local<HashSet<Entity>>,
//     pan: VolumePanning,
//     settings: Res<AudioSettings>,
//     mut all_instances: Local<HashMap<Entity, Vec<Handle<AudioInstance>>>>,
// ) {
//     let current_scenes = player
//         .single()
//         .ok()
//         .map(|p| containing_scene.get(p))
//         .unwrap_or_default();

//     let mut prev_instances = std::mem::take(&mut *all_instances);

//     for (ent, maybe_scene, maybe_source, mut emitter, layers, transform) in query.iter_mut() {
//         if maybe_scene.is_none_or(|scene| current_scenes.contains(&scene.root)) {
//             let (volume, panning) = if maybe_source.is_some_and(|source| source.0.global()) {
//                 (
//                     maybe_source
//                         .and_then(|source| source.0.volume)
//                         .unwrap_or(1.0),
//                     0.5,
//                 )
//             } else {
//                 let volume_adjust = if maybe_scene.is_some() {
//                     settings.scene()
//                 } else {
//                     settings.avatar()
//                 };

//                 let (volume, panning) = pan.volume_and_panning(transform.translation(), layers);

//                 (volume * volume_adjust, panning)
//             };

//             for h_instance in emitter.instances.iter_mut() {
//                 if let Some(instance) = audio_instances.get_mut(h_instance) {
//                     instance.set_volume(volume as f64, AudioTween::linear(Duration::ZERO));
//                     instance.set_panning(panning as f64, AudioTween::default());
//                 }
//             }
//         } else if maybe_scene.is_some_and(|scene| prev_scenes.contains(&scene.root)) {
//             debug!("set zero [{:?}] ({:?})", ent, emitter.instances);
//             for h_instance in &emitter.instances {
//                 if let Some(instance) = audio_instances.get_mut(h_instance) {
//                     instance.set_volume(0.0, AudioTween::default());
//                 }
//             }
//         }

//         // remove old audios
//         if let Some(prev_instances) = prev_instances.remove(&ent) {
//             let current_ids = emitter
//                 .instances
//                 .iter()
//                 .map(|h| h.id())
//                 .collect::<HashSet<_>>();

//             for h_instance in prev_instances {
//                 if !current_ids.contains(&h_instance.id()) {
//                     debug!("stop removed {:?}", h_instance);
//                     if let Some(instance) = audio_instances.get_mut(h_instance.id()) {
//                         instance.stop(AudioTween::default());
//                     }
//                 }
//             }
//         }

//         all_instances.insert(ent, emitter.instances.clone());
//     }

//     for (_ent, prev_instances) in prev_instances {
//         for h_instance in prev_instances {
//             if let Some(instance) = audio_instances.get_mut(h_instance.id()) {
//                 debug!("stop dropped {:?}", h_instance);
//                 instance.stop(AudioTween::default());
//             }
//         }
//     }

//     *prev_scenes = current_scenes;
// }
