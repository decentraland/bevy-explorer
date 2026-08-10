use bevy::{
    log::{Level, LogPlugin},
    prelude::*,
    state::app::StatesPlugin,
};
use livestream_manager::{
    plugin::LivestreamManagerPlugin,
    states::{Receiver, TransmissionKind, Transmitter},
    VideoStream,
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

    app.world_mut().spawn(VideoStream);

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
}
