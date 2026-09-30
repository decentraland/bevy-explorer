use bevy::asset::{io::Reader, AssetLoader, LoadContext};
use bevy_kira_audio::AudioSource;
use kira::sound::{static_sound::StaticSoundData, FromFileError};
use std::io::Cursor;
use thiserror::Error;

// Format-agnostic loader for scene-content audio assets served without a file
// extension on the wire. Kira's `StaticSoundData::from_cursor` internally uses
// symphonia's probe to sniff the container (mp3/ogg/wav/flac) from the byte
// stream, so we don't need to know the original extension up front.
#[derive(Default)]
pub struct AudioAssetLoader;

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum AudioLoaderError {
    #[error("Could not read audio asset: {0}")]
    Io(#[from] std::io::Error),
    #[error("Error decoding audio asset: {0}")]
    FileError(#[from] FromFileError),
}

impl AssetLoader for AudioAssetLoader {
    type Asset = AudioSource;
    type Settings = ();
    type Error = AudioLoaderError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let byte_count = bytes.len();
        let sound = StaticSoundData::from_cursor(Cursor::new(bytes))?;
        bevy::log::debug!(
            "loaded .audio asset {:?} ({} bytes, {} frames @ {} Hz)",
            load_context.path(),
            byte_count,
            sound.frames.len(),
            sound.sample_rate,
        );
        Ok(AudioSource { sound })
    }

    fn extensions(&self) -> &[&str] {
        &["audio"]
    }
}
