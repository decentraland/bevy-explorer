use std::path::PathBuf;

use bevy::{prelude::*, render::view::RenderLayers};
use common::{
    sets::{SceneSets, SetupSets},
    structs::{
        AudioEmitter, AudioType, PrimaryCameraRes, SystemAudio, PRIMARY_AVATAR_LIGHT_LAYER_INDEX,
    },
    util::AudioReceiver,
};
use dcl::interface::ComponentPosition;
use dcl_component::{proto_components::sdk::components::PbAudioSource, SceneComponentId};
use ipfs::{
    ipfs_path::{IpfsPath, IpfsType},
    IpfsAssetServer,
};
use scene_runner::{
    renderer_context::RendererSceneContext, update_world::AddCrdtInterfaceExt, SceneEntity,
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
        // we use kira for audio source asset management, regardless of native / wasm
        app.add_plugins(bevy_kira_audio::AudioPlugin);
        app.add_event::<SystemAudio>();
        app.add_crdt_lww_component::<PbAudioSource, AudioSource>(
            SceneComponentId::AUDIO_SOURCE,
            ComponentPosition::EntityOnly,
        );
        app.add_systems(Update, map_scene_audio_sources.in_set(SceneSets::PostLoop));
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
