pub mod plugin;

use bevy::prelude::*;
use dcl_component::proto_components::sdk::components::PbParticleSystem;

#[derive(Clone, Component, Deref, DerefMut)]
#[component(immutable)]
pub struct ParticleSystem(PbParticleSystem);

impl From<PbParticleSystem> for ParticleSystem {
    fn from(value: PbParticleSystem) -> Self {
        Self(value)
    }
}
