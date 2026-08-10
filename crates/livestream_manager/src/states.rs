use bevy::prelude::*;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, States)]
pub enum TransmissionKind {
    #[default]
    Off,
    Cast,
    Stream,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, States)]
pub enum Transmitter {
    #[default]
    Off,
    On,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, States)]
pub enum Receiver {
    #[default]
    Off,
    On,
}
