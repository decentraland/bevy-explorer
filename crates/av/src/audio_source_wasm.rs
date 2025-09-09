use bevy::{platform::collections::HashMap, prelude::*, render::view::RenderLayers};
use common::{
    structs::{AudioEmitter, AudioSettings, AudioType, PrimaryUser, SystemAudio},
    util::VolumePanning,
};
use ipfs::IpfsAssetServer;
use scene_runner::{ContainingScene, SceneEntity};
use web_sys::{
    js_sys::Float32Array, AudioBuffer, AudioBufferSourceNode, AudioContext,
    AudioScheduledSourceNode, GainNode, StereoPannerNode,
};

pub struct AudioSourcePluginImpl;

impl Plugin for AudioSourcePluginImpl {
    fn build(&self, app: &mut App) {
        // still use kira for audio source asset management
        app.init_non_send_resource::<HtmlAudioContext>();
        app.add_systems(
            PostUpdate,
            (
                manage_audio_sources,
                play_system_audio,
                |mut ctx: NonSendMut<HtmlAudioContext>,
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

pub struct HtmlAudioContext {
    context: AudioContext,
    buffers: HashMap<AssetId<bevy_kira_audio::AudioSource>, AudioBuffer>,
    graphs: HashMap<
        Entity,
        (
            AssetId<bevy_kira_audio::AudioSource>,
            AudioGraphHtmlElements,
        ),
    >,
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
                    let frame_count = asset.sound.frames.len();
                    let buffer = self
                        .context
                        .create_buffer(1, frame_count as u32, asset.sound.sample_rate as f32)
                        .unwrap();
                    let frames = asset
                        .sound
                        .frames
                        .iter()
                        .map(|f| (f.left + f.right) / 2.0)
                        .collect::<Vec<_>>();
                    let js_array = Float32Array::new_with_length(frames.len() as u32);
                    js_array.copy_from(&frames);
                    buffer.copy_to_channel_with_f32_array(&js_array, 0).unwrap();
                    self.buffers.insert(*id, buffer);
                }
                AssetEvent::Removed { id } => {
                    self.buffers.remove(id);
                }
                _ => (),
            }
        }
    }

    fn start(
        &mut self,
        id: AssetId<bevy_kira_audio::AudioSource>,
        offset: Option<f32>,
    ) -> Option<AudioGraphHtmlElements> {
        // make new graph
        let buffer = self.buffers.get(&id)?;
        let source_node = self.context.create_buffer_source().ok()?;
        source_node.set_buffer(Some(buffer));
        let gain_node = self.context.create_gain().ok()?;
        let panner_node = self.context.create_stereo_panner().ok()?;
        source_node.connect_with_audio_node(&gain_node).ok()?;
        gain_node.connect_with_audio_node(&panner_node).ok()?;
        panner_node
            .connect_with_audio_node(&self.context.destination())
            .ok()?;

        if let Some(offset) = offset {
            source_node
                .start_with_when_and_grain_offset(0.0, offset as f64)
                .ok()?;
        } else {
            source_node.start().ok()?;
        }

        Some(AudioGraphHtmlElements {
            source_node,
            gain_node,
            panner_node,
            elapsed_time: 0.0,
            duration: buffer.duration(),
        })
    }
}

pub struct AudioGraphHtmlElements {
    pub source_node: AudioBufferSourceNode,
    pub gain_node: GainNode,
    pub panner_node: StereoPannerNode,
    pub elapsed_time: f64,
    pub duration: f64,
}

impl AudioGraphHtmlElements {
    pub fn stop(&self, now: f64) {
        let _ = self
            .gain_node
            .gain()
            .linear_ramp_to_value_at_time(0.0, now + 0.01);
        let node: &AudioScheduledSourceNode = self.source_node.as_ref();
        let _ = node.stop_with_when(now + 0.01);
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
    mut audio: NonSendMut<HtmlAudioContext>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
    settings: Res<AudioSettings>,
    pan: VolumePanning,
    mut prev_time: Local<f64>,
) {
    let current_scenes = player
        .single()
        .ok()
        .map(|p| containing_scene.get(p))
        .unwrap_or_default();

    let mut prev_instances = std::mem::take(&mut audio.graphs);
    let now = audio.context.current_time();
    let elapsed = now - *prev_time;
    *prev_time = now;

    for (ent, emitter, maybe_gt, maybe_scene_ent, maybe_layers, maybe_retry) in query.iter() {
        commands.entity(ent).remove::<RetryEmitter>();

        if !emitter.playing {
            if let Some((_, instance)) = prev_instances.remove(&ent) {
                instance.stop(now);
            }

            commands.entity(ent).remove::<Playing>();
            continue;
        }

        let existing = match prev_instances.remove(&ent) {
            Some((id, instance)) => {
                if id == emitter.handle.id()
                    && emitter.seek_time.is_none()
                    && instance.elapsed_time < instance.duration
                {
                    // reuse existing only if same source AND still playing
                    Some(&mut audio.graphs.entry(ent).insert((id, instance)).into_mut().1)
                } else {
                    let _ = instance.stop(now);
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

        let instance = match existing {
            Some(existing) => {
                existing.elapsed_time += elapsed * emitter.playback_speed as f64;
                existing
            }
            None => {
                commands.entity(ent).insert(Playing);

                let Some(new_instance) = audio.start(emitter.handle.id(), emitter.seek_time) else {
                    commands.entity(ent).insert(RetryEmitter);
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
            instance
                .source_node
                .playback_rate()
                .set_value(emitter.playback_speed);
            instance.source_node.set_loop(emitter.r#loop);
        }

        instance
            .gain_node
            .gain()
            .set_value(source_volume * emitter_volume);
        // PannerNode is -1 to 1, vs kira range of 0 to 1
        instance.panner_node.pan().set_value(panning * 2.0 - 1.0);
    }
}

#[derive(Component)]
pub struct SystemSound;

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
