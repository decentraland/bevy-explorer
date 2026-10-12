pub mod plugin;

use bevy::{asset::embedded_asset, prelude::*};
use dcl_component::proto_components::sdk::components::PbParticleSystem;

#[derive(Clone, Component, Deref, DerefMut)]
#[component(immutable)]
pub struct ParticleSystem(PbParticleSystem);

impl From<PbParticleSystem> for ParticleSystem {
    fn from(value: PbParticleSystem) -> Self {
        Self(value)
    }
}

/// One shader file per effect, loaded through the asset server so they hot-reload with the
/// `hot_reload` feature. Paths are `embedded://particle_system/shaders/<name>.wgsl`.
pub(crate) fn embed_shaders(app: &mut App) {
    embedded_asset!(app, "shaders/common.wgsl");
    embedded_asset!(app, "shaders/shape.wgsl");
    embedded_asset!(app, "shaders/forces.wgsl");
    embedded_asset!(app, "shaders/limit_velocity.wgsl");
    embedded_asset!(app, "shaders/rotation.wgsl");
    embedded_asset!(app, "shaders/size.wgsl");
    embedded_asset!(app, "shaders/color.wgsl");
    embedded_asset!(app, "shaders/sprite_sheet.wgsl");
    embedded_asset!(app, "shaders/billboard.wgsl");
    embedded_asset!(app, "shaders/simulate.wgsl");
    embedded_asset!(app, "shaders/render.wgsl");
}
