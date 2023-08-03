pub mod audio_context;
pub mod audio_sink;
pub mod audio_source;
pub mod ffmpeg_util;
pub mod microphone;
pub mod stream_processor;
pub mod video_context;
pub mod video_player;
pub mod video_stream;

use audio_sink::{spawn_and_locate_foreign_streams, spawn_audio_streams};
use audio_source::{setup_audio, update_audio};
use bevy::prelude::*;
use bevy_kira_audio::prelude::SpacialAudio;
use common::sets::{SceneSets, SetupSets};
use dcl::interface::ComponentPosition;
use dcl_component::{proto_components::sdk::components::PbAudioSource, SceneComponentId};
use microphone::MicPlugin;
use scene_runner::update_world::AddCrdtInterfaceExt;
use video_player::VideoPlayerPlugin;

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(bevy_kira_audio::AudioPlugin);
        app.add_plugins(VideoPlayerPlugin);
        app.add_plugins(MicPlugin);
        app.add_crdt_lww_component::<PbAudioSource, audio_source::AudioSource>(
            SceneComponentId::AUDIO_SOURCE,
            ComponentPosition::EntityOnly,
        );
        app.add_systems(Update, update_audio.in_set(SceneSets::PostLoop));
        app.insert_resource(SpacialAudio { max_distance: 25. });
        app.add_systems(Startup, setup_audio.in_set(SetupSets::Main));
        app.add_systems(
            PostUpdate,
            (spawn_audio_streams, spawn_and_locate_foreign_streams),
        );
    }
}
