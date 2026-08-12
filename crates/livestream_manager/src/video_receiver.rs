use bevy::prelude::*;

/// An active receiver entity
#[derive(Component)]
pub struct ActiveReceiver;

/// Component with the handle to the image that the receiver
/// should read from
#[derive(Component, Deref)]
pub struct ReceiverImage(pub(crate) Handle<Image>);
