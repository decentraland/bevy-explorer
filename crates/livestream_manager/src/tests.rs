use bevy::{
    log::{Level, LogPlugin},
    prelude::*,
    state::app::StatesPlugin,
};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::*;

use crate::{
    plugin::LivestreamManagerPlugin,
    states::{Receiver, TransmissionKind, Transmitter},
    ActiveAudioTransmitter, ActiveReceiver, ActiveTransmitter, ActiveVideoCast,
    AudioTransmitterKind, ReceiverImage, TransmitterKind,
};

#[cfg(target_arch = "wasm32")]
wasm_bindgen_test_configure!(run_in_browser);

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

fn test_states(
    app: &mut App,
    transmission_kind: TransmissionKind,
    transmitter: Transmitter,
    receiver: Receiver,
) {
    assert_eq!(
        **app.world().resource::<State<TransmissionKind>>(),
        transmission_kind
    );
    assert_eq!(**app.world().resource::<State<Transmitter>>(), transmitter);
    assert_eq!(**app.world().resource::<State<Receiver>>(), receiver);
}

#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_add_receiver() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    let receiver = app.world_mut().spawn(ActiveReceiver).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::On,
    );

    assert!(app.world().entity(receiver).contains::<ReceiverImage>());
}

#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_add_multiple_receivers() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    let receiver1 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver2 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver3 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver4 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver5 = app.world_mut().spawn(ActiveReceiver).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::On,
    );

    assert!(app.world().entity(receiver1).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver2).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver3).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver4).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver5).contains::<ReceiverImage>());
}

#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_remove_a_receivers() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    let receiver1 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver2 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver3 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver4 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver5 = app.world_mut().spawn(ActiveReceiver).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::On,
    );

    assert!(app.world().entity(receiver1).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver2).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver3).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver4).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver5).contains::<ReceiverImage>());

    app.world_mut().entity_mut(receiver5).despawn();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::On,
    );

    assert!(app.world().entity(receiver1).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver2).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver3).contains::<ReceiverImage>());
    assert!(app.world().entity(receiver4).contains::<ReceiverImage>());
}

#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_remove_all_receivers() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    let receiver1 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver2 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver3 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver4 = app.world_mut().spawn(ActiveReceiver).id();
    let receiver5 = app.world_mut().spawn(ActiveReceiver).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::On,
    );

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

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );
}

#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_add_stream() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    let video_stream = app.world_mut().spawn(TransmitterKind::Stream).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Stream,
        Transmitter::Off,
        Receiver::Off,
    );

    assert!(!app
        .world()
        .entity(video_stream)
        .contains::<ActiveTransmitter>());

    app.world_mut().spawn(ActiveReceiver);

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Stream,
        Transmitter::On,
        Receiver::On,
    );

    assert!(app
        .world()
        .entity(video_stream)
        .contains::<ActiveTransmitter>());
}

#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_add_cast() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    let video_cast = app.world_mut().spawn(TransmitterKind::VideoCast).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::Off,
        Receiver::Off,
    );

    assert!(!app
        .world()
        .entity(video_cast)
        .contains::<ActiveTransmitter>());

    app.world_mut().spawn(ActiveReceiver);

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

    assert!(app
        .world()
        .entity(video_cast)
        .contains::<ActiveTransmitter>());
}

#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_add_screenshare() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    let video_cast = app.world_mut().spawn(TransmitterKind::Screenshare).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::Off,
        Receiver::Off,
    );

    assert!(!app
        .world()
        .entity(video_cast)
        .contains::<ActiveTransmitter>());

    app.world_mut().spawn(ActiveReceiver);

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

    assert!(app
        .world()
        .entity(video_cast)
        .contains::<ActiveTransmitter>());
}

#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_add_presentation() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    let video_cast = app.world_mut().spawn(TransmitterKind::Presentation).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::Off,
        Receiver::Off,
    );

    assert!(!app
        .world()
        .entity(video_cast)
        .contains::<ActiveTransmitter>());

    app.world_mut().spawn(ActiveReceiver);

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

    assert!(app
        .world()
        .entity(video_cast)
        .contains::<ActiveTransmitter>());
}

