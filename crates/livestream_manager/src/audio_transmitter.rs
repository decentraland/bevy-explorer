use bevy::prelude::*;

/// Component for the kind of transmitter that the entity is
#[derive(Clone, Copy, Component)]
#[component(immutable)]
pub enum AudioTransmitterKind {
    Cast,
    Stream,
}

/// Marker component for audio transmitters that should be playing
#[derive(Component)]
pub struct ActiveAudioTransmitter;
