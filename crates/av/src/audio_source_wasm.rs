use bevy::{platform::collections::HashMap, prelude::*, render::view::RenderLayers};
use common::{
    structs::{AudioEmitter, AudioSettings, AudioType, PrimaryUser, SystemAudio},
    util::{ReportErr, VolumePanning},
};
use ipfs::IpfsAssetServer;
use scene_runner::{ContainingScene, SceneEntity};
use wasm_bindgen::prelude::*;
use web_sys::{
    js_sys::Float32Array, AudioBuffer, AudioBufferSourceNode, AudioContext,
    AudioScheduledSourceNode, GainNode, StereoPannerNode,
};

#[wasm_bindgen]
extern "C" {
    /// One JSON audio op for the page audio host (worker mode). Keys are
    /// always double-quoted -- the host JSON.parses this.
    #[wasm_bindgen(js_name = __dclAudioOp, catch)]
    fn audio_op(json: &str) -> Result<(), JsValue>;
    /// Decoded mono PCM for buffer `id`; the shim copies out of wasm memory
    /// and transfers to the page.
    #[wasm_bindgen(js_name = __dclAudioPcm, catch)]
    fn audio_pcm(id: f64, samples: &[f32], sample_rate: f32) -> Result<(), JsValue>;
    /// Batched per-frame [instance, volume, pan] triples.
    #[wasm_bindgen(js_name = __dclAudioParams, catch)]
    fn audio_params(data: &[f64]) -> Result<(), JsValue>;
}

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

enum AudioBackend {
    Direct { context: AudioContext },
    Bridged,
}

enum BufferEntry {
    Direct(AudioBuffer),
    Bridged { id: u64, duration: f64 },
}

impl BufferEntry {
    fn duration(&self) -> f64 {
        match self {
            BufferEntry::Direct(buffer) => buffer.duration(),
            BufferEntry::Bridged { duration, .. } => *duration,
        }
    }
}

pub struct HtmlAudioContext {
    backend: AudioBackend,
    next_id: u64,
    buffers: HashMap<AssetId<bevy_kira_audio::AudioSource>, BufferEntry>,
    graphs: HashMap<Entity, (AssetId<bevy_kira_audio::AudioSource>, AudioGraphSlot)>,
}

impl Default for HtmlAudioContext {
    fn default() -> Self {
        // Direct on the page main thread; bridged on a worker global (no
        // AudioContext there -- engine-worker mode).
        let backend = match web_sys::window() {
            Some(_) => AudioBackend::Direct {
                context: AudioContext::new().unwrap(),
            },
            None => {
                info!("no window: scene audio bridged to the page audio host");
                AudioBackend::Bridged
            }
        };
        Self {
            backend,
            next_id: 1,
            buffers: default(),
            graphs: default(),
        }
    }
}

impl HtmlAudioContext {
    /// Keep elapsed-time bookkeeping monotonic on both rendering threads.
    fn now(&self) -> f64 {
        match &self.backend {
            AudioBackend::Direct { context } => context.current_time(),
            AudioBackend::Bridged => web_sys::js_sys::Reflect::get(&web_sys::js_sys::global(), &"performance".into())
                .unwrap()
                .unchecked_into::<web_sys::Performance>()
                .now() / 1000.0,
        }
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
                    let Some(asset) = assets.get(*id) else {
                        continue;
                    };
                    let frame_count = asset.sound.frames.len();
                    let sample_rate = asset.sound.sample_rate as f32;
                    let frames = asset
                        .sound
                        .frames
                        .iter()
                        .map(|f| (f.left + f.right) / 2.0)
                        .collect::<Vec<_>>();

                    match &self.backend {
                        AudioBackend::Direct { context } => {
                            let buffer = context
                                .create_buffer(1, frame_count as u32, sample_rate)
                                .unwrap();
                            let js_array = Float32Array::new_with_length(frames.len() as u32);
                            js_array.copy_from(&frames);
                            buffer.copy_to_channel_with_f32_array(&js_array, 0).unwrap();
                            self.buffers.insert(*id, BufferEntry::Direct(buffer));
                        }
                        AudioBackend::Bridged => {
                            let buf_id = self.next_id;
                            self.next_id += 1;
                            let duration = frame_count as f64 / sample_rate as f64;
                            audio_pcm(buf_id as f64, &frames, sample_rate).report();
                            self.buffers.insert(
                                *id,
                                BufferEntry::Bridged {
                                    id: buf_id,
                                    duration,
                                },
                            );
                        }
                    }
                }
                AssetEvent::Removed { id } => {
                    if let Some(BufferEntry::Bridged { id: buf_id, .. }) = self.buffers.remove(id) {
                        audio_op(&format!(r#"{{"op":"dropbuf","buf":{buf_id}}}"#)).report();
                    }
                }
                _ => (),
            }
        }
    }

    fn start(
        &mut self,
        id: AssetId<bevy_kira_audio::AudioSource>,
        offset: Option<f32>,
    ) -> Option<AudioGraphSlot> {
        let buffer = self.buffers.get(&id)?;
        let duration = buffer.duration();
        match (&self.backend, buffer) {
            (AudioBackend::Direct { context }, BufferEntry::Direct(buffer)) => {
                let source_node = context.create_buffer_source().ok()?;
                source_node.set_buffer(Some(buffer));
                let gain_node = context.create_gain().ok()?;
                let panner_node = context.create_stereo_panner().ok()?;
                source_node.connect_with_audio_node(&gain_node).ok()?;
                gain_node.connect_with_audio_node(&panner_node).ok()?;
                panner_node
                    .connect_with_audio_node(&context.destination())
                    .ok()?;

                if let Some(offset) = offset {
                    source_node
                        .start_with_when_and_grain_offset(0.0, offset as f64)
                        .ok()?;
                } else {
                    source_node.start().ok()?;
                }

                Some(AudioGraphSlot {
                    elapsed_time: 0.0,
                    duration,
                    kind: GraphKind::Direct(AudioGraphHtmlElements {
                        source_node,
                        gain_node,
                        panner_node,
                    }),
                })
            }
            (AudioBackend::Bridged, BufferEntry::Bridged { id: buf_id, .. }) => {
                let inst = self.next_id;
                self.next_id += 1;
                let offset = offset.unwrap_or(0.0);
                audio_op(&format!(
                    r#"{{"op":"play","inst":{inst},"buf":{buf_id},"offset":{offset}}}"#
                ))
                .report();
                Some(AudioGraphSlot {
                    elapsed_time: 0.0,
                    duration,
                    kind: GraphKind::Bridged { inst },
                })
            }
            // A backend/buffer mismatch cannot happen (buffers are created by
            // the same backend), but don't panic if it somehow does.
            _ => None,
        }
    }
}