#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_add_stream_then_cast() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    app.world_mut().spawn(ActiveReceiver);
    let video_stream = app.world_mut().spawn(TransmitterKind::Stream).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Stream,
        Transmitter::On,
        Receiver::On,
    );

    assert!(app
        .world()
        .entity(video_stream)
        .contains::<ActiveTransmitter>());

    let video_cast = app.world_mut().spawn(TransmitterKind::VideoCast).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

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
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_add_cast_then_cast() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    app.world_mut().spawn(ActiveReceiver);
    let video_cast1 = app.world_mut().spawn(TransmitterKind::VideoCast).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

    assert!(app
        .world()
        .entity(video_cast1)
        .contains::<ActiveTransmitter>());

    let video_cast2 = app.world_mut().spawn(TransmitterKind::VideoCast).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

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
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_add_cast_then_active_cast() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    app.world_mut().spawn(ActiveReceiver);
    let video_cast1 = app.world_mut().spawn(TransmitterKind::VideoCast).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

    assert!(app
        .world()
        .entity(video_cast1)
        .contains::<ActiveTransmitter>());

    let video_cast2 = app
        .world_mut()
        .spawn((TransmitterKind::VideoCast, ActiveVideoCast))
        .id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

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
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_add_active_cast_then_cast() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    app.world_mut().spawn(ActiveReceiver);
    let video_cast1 = app
        .world_mut()
        .spawn((TransmitterKind::VideoCast, ActiveVideoCast))
        .id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

    assert!(app
        .world()
        .entity(video_cast1)
        .contains::<ActiveTransmitter>());

    let video_cast2 = app.world_mut().spawn(TransmitterKind::VideoCast).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

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
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_add_active_cast_then_active_cast() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    app.world_mut().spawn(ActiveReceiver);
    let video_cast1 = app
        .world_mut()
        .spawn((TransmitterKind::VideoCast, ActiveVideoCast))
        .id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

    assert!(app
        .world()
        .entity(video_cast1)
        .contains::<ActiveTransmitter>());

    let video_cast2 = app
        .world_mut()
        .spawn((TransmitterKind::VideoCast, ActiveVideoCast))
        .id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

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
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_add_cast_then_screenshare() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    app.world_mut().spawn(ActiveReceiver);
    let video_cast = app.world_mut().spawn(TransmitterKind::VideoCast).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

    assert!(app
        .world()
        .entity(video_cast)
        .contains::<ActiveTransmitter>());

    let screenshare = app.world_mut().spawn(TransmitterKind::Screenshare).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

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
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_add_screenshare_then_cast() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    app.world_mut().spawn(ActiveReceiver);
    let screenshare = app.world_mut().spawn(TransmitterKind::Screenshare).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

    assert!(app
        .world()
        .entity(screenshare)
        .contains::<ActiveTransmitter>());

    let video_cast = app.world_mut().spawn(TransmitterKind::VideoCast).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

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
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_add_screenshare_then_screenshare() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    app.world_mut().spawn(ActiveReceiver);
    let screenshare1 = app.world_mut().spawn(TransmitterKind::Screenshare).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

    assert!(app
        .world()
        .entity(screenshare1)
        .contains::<ActiveTransmitter>());

    let screenshare2 = app.world_mut().spawn(TransmitterKind::Screenshare).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

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
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_add_screenshare_then_presentation() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    app.world_mut().spawn(ActiveReceiver);
    let screenshare = app.world_mut().spawn(TransmitterKind::Screenshare).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

    assert!(app
        .world()
        .entity(screenshare)
        .contains::<ActiveTransmitter>());

    let presentation = app.world_mut().spawn(TransmitterKind::Presentation).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

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
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_add_presentation_then_screenshare() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    app.world_mut().spawn(ActiveReceiver);
    let presentation = app.world_mut().spawn(TransmitterKind::Presentation).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

    assert!(app
        .world()
        .entity(presentation)
        .contains::<ActiveTransmitter>());

    let screenshare = app.world_mut().spawn(TransmitterKind::Screenshare).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

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
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_add_presentation_then_presentation() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    app.world_mut().spawn(ActiveReceiver);
    let presentation1 = app.world_mut().spawn(TransmitterKind::Presentation).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

    assert!(app
        .world()
        .entity(presentation1)
        .contains::<ActiveTransmitter>());

    let presentation2 = app.world_mut().spawn(TransmitterKind::Presentation).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

    assert!(app
        .world()
        .entity(presentation1)
        .contains::<ActiveTransmitter>());
    assert!(!app
        .world()
        .entity(presentation2)
        .contains::<ActiveTransmitter>());
}

