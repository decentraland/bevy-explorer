pub mod audio_context;
pub mod audio_sink;
pub mod audio_source;
pub mod ffmpeg_util;
pub mod microphone;
pub mod stream_processor;
#[cfg(test)]
pub mod test;
pub mod video_context;
pub mod video_player;
pub mod video_stream;

use audio_sink::{spawn_and_locate_foreign_streams, spawn_audio_streams};
use audio_source::AudioSourcePlugin;
use bevy::prelude::*;
use microphone::MicPlugin;
use video_player::VideoPlayerPlugin;

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(bevy_kira_audio::AudioPlugin);
        app.add_plugins(VideoPlayerPlugin);
        app.add_plugins(MicPlugin);
        app.add_plugins(AudioSourcePlugin);
        app.add_systems(
            PostUpdate,
            (spawn_audio_streams, spawn_and_locate_foreign_streams),
        );
    }
}
