use bevy::{
    log::{Level, LogPlugin},
    prelude::*,
    state::app::StatesPlugin,
};
use livestream_manager::{
    plugin::LivestreamManagerPlugin,
    states::{Receiver, TransmissionKind, Transmitter},
    ActiveReceiver, ActiveTransmitter, ActiveVideoCast, Presentation, ReceiverImage, Screenshare,
    VideoCast, VideoStream,
};

fn min_app() -> App {
    let mut app = App::new();

    app.add_plugins((
        AssetPlugin::default(),
        LogPlugin {
            level: Level::DEBUG,
            ..Default::default()
        },
        StatesPlugin,
    ));

    app.init_asset::<Image>();

    app.add_plugins(LivestreamManagerPlugin);

    app.finish();

    app
}

#[test]
fn test_add_receiver() {
    let mut app = min_app();

    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    let receiver = app.world_mut().spawn(ActiveReceiver).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::On);

    assert!(app.world().entity(receiver).contains::<ReceiverImage>());
}

#[test]
fn test_add_multiple_receivers() {
    let mut app = min_app();

    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    let receiver1 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver2 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver3 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver4 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver5 = app.world_mut().spawn(ActiveReceiver).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::On);

    assert!(app.world().entity(receiver1).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver2).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver3).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver4).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver5).contains::<ReceiverImage>());
}

#[test]
fn test_remove_a_receivers() {
    let mut app = min_app();

    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    let receiver1 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver2 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver3 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver4 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver5 = app.world_mut().spawn(ActiveReceiver).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::On);

    assert!(app.world().entity(receiver1).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver2).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver3).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver4).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver5).contains::<ReceiverImage>());

    app.world_mut().entity_mut(receiver5).despawn();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::On);

    assert!(app.world().entity(receiver1).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver2).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver3).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver4).contains::<ReceiverImage>());
    assert!(app.world().get_entity(receiver5).is_err());
}

#[test]
fn test_remove_all_receivers() {
    let mut app = min_app();

    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    let receiver1 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver2 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver3 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver4 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver5 = app.world_mut().spawn(ActiveReceiver).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::On);

    assert!(app.world().entity(receiver1).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver2).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver3).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver4).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver5).contains::<ReceiverImage>());

    app.world_mut().entity_mut(receiver1).despawn();
    app.world_mut().entity_mut(receiver2).despawn();
    app.world_mut().entity_mut(receiver3).despawn();
    app.world_mut().entity_mut(receiver4).despawn();
    app.world_mut().entity_mut(receiver5).despawn();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app.world().get_entity(receiver1).is_err());
    assert!(app.world().get_entity(receiver2).is_err());
    assert!(app.world().get_entity(receiver3).is_err());
    assert!(app.world().get_entity(receiver4).is_err());
    assert!(app.world().get_entity(receiver5).is_err());
}

#[test]
fn test_add_stream() {
    let mut app = min_app();

    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    let video_stream = app.world_mut().spawn(VideoStream).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Stream
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(video_stream)
        .contains::<ActiveTransmitter>());
}

#[test]
fn test_add_cast() {
    let mut app = min_app();

    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    let video_cast = app.world_mut().spawn(VideoCast).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(video_cast)
        .contains::<ActiveTransmitter>());
}

#[test]
fn test_add_screenshare() {
    let mut app = min_app();

    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    let video_cast = app.world_mut().spawn(Screenshare).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(video_cast)
        .contains::<ActiveTransmitter>());
}

#[test]
fn test_add_presentation() {
    let mut app = min_app();

    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    let video_cast = app.world_mut().spawn(Presentation).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(video_cast)
        .contains::<ActiveTransmitter>());
}

#[test]
fn test_add_stream_then_cast() {
    let mut app = min_app();

    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    let video_stream = app.world_mut().spawn(VideoStream).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Stream
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(video_stream)
        .contains::<ActiveTransmitter>());

    let video_cast = app.world_mut().spawn(VideoCast).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(!app
        .world()
        .entity(video_stream)
        .contains::<ActiveTransmitter>());
    assert!(app
        .world()
        .entity(video_cast)
        .contains::<ActiveTransmitter>());
}

#[test]
fn test_add_cast_then_cast() {
    let mut app = min_app();

    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    let video_cast1 = app.world_mut().spawn(VideoCast).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(video_cast1)
        .contains::<ActiveTransmitter>());

    let video_cast2 = app.world_mut().spawn(VideoCast).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(video_cast1)
        .contains::<ActiveTransmitter>());
    assert!(!app
        .world()
        .entity(video_cast2)
        .contains::<ActiveTransmitter>());
}

