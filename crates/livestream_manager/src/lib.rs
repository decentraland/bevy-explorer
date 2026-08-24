mod audio_transmitter;
pub mod plugin;
pub mod states;
#[cfg(test)]
mod tests;
mod video_receiver;
mod video_transmitter;

use bevy::prelude::*;

pub use crate::{audio_transmitter::*, video_receiver::*, video_transmitter::*};

#[derive(Event)]
pub struct TransmissionUpdated;
