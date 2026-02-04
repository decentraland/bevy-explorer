#[cfg(feature = "ffmpeg")]
use ffmpeg_next::format::input;

#[cfg(feature = "ffmpeg")]
use crate::video_context::VideoContext;

#[cfg(feature = "ffmpeg")]
#[test]
fn test_ffmpeg() {
    let context = input(&"http://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4".to_owned()).unwrap();
    let (sx, _rx) = tokio::sync::mpsc::channel(1);
    VideoContext::init(&context, sx).unwrap();
}
