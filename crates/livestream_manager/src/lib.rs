pub mod plugin;
pub mod states;
#[cfg(test)]
mod tests;

use bevy::prelude::*;

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

/// Component for the kind of transmitter that the entity is
#[derive(Clone, Copy, Component)]
#[component(immutable)]
pub enum AudioTransmitterKind {
    Cast,
    Stream,
}

/// Component with the handle to the image that the transmitter
/// should write to
#[derive(Component, Deref)]
pub struct ActiveTransmitter(Handle<Image>);

/// Marker component for audio transmitters that should be playing
#[derive(Component)]
pub struct ActiveAudioTransmitter;

/// An active receiver entity
#[derive(Component)]
pub struct ActiveReceiver;

/// Component with the handle to the image that the receiver
/// should read from
#[derive(Component, Deref)]
pub struct ReceiverImage(Handle<Image>);
