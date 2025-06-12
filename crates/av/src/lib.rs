#[cfg(feature = "ffmpeg")]
pub mod audio_context;
#[cfg(feature = "ffmpeg")]
pub mod audio_sink;
pub mod audio_source;
#[cfg(feature = "ffmpeg")]
pub mod ffmpeg_util;
pub mod microphone;
#[cfg(feature = "ffmpeg")]
pub mod stream_processor;
#[cfg(test)]
pub mod test;
#[cfg(feature = "ffmpeg")]
pub mod video_context;
#[cfg(feature = "ffmpeg")]
pub mod video_player;
#[cfg(feature = "ffmpeg")]
pub mod video_stream;

#[cfg(feature = "ffmpeg")]
use audio_sink::{spawn_and_locate_foreign_streams, spawn_audio_streams};
use audio_source::AudioSourcePlugin;
use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use microphone::MicPlugin;
#[cfg(feature = "ffmpeg")]
use video_player::VideoPlayerPlugin;

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(bevy_kira_audio::AudioPlugin);
        #[cfg(feature = "ffmpeg")]
        app.add_plugins(VideoPlayerPlugin);

        #[cfg(not(target_arch = "wasm32"))]
        app.add_plugins(MicPlugin);
        app.add_plugins(AudioSourcePlugin);
        #[cfg(feature = "ffmpeg")]
        app.add_systems(
            PostUpdate,
            (spawn_audio_streams, spawn_and_locate_foreign_streams),
        );
    }
}
