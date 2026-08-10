use bevy::prelude::*;

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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Transmission {
    #[default]
    Off,
    NeedsTransmitter,
    Transmitting,
}

impl ComputedStates for Transmission {
    type SourceStates = (Transmitter, Receiver);

    fn compute(sources: Self::SourceStates) -> Option<Self> {
        match sources {
            (Transmitter::On, Receiver::On) => Some(Self::Transmitting),
            (Transmitter::Off, Receiver::On) => Some(Self::NeedsTransmitter),
            _ => Some(Self::Off),
        }
    }
}
