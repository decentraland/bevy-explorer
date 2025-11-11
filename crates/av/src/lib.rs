#[cfg(test)]
pub mod test;

use bevy::prelude::*;

// util
#[cfg(feature = "ffmpeg")]
pub mod audio_context;
#[cfg(feature = "ffmpeg")]
pub mod audio_sink;
#[cfg(feature = "ffmpeg")]
pub mod ffmpeg_util;
#[cfg(feature = "ffmpeg")]
pub mod stream_processor;
#[cfg(feature = "ffmpeg")]
pub mod video_context;
#[cfg(feature = "ffmpeg")]
pub mod video_stream;

// audio source (non-streaming audio)
pub mod audio_source;
use audio_source::AudioSourcePlugin;
#[cfg(not(feature = "html"))]
pub mod audio_source_native;
#[cfg(not(feature = "html"))]
use audio_source_native::AudioSourcePluginImpl;
#[cfg(feature = "html")]
pub mod audio_source_wasm;
#[cfg(feature = "html")]
use audio_source_wasm::AudioSourcePluginImpl;

// foreign players
#[cfg(feature = "ffmpeg")]
use audio_sink::{pipe_voice_to_scene, spawn_and_locate_foreign_streams, spawn_audio_streams};

// video
#[cfg(feature = "ffmpeg")]
pub mod video_player;
#[cfg(feature = "ffmpeg")]
use video_player::VideoPlayerPlugin;
#[cfg(feature = "html")]
pub mod html_video_player;
#[cfg(feature = "html")]
use html_video_player::VideoPlayerPlugin;

#[derive(Default)]
pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(any(feature = "ffmpeg", feature = "html"))]
        app.add_plugins(VideoPlayerPlugin);

        app.add_plugins(AudioSourcePlugin);
        app.add_plugins(AudioSourcePluginImpl);
        #[cfg(feature = "ffmpeg")]
        app.add_systems(
            PostUpdate,
            (
                spawn_audio_streams,
                spawn_and_locate_foreign_streams,
                pipe_voice_to_scene,
            )
                .chain(),
        );
    }
}
