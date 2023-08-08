use ffmpeg_next::format::input;

use crate::video_context::VideoContext;

#[test]
fn test_ffmpeg() {
    let context = input(&"https://player.vimeo.com/external/552481870.m3u8?s=c312c8533f97e808fccc92b0510b085c8122a875".to_owned()).unwrap();
    let (sx, _rx) = tokio::sync::mpsc::channel(1);
    VideoContext::init(&context, sx).unwrap();
}