#[test]
fn test_add_cast_then_active_cast() {
    let mut app = min_app();

    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    let video_cast1 = app.world_mut().spawn(VideoCast).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(video_cast1)
        .contains::<ActiveTransmitter>());

    let video_cast2 = app.world_mut().spawn((VideoCast, ActiveVideoCast)).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(!app
        .world()
        .entity(video_cast1)
        .contains::<ActiveTransmitter>());
    assert!(app
        .world()
        .entity(video_cast2)
        .contains::<ActiveTransmitter>());
}

#[test]
fn test_add_active_cast_then_cast() {
    let mut app = min_app();

    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    let video_cast1 = app.world_mut().spawn((VideoCast, ActiveVideoCast)).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(video_cast1)
        .contains::<ActiveTransmitter>());

    let video_cast2 = app.world_mut().spawn(VideoCast).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(video_cast1)
        .contains::<ActiveTransmitter>());
    assert!(!app
        .world()
        .entity(video_cast2)
        .contains::<ActiveTransmitter>());
}

#[test]
fn test_add_active_cast_then_active_cast() {
    let mut app = min_app();

    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    let video_cast1 = app.world_mut().spawn((VideoCast, ActiveVideoCast)).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(video_cast1)
        .contains::<ActiveTransmitter>());

    let video_cast2 = app.world_mut().spawn((VideoCast, ActiveVideoCast)).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(video_cast1)
        .contains::<ActiveTransmitter>());
    assert!(!app
        .world()
        .entity(video_cast2)
        .contains::<ActiveTransmitter>());
}

#[test]
fn test_add_cast_then_screenshare() {
    let mut app = min_app();

    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    let video_cast = app.world_mut().spawn(VideoCast).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(video_cast)
        .contains::<ActiveTransmitter>());

    let screenshare = app.world_mut().spawn(Screenshare).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(!app
        .world()
        .entity(video_cast)
        .contains::<ActiveTransmitter>());
    assert!(app
        .world()
        .entity(screenshare)
        .contains::<ActiveTransmitter>());
}

#[test]
fn test_add_screenshare_then_cast() {
    let mut app = min_app();

    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    let screenshare = app.world_mut().spawn(Screenshare).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(screenshare)
        .contains::<ActiveTransmitter>());

    let video_cast = app.world_mut().spawn(VideoCast).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(screenshare)
        .contains::<ActiveTransmitter>());
    assert!(!app
        .world()
        .entity(video_cast)
        .contains::<ActiveTransmitter>());
}

#[test]
fn test_add_screenshare_then_screenshare() {
    let mut app = min_app();

    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    let screenshare1 = app.world_mut().spawn(Screenshare).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(screenshare1)
        .contains::<ActiveTransmitter>());

    let screenshare2 = app.world_mut().spawn(Screenshare).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(screenshare1)
        .contains::<ActiveTransmitter>());
    assert!(!app
        .world()
        .entity(screenshare2)
        .contains::<ActiveTransmitter>());
}

#[test]
fn test_add_screenshare_then_presentation() {
    let mut app = min_app();

    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    let screenshare = app.world_mut().spawn(Screenshare).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(screenshare)
        .contains::<ActiveTransmitter>());

    let presentation = app.world_mut().spawn(Presentation).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(!app
        .world()
        .entity(screenshare)
        .contains::<ActiveTransmitter>());
    assert!(app
        .world()
        .entity(presentation)
        .contains::<ActiveTransmitter>());
}

#[test]
fn test_add_presentation_then_screenshare() {
    let mut app = min_app();

    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    let presentation = app.world_mut().spawn(Presentation).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(presentation)
        .contains::<ActiveTransmitter>());

    let screenshare = app.world_mut().spawn(Screenshare).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(presentation)
        .contains::<ActiveTransmitter>());
    assert!(!app
        .world()
        .entity(screenshare)
        .contains::<ActiveTransmitter>());
}

#[test]
fn test_add_presentation_then_presentation() {
    let mut app = min_app();

    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Off
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::Off
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    let presentation1 = app.world_mut().spawn(Presentation).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(presentation1)
        .contains::<ActiveTransmitter>());

    let presentation2 = app.world_mut().spawn(Presentation).id();

    app.update();
    app.update();
    app.update();

    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        TransmissionKind::Cast
    );
    assert_eq!(
        **app.world().resource::<State<Transmitter>>(),
        Transmitter::On
    );
    assert_eq!(**app.world().resource::<State<Receiver>>(), Receiver::Off);

    assert!(app
        .world()
        .entity(presentation1)
        .contains::<ActiveTransmitter>());
    assert!(!app
        .world()
        .entity(presentation2)
        .contains::<ActiveTransmitter>());
}