#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_despawns() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    app.world_mut().spawn(ActiveReceiver);
    let stream = app.world_mut().spawn(TransmitterKind::Stream).id();
    let video_cast = app.world_mut().spawn(TransmitterKind::VideoCast).id();
    let active_video_cast = app
        .world_mut()
        .spawn((ActiveVideoCast, TransmitterKind::VideoCast))
        .id();
    let screenshare = app.world_mut().spawn(TransmitterKind::Screenshare).id();
    let presentation = app.world_mut().spawn(TransmitterKind::Presentation).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

    assert!(!app.world().entity(stream).contains::<ActiveTransmitter>());
    assert!(!app
        .world()
        .entity(video_cast)
        .contains::<ActiveTransmitter>());
    assert!(!app
        .world()
        .entity(active_video_cast)
        .contains::<ActiveTransmitter>());
    assert!(!app
        .world()
        .entity(screenshare)
        .contains::<ActiveTransmitter>());
    assert!(app
        .world()
        .entity(presentation)
        .contains::<ActiveTransmitter>());

    app.world_mut().entity_mut(presentation).despawn();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

    assert!(!app.world().entity(stream).contains::<ActiveTransmitter>());
    assert!(!app
        .world()
        .entity(video_cast)
        .contains::<ActiveTransmitter>());
    assert!(!app
        .world()
        .entity(active_video_cast)
        .contains::<ActiveTransmitter>());
    assert!(app
        .world()
        .entity(screenshare)
        .contains::<ActiveTransmitter>());

    app.world_mut().entity_mut(screenshare).despawn();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

    assert!(!app.world().entity(stream).contains::<ActiveTransmitter>());
    assert!(!app
        .world()
        .entity(video_cast)
        .contains::<ActiveTransmitter>());
    assert!(app
        .world()
        .entity(active_video_cast)
        .contains::<ActiveTransmitter>());

    app.world_mut().entity_mut(active_video_cast).despawn();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

    assert!(!app.world().entity(stream).contains::<ActiveTransmitter>());
    assert!(app
        .world()
        .entity(video_cast)
        .contains::<ActiveTransmitter>());

    app.world_mut().entity_mut(video_cast).despawn();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Stream,
        Transmitter::On,
        Receiver::On,
    );

    assert!(app.world().entity(stream).contains::<ActiveTransmitter>());

    app.world_mut().entity_mut(stream).despawn();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::On,
    );
}

#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_audios_without_receiver() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    app.update();

    let audio_casts = app
        .world_mut()
        .spawn_batch(std::array::repeat::<AudioTransmitterKind, 5>(
            AudioTransmitterKind::Cast,
        ))
        .collect::<Vec<_>>();
    let audio_streams = app
        .world_mut()
        .spawn_batch(std::array::repeat::<AudioTransmitterKind, 5>(
            AudioTransmitterKind::Stream,
        ))
        .collect::<Vec<_>>();

    for cast in &audio_casts {
        assert!(!app
            .world()
            .entity(*cast)
            .contains::<ActiveAudioTransmitter>());
    }
    for stream in &audio_streams {
        assert!(!app
            .world()
            .entity(*stream)
            .contains::<ActiveAudioTransmitter>());
    }
}

#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_audios_with_stream_without_receiver() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    app.world_mut().spawn(TransmitterKind::Stream);

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Stream,
        Transmitter::Off,
        Receiver::Off,
    );

    let audio_casts = app
        .world_mut()
        .spawn_batch(std::array::repeat::<AudioTransmitterKind, 5>(
            AudioTransmitterKind::Cast,
        ))
        .collect::<Vec<_>>();
    let audio_streams = app
        .world_mut()
        .spawn_batch(std::array::repeat::<AudioTransmitterKind, 5>(
            AudioTransmitterKind::Stream,
        ))
        .collect::<Vec<_>>();

    for cast in &audio_casts {
        assert!(!app
            .world()
            .entity(*cast)
            .contains::<ActiveAudioTransmitter>());
    }
    for stream in &audio_streams {
        assert!(!app
            .world()
            .entity(*stream)
            .contains::<ActiveAudioTransmitter>());
    }
}