pub struct AudioGraphSlot {
    pub elapsed_time: f64,
    pub duration: f64,
    kind: GraphKind,
}

enum GraphKind {
    Direct(AudioGraphHtmlElements),
    Bridged { inst: u64 },
}

impl AudioGraphSlot {
    fn stop(&self, now: f64) {
        match &self.kind {
            GraphKind::Direct(elements) => elements.stop(now),
            GraphKind::Bridged { inst } => {
                audio_op(&format!(r#"{{"op":"stop","inst":{inst}}}"#)).report();
            }
        }
    }

    fn configure(&self, playback_rate: f32, looping: bool) {
        match &self.kind {
            GraphKind::Direct(elements) => {
                elements
                    .source_node
                    .playback_rate()
                    .set_value(playback_rate);
                elements.source_node.set_loop(looping);
            }
            GraphKind::Bridged { inst } => {
                audio_op(&format!(
                    r#"{{"op":"cfg","inst":{inst},"rate":{playback_rate},"loop":{looping}}}"#
                ))
                .report();
            }
        }
    }

    /// Apply volume/pan: direct sets the nodes; bridged appends to the
    /// per-frame batch flushed once by `manage_audio_sources`.
    fn set_output(&self, volume: f32, panning: f32, batch: &mut Vec<f64>) {
        match &self.kind {
            GraphKind::Direct(elements) => {
                elements.gain_node.gain().set_value(volume);
                elements.panner_node.pan().set_value(panning * 2.0 - 1.0);
            }
            GraphKind::Bridged { inst } => {
                batch.extend_from_slice(&[
                    *inst as f64,
                    volume as f64,
                    (panning * 2.0 - 1.0) as f64,
                ]);
            }
        }
    }
}

pub struct AudioGraphHtmlElements {
    pub source_node: AudioBufferSourceNode,
    pub gain_node: GainNode,
    pub panner_node: StereoPannerNode,
}

impl AudioGraphHtmlElements {
    pub fn stop(&self, now: f64) {
        self.gain_node
            .gain()
            .linear_ramp_to_value_at_time(0.0, now + 0.01)
            .report();
        let node: &AudioScheduledSourceNode = self.source_node.as_ref();
        node.stop_with_when(now + 0.01).report();
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
    mut audio: NonSendMut<HtmlAudioContext>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
    settings: Res<AudioSettings>,
    pan: VolumePanning,
    mut prev_time: Local<f64>,
    mut params_batch: Local<Vec<f64>>,
) {
    let current_scenes = player
        .single()
        .ok()
        .map(|p| containing_scene.get(p))
        .unwrap_or_default();

    let mut prev_instances = std::mem::take(&mut audio.graphs);
    let now = audio.now();
    let elapsed = now - *prev_time;
    *prev_time = now;
    params_batch.clear();

    for (ent, emitter, maybe_gt, maybe_scene_ent, maybe_layers, maybe_retry) in query.iter() {
        commands.entity(ent).try_remove::<RetryEmitter>();

        if !emitter.playing {
            if let Some((_, instance)) = prev_instances.remove(&ent) {
                instance.stop(now);
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
                    instance.stop(now);
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
            instance.configure(emitter.playback_speed, emitter.r#loop);
        }

        instance.set_output(source_volume * emitter_volume, panning, &mut params_batch);
    }

    for (_, (_, instance)) in prev_instances.drain() {
        instance.stop(now);
    }
    if !params_batch.is_empty() {
        audio_params(&params_batch).report();
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
