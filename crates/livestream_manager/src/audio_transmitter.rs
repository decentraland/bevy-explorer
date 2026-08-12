use bevy::prelude::*;

/// Component for the kind of transmitter that the entity is
#[derive(Clone, Copy, PartialEq, Eq, Component)]
pub enum AudioTransmitterKind {
    Cast,
    Stream,
}

/// Marker component for audio transmitters that should be playing
#[derive(Component)]
pub struct ActiveAudioTransmitter;

/// Volume of the transmitter, is matched to the value of [`ReceiverVolume`]
///
/// [`ReceiverVolume`]: crate::video_receiver::ReceiverVolume
#[derive(Component, Deref)]
pub struct AudioTransmitterVolume(pub(crate) f32);
