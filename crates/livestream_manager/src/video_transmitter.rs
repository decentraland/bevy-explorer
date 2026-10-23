use bevy::prelude::*;
use dcl_component::proto_components::sdk::components::VideoState;

/// Component for the kind of transmitter that the entity is
#[derive(Debug, Clone, Copy, Component)]
#[component(immutable)]
pub enum TransmitterKind {
    Presentation,
    Screenshare,
    VideoCast,
    Stream,
}

/// Extra marker component for [`TransmitterKind::VideoCast`] that
/// are active
#[derive(Component)]
pub struct ActiveVideoCast;

/// Component with the handle to the image that the transmitter
/// should write to
#[derive(Component)]
pub struct ActiveTransmitter {
    pub image: Handle<Image>,
    pub state: tokio::sync::watch::Sender<VideoState>,
}
