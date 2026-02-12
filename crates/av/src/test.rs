#[cfg(feature = "ffmpeg")]
use ffmpeg_next::format::input;

#[cfg(feature = "ffmpeg")]
use crate::video_context::VideoContext;

#[cfg(feature = "ffmpeg")]
#[test]
fn test_ffmpeg() {
    let context = input(
        &"https://vz-7c61c1b5-d59.b-cdn.net/ccea595a-b910-4de6-b160-092819db021d/play_480p.mp4"
            .to_owned(),
    )
    .unwrap();
    let (sx, _rx) = tokio::sync::mpsc::channel(1);
    VideoContext::init(&context, sx).unwrap();
}
