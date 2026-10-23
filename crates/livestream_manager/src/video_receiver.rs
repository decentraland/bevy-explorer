use bevy::prelude::*;
use dcl_component::proto_components::sdk::components::VideoState;

/// An active receiver entity
#[derive(Component)]
#[require(ReceiverVolume)]
pub struct ActiveReceiver;

/// Audio volume of the receiver
#[derive(Component, Deref, DerefMut)]
pub struct ReceiverVolume(pub f32);

impl Default for ReceiverVolume {
    fn default() -> Self {
        Self(1.)
    }
}

/// Component with the handle to the image that the receiver
/// should read from
#[derive(Component)]
pub struct ReceiverImage {
    pub image: Handle<Image>,
    pub state: tokio::sync::watch::Receiver<VideoState>,
}
