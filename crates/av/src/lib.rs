pub mod audio_source;
pub mod video_player;
pub mod video_thread;

use audio_source::{setup_audio, update_audio};
use bevy::prelude::*;
use bevy_kira_audio::prelude::SpacialAudio;
use common::sets::{SceneSets, SetupSets};
use dcl::interface::ComponentPosition;
use dcl_component::{proto_components::sdk::components::PbAudioSource, SceneComponentId};
use scene_runner::update_world::AddCrdtInterfaceExt;
use video_player::VideoPlayerPlugin;

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(bevy_kira_audio::AudioPlugin);
        app.add_plugins(VideoPlayerPlugin);
        app.add_crdt_lww_component::<PbAudioSource, audio_source::AudioSource>(
            SceneComponentId::AUDIO_SOURCE,
            ComponentPosition::EntityOnly,
        );
        app.add_systems(Update, update_audio.in_set(SceneSets::PostLoop));
        app.insert_resource(SpacialAudio { max_distance: 25. });
        app.add_systems(Startup, setup_audio.in_set(SetupSets::Main));
    }
}