#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_audios_with_cast_without_receiver() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    app.world_mut().spawn(TransmitterKind::VideoCast);

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::Off,
        Receiver::Off,
    );

    let audio_casts = app
        .world_mut()
        .spawn_batch(std::array::repeat::<AudioTransmitterKind, 5>(
            AudioTransmitterKind::Cast,
        ))
        .collect::<Vec<_>>();
    let audio_streams = app
        .world_mut()
        .spawn_batch(std::array::repeat::<AudioTransmitterKind, 5>(
            AudioTransmitterKind::Stream,
        ))
        .collect::<Vec<_>>();

    for cast in &audio_casts {
        assert!(!app
            .world()
            .entity(*cast)
            .contains::<ActiveAudioTransmitter>());
    }
    for stream in &audio_streams {
        assert!(!app
            .world()
            .entity(*stream)
            .contains::<ActiveAudioTransmitter>());
    }
}

#[test]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn test_audios_on_transmission_kind_change() {
    let mut app = min_app();

    app.update();

    test_states(
        &mut app,
        TransmissionKind::Off,
        Transmitter::Off,
        Receiver::Off,
    );

    let audio_casts = app
        .world_mut()
        .spawn_batch(std::array::repeat::<AudioTransmitterKind, 5>(
            AudioTransmitterKind::Cast,
        ))
        .collect::<Vec<_>>();
    let audio_streams = app
        .world_mut()
        .spawn_batch(std::array::repeat::<AudioTransmitterKind, 5>(
            AudioTransmitterKind::Stream,
        ))
        .collect::<Vec<_>>();

    app.world_mut().spawn(ActiveReceiver);

    // From Off to Cast
    let video_cast = app.world_mut().spawn(TransmitterKind::VideoCast).id();

    app.update();
    app.update();
    app.update();

    test_states(
        &mut app,
        TransmissionKind::Cast,
        Transmitter::On,
        Receiver::On,
    );

    for cast in &audio_casts {
        assert!(app
            .world()
            .entity(*cast)
            .contains::<ActiveAudioTransmitter>());
    }
    for stream in &audio_streams {
        assert!(!app
            .world()
            .entity(*stream)
            .contains::<ActiveAudioTransmitter>());
    }

    // From Cast to Off
    app.world_mut().entity_mut(video_cast).despawn();

    app.update();
    app.update();
    app.update();

    for cast in &audio_casts {
        assert!(!app
            .world()
            .entity(*cast)
            .contains::<ActiveAudioTransmitter>());
    }
    for stream in &audio_streams {
        assert!(!app
            .world()
            .entity(*stream)
            .contains::<ActiveAudioTransmitter>());
    }

    // From Off to Stream
    let stream = app.world_mut().spawn(TransmitterKind::Stream).id();

    app.update();
    app.update();
    app.update();

    for cast in &audio_casts {
        assert!(!app
            .world()
            .entity(*cast)
            .contains::<ActiveAudioTransmitter>());
    }
    for stream in &audio_streams {
        assert!(app
            .world()
            .entity(*stream)
            .contains::<ActiveAudioTransmitter>());
    }

    // From Stream to Cast
    let video_cast = app.world_mut().spawn(TransmitterKind::VideoCast).id();

    app.update();
    app.update();
    app.update();

    for cast in &audio_casts {
        assert!(app
            .world()
            .entity(*cast)
            .contains::<ActiveAudioTransmitter>());
    }
    for stream in &audio_streams {
        assert!(!app
            .world()
            .entity(*stream)
            .contains::<ActiveAudioTransmitter>());
    }

    // From Cast to Stream
    app.world_mut().entity_mut(video_cast).despawn();

    app.update();
    app.update();
    app.update();

    for cast in &audio_casts {
        assert!(!app
            .world()
            .entity(*cast)
            .contains::<ActiveAudioTransmitter>());
    }
    for stream in &audio_streams {
        assert!(app
            .world()
            .entity(*stream)
            .contains::<ActiveAudioTransmitter>());
    }

    // From Stream to Off
    app.world_mut().entity_mut(stream).despawn();

    app.update();
    app.update();
    app.update();

    for cast in &audio_casts {
        assert!(!app
            .world()
            .entity(*cast)
            .contains::<ActiveAudioTransmitter>());
    }
    for stream in &audio_streams {
        assert!(!app
            .world()
            .entity(*stream)
            .contains::<ActiveAudioTransmitter>());
    }
}
