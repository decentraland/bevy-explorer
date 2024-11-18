pub mod bake_scene;
pub mod floor_imposter;
pub mod imposter_spec;
pub mod render;

use bake_scene::DclImposterBakeScenePlugin;
use bevy::prelude::*;
use render::DclImposterRenderPlugin;

pub struct DclImposterPlugin;

impl Plugin for DclImposterPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((DclImposterBakeScenePlugin, DclImposterRenderPlugin));
    }
}
