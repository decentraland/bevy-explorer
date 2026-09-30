use bevy::prelude::*;

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
#[derive(Component, Deref)]
pub struct ReceiverImage(pub(crate) Handle<Image>);
