use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackSource {
    Unknown,
    Camera,
    Microphone,
    Screenshare,
    ScreenshareAudio,
}

impl TrackSource {
    pub(super) fn from_js_str(source: &str) -> Self {
        match source {
            "microphone" => Self::Microphone,
            "camera" => Self::Camera,
            "screen_share" => Self::Screenshare,
            "screen_share_audio" => Self::ScreenshareAudio,
            "unknown" => Self::Unknown,
            other => {
                error!("TrackSource was not a known source. Was '{other}'. Return Unknown.");
                Self::Unknown
            }
        }
    }
}
