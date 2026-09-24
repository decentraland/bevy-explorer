//! The page side of web audio for scene audio sources: owns the `AudioContext` and the per-sound
//! graphs, driven by [`AudioCommand`]s from the engine worker (which has no WebAudio). Runs as a
//! task on the page's event loop; nothing here may block.

use bevy::{log::debug, platform::collections::HashMap, prelude::AssetId};
use common::util::ReportErr;
use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_lite::StreamExt;
use once_cell::sync::OnceCell;
use web_sys::{
    js_sys::Float32Array, AudioBuffer, AudioBufferSourceNode, AudioContext,
    AudioScheduledSourceNode, GainNode, StereoPannerNode,
};

pub type InstanceId = u32;

/// Engine → page.
#[derive(Debug)]
pub enum AudioCommand {
    /// Decoded mono samples for a sound the engine may start later.
    LoadBuffer {
        id: AssetId<bevy_kira_audio::AudioSource>,
        sample_rate: f32,
        frames: Vec<f32>,
    },
    DropBuffer {
        id: AssetId<bevy_kira_audio::AudioSource>,
    },
    Start {
        instance: InstanceId,
        buffer: AssetId<bevy_kira_audio::AudioSource>,
        offset: Option<f32>,
    },
    /// Per-frame gain and pan (`pan` in the panner's -1..1 range).
    Set {
        instance: InstanceId,
        gain: f32,
        pan: f32,
    },
    SetPlayback {
        instance: InstanceId,
        looping: bool,
        rate: f32,
    },
    Stop {
        instance: InstanceId,
    },
}

static HOST: OnceCell<UnboundedSender<AudioCommand>> = OnceCell::new();

pub(crate) fn send(command: AudioCommand) {
    match HOST.get() {
        Some(host) => {
            let _ = host.unbounded_send(command);
        }
        None => debug!("no audio host: dropping {command:?}"),
    }
}

/// Page entry: starts serving audio commands. Call after the module is initialised on the page
/// and before the engine worker starts (exported by the root crate's `web.rs`).
pub fn audio_host_main() {
    let (tx, mut rx) = unbounded::<AudioCommand>();
    if HOST.set(tx).is_err() {
        panic!("audio host already started");
    }

    wasm_bindgen_futures::spawn_local(async move {
        let context = AudioContext::new().unwrap();
        let mut buffers: HashMap<AssetId<bevy_kira_audio::AudioSource>, AudioBuffer> =
            HashMap::default();
        let mut graphs: HashMap<InstanceId, Graph> = HashMap::default();

        while let Some(command) = rx.next().await {
            match command {
                AudioCommand::LoadBuffer {
                    id,
                    sample_rate,
                    frames,
                } => {
                    let buffer = context
                        .create_buffer(1, frames.len() as u32, sample_rate)
                        .unwrap();
                    let js_array = Float32Array::new_with_length(frames.len() as u32);
                    js_array.copy_from(&frames);
                    buffer.copy_to_channel_with_f32_array(&js_array, 0).unwrap();
                    buffers.insert(id, buffer);
                }
                AudioCommand::DropBuffer { id } => {
                    buffers.remove(&id);
                }
                AudioCommand::Start {
                    instance,
                    buffer,
                    offset,
                } => {
                    let Some(buffer) = buffers.get(&buffer) else {
                        debug!("audio {instance}: buffer {buffer:?} not loaded");
                        continue;
                    };
                    if let Some(graph) = Graph::start(&context, buffer, offset) {
                        if let Some(prev) = graphs.insert(instance, graph) {
                            prev.stop(context.current_time());
                        }
                    }
                }
                AudioCommand::Set {
                    instance,
                    gain,
                    pan,
                } => {
                    if let Some(graph) = graphs.get(&instance) {
                        graph.gain_node.gain().set_value(gain);
                        graph.panner_node.pan().set_value(pan);
                    }
                }
                AudioCommand::SetPlayback {
                    instance,
                    looping,
                    rate,
                } => {
                    if let Some(graph) = graphs.get(&instance) {
                        graph.source_node.playback_rate().set_value(rate);
                        graph.source_node.set_loop(looping);
                    }
                }
                AudioCommand::Stop { instance } => {
                    if let Some(graph) = graphs.remove(&instance) {
                        graph.stop(context.current_time());
                    }
                }
            }
        }
    });
}

struct Graph {
    source_node: AudioBufferSourceNode,
    gain_node: GainNode,
    panner_node: StereoPannerNode,
}

impl Graph {
    fn start(context: &AudioContext, buffer: &AudioBuffer, offset: Option<f32>) -> Option<Self> {
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

        Some(Self {
            source_node,
            gain_node,
            panner_node,
        })
    }

    fn stop(&self, now: f64) {
        self.gain_node
            .gain()
            .linear_ramp_to_value_at_time(0.0, now + 0.01)
            .report();
        let node: &AudioScheduledSourceNode = self.source_node.as_ref();
        node.stop_with_when(now + 0.01).report();
    }
}
